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

#[cfg(not(feature = "laputa-asterinas"))]
pub fn stub_main() -> ExitCode {
    use std::io::Write;
    let message = "xshi: disabled — build with the laputa-asterinas feature to enable the interactive shell\n";
    let output = std::io::stderr().write_all(message.as_bytes());
    let help = std::env::args_os().nth(1).is_some_and(|flag| flag == "-h" || flag == "--help");
    if help && output.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
