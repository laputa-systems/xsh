use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, ChildStdin, ChildStdout, ExitStatus, Stdio};

pub(crate) enum ChildStdio {
    Inherit,
    Null,
    Piped,
    Fd(OwnedFd),
}

impl From<File> for ChildStdio {
    fn from(file: File) -> Self {
        Self::Fd(file.into())
    }
}

impl From<OwnedFd> for ChildStdio {
    fn from(fd: OwnedFd) -> Self {
        Self::Fd(fd)
    }
}

impl From<ChildStdout> for ChildStdio {
    fn from(stdout: ChildStdout) -> Self {
        Self::Fd(stdout.into())
    }
}

impl From<io::PipeWriter> for ChildStdio {
    fn from(writer: io::PipeWriter) -> Self {
        Self::Fd(writer.into())
    }
}

impl From<ChildStdio> for Stdio {
    fn from(stdio: ChildStdio) -> Self {
        match stdio {
            ChildStdio::Inherit => Stdio::inherit(),
            ChildStdio::Null => Stdio::null(),
            ChildStdio::Piped => Stdio::piped(),
            ChildStdio::Fd(fd) => Stdio::from(fd),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SignalReset {
    pub signals: &'static [libc::c_int],
    pub sigpipe_ignored: bool,
}

type ChildHook = Box<dyn FnMut() -> io::Result<()> + Send + Sync + 'static>;

pub(crate) struct Command {
    program: OsString,
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    stdio: [Option<ChildStdio>; 3],
    group: Option<libc::pid_t>,
    reset: Option<SignalReset>,
    hooks: Vec<ChildHook>,
}

impl Command {
    /// The environment starts empty: only variables added with `env` reach the child.
    pub(crate) fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            stdio: [None, None, None],
            group: None,
            reset: None,
            hooks: Vec::new(),
        }
    }

    pub(crate) fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args.extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    pub(crate) fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_owned());
        self
    }

    pub(crate) fn env(&mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        let name = name.as_ref();
        let value = value.as_ref().to_owned();
        match self.env.iter_mut().find(|(existing, _)| existing == name) {
            Some(entry) => entry.1 = value,
            None => self.env.push((name.to_owned(), value)),
        }
        self
    }

    pub(crate) fn stdin(&mut self, stdio: impl Into<ChildStdio>) -> &mut Self {
        self.stdio[0] = Some(stdio.into());
        self
    }

    pub(crate) fn stdout(&mut self, stdio: impl Into<ChildStdio>) -> &mut Self {
        self.stdio[1] = Some(stdio.into());
        self
    }

    pub(crate) fn stderr(&mut self, stdio: impl Into<ChildStdio>) -> &mut Self {
        self.stdio[2] = Some(stdio.into());
        self
    }

    pub(crate) fn process_group(&mut self, pgid: libc::pid_t) -> &mut Self {
        self.group = Some(pgid);
        self
    }

    /// Starts the child with `reset.signals` at their default action even
    /// when this process ignores them, and SIGPIPE as the script sees it.
    pub(crate) fn reset_signals(&mut self, reset: SignalReset) -> &mut Self {
        self.reset = Some(reset);
        self
    }

    /// Runs `hook` in the child before the exec, which makes the spawn fork.
    ///
    /// # Safety
    /// The hook runs between fork and exec of a multi-threaded process, so it
    /// may call only async-signal-safe functions and must not allocate.
    pub(crate) unsafe fn pre_exec(
        &mut self,
        hook: impl FnMut() -> io::Result<()> + Send + Sync + 'static,
    ) -> &mut Self {
        self.hooks.push(Box::new(hook));
        self
    }

    pub(crate) fn spawns_directly(&self) -> bool {
        direct::SUPPORTED
            && self.reset.is_some()
            && self.hooks.is_empty()
            && Path::new(&self.program).is_absolute()
    }

    pub(crate) fn spawn(self) -> io::Result<Child> {
        if self.spawns_directly() {
            return direct::spawn(self);
        }
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.args);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command.env_clear();
        command.envs(self.env.iter().map(|(name, value)| (name, value)));
        let [stdin, stdout, stderr] = self.stdio;
        if let Some(stdio) = stdin {
            command.stdin(Stdio::from(stdio));
        }
        if let Some(stdio) = stdout {
            command.stdout(Stdio::from(stdio));
        }
        if let Some(stdio) = stderr {
            command.stderr(Stdio::from(stdio));
        }
        if let Some(pgid) = self.group {
            command.process_group(pgid);
        }
        if let Some(reset) = self.reset {
            // SAFETY: signal is async-signal-safe and the closure allocates nothing.
            unsafe {
                command.pre_exec(move || {
                    reset_signal_dispositions(reset);
                    Ok(())
                });
            }
        }
        for hook in self.hooks {
            // SAFETY: `Command::pre_exec` requires the caller to uphold this.
            unsafe {
                command.pre_exec(hook);
            }
        }
        command.spawn().map(Child::from_std)
    }

    /// Hands the stdio choices and child hooks to a standard command, for
    /// callers that spawn through the standard library themselves.
    pub(crate) fn transfer_setup_to(self, target: &mut std::process::Command) {
        let [stdin, stdout, stderr] = self.stdio;
        if let Some(stdio) = stdin {
            target.stdin(Stdio::from(stdio));
        }
        if let Some(stdio) = stdout {
            target.stdout(Stdio::from(stdio));
        }
        if let Some(stdio) = stderr {
            target.stderr(Stdio::from(stdio));
        }
        for hook in self.hooks {
            // SAFETY: `Command::pre_exec` requires the caller to uphold this.
            unsafe {
                target.pre_exec(hook);
            }
        }
    }
}

fn reset_signal_dispositions(reset: SignalReset) {
    for &signal in reset.signals {
        // SAFETY: signal is async-signal-safe.
        unsafe {
            libc::signal(signal, libc::SIG_DFL);
        }
    }
    let sigpipe = if reset.sigpipe_ignored { libc::SIG_IGN } else { libc::SIG_DFL };
    // SAFETY: signal is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGPIPE, sigpipe);
    }
}

pub(crate) struct Child {
    pid: u32,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) stdout: Option<ChildStdout>,
    pub(crate) stderr: Option<ChildStderr>,
    status: Option<ExitStatus>,
}

impl Child {
    fn from_std(mut child: std::process::Child) -> Self {
        Self {
            pid: child.id(),
            stdin: child.stdin.take(),
            stdout: child.stdout.take(),
            stderr: child.stderr.take(),
            status: None,
        }
    }

    pub(crate) fn id(&self) -> u32 {
        self.pid
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        drop(self.stdin.take());
        if let Some(status) = self.status {
            return Ok(status);
        }
        let mut raw = 0;
        loop {
            // SAFETY: waitpid writes one c_int through a valid pointer.
            let waited = unsafe { libc::waitpid(self.pid as libc::pid_t, &mut raw, 0) };
            if waited >= 0 {
                break;
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINTR) {
                return Err(error);
            }
        }
        let status = ExitStatus::from_raw(raw);
        self.status = Some(status);
        Ok(status)
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let mut raw = 0;
        loop {
            // SAFETY: waitpid writes one c_int through a valid pointer.
            let waited = unsafe { libc::waitpid(self.pid as libc::pid_t, &mut raw, libc::WNOHANG) };
            if waited == 0 {
                return Ok(None);
            }
            if waited > 0 {
                break;
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINTR) {
                return Err(error);
            }
        }
        let status = ExitStatus::from_raw(raw);
        self.status = Some(status);
        Ok(Some(status))
    }

    pub(crate) fn kill(&mut self) -> io::Result<()> {
        if self.status.is_some() {
            return Ok(());
        }
        // SAFETY: kill takes plain integers.
        if unsafe { libc::kill(self.pid as libc::pid_t, libc::SIGKILL) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
mod direct {
    use super::{Child, ChildStdio, Command, SignalReset};
    use std::ffi::{CString, OsStr};
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::process::{ChildStderr, ChildStdin, ChildStdout};

    pub(super) const SUPPORTED: bool = true;

    struct SpawnAttributes(libc::posix_spawnattr_t);

    impl SpawnAttributes {
        fn new() -> io::Result<Self> {
            // SAFETY: the structure is plain data that init fully sets.
            let mut raw: libc::posix_spawnattr_t = unsafe { std::mem::zeroed() };
            check(unsafe { libc::posix_spawnattr_init(&mut raw) })?;
            Ok(Self(raw))
        }
    }

    impl Drop for SpawnAttributes {
        fn drop(&mut self) {
            // SAFETY: the value was initialized by `posix_spawnattr_init`.
            unsafe {
                libc::posix_spawnattr_destroy(&mut self.0);
            }
        }
    }

    struct FileActions(libc::posix_spawn_file_actions_t);

    impl FileActions {
        fn new() -> io::Result<Self> {
            // SAFETY: the structure is plain data that init fully sets.
            let mut raw: libc::posix_spawn_file_actions_t = unsafe { std::mem::zeroed() };
            check(unsafe { libc::posix_spawn_file_actions_init(&mut raw) })?;
            Ok(Self(raw))
        }
    }

    impl Drop for FileActions {
        fn drop(&mut self) {
            // SAFETY: the value was initialized by `posix_spawn_file_actions_init`.
            unsafe {
                libc::posix_spawn_file_actions_destroy(&mut self.0);
            }
        }
    }

    fn check(code: libc::c_int) -> io::Result<()> {
        if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code)) }
    }

    fn c_string(value: &OsStr) -> io::Result<CString> {
        CString::new(value.as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "nul byte found in provided data")
        })
    }

    fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: pipe2 fills two descriptors.
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: pipe2 returned two descriptors this function now owns.
        Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
    }

    fn signal_set(signals: &[libc::c_int]) -> libc::sigset_t {
        // SAFETY: sigemptyset initializes the set before sigaddset reads it.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            for &signal in signals {
                libc::sigaddset(&mut set, signal);
            }
            set
        }
    }

    pub(super) fn spawn(command: Command) -> io::Result<Child> {
        let Command { program, args, cwd, env, stdio, group, reset, hooks: _ } = command;
        let SignalReset { signals, sigpipe_ignored } = reset.expect("a direct spawn resets signals");

        let program = c_string(&program)?;
        let mut argv_storage = vec![program.clone()];
        for arg in &args {
            argv_storage.push(c_string(arg)?);
        }
        let mut env_storage = Vec::with_capacity(env.len());
        for (name, value) in &env {
            let mut entry = name.as_bytes().to_vec();
            entry.push(b'=');
            entry.extend_from_slice(value.as_bytes());
            env_storage.push(CString::new(entry).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "nul byte found in provided data")
            })?);
        }
        let cwd = cwd.as_deref().map(|cwd| c_string(cwd.as_os_str())).transpose()?;
        let argv: Vec<*mut libc::c_char> = argv_storage
            .iter()
            .map(|arg| arg.as_ptr().cast_mut())
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect();
        let envp: Vec<*mut libc::c_char> = env_storage
            .iter()
            .map(|entry| entry.as_ptr().cast_mut())
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect();

        let mut attributes = SpawnAttributes::new()?;
        let mut actions = FileActions::new()?;
        let mut flags = libc::POSIX_SPAWN_SETSIGDEF | libc::POSIX_SPAWN_SETSIGMASK;

        let mut default_signals = signals.to_vec();
        if !sigpipe_ignored {
            default_signals.push(libc::SIGPIPE);
        }
        let defaults = signal_set(&default_signals);
        let empty_mask = signal_set(&[]);
        // SAFETY: the attributes were initialized and the sets are valid.
        unsafe {
            check(libc::posix_spawnattr_setsigdefault(&mut attributes.0, &defaults))?;
            check(libc::posix_spawnattr_setsigmask(&mut attributes.0, &empty_mask))?;
        }
        if let Some(pgid) = group {
            flags |= libc::POSIX_SPAWN_SETPGROUP;
            // SAFETY: the attributes were initialized.
            check(unsafe { libc::posix_spawnattr_setpgroup(&mut attributes.0, pgid) })?;
        }
        // SAFETY: the attributes were initialized.
        check(unsafe { libc::posix_spawnattr_setflags(&mut attributes.0, flags as _) })?;

        let mut child_ends: Vec<OwnedFd> = Vec::new();
        let mut parent_ends: [Option<OwnedFd>; 3] = [None, None, None];
        for (target, choice) in stdio.into_iter().enumerate() {
            let target = target as libc::c_int;
            match choice {
                None | Some(ChildStdio::Inherit) => {}
                Some(ChildStdio::Null) => {
                    let access = if target == 0 { libc::O_RDONLY } else { libc::O_WRONLY };
                    // SAFETY: the actions were initialized and the path is a C string literal.
                    check(unsafe {
                        libc::posix_spawn_file_actions_addopen(
                            &mut actions.0,
                            target,
                            c"/dev/null".as_ptr(),
                            access,
                            0,
                        )
                    })?;
                }
                Some(ChildStdio::Piped) => {
                    let (read, write) = pipe()?;
                    let (child_end, parent_end) = if target == 0 { (read, write) } else { (write, read) };
                    // SAFETY: the actions were initialized and the descriptor outlives the spawn.
                    check(unsafe {
                        libc::posix_spawn_file_actions_adddup2(&mut actions.0, child_end.as_raw_fd(), target)
                    })?;
                    child_ends.push(child_end);
                    parent_ends[target as usize] = Some(parent_end);
                }
                Some(ChildStdio::Fd(fd)) => {
                    // SAFETY: the actions were initialized and the descriptor outlives the spawn.
                    check(unsafe {
                        libc::posix_spawn_file_actions_adddup2(&mut actions.0, fd.as_raw_fd(), target)
                    })?;
                    child_ends.push(fd);
                }
            }
        }
        if let Some(cwd) = &cwd {
            // SAFETY: the actions were initialized and the path outlives the spawn.
            check(unsafe { libc::posix_spawn_file_actions_addchdir_np(&mut actions.0, cwd.as_ptr()) })?;
        }

        let mut pid: libc::pid_t = 0;
        // SAFETY: every pointer refers to storage that lives until the call returns,
        // and the argument and environment arrays are null-terminated.
        let code = unsafe {
            libc::posix_spawn(
                &mut pid,
                program.as_ptr(),
                &actions.0,
                &attributes.0,
                argv.as_ptr(),
                envp.as_ptr(),
            )
        };
        check(code)?;
        drop(child_ends);

        let [stdin, stdout, stderr] = parent_ends;
        Ok(Child {
            pid: pid as u32,
            stdin: stdin.map(ChildStdin::from),
            stdout: stdout.map(ChildStdout::from),
            stderr: stderr.map(ChildStderr::from),
            status: None,
        })
    }
}

#[cfg(not(all(target_os = "linux", target_env = "musl")))]
mod direct {
    use super::{Child, Command};
    use std::io;

    pub(super) const SUPPORTED: bool = false;

    pub(super) fn spawn(_command: Command) -> io::Result<Child> {
        unreachable!("direct spawning is not supported on this target")
    }
}
