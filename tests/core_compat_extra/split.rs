use super::{applet, Running};
use std::io::Read;
use std::process::Stdio;
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::time::Instant;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;

// The infinite input cannot complete its size discovery. GNU remains alive
// without creating chunk files; an explicit cleanup signal is not its result.
fn observe_zero_device() {
    #[cfg(target_os = "linux")]
    require_cpu_quota();
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
    #[cfg(target_os = "linux")]
    wait_for_zero_descriptor(&mut child);
    std::thread::sleep(Duration::from_millis(500));
    assert_split_running(&mut child);
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

fn assert_split_running(child: &mut Running) {
    if let Some(status) = child.0.try_wait().expect("poll owned split child") {
        let mut stderr = Vec::new();
        child.0.stderr.as_mut().expect("piped split stderr")
            .read_to_end(&mut stderr).expect("read exited split stderr");
        panic!("split exited during infinite size discovery: {status:?}; stderr={}",
            String::from_utf8_lossy(&stderr));
    }
}

// The infinite source consumes memory as fast as it is read. These cases need
// the same controlled CPU quota as the reference observation, not just an
// address-space limit that could turn an ongoing read into allocation failure.
#[cfg(target_os = "linux")]
fn require_cpu_quota() {
    let value = std::fs::read_to_string("/sys/fs/cgroup/cpu.max")
        .expect("split infinite-device fixture requires readable cgroup v2 cpu.max");
    let fields: Vec<_> = value.split_whitespace().collect();
    assert_eq!(fields.len(), 2, "invalid cgroup CPU quota: {value:?}");
    let quota: u128 = fields[0].parse()
        .expect("split infinite-device fixture requires finite CPU quota <= 0.01 CPU");
    let period: u128 = fields[1].parse().expect("invalid cgroup CPU period");
    assert!(quota > 0 && period > 0 && quota <= period / 100,
        "split infinite-device fixture requires CPU quota <= 0.01 CPU; cpu.max={value:?}");
}

// Opening the input establishes that the applet has reached size discovery;
// merely observing a live launcher could otherwise count source parsing as
// successful infinite-input behavior under the deliberately small CPU quota.
#[cfg(target_os = "linux")]
fn wait_for_zero_descriptor(child: &mut Running) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let descriptors = std::path::PathBuf::from(format!("/proc/{}/fd", child.0.id()));
    loop {
        assert_split_running(child);
        let entries = std::fs::read_dir(&descriptors).unwrap_or_else(|error| {
            assert_split_running(child);
            panic!("inspect owned split child descriptors: {error}");
        });
        for entry in entries {
            let path = entry.expect("read owned split descriptor entry").path();
            match std::fs::read_link(path) {
                Ok(target) if target == std::path::Path::new("/dev/zero") => return,
                Ok(_) => {}
                // A descriptor can close between directory enumeration and
                // readlink while the owned process continues opening files.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("inspect owned split descriptor target: {error}"),
            }
        }
        assert!(Instant::now() < deadline, "split did not open /dev/zero within 60 seconds");
        std::thread::sleep(Duration::from_millis(10));
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
