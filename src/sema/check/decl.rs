#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, CheckBundleView, Checker, CheckedFunctionKind, CoreCommand, ErrorFamilyInfo,
    ErrorVariantInfo, FxHashMap, FxHashSet, Name, QualifiedName, RecordConstructors, Span,
    TagVariantInfo, Type, TypeAnnRef, TypeDefBody, UserModuleSig, api_spec,
};
use crate::diagnostic::{Diagnostic, DiagnosticCode, Label};
use crate::sema::check::{
    Binding, ContractParam, FunctionDeclarationId, FunctionParamSig, ModuleContractEntry,
    ModuleContractEntryKind, PendingCallable, PendingCallableShape, PendingFunctionParamSig,
    PendingFunctionSig, SchemaField, TagVariant, TypeAuthority, FunctionSig,
    standard_record_type,
};
use crate::sema::schemes::{
    CallableScheme, SchemeCallableKind, SchemeParameter, TypeScheme,
};
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaChild, ArenaModuleContractEntryKind, ArenaProgram, ArenaRange,
    ArenaStmtKind, ArenaTypeDef, ArenaTypeDefBody, ArenaUserModule, BlockId, ErrorDefId, ExprId,
    FunctionDefId, StmtId, SugarView, TypeDefId, TypeExprId,
};
use std::sync::Arc;

#[cfg(test)]
thread_local! {
    pub(super) static USER_MODULE_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn collect_body_type_facts(
    program: &ArenaProgram,
    body: BlockId,
    checked: &BTreeMap<Span, Type>,
    bindings: &super::ResolvedBindings,
    selected_calls: &BTreeMap<Span, super::CheckedFunctionTarget>,
) -> (
    BTreeMap<ExprId, Type>,
    BTreeMap<StmtId, Type>,
    std::collections::BTreeSet<Span>,
    std::collections::BTreeSet<super::BindingId>,
) {
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    enum BodyFactUse { Executable, SelectedCallee }

    let arena = &program.arena;
    let mut pending = vec![(ArenaChild::Block(body), BodyFactUse::Executable)];
    let mut expressions = FxHashSet::default();
    let mut statements = FxHashSet::default();
    let mut blocks = FxHashSet::default();
    let mut patterns = FxHashSet::default();
    let mut binding_targets = FxHashSet::default();
    let mut assign_targets = FxHashSet::default();
    let mut builder_blocks = FxHashSet::default();
    let mut expression_spans = std::collections::BTreeSet::new();
    let mut types = BTreeMap::new();
    let mut tail_types = BTreeMap::new();
    while let Some((child, usage)) = pending.pop() {
        match child {
            ArenaChild::Expr(id) if expressions.insert((id, usage)) => {
                expression_spans.insert(arena.expr(id).span);
                if let Some(ty) = checked.get(&arena.expr(id).span) {
                    types.insert(id, ty.clone());
                }
                if let crate::syntax::arena::ArenaExprKind::Call { callee, args } = arena.expr(id).kind
                    && selected_calls.contains_key(&arena.expr(id).span)
                {
                    pending.push((ArenaChild::Expr(callee), BodyFactUse::SelectedCallee));
                    for argument in arena.call_args(args) {
                        pending.push((ArenaChild::Expr(super::call_arg_expr_id_arena(&argument.kind)), usage));
                    }
                } else {
                    arena.for_each_expr_child(id, |child| pending.push((child, usage)));
                }
            }
            ArenaChild::Stmt(id) if statements.insert((id, usage)) => {
                let statement = arena.stmt(id);
                if matches!(statement.kind, ArenaStmtKind::TailBareIdent(_))
                    && let Some(ty) = checked.get(&statement.span)
                {
                    tail_types.insert(id, ty.clone());
                }
                if !matches!(
                    statement.kind,
                    ArenaStmtKind::ProcDef(_)
                        | ArenaStmtKind::PureDef(_)
                        | ArenaStmtKind::StreamDef(_)
                        | ArenaStmtKind::CliMain(_)
                        | ArenaStmtKind::SignalHook(_)
                ) {
                    arena.for_each_stmt_child(id, SugarView::Core, |child| pending.push((child, usage)));
                }
            }
            ArenaChild::Block(id) if blocks.insert((id, usage)) => {
                arena.for_each_block_child(id, |child| pending.push((child, usage)));
            }
            ArenaChild::Pattern(id) if patterns.insert((id, usage)) => {
                arena.for_each_pattern_child(id, |child| pending.push((child, usage)));
            }
            ArenaChild::BindingTarget(id) if binding_targets.insert((id, usage)) => {
                arena.for_each_binding_target_child(id, |child| pending.push((child, usage)));
            }
            ArenaChild::AssignTarget(id) if assign_targets.insert((id, usage)) => {
                arena.for_each_assign_target_child(id, |child| pending.push((child, usage)));
            }
            ArenaChild::BuilderBlock(id) if builder_blocks.insert((id, usage)) => {
                arena.for_each_builder_child(id, |child| pending.push((child, usage)));
            }
            ArenaChild::Expr(_)
            | ArenaChild::Stmt(_)
            | ArenaChild::Block(_)
            | ArenaChild::Pattern(_)
            | ArenaChild::BindingTarget(_)
            | ArenaChild::AssignTarget(_)
            | ArenaChild::TypeExpr(_)
            | ArenaChild::BuilderBlock(_) => {}
        }
    }
    let reads = expressions.iter().filter(|(_, usage)| *usage == BodyFactUse::Executable).filter_map(|(expression, _)| bindings.expressions.get(expression)).chain(statements.iter().filter(|(_, usage)| *usage == BodyFactUse::Executable).filter_map(|(statement, _)| bindings.tails.get(statement))).copied().collect();
    (types, tail_types, expression_spans, reads)
}

impl Checker {
    pub(super) fn generalize_pending_function(
        &mut self,
        program: &ArenaProgram,
        def_id: FunctionDefId,
    ) -> Option<()> {
        let def = program.arena.function_def(def_id);
        let declaration = FunctionDeclarationId::new(
            self.effect_declaration_id(program, def.body),
        );
        let pending = self.declaration_inference.pending(declaration)?.clone();
        if pending.generalized().is_some() {
            return Some(());
        }
        let shape = pending.provisional()?;
        let params = program.arena.params(def.params);
        let mut types = Vec::with_capacity(shape.params().len() + 1);
        for (param, pending_param) in params.iter().zip(shape.params()) {
            let span = program.arena.span(param.span);
            types.push(
                self.parameter_types
                    .get(&span)
                    .cloned()
                    .unwrap_or_else(|| pending_param.signature.ty.clone()),
            );
        }
        let body_span = program.arena.span(program.arena.block(def.body).span);
        types.push(
            self.function_return_types
                .get(&body_span)
                .cloned()
                .unwrap_or_else(|| shape.return_ty().clone()),
        );
        let (body_types, tail_types, expression_spans, read_bindings) =
            collect_body_type_facts(program, def.body, &self.expr_types, &self.resolved_bindings, &self.selected_function_calls);
        let calls = expression_spans.iter().filter_map(|&span| {
            self.pending_call_instantiations.get(&span).map(|(callable, instantiated)| {
                (span, (Arc::clone(callable), instantiated.clone()))
            })
        }).collect::<BTreeMap<_, _>>();
        let (generalized, generalized_body_types, generalized_tail_types, generalized_calls) =
            match self.type_constraints.generalize_callable_body_with_calls(
                &types,
                &body_types,
                &tail_types,
                &calls,
                0,
            )
        {
            Ok(generalized) => generalized,
            Err(crate::sema::constraints::SchemeOperationError::Build(
                crate::sema::schemes::SchemeBuildError::BodyTypeNotBound,
            )) => {
                self.error(
                    body_span,
                    "function body retains a type variable outside its callable signature",
                    DiagnosticCode::CheckLocalInference,
                );
                return None;
            }
            Err(_) => return None,
        };
        if !pending.generalizable()
            && (!generalized.type_parameters.is_empty() || !generalized.row_parameters.is_empty())
        {
            return None;
        }
        let quantified = !generalized.type_parameters.is_empty() || !generalized.row_parameters.is_empty();
        let registered_calls = body_types.keys().filter(|_| quantified).filter_map(|&expression| {
            let span = program.arena.expr(expression).span;
            let call = self.api_calls.get(&span)?;
            let crate::syntax::arena::ArenaExprKind::Call { callee, .. } = program.arena.expr(expression).kind else { return None };
            let name = match program.arena.expr(callee).kind {
                crate::syntax::arena::ArenaExprKind::Field { name, .. }
                | crate::syntax::arena::ArenaExprKind::Ident(name) => Arc::from(name.as_str().as_ref()),
                _ => return None,
            };
            let receiver = if call.receiver.is_some() {
                let crate::syntax::arena::ArenaExprKind::Field { base, .. } = program.arena.expr(callee).kind else { return None };
                generalized_body_types.get(&base).and_then(crate::sema::schemes::SchemeType::closed_ground_type)
            } else { None };
            let argument_checks = call.params.iter().enumerate().map(|(slot, parameter)| crate::sema::builtin_templates::registered_argument_check(receiver.as_ref(), slot, parameter)).collect::<Vec<_>>();
            let plan = crate::sema::schemes::RegisteredCallPlan {
                receiver: call.receiver,
                name,
                signature: call.sig,
                argument_slots: call.argument_slots.clone().into(),
                argument_checks: argument_checks.into(),
            };
            crate::sema::builtin_templates::registered_call_is_executable(&plan).then(|| (expression, Arc::new(plan)))
        }).collect::<BTreeMap<_, _>>();
        if !generalized.type_parameters.is_empty() || !generalized.row_parameters.is_empty() {
            let parameter_bindings = params
                .iter()
                .filter_map(|parameter| {
                    self.resolved_bindings
                        .declaration_sites
                        .get(&(program.arena.span(parameter.span), parameter.name))
                        .copied()
                })
                .collect::<super::FxHashSet<_>>();
            let pure_calls = generalized_body_types.keys().filter_map(|&expression| {
                let span = program.arena.expr(expression).span;
                let target = self.selected_function_calls.get(&span)?;
                let definition = program.arena.function_def(target.definition);
                if program.arena.params(definition.params).iter().any(|parameter| parameter.rest || parameter.default.is_some()) { return None; }
                let id = FunctionDeclarationId::new(super::EffectDeclarationId {
                    namespace: target.namespace,
                    body: program.arena.span(program.arena.block(definition.body).span),
                });
                let pure = self.checked_declarations.functions.get(&target.definition)
                    .is_some_and(|declaration| declaration.kind == CheckedFunctionKind::Pure)
                    || self.declaration_inference.pending(id).is_some_and(|signature| signature.kind() == SchemeCallableKind::Pure)
                    || self.declaration_inference.checked(id).is_some_and(|signature| signature.scheme.kind() == SchemeCallableKind::Pure)
                    || self.qualified_pures.values()
                        .any(|signature| signature.definition_target() == Some(*target)
                            && signature.scheme().kind() == SchemeCallableKind::Pure);
                pure.then_some(span)
            }).collect::<super::FxHashSet<_>>();
            let captures = generalized_body_types.keys()
                .filter_map(|expression| self.resolved_bindings.expressions.get(expression))
                .chain(generalized_tail_types.keys().filter_map(|statement| self.resolved_bindings.tails.get(statement)))
                .filter_map(|&id| {
                    let declaration = self.resolved_bindings.declarations.get(&id)?;
                    if declaration.kind != super::BindingKind::Runtime { return None; }
                    self.scopes.iter().rev().find_map(|scope| scope.get(&declaration.name).filter(|binding| binding.id == Some(id)))
                        .filter(|binding| !binding.mutable && !binding.ty.contains_inference() && !binding.ty.may_hold_resource())
                        .map(|_| id)
                }).collect::<super::FxHashSet<_>>();
            if !super::scheme_body::supports_generalized_body(
                program,
                def.body,
                &self.resolved_bindings,
                &parameter_bindings,
                &captures,
                &generalized_tail_types,
                &self.statement_positions,
                &pure_calls,
                &registered_calls,
                &generalized_body_types,
                &self.result_constructors,
                super::scheme_body::BodyCompletion::Function,
            ) {
                let message = "generic pure body requires immutable bindings and checked value constructors, branches, projections, or selected pure calls";
                let reported = self.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == Some(DiagnosticCode::CheckLocalInference)
                        && diagnostic.message == message
                        && diagnostic
                            .labels
                            .iter()
                            .any(|label| label.span == body_span)
                });
                if !reported {
                    self.error(body_span, message, DiagnosticCode::CheckLocalInference);
                }
                return None;
            }
        }
        let mut generalized_types = generalized.types;
        let return_type = generalized_types.pop()?;
        if pending.return_authority != TypeAuthority::Inferred
            && return_type.contains_parameter()
        {
            return None;
        }
        let params = shape
            .params()
            .iter()
            .zip(generalized_types)
            .map(|(parameter, ty)| {
                (parameter.authority == TypeAuthority::Inferred || !ty.contains_parameter())
                    .then(|| SchemeParameter {
                        name: parameter.signature.name,
                        ty,
                        schema_expectation: parameter.signature.schema_expectation.clone(),
                        defaulted: parameter.signature.defaulted,
                        rest: parameter.signature.rest,
                    })
            })
            .collect::<Option<Vec<_>>>()?;
        let callable = Arc::new(
            CallableScheme::new(
                generalized.type_parameters,
                generalized.row_parameters,
                params,
                return_type,
                shape.return_schema.clone(),
                shape.explicit_return,
            )
            .ok()?,
        );
        for (parameter, scheme_parameter) in
            program.arena.params(def.params).iter().zip(callable.params())
        {
            if scheme_parameter.ty.contains_parameter() {
                self.parameter_types
                    .remove(&program.arena.span(parameter.span));
            }
        }
        if callable.return_type().contains_parameter() {
            self.function_return_types.remove(&body_span);
            self.annotation_facts.retain(|fact| {
                !matches!(
                    &fact.kind,
                    super::AnnotationFactKind::InferredPureReturn { body }
                        if *body == body_span
                )
            });
        }
        let generalized_body_spans = generalized_body_types
            .iter()
            .filter_map(|(&expression, ty)| {
                ty.contains_parameter()
                    .then_some(program.arena.expr(expression).span)
            })
            .chain(generalized_tail_types.iter().filter_map(|(&statement, ty)| {
                ty.contains_parameter()
                    .then_some(program.arena.stmt(statement).span)
            }))
            .collect::<std::collections::BTreeSet<_>>();
        let generalized_bindings = program.arena.stmt_ids(program.arena.block(def.body).statements)
            .filter_map(|statement| match program.arena.stmt(statement).kind {
                ArenaStmtKind::Let { initializer: crate::syntax::arena::ArenaExprOrRun::Expr(value), .. }
                    if generalized_body_types.get(&value).is_some_and(|ty| ty.contains_parameter())
                        && self.local_inference.checked_bindings.get(&program.arena.stmt(statement).span).is_some_and(Type::contains_inference) => Some(program.arena.stmt(statement).span),
                _ => None,
            }).collect::<std::collections::BTreeSet<_>>();
        self.local_inference.checked_bindings.retain(|span, _| !generalized_bindings.contains(span));
        self.annotation_facts.retain(|fact| !matches!(fact.kind, super::AnnotationFactKind::Binding { span, .. } if generalized_bindings.contains(&span)));
        for &expression in registered_calls.keys() { self.api_calls.remove(&program.arena.expr(expression).span); }
        self.declaration_inference
            .pending_mut(declaration)?
            .generalize(callable, generalized_body_types, generalized_tail_types, registered_calls, read_bindings);
        for (span, instantiation) in generalized_calls {
            let previous = self.call_instantiations.insert(span, instantiation);
            assert!(previous.is_none(), "a call instantiation is published once");
        }
        for span in generalized_body_spans {
            self.expr_types.remove(&span);
        }
        Some(())
    }

    pub(super) fn local_function_target(
        &self,
        declaration: FunctionDeclarationId,
    ) -> Option<super::CheckedFunctionTarget> {
        self.declaration_inference
            .checked(declaration)
            .and_then(super::FunctionSig::definition_target)
            .or_else(|| {
                self.declaration_inference
                    .pending(declaration)
                    .and_then(PendingFunctionSig::definition_target)
            })
    }

    pub(super) fn local_function_return_schema(
        &self,
        declaration: FunctionDeclarationId,
    ) -> Option<super::super::constants::SchemaExpectation> {
        if let Some(signature) = self.declaration_inference.checked(declaration) {
            return signature.scheme().return_schema().cloned();
        }
        let pending = self.declaration_inference.pending(declaration)?;
        if let Some(shape) = pending.provisional() {
            return shape.return_schema.clone();
        }
        pending.generalized()?.return_schema().cloned()
    }

    pub(super) fn local_function_signature(
        &self,
        declaration: FunctionDeclarationId,
    ) -> Option<super::FunctionSig> {
        if let Some(signature) = self.declaration_inference.checked(declaration) {
            return Some(signature.clone());
        }
        let pending = self.declaration_inference.pending(declaration)?;
        let effects = self.declaration_effects(pending.effect_declaration());
        let inferred_effects = pending
            .effect_declaration()
            .is_some_and(|id| self.effect_graph.is_inferred(id));
        if let Some(callable) = pending.generalized() {
            let scheme = TypeScheme::with_callable(
                pending.kind(),
                Arc::clone(callable),
                effects,
                inferred_effects,
            )
            .ok()?;
            return Some(super::FunctionSig {
                effect_declaration: pending.effect_declaration(),
                is_alias: pending.is_alias,
                definition: pending.definition(),
                definition_target: pending.definition_target(),
                scheme: Arc::new(scheme),
            });
        }
        let shape = pending.provisional()?;
        let params = shape
            .params()
            .iter()
            .map(|param| {
                Some(FunctionParamSig {
                    ty: self.type_constraints.resolve(&param.signature.ty).ok()?,
                    ..param.signature.clone()
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let return_ty = self.type_constraints.resolve(shape.return_ty()).ok()?;
        let return_schema = shape.return_schema.clone();
        let explicit_return = shape.explicit_return;
        super::FunctionSig::ground(
            pending.kind(),
            pending.effect_declaration(),
            pending.is_alias,
            pending.definition(),
            pending.definition_target(),
            params,
            return_ty,
            return_schema,
            effects,
            inferred_effects,
            explicit_return,
        )
    }

    pub(super) fn update_pending_return_type(
        &mut self,
        program: &ArenaProgram,
        def: &crate::syntax::arena::ArenaFunctionDef,
        ty: Type,
    ) {
        let declaration = FunctionDeclarationId::new(
            self.effect_declaration_id(program, def.body),
        );
        if let Some(signature) = self.declaration_inference.pending_mut(declaration)
            && let Some(shape) = signature.provisional_mut()
        {
            shape.return_ty = ty;
        }
    }

    pub(super) fn checked_function_sig_arena(
        &mut self,
        program: &ArenaProgram,
        def_id: FunctionDefId,
        effect_fact: Option<&super::FunctionEffectFact>,
    ) -> Option<super::FunctionSig> {
        let def = program.arena.function_def(def_id);
        let effect_declaration = self.effect_declaration_id(program, def.body);
        let declaration = FunctionDeclarationId::new(effect_declaration);
        self.generalize_pending_function(program, def_id)?;
        let pending = self.declaration_inference.pending(declaration)?.clone();
        let callable = Arc::clone(pending.generalized()?);
        let effects = effect_fact.and_then(|fact| fact.effective.clone());
        let inferred_effects = effect_fact.is_some_and(|fact| fact.inferred);
        let scheme = Arc::new(
            TypeScheme::with_callable(
                pending.kind,
                Arc::clone(&callable),
                effects,
                inferred_effects,
            )
            .ok()?,
        );
        let scheme_body_facts = if callable.type_parameters().is_empty()
            && callable.row_parameters().is_empty()
        {
            None
        } else {
            match super::super::schemes::SchemeBodyFacts::new(
                Arc::clone(&scheme),
                pending.generalized_body_types()?.clone(),
                pending.generalized_tail_types()?.clone(),
            ) {
                Ok(facts) => Some(facts.with_registered_calls(pending.generalized_registered_calls()?.clone()).with_read_bindings(pending.generalized_read_bindings()?.clone())),
                Err(_) => {
                    let message = "generic function body has a type outside its checked scheme";
                    self.error(
                        program.arena.span(program.arena.block(def.body).span),
                        message,
                        DiagnosticCode::CheckLocalInference,
                    );
                    return None;
                }
            }
        };
        let signature = super::FunctionSig {
            effect_declaration: pending.effect_declaration,
            is_alias: pending.is_alias,
            definition: pending.definition,
            definition_target: pending.definition_target,
            scheme,
        };
        self.declaration_inference
            .mark_checked(declaration, signature.clone())
            .ok()?;
        self.publish_function_declaration(program, def_id, match pending.kind {
            SchemeCallableKind::Pure => CheckedFunctionKind::Pure,
            SchemeCallableKind::Proc => CheckedFunctionKind::Proc,
            SchemeCallableKind::Stream => CheckedFunctionKind::Stream,
        }, pending.exported, Some(&signature), scheme_body_facts);
        Some(signature)
    }

    pub(super) fn finalize_function_declarations(
        &mut self,
        program: &ArenaProgram,
        facts: &std::collections::BTreeMap<super::EffectDeclarationId, super::FunctionEffectFact>,
    ) {
        let declarations = self
            .declaration_inference
            .pending_entries()
            .filter_map(|(_, pending)| {
                let target = pending.definition_target()?;
                Some((target.definition, pending.effect_declaration()))
            })
            .collect::<Vec<_>>();
        for (def_id, effect_declaration) in declarations {
            let effect_fact = effect_declaration.and_then(|id| facts.get(&id));
            self.checked_function_sig_arena(program, def_id, effect_fact);
        }
    }

    pub(super) fn finalize_user_module_functions(
        &self,
        program: &ArenaProgram,
        module: &ArenaUserModule,
        signature: &mut UserModuleSig,
    ) {
        for statement in program.arena.stmt_ids(module.statements) {
            let ArenaStmtKind::Export(inner) = program.arena.stmt(statement).kind else {
                continue;
            };
            let (def_id, kind) = match program.arena.stmt(inner).kind {
                ArenaStmtKind::ProcDef(def_id) => (def_id, CheckedFunctionKind::Proc),
                ArenaStmtKind::PureDef(def_id) => (def_id, CheckedFunctionKind::Pure),
                ArenaStmtKind::StreamDef(def_id) => (def_id, CheckedFunctionKind::Stream),
                _ => continue,
            };
            let def = program.arena.function_def(def_id);
            let declaration = FunctionDeclarationId::new(self.effect_declaration_id(program, def.body));
            let Some(function) = self.declaration_inference.checked(declaration).cloned() else {
                continue;
            };
            match kind {
                CheckedFunctionKind::Proc => { signature.procs.insert(def.name, function); }
                CheckedFunctionKind::Pure => { signature.pures.insert(def.name, function); }
                CheckedFunctionKind::Stream => { signature.streams.insert(def.name, function); }
                CheckedFunctionKind::CliMain => unreachable!(),
            }
        }
    }

    /// Resolve exported type headers before the entry selects its argument domain.
    pub(super) fn module_type_headers(
        view: &CheckBundleView<'_>,
        options: super::CheckOptions,
        type_program: Arc<ArenaProgram>,
    ) -> FxHashMap<String, UserModuleSig> {
        let program = view.program();
        let record_constructors = RecordConstructors::collect_declarations_bundle(view);
        let mut headers = FxHashMap::default();
        for module in view.modules() {
            let mut checker = Self::new(options);
            checker.current_namespace = Some(module.name);
            checker.user_modules = Arc::new(headers.clone());
            checker.record_constructors = record_constructors.detached();
            let statements = program.module_statements(module).collect::<Vec<_>>();
            checker.collect_type_imports_arena(program, statements.iter().copied());
            let mut exported = Vec::new();
            for &statement in &statements {
                let stmt = program.arena.stmt(statement);
                let (kind, span, is_exported) = match stmt.kind {
                    ArenaStmtKind::Export(inner) => {
                        let inner = program.arena.stmt(inner);
                        (inner.kind, inner.span, true)
                    }
                    kind => (kind, stmt.span, false),
                };
                let ArenaStmtKind::TypeDef(id) = kind else {
                    continue;
                };
                let def = program.arena.type_def(id);
                checker.type_defs.insert(
                    def.name,
                    type_def_body_arena(type_program.clone(), id, Some(module.name)),
                );
                if is_exported {
                    exported.push((id, def.name, span));
                }
            }
            let mut header = UserModuleSig::default();
            for (id, name, span) in exported {
                let def = program.arena.type_def(id);
                header.types.insert(
                    name,
                    type_def_body_arena(type_program.clone(), id, Some(module.name)),
                );
                if def.type_parameters.is_empty() {
                    header.resolved_types.insert(name, checker.type_from_name(name, span));
                }
            }
            headers.insert(module.key.clone(), header);
        }
        headers
    }

    pub(super) fn collect_type_imports_arena(
        &mut self,
        program: &ArenaProgram,
        statements: impl IntoIterator<Item = StmtId>,
    ) {
        for stmt_id in statements {
            let stmt = program.arena.stmt(stmt_id);
            let ArenaStmtKind::Use(use_id) = stmt.kind else {
                continue;
            };
            let use_stmt = program.arena.use_stmt(use_id);
            if let Some(key) = use_stmt.resolved.as_deref() {
                let namespace = use_stmt
                    .alias
                    .or_else(|| program.arena.names(use_stmt.path).last());
                self.import_user_module_types(key, namespace, stmt.span, true);
            }
        }
    }

    fn check_test_declaration_names_arena(
        &mut self,
        program: &ArenaProgram,
        statements: &[StmtId],
        module: bool,
    ) {
        let mut tests = FxHashSet::default();
        for &statement in statements {
            let (kind, span) = exported_stmt_kind_arena(program, statement);
            if let ArenaStmtKind::ProcDef(id) = kind {
                let def = program.arena.function_def(id);
                if def.test_declaration && !tests.insert(def.name) && module {
                    self.error(
                        span,
                        "duplicate module test name",
                        DiagnosticCode::CheckDuplicateName,
                    );
                }
            }
        }
        for &statement in statements {
            let (kind, span) = exported_stmt_kind_arena(program, statement);
            let mut names = Vec::new();
            match kind {
                ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
                    append_binding_target_names_arena(program, target, &mut names);
                }
                ArenaStmtKind::Use(id) => {
                    let declaration = program.arena.use_stmt(id);
                    names.extend(
                        declaration
                            .alias
                            .or_else(|| program.arena.names(declaration.path).last()),
                    );
                }
                ArenaStmtKind::ProcDef(id)
                | ArenaStmtKind::PureDef(id)
                | ArenaStmtKind::StreamDef(id) => {
                    if module {
                        let definition = program.arena.function_def(id);
                        if !definition.test_declaration {
                            names.push(definition.name);
                        }
                    }
                }
                ArenaStmtKind::TypeDef(id) => {
                    if module {
                        names.push(program.arena.type_def(id).name);
                    }
                }
                ArenaStmtKind::ErrorDef(id) => {
                    if module {
                        names.push(program.arena.error_def(id).name);
                    }
                }
                ArenaStmtKind::Const { .. }
                | ArenaStmtKind::Export(_)
                | ArenaStmtKind::Assign { .. }
                | ArenaStmtKind::CliMain(_)
                | ArenaStmtKind::SignalHook(_)
                | ArenaStmtKind::Return(_)
                | ArenaStmtKind::Yield(_)
                | ArenaStmtKind::YieldDelegate(_)
                | ArenaStmtKind::Defer(_, _)
                | ArenaStmtKind::If { .. }
                | ArenaStmtKind::While { .. }
                | ArenaStmtKind::For { .. }
                | ArenaStmtKind::With { .. }
                | ArenaStmtKind::Loop { .. }
                | ArenaStmtKind::Guard { .. }
                | ArenaStmtKind::Sugar { .. }
                | ArenaStmtKind::Assert { .. }
                | ArenaStmtKind::Break { .. }
                | ArenaStmtKind::Continue
                | ArenaStmtKind::Exit(_)
                | ArenaStmtKind::Match { .. }
                | ArenaStmtKind::Command(_)
                | ArenaStmtKind::TailBareIdent(_)
                | ArenaStmtKind::Expr(_) => {}
            }
            if names.iter().any(|name| tests.contains(name)) {
                self.error(
                    span,
                    "name conflicts with a test declaration",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
        }
    }

    pub(super) fn collect_definitions_arena(
        &mut self,
        program: &ArenaProgram,
        type_program: Arc<ArenaProgram>,
        source: &str,
        statements: impl IntoIterator<Item = StmtId>,
    ) {
        let stmt_ids: Vec<StmtId> = statements.into_iter().collect();
        self.check_test_declaration_names_arena(program, &stmt_ids, false);
        let mut names = FxHashSet::default();
        for stmt_id in &stmt_ids {
            let (kind, span) = exported_stmt_kind_arena(program, *stmt_id);
            match kind {
                ArenaStmtKind::TypeDef(def_id) => {
                    let def = program.arena.type_def(def_id);
                    if is_builtin_or_standard_record_type_name(def.name.as_str()) {
                        self.error(
                            span,
                            "type name conflicts with a built-in type",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if self.type_defs.contains_key(&def.name) {
                        self.error(
                            span,
                            "type name conflicts with an imported type",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_enum_constructor_names(program, def, &mut names);
                    let body =
                        type_def_body_arena(type_program.clone(), def_id, self.current_namespace);
                    self.type_defs.insert(def.name, body.clone());
                    if let TypeDefBody::TagUnion(variants) = &body {
                        for variant in variants {
                            let field_types = variant
                                .fields
                                .iter()
                                .map(|field| self.type_from_ann(field))
                                .collect();
                            self.tag_variants.insert(
                                variant.name,
                                TagVariantInfo {
                                    type_name: variant.type_name,
                                    field_count: variant.fields.len(),
                                    field_types,
                                },
                            );
                        }
                    }
                }
                ArenaStmtKind::ErrorDef(def_id) => {
                    let def = program.arena.error_def(def_id);
                    if is_builtin_or_standard_record_type_name(def.name.as_str()) {
                        self.error(
                            span,
                            "error family name conflicts with a built-in type",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.register_error_family_arena(program, source, def_id);
                }
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if CoreCommand::from_name(&def.name.as_str()).is_some() {
                        self.error(
                            span,
                            "proc name conflicts with a core command",
                            DiagnosticCode::CheckCoreCommandShadow,
                        );
                    }
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                }
                _ => {}
            }
        }
        for stmt_id in stmt_ids {
            let (kind, _) = exported_stmt_kind_arena(program, stmt_id);
            match kind {
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    let sig = self.function_sig_arena(
                        program,
                        source,
                        def_id,
                        CheckedFunctionKind::Proc,
                        statement_is_exported_arena(program, stmt_id),
                    );
                    if !def.test_declaration {
                        self.procs.insert(def.name, sig);
                    }
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    let sig = self.function_sig_arena(
                        program,
                        source,
                        def_id,
                        CheckedFunctionKind::Pure,
                        statement_is_exported_arena(program, stmt_id),
                    );
                    self.pures.insert(def.name, sig);
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    let sig = self.function_sig_arena(
                        program,
                        source,
                        def_id,
                        CheckedFunctionKind::Stream,
                        statement_is_exported_arena(program, stmt_id),
                    );
                    self.streams.insert(def.name, sig);
                }
                ArenaStmtKind::ErrorDef(_) => {}
                _ => {}
            }
        }
    }

    fn check_enum_constructor_names(
        &mut self,
        program: &ArenaProgram,
        def: &ArenaTypeDef,
        names: &mut FxHashSet<Name>,
    ) {
        let ArenaTypeDefBody::TagUnion(variants) = def.body else {
            return;
        };
        for variant in program.arena.tag_variants(variants) {
            let span = program.arena.span(variant.span);
            self.check_standard_module_shadow(&variant.name.as_str(), span);
            if !names.insert(variant.name)
                || (self.current_namespace.is_none()
                    && self.tag_variants.contains_key(&variant.name))
            {
                self.error(
                    span,
                    "duplicate enum constructor name",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            if is_builtin_or_standard_record_type_name(variant.name.as_str()) {
                self.error(
                    span,
                    "enum constructor conflicts with a built-in type",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
        }
    }

    pub(super) fn check_user_module_arena(
        &mut self,
        view: &CheckBundleView<'_>,
        record: &super::module_record::ModuleRecord,
        type_program: Arc<ArenaProgram>,
        source: &str,
        module: &ArenaUserModule,
    ) -> UserModuleSig {
        #[cfg(test)]
        USER_MODULE_CHECKS.with(|checks| checks.set(checks.get() + 1));
        let program = view.program;
        self.current_namespace = Some(module.name);
        self.module_depth += 1;

        let stmt_ids = record.statements();
        self.check_test_declaration_names_arena(program, stmt_ids, true);
        if !module.internal {
            self.check_public_docs(program, module.statements, stmt_ids);
            self.check_public_result_types(program, stmt_ids);
        }
        for &statement in stmt_ids {
            if matches!(
                program.arena.stmt(statement).kind,
                ArenaStmtKind::CliMain(_)
            ) {
                self.error(
                    program.arena.stmt(statement).span,
                    "`cli main` is only permitted in the entry module",
                    DiagnosticCode::CheckCliEntry,
                );
            }
        }
        self.collect_type_imports_arena(program, stmt_ids.iter().copied());
        let mut names = FxHashSet::default();
        for &stmt_id in stmt_ids {
            let (kind, span) = exported_stmt_kind_arena(program, stmt_id);
            match kind {
                ArenaStmtKind::TypeDef(def_id) => {
                    let def = program.arena.type_def(def_id);
                    if is_builtin_or_standard_record_type_name(def.name.as_str()) {
                        self.error(
                            span,
                            "type name conflicts with a built-in type",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if self.type_defs.contains_key(&def.name) || !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module type name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_enum_constructor_names(program, def, &mut names);
                    self.type_defs.insert(
                        def.name,
                        type_def_body_arena(type_program.clone(), def_id, self.current_namespace),
                    );
                }
                ArenaStmtKind::ErrorDef(def_id) => {
                    let def = program.arena.error_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module type name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.register_error_family_arena(program, source, def_id);
                }
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    let sig = self.function_sig_arena(
                        program,
                        source,
                        def_id,
                        CheckedFunctionKind::Proc,
                        statement_is_exported_arena(program, stmt_id),
                    );
                    if !def.test_declaration {
                        self.procs.insert(def.name, sig);
                    }
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    let sig = self.function_sig_arena(
                        program,
                        source,
                        def_id,
                        CheckedFunctionKind::Pure,
                        statement_is_exported_arena(program, stmt_id),
                    );
                    self.pures.insert(def.name, sig);
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    let sig = self.function_sig_arena(
                        program,
                        source,
                        def_id,
                        CheckedFunctionKind::Stream,
                        statement_is_exported_arena(program, stmt_id),
                    );
                    self.streams.insert(def.name, sig);
                }
                _ => {}
            }
        }

        // Inferred bodies need constructor signatures before any statement
        // bodies are checked, just as root declarations do.
        for &stmt_id in stmt_ids {
            if let ArenaStmtKind::TypeDef(def_id) = exported_stmt_kind_arena(program, stmt_id).0 {
                let def = program.arena.type_def(def_id);
                if matches!(def.body, ArenaTypeDefBody::TagUnion(_)) {
                    self.check_type_def_arena(
                        program,
                        source,
                        def_id,
                        program.arena.stmt(stmt_id).span,
                    );
                }
            }
        }
        self.infer_default_parameter_types(program, source, stmt_ids);
        self.infer_local_pure_returns(view, source, stmt_ids);
        self.infer_default_parameter_types(program, source, stmt_ids);
        let mut exports = UserModuleSig::default();
        for &stmt_id in stmt_ids {
            let stmt = program.arena.stmt(stmt_id);
            if let ArenaStmtKind::SignalHook(hook_id) = stmt.kind {
                let hook = program.arena.signal_hook(hook_id).clone();
                self.check_signal_hook_arena(program, source, &hook, stmt.span);
                continue;
            }
            match &stmt.kind {
                ArenaStmtKind::Export(inner_id) => {
                    let previous_exported = self.current_exported;
                    self.current_exported = true;
                    let inner = program.arena.stmt(*inner_id);
                    match inner.kind {
                        ArenaStmtKind::Let {
                            target,
                            ty,
                            initializer,
                        }
                        | ArenaStmtKind::Const {
                            target,
                            ty,
                            initializer,
                        } => {
                            if matches!(
                                program.arena.binding_target(target).kind,
                                ArenaBindingTargetKind::Record { .. }
                            ) {
                                self.error(
                                    inner.span,
                                    "destructured exports are not supported",
                                    DiagnosticCode::CheckExportDestructure,
                                );
                            }
                            self.check_binding_arena(
                                program,
                                source,
                                target,
                                ty,
                                initializer,
                                false,
                                inner.span,
                            );
                            if let Some(name) = binding_target_simple_name_arena(program, target)
                                && let Some(binding) = self.lookup(name)
                            {
                                if let Some(alias) = &binding.callable_alias {
                                    if alias.pure {
                                        exports.pures.insert(name, alias.signature.clone());
                                    } else {
                                        exports.procs.insert(name, alias.signature.clone());
                                    }
                                } else {
                                    exports.values.insert(name, binding.ty.clone());
                                }
                            }
                        }
                        ArenaStmtKind::ProcDef(def_id) => {
                            let def = program.arena.function_def(def_id).clone();
                            self.check_function_arena(program, source, &def, false);
                        }
                        ArenaStmtKind::PureDef(def_id) => {
                            let def = program.arena.function_def(def_id).clone();
                            self.check_function_arena(program, source, &def, true);
                        }
                        ArenaStmtKind::StreamDef(def_id) => {
                            let def = program.arena.function_def(def_id).clone();
                            self.check_stream_function_arena(program, source, &def);
                        }
                        ArenaStmtKind::TypeDef(def_id) => {
                            let def = program.arena.type_def(def_id);
                            self.check_type_def_arena(program, source, def_id, inner.span);
                            exports.types.insert(
                                def.name,
                                type_def_body_arena(
                                    type_program.clone(),
                                    def_id,
                                    self.current_namespace,
                                ),
                            );
                            if def.type_parameters.is_empty() {
                                exports
                                    .resolved_types
                                    .insert(def.name, self.type_from_name(def.name, inner.span));
                            }
                            if let ArenaTypeDefBody::TagUnion(variants) = def.body {
                                for variant in program.arena.tag_variants(variants) {
                                    if let Some(info) =
                                        self.tag_variants.get(&variant.name).cloned()
                                    {
                                        exports.tag_variants.insert(variant.name, info);
                                    }
                                }
                            }
                        }
                        ArenaStmtKind::ErrorDef(def_id) => {
                            self.check_error_def_arena(program, source, def_id);
                            let def = program.arena.error_def(def_id);
                            if let Some(family) = self.error_families.get(&def.name).cloned() {
                                exports.error_families.insert(def.name, family);
                            }
                        }
                        ArenaStmtKind::SignalHook(hook_id) => {
                            let hook = program.arena.signal_hook(hook_id).clone();
                            self.check_signal_hook_arena(program, source, &hook, inner.span);
                        }
                        ArenaStmtKind::Use(_)
                        | ArenaStmtKind::Export(_)
                        | ArenaStmtKind::Var { .. }
                        | ArenaStmtKind::Assign { .. }
                        | ArenaStmtKind::CliMain(_)
                        | ArenaStmtKind::Return(_)
                        | ArenaStmtKind::Yield(_)
                        | ArenaStmtKind::YieldDelegate(_)
                        | ArenaStmtKind::Defer(_, _)
                        | ArenaStmtKind::If { .. }
                        | ArenaStmtKind::While { .. }
                        | ArenaStmtKind::For { .. }
                        | ArenaStmtKind::With { .. }
                        | ArenaStmtKind::Loop { .. }
                        | ArenaStmtKind::Guard { .. }
                        | ArenaStmtKind::Sugar { .. }
                        | ArenaStmtKind::Assert { .. }
                        | ArenaStmtKind::Break { .. }
                        | ArenaStmtKind::Continue
                        | ArenaStmtKind::Exit(_)
                        | ArenaStmtKind::Match { .. }
                        | ArenaStmtKind::Command(_)
                        | ArenaStmtKind::TailBareIdent(_)
                        | ArenaStmtKind::Expr(_) => {}
                    }
                    self.current_exported = previous_exported;
                }
                ArenaStmtKind::Use(use_id) => {
                    let use_stmt = program.arena.use_stmt(*use_id);
                    self.check_use_arena(
                        program,
                        use_stmt.path,
                        use_stmt.alias,
                        use_stmt.resolved.as_deref(),
                        use_stmt.searched_candidates.as_deref(),
                        stmt.span,
                    );
                }
                ArenaStmtKind::Let {
                    target,
                    ty,
                    initializer,
                }
                | ArenaStmtKind::Const {
                    target,
                    ty,
                    initializer,
                } => self.check_binding_arena(
                    program,
                    source,
                    *target,
                    *ty,
                    *initializer,
                    false,
                    stmt.span,
                ),
                ArenaStmtKind::TypeDef(def_id) => {
                    self.check_type_def_arena(program, source, *def_id, stmt.span);
                }
                ArenaStmtKind::ErrorDef(def_id) => {
                    self.check_error_def_arena(program, source, *def_id);
                }
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(*def_id).clone();
                    self.check_function_arena(program, source, &def, false);
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(*def_id).clone();
                    self.check_function_arena(program, source, &def, true);
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(*def_id).clone();
                    self.check_stream_function_arena(program, source, &def);
                }
                ArenaStmtKind::SignalHook(_) => {
                    unreachable!("signal hooks are handled before module declaration dispatch")
                }
                ArenaStmtKind::Var { .. }
                | ArenaStmtKind::Assign { .. }
                | ArenaStmtKind::CliMain(_)
                | ArenaStmtKind::Return(_)
                | ArenaStmtKind::Yield(_)
                | ArenaStmtKind::YieldDelegate(_)
                | ArenaStmtKind::Defer(_, _)
                | ArenaStmtKind::If { .. }
                | ArenaStmtKind::While { .. }
                | ArenaStmtKind::For { .. }
                | ArenaStmtKind::With { .. }
                | ArenaStmtKind::Loop { .. }
                | ArenaStmtKind::Guard { .. }
                | ArenaStmtKind::Sugar { .. }
                | ArenaStmtKind::Assert { .. }
                | ArenaStmtKind::Break { .. }
                | ArenaStmtKind::Continue
                | ArenaStmtKind::Exit(_)
                | ArenaStmtKind::Match { .. }
                | ArenaStmtKind::Command(_)
                | ArenaStmtKind::TailBareIdent(_)
                | ArenaStmtKind::Expr(_) => self.error(
                    stmt.span,
                    "imported modules cannot run top-level mutation or commands",
                    DiagnosticCode::CheckModuleTopLevel,
                ),
            }
        }

        exports
    }

    pub(super) fn function_sig_arena(
        &mut self,
        program: &ArenaProgram,
        _source: &str,
        def_id: FunctionDefId,
        kind: CheckedFunctionKind,
        exported: bool,
    ) -> FunctionDeclarationId {
        let def = program.arena.function_def(def_id);
        let effect_declaration = self.effect_declaration_id(program, def.body);
        let declaration = FunctionDeclarationId::new(effect_declaration);
        if kind == CheckedFunctionKind::Pure {
            for parameter in program.arena.params(def.params) {
                let span = program.arena.span(parameter.span);
                if parameter.annotation.is_none()
                    && parameter.default.is_none()
                    && !self.parameter_types.contains_key(&span)
                {
                    let ty = self.type_constraints.fresh_at_level(span, 1);
                    self.parameter_types.insert(span, ty);
                }
            }
        }
        let signature = PendingFunctionSig {
            kind: match kind {
                CheckedFunctionKind::Pure => SchemeCallableKind::Pure,
                CheckedFunctionKind::Proc => SchemeCallableKind::Proc,
                CheckedFunctionKind::Stream => SchemeCallableKind::Stream,
                CheckedFunctionKind::CliMain => unreachable!(),
            },
            exported,
            effect_declaration: Some(effect_declaration),
            is_alias: false,
            definition: Some(program.arena.span(program.arena.block(def.body).span)),
            definition_target: Some(super::CheckedFunctionTarget {
                definition: def_id,
                namespace: self.current_namespace,
                name: def.name,
            }),
            solver_level: 1,
            return_authority: if def.return_ty_defaulted {
                TypeAuthority::Inferred
            } else {
                TypeAuthority::Explicit
            },
            body_checked: false,
            generalizable: true,
            callable: PendingCallable::Provisional(PendingCallableShape {
                params: program
                    .arena
                    .params(def.params)
                    .iter()
                    .map(|param| PendingFunctionParamSig {
                        signature: FunctionParamSig {
                            name: param.name,
                            ty: self
                                .checked_parameter_type(program, param)
                                .unwrap_or_else(|| {
                                    param.annotation.map_or(Type::Unknown, |annotation| {
                                        self.type_from_arena(program, annotation)
                                    })
                                }),
                            schema_expectation: param.annotation.and_then(|annotation| {
                                self.record_constructors
                                    .annotation_expectation(
                                        &program.arena,
                                        annotation,
                                        self.current_namespace,
                                    )
                                    .ok()
                            }),
                            defaulted: param.default.is_some(),
                            rest: param.rest,
                        },
                        authority: if param.annotation.is_some() {
                            TypeAuthority::Explicit
                        } else if param.default.is_some()
                            && self.checked_parameter_type(program, param).is_some()
                        {
                            TypeAuthority::Default
                        } else {
                            TypeAuthority::Inferred
                        },
                    })
                    .collect(),
                return_ty: if def.return_ty_defaulted && kind == CheckedFunctionKind::Pure {
                    Type::Unknown
                } else if def.return_ty_defaulted
                    && kind == CheckedFunctionKind::Proc
                    && super::infer_return::proc_may_return_value(program, def)
                {
                    Type::Result(Box::new(Type::Unknown), Box::new(Type::Error))
                } else {
                    self.type_from_arena(program, def.return_ty)
                },
                return_schema: (!def.return_ty_defaulted)
                    .then(|| {
                        self.record_constructors
                            .annotation_expectation(
                                &program.arena,
                                def.return_ty,
                                self.current_namespace,
                            )
                            .ok()
                    })
                    .flatten(),
                explicit_return: !def.return_ty_defaulted,
            }),
        };
        self.declaration_inference
            .insert_pending(declaration, signature);
        self.publish_function_declaration(program, def_id, kind, exported, None, None);
        declaration
    }

}


pub(super) fn callable_type_from_function_signature(
    sig: &FunctionSig,
) -> Option<super::CallableType> {
    let scheme = sig.scheme();
    if !scheme.type_parameters().is_empty() || !scheme.row_parameters().is_empty() {
        return None;
    }
    let params = scheme
        .params()
        .iter()
        .map(|parameter| {
            Some(super::CallableParamType {
                name: parameter.name,
                ty: parameter.ty.closed_ground_type()?,
                defaulted: parameter.defaulted,
                rest: parameter.rest,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(super::CallableType {
        params,
        return_ty: Box::new(scheme.return_type().closed_ground_type()?),
        effects: scheme.effects().map(<[_]>::to_vec),
    })
}

/// A module names its own error families bare, while its importer spells
/// them through the namespace (`mod.E`), as constructors and patterns do.
fn qualify_imported_error_types(mut module: UserModuleSig, namespace: Name) -> UserModuleSig {
    let families = module
        .error_families
        .keys()
        .copied()
        .collect::<FxHashSet<_>>();
    if families.is_empty() {
        return module;
    }
    for ty in module.values.values_mut() {
        qualify_error_type(ty, namespace, &families);
    }
    for sig in module
        .procs
        .values_mut()
        .chain(module.pures.values_mut())
        .chain(module.streams.values_mut())
    {
        sig.scheme = Arc::new(sig.scheme.map_ground_types(|ty| {
            qualify_error_type(ty, namespace, &families);
        }));
    }
    module
}

fn qualify_error_type(ty: &mut Type, namespace: Name, families: &FxHashSet<Name>) {
    match ty {
        Type::ErrorFamily(family) | Type::ErrorVariant { family, .. }
            if families.contains(family) =>
        {
            *family = Name::intern(format!("{namespace}.{family}"));
        }
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
            qualify_error_type(inner, namespace, families)
        }
        Type::Map(left, right) | Type::Result(left, right) => {
            qualify_error_type(left, namespace, families);
            qualify_error_type(right, namespace, families);
        }
        Type::Record(fields) => {
            for field in Arc::make_mut(fields).values_mut() {
                qualify_error_type(field, namespace, families);
            }
        }
        Type::Validated(validated) => {
            validated.rewrite_base(|base| qualify_error_type(base, namespace, families))
        }
        _ => {}
    }
}

fn module_type_from_user_signature(module: &UserModuleSig) -> Type {
    let mut exports = std::collections::BTreeMap::new();
    for (name, ty) in &module.values {
        exports.insert(
            *name,
            super::ModuleExportType::Value {
                ty: ty.clone(),
                optional: false,
            },
        );
    }
    for (name, sig) in &module.procs {
        if let Some(sig) = callable_type_from_function_signature(sig) {
            exports.insert(
                *name,
                super::ModuleExportType::Proc { sig, optional: false },
            );
        }
    }
    for (name, sig) in &module.pures {
        if let Some(sig) = callable_type_from_function_signature(sig) {
            exports.insert(
                *name,
                super::ModuleExportType::Pure { sig, optional: false },
            );
        } else if !sig.scheme().type_parameters().is_empty()
            || !sig.scheme().row_parameters().is_empty()
        {
            exports.insert(
                *name,
                super::ModuleExportType::GenericPure { optional: false },
            );
        }
    }
    // The checker has every export of a statically imported module, so its
    // type is the module's whole surface.
    Type::Module(std::sync::Arc::new(crate::sema::types::ModuleType::exact(
        exports,
    )))
}

impl Checker {
    pub(super) fn import_user_module(
        &mut self,
        key: &str,
        alias: Option<Name>,
        path: &[Name],
        span: Span,
    ) {
        let Some(module) = self.user_modules.get(key).cloned() else {
            self.error(
                span,
                "unknown user module",
                DiagnosticCode::CheckUnknownModule,
            );
            return;
        };
        let Some(namespace) = alias.or_else(|| path.last().copied()) else {
            self.error(
                span,
                "empty module path",
                DiagnosticCode::CheckUnknownModule,
            );
            return;
        };
        let module = qualify_imported_error_types(module, namespace);
        self.import_user_module_types(key, Some(namespace), span, false);
        let mut binding = Binding::new(module_type_from_user_signature(&module), false);
        binding.static_namespace = true;
        self.define(namespace, binding, span);
        for (name, sig) in &module.procs {
            Arc::make_mut(&mut self.qualified_procs)
                .insert(QualifiedName::new(namespace, *name), sig.clone());
        }
        for (name, sig) in &module.pures {
            Arc::make_mut(&mut self.qualified_pures)
                .insert(QualifiedName::new(namespace, *name), sig.clone());
        }
        for (name, sig) in &module.streams {
            Arc::make_mut(&mut self.qualified_streams)
                .insert(QualifiedName::new(namespace, *name), sig.clone());
        }
    }

    pub(super) fn import_user_module_types(
        &mut self,
        key: &str,
        alias: Option<Name>,
        span: Span,
        diagnose: bool,
    ) {
        let Some(module) = self.user_modules.get(key).cloned() else {
            if diagnose {
                self.error(
                    span,
                    "unknown user module",
                    DiagnosticCode::CheckUnknownModule,
                );
            }
            return;
        };
        if let Some(alias) = alias {
            if diagnose {
                self.check_standard_module_shadow(&alias.as_str(), span);
                if self.type_namespaces.contains_key(&alias) {
                    self.error(
                        span,
                        "duplicate imported type namespace",
                        DiagnosticCode::CheckDuplicateName,
                    );
                }
            }
            self.type_namespaces
                .insert(alias, module.resolved_types.clone());
            for (name, info) in module.tag_variants {
                self.tag_variants
                    .insert(Name::intern(format!("{alias}.{name}")), info);
            }
            for (name, family) in module.error_families {
                let qualified = Name::intern(format!("{alias}.{name}"));
                for variant in family.variants.values() {
                    for facet in &variant.facets {
                        self.error_facets
                            .insert(Name::intern(format!("{alias}.{facet}")));
                    }
                }
                self.error_families.insert(qualified, family);
            }
            return;
        }
        let resolved_types = module.resolved_types.clone();
        for (name, body) in module.types {
            let body = match body {
                TypeDefBody::TagUnion(_) => body,
                _ => resolved_types
                    .get(&name)
                    .cloned()
                    .map(TypeDefBody::Resolved)
                    .unwrap_or(body),
            };
            if diagnose {
                if is_builtin_or_standard_record_type_name(name.as_str())
                    || self.type_defs.contains_key(&name)
                {
                    self.error(
                        span,
                        "duplicate imported type",
                        DiagnosticCode::CheckDuplicateName,
                    );
                }
                if let TypeDefBody::TagUnion(variants) = &body {
                    for variant in variants {
                        let field_types = variant
                            .fields
                            .iter()
                            .map(|field| self.type_from_ann(field))
                            .collect();
                        self.tag_variants.insert(
                            variant.name,
                            TagVariantInfo {
                                type_name: variant.type_name,
                                field_count: variant.fields.len(),
                                field_types,
                            },
                        );
                    }
                }
                self.type_defs.insert(name, body);
            } else {
                if let TypeDefBody::TagUnion(variants) = &body {
                    for variant in variants {
                        if !self.tag_variants.contains_key(&variant.name) {
                            let field_types = variant
                                .fields
                                .iter()
                                .map(|field| self.type_from_ann(field))
                                .collect();
                            self.tag_variants.insert(
                                variant.name,
                                TagVariantInfo {
                                    type_name: variant.type_name,
                                    field_count: variant.fields.len(),
                                    field_types,
                                },
                            );
                        }
                    }
                }
                self.type_defs.entry(name).or_insert(body);
            }
        }
        for (name, family) in module.error_families {
            if diagnose && self.error_families.contains_key(&name) {
                self.error(
                    span,
                    "duplicate imported error family",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            for variant in family.variants.values() {
                for facet in &variant.facets {
                    self.error_facets.insert(*facet);
                }
            }
            self.error_families.entry(name).or_insert(family);
        }
    }
}

/// Collects destructured names through the arena's binding-target child relation.
fn append_binding_target_names_arena(
    program: &ArenaProgram,
    target: crate::syntax::arena::BindingTargetId,
    names: &mut Vec<Name>,
) {
    let mut targets = vec![target];
    while let Some(target) = targets.pop() {
        match program.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => names.push(name),
            ArenaBindingTargetKind::Record { .. } => {
                program
                    .arena
                    .for_each_binding_target_child(target, |child| match child {
                        ArenaChild::BindingTarget(target) => targets.push(target),
                        ArenaChild::Stmt(_)
                        | ArenaChild::Expr(_)
                        | ArenaChild::Pattern(_)
                        | ArenaChild::Block(_)
                        | ArenaChild::AssignTarget(_)
                        | ArenaChild::TypeExpr(_)
                        | ArenaChild::BuilderBlock(_) => {
                            unreachable!("binding targets only have binding-target children")
                        }
                    });
            }
        }
    }
}

fn statement_is_exported_arena(program: &ArenaProgram, stmt_id: StmtId) -> bool {
    matches!(program.arena.stmt(stmt_id).kind, ArenaStmtKind::Export(_))
}

fn exported_stmt_kind_arena(program: &ArenaProgram, stmt_id: StmtId) -> (ArenaStmtKind, Span) {
    let stmt = program.arena.stmt(stmt_id);
    match stmt.kind {
        ArenaStmtKind::Export(inner) => {
            let inner = program.arena.stmt(inner);
            (inner.kind, inner.span)
        }
        kind => (kind, stmt.span),
    }
}

fn binding_target_simple_name_arena(
    program: &ArenaProgram,
    target: crate::syntax::arena::BindingTargetId,
) -> Option<Name> {
    match &program.arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(name) => Some(*name),
        ArenaBindingTargetKind::Record { .. } => None,
    }
}

fn type_def_body_arena(
    program: Arc<ArenaProgram>,
    id: crate::syntax::arena::TypeDefId,
    namespace: Option<Name>,
) -> TypeDefBody {
    let def = program.arena.type_def(id);
    if !def.type_parameters.is_empty() {
        return TypeDefBody::Parameterized(def.type_parameters.len());
    }
    if matches!(
        def.body,
        ArenaTypeDefBody::Alias(_) | ArenaTypeDefBody::RecordSchema(_)
    ) {
        return TypeDefBody::Declared(program, id);
    }
    match def.body {
        ArenaTypeDefBody::Alias(ty) => TypeDefBody::Alias(TypeAnnRef::new(program, ty)),
        ArenaTypeDefBody::RecordSchema(fields) => TypeDefBody::RecordSchema(
            program
                .arena
                .schema_fields(fields)
                .iter()
                .map(|field| SchemaField {
                    name: field.name,
                    ty: TypeAnnRef::new(program.clone(), field.ty),
                })
                .collect(),
        ),
        ArenaTypeDefBody::ModuleContract { entries, exact } => TypeDefBody::ModuleContract {
            exact,
            entries: program
                .arena
                .module_contract_entries(entries)
                .iter()
                .map(|entry| ModuleContractEntry {
                    name: entry.name,
                    optional: entry.optional,
                    kind: match &entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => {
                            ModuleContractEntryKind::Value(TypeAnnRef::new(program.clone(), *ty))
                        }
                        ArenaModuleContractEntryKind::Proc {
                            params,
                            effects,
                            return_ty,
                        } => ModuleContractEntryKind::Proc {
                            params: params_arena(program.clone(), *params),
                            effects: effects
                                .map(|effects| program.arena.effects(effects).collect()),
                            return_ty: TypeAnnRef::new(program.clone(), *return_ty),
                        },
                        ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            ModuleContractEntryKind::Pure {
                                params: params_arena(program.clone(), *params),
                                return_ty: TypeAnnRef::new(program.clone(), *return_ty),
                            }
                        }
                    },
                })
                .collect(),
        },
        ArenaTypeDefBody::TagUnion(variants) => TypeDefBody::TagUnion(
            program
                .arena
                .tag_variants(variants)
                .iter()
                .map(|variant| TagVariant {
                    type_name: crate::sema::wire_enums::nominal_enum_name(
                        namespace.or(program.root_nominal_namespace),
                        program.arena.type_def(id).name,
                    ),
                    name: variant.name,
                    fields: program
                        .arena
                        .extra_range(variant.fields)
                        .iter()
                        .map(|raw| {
                            TypeAnnRef::new(program.clone(), TypeExprId::from_index(*raw as usize))
                        })
                        .collect(),
                })
                .collect(),
        ),
    }
}

fn params_arena(program: Arc<ArenaProgram>, range: ArenaRange) -> Vec<ContractParam> {
    program
        .arena
        .params(range)
        .iter()
        .map(|param| ContractParam {
            source: param.clone(),
            name: param.name,
            program: program.clone(),
            ty: param.annotation.map(|ty| TypeAnnRef::new(program.clone(), ty)),
            defaulted: param.default.is_some(),
            rest: param.rest,
        })
        .collect()
}

impl Checker {
    pub(super) fn check_use_arena(
        &mut self,
        arena: &ArenaProgram,
        path: ArenaRange,
        alias: Option<Name>,
        resolved: Option<&str>,
        searched_candidates: Option<&[std::path::PathBuf]>,
        span: Span,
    ) {
        let path_names: Vec<Name> = arena.arena.names(path).collect();
        if let Some(key) = resolved {
            if alias.is_none()
                && let Some(last) = path_names.last()
                && last.as_str().contains('-')
            {
                let suggested = last.as_str().replace('-', "_");
                let message = format!(
                    "module path segment `{last}` contains a hyphen; \
                             use `as {suggested}` to give it a valid binding name"
                );
                self.error(span, &message, DiagnosticCode::CheckHyphenatedModuleAlias);
                return;
            }
            self.import_user_module(key, alias, &path_names, span);
            return;
        }
        if path_names.len() != 1 || api_spec().module(&path_names[0].as_str()).is_none() {
            let mut diagnostic = Diagnostic::error("only standard modules can be imported")
                .with_code(DiagnosticCode::CheckUnknownModule)
                .with_label(Label::primary(span, "unknown module"));
            if let Some(paths) = searched_candidates {
                diagnostic.labels.push(Label::secondary(
                    span,
                    format!(
                        "module search tried {}",
                        paths.iter()
                            .map(|path| format!("`{}`", path.display()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
            self.diagnostics.push(diagnostic);
            return;
        }
        if let Some(alias) = alias {
            let message = format!(
                "standard module `{}` cannot be aliased as `{alias}`",
                path_names[0]
            );
            self.error(span, &message, DiagnosticCode::CheckStandardModuleAlias);
        }
    }

    pub(super) fn check_type_def_arena(
        &mut self, arena: &ArenaProgram, source: &str, id: TypeDefId, span: Span,
    ) {
        let def = arena.arena.type_def(id);
        let checked = self.check_type_declaration_contents(arena, source, id, def, span);
        self.publish_type_declaration(arena, id, checked);
    }

    fn check_type_declaration_contents(
        &mut self, arena: &ArenaProgram, source: &str, id: TypeDefId,
        def: &ArenaTypeDef, span: Span,
    ) -> bool {
        // A nominal identity names one record type. Resolution rejects the
        // same declaration where the type is used; a declaration nothing
        // uses is reported here.
        if def.nominal
            && (!matches!(def.body, ArenaTypeDefBody::RecordSchema(_))
                || !def.type_parameters.is_empty())
        {
            self.error(
                span,
                &format!(
                    "`nominal type {}` must be a record schema without type parameters",
                    def.name
                ),
                DiagnosticCode::CheckSchema,
            );
            return false;
        }
        // A range bounds one integer type, so its declaration has no
        // parameter to apply.
        if def.bounds.is_some() && !def.type_parameters.is_empty() {
            self.error(
                span,
                &format!(
                    "`type {}` has a range, so it takes no type parameters",
                    def.name
                ),
                DiagnosticCode::CheckSchema,
            );
            return false;
        }
        if !def.type_parameters.is_empty() {
            let mut names = FxHashSet::default();
            for parameter in arena.arena.names(def.type_parameters) {
                if !names.insert(parameter) {
                    self.error(
                        span,
                        "duplicate type parameter",
                        DiagnosticCode::CheckTypeParameters,
                    );
                }
                if Type::builtin_from_name(&parameter.as_str()).is_some()
                    || standard_record_type(&parameter.as_str()).is_some()
                    || matches!(
                        parameter.as_str().as_str(),
                        "List"
                            | "Map"
                            | "Stream"
                            | "Result"
                            | "Module"
                            | "Optional"
                            | "Unknown"
                            | "Union"
                            | "NonEmpty"
                            | "Set"
                    )
                {
                    self.error(
                        span,
                        "type parameter name is reserved",
                        DiagnosticCode::CheckTypeParameters,
                    );
                }
            }
            let namespace = self.current_namespace;
            let checked = match self
                .record_constructors
                .template_field_types(&arena.arena, def, namespace)
            {
                Ok(fields) => {
                    if !self.collecting_effects && let ArenaTypeDefBody::RecordSchema(schema_fields) = def.body {
                        for field in arena.arena.schema_fields(schema_fields) {
                            if let Some(ty) = fields.get(&field.name) {
                                self.resolved_type_expressions.templates.insert((id, field.ty), ty.clone());
                            }
                        }
                    }
                    if let ArenaTypeDefBody::RecordSchema(schema_fields) = def.body {
                        let mut field_names = FxHashSet::default();
                        for field in arena.arena.schema_fields(schema_fields) {
                            if !field_names.insert(field.name) {
                                self.error(
                                    arena.arena.span(field.span),
                                    "duplicate schema field",
                                    DiagnosticCode::CheckDuplicateRecordField,
                                );
                            }
                            if let Some(default) = field.default {
                                let allowed = self
                                    .record_constructors
                                    .definition(namespace, def.name)
                                    .and_then(|id| self.record_constructors.defaults(id))
                                    .is_some_and(|defaults| defaults.contains_key(&field.name));
                                if !allowed {
                                    self.error(arena.arena.expr(default).span, "record default must be a literal or a previously declared immutable literal constant", DiagnosticCode::CheckRecordDefault);
                                }
                                if let Some(expected) = fields.get(&field.name) {
                                    let actual = self.check_expr_arena(
                                        arena,
                                        source,
                                        default,
                                        Some(expected),
                                    );
                                    self.expect_type(
                                        expected,
                                        &actual,
                                        arena.arena.expr(default).span,
                                    );
                                }
                            }
                        }
                        if field_names.is_empty() {
                            self.error(
                                span,
                                "record schema needs at least one field",
                                DiagnosticCode::CheckSchema,
                            );
                        }
                    }
                    true
                }
                Err(error) => { self.error(span, &error.message, error.code); false }
            };
            return checked;
        }
        match &def.body {
            ArenaTypeDefBody::Alias(ty) => {
                let base = self.type_from_arena(arena, *ty);
                // Resolution rejects the same bounds where the type is used;
                // a declaration nothing uses is reported here.
                if let Err(error) =
                    crate::sema::constants::bounded_alias_type(&arena.arena, def, base)
                {
                    self.error(span, &error.message, error.code);
                }
            }
            ArenaTypeDefBody::RecordSchema(fields) => {
                let field_list = arena.arena.schema_fields(*fields);
                let mut names = FxHashSet::default();
                for field in field_list {
                    if !names.insert(field.name) {
                        let field_span = arena.arena.span(field.span);
                        self.error(
                            field_span,
                            "duplicate schema field",
                            DiagnosticCode::CheckDuplicateRecordField,
                        );
                    }
                    let expected = self.type_from_arena(arena, field.ty);
                    if let Some(default) = field.default {
                        let definition = self
                            .record_constructors
                            .definition(self.current_namespace, def.name);
                        let allowed = definition
                            .and_then(|id| self.record_constructors.defaults(id))
                            .is_some_and(|defaults| defaults.contains_key(&field.name));
                        if !allowed {
                            self.error(arena.arena.expr(default).span,
                                "record default must be a literal or a previously declared immutable literal constant",
                                DiagnosticCode::CheckRecordDefault);
                        }
                        let actual = self.check_expr_arena(arena, source, default, Some(&expected));
                        self.expect_type(&expected, &actual, arena.arena.expr(default).span);
                    }
                }
                if field_list.is_empty() {
                    self.error(
                        span,
                        "record schema needs at least one field",
                        DiagnosticCode::CheckSchema,
                    );
                }
            }
            ArenaTypeDefBody::ModuleContract { entries, .. } => {
                let entry_list = arena.arena.module_contract_entries(*entries);
                let mut names = FxHashSet::default();
                for entry in entry_list {
                    if !names.insert(entry.name) {
                        let entry_span = arena.arena.span(entry.span);
                        self.error(
                            entry_span,
                            "duplicate module contract export",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    match &entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => {
                            self.type_from_arena(arena, *ty);
                        }
                        ArenaModuleContractEntryKind::Proc {
                            params, return_ty, ..
                        }
                        | ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            for param in arena.arena.params(*params) {
                                self.infer_checked_parameter(arena, source, param);
                            }
                            self.type_from_arena(arena, *return_ty);
                        }
                    }
                }
                if entry_list.is_empty() {
                    self.error(
                        span,
                        "module contract needs at least one export",
                        DiagnosticCode::CheckModuleContract,
                    );
                }
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                for variant in arena.arena.tag_variants(*variants) {
                    let field_types: Vec<Type> = arena
                        .arena
                        .extra_range(variant.fields)
                        .iter()
                        .map(|raw| {
                            let ty_id = TypeExprId::from_index(*raw as usize);
                            self.type_from_arena(arena, ty_id)
                        })
                        .collect();
                    self.tag_variants.insert(
                        variant.name,
                        TagVariantInfo {
                            type_name: crate::sema::wire_enums::nominal_enum_name(
                                self.current_namespace.or(arena.root_nominal_namespace),
                                def.name,
                            ),
                            field_count: field_types.len(),
                            field_types,
                        },
                    );
                }
            }
        }
        true
    }

    pub(super) fn check_error_def_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: ErrorDefId,
    ) {
        let def = arena.arena.error_def(id);
        let mut variants = FxHashSet::default();
        for variant in arena.arena.error_variants(def.variants) {
            if !variants.insert(variant.name) {
                let variant_span = arena.arena.span(variant.span);
                self.error(
                    variant_span,
                    "duplicate error variant",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            let mut fields = FxHashSet::default();
            for field in arena.arena.error_fields(variant.fields) {
                if !fields.insert(field.name) {
                    let field_span = arena.arena.span(field.span);
                    self.error(
                        field_span,
                        "duplicate error payload field",
                        DiagnosticCode::CheckDuplicateRecordField,
                    );
                }
                self.type_from_arena(arena, field.ty);
            }
        }
        self.register_error_family_arena(arena, source, id);
    }

    pub(super) fn register_error_family_arena(
        &mut self,
        arena: &ArenaProgram,
        _source: &str,
        id: ErrorDefId,
    ) {
        let def = arena.arena.error_def(id);
        let mut variants = BTreeMap::new();
        for variant in arena.arena.error_variants(def.variants) {
            let fields = arena
                .arena
                .error_fields(variant.fields)
                .iter()
                .map(|field| (field.name, self.type_from_arena(arena, field.ty)))
                .collect::<Vec<_>>();
            let facets: Vec<Name> = arena.arena.names(variant.facets).collect();
            for facet in &facets {
                self.error_facets.insert(*facet);
            }
            variants.insert(variant.name, ErrorVariantInfo::declared(fields, facets));
        }
        self.error_families
            .insert(def.name, ErrorFamilyInfo { variants });
        self.publish_error_declaration(arena, id);
    }
}

pub(super) fn is_builtin_or_standard_record_type_name(name: impl AsRef<str>) -> bool {
    let name = name.as_ref();
    Type::from_name(name) != Type::Unknown || standard_record_type(name).is_some()
}

#[cfg(test)]
mod imported_pure_tests {
    use super::*;
    use crate::sema::check::module_driver::CheckSession;
    use crate::source::SourceMap;
    use crate::syntax::arena::ArenaProgramBuilder;
    use crate::syntax::parser::Parser;

    fn checked_imported_call(globals: &str, prefix: &str, argument: &str) -> (Arc<ArenaProgram>, super::super::CheckOutput, BlockId, super::super::BindingId) {
        let entry_text = format!("use api\n{globals}\npure invoke(row) {{ {prefix}api.required({argument}) }}\n");
        let module_text = "##! Required operation.\n## Returns the supplied integer.\nexport pure required(value: Int) -> Int { value }\n## An ordinary module value.\nexport let marker = 7\n";
        let mut sources = SourceMap::new();
        let entry_source = sources.add_file("entry.xsh", &entry_text);
        let module_source = sources.add_file("api.xsh", module_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let entry = Parser::parse_source_into_arena_builder(entry_source, &entry_text, &mut builder);
        let module = Parser::parse_source_into_arena_builder(module_source, module_text, &mut builder);
        assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let namespace = builder.symbol_owner().intern("api");
        builder.push_arena_module("api".to_string(), namespace, module.statements);
        for statement in builder.statement_ids(entry.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from("api"));
            }
        }
        let program = Arc::new(builder.finish_with_statements(entry.statements));
        let _symbols = program.symbol_owner().enter();
        let session = CheckSession::new(Arc::clone(&program), Arc::new(sources));
        let output = session.check(entry.statements, entry_source, super::super::CheckOptions::default());
        let body = program.arena.stmt_ids(entry.statements).find_map(|statement| match program.arena.stmt(statement).kind {
            ArenaStmtKind::PureDef(definition) => Some(program.arena.function_def(definition).body),
            _ => None,
        }).unwrap();
        let bindings = super::super::ResolvedBindings::collect(&program);
        let binding = *bindings.declarations.iter().find(|(_, declaration)| declaration.name == namespace && declaration.namespace.is_none()).unwrap().0;
        (program, output, body, binding)
    }

    #[test]
    fn selected_imported_callee_facts_keep_types_without_a_runtime_namespace_read() {
        let (program, output, body, namespace) = checked_imported_call("", "", "row.value");
        let _symbols = program.symbol_owner().enter();
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let facts = output.checked_declarations.functions.values().find(|declaration| declaration.name == "invoke").unwrap().scheme_body_facts.as_ref().unwrap();
        assert!(!facts.read_bindings().contains(&namespace));
        let bindings = super::super::ResolvedBindings::collect(&program);
        let (types, _, spans, _) = collect_body_type_facts(&program, body, &output.expr_types, &bindings, &output.selected_function_calls);
        let callee = *bindings.expressions.iter().find(|(_, binding)| **binding == namespace).unwrap().0;
        assert!(spans.contains(&program.arena.expr(callee).span));
        assert!(types.keys().any(|expression| output.selected_function_calls.contains_key(&program.arena.expr(*expression).span)));
        let (unselected_types, _, _, unselected_reads) = collect_body_type_facts(&program, body, &output.expr_types, &bindings, &BTreeMap::new());
        assert_eq!(types, unselected_types);
        assert!(unselected_reads.contains(&namespace));
        let parameter = *bindings.declarations.iter().find(|(_, declaration)| declaration.name == "row").unwrap().0;
        assert!(facts.read_bindings().contains(&parameter));
    }

    #[test]
    fn selected_imported_calls_retain_real_namespace_mutable_and_resource_reads() {
        for (globals, prefix, argument) in [
            ("", "let saved = api; ", "row.value"),
            ("", "", "api.marker"),
            ("var captured = 1", "let saved = captured; ", "row.value"),
            ("let captured = fs.root(/)?", "let saved = captured; ", "row.value"),
        ] {
            let (program, output, body, namespace) = checked_imported_call(globals, prefix, argument);
            let _symbols = program.symbol_owner().enter();
            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)), "{globals} {prefix} {argument}: {:?}", output.diagnostics);
            let bindings = super::super::ResolvedBindings::collect(&program);
            let (_, _, _, reads) = collect_body_type_facts(&program, body, &output.expr_types, &bindings, &output.selected_function_calls);
            if globals.is_empty() {
                assert!(reads.contains(&namespace));
            } else {
                let captured = *bindings.declarations.iter().find(|(_, declaration)| declaration.name == "captured").unwrap().0;
                assert!(reads.contains(&captured));
            }
        }
    }
}
