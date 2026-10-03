//! Automatic `.envrc` / `.env` loading.
//!
//! The shell owns discovery, trust, the active-state restore, and the prompt
//! flag directly. `.env` is parsed here. A trusted `.envrc` is evaluated by
//! `bash` when its first line mentions bash, as XSH source when it mentions
//! `xsh`, and by `sh` otherwise; the result is diffed against the session's
//! exported environment.
//!
//! State lives in exported variables so scripts and the prompt can see it:
//! `__DENV_DIR`, `__DENV_DIRTY`, and `__DENV_STATE` (`ENVRC_MTIME DOTENV_MTIME
//! DIR`). Trust is stored per `.envrc` under `~/.local/share/xshi/denv/allow/`
//! as the file's mtime, so editing the file revokes trust.

use super::session::Session;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use xsh::diagnostic::DiagnosticRenderer;
use xsh::execution::evaluator::Evaluator;
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceMap;
use xsh::frontend::syntax::parser::Parser;

const BASH_STDLIB: &str = include_str!("denv_stdlib.sh");

type EnvMap = BTreeMap<Vec<u8>, Vec<u8>>;

/// Per-shell denv bookkeeping.
#[derive(Clone, Debug, Default)]
pub(super) struct DenvState {
    /// Directory holding the `allow/` trust files.
    data_dir: Option<PathBuf>,
    /// What loading the current directory changed, for restoring on leave.
    active: Option<ActiveState>,
}

#[derive(Clone, Debug)]
struct ActiveState {
    prev: Vec<PrevVar>,
}

#[derive(Clone, Debug)]
enum PrevVar {
    Restore(String, Vec<u8>),
    Unset(String),
}

struct EnvFiles {
    dir: PathBuf,
    envrc: Option<(PathBuf, u64)>,
    dotenv: Option<(PathBuf, u64)>,
}

struct EnvDiff {
    set: Vec<(String, Vec<u8>)>,
    unset: Vec<String>,
}

enum EnvRcInterpreter {
    Bash,
    Sh,
    Xsh,
}

pub(super) enum DenvCommand {
    Allow,
    Deny,
    Reload,
}

impl DenvState {
    pub(super) fn load(data_dir: Option<PathBuf>) -> Self {
        Self {
            data_dir,
            active: None,
        }
    }

    fn allow_dir(&self) -> Result<PathBuf, String> {
        Ok(self.data_dir.as_ref().ok_or("HOME is unset")?.join("allow"))
    }
}

impl Session {
    /// Whether an `.envrc` in the current tree is waiting for `denv allow`.
    pub(super) fn denv_dirty(&self) -> bool {
        self.env.get(b"__DENV_DIRTY".as_slice()).map(Vec::as_slice) == Some(b"1")
    }
}

/// Exports the variables that identify this shell to `.envrc` scripts.
fn init(session: &mut Session) {
    let pid = std::process::id().to_string();
    session.export_var(b"__DENV_PID", pid.as_bytes());
    session.export_var(b"__DENV_SHELL", b"bash");
}

/// Called once at shell startup.
pub(super) fn startup(session: &mut Session, stderr: &mut dyn Write) {
    init(session);
    let result = refresh_changes(session, false);
    apply(session, result, stderr);
}

/// Called after any directory change.
pub(super) fn after_cwd_change(session: &mut Session, stderr: &mut dyn Write) {
    let result = refresh_changes(session, false);
    apply(session, result, stderr);
}

/// Runs `denv allow|deny|reload` and returns its status.
pub(super) fn run_command(
    session: &mut Session,
    command: &DenvCommand,
    stderr: &mut dyn Write,
) -> i32 {
    let result = match command {
        DenvCommand::Allow => allow_current_dir(session, stderr),
        DenvCommand::Deny => deny_current_dir(session, stderr),
        DenvCommand::Reload => refresh_changes(session, true),
    };
    match result {
        Ok(changes) => {
            apply_changes(session, changes, stderr);
            0
        }
        Err(err) => {
            writeln!(stderr, "denv: {err}").ok();
            1
        }
    }
}

fn apply(session: &mut Session, result: Result<Vec<EnvChange>, String>, stderr: &mut dyn Write) {
    match result {
        Ok(changes) => apply_changes(session, changes, stderr),
        Err(err) => {
            writeln!(stderr, "denv: {err}").ok();
        }
    }
}

enum EnvChange {
    Set(String, Vec<u8>),
    Unset(String),
    /// A line for the terminal, kept in order with the changes around it.
    Note(String),
}

fn apply_changes(session: &mut Session, changes: Vec<EnvChange>, stderr: &mut dyn Write) {
    for change in changes {
        match change {
            EnvChange::Set(name, value) => session.export_var(name.as_bytes(), &value),
            EnvChange::Unset(name) => session.unset_var(name.as_bytes()),
            EnvChange::Note(line) => {
                writeln!(stderr, "{line}").ok();
            }
        }
    }
}

fn set_change(changes: &mut Vec<EnvChange>, name: &str, value: impl Into<Vec<u8>>) {
    changes.push(EnvChange::Set(name.to_string(), value.into()));
}

fn unset_change(changes: &mut Vec<EnvChange>, name: &str) {
    changes.push(EnvChange::Unset(name.to_string()));
}

fn refresh_changes(session: &mut Session, force: bool) -> Result<Vec<EnvChange>, String> {
    let cwd = session.cwd.clone();

    // Fast path: the cached directory still holds the tree we are in and its
    // files have not changed.
    if !force
        && let Some(state) = state_value(session)
        && fast_path_ok(&state, &cwd)
    {
        return Ok(Vec::new());
    }

    let found = find_env_files(&cwd);

    if !force
        && let Some(found) = &found
        && state_matches(session, found)
    {
        return Ok(Vec::new());
    }

    let active = session.denv.active.take();
    let mut changes = Vec::new();

    if let Some(state) = &active {
        apply_restore(&state.prev, &mut changes);
    }
    // Everything after this point starts from the restored environment.
    let mut baseline = session.env.clone();
    for change in &changes {
        match change {
            EnvChange::Set(name, value) => {
                baseline.insert(name.as_bytes().to_vec(), value.clone());
            }
            EnvChange::Unset(name) => {
                baseline.remove(name.as_bytes());
            }
            EnvChange::Note(_) => {}
        }
    }

    let Some(found) = found else {
        clear_runtime_state(&mut changes);
        if let Some(state) = &active {
            print_restore_summary(&state.prev, &mut changes);
        }
        return Ok(changes);
    };

    let envrc_mtime = found.envrc.as_ref().map_or(0, |(_, mtime)| *mtime);
    let dotenv_mtime = found.dotenv.as_ref().map_or(0, |(_, mtime)| *mtime);

    if let Some((envrc_path, _)) = &found.envrc
        && !is_allowed(session, envrc_path)
    {
        let dir = canonicalize_fallback(&found.dir);
        let envrc = canonicalize_fallback(envrc_path);
        changes.push(EnvChange::Note(format!(
            "denv: {} is blocked. Run `denv allow` to trust it.",
            envrc.display()
        )));
        set_change(&mut changes, "__DENV_DIR", dir.as_os_str().as_bytes());
        set_change(&mut changes, "__DENV_DIRTY", "1");
        unset_change(&mut changes, "__DENV_STATE");
        if let Some(state) = &active {
            print_restore_summary(&state.prev, &mut changes);
        }
        return Ok(changes);
    }

    let dotenv_entries = load_dotenv_entries(&found)?;
    if found.envrc.is_some() {
        changes.push(EnvChange::Note("denv: loading .envrc".to_string()));
    }
    if found.dotenv.is_some() {
        changes.push(EnvChange::Note("denv: loading .env".to_string()));
    }

    let diff = if let Some((envrc_path, _)) = &found.envrc {
        let dir = canonicalize_fallback(&found.dir);
        let envrc = canonicalize_fallback(envrc_path);
        match eval_env(session, &dir, &envrc, &dotenv_entries, &baseline) {
            Ok(diff) => diff,
            Err(err) => {
                changes.push(EnvChange::Note(format!("denv: {err}")));
                set_change(&mut changes, "__DENV_DIR", dir.as_os_str().as_bytes());
                set_change(&mut changes, "__DENV_DIRTY", "1");
                unset_change(&mut changes, "__DENV_STATE");
                return Ok(changes);
            }
        }
    } else {
        diff_dotenv_only(&dotenv_entries, &baseline)
    };

    let prev = capture_prev(&diff, &baseline);
    for (name, value) in &diff.set {
        set_change(&mut changes, name, value.clone());
    }
    for name in &diff.unset {
        unset_change(&mut changes, name);
    }
    let dir = canonicalize_fallback(&found.dir);
    set_change(&mut changes, "__DENV_DIR", dir.as_os_str().as_bytes());
    unset_change(&mut changes, "__DENV_DIRTY");
    set_change(
        &mut changes,
        "__DENV_STATE",
        format!("{envrc_mtime} {dotenv_mtime} {}", found.dir.display()),
    );
    summary(
        diff.set
            .iter()
            .map(|(name, _)| ('+', name.as_str()))
            .chain(diff.unset.iter().map(|name| ('-', name.as_str()))),
        &mut changes,
    );
    session.denv.active = Some(ActiveState { prev });
    Ok(changes)
}

fn allow_current_dir(
    session: &mut Session,
    stderr: &mut dyn Write,
) -> Result<Vec<EnvChange>, String> {
    let found = find_env_files(&session.cwd).ok_or("no .envrc or .env found")?;
    let Some((envrc, _)) = found.envrc else {
        return Err("no .envrc found".to_string());
    };
    let envrc = envrc.canonicalize().unwrap_or(envrc);
    allow_envrc(session, &envrc, stderr)?;
    refresh_changes(session, true)
}

fn deny_current_dir(
    session: &mut Session,
    stderr: &mut dyn Write,
) -> Result<Vec<EnvChange>, String> {
    let found = find_env_files(&session.cwd).ok_or("no .envrc or .env found")?;
    let Some((envrc, _)) = found.envrc else {
        return Err("no .envrc found".to_string());
    };
    let envrc = envrc.canonicalize().unwrap_or(envrc);
    deny_envrc(session, &envrc, stderr)?;
    refresh_changes(session, true)
}

/// Walks up from `start` to the first directory holding `.envrc` or `.env`.
fn find_env_files(start: &Path) -> Option<EnvFiles> {
    let mut dir = start.to_path_buf();
    loop {
        let envrc = stat_file(&dir, ".envrc");
        let dotenv = stat_file(&dir, ".env");
        if envrc.is_some() || dotenv.is_some() {
            return Some(EnvFiles { dir, envrc, dotenv });
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn stat_file(dir: &Path, name: &str) -> Option<(PathBuf, u64)> {
    let path = dir.join(name);
    let mtime = regular_file_mtime(&path)?;
    Some((path, mtime))
}

fn regular_file_mtime(path: &Path) -> Option<u64> {
    let stat = rustix::fs::stat(path).ok()?;
    (rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::RegularFile)
        .then_some(stat.st_mtime as u64)
}

fn canonicalize_fallback(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn parse_denv_state(state: &str) -> Option<(u64, u64, &str)> {
    let (envrc, rest) = state.split_once(' ')?;
    let (dotenv, dir) = rest.split_once(' ')?;
    Some((envrc.parse().ok()?, dotenv.parse().ok()?, dir))
}

fn state_value(session: &Session) -> Option<String> {
    session
        .env
        .get(b"__DENV_STATE".as_slice())
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

/// Whether `cwd` is inside the directory a `__DENV_STATE` value names and the
/// files it recorded are unchanged.
fn fast_path_ok(state: &str, cwd: &Path) -> bool {
    let Some((envrc_mtime, dotenv_mtime, dir)) = parse_denv_state(state) else {
        return false;
    };
    let cached = Path::new(dir);
    if !cwd.starts_with(cached) {
        return false;
    }
    let same = |name: &str, expected: u64| {
        expected == 0 || regular_file_mtime(&cached.join(name)) == Some(expected)
    };
    same(".envrc", envrc_mtime) && same(".env", dotenv_mtime)
}

fn state_matches(session: &Session, found: &EnvFiles) -> bool {
    let envrc_mtime = found.envrc.as_ref().map_or(0, |(_, mtime)| *mtime);
    let dotenv_mtime = found.dotenv.as_ref().map_or(0, |(_, mtime)| *mtime);
    let Some(state) = state_value(session) else {
        return false;
    };
    let Some((state_envrc, state_dotenv, state_dir)) = parse_denv_state(&state) else {
        return false;
    };
    state_envrc == envrc_mtime
        && state_dotenv == dotenv_mtime
        && (state_dir == found.dir.to_string_lossy().as_ref()
            || found
                .dir
                .canonicalize()
                .is_ok_and(|dir| state_dir == dir.to_string_lossy().as_ref()))
}

fn trust_key(path: &Path) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = path.as_os_str().as_bytes();
    let mut key = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        key.push(HEX[(byte >> 4) as usize] as char);
        key.push(HEX[(byte & 0x0f) as usize] as char);
    }
    key
}

fn is_allowed(session: &Session, envrc: &Path) -> bool {
    let Ok(allow_dir) = session.denv.allow_dir() else {
        return false;
    };
    let stored = match fs::read_to_string(allow_dir.join(trust_key(envrc))) {
        Ok(stored) => stored,
        Err(_) => {
            let canonical = canonicalize_fallback(envrc);
            if canonical == envrc {
                return false;
            }
            match fs::read_to_string(allow_dir.join(trust_key(&canonical))) {
                Ok(stored) => stored,
                Err(_) => return false,
            }
        }
    };
    let Some(current) = regular_file_mtime(&canonicalize_fallback(envrc)) else {
        return false;
    };
    stored.trim().parse::<u64>() == Ok(current)
}

fn allow_envrc(session: &Session, envrc: &Path, stderr: &mut dyn Write) -> Result<(), String> {
    let dir = session.denv.allow_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create allow dir: {e}"))?;
    let mtime =
        regular_file_mtime(envrc).ok_or("failed to read .envrc mtime: not a regular file")?;
    fs::write(dir.join(trust_key(envrc)), mtime.to_string())
        .map_err(|e| format!("failed to write trust file: {e}"))?;
    writeln!(stderr, "denv: allowed {}", envrc.display()).ok();
    Ok(())
}

fn deny_envrc(session: &Session, envrc: &Path, stderr: &mut dyn Write) -> Result<(), String> {
    let trust_file = session.denv.allow_dir()?.join(trust_key(envrc));
    match fs::remove_file(&trust_file) {
        Ok(()) => {
            writeln!(stderr, "denv: denied {}", envrc.display()).ok();
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            writeln!(stderr, "denv: not currently allowed").ok();
        }
        Err(err) => return Err(format!("failed to remove trust file: {err}")),
    }
    Ok(())
}

fn load_dotenv_entries(found: &EnvFiles) -> Result<Vec<(String, String)>, String> {
    let Some((path, _)) = &found.dotenv else {
        return Ok(Vec::new());
    };
    let content = fs::read_to_string(path).map_err(|e| format!("read .env: {e}"))?;
    Ok(parse_dotenv(&content)
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.into_owned()))
        .collect())
}

fn parse_dotenv(content: &str) -> Vec<(&str, Cow<'_, str>)> {
    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some(eq) = line.find('=') else {
            continue;
        };
        let key = line[..eq].trim();
        if key.is_empty() {
            continue;
        }
        let value = line[eq + 1..].trim();
        let value = if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            let inner = &value[1..value.len() - 1];
            if inner.contains('\\') {
                let mut out = String::with_capacity(inner.len());
                let mut chars = inner.chars();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        match chars.next() {
                            Some('n') => out.push('\n'),
                            Some('t') => out.push('\t'),
                            Some('\\') => out.push('\\'),
                            Some('"') => out.push('"'),
                            Some('$') => out.push('$'),
                            Some(other) => {
                                out.push('\\');
                                out.push(other);
                            }
                            None => out.push('\\'),
                        }
                    } else {
                        out.push(c);
                    }
                }
                Cow::Owned(out)
            } else {
                Cow::Borrowed(inner)
            }
        } else if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
            Cow::Borrowed(&value[1..value.len() - 1])
        } else {
            Cow::Borrowed(value)
        };
        entries.push((key, value));
    }
    entries
}

fn diff_dotenv_only(dotenv_entries: &[(String, String)], baseline: &EnvMap) -> EnvDiff {
    let mut set = Vec::new();
    for (key, value) in dotenv_entries {
        if baseline.get(key.as_bytes()).map(Vec::as_slice) != Some(value.as_bytes()) {
            set.push((key.clone(), value.clone().into_bytes()));
        }
    }
    EnvDiff {
        set,
        unset: Vec::new(),
    }
}

fn is_ignored_env_key(key: &[u8]) -> bool {
    matches!(key, b"_" | b"SHLVL" | b"PWD" | b"OLDPWD")
}

/// Parses `env -0` output into sorted `(name, value)` pairs, skipping the
/// variables a shell manages itself.
fn parse_env_null(data: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut entries = Vec::new();
    for entry in data.split(|&b| b == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq) = entry.iter().position(|&b| b == b'=') else {
            continue;
        };
        let key = &entry[..eq];
        if is_ignored_env_key(key) {
            continue;
        }
        entries.push((key, &entry[eq + 1..]));
    }
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    entries
}

fn diff_sorted_env(before: &[(&[u8], &[u8])], after: &[(&[u8], &[u8])]) -> EnvDiff {
    let (mut before_idx, mut after_idx) = (0, 0);
    let mut set = Vec::new();
    let mut unset = Vec::new();
    let name = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    while before_idx < before.len() && after_idx < after.len() {
        match before[before_idx].0.cmp(after[after_idx].0) {
            Ordering::Less => {
                unset.push(name(before[before_idx].0));
                before_idx += 1;
            }
            Ordering::Greater => {
                set.push((name(after[after_idx].0), after[after_idx].1.to_vec()));
                after_idx += 1;
            }
            Ordering::Equal => {
                if before[before_idx].1 != after[after_idx].1 {
                    set.push((name(after[after_idx].0), after[after_idx].1.to_vec()));
                }
                before_idx += 1;
                after_idx += 1;
            }
        }
    }
    for (key, _) in &before[before_idx..] {
        unset.push(name(key));
    }
    for (key, value) in &after[after_idx..] {
        set.push((name(key), value.to_vec()));
    }
    EnvDiff { set, unset }
}

fn push_sh_escaped(out: &mut String, value: &str) {
    out.push('\'');
    let bytes = value.as_bytes();
    let mut start = 0;
    for i in 0..bytes.len() {
        if bytes[i] == b'\'' {
            out.push_str(&value[start..i]);
            out.push_str("'\\''");
            start = i + 1;
        }
    }
    out.push_str(&value[start..]);
    out.push('\'');
}

fn first_line_uses(line: &[u8], word: &[u8]) -> bool {
    line.windows(word.len()).any(|window| window == word)
}

fn envrc_interpreter(envrc: &Path) -> Result<EnvRcInterpreter, String> {
    let mut file = fs::File::open(envrc).map_err(|e| format!("read {}: {e}", envrc.display()))?;
    let mut buf = [0_u8; 256];
    let n = file
        .read(&mut buf)
        .map_err(|e| format!("read {}: {e}", envrc.display()))?;
    let first_line = buf[..n].split(|&b| b == b'\n').next().unwrap_or_default();
    Ok(if first_line_uses(first_line, b"bash") {
        EnvRcInterpreter::Bash
    } else if first_line_uses(first_line, b"xsh") {
        EnvRcInterpreter::Xsh
    } else {
        EnvRcInterpreter::Sh
    })
}

fn eval_env(
    session: &Session,
    dir: &Path,
    envrc: &Path,
    dotenv_entries: &[(String, String)],
    baseline: &EnvMap,
) -> Result<EnvDiff, String> {
    match envrc_interpreter(envrc)? {
        EnvRcInterpreter::Bash => eval_env_shell("bash", dir, envrc, dotenv_entries, baseline),
        EnvRcInterpreter::Sh => eval_env_shell("sh", dir, envrc, dotenv_entries, baseline),
        EnvRcInterpreter::Xsh => eval_env_xsh(session, dir, envrc, dotenv_entries, baseline),
    }
}

/// Evaluates a shell `.envrc` with the denv helper library and returns the
/// difference between the environment before and after, captured through two
/// pipes so nothing the script prints can be mistaken for it.
fn eval_env_shell(
    interpreter: &str,
    dir: &Path,
    envrc: &Path,
    dotenv_entries: &[(String, String)],
    baseline: &EnvMap,
) -> Result<EnvDiff, String> {
    let (before_r, before_w) = pipe_cloexec().map_err(|e| format!("create before pipe: {e}"))?;
    let (after_r, after_w) = pipe_cloexec().map_err(|e| format!("create after pipe: {e}"))?;
    // Stable source descriptors for the child remap: the pipe ends can occupy
    // 3 or 4 themselves, so remapping from them directly could overwrite the
    // other source before it is installed.
    let before_src = rustix::io::dup(&before_w).map_err(|e| format!("dup before pipe: {e}"))?;
    let after_src = rustix::io::dup(&after_w).map_err(|e| format!("dup after pipe: {e}"))?;
    rustix::io::fcntl_setfd(&before_src, rustix::io::FdFlags::CLOEXEC)
        .map_err(|e| format!("set before pipe close-on-exec: {e}"))?;
    rustix::io::fcntl_setfd(&after_src, rustix::io::FdFlags::CLOEXEC)
        .map_err(|e| format!("set after pipe close-on-exec: {e}"))?;

    let mut script = String::with_capacity(BASH_STDLIB.len() + 256);
    script.push_str(BASH_STDLIB);
    script.push('\n');
    script.push_str("env -0 >&3\n");
    script.push_str(". ");
    push_sh_escaped(&mut script, &envrc.to_string_lossy());
    script.push('\n');
    for (key, value) in dotenv_entries {
        script.push_str("export ");
        script.push_str(key);
        script.push('=');
        push_sh_escaped(&mut script, value);
        script.push('\n');
    }
    script.push_str("env -0 >&4\n");

    let stderr_dup = std::io::stderr()
        .as_fd()
        .try_clone_to_owned()
        .map_err(|e| format!("dup stderr: {e}"))?;
    let reader = |fd: OwnedFd| {
        std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
            let mut data = Vec::new();
            fs::File::from(fd).read_to_end(&mut data)?;
            Ok(data)
        })
    };
    let before_read = reader(before_r);
    let after_read = reader(after_r);

    let mut command = Command::new(interpreter);
    command.env_clear();
    for (key, value) in baseline {
        command.env(
            std::ffi::OsString::from_vec(key.clone()),
            std::ffi::OsString::from_vec(value.clone()),
        );
    }
    let (before_raw, after_raw) = (before_src.as_raw_fd(), after_src.as_raw_fd());
    // SAFETY: the closure runs in the child between fork and exec and only
    // calls dup2, which is async-signal-safe, on descriptors it inherited.
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(before_raw, 3) < 0 || libc::dup2(after_raw, 4) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let status = command
        .arg("-e")
        .arg("-c")
        .arg(&script)
        .current_dir(dir)
        .stdout(stderr_dup)
        .stderr(
            std::io::stderr()
                .as_fd()
                .try_clone_to_owned()
                .map_err(|e| format!("dup stderr for child: {e}"))?,
        )
        .status()
        .map_err(|e| format!("failed to run {interpreter}: {e}"))?;
    drop(command);
    drop(before_w);
    drop(after_w);
    drop(before_src);
    drop(after_src);

    let before_data = before_read
        .join()
        .map_err(|_| "read before env thread panicked".to_string())?
        .map_err(|e| format!("read before env: {e}"))?;
    let after_data = after_read
        .join()
        .map_err(|_| "read after env thread panicked".to_string())?
        .map_err(|e| format!("read after env: {e}"))?;

    if !status.success() {
        return Err(".envrc evaluation failed".to_string());
    }

    let before = parse_env_null(&before_data);
    let after = parse_env_null(&after_data);
    Ok(diff_sorted_env(&before, &after))
}

fn pipe_cloexec() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let (read, write) = rustix::pipe::pipe()?;
    rustix::io::fcntl_setfd(&read, rustix::io::FdFlags::CLOEXEC)?;
    rustix::io::fcntl_setfd(&write, rustix::io::FdFlags::CLOEXEC)?;
    Ok((read, write))
}

/// Evaluates an XSH `.envrc`; `.env` entries apply on top of its result.
fn eval_env_xsh(
    session: &Session,
    dir: &Path,
    envrc: &Path,
    dotenv_entries: &[(String, String)],
    baseline: &EnvMap,
) -> Result<EnvDiff, String> {
    let text =
        fs::read_to_string(envrc).map_err(|err| format!("read {}: {err}", envrc.display()))?;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(envrc.display().to_string(), text.clone());
    let parsed = Parser::parse_source_arena_only(source_id, &text);
    if !parsed.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new()
            .render(&parsed.diagnostics, &sources)
            .trim_end()
            .to_string());
    }
    let checked = Checker::check_arena_interactive(&parsed.arena, &text);
    if !checked.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new()
            .render(&checked.diagnostics, &sources)
            .trim_end()
            .to_string());
    }
    let output = Evaluator::new_interactive_session_with_sources(
        Vec::new(),
        sources,
        dir.to_path_buf(),
        baseline.clone(),
        session.last_process_status.clone(),
    )
    .eval(&parsed.arena, source_id);
    if !output.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new()
            .render(&output.diagnostics, &output.sources)
            .trim_end()
            .to_string());
    }
    if output.status != 0 {
        return Err(".envrc evaluation failed".to_string());
    }
    let mut after = output.env;
    for (key, value) in dotenv_entries {
        after.insert(key.as_bytes().to_vec(), value.as_bytes().to_vec());
    }
    let flatten = |env: &EnvMap| -> Vec<u8> {
        let mut data = Vec::new();
        for (key, value) in env {
            data.extend_from_slice(key);
            data.push(b'=');
            data.extend_from_slice(value);
            data.push(0);
        }
        data
    };
    let (before_data, after_data) = (flatten(baseline), flatten(&after));
    Ok(diff_sorted_env(
        &parse_env_null(&before_data),
        &parse_env_null(&after_data),
    ))
}

fn capture_prev(diff: &EnvDiff, baseline: &EnvMap) -> Vec<PrevVar> {
    let mut prev = Vec::new();
    for (key, _) in &diff.set {
        match baseline.get(key.as_bytes()) {
            Some(value) => prev.push(PrevVar::Restore(key.clone(), value.clone())),
            None => prev.push(PrevVar::Unset(key.clone())),
        }
    }
    for key in &diff.unset {
        if let Some(value) = baseline.get(key.as_bytes()) {
            prev.push(PrevVar::Restore(key.clone(), value.clone()));
        }
    }
    prev
}

fn apply_restore(prev: &[PrevVar], changes: &mut Vec<EnvChange>) {
    for item in prev {
        match item {
            PrevVar::Restore(key, value) => set_change(changes, key, value.clone()),
            PrevVar::Unset(key) => unset_change(changes, key),
        }
    }
}

fn clear_runtime_state(changes: &mut Vec<EnvChange>) {
    unset_change(changes, "__DENV_DIR");
    unset_change(changes, "__DENV_DIRTY");
    unset_change(changes, "__DENV_STATE");
}

fn print_restore_summary(prev: &[PrevVar], changes: &mut Vec<EnvChange>) {
    summary(
        prev.iter().map(|item| match item {
            PrevVar::Restore(key, _) | PrevVar::Unset(key) => ('-', key.as_str()),
        }),
        changes,
    );
}

/// `denv: +A -B` on one line, skipping the shell's own bookkeeping variables.
fn summary<'a>(items: impl Iterator<Item = (char, &'a str)>, changes: &mut Vec<EnvChange>) {
    let mut line = String::new();
    for (sign, key) in items {
        if key.starts_with("__DENV_") {
            continue;
        }
        if line.is_empty() {
            line.push_str("denv: ");
        } else {
            line.push(' ');
        }
        line.push(sign);
        line.push_str(key);
    }
    if !line.is_empty() {
        changes.push(EnvChange::Note(line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(prefix: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("temp dir")
    }

    #[test]
    fn parse_dotenv_skips_empty_and_comments() {
        assert!(parse_dotenv("").is_empty());
        assert!(parse_dotenv("# comment\n\n# another\n").is_empty());
    }

    #[test]
    fn parse_dotenv_plain_and_export_prefix() {
        let entries = parse_dotenv("FOO=bar\nexport BAZ=qux");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "FOO");
        assert_eq!(entries[0].1.as_ref(), "bar");
        assert_eq!(entries[1].0, "BAZ");
        assert_eq!(entries[1].1.as_ref(), "qux");
    }

    #[test]
    fn parse_dotenv_double_quotes_unescape_common_sequences() {
        let entries =
            parse_dotenv("A=\"a\\nb\"\nB=\"c\\td\"\nC=\"say \\\"hi\\\"\"\nD=\"cost \\$5\"");
        assert_eq!(entries[0].1.as_ref(), "a\nb");
        assert_eq!(entries[1].1.as_ref(), "c\td");
        assert_eq!(entries[2].1.as_ref(), "say \"hi\"");
        assert_eq!(entries[3].1.as_ref(), "cost $5");
    }

    #[test]
    fn parse_dotenv_single_quotes_are_literal() {
        let entries = parse_dotenv("A='a\\nb'");
        assert_eq!(entries[0].1.as_ref(), "a\\nb");
    }

    #[test]
    fn parse_env_null_skips_shell_internal_vars() {
        let parsed = parse_env_null(b"PWD=/tmp\0OLDPWD=/old\0SHLVL=1\0KEEP=yes\0");
        assert_eq!(parsed, vec![(&b"KEEP"[..], &b"yes"[..])]);
    }

    #[test]
    fn diff_sorted_env_reports_sets_and_unsets() {
        let before: [(&[u8], &[u8]); 3] = [(b"A", b"1"), (b"B", b"2"), (b"C", b"3")];
        let after: [(&[u8], &[u8]); 3] = [(b"A", b"1"), (b"B", b"changed"), (b"D", b"4")];
        let diff = diff_sorted_env(&before, &after);
        assert_eq!(
            diff.set,
            vec![
                ("B".to_string(), b"changed".to_vec()),
                ("D".to_string(), b"4".to_vec())
            ]
        );
        assert_eq!(diff.unset, vec!["C".to_string()]);
    }

    #[test]
    fn push_sh_escaped_handles_single_quotes() {
        let mut out = String::new();
        push_sh_escaped(&mut out, "it's here");
        assert_eq!(out, "'it'\\''s here'");
    }

    #[test]
    fn parse_denv_state_supports_spaces_in_dir() {
        let parsed = parse_denv_state("1 2 /tmp/path with spaces").unwrap();
        assert_eq!(parsed.0, 1);
        assert_eq!(parsed.1, 2);
        assert_eq!(parsed.2, "/tmp/path with spaces");
    }

    #[test]
    fn first_line_uses_bash_detects_substring() {
        assert!(first_line_uses(b"#!/usr/bin/env bash", b"bash"));
        assert!(first_line_uses(b"bash -eu", b"bash"));
        assert!(!first_line_uses(b"#!/bin/sh", b"bash"));
    }

    fn baseline() -> EnvMap {
        let mut env = EnvMap::new();
        env.insert(
            b"PATH".to_vec(),
            std::env::var_os("PATH").unwrap_or_default().into_vec(),
        );
        env
    }

    #[test]
    fn sh_envrc_is_evaluated_by_sh() {
        let tmp = temp("xshi_denv_sh_eval");
        let envrc = tmp.path().join(".envrc");
        std::fs::write(&envrc, "#!/bin/sh\nexport DENV_SH_TEST=ok\n").unwrap();
        let session = Session::for_test();
        let diff = eval_env(&session, tmp.path(), &envrc, &[], &baseline()).unwrap();
        assert!(
            diff.set
                .iter()
                .any(|(key, value)| key == "DENV_SH_TEST" && value == b"ok")
        );
    }

    #[test]
    fn bash_envrc_sees_only_the_explicit_environment() {
        if Command::new("bash").arg("-c").arg(":").status().is_err() {
            return;
        }
        let tmp = temp("xshi_denv_bash_env");
        let envrc = tmp.path().join(".envrc");
        std::fs::write(
            &envrc,
            "#!/usr/bin/env bash\nif [ -n \"${DENV_AMBIENT_ONLY+x}\" ]; then export DENV_LEAK=1; fi\n",
        )
        .unwrap();
        let session = Session::for_test();
        let diff = eval_env(&session, tmp.path(), &envrc, &[], &baseline()).unwrap();
        assert!(!diff.set.iter().any(|(key, _)| key == "DENV_LEAK"));
    }

    #[test]
    fn failing_envrc_reports_a_failed_evaluation_and_changes_nothing() {
        let tmp = temp("xshi_denv_fail");
        let envrc = tmp.path().join(".envrc");
        std::fs::write(
            &envrc,
            "#!/bin/sh\nexport DENV_TMP_SHOULD_ROLLBACK=1\nfalse\n",
        )
        .unwrap();
        let session = Session::for_test();
        let err = match eval_env(&session, tmp.path(), &envrc, &[], &baseline()) {
            Ok(_) => panic!("expected the failing .envrc to be rejected"),
            Err(err) => err,
        };
        assert_eq!(err, ".envrc evaluation failed");
        assert!(session.var(b"DENV_TMP_SHOULD_ROLLBACK").is_none());
    }

    #[test]
    fn state_fast_path_uses_the_cached_directory() {
        let tmp = temp("xshi_denv_fast_path");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let envrc = project.join(".envrc");
        std::fs::write(&envrc, "export OK=1\n").unwrap();
        let mtime = regular_file_mtime(&envrc).unwrap();
        let state = format!("{mtime} 0 {}", project.display());

        assert!(fast_path_ok(&state, &project));
        assert!(fast_path_ok(&state, &project.join("nested")));
        assert!(!fast_path_ok(&state, tmp.path()));

        // Stamp the file into the future rather than sleeping across the
        // one-second timestamp boundary.
        let file = fs::File::options().write(true).open(&envrc).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!fast_path_ok(&state, &project));
    }

    #[test]
    fn trust_is_bound_to_the_file_mtime() {
        let tmp = temp("xshi_denv_trust");
        let mut session = Session::for_test();
        session.denv = DenvState::load(Some(tmp.path().join("denv")));
        let envrc = tmp.path().join(".envrc");
        std::fs::write(&envrc, "export A=1\n").unwrap();
        assert!(!is_allowed(&session, &envrc));
        allow_envrc(&session, &envrc, &mut Vec::new()).unwrap();
        assert!(is_allowed(&session, &envrc));
        let file = fs::File::options().write(true).open(&envrc).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(7))
            .unwrap();
        assert!(
            !is_allowed(&session, &envrc),
            "editing the file revokes trust"
        );
        deny_envrc(&session, &envrc, &mut Vec::new()).unwrap();
    }

    #[test]
    fn dotenv_loads_and_restores_across_directories() {
        let tmp = temp("xshi_denv_session");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(".env"), "DENV_SESSION_PROBE=loaded\n").unwrap();
        let mut session = Session::for_test();
        session.denv = DenvState::load(Some(tmp.path().join("denv")));
        session.cwd = project.canonicalize().unwrap();

        let mut stderr = Vec::new();
        after_cwd_change(&mut session, &mut stderr);
        assert_eq!(session.var(b"DENV_SESSION_PROBE"), Some(&b"loaded"[..]));
        assert!(!session.denv_dirty());
        assert!(String::from_utf8_lossy(&stderr).contains("denv: loading .env"));

        session.cwd = tmp.path().canonicalize().unwrap();
        after_cwd_change(&mut session, &mut stderr);
        assert_eq!(session.var(b"DENV_SESSION_PROBE"), None);
    }

    #[test]
    fn blocked_envrc_marks_the_session_dirty_until_allowed() {
        let tmp = temp("xshi_denv_blocked");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join(".envrc"),
            "#!/bin/sh\nexport DENV_ALLOWED=yes\n",
        )
        .unwrap();
        let mut session = Session::for_test();
        session.denv = DenvState::load(Some(tmp.path().join("denv")));
        session.cwd = project.canonicalize().unwrap();

        let mut stderr = Vec::new();
        after_cwd_change(&mut session, &mut stderr);
        assert!(session.denv_dirty());
        assert!(String::from_utf8_lossy(&stderr).contains("is blocked. Run `denv allow`"));
        assert_eq!(session.var(b"DENV_ALLOWED"), None);

        let status = run_command(&mut session, &DenvCommand::Allow, &mut stderr);
        assert_eq!(status, 0);
        assert!(!session.denv_dirty());
        assert_eq!(session.var(b"DENV_ALLOWED"), Some(&b"yes"[..]));
    }
}
