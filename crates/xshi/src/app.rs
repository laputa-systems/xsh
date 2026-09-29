use std::os::unix::ffi::OsStrExt;
use std::process::ExitCode;

use crate::xshi::interactive;
use xsh::process::{
    clear_cancellation_request, install_cancellation_signal_handlers,
    install_interactive_signal_handlers,
};

const HELP: &str = "\
xshi 0.0.1 \u{2014} interactive shell for XSH

usage: xshi [-c COMMAND] [--config PATH] [--no-config] [-V|--version] [-h|--help]

options:
  -c COMMAND      Run COMMAND and exit
  --config PATH   Use PATH instead of ~/.config/xshi/config.ish
  --no-config     Skip loading the config file
  -V, --version   Show version
  -h, --help      Show this help message
";

const REFUSAL: &str = "\
xshi: this shell is interactive-only and does not run scripts
usage: xshi [-c COMMAND] [--config PATH] [--no-config] [-V|--version] [-h|--help]
";

pub fn main() -> ExitCode {
    let login_shell = login_shell_argv0();
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
            eprintln!("xshi: {message}");
            return ExitCode::from(2);
        }
    };

    match parse_interactive(args) {
        Ok(Command::Help) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("xshi {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Refuse) => {
            eprint!("{REFUSAL}");
            ExitCode::from(1)
        }
        Ok(Command::Run { config }) => {
            let _signal_guard = match install_interactive_signal_handlers() {
                Ok(guard) => guard,
                Err(error) => {
                    eprintln!("xshi: failed to install signal handlers: {error}");
                    return ExitCode::from(2);
                }
            };
            clear_cancellation_request();
            let allow_non_tty = std::env::var_os("XSHI_ALLOW_NON_TTY_FOR_TESTS").is_some();
            // `exit N` hands the value to exit(3) unchanged; the kernel keeps
            // its low eight bits, as a script that calls exit(N) would see.
            std::process::exit(interactive::run_with_options(interactive::RunOptions {
                load_config: config.loads_default(),
                config_path: match config {
                    ConfigChoice::Path(path) => Some(path),
                    _ => None,
                },
                load_profile: true,
                require_tty: !allow_non_tty,
            }))
        }
        Ok(Command::Eval { source, config }) => {
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
                    load_config: config.loads_default(),
                    config_path: match config {
                        ConfigChoice::Path(path) => Some(path),
                        _ => None,
                    },
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
    Version,
    Run { config: ConfigChoice },
    Eval { source: String, config: ConfigChoice },
    Refuse,
}

/// Which configuration file to load.
#[derive(Clone)]
enum ConfigChoice {
    Default,
    None,
    Path(std::path::PathBuf),
}

impl ConfigChoice {
    fn loads_default(&self) -> bool {
        matches!(self, Self::Default)
    }
}

#[allow(clippy::single_call_fn)]
fn parse_interactive(args: Vec<String>) -> Result<Command, String> {
    // Strip `-- ARG0` prefix: docker passes `-- xshi [...]` when the image CMD
    // is `xshi [...]`; ARG0 is the script name / $0 and is not an xshi option.
    let args = match args.as_slice() {
        [sep, _arg0, rest @ ..] if sep == "--" => rest.to_vec(),
        _ => args,
    };
    let mut config = ConfigChoice::Default;
    let mut command = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--no-config" => config = ConfigChoice::None,
            "--config" => match iter.next() {
                Some(path) => config = ConfigChoice::Path(path.into()),
                None => return Err("--config requires a config file path".to_string()),
            },
            "-c" => match iter.next() {
                Some(source) => command = Some(source),
                None => return Err("-c requires a command".to_string()),
            },
            _ => return Ok(Command::Refuse),
        }
    }
    Ok(match command {
        Some(source) => Command::Eval { source, config },
        None => Command::Run { config },
    })
}
