use super::{BTreeMap, BTreeSet, Checker, Diagnostic, Label, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaFunctionDef, ArenaProgram, ArenaStmtKind, BindingTargetId, StmtId};

#[derive(Clone, Default)]
pub(super) struct LocalInference {
    collecting: bool,
    seeds: BTreeSet<Span>,
    bindings: BTreeMap<Span, Type>,
    solved_functions: BTreeSet<Span>,
    nonmaterial_expressions: BTreeSet<Span>,
    pub(super) checked_bindings: BTreeMap<Span, Type>,
    /// Expression facts recorded with inference identities since the last
    /// record-constructor publication; every other fact is already canonical.
    unresolved_expr_types: Vec<Span>,
}

impl Checker {
    pub(super) fn record_expr_type(&mut self, span: Span, ty: Type) {
        if ty.contains_inference() { self.local_inference.unresolved_expr_types.push(span); }
        self.expr_types.insert(span, ty);
    }

    /// Canonicalize the expression facts that still carry inference identities.
    /// A fact that remains unsolved becomes a recovery fact.
    pub(super) fn publish_unresolved_expr_types(&mut self) {
        for span in std::mem::take(&mut self.local_inference.unresolved_expr_types) {
            if let Some(ty) = self.expr_types.get_mut(&span) && ty.contains_inference() {
                *ty = self.type_constraints.resolve(ty).ok().filter(|resolved| !resolved.contains_inference()).unwrap_or(Type::Invalid);
            }
        }
    }

    /// Checked facts carry canonical types, never inference identities. A
    /// failed material contract becomes a recovery fact after its diagnostic.
    pub(super) fn resolve_checked_types(&mut self) {
        let mut reported = self.diagnostics.iter().filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference))
            .flat_map(|diagnostic| diagnostic.labels.iter().map(|label| label.span)).collect::<BTreeSet<_>>();
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
            finalize_type(constraints, &mut fact.input, *span, &mut reported, diagnostics);
            finalize_type(constraints, &mut fact.output, *span, &mut reported, diagnostics);
        }
        for (span, ty) in &mut self.function_return_types {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        for fact in &mut self.annotation_facts {
            let span = match fact.kind {
                super::AnnotationFactKind::Binding { span, .. } | super::AnnotationFactKind::DefaultedParam { span, .. } => span,
                super::AnnotationFactKind::InferredPureReturn { body } | super::AnnotationFactKind::ExportedProcReturn { body } => body,
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
            finalize_callable(constraints, &mut alias.signature, *span, &mut reported, diagnostics);
        }
        let fallback = Span::at(crate::source::SourceId::new(0), 0);
        for signature in self.procs.values_mut().chain(self.pures.values_mut()).chain(self.streams.values_mut())
            .chain(self.qualified_procs.values_mut()).chain(self.qualified_pures.values_mut()).chain(self.qualified_streams.values_mut())
        {
            for parameter in &mut signature.params {
                finalize_type(constraints, &mut parameter.ty, fallback, &mut reported, diagnostics);
            }
            finalize_type(constraints, &mut signature.return_ty, fallback, &mut reported, diagnostics);
        }
        for module in self.user_modules.values_mut() {
            for ty in module.values.values_mut().chain(module.resolved_types.values_mut()) {
                finalize_type(constraints, ty, fallback, &mut reported, diagnostics);
            }
            for signature in module.procs.values_mut().chain(module.pures.values_mut()).chain(module.streams.values_mut()) {
                for parameter in &mut signature.params {
                    finalize_type(constraints, &mut parameter.ty, fallback, &mut reported, diagnostics);
                }
                finalize_type(constraints, &mut signature.return_ty, fallback, &mut reported, diagnostics);
            }
        }
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if constraints.resolve_in_place(&mut binding.ty).is_err() || binding.ty.contains_inference() { binding.ty = Type::Invalid; }
                if let Some(ty) = &mut binding.unrefined_ty
                    && (constraints.resolve_in_place(ty).is_err() || ty.contains_inference()) {
                    *ty = Type::Invalid;
                }
            }
        }
    }

    pub(super) fn record_inert_local_discard(&mut self, program: &ArenaProgram, target: BindingTargetId, initializer: ArenaExprOrRun) {
        if matches!(program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "_") {
            self.record_inert_expression_discard(program, initializer);
        }
    }

    pub(super) fn record_inert_expression_discard(&mut self, program: &ArenaProgram, expression: ArenaExprOrRun) {
        if self.lookup(super::Name::intern("map")).is_none() && empty_map_call(program, expression) {
            self.local_inference.nonmaterial_expressions.insert(super::expr::expr_or_run_span_arena(program, expression));
        }
    }

    pub(super) fn is_inert_expression_discard(&self, span: Span) -> bool {
        self.local_inference.nonmaterial_expressions.contains(&span)
    }

    pub(super) fn prepare_local_inference(&mut self, program: &ArenaProgram) {
        for index in 0..program.arena.stmt_tags.len() {
            let statement = program.arena.stmt(StmtId::from_index(index));
            let (target, annotation, initializer, mutable) = match statement.kind {
                ArenaStmtKind::Let { target, ty, initializer } => (target, ty, initializer, false),
                ArenaStmtKind::Var { target, ty, initializer } => (target, ty, initializer, true),
                _ => continue,
            };
            if annotation.is_none() && local_seed_kind(program, target, initializer, mutable).is_some() {
                self.local_inference.seeds.insert(statement.span);
            }
        }
    }

    /// Gather constraints before checking concrete operations. The cloned
    /// checker owns speculative diagnostics and facts; only its substitutions
    /// and seed identities are retained for ordinary checking.
    pub(super) fn collect_function_local_constraints(&mut self, program: &ArenaProgram, source: &str, definition: &ArenaFunctionDef, pure: bool) {
        self.collect_callable_local_constraints(program, source, definition, pure, false);
    }

    pub(super) fn collect_stream_local_constraints(&mut self, program: &ArenaProgram, source: &str, definition: &ArenaFunctionDef) {
        self.collect_callable_local_constraints(program, source, definition, false, true);
    }

    fn collect_callable_local_constraints(&mut self, program: &ArenaProgram, source: &str, definition: &ArenaFunctionDef, pure: bool, stream: bool) {
        if self.local_inference.collecting { return; }
        let body = program.arena.span(program.arena.block(definition.body).span);
        if self.local_inference.solved_functions.contains(&body) { return; }
        let start = Span::new(body.source_id, body.start(), body.start());
        let end = Span::new(body.source_id, body.end(), body.end());
        if self.local_inference.seeds.range(start..end).next().is_none() { return; }
        let mut probe = self.constraint_probe();
        probe.local_inference.collecting = true;
        if stream { probe.check_stream_function_arena(program, source, definition); }
        else { probe.check_function_arena(program, source, definition, pure); }
        self.type_constraints = probe.type_constraints;
        self.local_inference.bindings = probe.local_inference.bindings;
        self.local_inference.solved_functions.insert(body);
    }

    /// Speculative checks retain declaration and lexical contracts but rebuild
    /// expression facts and branch joins before consuming them. Prior published
    /// facts carry no additional constraints, and copying them makes each probe
    /// pay for every earlier body, including captured module contracts.
    pub(super) fn constraint_probe(&mut self) -> Self {
        let block_exit_bindings = std::mem::take(&mut self.block_exit_bindings);
        let expr_types = std::mem::take(&mut self.expr_types);
        let projections = std::mem::take(&mut self.projections);
        let record_constructor_instances = std::mem::take(&mut self.record_constructor_instances);
        let requirement_targets = std::mem::take(&mut self.requirement_targets);
        let requirement_expected_targets = std::mem::take(&mut self.requirement_expected_targets);
        let condition_proofs = std::mem::take(&mut self.condition_proofs);
        let proven_nonnull_fallback_receivers = std::mem::take(&mut self.proven_nonnull_fallback_receivers);
        let static_callable_aliases = std::mem::take(&mut self.static_callable_aliases);
        let diagnostics = std::mem::take(&mut self.diagnostics);
        let annotation_facts = std::mem::take(&mut self.annotation_facts);
        let reveal_types = std::mem::take(&mut self.reveal_types);
        let stream_stage_types = std::mem::take(&mut self.stream_stage_types);
        let statement_positions = std::mem::take(&mut self.statement_positions);
        let pattern_test_types = std::mem::take(&mut self.pattern_test_types);
        let terminating_call_spans = std::mem::take(&mut self.terminating_call_spans);
        let assertion_effect_spans = std::mem::take(&mut self.assertion_effect_spans);
        let statement_expression_spans = std::mem::take(&mut self.statement_expression_spans);
        let membership_migration_spans = std::mem::take(&mut self.membership_migration_spans);
        let standard_call_spans = std::mem::take(&mut self.standard_call_spans);
        let statically_resolved_call_spans = std::mem::take(&mut self.statically_resolved_call_spans);
        let api_calls = std::mem::take(&mut self.api_calls);
        let argument_bindings = std::mem::take(&mut self.argument_bindings);
        let definitely_exiting_block_spans = std::mem::take(&mut self.definitely_exiting_block_spans);
        let checked_bindings = std::mem::take(&mut self.local_inference.checked_bindings);
        let probe = self.clone();
        self.block_exit_bindings = block_exit_bindings;
        self.expr_types = expr_types;
        self.projections = projections;
        self.record_constructor_instances = record_constructor_instances;
        self.requirement_targets = requirement_targets;
        self.requirement_expected_targets = requirement_expected_targets;
        self.condition_proofs = condition_proofs;
        self.proven_nonnull_fallback_receivers = proven_nonnull_fallback_receivers;
        self.static_callable_aliases = static_callable_aliases;
        self.diagnostics = diagnostics;
        self.annotation_facts = annotation_facts;
        self.reveal_types = reveal_types;
        self.stream_stage_types = stream_stage_types;
        self.statement_positions = statement_positions;
        self.pattern_test_types = pattern_test_types;
        self.terminating_call_spans = terminating_call_spans;
        self.assertion_effect_spans = assertion_effect_spans;
        self.statement_expression_spans = statement_expression_spans;
        self.membership_migration_spans = membership_migration_spans;
        self.standard_call_spans = standard_call_spans;
        self.statically_resolved_call_spans = statically_resolved_call_spans;
        self.api_calls = api_calls;
        self.argument_bindings = argument_bindings;
        self.definitely_exiting_block_spans = definitely_exiting_block_spans;
        self.local_inference.checked_bindings = checked_bindings;
        probe
    }

    pub(super) fn local_binding_expectation(&self, span: Span) -> Option<Type> {
        self.local_inference.bindings.get(&span).and_then(|ty| self.type_constraints.resolve(ty).ok())
    }

    /// Returned local values retain the binding's monomorphic identities. Resolve
    /// earlier contributions before comparison, and let a declared return
    /// anchor remaining holes through the same implicit Ok boundary as expressions.
    pub(super) fn resolve_local_tail_type(&mut self, ty: Type, expected: Option<&Type>, span: Span) -> Type {
        let resolved = self.type_constraints.resolve(&ty).unwrap_or(Type::Invalid);
        if resolved.contains_inference() && let Some(expected) = expected {
            let context = if resolved.is_result() { expected } else { expected.result_ok().unwrap_or(expected) };
            self.expect_type(context, &resolved, span);
        }
        self.type_constraints.resolve(&resolved).unwrap_or(Type::Invalid)
    }

    // Later parameter and return destinations can solve holes nested in a
    // binding. Preserve their identities until every source constraint is checked.
    pub(super) fn record_checked_local_binding(&mut self, program: &ArenaProgram, target: BindingTargetId, span: Span, ty: &Type) {
        if !self.local_inference.collecting
            && matches!(program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name != "_")
        {
            self.local_inference.checked_bindings.insert(span, ty.clone());
        }
    }

    pub(super) fn infer_local_binding(&mut self, program: &ArenaProgram, target: BindingTargetId, initializer: ArenaExprOrRun, mutable: bool, span: Span, ordinary: Type) -> Type {
        if self.current_return.is_none() { return ordinary; }
        let Some(seed) = local_seed_kind(program, target, initializer, mutable) else { return ordinary; };
        if matches!(seed, LocalSeed::Map) && (self.lookup(super::Name::intern("map")).is_some() || !matches!(ordinary, Type::Map(_, _))) {
            return ordinary;
        }
        let raw = if let Some(ty) = self.local_inference.bindings.get(&span) { ty.clone() } else {
            let origin = super::expr::expr_or_run_span_arena(program, initializer);
            let variable = self.type_constraints.fresh(origin);
            let ty = match seed {
                LocalSeed::List => Type::List(Box::new(variable)),
                LocalSeed::Nullable => Type::Optional(Box::new(variable)),
                LocalSeed::Map => Type::Map(Box::new(variable), Box::new(self.type_constraints.fresh(origin))),
            };
            self.local_inference.bindings.insert(span, ty.clone());
            ty
        };
        if !self.local_inference.collecting {
            let resolved = self.type_constraints.resolve(&raw).unwrap_or(Type::Invalid);
            if resolved.contains_inference() {
                self.diagnostics.push(Diagnostic::error("local type needs an annotation")
                    .with_code(DiagnosticCode::CheckLocalInference)
                    .with_label(Label::primary(super::expr::expr_or_run_span_arena(program, initializer), "no unique concrete type is established for this initializer")));
                self.local_inference.checked_bindings.insert(span, Type::Invalid);
                return Type::Invalid;
            }
            if matches!(seed, LocalSeed::Map) {
                self.record_expr_type(super::expr::expr_or_run_span_arena(program, initializer), resolved.clone());
            }
            self.local_inference.checked_bindings.insert(span, resolved);
        }
        raw
    }
}

enum LocalSeed { List, Nullable, Map }

pub(super) fn finalize_type(constraints: &crate::sema::constraints::TypeConstraints, ty: &mut Type, span: Span, reported: &mut BTreeSet<Span>, diagnostics: &mut Vec<Diagnostic>) {
    match constraints.resolve_in_place(ty) {
        Ok(()) if !ty.contains_inference() => {}
        Ok(()) => {
            let origins = constraints.unresolved(ty).unwrap_or_default();
            let new_origins = origins.into_iter().filter(|origin| reported.insert(*origin)).collect::<Vec<_>>();
            if !new_origins.is_empty() {
                let mut diagnostic = Diagnostic::error("material type needs an annotation")
                    .with_code(DiagnosticCode::CheckLocalInference)
                    .with_label(Label::primary(span, "no unique concrete type is established for this value"));
                for origin in new_origins { diagnostic = diagnostic.with_label(Label::secondary(origin, "type inference started here")); }
                diagnostics.push(diagnostic);
            }
            *ty = Type::Invalid;
        }
        Err(_) => {
            if reported.insert(span) {
                diagnostics.push(Diagnostic::error("type inference cannot publish this contract")
                    .with_code(DiagnosticCode::CheckLocalInference)
                    .with_label(Label::primary(span, "inference variables must belong to one bounded checking problem")));
            }
            *ty = Type::Invalid;
        }
    }
}

fn local_seed_kind(program: &ArenaProgram, target: BindingTargetId, initializer: ArenaExprOrRun, mutable: bool) -> Option<LocalSeed> {
    let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind else { return None; };
    if name == "_" { return None; }
    let ArenaExprOrRun::Expr(expression) = initializer else { return None; };
    match program.arena.expr(expression).kind {
        ArenaExprKind::List(elements) if elements.is_empty() => Some(LocalSeed::List),
        ArenaExprKind::Null if mutable => Some(LocalSeed::Nullable),
        _ if empty_map_call(program, initializer) => Some(LocalSeed::Map),
        _ => None,
    }
}

pub(super) fn empty_map_call(program: &ArenaProgram, initializer: ArenaExprOrRun) -> bool {
    let ArenaExprOrRun::Expr(expression) = initializer else { return false; };
    let ArenaExprKind::Call { callee, args } = program.arena.expr(expression).kind else { return false; };
    if !args.is_empty() { return false; }
    let ArenaExprKind::Field { base, name } = program.arena.expr(callee).kind else { return false; };
    name == "empty" && matches!(program.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "map")
}

pub(super) fn finalize_callable(constraints: &crate::sema::constraints::TypeConstraints, callable: &mut crate::sema::types::CallableType, span: Span, reported: &mut BTreeSet<Span>, diagnostics: &mut Vec<Diagnostic>) {
    for parameter in &mut callable.params {
        finalize_type(constraints, &mut parameter.ty, span, reported, diagnostics);
    }
    finalize_type(constraints, &mut callable.return_ty, span, reported, diagnostics);
}

pub(super) fn finalize_projection(constraints: &crate::sema::constraints::TypeConstraints, projection: &mut crate::sema::projection::CheckedProjection, span: Span, reported: &mut BTreeSet<Span>, diagnostics: &mut Vec<Diagnostic>) {
    finalize_type(constraints, &mut projection.value_type, span, reported, diagnostics);
    match &mut projection.callable {
        Some(crate::sema::types::ModuleExportType::Value { ty, .. }) => finalize_type(constraints, ty, span, reported, diagnostics),
        Some(crate::sema::types::ModuleExportType::Proc { sig, .. } | crate::sema::types::ModuleExportType::Pure { sig, .. }) => finalize_callable(constraints, sig, span, reported, diagnostics),
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
                checker.block_exit_bindings.insert(BlockId::from_index(block), rustc_hash::FxHashMap::from_iter([
                    (name, super::super::Binding::new(Type::Int, false)),
                ]));
            }
            checker.expr_types.insert(span, Type::Int);
            let probe = checker.constraint_probe();
            let copied_bindings = probe.block_exit_bindings.values().map(|scope| scope.len()).sum::<usize>();
            assert_eq!(copied_bindings, 0, "completed body bindings copied into the constraint probe");
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
            checker.define(name, super::super::Binding::new(Type::Int, false), contract);
            checker.parameter_types.insert(contract, Type::Str);
            checker.function_return_types.insert(contract, Type::Bool);
            let variable = checker.type_constraints.fresh(contract);
            checker.type_constraints.constrain(&variable, &Type::Int, contract).unwrap();
            for index in 0..256 {
                checker.expr_types.insert(Span::new(SourceId::new(1), index, index + 1), Type::Int);
            }
            let probe = checker.constraint_probe();
            assert_eq!(probe.expr_types.len(), 0, "checked expression facts copied into the constraint probe");
            assert_eq!(checker.expr_types.len(), 256);
            assert_eq!(probe.lookup(name).unwrap().ty, Type::Int);
            assert_eq!(probe.parameter_types[&contract], Type::Str);
            assert_eq!(probe.function_return_types[&contract], Type::Bool);
            assert_eq!(probe.type_constraints.resolve(&variable).unwrap(), Type::Int);
        });
    }
}
