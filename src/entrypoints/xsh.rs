use std::process::ExitCode;
use std::ffi::{OsStr, OsString};
use xsh::execution::script::{
    RunOptions, ScriptOutput, run_script_with_shared_stdio_and_argv_bytes, run_startup,
};

#[cfg(unix)]
use std::{
    os::unix::{ffi::OsStrExt, fs::PermissionsExt, process::CommandExt},
    path::Path,
    process::Command,
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
Use `xsh -- SCRIPT ARGS...` when SCRIPT begins with `-`.
";

pub fn main() -> ExitCode {
    let process_args: Vec<OsString> = std::env::args_os().collect();
    #[cfg(unix)]
    reset_fatal_signal_handlers();

    #[cfg(unix)]
    if let Some(status) = dispatch_uutils(&process_args) {
        return status;
    }

    let _signal_guard = match install_cancellation_signal_handlers() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("xsh: failed to install signal handlers: {error}");
            return ExitCode::from(2);
        }
    };
    clear_cancellation_request();

    let args: Vec<OsString> = process_args.into_iter().skip(1).collect();
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

#[cfg(unix)]
fn reset_fatal_signal_handlers() {
    unsafe {
        libc::signal(libc::SIGBUS, libc::SIG_DFL);
        libc::signal(libc::SIGSEGV, libc::SIG_DFL);
    }
}

#[cfg(unix)]
fn dispatch_uutils(args: &[OsString]) -> Option<ExitCode> {
    let invoked = Path::new(args.first()?).file_name()?;
    if invoked != OsStr::new("xsh-uutests") {
        return None;
    }

    Some(dispatch_uutils_applet(args))
}

#[cfg(unix)]
fn dispatch_uutils_applet(args: &[OsString]) -> ExitCode {
    let Some(invoked_as) = args.first() else {
        return ExitCode::from(2);
    };
    let Some(utility) = args.get(1) else {
        eprintln!("usage: xsh-uutests UTILITY [ARG...]");
        return ExitCode::from(2);
    };
    let utility_bytes = utility.as_os_str().as_bytes();
    if utility_bytes.is_empty()
        || utility_bytes.contains(&b'/')
        || utility.as_os_str() == OsStr::new(".")
        || utility.as_os_str() == OsStr::new("..")
    {
        eprintln!(
            "xsh-uutests: invalid utility name '{}'",
            utility.to_string_lossy()
        );
        return ExitCode::from(2);
    }

    let stage = Path::new(invoked_as).parent().unwrap_or_else(|| Path::new("."));
    let applet = stage.join("bin").join(utility);
    let executable = std::fs::metadata(&applet).is_ok_and(|metadata| {
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    });
    if !executable {
        eprintln!(
            "xsh-uutests: XSH provides no applet for '{}'",
            utility.to_string_lossy()
        );
        return ExitCode::from(127);
    }

    apply_uutils_memory_limit(stage);

    let phrase = format!("{} {}", invoked_as.to_string_lossy(), utility.to_string_lossy());
    let mut command = Command::new(applet);
    command
        .args(args.iter().skip(2))
        .env("XSH_EXECUTION_PHRASE", phrase);
    let error = command.exec();
    eprintln!(
        "xsh-uutests: failed to execute '{}': {error}",
        utility.to_string_lossy()
    );
    ExitCode::from(126)
}

#[cfg(all(unix, target_os = "linux"))]
fn apply_uutils_memory_limit(stage: &Path) {
    let limit_path = stage.join("mem-limit-kb");
    let Ok(value) = std::fs::read_to_string(limit_path) else {
        return;
    };
    let value = value.trim();
    if value == "unlimited" {
        return;
    }
    let Ok(kibibytes) = value.parse::<u64>() else {
        return;
    };
    let bytes = kibibytes.saturating_mul(1024) as libc::rlim_t;
    let limit = libc::rlimit {
        rlim_cur: bytes,
        rlim_max: bytes,
    };
    // The shell adapter treated a rejected `ulimit -v` as best effort.
    unsafe {
        let _ = libc::setrlimit(libc::RLIMIT_AS, &limit);
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn apply_uutils_memory_limit(_stage: &Path) {}

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
    let argument_start = 1;
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
