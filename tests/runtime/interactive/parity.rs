//! Real-binary PTY parity scenarios for `xshi` against `ish`.
//!
//! Every scenario is a script of terminal input driven through a kernel PTY
//! (`ptytest`, with an independent `vt100` screen model). Each `frame` step
//! records the visible rows and cursor position; `effect_*` steps record
//! persistent state. The transcript is compared with a golden recorded from
//! `ish` under `tests/fixtures/interactive-parity/`, so ordinary runs need no
//! `ish` install. With `XSHI_PARITY_ISH_BIN=/path/to/ish` the same script also
//! runs against `ish` and must reproduce the golden (drift detection).
//! `XSHI_PARITY_RECORD=1` with that variable rewrites the goldens from `ish`.
//!
//! Synchronization is event driven: a new prompt is recognized by the OSC 7
//! working-directory report both shells emit at the start of every prompt
//! cycle, and key-level steps wait for terminal output to go quiet with a
//! bounded deadline. Normalization is limited to the scratch HOME path and the
//! machine host name; layout-sensitive widths are expressed relative to the
//! prompt so goldens do not depend on either.

use ptytest::{
    CommandSpec, ExitStatus, ProtocolProfile, PtyTest, Scenario, Size, TerminalBaseline, TestEnv,
};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[path = "parity/extended.rs"]
mod extended;
#[path = "parity/scenarios.rs"]
mod scenarios;

const STEP_TIMEOUT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(60);
const OSC7: &[u8] = b"\x1b]7;";
const USER: &str = "testuser";

/// Terminal input byte sequences.
pub(super) mod key {
    pub const ENTER: &[u8] = b"\r";
    pub const TAB: &[u8] = b"\t";
    pub const ESC: &[u8] = b"\x1b";
    pub const CTRL_A: &[u8] = b"\x01";
    pub const CTRL_C: &[u8] = b"\x03";
    pub const CTRL_D: &[u8] = b"\x04";
    pub const CTRL_E: &[u8] = b"\x05";
    pub const CTRL_K: &[u8] = b"\x0b";
    pub const CTRL_L: &[u8] = b"\x0c";
    pub const CTRL_N: &[u8] = b"\x0e";
    pub const CTRL_P: &[u8] = b"\x10";
    pub const CTRL_R: &[u8] = b"\x12";
    pub const CTRL_U: &[u8] = b"\x15";
    pub const CTRL_W: &[u8] = b"\x17";
    pub const CTRL_Y: &[u8] = b"\x19";
    pub const CTRL_Z: &[u8] = b"\x1a";
    pub const UP: &[u8] = b"\x1b[A";
    pub const DOWN: &[u8] = b"\x1b[B";
    pub const RIGHT: &[u8] = b"\x1b[C";
    pub const LEFT: &[u8] = b"\x1b[D";
    pub const HOME: &[u8] = b"\x1b[H";
    pub const END: &[u8] = b"\x1b[F";
    pub const BACKSPACE: &[u8] = b"\x7f";
    pub const CTRL_BACKSPACE: &[u8] = b"\x08";
    pub const DELETE: &[u8] = b"\x1b[3~";
    pub const CTRL_DELETE: &[u8] = b"\x1b[3;5~";
    pub const CTRL_LEFT: &[u8] = b"\x1b[1;5D";
    pub const CTRL_RIGHT: &[u8] = b"\x1b[1;5C";
    pub const ALT_B: &[u8] = b"\x1bb";
    pub const ALT_D: &[u8] = b"\x1bd";
    pub const ALT_F: &[u8] = b"\x1bf";
    pub const PASTE_START: &[u8] = b"\x1b[200~";
    pub const PASTE_END: &[u8] = b"\x1b[201~";
}

/// `text` wrapped as a bracketed paste.
pub(super) fn paste(text: &str) -> Vec<u8> {
    let mut payload = key::PASTE_START.to_vec();
    payload.extend_from_slice(text.as_bytes());
    payload.extend_from_slice(key::PASTE_END);
    payload
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShellKind {
    Xshi,
    Ish,
}

impl ShellKind {
    fn label(self) -> &'static str {
        match self {
            Self::Xshi => "xshi",
            Self::Ish => "ish",
        }
    }

    fn config_file(self) -> String {
        format!(".config/{}/config.ish", self.label())
    }

    fn history_file(self) -> String {
        format!(".local/share/{}/history", self.label())
    }

    /// Where the shell keeps its `denv` trust files.
    fn denv_allow_dir(self) -> String {
        match self {
            Self::Xshi => ".local/share/xshi/denv/allow".to_owned(),
            Self::Ish => ".local/share/denv/allow".to_owned(),
        }
    }
}

pub(super) struct FixtureFile {
    path: String,
    body: Vec<u8>,
    mode: u32,
    mtime: i64,
    symlink: Option<String>,
    dir: bool,
}

/// Initial state of the isolated HOME and terminal for one scenario.
pub(super) struct Fixture {
    files: Vec<FixtureFile>,
    history: Option<Vec<u8>>,
    config: Option<String>,
    env: Vec<(String, String)>,
    cwd: Option<String>,
    rows: u16,
    cols: Cols,
    args: Vec<String>,
    /// `.envrc` files (relative to HOME) trusted before the shell starts.
    allow: Vec<String>,
}

#[derive(Clone, Copy)]
enum Cols {
    Absolute(u16),
    /// Prompt display width at HOME plus this many columns.
    PromptPlus(u16),
}

impl Fixture {
    pub(super) fn new() -> Self {
        Self {
            files: Vec::new(),
            history: None,
            config: None,
            env: Vec::new(),
            cwd: None,
            rows: 24,
            cols: Cols::Absolute(80),
            args: Vec::new(),
            allow: Vec::new(),
        }
    }

    pub(super) fn file(mut self, path: &str, body: &str) -> Self {
        self.files.push(FixtureFile {
            path: path.to_owned(),
            body: body.as_bytes().to_vec(),
            mode: 0o644,
            mtime: 1_700_000_000,
            symlink: None,
            dir: false,
        });
        self
    }

    pub(super) fn file_at(mut self, path: &str, body: &str, mtime: i64) -> Self {
        self = self.file(path, body);
        self.files.last_mut().expect("just pushed").mtime = mtime;
        self
    }

    pub(super) fn executable(mut self, path: &str, body: &str) -> Self {
        self = self.file(path, body);
        self.files.last_mut().expect("just pushed").mode = 0o755;
        self
    }

    pub(super) fn dir(mut self, path: &str) -> Self {
        self.files.push(FixtureFile {
            path: path.to_owned(),
            body: Vec::new(),
            mode: 0o755,
            mtime: 1_700_000_000,
            symlink: None,
            dir: true,
        });
        self
    }

    pub(super) fn dir_at(mut self, path: &str, mtime: i64) -> Self {
        self = self.dir(path);
        self.files.last_mut().expect("just pushed").mtime = mtime;
        self
    }

    pub(super) fn symlink(mut self, path: &str, target: &str) -> Self {
        self.files.push(FixtureFile {
            path: path.to_owned(),
            body: Vec::new(),
            mode: 0o777,
            mtime: 1_700_000_000,
            symlink: Some(target.to_owned()),
            dir: false,
        });
        self
    }

    /// One history entry per line, the legacy plain-text form.
    pub(super) fn history(mut self, entries: &[&str]) -> Self {
        let mut body = entries.join("\n").into_bytes();
        body.push(b'\n');
        self.history = Some(body);
        self
    }

    /// Structured records, oldest first, as `(directory relative to HOME,
    /// command)`. Timestamps are fixed and old, so every record is recallable
    /// by the shells under test.
    pub(super) fn history_records(mut self, records: &[(&str, &str)]) -> Self {
        let mut body = String::new();
        for (index, (directory, command)) in records.iter().enumerate() {
            let cwd = if directory.is_empty() {
                "{HOME}".to_owned()
            } else {
                format!("{{HOME}}/{directory}")
            };
            body.push_str(&format!(
                ":ish-history:v2\t{}\t7\t{cwd}\t{command}\n",
                1_700_000_000_000_u64 + index as u64 * 1_000
            ));
        }
        self.history = Some(body.into_bytes());
        self
    }

    pub(super) fn config(mut self, text: &str) -> Self {
        self.config = Some(text.to_owned());
        self
    }

    pub(super) fn env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_owned(), value.to_owned()));
        self
    }

    pub(super) fn cwd(mut self, relative: &str) -> Self {
        self.cwd = Some(relative.to_owned());
        self
    }

    pub(super) fn size(mut self, rows: u16, cols: u16) -> Self {
        self.rows = rows;
        self.cols = Cols::Absolute(cols);
        self
    }

    /// Terminal `extra` columns wider than the prompt at HOME, so wrapping
    /// scenarios do not depend on the machine host name length.
    pub(super) fn size_over_prompt(mut self, rows: u16, extra: u16) -> Self {
        self.rows = rows;
        self.cols = Cols::PromptPlus(extra);
        self
    }

    /// Trusts the `.envrc` at `relative` (under HOME) for `denv`.
    pub(super) fn allow_envrc(mut self, relative: &str) -> Self {
        self.allow.push(relative.to_owned());
        self
    }

    /// An extra command-line argument for the shell under test.
    pub(super) fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_owned());
        self
    }
}

static NEXT_HOME: AtomicUsize = AtomicUsize::new(0);

/// A short scratch HOME under the system temporary directory: keeping it short
/// stops absolute paths from wrapping in an 80-column terminal, and the
/// variable part of the name lives in a middle component so the prompt's
/// path shortening (`/p/t/x/home`) is identical from run to run.
struct ScratchHome {
    root: PathBuf,
    home: PathBuf,
}

impl ScratchHome {
    fn new() -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let root = std::fs::canonicalize("/tmp")
            .expect("canonical /tmp")
            .join(format!("xp-{:07}-{sequence:04}", std::process::id()));
        let home = root.join("home");
        std::fs::create_dir_all(&home).expect("create scratch HOME");
        Self { root, home }
    }
}

impl Drop for ScratchHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The host label both shells must show. Narrow-terminal scenarios wrap the
/// prompt, so the *length* of the host name changes what lands on each row.
/// `xshi` is told to show this name; `ish` cannot be, so it must run on a
/// machine whose short name has the same length (recording and drift checks).
const PINNED_HOST: &str = "sentry";

fn host_for(kind: ShellKind) -> String {
    match kind {
        ShellKind::Xshi => PINNED_HOST.to_owned(),
        ShellKind::Ish => {
            let host = hostname();
            assert_eq!(
                host.len(),
                PINNED_HOST.len(),
                "the recorded goldens assume a {}-character host name; run ish on a machine \
                 (or `docker run --hostname {PINNED_HOST}`) where `{host}` has that length",
                PINNED_HOST.len()
            );
            host
        }
    }
}

fn hostname() -> String {
    let mut buffer = [0_u8; 256];
    // SAFETY: the buffer is valid for its length and gethostname NUL-terminates
    // within it on success.
    let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    assert_eq!(status, 0, "gethostname failed");
    let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
    let full = String::from_utf8_lossy(&buffer[..end]).into_owned();
    full.split('.').next().unwrap_or("localhost").to_owned()
}

/// Display width of `user@host ~ $ ` for the fixed test user.
fn prompt_width_at_home(host: &str) -> u16 {
    u16::try_from(USER.len() + 1 + host.len() + " ~ $ ".len()).expect("prompt width")
}

fn set_mtime(path: &Path, mtime: i64, follow: bool) {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).expect("path without NUL");
    let stamp = libc::timespec { tv_sec: mtime, tv_nsec: 0 };
    let times = [stamp, stamp];
    let flags = if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW };
    // SAFETY: c_path is NUL-terminated and times points at two timespecs.
    let status = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), flags) };
    assert_eq!(status, 0, "utimensat {}: {}", path.display(), std::io::Error::last_os_error());
}

fn materialize(home: &Path, fixture: &Fixture, kind: ShellKind) {
    use std::os::unix::fs::PermissionsExt;
    // Parents first, then children, then directory mtimes last so populating a
    // directory does not disturb the timestamp the scenario asked for.
    for file in &fixture.files {
        let path = home.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent");
        }
        if let Some(target) = &file.symlink {
            std::os::unix::fs::symlink(target, &path).expect("create fixture symlink");
        } else if file.dir {
            std::fs::create_dir_all(&path).expect("create fixture directory");
        } else {
            std::fs::write(&path, &file.body).expect("write fixture file");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(file.mode))
                .expect("chmod fixture file");
        }
    }
    // Implicitly created parent directories get the default timestamp too, so
    // listings never depend on when the fixture was built.
    let mut explicit: std::collections::BTreeMap<PathBuf, i64> = std::collections::BTreeMap::new();
    for file in &fixture.files {
        let path = home.join(&file.path);
        for ancestor in path.ancestors().skip(1) {
            if ancestor == home {
                break;
            }
            explicit.entry(ancestor.to_path_buf()).or_insert(1_700_000_000);
        }
    }
    for file in &fixture.files {
        explicit.insert(home.join(&file.path), file.mtime);
    }
    // Children before parents: deepest paths first.
    let mut ordered: Vec<_> = explicit.into_iter().collect();
    ordered.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    for (path, mtime) in ordered {
        set_mtime(&path, mtime, false);
    }
    let history = home.join(kind.history_file());
    std::fs::create_dir_all(history.parent().expect("history parent")).expect("history dir");
    if let Some(body) = &fixture.history {
        let body = String::from_utf8_lossy(body).replace("{HOME}", &home.to_string_lossy());
        std::fs::write(&history, body).expect("write fixture history");
    }
    let config = home.join(kind.config_file());
    std::fs::create_dir_all(config.parent().expect("config parent")).expect("config dir");
    if let Some(text) = &fixture.config {
        std::fs::write(&config, text).expect("write fixture config");
    }
    for relative in &fixture.allow {
        use std::os::unix::fs::MetadataExt as _;
        let envrc = home.join(relative);
        let canonical = std::fs::canonicalize(&envrc).unwrap_or(envrc);
        let allow_dir = home.join(kind.denv_allow_dir());
        std::fs::create_dir_all(&allow_dir).expect("create denv allow dir");
        let mtime = std::fs::metadata(&canonical).expect("envrc metadata").mtime();
        let key: String = canonical
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        std::fs::write(allow_dir.join(key), mtime.to_string()).expect("write denv trust");
    }
    // The state directories the harness itself creates must not carry the
    // time of the run into `l` listings.
    for relative in [".local/share/xshi", ".local/share/ish", ".local/share", ".local", ".config/xshi", ".config/ish", ".config"] {
        let path = home.join(relative);
        if path.exists() && !fixture.files.iter().any(|file| home.join(&file.path) == path) {
            set_mtime(&path, 1_700_000_000, false);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Frame {
    label: String,
    cursor: (usize, usize),
    cursor_visible: bool,
    rows: Vec<String>,
    /// Styled runs as `row attributes text`, in reading order.
    styles: Vec<String>,
}

/// How a shell was started, so a peer can be started the same way.
#[derive(Clone)]
struct Launch {
    binary: PathBuf,
    cwd: PathBuf,
    size: (u16, u16),
    env: Vec<(String, String)>,
    args: Vec<String>,
}

/// One running shell under a PTY with its recorded transcript.
pub(super) struct Live {
    kind: ShellKind,
    term: PtyTest,
    baseline: TerminalBaseline,
    launch: Launch,
    /// Owned by the first shell of a scenario; peers borrow its HOME.
    _scratch: Option<ScratchHome>,
    home: PathBuf,
    host: String,
    prompts_seen: usize,
    frames: Vec<Frame>,
    effects: Vec<(String, String)>,
    exited: Option<ExitStatus>,
}

fn color_text(color: &ptytest::Color) -> String {
    match color {
        ptytest::Color::Default => "default".to_owned(),
        ptytest::Color::Indexed(index) => format!("{index}"),
        ptytest::Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
    }
}

fn count_osc7(output: &[u8]) -> usize {
    output.windows(OSC7.len()).filter(|window| *window == OSC7).count()
}

impl Live {
    fn spawn(kind: ShellKind, binary: &Path, fixture: &Fixture) -> Self {
        let scratch = ScratchHome::new();
        let home = scratch.home.clone();
        materialize(&home, fixture, kind);
        let cwd = fixture
            .cwd
            .as_deref()
            .map_or_else(|| home.clone(), |relative| home.join(relative));
        let host = host_for(kind);
        let cols = match fixture.cols {
            Cols::Absolute(cols) => cols,
            Cols::PromptPlus(extra) => prompt_width_at_home(&host) + extra,
        };
        let launch = Launch {
            binary: binary.to_path_buf(),
            cwd,
            size: (cols, fixture.rows),
            env: fixture.env.clone(),
            args: fixture.args.clone(),
        };
        Self::start(kind, launch, Some(scratch), home, host)
    }

    /// Starts a second shell of the same kind sharing this shell's HOME, the
    /// way a second terminal window would. Fold its transcript into this one
    /// with `absorb`.
    pub(super) fn spawn_peer(&self) -> Self {
        Self::start(self.kind, self.launch.clone(), None, self.home.clone(), self.host.clone())
    }

    /// Ends `peer` and appends its frames and effects, labelled `name/…`.
    pub(super) fn absorb(&mut self, peer: Self, name: &str) {
        let transcript = peer.finish();
        for frame in transcript.frames {
            self.frames.push(Frame { label: format!("{name}/{}", frame.label), ..frame });
        }
        for (effect, text) in transcript.effects {
            self.effects.push((format!("{name}/{effect}"), text));
        }
    }

    fn start(
        kind: ShellKind,
        launch: Launch,
        scratch: Option<ScratchHome>,
        home: PathBuf,
        host: String,
    ) -> Self {
        // Linux images have no `locale` tool to list UTF-8 locales, and both
        // shells decode UTF-8 themselves, so the environment carries the ASCII
        // locale there, as `ish`'s own PTY suite does.
        let environment = if cfg!(target_os = "linux") {
            TestEnv::hermetic_ascii()
        } else {
            TestEnv::hermetic_utf8("C.UTF-8")
        }
        .expect("a hermetic locale must be available on PTY test platforms");
        let mut command = CommandSpec::new(&launch.binary)
            .current_dir(&launch.cwd)
            .env("HOME", &home)
            .env("USER", USER)
            .env("PWD", &launch.cwd)
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("XSHI_PROFILE_PATH", "/dev/null")
            .env("XSHI_HOSTNAME", PINNED_HOST);
        for arg in &launch.args {
            command = command.arg(arg);
        }
        for (name, value) in &launch.env {
            command = command.env(name, value);
        }
        let mut environment = environment
            .env("HOME", &home)
            .env("USER", USER)
            .env("PWD", &launch.cwd)
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("XSHI_PROFILE_PATH", "/dev/null")
            .env("XSHI_HOSTNAME", PINNED_HOST);
        for (name, value) in &launch.env {
            environment = environment.env(name, value);
        }
        let scenario = Scenario::new(format!("parity {}", kind.label()))
            .expect("valid scenario label")
            .command(command)
            .size(Size::new(launch.size.0, launch.size.1).expect("non-zero terminal size"))
            .environment(environment)
            .protocol_profile(ProtocolProfile::xterm_minimal_v1());
        let term = PtyTest::spawn(scenario).expect("spawn shell through ptytest");
        let baseline = term.terminal_baseline();
        let mut live = Self {
            kind,
            term,
            baseline,
            launch,
            _scratch: scratch,
            home,
            host,
            prompts_seen: 0,
            frames: Vec::new(),
            effects: Vec::new(),
            exited: None,
        };
        live.wait_prompt();
        live
    }

    fn deadline(&self) -> ptytest::Deadline {
        self.term.deadline(STEP_TIMEOUT)
    }

    fn settle(&mut self) {
        let deadline = self.deadline();
        self.term.drain(deadline).expect("drain shell output");
        self.term
            .wait_for_quiescence(deadline, QUIET)
            .expect("wait for shell output to go quiet");
    }

    /// Waits for the shell to begin a new prompt cycle (OSC 7), then for its
    /// rendering to go quiet.
    pub(super) fn wait_prompt(&mut self) {
        let deadline = self.deadline();
        loop {
            self.term.drain(deadline).expect("drain shell output");
            if count_osc7(self.term.raw_output()) > self.prompts_seen {
                break;
            }
            let arrived = self.term.wait_for_output(deadline).expect("wait for shell output");
            assert!(
                arrived || count_osc7(self.term.raw_output()) > self.prompts_seen,
                "{}: no new prompt within {STEP_TIMEOUT:?}; screen:\n{}",
                self.kind.label(),
                self.term.screen().to_string()
            );
        }
        self.settle();
        self.prompts_seen = count_osc7(self.term.raw_output());
    }

    pub(super) fn kind(&self) -> ShellKind {
        self.kind
    }

    pub(super) fn home(&self) -> &Path {
        &self.home
    }

    /// Sends raw bytes, waits for the shell to answer with output (every key
    /// repaints), then waits for the terminal to go quiet. Waiting for the
    /// answer keeps a lone Escape from racing the next key: the shell only
    /// resolves Escape after its own short follow-up timeout.
    pub(super) fn keys(&mut self, bytes: &[u8]) {
        let before = self.term.raw_output().len();
        let deadline = self.deadline();
        self.term.send_bytes(deadline, bytes).expect("write to shell PTY");
        loop {
            self.term.drain(deadline).expect("drain shell output");
            if self.term.raw_output().len() > before {
                break;
            }
            let arrived = self.term.wait_for_output(deadline).expect("wait for shell output");
            assert!(
                arrived || self.term.raw_output().len() > before,
                "{}: no output within {STEP_TIMEOUT:?} after {bytes:?}; screen:\n{}",
                self.kind.label(),
                self.term.screen().to_string()
            );
        }
        self.settle();
    }

    /// Sends bytes the shell handles without repainting (for example Ctrl+P).
    pub(super) fn keys_silent(&mut self, bytes: &[u8]) {
        let deadline = self.deadline();
        self.term.send_bytes(deadline, bytes).expect("write to shell PTY");
        self.settle();
    }

    pub(super) fn text(&mut self, text: &str) {
        self.keys(text.as_bytes());
    }

    /// Sends `bytes` repeated `count` times in one write, then settles once.
    pub(super) fn repeat(&mut self, bytes: &[u8], count: usize) {
        self.keys(&bytes.repeat(count));
    }

    /// Types `text`, presses Enter and waits for the next prompt.
    pub(super) fn line(&mut self, text: &str) {
        self.text(text);
        let deadline = self.deadline();
        self.term.send_bytes(deadline, b"\r").expect("write to shell PTY");
        self.wait_prompt();
    }

    /// Presses Enter without expecting a new prompt (for continuation lines).
    pub(super) fn enter(&mut self) {
        self.keys(b"\r");
    }

    /// Sends bytes and waits for the next prompt cycle instead of quiescence.
    pub(super) fn keys_to_prompt(&mut self, bytes: &[u8]) {
        let deadline = self.deadline();
        self.term.send_bytes(deadline, bytes).expect("write to shell PTY");
        self.wait_prompt();
    }

    /// Waits until the visible screen contains `needle`.
    pub(super) fn wait_screen_contains(&mut self, needle: &str) {
        let deadline = self.deadline();
        self.term
            .wait_for_screen(deadline, format!("screen contains {needle:?}"), |screen| {
                screen.contains(needle)
            })
            .expect("wait for screen text");
        self.settle();
    }

    /// Types `text` and Enter, then waits for `needle` on screen instead of a
    /// new prompt (for commands that keep running).
    pub(super) fn line_until(&mut self, text: &str, needle: &str) {
        self.text(text);
        let deadline = self.deadline();
        self.term.send_bytes(deadline, b"\r").expect("write to shell PTY");
        self.wait_screen_contains(needle);
    }

    pub(super) fn resize(&mut self, rows: u16, cols: u16) {
        self.term.resize(Size::new(cols, rows).expect("non-zero terminal size")).expect("resize");
        self.settle();
    }

    /// Records the current screen as a labelled frame: visible text, cursor
    /// position and visibility, and every run of styled cells.
    pub(super) fn frame(&mut self, label: &str) {
        let screen = self.term.screen();
        let mut rows = Vec::new();
        let mut styles = Vec::new();
        for row in 0..screen.row_count() {
            rows.push(self.normalize(screen.row(row).unwrap_or_default().trim_end()));
            let mut run: Option<(String, String)> = None;
            let mut flush = |run: &mut Option<(String, String)>, live: &Self| {
                if let Some((attributes, text)) = run.take() {
                    let text = live.normalize(text.trim_end());
                    if !text.is_empty() {
                        styles.push(format!("{row} {attributes} {text}"));
                    }
                }
            };
            let mut column = 0;
            while let Some(cell) = screen.cell(row, column) {
                column += 1;
                if cell.is_wide_continuation() {
                    continue;
                }
                let attributes = cell.attributes();
                if attributes.is_default() {
                    flush(&mut run, self);
                    continue;
                }
                let text = format!(
                    "fg={} bg={} bold={} dim={} italic={} underline={} inverse={}",
                    color_text(&attributes.foreground),
                    color_text(&attributes.background),
                    u8::from(attributes.bold),
                    u8::from(attributes.dim),
                    u8::from(attributes.italic),
                    u8::from(attributes.underline),
                    u8::from(attributes.inverse),
                );
                match &mut run {
                    Some((current, contents)) if *current == text => contents.push_str(cell.contents()),
                    _ => {
                        flush(&mut run, self);
                        run = Some((text, cell.contents().to_owned()));
                    }
                }
            }
            flush(&mut run, self);
        }
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        let cursor = screen.cursor();
        self.frames.push(Frame {
            label: label.to_owned(),
            cursor: (usize::from(cursor.row), usize::from(cursor.column)),
            cursor_visible: cursor.visible,
            rows,
            styles,
        });
    }

    /// Records only the visible rows containing `needle`. For output whose
    /// surroundings depend on the terminal's raw-mode line handling (a notice
    /// printed while the editor owns the terminal), where only the notice
    /// itself is the contract.
    pub(super) fn frame_rows_containing(&mut self, label: &str, needle: &str) {
        let screen = self.term.screen();
        let rows: Vec<String> = (0..screen.row_count())
            .map(|row| self.normalize(screen.row(row).unwrap_or_default().trim()))
            .filter(|row| row.contains(needle))
            .collect();
        self.frames.push(Frame {
            label: label.to_owned(),
            cursor: (0, 0),
            cursor_visible: true,
            rows,
            styles: Vec::new(),
        });
    }

    fn normalize(&self, text: &str) -> String {
        let home = self.home.to_string_lossy();
        let text = text
            .replace(home.as_ref(), "<HOME>")
            .replace(&format!("@{} ", self.host), "@<HOST> ")
            .replace("xshi-dump", "ish-dump")
            .replace("xshi layout dump", "ish layout dump")
            .replace(".cache/xshi/", ".cache/ish/");
        normalize_dump_names(&normalize_pgid(&normalize_ls_row(&normalize_shell_name(&text))))
    }

    /// Records how many layout dumps the shell has written.
    pub(super) fn effect_dump_count(&mut self, name: &str) {
        let dir = self.home.join(".cache").join(self.kind.label());
        let count = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("dump-"))
            .count();
        self.effects.push((name.to_owned(), count.to_string()));
    }

    /// Records every layout dump the shell has written, oldest first, under
    /// `<cache>/dump-<random>`; the random part is replaced by a placeholder.
    pub(super) fn effect_dumps(&mut self, name: &str) {
        let dir = self.home.join(".cache").join(self.kind.label());
        let mut dumps: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("dump-"))
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                let text = std::fs::read_to_string(entry.path()).ok()?;
                // The package version differs by shell.
                let text = text
                    .lines()
                    .map(|line| if line.starts_with("version: ") { "version: <VERSION>" } else { line })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some((modified, self.normalize(&text)))
            })
            .collect();
        dumps.sort();
        let text = dumps
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join("\n--- next dump ---\n");
        self.effects.push((name.to_owned(), text));
    }

    /// Records the text of a file under HOME as a named persistent effect.
    pub(super) fn effect_file(&mut self, name: &str, relative: &str) {
        let text = match std::fs::read(self.home.join(relative)) {
            Ok(bytes) => self.normalize(&String::from_utf8_lossy(&bytes)),
            Err(error) => format!("<{:?}>", error.kind()),
        };
        self.effects.push((name.to_owned(), text));
    }

    /// Records the shell's history file with timestamps and session ids
    /// replaced by placeholders.
    pub(super) fn effect_history(&mut self, name: &str) {
        let path = self.kind.history_file();
        let text = match std::fs::read(self.home.join(&path)) {
            Ok(bytes) => normalize_history(&self.normalize(&String::from_utf8_lossy(&bytes))),
            Err(error) => format!("<{:?}>", error.kind()),
        };
        self.effects.push((name.to_owned(), text));
    }

    /// Replaces a file under HOME and stamps it `seconds` into the future, the
    /// way a later edit would look to mtime-based trust, without sleeping.
    pub(super) fn rewrite_file(&mut self, relative: &str, body: &str, seconds: u64) {
        let path = self.home.join(relative);
        std::fs::write(&path, body).expect("rewrite file");
        let file = std::fs::File::options().write(true).open(&path).expect("open rewritten file");
        file.set_modified(std::time::SystemTime::now() + Duration::from_secs(seconds))
            .expect("set future mtime");
    }

    /// Records the sorted names of entries directly under a HOME directory.
    pub(super) fn effect_listing(&mut self, name: &str, relative: &str) {
        let mut names: Vec<String> = match std::fs::read_dir(self.home.join(relative)) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(error) => vec![format!("<{:?}>", error.kind())],
        };
        names.sort();
        self.effects.push((name.to_owned(), names.join("\n")));
    }

    /// Waits for the shell to exit and records its exit status as an effect.
    pub(super) fn expect_exit(&mut self, name: &str) {
        let deadline = self.deadline();
        let status = self.term.wait_for_exit(deadline).expect("wait for shell exit");
        if status == ExitStatus::Code(0) {
            self.term
                .assert_terminal_restored(&self.baseline)
                .expect("exit restores represented terminal modes");
        }
        self.exited = Some(status);
        self.effects.push((name.to_owned(), format!("{status:?}")));
    }

    /// The visible rows, trailing blanks trimmed, without terminal metadata.
    pub(super) fn screen_text(&self) -> String {
        let screen = self.term.screen();
        let rows: Vec<String> = (0..screen.row_count())
            .map(|row| self.normalize(screen.row(row).unwrap_or_default().trim_end()))
            .collect();
        rows.join("\n").trim_end().to_owned()
    }

    fn finish(mut self) -> Transcript {
        let deadline = self.term.deadline(Duration::from_secs(3));
        let _ = self.term.finish(deadline);
        Transcript { frames: self.frames, effects: self.effects }
    }
}

/// Layout dump file names end in a random 32-digit hex string.
fn normalize_dump_names(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("dump-") {
        out.push_str(&rest[..index + 5]);
        rest = &rest[index + 5..];
        let hex = rest.chars().take_while(char::is_ascii_hexdigit).count();
        if hex == 32 {
            out.push_str("<HEX>");
            rest = &rest[hex..];
        }
    }
    out.push_str(rest);
    out
}

/// Process group ids in job notices (`pgid=1234`) differ on every run.
fn normalize_pgid(row: &str) -> String {
    let mut out = String::with_capacity(row.len());
    let mut rest = row;
    while let Some(index) = rest.find("pgid=") {
        out.push_str(&rest[..index + 5]);
        rest = &rest[index + 5..];
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 {
            out.push_str("<N>");
        }
        rest = &rest[digits..];
    }
    out.push_str(rest);
    out
}

/// `l` rows carry the account that owns the file and the size of directory
/// entries, both of which depend on the machine; everything else about the row
/// (mode, link count, dates, name, spacing after the group) is kept.
fn normalize_ls_row(row: &str) -> String {
    let mut fields = row.split_whitespace();
    let (Some(mode), Some(nlink), Some(_owner), Some(_group)) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return row.to_owned();
    };
    let mode_ok = mode.len() == 10
        && mode.starts_with(['-', 'd', 'l', 'c', 'b', 'p', 's'])
        && mode[1..].chars().all(|ch| "rwxsStT-".contains(ch));
    if !mode_ok || nlink.parse::<u64>().is_err() {
        return row.to_owned();
    }
    // Locate the end of the group token in the original row.
    let mut end = 0;
    let mut seen = 0;
    let mut in_field = false;
    for (index, ch) in row.char_indices() {
        if ch.is_whitespace() {
            if in_field {
                seen += 1;
                in_field = false;
                if seen == 4 {
                    end = index;
                    break;
                }
            }
        } else {
            in_field = true;
        }
    }
    if end == 0 {
        return row.to_owned();
    }
    let rest = &row[end..];
    let rest = if mode.starts_with(['d', 'l']) {
        // Directory and symlink sizes are filesystem-specific.
        let trimmed = rest.trim_start();
        let size_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        format!(" <SIZE>{}", &trimmed[size_end..])
    } else {
        rest.to_owned()
    };
    format!("{mode} {nlink} <USER> <GROUP>{rest}")
}

/// Message prefixes name the shell (`ish: cd: ...` / `xshi: cd: ...`); that
/// identity is the one intentional textual difference between the two.
fn normalize_shell_name(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous: Option<char> = None;
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        // A caret-echoed control key (`^Z`, `^C`) runs straight into the notice.
        let after_control_echo = out.ends_with("^Z") || out.ends_with("^C");
        let at_boundary = after_control_echo || previous.is_none_or(|ch| !ch.is_alphanumeric());
        let matched = if at_boundary {
            ["xshi:", "epsh:", "ish:"].into_iter().find(|name| rest.starts_with(name))
        } else {
            None
        };
        if let Some(name) = matched {
            out.push_str("<SHELL>:");
            previous = Some(':');
            index += name.len();
        } else {
            let ch = rest.chars().next().expect("index is inside the text");
            out.push(ch);
            previous = Some(ch);
            index += ch.len_utf8();
        }
    }
    out
}

fn normalize_history(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let normalized = if let Some(rest) = line.strip_prefix(":ish-history:v2\t") {
            let mut parts = rest.splitn(4, '\t');
            match (parts.next(), parts.next(), parts.next(), parts.next()) {
                (Some(_), Some(_), Some(cwd), Some(command)) => format!("v2 <TS> <SESSION> {cwd} {command}"),
                _ => line.to_owned(),
            }
        } else if let Some(rest) = line.strip_prefix(":ish-history:v1\t") {
            let mut parts = rest.splitn(3, '\t');
            match (parts.next(), parts.next(), parts.next()) {
                (Some(_), Some(_), Some(command)) => format!("v1 <TS> <SESSION> {command}"),
                _ => line.to_owned(),
            }
        } else {
            line.to_owned()
        };
        out.push_str(&normalized);
        out.push('\n');
    }
    out
}

struct Transcript {
    frames: Vec<Frame>,
    effects: Vec<(String, String)>,
}

impl Transcript {
    fn render(&self) -> String {
        let mut out = String::new();
        for frame in &self.frames {
            let hidden = if frame.cursor_visible { "" } else { " hidden" };
            let _ = writeln!(
                out,
                "== frame {} cursor={},{}{hidden}",
                frame.label, frame.cursor.0, frame.cursor.1
            );
            for row in &frame.rows {
                let _ = writeln!(out, "|{row}");
            }
            for style in &frame.styles {
                let _ = writeln!(out, "@{style}");
            }
        }
        for (name, text) in &self.effects {
            let _ = writeln!(out, "== effect {name}");
            for line in text.lines() {
                let _ = writeln!(out, "|{line}");
            }
        }
        out
    }
}

/// Goldens are per operating system: the programs scenarios run (`cat`, `sh`,
/// `sleep`) word their diagnostics differently on macOS and Linux, and each
/// golden is recorded from `ish` on the platform it is compared on.
fn golden_path(name: &str) -> PathBuf {
    Path::new(cargo_env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/interactive-parity")
        .join(std::env::consts::OS)
        .join(format!("{name}.txt"))
}

fn first_difference(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let mut frame = "<start>";
    for index in 0..expected_lines.len().max(actual_lines.len()) {
        let want = expected_lines.get(index).copied();
        let got = actual_lines.get(index).copied();
        if let Some(line) = want.filter(|line| line.starts_with("== ")) {
            frame = line;
        }
        if want != got {
            let mut context = String::new();
            let _ = writeln!(context, "first difference at line {} in {frame}", index + 1);
            let _ = writeln!(context, "  expected: {want:?}");
            let _ = writeln!(context, "  actual:   {got:?}");
            let full = std::env::var_os("XSHI_PARITY_FULL").is_some();
            let window = |lines: &[&str]| -> String {
                let start = if full { 0 } else { index.saturating_sub(8) };
                let end = if full { lines.len() } else { (index + 8).min(lines.len()) };
                lines[start.min(lines.len())..end].join("\n")
            };
            let _ = writeln!(context, "--- expected\n{}\n--- actual\n{}\n---", window(&expected_lines), window(&actual_lines));
            return context;
        }
    }
    String::new()
}

fn ish_binary() -> Option<PathBuf> {
    std::env::var_os("XSHI_PARITY_ISH_BIN").map(PathBuf::from).inspect(|path| {
        assert!(path.exists(), "XSHI_PARITY_ISH_BIN does not exist: {}", path.display());
    })
}

fn run_shell(kind: ShellKind, binary: &Path, fixture: &Fixture, script: fn(&mut Live)) -> String {
    let mut live = Live::spawn(kind, binary, fixture);
    script(&mut live);
    live.finish().render()
}

/// Runs `script` against `xshi` alone, for behavior the reference shell does
/// not have; the script asserts on the live screen itself.
pub(super) fn run_xshi_only(fixture: &Fixture, script: impl FnOnce(&mut Live)) {
    let xshi = PathBuf::from(cargo_env!("CARGO_BIN_EXE_xshi"));
    let mut live = Live::spawn(ShellKind::Xshi, &xshi, fixture);
    script(&mut live);
    let _ = live.finish();
}

/// Runs `script` against `xshi` and compares with the recorded ish golden.
pub(super) fn run_scenario(name: &str, fixture: &Fixture, script: fn(&mut Live)) {
    let golden = golden_path(name);
    let record = std::env::var_os("XSHI_PARITY_RECORD").is_some();
    if let Some(ish) = ish_binary() {
        let from_ish = run_shell(ShellKind::Ish, &ish, fixture, script);
        if record {
            std::fs::create_dir_all(golden.parent().expect("golden parent")).expect("golden dir");
            std::fs::write(&golden, &from_ish).expect("write golden");
        } else {
            let expected = std::fs::read_to_string(&golden)
                .unwrap_or_else(|error| panic!("read golden {}: {error}", golden.display()));
            assert!(
                expected == from_ish,
                "{name}: ish no longer matches its recorded golden\n{}",
                first_difference(&expected, &from_ish)
            );
        }
    }
    let expected = std::fs::read_to_string(&golden).unwrap_or_else(|error| {
        panic!(
            "{name}: read golden {}: {error}; record it with XSHI_PARITY_ISH_BIN and XSHI_PARITY_RECORD=1",
            golden.display()
        )
    });
    let xshi = PathBuf::from(cargo_env!("CARGO_BIN_EXE_xshi"));
    let actual = run_shell(ShellKind::Xshi, &xshi, fixture, script);
    assert!(
        expected == actual,
        "{name}: xshi differs from the ish golden\n{}",
        first_difference(&expected, &actual)
    );
}
