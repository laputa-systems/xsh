#![allow(clippy::single_call_fn)]

use super::alias::{self, AliasMap};
use super::builtin;
use super::config::{load_config, load_config_path, load_profile};
use super::denv::{self, DenvCommand};
use super::repl::{self, ReadResult, Ui};
use super::session::{InteractiveJob, InteractiveJobState, Session, set_env_bytes, stdio_is_tty};
use super::shell::{
    Chain, ChainOp, PipeOp, Pipeline, RedirectionKind as ShellRedirectionKind, ShellLine,
    ShellParser, ShellToken, ShellWord, ShellWordPart, SimpleCommand, expand_glob, has_glob_meta,
    lex_shell, shell_line_source,
};
use super::signal;
use rustix::termios::{self as rtermios, OptionalActions, Termios};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use xsh::diagnostic::DiagnosticRenderer;
use xsh::execution::evaluator::Evaluator;
use xsh::execution::value::RunError;
use xsh::frontend::check::Checker;
use xsh::frontend::load::{entry_source_from_text, parse_load_entry_source_arena_only};
use xsh::frontend::source::{SourceId, SourceMap, Span};
use xsh::frontend::syntax::arena::ArenaProgram;
use xsh::frontend::syntax::node::RunKind;
use xsh::process::{
    CancellationDecision, CancellationPolicy, ChildWaitOutcome, FileRedirectionMode,
    ForegroundTerminal, ManagedStdio, ProcessGroup, ProcessGroupConfig, ProcessInvocation,
    ProcessRedirection, ProcessSegmentStatus, ProcessSegmentStatusKind, ProcessStatus,
    ProcessStatusKind, RedirectionStream, SpawnManagedOptions, WaitMode,
    initialize_interactive_process_group, poll_managed, run_capture_with_policy,
    run_pipeline_inherit_with_policy, spawn_managed, wait_managed,
};

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
    /// An explicit config file, which replaces the default one.
    pub config_path: Option<std::path::PathBuf>,
    pub load_profile: bool,
    pub require_tty: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            load_config: true,
            config_path: None,
            load_profile: false,
            require_tty: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OneCommandOptions {
    pub load_config: bool,
    pub config_path: Option<std::path::PathBuf>,
    pub load_profile: bool,
}

impl Default for OneCommandOptions {
    fn default() -> Self {
        Self {
            load_config: true,
            config_path: None,
            load_profile: false,
        }
    }
}

pub(super) struct CommandOutput {
    status: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    process_status: Option<ProcessStatus>,
    history_source: Option<String>,
}

impl CommandOutput {
    pub(super) fn output_len(&self) -> usize {
        self.status.unsigned_abs() as usize + self.stdout.len() + self.stderr.len()
    }
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
    pub(super) fn message(&self) -> &str {
        &self.message
    }

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
    if let Some(path) = &options.config_path {
        load_config_path(&mut session, path, true, &mut stderr);
    } else if options.load_config {
        load_config(&mut session, &mut stderr);
    }
    session.sync_prompt_identity();
    denv::startup(&mut session, &mut stderr);
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
    let signal_fd = if options.require_tty {
        signal::init()
    } else {
        -1
    };
    let mut ui = Ui::new(signal_fd);
    let mut line = String::new();

    loop {
        session.history.sync();
        reap_interactive_job(&mut session, &mut stderr);
        let command = if options.require_tty {
            match repl::read_line(&mut session, &mut ui) {
                ReadResult::Line(command) => command,
                ReadResult::Empty => continue,
                ReadResult::Exit => {
                    session.history.compact();
                    return 0;
                }
            }
        } else {
            let stdin = io::stdin();
            let mut input = stdin.lock();
            let mut prompt_text = String::new();
            let pwd = session
                .env
                .get(b"PWD".as_slice())
                .map(|value| String::from_utf8_lossy(value).into_owned())
                .unwrap_or_default();
            session.prompt.render_into(
                &mut prompt_text,
                session.last_status,
                &pwd,
                session.denv_dirty(),
            );
            if write!(stdout, "{prompt_text}")
                .and_then(|()| stdout.flush())
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
                return 0;
            }
            line.trim_end_matches(['\n', '\r']).to_string()
        };

        if command.trim().is_empty() {
            continue;
        }
        match handle_submitted_line(&mut session, &mut ui, &command, &mut stdout, &mut stderr) {
            LineOutcome::Continue => {}
            LineOutcome::Exit(code) => return code,
        }
    }
}

enum LineOutcome {
    Continue,
    Exit(i32),
}

/// Runs one submitted line: interactive rewrites, the session-level commands
/// that need terminal state, then the normal execution path, and records the
/// line in history.
fn handle_submitted_line(
    session: &mut Session,
    ui: &mut Ui,
    line: &str,
    stdout: &mut io::Stdout,
    stderr: &mut io::Stderr,
) -> LineOutcome {
    // Only another `exit` keeps the warning armed; the reference shell clears
    // it before looking at the line, so typing `exit` twice never forces.
    let is_exit = !is_xsh_source(line) && line.split_whitespace().next() == Some("exit");
    ui.exit_warned &= is_exit;

    let xsh_source = is_xsh_source(line);
    let home = session
        .env
        .get(b"HOME".as_slice())
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .unwrap_or_default();
    let line = if xsh_source {
        line.to_string()
    } else {
        maybe_rewrite_cd(line, &session.aliases, &home)
    };

    // Aliases are expanded before recording so `g status` is stored as the
    // `git status` that actually ran, and relative cd targets are made
    // absolute so `z` can use them.
    let history_line = if xsh_source {
        line.clone()
    } else {
        resolve_cd_for_history(&session.aliases.expand_line(&line))
    };
    let history_cwd = session.cwd.clone();

    ui.session_log.push_str(&line);
    ui.session_log.push('\n');

    let first_word = line.split_whitespace().next().unwrap_or("");
    if !xsh_source {
        match first_word {
            "exit" => {
                if let Some(code) = handle_exit_command(session, ui, &line, stderr) {
                    return LineOutcome::Exit(code);
                }
                session
                    .history
                    .add_in_dir(&history_line, Some(&history_cwd));
                return LineOutcome::Continue;
            }
            "exec" => {
                // The command replaces the shell: run it, then leave with its
                // status. A command that cannot start leaves the shell running,
                // as a failed `exec` does.
                let command = line.trim_start().strip_prefix("exec").unwrap_or("").trim();
                if command.is_empty() {
                    return LineOutcome::Continue;
                }
                let output = execute_line(session, command);
                let _ = stdout.write_all(&output.stdout);
                let _ = stdout.flush();
                let _ = stderr.write_all(&output.stderr);
                session
                    .history
                    .add_in_dir(&history_line, Some(&history_cwd));
                if matches!(output.status, 126 | 127) {
                    session.last_status = output.status;
                    return LineOutcome::Continue;
                }
                session.history.compact();
                return LineOutcome::Exit(output.status);
            }
            "alias" => {
                let words = alias::lex_words(line.trim_start().strip_prefix("alias").unwrap_or(""));
                let mut out = Vec::new();
                let mut err = Vec::new();
                let status = run_alias(session, &words, &mut out, &mut err);
                let _ = stdout.write_all(&out);
                let _ = stdout.flush();
                let _ = stderr.write_all(&err);
                let _ = stderr.flush();
                session.last_status = status;
                session.last_process_status = Some(ProcessStatus::exited(status));
                session
                    .history
                    .add_in_dir(&history_line, Some(&history_cwd));
                return LineOutcome::Continue;
            }
            "copy-scrollback" => {
                let encoded = base64_encode(ui.session_log.as_bytes());
                let _ = stdout.write_all(format!("\x1b]52;c;{encoded}\x07").as_bytes());
                let _ = stdout.flush();
                session.last_status = 0;
                session.last_process_status = Some(ProcessStatus::exited(0));
                session
                    .history
                    .add_in_dir(&history_line, Some(&history_cwd));
                return LineOutcome::Continue;
            }
            "xshi-dump" => {
                let (status, out, err) = repl::write_layout_dump(session, ui);
                let _ = stdout.write_all(out.as_bytes());
                let _ = stdout.flush();
                let _ = stderr.write_all(err.as_bytes());
                let _ = stderr.flush();
                session.last_status = status;
                session.last_process_status = Some(ProcessStatus::exited(status));
                session
                    .history
                    .add_in_dir(&history_line, Some(&history_cwd));
                return LineOutcome::Continue;
            }
            _ => {}
        }
    }

    // Commands the shell answers itself never leave the terminal mid-line;
    // everything else may have written arbitrary output.
    let intercepted = !xsh_source && is_intercepted_line(&line);

    let dollar_before = session.dollar_status;
    let output = execute_line(session, &line);
    // A command stopped by Ctrl-Z never finished, so it leaves `$?` alone.
    let stopped = output.status == 148
        && session
            .job
            .as_ref()
            .is_some_and(|job| job.state == InteractiveJobState::Stopped);
    if intercepted || stopped {
        session.dollar_status = dollar_before;
    } else {
        session.dollar_status = output.status;
    }
    let _ = stdout.write_all(&output.stdout);
    let _ = stdout.flush();
    let _ = stderr.write_all(&output.stderr);
    let _ = stderr.flush();
    session.last_status = output.status;
    session.last_process_status = match output.process_status {
        Some(status) => Some(status),
        None => Some(ProcessStatus::exited(output.status)),
    };
    session.prompt.invalidate_git();

    let reset_history = first_word == "history" && line.split_whitespace().nth(1) == Some("reset");
    if history_line.trim() != "l" && !reset_history {
        let recorded = output.history_source.as_deref().filter(|_| xsh_source);
        session
            .history
            .add_in_dir(recorded.unwrap_or(&history_line), Some(&history_cwd));
    }
    if !intercepted {
        ui.prompt_needs_line_start = true;
    }
    LineOutcome::Continue
}

/// Whether the line's first command is answered by the shell without running
/// a program, in which case it cannot leave the terminal mid-line.
fn is_intercepted_line(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    let simple = || {
        lex_shell(line)
            .map(|tokens| {
                tokens
                    .iter()
                    .all(|token| matches!(token, ShellToken::Word(_)))
            })
            .unwrap_or(false)
    };
    match first {
        "alias" | "history" | "denv" | "fg" | "z" | "w" | "which" | "type" | "c" => true,
        "cd" | "l" => simple(),
        _ => false,
    }
}

/// `source FILE` / `. FILE`: runs the file's lines as if typed, in this shell.
fn builtin_source(
    session: &mut Session,
    args: &[String],
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> i32 {
    let Some(filename) = args.first() else {
        writeln!(stderr, ".: filename argument required").ok();
        return 2;
    };
    let path = session.cwd.join(filename);
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            writeln!(stderr, ".: {filename}: {err}").ok();
            return 127;
        }
    };
    let mut status = 0;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let output = execute_line(session, trimmed);
        stdout.extend_from_slice(&output.stdout);
        stderr.extend_from_slice(&output.stderr);
        status = output.status;
        session.last_status = status;
    }
    status
}

/// `cd [DIR]`, `cd -`: reports failures as `xshi: cd: ...` with status 1.
fn builtin_cd(session: &mut Session, args: &[String], stderr: &mut Vec<u8>) -> i32 {
    if args.len() > 1 {
        writeln!(stderr, "xshi: cd: too many arguments").ok();
        return 1;
    }
    if args.first().map(String::as_str) == Some("-") {
        let previous = session
            .var(b"OLDPWD")
            .filter(|value| !value.is_empty())
            .map(|value| PathBuf::from(OsString::from_vec(value.to_vec())));
        return match previous {
            Some(previous) => {
                writeln!(stderr, "{}", previous.display()).ok();
                change_directory_path(session, &previous, stderr)
            }
            None => {
                writeln!(stderr, "xshi: cd: no previous directory").ok();
                1
            }
        };
    }
    let target = match args.first() {
        Some(target) => target.clone(),
        None => session
            .var(b"HOME")
            .map(|home| String::from_utf8_lossy(home).into_owned())
            .unwrap_or_default(),
    };
    let resolved = resolve_tilde(session, &target);
    change_directory_path(session, Path::new(&resolved), stderr)
}

/// `~` and `~/rest` resolve against `$HOME`; other words pass through.
fn resolve_tilde(session: &Session, target: &str) -> String {
    let home = || {
        session
            .var(b"HOME")
            .map(|home| String::from_utf8_lossy(home).into_owned())
            .unwrap_or_default()
    };
    if target == "~" || target.is_empty() {
        home()
    } else if let Some(rest) = target.strip_prefix("~/") {
        format!("{}/{rest}", home())
    } else {
        target.to_string()
    }
}

fn change_directory_path(session: &mut Session, target: &Path, stderr: &mut Vec<u8>) -> i32 {
    match session.set_cwd(target.to_path_buf()) {
        Ok(()) => {
            denv::after_cwd_change(session, stderr);
            0
        }
        Err(err) => {
            writeln!(stderr, "xshi: cd: {}: {err}", target.display()).ok();
            1
        }
    }
}

/// `set`: with no arguments lists the exported environment; `set NAME value...`
/// sets and exports `NAME` (extra words join with spaces); option forms such
/// as `-e` are accepted.
fn set_builtin(session: &mut Session, args: &[String], stdout: &mut Vec<u8>) -> i32 {
    let Some(first) = args.first() else {
        for (name, value) in &session.env {
            writeln!(
                stdout,
                "{}={}",
                String::from_utf8_lossy(name),
                String::from_utf8_lossy(value)
            )
            .ok();
        }
        return 0;
    };
    if first.starts_with('-') || first.starts_with('+') {
        return 0;
    }
    if !valid_env_name(first) {
        // Anything else sets positional parameters, which this shell does not keep.
        return 0;
    }
    let value = args[1..].join(" ");
    session.export_var(first.as_bytes(), value.as_bytes());
    0
}

/// `alias`: defines, shows, or lists aliases from raw shell words.
fn run_alias(
    session: &mut Session,
    words: &[String],
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> i32 {
    if words.len() >= 2 {
        session.aliases.set(words[0].clone(), words[1..].to_vec());
        0
    } else if let [name] = words {
        if let Some(expansion) = session.aliases.get(name) {
            writeln!(stdout, "alias {name} {}", expansion.join(" ")).ok();
            0
        } else {
            writeln!(stderr, "xshi: alias: not found: {name}").ok();
            1
        }
    } else {
        let mut entries: Vec<_> = session.aliases.iter().collect();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (name, expansion) in entries {
            writeln!(stdout, "alias {name} {}", expansion.join(" ")).ok();
        }
        0
    }
}

/// `exit`: returns `Some(code)` when the shell should exit. With a live job
/// the first attempt warns and the second force-quits.
fn handle_exit_command(
    session: &mut Session,
    ui: &mut Ui,
    line: &str,
    stderr: &mut io::Stderr,
) -> Option<i32> {
    if session.job.is_some() {
        if ui.exit_warned {
            terminate_job(session);
            return Some(0);
        }
        let _ = writeln!(
            stderr,
            "xshi: there is a suspended job. Exit again to force quit."
        );
        ui.exit_warned = true;
        session.last_status = 1;
        session.last_process_status = Some(ProcessStatus::exited(1));
        return None;
    }
    let code: i32 = line
        .split_whitespace()
        .nth(1)
        .and_then(|word| word.parse().ok())
        .unwrap_or(0);
    session.history.compact();
    Some(code)
}

/// Sends SIGTERM to the managed job's process group and forgets it.
pub(super) fn terminate_job(session: &mut Session) {
    if let Some(job) = session.job.take() {
        let group = ProcessGroup::from_pgid(job.pgid);
        group.signal(libc::SIGTERM);
        // A stopped process acts on SIGTERM only once it is continued.
        group.signal(libc::SIGCONT);
    }
}

/// Changes directory (with `~` resolution) and runs the shared post-cd hooks.
/// Returns 0 on success, 1 on failure.
pub(super) fn change_directory(session: &mut Session, target: &str) -> i32 {
    let resolved = resolve_tilde(session, target);
    let mut stderr = Vec::new();
    let status = change_directory_path(session, Path::new(&resolved), &mut stderr);
    let _ = io::stderr().write_all(&stderr);
    status
}

/// Rewrites shorthand cd forms: `..` stays, `...` becomes `cd ../..`, and so
/// on; a single word naming a directory (and not an alias, builtin, or
/// executable) becomes `cd <word>`.
fn maybe_rewrite_cd(line: &str, aliases: &AliasMap, home: &str) -> String {
    let trimmed = line.trim();
    let Ok(tokens) = lex_shell(trimmed) else {
        return line.to_string();
    };
    let mut words = Vec::new();
    for token in &tokens {
        match token {
            ShellToken::Word(word) => words.push(word),
            _ => return line.to_string(),
        }
    }
    let Some(first) = words.first() else {
        return line.to_string();
    };
    let first_text = first.text();
    let first_quoted = first
        .parts
        .iter()
        .any(|part| matches!(part, ShellWordPart::Text { expand: false, .. }));

    // Plain cd commands pass through.
    if !first_quoted && first_text == "cd" {
        return line.to_string();
    }

    // Dot-dot shorthand: ".." is already valid, "..." is "../..", and so on.
    if words.len() == 1
        && !first_quoted
        && first_text.len() >= 2
        && first_text.bytes().all(|byte| byte == b'.')
    {
        let levels = first_text.len() - 1;
        let path = (0..levels).map(|_| "..").collect::<Vec<_>>().join("/");
        return format!("cd {path}");
    }

    // Implicit cd: a single word that is a directory and not a builtin,
    // alias, or executable.
    if words.len() != 1 || builtin::is_builtin(&first_text) || aliases.get(&first_text).is_some() {
        return line.to_string();
    }
    let expanded = if let Some(rest) = first_text.strip_prefix('~') {
        format!("{home}{rest}")
    } else {
        first_text
    };
    let path = Path::new(&expanded);
    if path.is_dir() && !is_executable_file(path) {
        return format!("cd {trimmed}");
    }
    line.to_string()
}

/// Resolves relative `cd`/`z` paths to absolute ones for history so `z` can
/// match them: `cd src` becomes `cd /abs/src`, while `cd ~/d` stays as is.
fn resolve_cd_for_history(line: &str) -> String {
    let trimmed = line.trim();
    let (prefix, rest) = if let Some(rest) = trimmed.strip_prefix("cd ") {
        ("cd ", rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix("z ") {
        ("z ", rest.trim_start())
    } else {
        return line.to_string();
    };

    // Already absolute or tilde-prefixed: no resolution needed.
    if rest.starts_with('/') || rest.starts_with('~') || rest == "-" || rest.is_empty() {
        return line.to_string();
    }

    // Split at the first whitespace or operator to get just the path argument.
    let (path_arg, suffix) =
        match rest.find(|c: char| c.is_whitespace() || c == '&' || c == '|' || c == ';') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };

    if let Ok(pwd) = std::env::current_dir() {
        let resolved = pwd.join(path_arg);
        if let Ok(canonical) = resolved.canonicalize() {
            return format!("{prefix}{}{suffix}", canonical.display());
        }
        // canonicalize failed (the directory may not exist): use the joined path.
        return format!("{prefix}{}{suffix}", resolved.display());
    }

    line.to_string()
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).map_or(0, |byte| u32::from(*byte));
        let b2 = chunk.get(2).map_or(0, |byte| u32::from(*byte));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub(super) fn execute_line(session: &mut Session, source: &str) -> CommandOutput {
    if is_xsh_source(source) {
        return run_xsh_source(session, "<interactive>", source);
    }

    // Aliases expand textually on the first word of the whole line, unless
    // that word names a session builtin, which always wins.
    let expanded;
    let source = if first_word_is_session_builtin(source) {
        source
    } else {
        expanded = session.aliases.expand_line(source);
        expanded.as_ref()
    };

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
    // Prepare through the loader so each submitted input carries the embedded
    // standard-library implementations it can reach. The preparation boundary
    // is this call: preparation happens before the input executes and never
    // during it.
    let entry_source = entry_source_from_text(source_name, text.to_string());
    let source_id = entry_source.source_id;
    let (sources, parsed) =
        parse_load_entry_source_arena_only(source_name, entry_source, Vec::new());

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
            config_path: None,
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
    if let Some(path) = &options.config_path {
        load_config_path(&mut session, path, true, &mut stderr);
    } else if options.load_config {
        load_config(&mut session, &mut stderr);
    }
    denv::startup(&mut session, &mut stderr);
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

    let last_status = session.last_status;
    let last_process_status = session.last_process_status.clone();
    run_chains(session, line.chains, last_status, last_process_status)
}

/// Runs the chains of a list in order. A command that stops (Ctrl-Z) becomes
/// the suspended job and the rest of the list waits in it for `fg`.
fn run_chains(
    session: &mut Session,
    chains: Vec<Chain>,
    mut last_status: i32,
    mut last_process_status: Option<ProcessStatus>,
) -> CommandOutput {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let mut chains = chains.into_iter();
    while let Some(chain) = chains.next() {
        let should_run = match chain.op {
            ChainOp::Start | ChainOp::Sequence => true,
            ChainOp::And => last_status == 0,
            ChainOp::Or => last_status != 0,
        };
        if !should_run {
            continue;
        }
        let job_before = session.job.as_ref().map(|job| job.pid);
        let output = execute_pipeline(session, chain.pipeline);
        stdout.extend(output.stdout);
        stderr.extend(output.stderr);
        last_status = output.status;
        last_process_status = output.process_status;
        session.last_status = last_status;
        session.dollar_status = last_status;
        if last_process_status.is_some() {
            session.last_process_status = last_process_status.clone();
        }
        let stopped_here = last_status == 148
            && session.job.as_ref().is_some_and(|job| {
                job.state == InteractiveJobState::Stopped && Some(job.pid) != job_before
            });
        if stopped_here {
            if let Some(job) = session.job.as_mut() {
                job.continuation = chains.by_ref().collect();
            }
            break;
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
    let command = chain.pipeline.commands.into_iter().next().unwrap();
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
            session.job = Some(InteractiveJob {
                child,
                pid,
                pgid,
                command: display.clone(),
                state: InteractiveJobState::RunningBackground,
                terminal_attrs: None,
                last_status: None,
                notified: false,
                continuation: Vec::new(),
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

fn execute_pipeline(session: &mut Session, pipeline: Pipeline) -> CommandOutput {
    if pipeline.commands.len() == 1 {
        return execute_simple_command(session, pipeline.commands.into_iter().next().unwrap());
    }

    if pipeline
        .commands
        .iter()
        .any(|command| is_builtin_stage(session, command))
    {
        return execute_mixed_pipeline(session, pipeline);
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

    run_external_pipeline(session, &pipeline.commands, &pipeline.pipes)
}

/// Runs a pipeline of programs. `|&` also sends the left program's standard
/// error into the pipe.
fn run_external_pipeline(
    session: &mut Session,
    commands: &[SimpleCommand],
    pipes: &[PipeOp],
) -> CommandOutput {
    let mut invocations = Vec::with_capacity(commands.len());
    for (index, command) in commands.iter().enumerate() {
        let mut invocation = match external_invocation(session, command) {
            Ok(invocation) => invocation,
            Err(error) => return expansion_error_output(error),
        };
        if pipes.get(index) == Some(&PipeOp::StdoutStderr) {
            invocation.redirections.insert(
                0,
                ProcessRedirection::ChildDup {
                    stream: RedirectionStream::Stderr,
                    fd: 1,
                },
            );
        }
        invocations.push(invocation);
    }

    // Inside a command substitution the last program writes to a pipe the
    // shell reads, through /dev/fd so the process layer needs no new option.
    let mut capture = None;
    if session.capturing {
        match StdoutCapture::new() {
            Ok(sink) => {
                if let Some(last) = invocations.last_mut() {
                    // The sink is the program's standard output before any
                    // redirection of its own: `2>&1` reaches it, `> file` wins.
                    last.redirections.insert(
                        0,
                        ProcessRedirection::File {
                            stream: RedirectionStream::Stdout,
                            mode: FileRedirectionMode::Write,
                            path: PathBuf::from(&sink.path),
                        },
                    );
                }
                capture = Some(sink);
            }
            Err(error) => {
                return CommandOutput {
                    status: 1,
                    stdout: Vec::new(),
                    stderr: text_bytes(format!("xshi: pipe: {error}\n")),
                    process_status: Some(ProcessStatus::exited(1)),
                    history_source: None,
                };
            }
        }
    }

    let mut policy = InteractiveNoCancellation;
    let end = run_pipeline_inherit_with_policy(&invocations, &mut policy);
    let stdout = capture.map(StdoutCapture::finish).unwrap_or_default();
    match end {
        Ok(end) => {
            let status = end.status.unwrap_or_else(|| ProcessStatus::exited(0));
            let mut stderr = Vec::new();
            for segment in &status.segments {
                if segment.kind == ProcessSegmentStatusKind::Exec {
                    let kind = segment.error_kind.clone().unwrap_or_default();
                    let mut error =
                        RunError::new(kind, segment.error_message.clone().unwrap_or_default());
                    if let Some(message) =
                        exec_failure_message(session, &segment.target, &mut error)
                    {
                        stderr.extend_from_slice(message.as_bytes());
                    }
                }
            }
            status_from_process_output(CommandOutput {
                status: 0,
                stdout,
                stderr,
                process_status: Some(status),
                history_source: None,
            })
        }
        Err(error) => CommandOutput {
            status: 1,
            stdout,
            stderr: text_bytes(format!("xshi: {}\n", error.message)),
            process_status: Some(ProcessStatus::exited(1)),
            history_source: None,
        },
    }
}

/// A pipe that collects a pipeline's standard output for a command substitution.
struct StdoutCapture {
    path: String,
    _write: rustix::fd::OwnedFd,
    reader: std::thread::JoinHandle<Vec<u8>>,
}

impl StdoutCapture {
    fn new() -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let (read, write) = cloexec_pipe()?;
        let path = format!("/dev/fd/{}", write.as_raw_fd());
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = io::Read::read_to_end(&mut fs::File::from(read), &mut bytes);
            bytes
        });
        Ok(Self {
            path,
            _write: write,
            reader,
        })
    }

    /// Closes the write end (ending the reader) and returns what was captured.
    fn finish(self) -> Vec<u8> {
        let Self { _write, reader, .. } = self;
        drop(_write);
        reader.join().unwrap_or_default()
    }
}

fn is_builtin_stage(session: &Session, command: &SimpleCommand) -> bool {
    let leading = command
        .words
        .iter()
        .take_while(|word| parse_env_assignment(&word.text()).is_some())
        .count();
    let Some(first) = command.words.get(leading) else {
        return false;
    };
    let Ok(name) = expand_word_to_string(session, first) else {
        return false;
    };
    session_builtin_name(&name).is_some() || internal_command_name(&name).is_some()
}

/// Runs one builtin as a pipeline stage. Builtins that change shell state run
/// in a detached copy, as in a subshell; display builtins read the live session.
fn run_builtin_stage(session: &mut Session, command: &SimpleCommand) -> CommandOutput {
    let leading = command
        .words
        .iter()
        .take_while(|word| parse_env_assignment(&word.text()).is_some())
        .count();
    let words = &command.words[leading..];
    let name = words
        .first()
        .and_then(|word| expand_word_to_string(session, word).ok())
        .unwrap_or_default();
    let args = match expand_command_words(session, &words[1..]) {
        Ok(args) => args,
        Err(error) => return expansion_error_output(error),
    };
    let run = if let Some(kind) = internal_command_name(&name) {
        run_internal(session, kind, &args)
    } else {
        let kind = session_builtin_name(&name).expect("stage was classified as a builtin");
        let mutating = matches!(
            kind,
            SessionBuiltin::Cd
                | SessionBuiltin::Unset
                | SessionBuiltin::Eval
                | SessionBuiltin::Source
                | SessionBuiltin::Z
                | SessionBuiltin::Denv
                | SessionBuiltin::Fg
                | SessionBuiltin::Bg
        ) || (matches!(
            kind,
            SessionBuiltin::Alias | SessionBuiltin::Export | SessionBuiltin::Set
        ) && !args.is_empty());
        let output = if mutating {
            let mut detached = session.fork_for_substitution();
            detached.capturing = false;
            execute_session_builtin(&mut detached, kind, &args)
        } else {
            execute_session_builtin(session, kind, &args)
        };
        BuiltinRun {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        }
    };
    apply_builtin_redirections(session, &command.redirections, run)
}

/// A pipe neither end of which leaks into spawned programs: a stray inherited
/// write end would keep a reader from ever seeing end of file.
fn cloexec_pipe() -> io::Result<(rustix::fd::OwnedFd, rustix::fd::OwnedFd)> {
    let (read, write) = rustix::pipe::pipe()?;
    rustix::io::fcntl_setfd(&read, rustix::io::FdFlags::CLOEXEC)?;
    rustix::io::fcntl_setfd(&write, rustix::io::FdFlags::CLOEXEC)?;
    Ok((read, write))
}

/// Bytes handed to a program's standard input through `/dev/fd/N`.
struct StdinFeed {
    path: String,
    _read: rustix::fd::OwnedFd,
    writer: Option<std::thread::JoinHandle<()>>,
}

impl StdinFeed {
    fn new(bytes: Vec<u8>) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let (read, write) = cloexec_pipe()?;
        let path = format!("/dev/fd/{}", read.as_raw_fd());
        let writer = std::thread::spawn(move || {
            let mut file = fs::File::from(write);
            let _ = file.write_all(&bytes);
        });
        Ok(Self {
            path,
            _read: read,
            writer: Some(writer),
        })
    }
}

impl Drop for StdinFeed {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

fn literal_word(text: &str) -> ShellWord {
    ShellWord {
        parts: vec![ShellWordPart::Text {
            text: text.to_string(),
            expand: false,
            glob: false,
        }],
    }
}

fn with_redirection(
    mut command: SimpleCommand,
    kind: ShellRedirectionKind,
    path: &str,
) -> SimpleCommand {
    command.redirections.insert(
        0,
        super::shell::Redirection {
            kind,
            target: literal_word(path),
        },
    );
    command
}

/// A pipeline containing builtins. Stages up to the last builtin run in order,
/// each seeing the previous stage's output as its input; the programs after it
/// run as an ordinary pipeline fed by that builtin's output.
fn execute_mixed_pipeline(session: &mut Session, pipeline: Pipeline) -> CommandOutput {
    let Pipeline { commands, pipes } = pipeline;
    let last_builtin = commands
        .iter()
        .rposition(|command| is_builtin_stage(session, command))
        .expect("caller found a builtin stage");
    let mut stderr = Vec::new();
    let mut statuses = Vec::new();
    let mut carry: Option<Vec<u8>> = None;

    for (index, command) in commands.iter().enumerate().take(last_builtin + 1) {
        let merge_stderr = pipes
            .get(index)
            .is_some_and(|pipe| *pipe == PipeOp::StdoutStderr);
        if is_builtin_stage(session, command) {
            let output = run_builtin_stage(session, command);
            statuses.push(output.status);
            let mut stdout = output.stdout;
            if merge_stderr {
                stdout.extend_from_slice(&output.stderr);
            } else {
                stderr.extend_from_slice(&output.stderr);
            }
            carry = Some(stdout);
        } else {
            let mut stage = command.clone();
            let feed = if index == 0 {
                None
            } else {
                match StdinFeed::new(carry.take().unwrap_or_default()) {
                    Ok(feed) => {
                        stage = with_redirection(stage, ShellRedirectionKind::Stdin, &feed.path);
                        Some(feed)
                    }
                    Err(error) => {
                        return CommandOutput {
                            status: 1,
                            stdout: Vec::new(),
                            stderr: text_bytes(format!("xshi: pipe: {error}\n")),
                            process_status: Some(ProcessStatus::exited(1)),
                            history_source: None,
                        };
                    }
                }
            };
            // Nothing after this stage reads its output.
            stage = with_redirection(stage, ShellRedirectionKind::StdoutWrite, "/dev/null");
            let output = execute_simple_command(session, stage);
            drop(feed);
            statuses.push(output.status);
            stderr.extend_from_slice(&output.stderr);
            carry = None;
        }
    }

    let final_stdout = if last_builtin + 1 == commands.len() {
        carry.take().unwrap_or_default()
    } else {
        let mut suffix: Vec<SimpleCommand> = commands[last_builtin + 1..].to_vec();
        let feed = match StdinFeed::new(carry.take().unwrap_or_default()) {
            Ok(feed) => feed,
            Err(error) => {
                return CommandOutput {
                    status: 1,
                    stdout: Vec::new(),
                    stderr: text_bytes(format!("xshi: pipe: {error}\n")),
                    process_status: Some(ProcessStatus::exited(1)),
                    history_source: None,
                };
            }
        };
        let first = suffix.remove(0);
        suffix.insert(
            0,
            with_redirection(first, ShellRedirectionKind::Stdin, &feed.path),
        );
        let output = execute_pipeline(
            session,
            Pipeline {
                commands: suffix,
                pipes: pipes[last_builtin + 1..].to_vec(),
            },
        );
        drop(feed);
        statuses.push(output.status);
        stderr.extend_from_slice(&output.stderr);
        output.stdout
    };

    // Like a pipeline of programs, the last stage decides.
    let status = statuses.last().copied().unwrap_or(0);
    CommandOutput {
        status,
        stdout: final_stdout,
        stderr,
        process_status: Some(ProcessStatus::exited(status)),
        history_source: None,
    }
}

fn execute_simple_command(session: &mut Session, command: SimpleCommand) -> CommandOutput {
    if command.words.is_empty() {
        return CommandOutput {
            status: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            process_status: Some(ProcessStatus::exited(0)),
            history_source: None,
        };
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
        let output = execute_session_builtin(session, kind, &args);
        return apply_builtin_redirections(
            session,
            &command.redirections,
            BuiltinRun {
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            },
        );
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
                session.assign_var(name.as_bytes(), value.as_bytes());
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

    if is_builtin_stage(session, &command) {
        let output = run_builtin_stage(session, &command);
        return output;
    }

    let source = shell_line_source(std::slice::from_ref(&command));
    let invocation = match external_invocation(session, &command) {
        Ok(invocation) => invocation,
        Err(error) => return expansion_error_output(error),
    };
    run_external_foreground(session, source, invocation)
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
        SessionBuiltin::Eval => {
            let source = args.join(" ");
            if source.trim().is_empty() {
                0
            } else {
                let output = execute_line(session, &source);
                stdout.extend_from_slice(&output.stdout);
                stderr.extend_from_slice(&output.stderr);
                output.status
            }
        }
        SessionBuiltin::Cd => builtin_cd(session, args, &mut stderr),
        SessionBuiltin::Source => builtin_source(session, args, &mut stdout, &mut stderr),
        SessionBuiltin::Export => {
            if args.is_empty() {
                for (name, value) in &session.env {
                    writeln!(
                        stdout,
                        "export {}=\"{}\"",
                        String::from_utf8_lossy(name),
                        String::from_utf8_lossy(value)
                    )
                    .ok();
                }
                0
            } else {
                let status = 0;
                for arg in args {
                    if let Some((name, value)) = arg.split_once('=') {
                        if valid_env_name(name) {
                            session.export_var(name.as_bytes(), value.as_bytes());
                        }
                    } else if valid_env_name(arg) {
                        session.export_existing(arg.as_bytes());
                    }
                }
                status
            }
        }
        SessionBuiltin::Set => set_builtin(session, args, &mut stdout),
        SessionBuiltin::Unset => {
            for name in args {
                session.unset_var(name.as_bytes());
            }
            0
        }
        SessionBuiltin::Alias => run_alias(session, args, &mut stdout, &mut stderr),
        SessionBuiltin::Which => {
            let path_env = session
                .env
                .get(b"PATH".as_slice())
                .map_or(&[][..], Vec::as_slice)
                .to_vec();
            builtin::locate_command(args, &session.aliases, &path_env, &mut stdout, &mut stderr)
        }
        SessionBuiltin::History => match args.first().map(String::as_str) {
            None => {
                for index in 0..session.history.len() {
                    writeln!(stdout, "{}", session.history.get(index)).ok();
                }
                0
            }
            Some("-h" | "--help") => {
                writeln!(stdout, "Usage: history [compact|rebuild|reset]").ok();
                writeln!(stdout).ok();
                writeln!(
                    stdout,
                    "Show history, compact its storage, rebuild its cache, or reset it."
                )
                .ok();
                0
            }
            Some("compact") => {
                session.history.compact();
                0
            }
            Some("rebuild") => {
                session.history.rebuild();
                0
            }
            Some("reset") => match session.history.reset() {
                Ok(()) => 0,
                Err(error) => {
                    writeln!(stderr, "xshi: history reset: {error}").ok();
                    1
                }
            },
            Some(other) => {
                writeln!(stderr, "xshi: history: unknown subcommand: {other}").ok();
                1
            }
        },
        SessionBuiltin::Clear => {
            stdout.extend_from_slice(b"\x1b[H\x1b[2J");
            0
        }
        SessionBuiltin::List => builtin::list_directory(args, &mut stdout, &mut stderr),
        SessionBuiltin::Z => {
            let home = session
                .var(b"HOME")
                .map(|home| String::from_utf8_lossy(home).into_owned())
                .unwrap_or_default();
            let mut announced = Vec::new();
            match super::z::jump(args, &session.history, &home, &mut stderr, &mut announced) {
                Ok(target) => {
                    stderr.extend_from_slice(&announced);
                    match session.set_cwd(target.clone()) {
                        Ok(()) => {
                            denv::after_cwd_change(session, &mut stderr);
                            0
                        }
                        Err(err) => {
                            writeln!(stderr, "xshi: z: {}: {err}", target.display()).ok();
                            1
                        }
                    }
                }
                Err(status) => status,
            }
        }
        SessionBuiltin::Denv => match args.first().map(String::as_str) {
            Some("allow") => denv::run_command(session, &DenvCommand::Allow, &mut stderr),
            Some("deny") => denv::run_command(session, &DenvCommand::Deny, &mut stderr),
            Some("reload") => denv::run_command(session, &DenvCommand::Reload, &mut stderr),
            _ => {
                stderr.extend_from_slice(b"usage: denv <allow|deny|reload>\n");
                1
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

fn execute_fg_builtin(session: &mut Session, _args: &[String]) -> CommandOutput {
    let Some(mut job) = session.job.take() else {
        return CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: text_bytes("xshi: fg: no suspended job\n"),
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

    let _ = writeln!(io::stderr(), "xshi: resuming: {}", job.command);
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
            let finished = status_command_output(status, Vec::new());
            let rest = std::mem::take(&mut job.continuation);
            if rest.is_empty() {
                return finished;
            }
            session.last_status = finished.status;
            session.dollar_status = finished.status;
            let mut rest = run_chains(session, rest, finished.status, finished.process_status);
            rest.stdout.splice(0..0, finished.stdout);
            rest.stderr.splice(0..0, finished.stderr);
            rest
        }
        Ok((ChildWaitOutcome::Stopped { signal: _ }, _)) => {
            // Stopped again: the job stays suspended in the slot, silently.
            job.terminal_attrs = terminal_attrs();
            drop(foreground);
            job.state = InteractiveJobState::Stopped;
            job.notified = true;
            session.job = Some(job);
            CommandOutput {
                status: 148,
                stdout: Vec::new(),
                stderr: Vec::new(),
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
            // Nothing waits for a background job, so the list ends here.
            job.continuation.clear();
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
    if session.capturing {
        return run_external_captured(session, source, invocation);
    }
    let mut spawn_error_message: Option<String> = None;
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
                    let name = String::from_utf8_lossy(&invocation.target).into_owned();
                    let pid = child.pid;
                    let pgid = child.pgid;
                    session.job = Some(InteractiveJob {
                        child,
                        pid,
                        pgid,
                        command: name.clone(),
                        state: InteractiveJobState::Stopped,
                        terminal_attrs: attrs,
                        last_status: None,
                        notified: true,
                        continuation: Vec::new(),
                    });
                    return CommandOutput {
                        status: 148,
                        stdout: Vec::new(),
                        stderr: text_bytes(format!("xshi: stopped: {name} (pgid={pgid})\n")),
                        process_status: Some(ProcessStatus::exited(148)),
                        history_source: Some(source),
                    };
                }
                Ok((ChildWaitOutcome::StillRunning, _)) => ProcessStatus::signaled(libc::SIGTERM),
                Err(error) => process_error_status(&invocation.target, error),
            }
        }
        Err(mut error) => {
            spawn_error_message = exec_failure_message(session, &invocation.target, &mut error);
            process_error_status(&invocation.target, error)
        }
    };
    status_from_process_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: spawn_error_message.map(text_bytes).unwrap_or_default(),
        process_status: Some(status),
        history_source: Some(source),
    })
}

/// Runs an external program whose standard output belongs to a command
/// substitution: stdout is captured, stderr and stdin stay with the terminal.
fn run_external_captured(
    session: &Session,
    source: String,
    invocation: ProcessInvocation,
) -> CommandOutput {
    let mut policy = InteractiveNoCancellation;
    match run_capture_with_policy(&invocation, &mut policy) {
        Ok(output) => {
            let status = output
                .end
                .status
                .unwrap_or_else(|| ProcessStatus::exited(0));
            status_from_process_output(CommandOutput {
                status: 0,
                stdout: output.stdout,
                stderr: Vec::new(),
                process_status: Some(status),
                history_source: Some(source),
            })
        }
        Err(mut error) => {
            let message = exec_failure_message(session, &invocation.target, &mut error);
            status_from_process_output(CommandOutput {
                status: 0,
                stdout: Vec::new(),
                stderr: message.map(text_bytes).unwrap_or_default(),
                process_status: Some(process_error_status(&invocation.target, error)),
                history_source: Some(source),
            })
        }
    }
}

/// The diagnostic printed when a program cannot be started: `name: not found`
/// (127), `name: permission denied` (126), or, when the file exists and is
/// executable but its `#!` interpreter cannot be started,
/// `name: interp: bad interpreter: <reason>`.
fn exec_failure_message(session: &Session, target: &[u8], error: &mut RunError) -> Option<String> {
    let name = String::from_utf8_lossy(target);
    let (code_text, fallback) = match error.kind.as_str() {
        "not-found" => ("No such file or directory", format!("{name}: not found\n")),
        "permission-denied" | "not-executable" | "exec-format" => {
            ("Permission denied", format!("{name}: permission denied\n"))
        }
        _ => return Some(format!("xshi: {name}: {}\n", error.message)),
    };
    if let Some(path) = resolve_exec_path(session, &name)
        && is_executable_file(&path)
        && let Some(interpreter) = read_shebang_interpreter(&path)
    {
        // A broken interpreter is "found but not executable": status 126.
        error.kind = "bad-interpreter".to_string();
        return Some(format!(
            "{name}: {interpreter}: bad interpreter: {code_text}\n"
        ));
    }
    Some(fallback)
}

fn resolve_exec_path(session: &Session, name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return Some(if path.is_absolute() {
            path
        } else {
            session.cwd.join(path)
        });
    }
    let path_env = session.env.get(b"PATH".as_slice())?;
    std::env::split_paths(&OsString::from_vec(path_env.clone()))
        .map(|dir| {
            if dir.as_os_str().is_empty() {
                session.cwd.clone()
            } else {
                dir
            }
        })
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable_file(candidate))
}

/// The interpreter named by a `#!` first line, if the file has one.
fn read_shebang_interpreter(path: &Path) -> Option<String> {
    use std::io::Read as _;
    let mut head = [0_u8; 256];
    let read = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    let head = &head[..read];
    let line = head.strip_prefix(b"#!")?;
    let line = line.split(|byte| *byte == b'\n').next()?;
    let text = String::from_utf8_lossy(line);
    let interpreter = text.split_whitespace().next()?;
    Some(interpreter.to_string())
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

/// The shell status of a finished command or pipeline: the last program's.
fn shell_status(status: &ProcessStatus) -> i32 {
    if let Some(segment) = status.segments.last() {
        return match segment.kind {
            ProcessSegmentStatusKind::Exit => segment.code.unwrap_or(1),
            ProcessSegmentStatusKind::Signal => 128 + segment.code.unwrap_or(0),
            ProcessSegmentStatusKind::Exec => match segment.error_kind.as_deref() {
                Some("not-found") => 127,
                _ => 126,
            },
        };
    }
    match status.kind {
        ProcessStatusKind::Exit => status.code.unwrap_or(if status.success { 0 } else { 1 }),
        ProcessStatusKind::Signal => 128 + status.code.unwrap_or(0),
        ProcessStatusKind::Exec => 126,
    }
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
            .map(|redirection| external_redirections(session, redirection))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect(),
        timeout: None,
        cpu_max: None,
        accepted_exit_codes: None,
    })
}

fn external_redirections(
    session: &Session,
    redirection: &super::shell::Redirection,
) -> Result<Vec<ProcessRedirection>, ExpansionError> {
    let path_target = |word: &ShellWord| -> Result<PathBuf, ExpansionError> {
        let target = expand_word_to_string(session, word)?;
        let path = PathBuf::from(target);
        Ok(if path.is_absolute() {
            path
        } else {
            session.cwd.join(path)
        })
    };
    let file = |stream, mode, redirection: &super::shell::Redirection| {
        path_target(&redirection.target).map(|path| ProcessRedirection::File { stream, mode, path })
    };
    Ok(match redirection.kind {
        ShellRedirectionKind::Stdin => {
            vec![file(
                RedirectionStream::Stdin,
                FileRedirectionMode::Read,
                redirection,
            )?]
        }
        ShellRedirectionKind::StdoutWrite => {
            vec![file(
                RedirectionStream::Stdout,
                FileRedirectionMode::Write,
                redirection,
            )?]
        }
        ShellRedirectionKind::StdoutAppend => {
            vec![file(
                RedirectionStream::Stdout,
                FileRedirectionMode::Append,
                redirection,
            )?]
        }
        ShellRedirectionKind::StderrWrite => {
            vec![file(
                RedirectionStream::Stderr,
                FileRedirectionMode::Write,
                redirection,
            )?]
        }
        ShellRedirectionKind::StderrAppend => {
            vec![file(
                RedirectionStream::Stderr,
                FileRedirectionMode::Append,
                redirection,
            )?]
        }
        ShellRedirectionKind::StdoutToStderr => vec![ProcessRedirection::ChildDup {
            stream: RedirectionStream::Stdout,
            fd: 2,
        }],
        ShellRedirectionKind::StderrToStdout => vec![ProcessRedirection::ChildDup {
            stream: RedirectionStream::Stderr,
            fd: 1,
        }],
        ShellRedirectionKind::BothWrite => vec![
            file(
                RedirectionStream::Stdout,
                FileRedirectionMode::Write,
                redirection,
            )?,
            ProcessRedirection::ChildDup {
                stream: RedirectionStream::Stderr,
                fd: 1,
            },
        ],
        ShellRedirectionKind::BothAppend => vec![
            file(
                RedirectionStream::Stdout,
                FileRedirectionMode::Append,
                redirection,
            )?,
            ProcessRedirection::ChildDup {
                stream: RedirectionStream::Stderr,
                fd: 1,
            },
        ],
    })
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
    if first == "export" {
        return looks_like_xsh_export(trimmed);
    }
    matches!(
        first,
        "let"
            | "var"
            | "proc"
            | "pure"
            | "use"
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

/// `export` is a shell builtin (`export NAME=value`) unless it begins an XSH
/// export: a declaration keyword, or the short form `export name: Type`.
fn looks_like_xsh_export(trimmed: &str) -> bool {
    let rest = trimmed
        .strip_prefix("export")
        .unwrap_or_default()
        .trim_start();
    let word_end = rest
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len());
    let (word, after) = rest.split_at(word_end);
    if word.is_empty() {
        return false;
    }
    matches!(word, "let" | "var" | "proc" | "pure" | "stream" | "type")
        && after.starts_with(|ch: char| ch.is_whitespace())
        || after.trim_start().starts_with(':')
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

pub(super) fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// One word after expansion and field splitting.
struct Field {
    /// The characters the word produced.
    text: String,
    /// The same characters as a glob pattern: quoted metacharacters escaped.
    pattern: String,
    /// Whether an unquoted metacharacter makes this a pattern.
    glob: bool,
}

fn is_glob_meta(ch: char) -> bool {
    matches!(ch, '*' | '?' | '[' | '\\')
}

/// Accumulates the fields of one shell word. Literal text and quoted
/// expansions extend the current field; unquoted expansions split on white
/// space, so an empty one can vanish entirely.
#[derive(Default)]
struct FieldBuilder {
    fields: Vec<Field>,
    text: String,
    pattern: String,
    started: bool,
}

impl FieldBuilder {
    fn literal(&mut self, text: &str, quoted: bool) {
        if text.is_empty() {
            self.started |= quoted;
            return;
        }
        self.started = true;
        self.text.push_str(text);
        for ch in text.chars() {
            if quoted && is_glob_meta(ch) {
                self.pattern.push('\\');
            }
            self.pattern.push(ch);
        }
    }

    fn expansion(&mut self, value: &str, quoted: bool) {
        if quoted {
            self.literal(value, true);
            self.started = true;
            return;
        }
        let mut buffer = [0_u8; 4];
        for ch in value.chars() {
            if matches!(ch, ' ' | '\t' | '\n') {
                self.end_field();
            } else {
                self.literal(ch.encode_utf8(&mut buffer), false);
            }
        }
    }

    fn end_field(&mut self) {
        if !self.started {
            return;
        }
        let pattern = std::mem::take(&mut self.pattern);
        let glob = pattern_has_meta(&pattern);
        self.fields.push(Field {
            text: std::mem::take(&mut self.text),
            pattern,
            glob,
        });
        self.started = false;
    }

    fn finish(mut self) -> Vec<Field> {
        self.end_field();
        self.fields
    }
}

/// Whether a pattern holds an unescaped `*`, `?`, or closed `[`.
fn pattern_has_meta(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'*' | b'?' => return true,
            b'[' if bytes[index + 1..].contains(&b']') => return true,
            _ => {}
        }
        index += 1;
    }
    false
}

fn expand_fields(
    session: &Session,
    word: &ShellWord,
    split: bool,
) -> Result<Vec<Field>, ExpansionError> {
    let mut builder = FieldBuilder::default();
    let mut first = true;
    for part in &word.parts {
        match part {
            ShellWordPart::Text {
                text,
                expand,
                glob: unquoted,
            } => {
                if *expand {
                    let text = if first && *unquoted {
                        expand_tilde(session, text)
                    } else {
                        text.clone()
                    };
                    expand_text(&mut builder, session, &text, !*unquoted || !split);
                } else {
                    builder.literal(text, true);
                }
            }
            ShellWordPart::CommandSubstitution {
                source,
                glob: unquoted,
            } => {
                let value = expand_command_substitution(session, source)?;
                builder.expansion(&value, !*unquoted || !split);
            }
            ShellWordPart::ArithmeticExpansion {
                source,
                glob: unquoted,
            } => {
                let value = expand_arithmetic(session, source)?;
                builder.expansion(&value, !*unquoted || !split);
            }
        }
        first = false;
    }
    Ok(builder.finish())
}

/// Expands `$NAME`, `${NAME}`, `$?`, and `$$` in `text`, feeding literal runs
/// and variable values to `builder`.
fn expand_text(builder: &mut FieldBuilder, session: &Session, text: &str, quoted: bool) {
    let mut literal = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            literal.push(ch);
            continue;
        }
        let value = match chars.peek().copied() {
            Some('?') => {
                chars.next();
                Some(session.dollar_status.to_string())
            }
            Some('$') => {
                chars.next();
                Some(std::process::id().to_string())
            }
            Some('{') => {
                chars.next();
                let mut name = String::new();
                while chars.peek().is_some_and(|ch| *ch != '}') {
                    name.push(chars.next().unwrap());
                }
                if chars.peek() == Some(&'}') {
                    chars.next();
                }
                Some(
                    session
                        .var(name.as_bytes())
                        .map(|value| String::from_utf8_lossy(value).into_owned())
                        .unwrap_or_default(),
                )
            }
            _ => {
                let mut name = String::new();
                while chars
                    .peek()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                {
                    name.push(chars.next().unwrap());
                }
                if name.is_empty() {
                    literal.push('$');
                    None
                } else {
                    Some(
                        session
                            .var(name.as_bytes())
                            .map(|value| String::from_utf8_lossy(value).into_owned())
                            .unwrap_or_default(),
                    )
                }
            }
        };
        if let Some(value) = value {
            builder.literal(&std::mem::take(&mut literal), quoted);
            builder.expansion(&value, quoted);
        }
    }
    builder.literal(&literal, quoted);
}

pub(super) fn expand_word(
    session: &Session,
    word: &ShellWord,
) -> Result<Vec<String>, ExpansionError> {
    let mut output = Vec::new();
    for field in expand_fields(session, word, true)? {
        if field.glob && has_glob_meta(&field.pattern) {
            output.extend(expand_glob(session, &field.pattern)?);
        } else {
            output.push(field.text);
        }
    }
    Ok(output)
}

/// Expands a word that stays one string: no splitting and no globbing, as for
/// assignment values and redirection targets.
pub(super) fn expand_word_to_string(
    session: &Session,
    word: &ShellWord,
) -> Result<String, ExpansionError> {
    let fields = expand_fields(session, word, false)?;
    Ok(fields
        .into_iter()
        .map(|field| field.text)
        .collect::<Vec<_>>()
        .join(" "))
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
    let mut nested = session.fork_for_substitution();
    let output = execute_shell_line(&mut nested, line);
    // Diagnostics reach the terminal as they would from any command; the
    // substitution's status does not stop the command that uses it.
    let _ = io::stderr().write_all(&output.stderr);
    let mut text = String::from_utf8(output.stdout)
        .map_err(|_| ExpansionError::usage("command substitution produced non-UTF-8 output"))?;
    // Trailing newlines are not part of a substitution's value.
    text.truncate(text.trim_end_matches('\n').len());
    Ok(text)
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
        let Some(value) = self.session.var(name.as_bytes()) else {
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

/// Commands the shell runs itself, after alias expansion, so their behavior
/// does not depend on which `echo` the platform ships.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InternalCommand {
    Echo,
    Pwd,
    True,
    False,
}

fn internal_command_name(name: &str) -> Option<InternalCommand> {
    Some(match name {
        "echo" => InternalCommand::Echo,
        "pwd" => InternalCommand::Pwd,
        "true" => InternalCommand::True,
        "false" => InternalCommand::False,
        _ => return None,
    })
}

/// Runs an internal command. `args` excludes the command name.
fn run_internal(session: &Session, kind: InternalCommand, args: &[String]) -> BuiltinRun {
    match kind {
        InternalCommand::True => BuiltinRun::status(0),
        InternalCommand::False => BuiltinRun::status(1),
        InternalCommand::Pwd => BuiltinRun {
            status: 0,
            stdout: format!("{}\n", session.cwd.display()).into_bytes(),
            stderr: Vec::new(),
        },
        InternalCommand::Echo => {
            let mut index = 0;
            let mut newline = true;
            let mut escape = false;
            while let Some(flag) = args.get(index) {
                match flag.as_str() {
                    "-n" => newline = false,
                    "-e" => escape = true,
                    "-E" => escape = false,
                    _ => break,
                }
                index += 1;
            }
            let mut text = args[index..].join(" ");
            if escape {
                text = unescape_echo(&text);
            }
            if newline {
                text.push('\n');
            }
            BuiltinRun {
                status: 0,
                stdout: text.into_bytes(),
                stderr: Vec::new(),
            }
        }
    }
}

fn unescape_echo(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('\\') => result.push('\\'),
            Some('a') => result.push('\x07'),
            Some('b') => result.push('\x08'),
            Some('f') => result.push('\x0c'),
            Some('r') => result.push('\r'),
            Some('v') => result.push('\x0b'),
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    result
}

/// What a builtin produced, before redirections and pipes decide where it goes.
struct BuiltinRun {
    status: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl BuiltinRun {
    fn status(status: i32) -> Self {
        Self {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn into_output(self) -> CommandOutput {
        CommandOutput {
            status: self.status,
            stdout: self.stdout,
            stderr: self.stderr,
            process_status: Some(ProcessStatus::exited(self.status)),
            history_source: None,
        }
    }
}

/// Where a builtin's stream ends up once redirections are applied.
#[derive(Clone, Copy)]
enum Sink {
    Stdout,
    Stderr,
    File(usize),
}

/// Applies a command's redirections to the output of a builtin. Returns the
/// bytes that still belong on stdout and stderr; a redirection that cannot be
/// opened fails the command.
fn apply_builtin_redirections(
    session: &Session,
    redirections: &[super::shell::Redirection],
    run: BuiltinRun,
) -> CommandOutput {
    if redirections.is_empty() {
        return run.into_output();
    }
    let mut files: Vec<fs::File> = Vec::new();
    let mut stdout_sink = Sink::Stdout;
    let mut stderr_sink = Sink::Stderr;
    for redirection in redirections {
        let target = match expand_word_to_string(session, &redirection.target) {
            Ok(target) => target,
            Err(error) => return expansion_error_output(error),
        };
        let path = if Path::new(&target).is_absolute() {
            PathBuf::from(&target)
        } else {
            session.cwd.join(&target)
        };
        let open = |append: bool, files: &mut Vec<fs::File>| -> Result<Sink, CommandOutput> {
            let mut options = fs::OpenOptions::new();
            options.create(true).write(true);
            if append {
                options.append(true);
            } else {
                options.truncate(true);
            }
            match options.open(&path) {
                Ok(file) => {
                    files.push(file);
                    Ok(Sink::File(files.len() - 1))
                }
                Err(error) => Err(BuiltinRun {
                    status: 2,
                    stdout: Vec::new(),
                    stderr: text_bytes(format!("{target}: {error}\nxshi: {error}\n")),
                }
                .into_output()),
            }
        };
        match redirection.kind {
            ShellRedirectionKind::Stdin => {
                if let Err(error) = fs::File::open(&path) {
                    return BuiltinRun {
                        status: 1,
                        stdout: Vec::new(),
                        stderr: text_bytes(format!("xshi: {target}: {error}\n")),
                    }
                    .into_output();
                }
            }
            ShellRedirectionKind::StdoutWrite | ShellRedirectionKind::StdoutAppend => {
                match open(
                    redirection.kind == ShellRedirectionKind::StdoutAppend,
                    &mut files,
                ) {
                    Ok(sink) => stdout_sink = sink,
                    Err(output) => return output,
                }
            }
            ShellRedirectionKind::StderrWrite | ShellRedirectionKind::StderrAppend => {
                match open(
                    redirection.kind == ShellRedirectionKind::StderrAppend,
                    &mut files,
                ) {
                    Ok(sink) => stderr_sink = sink,
                    Err(output) => return output,
                }
            }
            ShellRedirectionKind::BothWrite | ShellRedirectionKind::BothAppend => {
                match open(
                    redirection.kind == ShellRedirectionKind::BothAppend,
                    &mut files,
                ) {
                    Ok(sink) => {
                        stdout_sink = sink;
                        stderr_sink = sink;
                    }
                    Err(output) => return output,
                }
            }
            ShellRedirectionKind::StdoutToStderr => stdout_sink = stderr_sink,
            ShellRedirectionKind::StderrToStdout => stderr_sink = stdout_sink,
        }
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    for (sink, bytes) in [(stdout_sink, run.stdout), (stderr_sink, run.stderr)] {
        match sink {
            Sink::Stdout => stdout.extend_from_slice(&bytes),
            Sink::Stderr => stderr.extend_from_slice(&bytes),
            Sink::File(index) => {
                let _ = files[index].write_all(&bytes);
            }
        }
    }
    CommandOutput {
        status: run.status,
        stdout,
        stderr,
        process_status: Some(ProcessStatus::exited(run.status)),
        history_source: None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionBuiltin {
    Eval,
    Source,
    Export,
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

fn first_word_is_session_builtin(source: &str) -> bool {
    source
        .split_whitespace()
        .next()
        .is_some_and(|word| session_builtin_name(word).is_some())
}

fn session_builtin(words: &[ShellWord]) -> Option<SessionBuiltin> {
    words
        .first()
        .and_then(|name| session_builtin_name(&name.text()))
}

fn session_builtin_name(name: &str) -> Option<SessionBuiltin> {
    Some(match name {
        "eval" => SessionBuiltin::Eval,
        "source" | "." => SessionBuiltin::Source,
        "export" => SessionBuiltin::Export,
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
        "w" | "which" | "type" => SessionBuiltin::Which,
        "history" => SessionBuiltin::History,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ChainOp, InteractiveJobState, ProcessGroup, ProcessStatus, Session, ShellParser,
        ShellRedirectionKind, ShellToken, ShellWord, ShellWordPart, execute_line, is_xsh_source,
        lex_shell, reap_interactive_job, set_env_bytes, shell_status, validate_assignment_prefix,
    };
    use crate::xshi::interactive::denv::DenvState;
    use crate::xshi::interactive::history::History;
    use crate::xshi::interactive::shell::SimpleCommand;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    struct JobCleanup(libc::pid_t);

    impl Drop for JobCleanup {
        fn drop(&mut self) {
            if self.0 > 0 {
                ProcessGroup::from_pgid(self.0).signal(libc::SIGKILL);
            }
        }
    }

    fn wait_for_job_state(session: &mut Session, expected: InteractiveJobState) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stderr = Vec::new();
        while session
            .job
            .as_ref()
            .is_some_and(|job| job.state != expected)
        {
            reap_interactive_job(session, &mut stderr);
            assert!(Instant::now() < deadline, "job did not reach {expected:?}");
            std::thread::yield_now();
        }
        assert!(session.job.is_some(), "job completed before {expected:?}");
        stderr
    }

    fn wait_for_job_completion(session: &mut Session) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stderr = Vec::new();
        while session.job.is_some() {
            reap_interactive_job(session, &mut stderr);
            assert!(Instant::now() < deadline, "job did not complete");
            std::thread::yield_now();
        }
        stderr
    }

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
        let mut session = Session::for_test();
        set_env_bytes(&mut session.env, b"N", b"5");
        let output = execute_line(&mut session, "RESULT=$((1 + 2 * N))");
        assert_eq!(output.status, 0);
        assert_eq!(session.var(b"RESULT"), Some(&b"11"[..]));
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
        assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    }

    #[test]
    fn repeated_shell_runs_accept_dynamic_environment_names() {
        let mut session = Session::for_test();
        let true_path = if cfg!(target_os = "macos") {
            "/usr/bin/true"
        } else {
            "/bin/true"
        };

        for index in 0..8 {
            let output = execute_line(
                &mut session,
                &format!("DYNAMIC_SESSION_{index}=ok {true_path}"),
            );
            assert_eq!(output.status, 0);
        }
    }

    #[test]
    fn submitted_xsh_sources_prepare_embedded_calls_after_failures_in_one_session() {
        let mut session = Session::for_test();
        for source in [
            "print shlex.quote(\"a b\")",
            "print bytes.human(4096)",
            "print tui.left_pad(\"x\", 4)",
            "print hash.parse_check_line(\"aa  b\")?.hex",
        ] {
            let output = execute_line(&mut session, source);
            assert_eq!(output.status, 0, "{source}: {:?}", output.stderr);
            assert!(!output.stdout.is_empty(), "{source}");
        }

        assert_eq!(
            execute_line(&mut session, "let submission_only = 7").status,
            0
        );

        for (source, diagnostic) in [
            ("print (", true),
            ("let value = submission_only", true),
            ("abort(9)", false),
        ] {
            let output = execute_line(&mut session, source);
            assert_ne!(output.status, 0, "{source}");
            if diagnostic {
                assert!(!output.stderr.is_empty(), "{source}");
            }

            let recovered = execute_line(&mut session, "print shlex.quote(\"after failure\")");
            assert_eq!(recovered.status, 0, "{source}: {:?}", recovered.stderr);
            assert_eq!(recovered.stdout, b"'after failure'\n");
        }

        assert_eq!(execute_line(&mut session, "true").status, 0);
        let after_shell = execute_line(&mut session, "print bytes.human(4096)");
        assert_eq!(after_shell.status, 0, "{:?}", after_shell.stderr);
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
    fn comments_and_combined_redirections_lex() {
        // A comment runs to the end of the line and is not part of a word.
        let tokens = lex_shell("echo hi # later").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(lex_shell("echo a#b").unwrap().len(), 2);
        assert_eq!(lex_shell("# only a comment").unwrap().len(), 0);
        // `&>` and `&>>` send both output streams to a file.
        assert!(matches!(
            lex_shell("echo hi &> out").unwrap().as_slice(),
            [_, _, ShellToken::Redir(ShellRedirectionKind::BothWrite), _]
        ));
        assert!(matches!(
            lex_shell("echo hi &>> out").unwrap().as_slice(),
            [_, _, ShellToken::Redir(ShellRedirectionKind::BothAppend), _]
        ));
    }

    #[test]
    fn fd_duplication_carries_its_own_target() {
        let line = ShellParser::new("cat missing 2>&1 | wc")
            .parse_line()
            .unwrap();
        let command = &line.chains[0].pipeline.commands[0];
        assert_eq!(command.words.len(), 2);
        assert_eq!(command.redirections.len(), 1);
        assert_eq!(
            command.redirections[0].kind,
            ShellRedirectionKind::StderrToStdout
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
        let mut session = Session::for_test();
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
        let mut session = Session::for_test();
        let w = execute_line(&mut session, "w fg");
        assert_eq!(w.status, 0);
        assert!(String::from_utf8(w.stdout).unwrap().contains("builtin"));

        session.aliases.set(
            "fg".to_string(),
            vec!["echo".to_string(), "alias".to_string()],
        );
        session.aliases.set(
            "bg".to_string(),
            vec!["echo".to_string(), "alias".to_string()],
        );

        let fg = execute_line(&mut session, "fg");
        assert_eq!(fg.status, 1);
        assert!(
            String::from_utf8(fg.stderr)
                .unwrap()
                .contains("xshi: fg: no suspended job")
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
    fn shell_guidance_shims_do_not_shadow_xsh_list_expressions() {
        let mut session = Session::for_test();

        // The XSH checker, not a `[` shell shim, judges the line: a bare list
        // is a discarded value there.
        let list = execute_line(&mut session, "[1, 2]");
        assert_eq!(list.status, 2);
        assert!(
            String::from_utf8_lossy(&list.stderr).contains("check.ignored-result"),
            "{}",
            String::from_utf8_lossy(&list.stderr)
        );
    }

    #[test]
    fn single_background_job_rejects_second_slot_and_reaps_without_changing_prompt_status() {
        let mut session = Session::for_test();
        let started = execute_line(&mut session, "/bin/sleep 30 &");
        assert_eq!(started.status, 0);
        let pgid = session.job.as_ref().expect("background job slot").pgid;
        let mut cleanup = JobCleanup(pgid);
        assert_eq!(
            session.job.as_ref().unwrap().state,
            InteractiveJobState::RunningBackground
        );

        let second = execute_line(&mut session, "/bin/sleep 30 &");
        assert_eq!(second.status, 1);
        assert!(String::from_utf8_lossy(&second.stderr).contains("background job already exists"));
        assert_eq!(session.job.as_ref().unwrap().pgid, pgid);

        let already_running = execute_line(&mut session, "bg");
        assert_eq!(already_running.status, 1);
        assert!(String::from_utf8_lossy(&already_running.stderr).contains("job already running"));

        session.last_status = 42;
        ProcessGroup::from_pgid(pgid).signal(libc::SIGTERM);
        let notice = wait_for_job_completion(&mut session);
        cleanup.0 = 0;
        assert!(String::from_utf8_lossy(&notice).contains("xshi: completed:"));
        assert_eq!(session.last_status, 42);
        assert_eq!(execute_line(&mut session, "bg").status, 1);
        assert_eq!(execute_line(&mut session, "fg").status, 1);
    }

    #[test]
    fn stopped_background_job_resumes_then_foregrounds_to_its_exit_status() {
        let root = tempfile::tempdir().expect("temporary directory");
        let command = root.path().join("stop-then-exit");
        fs::write(&command, b"#!/bin/sh\nkill -STOP $$\nexit 7\n").expect("stopping command");
        fs::set_permissions(&command, fs::Permissions::from_mode(0o755))
            .expect("make stopping command executable");
        let source = format!("{} &", command.display());
        let mut session = Session::for_test();

        assert_eq!(execute_line(&mut session, &source).status, 0);
        let mut cleanup = JobCleanup(session.job.as_ref().unwrap().pgid);
        let stopped = wait_for_job_state(&mut session, InteractiveJobState::Stopped);
        assert!(String::from_utf8_lossy(&stopped).contains("xshi: stopped:"));
        let resumed = execute_line(&mut session, "bg");
        assert_eq!(resumed.status, 0);
        assert_eq!(
            session.job.as_ref().unwrap().state,
            InteractiveJobState::RunningBackground
        );
        assert!(String::from_utf8_lossy(&resumed.stderr).contains("xshi: resumed:"));
        wait_for_job_completion(&mut session);
        cleanup.0 = 0;

        assert_eq!(execute_line(&mut session, &source).status, 0);
        let mut cleanup = JobCleanup(session.job.as_ref().unwrap().pgid);
        wait_for_job_state(&mut session, InteractiveJobState::Stopped);
        let foreground = execute_line(&mut session, "fg");
        cleanup.0 = 0;
        assert_eq!(foreground.status, 7);
        assert!(session.job.is_none());
    }

    #[test]
    fn maps_exec_statuses_to_shell_codes() {
        let mut status = ProcessStatus::from_segments(vec![xsh::process::ProcessSegmentStatus {
            index: 0,
            target: b"missing".to_vec(),
            pid: None,
            success: false,
            kind: xsh::process::ProcessSegmentStatusKind::Exec,
            code: None,
            error_kind: Some("not-found".to_string()),
            error_message: Some("executable not found".to_string()),
        }]);
        assert_eq!(shell_status(&status), 127);
        status.segments[0].error_kind = Some("permission-denied".to_string());
        assert_eq!(shell_status(&status), 126);
    }
}
