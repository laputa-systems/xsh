use std::fs::{self, File};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const TAG_VARIABLE: &str = "XSH_TERMINAL_STORM_TAG";
const EXIT_BOUND: Duration = Duration::from_secs(10);
const RAW_BOUND: Duration = Duration::from_secs(10);
const STRAGGLER_BOUND: Duration = Duration::from_secs(2);
const IGNORED_SIGNAL_SURVIVAL: Duration = Duration::from_millis(100);
const FIXED_SEEDS: std::ops::Range<u64> = 0..24;

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

#[derive(Clone, Copy, Debug)]
struct Plan {
    seed: u64,
    signal: i32,
    after: Duration,
}

impl Plan {
    fn from_seed(seed: u64) -> Self {
        let mut rng = SplitMix64(seed);
        let signal = match rng.below(4) {
            0 => libc::SIGINT,
            1 | 2 => libc::SIGTERM,
            _ => libc::SIGHUP,
        };
        let after = Duration::from_micros(rng.below(250_000));
        Self {
            seed,
            signal,
            after,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hangup {
    Default,
    Ignored,
}

fn tag_for(seed: u64) -> String {
    format!("{}-{seed}", std::process::id())
}

fn live_tagged_processes(tag: &str) -> Vec<(i32, String)> {
    let needle = format!("{TAG_VARIABLE}={tag}\0").into_bytes();
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        panic!("/proc is not readable");
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        let Ok(environ) = fs::read(entry.path().join("environ")) else {
            continue;
        };
        if !environ
            .windows(needle.len())
            .any(|window| window == needle.as_slice())
        {
            continue;
        }
        let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some(after_name) = stat.rsplit_once(')').map(|(_, rest)| rest.trim_start()) else {
            continue;
        };
        if after_name.starts_with('Z') {
            continue;
        }
        let comm = fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        found.push((pid, comm.trim().to_owned()));
    }
    found
}

fn kill_tagged(tag: &str) {
    for (pid, _) in live_tagged_processes(tag) {
        // SAFETY: the pid was just observed in /proc and is only used to stop
        // this run's stray processes.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

struct Bystander(Child);

impl Bystander {
    fn start() -> Self {
        let child = Command::new("sleep")
            .arg("120")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("start bystander");
        Self(child)
    }

    fn alive(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }
}

impl Drop for Bystander {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Terminal {
    master: File,
    slave: File,
}

impl Terminal {
    fn new() -> Self {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: openpty initializes both output descriptors on success.
        let created = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &size,
            )
        };
        assert_eq!(
            created,
            0,
            "openpty: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: openpty returned two owned descriptors.
        let terminal = Self {
            master: unsafe { File::from_raw_fd(master) },
            slave: unsafe { File::from_raw_fd(slave) },
        };
        for file in [&terminal.master, &terminal.slave] {
            assert_eq!(
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        let flags = unsafe { libc::fcntl(terminal.master.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0, "read pty master flags");
        assert_eq!(
            unsafe {
                libc::fcntl(
                    terminal.master.as_raw_fd(),
                    libc::F_SETFL,
                    flags | libc::O_NONBLOCK,
                )
            },
            0,
            "set pty master nonblocking"
        );
        terminal
    }

    fn stdio(&self) -> Stdio {
        self.slave.try_clone().expect("clone pty slave").into()
    }

    fn state(&self) -> TerminalState {
        terminal_state(self.slave.as_raw_fd())
    }

    fn drain(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            match self.master.read(&mut buffer) {
                Ok(0) => return bytes,
                Ok(count) => bytes.extend_from_slice(&buffer[..count]),
                Err(error) if error.raw_os_error() == Some(libc::EIO) => return bytes,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return bytes,
                Err(error) => panic!("read pty master: {error}"),
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalState {
    input: libc::tcflag_t,
    output: libc::tcflag_t,
    control: libc::tcflag_t,
    local: libc::tcflag_t,
    vmin: libc::cc_t,
    vtime: libc::cc_t,
}

fn terminal_state(fd: libc::c_int) -> TerminalState {
    let mut attrs = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(
        unsafe { libc::tcgetattr(fd, &mut attrs) },
        0,
        "tcgetattr: {}",
        std::io::Error::last_os_error()
    );
    TerminalState {
        input: attrs.c_iflag,
        output: attrs.c_oflag,
        control: attrs.c_cflag,
        // PENDIN reports bytes awaiting line-discipline processing, not a
        // terminal mode, and appears only while input is unread.
        local: attrs.c_lflag & !libc::PENDIN,
        vmin: attrs.c_cc[libc::VMIN],
        vtime: attrs.c_cc[libc::VTIME],
    }
}

fn assert_raw(state: &TerminalState) {
    for (flag, label) in [
        (libc::ICANON, "ICANON"),
        (libc::ECHO, "ECHO"),
        (libc::ISIG, "ISIG"),
    ] {
        assert_eq!(state.local & flag, 0, "raw mode left {label} set");
    }
    for (flag, label) in [(libc::IXON, "IXON"), (libc::ICRNL, "ICRNL")] {
        assert_eq!(state.input & flag, 0, "raw mode left {label} set");
    }
    assert_eq!(state.output & libc::OPOST, 0, "raw mode left OPOST set");
}

fn spawn(terminal: &Terminal, home: &Path, tag: &str, hangup: Hangup) -> Child {
    let slave_fd = terminal.slave.as_raw_fd();
    let mut command = Command::new(test_bin!("xshi"));
    command
        .arg("--no-config")
        .env("HOME", home)
        .env("USER", "testuser")
        .env("TERM", "xterm-256color")
        .env("XSHI_PROFILE_PATH", home.join("profile"))
        .env(TAG_VARIABLE, tag)
        .current_dir(home)
        .stdin(terminal.stdio())
        .stdout(terminal.stdio())
        .stderr(terminal.stdio());
    // SAFETY: setsid and TIOCSCTTY only touch the forked child, before exec.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if hangup == Hangup::Ignored && libc::signal(libc::SIGHUP, libc::SIG_IGN) == libc::SIG_ERR {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().expect("start xshi on a pty")
}

fn wait_until_raw(terminal: &mut Terminal, child: &mut Child, started: Instant) -> TerminalState {
    let deadline = started + RAW_BOUND;
    loop {
        let state = terminal.state();
        if state.local & (libc::ICANON | libc::ECHO | libc::ISIG) == 0 {
            return state;
        }
        if let Some(status) = child.try_wait().expect("poll xshi") {
            let output = terminal.drain();
            panic!(
                "xshi exited before entering raw mode: {status:?}; pty output {:?}",
                String::from_utf8_lossy(&output)
            );
        }
        assert!(
            Instant::now() < deadline,
            "xshi did not enter raw mode within {RAW_BOUND:?}"
        );
        let _ = terminal.drain();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait_until_scheduled(
    plan: &Plan,
    started: Instant,
    child: &mut Child,
    terminal: &mut Terminal,
) {
    while started.elapsed() < plan.after {
        if let Some(status) = child.try_wait().expect("poll xshi") {
            panic!(
                "seed {} exited before the scheduled event: {status:?}",
                plan.seed
            );
        }
        let _ = terminal.drain();
        std::thread::sleep(Duration::from_micros(200));
    }
}

fn wait_for_exit(
    plan: &Plan,
    started: Instant,
    child: &mut Child,
    terminal: &mut Terminal,
) -> ExitStatus {
    let deadline = started + EXIT_BOUND;
    loop {
        if let Some(status) = child.try_wait().expect("poll xshi") {
            return status;
        }
        if Instant::now() >= deadline {
            // SAFETY: xshi is its own process-group leader from setsid.
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let status = child.wait().expect("reap killed xshi");
            panic!(
                "seed {} did not exit within {EXIT_BOUND:?} ({status:?})",
                plan.seed
            );
        }
        let _ = terminal.drain();
        std::thread::sleep(Duration::from_micros(500));
    }
}

fn expected_signal(plan: &Plan) -> i32 {
    if plan.signal == libc::SIGINT {
        libc::SIGTERM
    } else {
        plan.signal
    }
}

fn check_outcome(
    plan: &Plan,
    status: &ExitStatus,
    baseline: &TerminalState,
    restored: &TerminalState,
    bystander: &mut Bystander,
) -> Result<(), String> {
    let signal = expected_signal(plan);
    let died_by_signals = status.signal() == Some(signal) || status.code() == Some(128 + signal);
    if !died_by_signals {
        return Err(format!(
            "expected termination by signal {signal}, got {:?} / {:?}",
            status.code(),
            status.signal()
        ));
    }
    if restored != baseline {
        return Err(format!(
            "terminal not restored: baseline {baseline:?}, after {restored:?}"
        ));
    }
    let tag = tag_for(plan.seed);
    let deadline = Instant::now() + STRAGGLER_BOUND;
    let mut survivors = live_tagged_processes(&tag);
    while !survivors.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        survivors = live_tagged_processes(&tag);
    }
    if !survivors.is_empty() {
        kill_tagged(&tag);
        return Err(format!("xshi left live processes behind: {survivors:?}"));
    }
    if !bystander.alive() {
        return Err("a bystander outside the run was signalled".to_owned());
    }
    Ok(())
}

fn run_plan(plan: &Plan) {
    let home = tempfile::tempdir().expect("temporary HOME");
    let mut terminal = Terminal::new();
    let baseline = terminal.state();
    assert_ne!(baseline.local & libc::ICANON, 0, "baseline must be canonical");
    assert_ne!(baseline.local & libc::ECHO, 0, "baseline must echo");
    assert_ne!(baseline.output & libc::OPOST, 0, "baseline must process output");

    let mut bystander = Bystander::start();
    let mut child = spawn(&terminal, home.path(), &tag_for(plan.seed), Hangup::Default);
    let pid = child.id() as libc::pid_t;
    let started = Instant::now();
    let raw = wait_until_raw(&mut terminal, &mut child, started);
    assert_raw(&raw);

    wait_until_scheduled(plan, started, &mut child, &mut terminal);
    if plan.signal == libc::SIGINT {
        // SAFETY: the pid belongs to the child started above.
        unsafe {
            libc::kill(pid, libc::SIGINT);
        }
        let deadline = Instant::now() + IGNORED_SIGNAL_SURVIVAL;
        while Instant::now() < deadline {
            if let Some(status) = child.try_wait().expect("poll xshi") {
                panic!("seed {} exited on an ignored SIGINT: {status:?}", plan.seed);
            }
            let _ = terminal.drain();
            std::thread::sleep(Duration::from_micros(500));
        }
        // SAFETY: the pid belongs to the child started above.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    } else {
        // SAFETY: the pid belongs to the child started above.
        unsafe {
            libc::kill(pid, plan.signal);
        }
    }

    let status = wait_for_exit(plan, started, &mut child, &mut terminal);
    let restored = terminal.state();
    if let Err(reason) = check_outcome(plan, &status, &baseline, &restored, &mut bystander) {
        let output = terminal.drain();
        panic!(
            "terminal storm seed {} failed: {reason}\nplan {plan:?}\nstatus {status:?}\npty output: {:?}",
            plan.seed,
            String::from_utf8_lossy(&output)
        );
    }
}

#[test]
fn terminal_storm_restores_termios_after_seeded_signals() {
    for seed in FIXED_SEEDS {
        run_plan(&Plan::from_seed(seed));
    }
}

#[test]
fn an_inherited_ignored_hangup_stays_ignored_in_raw_mode() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let mut terminal = Terminal::new();
    let baseline = terminal.state();
    let plan = Plan {
        seed: u64::MAX,
        signal: libc::SIGTERM,
        after: Duration::ZERO,
    };
    let mut bystander = Bystander::start();
    let mut child = spawn(&terminal, home.path(), &tag_for(plan.seed), Hangup::Ignored);
    let pid = child.id() as libc::pid_t;
    let started = Instant::now();
    assert_raw(&wait_until_raw(&mut terminal, &mut child, started));
    // SAFETY: the pid belongs to the child started above.
    unsafe {
        libc::kill(pid, libc::SIGHUP);
    }
    let deadline = Instant::now() + IGNORED_SIGNAL_SURVIVAL;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("poll xshi") {
            panic!("xshi exited on an inherited ignored SIGHUP: {status:?}");
        }
        let _ = terminal.drain();
        std::thread::sleep(Duration::from_micros(500));
    }
    // SAFETY: the pid belongs to the child started above.
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }
    let status = wait_for_exit(&plan, started, &mut child, &mut terminal);
    let restored = terminal.state();
    if let Err(reason) = check_outcome(&plan, &status, &baseline, &restored, &mut bystander) {
        panic!("after an ignored SIGHUP: {reason}");
    }
}

#[test]
fn terminal_storm_oracle_rejects_a_terminal_left_in_raw_mode() {
    let plan = Plan {
        seed: 0,
        signal: libc::SIGTERM,
        after: Duration::ZERO,
    };
    let baseline = TerminalState {
        input: 0,
        output: 0,
        control: 0,
        local: libc::ICANON | libc::ECHO,
        vmin: 1,
        vtime: 0,
    };
    let raw = TerminalState {
        local: 0,
        ..baseline.clone()
    };
    let status = {
        let mut child = Command::new("sh")
            .args(["-c", "kill -TERM $$"])
            .spawn()
            .expect("run a child that dies by SIGTERM");
        child.wait().expect("wait for the signalled child")
    };
    let mut bystander = Bystander::start();
    let reason = check_outcome(&plan, &status, &baseline, &raw, &mut bystander)
        .expect_err("a raw terminal must fail the oracle");
    assert!(reason.contains("not restored"), "{reason}");
}
