use super::{AnnotationFact, AnnotationFactKind, Checker, FxHashMap, FxHashSet, Name, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaCompQualifier, ArenaExprKind, ArenaPatternKind, ArenaProgram,
    ArenaRecordFieldKind, ArenaStmtKind, BindingTargetId, ExprId, FunctionDefId, PatternId, StmtId,
};

struct ReturnDeclaration {
    name: Name,
    names: Vec<Name>,
    span: Span,
    function: Option<FunctionDefId>,
    proc_def: bool,
    statement: StmtId,
    exported: bool,
    parameters: FxHashSet<Name>,
}

/// Where the program reads and rebinds names. Dependency ordering consults
/// it for every module, so it is collected once per checked program rather
/// than once per module.
pub(super) struct ReturnInferenceIndex {
    /// The source ranges in which a local binding hides each name.
    shadows: FxHashMap<Name, Vec<Span>>,
    /// Every name read, in source order, and whether it is a call's callee.
    references: Vec<(Span, Name, bool)>,
}

impl ReturnInferenceIndex {
    fn collect(program: &ArenaProgram) -> Self {
        let mut shadows: FxHashMap<Name, Vec<Span>> = FxHashMap::default();
        let mut add_shadow = |target, span| {
            for name in binding_names(program, target) {
                shadows.entry(name).or_default().push(span);
            }
        };
        for block in &program.arena.blocks {
            let block_span = program.arena.span(block.span);
            for id in program.arena.stmt_ids(block.statements) {
                let stmt = program.arena.stmt(program.arena.core_stmt_id(id));
                match stmt.kind {
                    ArenaStmtKind::Let { target, .. }
                    | ArenaStmtKind::Const { target, .. }
                    | ArenaStmtKind::Var { target, .. }
                    | ArenaStmtKind::Guard { target, .. } => {
                        add_shadow(
                            target,
                            Span::new(stmt.span.source_id, stmt.span.end(), block_span.end()),
                        );
                    }
                    ArenaStmtKind::For { target, block, .. } => {
                        add_shadow(target, program.arena.span(program.arena.block(block).span));
                    }
                    _ => {}
                }
            }
        }
        drop(add_shadow);
        for block in &program.arena.blocks {
            let span = program.arena.span(block.span);
            for param in program.arena.block_params(block.params) {
                shadows.entry(param.name).or_default().push(span);
            }
        }
        for raw in 0..program.arena.stmt_tags.len() {
            match program.arena.stmt(StmtId::from_index(raw)).kind {
                ArenaStmtKind::Match { arms, .. } => {
                    for arm in program.arena.match_arms(arms) {
                        let span = program.arena.span(arm.span);
                        for name in pattern_binding_names(program, arm.pattern) {
                            shadows.entry(name).or_default().push(span);
                        }
                    }
                }
                ArenaStmtKind::If { branches, .. } => {
                    for branch in program.arena.if_branches(branches) {
                        let span = program.arena.span(program.arena.block(branch.block).span);
                        for name in pattern_condition_bindings(program, branch.condition) {
                            shadows.entry(name).or_default().push(span);
                        }
                    }
                }
                ArenaStmtKind::While { condition, block } => {
                    let span = program.arena.span(program.arena.block(block).span);
                    for name in pattern_condition_bindings(program, condition) {
                        shadows.entry(name).or_default().push(span);
                    }
                }
                ArenaStmtKind::With { bindings, body, .. } => {
                    let span = program.arena.span(program.arena.block(body).span);
                    for binding in program.arena.with_bindings(bindings) {
                        shadows.entry(binding.name).or_default().push(span);
                    }
                }
                _ => {}
            }
        }
        for raw in 0..program.arena.expr_tags.len() {
            let expr = program.arena.expr(ExprId::from_index(raw));
            match expr.kind {
                ArenaExprKind::Match { arms, .. } => {
                    for arm in program.arena.match_expr_arms(arms) {
                        let span = program.arena.span(arm.span);
                        for name in pattern_binding_names(program, arm.pattern) {
                            shadows.entry(name).or_default().push(span);
                        }
                    }
                }
                ArenaExprKind::If { branches, .. } => {
                    for branch in program.arena.if_expr_branches(branches) {
                        let span = program.arena.expr(branch.value).span;
                        for name in pattern_condition_bindings(program, branch.condition) {
                            shadows.entry(name).or_default().push(span);
                        }
                    }
                }
                // A resource binding is in scope from the end of its value
                // to the end of the scope.
                ArenaExprKind::ResourceScope { bindings, .. } => {
                    for binding in program.arena.with_bindings(bindings) {
                        let span = Span::new(
                            expr.span.source_id,
                            program.arena.expr(binding.initializer).span.end(),
                            expr.span.end(),
                        );
                        shadows.entry(binding.name).or_default().push(span);
                    }
                }
                ArenaExprKind::ListComp { qualifiers, .. }
                | ArenaExprKind::MapComp { qualifiers, .. } => {
                    for qualifier in program.arena.comp_qualifiers(qualifiers) {
                        if let ArenaCompQualifier::For { target, iter, .. } = qualifier {
                            let span = Span::new(
                                expr.span.source_id,
                                program.arena.expr(*iter).span.end(),
                                expr.span.end(),
                            );
                            for name in binding_names(program, *target) {
                                shadows.entry(name).or_default().push(span);
                                let outputs = match expr.kind {
                                    ArenaExprKind::ListComp { expr, .. } => vec![expr],
                                    ArenaExprKind::MapComp { key, value, .. } => vec![key, value],
                                    _ => Vec::new(),
                                };
                                for output in outputs {
                                    shadows
                                        .entry(name)
                                        .or_default()
                                        .push(program.arena.expr(output).span);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let mut callees = FxHashSet::default();
        for raw in 0..program.arena.expr_tags.len() {
            if let ArenaExprKind::Call { callee, .. } =
                program.arena.expr(ExprId::from_index(raw)).kind
            {
                callees.insert(callee);
            }
        }
        // Named record keys may have speculative Ident rows in the arena.
        // Their spelling selects a field and does not read a binding.
        let record_keys = program
            .arena
            .record_fields
            .iter()
            .filter_map(|field| match field.kind {
                ArenaRecordFieldKind::Path { path, span, .. } => {
                    let span = program.arena.span(span);
                    program
                        .arena
                        .names(path)
                        .next()
                        .map(|name| (span.source_id, span.start(), name))
                }
                ArenaRecordFieldKind::Named { name, span, .. } => {
                    let span = program.arena.span(span);
                    Some((span.source_id, span.start(), name))
                }
                _ => None,
            })
            .collect::<FxHashSet<_>>();
        let mut references = Vec::new();
        for raw in 0..program.arena.expr_tags.len() {
            let id = ExprId::from_index(raw);
            let expr = program.arena.expr(id);
            if let ArenaExprKind::Ident(name) = expr.kind
                && !record_keys.contains(&(expr.span.source_id, expr.span.start(), name))
            {
                references.push((expr.span, name, callees.contains(&id)));
            }
            if let ArenaExprKind::Record(fields) = expr.kind {
                for field in program.arena.record_fields(fields) {
                    if let ArenaRecordFieldKind::Shorthand { name, span } = &field.kind {
                        references.push((program.arena.span(*span), *name, false));
                    }
                }
            }
        }
        for raw in 0..program.arena.stmt_tags.len() {
            let stmt = program.arena.stmt(StmtId::from_index(raw));
            if let ArenaStmtKind::TailBareIdent(name) = stmt.kind {
                references.push((stmt.span, name, false));
            }
        }
        references.sort_unstable_by_key(|(span, _, _)| (span.source_id, span.start()));
        Self {
            shadows,
            references,
        }
    }
}

impl Checker {
    /// Infer definitions in dependency order before checking their callers.
    /// An annotation fixes a callable boundary, but every recursive component
    /// still requires its unannotated pure members to declare that boundary.
    /// Unannotated procs whose body may produce a value are probed here too;
    /// their bodies are still checked in source order with the inferred return.
    pub(super) fn infer_local_pure_returns(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        statements: &[StmtId],
    ) {
        let mut declarations = Vec::new();
        for &statement in statements {
            let (statement, exported) = match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => (inner, true),
                _ => (statement, false),
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
                        exported,
                        parameters: program
                            .arena
                            .params(def.params)
                            .iter()
                            .map(|param| param.name)
                            .collect(),
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
                            exported,
                            parameters: FxHashSet::default(),
                        });
                    }
                }
                _ => {}
            }
        }
        if !declarations.iter().any(|decl| {
            decl.function.is_some_and(|id| {
                let def = program.arena.function_def(id);
                def.return_ty_defaulted && (!decl.proc_def || proc_may_return_value(program, def))
            })
        }) {
            return;
        }
        let indices: FxHashMap<_, _> = declarations
            .iter()
            .enumerate()
            .flat_map(|(index, decl)| decl.names.iter().map(move |name| (*name, index)))
            .collect();
        let index = match &self.return_inference_index {
            Some(index) => index.clone(),
            None => {
                let index = std::sync::Arc::new(ReturnInferenceIndex::collect(program));
                self.return_inference_index = Some(index.clone());
                index
            }
        };
        let ReturnInferenceIndex {
            shadows,
            references,
        } = &*index;
        let mut edges = vec![Vec::new(); declarations.len()];
        for (index, decl) in declarations.iter().enumerate() {
            // Binding target spellings are declarations, not dependencies.
            // Parser speculation may retain nodes with those source spans.
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
            for &(span, name, callee) in &references[start..] {
                if span.source_id != dependency_span.source_id
                    || span.start() >= dependency_span.end()
                {
                    break;
                }
                if span.end() > dependency_span.end() {
                    continue;
                }
                let parameter_shadow = decl.parameters.contains(&name)
                    && decl.function.is_some_and(|id| {
                        let body_span = program.arena.span(
                            program
                                .arena
                                .block(program.arena.function_def(id).body)
                                .span,
                        );
                        span.start() >= body_span.start() && span.end() <= body_span.end()
                    });
                let shadowed = parameter_shadow
                    || shadows.get(&name).is_some_and(|ranges| {
                        ranges.iter().any(|range| {
                            range.source_id == span.source_id
                                && range.start() <= span.start()
                                && range.end() >= span.end()
                        })
                    });
                if (!shadowed
                    || callee && (self.pures.contains_key(&name) || self.procs.contains_key(&name)))
                    && let Some(&dependency) = indices.get(&name)
                {
                    edges[index].push(dependency);
                }
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
                    program.arena.stmt(id).span,
                );
            }
        }
        for index in order {
            let decl = &declarations[index];
            let Some(id) = decl.function else {
                if !cyclic.contains(&index) {
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
                        // This pass only needs the binding's type. The
                        // statement check reports everything about the
                        // initializer, with its schema context, so nothing
                        // reported here is kept: it would be said twice, or
                        // said only because that context is missing.
                        let reported = self.diagnostics.len();
                        // The annotation is the initializer's expected type
                        // here as it is there: a target-typed initializer has
                        // no type without it.
                        let annotated = ty.map(|id| self.type_from_arena(program, id));
                        let actual = self.check_expr_or_run_arena(
                            program,
                            source,
                            initializer,
                            annotated.as_ref(),
                        );
                        self.diagnostics.truncate(reported);
                        let ty = annotated.unwrap_or(actual);
                        self.define_binding_target_arena(
                            program,
                            target,
                            &ty,
                            matches!(kind, ArenaStmtKind::Var { .. }),
                            decl.span,
                        );
                        self.annotation_facts.truncate(before);
                    }
                }
                continue;
            };
            let def = program.arena.function_def(id);
            if !def.return_ty_defaulted {
                continue;
            }
            if decl.proc_def {
                if proc_may_return_value(program, def) {
                    let boundary = if decl.exported {
                        Some("exported")
                    } else if decl.name == "main" {
                        Some("entry")
                    } else if cyclic.contains(&index) {
                        Some("recursive")
                    } else {
                        None
                    };
                    self.infer_proc_return(program, source, decl, &declarations, def, boundary);
                }
                continue;
            }
            let body_span = program.arena.span(program.arena.block(def.body).span);
            if decl.exported || cyclic.contains(&index) {
                let message = if decl.exported {
                    format!(
                        "exported pure `{}` requires an explicit return annotation",
                        decl.name
                    )
                } else {
                    format!(
                        "recursive pure `{}` requires an explicit return annotation",
                        decl.name
                    )
                };
                self.error(decl.span, &message, DiagnosticCode::CheckRequiredReturn);
                self.function_return_types.insert(body_span, Type::Invalid);
                if let Some(sig) = self.pures.get_mut(&decl.name) {
                    sig.return_ty = Type::Invalid;
                }
                continue;
            }
            self.inferred_returns = Some(Vec::new());
            self.inferred_propagations.clear();
            self.inference_reachable = true;
            // Callable signatures are available independently of declaration
            // order. Captured values retain their lexical prefix visibility.
            let visible_scopes = self.scopes.clone();
            for binding in &declarations {
                if binding.function.is_none()
                    && binding.span.source_id == decl.span.source_id
                    && binding.span.start() >= decl.span.start()
                {
                    for name in &binding.names {
                        self.current_scope_mut().remove(name);
                    }
                }
            }
            self.check_function_arena(program, source, def, true);
            self.scopes = visible_scopes;
            let candidates = self.inferred_returns.take().unwrap();
            let mut inferred = None;
            for (candidate, span) in candidates {
                inferred = Some(match inferred {
                    None => candidate,
                    Some(previous) => self.unify_inferred_returns(previous, candidate, span),
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
            if let Some(sig) = self.pures.get_mut(&decl.name) {
                sig.return_ty = ty.clone();
            }
            if ty != Type::Invalid {
                self.annotation_facts.push(AnnotationFact {
                    kind: AnnotationFactKind::InferredPureReturn { body: body_span },
                    ty,
                });
            }
        }
        self.scopes = saved_scopes;
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
        probe.current_return = None;
        // Recursive calls contribute no completion, as for inferred pures.
        if boundary == Some("recursive")
            && let Some(sig) = probe.procs.get_mut(&decl.name)
        {
            sig.return_ty = Type::Unknown;
        }
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
            .map(|(ty, _)| ty)
            .chain(conflicts.iter().flat_map(|(left, right, _)| [left, right]))
            .any(produces_value)
        {
            return;
        }
        // Once a completion produces a value, a statement completion conflicts
        // with it as any other disagreeing completion does.
        self.return_conflicts = Some(conflicts);
        let mut inferred: Option<Type> = None;
        for (candidate, span) in candidates {
            inferred = Some(match inferred {
                None => candidate,
                Some(previous) => self.unify_inferred_returns(previous, candidate, span),
            });
        }
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
            if let Some(sig) = self.procs.get_mut(&decl.name) {
                sig.return_ty = Type::Invalid;
            }
            return;
        }
        if !clean {
            return;
        }
        let ty = if let Some(boundary) = boundary {
            self.error(decl.span, &format!("{boundary} proc `{}` returns a value and requires an explicit return annotation", decl.name), DiagnosticCode::CheckRequiredReturn);
            Type::Invalid
        } else {
            match inferred.unwrap_or(Type::Invalid) {
                Type::Invalid => Type::Invalid,
                ty if !return_type_is_concrete(&ty) => {
                    self.error(decl.span, &format!("proc `{}` needs a return annotation: its return shape is underdetermined", decl.name), DiagnosticCode::CheckInferReturn);
                    Type::Invalid
                }
                // Every `?` in the body propagates through the inferred error
                // type, so it joins with the errors of `Err(..)` completions.
                Type::Result(ok, err) => Type::Result(
                    ok,
                    Box::new(
                        propagated
                            .iter()
                            .fold(*err, |joined, (ty, _)| join_error_types(&joined, ty)),
                    ),
                ),
                ty => Type::Result(Box::new(ty), Box::new(Type::Error)),
            }
        };
        self.function_return_types.insert(body_span, ty.clone());
        if let Some(sig) = self.procs.get_mut(&decl.name) {
            sig.return_ty = ty;
        }
    }

    pub(super) fn unify_inferred_returns(&mut self, left: Type, right: Type, span: Span) -> Type {
        match unify_return_shapes(&left, &right) {
            Some(ty) => ty,
            None if matches!(left, Type::Invalid) || matches!(right, Type::Invalid) => {
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

// Failure-only tails such as `Err(..)` and recursive calls fix no success
// type, so they join with any completion.
fn produces_value(ty: &Type) -> bool {
    match ty {
        Type::Unit | Type::Unknown | Type::Invalid => false,
        Type::Result(ok, _) => produces_value(ok),
        _ => true,
    }
}

// Only these completions can make a proc body a value body. Everything else
// completes as a statement, so the probe would contribute nothing.
fn proc_may_return_value(
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
        || (0..program.arena.stmt_tags.len()).any(|raw| {
            let stmt = program.arena.stmt(StmtId::from_index(raw));
            matches!(stmt.kind, ArenaStmtKind::Return(Some(_)))
                && stmt.span.source_id == body.source_id
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
        (Type::Null, Type::Optional(inner)) | (Type::Optional(inner), Type::Null) => {
            Some(Type::Optional(inner.clone()))
        }
        // A failure-only `Err(..)` completion fixes the error type of a plain value.
        (Type::Result(ok, err), other) | (other, Type::Result(ok, err))
            if **ok == Type::Unknown && !matches!(other, Type::Result(_, _) | Type::Unit) =>
        {
            Some(Type::Result(Box::new(other.clone()), err.clone()))
        }
        (Type::Null, other) | (other, Type::Null) => Some(Type::Optional(Box::new(other.clone()))),
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
                    .collect::<Option<_>>()?,
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

// Failures from different completions join to their narrowest common error:
// variants of one family join to the family, anything else to `Error`.
fn join_error_types(left: &Type, right: &Type) -> Type {
    match (left, right) {
        _ if left == right => left.clone(),
        (Type::Unknown, other) | (other, Type::Unknown) => other.clone(),
        (Type::ErrorVariant { family, .. }, Type::ErrorVariant { family: other, .. })
        | (Type::ErrorVariant { family, .. }, Type::ErrorFamily(other))
        | (Type::ErrorFamily(family), Type::ErrorVariant { family: other, .. })
            if family == other =>
        {
            Type::ErrorFamily(*family)
        }
        _ => Type::Error,
    }
}

// Iterative depth-first traversals bound host stack use independently of the
// number of declarations. Reverse edges identify complete recursive groups.
fn declaration_dependency_order(edges: &[Vec<usize>]) -> (Vec<usize>, FxHashSet<usize>) {
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
            cyclic.extend(members);
        }
    }
    (order, cyclic)
}

fn binding_names(program: &ArenaProgram, target: BindingTargetId) -> Vec<Name> {
    let mut names = Vec::new();
    let mut work = vec![target];
    while let Some(target) = work.pop() {
        match program.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => names.push(name),
            ArenaBindingTargetKind::Record { fields, .. } => work.extend(
                program
                    .arena
                    .destructure_fields(fields)
                    .iter()
                    .map(|field| field.target),
            ),
        }
    }
    names
}

fn pattern_binding_names(program: &ArenaProgram, pattern: PatternId) -> Vec<Name> {
    let mut names = Vec::new();
    let mut work = vec![pattern];
    while let Some(pattern) = work.pop() {
        match program.arena.pattern(pattern).kind {
            ArenaPatternKind::Alias { pattern, name, .. } => {
                names.push(name);
                work.push(pattern);
            }
            ArenaPatternKind::Group(pattern) => work.push(pattern),
            ArenaPatternKind::List { elements, rest } => {
                work.extend(program.arena.pattern_ids(elements));
                work.extend(rest);
            }
            ArenaPatternKind::Binding(name)
            | ArenaPatternKind::Type {
                binding: Some(name),
                ..
            } => names.push(name),
            ArenaPatternKind::Record { fields, .. }
            | ArenaPatternKind::ErrorVariant { fields, .. } => work.extend(
                program
                    .arena
                    .pattern_fields(fields)
                    .iter()
                    .map(|field| field.pattern),
            ),
            ArenaPatternKind::Constructor {
                arg: Some(pattern), ..
            } => work.push(pattern),
            ArenaPatternKind::Tuple(patterns)
            | ArenaPatternKind::Alternation(patterns)
            | ArenaPatternKind::Text(patterns) => work.extend(program.arena.pattern_ids(patterns)),
            ArenaPatternKind::TextHole {
                binding: Some(name),
                ..
            } => names.push(name),
            _ => {}
        }
    }
    names
}

// Pattern captures are visible only in the selected body, never in the
// condition subject or a sibling branch.
fn pattern_condition_bindings(program: &ArenaProgram, condition: ExprId) -> Vec<Name> {
    match program.arena.expr(condition).kind {
        ArenaExprKind::PatternCondition { arms, .. } => {
            pattern_binding_names(program, program.arena.match_expr_arms(arms)[0].pattern)
        }
        _ => Vec::new(),
    }
}
