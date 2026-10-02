use super::*;

/// A saved receiver retains its original source and compiler allocation.
/// A present-arm read also retains the separate optional narrowing receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedNativeReceiverTransport {
    pub initializer: u32,
    pub guarded_read: Option<u32>,
    pub initializer_source_instruction: u32,
    pub initializer_wrappers: Box<[ValueInitializerWrapper]>,
    pub wrapper: u32,
    pub pattern: u32,
    pub body: u32,
    pub slot: u32,
}

impl PreparedNativeReceiverTransport {
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.initializer_wrappers.len() * size_of::<ValueInitializerWrapper>()
            + self.initializer_wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()
    }
}

impl GenericEvidenceStore {
    pub fn has_saved_native_receivers(&self) -> bool {
        self.native_call_sources.iter().any(|entry| entry.value.expected.receiver.as_ref().is_some_and(|receiver| receiver.saved.is_some()))
    }
    pub(super) fn native_receiver_has_original_source(&self, receiver: &PreparedNativeReceiver, owner: InstructionOwner, owners: &[Option<InstructionOwner>]) -> Result<bool, IrVerifyError> {
        let expected = Some((OperationSourceOrigin::Expression(receiver.origin), owner));
        if [receiver.instruction, receiver.source_instruction].into_iter().any(|instruction| owners.get(instruction as usize) != Some(&Some(owner)))
            || self.registered_instruction_origin(receiver.source_instruction, false) != expected { return Ok(false); }
        let Some(saved) = &receiver.saved else {
            if let Some(postfix) = &receiver.postfix {
                return Ok(postfix.origin == receiver.origin && postfix.owner == owner
                    && postfix.instruction == receiver.instruction && postfix.source_instruction == receiver.source_instruction
                    && postfix.source_wrappers == receiver.source_wrappers && postfix.source_type == receiver.source_type
                    && postfix.success_type == receiver.ty && owners.get(postfix.carrier as usize) == Some(&Some(owner))
                    && self.registered_instruction_origin(postfix.instruction, false).is_none());
            }
            if let Some(guard) = self.original_optional_receiver_guard(receiver.instruction)? {
                return Ok(guard.origin == receiver.origin && guard.owner == owner && guard.source_type == receiver.source_type && guard.success_type == receiver.ty);
            }
            return Ok(true);
        };
        if receiver.postfix.is_some() { return Ok(false); }
        if [receiver.instruction, saved.initializer, saved.initializer_source_instruction, saved.wrapper, saved.body].into_iter().any(|instruction| owners.get(instruction as usize) != Some(&Some(owner)))
            || self.registered_instruction_origin(receiver.instruction, false).is_some()
            || self.registered_instruction_origin(saved.initializer_source_instruction, false) != expected
            || receiver.source_instruction != saved.initializer_source_instruction || receiver.source_wrappers != saved.initializer_wrappers { return Ok(false); }
        if let Some(read) = saved.guarded_read {
            let guard = self.original_optional_receiver_guard(read)?.ok_or_else(|| failure("saved native receiver loses its optional narrowing receipt"))?;
            if read != saved.initializer || guard.origin != receiver.origin || guard.owner != owner || guard.source_type != receiver.source_type || guard.success_type != receiver.ty { return Ok(false); }
        }
        let wrapper = self.original_compiler_argument_wrapper(saved.wrapper)?.ok_or_else(|| failure("saved native receiver loses its compiler allocation receipt"))?;
        Ok(wrapper.owner == owner && wrapper.initializer == saved.initializer && wrapper.pattern == saved.pattern && wrapper.body == saved.body && wrapper.slot == saved.slot)
    }
}
