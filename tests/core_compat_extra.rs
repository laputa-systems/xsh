#![cfg(unix)]

#[macro_use]
mod release_binary;

use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

#[path = "core_compat_extra/fd.rs"]
mod fd;
#[path = "core_compat_extra/pty.rs"]
mod pty;
#[path = "core_compat_extra/privilege.rs"]
mod privilege;
#[path = "core_compat_extra/cp.rs"]
mod cp;
#[path = "core_compat_extra/lifecycle.rs"]
mod lifecycle;
#[path = "core_compat_extra/descriptor.rs"]
mod descriptor;
#[path = "core_compat_extra/startup.rs"]
mod startup;
#[path = "core_compat_extra/rm.rs"]
mod rm;

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

    #[cfg(target_os = "linux")]
    fn finish_with_usage(mut self) -> (Output, libc::rusage) {
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
        let mut status = 0;
        let mut usage = unsafe { std::mem::zeroed::<libc::rusage>() };
        loop {
            let result = unsafe { libc::wait4(self.0.id() as libc::pid_t, &mut status, libc::WNOHANG, &mut usage) };
            if result > 0 { break; }
            if result < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted { continue; }
                panic!("wait4 applet: {error}");
            }
            assert!(Instant::now() < deadline, "applet did not exit within {TIMEOUT:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
        use std::os::unix::process::ExitStatusExt;
        let output = Output {
            status: std::process::ExitStatus::from_raw(status),
            stdout: stdout_reader.map(|reader| reader.join().expect("join stdout reader")).unwrap_or_default(),
            stderr: stderr_reader.map(|reader| reader.join().expect("join stderr reader")).unwrap_or_default(),
        };
        // wait4 already reaped this child; Drop must not send a signal to a
        // potentially reused PID or wait for it again.
        std::mem::forget(self);
        (output, usage)
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
