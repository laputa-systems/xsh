#[cfg(feature = "laputa-asterinas")]
pub mod xshi;

#[cfg(feature = "laputa-asterinas")]
pub use xshi::app;
#[cfg(feature = "laputa-asterinas")]
pub use xshi::interactive;
#[cfg(feature = "laputa-asterinas")]
pub use xshi::interactive::{
    OneCommandOptions, RunOptions, check_source, run, run_one_command,
    run_one_command_with_options, run_with_options,
};

#[cfg(not(feature = "laputa-asterinas"))]
use std::process::ExitCode;

/// Keep non-Laputa builds explicit while the interactive shell is opt-in.
#[cfg(not(feature = "laputa-asterinas"))]
pub fn stub_main() -> ExitCode {
    eprintln!("xshi: disabled — build with the laputa-asterinas feature to enable the interactive shell");
    ExitCode::FAILURE
}
