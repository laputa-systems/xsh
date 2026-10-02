use super::*;
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum OriginalRecordEntryKind { Field(Name), Spread }

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalRecordEntry {
    pub kind: OriginalRecordEntryKind,
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub row: BuildExprId,
    pub material: BuildExprId,
}

/// A spread record retains authored entries independently of the flattened row.
/// This source proof does not authorize a numeric constructor layout.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalRecordSource {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub entries: Box<[OriginalRecordEntry]>,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_record_source(&mut self, id: ExprId, lowered: BuildExprId) -> Option<()> {
        let ArenaExprKind::Record(fields) = self.program.arena.expr(id).kind else { return Some(()); };
        let fields = self.program.arena.record_fields(fields).to_vec();
        if !fields.iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Spread { .. })) { return Some(()); }
        let origin = self.expression_identity(id);
        let solved = self.solved();
        let Some(&ty) = solved.expressions.get(&origin) else { return Some(()); };
        let owner = solved.expression_owners.get(&origin).copied();
        let checked = ScopedRoot { ty, scope: solved.expression_scope(origin, owner).ok()? };
        solved.graph.validate_scoped(checked).ok()?;
        if !matches!(super::super::indexed::generic::graph_ground_type(&solved.graph, ty), Ok(Type::Record(_))) { return Some(()); }
        let material = self.original_source_instruction(lowered)?;
        let entries = {
            let scratch = self.scratch.borrow();
            let Some(BuildExprRow::Record(entries)) = scratch.expressions.get(material.index()) else { return Some(()); };
            entries.clone()
        };
        if fields.len() != entries.len() { return None; }
        let mut originals = Vec::with_capacity(entries.len());
        for (field, entry) in fields.iter().zip(entries) {
            let (kind, source, row) = match (&field.kind, entry) {
                (ArenaRecordFieldKind::Named { name, value, .. }, LoweredRecordEntry::Field(actual, row)) if *name == actual =>
                    (OriginalRecordEntryKind::Field(*name), *value, row),
                (ArenaRecordFieldKind::Spread { expr, .. }, LoweredRecordEntry::Spread(row)) =>
                    (OriginalRecordEntryKind::Spread, *expr, row),
                _ => return None,
            };
            let source = self.expression_identity(source);
            let solved = self.solved();
            let &ty = solved.expressions.get(&source)?;
            let child = ScopedRoot { ty, scope: solved.expression_scope(source, owner).ok()? };
            solved.graph.validate_scoped(child).ok()?;
            if super::super::indexed::generic::graph_ground_type(&solved.graph, ty).is_err() { return Some(()); }
            let child_material = self.original_source_instruction(row)?;
            if self.expression_origins.get(&child_material) != Some(&source) { return None; }
            originals.push(OriginalRecordEntry { kind, origin: source, checked: child, row, material: child_material });
        }
        self.scratch.borrow_mut().record_sources.insert(material, OriginalRecordSource { origin, checked, entries: originals.into_boxed_slice() });
        Some(())
    }
}
