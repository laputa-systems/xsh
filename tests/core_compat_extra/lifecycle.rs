use super::{applet, Running};
use std::io::Read;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// origin: uutils test_echo::test_uchild_when_run_no_wait_with_a_non_blocking_util
#[test]
fn test_uchild_when_run_no_wait_with_a_non_blocking_util() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("echo", directory.path());
    command.arg("hello world");
    let mut child = Running::spawn(&mut command);
    let mut remaining = 10;
    while child.0.try_wait().unwrap().is_none() {
        assert!(remaining > 0, "echo remains alive after bounded polling");
        std::thread::sleep(Duration::from_millis(500));
        remaining -= 1;
    }
    assert!(child.0.try_wait().unwrap().is_some());
    assert!(child.0.try_wait().unwrap().is_some());

    // Keep the consumed bytes separately so subsequent reads and the final
    // wait can distinguish new output from the complete output history.
    let stdout_all = child.prefix(b"hello world\n".len());
    assert_eq!(stdout_all, b"hello world\n");
    let mut stderr_all = Vec::new();
    child.0.stderr.as_mut().unwrap().read_to_end(&mut stderr_all).unwrap();
    assert!(stderr_all.is_empty());
    let mut stdout = Vec::new();
    child.0.stdout.as_mut().unwrap().read_to_end(&mut stdout).unwrap();
    assert!(stdout.is_empty());
    let mut stderr = Vec::new();
    child.0.stderr.as_mut().unwrap().read_to_end(&mut stderr).unwrap();
    assert!(stderr.is_empty());
    assert_eq!(stdout_all, b"hello world\n");
    assert!(stderr_all.is_empty());
    assert!(child.0.try_wait().unwrap().is_some());
    child.kill();
    let output = child.finish();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

// Observe an exit without reaping it: Child::kill must still address the same
// unreaped child, and Child::wait must receive its original signal status.
fn wait_without_reaping(child: &Running) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
        let result = unsafe {
            libc::waitid(libc::P_PID, child.0.id(), &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT)
        };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted { continue; }
            panic!("observe owned child exit: {error}");
        }
        if unsafe { info.si_pid() } != 0 { return; }
        assert!(Instant::now() < deadline, "owned target did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
}

// origin: uutils test_kill::test_kill_out_of_range_signal_is_rejected_not_sent
#[test]
fn test_kill_out_of_range_signal_is_rejected_not_sent() {
    let directory = tempfile::tempdir().unwrap();
    // GNU rejects 65 but decodes 129 as the HUP wait status. Preserve both
    // operands and the actual terminating signal rather than treating every
    // number above the signal range as invalid.
    for (argument, signal) in [("-65", libc::SIGKILL), ("-129", libc::SIGHUP)] {
        let mut command = Command::new("sleep");
        command.arg("30").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        unsafe {
            command.pre_exec(|| {
                if libc::signal(libc::SIGHUP, libc::SIG_DFL) == libc::SIG_ERR {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut target = Running::spawn(&mut command);
        let mut command = applet("kill", directory.path());
        command.arg(argument).arg(target.0.id().to_string());
        let output = Running::spawn(&mut command).finish();
        if argument == "-65" {
            assert_eq!(output.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&output.stderr).contains("invalid signal"));
        } else {
            assert_eq!(output.status.code(), Some(0));
            // Waiting without reaping prevents cleanup SIGKILL from winning
            // a scheduling race with the applet's successful HUP delivery.
            wait_without_reaping(&target);
        }
        target.0.kill().expect("kill same unreaped target");
        wait_without_reaping(&target);
        let status = target.0.wait().expect("wait same owned target");
        assert_eq!(status.signal(), Some(signal));
    }
}

// origin: uutils test_timeout::test_sigchld_ignored_by_parent
#[test]
fn test_sigchld_ignored_by_parent() {
    let directory = tempfile::tempdir().unwrap();
    let inner = applet("timeout", directory.path());
    let mut outer = applet("timeout", directory.path());
    // Shell positional arguments carry the complete launcher argv through
    // exec, including paths with spaces and the script argument separator.
    outer.args(["10", "sh", "-c", "trap '' CHLD; exec \"$@\"", "nested-timeout"]);
    outer.arg(inner.get_program()).args(inner.get_args()).args(["1", "true"]);
    let output = Running::spawn(&mut outer).finish();
    assert_eq!(output.status.code(), Some(0), "{}", String::from_utf8_lossy(&output.stderr));
}
