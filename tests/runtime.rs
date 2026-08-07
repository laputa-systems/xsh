macro_rules! env {
    ("CARGO_BIN_EXE_xsht") => {
        crate::runtime::common::workspace_binary("xsht")
    };
    ("CARGO_BIN_EXE_xshi") => {
        crate::runtime::common::workspace_binary("xshi")
    };
    ($name:literal) => {
        option_env!($name).expect(concat!("missing Cargo test binary: ", $name))
    };
}

#[path = "runtime/collections.rs"]
mod collections;
#[path = "runtime/common.rs"]
mod common;
#[path = "runtime/coverage.rs"]
mod coverage;
#[cfg(feature = "tools")]
#[path = "runtime/examples.rs"]
mod examples;
#[path = "runtime/frontend_indexed.rs"]
mod frontend_indexed;
#[path = "runtime/interactive.rs"]
mod interactive;
#[path = "runtime/linux.rs"]
mod linux;
#[path = "runtime/modules.rs"]
mod modules;
#[path = "runtime/os.rs"]
mod os;
#[path = "runtime/process.rs"]
mod process;
#[path = "runtime/run.rs"]
mod run;
#[path = "runtime/stack_depth.rs"]
mod stack_depth;
#[path = "runtime/streams.rs"]
mod streams;
#[path = "runtime/unix.rs"]
mod unix;
