use super::*;

/// An immutable callable capture keeps its checked binding allocation and the
/// exact receiving header independently of compiler-created receiver slots.
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// An authored callable read keeps its immutable capture allocation separately
/// from any compiler temporary that later saves the selected branch value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct OriginalCapturedCallableSource {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub binding: crate::sema::check::BindingIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub capture: CapturedCallableReceiver,
    pub source_type: crate::sema::inference::ScopedRoot,
    pub contract: UserCallableContract,
}

#[derive(Clone, Debug)]
enum CallableReceiverReceipt {
    Saved(OriginalCallableReceiver),
    CapturedSource(OriginalCapturedCallableSource),
}

impl CallableReceiverReceipt {
    fn instruction(&self) -> u32 {
        match self { Self::Saved(receiver) => receiver.instruction, Self::CapturedSource(source) => source.instruction }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct CallableReceiverEvidence {
    program: Option<u64>,
    receipts: Vec<Entry<Arc<CallableReceiverReceipt>>>,
    originals: Vec<Arc<CallableReceiverReceipt>>,
    instructions: Vec<(u32, usize)>,
}

impl CallableReceiverEvidence {
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.receipts.capacity() * size_of::<Entry<Arc<CallableReceiverReceipt>>>()
            + self.originals.capacity() * size_of::<Arc<CallableReceiverReceipt>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>()
            + self.receipts.len() * (size_of::<CallableReceiverReceipt>() + 2 * size_of::<usize>())
    }
    pub(super) fn shrink_to_fit(&mut self) { self.receipts.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn checkpoint(&self) -> usize { self.receipts.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial: u64) -> Result<(), IrVerifyError> {
        if count > self.receipts.len() || self.receipts.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("callable receiver checkpoint references retired entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) { self.receipts.truncate(count); self.originals.truncate(count); self.instructions.clear(); }
    pub(super) fn finish_indexes(&mut self) {
        self.instructions = self.receipts.iter().enumerate().map(|(index, entry)| (entry.value.instruction(), index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    fn receipt(&self, index: usize) -> Result<&CallableReceiverReceipt, IrVerifyError> {
        let receipt = &self.receipts.get(index).ok_or_else(|| failure("saved callable receiver is missing"))?.value;
        if !self.originals.get(index).is_some_and(|original| Arc::ptr_eq(receipt, original)) { return Err(failure("saved callable receiver changes its original receipt")); }
        Ok(receipt)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn original_callable_receivers(&self) -> impl Iterator<Item = &OriginalCallableReceiver> {
        self.callable_receivers.receipts.iter().filter_map(|entry| match entry.value.as_ref() { CallableReceiverReceipt::Saved(receiver) => Some(receiver), _ => None })
    }
    pub(in crate::runtime::eval) fn original_callable_receiver(&self, instruction: u32) -> Result<Option<&OriginalCallableReceiver>, IrVerifyError> {
        let Some(index) = self.callable_receivers.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok() else { return Ok(None); };
        Ok(match self.callable_receivers.receipt(self.callable_receivers.instructions[index].1)? { CallableReceiverReceipt::Saved(receiver) => Some(receiver), _ => None })
    }
    pub(in crate::runtime::eval) fn original_captured_callable_sources(&self) -> impl Iterator<Item = &OriginalCapturedCallableSource> {
        self.callable_receivers.receipts.iter().filter_map(|entry| match entry.value.as_ref() { CallableReceiverReceipt::CapturedSource(source) => Some(source), _ => None })
    }
    pub(in crate::runtime::eval) fn original_captured_callable_source(&self, instruction: u32) -> Result<Option<&OriginalCapturedCallableSource>, IrVerifyError> {
        if !self.callable_receivers.receipts.is_empty() && self.callable_receivers.program != Some(self.root) {
            return Err(failure("captured callable source belongs to a foreign program"));
        }
        let Some(index) = self.callable_receivers.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok() else { return Ok(None); };
        Ok(match self.callable_receivers.receipt(self.callable_receivers.instructions[index].1)? { CallableReceiverReceipt::CapturedSource(source) => Some(source), _ => None })
    }
    pub(super) fn verify_callable_receivers(&self, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let evidence = &self.callable_receivers;
        if !evidence.receipts.is_empty() && evidence.program != Some(self.root) { return Err(failure("saved callable receiver belongs to a foreign program")); }
        if evidence.receipts.len() != evidence.originals.len() { return Err(failure("saved callable receiver ledger is incomplete")); }
        let mut expected = Vec::with_capacity(evidence.receipts.len());
        for index in 0..evidence.receipts.len() {
            let receipt = evidence.receipt(index)?;
            if let CallableReceiverReceipt::CapturedSource(source) = receipt {
                if owners.get(source.instruction as usize) != Some(&Some(source.owner))
                    || !matches!(source.owner, InstructionOwner::Function(_))
                    || source.capture.definition_owner == Some(source.capture.declaration)
                    || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner))
                    || self.original_callable_use(source.instruction).is_some() {
                    return Err(failure("captured callable source changes its original read or allocation owner"));
                }
                expected.push((source.instruction, index));
                continue;
            }
            let CallableReceiverReceipt::Saved(receiver) = receipt else { unreachable!() };
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
    pub(in crate::runtime::eval) fn original_captured_callable_source_contract(&self, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner) -> Result<Option<UserCallableContract>, IrVerifyError> {
        if !self.store.callable_receivers.receipts.is_empty() && self.store.callable_receivers.program != Some(self.store.root) {
            return Err(failure("captured callable source belongs to a foreign program"));
        }
        let mut contract = None;
        for index in 0..self.store.callable_receivers.receipts.len() {
            let CallableReceiverReceipt::CapturedSource(source) = self.store.callable_receivers.receipt(index)? else { continue; };
            if source.instruction != instruction { continue; }
            if contract.is_some() || source.origin != origin || source.owner != owner {
                return Err(failure("conditional callable changes its original captured read authority"));
            }
            contract = Some(source.contract);
        }
        Ok(contract)
    }

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
        self.add_callable_receiver_receipt(CallableReceiverReceipt::Saved(receiver))
    }

    pub(in crate::runtime::eval) fn add_original_captured_callable_source(&mut self, source: OriginalCapturedCallableSource) -> Result<(), IrVerifyError> {
        self.add_callable_receiver_receipt(CallableReceiverReceipt::CapturedSource(source))
    }

    fn add_callable_receiver_receipt(&mut self, receiver: CallableReceiverReceipt) -> Result<(), IrVerifyError> {
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
    pub(in crate::runtime::eval) fn test_remove_original_captured_callable_sources(&mut self) {
        let receipts = std::mem::take(&mut self.callable_receivers.receipts);
        let originals = std::mem::take(&mut self.callable_receivers.originals);
        for (receipt, original) in receipts.into_iter().zip(originals) {
            if matches!(receipt.value.as_ref(), CallableReceiverReceipt::Saved(_)) {
                self.callable_receivers.receipts.push(receipt);
                self.callable_receivers.originals.push(original);
            }
        }
        self.callable_receivers.finish_indexes();
    }
    pub(in crate::runtime::eval) fn test_replace_original_callable_receivers(&mut self, other: &Self) {
        self.callable_receivers = other.callable_receivers.clone();
    }
    pub(in crate::runtime::eval) fn test_original_callable_receiver_mut(&mut self, instruction: u32) -> Result<&mut OriginalCallableReceiver, IrVerifyError> {
        let index = self.callable_receivers.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("saved callable receiver is missing"))?;
        let index = self.callable_receivers.instructions[index].1;
        match Arc::make_mut(&mut self.callable_receivers.receipts[index].value) { CallableReceiverReceipt::Saved(receiver) => Ok(receiver), _ => Err(failure("saved callable receiver is missing")) }
    }
    pub(in crate::runtime::eval) fn test_original_captured_callable_source_mut(&mut self, instruction: u32) -> Result<&mut OriginalCapturedCallableSource, IrVerifyError> {
        let index = self.callable_receivers.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("captured callable source is missing"))?;
        let index = self.callable_receivers.instructions[index].1;
        match Arc::make_mut(&mut self.callable_receivers.receipts[index].value) { CallableReceiverReceipt::CapturedSource(source) => Ok(source), _ => Err(failure("captured callable source is missing")) }
    }
}
