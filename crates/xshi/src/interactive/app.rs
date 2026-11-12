#![allow(clippy::single_call_fn)]

use super::alias::{self, AliasMap};
use super::builtin;
use super::config::{load_config, load_config_path, load_profile};
use super::denv::{self, DenvCommand};
use super::history::History;
use super::repl::{self, ReadResult, Ui};
use super::session::{
    InteractiveJob, InteractiveJobProcess, Session, set_env_bytes, stdio_is_tty,
};
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
    ManagedStdio, PipelineJob, ProcessGroup, ProcessGroupConfig,
    ProcessInvocation, ProcessRedirection, ProcessSegmentStatus, ProcessSegmentStatusKind,
    ProcessStatus, ProcessStatusKind, RedirectionStream, SpawnManagedOptions, WaitMode,
    initialize_interactive_process_group, poll_managed, run_capture_with_policy,
    run_pipeline_inherit_with_policy, spawn_managed, wait_managed,
};

fn text_bytes(text: impl Into<String>) -> Vec<u8> {
    text.into().into_bytes()
}

fn new_root_process_group_id(pid: u32) -> libc::pid_t {
    // ProcessGroupConfig::NewRoot makes the child its process-group leader.
    pid as libc::pid_t
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
            LineOutcome::Exit(code) => {
                super::term::write_osc133_d(code);
                return code;
            }
        }
    }
}

enum LineOutcome {
    Continue,
    Exit(i32),
}

fn is_exit_command(line: &str) -> bool {
    !is_xsh_source(line) && is_simple_word_line(line) && line.split_whitespace().next() == Some("exit")
}

fn is_simple_word_line(line: &str) -> bool {
    lex_shell(line)
        .map(|tokens| {
            tokens
                .iter()
                .all(|token| matches!(token, ShellToken::Word(_)))
        })
        .unwrap_or(false)
}

/// Builtins that read or change shell state or need the shell process, so the
/// reference shell only runs them as the whole input line.
fn is_whole_line_builtin(name: &str) -> bool {
    matches!(
        name,
        "alias"
            | "copy-scrollback"
            | "denv"
            | "exit"
            | "fg"
            | "xshi-dump"
            | "z"
            | "w"
            | "which"
            | "type"
    )
}

fn static_word_text(word: &ShellWord) -> Option<String> {
    let mut text = String::new();
    for part in &word.parts {
        match part {
            ShellWordPart::Text {
                text: part_text,
                expand,
                ..
            } if !expand || !part_text.contains('$') => text.push_str(part_text),
            _ => return None,
        }
    }
    Some(text)
}

const BACKGROUND_JOBS_REFUSED: &str = "background jobs (`&`) are not supported";
const PARAMETER_OPERATORS_REFUSED: &str =
    "`${...}` parameter operators are not supported; use `$NAME`";
const MAX_REFUSAL_DEPTH: usize = 16;
const UNSUPPORTED_BUILTINS: &[&str] = &[
    "readonly", "shift", "eval", ".", "source", "read", "local", "exec", "wait", "trap", "umask",
    "getopts", "return", "break", "continue",
];

fn unquoted_word_text(word: &ShellWord) -> Option<&str> {
    match word.parts.as_slice() {
        [ShellWordPart::Text {
            text,
            expand: true,
            glob: true,
        }] => Some(text),
        _ => None,
    }
}

fn compound_kind(name: &str) -> Option<&'static str> {
    Some(match name {
        "if" => "if",
        "while" => "while",
        "until" => "until",
        "for" => "for",
        "case" => "case",
        "{" => "brace group",
        _ => return None,
    })
}

fn valid_function_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_function_definition(words: &[ShellWord]) -> bool {
    let Some(first) = words.first().and_then(unquoted_word_text) else {
        return false;
    };
    if let Some(name) = first.strip_suffix("()") {
        return valid_function_name(name);
    }
    valid_function_name(first) && words.get(1).and_then(unquoted_word_text) == Some("()")
}

fn matching_brace(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 1_usize;
    let mut index = 0_usize;
    while index < bytes.len() {
        match bytes[index] {
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
                index += 1;
            }
            b'$' if bytes.get(index + 1) == Some(&b'{') => {
                depth += 1;
                index += 2;
            }
            _ => index += 1,
        }
    }
    None
}

fn refused_parameter_operator(text: &str) -> bool {
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let body_start = &rest[start + 2..];
        let Some(end) = matching_brace(body_start) else {
            return false;
        };
        let body = &body_start[..end];
        if body.starts_with('#') || body.starts_with(|ch: char| ch.is_ascii_digit()) {
            return true;
        }
        let name_end = body
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(body.len());
        if let Some(operator) = body[name_end..].chars().next() {
            match operator {
                '-' | '=' | '?' | '+' | '%' | '#' => return true,
                ':'
                    if body[name_end..]
                        .chars()
                        .nth(1)
                        .is_some_and(|next| matches!(next, '-' | '=' | '?' | '+')) =>
                {
                    return true;
                }
                _ => {}
            }
        }
        rest = &body_start[end + 1..];
    }
    false
}

fn expandable_word_text(word: &ShellWord) -> String {
    let mut text = String::new();
    for part in &word.parts {
        match part {
            ShellWordPart::Text {
                text: part_text,
                expand: true,
                ..
            } => text.push_str(part_text),
            ShellWordPart::Text { .. }
            | ShellWordPart::CommandSubstitution { .. }
            | ShellWordPart::ArithmeticExpansion { .. } => text.push('X'),
        }
    }
    text
}

fn word_refusal(word: &ShellWord, depth: usize) -> Option<String> {
    if refused_parameter_operator(&expandable_word_text(word)) {
        return Some(PARAMETER_OPERATORS_REFUSED.to_string());
    }
    for part in &word.parts {
        match part {
            ShellWordPart::CommandSubstitution { source, .. } => {
                if depth < MAX_REFUSAL_DEPTH
                    && let Some(rejection) = source_rejection(source, false, depth + 1)
                {
                    return Some(rejection);
                }
            }
            ShellWordPart::ArithmeticExpansion { source, .. }
                if refused_parameter_operator(source) =>
            {
                return Some(PARAMETER_OPERATORS_REFUSED.to_string());
            }
            _ => {}
        }
    }
    None
}

fn nested_command_refusal(words: &[ShellWord]) -> Option<String> {
    let mut arguments = words;
    loop {
        let static_arguments: Vec<String> =
            arguments.iter().map_while(static_word_text).collect();
        let mut index = 0_usize;
        while static_arguments
            .get(index)
            .is_some_and(|argument| matches!(argument.as_str(), "-p" | "--"))
        {
            index += 1;
        }
        let name = static_arguments.get(index)?;
        if matches!(name.as_str(), "-v" | "-V") {
            return None;
        }
        if UNSUPPORTED_BUILTINS.contains(&name.as_str()) {
            return Some(format!("`{name}` is not supported"));
        }
        if is_whole_line_builtin(name) {
            return Some(format!(
                "{name} changes shell state or needs the ish process, so it must be the only command on its line"
            ));
        }
        if name != "command" {
            return None;
        }
        arguments = arguments.get(index + 1..)?;
    }
}

fn command_rejection(command: &SimpleCommand, whole_line: bool, depth: usize) -> Option<String> {
    let leading = command
        .words
        .iter()
        .take_while(|word| parse_env_assignment(&word.text()).is_some())
        .count();
    if let Some(kind) = command
        .words
        .get(leading)
        .and_then(unquoted_word_text)
        .and_then(compound_kind)
    {
        return Some(format!("{kind} commands are not supported"));
    }
    if is_function_definition(&command.words) {
        return Some("function definitions are not supported".to_string());
    }
    if let Some(name) = command.words.get(leading).and_then(static_word_text) {
        if UNSUPPORTED_BUILTINS.contains(&name.as_str()) {
            return Some(format!("`{name}` is not supported"));
        }
        if name == "set" {
            let static_arguments: Vec<String> = command.words[leading + 1..]
                .iter()
                .map_while(static_word_text)
                .collect();
            if let Some(argument) = static_arguments.first()
                && !valid_env_name(argument)
            {
                return Some("set: expected a variable name".to_string());
            }
        }
        let command_is_whole_line = whole_line && command.redirections.is_empty() && leading == 0;
        if !command_is_whole_line && is_whole_line_builtin(&name) {
            return Some(format!(
                "{name} changes shell state or needs the ish process, so it must be the only command on its line"
            ));
        }
        if name == "command"
            && let Some(rejection) = nested_command_refusal(&command.words[leading + 1..])
        {
            return Some(rejection);
        }
    }
    for word in &command.words {
        if let Some(rejection) = word_refusal(word, depth) {
            return Some(rejection);
        }
    }
    for redirection in &command.redirections {
        if let Some(rejection) = word_refusal(&redirection.target, depth) {
            return Some(rejection);
        }
    }
    None
}

fn token_compound_rejection(tokens: &[ShellToken]) -> Option<String> {
    let mut at_command_start = true;
    for token in tokens {
        match token {
            ShellToken::Word(word) => {
                if at_command_start
                    && let Some(kind) = unquoted_word_text(word).and_then(compound_kind)
                {
                    return Some(format!("{kind} commands are not supported"));
                }
                at_command_start = false;
            }
            ShellToken::Pipe
            | ShellToken::PipeErr
            | ShellToken::And
            | ShellToken::Or
            | ShellToken::Semi => at_command_start = true,
            ShellToken::Background | ShellToken::Redir(_) => {}
        }
    }
    None
}

fn source_rejection(source: &str, top_level: bool, depth: usize) -> Option<String> {
    let tokens = lex_shell(source).ok()?;
    if tokens
        .iter()
        .any(|token| matches!(token, ShellToken::Background))
    {
        return Some(BACKGROUND_JOBS_REFUSED.to_string());
    }
    if let Some(rejection) = token_compound_rejection(&tokens) {
        return Some(rejection);
    }
    let parsed = ShellParser::new(source).parse_line().ok()?;
    let whole_line = top_level
        && parsed.chains.len() == 1
        && parsed.chains[0].op == ChainOp::Start
        && parsed.chains[0].pipeline.commands.len() == 1;
    for chain in &parsed.chains {
        for command in &chain.pipeline.commands {
            if let Some(rejection) = command_rejection(command, whole_line, depth) {
                return Some(rejection);
            }
        }
    }
    None
}

fn plan_rejection(line: &str) -> Option<String> {
    source_rejection(line, true, 0)
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
    let is_exit = is_exit_command(line);
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

    if !xsh_source && !matches!(history_line.trim(), "l" | "ll") {
        session
            .history
            .add_in_dir(&history_line, Some(&history_cwd));
    }

    let plan_line = session.aliases.expand_line(&line);
    if let Some(rejection) = plan_rejection(&plan_line) {
        let _ = stderr.write_all(wrapped_diagnostic(&format!("xshi: {rejection}")).as_bytes());
        let _ = stderr.flush();
        session.last_status = 2;
        session.dollar_status = 2;
        session.last_process_status = Some(ProcessStatus::exited(2));
        return LineOutcome::Continue;
    }

    let first_word = line.split_whitespace().next().unwrap_or("");
    if !xsh_source {
        match first_word {
            "exit" => {
                if let Some(code) = handle_exit_command(session, ui, &line, stderr) {
                    return LineOutcome::Exit(code);
                }
                return LineOutcome::Continue;
            }
            "alias" if is_simple_word_line(&line) => {
                let words = alias::lex_words(line.trim_start().strip_prefix("alias").unwrap_or(""));
                let mut out = Vec::new();
                let mut err = Vec::new();
                let status = run_alias(session, &words, &mut out, &mut err);
                let _ = stdout.write_all(&out);
                let _ = stdout.flush();
                let _ = stderr.write_all(&err);
                let _ = stderr.flush();
                session.last_status = status;
                session.dollar_status = status;
                session.last_process_status = Some(ProcessStatus::exited(status));
                return LineOutcome::Continue;
            }
            "copy-scrollback" => {
                let encoded = base64_encode(ui.session_log.as_bytes());
                let _ = stdout.write_all(format!("\x1b]52;c;{encoded}\x07").as_bytes());
                let _ = stdout.flush();
                session.last_status = 0;
                session.dollar_status = 0;
                session.last_process_status = Some(ProcessStatus::exited(0));
                return LineOutcome::Continue;
            }
            "xshi-dump" => {
                let (status, out, err) = repl::write_layout_dump(session, ui);
                let _ = stdout.write_all(out.as_bytes());
                let _ = stdout.flush();
                let _ = stderr.write_all(err.as_bytes());
                let _ = stderr.flush();
                session.last_status = status;
                session.dollar_status = status;
                session.last_process_status = Some(ProcessStatus::exited(status));
                return LineOutcome::Continue;
            }
            _ => {}
        }
    }

    // Commands the shell answers itself never leave the terminal mid-line;
    // everything else may have written arbitrary output. The alias-expanded
    // line decides, as it does in the reference shell.
    let intercepted = !xsh_source && is_intercepted_line(&plan_line);

    let dollar_before = session.dollar_status;
    let output = execute_line(session, &line);
    // A command stopped by Ctrl-Z never finished, so it leaves `$?` alone.
    let stopped = output.status == 148 && session.job.is_some();
    if stopped {
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

    if xsh_source {
        let recorded = output.history_source.as_deref().unwrap_or(&history_line);
        session
            .history
            .add_in_dir(recorded, Some(&history_cwd));
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
    match first {
        "alias" | "history" | "denv" | "fg" | "z" | "w" | "which" | "type" | "cd" => {
            is_simple_word_line(line)
        }
        _ => false,
    }
}

/// Wraps a diagnostic at the terminal width on word boundaries, so the
/// wrapped rows are the ones the reference shell's terminal produces without
/// leaving a space at the start of a continuation row.
fn wrapped_diagnostic(text: &str) -> String {
    let cols = usize::from(super::term::term_size().1).max(1);
    let mut out = String::with_capacity(text.len() + 1);
    let mut column = 0_usize;
    for word in text.split(' ') {
        let width = word.chars().count();
        if column > 0 && column + 1 + width > cols {
            out.push('\n');
            column = 0;
        } else if column > 0 {
            out.push(' ');
            column += 1;
        }
        out.push_str(word);
        column += width;
    }
    out.push('\n');
    out
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
/// sets and exports `NAME` (extra words join with spaces). Any other first
/// word, including an option form, is refused as a variable name.
fn set_builtin(session: &mut Session, args: &[String], stdout: &mut Vec<u8>, stderr: &mut Vec<u8>) -> i32 {
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
    if !valid_env_name(first) {
        writeln!(stderr, "xshi: set: expected a variable name").ok();
        return 2;
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
    let refuse = |session: &mut Session, stderr: &mut io::Stderr, message: String, status: i32| {
        let _ = writeln!(stderr, "xshi: exit: {message}");
        session.last_status = status;
        session.dollar_status = status;
        session.last_process_status = Some(ProcessStatus::exited(status));
        None
    };
    let words = lex_shell(line).unwrap_or_default();
    let mut operands = Vec::new();
    for token in words.iter().skip(1) {
        if let ShellToken::Word(word) = token {
            match expand_word(session, word) {
                Ok(fields) => operands.extend(fields),
                Err(error) => {
                    return refuse(session, stderr, format!("expansion error: {}", error.message), 1);
                }
            }
        }
    }
    let code = match operands.as_slice() {
        [] => 0,
        [status] => match status.parse::<i32>() {
            Ok(code) => code,
            Err(_) => return refuse(session, stderr, format!("{status}: numeric argument required"), 2),
        },
        _ => return refuse(session, stderr, "too many arguments".to_owned(), 2),
    };
    if session.job.is_some() {
        if ui.exit_warned {
            terminate_job(session);
            session.history.compact();
            return Some(code);
        }
        let _ = writeln!(
            stderr,
            "xshi: there is a suspended job. Exit again to force quit."
        );
        ui.exit_warned = true;
        session.last_status = 1;
        session.dollar_status = 1;
        session.last_process_status = Some(ProcessStatus::exited(1));
        return None;
    }
    session.history.compact();
    Some(code)
}

/// Sends SIGTERM to the managed job's process group and forgets it. A pipeline
/// job's drop then kills and reaps every stage that outlives the signal.
pub(super) fn terminate_job(session: &mut Session) {
    if let Some(job) = session.job.take() {
        let group = job.process_group();
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

    // Aliases expand textually on the first word of the whole line, as the
    // reference shell does before dispatch.
    let expanded = session.aliases.expand_line(source);
    let source = expanded.as_ref();

    match ShellParser::new(source).parse_line() {
        Ok(line) => execute_shell_line(session, line, source),
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
    if is_exit_command(source) {
        let mut ui = Ui::new(-1);
        return match handle_exit_command(&mut session, &mut ui, source, &mut stderr) {
            Some(code) => code,
            None => session.last_status,
        };
    }
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

fn execute_shell_line(session: &mut Session, line: ShellLine, source: &str) -> CommandOutput {
    if line.background {
        return refusal_output(BACKGROUND_JOBS_REFUSED);
    }

    let last_status = session.last_status;
    let last_process_status = session.last_process_status.clone();
    run_chains(session, line.chains, last_status, last_process_status, source)
}

fn run_chains(
    session: &mut Session,
    chains: Vec<Chain>,
    mut last_status: i32,
    mut last_process_status: Option<ProcessStatus>,
    source: &str,
) -> CommandOutput {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    for chain in chains {
        let should_run = match chain.op {
            ChainOp::Start | ChainOp::Sequence => true,
            ChainOp::And => last_status == 0,
            ChainOp::Or => last_status != 0,
        };
        if !should_run {
            continue;
        }
        let job_before = session.job.as_ref().map(|job| job.process_group().id());
        let output = execute_pipeline(session, chain.pipeline, source);
        stdout.extend(output.stdout);
        stderr.extend(output.stderr);
        last_status = output.status;
        last_process_status = output.process_status;
        session.last_status = last_status;
        session.dollar_status = last_status;
        if last_process_status.is_some() {
            session.last_process_status = last_process_status.clone();
        }
        if new_stopped_job(session, job_before) {
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

fn new_stopped_job(session: &Session, job_before: Option<libc::pid_t>) -> bool {
    session
        .job
        .as_ref()
        .is_some_and(|job| Some(job.process_group().id()) != job_before)
}

fn refusal_output(message: &str) -> CommandOutput {
    CommandOutput {
        status: 2,
        stdout: Vec::new(),
        stderr: text_bytes(format!("xshi: {message}\n")),
        process_status: Some(ProcessStatus::exited(2)),
        history_source: None,
    }
}

fn execute_pipeline(session: &mut Session, pipeline: Pipeline, source: &str) -> CommandOutput {
    if pipeline.commands.len() == 1 {
        return execute_simple_command(session, pipeline.commands.into_iter().next().unwrap());
    }

    if pipeline
        .commands
        .iter()
        .any(|command| is_builtin_stage(session, command))
    {
        return execute_mixed_pipeline(session, pipeline, source);
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

    run_external_pipeline(session, &pipeline.commands, &pipeline.pipes, source)
}

/// Runs a pipeline of programs. `|&` also sends the left program's standard
/// error into the pipe. A foreground pipeline is one job: its stages share a
/// process group, and a stop suspends and resumes every stage together.
fn run_external_pipeline(
    session: &mut Session,
    commands: &[SimpleCommand],
    pipes: &[PipeOp],
    source: &str,
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

    if session.capturing {
        return run_captured_pipeline(session, invocations);
    }
    run_foreground_pipeline(session, invocations, source)
}

fn run_captured_pipeline(
    session: &Session,
    mut invocations: Vec<ProcessInvocation>,
) -> CommandOutput {
    // Inside a command substitution the last program writes to a pipe the
    // shell reads, through /dev/fd so the process layer needs no new option.
    let capture = match StdoutCapture::new() {
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
            sink
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
    };

    let mut policy = InteractiveNoCancellation;
    let end = run_pipeline_inherit_with_policy(&invocations, &mut policy);
    let stdout = capture.finish();
    match end {
        Ok(end) => {
            let status = end.status.unwrap_or_else(|| ProcessStatus::exited(0));
            let stderr = pipeline_exec_diagnostics(session, &status);
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

fn run_foreground_pipeline(
    session: &mut Session,
    invocations: Vec<ProcessInvocation>,
    source: &str,
) -> CommandOutput {
    let target = invocations
        .last()
        .map_or(&[][..], |invocation| invocation.target.as_slice());
    let mut job = match ProcessGroup::spawn_pipeline_job(&invocations) {
        Ok(job) => job,
        Err(mut error) => return pipeline_start_error(session, &mut error),
    };
    let group = job.process_group();
    let foreground = signal::ForegroundJob::take(group);
    let outcomes = match job.wait() {
        Ok(outcomes) => outcomes,
        Err(error) => {
            drop(foreground);
            return CommandOutput {
                status: 1,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xshi: {}\n", error.message)),
                process_status: Some(process_error_status(target, error)),
                history_source: None,
            };
        }
    };
    if !pipeline_stopped(&outcomes) {
        let status = pipeline_status(&outcomes);
        drop(foreground);
        return status_from_process_output(CommandOutput {
            status: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            process_status: Some(status),
            history_source: None,
        });
    }

    if session.job.is_some() {
        let _ = io::stderr().write_all(
            b"xshi: a job is already suspended; resume it with fg before suspending another\n",
        );
        let status = loop {
            job.resume();
            match job.wait() {
                Ok(outcomes) if !pipeline_stopped(&outcomes) => {
                    break pipeline_status(&outcomes);
                }
                Ok(_) => {}
                Err(error) => break process_error_status(target, error),
            }
        };
        drop(foreground);
        return status_from_process_output(CommandOutput {
            status: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            process_status: Some(status),
            history_source: None,
        });
    }

    let attrs = terminal_attrs();
    drop(foreground);
    let pgid = group.id();
    // A pipeline runs on the shell's own terminal, so the terminal driver's
    // `^Z` echo stays in front of the notice, as it does in the reference
    // shell; unlike a single command, there is no converged session to hide it.
    let stderr = text_bytes(format!("xshi: stopped: {source} (pgid={pgid})\n"));
    session.job = Some(InteractiveJob {
        process: InteractiveJobProcess::Pipeline(job),
        command: source.to_string(),
        terminal_attrs: attrs,
    });
    CommandOutput {
        status: 148,
        stdout: Vec::new(),
        stderr,
        process_status: Some(ProcessStatus::exited(148)),
        history_source: None,
    }
}

/// The pipeline status rule: the last stage decides, and a stage killed by a
/// signal counts as `128 + signal`.
fn pipeline_status(outcomes: &[ChildWaitOutcome]) -> ProcessStatus {
    let mut segments = Vec::new();
    for outcome in outcomes {
        if let ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) = outcome {
            segments.extend(status.segments.iter().cloned());
        }
    }
    ProcessStatus::from_segments(segments)
}

fn pipeline_stopped(outcomes: &[ChildWaitOutcome]) -> bool {
    outcomes
        .iter()
        .any(|outcome| matches!(outcome, ChildWaitOutcome::Stopped { .. }))
}

fn pipeline_start_error(session: &Session, error: &mut RunError) -> CommandOutput {
    let Some(status) = error.status.take() else {
        return CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: text_bytes(format!("xshi: {}\n", error.message)),
            process_status: Some(ProcessStatus::exited(1)),
            history_source: None,
        };
    };
    status_from_process_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: pipeline_exec_diagnostics(session, &status),
        process_status: Some(*status),
        history_source: None,
    })
}

fn pipeline_exec_diagnostics(session: &Session, status: &ProcessStatus) -> Vec<u8> {
    let mut stderr = Vec::new();
    for segment in &status.segments {
        if segment.kind == ProcessSegmentStatusKind::Exec {
            let kind = segment.error_kind.clone().unwrap_or_default();
            let mut error = RunError::new(kind, segment.error_message.clone().unwrap_or_default());
            if let Some(message) = exec_failure_message(session, &segment.target, &mut error) {
                stderr.extend_from_slice(message.as_bytes());
            }
        }
    }
    stderr
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
                | SessionBuiltin::History
                | SessionBuiltin::Z
                | SessionBuiltin::Denv
                | SessionBuiltin::Fg
        ) || (matches!(
            kind,
            SessionBuiltin::Alias | SessionBuiltin::Export | SessionBuiltin::Set
        ) && !args.is_empty());
        let output = if mutating {
            let mut detached = session.fork_for_substitution();
            detached.capturing = kind == SessionBuiltin::History;
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
fn execute_mixed_pipeline(
    session: &mut Session,
    pipeline: Pipeline,
    source: &str,
) -> CommandOutput {
    let Pipeline { commands, pipes } = pipeline;
    let last_builtin = commands
        .iter()
        .rposition(|command| is_builtin_stage(session, command))
        .expect("caller found a builtin stage");
    let mut stderr = Vec::new();
    let mut statuses = Vec::new();
    let mut carry: Option<Vec<u8>> = None;

    for (index, command) in commands.iter().enumerate().take(last_builtin + 1) {
        let job_before = session.job.as_ref().map(|job| job.process_group().id());
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
            if new_stopped_job(session, job_before) {
                return CommandOutput {
                    status: 148,
                    stdout: Vec::new(),
                    stderr,
                    process_status: Some(ProcessStatus::exited(148)),
                    history_source: None,
                };
            }
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
            source,
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

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = match kind {
        SessionBuiltin::Fg => unreachable!("fg returned above"),
        SessionBuiltin::Noop => 0,
        SessionBuiltin::Cd => builtin_cd(session, args, &mut stderr),
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
        SessionBuiltin::Set => set_builtin(session, args, &mut stdout, &mut stderr),
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
        SessionBuiltin::History => {
            if session.capturing {
                stderr.extend_from_slice(
                    b"history requires the original shell process and cannot run in a forked context\n",
                );
                1
            } else {
                match args.first().map(String::as_str) {
                    None => {
                        let mut indices = Vec::new();
                        session.history.command_indices_into(&mut indices);
                        for index in indices {
                            writeln!(stdout, "{}", session.history.display(index)).ok();
                        }
                        0
                    }
                    Some("-h" | "--help") => {
                        stdout.extend_from_slice(
                            b"Usage: history [reset]\nShow saved history, or clear it with `reset`.",
                        );
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
                }
            }
        }
        SessionBuiltin::Clear => {
            stdout.extend_from_slice(b"\x1b[H\x1b[2J");
            0
        }
        SessionBuiltin::Fdu => builtin::run_fdu(args, &mut stdout, &mut stderr),
        SessionBuiltin::List => builtin::list_directory(args, false, &mut stdout, &mut stderr),
        SessionBuiltin::ListHidden => builtin::list_directory(args, true, &mut stdout, &mut stderr),
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
                stderr.extend_from_slice(b"denv: usage: denv <allow|deny|reload>\n");
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
    match poll_job_terminated(&mut job) {
        Ok(Some(status)) => {
            let stderr = text_bytes(format!(
                "xshi: completed: {} (pid={}, {})\n",
                job.command,
                job.pid(),
                process_status_label(&status)
            ));
            return status_command_output(status, stderr);
        }
        Ok(None) => {}
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
    let group = job.process_group();
    let foreground = signal::ForegroundJob::take(group);
    if let Some(attrs) = job.terminal_attrs.as_ref() {
        restore_terminal_attrs(attrs);
    }
    resume_job(&mut job);
    match wait_job_foreground(&mut job) {
        Ok(JobForegroundOutcome::Finished(status)) => {
            drop(foreground);
            status_command_output(status, Vec::new())
        }
        Ok(JobForegroundOutcome::Stopped) => {
            // Stopped again: the job stays suspended in the slot, silently.
            job.terminal_attrs = terminal_attrs();
            drop(foreground);
            let stderr = erase_stopped_key_echo(job.terminal_attrs.as_ref());
            session.job = Some(job);
            CommandOutput {
                status: 148,
                stdout: Vec::new(),
                stderr,
                process_status: Some(ProcessStatus::exited(148)),
                history_source: None,
            }
        }
        Ok(JobForegroundOutcome::Running) => {
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

enum JobForegroundOutcome {
    Finished(ProcessStatus),
    Stopped,
    Running,
}

fn poll_job_terminated(job: &mut InteractiveJob) -> Result<Option<ProcessStatus>, RunError> {
    match &mut job.process {
        InteractiveJobProcess::Single(child) => Ok(match poll_managed(child)? {
            ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) => Some(status),
            ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning => None,
        }),
        InteractiveJobProcess::Pipeline(pipeline) => {
            let outcomes = pipeline.wait()?;
            Ok(if pipeline_stopped(&outcomes) {
                None
            } else {
                Some(pipeline_status(&outcomes))
            })
        }
    }
}

fn resume_job(job: &mut InteractiveJob) {
    let group = job.process_group();
    match &mut job.process {
        InteractiveJobProcess::Single(_) => group.signal(libc::SIGCONT),
        InteractiveJobProcess::Pipeline(pipeline) => pipeline.resume(),
    }
}

fn wait_job_foreground(job: &mut InteractiveJob) -> Result<JobForegroundOutcome, RunError> {
    match &mut job.process {
        InteractiveJobProcess::Single(child) => {
            let mut policy = InteractiveNoCancellation;
            let (outcome, _) = wait_managed(child, WaitMode::InteractiveForeground, &mut policy)?;
            Ok(match outcome {
                ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) => {
                    JobForegroundOutcome::Finished(status)
                }
                ChildWaitOutcome::Stopped { .. } => JobForegroundOutcome::Stopped,
                ChildWaitOutcome::StillRunning => JobForegroundOutcome::Running,
            })
        }
        InteractiveJobProcess::Pipeline(pipeline) => {
            let outcomes = pipeline.wait()?;
            Ok(if pipeline_stopped(&outcomes) {
                JobForegroundOutcome::Stopped
            } else {
                JobForegroundOutcome::Finished(pipeline_status(&outcomes))
            })
        }
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

/// A stopped child leaves the terminal's control-key echo (`^Z`) on the row
/// before the notice. The reference shell intercepts the suspend key before
/// its line discipline can echo it; erase the two columns here instead.
fn erase_stopped_key_echo(attrs: Option<&Termios>) -> Vec<u8> {
    match attrs {
        Some(attrs) if attrs.local_modes.contains(rtermios::LocalModes::ECHOCTL) => {
            b"\x08\x08  \x08\x08".to_vec()
        }
        _ => Vec::new(),
    }
}

fn stopped_notice(attrs: Option<&Termios>, name: &str, pgid: libc::pid_t) -> Vec<u8> {
    let mut stderr = erase_stopped_key_echo(attrs);
    stderr.extend_from_slice(format!("xshi: stopped: {name} (pgid={pgid})\n").as_bytes());
    stderr
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
            let foreground = signal::ForegroundJob::take(child.process_group());
            let mut policy = InteractiveNoCancellation;
            match wait_managed(&mut child, WaitMode::InteractiveForeground, &mut policy) {
                Ok((ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status), _)) => {
                    status
                }
                Ok((ChildWaitOutcome::Stopped { signal: _ }, _)) => {
                    if session.job.is_some() {
                        let attrs = terminal_attrs();
                        let mut message = erase_stopped_key_echo(attrs.as_ref());
                        message.extend_from_slice(
                            b"xshi: a job is already suspended; resume it with fg before suspending another\n",
                        );
                        let _ = io::stderr().write_all(&message);
                        let group = ProcessGroup::from_pgid(new_root_process_group_id(child.pid));
                        group.signal(libc::SIGCONT);
                        let mut policy = InteractiveNoCancellation;
                        let status = loop {
                            match wait_managed(
                                &mut child,
                                WaitMode::InteractiveForeground,
                                &mut policy,
                            ) {
                                Ok((
                                    ChildWaitOutcome::Exited(status)
                                    | ChildWaitOutcome::Signaled(status),
                                    _,
                                )) => break status,
                                Ok((ChildWaitOutcome::Stopped { .. }, _)) => {
                                    group.signal(libc::SIGCONT);
                                }
                                Ok((ChildWaitOutcome::StillRunning, _)) => {
                                    break ProcessStatus::signaled(libc::SIGTERM);
                                }
                                Err(error) => {
                                    break process_error_status(&invocation.target, error);
                                }
                            }
                        };
                        drop(foreground);
                        return status_from_process_output(CommandOutput {
                            status: 0,
                            stdout: Vec::new(),
                            stderr: Vec::new(),
                            process_status: Some(status),
                            history_source: Some(source),
                        });
                    }
                    let attrs = terminal_attrs();
                    drop(foreground);
                    let name = String::from_utf8_lossy(&invocation.target).into_owned();
                    let pgid = new_root_process_group_id(child.pid);
                    let stderr = stopped_notice(attrs.as_ref(), &name, pgid);
                    session.job = Some(InteractiveJob {
                        process: InteractiveJobProcess::Single(Box::new(child)),
                        command: name.clone(),
                        terminal_attrs: attrs,
                    });
                    return CommandOutput {
                        status: 148,
                        stdout: Vec::new(),
                        stderr,
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
    // Only a single child has a nonblocking waiter; a suspended pipeline has
    // no running stage to observe, so `fg` reports its state instead.
    let InteractiveJobProcess::Single(child) = &mut job.process else {
        return;
    };
    match poll_managed(child) {
        Ok(ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning) => {}
        Ok(ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status)) => {
            let command = job.command.clone();
            let pid = child.pid;
            let label = process_status_label(&status);
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
        namespaces: None,
        confinement: None,
        rooted_executable: None,
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

fn shell_compound_header(source: &str, first: &str) -> bool {
    let Ok(tokens) = lex_shell(source) else {
        return false;
    };
    let has = |keyword: &str| {
        tokens.iter().any(
            |token| matches!(token, ShellToken::Word(word) if unquoted_word_text(word) == Some(keyword)),
        )
    };
    match first {
        "if" => has("then") && has("fi"),
        "for" | "while" => has("do") && has("done"),
        _ => false,
    }
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
    if matches!(first, "if" | "for" | "while") && shell_compound_header(trimmed, first) {
        return false;
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
    let output = execute_shell_line(&mut nested, line, source);
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
        ExpansionError::usage(format!("0:0: arithmetic error: {}", message.into()))
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
    Export,
    Fg,
    Noop,
    Cd,
    Set,
    Unset,
    Alias,
    Z,
    Denv,
    Clear,
    Fdu,
    List,
    ListHidden,
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
        "export" => SessionBuiltin::Export,
        "fg" => SessionBuiltin::Fg,
        ":" => SessionBuiltin::Noop,
        "cd" => SessionBuiltin::Cd,
        "set" => SessionBuiltin::Set,
        "unset" => SessionBuiltin::Unset,
        "alias" => SessionBuiltin::Alias,
        "z" => SessionBuiltin::Z,
        "denv" => SessionBuiltin::Denv,
        "c" => SessionBuiltin::Clear,
        "fdu" => SessionBuiltin::Fdu,
        "l" => SessionBuiltin::List,
        "ll" => SessionBuiltin::ListHidden,
        "w" | "which" | "type" => SessionBuiltin::Which,
        "history" => SessionBuiltin::History,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ChainOp, ProcessStatus, Session, ShellParser, ShellRedirectionKind, ShellToken, ShellWord,
        ShellWordPart, execute_line, is_xsh_source, lex_shell, plan_rejection, set_env_bytes,
        shell_status, validate_assignment_prefix,
    };
    use crate::xshi::interactive::denv::DenvState;
    use crate::xshi::interactive::history::History;
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
    fn ish_refused_syntax_is_rejected_before_running() {
        for (line, message) in [
            ("sleep 1 &", "background jobs (`&`) are not supported"),
            ("sleep 1 & echo ran", "background jobs (`&`) are not supported"),
            ("if true; then echo ran; fi", "if commands are not supported"),
            ("for i in a; do echo ran; done", "for commands are not supported"),
            ("while false; do :; done", "while commands are not supported"),
            ("until false; do :; done", "until commands are not supported"),
            ("case x in x) :;; esac", "case commands are not supported"),
            ("f() { echo ran; }", "function definitions are not supported"),
            ("eval echo ran", "`eval` is not supported"),
            ("exec echo ran", "`exec` is not supported"),
            ("source /dev/null", "`source` is not supported"),
            ("trap ignored TERM", "`trap` is not supported"),
            ("echo ${HOME:-ran}", "parameter operators are not supported"),
            ("echo ${#HOME}", "parameter operators are not supported"),
            ("echo ${HOME%x}", "parameter operators are not supported"),
            ("echo $(( ${HOME:-1} ))", "parameter operators are not supported"),
            ("echo \"${HOME:-ran}\"", "parameter operators are not supported"),
            ("echo $(eval ran)", "`eval` is not supported"),
            ("echo `eval ran`", "`eval` is not supported"),
            ("z nothing; echo ran", "must be the only command on its line"),
            ("command exit", "must be the only command on its line"),
        ] {
            let rejection = plan_rejection(line).unwrap_or_else(|| panic!("{line} was accepted"));
            assert!(
                rejection.contains(message),
                "{line}: {rejection} did not contain {message}"
            );
        }

        for line in [
            "echo ${HOME}",
            "echo '${HOME:-literal}'",
            "echo \\${HOME:-literal}",
            "echo ok${HOME}; echo fine",
            "sleep 1 && echo fine",
        ] {
            assert_eq!(plan_rejection(line), None, "{line}");
        }
    }

    #[test]
    fn xsh_control_starts_are_not_shell_compound_commands() {
        assert!(is_xsh_source("if true { print 1 }"));
        assert!(is_xsh_source("for x in [1] { print x }"));
        assert!(is_xsh_source("while false { print 1 }"));
        assert!(!is_xsh_source("if true; then echo x; fi"));
        assert!(!is_xsh_source("for i in a; do echo $i; done"));
        assert!(!is_xsh_source("while false; do :; done"));

        let mut session = Session::for_test();
        let output = execute_line(&mut session, "if true { print 1 }");
        assert_eq!(output.status, 0, "{:?}", output.stderr);
        assert_eq!(output.stdout, b"1\n");
    }

    #[test]
    fn aliases_expand_before_builtin_dispatch() {
        let mut session = Session::for_test();
        let w = execute_line(&mut session, "w fg");
        assert_eq!(w.status, 0);
        assert!(String::from_utf8(w.stdout).unwrap().contains("fg: builtin"));

        session.aliases.set(
            "fg".to_string(),
            vec!["echo".to_string(), "alias".to_string()],
        );

        let fg = execute_line(&mut session, "fg");
        assert_eq!(fg.status, 0);
        assert_eq!(String::from_utf8(fg.stdout).unwrap(), "alias\n");
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
