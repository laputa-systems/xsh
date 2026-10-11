use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaCallArgKind, ArenaChild,
    ArenaExprKind, ArenaExprOrRun, ArenaProgram, ArenaRange, ArenaStmtKind, ArenaUserModule,
    AssignTargetId, BlockId, ExprId, StmtId, SugarView,
};
use crate::syntax::node::AssignOp;
use super::{BindingId, CheckBundleView, Checker, Diagnostic, DiagnosticCode, Label, Name, Span, Type};
use rustc_hash::{FxHashMap, FxHashSet};
use crate::modules::ManagedResource;
use xsh_registry::managed_resource::ResourceConsumption;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResourceTransferSite {
    Return(StmtId),
    BlockValue(BlockId),
    Break(StmtId),
    /// A resolved assignment target; same-scope assignments carry `false`.
    Assignment(StmtId),
    Yield(StmtId),
}

pub type ResourceTransferFacts = FxHashMap<ResourceTransferSite, bool>;

pub(super) struct ResourceCheck {
    pub diagnostics: Vec<Diagnostic>,
    pub transfers: ResourceTransferFacts,
}

pub(super) fn check(checker: &Checker, view: &CheckBundleView<'_>) -> ResourceCheck {
    Analyzer {
        checker,
        program: view.program,
        root_statements: view.root_statements,
        modules: view.modules.clone(),
        blocks: FxHashSet::default(),
        diagnostics: Vec::new(),
        transfers: FxHashMap::default(),
        diagnosed: FxHashSet::default(),
    }
    .run()
}

struct Analyzer<'a> {
    checker: &'a Checker,
    program: &'a ArenaProgram,
    root_statements: ArenaRange,
    modules: Vec<&'a ArenaUserModule>,
    blocks: FxHashSet<BlockId>,
    diagnostics: Vec<Diagnostic>,
    transfers: ResourceTransferFacts,
    diagnosed: FxHashSet<(BindingId, Span)>,
}

impl Analyzer<'_> {
    fn run(mut self) -> ResourceCheck {
        let root = self.program.arena.stmt_ids(self.root_statements).collect::<Vec<_>>();
        self.analyze_scope(&root);
        let modules = self.modules.iter().map(|module| {
            self.program.module_statements(module).collect::<Vec<_>>()
        }).collect::<Vec<_>>();
        for statements in &modules {
            self.analyze_scope(statements);
        }
        let mut pending = root
            .into_iter()
            .chain(modules.into_iter().flatten())
            .map(ArenaChild::Stmt)
            .collect::<Vec<_>>();

        while let Some(child) = pending.pop() {
            match child {
                ArenaChild::Block(block) => {
                    if !self.blocks.insert(block) {
                        continue;
                    }
                    self.analyze_block(block);
                    self.program
                        .arena
                        .for_each_block_child(block, |child| pending.push(child));
                }
                ArenaChild::Stmt(stmt) => self.program.arena.for_each_stmt_child(
                    stmt,
                    SugarView::Core,
                    |child| pending.push(child),
                ),
                ArenaChild::Expr(expr) => self
                    .program
                    .arena
                    .for_each_expr_child(expr, |child| pending.push(child)),
                ArenaChild::Pattern(pattern) => self
                    .program
                    .arena
                    .for_each_pattern_child(pattern, |child| pending.push(child)),
                ArenaChild::AssignTarget(target) => self
                    .program
                    .arena
                    .for_each_assign_target_child(target, |child| pending.push(child)),
                ArenaChild::BindingTarget(target) => self
                    .program
                    .arena
                    .for_each_binding_target_child(target, |child| pending.push(child)),
                ArenaChild::BuilderBlock(block) => self
                    .program
                    .arena
                    .for_each_builder_child(block, |child| pending.push(child)),
                ArenaChild::TypeExpr(_) => {}
            }
        }

        ResourceCheck {
            diagnostics: self.diagnostics,
            transfers: self.transfers,
        }
    }

    fn analyze_block(&mut self, block: BlockId) {
        let statements = self.program.arena.stmt_ids(self.program.arena.block(block).statements)
            .collect::<Vec<_>>();
        self.analyze_scope(&statements);
        self.publish_block_transfer(block, &statements);
    }

    fn analyze_scope(&mut self, statements: &[StmtId]) {
        let declaration_spans = statements
            .iter()
            .filter_map(|&stmt| match self.program.arena.stmt(stmt).kind {
                ArenaStmtKind::Let { .. } | ArenaStmtKind::Var { .. } => {
                    Some(self.program.arena.stmt(stmt).span)
                }
                _ => None,
            })
            .collect::<FxHashSet<_>>();
        let declared_bindings = self
            .checker
            .resolved_bindings
            .declaration_sites
            .iter()
            .filter_map(|(&(span, _), &binding)| {
                declaration_spans.contains(&span).then_some(binding)
            })
            .collect::<FxHashSet<_>>();
        let locals = statements
            .iter()
            .filter_map(|&stmt| self.local_resource(stmt))
            .collect::<FxHashMap<_, _>>();
        let mut released = FxHashMap::<BindingId, Span>::default();

        for &stmt in statements {
            let statement = self.program.arena.stmt(stmt);
            self.publish_statement_transfer(stmt, &statement.kind, &declared_bindings);
            let releases = self.statement_releases(stmt, &locals);
            let mut exempt = self.process_cancel_sites(statement.span, &locals);
            if let ArenaStmtKind::Assign { target, op: AssignOp::Set, .. } = statement.kind
                && let Some((name, true)) = self.simple_assignment_target(target)
                && let Some(binding) = self.checker.resolved_bindings.at(statement.span, name)
                && locals.contains_key(&binding)
            {
                exempt.insert((binding, statement.span));
            }
            self.report_released_mentions(statement.span, &locals, &released, &exempt);

            for (binding, _) in releases {
                released.entry(binding).or_insert(statement.span);
            }

            if let ArenaStmtKind::Assign { target, op: AssignOp::Set, .. } = statement.kind
                && let Some((name, true)) = self.simple_assignment_target(target)
                && let Some(binding) = self.checker.resolved_bindings.at(statement.span, name)
                && locals.contains_key(&binding)
            {
                released.remove(&binding);
            }
        }

    }

    fn local_resource(&self, stmt: StmtId) -> Option<(BindingId, ManagedResource)> {
        let node = self.program.arena.stmt(stmt);
        let (target, initializer) = match node.kind {
            ArenaStmtKind::Let { target, initializer, .. }
            | ArenaStmtKind::Var { target, initializer, .. } => (target, initializer),
            _ => return None,
        };
        if let ArenaExprOrRun::Expr(expr) = initializer {
            let expr = self.unwrap_try(expr);
            if matches!(
                self.program.arena.expr(expr).kind,
                ArenaExprKind::Ident(_)
                    | ArenaExprKind::Field { .. }
                    | ArenaExprKind::NullSafeField { .. }
                    | ArenaExprKind::Index { .. }
                    | ArenaExprKind::Slice { .. }
                    | ArenaExprKind::Binary { .. }
                    | ArenaExprKind::If { .. }
                    | ArenaExprKind::Match { .. }
                    | ArenaExprKind::ValuePipelineCall { .. }
                    | ArenaExprKind::Pipeline { .. }
                    | ArenaExprKind::StructuredPipeline { .. }
                    | ArenaExprKind::Require { .. }
                    | ArenaExprKind::Convert { .. }
            ) {
                return None;
            }
        }
        let ArenaBindingTargetKind::Name(name) = self.program.arena.binding_target(target).kind else {
            return None;
        };
        let ty = self.checker.local_inference.checked_bindings.get(&node.span)?;
        let resource = match ty {
            Type::ProcessHandle => ManagedResource::ProcessHandle,
            Type::NetJob => ManagedResource::NetJob,
            Type::FsRoot => ManagedResource::FsRoot,
            Type::FsLock => ManagedResource::FsLock,
            _ => return None,
        };
        if resource == ManagedResource::Stream {
            return None;
        }
        let binding = self.checker.resolved_bindings.declaration_sites.get(&(node.span, name)).copied()?;
        Some((binding, resource))
    }

    fn publish_statement_transfer(
        &mut self,
        stmt: StmtId,
        kind: &ArenaStmtKind,
        declared_bindings: &FxHashSet<BindingId>,
    ) {
        match *kind {
            ArenaStmtKind::Return(value) => {
                if let Some(value) = value {
                    if let Some(holds) = self.expr_or_run_may_hold(value) {
                        self.transfers.insert(ResourceTransferSite::Return(stmt), holds);
                    }
                } else {
                    self.transfers.insert(ResourceTransferSite::Return(stmt), false);
                }
            }
            ArenaStmtKind::Break { value: Some(expr) } => {
                if let Some(holds) = self.expr_may_hold(expr) {
                    self.transfers.insert(ResourceTransferSite::Break(stmt), holds);
                }
            }
            ArenaStmtKind::Break { value: None } => {
                self.transfers.insert(ResourceTransferSite::Break(stmt), false);
            }
            ArenaStmtKind::Yield(value) => {
                if let Some(holds) = self.expr_or_run_may_hold(value) {
                    self.transfers.insert(ResourceTransferSite::Yield(stmt), holds);
                }
            }
            ArenaStmtKind::YieldDelegate(expr) => {
                if let Some(holds) = self.expr_may_hold(expr) {
                    self.transfers.insert(ResourceTransferSite::Yield(stmt), holds);
                }
            }
            ArenaStmtKind::Assign { target, value, .. } => {
                if let Some((name, _)) = self.assignment_root(target)
                    && let Some(binding) = self.checker.resolved_bindings.at(
                        self.program.arena.stmt(stmt).span,
                        name,
                    )
                {
                    let holds = if declared_bindings.contains(&binding) {
                        Some(false)
                    } else {
                        self.expr_or_run_may_hold(value)
                    };
                    if let Some(holds) = holds {
                        self.transfers.insert(ResourceTransferSite::Assignment(stmt), holds);
                    }
                }
            }
            _ => {}
        }
    }

    fn publish_block_transfer(&mut self, block: BlockId, statements: &[StmtId]) {
        let holds = match statements.last() {
            None => Some(false),
            Some(stmt) => {
                let node = self.program.arena.stmt(*stmt);
                match node.kind {
                    ArenaStmtKind::Expr(expr)
                        if self.checker.statement_positions.get(&node.span).is_some_and(|position|
                            *position == super::StatementPosition::Value
                        ) => self.expr_may_hold(expr),
                    ArenaStmtKind::TailBareIdent(_)
                        if self.checker.statement_positions.get(&node.span).is_some_and(|position|
                            *position == super::StatementPosition::Value
                        ) => self.checker.expr_types.get(&node.span).map(Type::may_hold_resource),
                    _ => Some(false),
                }
            }
        };
        if let Some(holds) = holds {
            self.transfers.insert(ResourceTransferSite::BlockValue(block), holds);
        }
    }

    fn expr_or_run_may_hold(&self, value: ArenaExprOrRun) -> Option<bool> {
        let span = super::expr::expr_or_run_span_arena(self.program, value);
        self.checker.expr_types.get(&span).map(Type::may_hold_resource)
    }

    fn expr_may_hold(&self, expr: ExprId) -> Option<bool> {
        let span = self.program.arena.expr(expr).span;
        self.checker.expr_types.get(&span).map(Type::may_hold_resource)
    }

    fn report_released_mentions(
        &mut self,
        statement_span: Span,
        locals: &FxHashMap<BindingId, ManagedResource>,
        released: &FxHashMap<BindingId, Span>,
        exempt: &FxHashSet<(BindingId, Span)>,
    ) {
        for (&(span, name), &binding) in &self.checker.resolved_bindings.value_sites {
            if !locals.contains_key(&binding) || !span_in(statement_span, span) {
                continue;
            }
            let Some(released_at) = released.get(&binding).copied() else { continue };
            if exempt.contains(&(binding, span)) || !self.diagnosed.insert((binding, span)) {
                continue;
            }
            self.diagnostics.push(
                Diagnostic::error(format!("`{name}` is used after it was released"))
                    .with_code(DiagnosticCode::CheckUseAfterRelease)
                    .with_label(Label::primary(span, "released local used here"))
                    .with_label(Label::secondary(released_at, "resource released here")),
            );
        }
    }

    fn statement_releases(
        &self,
        stmt: StmtId,
        locals: &FxHashMap<BindingId, ManagedResource>,
    ) -> Vec<(BindingId, Span)> {
        let statement = self.program.arena.stmt(stmt);
        let mut pending = match statement.kind {
            ArenaStmtKind::Expr(expr)
            | ArenaStmtKind::YieldDelegate(expr)
            | ArenaStmtKind::Break { value: Some(expr) }
            | ArenaStmtKind::Exit(expr) => vec![expr],
            ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(expr), .. }
            | ArenaStmtKind::Var { initializer: ArenaExprOrRun::Expr(expr), .. }
            | ArenaStmtKind::Assign { value: ArenaExprOrRun::Expr(expr), .. }
            | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr)))
            | ArenaStmtKind::Yield(ArenaExprOrRun::Expr(expr)) => vec![expr],
            ArenaStmtKind::Assert { condition, .. } => vec![condition],
            _ => return Vec::new(),
        };
        let mut releases = Vec::new();
        let mut seen = FxHashSet::default();
        while let Some(expr) = pending.pop() {
            let node = self.program.arena.expr(expr);
            let lazy = match node.kind {
                ArenaExprKind::Binary { op, .. } => matches!(
                    op,
                    crate::syntax::node::BinaryOp::ResultFallback
                        | crate::syntax::node::BinaryOp::Or
                        | crate::syntax::node::BinaryOp::And
                ),
                ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::ValuePipelineCall { .. }
                | ArenaExprKind::Pipeline { .. }
                | ArenaExprKind::StructuredPipeline { .. }
                | ArenaExprKind::ContextScope { .. }
                | ArenaExprKind::Loop { .. } => true,
                _ => false,
            };
            if let Some(release) = self.expression_release(expr, locals)
                && seen.insert(release)
            {
                releases.push(release);
            }
            if lazy {
                continue;
            }
            self.program.arena.for_each_expr_child(expr, |child| {
                if let ArenaChild::Expr(child) = child {
                    pending.push(child);
                }
            });
        }
        releases
    }

    fn expression_release(
        &self,
        expr: ExprId,
        locals: &FxHashMap<BindingId, ManagedResource>,
    ) -> Option<(BindingId, Span)> {
        let expr = self.unwrap_try(expr);
        match self.program.arena.expr(expr).kind {
            ArenaExprKind::Wait(form) => {
                let ArenaExprKind::Ident(_) = self.program.arena.expr(form.target).kind else {
                    return None;
                };
                let span = self.program.arena.expr(form.target).span;
                let id = self.checker.resolved_bindings.expressions.get(&form.target).copied()?;
                let kind = locals.get(&id)?;
                kind.rule().consuming.contains(&ResourceConsumption::Wait).then_some((id, span))
            }
            ArenaExprKind::Call { callee, args } => {
                let api = self.checker.api_calls.get(&self.program.arena.expr(expr).span)?;
                if api.receiver.is_some()
                    && let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind
                    && let ArenaExprKind::Ident(_) = self.program.arena.expr(base).kind
                {
                    let id = self.checker.resolved_bindings.expressions.get(&base).copied()?;
                    let kind = locals.get(&id)?;
                    let op = ResourceConsumption::Operation(api.sig.op);
                    if kind.rule().consuming.contains(&op) {
                        Some((id, self.program.arena.expr(base).span))
                    } else {
                        None
                    }
                } else if api.receiver.is_none() {
                    let mut consumed = None;
                    for argument in self.program.arena.call_args(args) {
                        let (ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. }) = &argument.kind else {
                            continue;
                        };
                        if !matches!(self.program.arena.expr(*expr).kind, ArenaExprKind::Ident(_)) {
                            continue;
                        }
                        let Some(id) = self.checker.resolved_bindings.expressions.get(expr).copied() else { continue };
                        let Some(kind) = locals.get(&id) else { continue };
                        if kind.rule().consuming.contains(&ResourceConsumption::Operation(api.sig.op)) {
                            consumed = Some((id, self.program.arena.expr(*expr).span));
                            break;
                        }
                    }
                    consumed
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn process_cancel_sites(
        &self,
        statement: Span,
        locals: &FxHashMap<BindingId, ManagedResource>,
    ) -> FxHashSet<(BindingId, Span)> {
        let mut sites = FxHashSet::default();
        for index in 0..self.program.arena.expr_tags.len() {
            let call = ExprId::from_index(index);
            let node = self.program.arena.expr(call);
            if !span_in(statement, node.span)
                || !matches!(node.kind, ArenaExprKind::Call { .. })
                || !self.checker.api_calls.get(&node.span).is_some_and(|api|
                    api.sig.op == crate::modules::RuntimeOp::ProcessHandleCancel
                )
            {
                continue;
            }
            let ArenaExprKind::Call { callee, .. } = node.kind else { continue };
            let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind else { continue };
            if !matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(_)) {
                continue;
            }
            if let Some(binding) = self.checker.resolved_bindings.expressions.get(&base).copied()
                && locals.get(&binding) == Some(&ManagedResource::ProcessHandle)
            {
                sites.insert((binding, self.program.arena.expr(base).span));
            }
        }
        sites
    }

    fn unwrap_try(&self, mut expr: ExprId) -> ExprId {
        while let ArenaExprKind::Try(inner) = self.program.arena.expr(expr).kind {
            expr = inner;
        }
        expr
    }

    fn simple_assignment_target(&self, target: AssignTargetId) -> Option<(Name, bool)> {
        match self.program.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(name) => Some((name, true)),
            _ => None,
        }
    }

    fn assignment_root(&self, target: AssignTargetId) -> Option<(Name, bool)> {
        let mut target = target;
        loop {
            match self.program.arena.assign_target(target).kind {
                ArenaAssignTargetKind::Name(name) => return Some((name, true)),
                ArenaAssignTargetKind::Field { base, .. }
                | ArenaAssignTargetKind::Index { base, .. } => target = base,
                ArenaAssignTargetKind::Env(_) => return None,
            }
        }
    }
}

fn span_in(container: Span, span: Span) -> bool {
    container.source_id == span.source_id
        && container.start() <= span.start()
        && span.end() <= container.end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn return_stmt(program: &ArenaProgram, name: &str) -> StmtId {
        let function = program
            .statement_ids()
            .find_map(|stmt| match program.arena.stmt(stmt).kind {
                ArenaStmtKind::ProcDef(function)
                    if program.arena.function_def(function).name.as_str() == name => Some(function),
                _ => None,
            })
            .expect("test function exists");
        let body = program.arena.function_def(function).body;
        program
            .arena
            .stmt_ids(program.arena.block(body).statements)
            .find(|&stmt| matches!(program.arena.stmt(stmt).kind, ArenaStmtKind::Return(_)))
            .expect("test function returns")
    }

    fn function_body(program: &ArenaProgram, name: &str) -> BlockId {
        program
            .statement_ids()
            .find_map(|stmt| match program.arena.stmt(stmt).kind {
                ArenaStmtKind::ProcDef(function)
                    if program.arena.function_def(function).name.as_str() == name =>
                {
                    Some(program.arena.function_def(function).body)
                }
                _ => None,
            })
            .expect("test function exists")
    }

    fn assignment_stmt(program: &ArenaProgram, name: &str) -> StmtId {
        (0..program.arena.stmt_tags.len())
            .map(StmtId::from_index)
            .find(|&stmt| match program.arena.stmt(stmt).kind {
                ArenaStmtKind::Assign { target, .. } => {
                    matches!(program.arena.assign_target(target).kind,
                        ArenaAssignTargetKind::Name(actual) if actual.as_str() == name)
                }
                _ => false,
            })
            .expect("test assignment exists")
    }

    #[test]
    fn transfer_facts_retain_resource_and_scalar_results_through_compaction() {
        let source = "proc transfer() [fs, error] -> Result[FsRoot] { let root = fs.tempdir()?; return root }\nproc tail() [fs, error] -> Result[FsRoot] { let root = fs.tempdir()?; root }\nproc scalar() -> Int { return 0 }\n";
        let program = Parser::parse_source_arena_only(SourceId::new(0), source).arena;
        let checked = Checker::check_arena_with_options(
            &program,
            source,
            super::super::CheckOptions {
                embedded_bodies: true,
                ..super::super::CheckOptions::default()
            },
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

        let resource_return = ResourceTransferSite::Return(return_stmt(&program, "transfer"));
        let bare_tail = ResourceTransferSite::BlockValue(function_body(&program, "tail"));
        let scalar_return = ResourceTransferSite::Return(return_stmt(&program, "scalar"));
        assert_eq!(checked.resource_transfer(resource_return), Some(true));
        assert_eq!(checked.resource_transfer(bare_tail), Some(true));
        assert_eq!(checked.resource_transfer(scalar_return), Some(false));

        let compact = Checker::compact_declarations(&program, checked);
        assert_eq!(compact.bodies.resource_transfer(resource_return), Some(true));
        assert_eq!(compact.bodies.resource_transfer(bare_tail), Some(true));
        assert_eq!(compact.bodies.resource_transfer(scalar_return), Some(false));
    }

    #[test]
    fn transfer_facts_distinguish_outer_and_same_block_assignments() {
        let source = "proc transfers() [fs, error] -> Result[Unit] { var local: FsRoot? = null; local = fs.tempdir()?; var outer: FsRoot? = null; { outer = fs.tempdir()? }; return Ok() }\n";
        let program = Parser::parse_source_arena_only(SourceId::new(0), source).arena;
        let checked = Checker::check_arena_with_options(
            &program,
            source,
            super::super::CheckOptions {
                embedded_bodies: true,
                ..super::super::CheckOptions::default()
            },
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

        let local = ResourceTransferSite::Assignment(assignment_stmt(&program, "local"));
        let outer = ResourceTransferSite::Assignment(assignment_stmt(&program, "outer"));
        assert_eq!(checked.resource_transfer(local), Some(false));
        assert_eq!(checked.resource_transfer(outer), Some(true));

        let compact = Checker::compact_declarations(&program, checked);
        assert_eq!(compact.bodies.resource_transfer(local), Some(false));
        assert_eq!(compact.bodies.resource_transfer(outer), Some(true));
    }
}
