use super::*;
use crate::sema::check::{ExpressionIdentity, RecordUpdateValueSource, SolvedRecordUpdate};
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalRecordUpdateValue {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub row: BuildExprId,
    pub material: BuildExprId,
}

/// The receiver and every replacement retain their own authored source. A
/// record update preserves the complete receiver row rather than constructing
/// a new row from the selected fields.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalRecordUpdate {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub contract: SolvedRecordUpdate,
    pub base: OriginalRecordUpdateValue,
    pub replacements: Box<[OriginalRecordUpdateValue]>,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_record_update(&mut self, id: ExprId, lowered: BuildExprId) -> Option<()> {
        let origin = self.expression_identity(id);
        let Some(contract) = self.solved().record_updates.get(&origin).cloned() else { return Some(()); };
        let solved = self.solved();
        let &ty = solved.expressions.get(&origin)?;
        let checked = ScopedRoot { ty, scope: solved.expression_scope(origin, contract.caller).ok()? };
        solved.graph.validate_scoped(checked).ok()?;
        if !matches!(super::super::indexed::generic::graph_ground_type(&solved.graph, ty), Ok(Type::Record(_))) { return Some(()); }
        let material = self.original_source_instruction(lowered)?;
        let (base, updates) = {
            let scratch = self.scratch.borrow();
            let Some(BuildExprRow::RecordUpdate { base, updates, .. }) = scratch.expressions.get(material.index()) else { return None; };
            (*base, updates.0.clone())
        };
        let ArenaExprKind::Record(fields) = self.program.arena.expr(id).kind else { return None; };
        let fields = self.program.arena.record_fields(fields).to_vec();
        let ArenaRecordFieldKind::Spread { expr: authored_base, .. } = fields.first()?.kind else { return None; };
        if self.expression_identity(authored_base) != contract.base || updates.len() != contract.replacements.len() || fields.len() != updates.len() + 1 { return None; }
        let base = self.original_record_update_value(contract.base, base, contract.caller)?;
        let mut replacements = Vec::with_capacity(updates.len());
        for ((field, (path, row, _)), replacement) in fields.iter().skip(1).zip(updates).zip(&contract.replacements) {
            let (authored_path, value) = match field.kind {
                ArenaRecordFieldKind::Path { path, value, .. } => (self.program.arena.names(path).collect::<Vec<_>>(), value),
                ArenaRecordFieldKind::Named { name, value, .. } => (vec![name], value),
                ArenaRecordFieldKind::Shorthand { .. } => return Some(()),
                _ => return None,
            };
            let RecordUpdateValueSource::Expression(source) = replacement.source else { return Some(()); };
            if authored_path != path || path != replacement.path || self.expression_identity(value) != source { return None; }
            if super::super::indexed::generic::graph_ground_type(&self.solved().graph, replacement.value).is_err() { return Some(()); }
            replacements.push(self.original_record_update_value(source, row, contract.caller)?);
        }
        self.scratch.borrow_mut().record_update_sources.insert(material, OriginalRecordUpdate {
            origin, checked, contract, base, replacements: replacements.into_boxed_slice(),
        });
        Some(())
    }

    fn original_record_update_value(&self, origin: ExpressionIdentity, row: BuildExprId, owner: Option<crate::sema::check::DeclarationIdentity>) -> Option<OriginalRecordUpdateValue> {
        let solved = self.solved();
        let &ty = solved.expressions.get(&origin)?;
        let checked = ScopedRoot { ty, scope: solved.expression_scope(origin, owner).ok()? };
        solved.graph.validate_scoped(checked).ok()?;
        let material = self.original_source_instruction(row)?;
        if self.expression_origins.get(&material) != Some(&origin) { return None; }
        Some(OriginalRecordUpdateValue { origin, checked, row, material })
    }
}
