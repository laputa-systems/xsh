// Reuse the ordinary aggregator while compiling its Linux-only cases.
#[path = "core_compat_extra.rs"]
mod core_compat_extra;

pub(crate) use core_compat_extra::test_binary;
