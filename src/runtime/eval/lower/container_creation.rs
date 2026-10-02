use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_container_creation_check(&self, expression: ExprId, container: BuildExprId, wrapper: BuildExprId) -> Option<()> {
        let scratch = self.scratch.borrow();
        if !matches!(scratch.expressions.get(container.index()), Some(BuildExprRow::List(_) | BuildExprRow::MapLiteral(_))) { return Some(()); }
        let BuildExprRow::CheckedValue { value, check, .. } = scratch.expressions.get(wrapper.index())? else { return None; };
        let origin = self.expression_identity(expression);
        let caller = self.solved().expression_owners.get(&origin).copied();
        let checked = crate::sema::inference::ScopedRoot {
            ty: *self.solved().expressions.get(&origin)?,
            scope: self.solved().expression_scope(origin, caller).ok()?,
        };
        self.solved().graph.validate_scoped(checked).ok()?;
        let Ok(ty) = self.solved().graph.export_type(checked.ty) else { return Some(()); };
        if !matches!(ty, Type::List(_) | Type::Map(_, _)) || !ty.has_unsigned_constraint() { return Some(()); }
        if *value != container || check.ty != ty { return None; }
        drop(scratch);
        let original = super::super::indexed::full::BuildContainerCreationCheck { origin, checked, container };
        if self.scratch.borrow_mut().container_creation_checks.insert(wrapper, original).is_some() { return None; }
        Some(())
    }
}
