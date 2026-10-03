use super::*;
use super::super::generic::{PreparedNativeReceiverTransport, ValueInitializerWrapper, ValueInitializerWrapperKind};

fn fs_root_problem(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

pub(super) fn fs_root_receiver_type(source: Type, saved: Option<&PreparedNativeReceiverTransport>) -> Result<Type, IrVerifyError> {
    let propagation = saved.and_then(|saved| saved.initializer_wrappers.first()).is_some_and(|wrapper| wrapper.kind == ValueInitializerWrapperKind::FsRootReceiverTry);
    match source {
        Type::FsRoot if !propagation => Ok(Type::FsRoot),
        Type::Result(inner, error) if propagation && *inner == Type::FsRoot && *error == Type::Error => Ok(Type::FsRoot),
        Type::Optional(inner) if !propagation && *inner == Type::FsRoot
            && saved.is_some_and(|saved| saved.guarded_read.is_some()) => Ok(Type::FsRoot),
        _ => Err(IrVerifyError::new("filesystem root receiver changes its original capability or Result propagation")),
    }
}

impl FullBuilder {
    pub(super) fn fs_root_receiver_lineage(&self, initializer: u32, owner: InstructionOwner, source_type: &Type) -> Result<(u32, Box<[ValueInitializerWrapper]>), IrBuildError> {
        if *source_type == Type::FsRoot { return self.argument_initializer_lineage(initializer, owner); }
        if !matches!(source_type, Type::Result(inner, error) if **inner == Type::FsRoot && **error == Type::Error)
            || self.store.tags.get(initializer as usize) != Some(&FullTag::ExprTry) {
            return Err(fs_root_problem("fs_root_receiver_original_propagation"));
        }
        let payload = self.store.payload(self.store.data[initializer as usize].range()).map_err(|_| fs_root_problem("fs_root_receiver_propagation_payload"))?;
        let [child] = payload else { return Err(fs_root_problem("fs_root_receiver_propagation_child")); };
        if *child >= initializer { return Err(fs_root_problem("fs_root_receiver_propagation_cycle")); }
        let (source, wrappers) = self.argument_initializer_lineage(*child, owner)?;
        let mut retained = Vec::with_capacity(wrappers.len() + 1);
        retained.push(ValueInitializerWrapper { instruction: initializer, payload: payload.to_vec().into_boxed_slice(), kind: ValueInitializerWrapperKind::FsRootReceiverTry });
        retained.extend(wrappers);
        Ok((source, retained.into_boxed_slice()))
    }
}

impl FullVerifier {
    pub(super) fn verify_fs_root_receiver_lineage(store: &FullStore, generic: &GenericEvidenceStore, saved: &PreparedNativeReceiverTransport, owner: InstructionOwner) -> Result<(), IrVerifyError> {
        let Some(wrapper) = saved.initializer_wrappers.first().filter(|wrapper| wrapper.kind == ValueInitializerWrapperKind::FsRootReceiverTry) else {
            return Self::verify_argument_initializer_lineage(store, generic, saved.initializer, saved.initializer_source_instruction, &saved.initializer_wrappers, owner);
        };
        if wrapper.instruction != saved.initializer || store.tags.get(saved.initializer as usize) != Some(&FullTag::ExprTry)
            || store.payload(store.data[saved.initializer as usize].range())? != wrapper.payload.as_ref()
            || wrapper.payload.len() != 1 || wrapper.payload[0] >= saved.initializer
            || generic.registered_instruction_origin(saved.initializer, false).is_some() {
            return Err(IrVerifyError::new("filesystem root receiver changes its original Result propagation wrapper"));
        }
        Self::verify_argument_initializer_lineage(store, generic, wrapper.payload[0], saved.initializer_source_instruction, &saved.initializer_wrappers[1..], owner)
    }
}

/// Capability methods use the native optional-slot packet with a hidden
/// receiver first. An absent user slot preserves the selected native default.
pub(super) fn encoded_fs_root_method_arguments(
    store: &FullStore, instruction: u32, parameter_count: usize,
) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprModuleCall) {
        return Err(IrVerifyError::new("filesystem root method changes its native packet opcode"));
    }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || words[1] != 0 || parameter_count == 0 || parameter_count > 65536 {
        return Err(IrVerifyError::new("filesystem root method has another argument protocol"));
    }
    let operation = *store.runtime_ops.get(words[0] as usize).ok_or_else(|| IrVerifyError::new("filesystem root method operation is invalid"))?;
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("filesystem root method argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST {
        return Err(IrVerifyError::new("filesystem root method arguments have another block kind"));
    }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    if cursor.raw()? as usize != parameter_count {
        return Err(IrVerifyError::new("filesystem root method changes its hidden receiver or default slots"));
    }
    let mut arguments = Vec::with_capacity(parameter_count);
    for _ in 0..parameter_count {
        arguments.push(match cursor.raw()? {
            0 => None, 1 => Some(cursor.raw()?),
            _ => return Err(IrVerifyError::new("filesystem root method optional argument is invalid")),
        });
    }
    cursor.finish()?;
    if arguments[0].is_none() { return Err(IrVerifyError::new("filesystem root method omits its capability receiver")); }
    Ok((operation, arguments, words[3]))
}

#[cfg(test)]
#[path = "fs_root_prepare/tests.rs"]
mod tests;
