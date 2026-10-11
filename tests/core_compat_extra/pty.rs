use super::{applet, Running};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Output, Stdio};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

struct Terminal {
    master: File,
    slave: File,
}

impl Terminal {
    fn new(rows: u16, columns: u16) -> Self {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: rows, ws_col: columns,
            ws_xpixel: columns * 8, ws_ypixel: rows * 10,
        };
        assert_eq!(unsafe {
            libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size)
        }, 0, "open terminal: {}", std::io::Error::last_os_error());
        let terminal = Self {
            master: unsafe { File::from_raw_fd(master) },
            slave: unsafe { File::from_raw_fd(slave) },
        };
        for file in [&terminal.master, &terminal.slave] {
            assert_eq!(unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) }, 0);
        }
        terminal
    }

    fn stdio(&self) -> Stdio {
        self.slave.try_clone().unwrap().into()
    }
}

// Linux returns EIO after the final slave closes; other Unix hosts return EOF.
// Poll bounds the capture even if an inherited descriptor unexpectedly stays open.
fn terminal_bytes(mut master: File) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut bytes = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "terminal capture did not close");
        let mut event = libc::pollfd { fd: master.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut event, 1, remaining.as_millis() as i32) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted { continue; }
            panic!("poll terminal: {error}");
        }
        assert!(ready > 0, "terminal capture timed out");
        let mut buffer = [0; 4096];
        match master.read(&mut buffer) {
            Ok(0) => return bytes,
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error) if error.raw_os_error() == Some(libc::EIO) => return bytes,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("read terminal: {error}"),
        }
    }
}

fn terminal_run(mut command: Command, streams: [bool; 3], size: (u16, u16), input: &[u8], write_after_spawn: bool) -> Output {
    let mut terminals: Vec<Option<Terminal>> = streams.into_iter()
        .map(|enabled| enabled.then(|| Terminal::new(size.0, size.1))).collect();
    if let Some(terminal) = &terminals[0] { command.stdin(terminal.stdio()); }
    if let Some(terminal) = &terminals[1] { command.stdout(terminal.stdio()); }
    if let Some(terminal) = &terminals[2] { command.stderr(terminal.stdio()); }
    if !write_after_spawn {
        if let Some(terminal) = &mut terminals[0] { terminal.master.write_all(input).unwrap(); }
    }
    let child = Running::spawn(&mut command);
    drop(command);
    // Only the child keeps the slaves alive during capture.
    let mut masters: Vec<Option<File>> = terminals.into_iter().map(|terminal| {
        terminal.map(|Terminal { master, slave }| { drop(slave); master })
    }).collect();
    if let Some(master) = &mut masters[0] {
        if write_after_spawn { master.write_all(input).unwrap(); }
        // Canonical input needs a completed line before its EOF character.
        master.write_all(b"\n\x04").unwrap();
    }
    let mut output = child.finish();
    if let Some(master) = masters[1].take() { output.stdout = terminal_bytes(master); }
    if let Some(master) = masters[2].take() { output.stderr = terminal_bytes(master); }
    output
}

#[derive(Debug, PartialEq, Eq)]
struct TermiosSnapshot {
    input: libc::tcflag_t,
    output: libc::tcflag_t,
    control: libc::tcflag_t,
    local: libc::tcflag_t,
    speeds: (libc::speed_t, libc::speed_t),
    control_chars: Vec<libc::cc_t>,
}

fn termios_snapshot(fd: libc::c_int) -> TermiosSnapshot {
    let mut attrs = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(unsafe { libc::tcgetattr(fd, &mut attrs) }, 0, "read terminal attributes");
    TermiosSnapshot {
        input: attrs.c_iflag,
        output: attrs.c_oflag,
        control: attrs.c_cflag,
        local: attrs.c_lflag,
        speeds: (unsafe { libc::cfgetispeed(&attrs) }, unsafe { libc::cfgetospeed(&attrs) }),
        control_chars: attrs.c_cc.to_vec(),
    }
}

fn write_btop_frame(root: &Path, index: usize, counters: [u64; 4], receive: u64, transmit: u64) {
    let proc = root.join(format!("frames/{index}/proc"));
    fs::create_dir_all(proc.join("net")).unwrap();
    let [user, system, idle, _other] = counters;
    let stat = format!(
        "cpu {user} 0 {system} {idle} 0 0 0 0 0 0\ncpu0 {user} 0 {system} {idle} 0 0 0 0 0 0\nbtime 1700000000\nprocesses 1\nprocs_running 1\nprocs_blocked 0\n"
    );
    fs::write(proc.join("stat"), stat).unwrap();
    fs::write(proc.join("uptime"), "25.50 100.00\n").unwrap();
    fs::write(proc.join("loadavg"), "0.10 0.20 0.30 1/100 42\n").unwrap();
    fs::write(
        proc.join("meminfo"),
        "MemTotal: 1048576 kB\nMemFree: 131072 kB\nMemAvailable: 262144 kB\nBuffers: 32768 kB\nCached: 65536 kB\nActive: 524288 kB\nInactive: 131072 kB\nDirty: 128 kB\nWriteback: 0 kB\n",
    ).unwrap();
    fs::write(
        proc.join("net/dev"),
        format!(
            "Inter-| Receive                                                | Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\neth0: {receive} 100 0 0 0 0 0 0 {transmit} 100 0 0 0 0 0 0\n"
        ),
    ).unwrap();
}

fn write_btop_bundle(root: &Path, frames: &[([u64; 4], u64, u64, u64)]) {
    for (index, (counters, receive, transmit, _at_ns)) in frames.iter().enumerate() {
        write_btop_frame(root, index, *counters, *receive, *transmit);
    }
    let frame_metadata = frames
        .iter()
        .map(|(_, _, _, at_ns)| format!("{{\"at_ns\":{at_ns}}}"))
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        root.join("manifest.json"),
        format!(
            "{{\"schema\":1,\"page_size_bytes\":4096,\"clock_ticks_per_second\":100,\"source_id\":\"pty-fixture\",\"frames\":[{frame_metadata}]}}\n"
        ),
    ).unwrap();
}

fn btop_applet(directory: &Path, bundle: &Path, frame_limit: usize) -> Command {
    let mut command = applet("btop", directory);
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    command
        .env(
            "XSH_MODULE_PATH",
            format!("{}:{}", repository.display(), repository.join("core").display()),
        )
        .args([
            "--replay",
            bundle.to_str().expect("fixture path is UTF-8"),
            "--frames",
            &frame_limit.to_string(),
        ]);
    command
}

fn set_terminal_size(master_fd: libc::c_int, rows: u16, columns: u16) {
    let size = libc::winsize {
        ws_row: rows,
        ws_col: columns,
        ws_xpixel: columns * 8,
        ws_ypixel: rows * 10,
    };
    assert_eq!(unsafe { libc::ioctl(master_fd, libc::TIOCSWINSZ, &size) }, 0, "resize terminal");
}

fn append_terminal_output(master: &mut File, output: &mut Vec<u8>) {
    let mut buffer = [0; 8192];
    loop {
        match master.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => output.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => return,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("read terminal output: {error}"),
        }
    }
}

fn wait_for_terminal_output(master: &mut File, output: &mut Vec<u8>, timeout: Duration, condition: impl Fn(&[u8]) -> bool) {
    let deadline = Instant::now() + timeout;
    while !condition(output) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let start = output.len().saturating_sub(3000);
            panic!("timed out waiting for terminal output; captured tail: {}", String::from_utf8_lossy(&output[start..]));
        }
        let mut event = libc::pollfd { fd: master.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut event, 1, remaining.as_millis().min(100) as i32) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted { continue; }
            panic!("poll terminal output: {error}");
        }
        if ready > 0 {
            append_terminal_output(master, output);
        }
    }
}

fn terminal_screen_line_counts(output: &[u8]) -> Vec<usize> {
    let home = b"\x1b[H";
    let mut counts = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = output[offset..].windows(home.len()).position(|window| window == home) {
        let content_start = offset + relative_start + home.len();
        let content_end = output[content_start..]
            .iter()
            .position(|byte| *byte == 0x1b)
            .map_or(output.len(), |relative_end| content_start + relative_end);
        counts.push(output[content_start..content_end].iter().filter(|byte| **byte == b'\n').count() + 1);
        offset = content_end;
        if offset == output.len() { break; }
    }
    counts
}

fn set_master_nonblocking(master: &File) {
    let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0, "read terminal flags");
    assert_eq!(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }, 0);
}

#[test]
fn btop_replay_updates_after_resize_and_quits_with_restored_terminal() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([130, 60, 910, 0], 1_005_120, 501_024, 500_000_000),
            ([160, 70, 970, 0], 1_010_240, 502_048, 5_000_000_000),
        ],
    );
    let terminal = Terminal::new(24, 80);
    set_master_nonblocking(&terminal.master);
    let original_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    let mut command = btop_applet(directory.path(), &bundle, 3);
    command.stdin(terminal.stdio()).stdout(terminal.stdio()).stderr(Stdio::piped());
    let child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();

    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(3), |bytes| {
        bytes.windows(b"CPU warming".len()).any(|window| window == b"CPU warming")
    });
    let active_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    assert_eq!(active_attrs.local & libc::ICANON, 0, "btop must enter raw mode");
    assert_eq!(active_attrs.local & libc::ISIG, 0, "raw Ctrl-C must arrive as a byte");
    set_terminal_size(terminal.master.as_raw_fd(), 30, 100);

    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(3), |bytes| {
        let screens = terminal_screen_line_counts(bytes);
        screens.len() >= 3
            && screens.contains(&30)
            && bytes.windows(b"Usage 40.0%".len()).any(|window| window == b"Usage 40.0%")
    });
    terminal.master.try_clone().unwrap().write_all(b"q").unwrap();
    let result = child.finish();
    append_terminal_output(&mut terminal.master.try_clone().unwrap(), &mut output);

    assert!(result.status.success(), "btop exited with {}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    let screens = terminal_screen_line_counts(&output);
    assert!(screens.contains(&24), "initial frame was not 24 rows: {screens:?}");
    assert!(screens.contains(&30), "resized frame was not 30 rows: {screens:?}");
    assert!(screens.len() >= 3, "expected resize redraw and a second observation: {screens:?}");
    assert_eq!(termios_snapshot(terminal.slave.as_raw_fd()), original_attrs);
    assert!(output.windows(b"\x1b[0m".len()).any(|window| window == b"\x1b[0m"));
    assert!(output.windows(b"\x1b[?25h".len()).any(|window| window == b"\x1b[?25h"));
}

#[test]
fn btop_replay_ctrl_c_byte_exits_and_restores_terminal() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([160, 70, 970, 0], 1_010_240, 502_048, 5_000_000_000),
        ],
    );
    let terminal = Terminal::new(24, 80);
    set_master_nonblocking(&terminal.master);
    let original_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    let mut command = btop_applet(directory.path(), &bundle, 2);
    command.stdin(terminal.stdio()).stdout(terminal.stdio()).stderr(Stdio::piped());
    let child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();

    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(3), |bytes| {
        bytes.windows(b"CPU warming".len()).any(|window| window == b"CPU warming")
    });
    let active_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    assert_eq!(active_attrs.local & libc::ISIG, 0, "raw Ctrl-C must not raise SIGINT");
    terminal.master.try_clone().unwrap().write_all(b"\x03").unwrap();
    let result = child.finish();
    append_terminal_output(&mut terminal.master.try_clone().unwrap(), &mut output);

    assert!(result.status.success(), "Ctrl-C byte should exit cleanly: {}", result.status);
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(terminal.slave.as_raw_fd()), original_attrs);
    assert!(output.windows(b"\x1b[0m".len()).any(|window| window == b"\x1b[0m"));
    assert!(output.windows(b"\x1b[?25h".len()).any(|window| window == b"\x1b[?25h"));
}

fn tty_case(streams: [bool; 3], size: (u16, u16), stdout: &[u8], stderr: &[u8]) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("is_a_tty.sh"), r#"for descriptor in '0 stdin' '1 stdout' '2 stderr'; do
    set -- $descriptor
    if test -t "$1"; then
        printf '%s is a tty\n' "$2"
        if test "$1" = 0; then
            printf 'terminal size: '
            stty size
        fi
    else
        printf '%s is not a tty\n' "$2"
    fi
done
printf 'This is an error message.\n' >&2
exit 0
"#).unwrap();
    let mut command = applet("env", directory.path());
    command.args(["sh", "is_a_tty.sh"]);
    let output = terminal_run(command, streams, size, b"", false);
    assert!(output.status.success());
    assert_eq!(output.stdout, stdout);
    assert_eq!(output.stderr, stderr);
}

// origin: uutils test_env::test_simulation_of_terminal_true
#[test]
fn env_terminal_all_streams() {
    tty_case([true; 3], (30, 80), b"stdin is a tty\r\nterminal size: 30 80\r\nstdout is a tty\r\nstderr is a tty\r\n", b"This is an error message.\r\n");
}

// origin: uutils test_env::test_simulation_of_terminal_for_stdin_only
#[test]
fn env_terminal_stdin_only() {
    tty_case([true, false, false], (30, 80), b"stdin is a tty\nterminal size: 30 80\nstdout is not a tty\nstderr is not a tty\n", b"This is an error message.\n");
}

// origin: uutils test_env::test_simulation_of_terminal_for_stdout_only
#[test]
fn env_terminal_stdout_only() {
    tty_case([false, true, false], (30, 80), b"stdin is not a tty\r\nstdout is a tty\r\nstderr is not a tty\r\n", b"This is an error message.\n");
}

// origin: uutils test_env::test_simulation_of_terminal_for_stderr_only
#[test]
fn env_terminal_stderr_only() {
    tty_case([false, false, true], (30, 80), b"stdin is not a tty\nstdout is not a tty\nstderr is a tty\n", b"This is an error message.\r\n");
}

// origin: uutils test_env::test_simulation_of_terminal_size_information
#[test]
fn env_terminal_dimensions() {
    tty_case([true; 3], (10, 40), b"stdin is a tty\r\nterminal size: 10 40\r\nstdout is a tty\r\nstderr is a tty\r\n", b"This is an error message.\r\n");
}

fn cat_terminal(input: &[u8], after_spawn: bool, expected: &[u8]) {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("env", directory.path());
    command.args(["cat", "-"]);
    let output = terminal_run(command, [true; 3], (30, 80), input, after_spawn);
    assert!(output.status.success());
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
}

// origin: uutils test_env::test_simulation_of_terminal_pty_sends_eot_automatically
#[test]
fn env_terminal_eot() { cat_terminal(b"", false, b"\r\n"); }

// origin: uutils test_env::test_simulation_of_terminal_pty_pipes_into_data_and_sends_eot_automatically
#[test]
fn env_terminal_forwarded_input_eot() {
    cat_terminal(b"Hello stdin forwarding!", false, b"Hello stdin forwarding!\r\n");
}

// origin: uutils test_env::test_simulation_of_terminal_pty_write_in_data_and_sends_eot_automatically
#[test]
fn env_terminal_written_input_eot() {
    cat_terminal(b"Hello stdin forwarding via write_in!", true, b"Hello stdin forwarding via write_in!\r\n");
}

// origin: uutils test_env::test_ignore_signal_pipe_broken_pipe_regression
#[test]
fn env_sigpipe_after_reader_closes() {
    let directory = tempfile::tempdir().unwrap();
    // Both images need the same producer's EPIPE exit policy. A bounded shell
    // fixture emits the original sequence and exits 1 when a write fails;
    // inherited default SIGPIPE still terminates it before that error path.
    fs::write(directory.path().join("seq"), br#"#!/bin/sh
[ "$#" -eq 2 ] && [ "$1" = 1 ] && [ "$2" = 1000000 ] || exit 2
value=$1
while [ "$value" -le "$2" ]; do
  printf '%s\n' "$value" || exit 1
  value=$((value + 1))
done
"#).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(directory.path().join("seq"), fs::Permissions::from_mode(0o755)).unwrap();
    let exits: Vec<i32> = [false, true].into_iter().map(|ignore| {
        let mut command = applet("env", directory.path());
        command.env("PATH", format!("{}:/bin:/usr/bin", directory.path().display()));
        if ignore { command.arg("--ignore-signal=PIPE"); }
        command.args(["seq", "1", "1000000"]).stderr(Stdio::null());
        // Rust ignores SIGPIPE in its own process; restore the normal inherited
        // disposition in this child without touching the parallel test runner.
        unsafe { command.pre_exec(|| {
            if libc::signal(libc::SIGPIPE, libc::SIG_DFL) == libc::SIG_ERR {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }); }
        let mut child = Running::spawn(&mut command);
        assert_eq!(child.prefix(2), b"1\n");
        drop(child.0.stdout.take().unwrap());
        let status = child.finish().status;
        status.code().unwrap_or_else(|| 128 + status.signal().unwrap())
    }).collect();
    assert_ne!(exits[1], 141);
    assert_eq!(exits[0], 141);
    assert!(exits[1] == 0 || exits[1] == 1, "producer exit statuses: {exits:?}");
}

// origin: uutils test_dd::diagnostics::test_plain_message_at_a_terminal_when_asked_for
#[test]
fn dd_plain_terminal_diagnostic() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("dd", directory.path());
    command.arg("bsx=1").env("UUTILS_DIAG", "never");
    let output = terminal_run(command, [false, false, true], (30, 80), b"", false);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap().replace("\r\n", "\n");
    assert!(stderr.contains("dd: unrecognized operand 'bsx=1'"));
    assert!(!stderr.contains('╭'));
}

fn expr_terminal(args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("expr", directory.path());
    command.args(args);
    terminal_run(command, [false, false, true], (30, 80), b"", false)
}

// origin: uutils test_expr::diagnostics::test_errors_without_a_position_stay_plain
#[test]
fn expr_terminal_errors_without_position() {
    let output = expr_terminal(&["6", "/", "0"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8(output.stderr).unwrap().replace("\r\n", "\n"), "expr: division by zero\n");
    let output = expr_terminal(&[]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("missing operand"));
    assert!(stderr.contains("for more information"));
}

// origin: uutils test_expr::diagnostics::test_computed_operand_stays_plain
#[test]
fn expr_terminal_computed_operand() {
    let output = expr_terminal(&["substr", "abc", "1", "2", "+", "1"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8(output.stderr).unwrap().replace("\r\n", "\n"), "expr: non-integer argument\n");
}

fn cp_prompt(input: &[u8], success: bool, expected: &[u8]) {
    let directory = tempfile::tempdir().unwrap();
    File::create(directory.path().join("old")).unwrap().set_modified(UNIX_EPOCH).unwrap();
    File::create(directory.path().join("new")).unwrap();
    let capture = File::create(directory.path().join("merged")).unwrap();
    let mut command = applet("cp", directory.path());
    command.args(["-i", "-v", "--update=older", "new", "old"])
        .stdin(Stdio::piped()).stdout(capture.try_clone().unwrap()).stderr(capture);
    let mut child = Running::spawn(&mut command);
    child.0.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.finish();
    assert_eq!(output.status.success(), success);
    assert_eq!(fs::read(directory.path().join("merged")).unwrap(), expected);
}

// origin: uutils test_cp::test_cp_update_older_interactive_prompt_yes
#[test]
fn cp_update_older_prompt_yes() { cp_prompt(b"Y\n", true, b"cp: overwrite 'old'? 'new' -> 'old'\n"); }

// origin: uutils test_cp::test_cp_update_older_interactive_prompt_no
#[test]
fn cp_update_older_prompt_no() { cp_prompt(b"N\n", false, b"cp: overwrite 'old'? "); }
