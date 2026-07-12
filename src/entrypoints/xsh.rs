use std::process::ExitCode;
#[cfg(feature = "perf-metrics")]
use xsh::perf::{AllocationSnapshot, allocation_metrics_requested, allocation_snapshot};
use xsh::runner::{RunOptions, ScriptOutput, run_script, run_startup};
use xsh::runtime::process::{clear_cancellation_request, install_cancellation_signal_handlers};

const HELP: &str = "\
xsh 0.0.1

Usage:
  xsh [--strict-lower] SCRIPT [ARGS...]
  xsh [--strict-lower] -- SCRIPT ARGS...
  xsh SCRIPT [ARGS...]
  xsh -- SCRIPT ARGS...
  xsh --startup
  xsh --help

--startup boots the interpreter and exits immediately, running no program. It
exposes the fixed startup cost for benchmarking (e.g. as a calibration baseline).
--strict-lower reports compact-lowering failures instead of allowing supported
dynamic lowered operations.
";

pub fn main() -> ExitCode {
    #[cfg(all(feature = "dhat-heap", not(feature = "perf-metrics")))]
    let _dhat_profiler = start_dhat_profiler();

    let _signal_guard = match install_cancellation_signal_handlers() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("xsh: failed to install signal handlers: {error}");
            return ExitCode::from(2);
        }
    };
    clear_cancellation_request();

    if std::env::args().nth(1).as_deref() == Some("--startup") {
        return finish(run_startup(), None);
    }

    match parse_run(std::env::args().skip(1).collect()) {
        Ok(None) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Some(options)) => {
            #[cfg(feature = "perf-metrics")]
            let measure_allocations = allocation_metrics_requested();
            #[cfg(feature = "perf-metrics")]
            if measure_allocations {
                xsh::perf::reset_allocations();
            }
            let output = run_script(options);
            #[cfg(feature = "perf-metrics")]
            let allocations = if measure_allocations {
                allocation_snapshot()
            } else {
                None
            };
            #[cfg(not(feature = "perf-metrics"))]
            let allocations = None;
            finish(output, allocations)
        }
        Err(message) => {
            eprintln!("xsh: {message}");
            ExitCode::from(2)
        }
    }
}

fn parse_run(args: Vec<String>) -> Result<Option<RunOptions>, String> {
    if args.is_empty() || matches!(args.first().map(String::as_str), Some("--help" | "-h")) {
        return Ok(None);
    }

    let mut index = 0;
    let mut strict_lower = false;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--strict-lower" => {
                strict_lower = true;
                index += 1;
                continue;
            }
            "-i" | "--interactive" => {
                return Err("interactive mode moved to `xshi`; run `xshi` instead".to_string());
            }
            "--pid1" => {
                return Err("PID 1 mode is not supported by `xsh`".to_string());
            }
            "--trace"
            | "--raw"
            | "--trace-format"
            | "--trace-file"
            | "--syscalls"
            | "--trace-top-syscalls" => return Err(trace_moved_message()),
            "run" | "check" | "fmt" | "lint" | "ast" | "trace" => {
                return Err("xsh does not take subcommands; use xsht for tools".to_string());
            }
            _ => {}
        }
        break;
    }
    if let Some(arg) = args.get(index)
        && matches!(
            arg.as_str(),
            "run" | "check" | "fmt" | "lint" | "ast" | "trace"
        )
    {
        return Err("xsh does not take subcommands; use xsht for tools".to_string());
    }

    if matches!(args.get(index).map(String::as_str), Some("--")) {
        index += 1;
        let script = args
            .get(index)
            .ok_or_else(|| "SCRIPT is required after `--`".to_string())?
            .clone();
        index += 1;
        return Ok(Some(RunOptions {
            script,
            args: args[index..].to_vec(),
            coverage_trace_dir: None,
            strict_lower,
        }));
    }

    let script = args
        .get(index)
        .ok_or_else(|| "SCRIPT is required".to_string())?
        .clone();
    index += 1;
    let script_args = if matches!(args.get(index).map(String::as_str), Some("--")) {
        args[index + 1..].to_vec()
    } else {
        args[index..].to_vec()
    };

    Ok(Some(RunOptions {
        script,
        args: script_args,
        coverage_trace_dir: None,
        strict_lower,
    }))
}

// Heap profiler guard for `--features dhat-heap`. Honors `XSH_DHAT_OUT` for the
// output path; defaults to dhat-heap.json in the cwd. Held for the whole run so
// the profile is written on drop. Inert when `perf-metrics` is also enabled (see
// the allocator note in src/perf.rs).
#[cfg(all(feature = "dhat-heap", not(feature = "perf-metrics")))]
fn start_dhat_profiler() -> dhat::Profiler {
    let mut builder = dhat::Profiler::builder();
    if let Ok(path) = std::env::var("XSH_DHAT_OUT") {
        builder = builder.file_name(path);
    }
    builder.build()
}

fn trace_moved_message() -> String {
    "trace options moved to `xsht trace`; run `xsht trace SCRIPT [ARGS...]`".to_string()
}

fn finish(
    output: ScriptOutput,
    #[cfg(feature = "perf-metrics")] allocations: Option<AllocationSnapshot>,
    #[cfg(not(feature = "perf-metrics"))] _allocations: Option<()>,
) -> ExitCode {
    use std::io::Write;

    let _ = std::io::stdout().lock().write_all(&output.stdout);
    let _ = std::io::stderr().lock().write_all(&output.stderr);
    #[cfg(feature = "perf-metrics")]
    if let Some(a) = allocations {
        eprintln!(
            "xsh perf: allocation_calls={} allocation_bytes={} deallocation_calls={} deallocation_bytes={} reallocation_calls={} reallocation_bytes={} peak_rss={}",
            a.allocation_calls,
            a.allocation_bytes,
            a.deallocation_calls,
            a.deallocation_bytes,
            a.reallocation_calls,
            a.reallocation_bytes,
            a.peak_rss_bytes,
        );
        if a.allocation_calls > 0 {
            eprintln!(
                "xsh perf sizes: ≤16b={} ≤64b={} ≤256b={} ≤4096b={} >4096b={}",
                a.alloc_calls_le16,
                a.alloc_calls_le64,
                a.alloc_calls_le256,
                a.alloc_calls_le4096,
                a.alloc_calls_gt4096,
            );
        }
    }
    ExitCode::from(output.status)
}
