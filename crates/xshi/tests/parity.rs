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

macro_rules! cargo_env {
    ($name:literal) => {
        env!($name)
    };
}

#[path = "parity/builtins.rs"]
mod builtins;
#[path = "parity/catchup.rs"]
mod catchup;
#[path = "parity/editor.rs"]
mod editor;
#[path = "parity/denv.rs"]
mod denv;
#[path = "parity/extended.rs"]
mod extended;
#[path = "parity/fdu.rs"]
mod fdu;
#[path = "parity/fs.rs"]
mod fs;
#[path = "parity/history.rs"] mod history;
#[path = "parity/scenarios.rs"]
mod scenarios;

const STEP_TIMEOUT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(60);
const OSC7: &[u8] = b"\x1b]7;";
const USER: &str = "testuser";

/// Terminal input byte sequences.
pub(crate) mod key {
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
pub(crate) fn paste(text: &str) -> Vec<u8> {
    let mut payload = key::PASTE_START.to_vec();
    payload.extend_from_slice(text.as_bytes());
    payload.extend_from_slice(key::PASTE_END);
    payload
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellKind {
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

pub(crate) struct FixtureFile {
    path: String,
    body: Vec<u8>,
    mode: u32,
    mtime: i64,
    symlink: Option<String>,
    dir: bool,
}

/// Initial state of the isolated HOME and terminal for one scenario.
pub(crate) struct Fixture {
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
    launcher: Option<String>,
}

#[derive(Clone, Copy)]
enum Cols {
    Absolute(u16),
    /// Prompt display width at HOME plus this many columns.
    PromptPlus(u16),
}

impl Fixture {
    pub(crate) fn new() -> Self {
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
            launcher: None,
        }
    }

    pub(crate) fn file(mut self, path: &str, body: &str) -> Self {
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

    /// A file whose contents are arbitrary bytes.
    pub(crate) fn file_bytes(mut self, path: &str, body: &[u8]) -> Self {
        self = self.file(path, "");
        self.files.last_mut().expect("just pushed").body = body.to_vec();
        self
    }

    pub(crate) fn file_at(mut self, path: &str, body: &str, mtime: i64) -> Self {
        self = self.file(path, body);
        self.files.last_mut().expect("just pushed").mtime = mtime;
        self
    }

    pub(crate) fn executable(mut self, path: &str, body: &str) -> Self {
        self = self.file(path, body);
        self.files.last_mut().expect("just pushed").mode = 0o755;
        self
    }

    pub(crate) fn dir(mut self, path: &str) -> Self {
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

    pub(crate) fn dir_at(mut self, path: &str, mtime: i64) -> Self {
        self = self.dir(path);
        self.files.last_mut().expect("just pushed").mtime = mtime;
        self
    }

    pub(crate) fn symlink(mut self, path: &str, target: &str) -> Self {
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
    pub(crate) fn history(mut self, entries: &[&str]) -> Self {
        let mut body = entries.join("\n").into_bytes();
        body.push(b'\n');
        self.history = Some(body);
        self
    }

    /// Structured records, oldest first, as `(directory relative to HOME,
    /// command)`. Timestamps are fixed and old, so every record is recallable
    /// by the shells under test.
    pub(crate) fn history_records(mut self, records: &[(&str, &str)]) -> Self {
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

    pub(crate) fn config(mut self, text: &str) -> Self {
        self.config = Some(text.to_owned());
        self
    }

    /// An environment variable for the shell; `{HOME}` in `value` becomes the
    /// scratch HOME path.
    pub(crate) fn env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_owned(), value.to_owned()));
        self
    }

    pub(crate) fn cwd(mut self, relative: &str) -> Self {
        self.cwd = Some(relative.to_owned());
        self
    }

    pub(crate) fn size(mut self, rows: u16, cols: u16) -> Self {
        self.rows = rows;
        self.cols = Cols::Absolute(cols);
        self
    }

    /// Terminal `extra` columns wider than the prompt at HOME, so wrapping
    /// scenarios do not depend on the machine host name length.
    pub(crate) fn size_over_prompt(mut self, rows: u16, extra: u16) -> Self {
        self.rows = rows;
        self.cols = Cols::PromptPlus(extra);
        self
    }

    /// Trusts the `.envrc` at `relative` (under HOME) for `denv`.
    pub(crate) fn allow_envrc(mut self, relative: &str) -> Self {
        self.allow.push(relative.to_owned());
        self
    }

    /// An extra command-line argument for the shell under test.
    pub(crate) fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_owned());
        self
    }

    /// Runs the shell as `"$1"` of a `/bin/sh -c SCRIPT` session leader, so
    /// the outer terminal outlives the shell and the script can inspect it.
    pub(crate) fn launcher(mut self, script: &str) -> Self {
        self.launcher = Some(script.to_owned());
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
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
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
    let stamp = libc::timespec {
        tv_sec: mtime,
        tv_nsec: 0,
    };
    let times = [stamp, stamp];
    let flags = if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW };
    // SAFETY: c_path is NUL-terminated and times points at two timespecs.
    let status = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), flags) };
    assert_eq!(
        status,
        0,
        "utimensat {}: {}",
        path.display(),
        std::io::Error::last_os_error()
    );
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
            explicit
                .entry(ancestor.to_path_buf())
                .or_insert(1_700_000_000);
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
        let envrc = home.join(relative);
        let canonical = std::fs::canonicalize(&envrc).unwrap_or(envrc);
        let allow_dir = home.join(kind.denv_allow_dir());
        std::fs::create_dir_all(&allow_dir).expect("create denv allow dir");
        let contents = std::fs::read(&canonical).expect("read trusted file");
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest as _;
        hasher.update(&contents);
        let content_hash: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let mut path_hasher = sha2::Sha256::new();
        path_hasher.update(canonical.as_os_str().as_encoded_bytes());
        let key: String = format!(
            "sha256-{}",
            path_hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        std::fs::write(allow_dir.join(key), content_hash).expect("write denv trust");
    }
    // The state directories the harness itself creates must not carry the
    // time of the run into `l` listings.
    for relative in [
        ".local/share/xshi",
        ".local/share/ish",
        ".local/share",
        ".local",
        ".config/xshi",
        ".config/ish",
        ".config",
    ] {
        let path = home.join(relative);
        if path.exists()
            && !fixture
                .files
                .iter()
                .any(|file| home.join(&file.path) == path)
        {
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
    launcher: Option<String>,
}

/// One running shell under a PTY with its recorded transcript.
pub(crate) struct Live {
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
    output
        .windows(OSC7.len())
        .filter(|window| *window == OSC7)
        .count()
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
            env: fixture
                .env
                .iter()
                .map(|(name, value)| {
                    (name.clone(), value.replace("{HOME}", &home.to_string_lossy()))
                })
                .collect(),
            args: fixture.args.clone(),
            launcher: fixture.launcher.clone(),
        };
        Self::start(kind, launch, Some(scratch), home, host)
    }

    /// Starts a second shell of the same kind sharing this shell's HOME, the
    /// way a second terminal window would. Fold its transcript into this one
    /// with `absorb`.
    pub(crate) fn spawn_peer(&self) -> Self {
        Self::start(
            self.kind,
            self.launch.clone(),
            None,
            self.home.clone(),
            self.host.clone(),
        )
    }

    /// Ends `peer` and appends its frames and effects, labelled `name/…`.
    pub(crate) fn absorb(&mut self, peer: Self, name: &str) {
        let transcript = peer.finish();
        for frame in transcript.frames {
            self.frames.push(Frame {
                label: format!("{name}/{}", frame.label),
                ..frame
            });
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
        let command = match &launch.launcher {
            Some(script) => CommandSpec::new("/bin/sh")
                .arg("-c")
                .arg(script)
                .arg("sh")
                .arg(&launch.binary),
            None => CommandSpec::new(&launch.binary),
        };
        let mut command = command
            .current_dir(&launch.cwd)
            .env("HOME", &home)
            .env("USER", USER)
            .env("PWD", &launch.cwd)
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("XSHI_PROFILE_PATH", "/dev/null")
            .env("XSHI_HOSTNAME", PINNED_HOST)
            // ish's OSC 133 prompt markers are outside xterm-minimal-v1; its
            // own PTY suite suppresses them with this variable.
            .env("ISH_TEST_SUPPRESS_OSC133", "1");
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
            .env("XSHI_HOSTNAME", PINNED_HOST)
            .env("ISH_TEST_SUPPRESS_OSC133", "1");
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
    pub(crate) fn wait_prompt(&mut self) {
        let deadline = self.deadline();
        loop {
            self.term.drain(deadline).expect("drain shell output");
            if count_osc7(self.term.raw_output()) > self.prompts_seen {
                break;
            }
            let arrived = self
                .term
                .wait_for_output(deadline)
                .expect("wait for shell output");
            assert!(
                arrived || count_osc7(self.term.raw_output()) > self.prompts_seen,
                "{}: no new prompt within {STEP_TIMEOUT:?}; screen:\n{}",
                self.kind.label(),
                self.term.screen()
            );
        }
        self.settle();
        self.prompts_seen = count_osc7(self.term.raw_output());
    }

    pub(crate) fn kind(&self) -> ShellKind {
        self.kind
    }

    pub(crate) fn home(&self) -> &Path {
        &self.home
    }

    /// Sends raw bytes, waits for the shell to answer with output (every key
    /// repaints), then waits for the terminal to go quiet. Waiting for the
    /// answer keeps a lone Escape from racing the next key: the shell only
    /// resolves Escape after its own short follow-up timeout.
    pub(crate) fn keys(&mut self, bytes: &[u8]) {
        let before = self.term.raw_output().len();
        let deadline = self.deadline();
        self.term
            .send_bytes(deadline, bytes)
            .expect("write to shell PTY");
        loop {
            self.term.drain(deadline).expect("drain shell output");
            if self.term.raw_output().len() > before {
                break;
            }
            let arrived = self
                .term
                .wait_for_output(deadline)
                .expect("wait for shell output");
            assert!(
                arrived || self.term.raw_output().len() > before,
                "{}: no output within {STEP_TIMEOUT:?} after {bytes:?}; screen:\n{}",
                self.kind.label(),
                self.term.screen()
            );
        }
        self.settle();
    }

    /// Sends bytes without waiting for an answer, for typeahead and pastes
    /// that stay unread until a child or the editor consumes them.
    pub(crate) fn send_raw(&mut self, bytes: &[u8]) {
        let deadline = self.deadline();
        self.term
            .send_bytes(deadline, bytes)
            .expect("write to shell PTY");
        self.settle();
    }

    /// Delivers a signal to the shell process.
    pub(crate) fn signal(&mut self, signal: i32) {
        self.term.signal(signal).expect("signal the shell");
    }

    /// Sends bytes and returns at once, without waiting for an answer or for
    /// the terminal to go quiet, so a scenario can observe work in flight.
    pub(crate) fn send_now(&mut self, bytes: &[u8]) {
        let deadline = self.deadline();
        self.term
            .send_bytes(deadline, bytes)
            .expect("write to shell PTY");
    }

    /// The emulated terminal's represented modes, for comparing the state a
    /// command leaves behind with the state before it.
    pub(crate) fn terminal_modes(&self) -> String {
        format!("{:?}", self.term.terminal_state().modes)
    }

    /// Sends bytes the shell handles without repainting (for example Ctrl+P).
    pub(crate) fn keys_silent(&mut self, bytes: &[u8]) {
        let deadline = self.deadline();
        self.term
            .send_bytes(deadline, bytes)
            .expect("write to shell PTY");
        self.settle();
    }

    pub(crate) fn text(&mut self, text: &str) {
        self.keys(text.as_bytes());
    }

    /// Sends `bytes` repeated `count` times in one write, then settles once.
    pub(crate) fn repeat(&mut self, bytes: &[u8], count: usize) {
        self.keys(&bytes.repeat(count));
    }

    /// Types `text`, presses Enter and waits for the next prompt.
    pub(crate) fn line(&mut self, text: &str) {
        self.text(text);
        let deadline = self.deadline();
        self.term
            .send_bytes(deadline, b"\r")
            .expect("write to shell PTY");
        self.wait_prompt();
    }

    /// Presses Enter without expecting a new prompt (for continuation lines).
    pub(crate) fn enter(&mut self) {
        self.keys(b"\r");
    }

    /// Sends bytes and waits for the next prompt cycle instead of quiescence.
    pub(crate) fn keys_to_prompt(&mut self, bytes: &[u8]) {
        let deadline = self.deadline();
        self.term
            .send_bytes(deadline, bytes)
            .expect("write to shell PTY");
        self.wait_prompt();
    }

    /// Waits until the visible screen contains `needle`.
    pub(crate) fn wait_screen_contains(&mut self, needle: &str) {
        let deadline = self.deadline();
        self.term
            .wait_for_screen(deadline, format!("screen contains {needle:?}"), |screen| {
                screen.contains(needle)
            })
            .expect("wait for screen text");
        self.settle();
    }

    /// Waits until the visible screen satisfies `predicate`.
    pub(crate) fn wait_screen(
        &mut self,
        description: &str,
        predicate: impl Fn(&ptytest::ScreenSnapshot) -> bool,
    ) {
        let deadline = self.deadline();
        self.term
            .wait_for_screen(deadline, description, predicate)
            .expect("wait for screen condition");
        self.settle();
    }

    /// Waits until the visible screen no longer contains `needle`.
    pub(crate) fn wait_until_absent(&mut self, needle: &str) {
        let deadline = self.deadline();
        self.term
            .wait_for_screen(deadline, format!("screen lacks {needle:?}"), |screen| {
                !screen.contains(needle)
            })
            .expect("wait for screen text to disappear");
        self.settle();
    }

    /// Types `text` and Enter, then waits for `needle` on screen instead of a
    /// new prompt (for commands that keep running).
    pub(crate) fn line_until(&mut self, text: &str, needle: &str) {
        self.text(text);
        let deadline = self.deadline();
        self.term
            .send_bytes(deadline, b"\r")
            .expect("write to shell PTY");
        self.wait_screen_contains(needle);
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        self.term
            .resize(Size::new(cols, rows).expect("non-zero terminal size"))
            .expect("resize");
        self.settle();
    }

    /// Records the current screen as a labelled frame: visible text, cursor
    /// position and visibility, and every run of styled cells.
    pub(crate) fn frame(&mut self, label: &str) {
        let screen = self.term.screen();
        let mut rows: Vec<String> = (0..screen.row_count()).map(|row| screen.row(row).unwrap_or_default()).collect();
        let wrapped: Vec<bool> = (0..screen.row_count()).map(|row| screen.row_wrapped(row).unwrap_or(false)).collect();
        normalize_diagnostic_brand_wraps(&mut rows, &wrapped, usize::from(screen.size().columns()));
        for row in &mut rows { *row = self.normalize(row.trim_end()); }
        let mut styles = Vec::new();
        for row in 0..screen.row_count() {
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
                    Some((current, contents)) if *current == text => {
                        contents.push_str(cell.contents())
                    }
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
    pub(crate) fn frame_rows_containing(&mut self, label: &str, needle: &str) {
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
            .replace(".cache/xshi/", ".cache/ish/")
            .replace(".config/xshi", ".config/ish")
            .replace(".local/share/xshi", ".local/share/ish");
        normalize_dump_names(&normalize_pgid(&normalize_ls_row(&normalize_shell_name(
            &normalize_pty_paths(&text),
        ))))
    }

    /// Records how many layout dumps the shell has written.
    pub(crate) fn effect_dump_count(&mut self, name: &str) {
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
    pub(crate) fn effect_dumps(&mut self, name: &str) {
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
                    .map(|line| {
                        if line.starts_with("version: ") {
                            "version: <VERSION>"
                        } else {
                            line
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some((modified, self.normalize(&normalize_completion_arena(&text))))
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
    pub(crate) fn effect_file(&mut self, name: &str, relative: &str) {
        let text = match std::fs::read(self.home.join(relative)) {
            Ok(bytes) => self.normalize(&String::from_utf8_lossy(&bytes)),
            Err(error) => format!("<{:?}>", error.kind()),
        };
        self.effects.push((name.to_owned(), text));
    }

    /// Records the shell's history file with timestamps and session ids
    /// replaced by placeholders.
    pub(crate) fn effect_history(&mut self, name: &str) {
        let path = self.kind.history_file();
        let text = match std::fs::read(self.home.join(&path)) {
            Ok(bytes) => normalize_history(&self.normalize(&String::from_utf8_lossy(&bytes))),
            Err(error) => format!("<{:?}>", error.kind()),
        };
        self.effects.push((name.to_owned(), text));
    }

    /// Replaces a file under HOME and stamps it `seconds` into the future, the
    /// way a later edit would look to mtime-based trust, without sleeping.
    pub(crate) fn rewrite_file(&mut self, relative: &str, body: &str, seconds: u64) {
        let path = self.home.join(relative);
        std::fs::write(&path, body).expect("rewrite file");
        let file = std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("open rewritten file");
        file.set_modified(std::time::SystemTime::now() + Duration::from_secs(seconds))
            .expect("set future mtime");
    }

    /// Records how many times `needle` appears in the raw terminal stream.
    pub(crate) fn effect_raw_count(&mut self, name: &str, needle: &[u8]) {
        let count = self
            .term
            .raw_output()
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count();
        self.effects.push((name.to_owned(), count.to_string()));
    }

    /// Records the mode of the layout-dump directory and of every dump file in
    /// it, so the privacy contract is visible in the golden.
    pub(crate) fn effect_dump_privacy(&mut self, name: &str) {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = self.home.join(".cache").join(self.kind.label());
        let mode = |path: &Path| {
            std::fs::metadata(path)
                .map(|metadata| format!("{:03o}", metadata.permissions().mode() & 0o777))
                .unwrap_or_else(|error| format!("<{:?}>", error.kind()))
        };
        let mut lines = vec![format!("dir {}", mode(&dir))];
        let mut dumps: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("dump-"))
            })
            .collect();
        dumps.sort();
        for dump in dumps {
            lines.push(format!("file {}", mode(&dump)));
        }
        self.effects.push((name.to_owned(), lines.join("\n")));
    }

    /// Records the sorted names, kinds and byte lengths of entries under a
    /// HOME directory, one path per line, so tree transfers are visible.
    pub(crate) fn effect_tree(&mut self, name: &str, relative: &str) {
        fn walk(base: &Path, directory: &Path, lines: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                lines.push(format!("<unreadable> {relative}", relative = directory.display()));
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let kind = match entry.file_type() {
                    Ok(kind) if kind.is_dir() => "dir",
                    Ok(kind) if kind.is_symlink() => "link",
                    _ => "file",
                };
                let relative = path.strip_prefix(base).unwrap_or(&path);
                let length = std::fs::symlink_metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or_default();
                lines.push(format!("{kind} {length} {}", relative.display()));
                if kind == "dir" {
                    walk(base, &path, lines);
                }
            }
        }
        let base = self.home.join(relative);
        let mut lines = Vec::new();
        if base.is_dir() {
            walk(&self.home, &base, &mut lines);
        } else if base.exists() {
            let length = std::fs::symlink_metadata(&base)
                .map(|metadata| metadata.len())
                .unwrap_or_default();
            lines.push(format!("file {length} {relative}"));
        } else {
            lines.push("<absent>".to_owned());
        }
        lines.sort();
        self.effects.push((name.to_owned(), lines.join("\n")));
    }

    /// Records `text` under `name` as a persistent effect.
    pub(crate) fn effect(&mut self, name: &str, text: &str) {
        self.effects.push((name.to_owned(), text.to_owned()));
    }

    /// Waits until a native transfer is filling a staged tree: a
    /// `.fdu-quarantine-v2-*` session under HOME holds a non-empty `*.entry`
    /// directory, the condition ish's own native filesystem tests wait for.
    /// Returns that staging path so a scenario can check it was cleaned up.
    pub(crate) fn wait_for_staging(&mut self) -> PathBuf {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let _ = self.term.drain(self.term.deadline(Duration::from_millis(5)));
            if let Some(stage) = staged_entry(&self.home) {
                return stage;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{}: native filesystem staging did not begin; screen:\n{}",
                self.kind.label(),
                self.term.screen()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Records the sorted names of entries directly under a HOME directory.
    pub(crate) fn effect_listing(&mut self, name: &str, relative: &str) {
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

    /// Waits for the shell to exit, asserts the terminal modes were restored
    /// whatever the status, and records the status as an effect.
    pub(crate) fn expect_exit_restored(&mut self, name: &str) {
        let deadline = self.deadline();
        let status = self
            .term
            .wait_for_exit(deadline)
            .expect("wait for shell exit");
        self.term
            .assert_terminal_restored(&self.baseline)
            .expect("exit restores represented terminal modes");
        self.exited = Some(status);
        self.effects.push((name.to_owned(), format!("{status:?}")));
    }

    /// Waits for the shell to exit and records its exit status as an effect.
    pub(crate) fn expect_exit(&mut self, name: &str) {
        let deadline = self.deadline();
        let status = self
            .term
            .wait_for_exit(deadline)
            .expect("wait for shell exit");
        if status == ExitStatus::Code(0) {
            self.term
                .assert_terminal_restored(&self.baseline)
                .expect("exit restores represented terminal modes");
        }
        self.exited = Some(status);
        self.effects.push((name.to_owned(), format!("{status:?}")));
    }

    /// The visible rows, trailing blanks trimmed, without terminal metadata.
    pub(crate) fn screen_text(&self) -> String {
        let screen = self.term.screen();
        let rows: Vec<String> = (0..screen.row_count())
            .map(|row| self.normalize(screen.row(row).unwrap_or_default().trim_end()))
            .collect();
        rows.join("\n").trim_end().to_owned()
    }

    fn finish(mut self) -> Transcript {
        let deadline = self.term.deadline(Duration::from_secs(3));
        let _ = self.term.finish(deadline);
        Transcript {
            frames: self.frames,
            effects: self.effects,
        }
    }
}

/// The first non-empty `*.entry` staging directory inside a journal session
/// under `home`.
fn staged_entry(home: &Path) -> Option<PathBuf> {
    for session in std::fs::read_dir(home).ok()?.flatten() {
        if !session
            .file_name()
            .to_string_lossy()
            .starts_with(".fdu-quarantine-v2-")
        {
            continue;
        }
        for entry in std::fs::read_dir(session.path()).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().ends_with(".entry")
                && entry.file_type().is_ok_and(|kind| kind.is_dir())
                && std::fs::read_dir(entry.path()).is_ok_and(|mut entries| entries.next().is_some())
            {
                return Some(entry.path());
            }
        }
    }
    None
}

fn parse_json_string(encoded: &str) -> Option<String> {
    let mut chars = encoded.strip_prefix('"')?.strip_suffix('"')?.chars();
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).ok()?;
                    out.push(char::from_u32(code)?);
                }
                _ => return None,
            },
            ch => out.push(ch),
        }
    }
    Some(out)
}

fn render_json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if u32::from(ch) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(ch));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

// Completion entries are sorted for display, but their arena offsets retain
// filesystem enumeration order. Compare the same entries with packed offsets.
fn normalize_completion_arena(text: &str) -> String {
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    for index in 0..lines.len().saturating_sub(1) {
        let Some(encoded) = lines[index].strip_prefix("completion.names: ") else {
            continue;
        };
        let Some(names) = parse_json_string(encoded) else {
            continue;
        };
        let mut canonical_names = String::new();
        let mut entries = String::new();
        let mut rest = lines[index + 1].as_str();
        while let Some((prefix, fields)) = rest.split_once("name_start: ") {
            entries.push_str(prefix);
            entries.push_str("name_start: ");
            let (start, fields) = fields
                .split_once(", name_len: ")
                .expect("completion offset");
            let (len, tail) = fields.split_once(',').expect("completion length");
            let start = start.parse::<usize>().expect("numeric completion offset");
            let len = len.parse::<usize>().expect("numeric completion length");
            entries.push_str(&canonical_names.len().to_string());
            entries.push_str(", name_len: ");
            entries.push_str(&len.to_string());
            entries.push(',');
            canonical_names.push_str(&names[start..start + len]);
            rest = tail;
        }
        entries.push_str(rest);
        lines[index] = format!(
            "completion.names: {}",
            render_json_string(&canonical_names)
        );
        lines[index + 1] = entries;
    }
    lines.join("\n")
}

#[test]
fn completion_dump_ignores_directory_enumeration_order() {
    let forward = "completion.names: \"aaab\"\ncompletion.entries: [CompletionEntry { name_start: 0, name_len: 2, flags: 0 }, CompletionEntry { name_start: 2, name_len: 2, flags: 0 }]";
    let reverse = "completion.names: \"abaa\"\ncompletion.entries: [CompletionEntry { name_start: 2, name_len: 2, flags: 0 }, CompletionEntry { name_start: 0, name_len: 2, flags: 0 }]";
    assert_eq!(normalize_completion_arena(reverse), forward);
    assert_eq!(normalize_completion_arena(forward), forward);
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

/// `l` rows carry account names, directory link counts, directory and symlink
/// sizes, and size-column padding that depend on the host filesystem.
/// Keep regular file link counts and sizes, modes, dates, names, and targets.
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
    if !mode_ok || (nlink != "<NLINK>" && nlink.parse::<u64>().is_err()) {
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
    let rest = row[end..].trim_start();
    let size_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let size = if mode.starts_with(['d', 'l']) {
        "<SIZE>"
    } else {
        &rest[..size_end]
    };
    let nlink = if mode.starts_with('d') {
        "<NLINK>"
    } else {
        nlink
    };
    let remainder = rest[size_end..].trim_start();
    format!("{mode} {nlink} <USER> <GROUP> {size} {remainder}")
}

fn normalize_completion_entry_offsets(row: &str) -> String {
    if !row.starts_with("completion.entries: [CompletionEntry {") { return row.to_string(); }
    let mut packed = String::new();
    let mut rest = row;
    let mut offset = 0;
    while let Some((before, fields)) = rest.split_once("name_start: ") {
        let Some((_, fields)) = fields.split_once(", name_len: ") else { return row.to_string(); };
        let Some((length, tail)) = fields.split_once(',') else { return row.to_string(); };
        let Ok(length) = length.parse::<usize>() else { return row.to_string(); };
        packed.push_str(before);
        packed.push_str(&format!("name_start: {offset}, name_len: {length},"));
        offset += length;
        rest = tail;
    }
    packed.push_str(rest);
    packed
}

#[test]
fn completion_entry_offsets_ignore_only_arena_storage_order() {
    let row = "completion.entries: [CompletionEntry { mtime: 1, name_start: 2, name_len: 2, name_display_width: 2, flags: 0 }, CompletionEntry { mtime: 1, name_start: 0, name_len: 2, name_display_width: 2, flags: 0 }]";
    let normalized = normalize_completion_entry_offsets(row);
    assert!(normalized.contains("name_start: 0, name_len: 2"));
    assert!(normalized.contains("name_start: 2, name_len: 2"));
    assert_eq!(normalize_completion_entry_offsets(&normalized), normalized);
    assert_ne!(normalize_completion_entry_offsets(&row.replace("flags: 0", "flags: 1")), normalized);
    assert_ne!(normalize_completion_entry_offsets(&row.replace("name_len: 2", "name_len: 3")), normalized);
    assert_ne!(normalize_completion_entry_offsets(&row.replace("mtime: 1", "mtime: 2")), normalized);
}

fn normalize_listing_transcript(transcript: &str) -> String {
    let mut out = String::with_capacity(transcript.len());
    for chunk in transcript.split_inclusive('\n') {
        let (line, ending) = chunk
            .strip_suffix('\n')
            .map_or((chunk, ""), |line| (line, "\n"));
        if let Some(row) = line.strip_prefix('|') {
            out.push('|');
            out.push_str(&normalize_completion_entry_offsets(&normalize_ls_row(row)));
        } else {
            out.push_str(line);
        }
        out.push_str(ending);
    }
    out
}

/// A command that opens its controlling terminal prints the PTY's name; each
/// run gets a fresh `/dev/pts/N`.
fn normalize_pty_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("/dev/pts/") {
        out.push_str(&rest[..index]);
        let after = &rest[index + "/dev/pts/".len()..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            out.push_str("/dev/pts/");
            rest = after;
        } else {
            out.push_str("/dev/pts/<N>");
            rest = &after[digits..];
        }
    }
    out.push_str(rest);
    out
}

/// Message prefixes name the shell (`ish: cd: ...` / `xshi: cd: ...`); that
/// identity is the one intentional textual difference between the two.
fn normalize_diagnostic_brand_wraps(rows: &mut [String], wrapped: &[bool], columns: usize) {
    let mut index = 0;
    while index < rows.len() {
        let Some(prefix) = ["xshi:", "epsh:"].into_iter().find(|prefix| rows[index].starts_with(prefix)) else {
            index += 1;
            continue;
        };
        let mut end = index;
        while end + 1 < rows.len() && wrapped[end] { end += 1; }
        let text = rows[index..=end].concat();
        if text.is_ascii() {
            let text = format!("ish:{}", &text[prefix.len()..]);
            let parts: Vec<String> = text.as_bytes().chunks(columns.max(1)).map(|part| String::from_utf8(part.to_vec()).unwrap()).collect();
            if parts.len() == end - index + 1 {
                rows[index..=end].clone_from_slice(&parts);
            }
        }
        index = end + 1;
    }
}

#[test]
fn diagnostic_brand_normalization_reflows_only_the_shell_prefix() {
    let message = "exit changes shell state or needs the ish process, so it must be the only command on its line";
    let source = format!("xshi: {message}");
    let mut rows: Vec<String> = source.as_bytes().chunks(80).map(|part| String::from_utf8(part.to_vec()).unwrap()).collect();
    normalize_diagnostic_brand_wraps(&mut rows, &[true, false], 80);
    assert_eq!(rows.concat(), format!("ish: {message}"));
    assert!(rows[0].ends_with("only c"));
    let mut user_rows = vec!["user text xshi: stays exactly as displayed".to_string(), "wrapped continuation".to_string()];
    let original = user_rows.clone();
    normalize_diagnostic_brand_wraps(&mut user_rows, &[true, false], 80);
    assert_eq!(user_rows, original);
}

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
            ["xshi:", "epsh:", "ish:"]
                .into_iter()
                .find(|name| rest.starts_with(name))
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
                (Some(_), Some(_), Some(cwd), Some(command)) => {
                    format!("v2 <TS> <SESSION> {cwd} {command}")
                }
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
        .ancestors()
        .nth(2)
        .expect("workspace root above crates/xshi")
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
                let end = if full {
                    lines.len()
                } else {
                    (index + 8).min(lines.len())
                };
                lines[start.min(lines.len())..end].join("\n")
            };
            let _ = writeln!(
                context,
                "--- expected\n{}\n--- actual\n{}\n---",
                window(&expected_lines),
                window(&actual_lines)
            );
            return context;
        }
    }
    String::new()
}

fn ish_binary() -> Option<PathBuf> {
    std::env::var_os("XSHI_PARITY_ISH_BIN")
        .map(PathBuf::from)
        .inspect(|path| {
            assert!(
                path.exists(),
                "XSHI_PARITY_ISH_BIN does not exist: {}",
                path.display()
            );
        })
}

fn run_shell(kind: ShellKind, binary: &Path, fixture: &Fixture, script: fn(&mut Live)) -> String {
    let mut live = Live::spawn(kind, binary, fixture);
    script(&mut live);
    live.finish().render()
}

/// Runs `script` against `xshi` alone, for behavior the reference shell does
/// not have; the script asserts on the live screen itself.
pub(crate) fn run_xshi_only(fixture: &Fixture, script: impl FnOnce(&mut Live)) {
    let xshi = PathBuf::from(cargo_env!("CARGO_BIN_EXE_xshi"));
    let mut live = Live::spawn(ShellKind::Xshi, &xshi, fixture);
    script(&mut live);
    let _ = live.finish();
}

/// Runs `script` against `xshi` and compares with the recorded ish golden.
pub(crate) fn run_scenario(name: &str, fixture: &Fixture, script: fn(&mut Live)) {
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
            let expected = normalize_listing_transcript(&expected);
            let from_ish = normalize_listing_transcript(&from_ish);
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
    let expected = normalize_listing_transcript(&expected);
    let actual = normalize_listing_transcript(&run_shell(ShellKind::Xshi, &xshi, fixture, script));
    if known_gap(name) {
        assert!(
            expected != actual,
            "{name}: xshi now matches the ish golden; remove it from {}",
            known_gaps_path().display()
        );
        return;
    }
    assert!(
        expected == actual,
        "{name}: xshi differs from the ish golden\n{}",
        first_difference(&expected, &actual)
    );
}

fn known_gaps_path() -> PathBuf {
    Path::new(cargo_env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/interactive-parity/known-gaps.txt")
}

fn known_gap(name: &str) -> bool {
    let path = known_gaps_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    text.lines().any(|line| {
        let id = line.split_once('\t').map_or(line, |(id, _cause)| id);
        id.trim() == name
    })
}

#[test]
fn history_search_survives_a_peer_shell_resetting_history() {
    run_xshi_only(&Fixture::new().history(&["echo old"]), |sh| {
        let mut peer = sh.spawn_peer();
        peer.line("history reset");
        sh.enter();
        sh.wait_prompt();
        sh.keys(key::CTRL_R);
        assert!(
            !sh.screen_text().contains("echo old"),
            "{}",
            sh.screen_text()
        );
        sh.keys(key::ESC);
        sh.absorb(peer, "peer");
    });
}

#[test]
fn dollar_completes_shell_variables_exported_or_not() {
    run_xshi_only(&Fixture::new().env("XSHI_ZEBRA_ONE", "1"), |sh| {
        sh.line("set XSHI_ZEBRA_TWO 2");
        sh.line("XSHI_ZEBRA_SH=3");

        sh.text("echo $XSHI_Z");
        sh.keys(key::TAB);
        let grid = sh.screen_text();
        for name in ["XSHI_ZEBRA_ONE", "XSHI_ZEBRA_SH", "XSHI_ZEBRA_TWO"] {
            assert!(grid.contains(name), "{name} missing from the grid:\n{grid}");
        }

        sh.text("EBRA_S");
        sh.keys(key::TAB);
        sh.keys(key::ENTER);
        let row = sh
            .screen_text()
            .lines()
            .rev()
            .find(|row| row.contains("echo"))
            .map(str::to_owned);
        assert!(
            row.as_deref()
                .is_some_and(|row| row.ends_with("echo $XSHI_ZEBRA_SH")),
            "accepted without quoting or a trailing slash: {row:?}"
        );
        sh.keys_to_prompt(key::ENTER);
        assert!(sh.screen_text().contains("\n3\n"), "{}", sh.screen_text());
    });
}

#[test]
fn dollar_completion_works_inside_double_quotes_and_ignores_escaped_dollars() {
    run_xshi_only(&Fixture::new().env("XSHI_QUOTED_VALUE", "ok"), |sh| {
        sh.text("echo \"value=$XSHI_QUOTED_V");
        sh.keys(key::TAB);
        let row = sh
            .screen_text()
            .lines()
            .rev()
            .find(|row| row.contains("echo"))
            .map(str::to_owned);
        assert!(
            row.as_deref()
                .is_some_and(|row| row.ends_with("echo \"value=$XSHI_QUOTED_VALUE")),
            "{row:?}"
        );
        sh.keys(key::CTRL_U);

        sh.text("echo \\$XSHI_QUOTED_V");
        sh.keys(key::TAB);
        assert!(
            !sh.screen_text().contains("XSHI_QUOTED_VALUE"),
            "{}",
            sh.screen_text()
        );
    });
}

#[test]
fn forced_exit_terminates_a_stopped_job() {
    let fixture = Fixture::new().executable(
        "bin/job",
        "#!/bin/sh\necho $$ > pid\necho ready\nsleep 60\n",
    );
    run_xshi_only(&fixture, |sh| {
        sh.line_until("./bin/job", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        let pid = std::fs::read_to_string(sh.home().join("pid")).expect("job wrote its pid");
        let pid = pid.trim().to_owned();

        sh.text("exit");
        sh.keys_to_prompt(key::ENTER);
        assert!(
            sh.screen_text().contains("suspended job"),
            "{}",
            sh.screen_text()
        );
        sh.text("exit");
        sh.enter();
        sh.expect_exit("forced_exit");

        let alive = || {
            std::process::Command::new("kill")
                .args(["-0", &pid])
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while alive() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(), "the stopped job outlived the shell");
    });
}
