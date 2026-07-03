use crate::xsht::cli::{
    AnnotationPolicy, AnnotationSelection, CliOutput, TraceFormat, TraceOptions, ast_script,
    check_paths_with_summary_options, docs_command, format_files, grep_scripts, lint_files,
    refactor_scripts, trace_script,
};
use crate::xsht::test::{TestOptions, test_scripts};
use std::process::ExitCode;
use xsh::perf::{AllocationSnapshot, allocation_metrics_requested, allocation_snapshot};
use xsh::runtime::process::{clear_cancellation_request, install_cancellation_signal_handlers};

const HELP: &str = "\
xsht 0.0.1

Usage:
  xsht <command> [OPTIONS]
  xsht help [COMMAND]
  xsht --help

Commands:
  check      Parse and type-check scripts
  fmt        Format scripts
  lint       Run quality checks and optional fixes
  ast        Print parser debug output
  trace      Run a script with trace output
  docs       Build or check generated docs
  test       Run XSH tests
  grep       Search scripts with AST patterns
  refactor   Rewrite scripts with AST patterns

Run `xsht COMMAND --help` for command-specific options.
";

const CHECK_HELP: &str = "\
xsht check

Usage:
  xsht check [--strict] [--summary] [--annotate[=default|signatures|locals|all|CLASS,...]] [PATH...]

Options:
  --strict       Enable strict dynamic-data migration diagnostics
  --summary      Append diagnostic counts by code after normal diagnostics
  --annotate     Apply configured inferred annotations in place
";

const FMT_HELP: &str = "\
xsht fmt

Usage:
  xsht fmt [--check] [FILE...]

Options:
  --check        Check formatting without rewriting files
";

const LINT_HELP: &str = "\
xsht lint

Usage:
  xsht lint [--fix] [--runless] [FILE...]

Options:
  --fix          Apply safe autofixes (removes redundant defaults, updates stale syntax)
  --runless      Error on any external command (run). Configure exceptions in xsht-config.ini
                 with [lint] runless-except entries
";

const AST_HELP: &str = "\
xsht ast

Usage:
  xsht ast SCRIPT
";

const TRACE_HELP: &str = "\
xsht trace

Usage:
  xsht trace [--raw] [--trace-format text|jsonl|flamegraph] [--trace-file PATH] [--syscalls] [--trace-top-syscalls N] SCRIPT [ARGS...]

Options:
  --raw                   Write verbose per-event trace output
  --trace-format FORMAT   Use text, jsonl, or flamegraph trace output
  --trace-file PATH       Write trace output to PATH instead of stderr
  --syscalls              Include Linux native ptrace syscall totals
  --trace-top-syscalls N  Number of syscall rows to show. Defaults to 8
";

const DOCS_HELP: &str = "\
xsht docs

Usage:
  xsht docs build
  xsht docs check
";

const TEST_HELP: &str = "\
xsht test

Usage:
  xsht test [OPTIONS] [FILTER]

Options:
  --examples              Run cataloged example integration tests instead of native tests
  --all                   Run native tests and cataloged example integration tests
  --list                  List matching tests without running them
  --exact                 Match FILTER exactly
  --cov                   Run matching tests and print XSH API coverage
  --nocapture             Print test stdout and stderr while tests run
  --fail-fast             Stop after the first failure
  --keep-temp             Preserve native test temporary directories
  --cov-json FILE         Write XSH API coverage JSON to FILE
  --trace-top-syscalls N  Run each example test under Linux ptrace and print top N syscalls (requires --examples or --all)
  --trace-json-out FILE   Write per-test syscall summaries to FILE as JSON (use with make test-trace-save)
  --syscall-budgets FILE  Load per-test syscall budgets from FILE; fail tests that exceed their budget
";

const GREP_HELP: &str = "\
xsht grep

Usage:
  xsht grep PATTERN [FILE...]

Arguments:
  PATTERN        XSH expression pattern; uppercase identifiers are metavariables
  FILE...        Files or directories to search (default: all .xsh files under .)
";

const REFACTOR_HELP: &str = "\
xsht refactor

Usage:
  xsht refactor PATTERN REPLACEMENT [FILE...]

Arguments:
  PATTERN        XSH expression pattern to find
  REPLACEMENT    XSH expression pattern to substitute (metavariables filled from match)
  FILE...        Files or directories to rewrite (default: all .xsh files under .)

Options:
  --dry-run      Show proposed changes without modifying files
";

pub fn main() -> ExitCode {
    let _signal_guard = match install_cancellation_signal_handlers() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("xsht: failed to install signal handlers: {error}");
            return ExitCode::from(2);
        }
    };
    clear_cancellation_request();

    match parse_tool(std::env::args().skip(1).collect()) {
        Ok(Command::Help(text)) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Ok(Command::Check {
            paths,
            strict,
            annotation_selection,
            summary,
        }) => finish_with_perf(|| {
            check_paths_with_summary_options(&paths, strict, annotation_selection, summary)
        }),
        Ok(Command::Fmt { files, check }) => finish_with_perf(|| format_files(&files, check)),
        Ok(Command::Lint {
            files,
            fix,
            runless,
        }) => finish_with_perf(|| lint_files(&files, fix, runless)),
        Ok(Command::Ast { script }) => finish_with_perf(|| ast_script(&script)),
        Ok(Command::Trace { options }) => finish_with_perf(|| trace_script(options)),
        Ok(Command::Docs { command }) => finish_with_perf(|| docs_command(&command)),
        Ok(Command::Test { options }) => finish_with_perf(|| test_scripts(options)),
        Ok(Command::Grep { pattern, files }) => finish_with_perf(|| grep_scripts(&pattern, &files)),
        Ok(Command::Refactor {
            pattern,
            replacement,
            files,
            dry_run,
        }) => finish_with_perf(|| refactor_scripts(&pattern, &replacement, &files, dry_run)),
        Err(message) => {
            eprintln!("xsht: {message}");
            ExitCode::from(2)
        }
    }
}

enum Command {
    Help(&'static str),
    Check {
        paths: Vec<String>,
        strict: bool,
        annotation_selection: Option<AnnotationSelection>,
        summary: bool,
    },
    Fmt {
        files: Vec<String>,
        check: bool,
    },
    Lint {
        files: Vec<String>,
        fix: bool,
        runless: bool,
    },
    Ast {
        script: String,
    },
    Trace {
        options: TraceOptions,
    },
    Docs {
        command: String,
    },
    Test {
        options: TestOptions,
    },
    Grep {
        pattern: String,
        files: Vec<String>,
    },
    Refactor {
        pattern: String,
        replacement: String,
        files: Vec<String>,
        dry_run: bool,
    },
}

fn parse_tool(args: Vec<String>) -> Result<Command, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(Command::Help(HELP));
    };

    match command {
        "--help" | "-h" => Ok(Command::Help(HELP)),
        "help" => parse_help(&args[1..]),
        "check" => parse_check(&args[1..]),
        "fmt" => parse_fmt(&args[1..]),
        "lint" => parse_lint(&args[1..]),
        "ast" => parse_ast(&args[1..]),
        "trace" => parse_trace(&args[1..]),
        "docs" => parse_docs(&args[1..]),
        "test" => parse_test(&args[1..]),
        "grep" => parse_grep(&args[1..]),
        "refactor" => parse_refactor(&args[1..]),
        "run" => Err("xsht has no `run`; use xsh SCRIPT instead".to_string()),
        other => Err(format!("unknown command '{other}'")),
    }
}

#[allow(clippy::single_call_fn)]
fn parse_help(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Help(HELP)),
        [command] => help_for_command(command)
            .map(Command::Help)
            .ok_or_else(|| format!("unknown help topic '{command}'")),
        _ => Err("`xsht help` accepts at most one command".to_string()),
    }
}

#[allow(clippy::single_call_fn)]
fn help_for_command(command: &str) -> Option<&'static str> {
    match command {
        "check" => Some(CHECK_HELP),
        "fmt" => Some(FMT_HELP),
        "lint" => Some(LINT_HELP),
        "ast" => Some(AST_HELP),
        "trace" => Some(TRACE_HELP),
        "docs" => Some(DOCS_HELP),
        "test" => Some(TEST_HELP),
        "grep" => Some(GREP_HELP),
        "refactor" => Some(REFACTOR_HELP),
        _ => None,
    }
}

fn parse_check(args: &[String]) -> Result<Command, String> {
    let mut strict = false;
    let mut summary = false;
    let mut annotation_selection = None;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--strict" => strict = true,
            "--summary" => summary = true,
            "--annotate" => annotation_selection = Some(AnnotationSelection::Configured),
            "--help" | "-h" => return Ok(Command::Help(CHECK_HELP)),
            other if other.starts_with("--annotate=") => {
                let value = other
                    .strip_prefix("--annotate=")
                    .expect("checked annotation prefix");
                annotation_selection = Some(AnnotationSelection::Policy(
                    AnnotationPolicy::from_arg(value)
                        .map_err(|message| format!("invalid `xsht check --annotate`: {message}"))?,
                ));
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown `xsht check` option '{other}'"));
            }
            _ => paths.push(arg.clone()),
        }
    }
    Ok(Command::Check {
        paths,
        strict,
        annotation_selection,
        summary,
    })
}

fn parse_test(args: &[String]) -> Result<Command, String> {
    let mut filter = None;
    let mut examples = false;
    let mut all = false;
    let mut list = false;
    let mut exact = false;
    let mut coverage = false;
    let mut nocapture = false;
    let mut fail_fast = false;
    let mut keep_temp = false;
    let mut coverage_json_out = None;
    let mut trace_top_syscalls = None;
    let mut syscall_json_out = None;
    let mut syscall_budgets_file: Option<String> = None;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(TEST_HELP)),
            "--examples" => examples = true,
            "--all" => all = true,
            "--list" => list = true,
            "--exact" => exact = true,
            "--cov" => coverage = true,
            "--nocapture" => nocapture = true,
            "--fail-fast" => fail_fast = true,
            "--keep-temp" => keep_temp = true,
            "--cov-json" => {
                coverage_json_out = Some(
                    iter.next()
                        .ok_or_else(|| "`--cov-json` requires FILE".to_string())?
                        .clone(),
                );
            }
            "--trace-top-syscalls" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "`--trace-top-syscalls` requires N".to_string())?;
                let n = value
                    .parse::<usize>()
                    .map_err(|_| "`--trace-top-syscalls` must be a positive integer".to_string())?;
                if n == 0 {
                    return Err("`--trace-top-syscalls` must be a positive integer".to_string());
                }
                trace_top_syscalls = Some(n);
            }
            "--trace-json-out" => {
                syscall_json_out = Some(
                    iter.next()
                        .ok_or_else(|| "`--trace-json-out` requires FILE".to_string())?
                        .clone(),
                );
            }
            "--syscall-budgets" => {
                syscall_budgets_file = Some(
                    iter.next()
                        .ok_or_else(|| "`--syscall-budgets` requires FILE".to_string())?
                        .clone(),
                );
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown `xsht test` option '{other}'"));
            }
            other => {
                if filter.is_some() {
                    return Err("`xsht test` accepts at most one FILTER".to_string());
                }
                filter = Some(other.to_string());
            }
        }
    }

    if examples && all {
        return Err("`xsht test` accepts only one of `--examples` or `--all`".to_string());
    }

    let syscall_budgets = if let Some(ref path) = syscall_budgets_file {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("`--syscall-budgets`: failed to read '{path}': {e}"))?;
        let parsed = parse_syscall_budgets(&text)
            .map_err(|e| format!("`--syscall-budgets`: invalid JSON in '{path}': {e}"))?;
        Some(parsed)
    } else {
        None
    };

    Ok(Command::Test {
        options: TestOptions {
            filter,
            native: !examples || all,
            examples: examples || all,
            list,
            exact,
            nocapture,
            fail_fast,
            keep_temp,
            coverage,
            coverage_json_out,
            trace_top_syscalls,
            syscall_json_out,
            syscall_budgets,
        },
    })
}

fn parse_syscall_budgets(
    text: &str,
) -> Result<std::collections::BTreeMap<String, std::collections::BTreeMap<String, u64>>, String> {
    let value: miniserde::json::Value =
        miniserde::json::from_str(text).map_err(|_| "invalid JSON".to_string())?;
    let miniserde::json::Value::Object(tests) = value else {
        return Err("expected object".to_string());
    };
    let mut parsed = std::collections::BTreeMap::new();
    for (test, value) in tests {
        let miniserde::json::Value::Object(budgets) = value else {
            return Err(format!("budget for '{test}' must be an object"));
        };
        let mut parsed_budgets = std::collections::BTreeMap::new();
        for (name, value) in budgets {
            let Some(limit) = raw_json_as_u64(&value) else {
                return Err(format!(
                    "budget '{test}.{name}' must be a non-negative integer"
                ));
            };
            parsed_budgets.insert(name, limit);
        }
        parsed.insert(test, parsed_budgets);
    }
    Ok(parsed)
}

fn raw_json_as_u64(value: &miniserde::json::Value) -> Option<u64> {
    match value {
        miniserde::json::Value::Number(miniserde::json::Number::U64(value)) => Some(*value),
        miniserde::json::Value::Number(miniserde::json::Number::I64(value)) => {
            u64::try_from(*value).ok()
        }
        miniserde::json::Value::Number(miniserde::json::Number::F64(value))
            if *value >= 0.0 && value.fract() == 0.0 =>
        {
            Some(*value as u64)
        }
        _ => None,
    }
}

fn parse_ast(args: &[String]) -> Result<Command, String> {
    if matches!(args.first().map(String::as_str), Some("--help" | "-h")) {
        return Ok(Command::Help(AST_HELP));
    }
    let script = args
        .first()
        .ok_or_else(|| "`xsht ast` requires SCRIPT".to_string())?;
    if args.len() > 1 {
        return Err("`xsht ast` accepts exactly one SCRIPT".to_string());
    }
    Ok(Command::Ast {
        script: script.clone(),
    })
}

fn parse_docs(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Err("`xsht docs` requires build|check".to_string()),
        [arg] if matches!(arg.as_str(), "--help" | "-h" | "help") => Ok(Command::Help(DOCS_HELP)),
        [command] if matches!(command.as_str(), "build" | "check") => Ok(Command::Docs {
            command: command.clone(),
        }),
        [command] => Err(format!("unknown `xsht docs` command '{command}'")),
        _ => Err("`xsht docs` accepts exactly one command: build or check".to_string()),
    }
}

fn parse_trace(args: &[String]) -> Result<Command, String> {
    let mut index = 0;
    let mut raw = false;
    let mut format = TraceFormat::Text;
    let mut file = None;
    let mut syscalls = false;
    let mut top_syscalls = 8usize;

    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(TRACE_HELP)),
            "--raw" => {
                raw = true;
                index += 1;
            }
            "--trace-format" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "`--trace-format` requires a value".to_string())?;
                format = match value.as_str() {
                    "text" => TraceFormat::Text,
                    "jsonl" => TraceFormat::Jsonl,
                    "flamegraph" => TraceFormat::Flamegraph,
                    _ => {
                        return Err(
                            "`--trace-format` must be `text`, `jsonl`, or `flamegraph`".to_string()
                        );
                    }
                };
                index += 2;
            }
            "--trace-file" => {
                file = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "`--trace-file` requires PATH".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--syscalls" => {
                syscalls = true;
                index += 1;
            }
            "--trace-top-syscalls" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "`--trace-top-syscalls` requires N".to_string())?;
                top_syscalls = value
                    .parse::<usize>()
                    .map_err(|_| "`--trace-top-syscalls` must be a positive integer".to_string())?;
                if top_syscalls == 0 {
                    return Err("`--trace-top-syscalls` must be a positive integer".to_string());
                }
                index += 2;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown `xsht trace` option '{other}'"));
            }
            _ => break,
        }
    }

    let script = args
        .get(index)
        .ok_or_else(|| "`xsht trace` requires SCRIPT".to_string())?
        .clone();
    index += 1;
    let script_args = if matches!(args.get(index).map(String::as_str), Some("--")) {
        args[index + 1..].to_vec()
    } else {
        args[index..].to_vec()
    };

    Ok(Command::Trace {
        options: TraceOptions {
            script,
            args: script_args,
            raw,
            format,
            file,
            syscalls,
            top_syscalls,
        },
    })
}

fn parse_lint(args: &[String]) -> Result<Command, String> {
    let mut files = Vec::new();
    let mut fix = false;
    let mut runless = false;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(LINT_HELP)),
            "--fix" => fix = true,
            "--runless" => runless = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown `xsht lint` option '{other}'"));
            }
            _ => files.push(arg.clone()),
        }
    }

    Ok(Command::Lint {
        files,
        fix,
        runless,
    })
}

fn parse_fmt(args: &[String]) -> Result<Command, String> {
    let mut check = false;
    let mut files = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(FMT_HELP)),
            "--check" => check = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown `xsht fmt` option '{other}'"));
            }
            _ => files.push(arg.clone()),
        }
    }

    Ok(Command::Fmt { files, check })
}

fn parse_grep(args: &[String]) -> Result<Command, String> {
    let mut files = Vec::new();
    let mut pattern: Option<String> = None;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(GREP_HELP)),
            other if other.starts_with('-') => {
                return Err(format!("unknown `xsht grep` option '{other}'"));
            }
            _ => {
                if pattern.is_none() {
                    pattern = Some(arg.clone());
                } else {
                    files.push(arg.clone());
                }
            }
        }
    }

    let pattern = pattern.ok_or_else(|| "`xsht grep` requires PATTERN".to_string())?;
    Ok(Command::Grep { pattern, files })
}

fn parse_refactor(args: &[String]) -> Result<Command, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut dry_run = false;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(REFACTOR_HELP)),
            "--dry-run" => dry_run = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown `xsht refactor` option '{other}'"));
            }
            _ => positional.push(arg.clone()),
        }
    }

    let pattern = positional
        .first()
        .ok_or_else(|| "`xsht refactor` requires PATTERN".to_string())?
        .clone();
    let replacement = positional
        .get(1)
        .ok_or_else(|| "`xsht refactor` requires REPLACEMENT".to_string())?
        .clone();
    let files = positional.into_iter().skip(2).collect();

    Ok(Command::Refactor {
        pattern,
        replacement,
        files,
        dry_run,
    })
}

fn finish_with_perf(run: impl FnOnce() -> CliOutput) -> ExitCode {
    let measure_allocations = allocation_metrics_requested();
    if measure_allocations {
        xsh::perf::reset_allocations();
    }
    let output = run();
    let allocations = if measure_allocations {
        allocation_snapshot()
    } else {
        None
    };
    finish(output, allocations)
}

fn finish(output: CliOutput, allocations: Option<AllocationSnapshot>) -> ExitCode {
    use std::io::Write;

    let _ = std::io::stdout().lock().write_all(&output.stdout);
    let _ = std::io::stderr().lock().write_all(&output.stderr);
    if !output.trace_text.is_empty() {
        eprint!("{}", output.trace_text);
    }
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
