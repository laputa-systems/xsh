use std::process::ExitCode;
use std::ffi::OsString;
use xsh::execution::script::{
    RunOptions, ScriptOutput, on_preparation_stack, run_script_with_shared_stdio, run_startup,
};
use xsh::process::{clear_cancellation_request, install_cancellation_signal_handlers};

struct OptionSpec {
    names: &'static [&'static str],
    description: &'static str,
}

const HELP: OptionSpec = OptionSpec { names: &["-h", "--help"], description: "Print this reference and exit" };
const STARTUP: OptionSpec = OptionSpec { names: &["--startup"], description: "Boot the interpreter and exit without running a program" };
const EXAMPLES: &[&[&str]] = &[&["hello.xsh"], &["hello.xsh", "--", "word"]];

fn help() -> String {
    let mut text = format!("xsh {}\n\nUsage:\n  xsh SCRIPT [ARGS...]\n  xsh -- SCRIPT [ARGS...]\n  xsh SCRIPT -- [ARGS...]\n  xsh --startup\n\nArguments:\n  SCRIPT    Required script path; no default\n  ARGS...   Optional, repeated script arguments\n\nOptions:\n", env!("CARGO_PKG_VERSION"));
    for option in [&HELP, &STARTUP] {
        text.push_str(&format!("  {:<16} {}\n", option.names.join(", "), option.description));
    }
    text.push_str("\nUse -- before a script path or first script argument that could be ambiguous.\nRun xsht --help for checking, testing, tracing and other tooling.\n\nExamples:\n");
    for example in EXAMPLES {
        text.push_str(&format!("  xsh {}\n", example.join(" ")));
    }
    text.push('\n');
    text.push_str(include_str!("owned-help.txt"));
    text
}

pub fn main() -> ExitCode {
    clear_cancellation_request();
    let _signal_guard = match install_cancellation_signal_handlers() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("xsh: failed to install signal handlers: {error}");
            return ExitCode::from(2);
        }
    };
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    if args.first().and_then(|word| word.to_str()) == Some(STARTUP.names[0]) {
        if let Err(message) = validate_text_arguments(&args) {
            eprintln!("xsh: {message}");
            return ExitCode::from(2);
        }
        return finish(run_startup());
    }

    match parse_run(args) {
        Ok(None) => {
            print!("{}", help());
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
    if args.is_empty() || args.first().and_then(|word| word.to_str()).is_some_and(|word| HELP.names.contains(&word)) {
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

#[cfg(test)]
mod help_tests {
    use super::*;

    #[test]
    fn examples_parse_without_running_scripts() {
        for example in EXAMPLES {
            assert!(matches!(parse_run(example.iter().map(OsString::from).collect()), Ok(Some(_))));
        }
    }
}
