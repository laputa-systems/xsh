use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn lower_retry_capture(&mut self, id: ExprId, delays: crate::syntax::arena::ArenaRange, pattern: Option<PatternId>, block: BlockId, slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>, span: Span) -> Option<BuildExprId> {
        use crate::runtime::eval::indexed::full::{BuildRetryCapturePolicy, BuildTryCaptureOrigin};
        let origin = self.expression_identity(id);
        let caller = self.solved().expression_owners.get(&origin).copied();
        let root = |lowerer: &Self, source| {
            let ty = *lowerer.solved().expressions.get(&source)?;
            Some(crate::sema::inference::ScopedRoot { ty, scope: lowerer.solved().expression_scope(source, caller).ok()? })
        };
        let mut original_delays = Vec::new();
        for delay in self.program.arena.expr_ids(delays).collect::<Vec<_>>() {
            let source = self.expression_identity(delay);
            let ty = root(self, source)?;
            let row = self.lower_expr(delay, slots, current_function, item_slot)?;
            original_delays.push((row, source, ty));
        }
        let selection = if let Some(pattern) = pattern {
            let source = self.original_pattern_identity(pattern);
            let checked = self.solved().checked_pattern(source).ok()?;
            let ty = crate::sema::inference::ScopedRoot { ty: checked.input, scope: self.solved().checked_pattern_scope(source).ok()? };
            let row = self.lower_pattern(pattern, slots, None, None)?.0;
            Some((row, source, ty))
        } else { None };
        let body = self.lower_retry_block(block, slots, current_function, item_slot)?;
        let row = push_build_row!(self, expr, BuildExprRow::Retry { delays: original_delays.iter().map(|delay| delay.0).collect(), pattern: selection.map(|selection| selection.0), body: body.clone(), span });
        let tail = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last().and_then(|statement| {
            let ArenaStmtKind::Expr(expression) = self.program.arena.stmt(statement).kind else { return None; };
            Some(expression)
        });
        let completion = tail.and_then(|expression| { let source = self.expression_identity(expression); Some((source, root(self, source)?)) });
        let propagation = tail.and_then(|expression| {
            let ArenaExprKind::Try(producer) = self.program.arena.expr(expression).kind else { return None; };
            let source = self.expression_identity(producer); Some((source, root(self, source)?))
        });
        let propagation_row = if propagation.is_some() {
            let value = { let scratch = self.scratch.borrow(); body.last().and_then(|statement| match scratch.statements.get(statement.index()) { Some(BuildStmtRow::Value { value }) => Some(*value), _ => None }) }?;
            let value = self.original_source_instruction(value)?;
            let producer = { let scratch = self.scratch.borrow(); let BuildExprRow::Try(producer) = scratch.expressions.get(value.index())? else { return None; }; *producer };
            Some(self.original_source_instruction(producer)?)
        } else { None };
        let source_type = root(self, origin)?;
        self.scratch.borrow_mut().try_capture_origins.insert(row, BuildTryCaptureOrigin { origin, block, source_type, body: body.into_boxed_slice(), completion, propagation, propagation_row,
            retry: Some(BuildRetryCapturePolicy { delays: original_delays, selection }) });
        Some(row)
    }
}
