use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use xsh::execution::script::RunOptions;
use xsh::mem_track::CountingAllocator;
use xsh::runtime_stats::run_script;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator::new();

const HELP: &str = "\
xsh-runtime-stats

Usage:
  xsh-runtime-stats --json REPORT SCRIPT [-- ARGS...]

Runs one ordinary indexed script and writes thread-attributed allocation traffic
to REPORT. Script stdout and stderr are preserved; the report is never mixed
into stdout. Worker peaks are thread-local allocation-pressure evidence, not
process RSS or an exact concurrent-live total. The report separately attributes
worker allocation traffic to setup, result buffering, item evaluation, and
fused reduction.
";

struct Cli {
    report: PathBuf,
    options: RunOptions,
}

fn main() -> ExitCode {
    CountingAllocator::install_marker();
    let cli = match parse_args(std::env::args().skip(1).collect()) {
        Ok(Some(cli)) => cli,
        Ok(None) => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("xsh-runtime-stats: {message}");
            return ExitCode::from(2);
        }
    };
    let measured = run_script(cli.options);
    let _ = std::io::stdout().lock().write_all(&measured.output.stdout);
    let _ = std::io::stderr().lock().write_all(&measured.output.stderr);
    if let Err(error) = std::fs::write(&cli.report, measured.to_json()) {
        eprintln!(
            "xsh-runtime-stats: failed to write '{}': {error}",
            cli.report.display()
        );
        return ExitCode::from(2);
    }
    ExitCode::from(measured.output.status)
}

fn parse_args(args: Vec<String>) -> Result<Option<Cli>, String> {
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
    Ok(Some(Cli {
        report: PathBuf::from(report),
        options: RunOptions {
            script: script.clone(),
            args: script_args,
            coverage_trace_dir: None,
        },
    }))
}
