use super::{applet, Running};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Output, Stdio};
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

fn tty_case(streams: [bool; 3], size: (u16, u16), stdout: &[u8], stderr: &[u8]) {
    let directory = tempfile::tempdir().unwrap();
    fs::copy(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("core/tests/data/uutils/nohup/is_a_tty.sh"), directory.path().join("is_a_tty.sh")).unwrap();
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
    let exits: Vec<i32> = [false, true].into_iter().map(|ignore| {
        let mut command = applet("env", directory.path());
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
    assert!(exits[1] == 0 || exits[1] == 1);
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
