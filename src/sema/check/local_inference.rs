use super::{BTreeMap, BTreeSet, CheckBundleView, Checker, Diagnostic, Label, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::constraints::TypeVariableId;
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaFunctionDef, ArenaProgram,
    ArenaStmtKind, BindingTargetId, ExprId, ArenaListElementRange, ArenaRecordFieldKind, ArenaSignalHook,
};
use std::sync::Arc;

#[derive(Clone, Default)]
pub(super) struct LocalInference {
    collecting: bool,
    pub(super) host_obligations: super::method::HostObligations,
    seeds: BTreeSet<Span>,
    bindings: BTreeMap<Span, LocalBindingInference>,
    nullable_seed_cells: BTreeSet<TypeVariableId>,
    aggregate_scopes: Vec<AggregateSeedScope>,
    solved_functions: BTreeSet<Span>,
    nonmaterial_expressions: BTreeSet<Span>,
    constructor_producers: BTreeMap<ExprId, crate::sema::schemes::NonmaterialConstructor>,
    pub(super) checked_bindings: BTreeMap<Span, Type>,
    /// Expression facts recorded with inference identities since the last
    /// record-constructor publication; every other fact is already canonical.
    unresolved_expr_types: Vec<Span>,
}

#[derive(Clone)]
struct LocalBindingInference {
    ty: Type,
    holes: Vec<Type>,
    owner: Option<super::EffectDeclarationId>,
    initializer: Span,
}

enum LocalInferenceFailure {
    Unbound,
    Resolution(crate::sema::constraints::ConstraintResolutionError),
}

#[derive(Clone)]
enum FreshAggregateOrigin {
    List(ArenaListElementRange),
    Record(BTreeMap<super::Name, Option<ExprId>>),
}

#[derive(Clone)]
struct AggregateSeedScope {
    initializer: Span,
    origins: BTreeMap<Span, FreshAggregateOrigin>,
}

impl LocalInference {
    pub(super) fn merge_prepared_owner(&mut self, other: &Self) {
        assert!(
            !other.collecting
                && other.bindings.is_empty()
                && other.nullable_seed_cells.is_empty()
                && other.aggregate_scopes.is_empty()
                && other.solved_functions.is_empty()
                && other.nonmaterial_expressions.is_empty()
                && other.constructor_producers.is_empty()
                && other.checked_bindings.is_empty()
                && other.unresolved_expr_types.is_empty(),
            "module owner preparation must contain only local-inference seeds"
        );
        self.seeds.extend(other.seeds.iter().copied());
    }

    pub(super) fn prepare(view: &CheckBundleView<'_>) -> Self {
        let program = view.program;
        let mut inference = Self::default();
        for id in view.stmt_ids() {
            let statement = program.arena.stmt(id);
            let (target, annotation, initializer, mutable) = match statement.kind {
                ArenaStmtKind::Let {
                    target,
                    ty,
                    initializer,
                } => (target, ty, initializer, false),
                ArenaStmtKind::Var {
                    target,
                    ty,
                    initializer,
                } => (target, ty, initializer, true),
                _ => continue,
            };
            if annotation.is_none()
                && local_seed_kind(program, target, initializer, mutable).is_some()
            {
                inference.seeds.insert(statement.span);
            }
        }
        inference
    }
}

impl Checker {
    pub(super) fn record_expr_type(&mut self, span: Span, ty: Type) {
        if ty.contains_inference() {
            self.local_inference.unresolved_expr_types.push(span);
        }
        self.expr_types.insert(span, ty);
    }

    /// Canonicalize the expression facts that still carry inference identities.
    /// A fact that remains unsolved becomes a recovery fact.
    pub(super) fn publish_unresolved_expr_types(&mut self) {
        for span in std::mem::take(&mut self.local_inference.unresolved_expr_types) {
            if let Some(ty) = self.expr_types.get_mut(&span)
                && ty.contains_inference()
            {
                *ty = self
                    .type_constraints
                    .resolve(ty)
                    .ok()
                    .filter(|resolved| !resolved.contains_inference())
                    .unwrap_or(Type::Invalid);
            }
        }
    }

    /// Checked facts carry canonical types, never inference identities. A
    /// failed material contract becomes a recovery fact after its diagnostic.
    pub(super) fn resolve_checked_types(&mut self) {
        let mut reported = self
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference))
            .flat_map(|diagnostic| diagnostic.labels.iter().map(|label| label.span))
            .collect::<BTreeSet<_>>();
        let constraints = &self.type_constraints;
        let diagnostics = &mut self.diagnostics;
        self.expr_types.retain(|span, ty| {
            !self.local_inference.nonmaterial_expressions.contains(span)
                || !matches!(constraints.resolve(ty), Ok(resolved) if resolved.contains_inference())
        });
        for (span, ty) in &mut self.expr_types {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        for ((_, span), fact) in &mut self.stream_stage_types {
            finalize_type(
                constraints,
                &mut fact.input,
                *span,
                &mut reported,
                diagnostics,
            );
            finalize_type(
                constraints,
                &mut fact.output,
                *span,
                &mut reported,
                diagnostics,
            );
        }
        for (span, ty) in &mut self.function_return_types {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        let mut annotation_facts = Vec::with_capacity(self.annotation_facts.len());
        for mut fact in std::mem::take(&mut self.annotation_facts) {
            let span = match fact.kind {
                super::AnnotationFactKind::Binding { span, .. }
                | super::AnnotationFactKind::DefaultedParam { span, .. } => span,
                super::AnnotationFactKind::InferredPureReturn { body }
                | super::AnnotationFactKind::ExportedProcReturn { body } => body,
            };
            if let super::AnnotationFactKind::Binding { exported, .. } = &fact.kind
                && fact.ty.contains_inference()
            {
                match constraints.resolve(&fact.ty) {
                    Ok(resolved) if !resolved.contains_inference() => fact.ty = resolved,
                    _ => continue,
                }
                if matches!(fact.ty, Type::Unit)
                    || fact.ty.annotation_source().is_none()
                    || (!*exported && !super::stmt::annotation_type_is_nontrivial(&fact.ty))
                {
                    continue;
                }
            } else {
                finalize_type(constraints, &mut fact.ty, span, &mut reported, diagnostics);
            }
            annotation_facts.push(fact);
        }
        self.annotation_facts = annotation_facts;
        for (span, ty) in &mut self.local_inference.checked_bindings {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        for (span, ty) in &mut self.parameter_types {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        for (span, fact) in &mut self.record_constructor_instances {
            finalize_type(constraints, &mut fact.ty, *span, &mut reported, diagnostics);
            for argument in &mut fact.instance.arguments {
                finalize_type(constraints, argument, *span, &mut reported, diagnostics);
            }
        }
        for (span, projection) in &mut self.projections {
            finalize_projection(constraints, projection, *span, &mut reported, diagnostics);
        }
        for (span, alias) in &mut self.static_callable_aliases {
            finalize_callable(
                constraints,
                &mut alias.signature,
                *span,
                &mut reported,
                diagnostics,
            );
        }
        let mut typed_callable_calls = BTreeMap::new();
        for (span, callable) in std::mem::take(&mut self.typed_callable_calls) {
            let mut ty = Type::Callable(callable);
            finalize_type(constraints, &mut ty, span, &mut reported, diagnostics);
            if let Type::Callable(callable) = ty {
                typed_callable_calls.insert(span, callable);
            }
        }
        self.typed_callable_calls = typed_callable_calls;
        let fallback = Span::at(crate::source::SourceId::new(0), 0);
        for signature in self.declaration_inference.pending_iter_mut() {
            if let Some(shape) = signature.provisional_mut() {
                for parameter in &mut shape.params {
                    finalize_type(
                        constraints,
                        &mut parameter.signature.ty,
                        fallback,
                        &mut reported,
                        diagnostics,
                    );
                }
                finalize_type(
                    constraints,
                    &mut shape.return_ty,
                    fallback,
                    &mut reported,
                    diagnostics,
                );
            }
        }
        for module in Arc::make_mut(&mut self.user_modules).values_mut() {
            for ty in module
                .values
                .values_mut()
                .chain(module.resolved_types.values_mut())
            {
                finalize_type(constraints, ty, fallback, &mut reported, diagnostics);
            }
        }
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if constraints.resolve_in_place(&mut binding.ty).is_err()
                    || binding.ty.contains_inference()
                {
                    binding.ty = Type::Invalid;
                }
                if let Some(ty) = &mut binding.unrefined_ty
                    && (constraints.resolve_in_place(ty).is_err() || ty.contains_inference())
                {
                    *ty = Type::Invalid;
                }
            }
        }
        self.resolve_pending_call_instantiations();
    }

    pub(super) fn record_inert_local_discard(
        &mut self,
        program: &ArenaProgram,
        target: BindingTargetId,
        initializer: ArenaExprOrRun,
    ) {
        if matches!(program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "_")
        {
            self.record_inert_expression_discard(program, initializer);
        }
    }

    pub(super) fn record_inert_expression_discard(
        &mut self,
        program: &ArenaProgram,
        expression: ArenaExprOrRun,
    ) {
        if let ArenaExprOrRun::Expr(expression) = expression
            && matches!(program.arena.expr(expression).kind, crate::syntax::arena::ArenaExprKind::List(elements) if elements.is_empty())
        {
            self.local_inference.nonmaterial_expressions.insert(program.arena.expr(expression).span);
            if !self.local_inference.collecting {
                self.local_inference.constructor_producers.insert(expression, crate::sema::schemes::NonmaterialConstructor::EmptyList);
            }
            return;
        }
        if matches!(expression, ArenaExprOrRun::Expr(expression) if self.is_checked_empty_map_constructor(program, expression)) {
            self.local_inference
                .nonmaterial_expressions
                .insert(super::expr::expr_or_run_span_arena(program, expression));
            if !self.local_inference.collecting
                && let ArenaExprOrRun::Expr(expression) = expression
            {
                self.local_inference.constructor_producers.insert(expression, crate::sema::schemes::NonmaterialConstructor::EmptyMap);
            }
        }
    }

    pub(super) fn is_inert_expression_discard(&self, span: Span) -> bool {
        self.local_inference.nonmaterial_expressions.contains(&span)
    }

    pub(super) fn collect_function_local_constraints(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        definition: &ArenaFunctionDef,
        pure: bool,
    ) {
        self.collect_callable_local_constraints(program, source, definition, pure, false);
    }

    pub(super) fn collect_stream_local_constraints(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        definition: &ArenaFunctionDef,
    ) {
        self.collect_callable_local_constraints(program, source, definition, false, true);
    }

    fn collect_callable_local_constraints(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        definition: &ArenaFunctionDef,
        pure: bool,
        stream: bool,
    ) {
        if self.local_inference.collecting {
            return;
        }
        let body = program
            .arena
            .span(program.arena.block(definition.body).span);
        if self.local_inference.solved_functions.contains(&body) {
            return;
        }
        let has_seeds = |span: Span| {
            let start = Span::new(span.source_id, span.start(), span.start());
            let end = Span::new(span.source_id, span.end(), span.end());
            self.local_inference.seeds.range(start..end).next().is_some()
        };
        let has_local_seeds = has_seeds(body)
            || program.arena.params(definition.params).iter().filter_map(|parameter| parameter.default)
                .any(|expression| has_seeds(program.arena.expr(expression).span));
        let has_unresolved_parameters = pure
            && program.arena.params(definition.params).iter().any(|parameter| {
                parameter.annotation.is_none()
                    && parameter.default.is_none()
                    && self.checked_parameter_type(program, parameter).is_some_and(|ty| {
                        self.type_constraints.resolve(&ty)
                            .is_ok_and(|resolved| resolved.contains_inference())
                    })
            });
        if !has_local_seeds && !has_unresolved_parameters {
            return;
        }
        self.collect_local_body_constraints(program, body, |probe| {
            if stream {
                probe.check_stream_function_arena(program, source, definition);
            } else {
                probe.check_function_arena(program, source, definition, pure);
            }
        });
    }

    pub(super) fn collect_signal_hook_local_constraints(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        hook: &ArenaSignalHook,
        span: Span,
    ) {
        let body = program.arena.span(program.arena.block(hook.body).span);
        let start = Span::new(body.source_id, body.start(), body.start());
        let end = Span::new(body.source_id, body.end(), body.end());
        if self.local_inference.seeds.range(start..end).next().is_some() {
            self.collect_local_body_constraints(program, body, |probe| {
                probe.check_signal_hook_arena(program, source, hook, span);
            });
        }
    }

    fn collect_local_body_constraints(&mut self, program: &ArenaProgram, body: Span, check: impl FnOnce(&mut Self)) {
        if self.local_inference.collecting || self.local_inference.solved_functions.contains(&body) {
            return;
        }
        let mut probe = self.constraint_probe();
        probe.local_inference.collecting = true;
        check(&mut probe);
        if let Err(error) = probe.solve_host_obligations(program) {
            self.reject_host_inference_resolution(error, body);
        }
        self.type_constraints = probe.type_constraints;
        self.local_inference.bindings = probe.local_inference.bindings;
        self.local_inference.solved_functions.insert(body);
    }

    pub(super) fn collecting_local_constraints(&self) -> bool {
        self.local_inference.collecting
    }

    pub(super) fn prepared_nullable_seed(&self, ty: &Type) -> Option<TypeVariableId> {
        let Type::Optional(inner) = ty else {
            return None;
        };
        let Type::Inference(id) = inner.as_ref() else {
            return None;
        };
        self.local_inference
            .nullable_seed_cells
            .contains(id)
            .then_some(*id)
    }

    pub(super) fn constraint_probe(&mut self) -> Self {
        let checked_declarations = std::mem::take(&mut self.checked_declarations);
        let resolved_type_expressions = std::mem::take(&mut self.resolved_type_expressions);
        let block_exit_bindings = std::mem::take(&mut self.block_exit_bindings);
        let expr_types = std::mem::take(&mut self.expr_types);
        let binary_operators = std::mem::take(&mut self.binary_operators);
        let stream_expr_origins = std::mem::take(&mut self.stream_expr_origins);
        let stream_stmt_origins = std::mem::take(&mut self.stream_stmt_origins);
        let proc_return_sites = std::mem::take(&mut self.proc_return_sites);
        let stream_stmt_results = std::mem::take(&mut self.stream_stmt_results);
        let projections = std::mem::take(&mut self.projections);
        let record_constructor_instances = std::mem::take(&mut self.record_constructor_instances);
        let checked_schema_origins = std::mem::take(&mut self.checked_schema_origins);
        let requirement_targets = std::mem::take(&mut self.requirement_targets);
        let requirement_expected_targets = std::mem::take(&mut self.requirement_expected_targets);
        let condition_proofs = std::mem::take(&mut self.condition_proofs);
        let proven_nonnull_fallback_receivers =
            std::mem::take(&mut self.proven_nonnull_fallback_receivers);
        let static_callable_aliases = std::mem::take(&mut self.static_callable_aliases);
        let typed_callable_calls = std::mem::take(&mut self.typed_callable_calls);
        let selected_function_calls = std::mem::take(&mut self.selected_function_calls);
        let selected_function_value_references = std::mem::take(&mut self.selected_function_value_references);
        let pending_call_instantiations = std::mem::take(&mut self.pending_call_instantiations);
        let call_instantiations = std::mem::take(&mut self.call_instantiations);
        let prepared_function_call_contracts =
            std::mem::take(&mut self.prepared_function_call_contracts);
        let diagnostics = std::mem::take(&mut self.diagnostics);
        let annotation_facts = std::mem::take(&mut self.annotation_facts);
        let reveal_types = std::mem::take(&mut self.reveal_types);
        let stream_stage_types = std::mem::take(&mut self.stream_stage_types);
        let statement_positions = std::mem::take(&mut self.statement_positions);
        let pattern_test_types = std::mem::take(&mut self.pattern_test_types);
        let assertion_effect_spans = std::mem::take(&mut self.assertion_effect_spans);
        let statement_expression_spans = std::mem::take(&mut self.statement_expression_spans);
        let propagating_statements = std::mem::take(&mut self.propagating_statements);
        let membership_migration_spans = std::mem::take(&mut self.membership_migration_spans);
        let standard_call_spans = std::mem::take(&mut self.standard_call_spans);
        let statically_resolved_call_spans =
            std::mem::take(&mut self.statically_resolved_call_spans);
        let api_calls = std::mem::take(&mut self.api_calls);
        let argument_bindings = std::mem::take(&mut self.argument_bindings);
        let definitely_exiting_block_spans =
            std::mem::take(&mut self.definitely_exiting_block_spans);
        let checked_bindings = std::mem::take(&mut self.local_inference.checked_bindings);
        let aggregate_scopes = std::mem::take(&mut self.local_inference.aggregate_scopes);
        let constructor_producers = std::mem::take(&mut self.local_inference.constructor_producers);
        let probe = self.clone();
        self.checked_declarations = checked_declarations;
        self.resolved_type_expressions = resolved_type_expressions;
        self.block_exit_bindings = block_exit_bindings;
        self.expr_types = expr_types;
        self.binary_operators = binary_operators;
        self.stream_expr_origins = stream_expr_origins;
        self.stream_stmt_origins = stream_stmt_origins;
        self.proc_return_sites = proc_return_sites;
        self.stream_stmt_results = stream_stmt_results;
        self.projections = projections;
        self.record_constructor_instances = record_constructor_instances;
        self.checked_schema_origins = checked_schema_origins;
        self.requirement_targets = requirement_targets;
        self.requirement_expected_targets = requirement_expected_targets;
        self.condition_proofs = condition_proofs;
        self.proven_nonnull_fallback_receivers = proven_nonnull_fallback_receivers;
        self.static_callable_aliases = static_callable_aliases;
        self.typed_callable_calls = typed_callable_calls;
        self.selected_function_calls = selected_function_calls;
        self.selected_function_value_references = selected_function_value_references;
        self.pending_call_instantiations = pending_call_instantiations;
        self.call_instantiations = call_instantiations;
        self.prepared_function_call_contracts = prepared_function_call_contracts;
        self.diagnostics = diagnostics;
        self.annotation_facts = annotation_facts;
        self.reveal_types = reveal_types;
        self.stream_stage_types = stream_stage_types;
        self.statement_positions = statement_positions;
        self.pattern_test_types = pattern_test_types;
        self.assertion_effect_spans = assertion_effect_spans;
        self.statement_expression_spans = statement_expression_spans;
        self.propagating_statements = propagating_statements;
        self.membership_migration_spans = membership_migration_spans;
        self.standard_call_spans = standard_call_spans;
        self.statically_resolved_call_spans = statically_resolved_call_spans;
        self.api_calls = api_calls;
        self.argument_bindings = argument_bindings;
        self.definitely_exiting_block_spans = definitely_exiting_block_spans;
        self.local_inference.checked_bindings = checked_bindings;
        self.local_inference.aggregate_scopes = aggregate_scopes;
        self.local_inference.constructor_producers = constructor_producers;
        probe
    }

    pub(super) fn local_binding_expectation(
        &mut self,
        program: &ArenaProgram,
        target: BindingTargetId,
        initializer: ArenaExprOrRun,
        mutable: bool,
        span: Span,
    ) -> Option<Type> {
        let aggregate = self.boundary.current_return.is_some()
            && (matches!(initializer, ArenaExprOrRun::Expr(expression) if matches!(program.arena.expr(expression).kind, ArenaExprKind::Record(_)))
                || matches!(local_seed_kind(program, target, initializer, mutable), Some(LocalSeed::Aggregate)));
        if aggregate {
            self.local_inference.aggregate_scopes.push(AggregateSeedScope {
                initializer: super::expr::expr_or_run_span_arena(program, initializer),
                origins: BTreeMap::new(),
            });
        }
        if let Some(inference) = self.local_inference.bindings.get(&span) {
            return self.type_constraints.resolve(&inference.ty).ok();
        }
        if aggregate { return None; }
        if self.boundary.current_return.is_none()
            || !self.local_inference.seeds.contains(&span)
            || !matches!(local_seed_kind(program, target, initializer, mutable), Some(LocalSeed::Set))
            || !self.has_unshadowed_empty_set(program, initializer)
        {
            return None;
        }
        let origin = super::expr::expr_or_run_span_arena(program, initializer);
        let variable = self.fresh_local_inference_type(origin);
        let ty = Type::Set(Box::new(variable.clone()));
        self.local_inference.bindings.insert(span, LocalBindingInference { ty: ty.clone(), holes: vec![variable], owner: self.local_inference_owner(), initializer: origin });
        self.type_constraints.resolve(&ty).ok()
    }

    pub(super) fn begin_record_constructor_scope(&mut self, program: &ArenaProgram, expression: ExprId) -> bool {
        let span = program.arena.expr(expression).span;
        if self.boundary.current_return.is_none()
            || !matches!(program.arena.expr(expression).kind, ArenaExprKind::Record(_))
            || self.local_inference.aggregate_scopes.last().is_some_and(|scope| scope.initializer.source_id == span.source_id && scope.initializer.start() <= span.start() && span.end() <= scope.initializer.end())
        { return false; }
        self.local_inference.aggregate_scopes.push(AggregateSeedScope { initializer: span, origins: BTreeMap::new() });
        true
    }

    pub(super) fn finish_record_constructor_scope(&mut self, program: &ArenaProgram, expression: ExprId) {
        let scope = self.local_inference.aggregate_scopes.pop().expect("the checked record owns its constructor scope");
        assert_eq!(scope.initializer, program.arena.expr(expression).span);
        self.record_overwritten_constructor_producers(program, expression, &scope.origins, false);
    }

    pub(super) fn local_aggregate_record_fields(
        &self,
        span: Span,
    ) -> Option<BTreeMap<super::Name, Option<ExprId>>> {
        self.local_inference.aggregate_scopes.last().filter(|scope| {
            scope.initializer.source_id == span.source_id
                && scope.initializer.start() <= span.start()
                && span.end() <= scope.initializer.end()
        }).map(|_| BTreeMap::new())
    }

    pub(super) fn local_aggregate_spread_fields(
        &self,
        span: Span,
    ) -> Option<BTreeMap<super::Name, Option<ExprId>>> {
        match self.local_inference.aggregate_scopes.last()?.origins.get(&span)? {
            FreshAggregateOrigin::Record(fields) => Some(fields.clone()),
            FreshAggregateOrigin::List(_) => None,
        }
    }

    pub(super) fn record_local_aggregate_fields(
        &mut self,
        span: Span,
        fields: BTreeMap<super::Name, Option<ExprId>>,
    ) {
        if let Some(scope) = self.local_inference.aggregate_scopes.last_mut() {
            scope.origins.insert(span, FreshAggregateOrigin::Record(fields));
        }
    }

    pub(super) fn record_local_aggregate_list(&mut self, span: Span, range: ArenaListElementRange) {
        if self.local_aggregate_record_fields(span).is_some()
            && let Some(scope) = self.local_inference.aggregate_scopes.last_mut()
        {
            scope.origins.insert(span, FreshAggregateOrigin::List(range));
        }
    }

    pub(super) fn is_checked_empty_map_constructor(&self, program: &ArenaProgram, expression: ExprId) -> bool {
        matches!(program.arena.expr(expression).kind, ArenaExprKind::Call { callee, args } if args.is_empty()
            && matches!(program.arena.expr(callee).kind, ArenaExprKind::Field { base, .. } if matches!(program.arena.expr(base).kind, ArenaExprKind::Ident(_))))
            && self.api_calls.get(&program.arena.expr(expression).span).is_some_and(|call| {
                call.receiver.is_none() && call.params.is_empty() && call.argument_slots.is_empty()
                    && call.sig.pure && call.sig.effects.is_empty()
                    && super::api_spec().module_overloads("map", "empty").is_some_and(|signatures| signatures.iter().any(|signature| std::ptr::eq(signature, call.sig)))
            })
    }

    pub(super) fn nonmaterial_constructor_producers(&self) -> &BTreeMap<ExprId, crate::sema::schemes::NonmaterialConstructor> {
        &self.local_inference.constructor_producers
    }

    fn record_overwritten_constructor_producers(
        &mut self,
        program: &ArenaProgram,
        expression: ExprId,
        origins: &BTreeMap<Span, FreshAggregateOrigin>,
        discarded: bool,
    ) {
        if self.local_inference.collecting || !origins.contains_key(&program.arena.expr(expression).span) {
            return;
        }
        let mut candidates = BTreeMap::new();
        collect_empty_constructors(program, expression, &mut candidates);
        candidates.retain(|expression, kind| {
            self.expr_types.get(&program.arena.expr(*expression).span).is_some_and(|ty| ty.contains_recovery() || ty.contains_inference())
                && (*kind != crate::sema::schemes::NonmaterialConstructor::EmptyMap || self.is_checked_empty_map_constructor(program, *expression))
        });
        let mut surviving = BTreeSet::new();
        if !discarded { collect_surviving_empty_constructors(program, expression, origins, &candidates, &mut surviving); }
        for (&expression, &kind) in &candidates {
            if !surviving.contains(&expression) { self.local_inference.constructor_producers.insert(expression, kind); }
        }
        retain_nonmaterial_record_producers(program, expression, origins, &mut self.local_inference.constructor_producers);
    }

    fn local_inference_owner(&self) -> Option<super::EffectDeclarationId> {
        self.stream_origin_owner.filter(|owner| {
            self.declaration_inference.pending(super::FunctionDeclarationId::new(*owner)).is_some()
        })
    }

    pub(super) fn validate_local_inference(
        &mut self,
        owner: super::EffectDeclarationId,
        component: &BTreeSet<super::EffectDeclarationId>,
        formals: &[Type],
    ) -> bool {
        let failures = self.local_inference.bindings.iter().filter_map(|(&binding, inference)| {
            if inference.owner != Some(owner) { return None; }
            match self.type_constraints.unresolved_local_holes_are_input_bound(owner, component, &inference.holes, formals) {
                Ok(true) => None,
                Ok(false) => Some((binding, inference.initializer, LocalInferenceFailure::Unbound)),
                Err(error) => Some((binding, inference.initializer, LocalInferenceFailure::Resolution(error))),
            }
        }).collect::<Vec<_>>();
        let valid = failures.is_empty();
        for (binding, initializer, failure) in failures {
            if !self.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)
                && diagnostic.labels.iter().any(|label| label.span == initializer))
            {
                let (message, label) = match failure {
                    LocalInferenceFailure::Unbound =>
                        ("local type needs an annotation", "no unique concrete type or input relationship is established for this initializer"),
                    LocalInferenceFailure::Resolution(crate::sema::constraints::ConstraintResolutionError::ForeignVariable) =>
                        ("local type cannot be established", "inference variables must belong to one bounded checking problem"),
                    LocalInferenceFailure::Resolution(crate::sema::constraints::ConstraintResolutionError::TypeDepth) =>
                        ("local type cannot be established", "type inference exceeds the supported nesting depth"),
                };
                self.diagnostics.push(Diagnostic::error(message).with_code(DiagnosticCode::CheckLocalInference)
                    .with_label(Label::primary(initializer, label)));
            }
            self.local_inference.checked_bindings.insert(binding, Type::Invalid);
        }
        valid
    }

    fn fresh_local_inference_type(&mut self, origin: Span) -> Type {
        let level = self.stream_origin_owner
            .and_then(|owner| self.declaration_inference.pending(super::FunctionDeclarationId::new(owner)))
            .map_or(0, super::PendingFunctionSig::solver_level);
        self.type_constraints.fresh_at_level(origin, level)
    }

    fn materialize_fresh_aggregate(
        &mut self,
        program: &ArenaProgram,
        expression: ExprId,
        ordinary: &Type,
        source_types: &[Type],
        origins: &BTreeMap<Span, FreshAggregateOrigin>,
        holes: &mut Vec<Type>,
    ) -> Type {
        if source_types.iter().any(|ty| matches!(ty, Type::Any)) {
            return ordinary.clone();
        }
        let span = program.arena.expr(expression).span;
        match (origins.get(&span), ordinary) {
            (Some(FreshAggregateOrigin::List(range)), Type::List(item)) if range.is_empty() => {
                if **item == Type::Unknown
                    && !source_types.iter().any(|ty| matches!(ty, Type::List(item) if **item == Type::Any))
                {
                    let variable = self.fresh_local_inference_type(span);
                    holes.push(variable.clone());
                    Type::List(Box::new(variable))
                } else {
                    ordinary.clone()
                }
            }
            (Some(FreshAggregateOrigin::Record(selected)), Type::Record(fields)) => {
                let mut fields = fields.as_ref().clone();
                for (name, selected) in selected {
                    if let Some(expression) = selected
                        && let Some(ty) = fields.get(name).cloned()
                    {
                        let source_types = source_types.iter().filter_map(|source_type| match source_type {
                            Type::Record(fields) => fields.get(name).cloned(),
                            _ => None,
                        }).collect::<Vec<_>>();
                        fields.insert(*name, self.materialize_fresh_aggregate(program, *expression, &ty, &source_types, origins, holes));
                    }
                }
                Type::Record(Arc::new(fields))
            }
            (Some(FreshAggregateOrigin::List(range)), Type::List(item)) => {
                let elements = program.arena.list_elements(*range).collect::<Vec<_>>();
                let mut item_source_types = source_types.iter().filter_map(|source_type| match source_type {
                    Type::List(item) => Some(item.as_ref().clone()),
                    _ => None,
                }).collect::<Vec<_>>();
                for element in &elements {
                    if let Some(ty) = self.expr_types.get(&program.arena.expr(element.value).span) {
                        if element.splice_span.is_some() {
                            if let Type::List(item) = ty { item_source_types.push(item.as_ref().clone()); }
                        } else {
                            item_source_types.push(ty.clone());
                        }
                    }
                }
                let mut contributions = Vec::new();
                for element in &elements {
                    let child_span = program.arena.expr(element.value).span;
                    let Some(ty) = self.expr_types.get(&child_span).cloned() else { continue };
                    let ty = if element.splice_span.is_none() {
                        self.materialize_fresh_aggregate(program, element.value, &ty, &item_source_types, origins, holes)
                    } else if let Type::List(item) = ty {
                        *item
                    } else {
                        continue;
                    };
                    contributions.push((child_span, ty));
                }
                if let Some((_, inferred)) = contributions.iter().find(|(_, ty)| ty.contains_inference()) {
                    let inferred = inferred.clone();
                    for (span, actual) in &contributions { self.expect_type(&inferred, actual, *span); }
                    Type::List(Box::new(inferred))
                } else {
                    Type::List(item.clone())
                }
            }
            _ => ordinary.clone(),
        }
    }

    fn has_unshadowed_empty_set(
        &self,
        program: &ArenaProgram,
        initializer: ArenaExprOrRun,
    ) -> bool {
        empty_set_call_receiver(program, initializer).is_some_and(|base| {
            matches!(
                self.lookup_value_binding_for_expr(program, base),
                super::ValueBindingLookup::NoSite
            )
        })
    }

    pub(super) fn resolve_local_tail_type(
        &mut self,
        ty: Type,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        self.resolve_local_tail_type_with_expectation(
            ty,
            expected.map(super::types::TypeExpectation::Required),
            span,
        )
    }

    pub(super) fn resolve_local_tail_type_with_expectation(
        &mut self,
        ty: Type,
        expectation: Option<super::types::TypeExpectation<'_>>,
        span: Span,
    ) -> Type {
        let expected = expectation.map(super::types::TypeExpectation::context);
        let resolved = self.type_constraints.resolve(&ty).unwrap_or(Type::Invalid);
        if resolved.contains_inference()
            && let Some(expected) = expected
        {
            let context = if resolved.is_result() {
                expected
            } else {
                expected.result_ok().unwrap_or(expected)
            };
            if !expectation
                .is_some_and(|expectation| expectation.accepts_optional_branch(&resolved))
            {
                self.expect_type(context, &resolved, span);
            }
        }
        self.type_constraints
            .resolve(&resolved)
            .unwrap_or(Type::Invalid)
    }

    pub(super) fn record_checked_local_binding(
        &mut self,
        program: &ArenaProgram,
        target: BindingTargetId,
        span: Span,
        ty: &Type,
    ) {
        if !self.local_inference.collecting
            && matches!(program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name != "_")
        {
            self.local_inference
                .checked_bindings
                .insert(span, ty.clone());
        }
    }

    pub(super) fn infer_local_binding(
        &mut self,
        program: &ArenaProgram,
        target: BindingTargetId,
        initializer: ArenaExprOrRun,
        mutable: bool,
        span: Span,
        ordinary: Type,
    ) -> Type {
        if self.boundary.current_return.is_none() {
            return ordinary;
        }
        let aggregate_scope = if self.local_inference.aggregate_scopes.last().is_some_and(|scope| scope.initializer == super::expr::expr_or_run_span_arena(program, initializer)) {
            let scope = self.local_inference.aggregate_scopes.pop().expect("the matching initializer owns its origin scope");
            if let ArenaExprOrRun::Expr(expression) = initializer {
                let discarded = matches!(program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "_");
                self.record_overwritten_constructor_producers(program, expression, &scope.origins, discarded);
            }
            Some(scope)
        } else { None };
        let Some(seed) = local_seed_kind(program, target, initializer, mutable) else {
            return ordinary;
        };
        if matches!(seed, LocalSeed::Map) {
            let has_unshadowed_empty_map = empty_map_call_receiver(program, initializer)
                .is_some_and(|base| {
                    matches!(
                        self.lookup_value_binding_for_expr(program, base),
                        super::ValueBindingLookup::NoSite
                    )
                });
            if !has_unshadowed_empty_map || !matches!(ordinary, Type::Map(_, _)) {
                return ordinary;
            }
        }
        if matches!(seed, LocalSeed::Set)
            && (!self.has_unshadowed_empty_set(program, initializer)
                || !matches!(ordinary, Type::Set(_)))
        {
            return ordinary;
        }
        let inference = if let Some(inference) = self.local_inference.bindings.get(&span) {
            inference.clone()
        } else if matches!(seed, LocalSeed::Aggregate) {
            let origin = super::expr::expr_or_run_span_arena(program, initializer);
            let scope = aggregate_scope.expect("aggregate seed retains its checked origins");
            let ArenaExprOrRun::Expr(expression) = initializer else { unreachable!() };
            let mut holes = Vec::new();
            let ty = self.materialize_fresh_aggregate(program, expression, &ordinary, &[ordinary.clone()], &scope.origins, &mut holes);
            if holes.is_empty() { return ordinary; }
            let inference = LocalBindingInference { ty, holes, owner: self.local_inference_owner(), initializer: origin };
            self.local_inference.bindings.insert(span, inference.clone());
            inference
        } else {
            let origin = super::expr::expr_or_run_span_arena(program, initializer);
            let variable = self.fresh_local_inference_type(origin);
            let mut holes = vec![variable.clone()];
            let ty = match seed {
                LocalSeed::List => Type::List(Box::new(variable)),
                LocalSeed::Nullable => Type::Optional(Box::new(variable)),
                LocalSeed::Map => {
                    let value = self.fresh_local_inference_type(origin);
                    holes.push(value.clone());
                    Type::Map(Box::new(variable), Box::new(value))
                }
                LocalSeed::Set => Type::Set(Box::new(variable)),
                LocalSeed::Aggregate => unreachable!(),
            };
            let inference = LocalBindingInference { ty, holes, owner: self.local_inference_owner(), initializer: origin };
            self.local_inference.bindings.insert(span, inference.clone());
            inference
        };
        let raw = inference.ty;
        if matches!(&seed, LocalSeed::Nullable)
            && self.local_inference.seeds.contains(&span)
            && let Type::Optional(inner) = &raw
            && let Type::Inference(id) = inner.as_ref()
        {
            self.local_inference.nullable_seed_cells.insert(*id);
        }
        if !self.local_inference.collecting {
            let resolved = self.type_constraints.resolve(&raw).unwrap_or(Type::Invalid);
            if inference.owner.is_none()
                && inference.holes.iter().any(|hole| self.type_constraints.resolve(hole).map_or(true, |ty| ty.contains_inference()))
            {
                self.diagnostics.push(
                    Diagnostic::error("local type needs an annotation")
                        .with_code(DiagnosticCode::CheckLocalInference)
                        .with_label(Label::primary(
                            super::expr::expr_or_run_span_arena(program, initializer),
                            "no unique concrete type is established for this initializer",
                        )),
                );
                self.local_inference
                    .checked_bindings
                    .insert(span, Type::Invalid);
                return Type::Invalid;
            }
            if matches!(seed, LocalSeed::Map) {
                self.record_expr_type(
                    super::expr::expr_or_run_span_arena(program, initializer),
                    resolved.clone(),
                );
            }
            if matches!(seed, LocalSeed::Set)
                && let Type::Set(element) = &resolved
            {
                self.require_set_element_type(
                    element,
                    super::expr::expr_or_run_span_arena(program, initializer),
                );
                self.record_expr_type(
                    super::expr::expr_or_run_span_arena(program, initializer),
                    resolved.clone(),
                );
            }
            self.local_inference.checked_bindings.insert(span, resolved);
        }
        raw
    }
}

enum LocalSeed {
    List,
    Nullable,
    Map,
    Set,
    Aggregate,
}

pub(super) fn finalize_type(
    constraints: &crate::sema::constraints::TypeConstraints,
    ty: &mut Type,
    span: Span,
    reported: &mut BTreeSet<Span>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match constraints.resolve_in_place(ty) {
        Ok(()) if !ty.contains_inference() => {}
        Ok(()) => {
            let origins = constraints.unresolved(ty).unwrap_or_default();
            let new_origins = origins
                .into_iter()
                .filter(|origin| reported.insert(*origin))
                .collect::<Vec<_>>();
            if !new_origins.is_empty() {
                let mut diagnostic = Diagnostic::error("material type needs an annotation")
                    .with_code(DiagnosticCode::CheckLocalInference)
                    .with_label(Label::primary(
                        span,
                        "no unique concrete type is established for this value",
                    ));
                for origin in new_origins {
                    diagnostic = diagnostic
                        .with_label(Label::secondary(origin, "type inference started here"));
                }
                diagnostics.push(diagnostic);
            }
            *ty = Type::Invalid;
        }
        Err(_) => {
            if reported.insert(span) {
                diagnostics.push(
                    Diagnostic::error("type inference cannot publish this contract")
                        .with_code(DiagnosticCode::CheckLocalInference)
                        .with_label(Label::primary(
                            span,
                            "inference variables must belong to one bounded checking problem",
                        )),
                );
            }
            *ty = Type::Invalid;
        }
    }
}

fn local_seed_kind(
    program: &ArenaProgram,
    target: BindingTargetId,
    initializer: ArenaExprOrRun,
    mutable: bool,
) -> Option<LocalSeed> {
    let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind else {
        return None;
    };
    if name == "_" {
        return None;
    }
    let ArenaExprOrRun::Expr(expression) = initializer else {
        return None;
    };
    match program.arena.expr(expression).kind {
        ArenaExprKind::List(elements) if elements.is_empty() => Some(LocalSeed::List),
        ArenaExprKind::Null if mutable => Some(LocalSeed::Nullable),
        ArenaExprKind::Record(_) | ArenaExprKind::List(_)
            if contains_fresh_empty_list(program, expression) => Some(LocalSeed::Aggregate),
        _ if empty_map_call(program, initializer) => Some(LocalSeed::Map),
        _ if mutable && empty_set_call_receiver(program, initializer).is_some() => {
            Some(LocalSeed::Set)
        }
        _ => None,
    }
}

fn collect_empty_constructors(program: &ArenaProgram, expression: ExprId, output: &mut BTreeMap<ExprId, crate::sema::schemes::NonmaterialConstructor>) {
    match program.arena.expr(expression).kind {
        ArenaExprKind::List(elements) if elements.is_empty() => { output.insert(expression, crate::sema::schemes::NonmaterialConstructor::EmptyList); }
        ArenaExprKind::List(elements) => for element in program.arena.list_elements(elements) {
            if element.splice_span.is_none() { collect_empty_constructors(program, element.value, output); }
        },
        ArenaExprKind::Record(fields) => for field in program.arena.record_fields(fields) {
            match field.kind {
                ArenaRecordFieldKind::Named { value, .. } | ArenaRecordFieldKind::Spread { expr: value, .. } => collect_empty_constructors(program, value, output),
                _ => {}
            }
        },
        ArenaExprKind::Call { args, .. } if args.is_empty() => { output.insert(expression, crate::sema::schemes::NonmaterialConstructor::EmptyMap); }
        _ => {}
    }
}

fn collect_surviving_empty_constructors(
    program: &ArenaProgram,
    expression: ExprId,
    origins: &BTreeMap<Span, FreshAggregateOrigin>,
    candidates: &BTreeMap<ExprId, crate::sema::schemes::NonmaterialConstructor>,
    output: &mut BTreeSet<ExprId>,
) {
    if candidates.contains_key(&expression) { output.insert(expression); return; }
    match origins.get(&program.arena.expr(expression).span) {
        Some(FreshAggregateOrigin::List(elements)) if elements.is_empty() => { output.insert(expression); }
        Some(FreshAggregateOrigin::List(elements)) => for element in program.arena.list_elements(*elements) {
            if element.splice_span.is_none() { collect_surviving_empty_constructors(program, element.value, origins, candidates, output); }
        },
        Some(FreshAggregateOrigin::Record(fields)) => for expression in fields.values().flatten() {
            collect_surviving_empty_constructors(program, *expression, origins, candidates, output);
        },
        None => {}
    }
}

fn retain_nonmaterial_record_producers(
    program: &ArenaProgram,
    expression: ExprId,
    origins: &BTreeMap<Span, FreshAggregateOrigin>,
    constructors: &mut BTreeMap<ExprId, crate::sema::schemes::NonmaterialConstructor>,
) {
    let crate::syntax::arena::ArenaExprKind::Record(fields) = program.arena.expr(expression).kind else { return };
    for field in program.arena.record_fields(fields) {
        if let ArenaRecordFieldKind::Named { value, .. } | ArenaRecordFieldKind::Spread { expr: value, .. } = field.kind {
            retain_nonmaterial_record_producers(program, value, origins, constructors);
        }
    }
    if let Some(FreshAggregateOrigin::Record(fields)) = origins.get(&program.arena.expr(expression).span)
        && fields.values().flatten().any(|expression| constructors.contains_key(expression))
    {
        constructors.insert(expression, crate::sema::schemes::NonmaterialConstructor::Record);
    }
}

fn contains_fresh_empty_list(program: &ArenaProgram, expression: ExprId) -> bool {
    match program.arena.expr(expression).kind {
        ArenaExprKind::List(elements) => elements.is_empty() || program.arena.list_elements(elements)
            .any(|element| element.splice_span.is_none() && contains_fresh_empty_list(program, element.value)),
        ArenaExprKind::Record(fields) => program.arena.record_fields(fields).iter().any(|field| match field.kind {
            ArenaRecordFieldKind::Named { value, .. } | ArenaRecordFieldKind::Spread { expr: value, .. } => contains_fresh_empty_list(program, value),
            _ => false,
        }),
        _ => false,
    }
}

pub(super) fn empty_map_call(program: &ArenaProgram, initializer: ArenaExprOrRun) -> bool {
    empty_map_call_receiver(program, initializer).is_some()
}

fn empty_map_call_receiver(
    program: &ArenaProgram,
    initializer: ArenaExprOrRun,
) -> Option<crate::syntax::arena::ExprId> {
    empty_module_empty_call_receiver(program, initializer, "map")
}

fn empty_set_call_receiver(
    program: &ArenaProgram,
    initializer: ArenaExprOrRun,
) -> Option<crate::syntax::arena::ExprId> {
    empty_module_empty_call_receiver(program, initializer, "set")
}

fn empty_module_empty_call_receiver(
    program: &ArenaProgram,
    initializer: ArenaExprOrRun,
    module: &str,
) -> Option<crate::syntax::arena::ExprId> {
    let ArenaExprOrRun::Expr(expression) = initializer else {
        return None;
    };
    let ArenaExprKind::Call { callee, args } = program.arena.expr(expression).kind else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    let ArenaExprKind::Field { base, name } = program.arena.expr(callee).kind else {
        return None;
    };
    (name == "empty"
        && matches!(program.arena.expr(base).kind, ArenaExprKind::Ident(name) if name == module))
        .then_some(base)
}

pub(super) fn finalize_callable(
    constraints: &crate::sema::constraints::TypeConstraints,
    callable: &mut crate::sema::types::CallableType,
    span: Span,
    reported: &mut BTreeSet<Span>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for parameter in &mut callable.params {
        finalize_type(constraints, &mut parameter.ty, span, reported, diagnostics);
    }
    finalize_type(
        constraints,
        &mut callable.return_ty,
        span,
        reported,
        diagnostics,
    );
}

pub(super) fn finalize_projection(
    constraints: &crate::sema::constraints::TypeConstraints,
    projection: &mut crate::sema::projection::CheckedProjection,
    span: Span,
    reported: &mut BTreeSet<Span>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    finalize_type(
        constraints,
        &mut projection.value_type,
        span,
        reported,
        diagnostics,
    );
    match &mut projection.callable {
        Some(crate::sema::types::ModuleExportType::Value { ty, .. }) => {
            finalize_type(constraints, ty, span, reported, diagnostics)
        }
        Some(
            crate::sema::types::ModuleExportType::Callable { sig, .. },
        ) => finalize_callable(constraints, sig, span, reported, diagnostics),
        Some(crate::sema::types::ModuleExportType::GenericCallable { .. }) => {}
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SourceId, SourceMap};
    use crate::syntax::arena::BlockId;
    use crate::syntax::parser::Parser;
    use std::sync::Arc;

    fn with_checked_local_source(source: &str, check: impl FnOnce(&super::super::CheckOutput)) {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("main.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let arena = Arc::new(parsed.arena);
        let sources = Arc::new(sources);
        let _symbols = arena.symbol_owner().enter();
        let session = super::super::CheckSession::new(Arc::clone(&arena), sources);
        let view = session.view_for_root(arena.statements, source_id);
        let output = session.check_view(&view, super::super::CheckOptions::default());
        check(&output);
    }

    #[test]
    fn authored_callable_nonmaterial_constructors_prepare_before_raw_resolution() {
        for header in ["pure keep(payload: Int) -> Int", "proc keep(payload: Int) [] -> Int"] {
            let source = format!("{header} {{ let _ = {{items: map.empty(), payload}}; payload }}\n");
            with_checked_local_source(&source, |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let declaration = output.checked_declarations.functions.values().find(|declaration| declaration.name == "keep").unwrap();
                let scheme = declaration.scheme.as_ref().unwrap();
                assert_eq!(scheme.params()[0].ty, crate::sema::schemes::SchemeType::Ground(Type::Int));
                assert_eq!(scheme.return_type(), &crate::sema::schemes::SchemeType::Ground(Type::Int));
                assert!(output.expr_types.values().all(|ty| !ty.contains_inference() && !ty.contains_recovery()));
            });
        }
    }

    #[test]
    fn authored_callable_captures_keep_the_source_order_boundary() {
        for header in ["pure keep() -> Int", "proc keep() [] -> Int"] {
            for early in [true, false] {
                let result = if header.starts_with("proc") { "return result" } else { "result" };
                let function = format!("{header} {{ let _ = {{items: map.empty()}}; let result: Int = prefix; {result} }}\n");
                let source = if early { format!("let prefix = 7\n{function}") } else { format!("{function}let prefix = 7\n") };
                with_checked_local_source(&source, |output| {
                    if early {
                        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                        assert!(output.checked_declarations.functions.values().find(|declaration| declaration.name == "keep").unwrap().scheme.is_some());
                    } else {
                        assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckUnresolvedName)), "{:?}", output.diagnostics);
                    }
                });
            }
        }
    }

    #[test]
    fn authored_callable_recursive_effect_failure_keeps_publication_atomic() {
        for forbidden in [false, true] {
            let bound = if forbidden { "[]" } else { "[env]" };
            let source = format!("proc left(count: Int) {bound} -> Int {{ let _ = {{items: map.empty()}}; if count == 0 {{ 0 }} else {{ right(count - 1) }} }}\nproc right(count: Int) [env] -> Int {{ let _ = env.get(\"XSH_AUTHORED_COMPONENT\"); if count == 0 {{ 0 }} else {{ left(count - 1) }} }}\n");
            with_checked_local_source(&source, |output| {
                let peers = output.checked_declarations.functions.values().filter(|declaration| declaration.name == "left" || declaration.name == "right").collect::<Vec<_>>();
                assert_eq!(peers.len(), 2);
                if forbidden {
                    assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)), "{:?}", output.diagnostics);
                    assert!(peers.iter().all(|declaration| declaration.scheme.is_none() && declaration.scheme_body_facts.is_none()));
                    assert!(output.call_instantiations.is_empty());
                } else {
                    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                    assert!(peers.iter().all(|declaration| declaration.scheme.is_some()));
                }
            });
        }
    }

    #[test]
    fn nested_aggregate_heterogeneous_literals_preserve_dynamic_item_domain() {
        with_checked_local_source(
            "pure sample() -> Int { var state = {items: [\"first\", /first]}; state = {items: [/second, \"second\"]}; 7 }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let ty = output.local_binding_types.values().next().expect("state binding");
                let Type::Record(fields) = ty else { panic!("{ty:?}") };
                assert_eq!(fields.values().next(), Some(&Type::List(Box::new(Type::Any))));
            },
        );
    }

    #[test]
    fn nested_aggregate_authored_any_keeps_heterogeneous_writers() {
        with_checked_local_source(
            "type Dynamic = {items: List[Any]}\npure sample(flag: Bool) -> Int { var state: Dynamic = {items: []}; if flag { state = {items: [7]} } else { state = {items: [\"word\"]} }; 7 }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let ty = output.local_binding_types.values().next().expect("state binding");
                let Type::Record(fields) = ty else { panic!("{ty:?}") };
                assert_eq!(fields.values().next(), Some(&Type::List(Box::new(Type::Any))));
            },
        );
    }

    #[test]
    fn nested_aggregate_fresh_reset_does_not_inherit_spread_item_domain() {
        with_checked_local_source(
            "type Prefix = {items: List[Str], count: Int}\npure sample(prefix: Prefix) -> Int { var state = {...prefix, items: []}; state = {...state, items: [7]}; state.count }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let ty = output.local_binding_types.values().next().expect("state binding");
                let Type::Record(fields) = ty else { panic!("{ty:?}") };
                assert_eq!(fields.get(&super::super::Name::intern("items")), Some(&Type::List(Box::new(Type::Int))));
            },
        );
    }

    #[test]
    fn nested_aggregate_incompatible_shared_writers_are_rejected() {
        with_checked_local_source(
            "pure sample(flag: Bool) -> Int { var state = {items: []}; if flag { state = {items: [7]} } else { state = {items: [\"word\"]} }; 7 }",
            |output| assert!(!output.diagnostics.is_empty(), "incompatible writers accepted"),
        );
    }

    #[test]
    fn nested_aggregate_surviving_field_before_unrelated_spread_is_inferred() {
        with_checked_local_source(
            "pure sample() -> Int { var state = {items: [], ...{count: 7}}; state = {...state, items: [7]}; state.count }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let Type::Record(fields) = output.local_binding_types.values().next().expect("state binding") else { panic!("record binding") };
                assert_eq!(fields.get(&super::super::Name::intern("items")), Some(&Type::List(Box::new(Type::Int))));
            },
        );
    }

    #[test]
    fn nested_aggregate_overwritten_empty_field_stays_nonmaterial() {
        with_checked_local_source(
            "type Prefix = {items: List[Str], count: Int}\npure sample(prefix: Prefix) -> Int { var state = {items: [], ...prefix}; state = {...state, items: [\"word\"]}; state.count }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let Type::Record(fields) = output.local_binding_types.values().next().expect("state binding") else { panic!("record binding") };
                assert_eq!(fields.get(&super::super::Name::intern("items")), Some(&Type::List(Box::new(Type::Str))));
            },
        );
    }

    #[test]
    fn nested_aggregate_record_and_list_layers_share_writer_constraints() {
        for writers in ["state = {rows: [{items: [7]}]}; state = {rows: [{items: [8]}]}", "state = {rows: [{items: [8]}]}; state = {rows: [{items: [7]}]}"] {
            with_checked_local_source(
                &format!("type Row = {{items: List[Int]}}\npure sample() -> List[Row] {{ var state = {{rows: [{{items: []}}]}}; {writers}; state.rows }}"),
                |output| {
                    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                    let Type::Record(fields) = output.local_binding_types.values().next().expect("state binding") else { panic!("record binding") };
                    let Type::List(row) = fields.get(&super::super::Name::intern("rows")).expect("rows") else { panic!("rows list") };
                    let Type::Record(row) = row.as_ref() else { panic!("row record") };
                    assert_eq!(row.get(&super::super::Name::intern("items")), Some(&Type::List(Box::new(Type::Int))));
                },
            );
        }
    }

    #[test]
    fn nested_aggregate_uninformed_empty_remains_ambiguous() {
        with_checked_local_source(
            "pure sample() -> Int { var state = {items: []}; 7 }",
            |output| assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)), "{:?}", output.diagnostics),
        );
    }

    #[test]
    fn nested_aggregate_sourced_any_does_not_hide_independent_fresh_hole() {
        with_checked_local_source(
            "pure sample(raw: Any) -> Int { var state = {items: [], payload: raw}; state.items = [7]; 7 }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let Type::Record(fields) = output.local_binding_types.values().next().expect("state binding") else { panic!("record binding") };
                assert_eq!(fields.get(&super::super::Name::intern("items")), Some(&Type::List(Box::new(Type::Int))));
                assert_eq!(fields.get(&super::super::Name::intern("payload")), Some(&Type::Any));
            },
        );
    }

    #[test]
    fn nested_aggregate_sourced_any_collection_is_not_a_fresh_hole() {
        with_checked_local_source(
            "pure sample(raw: List[Any]) -> Int { var state = {items: raw}; state.items = [7]; 7 }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let Type::Record(fields) = output.local_binding_types.values().next().expect("state binding") else { panic!("record binding") };
                assert_eq!(fields.get(&super::super::Name::intern("items")), Some(&Type::List(Box::new(Type::Any))));
            },
        );
    }

    #[test]
    fn nested_aggregate_independent_int_and_string_resets_ignore_client_order() {
        let definitions = "type Words = {items: List[Str]}\ntype Numbers = {items: List[Int]}\npure numbers(prefix: Words) -> List[Int] { var state = {...prefix, items: []}; state = {...state, items: [7]}; state.items }\npure words(prefix: Numbers) -> List[Str] { var state = {...prefix, items: []}; state = {...state, items: [\"word\"]}; state.items }\n";
        for clients in ["let first = numbers({items: [\"prefix\"]})\nlet second = words({items: [7]})\n", "let first = words({items: [7]})\nlet second = numbers({items: [\"prefix\"]})\n"] {
            with_checked_local_source(&format!("{definitions}{clients}"), |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let items = output.local_binding_types.values().filter_map(|ty| match ty {
                    Type::Record(fields) => fields.get(&super::super::Name::intern("items")),
                    _ => None,
                }).collect::<Vec<_>>();
                assert!(items.contains(&&Type::List(Box::new(Type::Int))));
                assert!(items.contains(&&Type::List(Box::new(Type::Str))));
            });
        }
    }

    #[test]
    fn nested_aggregate_any_callable_is_not_inferred_from_invocation() {
        let source = "pure sample(raw: Any) -> Int { var state = {items: [], action: raw}; state.items = [7]; state.action(7) }";
        with_checked_local_source(source, |output| {
            let call = source.find("state.action(7)").expect("dynamic call");
            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.labels.iter().any(|label| label.span.start() >= call)), "{:?}", output.diagnostics);
        });
    }

    #[test]
    fn nested_aggregate_parameter_default_uses_same_constraint_problem() {
        with_checked_local_source(
            "pure sample(value: Int = { var state = {items: []}; state.items = [7]; 1 }) -> Int { value }\nlet result: Int = sample()",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                assert!(output.local_binding_types.values().any(|ty| matches!(ty, Type::Record(fields) if fields.get(&super::super::Name::intern("items")) == Some(&Type::List(Box::new(Type::Int))))));
            },
        );
    }

    #[test]
    fn nested_aggregate_default_nested_initializer_scopes_are_disposed() {
        with_checked_local_source(
            "pure sample(value: Int = { var outer = {items: [], count: { var inner = {items: []}; inner.items = [7]; 1 }}; outer.items = [\"word\"]; 1 }) -> Int { value }\nlet result: Int = sample()",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let items = output.local_binding_types.values().filter_map(|ty| match ty {
                    Type::Record(fields) => fields.get(&super::super::Name::intern("items")),
                    _ => None,
                }).collect::<Vec<_>>();
                assert!(items.contains(&&Type::List(Box::new(Type::Int))));
                assert!(items.contains(&&Type::List(Box::new(Type::Str))));
            },
        );
    }

    #[test]
    fn nested_aggregate_default_error_preserves_diagnostic_without_scope_escape() {
        with_checked_local_source(
            "pure sample(value: Int = { var state = {items: []}; state.missing = [7]; 1 }) -> Int { value }\nlet result: Int = sample()",
            |output| assert!(!output.diagnostics.is_empty(), "missing record field accepted"),
        );
    }

    #[test]
    fn nested_aggregate_constraint_probe_does_not_copy_active_origin_history() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut checker = Checker::new(super::super::CheckOptions::default());
            let initializer = Span::new(SourceId::new(0), 0, 1000);
            let mut origins = BTreeMap::new();
            for index in 1..256 {
                origins.insert(Span::new(initializer.source_id, index, index + 1), FreshAggregateOrigin::Record(BTreeMap::new()));
            }
            checker.local_inference.aggregate_scopes.push(AggregateSeedScope { initializer, origins });
            let probe = checker.constraint_probe();
            assert!(probe.local_inference.aggregate_scopes.is_empty());
            assert_eq!(checker.local_inference.aggregate_scopes[0].origins.len(), 255);
        });
    }

    #[test]
    fn nested_aggregate_signature_default_probe_preserves_local_inference() {
        with_checked_local_source(
            "pure sample(value = { var state = {items: []}; state.items = [7]; 1 }) -> Int { value }\nlet result: Int = sample()",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                assert!(output.local_binding_types.values().any(|ty| matches!(ty, Type::Record(fields) if fields.get(&super::super::Name::intern("items")) == Some(&Type::List(Box::new(Type::Int))))));
            },
        );
    }

    #[test]
    fn nested_aggregate_signal_hook_uses_ordinary_local_constraints() {
        with_checked_local_source(
            "on SIGINT [] { var state = {items: []}; state.items = [7] }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                assert!(output.local_binding_types.values().any(|ty| matches!(ty, Type::Record(fields) if fields.get(&super::super::Name::intern("items")) == Some(&Type::List(Box::new(Type::Int))))));
            },
        );
    }

    #[test]
    fn nested_aggregate_signal_hook_preserves_effect_bound() {
        with_checked_local_source(
            "on SIGINT [] { var state = {items: []}; state.items = [7]; let _ = time.now() }",
            |output| assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)), "{:?}", output.diagnostics),
        );
    }

    #[test]
    fn nested_aggregate_signal_hook_preserves_captured_storage_contract() {
        with_checked_local_source(
            "var captured = [7]\non SIGINT [] { var state = {items: []}; state.items = [7]; captured = [\"wrong\"] }",
            |output| assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)), "{:?}", output.diagnostics),
        );
    }

    fn assert_generic_payload_relationship(output: &super::super::CheckOutput, list_result: bool) {
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let declaration = output.checked_declarations.functions.values().next().expect("generic declaration");
        let scheme = declaration.scheme.as_ref().expect("published generic scheme");
        assert_eq!(scheme.type_parameters().len(), 1);
        let parameter = &scheme.params()[0].ty;
        assert!(matches!(parameter, crate::sema::schemes::SchemeType::Parameter(_)));
        if list_result {
            assert_eq!(scheme.return_type(), &crate::sema::schemes::SchemeType::List(Box::new(parameter.clone())));
        } else {
            assert_eq!(scheme.return_type(), parameter);
        }
    }

    #[test]
    fn nested_aggregate_generic_payload_with_ground_list_keeps_relationship() {
        with_checked_local_source(
            "pure sample(payload) { var state = {items: [7], payload}; state.items = [8]; state.payload }\nlet number: Int = sample(7)\nlet word: Str = sample(\"word\")",
            |output| assert_generic_payload_relationship(output, false),
        );
    }

    #[test]
    fn nested_aggregate_overwritten_hole_keeps_unrelated_generic_payload() {
        with_checked_local_source(
            "pure sample(payload) { let state = {items: [], ...{items: [7]}, payload}; state.payload }\nlet number: Int = sample(7)\nlet word: Str = sample(\"word\")",
            |output| {
                assert_generic_payload_relationship(output, false);
                assert_nonmaterial_empty_role(output, crate::sema::schemes::NonmaterialConstructor::EmptyList);
            },
        );
    }

    fn assert_nonmaterial_empty_role(output: &super::super::CheckOutput, expected: crate::sema::schemes::NonmaterialConstructor) {
        let declaration = output.checked_declarations.functions.values().next().expect("generic declaration");
        let facts = declaration.scheme_body_facts.as_ref().expect("published body facts");
        assert_eq!(facts.constructor_producers().len(), 1);
        let (&expression, &role) = facts.constructor_producers().iter().next().expect("nonmaterial constructor");
        assert_eq!(role, expected);
        assert!(!facts.expression_types().contains_key(&expression));
    }

    #[test]
    fn nested_aggregate_discarded_map_keeps_unrelated_generic_payload() {
        with_checked_local_source(
            "pure sample(payload) { let _ = map.empty(); payload }\nlet number: Int = sample(7)\nlet word: Str = sample(\"word\")",
            |output| {
                assert_generic_payload_relationship(output, false);
                assert_nonmaterial_empty_role(output, crate::sema::schemes::NonmaterialConstructor::EmptyMap);
            },
        );
    }

    #[test]
    fn nested_aggregate_overwritten_wrapper_keeps_unrelated_generic_payload() {
        with_checked_local_source(
            "pure sample(payload) { let state = {...{items: []}, ...{items: [7]}, payload}; state.payload }\nlet number: Int = sample(7)\nlet word: Str = sample(\"word\")",
            |output| {
                assert_generic_payload_relationship(output, false);
                let declaration = output.checked_declarations.functions.values().next().unwrap();
                let facts = declaration.scheme_body_facts.as_ref().unwrap();
                assert_eq!(facts.constructor_producers().len(), 2);
                assert!(facts.constructor_producers().values().any(|kind| *kind == crate::sema::schemes::NonmaterialConstructor::EmptyList));
                assert!(facts.constructor_producers().values().any(|kind| *kind == crate::sema::schemes::NonmaterialConstructor::Record));
                assert!(facts.constructor_producers().keys().all(|expression| !facts.expression_types().contains_key(expression)));
            },
        );
    }

    #[test]
    fn nested_aggregate_overwritten_empty_map_keeps_unrelated_generic_payload() {
        with_checked_local_source(
            "pure sample(payload) { let state = {items: map.empty(), ...{items: {[\"key\"]: 7}}, payload}; state.payload }\nlet number: Int = sample(7)\nlet word: Str = sample(\"word\")",
            |output| {
                assert_generic_payload_relationship(output, false);
                assert_nonmaterial_empty_role(output, crate::sema::schemes::NonmaterialConstructor::EmptyMap);
            },
        );
    }

    #[test]
    fn nested_aggregate_surviving_empty_map_stays_ambiguous() {
        with_checked_local_source(
            "pure sample(payload) { let state = {items: map.empty(), payload}; state.payload }",
            |output| assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)), "{:?}", output.diagnostics),
        );
    }

    #[test]
    fn nested_aggregate_solved_hole_keeps_unrelated_generic_payload() {
        with_checked_local_source(
            "pure sample(payload) { var state = {items: [], payload}; state.items = [7]; state.payload }\nlet number: Int = sample(7)\nlet word: Str = sample(\"word\")",
            |output| assert_generic_payload_relationship(output, false),
        );
    }

    #[test]
    fn nested_aggregate_linked_generic_hole_keeps_formal_relationship() {
        with_checked_local_source(
            "pure singleton(value) { var state = {items: []}; state.items = [value]; state.items }\nlet number: List[Int] = singleton(7)\nlet word: List[Str] = singleton(\"word\")",
            |output| assert_generic_payload_relationship(output, true),
        );
    }

    #[test]
    fn nested_aggregate_whole_empty_accumulator_matches_generic_relation() {
        with_checked_local_source(
            "pure singleton(value) { var items = []; items = [value]; items }\nlet number: List[Int] = singleton(7)\nlet word: List[Str] = singleton(\"word\")",
            |output| assert_generic_payload_relationship(output, true),
        );
    }

    #[test]
    fn nested_aggregate_higher_order_output_keeps_application_relationship() {
        with_checked_local_source(
            "pure number() -> Int { 7 }\npure word() -> Str { \"word\" }\npure collect(action) { var state = {items: []}; state.items = [action()]; state.items }\nlet numbers: List[Int] = collect(number)\nlet words: List[Str] = collect(word)",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let declaration = output.checked_declarations.functions.values().find(|declaration| declaration.name == "collect").expect("higher-order declaration");
                let scheme = declaration.scheme.as_ref().expect("higher-order scheme");
                let application = scheme.callable().requirements().applications.first().expect("formal callable application");
                assert_eq!(&application.callee, &scheme.params()[0].ty);
                assert!(application.arguments.is_empty());
                assert_eq!(scheme.return_type(), &crate::sema::schemes::SchemeType::List(Box::new(application.result.clone())));
            },
        );
    }

    #[test]
    fn nested_aggregate_output_only_hole_does_not_create_generic_result() {
        with_checked_local_source(
            "pure empty() { let state = {items: []}; state.items }",
            |output| assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)), "{:?}", output.diagnostics),
        );
    }

    #[test]
    fn nested_aggregate_captured_mutable_storage_is_not_generalized() {
        with_checked_local_source(
            "var captured = [1]\npure read_captured() { var state = {items: [], payload: captured}; state.items = [7]; state.payload }",
            |output| {
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let declaration = output.checked_declarations.functions.values().next().expect("capturing declaration");
                let scheme = declaration.scheme.as_ref().expect("capturing scheme");
                assert!(scheme.type_parameters().is_empty());
                assert!(scheme.row_parameters().is_empty());
                assert_eq!(scheme.return_type().closed_ground_type(), Some(Type::List(Box::new(Type::Int))));
            },
        );
    }

    #[test]
    fn nested_aggregate_unanchored_local_application_does_not_justify_hole() {
        with_checked_local_source(
            "pure fabricated() { var state = {items: []}; let action = state.items[0]; action() }",
            |output| assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)), "{:?}", output.diagnostics),
        );
    }

    #[test]
    fn generic_call_result_annotation_fact_is_finalized() {
        let source = concat!(
            "pure identity(value) { value }\n",
            "let generic_result = identity([42])\n",
            "pure collect() -> List[Int] {\n",
            "  var values = []\n",
            "  values = values.push(7)\n",
            "  values\n",
            "}\n",
            "pure wrap(value) {\n",
            "  let values = [value]\n",
            "  values\n",
            "}\n",
        );
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("main.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let arena = Arc::new(parsed.arena);
        let sources = Arc::new(sources);
        let _symbols = arena.symbol_owner().enter();
        let session = super::super::CheckSession::new(
            Arc::clone(&arena),
            Arc::clone(&sources),
        );
        let view = session.view_for_root(arena.statements, source_id);
        let output = session.check_view(&view, super::super::CheckOptions::default());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);

        let call_start = source.find("identity([42])").expect("generic call source");
        let call_span = Span::new(source_id, call_start, call_start + "identity([42])".len());
        let empty_list_start = source.find("[]").expect("empty list initializer");
        let empty_list_span = Span::new(source_id, empty_list_start, empty_list_start + 2);
        for initializer in [call_span, empty_list_span] {
            let fact = output
                .annotation_facts
                .iter()
                .find(|fact| match &fact.kind {
                    super::super::AnnotationFactKind::Binding {
                        initializer: fact_initializer,
                        ..
                    } => *fact_initializer == initializer,
                    _ => false,
                })
                .expect("resolved collection binding annotation fact");
            assert_eq!(fact.ty, Type::List(Box::new(Type::Int)));
        }
    }

    #[test]
    fn local_constraint_probe_does_not_copy_completed_body_binding_history() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut checker = Checker::new(super::super::CheckOptions::default());
            let span = Span::new(SourceId::new(0), 0, 1);
            let name = super::super::Name::intern("prior");
            for block in 0..256 {
                checker.block_exit_bindings.insert(
                    BlockId::from_index(block),
                    rustc_hash::FxHashMap::from_iter([(
                        name,
                        super::super::Binding::new(Type::Int, false),
                    )]),
                );
            }
            checker.expr_types.insert(span, Type::Int);
            let probe = checker.constraint_probe();
            let copied_bindings = probe
                .block_exit_bindings
                .values()
                .map(|scope| scope.len())
                .sum::<usize>();
            assert_eq!(
                copied_bindings, 0,
                "completed body bindings copied into the constraint probe"
            );
            assert_eq!(checker.block_exit_bindings.len(), 256);
            assert_eq!(checker.expr_types[&span], Type::Int);
        });
    }

    #[test]
    fn local_constraint_probe_does_not_copy_checked_expression_history() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut checker = Checker::new(super::super::CheckOptions::default());
            let contract = Span::new(SourceId::new(0), 0, 1);
            let name = super::super::Name::intern("captured");
            let function = crate::syntax::arena::FunctionDefId::from_index(0);
            checker.checked_declarations.functions.insert(
                function,
                super::super::CheckedFunctionDeclaration {
                    name,
                    namespace: None,
                    internal: false,
                    exported: false,
                    kind: super::super::CheckedFunctionKind::Proc,
                    signature: None,
                    scheme: None,
                    symbol_owner: crate::symbol::SymbolOwner::current().unwrap(),
                    schema_identities: Default::default(),
                    scheme_body_facts: None,
                    body: contract,
                    parameter_spans: Vec::new(),
                },
            );
            let type_id = crate::syntax::arena::TypeExprId::from_index(0);
            let type_definition = crate::syntax::arena::TypeDefId::from_index(0);
            checker
                .resolved_type_expressions
                .annotations
                .insert((None, type_id), Type::Int);
            checker
                .resolved_type_expressions
                .templates
                .insert((type_definition, type_id), Type::Str);
            checker
                .resolved_type_expressions
                .instantiations
                .insert(
                    contract,
                    super::super::InstantiatedTypeExpressions {
                        instance: crate::sema::constants::SchemaInstance {
                            definition: type_definition,
                            arguments: vec![Type::Bool],
                        },
                        expressions: rustc_hash::FxHashMap::from_iter([(type_id, Type::Bool)]),
                    },
                );
            checker.define(name, super::super::Binding::new(Type::Int, false), contract);
            checker.parameter_types.insert(contract, Type::Str);
            checker.function_return_types.insert(contract, Type::Bool);
            let variable = checker.type_constraints.fresh(contract);
            checker
                .type_constraints
                .constrain(&variable, &Type::Int, contract)
                .unwrap();
            for index in 0..256 {
                checker
                    .expr_types
                    .insert(Span::new(SourceId::new(1), index, index + 1), Type::Int);
            }
            let probe = checker.constraint_probe();
            assert_eq!(
                probe.expr_types.len(),
                0,
                "checked expression facts copied into the constraint probe"
            );
            assert!(probe.checked_declarations.functions.is_empty());
            assert!(probe.resolved_type_expressions.annotations.is_empty());
            assert!(probe.resolved_type_expressions.templates.is_empty());
            assert!(probe.resolved_type_expressions.instantiations.is_empty());
            assert_eq!(checker.expr_types.len(), 256);
            assert_eq!(checker.checked_declarations.functions.len(), 1);
            assert_eq!(
                checker.resolved_type_expressions.annotations[&(None, type_id)],
                Type::Int
            );
            assert_eq!(
                checker.resolved_type_expressions.templates[&(type_definition, type_id)],
                Type::Str
            );
            assert_eq!(
                checker.resolved_type_expressions.instantiations[&contract].expressions[&type_id],
                Type::Bool
            );
            assert_eq!(probe.lookup(name).unwrap().ty, Type::Int);
            assert_eq!(probe.parameter_types[&contract], Type::Str);
            assert_eq!(probe.function_return_types[&contract], Type::Bool);
            assert_eq!(
                probe.type_constraints.resolve(&variable).unwrap(),
                Type::Int
            );
        });
    }
}
