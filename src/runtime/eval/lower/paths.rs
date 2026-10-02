use super::*;
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum FormattedTarget { Path, Str }

impl FormattedTarget {
    pub(in crate::runtime::eval) fn result_type(self) -> Type {
        match self { Self::Path => Type::Path, Self::Str => Type::Str }
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) enum OriginalPathPart {
    Text(Arc<str>),
    Expression { origin: ExpressionIdentity, checked: ScopedRoot, value: BuildExprId, source: BuildExprId, format: Option<crate::syntax::node::FormatSpec> },
}

/// The authored target fixes the checked formatting result. Each interpolation
/// retains its original source and scope independently of that result.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalFormattedPath {
    pub target: FormattedTarget,
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub caller: Option<DeclarationIdentity>,
    pub parts: Box<[OriginalPathPart]>,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_formatted_path(&self, id: ExprId, lowered: BuildExprId) -> Option<()> {
        let (target, parts) = match self.program.arena.expr(id).kind {
            ArenaExprKind::PathFmtString(parts) => (FormattedTarget::Path, parts),
            ArenaExprKind::FmtString(parts) => (FormattedTarget::Str, parts),
            _ => return Some(()),
        };
        let original_parts = self.program.arena.fmt_parts(parts).collect::<Vec<_>>();
        let emitted = self.scratch.borrow().expressions.get(lowered.index())?.clone();
        let emitted_parts = match (target, emitted) {
            (FormattedTarget::Path, BuildExprRow::PathFmtString { parts, .. })
                | (FormattedTarget::Str, BuildExprRow::FmtString(parts)) => parts,
            _ => return None,
        };
        if original_parts.len() != emitted_parts.len() { return None; }
        let origin = self.expression_identity(id);
        let solved = self.solved();
        let caller = solved.expression_owners.get(&origin).copied();
        let checked = ScopedRoot { ty: *solved.expressions.get(&origin)?, scope: solved.expression_scope(origin, caller).ok()? };
        solved.graph.validate_scoped(checked).ok()?;
        if super::super::indexed::generic::graph_ground_type(&solved.graph, checked.ty).ok()? != target.result_type() { return None; }
        let mut original = Vec::with_capacity(original_parts.len());
        for (part, emitted) in original_parts.into_iter().zip(emitted_parts) {
            match (part, emitted) {
                (ArenaFmtPart::Text(text), LoweredFmtPart::Text(emitted)) if self.text_value(&text)? == emitted.as_ref() => original.push(OriginalPathPart::Text(emitted)),
                (ArenaFmtPart::Expr(expression, format), LoweredFmtPart::Expr(value, _, actual_format)) if format == actual_format => {
                    let origin = self.expression_identity(expression);
                    if solved.expression_owners.get(&origin).copied() != caller { return None; }
                    let checked = ScopedRoot { ty: *solved.expressions.get(&origin)?, scope: solved.expression_scope(origin, caller).ok()? };
                    solved.graph.validate_scoped(checked).ok()?;
                    let source = self.original_source_instruction(value)?;
                    if self.expression_origins.get(&source) != Some(&origin) { return None; }
                    original.push(OriginalPathPart::Expression { origin, checked, value, source, format });
                }
                _ => return None,
            }
        }
        self.scratch.borrow_mut().formatted_paths.insert(lowered, OriginalFormattedPath { target, origin, checked, caller, parts: original.into_boxed_slice() });
        Some(())
    }
}
