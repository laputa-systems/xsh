//! Uninstrumented preparation boundary over the existing production library.
//! Successful preparation never executes the prepared script. Whole-process
//! timing includes entrypoint overhead, preparation, and ordinary owner teardown.

use std::io::{self, Write};
use xsh::execution::script::{RunOptions, prepare_benchmark_script};

fn main() {
    assert!(cfg!(target_os = "macos"), "the frozen preparation observer is macOS-only");
    let mut args = std::env::args().skip(1);
    assert_eq!(args.next().as_deref(), Some("prepare"), "usage: product-probe prepare ENTRY [ARGS...]");
    let script = args.next().expect("entry source required");
    let options = RunOptions { script, args: args.collect(), coverage_trace_dir: None };
    match prepare_benchmark_script(options) {
        Ok(prepared) => drop(prepared),
        Err(output) => {
            io::stdout().write_all(&output.stdout).expect("write stdout");
            io::stderr().write_all(&output.stderr).expect("write stderr");
            std::process::exit(i32::from(output.status));
        }
    }
}
