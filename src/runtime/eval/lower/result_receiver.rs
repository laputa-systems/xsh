use super::*;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildResultReceiver {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub source_type: crate::sema::inference::ScopedRoot,
    pub success_type: crate::sema::inference::ScopedRoot,
    pub error_type: crate::sema::inference::ScopedRoot,
    pub carrier: BuildExprId,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_result_receiver(
        &self, base: ExprId, carrier: BuildExprId, generated: BuildExprId,
    ) -> Option<()> {
        let origin = self.expression_identity(base);
        let solved = self.solved();
        let owner = solved.expression_owners.get(&origin).copied();
        let source_type = crate::sema::inference::ScopedRoot {
            ty: *solved.expressions.get(&origin)?, scope: solved.expression_scope(origin, owner).ok()?,
        };
        let graph = &solved.graph;
        graph.validate_scoped(source_type).ok()?;
        let crate::sema::inference::TypeNode::Result(success, error) = graph.node(graph.resolved(source_type.ty).ok()?).ok()? else { return Some(()); };
        let success_type = crate::sema::inference::ScopedRoot { ty: *success, scope: source_type.scope };
        let error_type = crate::sema::inference::ScopedRoot { ty: *error, scope: source_type.scope };
        graph.validate_scoped(success_type).ok()?;
        graph.validate_scoped(error_type).ok()?;
        let mut scratch = self.scratch.borrow_mut();
        if carrier.index() >= generated.index()
            || !matches!(scratch.expressions.get(generated.index())?, BuildExprRow::Try(actual) if *actual == carrier) { return None; }
        if scratch.result_receiver_origins.insert(generated, BuildResultReceiver {
            origin, source_type, success_type, error_type, carrier,
        }).is_some() { return None; }
        Some(())
    }
}
