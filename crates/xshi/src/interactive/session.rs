#![allow(clippy::single_call_fn)]

use super::alias::AliasMap;
use super::denv::DenvState;
use super::history::History;
use super::prompt::Prompt;
use super::shell::Chain;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use xsh::frontend::source::{SourceId, Span};
use xsh::host::fs::gitroot;
use xsh::process::{ManagedChild, ProcessStatus};

const DENV_DATA_PATH: &str = ".local/share/xshi/denv";

pub(super) struct Session {
    pub(super) cwd: PathBuf,
    /// Exported variables: the environment children inherit.
    pub(super) env: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Shell variables that are not exported.
    pub(super) vars: BTreeMap<Vec<u8>, Vec<u8>>,
    pub(super) aliases: AliasMap,
    /// Status shown by the prompt: the last line's status, including
    /// commands the shell answers itself.
    pub(super) last_status: i32,
    /// Value of `$?`: the status of the last command that ran a program or
    /// ordinary builtin. Commands answered by the shell itself (`cd`, `fg`,
    /// `w`, `history`, and so on) leave it unchanged.
    pub(super) dollar_status: i32,
    pub(super) last_process_status: Option<ProcessStatus>,
    pub(super) home: Option<PathBuf>,
    pub(super) history: History,
    pub(super) denv: DenvState,
    pub(super) prompt: Prompt,
    /// Directories visited by this shell, oldest first, for the Ctrl+Backspace picker.
    pub(super) dir_stack: Vec<String>,
    pub(super) denv_git_root_snapshot: Option<DenvGitRootSnapshot>,
    pub(super) job: Option<InteractiveJob>,
    /// Set for a command-substitution session: programs write to a captured
    /// buffer instead of the terminal.
    pub(super) capturing: bool,
    /// Keeps a test session's private HOME alive.
    #[cfg(test)]
    pub(super) test_home: Option<std::sync::Arc<tempfile::TempDir>>,
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
    /// The rest of the `&&`/`||`/`;` list the job was stopped in the middle
    /// of; `fg` runs it once the job finishes.
    pub(super) continuation: Vec<Chain>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InteractiveJobState {
    RunningBackground,
    Stopped,
}

impl Session {
    pub(super) fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::from_env(cwd, current_env())
    }

    /// A session with a private, empty HOME, so tests never touch real
    /// history, trust, or configuration.
    #[cfg(test)]
    pub(super) fn for_test() -> Self {
        let dir = tempfile::tempdir().expect("temporary HOME");
        let home = dir.path().canonicalize().expect("canonical temporary HOME");
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut env = current_env();
        set_env_bytes(&mut env, b"HOME", home.as_os_str().as_bytes());
        set_env_bytes(&mut env, b"USER", b"testuser");
        let mut session = Self::from_env(cwd, env);
        session.test_home = Some(std::sync::Arc::new(dir));
        session
    }

    fn from_env(cwd: PathBuf, env: BTreeMap<Vec<u8>, Vec<u8>>) -> Self {
        let home = home_dir(&env);
        let history = History::load_from_home(home.as_deref().map(Path::as_os_str));
        let denv_data_dir = home.as_ref().map(|home| home.join(DENV_DATA_PATH));
        Self::assemble(cwd, env, history, DenvState::load(denv_data_dir))
    }

    /// A session that reads and writes nothing on disk, for benchmarks.
    #[cfg(feature = "benchmark")]
    pub(super) fn detached(cwd: PathBuf, env: BTreeMap<Vec<u8>, Vec<u8>>, history: History) -> Self {
        Self::assemble(cwd, env, history, DenvState::default())
    }

    fn assemble(
        cwd: PathBuf,
        mut env: BTreeMap<Vec<u8>, Vec<u8>>,
        history: History,
        denv: DenvState,
    ) -> Self {
        set_env_bytes(&mut env, b"PWD", cwd.as_os_str().as_bytes());
        let home = home_dir(&env);
        let user = env_string(&env, b"USER");
        let home_text = env_string(&env, b"HOME");
        let denv_git_root_snapshot = DenvGitRootSnapshot::read(&cwd);
        Self {
            cwd,
            env,
            vars: BTreeMap::new(),
            aliases: AliasMap::default(),
            last_status: 0,
            dollar_status: 0,
            last_process_status: None,
            home,
            history,
            denv,
            prompt: Prompt::with_identity(&user, &home_text),
            dir_stack: Vec::with_capacity(32),
            denv_git_root_snapshot,
            job: None,
            capturing: false,
            #[cfg(test)]
            test_home: None,
        }
    }

    /// Changes the working directory and runs the shared post-cd bookkeeping:
    /// `OLDPWD`/`PWD`, the picker's directory stack, and the prompt's git cache.
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
        self.push_dir_stack();
        self.prompt.invalidate_git();
        Ok(())
    }

    /// A detached copy for evaluating a command substitution: it observes the
    /// current directory, environment, and aliases, but nothing it does
    /// reaches the parent's history, job slot, or prompt state.
    pub(super) fn fork_for_substitution(&self) -> Self {
        Self {
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            vars: self.vars.clone(),
            aliases: self.aliases.clone(),
            last_status: self.last_status,
            dollar_status: self.dollar_status,
            last_process_status: self.last_process_status.clone(),
            home: self.home.clone(),
            history: History::from_entries(Vec::new()),
            denv: self.denv.clone(),
            prompt: Prompt::with_identity("", ""),
            dir_stack: Vec::new(),
            denv_git_root_snapshot: None,
            job: None,
            capturing: true,
            #[cfg(test)]
            test_home: self.test_home.clone(),
        }
    }

    fn push_dir_stack(&mut self) {
        let pwd = self.cwd.to_string_lossy().into_owned();
        // Do not push duplicates at the top.
        if self.dir_stack.last() != Some(&pwd) {
            self.dir_stack.push(pwd);
            // Cap at 50 entries.
            if self.dir_stack.len() > 50 {
                self.dir_stack.remove(0);
            }
        }
    }

    /// Looks up a shell variable: exported variables first, then unexported.
    pub(super) fn var(&self, name: &[u8]) -> Option<&[u8]> {
        self.env
            .get(name)
            .or_else(|| self.vars.get(name))
            .map(Vec::as_slice)
    }

    /// `NAME=value`: updates an exported variable in place, otherwise sets an
    /// unexported shell variable.
    pub(super) fn assign_var(&mut self, name: &[u8], value: &[u8]) {
        if self.env.contains_key(name) {
            set_env_bytes(&mut self.env, name, value);
            self.note_env_change(name);
        } else {
            self.vars.insert(name.to_vec(), value.to_vec());
        }
    }

    /// `export NAME=value` and `set NAME value`: sets and exports.
    pub(super) fn export_var(&mut self, name: &[u8], value: &[u8]) {
        self.vars.remove(name);
        set_env_bytes(&mut self.env, name, value);
        self.note_env_change(name);
    }

    /// `export NAME`: exports an existing shell variable.
    pub(super) fn export_existing(&mut self, name: &[u8]) {
        if let Some(value) = self.vars.remove(name) {
            set_env_bytes(&mut self.env, name, &value);
            self.note_env_change(name);
        }
    }

    pub(super) fn unset_var(&mut self, name: &[u8]) {
        self.vars.remove(name);
        if self.env.remove(name).is_some() {
            self.note_env_change(name);
        }
    }

    /// Keeps values derived from the environment in step with it.
    fn note_env_change(&mut self, name: &[u8]) {
        match name {
            b"HOME" => {
                self.home = home_dir(&self.env);
                self.sync_prompt_identity();
            }
            b"USER" => self.sync_prompt_identity(),
            _ => {}
        }
    }

    /// Re-reads identity values that prompt rendering caches.
    pub(super) fn sync_prompt_identity(&mut self) {
        let user = env_string(&self.env, b"USER");
        let home = env_string(&self.env, b"HOME");
        self.prompt.set_identity(&user, &home);
    }

    pub(super) fn invalidate_denv_git_root_snapshot(&mut self) {
        self.denv_git_root_snapshot = None;
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
}

fn env_string(env: &BTreeMap<Vec<u8>, Vec<u8>>, name: &[u8]) -> String {
    env.get(name)
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .unwrap_or_default()
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

fn current_env() -> BTreeMap<Vec<u8>, Vec<u8>> {
    std::env::vars_os()
        .map(|(key, value)| (key.into_vec(), value.into_vec()))
        .collect()
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
