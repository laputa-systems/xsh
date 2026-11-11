#[allow(unused_imports, reason = "transfer API seam consumed by later filesystem slices")]
pub(crate) use xsh_root::transfer::{
    Action, Ancestor, Boundary, CancelReason, Cancellation, Collision, CopyMethod, CopyStats,
    EntryIdentity, MergeRequest, Observation, Operation, Operands, PathError, PlannedOperand,
    PolicyOptions, PolicyRequest, Preflight, Progress, Publication, PublishRequest, Removal,
    RemovalError, Retirement, Target, TreeTransfer, TRANSFER_BUFFER_BYTES, copy_data, decide,
    into_basename, merge_tree, observe_entry, observe_fd, preflight, publish_tree, transfer_tree,
};

#[cfg(target_os = "linux")]
#[allow(unused_imports, reason = "transfer API seam consumed by later filesystem slices")]
pub(crate) use xsh_root::transfer::Session;
