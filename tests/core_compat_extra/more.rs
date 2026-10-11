use super::{applet, Running};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

// An attached terminal without a controlling terminal must use concatenation.
// Keeping the master owned here lets every child be reaped before capture ends.
struct Terminal { master: File, slave: File }

impl Terminal {
    fn new() -> Self {
        let (mut master, mut slave) = (-1, -1);
        let size = libc::winsize { ws_row: 24, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0 };
        assert_eq!(unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size) }, 0);
        let terminal = Self { master: unsafe { File::from_raw_fd(master) }, slave: unsafe { File::from_raw_fd(slave) } };
        for file in [&terminal.master, &terminal.slave] {
            assert_eq!(unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) }, 0);
        }
        let mut attrs = unsafe { std::mem::zeroed::<libc::termios>() };
        assert_eq!(unsafe { libc::tcgetattr(slave, &mut attrs) }, 0);
        attrs.c_lflag &= !(libc::ECHO | libc::ECHONL);
        assert_eq!(unsafe { libc::tcsetattr(slave, libc::TCSANOW, &attrs) }, 0);
        terminal
    }

    fn read_all(mut self) -> Vec<u8> {
        drop(self.slave);
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut bytes = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "more terminal did not close");
            let mut event = libc::pollfd { fd: self.master.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            assert!(unsafe { libc::poll(&mut event, 1, left.as_millis() as i32) } > 0);
            let mut buffer = [0; 1024];
            match self.master.read(&mut buffer) {
                Ok(0) => return bytes,
                Ok(count) => bytes.extend_from_slice(&buffer[..count]),
                Err(error) if error.raw_os_error() == Some(libc::EIO) => return bytes,
                Err(error) => panic!("read terminal: {error}"),
            }
        }
    }
}

fn terminal_case(args: &[&std::ffi::OsStr], files: &[(&str, &[u8])], stdin: Option<&[u8]>, delay: Duration) -> Output {
    terminal_case_with_control(args, files, stdin, delay, false)
}

fn terminal_case_with_control(args: &[&std::ffi::OsStr], files: &[(&str, &[u8])], stdin: Option<&[u8]>, delay: Duration, controlling: bool) -> Output {
    let directory = tempfile::tempdir().unwrap();
    for (name, data) in files {
        if name.ends_with('/') { fs::create_dir(directory.path().join(name)).unwrap(); }
        else { fs::write(directory.path().join(name), data).unwrap(); }
    }
    let mut terminal = Terminal::new();
    let mut command = applet("more", directory.path());
    command.args(args).stdin(Stdio::from(terminal.slave.try_clone().unwrap()))
        .stdout(Stdio::from(terminal.slave.try_clone().unwrap()));
    let slave_fd = terminal.slave.as_raw_fd();
    unsafe { command.pre_exec(move || {
        if libc::setsid() < 0 { return Err(std::io::Error::last_os_error()); }
        if controlling && libc::ioctl(slave_fd, libc::TIOCSCTTY, 0) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }); }
    let mut child = Running::spawn(&mut command);
    drop(command);
    std::thread::sleep(delay);
    if let Some(input) = stdin {
        assert!(child.0.try_wait().unwrap().is_none(), "more must wait for terminal stdin");
        terminal.master.write_all(input).unwrap();
        terminal.master.write_all(b"\x04").unwrap();
    } else {
        let deadline = Instant::now() + Duration::from_secs(2);
        while child.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "more must exit at file EOF");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let mut output = child.finish();
    output.stdout = terminal.read_all();
    output
}

fn words<'a>(args: &[&'a str]) -> Vec<&'a std::ffi::OsStr> { args.iter().map(|arg| std::ffi::OsStr::new(*arg)).collect() }

// origin: uutils test_more::test_basic_display
#[test]
fn basic_display() {
    for controlling in [false, true] {
        let output = terminal_case_with_control(&words(&["test.txt"]), &[("test.txt", b"line1\nline2\nline3\n")], None, Duration::from_millis(200), controlling);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"line1\r\nline2\r\nline3\r\n");
        assert_eq!(output.stderr, b"");
    }
}

// origin: uutils test_more::test_file_arg
#[test]
fn file_errors() {
    for (args, expected) in [
        (vec!["."], b"more: read error: Is a directory\n".as_slice()),
        (vec!["folder"], b"more: read error: Is a directory\n".as_slice()),
        (vec!["nonexistent_file"], b"more: can't open 'nonexistent_file': No such file or directory\n".as_slice()),
        (vec!["file2", "file3"], b"more: can't open 'file2': No such file or directory\nmore: can't open 'file3': No such file or directory\n".as_slice()),
    ] {
        let output = terminal_case(&words(&args), &[("folder/", b"")], None, Duration::ZERO);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, b"");
        assert_eq!(output.stderr, expected);
    }
}

// origin: uutils test_more::test_more_non_utf8_paths
#[test]
fn missing_raw_filename_is_not_lossy_filename() {
    let raw = std::ffi::OsStr::from_bytes(b"test_\xff\xfe.txt");
    let output = terminal_case(&[raw], &[("test_��.txt", b"test content for non-UTF-8 file")], None, Duration::ZERO);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"more: can't open 'test_\xff\xfe.txt': No such file or directory\n");
}

// origin: uutils test_more::test_no_arg
#[test]
fn no_arguments_read_terminal_stdin() {
    let output = terminal_case(&[], &[], Some(b"stdin line\n"), Duration::from_millis(100));
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"stdin line\r\n");
    assert_eq!(output.stderr, b"");
}

// origin: uutils test_more::test_invalid_arg
#[test]
fn invalid_arguments_fail() {
    for args in [["--invalid"].as_slice(), ["--lines", "-10"].as_slice(), ["--from-line", "-10"].as_slice()] {
        let output = terminal_case(&words(args), &[], None, Duration::ZERO);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, b"");
        assert!(!output.stderr.is_empty());
    }
}

// origin: uutils test_more::test_valid_arg
#[test]
fn shared_valid_options_exit_at_short_file_eof() {
    for flag in ["-s", "-u", "-f", "-l"] {
        let output = terminal_case(&words(&[flag, "test_file"]), &[("test_file", b"test content")], None, Duration::from_millis(100));
        assert_eq!(output.status.code(), Some(0), "{flag}");
        assert_eq!(output.stdout, b"test content", "{flag}");
        assert_eq!(output.stderr, b"", "{flag}");
    }
}

// origin: uutils test_more::test_squeeze_blank_lines
#[test]
fn squeeze_option_displays_original_first_line() {
    let output = terminal_case(&words(&["-s", "test.txt"]), &[("test.txt", b"line1\n\n\n\nline2\n")], None, Duration::from_millis(200));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.windows(b"line1".len()).any(|part| part == b"line1"));
    assert_eq!(output.stderr, b"");
}
