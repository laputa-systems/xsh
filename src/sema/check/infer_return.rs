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
}

struct ReturnDeclaration {
    name: Name,
    names: Vec<Name>,
    span: Span,
    function: Option<FunctionDefId>,
    proc_def: bool,
    statement: StmtId,
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
                ArenaStmtKind::PureDef(function) | ArenaStmtKind::ProcDef(function)
                    if !program.arena.function_def(function).test_declaration =>
                {
                    let def = program.arena.function_def(function);
                    declarations.push(ReturnDeclaration {
                        name: def.name,
                        names: vec![def.name],
                        span: stmt.span,
                        function: Some(function),
                        proc_def: matches!(stmt.kind, ArenaStmtKind::ProcDef(_)),
                        statement,
                    });
                }
                ArenaStmtKind::Let { target, .. }
                | ArenaStmtKind::Const { target, .. }
                | ArenaStmtKind::Var { target, .. } => {
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
                            proc_def: false,
                            statement,
                        });
                    }
                }
                _ => {}
            }
        }
        if !declarations.iter().any(|declaration| declaration.function.is_some()) {
            return;
        }
        let function_indices = declarations.iter().enumerate().filter_map(|(index, declaration)| {
            let definition = declaration.function?;
            let def = program.arena.function_def(definition);
            Some((super::FunctionDeclarationId::new(self.effect_declaration_id(program, def.body)), index))
        }).collect::<BTreeMap<_, _>>();
        let mut binding_indices = BTreeMap::new();
        for (index, declaration) in declarations.iter().enumerate() {
            let target = match program.arena.stmt(declaration.statement).kind {
                ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Const { target, .. } | ArenaStmtKind::Var { target, .. } => target,
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
                let Some(id) = decl.function else {
                    let kind = program.arena.stmt(decl.statement).kind;
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
                        if let Some(alias) = alias
                            && let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind
                        { self.attach_callable_alias(name, alias, super::expr::expr_or_run_span_arena(program, initializer)); }
                        self.annotation_facts.truncate(before);
                    }
                    continue;
                };
                let def = program.arena.function_def(id);
                let infers_parameters = program.arena.params(def.params).iter().any(|parameter| {
                    parameter.annotation.is_none() && parameter.default.is_none()
                });
                if !def.return_ty_defaulted {
                    self.check_body_before_callers(program, source, decl, &declarations, def);
                    self.generalize_pending_function(program, id);
                    continue;
                }
                if decl.proc_def {
                    if proc_may_return_value(program, def) {
                        let boundary = if decl.name == "main" || decl.name.as_str().starts_with("main ") {
                            Some("entry")
                        } else {
                            None
                        };
                        self.infer_proc_return(program, source, decl, &declarations, def, boundary);
                        if infers_parameters {
                            self.check_body_before_callers(program, source, decl, &declarations, def);
                            self.generalize_pending_function(program, id);
                        }
                    } else if infers_parameters {
                        self.check_body_before_callers(program, source, decl, &declarations, def);
                        self.generalize_pending_function(program, id);
                    }
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
                for candidate in candidates {
                    inferred = Some(match inferred {
                        None => candidate.ty,
                        Some(previous) => self.unify_inferred_returns(previous, candidate.ty, candidate.span),
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
                    if !decl.proc_def {
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
        let definitions = component.iter().filter_map(|&index| declarations[index].function).collect::<Vec<_>>();
        let start = self.diagnostics.len();
        for &index in component {
            let declaration = &declarations[index];
            let Some(definition) = declaration.function else { continue; };
            let def = program.arena.function_def(definition);
            if !def.return_ty_defaulted { continue; }
            if declaration.proc_def && (declaration.name == "main" || declaration.name.as_str().starts_with("main "))
                && proc_may_return_value(program, def)
            {
                self.error(declaration.span, "entry proc returns a value and requires an explicit return annotation", DiagnosticCode::CheckRequiredReturn);
            }
            let body = program.arena.span(program.arena.block(def.body).span);
            let ty = if declaration.proc_def {
                if proc_may_return_value(program, def) {
                    Type::Result(Box::new(self.type_constraints.fresh_at_level(body, 1)), Box::new(Type::Error))
                } else { self.type_from_arena(program, def.return_ty) }
            } else { self.type_constraints.fresh_at_level(body, 1) };
            self.update_pending_return_type(program, def, ty.clone());
            self.function_return_types.insert(body, ty);
        }
        let mut propagations = Vec::new();
        for &index in component {
            let declaration = &declarations[index];
            let Some(definition) = declaration.function else {
                self.error(declaration.span, "recursive value bindings require a callable declaration boundary", DiagnosticCode::CheckLocalInference);
                continue;
            };
            let def = program.arena.function_def(definition);
            if !def.return_ty_defaulted {
                self.check_body_before_callers(program, source, declaration, declarations, def);
                continue;
            }
            self.inferred_returns = Some(Vec::new());
            self.inferred_propagations.clear();
            self.inference_reachable = true;
            self.return_conflicts = declaration.proc_def.then(Vec::new);
            self.check_body_before_callers(program, source, declaration, declarations, def);
            let candidates = self.inferred_returns.take().expect("recursive return checking owns its candidates");
            let errors = std::mem::take(&mut self.inferred_propagations);
            let body = program.arena.span(program.arena.block(def.body).span);
            let original = self.function_return_types[&body].clone();
            if declaration.proc_def {
                let actual = self.normalize_inferred_proc_returns(self.effect_declaration_id(program, def.body), candidates, &errors);
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
        if let Err(error) = self.solve_host_obligations() {
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
        for &definition in &definitions {
            let def = program.arena.function_def(definition);
            let body = program.arena.span(program.arena.block(def.body).span);
            let Some(ty) = self.function_return_types.get(&body) else { continue; };
            let ty = self.type_constraints.resolve(ty).unwrap_or(Type::Invalid);
            if !return_type_is_concrete(&ty) {
                self.error(body, "recursive return shape is not determined by its component", DiagnosticCode::CheckInferReturn);
            }
        }
        if self.diagnostics[start..].iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error) {
            self.reject_recursive_component(program, &definitions);
            return;
        }
        self.generalize_recursive_component(program, &definitions);
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
        self.check_function_arena(program, source, def, !decl.proc_def);
        self.scopes = visible_scopes;
    }

    /// A speculative value-body check decides the success type; the checker
    /// keeps only that type. A body whose completions are all Unit-like keeps
    /// the statement reading and `Result[Unit]`. Once any completion produces a
    /// value, every completion must join with it.
    fn infer_proc_return(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        decl: &ReturnDeclaration,
        declarations: &[ReturnDeclaration],
        def: &crate::syntax::arena::ArenaFunctionDef,
        boundary: Option<&str>,
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
        let mut probe = self.constraint_probe();
        self.scopes = visible_scopes;
        probe.inferred_returns = Some(Vec::new());
        probe.inferred_propagations.clear();
        probe.return_conflicts = Some(Vec::new());
        probe.inference_reachable = true;
        probe.boundary.current_return = None;
        probe.check_function_arena(program, source, def, false);
        let clean = !probe
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error);
        let candidates = probe.inferred_returns.take().unwrap_or_default();
        let conflicts = probe.return_conflicts.take().unwrap_or_default();
        let propagated = std::mem::take(&mut probe.inferred_propagations);
        if !candidates
            .iter()
            .map(|candidate| &candidate.ty)
            .chain(conflicts.iter().flat_map(|(left, right, _)| [left, right]))
            .any(produces_value)
        {
            let ty = self.type_from_arena(program, def.return_ty);
            self.update_pending_return_type(program, def, ty);
            return;
        }
        if !clean { return; }
        self.type_constraints = probe.type_constraints;
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
        let body_span = program.arena.span(program.arena.block(def.body).span);
        if let Some((left, right, span)) = conflicts.into_iter().next() {
            let (left, right) = if produces_value(&left) {
                (left, right)
            } else {
                (right, left)
            };
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
        let mut inferred = None;
        for candidate in candidates {
            let applications = candidate.origin.return_applications();
            let retain_applications = !applications.contains(&None);
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
                    match self.type_constraints.referenced_callable_application(reference)
                        .and_then(|id| self.type_constraints.defer_proc_application_return(id, &errors, candidate.span, level))
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
                } else { inferred_proc_return_envelope(candidate.ty.clone(), &errors) };
                inferred = Some(match inferred {
                    None => ty,
                    Some(previous) => self.unify_inferred_returns(previous, ty, candidate.span),
                });
            }
        }
        inferred.unwrap_or(Type::Invalid)
    }

    pub(super) fn unify_inferred_returns(&mut self, left: Type, right: Type, span: Span) -> Type {
        match unify_return_shapes(&left, &right) {
            Some(ty) => ty,
            None if matches!(left, Type::Invalid) || matches!(right, Type::Invalid) => {
                Type::Invalid
            }
            None if left.contains_inference() || right.contains_inference() => {
                let diagnostics = self.diagnostics.len();
                self.expect_type(&left, &right, span);
                if self.diagnostics.len() != diagnostics { return Type::Invalid; }
                self.type_constraints.resolve(&left).unwrap_or(Type::Invalid)
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

// Failure-only tails such as `Err(..)` and recursive calls fix no success
// type, so they join with any completion.
fn produces_value(ty: &Type) -> bool {
    match ty {
        Type::Unit | Type::Unknown | Type::Invalid => false,
        Type::Result(ok, _) => produces_value(ok),
        _ => true,
    }
}

pub(super) fn proc_may_return_value(
    program: &ArenaProgram,
    def: &crate::syntax::arena::ArenaFunctionDef,
) -> bool {
    let tail_value = program.arena.stmt_ids(program.arena.block(def.body).statements).last().is_some_and(|tail| match program.arena.stmt(program.arena.core_stmt_id(tail)).kind {
        ArenaStmtKind::Expr(_) | ArenaStmtKind::TailBareIdent(_) | ArenaStmtKind::Match { .. } | ArenaStmtKind::If { else_block: Some(_), .. } => true,
        ArenaStmtKind::Command(command) => matches!(program.arena.command_stmt(command).command,
            crate::syntax::arena::ArenaCommand::Run(run) if super::command::run_capture_result_type_arena(program, run).is_some()),
        _ => false,
    });
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
        | Type::Null
        | Type::Pure
        | Type::Proc
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

fn unify_return_shapes(left: &Type, right: &Type) -> Option<Type> {
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
            Some(Type::Optional(Box::new(unify_return_shapes(inner, other)?)))
        }
        (Type::Record(left), Type::Record(right)) if left.keys().eq(right.keys()) => {
            Some(Type::Record(
                left.iter()
                    .map(|(name, ty)| Some((*name, unify_return_shapes(ty, &right[name])?)))
                    .collect::<Option<std::collections::BTreeMap<_, _>>>()?
                    .into(),
            ))
        }
        (Type::List(left), Type::List(right)) => {
            Some(Type::List(Box::new(unify_return_shapes(left, right)?)))
        }
        (Type::Map(lk, left), Type::Map(rk, right)) => Some(Type::Map(
            Box::new(unify_return_shapes(lk, rk)?),
            Box::new(unify_return_shapes(left, right)?),
        )),
        (Type::Optional(left), Type::Optional(right)) => {
            Some(Type::Optional(Box::new(unify_return_shapes(left, right)?)))
        }
        (Type::Result(left, le), Type::Result(right, re)) => Some(Type::Result(
            Box::new(unify_return_shapes(left, right)?),
            Box::new(join_error_types(le, re)),
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
