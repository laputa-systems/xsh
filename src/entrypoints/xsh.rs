use std::process::ExitCode;
use std::ffi::{OsStr, OsString};
use xsh::execution::script::{
    RunOptions, ScriptOutput, run_script_with_shared_stdio_and_argv_bytes, run_startup,
};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use xsh::process::{clear_cancellation_request, install_cancellation_signal_handlers};

const HELP: &str = "\
xsh 0.0.1

Usage:
  xsh SCRIPT [ARGS...]
  xsh -- SCRIPT ARGS...
  xsh --startup
  xsh --help

--startup boots the interpreter and exits immediately, running no program. It
exposes the fixed startup cost for benchmarking (e.g. as a calibration baseline).
Use `--` between SCRIPT and ARGS when the script path or first argument could be
ambiguous; `xsh SCRIPT -- ARGS...` is also accepted.
";

pub fn main() -> ExitCode {
    let _signal_guard = match install_cancellation_signal_handlers() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("xsh: failed to install signal handlers: {error}");
            return ExitCode::from(2);
        }
    };
    clear_cancellation_request();

    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let text_args: Vec<String> = args
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();

    if text_args.first().map(String::as_str) == Some("--startup") {
        return finish(run_startup());
    }

    match parse_run(args) {
        Ok(None) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Some((options, argv_bytes))) => {
            let output = run_script_with_shared_stdio_and_argv_bytes(options, argv_bytes);
            finish(output)
        }
        Err(message) => {
            eprintln!("xsh: {message}");
            ExitCode::from(2)
        }
    }
}

fn parse_run(args: Vec<OsString>) -> Result<Option<(RunOptions, Vec<Vec<u8>>)>, String> {
    let text_args: Vec<String> = args
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    if args.is_empty()
        || matches!(
            text_args.first().map(String::as_str),
            Some("--help" | "-h")
        )
    {
        return Ok(None);
    }

    if let Some(arg) = text_args.first() {
        match arg.as_str() {
            "--" => {
                let script_index = 1;
                let script = args
                    .get(script_index)
                    .and_then(|script| script.to_str())
                    .ok_or_else(|| "SCRIPT is required after `--` and must be valid UTF-8".to_string())?
                    .to_string();
                let argument_start = script_index + 1;
                let raw_args = args[argument_start..]
                    .iter()
                    .map(|argument| os_str_bytes(argument))
                    .collect();
                return Ok(Some((
                    RunOptions {
                        script,
                        args: text_args[argument_start..].to_vec(),
                        coverage_trace_dir: None,
                    },
                    raw_args,
                )));
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
            other if other.starts_with('-') => {
                return Err(format!("unknown xsh option '{other}'"));
            }
            _ => {}
        }
    }
    if let Some(arg) = text_args.first()
        && matches!(
            arg.as_str(),
            "run" | "check" | "fmt" | "lint" | "ast" | "trace"
        )
    {
        return Err("xsh does not take subcommands; use xsht for tools".to_string());
    }

    let script_index = 0;
    let script = args
        .get(script_index)
        .and_then(|script| script.to_str())
        .ok_or_else(|| "SCRIPT is required and must be valid UTF-8".to_string())?
        .to_string();
    let argument_start = if matches!(text_args.get(1).map(String::as_str), Some("--")) {
        2
    } else {
        1
    };
    let raw_args = args[argument_start..]
        .iter()
        .map(|argument| os_str_bytes(argument))
        .collect();

    Ok(Some((
        RunOptions {
            script,
            args: text_args[argument_start..].to_vec(),
            coverage_trace_dir: None,
        },
        raw_args,
    )))
}

#[cfg(unix)]
fn os_str_bytes(argument: &OsStr) -> Vec<u8> {
    argument.as_bytes().to_vec()
}

#[cfg(not(unix))]
fn os_str_bytes(argument: &OsStr) -> Vec<u8> {
    argument.to_string_lossy().as_bytes().to_vec()
}

fn trace_moved_message() -> String {
    "trace options moved to `xsht trace`; run `xsht trace SCRIPT [ARGS...]`".to_string()
}

fn finish(output: ScriptOutput) -> ExitCode {
    use std::io::Write;

    let _ = std::io::stdout().lock().write_all(&output.stdout);
    let _ = std::io::stderr().lock().write_all(&output.stderr);
    ExitCode::from(output.status)
}
