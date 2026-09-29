use super::{AnnotationFact, AnnotationFactKind, Checker, FxHashMap, FxHashSet, Name, Span, Type};
use crate::syntax::arena::{ArenaBindingTargetKind, ArenaCompQualifier, ArenaExprKind, ArenaPatternKind, ArenaProgram, ArenaRecordFieldKind, ArenaStmtKind, BindingTargetId, ExprId, FunctionDefId, PatternId, StmtId};

struct ReturnDeclaration {
    name: Name,
    names: Vec<Name>,
    span: Span,
    function: Option<FunctionDefId>,
    statement: StmtId,
    exported: bool,
    parameters: FxHashSet<Name>,
}

impl Checker {
    /// Infer definitions in dependency order before checking their callers.
    /// An annotation fixes a callable boundary, but every recursive component
    /// still requires its unannotated pure members to declare that boundary.
    pub(super) fn infer_local_pure_returns(&mut self, program: &ArenaProgram, source: &str, statements: &[StmtId]) {
        let mut declarations = Vec::new();
        for &statement in statements {
            let (statement, exported) = match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => (inner, true),
                _ => (statement, false),
            };
            let stmt = program.arena.stmt(statement);
            match stmt.kind {
                ArenaStmtKind::PureDef(function) => {
                    let def = program.arena.function_def(function);
                    declarations.push(ReturnDeclaration {
                        name: def.name, names: vec![def.name], span: stmt.span, function: Some(function), statement, exported,
                        parameters: program.arena.params(def.params).iter().map(|param| param.name).collect(),
                    });
                }
                ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
                    let names = binding_names(program, target).into_iter().filter(|name| *name != "_").collect::<Vec<_>>();
                    if let Some(&name) = names.first() {
                        declarations.push(ReturnDeclaration { name, names, span: stmt.span, function: None, statement, exported, parameters: FxHashSet::default() });
                    }
                }
                _ => {}
            }
        }
        if !declarations.iter().any(|decl| decl.function.is_some_and(|id| program.arena.function_def(id).return_ty_defaulted)) { return; }
        let indices: FxHashMap<_, _> = declarations.iter().enumerate().flat_map(|(index, decl)| decl.names.iter().map(move |name| (*name, index))).collect();
        let mut shadows: FxHashMap<Name, Vec<Span>> = FxHashMap::default();
        let mut add_shadow = |target, span| {
            for name in binding_names(program, target) { shadows.entry(name).or_default().push(span); }
        };
        for block in &program.arena.blocks {
            let block_span = program.arena.span(block.span);
            for id in program.arena.stmt_ids(block.statements) {
                let stmt = program.arena.stmt(id);
                match stmt.kind {
                    ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. }
                    | ArenaStmtKind::Guard { target, .. } => {
                        add_shadow(target, Span::new(stmt.span.source_id, stmt.span.end(), block_span.end()));
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
            for param in program.arena.block_params(block.params) { shadows.entry(param.name).or_default().push(span); }
        }
        for raw in 0..program.arena.stmt_tags.len() {
            match program.arena.stmt(StmtId::from_index(raw)).kind {
                ArenaStmtKind::Match { arms, .. } => for arm in program.arena.match_arms(arms) {
                    let span = program.arena.span(arm.span);
                    for name in pattern_binding_names(program, arm.pattern) { shadows.entry(name).or_default().push(span); }
                },
                ArenaStmtKind::If { branches, .. } => {
                    for branch in program.arena.if_branches(branches) {
                        let span = program.arena.span(program.arena.block(branch.block).span);
                        for name in pattern_condition_bindings(program, branch.condition) { shadows.entry(name).or_default().push(span); }
                    }
                }
                ArenaStmtKind::While { condition, block } => {
                    let span = program.arena.span(program.arena.block(block).span);
                    for name in pattern_condition_bindings(program, condition) { shadows.entry(name).or_default().push(span); }
                }
                ArenaStmtKind::With { bindings, body, .. } => {
                    let span = program.arena.span(program.arena.block(body).span);
                    for binding in program.arena.with_bindings(bindings) { shadows.entry(binding.name).or_default().push(span); }
                }
                _ => {}
            }
        }
        for raw in 0..program.arena.expr_tags.len() {
            let expr = program.arena.expr(ExprId::from_index(raw));
            match expr.kind {
                ArenaExprKind::Match { arms, .. } => for arm in program.arena.match_expr_arms(arms) {
                    let span = program.arena.span(arm.span);
                    for name in pattern_binding_names(program, arm.pattern) { shadows.entry(name).or_default().push(span); }
                },
                ArenaExprKind::If { branches, .. } => {
                    for branch in program.arena.if_expr_branches(branches) {
                        let span = program.arena.expr(branch.value).span;
                        for name in pattern_condition_bindings(program, branch.condition) { shadows.entry(name).or_default().push(span); }
                    }
                }
                ArenaExprKind::ListComp { qualifiers, .. } | ArenaExprKind::MapComp { qualifiers, .. } => {
                    for qualifier in program.arena.comp_qualifiers(qualifiers) {
                        if let ArenaCompQualifier::For { target, iter, .. } = qualifier {
                            let span = Span::new(expr.span.source_id, program.arena.expr(*iter).span.end(), expr.span.end());
                            for name in binding_names(program, *target) {
                                shadows.entry(name).or_default().push(span);
                                let outputs = match expr.kind {
                                    ArenaExprKind::ListComp { expr, .. } => vec![expr],
                                    ArenaExprKind::MapComp { key, value, .. } => vec![key, value],
                                    _ => Vec::new(),
                                };
                                for output in outputs { shadows.entry(name).or_default().push(program.arena.expr(output).span); }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let mut callees = FxHashSet::default();
        for raw in 0..program.arena.expr_tags.len() {
            if let ArenaExprKind::Call { callee, .. } = program.arena.expr(ExprId::from_index(raw)).kind { callees.insert(callee); }
        }
        // Named record keys may have speculative Ident rows in the arena.
        // Their spelling selects a field and does not read a binding.
        let record_keys = program.arena.record_fields.iter().filter_map(|field| match field.kind {
            ArenaRecordFieldKind::Path { path, span, .. } => {
                let span = program.arena.span(span);
                program.arena.names(path).next().map(|name| (span.source_id, span.start(), name))
            }
            ArenaRecordFieldKind::Named { name, span, .. } => {
                let span = program.arena.span(span);
                Some((span.source_id, span.start(), name))
            }
            _ => None,
        }).collect::<FxHashSet<_>>();
        let mut references = Vec::new();
        for raw in 0..program.arena.expr_tags.len() {
            let id = ExprId::from_index(raw);
            let expr = program.arena.expr(id);
            if let ArenaExprKind::Ident(name) = expr.kind
                && !record_keys.contains(&(expr.span.source_id, expr.span.start(), name)) {
                references.push((expr.span, name, callees.contains(&id)));
            }
            if let ArenaExprKind::Record(fields) = expr.kind {
                for field in program.arena.record_fields(fields) {
                    if let ArenaRecordFieldKind::Shorthand { name, span } = &field.kind { references.push((program.arena.span(*span), *name, false)); }
                }
            }
        }
        for raw in 0..program.arena.stmt_tags.len() {
            let stmt = program.arena.stmt(StmtId::from_index(raw));
            if let ArenaStmtKind::TailBareIdent(name) = stmt.kind { references.push((stmt.span, name, false)); }
        }
        references.sort_unstable_by_key(|(span, _, _)| (span.source_id, span.start()));
        let mut edges = vec![Vec::new(); declarations.len()];
        for (index, decl) in declarations.iter().enumerate() {
            // Binding target spellings are declarations, not dependencies.
            // Parser speculation may retain nodes with those source spans.
            let dependency_span = match program.arena.stmt(decl.statement).kind {
                ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Var { initializer, .. } =>
                    super::expr::expr_or_run_span_arena(program, initializer),
                _ => decl.span,
            };
            let start = references.partition_point(|(span, _, _)| (span.source_id, span.start()) < (dependency_span.source_id, dependency_span.start()));
            for &(span, name, callee) in &references[start..] {
                if span.source_id != dependency_span.source_id || span.start() >= dependency_span.end() { break; }
                if span.end() > dependency_span.end() { continue; }
                let parameter_shadow = decl.parameters.contains(&name) && decl.function.is_some_and(|id| {
                    let body_span = program.arena.span(program.arena.block(program.arena.function_def(id).body).span);
                    span.start() >= body_span.start() && span.end() <= body_span.end()
                });
                let shadowed = parameter_shadow || shadows.get(&name).is_some_and(|ranges| ranges.iter().any(|range|
                    range.source_id == span.source_id && range.start() <= span.start() && range.end() >= span.end()));
                if (!shadowed || callee && self.pures.contains_key(&name)) && let Some(&dependency) = indices.get(&name) {
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
                self.check_use_arena(program, use_stmt.path, use_stmt.alias, use_stmt.resolved.as_deref(), program.arena.stmt(id).span);
            }
        }
        for index in order {
            let decl = &declarations[index];
            let Some(id) = decl.function else {
                if !cyclic.contains(&index) {
                    let kind = program.arena.stmt(decl.statement).kind;
                    if let ArenaStmtKind::Let { target, ty, initializer } | ArenaStmtKind::Var { target, ty, initializer } = kind {
                        let before = self.annotation_facts.len();
                        let actual = self.check_expr_or_run_arena(program, source, initializer, None);
                        let ty = ty.map(|id| self.type_from_arena(program, id)).unwrap_or(actual);
                        self.define_binding_target_arena(program, target, &ty, matches!(kind, ArenaStmtKind::Var { .. }), decl.span);
                        self.annotation_facts.truncate(before);
                    }
                }
                continue;
            };
            let def = program.arena.function_def(id);
            if !def.return_ty_defaulted { continue; }
            let body_span = program.arena.span(program.arena.block(def.body).span);
            if decl.exported || cyclic.contains(&index) {
                let message = if decl.exported {
                    format!("exported pure `{}` requires an explicit return annotation", decl.name)
                } else {
                    format!("recursive pure `{}` requires an explicit return annotation", decl.name)
                };
                self.error(decl.span, &message, "check.required-return");
                self.function_return_types.insert(body_span, Type::Invalid);
                if let Some(sig) = self.pures.get_mut(&decl.name) { sig.return_ty = Type::Invalid; }
                continue;
            }
            self.inferred_returns = Some(Vec::new());
            self.inferred_propagations.clear();
            self.inference_reachable = true;
            // Callable signatures are available independently of declaration
            // order. Captured values retain their lexical prefix visibility.
            let visible_scopes = self.scopes.clone();
            for binding in &declarations {
                if binding.function.is_none() && binding.span.source_id == decl.span.source_id
                    && binding.span.start() >= decl.span.start() {
                    for name in &binding.names { self.current_scope_mut().remove(name); }
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
            let ty = if return_type_is_concrete(&ty) { ty } else {
                self.error(decl.span, &format!("pure `{}` needs a return annotation: its return shape is underdetermined", decl.name), "check.infer-return");
                Type::Invalid
            };
            let mut ty = ty;
            for (error, span) in std::mem::take(&mut self.inferred_propagations) {
                match &ty {
                    Type::Result(_, expected) if error.matches_expected(expected) => {}
                    Type::Result(_, _) => {
                        self.error(span, "propagated error does not match the inferred Result; declare its return type", "check.try-error");
                        ty = Type::Invalid;
                    }
                    _ => {
                        self.error(span, "`?` requires a Result return determined by the body; declare a return annotation", "check.try-context");
                        ty = Type::Invalid;
                    }
                }
            }
            self.function_return_types.insert(body_span, ty.clone());
            if let Some(sig) = self.pures.get_mut(&decl.name) { sig.return_ty = ty.clone(); }
            if ty != Type::Invalid {
                self.annotation_facts.push(AnnotationFact { kind: AnnotationFactKind::InferredPureReturn { body: body_span }, ty });
            }
        }
        self.scopes = saved_scopes;
    }

    pub(super) fn unify_inferred_returns(&mut self, left: Type, right: Type, span: Span) -> Type {
        match unify_return_shapes(&left, &right) {
            Some(ty) => ty,
            None => {
                self.error(span, &format!("incompatible inferred return paths `{left}` and `{right}`; declare a return type"), "check.infer-return");
                Type::Invalid
            }
        }
    }
}

fn return_type_is_concrete(ty: &Type) -> bool {
    match ty {
        Type::Any | Type::Unknown | Type::Invalid | Type::Null | Type::Pure | Type::Proc | Type::DynamicModule => false,
        Type::List(inner) | Type::Map(inner) | Type::Stream(inner) | Type::Optional(inner) => return_type_is_concrete(inner),
        Type::Result(ok, err) => return_type_is_concrete(ok) && return_type_is_concrete(err),
        Type::Record(fields) => fields.values().all(return_type_is_concrete),
        Type::Module(_) => false,
        _ => true,
    }
}

fn unify_return_shapes(left: &Type, right: &Type) -> Option<Type> {
    if left == right { return Some(left.clone()); }
    match (left, right) {
        (Type::Unknown, other) | (other, Type::Unknown) => Some(other.clone()),
        (Type::Null, Type::Optional(inner)) | (Type::Optional(inner), Type::Null) => Some(Type::Optional(inner.clone())),
        (Type::Null, other) | (other, Type::Null) => Some(Type::Optional(Box::new(other.clone()))),
        (Type::Error, Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError)
        | (Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError, Type::Error) => Some(Type::Error),
        (Type::Optional(inner), other) | (other, Type::Optional(inner)) if !matches!(other, Type::Optional(_)) =>
            Some(Type::Optional(Box::new(unify_return_shapes(inner, other)?))),
        (Type::Record(left), Type::Record(right)) if left.keys().eq(right.keys()) => {
            Some(Type::Record(left.iter().map(|(name, ty)| Some((*name, unify_return_shapes(ty, &right[name])?))).collect::<Option<_>>()?))
        }
        (Type::List(left), Type::List(right)) => Some(Type::List(Box::new(unify_return_shapes(left, right)?))),
        (Type::Map(left), Type::Map(right)) => Some(Type::Map(Box::new(unify_return_shapes(left, right)?))),
        (Type::Optional(left), Type::Optional(right)) => Some(Type::Optional(Box::new(unify_return_shapes(left, right)?))),
        (Type::Result(left, le), Type::Result(right, re)) => Some(Type::Result(Box::new(unify_return_shapes(left, right)?), Box::new(unify_return_shapes(le, re)?))),
        _ => None,
    }
}

// Iterative depth-first traversals bound host stack use independently of the
// number of declarations. Reverse edges identify complete recursive groups.
fn declaration_dependency_order(edges: &[Vec<usize>]) -> (Vec<usize>, FxHashSet<usize>) {
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::new();
    for start in 0..edges.len() {
        if visited[start] { continue; }
        visited[start] = true;
        let mut work = vec![(start, 0)];
        while let Some((node, next)) = work.last_mut() {
            if let Some(&child) = edges[*node].get(*next) {
                *next += 1;
                if !visited[child] { visited[child] = true; work.push((child, 0)); }
            } else { let (node, _) = work.pop().unwrap(); order.push(node); }
        }
    }
    let mut reverse = vec![Vec::new(); edges.len()];
    for (node, children) in edges.iter().enumerate() { for &child in children { reverse[child].push(node); } }
    visited.fill(false);
    let mut cyclic = FxHashSet::default();
    for &start in order.iter().rev() {
        if visited[start] { continue; }
        visited[start] = true;
        let mut members = Vec::new();
        let mut work = vec![start];
        while let Some(node) = work.pop() {
            members.push(node);
            for &child in &reverse[node] { if !visited[child] { visited[child] = true; work.push(child); } }
        }
        if members.len() > 1 || edges[start].contains(&start) { cyclic.extend(members); }
    }
    (order, cyclic)
}

fn binding_names(program: &ArenaProgram, target: BindingTargetId) -> Vec<Name> {
    let mut names = Vec::new();
    let mut work = vec![target];
    while let Some(target) = work.pop() {
        match program.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => names.push(name),
            ArenaBindingTargetKind::Record { fields, .. } => work.extend(program.arena.destructure_fields(fields).iter().map(|field| field.target)),
        }
    }
    names
}

fn pattern_binding_names(program: &ArenaProgram, pattern: PatternId) -> Vec<Name> {
    let mut names = Vec::new();
    let mut work = vec![pattern];
    while let Some(pattern) = work.pop() {
        match program.arena.pattern(pattern).kind {
            ArenaPatternKind::Alias { pattern, name, .. } => { names.push(name); work.push(pattern); }
            ArenaPatternKind::Group(pattern) => work.push(pattern),
            ArenaPatternKind::List { elements, rest } => { work.extend(program.arena.pattern_ids(elements)); work.extend(rest); }
            ArenaPatternKind::Binding(name) | ArenaPatternKind::Type { binding: Some(name), .. } => names.push(name),
            ArenaPatternKind::Record { fields, .. } | ArenaPatternKind::ErrorVariant { fields, .. } => work.extend(program.arena.pattern_fields(fields).iter().map(|field| field.pattern)),
            ArenaPatternKind::Constructor { arg: Some(pattern), .. } => work.push(pattern),
            ArenaPatternKind::Tuple(patterns) | ArenaPatternKind::Alternation(patterns) => work.extend(program.arena.pattern_ids(patterns)),
            _ => {}
        }
    }
    names
}

// Pattern captures are visible only in the selected body, never in the
// condition subject or a sibling branch.
fn pattern_condition_bindings(program: &ArenaProgram, condition: ExprId) -> Vec<Name> {
    match program.arena.expr(condition).kind {
        ArenaExprKind::PatternCondition { arms, .. } =>
            pattern_binding_names(program, program.arena.match_expr_arms(arms)[0].pattern),
        _ => Vec::new(),
    }
}
