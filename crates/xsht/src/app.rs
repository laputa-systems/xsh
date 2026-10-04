use crate::xsht::api::{ApiDetails, ApiFormat, ApiOptions};
use crate::xsht::cli::{
    AnnotationPolicy, AnnotationSelection, CliOutput, TraceFormat, TraceOptions, api_command,
    ast_script, check_paths_with_summary_options, format_files, grep_scripts, lint_files,
    refactor_scripts, trace_script,
};
use crate::xsht::commands::{self, ParsedArgs};
use crate::xsht::help::{command_help as generated_command_help, root_help};
use crate::xsht::test::{TestOptions, install_test_cancellation_signal_handlers, test_scripts};
use std::process::ExitCode;
use xsh::process::{
    clear_cancellation_request, install_cancellation_signal_handlers,
};

pub fn main() -> ExitCode {
    let args: Vec<String> = match std::env::args_os()
        .skip(1)
        .enumerate()
        .map(|(index, arg)| {
            arg.into_string()
                .map_err(|_| format!("argument {} is not valid UTF-8", index + 1))
        })
        .collect()
    {
        Ok(args) => args,
        Err(message) => {
            eprintln!("xsht: {message}");
            return ExitCode::from(2);
        }
    };
    let _signal_guard = match install_cancellation_signal_handlers() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("xsht: failed to install signal handlers: {error}");
            return ExitCode::from(2);
        }
    };
    if args.first().is_some_and(|arg| arg == "test")
        && let Err(error) = install_test_cancellation_signal_handlers()
    {
        eprintln!("xsht: failed to install signal handlers: {error}");
        return ExitCode::from(2);
    }
    clear_cancellation_request();

    match parse_tool(args) {
        Ok(Command::Help(text) | Command::Text(text)) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Ok(Command::Check {
            paths,
            annotation_selection,
            summary,
        }) => finish_command(|| {
            check_paths_with_summary_options(&paths, annotation_selection, summary)
        }),
        Ok(Command::Fmt { files, check }) => finish_command(|| format_files(&files, check)),
        Ok(Command::Lint {
            files,
            fix,
            runless,
            only,
        }) => finish_command(|| lint_files(&files, fix, runless, only)),
        Ok(Command::Ast { script }) => finish_command(|| ast_script(&script)),
        Ok(Command::Trace { options }) => finish_command(|| trace_script(options)),
        Ok(Command::Api { options }) => finish_command(|| api_command(&options)),
        Ok(Command::Test { options }) => finish_command(|| test_scripts(options)),
        Ok(Command::Grep { pattern, files }) => finish_command(|| grep_scripts(&pattern, &files)),
        Ok(Command::Refactor {
            pattern,
            replacement,
            files,
            dry_run,
        }) => finish_command(|| refactor_scripts(&pattern, &replacement, &files, dry_run)),
        Ok(Command::TestFakeRun {
            options,
            linux,
            unix,
        }) => {
            let output = xsh::execution::script::run_script_with_test_fakes(options, linux, unix);
            use std::io::Write;
            let _ = std::io::stdout().lock().write_all(&output.stdout);
            let _ = std::io::stderr().lock().write_all(&output.stderr);
            ExitCode::from(output.status)
        }
        Err(message) => {
            eprintln!("xsht: {message}");
            ExitCode::from(2)
        }
    }
}

enum Command {
    Help(String),
    /// Static listing output, such as `xsht lint --list`.
    Text(String),
    Check {
        paths: Vec<String>,
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
        only: Option<Vec<String>>,
    },
    Ast {
        script: String,
    },
    Trace {
        options: TraceOptions,
    },
    Api {
        options: ApiOptions,
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
    /// Harness-internal: runs a script under the `linux` and `unix` test
    /// fakes for `test.run_script`/`test.run_xsh` after the test called
    /// `test.linux_fake` or `test.unix_fake`. Not listed in help.
    TestFakeRun {
        options: xsh::execution::script::RunOptions,
        linux: Option<xsh::execution::evaluator::LinuxFake>,
        unix: Option<xsh::execution::evaluator::UnixFake>,
    },
}

fn parse_tool(args: Vec<String>) -> Result<Command, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(Command::Help(root_help()));
    };

    match command {
        "--help" | "-h" => Ok(Command::Help(root_help())),
        "help" => parse_help(&args[1..]),
        "check" => parse_check(&args[1..]),
        "fmt" => parse_fmt(&args[1..]),
        "lint" => parse_lint(&args[1..]),
        "ast" => parse_ast(&args[1..]),
        "trace" => parse_trace(&args[1..]),
        "api" => parse_api(&args[1..]),
        "test" => parse_test(&args[1..]),
        "grep" => parse_grep(&args[1..]),
        "refactor" => parse_refactor(&args[1..]),
        TEST_FAKE_RUN => parse_test_fake_run(&args[1..]),
        "run" => Err("xsht has no `run`; use xsh SCRIPT instead".to_string()),
        other => Err(format!("unknown command '{other}'")),
    }
}

pub(crate) const TEST_FAKE_RUN: &str = "__test-fake-run";

/// `__test-fake-run [--fake MODULE | --fake MODULE.KEY=VALUE]... SCRIPT -- [ARGS...]`
///
/// `--fake MODULE` installs that module's fake; a setting installs it too.
fn parse_test_fake_run(args: &[String]) -> Result<Command, String> {
    let usage =
        || format!("usage: xsht {TEST_FAKE_RUN} [--fake MODULE[.KEY=VALUE]]... SCRIPT -- [ARGS...]");
    let mut linux: Option<xsh::execution::evaluator::LinuxFake> = None;
    let mut unix: Option<xsh::execution::evaluator::UnixFake> = None;
    let mut rest = args;
    while let [flag, setting, tail @ ..] = rest
        && flag == "--fake"
    {
        let (module, assignment) = match setting.split_once('.') {
            Some((module, assignment)) => (module, Some(assignment)),
            None => (setting.as_str(), None),
        };
        let pair = match assignment {
            Some(assignment) => Some(
                assignment
                    .split_once('=')
                    .ok_or_else(|| format!("fake setting '{setting}' is not MODULE.KEY=VALUE"))?,
            ),
            None => None,
        };
        match module {
            "linux" => {
                let fake = linux.get_or_insert_default();
                if let Some((key, value)) = pair {
                    fake.set(key, value)?;
                }
            }
            "unix" => {
                let fake = unix.get_or_insert_default();
                if let Some((key, value)) = pair {
                    fake.set(key, value)?;
                }
            }
            other => return Err(format!("unknown fake module '{other}'")),
        }
        rest = tail;
    }
    let [script, separator, script_args @ ..] = rest else {
        return Err(usage());
    };
    if separator != "--" {
        return Err(usage());
    }
    Ok(Command::TestFakeRun {
        options: xsh::execution::script::RunOptions {
            script: script.clone(),
            args: script_args.to_vec(),
            coverage_trace_dir: None,
        },
        linux,
        unix,
    })
}

#[allow(clippy::single_call_fn)]
fn parse_help(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Help(root_help())),
        [command] => help_for_command(command)
            .map(Command::Help)
            .ok_or_else(|| format!("unknown help topic '{command}'")),
        _ => Err("`xsht help` accepts at most one command".to_string()),
    }
}

#[allow(clippy::single_call_fn)]
fn help_for_command(command: &str) -> Option<String> {
    generated_command_help(command)
}

fn command_help_text(command: &str) -> String {
    help_for_command(command).expect("help metadata must cover every parsed command")
}

/// Sorts the command's arguments through its declared options; `build`
/// interprets them. `-h` and `--help` produce the command's help instead.
fn parse_command(
    name: &str,
    args: &[String],
    build: impl FnOnce(ParsedArgs) -> Result<Command, String>,
) -> Result<Command, String> {
    let spec = commands::find(name).expect("every parsed command is declared in the table");
    match commands::parse_command_args(spec, args)? {
        commands::Parsed::Help => Ok(Command::Help(command_help_text(name))),
        commands::Parsed::Args(parsed) => build(parsed),
    }
}

fn parse_check(args: &[String]) -> Result<Command, String> {
    parse_command("check", args, |parsed| {
        let annotation_selection = match parsed.occurrences("--annotate").last() {
            None => None,
            Some(None) => Some(AnnotationSelection::Configured),
            Some(Some(value)) => Some(AnnotationSelection::Policy(
                AnnotationPolicy::from_arg(value)
                    .map_err(|message| format!("invalid `xsht check --annotate`: {message}"))?,
            )),
        };
        Ok(Command::Check {
            summary: parsed.flag("--summary"),
            annotation_selection,
            paths: parsed.positionals,
        })
    })
}

/// Long enough for the slowest suite tests on a loaded machine, short enough
/// that a hung test fails the run instead of stalling it.
const DEFAULT_TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// `0` and `none` disable the limit; otherwise an XSH duration literal.
fn parse_test_timeout(value: &str) -> Result<Option<std::time::Duration>, String> {
    if value == "0" || value == "none" {
        return Ok(None);
    }
    match xsh::execution::value::DurationValue::from_literal(value) {
        Some(duration) if duration.millis == 0 => Ok(None),
        Some(duration) => Ok(Some(std::time::Duration::from_millis(duration.millis))),
        None => Err(format!(
            "`--timeout` expects a duration such as 30s or 5m, or 0 or none; got '{value}'"
        )),
    }
}

fn parse_test(args: &[String]) -> Result<Command, String> {
    parse_command("test", args, |parsed| {
        let jobs = parsed
            .value("--jobs")
            .map(|value| match value.parse::<usize>() {
                Ok(n) if n > 0 => Ok(n),
                _ => Err("`--jobs` must be a positive integer".to_string()),
            })
            .transpose()?;
        let timeout = match parsed.value("--timeout") {
            Some(value) => parse_test_timeout(value)?,
            None => Some(DEFAULT_TEST_TIMEOUT),
        };
        let coverage = parsed.flag("--cov");
        let api = parsed.flag("--api");
        if api && !coverage {
            return Err("`--api` requires `--cov`".to_string());
        }
        let mut positionals = parsed.positionals.iter();
        let filter = positionals.next().cloned();
        if positionals.next().is_some() {
            return Err("`xsht test` accepts at most one FILTER".to_string());
        }
        Ok(Command::Test {
            options: TestOptions {
                filter,
                list: parsed.flag("--list"),
                exact: parsed.flag("--exact"),
                nocapture: parsed.flag("--nocapture"),
                fail_fast: parsed.flag("--fail-fast"),
                keep_temp: parsed.flag("--keep-temp"),
                jobs,
                timeout,
                coverage,
                api,
                coverage_json_out: parsed.value("--cov-json").map(str::to_string),
            },
        })
    })
}

fn parse_ast(args: &[String]) -> Result<Command, String> {
    parse_command("ast", args, |parsed| match parsed.positionals.as_slice() {
        [] => Err("`xsht ast` requires SCRIPT".to_string()),
        [script] => Ok(Command::Ast {
            script: script.clone(),
        }),
        _ => Err("`xsht ast` accepts exactly one SCRIPT".to_string()),
    })
}

fn parse_api(args: &[String]) -> Result<Command, String> {
    parse_command("api", args, |parsed| {
        let mut queries = Vec::new();
        let mut summary = false;
        for positional in &parsed.positionals {
            if positional != "summary" {
                queries.push(positional.clone());
            } else if summary {
                return Err("`xsht api` accepts `summary` at most once".to_string());
            } else {
                summary = true;
            }
        }
        let format = match parsed.value("--format") {
            Some(value) => parse_api_format(value)?,
            None => ApiFormat::Text,
        };
        let details = parsed
            .value("--details")
            .map(parse_api_details)
            .transpose()?;
        let query_files: Vec<String> = parsed
            .values("--query-file")
            .into_iter()
            .map(str::to_string)
            .collect();
        if query_files.iter().any(String::is_empty) {
            return Err("`xsht api --query-file` requires PATH".to_string());
        }
        let read_stdin = parsed.flag("--stdin");

        if summary && (!queries.is_empty() || !query_files.is_empty() || read_stdin) {
            return Err(
                "`xsht api summary` cannot be combined with selectors or query inputs".to_string(),
            );
        }

        Ok(Command::Api {
            options: ApiOptions {
                summary,
                queries,
                query_files,
                read_stdin,
                format,
                strict: parsed.flag("--strict"),
                details,
            },
        })
    })
}

fn parse_api_format(value: &str) -> Result<ApiFormat, String> {
    match value {
        "text" => Ok(ApiFormat::Text),
        "jsonl" => Ok(ApiFormat::Jsonl),
        _ => Err("`xsht api --format` must be text or jsonl".to_string()),
    }
}

fn parse_api_details(value: &str) -> Result<ApiDetails, String> {
    match value {
        "basic" => Ok(ApiDetails::Basic),
        "full" => Ok(ApiDetails::Full),
        _ => Err("`xsht api --details` must be basic or full".to_string()),
    }
}

fn parse_trace(args: &[String]) -> Result<Command, String> {
    parse_command("trace", args, |parsed| {
        let format = match parsed.value("--trace-format") {
            None | Some("text") => TraceFormat::Text,
            Some("jsonl") => TraceFormat::Jsonl,
            Some("flamegraph") => TraceFormat::Flamegraph,
            Some(_) => {
                return Err("`--trace-format` must be `text`, `jsonl`, or `flamegraph`".to_string());
            }
        };
        let top_syscalls = match parsed.value("--trace-top-syscalls") {
            None => 8,
            Some(value) => match value.parse::<usize>() {
                Ok(n) if n > 0 => n,
                _ => return Err("`--trace-top-syscalls` must be a positive integer".to_string()),
            },
        };
        let Some((script, script_args)) = parsed.positionals.split_first() else {
            return Err("`xsht trace` requires SCRIPT".to_string());
        };
        let script_args = match script_args {
            [separator, rest @ ..] if separator == "--" => rest,
            all => all,
        };
        Ok(Command::Trace {
            options: TraceOptions {
                script: script.clone(),
                args: script_args.to_vec(),
                raw: parsed.flag("--raw"),
                format,
                file: parsed.value("--trace-file").map(str::to_string),
                syscalls: parsed.flag("--syscalls"),
                top_syscalls,
            },
        })
    })
}

fn parse_lint(args: &[String]) -> Result<Command, String> {
    parse_command("lint", args, |parsed| {
        let mut only: Option<Vec<String>> = None;
        for selection in parsed.values("--only") {
            for code in selection.split(',') {
                if !crate::xsht::lint::lint_code_known(code) {
                    return Err(format!("unknown lint rule '{code}' for `xsht lint --only`"));
                }
                only.get_or_insert_with(Vec::new).push(code.to_owned());
            }
        }
        let fix = parsed.flag("--fix");
        let runless = parsed.flag("--runless");
        let format = parsed.value("--format");

        if parsed.flag("--list") {
            if fix || runless || only.is_some() || !parsed.positionals.is_empty() {
                return Err("`xsht lint --list` accepts only --format".to_string());
            }
            return Ok(Command::Text(lint_code_list(format.unwrap_or("text"))?));
        }
        if format.is_some() {
            return Err("`xsht lint --format` requires --list".to_string());
        }

        Ok(Command::Lint {
            files: parsed.positionals,
            fix,
            runless,
            only,
        })
    })
}

/// `xsht lint --list`: one selectable code and its summary per line.
fn lint_code_list(format: &str) -> Result<String, String> {
    let catalog = crate::xsht::lint::lint_code_catalog();
    let lines = match format {
        "text" => {
            let width = crate::xsht::lint::lint_code_catalog()
                .map(|(code, _)| code.len())
                .max()
                .unwrap_or(0);
            catalog
                .map(|(code, summary)| format!("{code:width$}  {summary}\n"))
                .collect()
        }
        "jsonl" => catalog
            .map(|(code, summary)| {
                format!(
                    "{{\"code\":{},\"summary\":{}}}\n",
                    miniserde::json::to_string(code),
                    miniserde::json::to_string(summary)
                )
            })
            .collect(),
        other => {
            return Err(format!(
                "unsupported `xsht lint --format` '{other}'; use text or jsonl"
            ));
        }
    };
    Ok(lines)
}

fn parse_fmt(args: &[String]) -> Result<Command, String> {
    parse_command("fmt", args, |parsed| {
        Ok(Command::Fmt {
            check: parsed.flag("--check"),
            files: parsed.positionals,
        })
    })
}

fn parse_grep(args: &[String]) -> Result<Command, String> {
    parse_command("grep", args, |parsed| {
        let mut positionals = parsed.positionals.into_iter();
        let pattern = positionals
            .next()
            .ok_or_else(|| "`xsht grep` requires PATTERN".to_string())?;
        Ok(Command::Grep {
            pattern,
            files: positionals.collect(),
        })
    })
}

fn parse_refactor(args: &[String]) -> Result<Command, String> {
    parse_command("refactor", args, |parsed| {
        let dry_run = parsed.flag("--dry-run");
        let mut positionals = parsed.positionals.into_iter();
        let pattern = positionals
            .next()
            .ok_or_else(|| "`xsht refactor` requires PATTERN".to_string())?;
        let replacement = positionals
            .next()
            .ok_or_else(|| "`xsht refactor` requires REPLACEMENT".to_string())?;
        Ok(Command::Refactor {
            pattern,
            replacement,
            files: positionals.collect(),
            dry_run,
        })
    })
}

fn finish_command(run: impl FnOnce() -> CliOutput) -> ExitCode {
    finish(run())
}

fn finish(output: CliOutput) -> ExitCode {
    use std::io::Write;

    let _ = std::io::stdout().lock().write_all(&output.stdout);
    let _ = std::io::stderr().lock().write_all(&output.stderr);
    if !output.trace_text.is_empty() {
        eprint!("{}", output.trace_text);
    }
    ExitCode::from(output.status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declared_command_is_dispatched_to_its_own_parser() {
        for command in commands::COMMANDS {
            let parsed = parse_tool(vec![command.name.to_string(), "--help".to_string()])
                .unwrap_or_else(|error| panic!("`xsht {} --help`: {error}", command.name));
            let Command::Help(text) = parsed else {
                panic!("`xsht {} --help` did not print help", command.name);
            };
            assert_eq!(Some(text), generated_command_help(command.name));
        }
    }
}
