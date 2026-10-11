#![cfg(unix)]

#[macro_use]
mod release_binary;

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);

// An explicit reference directory runs the same descriptor setup and assertions
// against installed reference tools. Script resolution otherwise stays beside
// the core modules, even when the child's working directory is a temp fixture.
fn applet(name: &str, directory: &Path) -> Command {
    let mut command = if let Some(reference) = std::env::var_os("XSH_CORE_COMPAT_REFERENCE_DIR") {
        Command::new(Path::new(&reference).join(name))
    } else {
        let mut command = Command::new(release_bin!("xsh"));
        command.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("core").join(format!("{name}.xsh")));
        command.arg("--");
        command
    };
    command.current_dir(directory).env("LC_ALL", "C");
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

// Drop reaps the owned process on assertion failures as well as normal paths.
struct Running(Child);

impl Running {
    fn spawn(command: &mut Command) -> Self {
        Self(command.spawn().expect("spawn applet"))
    }

    fn finish(mut self) -> Output {
        // Drain both streams during the wait: a finite result can exceed the
        // pipe capacity, and must not block the child before it can exit.
        let stdout_reader = self.0.stdout.take().map(|mut pipe| std::thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).expect("read stdout");
            bytes
        }));
        let stderr_reader = self.0.stderr.take().map(|mut pipe| std::thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).expect("read stderr");
            bytes
        }));
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            if let Some(status) = self.0.try_wait().expect("poll applet") {
                break status;
            }
            assert!(Instant::now() < deadline, "applet did not exit within {TIMEOUT:?}");
            std::thread::sleep(Duration::from_millis(5));
        };
        let stdout = stdout_reader.map(|reader| reader.join().expect("join stdout reader")).unwrap_or_default();
        let stderr = stderr_reader.map(|reader| reader.join().expect("join stderr reader")).unwrap_or_default();
        Output { status, stdout, stderr }
    }

    fn prefix(&mut self, count: usize) -> Vec<u8> {
        let pipe = self.0.stdout.as_mut().expect("piped stdout");
        let deadline = Instant::now() + TIMEOUT;
        let mut bytes = vec![0; count];
        let mut offset = 0;
        while offset < count {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out reading stdout prefix");
            let mut descriptor = libc::pollfd {
                fd: pipe.as_raw_fd(), events: libc::POLLIN, revents: 0,
            };
            let result = unsafe { libc::poll(&mut descriptor, 1, remaining.as_millis().min(i32::MAX as u128) as i32) };
            if result < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted { continue; }
                panic!("poll stdout: {error}");
            }
            assert!(result > 0, "timed out reading stdout prefix");
            let read = pipe.read(&mut bytes[offset..]).expect("read stdout prefix");
            assert!(read > 0, "stdout ended before {count} bytes");
            offset += read;
        }
        bytes
    }

    fn kill(&mut self) {
        self.0.kill().expect("kill infinite applet");
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if self.0.try_wait().expect("poll owned applet").is_none() {
            self.0.kill().expect("kill owned applet");
        }
        self.0.wait().expect("reap owned applet");
    }
}

fn assert_silent_failure(output: Output) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
}

// The stdin producer is joined only after the child's bounded wait, so a child
// that stops reading cannot leave a blocked fixture writer behind.
fn pipe_input(mut command: Command, bytes: Vec<u8>) -> Output {
    command.stdin(Stdio::piped());
    let mut child = Running::spawn(&mut command);
    let mut stdin = child.0.stdin.take().expect("piped stdin");
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let output = child.finish();
    if let Err(error) = writer.join().expect("join stdin producer") {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    output
}

// origin: uutils test_cat::test_cat_broken_pipe_nonzero_and_message
#[test]
fn test_cat_broken_pipe_nonzero_and_message() {
    let directory = tempfile::tempdir().unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let mut command = applet("cat", directory.path());
    command.stdout(writer);
    assert!(!pipe_input(command, vec![b'x'; 10000]).status.success());
}

// origin: uutils test_cat::test_broken_pipe
#[test]
fn test_broken_pipe() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("cat", directory.path());
    command.stdin(File::open("/dev/zero").unwrap());
    let mut child = Running::spawn(&mut command);
    drop(child.0.stdout.take());
    assert_silent_failure(child.finish());
}

// origin: uutils test_cat::test_closes_file_descriptors
#[cfg(target_os = "linux")]
#[test]
fn test_closes_file_descriptors() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("alpha.txt"), b"abcde\nfghij\nklmno\npqrst\nuvwxyz\n").unwrap();
    let mut command = applet("cat", directory.path());
    command.args(["alpha.txt"; 5]);
    unsafe {
        command.pre_exec(|| {
            let limit = libc::rlimit { rlim_cur: 9, rlim_max: 9 };
            if libc::setrlimit(libc::RLIMIT_NOFILE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let output = Running::spawn(&mut command).finish();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

// Reading a prefix establishes that the infinite source is active without a
// timing race. Killing then reaping preserves the remaining pipe bytes.
fn infinite_cat(args: &[&str], stdin: Option<File>, count: usize) -> (Vec<u8>, Output) {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("cat", directory.path());
    command.args(args);
    if let Some(stdin) = stdin { command.stdin(stdin); }
    let mut child = Running::spawn(&mut command);
    let prefix = child.prefix(count);
    assert!(child.0.try_wait().unwrap().is_none(), "infinite source exited");
    child.kill();
    let output = child.finish();
    (prefix, output)
}

// origin: uutils test_cat::test_dev_random
#[test]
fn test_dev_random() {
    let device = if cfg!(target_os = "linux") { "/dev/urandom" } else { "/dev/random" };
    let (prefix, _) = infinite_cat(&[device], None, 2048);
    assert!(prefix.iter().filter(|&&byte| byte == 0).count() < 512);
}

// origin: uutils test_cat::test_dev_full
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "netbsd"))]
#[test]
fn test_dev_full() {
    let (prefix, output) = infinite_cat(&["/dev/full"], None, 2048);
    assert_eq!(prefix, vec![0; 2048]);
    assert!(output.stderr.is_empty());
}

// origin: uutils test_cat::test_dev_full_show_all
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "netbsd"))]
#[test]
fn test_dev_full_show_all() {
    let (prefix, output) = infinite_cat(&["-A", "/dev/full"], None, 2048);
    assert_eq!(prefix, b"^@".repeat(1024));
    assert!(output.stderr.is_empty());
}

// origin: uutils test_cat::test_cat_rw_self_succeeds
#[test]
fn test_cat_rw_self_succeeds() {
    let directory = tempfile::tempdir().unwrap();
    let combined = directory.path().join("combined");
    std::fs::write(&combined, b"hello").unwrap();
    std::fs::write(directory.path().join("extra"), b"world").unwrap();
    let stdout = OpenOptions::new().read(true).write(true).open(&combined).unwrap();
    let mut command = applet("cat", directory.path());
    command.args(["combined", "extra"]).stdout(stdout);
    let output = Running::spawn(&mut command).finish();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(std::fs::read(combined).unwrap(), b"helloworld");
}

// origin: uutils test_cat::test_cat_rw_self_conflict_fails
#[test]
fn test_cat_rw_self_conflict_fails() {
    let directory = tempfile::tempdir().unwrap();
    let dest = directory.path().join("dest");
    std::fs::write(&dest, b"bcde").unwrap();
    std::fs::write(directory.path().join("source"), b"a").unwrap();
    let stdout = OpenOptions::new().read(true).write(true).open(&dest).unwrap();
    let mut command = applet("cat", directory.path());
    command.args(["source", "dest"]).stdout(stdout);
    let output = Running::spawn(&mut command).finish();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"cat: dest: input file is output file\n");
    assert_eq!(std::fs::read(dest).unwrap(), b"acde");
}

// origin: uutils test_cat::test_uchild_when_no_capture_reading_from_infinite_source
#[test]
fn test_uchild_when_no_capture_reading_from_infinite_source() {
    let (prefix, output) = infinite_cat(&[], Some(File::open("/dev/zero").unwrap()), 12345);
    assert_eq!(prefix, vec![0; 12345]);
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout.first(), Some(&0));
}

// origin: uutils test_cat::test_child_when_pipe_in
#[test]
fn test_child_when_pipe_in() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("cat", directory.path());
    command.stdin(Stdio::piped());
    let mut child = Running::spawn(&mut command);
    let mut stdin = child.0.stdin.take().unwrap();
    stdin.write_all(b"content").unwrap();
    drop(stdin);
    let output = child.finish();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"content");
    assert!(output.stderr.is_empty());
    let output = pipe_input(applet("cat", directory.path()), b"content".to_vec());
    assert_eq!(output.stdout, b"content");
}

// origin: uutils test_tr::test_broken_pipe_no_error
#[test]
fn test_broken_pipe_no_error() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("tr", directory.path());
    command.args(["e", "a"]).stdin(Stdio::piped());
    let mut child = Running::spawn(&mut command);
    let mut stdout = child.0.stdout.take().unwrap();
    stdout.read_exact(&mut []).unwrap();
    drop(stdout);
    let mut stdin = child.0.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(&b"hello".repeat(100)));
    assert_silent_failure(child.finish());
    if let Err(error) = writer.join().unwrap() {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
}

// origin: uutils test_tr::test_stdin_is_socket
#[test]
fn test_stdin_is_socket() {
    let directory = tempfile::tempdir().unwrap();
    let (mut writer, reader) = UnixStream::pair().unwrap();
    writer.write_all(b"::").unwrap();
    drop(writer);
    let stdin: OwnedFd = reader.into();
    let mut command = applet("tr", directory.path());
    command.args([":", ";"]).stdin(stdin);
    let output = Running::spawn(&mut command).finish();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b";;");
}

fn boundary_directory() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/core-compat-boundaries");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

// A clone shares the child's open file description, so reading the clone after
// exit observes the actual stdin offset rather than a newly opened file.
fn head_shared_stdin(args: &[&str], input: &[u8], expected: &[u8], remainder: &[u8]) {
    let directory = boundary_directory();
    let path = directory.path().join("input");
    std::fs::write(&path, input).unwrap();
    let stdin = File::open(path).unwrap();
    let mut shadow = stdin.try_clone().unwrap();
    let mut command = applet("head", directory.path());
    command.args(args).stdin(stdin);
    let output = Running::spawn(&mut command).finish();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
    let mut remaining = Vec::new();
    assert_eq!(shadow.read_to_end(&mut remaining).unwrap(), remainder.len());
    assert_eq!(remaining, remainder);
}

fn sequence_bytes(first: usize, last: usize) -> Vec<u8> {
    (first..=last).map(|number| format!("{number}\n")).collect::<String>().into_bytes()
}

// origin: uutils test_head::test_validate_stdin_offset_lines
#[cfg(target_os = "linux")]
#[test]
fn test_validate_stdin_offset_lines() {
    head_shared_stdin(&["-n", "1"], b"a\nb\nc\n", b"a\n", b"b\nc\n");
    head_shared_stdin(&["-n", "-1"], b"a\nb\nc\n", b"a\nb\n", b"c\n");
    head_shared_stdin(&["-n", "-19000"], &sequence_bytes(1, 20000),
        &sequence_bytes(1, 1000), &sequence_bytes(1001, 20000));
}

// origin: uutils test_head::test_validate_stdin_offset_bytes
#[cfg(target_os = "linux")]
#[test]
fn test_validate_stdin_offset_bytes() {
    head_shared_stdin(&["-c", "2"], b"abc\ndef\n", b"ab", b"c\ndef\n");
    head_shared_stdin(&["-c", "-3"], b"abc\ndef\n", b"abc\nd", b"ef\n");
    head_shared_stdin(&["-c", "-0"], b"abc\ndef\n", b"abc\ndef\n", b"");
    let remainder = sequence_bytes(19001, 20000);
    let count = format!("-{}", remainder.len());
    head_shared_stdin(&["-c", &count], &sequence_bytes(1, 20000),
        &sequence_bytes(1, 19000), &remainder);
}

// origin: uutils test_expand::test_large_tab_stop_without_tabs_does_not_allocate
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
#[test]
fn test_large_tab_stop_without_tabs_does_not_allocate() {
    let directory = boundary_directory();
    let mut command = applet("expand", directory.path());
    command.arg("--tabs=267672676527678256");
    unsafe {
        command.pre_exec(|| {
            let limit = libc::rlimit { rlim_cur: 200 * 1024 * 1024, rlim_max: 200 * 1024 * 1024 };
            if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let output = pipe_input(command, b"hello\n".to_vec());
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"hello\n");
}

// origin: uutils test_paste::test_dev_zero_closed_pipe
#[cfg(target_os = "linux")]
#[test]
fn test_dev_zero_closed_pipe() {
    let directory = boundary_directory();
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let mut command = applet("paste", directory.path());
    command.arg("/dev/zero").stdout(writer);
    assert_silent_failure(Running::spawn(&mut command).finish());
}

// origin: uutils test_comm::test_comm_anonymous_pipes
#[cfg(target_os = "linux")]
#[test]
fn test_comm_anonymous_pipes() {
    let directory = boundary_directory();
    let (reader1, mut writer1) = std::io::pipe().unwrap();
    let (reader2, mut writer2) = std::io::pipe().unwrap();
    let content = (0..1500).map(|number| format!("{number:05}\n")).collect::<String>();
    let content2 = format!("{content}99999\n");
    // Pipe capacity may be smaller than the fixture. Producers run concurrently
    // with the child, and dropping the readers unblocks them on a child failure.
    let producer1 = std::thread::spawn(move || writer1.write_all(content.as_bytes()));
    let producer2 = std::thread::spawn(move || writer2.write_all(content2.as_bytes()));
    let path1 = format!("/proc/{}/fd/{}", std::process::id(), reader1.as_raw_fd());
    let path2 = format!("/proc/{}/fd/{}", std::process::id(), reader2.as_raw_fd());
    let mut command = applet("comm", directory.path());
    command.args(["-13", &path1, &path2]);
    let output = Running::spawn(&mut command).finish();
    drop((reader1, reader2));
    producer1.join().unwrap().unwrap();
    producer2.join().unwrap().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"99999\n");
}

// origin: uutils test_pwd::test_deleted_dir
#[test]
fn test_deleted_dir() {
    let directory = boundary_directory();
    let removed = directory.path().join("foo");
    std::fs::create_dir(&removed).unwrap();
    let removed_bytes = std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(removed.as_os_str())).unwrap();
    let mut command = applet("pwd", &removed);
    unsafe {
        command.pre_exec(move || {
            if libc::rmdir(removed_bytes.as_ptr()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let output = Running::spawn(&mut command).finish();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"pwd: couldn't find directory entry in '..' with matching i-node\n");
}

fn mkfifo_with_umask(mode: Option<&str>, mask: libc::mode_t, expected: u32) {
    let directory = boundary_directory();
    let mut command = applet("mkfifo", directory.path());
    if let Some(mode) = mode { command.args(["-m", mode]); }
    command.arg("fifo_test");
    unsafe {
        command.pre_exec(move || {
            libc::umask(mask);
            Ok(())
        });
    }
    let output = Running::spawn(&mut command).finish();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata = std::fs::metadata(directory.path().join("fifo_test")).unwrap();
    assert!(metadata.file_type().is_fifo());
    assert_eq!(metadata.permissions().mode() & 0o7777, expected);
}

// origin: uutils test_mkfifo::test_create_fifo_with_mode_and_umask
#[test]
fn test_uu_mkfifo_create_fifo_with_mode_and_umask() {
    for (mode, mask, expected) in [
        ("734", 0o077, 0o734),
        ("706", 0o777, 0o706),
        ("a=rwx", 0o022, 0o777),
        ("a=rx", 0o022, 0o555),
        ("a=r", 0o022, 0o444),
        ("=rwx", 0o022, 0o755),
        ("u+w", 0o022, 0o666),
        ("u-w", 0o022, 0o466),
        ("u+x", 0o022, 0o766),
        ("u-r,g-w,o+x", 0o022, 0o247),
        ("a=rwx,o-w", 0o022, 0o775),
        ("=rwx,o-w", 0o022, 0o755),
        ("ug+rw,o+r", 0o022, 0o666),
        ("u=rwx,g=rx,o=", 0o022, 0o750),
    ] {
        mkfifo_with_umask(Some(mode), mask, expected);
    }
}

// origin: uutils test_mkfifo::test_create_fifo_with_umask
#[test]
fn test_uu_mkfifo_create_fifo_with_umask() {
    mkfifo_with_umask(None, 0o022, 0o644);
    mkfifo_with_umask(None, 0o777, 0o000);
}

// origin: uutils test_mkfifo::diagnostics::test_plain_message_when_stderr_is_a_pipe
#[test]
fn test_uu_mkfifo_diagnostics_plain_message_when_stderr_is_a_pipe() {
    let directory = boundary_directory();
    let mut command = applet("mkfifo", directory.path());
    command.args(["-m", "+rw?", "some_pipe"]);
    let output = Running::spawn(&mut command).finish();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("mkfifo: "), "{stderr}");
    assert!(!stderr.contains(":1:"), "{stderr}");
}
