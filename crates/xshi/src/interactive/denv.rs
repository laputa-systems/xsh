#![allow(clippy::single_call_fn)]

use super::session::Session;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use xsh::diagnostic::DiagnosticRenderer;
use xsh::runtime::eval::Evaluator;
use xsh::sema::check::Checker;
use xsh::source::SourceMap;
use xsh::syntax::parser::Parser;

#[derive(Clone, Debug, Default)]
pub(super) struct DenvState {
    trust_path: Option<PathBuf>,
    trust: BTreeMap<PathBuf, TrustRecord>,
    active: Option<ActiveDenv>,
    pub(super) dirty: bool,
}

#[derive(Clone, Debug)]
struct TrustRecord {
    digest: String,
    denied: bool,
}

#[derive(Clone, Debug)]
struct ActiveDenv {
    dir: PathBuf,
    digest: String,
    saved: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
}

struct ApplyResult {
    status: i32,
    changed: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
struct DenvSources {
    dir: PathBuf,
    envrc: Option<PathBuf>,
    dotenv: Option<PathBuf>,
    digest: String,
}

pub(super) enum DenvCommand {
    Allow,
    Deny,
    Reload,
}

impl DenvState {
    pub(super) fn load(trust_path: Option<PathBuf>) -> Self {
        let mut state = Self {
            trust_path,
            trust: BTreeMap::new(),
            active: None,
            dirty: false,
        };
        let Some(path) = &state.trust_path else {
            return state;
        };
        let Ok(text) = fs::read_to_string(path) else {
            return state;
        };
        for line in text.lines() {
            let mut parts = line.splitn(3, '\t');
            let Some(mode) = parts.next() else {
                continue;
            };
            let Some(digest) = parts.next() else {
                continue;
            };
            let Some(path) = parts.next() else {
                continue;
            };
            state.trust.insert(
                PathBuf::from(path),
                TrustRecord {
                    digest: digest.to_string(),
                    denied: mode == "deny",
                },
            );
        }
        state
    }

    fn save(&self) {
        let Some(path) = &self.trust_path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut text = String::new();
        for (path, record) in &self.trust {
            text.push_str(if record.denied { "deny" } else { "allow" });
            text.push('\t');
            text.push_str(&record.digest);
            text.push('\t');
            text.push_str(&path.display().to_string());
            text.push('\n');
        }
        let _ = fs::write(path, text);
    }
}

pub(super) fn startup(session: &mut Session, stderr: &mut dyn Write) {
    refresh_for_cwd(session, stderr, false);
}

pub(super) fn after_cwd_change(session: &mut Session, stderr: &mut dyn Write) {
    refresh_for_cwd(session, stderr, false);
}

pub(super) fn refresh(session: &mut Session, stderr: &mut dyn Write) {
    refresh_for_cwd(session, stderr, false);
}

pub(super) fn run_command(
    session: &mut Session,
    command: DenvCommand,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> i32 {
    match command {
        DenvCommand::Allow => {
            let Some(sources) = discover(session) else {
                stderr.extend_from_slice(b"denv: no .env/.envrc found\n");
                session.denv.dirty = false;
                return 1;
            };
            session.denv.trust.insert(
                sources.dir.clone(),
                TrustRecord {
                    digest: sources.digest.clone(),
                    denied: false,
                },
            );
            session.denv.save();
            let result = apply_sources(session, &sources, stderr);
            if result.status == 0 {
                print_changed_keys(stdout, &session.env, &result.changed);
                writeln!(stdout, "denv: allowed {}", sources.dir.display()).ok();
            }
            result.status
        }
        DenvCommand::Deny => {
            let Some(sources) = discover(session) else {
                stderr.extend_from_slice(b"denv: no .env/.envrc found\n");
                session.denv.dirty = false;
                return 1;
            };
            unload_active(session);
            session.denv.trust.insert(
                sources.dir.clone(),
                TrustRecord {
                    digest: sources.digest,
                    denied: true,
                },
            );
            session.denv.dirty = false;
            session.denv.save();
            writeln!(stdout, "denv: denied {}", sources.dir.display()).ok();
            0
        }
        DenvCommand::Reload => {
            session.invalidate_denv_git_root_snapshot();
            refresh_for_cwd(session, stderr, true)
        }
    }
}

fn refresh_for_cwd(session: &mut Session, stderr: &mut dyn Write, report: bool) -> i32 {
    let Some(sources) = discover(session) else {
        unload_active(session);
        session.denv.dirty = false;
        return 0;
    };
    let record = session.denv.trust.get(&sources.dir).cloned();
    match record {
        Some(record) if record.denied && record.digest == sources.digest => {
            unload_active(session);
            session.denv.dirty = false;
            if report {
                writeln!(stderr, "denv: denied {}", sources.dir.display()).ok();
            }
            1
        }
        Some(record) if !record.denied && record.digest == sources.digest => {
            if session
                .denv
                .active
                .as_ref()
                .is_some_and(|active| active.dir == sources.dir && active.digest == sources.digest)
            {
                session.denv.dirty = false;
                return 0;
            }
            apply_sources(session, &sources, stderr).status
        }
        _ => {
            unload_active(session);
            session.denv.dirty = true;
            if report {
                writeln!(
                    stderr,
                    "denv: {} is not allowed; run `denv allow` to trust it",
                    sources.dir.display()
                )
                .ok();
            }
            1
        }
    }
}

fn apply_sources(
    session: &mut Session,
    sources: &DenvSources,
    stderr: &mut dyn Write,
) -> ApplyResult {
    unload_active(session);
    let before = session.env.clone();
    if let Some(path) = &sources.dotenv
        && let Err(message) = apply_dotenv(&mut session.env, path)
    {
        writeln!(stderr, "denv: {message}").ok();
        session.denv.dirty = true;
        return ApplyResult {
            status: 1,
            changed: Vec::new(),
        };
    }
    if let Some(path) = &sources.envrc {
        match eval_envrc(session, path) {
            Ok(next_env) => session.env = next_env,
            Err(message) => {
                write!(stderr, "{message}").ok();
                session.denv.dirty = true;
                return ApplyResult {
                    status: 1,
                    changed: Vec::new(),
                };
            }
        }
    }
    let changed = changed_keys(&before, &session.env);
    let mut saved = BTreeMap::new();
    for key in &changed {
        saved.insert(key.clone(), before.get(key).cloned());
    }
    session.denv.active = Some(ActiveDenv {
        dir: sources.dir.clone(),
        digest: sources.digest.clone(),
        saved,
    });
    session.denv.dirty = false;
    ApplyResult {
        status: 0,
        changed: changed.into_iter().collect(),
    }
}

fn print_changed_keys(stdout: &mut Vec<u8>, env: &BTreeMap<Vec<u8>, Vec<u8>>, changed: &[Vec<u8>]) {
    let mut first = true;
    for key in changed {
        if key.starts_with(b"__DENV_") {
            continue;
        }
        let sign = if env.contains_key(key) { '+' } else { '-' };
        if first {
            write!(stdout, "denv: {sign}{}", String::from_utf8_lossy(key)).ok();
            first = false;
        } else {
            write!(stdout, " {sign}{}", String::from_utf8_lossy(key)).ok();
        }
    }
    if !first {
        stdout.push(b'\n');
    }
}

fn unload_active(session: &mut Session) {
    let Some(active) = session.denv.active.take() else {
        return;
    };
    for (key, value) in active.saved {
        if let Some(value) = value {
            session.env.insert(key, value);
        } else {
            session.env.remove(&key);
        }
    }
}

fn apply_dotenv(env: &mut BTreeMap<Vec<u8>, Vec<u8>>, path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            return Err(format!(
                "{}:{}: expected NAME=VALUE",
                path.display(),
                index + 1
            ));
        };
        if !super::app::valid_env_name(name.trim()) {
            return Err(format!(
                "{}:{}: invalid environment name",
                path.display(),
                index + 1
            ));
        }
        env.insert(
            name.trim().as_bytes().to_vec(),
            strip_dotenv_quotes(value.trim()).as_bytes().to_vec(),
        );
    }
    Ok(())
}

fn strip_dotenv_quotes(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn eval_envrc(session: &Session, path: &Path) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
    let text =
        fs::read_to_string(path).map_err(|err| format!("denv: failed to read envrc: {err}\n"))?;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(path.display().to_string(), text.clone());
    let parsed = Parser::parse_source_arena_only(source_id, &text);
    if !parsed.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources));
    }
    let checked = Checker::check_arena_interactive(&parsed.arena, &text);
    if !checked.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new().render(&checked.diagnostics, &sources));
    }
    let output = Evaluator::new_interactive_session_with_sources(
        Vec::new(),
        sources,
        session.cwd.clone(),
        session.env.clone(),
        session.last_process_status.clone(),
    )
    .eval(&parsed.arena, source_id);
    if !output.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new().render(&output.diagnostics, &output.sources));
    }
    if output.status != 0 {
        return Err(format!(
            "denv: envrc exited with status {}\n",
            output.status
        ));
    }
    Ok(output.env)
}

fn changed_keys(
    before: &BTreeMap<Vec<u8>, Vec<u8>>,
    after: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> BTreeSet<Vec<u8>> {
    let mut keys = BTreeSet::new();
    for key in before.keys() {
        if before.get(key) != after.get(key) {
            keys.insert(key.clone());
        }
    }
    for key in after.keys() {
        if before.get(key) != after.get(key) {
            keys.insert(key.clone());
        }
    }
    keys
}

fn discover(session: &mut Session) -> Option<DenvSources> {
    let snapshot = session.denv_git_root_snapshot()?;
    let envrc = snapshot
        .has_entry(b".envrc")
        .then(|| snapshot.root().join(".envrc"));
    let dotenv = snapshot
        .has_entry(b".env")
        .then(|| snapshot.root().join(".env"));
    if envrc.is_none() && dotenv.is_none() {
        return None;
    }
    let digest = digest_sources(envrc.as_deref(), dotenv.as_deref())?;
    Some(DenvSources {
        dir: snapshot.root().to_path_buf(),
        envrc,
        dotenv,
        digest,
    })
}

fn digest_sources(envrc: Option<&Path>, dotenv: Option<&Path>) -> Option<String> {
    let mut hasher = Sha256::new();
    if let Some(path) = envrc {
        hasher.update(b"envrc\0");
        hasher.update(fs::read(path).ok()?);
    }
    if let Some(path) = dotenv {
        hasher.update(b"dotenv\0");
        hasher.update(fs::read(path).ok()?);
    }
    Some(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::discover;
    use crate::xshi::interactive::session::Session;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn discover_stops_at_git_root() {
        let _cwd_guard = CWD_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old_cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let base = std::env::temp_dir().join(format!(
            "xsh-denv-discover-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&base);

        let repo = base.join("repo");
        let nested = repo.join("nested");
        fs::create_dir_all(&nested).expect("create nested directory");
        fs::create_dir(repo.join(".git")).expect("create git marker");
        fs::write(base.join(".env"), "XSHI_DENV_PROBE=loaded\n").expect("write parent dotenv");

        let mut session = Session::new();
        session.set_cwd(nested).expect("set cwd");

        assert!(discover(&mut session).is_none());

        std::env::set_current_dir(old_cwd).expect("restore cwd");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn discover_uses_cached_git_root_snapshot_for_source_presence() {
        let _cwd_guard = CWD_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old_cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let base = std::env::temp_dir().join(format!(
            "xsh-denv-snapshot-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&base);

        let repo = base.join("repo");
        let nested = repo.join("nested");
        fs::create_dir_all(&nested).expect("create nested directory");
        fs::create_dir(repo.join(".git")).expect("create git marker");

        let mut session = Session::new();
        session.set_cwd(nested).expect("set cwd");
        assert!(discover(&mut session).is_none());

        fs::write(repo.join(".env"), "XSHI_DENV_PROBE=loaded\n").expect("write dotenv");
        assert!(discover(&mut session).is_none());

        session.invalidate_denv_git_root_snapshot();
        assert!(discover(&mut session).is_some());

        std::env::set_current_dir(old_cwd).expect("restore cwd");
        let _ = fs::remove_dir_all(base);
    }
}
