use super::*;

/// An immutable callable capture keeps its checked binding allocation and the
/// exact receiving header independently of compiler-created receiver slots.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct CapturedCallableReceiver {
    pub declaration: crate::sema::check::DeclarationIdentity,
    pub definition_owner: Option<crate::sema::check::DeclarationIdentity>,
    pub header_index: u32,
    pub slot: u32,
    pub name: u32,
    pub ty: super::super::TypeId,
    pub source_type: crate::sema::inference::ScopedRoot,
}

/// A compiler-saved callable receiver retains the original immutable binding.
/// It is not a supplied argument, and its temporary slot cannot replace that
/// binding's source identity or initialization authority.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalCallableReceiver {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub binding: crate::sema::check::BindingIdentity,
    pub instruction: u32,
    pub initializer: u32,
    pub slot: u32,
    pub wrapper: u32,
    pub pattern: u32,
    pub owner: InstructionOwner,
    pub capture: Option<CapturedCallableReceiver>,
    pub contract: UserCallableContract,
}

#[derive(Clone, Debug, Default)]
pub(super) struct CallableReceiverEvidence {
    program: Option<u64>,
    receipts: Vec<Entry<Arc<OriginalCallableReceiver>>>,
    originals: Vec<Arc<OriginalCallableReceiver>>,
    instructions: Vec<(u32, usize)>,
}

impl CallableReceiverEvidence {
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.receipts.capacity() * size_of::<Entry<Arc<OriginalCallableReceiver>>>()
            + self.originals.capacity() * size_of::<Arc<OriginalCallableReceiver>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>()
            + self.receipts.len() * (size_of::<OriginalCallableReceiver>() + 2 * size_of::<usize>())
    }
    pub(super) fn shrink_to_fit(&mut self) { self.receipts.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn checkpoint(&self) -> usize { self.receipts.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial: u64) -> Result<(), IrVerifyError> {
        if count > self.receipts.len() || self.receipts.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("callable receiver checkpoint references retired entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) { self.receipts.truncate(count); self.originals.truncate(count); self.instructions.clear(); }
    pub(super) fn finish_indexes(&mut self) {
        self.instructions = self.receipts.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    fn receipt(&self, index: usize) -> Result<&OriginalCallableReceiver, IrVerifyError> {
        let receipt = &self.receipts.get(index).ok_or_else(|| failure("saved callable receiver is missing"))?.value;
        if !self.originals.get(index).is_some_and(|original| Arc::ptr_eq(receipt, original)) { return Err(failure("saved callable receiver changes its original receipt")); }
        Ok(receipt)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn original_callable_receivers(&self) -> impl Iterator<Item = &OriginalCallableReceiver> {
        self.callable_receivers.receipts.iter().map(|entry| entry.value.as_ref())
    }
    pub(in crate::runtime::eval) fn original_callable_receiver(&self, instruction: u32) -> Result<Option<&OriginalCallableReceiver>, IrVerifyError> {
        self.callable_receivers.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.callable_receivers.receipt(self.callable_receivers.instructions[index].1)).transpose()
    }
    pub(super) fn verify_callable_receivers(&self, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let evidence = &self.callable_receivers;
        if !evidence.receipts.is_empty() && evidence.program != Some(self.root) { return Err(failure("saved callable receiver belongs to a foreign program")); }
        if evidence.receipts.len() != evidence.originals.len() { return Err(failure("saved callable receiver ledger is incomplete")); }
        let mut expected = Vec::with_capacity(evidence.receipts.len());
        for index in 0..evidence.receipts.len() {
            let receiver = evidence.receipt(index)?;
            if [receiver.instruction, receiver.initializer, receiver.wrapper].iter().any(|&instruction| owners.get(instruction as usize) != Some(&Some(receiver.owner))) { return Err(failure("saved callable receiver changes its lexical owner")); }
            if self.registered_instruction_origin(receiver.instruction, false).is_some()
                || self.registered_instruction_origin(receiver.initializer, false) != Some((OperationSourceOrigin::Expression(receiver.origin), receiver.owner)) {
                return Err(failure("saved callable receiver changes its original initializer or compiler read identity"));
            }
            if let Some(capture) = &receiver.capture {
                if !matches!(receiver.owner, InstructionOwner::Function(_)) || capture.definition_owner == Some(capture.declaration)
                    || self.original_callable_use(receiver.instruction).is_some() || self.original_callable_use(receiver.initializer).is_some() {
                    return Err(failure("captured saved callable receiver substitutes local binding authority"));
                }
            } else {
                let binding = self.original_callable_binding(receiver.binding).ok_or_else(|| failure("saved callable receiver has no original binding"))?;
                if binding.owner != receiver.owner || binding.contract != receiver.contract { return Err(failure("saved callable receiver changes its original binding authority")); }
                for instruction in [receiver.instruction, receiver.initializer] {
                    let use_ = self.original_callable_use(instruction).ok_or_else(|| failure("saved callable receiver loses its original source use"))?;
                    if use_.origin != receiver.origin || use_.binding != receiver.binding || use_.owner != receiver.owner { return Err(failure("saved callable receiver changes its original binding use")); }
                }
            }
            expected.push((receiver.instruction, index));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != evidence.instructions { return Err(failure("saved callable receiver index is incomplete or ambiguous")); }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn original_callable_creation_contract(&self, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner) -> Result<Option<UserCallableContract>, IrVerifyError> {
        let mut contract = None;
        for (_, value) in self.store.callable_values() {
            let source = self.store.callable_source(value.source)?;
            if source.instruction != instruction { continue; }
            if contract.is_some() || source.origin != origin || source.owner != owner || source.expected != value.contract {
                return Err(failure("conditional callable changes its original creation authority"));
            }
            contract = Some(value.contract);
        }
        Ok(contract)
    }

    pub(in crate::runtime::eval) fn add_original_callable_receiver(&mut self, receiver: OriginalCallableReceiver) -> Result<(), IrVerifyError> {
        if self.store.callable_receivers.receipts.len() >= 2_000_000 { return Err(failure("saved callable receiver limit exceeded")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("saved callable receiver serial overflow"))?;
        let receiver = Arc::new(receiver);
        self.store.callable_receivers.program = Some(self.store.root);
        self.store.callable_receivers.originals.push(Arc::clone(&receiver));
        self.store.callable_receivers.receipts.push(Entry { serial, value: receiver });
        Ok(())
    }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_callable_creation_expected_mut(&mut self, id: CallableValueSourceId) -> Result<&mut UserCallableContract, IrVerifyError> {
        self.callable_source(id)?;
        Ok(&mut self.callable_sources[id.index as usize].value.expected)
    }

    pub(in crate::runtime::eval) fn test_remove_original_callable_receivers(&mut self) {
        self.callable_receivers = CallableReceiverEvidence::default();
    }
    pub(in crate::runtime::eval) fn test_replace_original_callable_receivers(&mut self, other: &Self) {
        self.callable_receivers = other.callable_receivers.clone();
    }
    pub(in crate::runtime::eval) fn test_original_callable_receiver_mut(&mut self, instruction: u32) -> Result<&mut OriginalCallableReceiver, IrVerifyError> {
        let index = self.callable_receivers.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("saved callable receiver is missing"))?;
        let index = self.callable_receivers.instructions[index].1;
        Ok(Arc::make_mut(&mut self.callable_receivers.receipts[index].value))
    }
}
