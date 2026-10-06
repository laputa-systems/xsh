use std::process::ExitCode;
use std::ffi::OsString;
use xsh::execution::script::{
    RunOptions, ScriptOutput, on_preparation_stack, run_script_with_shared_stdio, run_startup,
};
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

    if args.first().and_then(|word| word.to_str()) == Some("--startup") {
        if let Err(message) = validate_text_arguments(&args) {
            eprintln!("xsh: {message}");
            return ExitCode::from(2);
        }
        return finish(run_startup());
    }

    match parse_run(args) {
        Ok(None) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Some(options)) => {
            let output = on_preparation_stack(move || run_script_with_shared_stdio(options));
            finish(output)
        }
        Err(message) => {
            eprintln!("xsh: {message}");
            ExitCode::from(2)
        }
    }
}

fn parse_run(args: Vec<OsString>) -> Result<Option<RunOptions>, String> {
    if args.is_empty() || matches!(args.first().and_then(|word| word.to_str()), Some("--help" | "-h")) {
        validate_text_arguments(&args)?;
        return Ok(None);
    }

    let mut index = 0;
    if let Some(arg) = args.get(index) {
        let text = arg.to_str().ok_or_else(|| format!("argument {} is not valid UTF-8", index + 1))?;
        match text {
            "--" => {}
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
    if matches!(args.get(index).and_then(|word| word.to_str()), Some("--")) {
        index += 1;
        let script = args
            .get(index)
            .ok_or_else(|| "SCRIPT is required after `--`".to_string())?
            .to_str().ok_or_else(|| format!("argument {} is not valid UTF-8", index + 1))?
            .to_owned();
        index += 1;
        return Ok(Some(RunOptions {
            script,
            args: args[index..].to_vec(),
            coverage_trace_dir: None,
        }));
    }

    let script = args
        .get(index)
        .ok_or_else(|| "SCRIPT is required".to_string())?
        .to_str().ok_or_else(|| format!("argument {} is not valid UTF-8", index + 1))?
        .to_owned();
    index += 1;
    let script_args = if matches!(args.get(index).and_then(|word| word.to_str()), Some("--")) {
        args[index + 1..].to_vec()
    } else {
        args[index..].to_vec()
    };

    Ok(Some(RunOptions {
        script,
        args: script_args,
        coverage_trace_dir: None,
    }))
}

fn validate_text_arguments(arguments: &[OsString]) -> Result<(), String> {
    for (index, argument) in arguments.iter().enumerate() {
        if argument.to_str().is_none() {
            return Err(format!("argument {} is not valid UTF-8", index + 1));
        }
    }
    Ok(())
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
