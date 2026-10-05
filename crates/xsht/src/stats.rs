//! `xsht frontend-stats` and `xsht runtime-stats`: profiling commands that are
//! not listed in help.
//!
//! They live beside the binary's entry point instead of in the command table
//! because they need the counting global allocator, which only the `xsht`
//! binary installs; the `xsht` library can be linked into other programs.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use xsh::execution::script::RunOptions;
use xsh::frontend_stats::{DEFAULT_ROOTS, measure_roots};
use xsh::mem_track;
use xsh::runtime_stats::run_script;

const FRONTEND_STATS: &str = "frontend-stats";
const RUNTIME_STATS: &str = "runtime-stats";

const FRONTEND_HELP: &str = "\
xsht frontend-stats

Usage:
  xsht frontend-stats [--json|--text] [ROOT ...]

With no roots, measures the frontend fixture corpus.
";

const RUNTIME_HELP: &str = "\
xsht runtime-stats

Usage:
  xsht runtime-stats --json REPORT SCRIPT [-- ARGS...]

Runs one ordinary indexed script and writes thread-attributed allocation traffic
to REPORT. Script stdout and stderr are preserved; the report is never mixed
into stdout. Worker peaks are thread-local allocation-pressure evidence, not
process RSS or an exact concurrent-live total. The report separately attributes
worker allocation traffic to setup, result buffering, item evaluation, and
fused reduction.
";

/// Runs the statistics command the process was started as and returns its exit
/// status, or `None` when the arguments name any other command.
pub(crate) fn run() -> Option<ExitCode> {
    let mut args = std::env::args_os().skip(1);
    let command = match args.next()?.to_str()? {
        FRONTEND_STATS => FRONTEND_STATS,
        RUNTIME_STATS => RUNTIME_STATS,
        _ => return None,
    };
    let mut rest = Vec::new();
    for (index, arg) in args.enumerate() {
        match arg.into_string() {
            Ok(arg) => rest.push(arg),
            Err(_) => {
                eprintln!(
                    "xsht {command}: argument {} is not valid UTF-8",
                    index + 2
                );
                return Some(ExitCode::from(2));
            }
        }
    }
    mem_track::enable_tracking();
    Some(if command == FRONTEND_STATS {
        frontend_stats(rest)
    } else {
        runtime_stats(rest)
    })
}

fn frontend_stats(args: Vec<String>) -> ExitCode {
    let mut json = false;
    let mut roots = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--text" => json = false,
            "--help" | "-h" => {
                print!("{FRONTEND_HELP}");
                return ExitCode::SUCCESS;
            }
            _ if arg.starts_with('-') => {
                eprintln!("xsht {FRONTEND_STATS}: unknown option `{arg}`");
                return ExitCode::from(2);
            }
            _ => roots.push(PathBuf::from(arg)),
        }
    }
    if roots.is_empty() {
        roots.extend(DEFAULT_ROOTS.iter().map(PathBuf::from));
    }

    match measure_roots(&roots) {
        Ok(stats) => {
            if json {
                print!("{}", stats.to_json());
            } else {
                print!("{}", stats.to_text());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xsht {FRONTEND_STATS}: {error}");
            ExitCode::from(1)
        }
    }
}

struct RuntimeStatsRun {
    report: PathBuf,
    options: RunOptions,
}

fn runtime_stats(args: Vec<String>) -> ExitCode {
    let run = match parse_runtime_stats(args) {
        Ok(Some(run)) => run,
        Ok(None) => {
            print!("{RUNTIME_HELP}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("xsht {RUNTIME_STATS}: {message}");
            return ExitCode::from(2);
        }
    };
    let measured = run_script(run.options);
    let _ = std::io::stdout().lock().write_all(&measured.output.stdout);
    let _ = std::io::stderr().lock().write_all(&measured.output.stderr);
    if let Err(error) = std::fs::write(&run.report, measured.to_json()) {
        eprintln!(
            "xsht {RUNTIME_STATS}: failed to write '{}': {error}",
            run.report.display()
        );
        return ExitCode::from(2);
    }
    ExitCode::from(measured.output.status)
}

fn parse_runtime_stats(args: Vec<String>) -> Result<Option<RuntimeStatsRun>, String> {
    if matches!(args.first().map(String::as_str), Some("--help" | "-h")) {
        return Ok(None);
    }
    if args.is_empty() {
        return Err("--json REPORT and SCRIPT are required; use --help for usage".to_string());
    }
    if args.first().map(String::as_str) != Some("--json") {
        return Err("--json REPORT is required".to_string());
    }
    let report = args
        .get(1)
        .ok_or_else(|| "REPORT is required after --json".to_string())?;
    let script = args
        .get(2)
        .ok_or_else(|| "SCRIPT is required after REPORT".to_string())?;
    if script.starts_with('-') {
        return Err("SCRIPT must follow REPORT directly".to_string());
    }
    let script_args = if matches!(args.get(3).map(String::as_str), Some("--")) {
        args[4..].to_vec()
    } else {
        args[3..].to_vec()
    };
    Ok(Some(RuntimeStatsRun {
        report: PathBuf::from(report),
        options: RunOptions {
            script: script.clone(),
            args: script_args,
            coverage_trace_dir: None,
        },
    }))
}
