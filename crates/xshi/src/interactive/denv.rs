//! Automatic `.envrc` / `.env` loading, ported from ish's `src/denv.rs`.
//!
//! Both files require trust: `denv allow` records the canonical path and the
//! SHA-256 of the file's content under `~/.local/share/xshi/denv/allow`, so
//! any edit revokes it. A trusted `.envrc` whose first line mentions `bash`
//! runs under `bash`, one that mentions `xsh` runs as XSH source, and anything
//! else runs under `sh`; the result is diffed against the environment the
//! shell selected. `.env` entries apply on top, as data.
//!
//! State lives in exported variables so scripts and the prompt can see it:
//! `__DENV_DIR`, `__DENV_DIRTY`, and `__DENV_STATE` (`ENVRC_MTIME DOTENV_MTIME
//! DIR`). The variables a load changed are also written to an active record
//! beside `allow/`, so a shell that exits normally removes a file it created.

use super::session::Session;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use xsh::diagnostic::DiagnosticRenderer;
use xsh::execution::evaluator::Evaluator;
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceMap;
use xsh::frontend::syntax::parser::Parser;

const ENVRC_SOURCE_VAR: &str = "__DENV_ENVRC_SOURCE";
const ENVRC_EVAL_TIMEOUT: Duration = Duration::from_secs(60);
const ACTIVE_FORMAT_HEADER: &str = "DENV-ACTIVE 2";
const BEFORE_SNAPSHOT_FD: RawFd = 3;
const AFTER_SNAPSHOT_FD: RawFd = 4;
const SNAPSHOT_SOURCE_FD_MIN: RawFd = AFTER_SNAPSHOT_FD + 1;
const REPORT_FD_MIN: RawFd = 10;

const BASH_STDLIB: &str = include_str!("denv_stdlib.sh");

type EnvMap = BTreeMap<Vec<u8>, Vec<u8>>;

/// Per-shell denv bookkeeping.
#[derive(Clone, Debug)]
pub(super) struct DenvState {
    /// Directory holding `allow/` and this shell's active record.
    data_dir: Option<PathBuf>,
    /// Name component that identifies this shell's active record.
    record_id: Option<String>,
    /// What loading the current directory changed, for restoring on leave.
    active: Option<ActiveState>,
    /// Whether the active record has been read from disk.
    loaded: bool,
    /// Owns the on-disk active record; dropping the last clone removes it.
    record: Option<Arc<ActiveRecord>>,
}

impl Default for DenvState {
    fn default() -> Self {
        Self {
            data_dir: None,
            record_id: None,
            active: None,
            loaded: true,
            record: None,
        }
    }
}

#[derive(Clone, Debug)]
struct ActiveState {
    dir: PathBuf,
    envrc_mtime: u64,
    dotenv_mtime: u64,
    prev: Vec<PrevVar>,
}

#[derive(Clone, Debug)]
enum PrevVar {
    Restore(String, Vec<u8>, bool),
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
            record_id: Some(new_record_id()),
            active: None,
            loaded: false,
            record: None,
        }
    }
}

impl Session {
    /// Whether an `.envrc` or `.env` in the current tree is waiting for
    /// `denv allow`.
    pub(super) fn denv_dirty(&self) -> bool {
        self.env.get(b"__DENV_DIRTY".as_slice()).map(Vec::as_slice) == Some(b"1")
    }
}

/// Called once at shell startup.
pub(super) fn startup(session: &mut Session, stderr: &mut dyn Write) {
    init(session);
    if let Err(err) = refresh(session, false, stderr) {
        writeln!(stderr, "denv: {err}").ok();
    }
}

/// Called after any directory change.
pub(super) fn after_cwd_change(session: &mut Session, stderr: &mut dyn Write) {
    if let Err(err) = refresh(session, false, stderr) {
        writeln!(stderr, "denv: {err}").ok();
    }
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
        DenvCommand::Reload => refresh(session, true, stderr),
    };
    match result {
        Ok(()) => 0,
        Err(err) => {
            writeln!(stderr, "denv: {err}").ok();
            1
        }
    }
}

/// Exports the variables that identify this shell's denv to `.envrc` scripts.
fn init(session: &mut Session) {
    let record = session
        .denv
        .record_id
        .clone()
        .unwrap_or_else(|| std::process::id().to_string());
    session.export_var(b"__DENV_PID", record.as_bytes());
    session.export_var(b"__DENV_SHELL", b"bash");
}

enum RecordUpdate {
    Keep,
    Clear,
    Save(ActiveState),
}

struct Transition {
    changes: Vec<EnvChange>,
    record: RecordUpdate,
}

#[derive(Clone, Debug)]
enum EnvChange {
    Set(String, Vec<u8>),
    Unset(String),
    Unexport(String),
}

fn apply_changes(session: &mut Session, changes: &[EnvChange]) {
    for change in changes {
        match change {
            EnvChange::Set(name, value) => session.export_var(name.as_bytes(), value),
            EnvChange::Unset(name) => session.unset_var(name.as_bytes()),
            EnvChange::Unexport(name) => {
                if let Some(value) = session.env.remove(name.as_bytes()) {
                    session.vars.insert(name.as_bytes().to_vec(), value);
                }
            }
        }
    }
}

/// Applies the changes a candidate environment can carry without the shell.
fn apply_to_environment(env: &mut EnvMap, changes: &[EnvChange]) {
    for change in changes {
        match change {
            EnvChange::Set(name, value) => {
                env.insert(name.as_bytes().to_vec(), value.clone());
            }
            EnvChange::Unset(name) | EnvChange::Unexport(name) => {
                env.remove(name.as_bytes());
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

fn refresh(session: &mut Session, force: bool, stderr: &mut dyn Write) -> Result<(), String> {
    let data_dir = data_dir(session)?;
    load_active_once(session, &data_dir, force, stderr)?;
    let allow_dir = data_dir.join("allow");
    let active = session.denv.active.clone();
    let transition = refresh_inner(force, session, active, &allow_dir, stderr)?;

    if let RecordUpdate::Save(state) = &transition.record {
        save_active(session, &data_dir, state)?;
    }
    apply_changes(session, &transition.changes);
    match transition.record {
        RecordUpdate::Keep => {}
        RecordUpdate::Clear => {
            session.denv.active = None;
            session.denv.record = None;
        }
        RecordUpdate::Save(state) => session.denv.active = Some(state),
    }
    print_summary(&transition.changes, stderr);
    Ok(())
}

fn refresh_inner(
    force: bool,
    session: &Session,
    active: Option<ActiveState>,
    allow_dir: &Path,
    stderr: &mut dyn Write,
) -> Result<Transition, String> {
    let found = find_env_files(&session.cwd);

    let envrc_file = found
        .as_ref()
        .and_then(|found| found.envrc.as_ref())
        .map(|(path, _)| read_trusted_candidate(path))
        .transpose()?;
    let dotenv_file = found
        .as_ref()
        .and_then(|found| found.dotenv.as_ref())
        .map(|(path, _)| read_trusted_candidate(path))
        .transpose()?;
    let blocked = [&envrc_file, &dotenv_file]
        .into_iter()
        .flatten()
        .find(|file| !is_allowed_in(allow_dir, file));
    let files_allowed = blocked.is_none();
    let dotenv_entries = if files_allowed {
        match &dotenv_file {
            Some(file) => load_dotenv_entries(&file.bytes, stderr)?,
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    if !force && let Some(found) = &found {
        if files_allowed && state_var_matches(found, session) {
            return Ok(Transition {
                changes: Vec::new(),
                record: RecordUpdate::Keep,
            });
        }
        if files_allowed
            && let Some(active) = &active
            && active.envrc_mtime == found.envrc.as_ref().map_or(0, |(_, mtime)| *mtime)
            && active.dotenv_mtime == found.dotenv.as_ref().map_or(0, |(_, mtime)| *mtime)
            && paths_match(&active.dir, &found.dir)
        {
            return Ok(Transition {
                changes: Vec::new(),
                record: RecordUpdate::Keep,
            });
        }
    }

    let mut changes = Vec::new();
    if let Some(state) = &active {
        apply_restore(&state.prev, &mut changes);
    }

    let mut working = session.env.clone();
    apply_to_environment(&mut working, &changes);
    let env_pairs = env_pairs(&working);
    let Some(found) = found else {
        clear_runtime_state(&mut changes);
        return Ok(Transition {
            changes,
            record: RecordUpdate::Clear,
        });
    };

    let envrc_mtime = found.envrc.as_ref().map_or(0, |(_, mtime)| *mtime);
    let dotenv_mtime = found.dotenv.as_ref().map_or(0, |(_, mtime)| *mtime);
    let dir = canonicalize_fallback(&found.dir);

    if let Some(blocked) = blocked {
        let path = &blocked.path;
        writeln!(
            stderr,
            "denv: {path} is blocked. Run `denv allow` to trust it.",
            path = path.display()
        )
        .ok();
        set_change(&mut changes, "__DENV_DIR", dir.as_os_str().as_bytes());
        set_change(&mut changes, "__DENV_DIRTY", "1");
        unset_change(&mut changes, "__DENV_STATE");
        return Ok(Transition {
            changes,
            record: RecordUpdate::Clear,
        });
    }

    if found.envrc.is_some() {
        writeln!(stderr, "denv: loading .envrc").ok();
    }
    if found.dotenv.is_some() {
        writeln!(stderr, "denv: loading .env").ok();
    }

    let diff = match &envrc_file {
        Some(file) => match eval_env(
            session,
            &dir,
            Some(&file.bytes),
            &dotenv_entries,
            &env_pairs,
            ENVRC_EVAL_TIMEOUT,
            stderr,
        ) {
            Ok(diff) => diff,
            Err(err) => {
                writeln!(stderr, "denv: {err}").ok();
                set_change(&mut changes, "__DENV_DIR", dir.as_os_str().as_bytes());
                set_change(&mut changes, "__DENV_DIRTY", "1");
                unset_change(&mut changes, "__DENV_STATE");
                return Ok(Transition {
                    changes,
                    record: RecordUpdate::Clear,
                });
            }
        },
        None => diff_dotenv_only(&dotenv_entries, &env_pairs),
    };

    let prev = capture_prev(&diff, &working, session);
    let state = ActiveState {
        dir: dir.clone(),
        envrc_mtime,
        dotenv_mtime,
        prev,
    };
    apply_diff(&diff, &mut changes);
    set_change(&mut changes, "__DENV_DIR", dir.as_os_str().as_bytes());
    unset_change(&mut changes, "__DENV_DIRTY");
    set_change(
        &mut changes,
        "__DENV_STATE",
        format!("{envrc_mtime} {dotenv_mtime} {}", found.dir.display()),
    );
    Ok(Transition {
        changes,
        record: RecordUpdate::Save(state),
    })
}

fn allow_current_dir(session: &mut Session, stderr: &mut dyn Write) -> Result<(), String> {
    let found = find_env_files(&session.cwd).ok_or("no .envrc or .env found")?;
    let allow_dir = data_dir(session)?.join("allow");
    for path in found
        .envrc
        .iter()
        .chain(found.dotenv.iter())
        .map(|(path, _)| path)
    {
        let file = read_trusted_candidate(path)?;
        allow_file_in(&allow_dir, &file)?;
        writeln!(stderr, "denv: allowed {}", file.path.display()).ok();
    }
    refresh(session, true, stderr)
}

fn deny_current_dir(session: &mut Session, stderr: &mut dyn Write) -> Result<(), String> {
    let found = find_env_files(&session.cwd).ok_or("no .envrc or .env found")?;
    let data_dir = data_dir(session)?;
    for (path, _) in found.envrc.iter().chain(found.dotenv.iter()) {
        deny_file(&data_dir, &canonicalize_fallback(path), stderr)?;
    }
    refresh(session, true, stderr)
}

fn data_dir(session: &Session) -> Result<PathBuf, String> {
    if let Some(path) = session.var(b"DENV_DATA_DIR") {
        return Ok(session
            .cwd
            .join(PathBuf::from(OsString::from_vec(path.to_vec()))));
    }
    session
        .denv
        .data_dir
        .clone()
        .ok_or_else(|| "HOME and DENV_DATA_DIR are unset".to_string())
}

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

fn paths_match(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

fn canonicalize_fallback(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn parse_denv_state(state: &str) -> Option<(u64, u64, &str)> {
    let (envrc, rest) = state.split_once(' ')?;
    let (dotenv, dir) = rest.split_once(' ')?;
    Some((envrc.parse().ok()?, dotenv.parse().ok()?, dir))
}

fn state_var_matches(found: &EnvFiles, session: &Session) -> bool {
    let envrc_mtime = found.envrc.as_ref().map_or(0, |(_, mtime)| *mtime);
    let dotenv_mtime = found.dotenv.as_ref().map_or(0, |(_, mtime)| *mtime);
    let Some(state) = session.var(b"__DENV_STATE") else {
        return false;
    };
    let state = String::from_utf8_lossy(state);
    let Some((state_envrc, state_dotenv, state_dir)) = parse_denv_state(&state) else {
        return false;
    };
    state_envrc == envrc_mtime
        && state_dotenv == dotenv_mtime
        && (found.dir.to_str().is_some_and(|dir| state_dir == dir)
            || found
                .dir
                .canonicalize()
                .is_ok_and(|dir| dir.to_str().is_some_and(|dir| state_dir == dir)))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let bytes = value.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err("invalid hex value".to_string());
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let high = hex_digit(pair[0]).ok_or("invalid hex value")?;
        let low = hex_digit(pair[1]).ok_or("invalid hex value")?;
        out.push((high << 4) | low);
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    hex_encode(&digest[..])
}

fn trust_key(path: &Path) -> String {
    format!("sha256-{}", sha256_hex(path.as_os_str().as_bytes()))
}

fn legacy_trust_key(path: &Path) -> String {
    hex_encode(path.as_os_str().as_bytes())
}

struct TrustCandidate {
    path: PathBuf,
    bytes: Vec<u8>,
    hash: String,
}

fn read_trusted_candidate(path: &Path) -> Result<TrustCandidate, String> {
    let path = path
        .canonicalize()
        .map_err(|err| format!("resolve {}: {err}", path.display()))?;
    let bytes = fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let hash = sha256_hex(&bytes);
    Ok(TrustCandidate { path, bytes, hash })
}

fn is_allowed_in(allow_dir: &Path, file: &TrustCandidate) -> bool {
    let stored = match fs::read_to_string(allow_dir.join(trust_key(&file.path))) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::read_to_string(allow_dir.join(legacy_trust_key(&file.path)))
        }
        result => result,
    };
    stored.is_ok_and(|stored| stored.trim() == file.hash)
}

fn allow_file_in(dir: &Path, file: &TrustCandidate) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|err| format!("failed to create allow dir: {err}"))?;
    fs::write(dir.join(trust_key(&file.path)), &file.hash)
        .map_err(|err| format!("failed to write trust file: {err}"))
}

fn deny_file(data_dir: &Path, path: &Path, stderr: &mut dyn Write) -> Result<(), String> {
    let allow = data_dir.join("allow");
    let mut removed = false;
    for (key, legacy) in [(legacy_trust_key(path), true), (trust_key(path), false)] {
        match fs::remove_file(allow.join(key)) {
            Ok(()) => removed = true,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) if legacy && err.raw_os_error() == Some(libc::ENAMETOOLONG) => {}
            Err(err) => return Err(format!("failed to remove trust file: {err}")),
        }
    }
    if removed {
        writeln!(stderr, "denv: denied {}", path.display()).ok();
    } else {
        writeln!(stderr, "denv: {} not currently allowed", path.display()).ok();
    }
    Ok(())
}

fn is_reserved_dotenv_key(key: &str) -> bool {
    key == "DENV_DATA_DIR" || key.starts_with("__DENV_")
}

fn is_valid_env_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn load_dotenv_entries(
    bytes: &[u8],
    stderr: &mut dyn Write,
) -> Result<Vec<(String, String)>, String> {
    let content = std::str::from_utf8(bytes).map_err(|err| format!("read .env: {err}"))?;
    let mut entries = Vec::new();
    for (key, value) in parse_dotenv(content) {
        if !is_valid_env_key(key) {
            writeln!(stderr, "denv: .env: skipping invalid variable name {key:?}").ok();
        } else if is_reserved_dotenv_key(key) {
            writeln!(stderr, "denv: .env: refusing to set {key}").ok();
        } else if value.contains('\0') {
            writeln!(stderr, "denv: .env: skipping {key}: value contains NUL").ok();
        } else {
            entries.push((key.to_string(), value.into_owned()));
        }
    }
    Ok(entries)
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

fn diff_dotenv_only(
    dotenv_entries: &[(String, String)],
    environment: &[(OsString, OsString)],
) -> EnvDiff {
    let mut set = Vec::new();
    for (key, value) in dotenv_entries {
        if environment
            .iter()
            .find(|(name, _)| name == OsStr::new(key))
            .is_none_or(|(_, current)| current != OsStr::new(value))
        {
            set.push((key.clone(), value.clone().into_bytes()));
        }
    }
    EnvDiff {
        set,
        unset: Vec::new(),
    }
}

fn is_ignored_env_key(key: &str) -> bool {
    matches!(key, "_" | "SHLVL" | "PWD" | "OLDPWD")
        || key.starts_with(ENVRC_SOURCE_VAR)
        || key.starts_with("__DENV_DOT_")
}

fn filtered_env_from_snapshot(entries: &[(OsString, OsString)]) -> Vec<(String, Vec<u8>)> {
    let mut filtered: Vec<_> = entries
        .iter()
        .filter_map(|(key, value)| {
            let key = key.to_str()?;
            (is_valid_env_key(key) && !is_ignored_env_key(key))
                .then(|| (key.to_string(), value.as_bytes().to_vec()))
        })
        .collect();
    filtered.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    filtered
}

fn parse_env_null(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let entries: Vec<_> = data
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let separator = entry.iter().position(|byte| *byte == b'=')?;
            Some((
                OsStr::from_bytes(&entry[..separator]).to_owned(),
                OsStr::from_bytes(&entry[separator + 1..]).to_owned(),
            ))
        })
        .collect();
    filtered_env_from_snapshot(&entries)
}

fn diff_sorted_env(before: &[(String, Vec<u8>)], after: &[(String, Vec<u8>)]) -> EnvDiff {
    let (mut before_index, mut after_index) = (0, 0);
    let mut set = Vec::new();
    let mut unset = Vec::new();
    while before_index < before.len() && after_index < after.len() {
        match before[before_index].0.cmp(&after[after_index].0) {
            Ordering::Less => {
                unset.push(before[before_index].0.clone());
                before_index += 1;
            }
            Ordering::Greater => {
                set.push((after[after_index].0.clone(), after[after_index].1.clone()));
                after_index += 1;
            }
            Ordering::Equal => {
                if before[before_index].1 != after[after_index].1 {
                    set.push((after[after_index].0.clone(), after[after_index].1.clone()));
                }
                before_index += 1;
                after_index += 1;
            }
        }
    }
    for (key, _) in &before[before_index..] {
        unset.push(key.clone());
    }
    for (key, value) in &after[after_index..] {
        set.push((key.clone(), value.clone()));
    }
    EnvDiff { set, unset }
}

fn envrc_interpreter(source: &[u8]) -> EnvRcInterpreter {
    let head = &source[..source.len().min(256)];
    let first_line = head.split(|&byte| byte == b'\n').next().unwrap_or_default();
    let mentions = |word: &[u8]| first_line.windows(word.len()).any(|window| window == word);
    if mentions(b"bash") {
        EnvRcInterpreter::Bash
    } else if mentions(b"xsh") {
        EnvRcInterpreter::Xsh
    } else {
        EnvRcInterpreter::Sh
    }
}

fn env_pairs(env: &EnvMap) -> Vec<(OsString, OsString)> {
    env.iter()
        .map(|(name, value)| {
            (
                OsString::from_vec(name.clone()),
                OsString::from_vec(value.clone()),
            )
        })
        .collect()
}

fn eval_env(
    session: &Session,
    dir: &Path,
    envrc_source: Option<&[u8]>,
    dotenv_entries: &[(String, String)],
    env_pairs: &[(OsString, OsString)],
    timeout: Duration,
    stderr: &mut dyn Write,
) -> Result<EnvDiff, String> {
    let Some(source) = envrc_source else {
        return Ok(diff_dotenv_only(dotenv_entries, env_pairs));
    };
    match envrc_interpreter(source) {
        EnvRcInterpreter::Bash => eval_env_shell(
            "bash",
            dir,
            source,
            dotenv_entries,
            env_pairs,
            timeout,
            stderr,
        ),
        EnvRcInterpreter::Sh => eval_env_shell(
            "sh",
            dir,
            source,
            dotenv_entries,
            env_pairs,
            timeout,
            stderr,
        ),
        EnvRcInterpreter::Xsh => {
            let baseline: EnvMap = env_pairs
                .iter()
                .map(|(name, value)| (name.as_bytes().to_vec(), value.as_bytes().to_vec()))
                .collect();
            eval_env_xsh(session, dir, source, dotenv_entries, &baseline)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RetiredHelper {
    SourceEnv,
    SourceEnvIfExists,
    SourceUp,
    SourceUpIfExists,
    Dotenv,
    DotenvIfExists,
}

impl RetiredHelper {
    const ALL: [Self; 6] = [
        Self::SourceEnv,
        Self::SourceEnvIfExists,
        Self::SourceUp,
        Self::SourceUpIfExists,
        Self::Dotenv,
        Self::DotenvIfExists,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::SourceEnv => "source_env",
            Self::SourceEnvIfExists => "source_env_if_exists",
            Self::SourceUp => "source_up",
            Self::SourceUpIfExists => "source_up_if_exists",
            Self::Dotenv => "dotenv",
            Self::DotenvIfExists => "dotenv_if_exists",
        }
    }
}

impl std::fmt::Display for RetiredHelper {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} was removed; use the automatically discovered .env or put setup in the approved .envrc",
            self.name()
        )
    }
}

struct FailureChannel {
    read: OwnedFd,
    write: OwnedFd,
    backup: OwnedFd,
}

#[derive(Default)]
enum Completion {
    #[default]
    Pending,
    Observed,
}

enum ChannelState {
    Open,
    Complete,
}

impl FailureChannel {
    fn new() -> Result<Self, String> {
        let (read, write) = pipe_cloexec().map_err(|err| format!("create failure pipe: {err}"))?;
        rustix::fs::fcntl_setfl(&read, rustix::fs::OFlags::NONBLOCK)
            .map_err(|err| format!("set failure pipe nonblocking: {err}"))?;
        let reporter = rustix::io::fcntl_dupfd_cloexec(&write, REPORT_FD_MIN)
            .map_err(|err| format!("dup failure pipe: {err}"))?;
        let backup = rustix::io::fcntl_dupfd_cloexec(&write, REPORT_FD_MIN)
            .map_err(|err| format!("dup failure pipe: {err}"))?;
        Ok(Self {
            read,
            write: reporter,
            backup,
        })
    }

    fn reporters(&self) -> [RawFd; 2] {
        [self.write.as_raw_fd(), self.backup.as_raw_fd()]
    }

    fn install_stubs(script: &mut String, reporters: [RawFd; 2]) {
        for (index, helper) in RetiredHelper::ALL.into_iter().enumerate() {
            let record = char::from(b'A' + index as u8);
            writeln!(script, "{}() {{", helper.name()).unwrap();
            for fd in reporters {
                writeln!(script, "  printf '%s' '{record}' >&{fd} 2>/dev/null || :").unwrap();
            }
            writeln!(script, "  printf '%s\\n' 'denv: {helper}' >&2").unwrap();
            writeln!(script, "  return 2").unwrap();
            writeln!(script, "}}").unwrap();
        }
    }

    fn read_state(read: &OwnedFd, completion: &mut Completion) -> Result<ChannelState, String> {
        let mut record = [0_u8; 1];
        loop {
            match rustix::io::read(read, &mut record) {
                Ok(0) => {
                    return match completion {
                        Completion::Observed => Ok(ChannelState::Complete),
                        Completion::Pending => {
                            Err("incomplete .envrc failure channel".to_string())
                        }
                    };
                }
                Ok(_) if record[0] == b'Z' && matches!(completion, Completion::Pending) => {
                    *completion = Completion::Observed;
                }
                Ok(_) => {
                    return Err(
                        match record[0]
                            .checked_sub(b'A')
                            .and_then(|index| RetiredHelper::ALL.get(usize::from(index)))
                            .copied()
                        {
                            Some(helper) => format!(".envrc rejected: {helper}"),
                            None => format!("invalid .envrc failure record: {}", record[0]),
                        },
                    );
                }
                Err(rustix::io::Errno::INTR) => continue,
                Err(rustix::io::Errno::AGAIN) => return Ok(ChannelState::Open),
                Err(err) => return Err(format!(".envrc failure channel: {err}")),
            }
        }
    }
}

fn pipe_cloexec() -> io::Result<(OwnedFd, OwnedFd)> {
    let (read, write) = rustix::pipe::pipe()?;
    rustix::io::fcntl_setfd(&read, rustix::io::FdFlags::CLOEXEC)?;
    rustix::io::fcntl_setfd(&write, rustix::io::FdFlags::CLOEXEC)?;
    Ok((read, write))
}

fn duplicate_snapshot_writers(
    before_write: &OwnedFd,
    after_write: &OwnedFd,
) -> Result<(OwnedFd, OwnedFd), String> {
    let before = rustix::io::fcntl_dupfd_cloexec(before_write, SNAPSHOT_SOURCE_FD_MIN)
        .map_err(|err| format!("dup before pipe: {err}"))?;
    let after = rustix::io::fcntl_dupfd_cloexec(after_write, SNAPSHOT_SOURCE_FD_MIN)
        .map_err(|err| format!("dup after pipe: {err}"))?;
    Ok((before, after))
}

fn remap_snapshot_writers(before: &OwnedFd, after: &OwnedFd) -> io::Result<()> {
    dup_to(before, BEFORE_SNAPSHOT_FD)?;
    dup_to(after, AFTER_SNAPSHOT_FD)
}

fn dup_to(source: &OwnedFd, target: RawFd) -> io::Result<()> {
    // SAFETY: dup2 accepts a closed target and replaces it with the owned
    // source descriptor.
    let result = unsafe { libc::dup2(source.as_raw_fd(), target) };
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn restore_child_signals() -> io::Result<()> {
    // SAFETY: the pre-exec hook only calls async-signal-safe signal(2).
    unsafe {
        for signal in [
            libc::SIGHUP,
            libc::SIGINT,
            libc::SIGQUIT,
            libc::SIGPIPE,
            libc::SIGTERM,
            libc::SIGTSTP,
            libc::SIGTTIN,
            libc::SIGTTOU,
            libc::SIGUSR1,
            libc::SIGUSR2,
            libc::SIGALRM,
            libc::SIGXCPU,
            libc::SIGXFSZ,
        ] {
            if libc::signal(signal, libc::SIG_DFL) == libc::SIG_ERR {
                return Err(io::Error::last_os_error());
            }
        }
    }
    Ok(())
}

fn drain_env_pipe(file: &mut fs::File, data: &mut Vec<u8>, label: &str) -> Result<bool, String> {
    let mut buffer = [0_u8; 8192];
    match file.read(&mut buffer) {
        Ok(0) => Ok(false),
        Ok(count) => {
            data.extend_from_slice(&buffer[..count]);
            Ok(true)
        }
        Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::Interrupted => Ok(true),
        Err(err) => Err(format!("read {label} env: {err}")),
    }
}

/// Reads a rejected candidate's stderr until it closes or `limit` expires, so
/// the notice its helper printed precedes the rejection line.
fn drain_rejected_stderr(file: &mut fs::File, data: &mut Vec<u8>, limit: Duration) {
    let deadline = Instant::now() + limit;
    while drain_env_pipe(file, data, "child stderr").unwrap_or(false) {
        if Instant::now() >= deadline {
            return;
        }
    }
}

fn stop_child_group(child: &mut Child, group: rustix::process::Pid, leader_exited: bool) {
    let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    if !leader_exited {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn eval_env_shell(
    interpreter: &str,
    dir: &Path,
    source: &[u8],
    dotenv_entries: &[(String, String)],
    env_pairs: &[(OsString, OsString)],
    timeout: Duration,
    stderr: &mut dyn Write,
) -> Result<EnvDiff, String> {
    if source.contains(&0) {
        return Err(".envrc contains a NUL byte".to_string());
    }
    let failure = FailureChannel::new()?;
    let (before_read_fd, before_write) =
        pipe_cloexec().map_err(|err| format!("create before pipe: {err}"))?;
    let (after_read_fd, after_write) =
        pipe_cloexec().map_err(|err| format!("create after pipe: {err}"))?;
    let (before_source, after_source) = duplicate_snapshot_writers(&before_write, &after_write)?;
    rustix::fs::fcntl_setfl(&before_read_fd, rustix::fs::OFlags::NONBLOCK)
        .map_err(|err| format!("set before pipe nonblocking: {err}"))?;
    rustix::fs::fcntl_setfl(&after_read_fd, rustix::fs::OFlags::NONBLOCK)
        .map_err(|err| format!("set after pipe nonblocking: {err}"))?;
    let mut before_read = fs::File::from(before_read_fd);
    let mut after_read = fs::File::from(after_read_fd);

    let mut script = String::with_capacity(BASH_STDLIB.len() + 512);
    script.push_str(BASH_STDLIB);
    script.push('\n');
    FailureChannel::install_stubs(&mut script, failure.reporters());
    script.push_str("__denv_src=$");
    script.push_str(ENVRC_SOURCE_VAR);
    script.push_str("\nunset ");
    script.push_str(ENVRC_SOURCE_VAR);
    script.push_str("\nenv -0 >&3\neval \"$__denv_src\"\nunset __denv_src\nenv -0 >&4\n");
    writeln!(script, "printf '%s' Z >&{}", failure.reporters()[0]).unwrap();

    let (child_stderr_read, child_stderr_write) =
        pipe_cloexec().map_err(|err| format!("create stderr pipe: {err}"))?;
    rustix::fs::fcntl_setfl(&child_stderr_read, rustix::fs::OFlags::NONBLOCK)
        .map_err(|err| format!("set stderr pipe nonblocking: {err}"))?;
    let mut child_stderr_read = fs::File::from(child_stderr_read);
    let mut child_stderr_data = Vec::new();
    let mut child_stderr_open = true;
    let stdout_dup = child_stderr_write
        .try_clone()
        .map_err(|err| format!("dup child stderr: {err}"))?;
    let mut command = Command::new(interpreter);
    command.process_group(0);
    command.env_clear();
    for (name, value) in env_pairs {
        command.env(name, value);
    }
    command.env(ENVRC_SOURCE_VAR, OsStr::from_bytes(source));
    let FailureChannel {
        read: failure_read,
        write: failure_write,
        backup: failure_backup,
    } = failure;
    // SAFETY: the hook runs between fork and exec and only resets signal
    // dispositions, remaps owned descriptors and clears close-on-exec on the
    // owned reporters.
    unsafe {
        command.pre_exec(move || {
            restore_child_signals()?;
            remap_snapshot_writers(&before_source, &after_source)?;
            rustix::io::fcntl_setfd(&failure_write, rustix::io::FdFlags::empty())?;
            rustix::io::fcntl_setfd(&failure_backup, rustix::io::FdFlags::empty())?;
            Ok(())
        });
    }
    let mut child = command
        .arg("-e")
        .arg("-c")
        .arg(&script)
        .current_dir(dir)
        .stdout(stdout_dup)
        .stderr(child_stderr_write)
        .spawn()
        .map_err(|err| format!("failed to run {interpreter}: {err}"))?;
    drop(command);
    drop(before_write);
    drop(after_write);

    let deadline = Instant::now() + timeout;
    let child_pid = rustix::process::Pid::from_child(&child);
    let process_group = child_pid;
    let mut leader_exited = false;
    let mut before_open = true;
    let mut after_open = true;
    let mut failure_open = true;
    let mut completion = Completion::Pending;
    let mut before_data = Vec::new();
    let mut after_data = Vec::new();
    while !leader_exited || before_open || after_open || failure_open || child_stderr_open {
        match FailureChannel::read_state(&failure_read, &mut completion) {
            Ok(ChannelState::Complete) => failure_open = false,
            Ok(ChannelState::Open) => {}
            Err(err) => {
                stop_child_group(&mut child, process_group, leader_exited);
                drain_rejected_stderr(
                    &mut child_stderr_read,
                    &mut child_stderr_data,
                    Duration::from_millis(50),
                );
                stderr.write_all(&child_stderr_data).ok();
                return Err(err);
            }
        }
        if Instant::now() >= deadline {
            stop_child_group(&mut child, process_group, leader_exited);
            drain_rejected_stderr(
                &mut child_stderr_read,
                &mut child_stderr_data,
                Duration::from_millis(50),
            );
            stderr.write_all(&child_stderr_data).ok();
            return Err(format!(
                ".envrc evaluation timed out after {}s",
                timeout.as_secs_f32()
            ));
        }
        if !leader_exited {
            match rustix::process::waitid(
                rustix::process::WaitId::Pid(child_pid),
                rustix::process::WaitIdOptions::EXITED
                    | rustix::process::WaitIdOptions::NOHANG
                    | rustix::process::WaitIdOptions::NOWAIT,
            ) {
                Ok(Some(_)) => leader_exited = true,
                Ok(None) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(err) => {
                    stop_child_group(&mut child, process_group, false);
                    return Err(format!("wait for {interpreter}: {err}"));
                }
            }
        }

        let mut readers = Vec::with_capacity(4);
        if before_open {
            readers.push(rustix::event::PollFd::from_borrowed_fd(
                before_read.as_fd(),
                rustix::event::PollFlags::IN,
            ));
        }
        if after_open {
            readers.push(rustix::event::PollFd::from_borrowed_fd(
                after_read.as_fd(),
                rustix::event::PollFlags::IN,
            ));
        }
        if failure_open {
            readers.push(rustix::event::PollFd::from_borrowed_fd(
                failure_read.as_fd(),
                rustix::event::PollFlags::IN,
            ));
        }
        if child_stderr_open {
            readers.push(rustix::event::PollFd::from_borrowed_fd(
                child_stderr_read.as_fd(),
                rustix::event::PollFlags::IN,
            ));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let poll_for = remaining.min(Duration::from_millis(10));
        let poll_timeout = rustix::event::Timespec {
            tv_sec: poll_for.as_secs() as i64,
            tv_nsec: poll_for.subsec_nanos() as i64,
        };
        match rustix::event::poll(&mut readers, Some(&poll_timeout)) {
            Err(rustix::io::Errno::INTR) => continue,
            Err(err) => {
                stop_child_group(&mut child, process_group, leader_exited);
                return Err(format!("poll env snapshot pipes: {err}"));
            }
            Ok(_) => {}
        }
        let ready_flags = rustix::event::PollFlags::IN
            | rustix::event::PollFlags::HUP
            | rustix::event::PollFlags::ERR;
        let mut index = 0;
        let before_ready = if before_open {
            let ready = readers[index].revents().intersects(ready_flags);
            index += 1;
            ready
        } else {
            false
        };
        let after_ready = if after_open {
            let ready = readers[index].revents().intersects(ready_flags);
            index += 1;
            ready
        } else {
            false
        };
        let stderr_ready = if child_stderr_open {
            readers[index].revents().intersects(ready_flags)
        } else {
            false
        };
        drop(readers);
        if before_ready {
            match drain_env_pipe(&mut before_read, &mut before_data, "before") {
                Ok(open) => before_open = open,
                Err(err) => {
                    stop_child_group(&mut child, process_group, leader_exited);
                    return Err(err);
                }
            }
        }
        if after_ready {
            match drain_env_pipe(&mut after_read, &mut after_data, "after") {
                Ok(open) => after_open = open,
                Err(err) => {
                    stop_child_group(&mut child, process_group, leader_exited);
                    return Err(err);
                }
            }
        }
        if stderr_ready {
            match drain_env_pipe(&mut child_stderr_read, &mut child_stderr_data, "child stderr") {
                Ok(open) => child_stderr_open = open,
                Err(err) => {
                    stop_child_group(&mut child, process_group, leader_exited);
                    return Err(err);
                }
            }
        }
    }
    let status = child
        .wait()
        .map_err(|err| format!("reap {interpreter}: {err}"))?;
    if !status.success() {
        return Err(".envrc evaluation failed".to_string());
    }
    stderr.write_all(&child_stderr_data).ok();

    let before = parse_env_null(&before_data);
    let mut after = parse_env_null(&after_data);
    for (key, value) in dotenv_entries {
        match after.binary_search_by(|(name, _)| name.as_str().cmp(key)) {
            Ok(index) => after[index].1 = value.clone().into_bytes(),
            Err(index) => after.insert(index, (key.clone(), value.clone().into_bytes())),
        }
    }
    Ok(diff_sorted_env(&before, &after))
}

/// Evaluates an XSH `.envrc`; `.env` entries apply on top of its result.
fn eval_env_xsh(
    session: &Session,
    dir: &Path,
    source: &[u8],
    dotenv_entries: &[(String, String)],
    baseline: &EnvMap,
) -> Result<EnvDiff, String> {
    let text = std::str::from_utf8(source).map_err(|err| format!(".envrc is not UTF-8: {err}"))?;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(dir.join(".envrc").display().to_string(), text.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, text);
    if !parsed.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new()
            .render(&parsed.diagnostics, &sources)
            .trim_end()
            .to_string());
    }
    let checked = Checker::check_arena_interactive(&parsed.arena, text);
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
        for (name, value) in env {
            data.extend_from_slice(name);
            data.push(b'=');
            data.extend_from_slice(value);
            data.push(0);
        }
        data
    };
    Ok(diff_sorted_env(
        &parse_env_null(&flatten(baseline)),
        &parse_env_null(&flatten(&after)),
    ))
}

fn capture_prev(diff: &EnvDiff, environment: &EnvMap, session: &Session) -> Vec<PrevVar> {
    let mut prev = Vec::new();
    for name in diff.set.iter().map(|(name, _)| name).chain(&diff.unset) {
        if let Some(value) = environment.get(name.as_bytes()) {
            prev.push(PrevVar::Restore(name.clone(), value.clone(), true));
        } else if let Some(value) = session.vars.get(name.as_bytes()) {
            prev.push(PrevVar::Restore(name.clone(), value.clone(), false));
        } else {
            prev.push(PrevVar::Unset(name.clone()));
        }
    }
    prev
}

fn apply_restore(prev: &[PrevVar], changes: &mut Vec<EnvChange>) {
    for item in prev {
        match item {
            PrevVar::Restore(name, value, true) => set_change(changes, name, value.clone()),
            PrevVar::Restore(name, value, false) => {
                set_change(changes, name, value.clone());
                changes.push(EnvChange::Unexport(name.clone()));
            }
            PrevVar::Unset(name) => unset_change(changes, name),
        }
    }
}

fn apply_diff(diff: &EnvDiff, changes: &mut Vec<EnvChange>) {
    for (name, value) in &diff.set {
        set_change(changes, name, value.clone());
    }
    for name in &diff.unset {
        unset_change(changes, name);
    }
}

fn clear_runtime_state(changes: &mut Vec<EnvChange>) {
    unset_change(changes, "__DENV_DIR");
    unset_change(changes, "__DENV_DIRTY");
    unset_change(changes, "__DENV_STATE");
}

fn print_summary(changes: &[EnvChange], stderr: &mut dyn Write) {
    let mut line = String::new();
    for change in changes {
        let (sign, name) = match change {
            EnvChange::Set(name, _) => ('+', name),
            EnvChange::Unset(name) => ('-', name),
            EnvChange::Unexport(_) => continue,
        };
        if name.starts_with("__DENV_") {
            continue;
        }
        if line.is_empty() {
            line.push_str("denv: ");
        } else {
            line.push(' ');
        }
        line.push(sign);
        line.push_str(name);
    }
    if !line.is_empty() {
        writeln!(stderr, "{line}").ok();
    }
}

/// Owns a shell's on-disk active record; the last clone of the state removes
/// it, so a shell that exits normally leaves no record behind.
struct ActiveRecord {
    path: PathBuf,
}

impl std::fmt::Debug for ActiveRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ActiveRecord")
            .field("path", &self.path)
            .finish()
    }
}

impl Drop for ActiveRecord {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn current_pid_namespace() -> u64 {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        fs::metadata("/proc/self/ns/pid")
            .map(|meta| meta.ino())
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

fn new_record_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!(
        "{}-{}-{nanos:x}",
        std::process::id(),
        current_pid_namespace()
    )
}

fn record_owner(name: &str) -> Option<(i32, u64)> {
    let rest = name.strip_prefix("active_")?;
    let (pid, rest) = rest.split_once('-')?;
    let (namespace, token) = rest.split_once('-')?;
    if token.is_empty() {
        return None;
    }
    Some((pid.parse().ok()?, namespace.parse().ok()?))
}

fn active_path(data_dir: &Path, record_id: &str) -> PathBuf {
    data_dir.join(format!("active_{record_id}"))
}

fn remove_dead_shell_records(data_dir: &Path, own_namespace: u64) -> Result<(), String> {
    let entries = match fs::read_dir(data_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("failed to list active records: {err}")),
    };
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to list active records: {err}"))?;
        let Some((pid, namespace)) = entry.file_name().to_str().and_then(record_owner) else {
            continue;
        };
        if own_namespace != 0 && namespace != 0 && namespace != own_namespace {
            continue;
        }
        let Some(pid) = rustix::process::Pid::from_raw(pid) else {
            continue;
        };
        if !matches!(
            rustix::process::test_kill_process(pid),
            Err(rustix::io::Errno::SRCH)
        ) {
            continue;
        }
        match fs::remove_file(entry.path()) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "failed to remove stale active record {}: {err}",
                    entry.path().display()
                ));
            }
        }
    }
    Ok(())
}

fn load_active_once(
    session: &mut Session,
    data_dir: &Path,
    force: bool,
    stderr: &mut dyn Write,
) -> Result<(), String> {
    if session.denv.loaded {
        return Ok(());
    }
    session.denv.loaded = true;
    let Some(record_id) = session.denv.record_id.clone() else {
        return Ok(());
    };
    if let Err(err) = remove_dead_shell_records(data_dir, current_pid_namespace()) {
        writeln!(stderr, "denv: {err}").ok();
    }
    let path = active_path(data_dir, &record_id);
    let content = match fs::read(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(format!(
                "failed to read active file {}: {err}",
                path.display()
            ));
        }
    };
    match parse_active_record(&content) {
        Ok(active) => {
            session.denv.active = Some(active);
            Ok(())
        }
        Err(message) if force => {
            let _ = fs::remove_file(&path);
            writeln!(
                stderr,
                "denv: discarded unreadable active record ({message}); variables it saved cannot be restored"
            )
            .ok();
            Ok(())
        }
        Err(message) => Err(message),
    }
}

fn parse_active_record(content: &[u8]) -> Result<ActiveState, String> {
    let content = std::str::from_utf8(content).map_err(|err| format!("invalid active file: {err}"))?;
    let invalid = |err: &str| format!("invalid active file: {err}");
    let mut lines = content.split_terminator('\n');
    if lines.next() != Some(ACTIVE_FORMAT_HEADER) {
        return Err(invalid("missing active format header"));
    }
    let dir = decode_hex(lines.next().ok_or_else(|| invalid("missing project path"))?)?;
    let dir = PathBuf::from(OsStr::from_bytes(&dir));
    if !dir.is_absolute() {
        return Err(invalid("invalid project path"));
    }
    let mtimes = lines.next().ok_or_else(|| invalid("missing timestamps"))?;
    let (envrc_mtime, dotenv_mtime) = mtimes
        .split_once(' ')
        .ok_or_else(|| invalid("invalid timestamps"))?;
    let envrc_mtime = envrc_mtime
        .parse()
        .map_err(|_| invalid("invalid envrc timestamp"))?;
    let dotenv_mtime = dotenv_mtime
        .parse()
        .map_err(|_| invalid("invalid dotenv timestamp"))?;
    let mut prev = Vec::new();
    for line in lines {
        let (kind, rest) = line.split_once(' ').ok_or_else(|| invalid("invalid binding"))?;
        let (name, value) = match kind {
            "U" => (rest, None),
            "E" | "L" => {
                let (name, value) = rest.split_once(' ').ok_or_else(|| invalid("missing value"))?;
                (name, Some(value))
            }
            _ => return Err(invalid("invalid binding kind")),
        };
        let name = String::from_utf8(decode_hex(name)?)
            .map_err(|_| invalid("invalid variable name"))?;
        if name.is_empty() || name.contains('=') {
            return Err(invalid("invalid variable name"));
        }
        match (kind, value) {
            ("U", None) => prev.push(PrevVar::Unset(name)),
            ("E" | "L", Some(value)) => {
                prev.push(PrevVar::Restore(name, decode_hex(value)?, kind == "E"));
            }
            _ => return Err(invalid("invalid binding")),
        }
    }
    Ok(ActiveState {
        dir,
        envrc_mtime,
        dotenv_mtime,
        prev,
    })
}

fn save_active(session: &mut Session, data_dir: &Path, state: &ActiveState) -> Result<(), String> {
    let Some(record_id) = session.denv.record_id.clone() else {
        return Ok(());
    };
    let mut buf = String::new();
    buf.push_str(ACTIVE_FORMAT_HEADER);
    buf.push('\n');
    buf.push_str(&hex_encode(state.dir.as_os_str().as_bytes()));
    buf.push('\n');
    writeln!(buf, "{} {}", state.envrc_mtime, state.dotenv_mtime).unwrap();
    for previous in &state.prev {
        match previous {
            PrevVar::Restore(name, value, exported) => {
                buf.push(if *exported { 'E' } else { 'L' });
                buf.push(' ');
                buf.push_str(&hex_encode(name.as_bytes()));
                buf.push(' ');
                buf.push_str(&hex_encode(value));
                buf.push('\n');
            }
            PrevVar::Unset(name) => {
                buf.push_str("U ");
                buf.push_str(&hex_encode(name.as_bytes()));
                buf.push('\n');
            }
        }
    }
    fs::create_dir_all(data_dir).map_err(|err| format!("failed to create data dir: {err}"))?;
    let path = active_path(data_dir, &record_id);
    // The previous record names the same path, so drop it before replacing it.
    session.denv.record = None;
    let temp = data_dir.join(format!(".active_{record_id}.tmp"));
    let result = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| file.write_all(buf.as_bytes()))
        .and_then(|()| fs::rename(&temp, &path));
    if let Err(err) = result {
        let _ = fs::remove_file(&temp);
        return Err(format!("failed to write active file: {err}"));
    }
    session.denv.record = Some(Arc::new(ActiveRecord { path }));
    Ok(())
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

    fn session_with_data(tmp: &tempfile::TempDir) -> Session {
        let mut session = Session::for_test();
        session.denv = DenvState::load(Some(tmp.path().join("denv")));
        session
    }

    fn pairs(session: &Session) -> Vec<(OsString, OsString)> {
        env_pairs(&session.env)
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
        assert_eq!(parsed, vec![("KEEP".to_string(), b"yes".to_vec())]);
    }

    #[test]
    fn diff_sorted_env_reports_sets_and_unsets() {
        let before = [("A", "1"), ("B", "2"), ("C", "3")]
            .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()));
        let after = [("A", "1"), ("B", "changed"), ("D", "4")]
            .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()));
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
    fn parse_denv_state_supports_spaces_in_dir() {
        let parsed = parse_denv_state("1 2 /tmp/path with spaces").unwrap();
        assert_eq!(parsed.0, 1);
        assert_eq!(parsed.1, 2);
        assert_eq!(parsed.2, "/tmp/path with spaces");
    }

    #[test]
    fn envrc_interpreter_keeps_bash_and_xsh_over_sh() {
        assert!(matches!(
            envrc_interpreter(b"#!/usr/bin/env bash\n"),
            EnvRcInterpreter::Bash
        ));
        assert!(matches!(
            envrc_interpreter(b"#!/usr/bin/env xsh\n"),
            EnvRcInterpreter::Xsh
        ));
        assert!(matches!(
            envrc_interpreter(b"#!/bin/sh\n"),
            EnvRcInterpreter::Sh
        ));
    }

    #[test]
    fn dotenv_rejects_injection_keys_nul_and_reserved_names() {
        let mut stderr = Vec::new();
        let entries = load_dotenv_entries(
            b"A;touch X;B=1\nGOOD=ok\nBAD=a\0b\nDENV_DATA_DIR=/x\n__DENV_STATE=1\n1X=2\n",
            &mut stderr,
        )
        .unwrap();
        assert_eq!(entries, vec![("GOOD".to_string(), "ok".to_string())]);
        assert!(String::from_utf8_lossy(&stderr).contains("invalid variable name"));
        assert!(String::from_utf8_lossy(&stderr).contains("refusing to set DENV_DATA_DIR"));
    }

    #[test]
    fn trust_binds_content_hash_not_mtime() {
        let tmp = temp("xshi_denv_trust");
        let file = tmp.path().join(".env");
        std::fs::write(&file, "A=1\n").unwrap();
        let allow = tmp.path().join("denv").join("allow");

        let before = read_trusted_candidate(&file).unwrap();
        assert!(!is_allowed_in(&allow, &before));
        allow_file_in(&allow, &before).unwrap();
        assert!(is_allowed_in(&allow, &read_trusted_candidate(&file).unwrap()));

        let mtime = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::fs::write(&file, "A=2\n").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
        assert!(!is_allowed_in(&allow, &read_trusted_candidate(&file).unwrap()));
    }

    #[test]
    fn legacy_trust_filename_with_stale_content_is_not_allowed() {
        let tmp = temp("xshi_denv_legacy_trust");
        let data = tmp.path().join("data");
        let allow = data.join("allow");
        fs::create_dir_all(&allow).unwrap();
        let path = tmp.path().join(".env");
        fs::write(&path, "A=1\n").unwrap();
        let file = read_trusted_candidate(&path).unwrap();
        let legacy = legacy_trust_key(&file.path);
        fs::write(allow.join(&legacy), "1700000000").unwrap();
        assert!(!is_allowed_in(&allow, &file));

        let mut stderr = Vec::new();
        deny_file(&data, &file.path, &mut stderr).unwrap();
        assert!(String::from_utf8_lossy(&stderr).contains("denv: denied"));
        assert!(!allow.join(legacy).exists());
    }

    #[test]
    fn retired_helpers_reject_the_candidate_for_bash_and_sh() {
        if Command::new("bash").arg("-c").arg(":").status().is_err() {
            return;
        }
        let tmp = temp("xshi_denv_retired");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let session = Session::for_test();
        for bash in [false, true] {
            for source in [
                "dotenv || true\nexport AFTER=1\n",
                "source_env shared || true\n",
                "source_up || true\n",
            ] {
                let source = if bash {
                    format!("#!/usr/bin/env bash\n{source}")
                } else {
                    source.to_string()
                };
                let err = match eval_env(
                    &session,
                    &project,
                    Some(source.as_bytes()),
                    &[],
                    &pairs(&session),
                    Duration::from_secs(5),
                    &mut Vec::new(),
                ) {
                    Ok(_) => panic!("accepted retired helper in {source}"),
                    Err(err) => err,
                };
                assert!(err.contains("was removed"), "{err}");
            }
        }
    }

    #[test]
    fn candidate_must_complete_its_failure_channel() {
        let tmp = temp("xshi_denv_incomplete");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let session = Session::for_test();
        for source in [
            "export DENV_INCOMPLETE=1; exit 0",
            "export DENV_INCOMPLETE=1; exec true",
        ] {
            let err = match eval_env(
                &session,
                &project,
                Some(source.as_bytes()),
                &[],
                &pairs(&session),
                Duration::from_secs(5),
                &mut Vec::new(),
            ) {
                Ok(_) => panic!("accepted incomplete candidate: {source}"),
                Err(err) => err,
            };
            assert!(err.contains("incomplete"), "{err}");
        }
    }

    #[test]
    fn failing_envrc_reports_a_failed_evaluation() {
        let tmp = temp("xshi_denv_fail");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let session = Session::for_test();
        let err = match eval_env(
            &session,
            &project,
            Some(b"#!/bin/sh\nexport DENV_TMP_SHOULD_ROLLBACK=1\nfalse\n"),
            &[],
            &pairs(&session),
            Duration::from_secs(5),
            &mut Vec::new(),
        ) {
            Ok(_) => panic!("expected the failing .envrc to be rejected"),
            Err(err) => err,
        };
        assert!(
            err == ".envrc evaluation failed" || err.contains("incomplete"),
            "{err}"
        );
        assert!(session.var(b"DENV_TMP_SHOULD_ROLLBACK").is_none());
    }

    #[test]
    fn bash_envrc_sees_only_the_explicit_environment() {
        if Command::new("bash").arg("-c").arg(":").status().is_err() {
            return;
        }
        let tmp = temp("xshi_denv_bash_env");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let session = Session::for_test();
        let source = b"#!/usr/bin/env bash\nif [ -n \"${DENV_AMBIENT_ONLY+x}\" ]; then export DENV_LEAK=1; fi\n";
        let mut environment = pairs(&session);
        environment.retain(|(name, _)| name != "DENV_AMBIENT_ONLY");
        let diff = eval_env(
            &session,
            &project,
            Some(source),
            &[],
            &environment,
            Duration::from_secs(5),
            &mut Vec::new(),
        )
        .unwrap();
        assert!(!diff.set.iter().any(|(name, _)| name == "DENV_LEAK"));
    }

    #[test]
    fn dotenv_values_reach_bash_as_data_not_script() {
        if Command::new("bash").arg("-c").arg(":").status().is_err() {
            return;
        }
        let tmp = temp("xshi_denv_dotenv_data");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let session = Session::for_test();
        let marker = tmp.path().join("marker");
        let value = format!("it's \"q\" $(touch {}) `x`\nline2", marker.display());
        let entries = vec![("DENV_DOTENV_VALUE".to_string(), value.clone())];
        let diff = eval_env(
            &session,
            &project,
            Some(b"#!/usr/bin/env bash\n:\n"),
            &entries,
            &pairs(&session),
            Duration::from_secs(5),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            diff.set,
            vec![("DENV_DOTENV_VALUE".to_string(), value.into_bytes())]
        );
        assert!(!marker.exists());
    }

    #[test]
    fn dotenv_precedence_survives_the_envrc() {
        if Command::new("bash").arg("-c").arg(":").status().is_err() {
            return;
        }
        let tmp = temp("xshi_denv_dotenv_precedence");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let session = Session::for_test();
        let entries = vec![("DENV_PRECEDENCE".to_string(), "from_dotenv".to_string())];
        let diff = eval_env(
            &session,
            &project,
            Some(b"#!/usr/bin/env bash\nunset __DENV_DOT_0\nexport DENV_PRECEDENCE=from_envrc\n"),
            &entries,
            &pairs(&session),
            Duration::from_secs(5),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            diff.set,
            vec![("DENV_PRECEDENCE".to_string(), b"from_dotenv".to_vec())]
        );
    }

    #[test]
    fn state_var_matches_uses_cached_dir() {
        let tmp = temp("xshi_denv_fast_path");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let envrc = project.join(".envrc");
        std::fs::write(&envrc, "export OK=1\n").unwrap();
        let mtime = regular_file_mtime(&envrc).unwrap();
        let state = format!("{mtime} 0 {}", project.display());

        let mut session = Session::for_test();
        session.export_var(b"__DENV_STATE", state.as_bytes());
        assert!(state_var_matches(&find_env_files(&project).unwrap(), &session));

        std::fs::write(&envrc, "export OK=2\n").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&envrc)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(mtime + 2))
            .unwrap();
        assert!(!state_var_matches(
            &find_env_files(&project).unwrap(),
            &session
        ));
    }

    #[test]
    fn dotenv_requires_trust_and_loads_after_allow() {
        let tmp = temp("xshi_denv_dotenv_allow");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(".env"), "DENV_SESSION_PROBE=loaded\n").unwrap();
        let mut session = session_with_data(&tmp);
        session.cwd = project.canonicalize().unwrap();

        let mut stderr = Vec::new();
        after_cwd_change(&mut session, &mut stderr);
        assert!(session.denv_dirty());
        assert_eq!(session.var(b"DENV_SESSION_PROBE"), None);
        assert!(String::from_utf8_lossy(&stderr).contains("is blocked. Run `denv allow`"));

        let status = run_command(&mut session, &DenvCommand::Allow, &mut stderr);
        assert_eq!(status, 0);
        assert!(!session.denv_dirty());
        assert_eq!(session.var(b"DENV_SESSION_PROBE"), Some(&b"loaded"[..]));
        assert!(String::from_utf8_lossy(&stderr).contains("denv: loading .env"));
    }

    #[test]
    fn leaving_restores_exported_and_unexported_values() {
        let tmp = temp("xshi_denv_restore");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join(".env"),
            "DENV_EXPORTED=inside\nDENV_LOCAL=inside\n",
        )
        .unwrap();
        let mut session = session_with_data(&tmp);
        session.export_var(b"DENV_EXPORTED", b"outside");
        session
            .vars
            .insert(b"DENV_LOCAL".to_vec(), b"outside".to_vec());
        session.cwd = project.canonicalize().unwrap();

        let mut stderr = Vec::new();
        let status = run_command(&mut session, &DenvCommand::Allow, &mut stderr);
        assert_eq!(status, 0);
        assert_eq!(session.var(b"DENV_EXPORTED"), Some(&b"inside"[..]));
        assert_eq!(session.var(b"DENV_LOCAL"), Some(&b"inside"[..]));
        assert!(session.env.contains_key(b"DENV_LOCAL".as_slice()));

        session.cwd = tmp.path().canonicalize().unwrap();
        after_cwd_change(&mut session, &mut stderr);
        assert_eq!(session.var(b"DENV_EXPORTED"), Some(&b"outside"[..]));
        assert_eq!(session.var(b"DENV_LOCAL"), Some(&b"outside"[..]));
        assert!(!session.env.contains_key(b"DENV_LOCAL".as_slice()));
    }

    #[test]
    fn active_record_round_trips_and_is_removed_when_the_state_drops() {
        let tmp = temp("xshi_denv_active");
        let project = tmp.path().join("project\nbreak");
        std::fs::create_dir_all(&project).unwrap();
        let data = tmp.path().join("denv");
        let mut session = session_with_data(&tmp);
        let state = ActiveState {
            dir: canonicalize_fallback(&project),
            envrc_mtime: 3,
            dotenv_mtime: 4,
            prev: vec![
                PrevVar::Restore("DENV_EXPORTED".to_string(), b"value".to_vec(), true),
                PrevVar::Restore("DENV_LOCAL".to_string(), b"local".to_vec(), false),
                PrevVar::Unset("DENV_UNSET".to_string()),
            ],
        };

        save_active(&mut session, &data, &state).unwrap();
        let record_id = session.denv.record_id.clone().unwrap();
        let path = active_path(&data, &record_id);
        assert!(path.exists());
        let loaded = parse_active_record(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded.dir, state.dir);
        assert_eq!(loaded.envrc_mtime, 3);
        assert_eq!(loaded.dotenv_mtime, 4);
        assert_eq!(loaded.prev.len(), 3);

        let clone = session.denv.clone();
        drop(session.denv);
        assert!(path.exists(), "a live clone keeps the record");
        drop(clone);
        assert!(!path.exists());
    }

    #[test]
    fn dead_shell_records_are_removed_and_foreign_namespaces_are_kept() {
        let tmp = temp("xshi_denv_dead_records");
        let data = tmp.path().join("denv");
        fs::create_dir_all(&data).unwrap();
        let namespace = current_pid_namespace();
        let dead = i32::MAX - 1;
        let own = format!("active_{dead}-{namespace}-abc");
        let live = format!("active_{}-{namespace}-abc", std::process::id());
        let foreign = format!("active_{dead}-1-abc");
        for name in [&own, &live, &foreign] {
            fs::write(data.join(name), "x").unwrap();
        }

        remove_dead_shell_records(&data, namespace).unwrap();

        assert!(!data.join(&own).exists());
        assert!(data.join(&live).exists());
        if namespace != 1 {
            assert!(data.join(&foreign).exists());
        }
    }
}
