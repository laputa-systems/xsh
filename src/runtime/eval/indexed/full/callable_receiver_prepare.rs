use super::*;
use super::super::generic::OriginalCallableReceiver;

impl FullBuilder {
    pub(super) fn prepare_callable_receivers(&mut self, solved: &crate::sema::check::SolvedTypes, origins: &FxHashMap<u32, (crate::sema::check::ExpressionIdentity, InstructionOwner)>, bindings: &BTreeMap<crate::sema::check::BindingIdentity, super::super::generic::UserCallableContract>) -> Result<(), IrBuildError> {
        let problem = |message| IrBuildError::format(message, None, 0, 0);
        for (original, instruction, owner, resolved) in self.callable_receiver_rows.clone() {
            let contract = bindings.get(&original.binding).ok_or_else(|| problem("callable_receiver_original_binding_missing"))?;
            let (wrapper, initializer, pattern) = resolved.ok_or_else(|| problem("callable_receiver_original_wrapper_missing"))?;
            if origins.get(&instruction) != Some(&(original.origin, owner)) || origins.get(&initializer) != Some(&(original.origin, owner)) {
                return Err(problem("callable_receiver_original_source_changed"));
            }
            let binding = solved.bindings.get(&original.binding).ok_or_else(|| problem("callable_receiver_original_binding_missing"))?;
            if solved.expression_owners.get(&original.origin).copied() != binding.owner { return Err(problem("callable_receiver_original_owner_changed")); }
            let ty = *solved.expressions.get(&original.origin).ok_or_else(|| problem("callable_receiver_original_type_missing"))?;
            let scope = solved.expression_scope(original.origin, binding.owner).map_err(|_| problem("callable_receiver_original_scope"))?;
            solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope }).map_err(|_| problem("callable_receiver_original_scope"))?;
            let descriptor = self.intern_checked_callable_type(&solved.graph, ty)?;
            if self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("callable_receiver_original_descriptor"))? != Some((contract.kind, contract.signature)) { return Err(problem("callable_receiver_original_contract_changed")); }
            self.generic_evidence_mut().add_original_callable_receiver(OriginalCallableReceiver {
                origin: original.origin, binding: original.binding, instruction, initializer,
                slot: u32::try_from(original.slot).map_err(|_| problem("callable_receiver_slot_overflow"))?, wrapper, pattern, owner,
            }).map_err(|_| problem("callable_receiver_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_saved_callable_receiver(store: &FullStore, tree: &super::super::pattern::PatternTree, index: &callable_prepare::CallableLexicalIndex, binding: &super::super::generic::OriginalCallableBinding, receiver: &OriginalCallableReceiver) -> Result<(), IrVerifyError> {
        let (initializer, pattern, body) = argument_prepare::saved_argument_wrapper(store, receiver.wrapper)?;
        if binding.binding != receiver.binding || binding.owner != receiver.owner || initializer != receiver.initializer || pattern != receiver.pattern
            || store.tags.get(receiver.instruction as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[receiver.instruction as usize].range())? != [receiver.slot]
            || store.tags.get(initializer as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[initializer as usize].range())? != [binding.slot]
            || store.patterns.get(pattern as usize) != Some(&FullPatternTag::Bind)
            || store.payload(store.pattern_data.get(pattern as usize).ok_or_else(|| IrVerifyError::new("saved callable receiver pattern is missing"))?.range())? != [receiver.slot]
            || !tree.is_descendant(body, receiver.instruction)? || !index.dominates(tree, binding.instruction, initializer)? {
            return Err(IrVerifyError::new("saved callable receiver changes its original initialization or binding scope"));
        }
        Ok(())
    }
}
