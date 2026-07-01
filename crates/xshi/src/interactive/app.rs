#![allow(clippy::single_call_fn)]

use super::config::{load_config, load_profile};
use super::denv::{self, DenvCommand};
#[cfg(test)]
use super::edit::{CompletionAction, LineBuffer, complete_buffer};
use super::edit::{EditorEvent, RawMode, read_interactive_command};
use super::history::add_history;
use super::listing;
use super::prompt::prompt;
use super::session::{InteractiveJob, InteractiveJobState, Session, set_env_bytes, stdio_is_tty};
use super::shell::{
    ChainOp, PipeOp, Pipeline, RedirectionKind as ShellRedirectionKind, ShellLine, ShellParser,
    ShellWord, ShellWordPart, SimpleCommand, expand_glob, has_glob_meta, lower_run_program,
    shell_line_source,
};
#[cfg(test)]
use super::shell::{ShellToken, lex_shell};
use rustix::termios::{self as rtermios, OptionalActions, Termios};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use xsh::diagnostic::DiagnosticRenderer;
use xsh::runtime::eval::Evaluator;
use xsh::runtime::process::{
    CancellationDecision, CancellationPolicy, ChildWaitOutcome, FileRedirectionMode,
    ForegroundTerminal, ManagedStdio, ProcessGroup, ProcessGroupConfig, ProcessInvocation,
    ProcessRedirection, ProcessSegmentStatus, ProcessSegmentStatusKind, ProcessStatus,
    ProcessStatusKind, RedirectionStream, SpawnManagedOptions, WaitMode,
    initialize_interactive_process_group, poll_managed, spawn_managed, wait_managed,
};
use xsh::runtime::value::RunError;
use xsh::sema::check::Checker;
use xsh::source::{SourceId, SourceMap, Span};
use xsh::syntax::arena::ArenaProgram;
use xsh::syntax::node::RunKind;
use xsh::syntax::parser::Parser;

fn text_bytes(text: impl Into<String>) -> Vec<u8> {
    text.into().into_bytes()
}

#[derive(Clone, Debug)]
pub struct CliOutput {
    pub status: u8,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct RunOptions {
    pub load_config: bool,
    pub load_profile: bool,
    pub require_tty: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            load_config: true,
            load_profile: false,
            require_tty: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OneCommandOptions {
    pub load_config: bool,
    pub load_profile: bool,
}

impl Default for OneCommandOptions {
    fn default() -> Self {
        Self {
            load_config: true,
            load_profile: false,
        }
    }
}

struct CommandOutput {
    status: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    process_status: Option<ProcessStatus>,
    history_source: Option<String>,
}

struct InteractiveNoCancellation;

impl CancellationPolicy for InteractiveNoCancellation {
    fn check_process_group(&mut self, _group: ProcessGroup) -> CancellationDecision {
        CancellationDecision::Continue
    }
}

#[derive(Clone, Debug)]
pub(super) struct ExpansionError {
    status: i32,
    message: String,
}

impl ExpansionError {
    pub(super) fn usage(message: impl Into<String>) -> Self {
        Self {
            status: 2,
            message: message.into(),
        }
    }

    pub(super) fn status(status: i32, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

pub fn run() -> i32 {
    run_with_options(RunOptions::default())
}

pub fn run_with_options(options: RunOptions) -> i32 {
    if options.require_tty && !stdio_is_tty() {
        eprintln!("xshi: interactive startup requires stdin and stdout to be terminals");
        return 2;
    }

    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let mut session = Session::new();
    if options.load_profile {
        load_profile(&mut session, &mut stderr);
    }
    if options.load_config {
        load_config(&mut session, &mut stderr);
    }
    denv::startup(&mut session, &mut stderr);
    session.refresh_path_commands();
    let _process_group_guard = if options.require_tty {
        match initialize_interactive_process_group() {
            Ok(guard) => guard,
            Err(err) => {
                eprintln!("xshi: failed to initialize job control: {err}");
                return 2;
            }
        }
    } else {
        None
    };
    let mut line = String::new();
    let mut raw_mode = if options.require_tty {
        match RawMode::enter() {
            Ok(mode) => Some(mode),
            Err(err) => {
                eprintln!("xshi: failed to initialize terminal: {err}");
                return 2;
            }
        }
    } else {
        None
    };

    loop {
        session.history.sync();
        reap_interactive_job(&mut session, &mut stderr);
        denv::refresh(&mut session, &mut stderr);
        let command = if options.require_tty {
            match read_interactive_command(&mut session, &mut stdout, &mut stderr) {
                EditorEvent::Submit(command) => command,
                EditorEvent::Cancel => {
                    session.last_status = 130;
                    session.last_process_status = Some(ProcessStatus::exited(130));
                    continue;
                }
                EditorEvent::Eof => {
                    let _ = writeln!(stdout);
                    session.history.compact();
                    return session.last_status.clamp(0, 255);
                }
                EditorEvent::Error(err) => {
                    let _ = writeln!(stderr, "xshi: failed to read line: {err}");
                    return 1;
                }
            }
        } else {
            let stdin = io::stdin();
            let mut input = stdin.lock();
            if write!(stdout, "{}", prompt(&session))
                .and_then(|_| stdout.flush())
                .is_err()
            {
                return 1;
            }

            line.clear();
            let read = match input.read_line(&mut line) {
                Ok(read) => read,
                Err(err) => {
                    let _ = writeln!(stderr, "xshi: failed to read line: {err}");
                    return 1;
                }
            };

            if read == 0 {
                let _ = writeln!(stdout);
                session.history.compact();
                return session.last_status.clamp(0, 255);
            }
            line.trim().to_string()
        };

        let command = command.trim();
        if command.is_empty() {
            continue;
        }

        if (command == "exit" || command.strip_prefix("exit ").is_some()) && session.job.is_some() {
            let _ = writeln!(stderr, "xshi: exit: background job is still running");
            session.last_status = 1;
            session.last_process_status = Some(ProcessStatus::exited(1));
            continue;
        }

        if let Some(code) = exit_code(command, session.last_status) {
            session.history.compact();
            return code;
        }

        let output = if let Some(raw_mode) = raw_mode.as_mut() {
            if let Err(err) = raw_mode.suspend() {
                let _ = writeln!(
                    stderr,
                    "xshi: failed to restore terminal before command: {err}"
                );
                return 1;
            }
            let output = execute_line(&mut session, command);
            if let Err(err) = raw_mode.resume() {
                let _ = writeln!(stderr, "xshi: failed to restore raw terminal mode: {err}");
                return 1;
            }
            output
        } else {
            execute_line(&mut session, command)
        };
        let _ = stdout.write_all(&output.stdout);
        let _ = stdout.flush();
        let _ = stderr.write_all(&output.stderr);
        let _ = stderr.flush();
        session.last_status = output.status;
        if output.process_status.is_some() {
            session.last_process_status = output.process_status;
        } else {
            session.last_process_status = Some(ProcessStatus::exited(output.status));
        }
        session.refresh_git_prompt();
        let history_source = output.history_source.as_deref().unwrap_or(command);
        add_history(&mut session, history_source);
    }
}

fn exit_code(source: &str, last_status: i32) -> Option<i32> {
    if source == "exit" {
        return Some(last_status.clamp(0, 255));
    }
    source
        .strip_prefix("exit ")
        .map(|code| code.trim().parse::<i32>().unwrap_or(2).clamp(0, 255))
}

fn execute_line(session: &mut Session, source: &str) -> CommandOutput {
    if is_xsh_source(source) {
        session.invalidate_cwd_snapshot();
        return run_xsh_source(session, "<interactive>", source);
    }

    match ShellParser::new(source).parse_line() {
        Ok(line) => execute_shell_line(session, line),
        Err(message) => CommandOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(format!("xshi: {message}\n")),
            process_status: Some(ProcessStatus::exited(2)),
            history_source: Some(source.to_string()),
        },
    }
}

fn run_xsh_source(session: &Session, source_name: &str, text: &str) -> CommandOutput {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(source_name, text.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, text);

    if !parsed.diagnostics.is_empty() {
        return CommandOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources)),
            process_status: Some(ProcessStatus::exited(2)),
            history_source: Some(text.to_string()),
        };
    }

    let checked = Checker::check_arena_interactive(&parsed.arena, text);
    if !checked.diagnostics.is_empty() {
        return CommandOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(DiagnosticRenderer::new().render(&checked.diagnostics, &sources)),
            process_status: Some(ProcessStatus::exited(2)),
            history_source: Some(text.to_string()),
        };
    }

    let output = Evaluator::new_interactive_session_with_sources(
        Vec::new(),
        sources,
        session.cwd.clone(),
        session.env.clone(),
        session.last_process_status.clone(),
    )
    .eval(&parsed.arena, source_id);
    let mut stderr = output.stderr;
    if !output.diagnostics.is_empty() {
        stderr.extend_from_slice(
            DiagnosticRenderer::new()
                .render(&output.diagnostics, &output.sources)
                .as_bytes(),
        );
    }
    CommandOutput {
        status: output.status as i32,
        stdout: output.stdout,
        stderr,
        process_status: output.last_status,
        history_source: Some(text.to_string()),
    }
}

fn run_interactive_program(
    session: &Session,
    source_name: &str,
    text: &str,
    arena: ArenaProgram,
) -> CommandOutput {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(source_name, text.to_string());
    let checked = Checker::check_arena_interactive(&arena, text);
    if !checked.diagnostics.is_empty() {
        return CommandOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(DiagnosticRenderer::new().render(&checked.diagnostics, &sources)),
            process_status: Some(ProcessStatus::exited(2)),
            history_source: Some(text.to_string()),
        };
    }

    let output = Evaluator::new_interactive_session_with_sources(
        Vec::new(),
        sources,
        session.cwd.clone(),
        session.env.clone(),
        session.last_process_status.clone(),
    )
    .eval(&arena, source_id);
    let mut stderr = output.stderr;
    if !output.diagnostics.is_empty() {
        stderr.extend_from_slice(
            DiagnosticRenderer::new()
                .render(&output.diagnostics, &output.sources)
                .as_bytes(),
        );
    }
    CommandOutput {
        status: output.status as i32,
        stdout: output.stdout,
        stderr,
        process_status: output.last_status,
        history_source: Some(text.to_string()),
    }
}

pub fn run_one_command(source: &str, with_config: bool) -> i32 {
    run_one_command_with_options(
        source,
        OneCommandOptions {
            load_config: with_config,
            load_profile: false,
        },
    )
}

pub fn run_one_command_with_options(source: &str, options: OneCommandOptions) -> i32 {
    let mut stderr = io::stderr();
    let mut session = Session::new();
    if options.load_profile {
        load_profile(&mut session, &mut stderr);
    }
    if options.load_config {
        load_config(&mut session, &mut stderr);
    }
    denv::startup(&mut session, &mut stderr);
    session.refresh_path_commands();
    let output = execute_line(&mut session, source);
    let _ = io::stdout().write_all(&output.stdout);
    let _ = io::stderr().write_all(&output.stderr);
    output.status.clamp(0, 255)
}

pub fn check_source(source_name: &str, text: &str) -> CliOutput {
    let session = Session::new();
    let output = run_xsh_source(&session, source_name, text);
    CliOutput {
        status: output.status.clamp(0, 255) as u8,
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

fn execute_shell_line(session: &mut Session, line: ShellLine) -> CommandOutput {
    if line.background {
        return execute_background_line(session, line);
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut last_status = session.last_status;
    let mut last_process_status = session.last_process_status.clone();

    for chain in line.chains {
        let should_run = match chain.op {
            ChainOp::Start | ChainOp::Sequence => true,
            ChainOp::And => last_status == 0,
            ChainOp::Or => last_status != 0,
        };
        if !should_run {
            continue;
        }
        let output = execute_pipeline(session, chain.pipeline);
        stdout.extend(output.stdout);
        stderr.extend(output.stderr);
        last_status = output.status;
        last_process_status = output.process_status;
        session.last_status = last_status;
        if last_process_status.is_some() {
            session.last_process_status = last_process_status.clone();
        }
    }

    CommandOutput {
        status: last_status,
        stdout,
        stderr,
        process_status: last_process_status,
        history_source: None,
    }
}

fn execute_background_line(session: &mut Session, mut line: ShellLine) -> CommandOutput {
    if line.chains.len() != 1 || line.chains[0].op != ChainOp::Start {
        return background_rejection("background jobs require one simple external command");
    }
    let chain = line.chains.remove(0);
    if chain.pipeline.commands.len() != 1 {
        return background_rejection("background pipelines are not supported");
    }
    let mut command = chain.pipeline.commands.into_iter().next().unwrap();
    if session_builtin(&command.words).is_none() {
        expand_alias(session, &mut command);
    }
    if let Err(message) = validate_assignment_prefix(&command) {
        return expansion_error_output(ExpansionError::usage(message));
    }
    if session_builtin(&command.words).is_some() {
        return background_rejection("session builtins cannot run in the background");
    }
    if command
        .words
        .iter()
        .all(|word| parse_env_assignment(&word.text()).is_some())
    {
        return background_rejection("assignment-only input cannot run in the background");
    }
    if session.job.is_some() {
        return CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: text_bytes("xshi: background job already exists\n"),
            process_status: Some(ProcessStatus::exited(1)),
            history_source: None,
        };
    }
    let source = shell_line_source(std::slice::from_ref(&command));
    let invocation = match external_invocation(session, &command) {
        Ok(invocation) => invocation,
        Err(error) => return expansion_error_output(error),
    };
    let options = SpawnManagedOptions {
        stdin: ManagedStdio::Inherit,
        stdout: ManagedStdio::Inherit,
        stderr: ManagedStdio::Inherit,
        apply_redirections: true,
        group: ProcessGroupConfig::NewRoot,
        reset_signals: true,
        spawn: Default::default(),
    };
    match spawn_managed(&invocation, options) {
        Ok(child) => {
            let pid = child.pid;
            let pgid = child.pgid;
            let display = invocation_display(&invocation);
            session.invalidate_cwd_snapshot();
            session.job = Some(InteractiveJob {
                child,
                pid,
                pgid,
                command: display.clone(),
                state: InteractiveJobState::RunningBackground,
                terminal_attrs: None,
                last_status: None,
                notified: false,
            });
            CommandOutput {
                status: 0,
                stdout: text_bytes(format!("[1] {pid} {display}\n")),
                stderr: Vec::new(),
                process_status: Some(ProcessStatus::exited(0)),
                history_source: Some(source),
            }
        }
        Err(error) => {
            let status = process_error_status(&invocation.target, error);
            status_from_process_output(CommandOutput {
                status: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
                process_status: Some(status),
                history_source: Some(source),
            })
        }
    }
}

fn background_rejection(message: &str) -> CommandOutput {
    CommandOutput {
        status: 2,
        stdout: Vec::new(),
        stderr: text_bytes(format!("xshi: {message}\n")),
        process_status: Some(ProcessStatus::exited(2)),
        history_source: None,
    }
}

fn execute_pipeline(session: &mut Session, mut pipeline: Pipeline) -> CommandOutput {
    if pipeline.commands.len() == 1 {
        return execute_simple_command(session, pipeline.commands.into_iter().next().unwrap());
    }

    for command in &mut pipeline.commands {
        expand_alias(session, command);
    }

    if pipeline
        .commands
        .iter()
        .any(|command| session_builtin(&command.words).is_some())
    {
        return CommandOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes("xshi: session builtins cannot be used in pipelines\n"),
            process_status: Some(ProcessStatus::exited(2)),
            history_source: None,
        };
    }

    for command in &pipeline.commands {
        if let Err(message) = validate_assignment_prefix(command) {
            return CommandOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xshi: {message}\n")),
                process_status: Some(ProcessStatus::exited(2)),
                history_source: None,
            };
        }
    }

    if pipeline.pipes.iter().all(|pipe| *pipe == PipeOp::Stdout)
        && !pipeline.commands.iter().any(command_has_dup_redirection)
    {
        let source = shell_line_source(&pipeline.commands);
        let span = Span::new(SourceId::new(0), 0, source.len());
        let arena = match lower_run_program(session, &pipeline.commands, RunKind::Status, span) {
            Ok(arena) => arena,
            Err(error) => return expansion_error_output(error),
        };
        return run_shell_run_program(session, source, arena);
    }

    let mut lowered = String::new();
    for (index, command) in pipeline.commands.iter().enumerate() {
        if index > 0 {
            lowered.push_str(match pipeline.pipes[index - 1] {
                PipeOp::Stdout => " | ",
                PipeOp::StdoutStderr => " |& ",
            });
            lowered.push_str("run ");
        } else {
            lowered.push_str("run.status ");
        }
        if let Err(error) = append_lowered_command(session, command, &mut lowered) {
            return expansion_error_output(error);
        }
    }
    let output = run_xsh_source(session, "<interactive-shell>", &lowered);
    status_from_process_output(output)
}

fn execute_simple_command(session: &mut Session, mut command: SimpleCommand) -> CommandOutput {
    if command.words.is_empty() {
        return CommandOutput {
            status: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            process_status: Some(ProcessStatus::exited(0)),
            history_source: None,
        };
    }

    if session_builtin(&command.words).is_none() {
        expand_alias(session, &mut command);
    }

    if let Err(message) = validate_assignment_prefix(&command) {
        return CommandOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(format!("xshi: {message}\n")),
            process_status: Some(ProcessStatus::exited(2)),
            history_source: None,
        };
    }

    if let Some(kind) = session_builtin(&command.words) {
        let args = if matches!(kind, SessionBuiltin::Alias) {
            command.words[1..]
                .iter()
                .map(ShellWord::text)
                .collect::<Vec<_>>()
        } else {
            match expand_command_words(session, &command.words[1..]) {
                Ok(args) => args,
                Err(error) => return expansion_error_output(error),
            }
        };
        return execute_session_builtin(session, kind, &args);
    }

    if command
        .words
        .iter()
        .all(|word| parse_env_assignment(&word.text()).is_some())
    {
        for word in command.words {
            let text = word.text();
            if let Some((name, _value)) = parse_env_assignment(&text) {
                let expanded = match expand_word_to_string(session, &word) {
                    Ok(expanded) => expanded,
                    Err(error) => return expansion_error_output(error),
                };
                let Some((_, value)) = parse_env_assignment(&expanded) else {
                    return expansion_error_output(ExpansionError::usage(format!(
                        "invalid environment assignment '{expanded}'"
                    )));
                };
                set_env_bytes(&mut session.env, name.as_bytes(), value.as_bytes());
                if name == "PATH" {
                    session.refresh_path_commands();
                }
            }
        }
        return CommandOutput {
            status: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            process_status: Some(ProcessStatus::exited(0)),
            history_source: None,
        };
    }

    if command.redirections.is_empty() {
        let source = shell_line_source(std::slice::from_ref(&command));
        let invocation = match external_invocation(session, &command) {
            Ok(invocation) => invocation,
            Err(error) => return expansion_error_output(error),
        };
        session.invalidate_cwd_snapshot();
        return run_external_foreground(session, source, invocation);
    }

    if !command_has_dup_redirection(&command) {
        let source = shell_line_source(std::slice::from_ref(&command));
        let span = Span::new(SourceId::new(0), 0, source.len());
        let arena = match lower_run_program(session, &[command], RunKind::Status, span) {
            Ok(arena) => arena,
            Err(error) => return expansion_error_output(error),
        };
        session.invalidate_cwd_snapshot();
        return run_shell_run_program(session, source, arena);
    }
    let mut lowered = String::from("run.status ");
    if let Err(error) = append_lowered_command(session, &command, &mut lowered) {
        return expansion_error_output(error);
    }
    session.invalidate_cwd_snapshot();
    let output = run_xsh_source(session, "<interactive-shell>", &lowered);
    status_from_process_output(output)
}

fn execute_session_builtin(
    session: &mut Session,
    kind: SessionBuiltin,
    args: &[String],
) -> CommandOutput {
    if kind == SessionBuiltin::Fg {
        return execute_fg_builtin(session, args);
    }
    if kind == SessionBuiltin::Bg {
        return execute_bg_builtin(session, args);
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = match kind {
        SessionBuiltin::Fg | SessionBuiltin::Bg => unreachable!("job builtins returned above"),
        SessionBuiltin::Noop => 0,
        SessionBuiltin::Cd => match args {
            [] => {
                if let Some(home) = session.home.clone() {
                    match session.set_cwd(home) {
                        Ok(()) => {
                            denv::after_cwd_change(session, &mut stderr);
                            session.refresh_path_commands();
                            0
                        }
                        Err(err) => {
                            writeln!(stderr, "cd: {err}").ok();
                            1
                        }
                    }
                } else {
                    stderr.extend_from_slice(b"cd: HOME is not set\n");
                    1
                }
            }
            [path] => {
                let mut print_target = false;
                let target = if path == "-" {
                    match session.env.get(b"OLDPWD".as_slice()) {
                        Some(oldpwd) if !oldpwd.is_empty() => {
                            print_target = true;
                            PathBuf::from(OsString::from_vec(oldpwd.clone()))
                        }
                        _ => {
                            stderr.extend_from_slice(b"cd: no previous directory\n");
                            return CommandOutput {
                                status: 1,
                                stdout,
                                stderr,
                                process_status: Some(ProcessStatus::exited(1)),
                                history_source: None,
                            };
                        }
                    }
                } else {
                    PathBuf::from(path)
                };
                match session.set_cwd(target) {
                    Ok(()) => {
                        if print_target {
                            writeln!(stdout, "{}", session.cwd.display()).ok();
                        }
                        denv::after_cwd_change(session, &mut stderr);
                        session.refresh_path_commands();
                        0
                    }
                    Err(err) => {
                        writeln!(stderr, "cd: {err}").ok();
                        1
                    }
                }
            }
            _ => {
                stderr.extend_from_slice(b"cd: expected zero or one path\n");
                2
            }
        },
        SessionBuiltin::Set => match args {
            [name, value] if valid_env_name(name) => {
                set_env_bytes(&mut session.env, name.as_bytes(), value.as_bytes());
                if name == "PATH" {
                    session.refresh_path_commands();
                }
                0
            }
            _ => {
                stderr.extend_from_slice(b"set: expected NAME value\n");
                2
            }
        },
        SessionBuiltin::Unset => {
            if args.is_empty() {
                stderr.extend_from_slice(b"unset: expected NAME...\n");
                2
            } else {
                for name in args {
                    session.env.remove(name.as_bytes());
                    if name == "PATH" {
                        session.refresh_path_commands();
                    }
                }
                0
            }
        }
        SessionBuiltin::Alias => {
            if args.is_empty() {
                for (name, source) in &session.aliases {
                    writeln!(stdout, "alias {name}={}", shell_quote(source)).ok();
                }
                0
            } else {
                let mut status = 0;
                for arg in args {
                    if let Some((name, value)) = arg.split_once('=') {
                        if validate_alias_source(name, value).is_ok() {
                            session.aliases.insert(name.to_string(), value.to_string());
                        } else {
                            writeln!(stderr, "alias: invalid alias name '{name}'").ok();
                            status = 2;
                        }
                    } else {
                        writeln!(stderr, "alias: expected NAME=SOURCE").ok();
                        status = 2;
                    }
                }
                status
            }
        }
        SessionBuiltin::Which => {
            if args.is_empty() {
                stderr.extend_from_slice(b"which: expected NAME...\n");
                2
            } else {
                let mut status = 0;
                for name in args {
                    if !describe_command(session, name, &mut stdout) {
                        status = 1;
                    }
                }
                status
            }
        }
        SessionBuiltin::History => {
            let limit = match args {
                [] => None,
                [value] => match value.parse::<usize>() {
                    Ok(limit) => Some(limit),
                    Err(_) => {
                        stderr.extend_from_slice(b"history: expected optional count\n");
                        return CommandOutput {
                            status: 2,
                            stdout,
                            stderr,
                            process_status: Some(ProcessStatus::exited(2)),
                            history_source: None,
                        };
                    }
                },
                _ => {
                    stderr.extend_from_slice(b"history: expected optional count\n");
                    return CommandOutput {
                        status: 2,
                        stdout,
                        stderr,
                        process_status: Some(ProcessStatus::exited(2)),
                        history_source: None,
                    };
                }
            };
            let len = session.history.len();
            let start = limit.map_or(0, |limit| len.saturating_sub(limit));
            for index in start..len {
                writeln!(stdout, "{:>5}  {}", index + 1, session.history.get(index)).ok();
            }
            0
        }
        SessionBuiltin::Clear => {
            stdout.extend_from_slice(b"\x1b[H\x1b[2J");
            0
        }
        SessionBuiltin::List => listing::run(session, args, &mut stdout, &mut stderr),
        SessionBuiltin::Z => match args {
            [query] => match super::z::select(session, query) {
                Ok(path) => match session.set_cwd(path) {
                    Ok(()) => {
                        denv::after_cwd_change(session, &mut stderr);
                        session.refresh_path_commands();
                        0
                    }
                    Err(err) => {
                        writeln!(stderr, "z: {err}").ok();
                        1
                    }
                },
                Err(message) => {
                    writeln!(stderr, "{message}").ok();
                    1
                }
            },
            _ => {
                stderr.extend_from_slice(b"z: expected query\n");
                2
            }
        },
        SessionBuiltin::Denv => match args {
            [command] if command == "allow" => {
                let status =
                    denv::run_command(session, DenvCommand::Allow, &mut stdout, &mut stderr);
                session.refresh_path_commands();
                status
            }
            [command] if command == "deny" => {
                let status =
                    denv::run_command(session, DenvCommand::Deny, &mut stdout, &mut stderr);
                session.refresh_path_commands();
                status
            }
            [command] if command == "reload" => {
                let status =
                    denv::run_command(session, DenvCommand::Reload, &mut stdout, &mut stderr);
                session.refresh_path_commands();
                status
            }
            _ => {
                stderr.extend_from_slice(b"denv: expected allow, deny, or reload\n");
                2
            }
        },
    };
    CommandOutput {
        status,
        stdout,
        stderr,
        process_status: Some(ProcessStatus::exited(status)),
        history_source: None,
    }
}

fn execute_fg_builtin(session: &mut Session, args: &[String]) -> CommandOutput {
    if !args.is_empty() {
        return CommandOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes("fg: expected no arguments\n"),
            process_status: Some(ProcessStatus::exited(2)),
            history_source: None,
        };
    }
    let Some(mut job) = session.job.take() else {
        return CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: text_bytes("xshi: fg: no background job\n"),
            process_status: Some(ProcessStatus::exited(1)),
            history_source: None,
        };
    };
    match poll_managed(&mut job.child) {
        Ok(ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status)) => {
            let stderr = text_bytes(format!(
                "xshi: completed: {} (pid={}, {})\n",
                job.command,
                job.pid,
                process_status_label(&status)
            ));
            return status_command_output(status, stderr);
        }
        Ok(ChildWaitOutcome::Stopped { .. }) => {
            job.state = InteractiveJobState::Stopped;
        }
        Ok(ChildWaitOutcome::StillRunning) => {}
        Err(error) => {
            return CommandOutput {
                status: 1,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xshi: fg: {}\n", error.message)),
                process_status: Some(ProcessStatus::exited(1)),
                history_source: None,
            };
        }
    }

    let group = ProcessGroup::from_pgid(job.pgid);
    let foreground = ForegroundTerminal::take(group);
    if let Some(attrs) = job.terminal_attrs.as_ref() {
        restore_terminal_attrs(attrs);
    }
    if job.state == InteractiveJobState::Stopped {
        group.signal(libc::SIGCONT);
    }
    let mut policy = InteractiveNoCancellation;
    match wait_managed(&mut job.child, WaitMode::InteractiveForeground, &mut policy) {
        Ok((ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status), _)) => {
            drop(foreground);
            status_command_output(status, Vec::new())
        }
        Ok((ChildWaitOutcome::Stopped { signal: _ }, _)) => {
            job.terminal_attrs = terminal_attrs();
            drop(foreground);
            group.signal(libc::SIGCONT);
            job.state = InteractiveJobState::RunningBackground;
            job.notified = true;
            let pid = job.pid;
            let command = job.command.clone();
            session.job = Some(job);
            CommandOutput {
                status: 148,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xshi: backgrounded: {command} (pid={pid})\n")),
                process_status: Some(ProcessStatus::exited(148)),
                history_source: None,
            }
        }
        Ok((ChildWaitOutcome::StillRunning, _)) => {
            drop(foreground);
            session.job = Some(job);
            CommandOutput {
                status: 1,
                stdout: Vec::new(),
                stderr: text_bytes("xshi: fg: wait ended before job completed\n"),
                process_status: Some(ProcessStatus::exited(1)),
                history_source: None,
            }
        }
        Err(error) => {
            drop(foreground);
            CommandOutput {
                status: 1,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xshi: fg: {}\n", error.message)),
                process_status: Some(ProcessStatus::exited(1)),
                history_source: None,
            }
        }
    }
}

fn execute_bg_builtin(session: &mut Session, args: &[String]) -> CommandOutput {
    if !args.is_empty() {
        return CommandOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes("bg: expected no arguments\n"),
            process_status: Some(ProcessStatus::exited(2)),
            history_source: None,
        };
    }
    let Some(job) = session.job.as_mut() else {
        return CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: text_bytes("xshi: bg: no background job\n"),
            process_status: Some(ProcessStatus::exited(1)),
            history_source: None,
        };
    };
    match poll_managed(&mut job.child) {
        Ok(ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status)) => {
            let stderr = text_bytes(format!(
                "xshi: completed: {} (pid={}, {})\n",
                job.command,
                job.pid,
                process_status_label(&status)
            ));
            session.job = None;
            return CommandOutput {
                status: 1,
                stdout: Vec::new(),
                stderr,
                process_status: Some(ProcessStatus::exited(1)),
                history_source: None,
            };
        }
        Ok(ChildWaitOutcome::Stopped { .. }) => {
            job.state = InteractiveJobState::Stopped;
        }
        Ok(ChildWaitOutcome::StillRunning) => {}
        Err(error) => {
            return CommandOutput {
                status: 1,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xshi: bg: {}\n", error.message)),
                process_status: Some(ProcessStatus::exited(1)),
                history_source: None,
            };
        }
    }
    match job.state {
        InteractiveJobState::Stopped => {
            ProcessGroup::from_pgid(job.pgid).signal(libc::SIGCONT);
            job.state = InteractiveJobState::RunningBackground;
            job.notified = false;
            CommandOutput {
                status: 0,
                stdout: Vec::new(),
                stderr: text_bytes(format!(
                    "xshi: resumed: {} (pid={})\n",
                    job.command, job.pid
                )),
                process_status: Some(ProcessStatus::exited(0)),
                history_source: None,
            }
        }
        InteractiveJobState::RunningBackground => CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: text_bytes("xshi: bg: job already running\n"),
            process_status: Some(ProcessStatus::exited(1)),
            history_source: None,
        },
    }
}

fn status_command_output(status: ProcessStatus, stderr: Vec<u8>) -> CommandOutput {
    let code = shell_status(&status);
    CommandOutput {
        status: code,
        stdout: Vec::new(),
        stderr,
        process_status: Some(status),
        history_source: None,
    }
}

fn restore_terminal_attrs(attrs: &Termios) {
    let _ = rtermios::tcsetattr(rustix::stdio::stdin(), OptionalActions::Now, attrs);
}

fn status_from_process_output(mut output: CommandOutput) -> CommandOutput {
    if let Some(status) = &output.process_status {
        output.status = shell_status(status);
    }
    output
}

fn expansion_error_output(error: ExpansionError) -> CommandOutput {
    CommandOutput {
        status: error.status,
        stdout: Vec::new(),
        stderr: text_bytes(format!("xshi: {}\n", error.message)),
        process_status: Some(ProcessStatus::exited(error.status)),
        history_source: None,
    }
}

fn run_shell_run_program(session: &Session, source: String, arena: ArenaProgram) -> CommandOutput {
    let output = run_interactive_program(session, "<interactive-shell>", &source, arena);
    status_from_process_output(output)
}

fn run_external_foreground(
    session: &mut Session,
    source: String,
    invocation: ProcessInvocation,
) -> CommandOutput {
    let options = SpawnManagedOptions {
        stdin: ManagedStdio::Inherit,
        stdout: ManagedStdio::Inherit,
        stderr: ManagedStdio::Inherit,
        apply_redirections: true,
        group: ProcessGroupConfig::NewRoot,
        reset_signals: true,
        spawn: Default::default(),
    };
    let status = match spawn_managed(&invocation, options) {
        Ok(mut child) => {
            let foreground = ForegroundTerminal::take(child.process_group());
            let mut policy = InteractiveNoCancellation;
            match wait_managed(&mut child, WaitMode::InteractiveForeground, &mut policy) {
                Ok((ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status), _)) => {
                    status
                }
                Ok((ChildWaitOutcome::Stopped { signal: _ }, _)) => {
                    let attrs = terminal_attrs();
                    drop(foreground);
                    if session.job.is_some() {
                        return CommandOutput {
                            status: 148,
                            stdout: Vec::new(),
                            stderr: text_bytes(
                                "xshi: background job already exists; stopped job is unmanaged\n",
                            ),
                            process_status: Some(ProcessStatus::exited(148)),
                            history_source: Some(source),
                        };
                    }
                    child.process_group().signal(libc::SIGCONT);
                    let pid = child.pid;
                    let pgid = child.pgid;
                    session.job = Some(InteractiveJob {
                        child,
                        pid,
                        pgid,
                        command: invocation_display(&invocation),
                        state: InteractiveJobState::RunningBackground,
                        terminal_attrs: attrs,
                        last_status: None,
                        notified: true,
                    });
                    return CommandOutput {
                        status: 148,
                        stdout: Vec::new(),
                        stderr: text_bytes(format!(
                            "xshi: backgrounded: {} (pid={pid})\n",
                            invocation_display(&invocation)
                        )),
                        process_status: Some(ProcessStatus::exited(148)),
                        history_source: Some(source),
                    };
                }
                Ok((ChildWaitOutcome::StillRunning, _)) => ProcessStatus::signaled(libc::SIGTERM),
                Err(error) => process_error_status(&invocation.target, error),
            }
        }
        Err(error) => process_error_status(&invocation.target, error),
    };
    status_from_process_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
        process_status: Some(status),
        history_source: Some(source),
    })
}

fn reap_interactive_job(session: &mut Session, stderr: &mut dyn Write) {
    let Some(job) = session.job.as_mut() else {
        return;
    };
    match poll_managed(&mut job.child) {
        Ok(ChildWaitOutcome::StillRunning) => {}
        Ok(ChildWaitOutcome::Stopped { signal }) => {
            job.state = InteractiveJobState::Stopped;
            if !job.notified {
                let _ = writeln!(
                    stderr,
                    "xshi: stopped: {} (pid={}, signal={signal})",
                    job.command, job.pid
                );
                job.notified = true;
            }
        }
        Ok(ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status)) => {
            let command = job.command.clone();
            let pid = job.pid;
            let label = process_status_label(&status);
            job.last_status = Some(status);
            let _ = writeln!(stderr, "xshi: completed: {command} (pid={pid}, {label})");
            session.job = None;
        }
        Err(error) => {
            let command = job.command.clone();
            let _ = writeln!(
                stderr,
                "xshi: job wait failed for {command}: {}",
                error.message
            );
            session.job = None;
        }
    }
}

fn process_status_label(status: &ProcessStatus) -> String {
    match status.kind {
        ProcessStatusKind::Exit => format!("exit={}", status.code.unwrap_or(1)),
        ProcessStatusKind::Signal => format!("signal={}", status.code.unwrap_or(0)),
        ProcessStatusKind::Exec => "exec-failure".to_string(),
    }
}

fn terminal_attrs() -> Option<Termios> {
    rtermios::tcgetattr(rustix::stdio::stdin()).ok()
}

fn invocation_display(invocation: &ProcessInvocation) -> String {
    std::iter::once(invocation.target.as_slice())
        .chain(invocation.argv.iter().map(Vec::as_slice))
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn process_error_status(target: &[u8], error: RunError) -> ProcessStatus {
    ProcessStatus::from_segments(vec![ProcessSegmentStatus {
        index: 0,
        target: target.to_vec(),
        pid: None,
        success: false,
        kind: ProcessSegmentStatusKind::Exec,
        code: None,
        error_kind: Some(error.kind),
        error_message: Some(error.message),
    }])
}

fn shell_status(status: &ProcessStatus) -> i32 {
    if let Some(segment) = status
        .segments
        .iter()
        .rev()
        .find(|segment| !segment.success)
    {
        return match segment.kind {
            ProcessSegmentStatusKind::Exit => segment.code.unwrap_or(1),
            ProcessSegmentStatusKind::Signal => 128 + segment.code.unwrap_or(0),
            ProcessSegmentStatusKind::Exec => match segment.error_kind.as_deref() {
                Some("not-found") => 127,
                Some("permission-denied") => 126,
                _ => 126,
            },
        };
    }
    match status.kind {
        ProcessStatusKind::Exit => status.code.unwrap_or(if status.success { 0 } else { 1 }),
        ProcessStatusKind::Signal => 128 + status.code.unwrap_or(0),
        ProcessStatusKind::Exec => {
            let kind = status
                .segments
                .iter()
                .find_map(|segment| segment.error_kind.as_deref());
            match kind {
                Some("not-found") => 127,
                Some("permission-denied") => 126,
                _ => 126,
            }
        }
    }
}

fn append_lowered_command(
    session: &Session,
    command: &SimpleCommand,
    lowered: &mut String,
) -> Result<(), ExpansionError> {
    let mut words = command.words.iter();
    while let Some(word) = words.clone().next() {
        let text = word.text();
        if let Some((name, _value)) = parse_env_assignment(&text) {
            lowered.push_str(name);
            lowered.push('=');
            let expanded = expand_word_to_string(session, word)?;
            let Some((_, value)) = parse_env_assignment(&expanded) else {
                return Err(ExpansionError::usage(format!(
                    "invalid environment assignment '{expanded}'"
                )));
            };
            lowered.push_str(&xsh_word(value));
            lowered.push(' ');
            words.next();
        } else {
            break;
        }
    }
    let mut argv = Vec::new();
    for word in words {
        for expanded in expand_word(session, word)? {
            argv.push(expanded);
        }
    }
    for (index, arg) in argv.iter().enumerate() {
        if index > 0 {
            lowered.push(' ');
        }
        lowered.push_str(&xsh_word(arg));
    }
    for redirection in &command.redirections {
        lowered.push(' ');
        lowered.push_str(match redirection.kind {
            ShellRedirectionKind::Stdin => "<",
            ShellRedirectionKind::StdoutWrite => ">",
            ShellRedirectionKind::StdoutAppend => ">>",
            ShellRedirectionKind::StderrWrite => "2>",
            ShellRedirectionKind::StderrAppend => "2>>",
            ShellRedirectionKind::StdoutToStderr => ">&",
            ShellRedirectionKind::StderrToStdout => "2>&",
        });
        lowered.push(' ');
        lowered.push_str(&xsh_word(&expand_word_to_string(
            session,
            &redirection.target,
        )?));
    }
    Ok(())
}

fn external_invocation(
    session: &Session,
    command: &SimpleCommand,
) -> Result<ProcessInvocation, ExpansionError> {
    let mut words = command.words.iter();
    let mut env_overlay = BTreeMap::new();
    while let Some(word) = words.clone().next() {
        let text = word.text();
        let Some((name, _value)) = parse_env_assignment(&text) else {
            break;
        };
        let expanded = expand_word_to_string(session, word)?;
        let Some((_, value)) = parse_env_assignment(&expanded) else {
            return Err(ExpansionError::usage(format!(
                "invalid environment assignment '{expanded}'"
            )));
        };
        env_overlay.insert(name.as_bytes().to_vec(), value.as_bytes().to_vec());
        words.next();
    }

    let mut argv = Vec::new();
    for word in words {
        argv.extend(expand_word(session, word)?);
    }
    let Some(target) = argv.first() else {
        return Err(ExpansionError::usage("expected command"));
    };
    let mut env = session.env.clone();
    env.extend(env_overlay.clone());
    Ok(ProcessInvocation {
        target: target.as_bytes().to_vec(),
        argv: argv
            .iter()
            .skip(1)
            .map(|arg| arg.as_bytes().to_vec())
            .collect(),
        cwd: session.cwd.clone(),
        env,
        env_overlay,
        redirections: command
            .redirections
            .iter()
            .map(|redirection| external_redirection(session, redirection))
            .collect::<Result<Vec<_>, _>>()?,
        timeout: None,
        cpu_max: None,
    })
}

fn external_redirection(
    session: &Session,
    redirection: &super::shell::Redirection,
) -> Result<ProcessRedirection, ExpansionError> {
    let path_target = |word: &ShellWord| -> Result<PathBuf, ExpansionError> {
        let target = expand_word_to_string(session, word)?;
        let path = PathBuf::from(target);
        Ok(if path.is_absolute() {
            path
        } else {
            session.cwd.join(path)
        })
    };
    Ok(match redirection.kind {
        ShellRedirectionKind::Stdin => ProcessRedirection::File {
            stream: RedirectionStream::Stdin,
            mode: FileRedirectionMode::Read,
            path: path_target(&redirection.target)?,
        },
        ShellRedirectionKind::StdoutWrite => ProcessRedirection::File {
            stream: RedirectionStream::Stdout,
            mode: FileRedirectionMode::Write,
            path: path_target(&redirection.target)?,
        },
        ShellRedirectionKind::StdoutAppend => ProcessRedirection::File {
            stream: RedirectionStream::Stdout,
            mode: FileRedirectionMode::Append,
            path: path_target(&redirection.target)?,
        },
        ShellRedirectionKind::StderrWrite => ProcessRedirection::File {
            stream: RedirectionStream::Stderr,
            mode: FileRedirectionMode::Write,
            path: path_target(&redirection.target)?,
        },
        ShellRedirectionKind::StderrAppend => ProcessRedirection::File {
            stream: RedirectionStream::Stderr,
            mode: FileRedirectionMode::Append,
            path: path_target(&redirection.target)?,
        },
        ShellRedirectionKind::StdoutToStderr => ProcessRedirection::Dup {
            stream: RedirectionStream::Stdout,
            fd: 2,
        },
        ShellRedirectionKind::StderrToStdout => ProcessRedirection::Dup {
            stream: RedirectionStream::Stderr,
            fd: 1,
        },
    })
}

fn expand_alias(session: &Session, command: &mut SimpleCommand) {
    let Some(source) = command
        .words
        .first()
        .and_then(|name| session.aliases.get(&name.text()))
        .cloned()
    else {
        return;
    };
    let Ok(mut alias_line) = ShellParser::new(&source).parse_line() else {
        return;
    };
    if alias_line.chains.len() != 1 {
        return;
    }
    let alias_chain = alias_line.chains.remove(0);
    if alias_chain.pipeline.commands.len() != 1 {
        return;
    }
    let mut alias_command = alias_chain.pipeline.commands.into_iter().next().unwrap();
    alias_command
        .words
        .extend(command.words.iter().skip(1).cloned());
    alias_command
        .redirections
        .extend(command.redirections.clone());
    *command = alias_command;
}

fn validate_assignment_prefix(command: &SimpleCommand) -> Result<(), String> {
    for word in &command.words {
        let text = word.text();
        let Some((name, _)) = text.split_once('=') else {
            break;
        };
        if valid_env_name(name) {
            continue;
        }
        if !name.contains('/') {
            return Err(format!("invalid environment assignment '{text}'"));
        }
        break;
    }
    Ok(())
}

fn command_has_dup_redirection(command: &SimpleCommand) -> bool {
    command.redirections.iter().any(|redirection| {
        matches!(
            redirection.kind,
            ShellRedirectionKind::StdoutToStderr | ShellRedirectionKind::StderrToStdout
        )
    })
}

fn is_xsh_source(source: &str) -> bool {
    let trimmed = source.trim_start();
    let first = trimmed
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ';' | '(' | '{' | '['))
        .next()
        .unwrap_or("");
    if first == "type" {
        return looks_like_type_definition(trimmed);
    }
    if matches!(first, "true" | "false") {
        return looks_like_bool_expression(trimmed, first);
    }
    matches!(
        first,
        "let"
            | "var"
            | "proc"
            | "pure"
            | "use"
            | "export"
            | "if"
            | "for"
            | "while"
            | "match"
            | "return"
            | "defer"
            | "guard"
            | "run"
            | "print"
            | "eprint"
    ) || trimmed.starts_with('{')
        || (trimmed.starts_with('[') && !matches!(trimmed.as_bytes().get(1), None | Some(b' ')))
        || trimmed.starts_with('(')
        || trimmed.starts_with('"')
        || trimmed.starts_with("p\"")
        || trimmed.starts_with("f\"")
        || trimmed.starts_with("fp\"")
        || trimmed.starts_with("null")
        || trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        || looks_like_module_qualified_start(first)
}

fn looks_like_type_definition(trimmed: &str) -> bool {
    let rest = trimmed
        .strip_prefix("type")
        .unwrap_or_default()
        .trim_start();
    let mut chars = rest.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let mut after_name = chars.as_str();
    while let Some(ch) = after_name.chars().next() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            after_name = &after_name[ch.len_utf8()..];
        } else {
            break;
        }
    }
    after_name.trim_start().starts_with('=')
}

fn looks_like_bool_expression(trimmed: &str, first: &str) -> bool {
    let rest = trimmed[first.len()..].trim_start();
    !rest.is_empty() && !matches!(rest.as_bytes().first().copied(), Some(b';' | b'|' | b'&'))
}

fn looks_like_module_qualified_start(first: &str) -> bool {
    let Some((module, member)) = first.split_once('.') else {
        return false;
    };
    valid_xsh_ident(module) && valid_xsh_ident(member)
}

fn valid_xsh_ident(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn describe_command(session: &Session, name: &str, stdout: &mut Vec<u8>) -> bool {
    if let Some(source) = session.aliases.get(name) {
        writeln!(stdout, "{name}: alias for {source}").ok();
        true
    } else if session_builtin_name(name).is_some() {
        writeln!(stdout, "{name}: xshi session builtin").ok();
        true
    } else if let Some(path) = find_in_path(session, name) {
        writeln!(stdout, "{}", path.display()).ok();
        true
    } else {
        writeln!(stdout, "{name}: not found").ok();
        false
    }
}

fn find_in_path(session: &Session, name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return is_executable_file(&path).then_some(path);
    }
    let path = session.env.get(b"PATH".as_slice())?;
    for dir in std::env::split_paths(&OsString::from_vec(path.clone())) {
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

pub(super) fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

pub(super) fn expand_word(
    session: &Session,
    word: &ShellWord,
) -> Result<Vec<String>, ExpansionError> {
    let (text, glob) = expand_word_inner(session, word)?;
    if glob && has_glob_meta(&text) {
        expand_glob(session, &text)
    } else {
        Ok(vec![text])
    }
}

pub(super) fn expand_word_to_string(
    session: &Session,
    word: &ShellWord,
) -> Result<String, ExpansionError> {
    expand_word_inner(session, word).map(|(text, _)| text)
}

fn expand_word_inner(
    session: &Session,
    word: &ShellWord,
) -> Result<(String, bool), ExpansionError> {
    let mut output = String::new();
    let mut glob = false;
    for part in &word.parts {
        match part {
            ShellWordPart::Text {
                text,
                expand,
                glob: part_glob,
            } => {
                let mut text = text.clone();
                if *expand {
                    if output.is_empty() && *part_glob {
                        text = expand_tilde(session, &text);
                    }
                    text = expand_vars(session, &text);
                }
                output.push_str(&text);
                glob |= *part_glob;
            }
            ShellWordPart::CommandSubstitution {
                source,
                glob: part_glob,
            } => {
                output.push_str(&expand_command_substitution(session, source)?);
                glob |= *part_glob;
            }
            ShellWordPart::ArithmeticExpansion {
                source,
                glob: part_glob,
            } => {
                output.push_str(&expand_arithmetic(session, source)?);
                glob |= *part_glob;
            }
        }
    }
    Ok((output, glob))
}

fn expand_command_words(
    session: &Session,
    words: &[ShellWord],
) -> Result<Vec<String>, ExpansionError> {
    let mut output = Vec::new();
    for word in words {
        output.extend(expand_word(session, word)?);
    }
    Ok(output)
}

fn expand_command_substitution(session: &Session, source: &str) -> Result<String, ExpansionError> {
    let line = ShellParser::new(source)
        .parse_line()
        .map_err(|message| ExpansionError::usage(format!("command substitution: {message}")))?;
    let mut nested = session.clone();
    let output = execute_shell_line(&mut nested, line);
    if output.status != 0 {
        let detail = String::from_utf8_lossy(&output.stderr);
        let message = if detail.trim().is_empty() {
            format!("command substitution failed with status {}", output.status)
        } else {
            format!(
                "command substitution failed with status {}: {}",
                output.status,
                detail.trim()
            )
        };
        return Err(ExpansionError::status(output.status, message));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| ExpansionError::usage("command substitution produced non-UTF-8 output"))
}

fn expand_arithmetic(session: &Session, source: &str) -> Result<String, ExpansionError> {
    ArithmeticParser::new(session, source)
        .parse()
        .map(|value| value.to_string())
}

fn expand_tilde(session: &Session, word: &str) -> String {
    let Some(home) = &session.home else {
        return word.to_string();
    };
    if word == "~" {
        home.display().to_string()
    } else if let Some(rest) = word.strip_prefix("~/") {
        home.join(rest).display().to_string()
    } else {
        word.to_string()
    }
}

fn expand_vars(session: &Session, word: &str) -> String {
    let mut out = String::new();
    let mut chars = word.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            out.push(ch);
            continue;
        }
        let mut name = String::new();
        while chars
            .peek()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        {
            name.push(chars.next().unwrap());
        }
        if name.is_empty() {
            out.push('$');
        } else if let Some(value) = session.env.get(name.as_bytes()) {
            out.push_str(&String::from_utf8_lossy(value));
        }
    }
    out
}

struct ArithmeticParser<'a> {
    session: &'a Session,
    source: &'a str,
    pos: usize,
}

impl<'a> ArithmeticParser<'a> {
    fn new(session: &'a Session, source: &'a str) -> Self {
        Self {
            session,
            source,
            pos: 0,
        }
    }

    fn parse(mut self) -> Result<i64, ExpansionError> {
        let value = self.parse_expr()?;
        self.skip_ws();
        if self.pos != self.source.len() {
            return Err(self.error("unexpected token in arithmetic expansion"));
        }
        Ok(value)
    }

    fn parse_expr(&mut self) -> Result<i64, ExpansionError> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_ws();
            if self.take('+') {
                value = value
                    .checked_add(self.parse_term()?)
                    .ok_or_else(|| self.error("arithmetic overflow"))?;
            } else if self.take('-') {
                value = value
                    .checked_sub(self.parse_term()?)
                    .ok_or_else(|| self.error("arithmetic overflow"))?;
            } else {
                return Ok(value);
            }
        }
    }

    fn parse_term(&mut self) -> Result<i64, ExpansionError> {
        let mut value = self.parse_unary()?;
        loop {
            self.skip_ws();
            if self.take('*') {
                value = value
                    .checked_mul(self.parse_unary()?)
                    .ok_or_else(|| self.error("arithmetic overflow"))?;
            } else if self.take('/') {
                let rhs = self.parse_unary()?;
                if rhs == 0 {
                    return Err(self.error("division by zero"));
                }
                value = value
                    .checked_div(rhs)
                    .ok_or_else(|| self.error("arithmetic overflow"))?;
            } else if self.take('%') {
                let rhs = self.parse_unary()?;
                if rhs == 0 {
                    return Err(self.error("division by zero"));
                }
                value = value
                    .checked_rem(rhs)
                    .ok_or_else(|| self.error("arithmetic overflow"))?;
            } else {
                return Ok(value);
            }
        }
    }

    fn parse_unary(&mut self) -> Result<i64, ExpansionError> {
        self.skip_ws();
        if self.take('+') {
            return self.parse_unary();
        }
        if self.take('-') {
            return self
                .parse_unary()?
                .checked_neg()
                .ok_or_else(|| self.error("arithmetic overflow"));
        }
        if self.take('!') {
            return Ok((self.parse_unary()? == 0) as i64);
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<i64, ExpansionError> {
        self.skip_ws();
        if self.take('(') {
            let value = self.parse_expr()?;
            self.skip_ws();
            if !self.take(')') {
                return Err(self.error("expected ')' in arithmetic expansion"));
            }
            return Ok(value);
        }
        if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            return self.parse_number();
        }
        if self.peek() == Some('$') {
            self.pos += '$'.len_utf8();
        }
        if self
            .peek()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        {
            return self.parse_variable();
        }
        Err(self.error("expected arithmetic value"))
    }

    fn parse_number(&mut self) -> Result<i64, ExpansionError> {
        let start = self.pos;
        while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.pos += 1;
        }
        self.source[start..self.pos]
            .parse::<i64>()
            .map_err(|_| self.error("invalid arithmetic integer"))
    }

    fn parse_variable(&mut self) -> Result<i64, ExpansionError> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            self.pos += 1;
        }
        let name = &self.source[start..self.pos];
        let Some(value) = self.session.env.get(name.as_bytes()) else {
            return Ok(0);
        };
        let text = String::from_utf8_lossy(value);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(0);
        }
        trimmed
            .parse::<i64>()
            .map_err(|_| self.error(format!("invalid arithmetic variable '{name}'")))
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|ch| ch.is_whitespace()) {
            self.pos += self.peek().unwrap().len_utf8();
        }
    }

    fn take(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn error(&self, message: impl Into<String>) -> ExpansionError {
        ExpansionError::usage(format!("arithmetic expansion: {}", message.into()))
    }
}

pub(super) fn xsh_word(word: &str) -> String {
    if !word.is_empty()
        && !word.starts_with('.')
        && word.bytes().all(|byte| {
            matches!(
                byte,
                b'A'..=b'Z'
                    | b'a'..=b'z'
                    | b'0'..=b'9'
                    | b'_'
                    | b'@'
                    | b'%'
                    | b'+'
                    | b'='
                    | b':'
                    | b','
                    | b'.'
                    | b'/'
                    | b'-'
            )
        })
    {
        return word.to_string();
    }
    let mut out = String::from("\"");
    for ch in word.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn shell_quote(word: &str) -> String {
    if word
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&byte))
    {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

pub(super) fn parse_env_assignment(word: &str) -> Option<(&str, &str)> {
    let (name, value) = word.split_once('=')?;
    valid_env_name(name).then_some((name, value))
}

pub(super) fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn valid_alias_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(char::is_whitespace) && !name.contains('=')
}

pub(super) fn validate_alias_source(name: &str, source: &str) -> Result<(), String> {
    if !valid_alias_name(name) {
        return Err("invalid alias name".to_string());
    }
    let line = ShellParser::new(source).parse_line()?;
    if line.chains.len() != 1 {
        return Err("aliases with chains are not implemented".to_string());
    }
    let chain = &line.chains[0];
    if chain.pipeline.commands.iter().any(|command| {
        command.words.len() == 1
            && command
                .words
                .first()
                .is_some_and(|word| word.text() == name)
    }) {
        return Err("aliases cannot expand to only themselves".to_string());
    }
    if chain.pipeline.commands.len() > 1
        && chain
            .pipeline
            .commands
            .iter()
            .any(|command| session_builtin(&command.words).is_some())
    {
        return Err("session builtins are not allowed in pipeline aliases".to_string());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionBuiltin {
    Fg,
    Bg,
    Noop,
    Cd,
    Set,
    Unset,
    Alias,
    Z,
    Denv,
    Clear,
    List,
    Which,
    History,
}

fn session_builtin(words: &[ShellWord]) -> Option<SessionBuiltin> {
    words
        .first()
        .and_then(|name| session_builtin_name(&name.text()))
}

fn session_builtin_name(name: &str) -> Option<SessionBuiltin> {
    Some(match name {
        "fg" => SessionBuiltin::Fg,
        "bg" => SessionBuiltin::Bg,
        ":" => SessionBuiltin::Noop,
        "cd" => SessionBuiltin::Cd,
        "set" => SessionBuiltin::Set,
        "unset" => SessionBuiltin::Unset,
        "alias" => SessionBuiltin::Alias,
        "z" => SessionBuiltin::Z,
        "denv" => SessionBuiltin::Denv,
        "c" => SessionBuiltin::Clear,
        "l" => SessionBuiltin::List,
        "w" | "which" => SessionBuiltin::Which,
        "history" => SessionBuiltin::History,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ChainOp, CompletionAction, LineBuffer, ProcessStatus, Session, ShellParser,
        ShellRedirectionKind, ShellToken, ShellWord, ShellWordPart, complete_buffer, execute_line,
        is_xsh_source, lex_shell, set_env_bytes, shell_status, validate_alias_source,
        validate_assignment_prefix,
    };
    use crate::xshi::interactive::history::History;
    use crate::xshi::interactive::session::PathCommand;
    use crate::xshi::interactive::shell::SimpleCommand;

    fn shell_word(text: &str) -> ShellWord {
        ShellWord {
            parts: vec![ShellWordPart::Text {
                text: text.to_string(),
                expand: true,
                glob: true,
            }],
        }
    }

    #[test]
    fn lexes_shell_operators_and_redirections() {
        let tokens = lex_shell("FOO=bar echo hi |& wc -c 2>&1").unwrap();
        assert!(tokens.contains(&ShellToken::PipeErr));
        assert!(tokens.contains(&ShellToken::Redir(ShellRedirectionKind::StderrToStdout)));
    }

    #[test]
    fn lexes_arithmetic_expansion_distinct_from_command_substitution() {
        let tokens = lex_shell("echo $((1 + 2)) \"x$((3 * 4))\" $(printf ok)").unwrap();
        let ShellToken::Word(word) = &tokens[1] else {
            panic!("expected arithmetic word");
        };
        assert_eq!(
            word.parts,
            vec![ShellWordPart::ArithmeticExpansion {
                source: "1 + 2".to_string(),
                glob: true,
            }]
        );
        let ShellToken::Word(word) = &tokens[2] else {
            panic!("expected quoted arithmetic word");
        };
        assert_eq!(
            word.parts,
            vec![
                ShellWordPart::Text {
                    text: "x".to_string(),
                    expand: true,
                    glob: false,
                },
                ShellWordPart::ArithmeticExpansion {
                    source: "3 * 4".to_string(),
                    glob: false,
                },
            ]
        );
        let ShellToken::Word(word) = &tokens[3] else {
            panic!("expected command substitution word");
        };
        assert!(matches!(
            word.parts.as_slice(),
            [ShellWordPart::CommandSubstitution { source, glob: true }] if source == "printf ok"
        ));
    }

    #[test]
    fn expands_shell_arithmetic() {
        let mut session = Session::new();
        set_env_bytes(&mut session.env, b"N", b"5");
        let output = execute_line(&mut session, "RESULT=$((1 + 2 * N))");
        assert_eq!(output.status, 0);
        assert_eq!(session.env.get(b"RESULT".as_slice()), Some(&b"11".to_vec()));
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
        assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    }

    #[test]
    fn parses_chains() {
        let line = ShellParser::new("false || echo ok; true")
            .parse_line()
            .unwrap();
        assert_eq!(line.chains.len(), 3);
        assert_eq!(line.chains[1].op, ChainOp::Or);
        assert_eq!(line.chains[2].op, ChainOp::Sequence);
    }

    #[test]
    fn rejects_deferred_shell_syntax() {
        assert!(
            lex_shell("echo hi # later")
                .unwrap_err()
                .contains("comments")
        );
        assert!(
            lex_shell("echo hi &> out")
                .unwrap_err()
                .contains("combined stdout/stderr")
        );
    }

    #[test]
    fn parses_trailing_background_marker() {
        let line = ShellParser::new("sleep 1 &").parse_line().unwrap();
        assert!(line.background);
        assert_eq!(line.chains.len(), 1);

        let chain = ShellParser::new("sleep 1 && echo ok &")
            .parse_line()
            .unwrap();
        assert!(chain.background);
        assert_eq!(chain.chains.len(), 2);

        assert!(ShellParser::new("&").parse_line().is_err());
        assert!(
            lex_shell("true && echo ok")
                .unwrap()
                .contains(&ShellToken::And)
        );
    }

    #[test]
    fn rejects_unsupported_background_shapes() {
        let mut session = Session::new();
        let cases = [
            (
                "/bin/true && /bin/echo ok &",
                "background jobs require one simple external command",
            ),
            (
                "/bin/echo ok | /usr/bin/wc -c &",
                "background pipelines are not supported",
            ),
            ("cd /tmp &", "session builtins cannot run in the background"),
            (
                "FOO=bar &",
                "assignment-only input cannot run in the background",
            ),
        ];

        for (source, expected) in cases {
            let output = execute_line(&mut session, source);
            assert_eq!(output.status, 2, "{source}");
            assert!(
                String::from_utf8(output.stderr).unwrap().contains(expected),
                "{source}"
            );
        }
    }

    #[test]
    fn job_control_builtins_are_reserved_before_aliases() {
        let mut session = Session::new();
        let w = execute_line(&mut session, "w fg");
        assert_eq!(w.status, 0);
        assert!(
            String::from_utf8(w.stdout)
                .unwrap()
                .contains("fg: xshi session builtin")
        );

        session
            .aliases
            .insert("fg".to_string(), "echo alias".to_string());
        session
            .aliases
            .insert("bg".to_string(), "echo alias".to_string());

        let fg = execute_line(&mut session, "fg");
        assert_eq!(fg.status, 1);
        assert!(
            String::from_utf8(fg.stderr)
                .unwrap()
                .contains("xshi: fg: no background job")
        );

        let bg = execute_line(&mut session, "bg");
        assert_eq!(bg.status, 1);
        assert!(
            String::from_utf8(bg.stderr)
                .unwrap()
                .contains("xshi: bg: no background job")
        );
    }

    #[test]
    fn rejects_invalid_leading_env_assignment() {
        let command = SimpleCommand {
            words: vec![shell_word("BAD-NAME=value"), shell_word("echo")],
            redirections: Vec::new(),
        };
        assert!(validate_assignment_prefix(&command).is_err());

        let command = SimpleCommand {
            words: vec![shell_word("echo"), shell_word("BAD-NAME=value")],
            redirections: Vec::new(),
        };
        assert!(validate_assignment_prefix(&command).is_ok());
    }

    #[test]
    fn bool_classification_keeps_commands_and_expressions_distinct() {
        assert!(!is_xsh_source("false"));
        assert!(!is_xsh_source("true && echo ok"));
        assert!(is_xsh_source("false or true"));
        assert!(is_xsh_source("true == false"));
    }

    #[test]
    fn module_classification_does_not_capture_paths_with_dots() {
        assert!(is_xsh_source("fs.write /tmp/x \"ok\""));
        assert!(!is_xsh_source(
            "/src/target/repo/.work/muon-0.5.0/build/tool --flag"
        ));
        assert!(!is_xsh_source("./tool.with.dot --flag"));
    }

    #[test]
    fn history_session_builtin_prints_numbered_entries_with_optional_limit() {
        let mut session = Session::new();
        session.history = History::from_entries(vec![
            "echo one".to_string(),
            "echo two".to_string(),
            "echo three".to_string(),
        ]);

        let output = execute_line(&mut session, "history 2");

        assert_eq!(output.status, 0);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "    2  echo two\n    3  echo three\n"
        );
    }

    #[test]
    fn shell_guidance_shims_do_not_shadow_xsh_list_expressions() {
        let mut session = Session::new();

        let list = execute_line(&mut session, "[1, 2]");
        assert_eq!(list.status, 0);
    }

    #[test]
    fn rejects_recursive_alias_sources() {
        assert!(validate_alias_source("gs", "git status").is_ok());
        assert!(validate_alias_source("rg", "rg -S").is_ok());
        assert!(validate_alias_source("loop", "loop").is_err());
    }

    #[test]
    fn line_buffer_moves_by_utf8_boundaries() {
        let mut buffer = LineBuffer::default();
        buffer.insert("aé中");
        buffer.move_left();
        assert_eq!(buffer.cursor, "aé".len());
        buffer.backspace();
        assert_eq!(buffer.text, "a中");
        assert_eq!(buffer.cursor, "a".len());
    }

    #[test]
    fn completion_uses_env_prefixes() {
        let mut session = Session::new();
        session.env.clear();
        set_env_bytes(&mut session.env, b"EDITOR", b"vim");
        set_env_bytes(&mut session.env, b"ENV", b"prod");
        let mut buffer = LineBuffer::default();
        buffer.insert("$ED");
        complete_buffer(&session, &mut buffer, 80);
        assert_eq!(buffer.text, "$EDITOR");
    }

    #[test]
    fn completion_displays_ambiguous_path_candidates() {
        let root = std::env::temp_dir().join(format!("xshi-completion-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Cargo.lock"), "").unwrap();
        std::fs::write(root.join("Cargo.toml"), "").unwrap();

        let mut session = Session::new();
        session.cwd = root.clone();
        session.refresh_cwd_snapshot();
        let mut buffer = LineBuffer::default();
        buffer.insert("ls Cargo.");
        let action = complete_buffer(&session, &mut buffer, 80);

        let CompletionAction::Display(state) = action else {
            panic!("expected ambiguous path display");
        };
        let candidates = (0..state.comp.len())
            .map(|index| state.comp.name(index).to_string())
            .collect::<Vec<_>>();
        assert!(candidates.contains(&"Cargo.lock".to_string()));
        assert!(candidates.contains(&"Cargo.toml".to_string()));
        assert_eq!(buffer.text, "ls Cargo.");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn completion_after_privilege_commands_uses_command_candidates() {
        let mut session = Session::new();
        session.path_commands.push(PathCommand {
            name: "echo".to_string(),
        });
        for source in ["sudo ech", "su ech"] {
            let mut buffer = LineBuffer::default();
            buffer.insert(source);

            complete_buffer(&session, &mut buffer, 80);

            assert_eq!(buffer.text, source.replace("ech", "echo"));
        }
    }

    #[test]
    fn maps_exec_statuses_to_shell_codes() {
        let mut status =
            ProcessStatus::from_segments(vec![xsh::runtime::process::ProcessSegmentStatus {
                index: 0,
                target: b"missing".to_vec(),
                pid: None,
                success: false,
                kind: xsh::runtime::process::ProcessSegmentStatusKind::Exec,
                code: None,
                error_kind: Some("not-found".to_string()),
                error_message: Some("executable not found".to_string()),
            }]);
        assert_eq!(shell_status(&status), 127);
        status.segments[0].error_kind = Some("permission-denied".to_string());
        assert_eq!(shell_status(&status), 126);
    }
}
