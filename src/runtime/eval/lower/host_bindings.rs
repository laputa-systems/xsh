use super::*;
use crate::sema::inference::ScopedRoot;

/// A host binding is created by entry hydration before authored statements run.
/// Its identity does not come from an authored declaration or a matching name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum HostBinding {
    Args,
}

impl HostBinding {
    pub(in crate::runtime::eval) fn name(self) -> Name {
        match self { Self::Args => Name::intern("args") }
    }

    pub(in crate::runtime::eval) fn ty(self) -> Type {
        match self { Self::Args => Type::List(Box::new(Type::Str)) }
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildHostBindingRead {
    pub binding: HostBinding,
    pub slot: usize,
    pub origin: ExpressionIdentity,
    pub expression: BuildExprId,
    pub source_type: ScopedRoot,
    pub caller: Option<crate::sema::check::DeclarationIdentity>,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_host_binding_read(&self, id: ExprId, lowered: BuildExprId, slots: &SlotScope) -> Option<()> {
        let ArenaExprKind::Ident(name) = self.program.arena.expr(id).kind else { return Some(()); };
        let Some(slot) = slots.resolve(name) else { return Some(()); };
        let Some(&binding) = slots.host_bindings_by_slot.get(&slot) else { return Some(()); };
        if name != binding.name() || slots.types.get(&name) != Some(&binding.ty()) { return None; }
        let origin = self.expression_identity(id);
        let solved = self.solved();
        let caller = solved.expression_owners.get(&origin).copied();
        let ty = *solved.expressions.get(&origin)?;
        let scope = solved.expression_scope(origin, caller).ok()?;
        let source_type = ScopedRoot { ty, scope };
        solved.graph.validate_scoped(source_type).ok()?;
        if crate::runtime::eval::indexed::generic::graph_ground_type(&solved.graph, ty).ok()? != binding.ty() { return None; }
        if !matches!(self.scratch.borrow().expressions.get(lowered.index()), Some(BuildExprRow::Param(actual)) if *actual == slot) { return None; }
        self.scratch.borrow_mut().host_binding_reads.insert(origin, BuildHostBindingRead { binding, slot, origin, expression: lowered, source_type, caller });
        Some(())
    }
}
