use super::*;
use std::mem::size_of;

/// A generated postfix Try transports the success domain of its original
/// Result carrier. The generated instruction has no authored expression ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedResultReceiver {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub carrier: u32,
    pub source_instruction: u32,
    pub source_wrappers: Box<[ValueInitializerWrapper]>,
    pub owner: InstructionOwner,
    pub source_type: TypeRef,
    pub success_type: TypeRef,
    pub error_type: TypeRef,
    pub payload: Box<[u32]>,
}

impl PreparedResultReceiver {
    pub(super) fn retained_bytes(&self) -> usize {
        self.payload.len() * size_of::<u32>()
            + self.source_wrappers.len() * size_of::<ValueInitializerWrapper>()
            + self.source_wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()
    }
}
