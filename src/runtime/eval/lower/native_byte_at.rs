use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_byte_at_fallback(&self, parent: BuildExprId, call: BuildExprId, receiver: BuildExprId, index: BuildExprId, fallback: BuildExprId, candidate: BuildIntId) -> Option<()> {
        let parent_origin = *self.expression_origins.get(&parent)?;
        let call_origin = *self.expression_origins.get(&call)?;
        let receiver_origin = *self.expression_origins.get(&receiver)?;
        let index_origin = *self.expression_origins.get(&index)?;
        let fallback_origin = *self.expression_origins.get(&fallback)?;
        let ArenaExprKind::Binary { op: BinaryOp::ResultFallback, left, right } = self.program.arena.expr(parent_origin.expression).kind else { return None; };
        if left != call_origin.expression || right != fallback_origin.expression { return None; }
        let ArenaExprKind::Ident(name) = self.program.arena.expr(receiver_origin.expression).kind else { return None; };
        let scratch = self.scratch.borrow();
        let BuildExprRow::Param(slot) = scratch.expressions.get(receiver.index())? else { return None; };
        let binding = scratch.value_binding_uses.get(&receiver_origin).copied();
        let receiver = super::super::indexed::full::BuildFoldedNativeReceiver { origin: receiver_origin, name, slot: u32::try_from(*slot).ok()?, binding };
        drop(scratch);
        let fallback_value = self.lowered_inert_int_literal(fallback)?;
        let original = super::super::indexed::full::BuildByteAtFallbackOriginal { call: call_origin, receiver, index_origin, fallback_origin, fallback_value };
        if self.scratch.borrow_mut().byte_at_fallback_origins.insert(candidate, original).is_some() { return None; }
        Some(())
    }
}
