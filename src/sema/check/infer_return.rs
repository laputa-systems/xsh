use crate::sema::types::CallableKind;
use crate::sema::callable_application::{inferred_proc_return_envelope, join_error_types};
use super::{
    AnnotationFact, AnnotationFactKind, BTreeMap, CheckBundleView, Checker, FxHashSet, Name, Span,
    Type,
};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaChild, ArenaExprKind, ArenaExprOrRun,
    ArenaProgram, ArenaRecordFieldKind, ArenaStmtKind, BindingTargetId, ExprId, FunctionDefId,
    StmtId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CheckedProcReturnSite {
    pub(super) application: crate::sema::constraints::CallableApplicationReference,
    pub(super) envelope: u32,
}

#[derive(Clone)]
pub(super) struct InferredReturn {
    pub(super) ty: Type,
    pub(super) span: Span,
    pub(super) origin: super::stream_origins::StreamOrigins,
    pub(super) schema: Option<crate::sema::constants::SchemaExpectation>,
    pub(super) failure_only_result: bool,
}

struct ReturnDeclaration {
    name: Name,
    names: Vec<Name>,
    span: Span,
    function: Option<(FunctionDefId, CallableKind)>,
    statement: StmtId,
}

impl ReturnDeclaration {
    fn definition(&self) -> Option<FunctionDefId> { self.function.map(|(definition, _)| definition) }
    fn kind(&self) -> Option<CallableKind> { self.function.map(|(_, kind)| kind) }
}

#[derive(Clone, Default)]
pub(super) struct ReturnInferenceIndex {
    references: Vec<(Span, Name, Option<ExprId>)>,
}

impl ReturnInferenceIndex {
    pub(super) fn merge_owner(&mut self, other: &Self) {
        self.references.extend(other.references.iter().copied());
        self.references.sort_unstable();
        self.references.dedup();
    }

    pub(super) fn collect(view: &CheckBundleView<'_>) -> Self {
        let program = view.program;
        let expressions = view.expr_ids();
        let statements = view.stmt_ids();
        let rows = program.arena.source_rows();
        let mut references = Vec::new();
        let mut reference_exprs = expressions.clone();
        reference_exprs.extend(rows.record_expr_ids());
        reference_exprs.sort_unstable();
        reference_exprs.dedup();
        for id in reference_exprs {
            let expr = program.arena.expr(id);
            if view.contains_source(expr.span.source_id)
                && let ArenaExprKind::Ident(name) = expr.kind
            {
                references.push((expr.span, name, Some(id)));
            }
            if let ArenaExprKind::Record(fields) = expr.kind {
                for field in program.arena.record_fields(fields) {
                    if let ArenaRecordFieldKind::Shorthand { name, span } = &field.kind {
                        references.push((program.arena.span(*span), *name, None));
                    }
                }
            }
        }
        for &statement_id in &statements {
            let stmt = program.arena.stmt(statement_id);
            if let ArenaStmtKind::TailBareIdent(name) = stmt.kind {
                references.push((stmt.span, name, None));
            }
        }
        references.sort_unstable_by_key(|(span, _, _)| (span.source_id, span.start()));
        Self { references }
    }
}

impl Checker {
    pub(super) fn failure_only_proc_unit_envelope(
        &mut self,
        program: &ArenaProgram,
        def: &crate::syntax::arena::ArenaFunctionDef,
    ) -> Type {
        match self.type_from_arena(program, def.return_ty) {
            Type::Result(_, error) => Type::Result(Box::new(Type::Unit), error),
            ty => ty,
        }
    }

    pub(super) fn infer_local_pure_returns(
        &mut self,
        view: &CheckBundleView<'_>,
        source: &str,
        statements: &[StmtId],
    ) {
        let program = view.program;
        let mut declarations = Vec::new();
        for &statement in statements {
            let statement = match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => inner,
                _ => statement,
            };
            let stmt = program.arena.stmt(statement);
            match stmt.kind {
                ArenaStmtKind::PureDef(function) | ArenaStmtKind::ProcDef(function) | ArenaStmtKind::StreamDef(function)
                    if !program.arena.function_def(function).test_declaration =>
                {
                    let def = program.arena.function_def(function);
                    declarations.push(ReturnDeclaration {
                        name: def.name,
                        names: vec![def.name],
                        span: stmt.span,
                        function: Some((function, match stmt.kind {
                            ArenaStmtKind::PureDef(_) => CallableKind::Pure,
                            ArenaStmtKind::ProcDef(_) => CallableKind::Proc,
                            ArenaStmtKind::StreamDef(_) => CallableKind::Stream,
                            _ => unreachable!(),
                        })),
                        statement,
                    });
                }
                ArenaStmtKind::Let { target, .. }
                | ArenaStmtKind::Const { target, .. }
                | ArenaStmtKind::Var { target, .. }
                | ArenaStmtKind::Guard { target, .. } => {
                    let names = binding_names(program, target)
                        .into_iter()
                        .filter(|name| *name != "_")
                        .collect::<Vec<_>>();
                    if let Some(&name) = names.first() {
                        declarations.push(ReturnDeclaration {
                            name,
                            names,
                            span: stmt.span,
                            function: None,
                            statement,
                        });
                    }
                }
                _ => {}
            }
        }
        if !declarations.iter().any(|declaration| declaration.definition().is_some()) {
            return;
        }
        let function_indices = declarations.iter().enumerate().filter_map(|(index, declaration)| {
            let definition = declaration.definition()?;
            let def = program.arena.function_def(definition);
            Some((super::FunctionDeclarationId::new(self.effect_declaration_id(program, def.body)), index))
        }).collect::<BTreeMap<_, _>>();
        let mut binding_indices = BTreeMap::new();
        for (index, declaration) in declarations.iter().enumerate() {
            let target = match program.arena.stmt(declaration.statement).kind {
                ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Const { target, .. } | ArenaStmtKind::Var { target, .. } | ArenaStmtKind::Guard { target, .. } => target,
                _ => continue,
            };
            let mut pending = vec![(target, declaration.span)];
            while let Some((target, span)) = pending.pop() {
                match program.arena.binding_target(target).kind {
                    ArenaBindingTargetKind::Name(name) => {
                        if let Some(&binding) = self.resolved_bindings.declaration_sites.get(&(span, name)) {
                            binding_indices.insert(binding, index);
                        }
                    }
                    ArenaBindingTargetKind::Record { fields, .. } => {
                        pending.extend(program.arena.destructure_fields(fields).iter().map(|field| (field.target, program.arena.span(field.span))));
                    }
                }
            }
        }
        let index = match &self.return_inference_index {
            Some(index) => index.clone(),
            None => {
                let index = std::sync::Arc::new(ReturnInferenceIndex::collect(view));
                self.return_inference_index = Some(index.clone());
                index
            }
        };
        let references = &index.references;
        let mut edges = vec![Vec::new(); declarations.len()];
        for (index, decl) in declarations.iter().enumerate() {
            let dependency_span = match program.arena.stmt(decl.statement).kind {
                ArenaStmtKind::Let { initializer, .. }
                | ArenaStmtKind::Const { initializer, .. }
                | ArenaStmtKind::Var { initializer, .. } => {
                    super::expr::expr_or_run_span_arena(program, initializer)
                }
                _ => decl.span,
            };
            let start = references.partition_point(|(span, _, _)| {
                (span.source_id, span.start())
                    < (dependency_span.source_id, dependency_span.start())
            });
            for &(span, name, expression) in &references[start..] {
                if span.source_id != dependency_span.source_id
                    || span.start() >= dependency_span.end()
                {
                    break;
                }
                if span.end() > dependency_span.end() {
                    continue;
                }
                if expression.is_some_and(|expression| !self.resolved_bindings.expression_namespaces.contains_key(&expression)) {
                    continue;
                }
                let dependency = if let Some(binding) = self.resolved_bindings.value_sites.get(&(span, name)) {
                    binding_indices.get(binding).copied()
                } else {
                    self.pures.get(&name).or_else(|| self.procs.get(&name)).or_else(|| self.streams.get(&name))
                        .and_then(|declaration| function_indices.get(declaration)).copied()
                };
                if let Some(dependency) = dependency { edges[index].push(dependency); }
            }
            edges[index].sort_unstable();
            edges[index].dedup();
        }
        let (order, cyclic) = declaration_dependency_order(&edges);
        let saved_scopes = self.scopes.clone();
        // Imports provide checked signatures and immutable module values to
        // inferred bodies even when their use statement follows a definition.
        for &id in statements {
            if let ArenaStmtKind::Use(use_id) = program.arena.stmt(id).kind {
                let use_stmt = program.arena.use_stmt(use_id);
                self.check_use_arena(
                    program,
                    use_stmt.path,
                    use_stmt.alias,
                    use_stmt.resolved.as_deref(),
                    use_stmt.searched_candidates.as_deref(),
                    program.arena.stmt(id).span,
                );
            }
        }
        for component in order {
            if component.iter().any(|index| cyclic.contains(index)) {
                self.infer_recursive_bodies(program, source, &component, &declarations);
                continue;
            }
            for &index in &component {
                let decl = &declarations[index];
                let Some(id) = decl.definition() else {
                    let kind = program.arena.stmt(decl.statement).kind;
                    if let ArenaStmtKind::Guard { target, ty, initializer, else_block } = kind {
                        let before = self.annotation_facts.len();
                        let reported = self.diagnostics.len();
                        self.without_pending_call_instantiations(|checker| {
                            checker.check_guard_arena(program, source, target, ty, initializer, else_block, decl.span);
                        });
                        self.diagnostics.truncate(reported);
                        self.annotation_facts.truncate(before);
                        continue;
                    }
                    if let ArenaStmtKind::Let {
                        target,
                        ty,
                        initializer,
                    }
                    | ArenaStmtKind::Const {
                        target,
                        ty,
                        initializer,
                    }
                    | ArenaStmtKind::Var {
                        target,
                        ty,
                        initializer,
                    } = kind
                    {
                        let before = self.annotation_facts.len();
                        let reported = self.diagnostics.len();
                        let annotated = ty.map(|id| self.type_from_arena(program, id));
                        let actual = self.without_pending_call_instantiations(|checker| {
                            checker.check_expr_or_run_arena(
                                program,
                                source,
                                initializer,
                                annotated.as_ref(),
                            )
                        });
                        self.diagnostics.truncate(reported);
                        let ty = annotated.unwrap_or(actual);
                        let origin = match initializer {
                            ArenaExprOrRun::Expr(expression) => self.stream_expr_origin(program, expression),
                            ArenaExprOrRun::Run(_) => super::stream_origins::StreamOrigins::unproven(&ty),
                        };
                        let alias = if matches!(kind, ArenaStmtKind::Let { ty: None, .. } | ArenaStmtKind::Const { ty: None, .. }) {
                            match initializer { ArenaExprOrRun::Expr(expression) => self.resolve_callable_alias_target(program, expression), _ => None }
                        } else { None };
                        self.define_binding_target_arena(
                            program,
                            target,
                            &ty,
                            matches!(kind, ArenaStmtKind::Var { .. }),
                            decl.span,
                            &origin,
                        );
                        self.record_literal_capture_payload(program, decl.statement);
                        if let Some(alias) = alias
                            && let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind
                        { self.attach_callable_alias(name, alias, super::expr::expr_or_run_span_arena(program, initializer)); }
                        self.annotation_facts.truncate(before);
                    }
                    continue;
                };
                let def = program.arena.function_def(id);
                if decl.kind() == Some(CallableKind::Stream) {
                    self.check_body_before_callers(program, source, decl, &declarations, def);
                    self.generalize_pending_function(program, id);
                    continue;
                }
                if !def.return_ty_defaulted {
                    self.check_body_before_callers(program, source, decl, &declarations, def);
                    self.generalize_pending_function(program, id);
                    continue;
                }
                if decl.kind() == Some(CallableKind::Proc) {
                    if proc_may_return_value(program, def) {
                        let boundary = if decl.name == "main" || decl.name.as_str().starts_with("main ") {
                            Some("entry")
                        } else {
                            None
                        };
                        let diagnostics_before = self.diagnostics.len();
                        self.infer_proc_return(program, source, decl, &declarations, def, boundary);
                        if !self.diagnostics[diagnostics_before..].iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error) {
                            let body = program.arena.span(program.arena.block(def.body).span);
                            if let Some(return_ty) = self.function_return_types.get(&body).cloned().filter(|ty| *ty != Type::Invalid) {
                                self.finish_recorded_stream_return_summary(program, def, &return_ty);
                            }
                        }
                        let declaration = super::FunctionDeclarationId::new(self.effect_declaration_id(program, def.body));
                        if let Some(signature) = self.declaration_inference.pending_mut(declaration) {
                            signature.mark_body_checked();
                        }
                    }
                    self.check_body_before_callers(program, source, decl, &declarations, def);
                    self.generalize_pending_function(program, id);
                    continue;
                }
                let body_span = program.arena.span(program.arena.block(def.body).span);
                self.inferred_returns = Some(Vec::new());
                self.inferred_propagations.clear();
                self.inference_reachable = true;
                self.check_body_before_callers(program, source, decl, &declarations, def);
                let candidates = self.inferred_returns.take().unwrap();
                let schema = Self::common_checked_schema(candidates.iter().map(|candidate| candidate.schema.clone()));
                let mut inferred = None;
                for candidate in &candidates {
                    inferred = Some(match inferred {
                        None => candidate.ty.clone(),
                        Some(previous) => self.unify_inferred_returns(previous, candidate.ty.clone(), candidate.span, false),
                    });
                }
                let ty = inferred.unwrap_or(Type::Unknown);
                let ty = if return_type_is_concrete(&ty) {
                    ty
                } else {
                    self.error(
                        decl.span,
                        &format!(
                            "pure `{}` needs a return annotation: its return shape is underdetermined",
                            decl.name
                        ),
                        DiagnosticCode::CheckInferReturn,
                    );
                    Type::Invalid
                };
                let mut ty = ty;
                if !ty.contains_recovery() {
                    let diagnostics = self.diagnostics.len();
                    for candidate in &candidates {
                        let expected = self.type_constraints.resolve(&ty);
                        let actual = self.type_constraints.resolve(&candidate.ty);
                        let expected = match (&expected, &actual) {
                            (Ok(expected), Ok(actual)) => {
                                if super::tail_type_matches_expected(expected, actual) { continue; }
                                match expected {
                                    Type::Result(success, _) if !actual.is_result() => success.as_ref(),
                                    _ => expected,
                                }
                            }
                            _ => &ty,
                        };
                        self.expect_type(expected, &candidate.ty, candidate.span);
                    }
                    if self.diagnostics[diagnostics..].iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error) {
                        ty = Type::Invalid;
                    }
                }
                for (error, span) in std::mem::take(&mut self.inferred_propagations) {
                    match &ty {
                        Type::Result(_, expected) if error.matches_expected(expected) => {}
                        Type::Result(_, _) => {
                            self.error(span, "propagated error does not match the inferred Result; declare its return type", DiagnosticCode::CheckTryError);
                            ty = Type::Invalid;
                        }
                        _ => {
                            self.error(span, "propagation requires a Result return determined by the body; declare a return annotation", DiagnosticCode::CheckTryContext);
                            ty = Type::Invalid;
                        }
                    }
                }
                self.function_return_types.insert(body_span, ty.clone());
                self.update_pending_return_type(program, def, ty.clone());
                self.update_pending_return_schema(program, def, schema);
                if ty != Type::Invalid {
                    self.annotation_facts.push(AnnotationFact {
                        kind: AnnotationFactKind::InferredPureReturn { body: body_span },
                        ty,
                    });
                    if decl.kind() != Some(CallableKind::Proc) {
                        self.generalize_pending_function(program, id);
                    }
                }
            }
        }
        self.scopes = saved_scopes;
    }

    fn infer_recursive_bodies(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        component: &[usize],
        declarations: &[ReturnDeclaration],
    ) {
        let definitions = component.iter().filter_map(|&index| declarations[index].definition()).collect::<Vec<_>>();
        let start = self.diagnostics.len();
        for &index in component {
            let declaration = &declarations[index];
            let Some(definition) = declaration.definition() else { continue; };
            let def = program.arena.function_def(definition);
            if !def.return_ty_defaulted || declaration.kind() == Some(CallableKind::Stream) { continue; }
            if declaration.kind() == Some(CallableKind::Proc) && (declaration.name == "main" || declaration.name.as_str().starts_with("main "))
                && proc_may_return_value(program, def)
            {
                self.error(declaration.span, "entry proc returns a value and requires an explicit return annotation", DiagnosticCode::CheckRequiredReturn);
            }
            let body = program.arena.span(program.arena.block(def.body).span);
            let ty = if declaration.kind() == Some(CallableKind::Proc) {
                if proc_may_return_value(program, def) {
                    Type::Result(Box::new(self.type_constraints.fresh_at_level(body, 1)), Box::new(self.type_constraints.fresh_at_level(body, 1)))
                } else { self.type_from_arena(program, def.return_ty) }
            } else { self.type_constraints.fresh_at_level(body, 1) };
            self.update_pending_return_type(program, def, ty.clone());
            self.function_return_types.insert(body, ty);
        }
        let mut propagations = Vec::new();
        let mut inferred_dynamic_returns = std::collections::BTreeSet::new();
        for &index in component {
            let declaration = &declarations[index];
            let Some(definition) = declaration.definition() else {
                self.error(declaration.span, "recursive value bindings require a callable declaration boundary", DiagnosticCode::CheckLocalInference);
                continue;
            };
            let def = program.arena.function_def(definition);
            if !def.return_ty_defaulted || declaration.kind() == Some(CallableKind::Stream)
                || (declaration.kind() == Some(CallableKind::Proc) && !proc_may_return_value(program, def))
            {
                self.check_body_before_callers(program, source, declaration, declarations, def);
                continue;
            }
            self.inferred_returns = Some(Vec::new());
            self.inferred_propagations.clear();
            self.inference_reachable = true;
            self.return_conflicts = (declaration.kind() == Some(CallableKind::Proc)).then(Vec::new);
            self.check_body_before_callers(program, source, declaration, declarations, def);
            let candidates = self.inferred_returns.take().expect("recursive return checking owns its candidates");
            let inferred_dynamic_return = declaration.kind() == Some(CallableKind::Pure)
                && candidates.iter().any(|candidate| candidate.ty.contains_any());
            if inferred_dynamic_return {
                inferred_dynamic_returns.insert(self.effect_declaration_id(program, def.body));
            }
            let errors = std::mem::take(&mut self.inferred_propagations);
            let body = program.arena.span(program.arena.block(def.body).span);
            let original = self.function_return_types[&body].clone();
            if declaration.kind() == Some(CallableKind::Proc) {
                let owner = self.effect_declaration_id(program, def.body);
                if !self.finish_proc_completion_group(owner, &candidates, &errors, body) {
                    self.function_return_types.insert(body, Type::Invalid);
                    continue;
                }
                let actual = self.normalize_inferred_proc_returns(owner, candidates, &errors);
                self.expect_type(&original, &actual, body);
                for (left, right, span) in self.return_conflicts.take().unwrap_or_default() {
                    self.error(span, &format!("incompatible recursive proc completions `{left}` and `{right}`"), DiagnosticCode::CheckTypeMismatch);
                }
            } else {
                for candidate in candidates {
                    self.expect_type(&original, &candidate.ty, candidate.span);
                }
                propagations.push((original, errors));
            }
        }
        if let Err(error) = self.solve_host_obligations(program) {
            let span = definitions.first().map(|&definition| program.arena.span(program.arena.block(program.arena.function_def(definition).body).span))
                .unwrap_or_else(|| Span::at(crate::source::SourceId::new(0), 0));
            self.reject_host_inference_resolution(error, span);
        }
        if let Err(error) = self.type_constraints.solve_callable_applications() {
            let span = declarations[component[0]].span;
            self.error(span, &format!("recursive callable application requirement failed: {error}"), DiagnosticCode::CheckCallableMismatch);
        }
        for (ty, errors) in propagations {
            let ty = self.type_constraints.resolve(&ty).unwrap_or(Type::Invalid);
            for (error, span) in errors {
                match &ty {
                    Type::Result(_, expected) => self.expect_type(expected, &error, span),
                    _ => self.error(span, "recursive propagation requires a Result return determined by the component", DiagnosticCode::CheckTryContext),
                }
            }
        }
        for &index in component {
            let declaration = &declarations[index];
            let Some(definition) = declaration.definition() else { continue; };
            let def = program.arena.function_def(definition);
            if !def.return_ty_defaulted || declaration.kind() == Some(CallableKind::Stream) { continue; }
            let body = program.arena.span(program.arena.block(def.body).span);
            let Some(ty) = self.function_return_types.get(&body) else { continue; };
            let ty = self.type_constraints.resolve(ty).unwrap_or(Type::Invalid);
            let owner = self.effect_declaration_id(program, def.body);
            let inferred_dynamic_return = inferred_dynamic_returns.contains(&owner);
            if !return_type_is_concrete(&ty) || inferred_dynamic_return {
                let message = if inferred_dynamic_return {
                    "pure recursive return includes dynamic Any; declare a return annotation"
                } else {
                    "recursive return shape is not determined by its component"
                };
                self.error(body, message, DiagnosticCode::CheckInferReturn);
            }
        }
        if self.diagnostics[start..].iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error) {
            self.reject_recursive_component(program, &definitions);
            self.mark_inferred_proc_component_bodies_checked(program, component, declarations);
            return;
        }
        for &index in component {
            let declaration = &declarations[index];
            let Some(definition) = declaration.definition() else { continue; };
            let def = program.arena.function_def(definition);
            if !def.return_ty_defaulted || declaration.kind() != Some(CallableKind::Proc)
                || !proc_may_return_value(program, def)
            { continue; }
            let body = program.arena.span(program.arena.block(def.body).span);
            let Some(return_ty) = self.function_return_types.get(&body).cloned() else { continue; };
            let return_ty = self.type_constraints.resolve(&return_ty).unwrap_or(Type::Invalid);
            self.finish_recorded_stream_return_summary(program, def, &return_ty);
        }
        if self.diagnostics[start..].iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error) {
            self.reject_recursive_component(program, &definitions);
            self.mark_inferred_proc_component_bodies_checked(program, component, declarations);
            return;
        }
        self.generalize_recursive_component(program, &definitions);
        self.mark_inferred_proc_component_bodies_checked(program, component, declarations);
    }

    fn mark_inferred_proc_component_bodies_checked(
        &mut self,
        program: &ArenaProgram,
        component: &[usize],
        declarations: &[ReturnDeclaration],
    ) {
        for &index in component {
            let declaration = &declarations[index];
            let Some(definition) = declaration.definition() else { continue; };
            let def = program.arena.function_def(definition);
            if !def.return_ty_defaulted || declaration.kind() != Some(CallableKind::Proc)
                || !proc_may_return_value(program, def)
            { continue; }
            let id = super::FunctionDeclarationId::new(self.effect_declaration_id(program, def.body));
            if let Some(signature) = self.declaration_inference.pending_mut(id) {
                signature.mark_body_checked();
            }
        }
    }

    fn check_body_before_callers(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        decl: &ReturnDeclaration,
        declarations: &[ReturnDeclaration],
        def: &crate::syntax::arena::ArenaFunctionDef,
    ) {
        let visible_scopes = self.scopes.clone();
        for binding in declarations {
            if binding.function.is_none()
                && binding.span.source_id == decl.span.source_id
                && binding.span.start() >= decl.span.start()
            {
                for name in &binding.names {
                    self.current_scope_mut().remove(name);
                }
            }
        }
        match decl.kind().expect("a callable declaration owns its body") {
            CallableKind::Pure => self.check_function_arena(program, source, def, true),
            CallableKind::Proc => self.check_function_arena(program, source, def, false),
            CallableKind::Stream => self.check_stream_function_arena(program, source, def),
        }
        self.scopes = visible_scopes;
    }

    fn infer_proc_return(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        decl: &ReturnDeclaration,
        declarations: &[ReturnDeclaration],
        def: &crate::syntax::arena::ArenaFunctionDef,
        boundary: Option<&str>,
    ) {
        let body_span = program.arena.span(program.arena.block(def.body).span);
        let diagnostics_before = self.diagnostics.len();
        self.inferred_returns = Some(Vec::new());
        self.inferred_propagations.clear();
        self.return_conflicts = Some(Vec::new());
        self.inference_reachable = true;
        let enclosing_return = self.boundary.current_return.take();
        self.check_body_before_callers(program, source, decl, declarations, def);
        self.boundary.current_return = enclosing_return;
        let candidates = self.inferred_returns.take().unwrap_or_default();
        let conflicts = self.return_conflicts.take().unwrap_or_default();
        let propagated = std::mem::take(&mut self.inferred_propagations);
        if self.diagnostics[diagnostics_before..]
            .iter()
            .any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error)
        {
            self.reject_recursive_component(program, &[decl.definition().expect("an inferred procedure retains its declaration")]);
            return;
        }
        let owner = self.effect_declaration_id(program, def.body);
        if !self.finish_proc_completion_group(owner, &candidates, &propagated, body_span) { return; }
        if !candidates.iter().any(proc_candidate_produces_value) && conflicts.is_empty()
        {
            let mut ty = self.type_from_arena(program, def.return_ty);
            if !candidates.is_empty() {
                let failure_only = candidates.iter().any(|candidate| candidate.failure_only_result);
                self.return_conflicts = Some(Vec::new());
                let owner = self.effect_declaration_id(program, def.body);
                let actual = self.normalize_inferred_proc_returns(
                    owner,
                    candidates,
                    &propagated,
                );
                let conflicts = self.return_conflicts.take().unwrap_or_default();
                if let Some((left, right, span)) = conflicts.into_iter().next() {
                    let (left, right) = (completion_success_type(&left), completion_success_type(&right));
                    let message = format!("completions of proc `{}` produce `{left}` and `{right}`", decl.name);
                    self.diagnostics.push(crate::diagnostic::Diagnostic::error(&message)
                        .with_code(DiagnosticCode::CheckTypeMismatch)
                        .with_label(crate::diagnostic::Label::primary(span, &message)));
                    ty = Type::Invalid;
                } else {
                    if failure_only {
                        ty = self.failure_only_proc_unit_envelope(program, def);
                    }
                    if failure_only {
                        let validation_start = self.diagnostics.len();
                        self.expect_type(&ty, &actual, body_span);
                        ty = if self.diagnostics[validation_start..].iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error) {
                            Type::Invalid
                        } else { actual };
                    } else if propagated.is_empty() {
                        self.expect_type(&ty, &actual, body_span);
                    } else {
                        ty = actual;
                    }
                }
            }
            for (error, span) in propagated {
                match &ty {
                    Type::Result(_, expected_error) if error.matches_expected(expected_error) => {}
                    Type::Result(_, _) => self.error(span, "propagated error does not match the inferred Result; declare its return type", DiagnosticCode::CheckTryError),
                    _ => self.error(span, "propagation requires a Result return determined by the body; declare a return annotation", DiagnosticCode::CheckTryContext),
                }
            }
            self.function_return_types.insert(body_span, ty.clone());
            self.update_pending_return_type(program, def, ty);
            return;
        }
        self.return_conflicts = Some(conflicts);
        let owner = self.effect_declaration_id(program, def.body);
        let schema = Self::common_checked_schema(candidates.iter().map(|candidate| {
            let schema = candidate.schema.clone()?;
            if candidate.ty.is_result() { Some(schema) } else {
                let mut wrapped = crate::sema::constants::SchemaExpectation::default();
                wrapped.insert_child(crate::sema::constants::SchemaComponent::Success, schema);
                Some(wrapped)
            }
        }));
        let inferred = self.normalize_inferred_proc_returns(owner, candidates, &propagated);

        let conflicts = self.return_conflicts.take().unwrap_or_default();
        if let Some((left, right, span)) = conflicts.into_iter().next() {
            let (left, right) = if produces_value(&left) {
                (left, right)
            } else {
                (right, left)
            };
            let (left, right) = (completion_success_type(&left), completion_success_type(&right));
            let message = format!(
                "completions of proc `{}` produce `{left}` and `{right}`",
                decl.name
            );
            self.diagnostics.push(crate::diagnostic::Diagnostic::error(&message).with_code(DiagnosticCode::CheckTypeMismatch)
                .with_label(crate::diagnostic::Label::primary(span, &message))
                .with_note(format!("make every completion produce one type, or declare the return explicitly, for example `-> Result[{left}]`")));
            self.function_return_types.insert(body_span, Type::Invalid);
            self.update_pending_return_type(program, def, Type::Invalid);
            return;
        }
        let ty = if let Some(boundary) = boundary {
            self.error(decl.span, &format!("{boundary} proc `{}` returns a value and requires an explicit return annotation", decl.name), DiagnosticCode::CheckRequiredReturn);
            Type::Invalid
        } else {
            match inferred {
                Type::Invalid => Type::Invalid,
                ty if !return_type_is_concrete(&ty) => {
                    self.error(decl.span, &format!("proc `{}` needs a return annotation: its return shape is underdetermined", decl.name), DiagnosticCode::CheckInferReturn);
                    Type::Invalid
                }
                ty => ty,
            }
        };
        self.function_return_types.insert(body_span, ty.clone());
        self.update_pending_return_type(program, def, ty);
        self.update_pending_return_schema(program, def, schema);
    }

    pub(super) fn normalize_inferred_proc_returns(
        &mut self,
        owner: super::EffectDeclarationId,
        candidates: Vec<InferredReturn>,
        propagated: &[(Type, Span)],
    ) -> Type {
        self.proc_return_sites.remove(&owner);
        let level = self.declaration_inference.pending(super::FunctionDeclarationId::new(owner)).map(|pending| pending.solver_level());
        let errors = propagated.iter().map(|(error, _)| error.clone()).collect::<Vec<_>>();
        let completion_error = match self.type_constraints.proc_completion_return(owner) {
            Ok(Some(returned)) => match self.type_constraints.resolve(&returned) {
                Ok(Type::Result(_, error)) => Some(*error),
                Ok(_) => {
                    self.error(owner.body, "proc completion wrapper has no Result return", DiagnosticCode::CheckLocalInference);
                    return Type::Invalid;
                }
                Err(error) => {
                    self.error(owner.body, &format!("proc completion wrapper type resolution failed: {error:?}"), DiagnosticCode::CheckLocalInference);
                    return Type::Invalid;
                }
            },
            Ok(None) => None,
            Err(error) => {
                self.error(owner.body, &format!("proc completion wrapper lookup failed: {error:?}"), DiagnosticCode::CheckLocalInference);
                return Type::Invalid;
            }
        };
        let mut inferred = None;
        let forwards_applications = candidates.iter().any(|candidate| !candidate.origin.return_applications().contains(&None));
        for candidate in candidates {
            let applications = candidate.origin.return_applications();
            let retain_applications = !applications.contains(&None);
            let candidate_ty = match self.type_constraints.resolve(&candidate.ty) {
                Ok(ty) => ty,
                Err(error) => {
                    self.error(candidate.span, &format!("proc completion constraint resolution failed: {error:?}"), DiagnosticCode::CheckLocalInference);
                    return Type::Invalid;
                }
            };
            for application in applications {
                let ty = if let Some(reference) = application {
                    let Some(level) = level else {
                        self.error(owner.body, "proc application return has no declaration inference scope", DiagnosticCode::CheckLocalInference);
                        return Type::Invalid;
                    };
                    if reference.scope != crate::sema::constraints::CallableApplicationScope::Declaration(owner) {
                        self.error(candidate.span, "proc return application belongs to another declaration", DiagnosticCode::CheckLocalInference);
                        return Type::Invalid;
                    }
                    let application_errors = completion_error.as_ref().map(std::slice::from_ref).unwrap_or(&errors);
                    match self.type_constraints.referenced_callable_application(reference)
                        .and_then(|id| self.type_constraints.defer_proc_application_return(id, application_errors, candidate.span, level))
                    {
                        Ok(ty) => {
                            if retain_applications {
                                let envelope = self.type_constraints.proc_return_envelope(reference, candidate.span).expect("the checked application return owns its envelope");
                                self.proc_return_sites.entry(owner).or_default().entry(candidate.span).or_default().push(CheckedProcReturnSite { application: reference, envelope });
                            }
                            ty
                        }
                        Err(error) => {
                            self.error(candidate.span, &format!("proc return application requirement failed: {error}"), DiagnosticCode::CheckLocalInference);
                            return Type::Invalid;
                        }
                    }
                } else if let Some(error) = &completion_error {
                    Self::proc_completion_envelope(candidate_ty.clone(), error)
                } else if errors.is_empty() && !forwards_applications && !matches!(candidate_ty, Type::Result(_, _) | Type::Unit) {
                    Type::Result(Box::new(candidate_ty.clone()), Box::new(Type::Unknown))
                } else {
                    inferred_proc_return_envelope(candidate_ty.clone(), &errors)
                };
                inferred = Some(match inferred {
                    None => ty,
                    Some(previous) => self.unify_inferred_returns(previous, ty, candidate.span, true),
                });
            }
        }
        match inferred {
            Some(Type::Result(ok, error)) if *error == Type::Unknown => Type::Result(ok, Box::new(Type::Error)),
            Some(inferred) => inferred,
            None => Type::Invalid,
        }
    }

    fn finish_proc_completion_group(
        &mut self,
        owner: super::EffectDeclarationId,
        candidates: &[InferredReturn],
        propagated: &[(Type, Span)],
        span: Span,
    ) -> bool {
        let mut values = Vec::new();
        let mut applications = Vec::new();
        for candidate in candidates {
            let sources = candidate.origin.return_applications();
            if sources.contains(&None) {
                values.push(candidate.ty.clone());
            } else {
                applications.extend(sources.into_iter().flatten());
            }
        }
        match self.type_constraints.finish_proc_completion_group(owner, propagated, &values, &applications) {
            Ok(_) => true,
            Err(error) => {
                self.error(span, &format!("proc completion wrapper failed: {error}"), DiagnosticCode::CheckLocalInference);
                false
            }
        }
    }

    fn proc_completion_envelope(value: Type, error: &Type) -> Type {
        let value = match value {
            Type::Result(ok, _) => *ok,
            value => value,
        };
        Type::Result(Box::new(value), Box::new(error.clone()))
    }

    pub(super) fn unify_inferred_returns(&mut self, left: Type, right: Type, span: Span, completion: bool) -> Type {
        if let Some(merged) = self.merge_scheme_callable_values(&left, &right) { return merged; }
        match unify_return_shapes(&left, &right, completion) {
            Some(ty) => ty,
            None if matches!(left, Type::Invalid) || matches!(right, Type::Invalid) => {
                Type::Invalid
            }
            None if left.contains_inference() || right.contains_inference() => {
                if let (Type::Result(left_ok, left_error), Type::Result(right_ok, right_error)) = (&left, &right)
                    && !left_error.contains_inference()
                    && !right_error.contains_inference()
                {
                    let Some(error) = join_result_errors(left_error, right_error, completion) else {
                        self.expect_type(&left, &right, span);
                        return Type::Invalid;
                    };
                    return match self.unify_inferred_returns((**left_ok).clone(), (**right_ok).clone(), span, completion) {
                        Type::Invalid => Type::Invalid,
                        ok => Type::Result(Box::new(ok), Box::new(error)),
                    };
                }
                let diagnostics = self.diagnostics.len();
                self.expect_type(&left, &right, span);
                if self.diagnostics.len() != diagnostics { return Type::Invalid; }
                self.type_constraints.resolve(&left).unwrap_or(Type::Invalid)
            }
            None if matches!((&left, &right), (Type::Result(left_ok, _), Type::Result(right_ok, _)) if unify_return_shapes(left_ok, right_ok, completion).is_some()) => {
                self.expect_type(&left, &right, span);
                Type::Invalid
            }
            None => {
                if let Some(conflicts) = &mut self.return_conflicts {
                    conflicts.push((left, right, span));
                    return Type::Invalid;
                }
                self.error(span, &format!("incompatible inferred return paths `{left}` and `{right}`; declare a return type"), DiagnosticCode::CheckInferReturn);
                Type::Invalid
            }
        }
    }
}

fn completion_success_type(envelope: &Type) -> &Type {
    match envelope {
        Type::Result(ok, _) => ok,
        other => other,
    }
}

// Failure-only tails such as `Err(..)` and recursive calls fix no success
// type, so they join with any completion.
fn produces_value(ty: &Type) -> bool {
    match ty {
        Type::Unit | Type::Unknown | Type::Invalid => false,
        Type::Result(ok, _) => produces_value(ok),
        _ => true,
    }
}

pub(super) fn proc_candidate_produces_value(candidate: &InferredReturn) -> bool {
    !candidate.failure_only_result && proc_candidate_produces_value_type(&candidate.ty)
}

fn proc_candidate_produces_value_type(ty: &Type) -> bool {
    match ty {
        Type::Unknown => true,
        Type::Result(ok, _) if matches!(ok.as_ref(), Type::Unknown) => false,
        ty => produces_value(ty),
    }
}

pub(super) fn proc_may_return_value(
    program: &ArenaProgram,
    def: &crate::syntax::arena::ArenaFunctionDef,
) -> bool {
    let tail_value = program.arena.stmt_ids(program.arena.block(def.body).statements).last().is_some_and(|tail| matches!(program.arena.stmt(program.arena.core_stmt_id(tail)).kind,
        ArenaStmtKind::Expr(_) | ArenaStmtKind::TailBareIdent(_) | ArenaStmtKind::Match { .. } | ArenaStmtKind::If { else_block: Some(_), .. }));
    let body = program.arena.span(program.arena.block(def.body).span);
    tail_value
        || program.arena.source_rows().stmt_ids(body.source_id).any(|id| {
            let stmt = program.arena.stmt(id);
            matches!(stmt.kind, ArenaStmtKind::Return(Some(_)))
                && stmt.span.start() >= body.start()
                && stmt.span.end() <= body.end()
        })
}

fn return_type_is_concrete(ty: &Type) -> bool {
    match ty {
        Type::Any
        | Type::ErasedRecord
        | Type::Unknown
        | Type::Invalid
        | Type::Function(_)
        | Type::DynamicModule => false,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
            return_type_is_concrete(inner)
        }
        Type::Map(key, value) => return_type_is_concrete(key) && return_type_is_concrete(value),
        Type::Result(ok, err) => return_type_is_concrete(ok) && return_type_is_concrete(err),
        Type::Record(fields) => fields.values().all(return_type_is_concrete),
        Type::Validated(validated) => return_type_is_concrete(validated.base()),
        Type::Module(_) => false,
        _ => true,
    }
}

fn is_error_value_type(ty: &Type) -> bool {
    matches!(ty, Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError | Type::ErrorFacet(_))
}

fn join_result_errors(left: &Type, right: &Type, completion: bool) -> Option<Type> {
    if completion || left == right || matches!(left, Type::Unknown) || matches!(right, Type::Unknown)
        || is_error_value_type(left) && is_error_value_type(right)
    {
        Some(join_error_types(left, right))
    } else { None }
}

pub(super) fn unify_return_shapes(left: &Type, right: &Type, completion: bool) -> Option<Type> {
    if left == right {
        return Some(left.clone());
    }
    match (left, right) {
        (Type::Unknown, other) | (other, Type::Unknown) => Some(other.clone()),
        (Type::Result(ok, err), other) | (other, Type::Result(ok, err))
            if **ok == Type::Unknown && !matches!(other, Type::Result(_, _) | Type::Unit) =>
        {
            Some(Type::Result(Box::new(other.clone()), err.clone()))
        }
        (Type::Null, _) | (_, Type::Null) => super::types::join_null_branches(left, right),
        (Type::Error, Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError)
        | (Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError, Type::Error) => {
            Some(Type::Error)
        }
        (Type::Optional(inner), other) | (other, Type::Optional(inner))
            if !matches!(other, Type::Optional(_)) =>
        {
            Some(Type::Optional(Box::new(unify_return_shapes(inner, other, completion)?)))
        }
        (Type::Record(left), Type::Record(right)) if left.keys().eq(right.keys()) => {
            Some(Type::Record(
                left.iter()
                    .map(|(name, ty)| Some((*name, unify_return_shapes(ty, &right[name], completion)?)))
                    .collect::<Option<std::collections::BTreeMap<_, _>>>()?
                    .into(),
            ))
        }
        (Type::List(left), Type::List(right)) => {
            Some(Type::List(Box::new(unify_return_shapes(left, right, completion)?)))
        }
        (Type::Map(lk, left), Type::Map(rk, right)) => Some(Type::Map(
            Box::new(unify_return_shapes(lk, rk, completion)?),
            Box::new(unify_return_shapes(left, right, completion)?),
        )),
        (Type::Optional(left), Type::Optional(right)) => {
            Some(Type::Optional(Box::new(unify_return_shapes(left, right, completion)?)))
        }
        (Type::Result(left, le), Type::Result(right, re)) => Some(Type::Result(
            Box::new(unify_return_shapes(left, right, completion)?),
            Box::new(join_result_errors(le, re, completion)?),
        )),
        _ => None,
    }
}


fn declaration_dependency_order(edges: &[Vec<usize>]) -> (Vec<Vec<usize>>, FxHashSet<usize>) {
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::new();
    for start in 0..edges.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut work = vec![(start, 0)];
        while let Some((node, next)) = work.last_mut() {
            if let Some(&child) = edges[*node].get(*next) {
                *next += 1;
                if !visited[child] {
                    visited[child] = true;
                    work.push((child, 0));
                }
            } else {
                let (node, _) = work.pop().unwrap();
                order.push(node);
            }
        }
    }
    let mut reverse = vec![Vec::new(); edges.len()];
    for (node, children) in edges.iter().enumerate() {
        for &child in children {
            reverse[child].push(node);
        }
    }
    visited.fill(false);
    let mut cyclic = FxHashSet::default();
    let mut components = Vec::new();
    for &start in order.iter().rev() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut members = Vec::new();
        let mut work = vec![start];
        while let Some(node) = work.pop() {
            members.push(node);
            for &child in &reverse[node] {
                if !visited[child] {
                    visited[child] = true;
                    work.push(child);
                }
            }
        }
        if members.len() > 1 || edges[start].contains(&start) {
            cyclic.extend(members.iter().copied());
        }
        components.push(members);
    }
    components.reverse();
    (components, cyclic)
}

fn binding_names(program: &ArenaProgram, target: BindingTargetId) -> Vec<Name> {
    let mut names = Vec::new();
    let mut work = vec![target];
    while let Some(target) = work.pop() {
        match program.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => names.push(name),
            ArenaBindingTargetKind::Record { .. } => {}
        }
        program.arena.for_each_binding_target_child(target, |child| {
            if let ArenaChild::BindingTarget(child) = child {
                work.push(child);
            }
        });
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_data_returns_publish_closed_function_contracts() {
        let source = "pure pure_null() { null }\npure pure_row() { {raw: null} }\npure pure_list() { [null] }\nproc proc_null() { null }\nproc proc_row() { {raw: null} }\npure joined(flag: Bool) { if flag { {raw: null} } else { {raw: \"present\"} } }";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, super::super::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{errors:?}");
        let _symbols = parsed.arena.symbol_owner().enter();
        let row = |field| Type::Record(BTreeMap::from([(Name::intern("raw"), field)]).into());
        for (name, expected) in [
            ("pure_null", Type::Null),
            ("pure_row", row(Type::Null)),
            ("pure_list", Type::List(Box::new(Type::Null))),
            ("proc_null", Type::Result(Box::new(Type::Null), Box::new(Type::Error))),
            ("proc_row", Type::Result(Box::new(row(Type::Null)), Box::new(Type::Error))),
            ("joined", row(Type::Optional(Box::new(Type::Str)))),
        ] {
            let declaration = checked.checked_declarations.functions.values().find(|declaration| declaration.name == name).unwrap();
            assert_eq!(checked.function_return_types.get(&declaration.body), Some(&expected), "{name}");
        }
    }
}

#[cfg(test)]
mod pure_error_return_tests {
    use crate::diagnostic::{DiagnosticCode, Severity};
    use crate::sema::check::{CheckOptions, Checker};

    #[test]
    fn pure_return_error_domains_are_checked_before_lowering() {
        let mut accepted_invalid = Vec::new();
        for (label, definition, observation, should_pass) in [
            ("typed string control", "pure attempt(flag: Bool, value: Int) -> Result[Int, Str] { if flag { return Err(\"stop\") }; Ok(value) }", "assert match attempt(true, 7) { Err(problem) => problem == \"stop\"; Ok(_) => false }", true),
            ("nominal error and generic success control", "error Local = Stop\npure failure() -> Error { Local.Stop() }\npure attempt(flag, value) { if flag { return Err(failure()) }; Ok(value) }", "let integer: Result[Int] = attempt(false, 7)\nlet word: Result[Str] = attempt(false, \"kept\")\nassert integer == Ok(7)\nassert word == Ok(\"kept\")\nassert match attempt(true, 7) { Err(_) => true; Ok(_) => false }", true),
            ("ordinary error and generic success control", "pure attempt(flag, value) { if flag { return Err(error.failure(\"stop\")) }; Ok(value) }", "let integer: Result[Int] = attempt(false, 7)\nlet word: Result[Str] = attempt(false, \"kept\")\nassert integer == Ok(7)\nassert word == Ok(\"kept\")", true),
            ("inferred string and default error", "pure attempt(flag, value) { if flag { return Err(\"stop\") }; Ok(value) }", "let result = attempt(true, 7)", false),
            ("inferred mixed authored errors", "pure attempt(flag, value) { if flag { return Err(\"stop\") }; if !flag { return Err(error.failure(\"structured\")) }; Ok(value) }", "let result = attempt(true, 7)", false),
            ("typed error negative", "pure attempt(flag: Bool, value: Int) -> Result[Int, Error] { if flag { return Err(\"stop\") }; Ok(value) }", "let result = attempt(true, 7)", false),
            ("ground string failure projection", "pure attempt(flag: Bool, value: Int) { if flag { return Err(\"stop\") }; Ok(value) }", "let result = attempt(true, 7)\nmatch result { Err(problem) => print $problem.message; Ok(_) => print \"unexpected success\" }", false),
        ] {
            let source = format!("{definition}\n{observation}\n");
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "pure-error-return.xsh",
                crate::loader::entry_source_from_text("pure-error-return.xsh", source.clone()),
                Vec::new(),
            );
            assert!(parsed.diagnostics.is_empty(), "{label}: {:?}", parsed.diagnostics);
            let source_id = parsed.arena.source_text_source_id().unwrap();
            let checked = Checker::check_arena_with_sources_and_options(&parsed.arena, &sources, source_id, CheckOptions::for_execution());
            let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
            if errors.is_empty() {
                let output = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
                if should_pass {
                    assert_eq!(output.status, 0, "{label}: {:?} {:?}", output.diagnostics, output.traceback);
                    assert!(output.diagnostics.is_empty() && output.traceback.is_none(), "{label}: {:?} {:?}", output.diagnostics, output.traceback);
                } else {
                    accepted_invalid.push(label);
                }
            } else {
                assert!(!should_pass, "{label}: {errors:?}");
                assert!(errors.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)), "{label}: {errors:?}");
                assert!(checked.annotation_facts.iter().all(|fact| !matches!(fact.kind, crate::sema::check::AnnotationFactKind::InferredPureReturn { .. })), "{label}: an invalid inferred pure return was published");
                let output = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
                assert_eq!(output.status, 2, "{label}: {:?}", output.diagnostics);
                assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)), "{label}: {:?}", output.diagnostics);
                assert!(output.traceback.is_none(), "{label}: {:?}", output.traceback);
            }
        }
        assert!(accepted_invalid.is_empty(), "invalid pure returns passed the checker: {accepted_invalid:?}");
    }

    #[test]
    fn pure_return_validation_preserves_nullable_parameter_relationships() {
        for source in [
            "pure choose(flag, value) { if flag { return null }; value }\nlet integer: Int? = choose(false, 7)\nlet word: Str? = choose(false, \"kept\")\n",
            "pure choose(flag, value) { if flag { return {row: {value: null}} }; {row: {value}} }\nlet integer: Int? = choose(false, 7).row.value\nlet word: Str? = choose(false, \"kept\").row.value\n",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn pure_error_return_native_controls_run_on_both_routes() {
        let source = include_str!("../../../tests/xsh/pure-error-return-domains.xsh");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "pure-error-return-domains.xsh",
            crate::loader::entry_source_from_text("pure-error-return-domains.xsh", source.to_owned()),
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources)
            .prepare_test_program(std::sync::Arc::new(parsed.arena), source_id, checked)
            .unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("pure_error_returns_preserve_explicit_payloads_and_nominal_errors", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Unit))
                || matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }
}

#[cfg(test)]
mod pure_result_completion_tests {
    fn accepts(annotation: &str) {
        for (shape, definition, observation) in [
            ("Int", "if failed { return Err(Local.Stop()) }; 7", "let result: Result[Int, Local] = attempt(false)"),
            ("Int", "if !failed { return 7 }; Err(Local.Stop())", "let result: Result[Int, Local] = attempt(false)"),
            ("Str", "if failed { return Err(Local.Stop()) }; \"kept\"", "let result: Result[Str, Local] = attempt(false)"),
            ("Row", "if failed { return Err(Local.Stop()) }; {value: 7}", "let result: Result[Row, Local] = attempt(false)"),
        ] {
            let returned = annotation.replace("PAYLOAD", shape);
            let source = format!("error Local = Stop\ntype Row = {{value: Int}}\npure attempt(failed: Bool) {returned} {{ {definition} }}\n{observation}\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
            let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
            assert!(errors.is_empty(), "{source}\n{errors:?}");
        }
    }

    #[test]
    fn typed_ground_result_returns_wrap_bare_success_values() {
        accepts("-> Result[PAYLOAD, Local]");
    }

    #[test]
    fn inferred_ground_result_returns_wrap_bare_success_values() {
        accepts("");
    }

    #[test]
    fn actual_result_candidates_keep_their_outer_error_domain() {
        for source in [
            "pure attempt(failed: Bool) { if failed { return Err(\"wrong\") }; Ok(7) }\n",
            "pure attempt(failed: Bool) { if failed { return Err(\"wrong\") }; Ok(Ok(7)) }\n",
            "pure attempt(failed: Bool) -> Result[Result[Int, Str], Error] { if failed { return Err(\"wrong\") }; Ok(Ok(7)) }\n",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn inferred_pure_result_native_controls_run_on_both_routes() {
        let source = include_str!("../../../tests/xsh/inferred-pure-result-completions.xsh");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("inferred-pure-result-completions.xsh", crate::loader::entry_source_from_text("inferred-pure-result-completions.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(std::sync::Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("inferred_pure_results_wrap_success_and_preserve_failures", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Unit)) || matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "{:?}", result.result);
        }
    }

}
