use super::*;
use std::mem::size_of;

/// A supplied operand retains its checked wrappers separately from the source
/// recipe, so an unsigned check cannot be removed by replacing a saved read.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedNativeArgumentLineage {
    pub ordinal: u32,
    pub instruction: u32,
    pub source_instruction: u32,
    pub source_type: TypeRef,
    pub material_type: TypeRef,
    pub wrappers: Box<[ValueInitializerWrapper]>,
}

impl PreparedNativeArgumentLineage {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        size_of::<Self>() + self.wrappers.len() * size_of::<ValueInitializerWrapper>()
            + self.wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn native_argument_has_original_source(&self, source: &NativeCallSource,
        ordinal: usize, argument: &PreparedInvocationArgument, owners: &[Option<InstructionOwner>],
    ) -> Result<bool, IrVerifyError> {
        if source.argument_lineages.len() != source.expected.arguments.len() {
            return Err(failure("native call loses its original argument lineages"));
        }
        let lineage = source.argument_lineages.get(ordinal).ok_or_else(|| failure("native argument lineage is missing"))?;
        if lineage.ordinal as usize != ordinal || lineage.instruction != argument.instruction
            || lineage.wrappers.len() > 256 || owners.get(lineage.source_instruction as usize) != Some(&Some(source.owner)) {
            return Err(failure("native argument changes its original lineage or owner"));
        }
        let mut current = argument.instruction;
        for wrapper in &lineage.wrappers {
            if wrapper.instruction != current || owners.get(current as usize) != Some(&Some(source.owner)) {
                return Err(failure("native argument changes its original wrapper lineage"));
            }
            let child = match wrapper.kind {
                ValueInitializerWrapperKind::CheckedValue => *wrapper.payload.first().ok_or_else(|| failure("native argument checked wrapper is empty"))?,
                ValueInitializerWrapperKind::CompilerArgument { body, .. } => body,
                _ => return Err(failure("native argument has another wrapper authority")),
            };
            if child >= current { return Err(failure("native argument wrapper lineage is cyclic")); }
            current = child;
        }
        if current != lineage.source_instruction { return Err(failure("native argument changes its original material source")); }
        if lineage.material_type != argument.ty { return Err(failure("native argument changes its original checked material type")); }
        Ok(self.argument_has_original_source(current, source.origin, ordinal, &argument.original, source.owner, lineage.source_type))
    }
}
