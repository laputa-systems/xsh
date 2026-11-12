#[allow(unused_imports, reason = "transfer API seam consumed by later filesystem slices")]
pub(crate) use xsh_root::transfer::{
    Action, Ancestor, Boundary, CancelReason, Cancellation, Collision, CopyMethod, CopyStats,
    DeferredFlags, DryRunEntry, EngineOptions, EntryIdentity, Jobs, MergeRequest, Observation,
    Operation, Operands, PathError, PlannedOperand, PolicyOptions, PolicyRequest, Preflight,
    Progress, Publication, PublishRequest, Removal, RemovalError, Retirement, SyncOperands,
    SyncOptions, SyncProgress, SyncRequest, Target, TreeTransfer, TRANSFER_BUFFER_BYTES, copy_data,
    decide, dry_run, into_basename, merge_tree, mount_id, observe_entry, observe_fd, preflight,
    publish_tree, sync_tree, transfer_tree, worker_count,
};

#[cfg(target_os = "linux")]
#[allow(unused_imports, reason = "transfer API seam consumed by later filesystem slices")]
pub(crate) use xsh_root::transfer::Session;
