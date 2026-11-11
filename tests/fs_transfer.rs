#![cfg(target_os = "linux")]

#[path = "fs_transfer/support.rs"]
mod support;
#[path = "fs_transfer/copy.rs"]
mod copy;
#[path = "fs_transfer/interruption.rs"]
mod interruption;
#[path = "fs_transfer/metadata.rs"]
mod metadata;
#[path = "fs_transfer/policy.rs"]
mod policy;
#[path = "fs_transfer/preservation.rs"]
mod preservation;
#[path = "fs_transfer/publication.rs"]
mod publication;
#[path = "fs_transfer/recovery.rs"]
mod recovery;
#[path = "fs_transfer/tree.rs"]
mod tree;
