use std::io::{self, Write as _};
use std::os::fd::AsFd as _;
use std::path::PathBuf;

use xsh_root::transfer::{
    EngineOptions, Jobs, SyncOperands, SyncOptions, SyncProgress, SyncRequest, sync_tree,
};

use super::fs_command::{Foreground, open_directory};
use super::rmrf::FileContext;

const SYNC_HELP: &str = "Usage: rsync [OPTIONS] SOURCE... DIRECTORY
       rsync [OPTIONS] --into DIRECTORY SOURCE...
       rsync [OPTIONS] --contents SOURCE DIRECTORY
       rsync [OPTIONS] SOURCE DEST

Synchronize files and directories locally.

Options:
      --into DIRECTORY     synchronize sources into target directory
      --contents           synchronize directory contents without creating top directory
      --delete             delete extraneous files from destination
      --dry-run            perform a trial run with no changes made
  -j, --jobs N             number of parallel transfer jobs (1..=32)
      --no-progress        suppress progress output
      --verbose            increase verbosity
  -h, --help               display this help and exit
";

const SYNC_USAGE: &str = "Usage: rsync [OPTIONS] SOURCE... DIRECTORY
       rsync [OPTIONS] --into DIRECTORY SOURCE...
       rsync [OPTIONS] --contents SOURCE DIRECTORY
       rsync [OPTIONS] SOURCE DEST";

pub(super) struct Invocation {
    operands: SyncOperands,
    delete: bool,
    dry_run: bool,
    jobs: Option<Jobs>,
    verbose: bool,
}

pub(super) enum Parsed {
    Help,
    Run(Invocation),
}

#[derive(Debug)]
pub(super) struct SyntaxError(String);

pub(super) fn parse(arguments: &[String], context: &FileContext) -> Result<Parsed, SyntaxError> {
    let mut delete = false;
    let mut dry_run = false;
    let mut jobs = None;
    let mut verbose = false;
    let mut contents = false;
    let mut into = None;
    let mut operands: Vec<PathBuf> = Vec::new();
    let mut options_enabled = true;
    let mut jobs_seen = false;
    let mut help = false;
    let mut index = 0;

    while index < arguments.len() {
        let argument = arguments[index].as_str();
        index += 1;
        if options_enabled && argument == "--" {
            options_enabled = false;
            continue;
        }
        if options_enabled && matches!(argument, "-h" | "--help") {
            help = true;
            continue;
        }
        if !options_enabled || !is_option(argument) {
            operands.push(PathBuf::from(argument));
            continue;
        }

        match argument {
            "--into" => {
                if into.is_some() {
                    return Err(SyntaxError("--into may be used only once".to_owned()));
                }
                let Some(directory) = arguments.get(index) else {
                    return Err(SyntaxError("--into requires a directory".to_owned()));
                };
                into = Some(PathBuf::from(directory));
                index += 1;
            }
            "--contents" => contents = true,
            "--delete" => delete = true,
            "--dry-run" => dry_run = true,
            "-n" | "--no-clobber" => {
                return Err(SyntaxError("-n is not supported; use --dry-run".to_owned()));
            }
            "-j" | "--jobs" => {
                if jobs_seen {
                    return Err(SyntaxError("--jobs may be set only once".to_owned()));
                }
                let Some(value) = arguments.get(index) else {
                    return Err(SyntaxError("--jobs requires a positive integer".to_owned()));
                };
                let count = value
                    .parse::<usize>()
                    .map_err(|_| SyntaxError("--jobs requires a positive integer".to_owned()))?;
                jobs = Some(
                    Jobs::new(count)
                        .map_err(|_| SyntaxError("--jobs must be in 1..=32".to_owned()))?,
                );
                jobs_seen = true;
                index += 1;
            }
            "--verbose" => verbose = true,
            "--no-progress" => {}
            _ => {
                return Err(SyntaxError(format!("unrecognized option '{argument}'")));
            }
        }
    }

    if help {
        return Ok(Parsed::Help);
    }
    if into.is_some() && contents {
        return Err(SyntaxError(
            "--into and --contents cannot be combined".to_owned(),
        ));
    }

    let operands = if let Some(directory) = into {
        if operands.is_empty() {
            return Err(SyntaxError("missing source operand".to_owned()));
        }
        SyncOperands::Into {
            directory,
            sources: operands,
        }
    } else if contents {
        if operands.len() != 2 {
            return Err(SyntaxError(
                "--contents requires exactly one source directory and one target directory"
                    .to_owned(),
            ));
        }
        let mut operands = operands.into_iter();
        SyncOperands::Contents {
            source: operands.next().unwrap(),
            directory: operands.next().unwrap(),
        }
    } else {
        if operands.len() < 2 {
            return Err(SyntaxError("missing destination operand".to_owned()));
        }
        let target = operands.last().expect("at least two operands").clone();
        let target_is_directory = is_directory(&context.cwd, &target);
        if operands.len() == 2 && !target_is_directory {
            let mut operands = operands.into_iter();
            SyncOperands::Exact {
                source: operands.next().unwrap(),
                destination: operands.next().unwrap(),
            }
        } else {
            if !target_is_directory {
                return Err(SyntaxError(format!(
                    "target '{}' is not a directory",
                    target.display()
                )));
            }
            let directory = operands.pop().expect("at least two operands");
            SyncOperands::Into {
                directory,
                sources: operands,
            }
        }
    };

    Ok(Parsed::Run(Invocation {
        operands,
        delete,
        dry_run,
        jobs,
        verbose,
    }))
}

fn is_option(argument: &str) -> bool {
    argument.as_bytes().starts_with(b"-") && argument.len() > 1
}

fn is_directory(cwd: &std::path::Path, target: &std::path::Path) -> bool {
    let path = if target.is_absolute() {
        target.to_path_buf()
    } else {
        cwd.join(target)
    };
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

pub(super) fn run(
    arguments: &[String],
    context: &FileContext,
    stderr: &mut Vec<u8>,
) -> i32 {
    let invocation = match parse(arguments, context) {
        Ok(Parsed::Help) => {
            stderr.extend_from_slice(SYNC_HELP.as_bytes());
            return 0;
        }
        Ok(Parsed::Run(invocation)) => invocation,
        Err(SyntaxError(message)) => {
            let _ = writeln!(stderr, "xshi: rsync: {message}\n{SYNC_USAGE}\n");
            return 2;
        }
    };
    let mut foreground = match Foreground::install(invocation.verbose, true) {
        Ok(foreground) => foreground,
        Err(error) => {
            let _ = writeln!(stderr, "xshi: rsync: {error}");
            return 1;
        }
    };
    foreground.set_progress_label("rsync");
    let result = execute(&invocation, context, &mut foreground);
    let _ = foreground.finish();
    if let Some(signal) = foreground.observed_interrupt() {
        if signal == libc::SIGTERM || signal == libc::SIGHUP {
            foreground.terminate(signal);
        }
        let _ = writeln!(stderr, "xshi: rsync: cancelled");
        return 130;
    }
    if foreground.output_failed() {
        let _ = writeln!(stderr, "xshi: rsync: progress output failed");
        return 1;
    }
    match result {
        Ok(progress) => {
            if invocation.verbose {
                report(&progress, invocation.delete, stderr);
            }
            0
        }
        Err(error) => {
            let _ = writeln!(stderr, "xshi: rsync: {error}");
            if error.kind() == io::ErrorKind::InvalidInput {
                2
            } else {
                1
            }
        }
    }
}

fn execute(
    invocation: &Invocation,
    context: &FileContext,
    foreground: &mut Foreground,
) -> io::Result<SyncProgress> {
    let cwd = open_directory(&context.cwd)?;
    sync_tree(SyncRequest {
        cwd: cwd.as_fd(),
        operands: invocation.operands.clone(),
        options: SyncOptions {
            delete: invocation.delete,
            dry_run: invocation.dry_run,
            engine: EngineOptions {
                jobs: invocation.jobs,
                ..EngineOptions::default()
            },
        },
        cancellation: foreground.cancellation(),
    })
}

fn report(progress: &SyncProgress, delete: bool, stderr: &mut Vec<u8>) {
    let line = if delete && progress.deleted_files > 0 {
        format!(
            "xshi: rsync: {} examined, {} transferred, {} skipped, {} deleted, {} bytes\n",
            progress.examined_files,
            progress.transferred_files,
            progress.skipped_files,
            progress.deleted_files,
            progress.streamed_bytes
        )
    } else {
        format!(
            "xshi: rsync: {} examined, {} transferred, {} skipped, {} bytes\n",
            progress.examined_files,
            progress.transferred_files,
            progress.skipped_files,
            progress.streamed_bytes
        )
    };
    stderr.extend_from_slice(line.as_bytes());
}
