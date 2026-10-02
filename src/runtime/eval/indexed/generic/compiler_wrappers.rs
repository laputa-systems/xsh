use super::*;

/// Compiler argument sequencing is a physical execution receipt. It carries
/// no original expression identity and cannot authorize a generated read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct OriginalCompilerArgumentWrapper {
    pub instruction: u32,
    pub initializer: u32,
    pub pattern: u32,
    pub body: u32,
    pub slot: u32,
    pub owner: InstructionOwner,
    pub payload: Box<[u32]>,
    pub arms_flags: u8,
    pub arms_payload: Box<[u32]>,
    pub pattern_payload: Box<[u32]>,
    pub optional_receiver_guard: Option<OriginalOptionalReceiverGuard>,
}

/// A present-arm read carries the checked optional narrowing while the carrier
/// retains its original optional descriptor and expression identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct OriginalOptionalReceiverGuard {
    pub call: crate::sema::check::ExpressionIdentity,
    pub origin: crate::sema::check::ExpressionIdentity,
    pub source_type: TypeRef,
    pub success_type: TypeRef,
    pub call_source_type: TypeRef,
    pub call_result_type: TypeRef,
    pub carrier: u32,
    pub read: u32,
    pub owner: InstructionOwner,
    pub wrapper: u32,
    pub body: u32,
    pub slot: u32,
    pub null_pattern: u32,
    pub absent: u32,
    pub null_pattern_payload: Box<[u32]>,
    pub absent_payload: Box<[u32]>,
    pub read_payload: Box<[u32]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct CompilerWrapperEvidence {
    program: Option<u64>,
    receipts: Vec<Entry<Arc<OriginalCompilerArgumentWrapper>>>,
    originals: Vec<Arc<OriginalCompilerArgumentWrapper>>,
    instructions: Vec<(u32, usize)>,
    guarded_reads: Vec<(u32, usize)>,
}

impl CompilerWrapperEvidence {
    pub(super) fn checkpoint(&self) -> usize { self.receipts.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial: u64) -> Result<(), IrVerifyError> {
        if count > self.receipts.len() || self.receipts.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) {
            return Err(failure("compiler wrapper checkpoint references retired entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) {
        self.receipts.truncate(count); self.originals.truncate(count); self.instructions.clear(); self.guarded_reads.clear();
    }
    pub(super) fn finish_indexes(&mut self) {
        self.instructions = self.receipts.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
        self.guarded_reads = self.receipts.iter().enumerate().filter_map(|(index, entry)| entry.value.optional_receiver_guard.as_ref().map(|guard| (guard.read, index))).collect();
        self.guarded_reads.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.receipts.capacity() * size_of::<Entry<Arc<OriginalCompilerArgumentWrapper>>>()
            + self.originals.capacity() * size_of::<Arc<OriginalCompilerArgumentWrapper>>()
            + (self.instructions.capacity() + self.guarded_reads.capacity()) * size_of::<(u32, usize)>()
            + self.receipts.len() * (size_of::<OriginalCompilerArgumentWrapper>() + 2 * size_of::<usize>())
            + self.receipts.iter().map(|entry| (entry.value.payload.len() + entry.value.arms_payload.len() + entry.value.pattern_payload.len() + entry.value.optional_receiver_guard.as_ref().map_or(0, |guard| guard.null_pattern_payload.len() + guard.absent_payload.len() + guard.read_payload.len())) * size_of::<u32>()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.receipts.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); self.guarded_reads.shrink_to_fit(); }
    fn receipt(&self, index: usize) -> Result<&OriginalCompilerArgumentWrapper, IrVerifyError> {
        let value = &self.receipts.get(index).ok_or_else(|| failure("compiler wrapper is missing"))?.value;
        if !self.originals.get(index).is_some_and(|original| Arc::ptr_eq(value, original)) {
            return Err(failure("compiler wrapper changes its original receipt"));
        }
        Ok(value)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn original_compiler_argument_wrapper(&self, instruction: u32) -> Result<Option<&OriginalCompilerArgumentWrapper>, IrVerifyError> {
        if !self.compiler_wrappers.receipts.is_empty() && self.compiler_wrappers.program != Some(self.root) { return Err(failure("compiler wrapper belongs to a foreign program")); }
        self.compiler_wrappers.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok()
            .map(|index| self.compiler_wrappers.receipt(self.compiler_wrappers.instructions[index].1)).transpose().map(|receipt| receipt.filter(|receipt| receipt.optional_receiver_guard.is_none()))
    }
    pub(in crate::runtime::eval) fn original_compiler_argument_wrappers(&self) -> impl Iterator<Item = &OriginalCompilerArgumentWrapper> {
        self.compiler_wrappers.receipts.iter().filter(|entry| entry.value.optional_receiver_guard.is_none()).map(|entry| entry.value.as_ref())
    }
    pub(in crate::runtime::eval) fn has_optional_receiver_guards(&self) -> bool { !self.compiler_wrappers.guarded_reads.is_empty() }
    pub(in crate::runtime::eval) fn original_optional_receiver_guard(&self, read: u32) -> Result<Option<&OriginalOptionalReceiverGuard>, IrVerifyError> {
        if !self.compiler_wrappers.receipts.is_empty() && self.compiler_wrappers.program != Some(self.root) { return Err(failure("optional receiver guard belongs to a foreign program")); }
        self.compiler_wrappers.guarded_reads.binary_search_by_key(&read, |entry| entry.0).ok()
            .map(|index| self.compiler_wrappers.receipt(self.compiler_wrappers.guarded_reads[index].1).and_then(|receipt| receipt.optional_receiver_guard.as_ref().ok_or_else(|| failure("optional receiver guard index changes receipt kind")))).transpose()
    }
    pub(in crate::runtime::eval) fn original_optional_receiver_guard_at(&self, wrapper: u32) -> Result<Option<&OriginalOptionalReceiverGuard>, IrVerifyError> {
        if !self.compiler_wrappers.receipts.is_empty() && self.compiler_wrappers.program != Some(self.root) { return Err(failure("optional receiver guard belongs to a foreign program")); }
        self.compiler_wrappers.instructions.binary_search_by_key(&wrapper, |entry| entry.0).ok()
            .map(|index| self.compiler_wrappers.receipt(self.compiler_wrappers.instructions[index].1)).transpose()
            .map(|receipt| receipt.and_then(|receipt| receipt.optional_receiver_guard.as_ref()))
    }
    pub(in crate::runtime::eval) fn original_optional_receiver_guards(&self) -> impl Iterator<Item = &OriginalOptionalReceiverGuard> {
        self.compiler_wrappers.receipts.iter().filter_map(|entry| entry.value.optional_receiver_guard.as_ref())
    }
    pub(in crate::runtime::eval) fn optional_receiver_wrapper(&self, read: u32) -> Result<Option<&OriginalCompilerArgumentWrapper>, IrVerifyError> {
        self.original_optional_receiver_guard(read)?;
        self.compiler_wrappers.guarded_reads.binary_search_by_key(&read, |entry| entry.0).ok()
            .map(|index| self.compiler_wrappers.receipt(self.compiler_wrappers.guarded_reads[index].1)).transpose()
    }
    pub(super) fn verify_compiler_wrappers(&self, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let evidence = &self.compiler_wrappers;
        if !evidence.receipts.is_empty() && evidence.program != Some(self.root) { return Err(failure("compiler wrapper belongs to a foreign program")); }
        if evidence.receipts.len() != evidence.originals.len() { return Err(failure("compiler wrapper original ledger is incomplete")); }
        let mut expected = Vec::with_capacity(evidence.receipts.len());
        for index in 0..evidence.receipts.len() {
            let wrapper = evidence.receipt(index)?;
            if [wrapper.instruction, wrapper.initializer, wrapper.body].into_iter().any(|instruction| owners.get(instruction as usize) != Some(&Some(wrapper.owner))) {
                return Err(failure("compiler wrapper changes its original owner"));
            }
            let origin = self.registered_instruction_origin(wrapper.instruction, false);
            if let Some(guard) = &wrapper.optional_receiver_guard {
                if origin != Some((OperationSourceOrigin::Expression(guard.call), guard.owner)) || guard.wrapper != wrapper.instruction
                    || guard.carrier != wrapper.initializer || guard.body != wrapper.body || guard.slot != wrapper.slot
                    || owners.get(guard.read as usize) != Some(&Some(wrapper.owner)) {
                    return Err(failure("optional receiver guard changes its original call, owner or carrier"));
                }
            } else if origin.is_some() { return Err(failure("compiler wrapper acquires an original expression identity")); }
            expected.push((wrapper.instruction, index));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        let mut guarded = (0..evidence.receipts.len()).filter_map(|index| evidence.receipts[index].value.optional_receiver_guard.as_ref().map(|guard| (guard.read, index))).collect::<Vec<_>>();
        guarded.sort_unstable_by_key(|entry| entry.0);
        if guarded != evidence.guarded_reads || guarded.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(failure("optional receiver guard read index is incomplete or ambiguous")); }
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != evidence.instructions {
            return Err(failure("compiler wrapper instruction index is incomplete or ambiguous"));
        }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_original_compiler_argument_wrapper(&mut self, wrapper: OriginalCompilerArgumentWrapper) -> Result<(), IrVerifyError> {
        if self.store.compiler_wrappers.receipts.len() >= 2_000_000 { return Err(failure("compiler wrapper limit exceeded")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("compiler wrapper serial overflow"))?;
        let wrapper = Arc::new(wrapper);
        self.store.compiler_wrappers.program = Some(self.store.root);
        self.store.compiler_wrappers.originals.push(Arc::clone(&wrapper));
        self.store.compiler_wrappers.receipts.push(Entry { serial, value: wrapper });
        Ok(())
    }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_remove_original_compiler_argument_wrappers(&mut self) {
        self.compiler_wrappers.receipts.clear(); self.compiler_wrappers.instructions.clear(); self.compiler_wrappers.guarded_reads.clear();
    }
    pub(in crate::runtime::eval) fn test_replace_original_compiler_argument_wrappers(&mut self, other: &Self) {
        self.compiler_wrappers = other.compiler_wrappers.clone();
    }
    pub(in crate::runtime::eval) fn test_original_compiler_argument_wrapper_mut(&mut self, instruction: u32) -> Result<&mut OriginalCompilerArgumentWrapper, IrVerifyError> {
        let index = self.compiler_wrappers.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("compiler wrapper is missing"))?;
        let index = self.compiler_wrappers.instructions[index].1;
        Ok(Arc::make_mut(&mut self.compiler_wrappers.receipts[index].value))
    }
}
