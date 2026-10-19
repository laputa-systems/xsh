use super::{BTreeMap, BTreeSet, CheckBundleView, Checker, Diagnostic, Label, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::constraints::TypeVariableId;
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaFunctionDef, ArenaProgram,
    ArenaStmtKind, BindingTargetId,
};
use std::sync::Arc;

#[derive(Clone, Default)]
pub(super) struct LocalInference {
    collecting: bool,
    pub(super) host_obligations: super::method::HostObligations,
    seeds: BTreeSet<Span>,
    bindings: BTreeMap<Span, Type>,
    nullable_seed_cells: BTreeSet<TypeVariableId>,
    solved_functions: BTreeSet<Span>,
    nonmaterial_expressions: BTreeSet<Span>,
    pub(super) checked_bindings: BTreeMap<Span, Type>,
    /// Expression facts recorded with inference identities since the last
    /// record-constructor publication; every other fact is already canonical.
    unresolved_expr_types: Vec<Span>,
}

impl LocalInference {
    pub(super) fn merge_prepared_owner(&mut self, other: &Self) {
        assert!(
            !other.collecting
                && other.bindings.is_empty()
                && other.nullable_seed_cells.is_empty()
                && other.solved_functions.is_empty()
                && other.nonmaterial_expressions.is_empty()
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
        for fact in &mut self.annotation_facts {
            let span = match fact.kind {
                super::AnnotationFactKind::Binding { span, .. }
                | super::AnnotationFactKind::DefaultedParam { span, .. } => span,
                super::AnnotationFactKind::InferredPureReturn { body }
                | super::AnnotationFactKind::ExportedProcReturn { body } => body,
            };
            finalize_type(constraints, &mut fact.ty, span, &mut reported, diagnostics);
        }
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
        if empty_map_call_receiver(program, expression).is_some_and(|base| {
            matches!(
                self.lookup_value_binding_for_expr(program, base),
                super::ValueBindingLookup::NoSite
            )
        }) {
            self.local_inference
                .nonmaterial_expressions
                .insert(super::expr::expr_or_run_span_arena(program, expression));
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
        let start = Span::new(body.source_id, body.start(), body.start());
        let end = Span::new(body.source_id, body.end(), body.end());
        let has_local_seeds = self
            .local_inference
            .seeds
            .range(start..end)
            .next()
            .is_some();
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
        let mut probe = self.constraint_probe();
        probe.local_inference.collecting = true;
        if stream {
            probe.check_stream_function_arena(program, source, definition);
        } else {
            probe.check_function_arena(program, source, definition, pure);
        }
        if let Err(error) = probe.solve_host_obligations() {
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

    /// Speculative checks retain declaration and lexical contracts but rebuild
    /// expression facts and branch joins before consuming them. Prior published
    /// facts carry no additional constraints, and copying them makes each probe
    /// pay for every earlier body, including captured module contracts.
    pub(super) fn constraint_probe(&mut self) -> Self {
        let checked_declarations = std::mem::take(&mut self.checked_declarations);
        let resolved_type_expressions = std::mem::take(&mut self.resolved_type_expressions);
        let block_exit_bindings = std::mem::take(&mut self.block_exit_bindings);
        let expr_types = std::mem::take(&mut self.expr_types);
        let projections = std::mem::take(&mut self.projections);
        let record_constructor_instances = std::mem::take(&mut self.record_constructor_instances);
        let requirement_targets = std::mem::take(&mut self.requirement_targets);
        let requirement_expected_targets = std::mem::take(&mut self.requirement_expected_targets);
        let condition_proofs = std::mem::take(&mut self.condition_proofs);
        let proven_nonnull_fallback_receivers =
            std::mem::take(&mut self.proven_nonnull_fallback_receivers);
        let static_callable_aliases = std::mem::take(&mut self.static_callable_aliases);
        let typed_callable_calls = std::mem::take(&mut self.typed_callable_calls);
        let selected_function_calls = std::mem::take(&mut self.selected_function_calls);
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
        let probe = self.clone();
        self.checked_declarations = checked_declarations;
        self.resolved_type_expressions = resolved_type_expressions;
        self.block_exit_bindings = block_exit_bindings;
        self.expr_types = expr_types;
        self.projections = projections;
        self.record_constructor_instances = record_constructor_instances;
        self.requirement_targets = requirement_targets;
        self.requirement_expected_targets = requirement_expected_targets;
        self.condition_proofs = condition_proofs;
        self.proven_nonnull_fallback_receivers = proven_nonnull_fallback_receivers;
        self.static_callable_aliases = static_callable_aliases;
        self.typed_callable_calls = typed_callable_calls;
        self.selected_function_calls = selected_function_calls;
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
        if let Some(ty) = self.local_inference.bindings.get(&span) {
            return self.type_constraints.resolve(ty).ok();
        }
        if self.boundary.current_return.is_none()
            || !self.local_inference.seeds.contains(&span)
            || !matches!(local_seed_kind(program, target, initializer, mutable), Some(LocalSeed::Set))
            || !self.has_unshadowed_empty_set(program, initializer)
        {
            return None;
        }
        let origin = super::expr::expr_or_run_span_arena(program, initializer);
        let ty = Type::Set(Box::new(self.type_constraints.fresh(origin)));
        self.local_inference.bindings.insert(span, ty.clone());
        self.type_constraints.resolve(&ty).ok()
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

    // Later parameter and return destinations can solve holes nested in a
    // binding. Preserve their identities until every source constraint is checked.
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
        let raw = if let Some(ty) = self.local_inference.bindings.get(&span) {
            ty.clone()
        } else {
            let origin = super::expr::expr_or_run_span_arena(program, initializer);
            let variable = self.type_constraints.fresh(origin);
            let ty = match seed {
                LocalSeed::List => Type::List(Box::new(variable)),
                LocalSeed::Nullable => Type::Optional(Box::new(variable)),
                LocalSeed::Map => Type::Map(
                    Box::new(variable),
                    Box::new(self.type_constraints.fresh(origin)),
                ),
                LocalSeed::Set => Type::Set(Box::new(variable)),
            };
            self.local_inference.bindings.insert(span, ty.clone());
            ty
        };
        if matches!(&seed, LocalSeed::Nullable)
            && self.local_inference.seeds.contains(&span)
            && let Type::Optional(inner) = &raw
            && let Type::Inference(id) = inner.as_ref()
        {
            self.local_inference.nullable_seed_cells.insert(*id);
        }
        if !self.local_inference.collecting {
            let resolved = self.type_constraints.resolve(&raw).unwrap_or(Type::Invalid);
            if resolved.contains_inference() {
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
        _ if empty_map_call(program, initializer) => Some(LocalSeed::Map),
        _ if mutable && empty_set_call_receiver(program, initializer).is_some() => {
            Some(LocalSeed::Set)
        }
        _ => None,
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
            crate::sema::types::ModuleExportType::Proc { sig, .. }
            | crate::sema::types::ModuleExportType::Pure { sig, .. },
        ) => finalize_callable(constraints, sig, span, reported, diagnostics),
        Some(crate::sema::types::ModuleExportType::GenericPure { .. }) => {}
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::arena::BlockId;

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
