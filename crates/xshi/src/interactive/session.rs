#![allow(clippy::single_call_fn)]

use super::denv::DenvState;
use super::history::History;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::CStr;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use xsh::modules::fs::gitroot;
use xsh::runtime::process::{ManagedChild, ProcessStatus};
use xsh::source::{SourceId, Span};

const HISTORY_PATH: &str = ".local/share/xshi/history";
const DENV_TRUST_PATH: &str = ".local/share/xshi/denv-trust";
static PASSWD_ITER_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct Session {
    pub(super) cwd: PathBuf,
    pub(super) env: BTreeMap<Vec<u8>, Vec<u8>>,
    pub(super) aliases: BTreeMap<String, String>,
    pub(super) last_status: i32,
    pub(super) last_process_status: Option<ProcessStatus>,
    pub(super) home: Option<PathBuf>,
    pub(super) history: History,
    pub(super) denv: DenvState,
    pub(super) user: Option<String>,
    pub(super) host: Option<String>,
    pub(super) colors: bool,
    pub(super) uid_names: BTreeMap<u32, String>,
    pub(super) cwd_snapshot: Option<CwdSnapshot>,
    pub(super) denv_git_root_snapshot: Option<DenvGitRootSnapshot>,
    pub(super) path_commands: Vec<PathCommand>,
    pub(super) git_prompt: Option<String>,
    pub(super) job: Option<InteractiveJob>,
    pub(super) completion_dir_cache: RefCell<BTreeMap<PathBuf, DirCompletionSnapshot>>,
}

pub(super) struct InteractiveJob {
    pub(super) child: ManagedChild,
    pub(super) pid: u32,
    pub(super) pgid: libc::pid_t,
    pub(super) command: String,
    pub(super) state: InteractiveJobState,
    pub(super) terminal_attrs: Option<rustix::termios::Termios>,
    pub(super) last_status: Option<ProcessStatus>,
    pub(super) notified: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InteractiveJobState {
    RunningBackground,
    Stopped,
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            aliases: self.aliases.clone(),
            last_status: self.last_status,
            last_process_status: self.last_process_status.clone(),
            home: self.home.clone(),
            history: self.history.clone(),
            denv: self.denv.clone(),
            user: self.user.clone(),
            host: self.host.clone(),
            colors: self.colors,
            uid_names: self.uid_names.clone(),
            cwd_snapshot: self.cwd_snapshot.clone(),
            denv_git_root_snapshot: self.denv_git_root_snapshot.clone(),
            path_commands: self.path_commands.clone(),
            git_prompt: self.git_prompt.clone(),
            job: None,
            completion_dir_cache: RefCell::new(self.completion_dir_cache.borrow().clone()),
        }
    }
}

impl Session {
    pub(super) fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut env = current_env();
        set_env_bytes(&mut env, b"PWD", cwd.as_os_str().as_bytes());
        let home = home_dir(&env);
        let history_path = home.as_ref().map(|home| home.join(HISTORY_PATH));
        let denv_trust_path = home.as_ref().map(|home| home.join(DENV_TRUST_PATH));
        let history = History::load(history_path);
        let user = env
            .get(b"USER".as_slice())
            .map(|value| String::from_utf8_lossy(value).into_owned())
            .filter(|value| !value.is_empty());
        let host = hostname();
        let colors = stdio_is_tty() && std::env::var_os("NO_COLOR").is_none();
        let uid_names = uid_name_map();
        let cwd_snapshot = CwdSnapshot::read(cwd.clone()).ok();
        let denv_git_root_snapshot = DenvGitRootSnapshot::read(&cwd);
        let path_commands = path_command_cache(&env);
        let git_prompt = git_prompt(&cwd);
        Self {
            cwd,
            env,
            aliases: BTreeMap::new(),
            last_status: 0,
            last_process_status: None,
            home,
            history,
            denv: DenvState::load(denv_trust_path),
            user,
            host,
            colors,
            uid_names,
            cwd_snapshot,
            denv_git_root_snapshot,
            path_commands,
            git_prompt,
            job: None,
            completion_dir_cache: RefCell::new(BTreeMap::new()),
        }
    }

    pub(super) fn set_cwd(&mut self, path: PathBuf) -> Result<(), String> {
        let old = self.cwd.clone();
        let next = if path.is_absolute() {
            path
        } else {
            self.cwd.join(path)
        };
        std::env::set_current_dir(&next).map_err(|err| err.to_string())?;
        self.cwd = std::env::current_dir().unwrap_or(next);
        set_env_bytes(&mut self.env, b"OLDPWD", old.as_os_str().as_bytes());
        set_env_bytes(&mut self.env, b"PWD", self.cwd.as_os_str().as_bytes());
        self.invalidate_denv_git_root_if_needed();
        self.refresh_cwd_snapshot();
        self.refresh_git_prompt();
        Ok(())
    }

    pub(super) fn refresh_cwd_snapshot(&mut self) {
        self.cwd_snapshot = CwdSnapshot::read(self.cwd.clone()).ok();
        self.completion_dir_cache.borrow_mut().clear();
    }

    pub(super) fn invalidate_cwd_snapshot(&mut self) {
        self.cwd_snapshot = None;
        self.denv_git_root_snapshot = None;
        self.completion_dir_cache.borrow_mut().clear();
    }

    pub(super) fn invalidate_denv_git_root_snapshot(&mut self) {
        self.denv_git_root_snapshot = None;
    }

    pub(super) fn refresh_path_commands(&mut self) {
        self.path_commands = path_command_cache(&self.env);
    }

    pub(super) fn refresh_git_prompt(&mut self) {
        self.git_prompt = git_prompt(&self.cwd);
    }

    pub(super) fn denv_git_root_snapshot(&mut self) -> Option<&DenvGitRootSnapshot> {
        self.invalidate_denv_git_root_if_needed();
        if self.denv_git_root_snapshot.is_none() {
            self.denv_git_root_snapshot = DenvGitRootSnapshot::read(&self.cwd);
        }
        self.denv_git_root_snapshot.as_ref()
    }

    fn invalidate_denv_git_root_if_needed(&mut self) {
        if self
            .denv_git_root_snapshot
            .as_ref()
            .is_some_and(|snapshot| !self.cwd.starts_with(snapshot.root()))
        {
            self.denv_git_root_snapshot = None;
        }
    }

    pub(super) fn history_timestamp_ms(&self) -> u64 {
        current_unix_millis()
    }

    pub(super) fn completion_dir_snapshot(&self, path: &Path) -> Option<DirCompletionSnapshot> {
        let metadata = fs::metadata(path).ok()?;
        if !metadata.is_dir() {
            return None;
        }
        let stamp = DirStamp {
            mtime: metadata.mtime(),
            mtime_nsec: metadata.mtime_nsec(),
        };
        if let Some(cached) = self.completion_dir_cache.borrow().get(path)
            && cached.stamp == stamp
        {
            return Some(cached.clone());
        }
        let snapshot = DirCompletionSnapshot::read(path.to_path_buf(), stamp).ok()?;
        self.completion_dir_cache
            .borrow_mut()
            .insert(path.to_path_buf(), snapshot.clone());
        Some(snapshot)
    }
}

#[derive(Clone, Debug)]
pub(super) struct PathCommand {
    pub(super) name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirStamp {
    mtime: i64,
    mtime_nsec: i64,
}

#[derive(Clone, Debug)]
pub(super) struct DirCompletionSnapshot {
    stamp: DirStamp,
    pub(super) entries: Vec<DirCompletionEntry>,
}

impl DirCompletionSnapshot {
    fn read(path: PathBuf, stamp: DirStamp) -> Result<Self, std::io::Error> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.as_bytes() == b"." || name.as_bytes() == b".." {
                continue;
            }
            if name
                .as_bytes()
                .iter()
                .any(|&byte| byte < b' ' || byte == 0x7f)
            {
                continue;
            }
            let metadata = entry.metadata()?;
            let file_type = entry.file_type()?;
            entries.push(DirCompletionEntry {
                name_bytes: name.as_bytes().to_vec(),
                is_dir: metadata.is_dir(),
                is_link: file_type.is_symlink(),
                is_exec: !metadata.is_dir() && metadata.mode() & 0o111 != 0,
                mtime: metadata.mtime(),
            });
        }
        Ok(Self { stamp, entries })
    }
}

#[derive(Clone, Debug)]
pub(super) struct DirCompletionEntry {
    pub(super) name_bytes: Vec<u8>,
    pub(super) is_dir: bool,
    pub(super) is_link: bool,
    pub(super) is_exec: bool,
    pub(super) mtime: i64,
}

#[derive(Clone, Debug)]
pub(super) struct CwdSnapshot {
    pub(super) path: PathBuf,
    pub(super) captured_at: SystemTime,
    pub(super) entries: Vec<CwdEntry>,
}

impl CwdSnapshot {
    fn read(path: PathBuf) -> Result<Self, std::io::Error> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.as_bytes() == b"." || name.as_bytes() == b".." {
                continue;
            }
            if name
                .as_bytes()
                .iter()
                .any(|&byte| byte < b' ' || byte == 0x7f)
            {
                continue;
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            let target_metadata = metadata
                .file_type()
                .is_symlink()
                .then(|| fs::metadata(&path).ok())
                .flatten();
            let link_target = metadata
                .file_type()
                .is_symlink()
                .then(|| fs::read_link(&path).ok())
                .flatten();
            entries.push(CwdEntry {
                path,
                name_bytes: name.as_bytes().to_vec(),
                name: name.to_string_lossy().into_owned(),
                metadata,
                target_metadata,
                link_target,
            });
        }
        Ok(Self {
            path,
            captured_at: SystemTime::now(),
            entries,
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct DenvGitRootSnapshot {
    root: PathBuf,
    entries: Vec<Vec<u8>>,
}

impl DenvGitRootSnapshot {
    fn read(cwd: &Path) -> Option<Self> {
        let root = gitroot(cwd.to_path_buf(), Span::new(SourceId::new(0), 0, 0)).ok()?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(&root).ok()? {
            let name = entry.ok()?.file_name();
            entries.push(name.as_bytes().to_vec());
        }
        Some(Self { root, entries })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn has_entry(&self, name: &[u8]) -> bool {
        self.entries.iter().any(|entry| entry == name)
    }
}

#[derive(Clone, Debug)]
pub(super) struct CwdEntry {
    pub(super) path: PathBuf,
    pub(super) name_bytes: Vec<u8>,
    pub(super) name: String,
    pub(super) metadata: fs::Metadata,
    pub(super) target_metadata: Option<fs::Metadata>,
    pub(super) link_target: Option<PathBuf>,
}

impl CwdEntry {
    pub(super) fn completion_is_dir(&self) -> bool {
        self.target_metadata
            .as_ref()
            .unwrap_or(&self.metadata)
            .is_dir()
    }

    pub(super) fn completion_is_exec(&self) -> bool {
        !self.completion_is_dir() && self.metadata.mode() & 0o111 != 0
    }
}

fn hostname() -> Option<String> {
    let name = rustix::system::uname();
    let s = name.nodename().to_string_lossy();
    if s.is_empty() {
        None
    } else {
        Some(s.into_owned())
    }
}

fn current_env() -> BTreeMap<Vec<u8>, Vec<u8>> {
    std::env::vars_os()
        .map(|(key, value)| (key.into_vec(), value.into_vec()))
        .collect()
}

fn path_command_cache(env: &BTreeMap<Vec<u8>, Vec<u8>>) -> Vec<PathCommand> {
    let Some(path) = env.get(b"PATH".as_slice()) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    for dir in std::env::split_paths(&OsString::from_vec(path.clone())) {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.is_empty() || name.bytes().any(|byte| byte < b' ' || byte == 0x7f) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_file() && metadata.mode() & 0o111 != 0 {
                commands.push(PathCommand {
                    name: name.to_string(),
                });
            }
        }
    }
    commands
}

fn git_prompt(cwd: &Path) -> Option<String> {
    let root = gitroot(
        cwd.to_path_buf(),
        Span::new(xsh::source::SourceId::new(0), 0, 0),
    )
    .ok()?;
    let git_path = root.join(".git");
    let head_path = if git_path.is_dir() {
        git_path.join("HEAD")
    } else {
        let text = fs::read_to_string(&git_path).ok()?;
        let path = text.trim().strip_prefix("gitdir:")?.trim();
        root.join(path).join("HEAD")
    };
    let head = fs::read_to_string(head_path).ok()?;
    let head = head.trim();
    if let Some(name) = head.strip_prefix("ref: refs/heads/") {
        Some(name.to_string())
    } else {
        Some(head.chars().take(12).collect())
    }
}

fn uid_name_map() -> BTreeMap<u32, String> {
    let _guard = PASSWD_ITER_LOCK.lock().expect("lock passwd iterator");
    let mut users = BTreeMap::new();
    unsafe {
        libc::setpwent();
        loop {
            let passwd = libc::getpwent();
            if passwd.is_null() {
                break;
            }
            let name = CStr::from_ptr((*passwd).pw_name)
                .to_string_lossy()
                .into_owned();
            if !name.is_empty() {
                users.insert((*passwd).pw_uid, name);
            }
        }
        libc::endpwent();
    }
    users
}

fn current_unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(super) fn set_env_bytes(env: &mut BTreeMap<Vec<u8>, Vec<u8>>, name: &[u8], value: &[u8]) {
    env.insert(name.to_vec(), value.to_vec());
}

pub(super) fn home_dir(env: &BTreeMap<Vec<u8>, Vec<u8>>) -> Option<PathBuf> {
    env.get(b"HOME".as_slice())
        .map(|value| PathBuf::from(OsString::from_vec(value.clone())))
}

pub(super) fn stdio_is_tty() -> bool {
    rustix::termios::isatty(rustix::stdio::stdin())
        && rustix::termios::isatty(rustix::stdio::stdout())
}
