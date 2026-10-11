#![cfg(unix)]

#[macro_use]
mod release_binary;

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
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
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            if let Some(status) = self.0.try_wait().expect("poll applet") {
                break status;
            }
            assert!(Instant::now() < deadline, "applet did not exit within {TIMEOUT:?}");
            std::thread::sleep(Duration::from_millis(5));
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut pipe) = self.0.stdout.take() {
            pipe.read_to_end(&mut stdout).expect("read stdout");
        }
        if let Some(mut pipe) = self.0.stderr.take() {
            pipe.read_to_end(&mut stderr).expect("read stderr");
        }
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
