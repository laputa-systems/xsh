//! The `rsync` builtin: ish's local synchronization grammar over XSH's
//! internal transfer engine.

use std::io::{self, Write as _};
use std::os::fd::AsFd as _;
use std::path::{Path, PathBuf};

use rustix::fs::{self as rfs, Mode, OFlags};
use xsh_root::transfer::{
    CancelReason, EngineOptions, Jobs, SyncOperands, SyncOptions, SyncProgress, SyncRequest,
    sync_tree,
};

use super::foreground::{self, Foreground, Streams};
use super::fs_command::FileContext;

const USAGE: &str = "Usage: rsync [OPTIONS] SOURCE... DIRECTORY
       rsync [OPTIONS] --into DIRECTORY SOURCE...
       rsync [OPTIONS] --contents SOURCE DIRECTORY
       rsync [OPTIONS] SOURCE DEST";

/// The `--help` text of `rsync`.
pub(super) fn help() -> String {
    format!(
        "{USAGE}\n\nSynchronize files and directories locally.\n\nOptions:\n      --into DIRECTORY     synchronize sources into target directory\n      --contents           synchronize directory contents without creating top directory\n      --delete             delete extraneous files from destination\n      --dry-run            perform a trial run with no changes made\n  -j, --jobs N             number of parallel transfer jobs (1..=32)\n      --no-progress        suppress progress output\n      --verbose            increase verbosity\n  -h, --help               display this help and exit\n"
    )
}

struct Invocation {
    operands: SyncOperands,
    options: SyncOptions,
    verbose: bool,
}

enum Parsed {
    Help,
    Run(Invocation),
}

#[derive(Debug)]
struct SyntaxError(String);

fn parse(arguments: &[String], context: &FileContext) -> Result<Parsed, SyntaxError> {
    let mut options = SyncOptions::default();
    let mut jobs: Option<Jobs> = None;
    let mut verbose = false;
    let mut contents = false;
    let mut into = None;
    let mut operands: Vec<PathBuf> = Vec::new();
    let mut options_enabled = true;
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
            "--delete" => options.delete = true,
            "--dry-run" => options.dry_run = true,
            "-n" | "--no-clobber" => {
                return Err(SyntaxError("-n is not supported; use --dry-run".to_owned()));
            }
            "-j" | "--jobs" => {
                if jobs.is_some() {
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
                index += 1;
            }
            "--verbose" => verbose = true,
            "--no-progress" => {}
            _ => return Err(SyntaxError(format!("unrecognized option '{argument}'"))),
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
    options.engine = EngineOptions {
        jobs,
        ..EngineOptions::default()
    };

    let operands = if let Some(directory) = into {
        if operands.is_empty() {
            return Err(SyntaxError("missing source operand".to_owned()));
        }
        SyncOperands::Into {
            directory,
            sources: operands,
        }
    } else if contents {
        if operands.is_empty() {
            return Err(SyntaxError("missing operand".to_owned()));
        }
        if operands.len() != 2 {
            return Err(SyntaxError(
                "--contents requires exactly one source directory and one target directory"
                    .to_owned(),
            ));
        }
        let directory = operands.pop().expect("two operands");
        let source = operands.pop().expect("two operands");
        SyncOperands::Contents { source, directory }
    } else {
        if operands.len() < 2 {
            return Err(SyntaxError("missing destination operand".to_owned()));
        }
        let target_is_directory = is_directory(&context.cwd, operands.last().expect("operands"));
        if operands.len() == 2 && !target_is_directory {
            let destination = operands.pop().expect("two operands");
            let source = operands.pop().expect("two operands");
            SyncOperands::Exact {
                source,
                destination,
            }
        } else {
            let directory = operands.pop().expect("two or more operands");
            if !target_is_directory {
                return Err(SyntaxError(format!(
                    "target '{}' is not a directory",
                    directory.display()
                )));
            }
            SyncOperands::Into {
                directory,
                sources: operands,
            }
        }
    };

    Ok(Parsed::Run(Invocation {
        operands,
        options,
        verbose,
    }))
}

fn is_option(argument: &str) -> bool {
    argument.starts_with('-') && argument.len() > 1
}

fn is_directory(cwd: &Path, target: &Path) -> bool {
    std::fs::metadata(cwd.join(target)).is_ok_and(|metadata| metadata.is_dir())
}

/// Runs `rsync` in the shell process and returns the shell status.
pub(super) fn run(arguments: &[String], context: &FileContext, streams: &mut Streams<'_>) -> i32 {
    let status = run_command(arguments, context, streams);
    foreground::exit_if_terminated(streams);
    status
}

fn run_command(arguments: &[String], context: &FileContext, streams: &mut Streams<'_>) -> i32 {
    let invocation = match parse(arguments, context) {
        Ok(Parsed::Help) => {
            return if streams.stdout.write_all(help().as_bytes()).is_ok() {
                0
            } else {
                1
            };
        }
        Ok(Parsed::Run(invocation)) => invocation,
        Err(SyntaxError(message)) => {
            let text = format!("xshi: rsync: {message}\n{USAGE}\n");
            return if streams.stderr.write_all(text.as_bytes()).is_ok() {
                2
            } else {
                1
            };
        }
    };
    let mut foreground = match Foreground::start(streams.progress_terminal()) {
        Ok(foreground) => foreground,
        Err(error) => {
            let _ = writeln!(streams.stderr, "xshi: rsync: {error}");
            return 1;
        }
    };
    let cancellation = foreground.cancellation();
    let result = foreground.run(
        |cancellation, _events: &foreground::EventSink<()>| {
            let cwd = rfs::open(&context.cwd, OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC, Mode::empty())
                .map_err(|error| {
                    io::Error::new(
                        io::Error::from(error).kind(),
                        format!(
                            "{}: cannot open working directory: {}",
                            context.cwd.display(),
                            io::Error::from(error)
                        ),
                    )
                })?;
            sync_tree(SyncRequest {
                cwd: cwd.as_fd(),
                operands: invocation.operands.clone(),
                options: invocation.options,
                cancellation,
            })
        },
        |_, ()| Ok(()),
        |_| Ok(()),
    );
    let finish = foreground.finish();
    drop(foreground);

    if cancellation.reason() == Some(CancelReason::User) {
        let _ = writeln!(streams.stderr, "xshi: rsync: cancelled");
        foreground::cancel_line();
        return 130;
    }
    let outcome = match (result, finish) {
        (Ok(outcome), Ok(())) => outcome,
        (Err(error), Err(finish_error)) => {
            let _ = writeln!(
                streams.stderr,
                "xshi: rsync: {error}; foreground cleanup failed: {finish_error}"
            );
            return 1;
        }
        (Err(error), Ok(())) | (Ok(_), Err(error)) => {
            let _ = writeln!(streams.stderr, "xshi: rsync: {error}");
            return 1;
        }
    };
    match outcome {
        Ok(progress) => {
            if invocation.verbose {
                let _ = streams
                    .stderr
                    .write_all(summary(&progress, invocation.options.delete).as_bytes());
            }
            0
        }
        Err(error) => {
            let _ = writeln!(streams.stderr, "xshi: rsync: {error}");
            if error.kind() == io::ErrorKind::InvalidInput {
                2
            } else {
                1
            }
        }
    }
}

fn summary(progress: &SyncProgress, delete: bool) -> String {
    if delete && progress.deleted_files > 0 {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(cwd: &Path) -> FileContext {
        FileContext {
            cwd: cwd.to_path_buf(),
            home: None,
            scratch: PathBuf::from("/tmp"),
        }
    }

    fn parse_error(arguments: &[&str]) -> String {
        let arguments: Vec<String> = arguments.iter().map(|argument| (*argument).to_owned()).collect();
        match parse(&arguments, &context(Path::new("/"))) {
            Err(SyntaxError(message)) => message,
            Ok(_) => panic!("{arguments:?} parsed"),
        }
    }

    #[test]
    fn syntax_errors_use_the_reference_wording() {
        assert_eq!(parse_error(&["-n", "a", "b"]), "-n is not supported; use --dry-run");
        assert_eq!(parse_error(&["--bogus", "a", "b"]), "unrecognized option '--bogus'");
        assert_eq!(parse_error(&["--contents"]), "missing operand");
        assert_eq!(
            parse_error(&["--contents", "a"]),
            "--contents requires exactly one source directory and one target directory"
        );
        assert_eq!(
            parse_error(&["--into", "a", "--contents", "b"]),
            "--into and --contents cannot be combined"
        );
        assert_eq!(parse_error(&["a"]), "missing destination operand");
        assert_eq!(parse_error(&["--into", "dir"]), "missing source operand");
        assert_eq!(
            parse_error(&["a", "b", "not-a-directory-here"]),
            "target 'not-a-directory-here' is not a directory"
        );
    }

    fn scratch(label: &str) -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.work/tmp")
            .join(format!("xshi-rsync-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    fn rsync(cwd: &Path, arguments: &[&str]) -> (i32, String, String) {
        let arguments: Vec<String> = arguments.iter().map(|argument| (*argument).to_owned()).collect();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let status = run(
            &arguments,
            &context(cwd),
            &mut Streams {
                stdout: foreground::Sink::Buffer(&mut stdout),
                stderr: foreground::Sink::Buffer(&mut stderr),
            },
        );
        (
            status,
            String::from_utf8_lossy(&stdout).into_owned(),
            String::from_utf8_lossy(&stderr).into_owned(),
        )
    }

    #[test]
    fn help_and_option_rejections_report_the_reference_statuses() {
        let directory = scratch("help");
        let (status, help, _) = rsync(&directory, &["--help"]);
        assert_eq!(status, 0);
        for needle in [
            "Synchronize files and directories locally",
            "--into DIRECTORY",
            "--contents",
            "--delete",
            "--dry-run",
            "-j, --jobs N",
        ] {
            assert!(help.contains(needle), "{needle}: {help}");
        }
        let (status, _, error) = rsync(&directory, &["-n"]);
        assert_eq!(status, 2);
        assert!(error.starts_with("xshi: rsync: -n is not supported; use --dry-run\nUsage: rsync"), "{error}");
        assert_eq!(rsync(&directory, &["--bogus-option"]).0, 2);
        std::fs::remove_dir_all(&directory).expect("clean scratch");
    }

    #[test]
    fn operand_mapping_is_invariant_under_a_trailing_slash() {
        let directory = scratch("mapping");
        std::fs::create_dir_all(directory.join("src")).expect("create source");
        std::fs::write(directory.join("src/file1.txt"), b"hello").expect("write source file");
        for (target, source) in [("dst_into1", "src"), ("dst_into2", "src/")] {
            std::fs::create_dir(directory.join(target)).expect("create target");
            assert_eq!(rsync(&directory, &["--into", target, source]).0, 0);
            assert!(directory.join(target).join("src/file1.txt").exists());
            assert!(!directory.join(target).join("file1.txt").exists());
        }
        for (target, source) in [("dst_exist1", "src"), ("dst_exist2", "src/")] {
            std::fs::create_dir(directory.join(target)).expect("create target");
            assert_eq!(rsync(&directory, &[source, target]).0, 0);
            assert!(directory.join(target).join("src/file1.txt").exists());
            assert!(!directory.join(target).join("file1.txt").exists());
        }
        std::fs::create_dir(directory.join("dst_contents")).expect("create contents target");
        assert_eq!(rsync(&directory, &["--contents", "src", "dst_contents"]).0, 0);
        assert!(directory.join("dst_contents/file1.txt").exists());
        assert!(!directory.join("dst_contents/src").exists());
        assert_eq!(rsync(&directory, &["src", "dst_exact"]).0, 0);
        assert!(directory.join("dst_exact/file1.txt").exists());
        assert!(!directory.join("dst_exact/src").exists());
        std::fs::remove_dir_all(&directory).expect("clean scratch");
    }

    #[test]
    fn modes_and_modification_times_are_preserved() {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
        let directory = scratch("metadata");
        std::fs::create_dir_all(directory.join("src")).expect("create source");
        let readonly = directory.join("src/readonly.txt");
        std::fs::write(&readonly, b"content-readonly").expect("write read-only file");
        std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o444))
            .expect("make read-only");
        let script = directory.join("src/script.sh");
        std::fs::write(&script, b"#!/bin/sh\necho hi\n").expect("write script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("make executable");
        assert_eq!(rsync(&directory, &["src", "dst"]).0, 0);
        let copied = std::fs::symlink_metadata(directory.join("dst/readonly.txt")).expect("stat copy");
        assert_eq!(copied.mode() & 0o777, 0o444);
        let executable = std::fs::symlink_metadata(directory.join("dst/script.sh")).expect("stat script");
        assert_eq!(executable.mode() & 0o777, 0o755);
        assert_eq!(
            copied.mtime(),
            std::fs::symlink_metadata(&readonly).expect("stat source").mtime()
        );
        for path in [readonly, directory.join("dst/readonly.txt")] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
                .expect("restore write permission");
        }
        std::fs::remove_dir_all(&directory).expect("clean scratch");
    }

    #[test]
    fn two_operands_map_into_an_existing_directory_or_exactly() {
        let arguments: Vec<String> = ["source", "tmp"].into_iter().map(str::to_owned).collect();
        let Ok(Parsed::Run(into)) = parse(&arguments, &context(Path::new("/"))) else {
            panic!("expected a sync request");
        };
        assert_eq!(
            into.operands,
            SyncOperands::Into {
                directory: PathBuf::from("tmp"),
                sources: vec![PathBuf::from("source")],
            }
        );
        let arguments: Vec<String> = ["source", "absent-target"].into_iter().map(str::to_owned).collect();
        let Ok(Parsed::Run(exact)) = parse(&arguments, &context(Path::new("/"))) else {
            panic!("expected a sync request");
        };
        assert_eq!(
            exact.operands,
            SyncOperands::Exact {
                source: PathBuf::from("source"),
                destination: PathBuf::from("absent-target"),
            }
        );
    }
}
