use super::{applet, Running};
use std::process::Stdio;
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;

// The infinite input cannot complete its size discovery. GNU remains alive
// without creating chunk files; an explicit cleanup signal is not its result.
fn observe_zero_device() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("split", directory.path());
    command.args(["-n", "3", "/dev/zero"]).stdout(Stdio::null());
    // Size discovery can retain bytes from the infinite source. Bound this
    // disposable child's address space below the container memory ceiling;
    // reference runs also throttle CPU so the observation precedes exhaustion.
    #[cfg(target_os = "linux")]
    unsafe {
        command.pre_exec(|| {
            let limit = libc::rlimit { rlim_cur: 768 * 1024 * 1024, rlim_max: 768 * 1024 * 1024 };
            if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = Running::spawn(&mut command);
    std::thread::sleep(Duration::from_millis(500));
    assert!(child.0.try_wait().unwrap().is_none(), "split exited during infinite size discovery");
    for name in ["xaa", "xab", "xac"] {
        assert!(!directory.path().join(name).exists(), "created {name} before determining size");
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    child.kill();
    let output = child.finish();
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    for name in ["xaa", "xab", "xac"] {
        assert!(!directory.path().join(name).exists(), "cleanup left {name}");
    }
}

// origin: uutils test_split::test_dev_zero
#[test]
fn test_dev_zero() {
    observe_zero_device();
}

// origin: uutils test_split::test_number_by_bytes_dev_zero
#[test]
fn test_number_by_bytes_dev_zero() {
    observe_zero_device();
}
