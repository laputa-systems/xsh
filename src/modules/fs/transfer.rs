#[allow(unused_imports, reason = "transfer API seam consumed by later filesystem slices")]
pub(crate) use xsh_root::transfer::{
    CancelReason, Cancellation, CopyMethod, CopyStats, Progress, TRANSFER_BUFFER_BYTES,
    TreeTransfer, copy_data, transfer_tree,
};
