//! Preparation and one ordinary prepared execution are timed separately.
//! Output and report writes happen after both intervals; execution includes
//! the normal evaluator owner teardown performed by the prepared run.

use std::alloc::System;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Instant;
use xsh::execution::script::{RunOptions, prepare_benchmark_script};

#[global_allocator]
static ALLOCATOR: System = System;

fn execute() -> Result<u8, String> {
    if !cfg!(target_os = "macos") {
        return Err("execution observation is macOS-only".into());
    }
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("execute") {
        return Err("usage: execution-probe execute REPORT ENTRY [ARGS...]".into());
    }
    let report_path = args.next().ok_or("REPORT is required")?;
    let script = args.next().ok_or("ENTRY is required")?;
    let options = RunOptions { script, args: args.collect(), coverage_trace_dir: None };
    // Refuse an existing or unwritable report before loading or executing code.
    let mut report = OpenOptions::new().write(true).create_new(true).open(&report_path)
        .map_err(|error| format!("cannot create report '{report_path}': {error}"))?;
    let started = Instant::now();
    let prepared = prepare_benchmark_script(options);
    let preparation_finished = Instant::now();
    let preparation_ms = preparation_finished.duration_since(started).as_secs_f64() * 1000.0;
    let (output, execution_ms) = match prepared {
        Ok(prepared) => {
            let output = prepared.run();
            let finished = Instant::now();
            (output, Some(finished.duration_since(preparation_finished).as_secs_f64() * 1000.0))
        }
        Err(output) => (output, None),
    };
    let total_ms = preparation_ms + execution_ms.unwrap_or(0.0);
    let execution_value = execution_ms.map(|value| value.to_string()).unwrap_or_else(|| "null".into());
    let serialized = format!(
        "{{\"schema_version\":1,\"status\":{},\"preparation_ms\":{},\"execution_ms\":{},\"total_ms\":{},\"stdout_bytes\":{},\"stderr_bytes\":{},\"clock\":\"std::time::Instant\",\"execution_scope\":\"one prepared run including normal evaluator teardown; output and report writes excluded\"}}\n",
        output.status, preparation_ms, execution_value, total_ms, output.stdout.len(), output.stderr.len(),
    );
    io::stdout().lock().write_all(&output.stdout).map_err(|error| format!("write script stdout: {error}"))?;
    io::stderr().lock().write_all(&output.stderr).map_err(|error| format!("write script stderr: {error}"))?;
    report.write_all(serialized.as_bytes()).map_err(|error| format!("write report '{report_path}': {error}"))?;
    Ok(output.status)
}

fn main() -> ExitCode {
    match execute() {
        Ok(status) => ExitCode::from(status),
        Err(error) => {
            eprintln!("execution-probe: {error}");
            ExitCode::from(2)
        }
    }
}
