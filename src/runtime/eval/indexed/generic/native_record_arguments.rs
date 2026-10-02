use super::*;
use crate::sema::arguments::ArgumentValueSource;
use crate::sema::types::Type;

/// A finite spread field retains the authored record entry separately from
/// its saved record read, field projection and reordered parameter operand.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedNativeRecordFieldArgument {
    pub ordinal: u32,
    pub formal_slot: u32,
    pub entry_index: u32,
    pub record_origin: crate::sema::check::ExpressionIdentity,
    pub field: Name,
    pub record_type: GroundTypeId,
    pub instruction: u32,
    pub wrappers: Box<[ValueInitializerWrapper]>,
    pub field_read: u32,
    pub field_initializer: u32,
    pub field_wrapper: u32,
    pub field_pattern: u32,
    pub field_body: u32,
    pub field_slot: u32,
    pub record_read: u32,
    pub record_initializer: u32,
    pub record_source_instruction: u32,
    pub record_wrappers: Box<[ValueInitializerWrapper]>,
    pub record_wrapper: u32,
    pub record_pattern: u32,
    pub record_body: u32,
    pub record_slot: u32,
}

impl PreparedNativeRecordFieldArgument {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        [&self.wrappers, &self.record_wrappers].into_iter().map(|wrappers| wrappers.len() * size_of::<ValueInitializerWrapper>()
            + wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()).sum()
    }

    fn matches_argument(&self, source: &NativeCallSource, argument: &PreparedInvocationArgument, ordinal: usize) -> bool {
        self.ordinal as usize == ordinal && self.instruction == argument.instruction
            && source.expected.binding.supplied_slots.get(ordinal) == Some(&self.formal_slot)
            && self.entry_index as usize == argument.original.entry_index && argument.original.name == Some(self.field)
            && self.record_origin.source == source.origin.source && self.record_origin.namespace == source.origin.namespace
            && matches!(argument.original.value, ArgumentValueSource::RecordField { record, field } if record == self.record_origin.expression && field == self.field)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn has_native_record_arguments(&self) -> bool { self.native_call_sources().any(|(_, source)| !source.record_arguments.is_empty()) }

    pub(super) fn native_record_argument_has_original_source(&self, instruction: u32, call: crate::sema::check::ExpressionIdentity, ordinal: usize,
        recipe: &crate::sema::check::SolvedArgumentSource, owner: InstructionOwner, ty: TypeRef,
    ) -> bool {
        self.native_call_sources().any(|(id, source)| {
            if source.origin != call || source.owner != owner || self.native_call_source(id).is_err() { return false; }
            let Some(argument) = source.expected.arguments.get(ordinal) else { return false; };
            if argument.instruction != instruction || argument.original != *recipe || argument.ty != ty { return false; }
            source.record_arguments.iter().any(|field| field.matches_argument(source, argument, ordinal)
                && self.registered_instruction_origin(field.record_source_instruction, false) == Some((OperationSourceOrigin::Expression(field.record_origin), owner)))
        })
    }

    pub(super) fn verify_native_record_arguments(&self, pools: &SemanticPools, source: &NativeCallSource, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let mut previous = None;
        for field in source.record_arguments.iter() {
            let argument = source.expected.arguments.get(field.ordinal as usize).ok_or_else(|| failure("native spread field changes its original argument ordinal"))?;
            let TypeRef::Ground(actual) = argument.ty else { return Err(failure("native spread field requires its original closed type")); };
            let Type::Record(fields) = pools.to_type(field.record_type)? else { return Err(failure("native spread source loses its original finite Record type")); };
            if previous.is_some_and(|ordinal| ordinal >= field.ordinal) || fields.is_empty()
                || fields.get(&field.field) != Some(&pools.to_type(actual)?) || !field.matches_argument(source, argument, field.ordinal as usize)
                || self.registered_instruction_origin(field.record_source_instruction, false) != Some((OperationSourceOrigin::Expression(field.record_origin), source.owner))
                || [field.instruction, field.field_read, field.field_initializer, field.field_wrapper, field.field_body, field.record_read,
                    field.record_initializer, field.record_source_instruction, field.record_wrapper, field.record_body].into_iter()
                    .any(|instruction| owners.get(instruction as usize) != Some(&Some(source.owner))) {
                return Err(failure("native spread field changes its original record, membership, type or allocation owner"));
            }
            for (wrapper, initializer, pattern, body, slot) in [(field.record_wrapper, field.record_initializer, field.record_pattern, field.record_body, field.record_slot),
                (field.field_wrapper, field.field_initializer, field.field_pattern, field.field_body, field.field_slot)] {
                let receipt = self.original_compiler_argument_wrapper(wrapper)?.ok_or_else(|| failure("native spread field loses its original compiler allocation"))?;
                if receipt.owner != source.owner || receipt.initializer != initializer || receipt.pattern != pattern || receipt.body != body || receipt.slot != slot {
                    return Err(failure("native spread field changes its original saved record or field allocation"));
                }
            }
            previous = Some(field.ordinal);
        }
        if source.expected.arguments.iter().filter(|argument| matches!(argument.original.value, ArgumentValueSource::RecordField { .. })).count() != source.record_arguments.len() {
            return Err(failure("native call loses its original finite spread field receipts"));
        }
        Ok(())
    }
}
