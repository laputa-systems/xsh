use std::os::unix::ffi::OsStrExt;
use std::process::ExitCode;

use crate::xshi::interactive;
use xsh::runtime::process::{
    clear_cancellation_request, install_cancellation_signal_handlers,
    install_interactive_signal_handlers,
};

const HELP: &str = "\
xshi 0.0.1

Usage:
  xshi
  xshi -c COMMAND
  xshi --no-config
  xshi --no-config -c COMMAND
  xshi --help

Options:
  --help, -h              Show this help.
  --no-config             Start without ~/.config/xshi/config.xsh.
  -c COMMAND              Execute COMMAND and exit.
";

pub fn main() -> ExitCode {
    let login_shell = login_shell_argv0();
    match parse_interactive(std::env::args().skip(1).collect()) {
        Ok(Command::Help) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Command::Run { load_config }) => {
            let _signal_guard = match install_interactive_signal_handlers() {
                Ok(guard) => guard,
                Err(error) => {
                    eprintln!("xshi: failed to install signal handlers: {error}");
                    return ExitCode::from(2);
                }
            };
            clear_cancellation_request();
            let allow_non_tty = std::env::var_os("XSHI_ALLOW_NON_TTY_FOR_TESTS").is_some();
            ExitCode::from(interactive::run_with_options(interactive::RunOptions {
                load_config,
                load_profile: true,
                require_tty: !allow_non_tty,
            }) as u8)
        }
        Ok(Command::Eval {
            source,
            load_config,
        }) => {
            let _signal_guard = match install_cancellation_signal_handlers() {
                Ok(guard) => guard,
                Err(error) => {
                    eprintln!("xshi: failed to install signal handlers: {error}");
                    return ExitCode::from(2);
                }
            };
            clear_cancellation_request();
            ExitCode::from(interactive::run_one_command_with_options(
                &source,
                interactive::OneCommandOptions {
                    load_config,
                    load_profile: login_shell,
                },
            ) as u8)
        }
        Err(message) => {
            eprintln!("xshi: {message}");
            ExitCode::from(2)
        }
    }
}

fn login_shell_argv0() -> bool {
    std::env::args_os()
        .next()
        .is_some_and(|arg0| arg0.as_os_str().as_bytes().starts_with(b"-"))
}

enum Command {
    Help,
    Run { load_config: bool },
    Eval { source: String, load_config: bool },
}

#[allow(clippy::single_call_fn)]
fn parse_interactive(args: Vec<String>) -> Result<Command, String> {
    // Strip `-- ARG0` prefix: docker passes `-- xshi [...]` when the image CMD
    // is `xshi [...]`; ARG0 is the script name / $0 and is not an xshi option.
    let args = match args.as_slice() {
        [sep, _arg0, rest @ ..] if sep == "--" => rest.to_vec(),
        _ => args,
    };
    match args.as_slice() {
        [] => Ok(Command::Run { load_config: true }),
        [arg] if arg == "--help" || arg == "-h" => Ok(Command::Help),
        [arg] if arg == "--no-config" => Ok(Command::Run { load_config: false }),
        [flag, source] if flag == "-c" => Ok(Command::Eval {
            source: source.clone(),
            load_config: true,
        }),
        [no_config, flag, source] if no_config == "--no-config" && flag == "-c" => {
            Ok(Command::Eval {
                source: source.clone(),
                load_config: false,
            })
        }
        [arg] => Err(format!("unexpected argument '{arg}'")),
        _ => Err("xshi does not accept script paths or script arguments".to_string()),
    }
}
