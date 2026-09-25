#![cfg(target_os = "linux")]

// Tests for Linux-specific XSH module functions that require elevated capabilities.
// Run in the pinned Dockerfile.test image with --privileged. Cases report the
// missing capability when invoked without it.

use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Returns true when running as root (i.e. under test-linux-priv).
/// Use at the top of any test that needs CAP_SYS_ADMIN, CAP_MKNOD, or CAP_NET_ADMIN:
///
///   #[test]
///   fn example() {
///       if !is_root() { return; }
///       // ...
///   }
pub fn is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

/// Create a sparse image file of `size_mb` MiB, attach it as a loop device via the system
/// `losetup`, call `f` with the device path, then detach. Cleanup runs even on panic.
/// Requires CAP_SYS_ADMIN — call is_root() first to skip gracefully.
pub fn with_loop_image<F: FnOnce(&std::path::Path)>(size_mb: u64, f: F) {
    let image = std::env::temp_dir().join(format!("xsh-loop-{}.img", std::process::id()));
    {
        let file = std::fs::File::create(&image).expect("create loop image");
        file.set_len(size_mb * 1024 * 1024)
            .expect("set loop image size");
    }

    let out = std::process::Command::new("losetup")
        .args(["--find", "--show"])
        .arg(&image)
        .output()
        .expect("losetup --find --show");
    assert!(
        out.status.success(),
        "losetup attach failed: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    let device = std::path::PathBuf::from(
        String::from_utf8(out.stdout)
            .expect("utf8")
            .trim()
            .to_string(),
    );

    let _guard = LoopGuard {
        device: device.clone(),
        image,
    };
    f(&device);
}

fn run_script(source: &str) -> std::process::Output {
    let path = temp_xsh_path("linux-priv");
    std::fs::write(&path, source).expect("write linux priv script");
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .output()
        .expect("run xsh");
    let _ = std::fs::remove_file(path);
    output
}

fn run_script_in_private_mount_namespace(
    source: &str,
    root: &Path,
    fstab: Option<&Path>,
) -> std::io::Result<std::process::Output> {
    let script = root.join("mount.xsh");
    std::fs::write(&script, source)?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_xsh"));
    command.arg(script);
    let fstab_source = fstab.map(|path| {
        std::ffi::CString::new(path.as_os_str().as_bytes()).expect("fstab fixture path is cstr")
    });
    // This Rust harness owns the privilege boundary. The script asserts the
    // XSH result while the child namespace prevents mount propagation.
    unsafe {
        command.pre_exec(move || {
            if libc::unshare(libc::CLONE_NEWNS) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::mount(
                std::ptr::null(),
                b"/\0".as_ptr().cast(),
                std::ptr::null(),
                (libc::MS_REC | libc::MS_PRIVATE) as libc::c_ulong,
                std::ptr::null(),
            ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            if let Some(source) = &fstab_source
                && libc::mount(
                    source.as_ptr(),
                    b"/etc/fstab\0".as_ptr().cast(),
                    std::ptr::null(),
                    libc::MS_BIND as libc::c_ulong,
                    std::ptr::null(),
                ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.output()
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xsh-{name}-{}", unique_suffix()))
}

fn temp_xsh_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xsh-{name}-{}.xsh", unique_suffix()))
}

fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn xsh_string_literal(text: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\0' => quoted.push_str("\\0"),
            ch => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

fn lacks_capability(output: &std::process::Output) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr.contains("Operation not permitted")
        || stderr.contains("operation not permitted")
        || stderr.contains("Permission denied")
        || stderr.contains("permission denied")
}

fn wait_for_path(path: &Path, timeout: Duration, child: &mut Child) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll child") {
            panic!("child exited before marker: {status}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("timed out waiting for marker {}", path.display());
}

fn wait_child_status(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("poll child") {
            return status;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let status = child.wait().expect("wait killed child");
    panic!("timed out waiting for child: {status}");
}

fn mount_visible_in_parent(path: &Path) -> bool {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").expect("read mountinfo");
    mountinfo
        .lines()
        .any(|line| line.split_whitespace().nth(4) == path.to_str())
}

#[test]
fn linux_priv_tmpfs_mount_is_mountpoint_disk_usage_and_cleanup() {
    if !is_root() {
        eprintln!("skipped: private mount namespace requires root and CAP_SYS_ADMIN");
        return;
    }
    let root = tempfile::Builder::new()
        .prefix("xsh-linux-priv-mount-")
        .tempdir()
        .expect("create mount test root");
    let target = root.path().join("target");
    std::fs::create_dir_all(&target).expect("create tmpfs target");
    assert!(!mount_visible_in_parent(&target));
    let source = format!(
        "\
let target = Path({})
env XSH_LINUX_REAL=1 XSH_LINUX_DRY_RUN=0 {{
  linux.mount(\"none\", target, fstype: \"tmpfs\", options: [\"size=4m\", \"nosuid\", \"nodev\"])?
  let mounted = linux.is_mountpoint(target)?
  let usage = linux.disk_usage(target)?.collect()
  print ${{mounted}} ${{usage[0].mount == target.display()}} ${{usage[0].fstype == \"tmpfs\"}} ${{usage[0].total > 0}}
}} ?
",
        xsh_string_literal(target.to_str().unwrap())
    );

    let output = match run_script_in_private_mount_namespace(&source, root.path(), None) {
        Ok(output) => output,
        Err(error) if error.raw_os_error() == Some(libc::EPERM) => {
            eprintln!("skipped: CAP_SYS_ADMIN is required to create a private mount namespace");
            return;
        }
        Err(error) => panic!("start private mount test: {error}"),
    };
    assert!(!mount_visible_in_parent(&target), "mount escaped its namespace");
    if !output.status.success() && lacks_capability(&output) {
        eprintln!("skipped: CAP_SYS_ADMIN is required for the tmpfs mount");
        return;
    }

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true true\n"
    );
    root.close().expect("remove mount test root");
    assert!(!target.exists(), "mount test root was not removed");
}

#[test]
fn linux_priv_mount_and_switch_root_fail_within_private_namespace() {
    if !is_root() {
        eprintln!("skipped: private mount namespace requires root and CAP_SYS_ADMIN");
        return;
    }
    let root = tempfile::Builder::new()
        .prefix("xsh-linux-priv-failure-")
        .tempdir()
        .expect("create failure test root");
    let target = root.path().join("target");
    let missing_root = root.path().join("missing-root");
    let fstab = root.path().join("fstab");
    std::fs::create_dir_all(&target).expect("create mount target");
    std::fs::write(
        &fstab,
        format!("none {} xsh_missing_fs defaults 0 0\n", target.display()),
    )
    .expect("write fstab fixture");
    let parent_fstab = std::fs::read_to_string("/etc/fstab").expect("read parent fstab");
    // This host test creates the mount namespace before XSH starts. A native
    // test cannot safely establish that privilege boundary by itself.
    let source = format!(
        "\
let target = Path({})
let missing_root = Path({})
env XSH_LINUX_REAL=1 XSH_LINUX_DRY_RUN=0 {{
  match linux.mount(\"none\", target, fstype: \"xsh_missing_fs\") {{
    Err(error) => test.error_kind(error, \"linux-mount\")?
    Ok(_) => test.fail(\"mount with unknown filesystem succeeded\")?
  }}
  test.ok(! linux.is_mountpoint(target)?)?
  match linux.mount_all() {{
    Err(error) => test.error_kind(error, \"linux-mount\")?
    Ok(_) => test.fail(\"mount_all with unknown filesystem succeeded\")?
  }}
  test.ok(! linux.is_mountpoint(target)?)?
  match linux.switch_root(missing_root, /sbin/init) {{
    Err(error) => test.error_kind(error, \"linux-switch-root\")?
    Ok(_) => test.fail(\"switch_root with missing root succeeded\")?
  }}
  print \"failure paths checked\"
}} ?
",
        xsh_string_literal(target.to_str().unwrap()),
        xsh_string_literal(missing_root.to_str().unwrap()),
    );

    let output = match run_script_in_private_mount_namespace(&source, root.path(), Some(&fstab)) {
        Ok(output) => output,
        Err(error) if error.raw_os_error() == Some(libc::EPERM) => {
            eprintln!("skipped: CAP_SYS_ADMIN is required to create a private mount namespace");
            return;
        }
        Err(error) => panic!("start private mount failure test: {error}"),
    };
    assert!(!mount_visible_in_parent(&target), "mount escaped its namespace");
    assert_eq!(
        std::fs::read_to_string("/etc/fstab").expect("read parent fstab after child"),
        parent_fstab,
        "fstab fixture escaped its namespace"
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "failure paths checked\n"
    );
    root.close().expect("remove failure test root");
    assert!(!target.exists(), "failure test root was not removed");
}

#[test]
fn linux_priv_mknod_creates_character_device_when_permitted() {
    if !is_root() {
        return;
    }
    let root = temp_path("linux-priv-mknod");
    let node = root.join("xsh-null");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create mknod root");
    let source = format!(
        "\
let node = Path({})
env XSH_LINUX_REAL=1 XSH_LINUX_DRY_RUN=0 {{
  linux.mknod(node, \"char\", 1, 3)?
  print ${{node.exists()?}}
}} ?
",
        xsh_string_literal(node.to_str().unwrap())
    );

    let output = run_script(&source);
    let _ = std::fs::remove_file(&node);
    let _ = std::fs::remove_dir_all(&root);
    if !output.status.success() && lacks_capability(&output) {
        return;
    }

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
}

#[test]
fn linux_priv_kill_all_signals_contained_new_session_process() {
    if !is_root() {
        return;
    }
    let marker = temp_path("linux-priv-kill-all-ready");
    let _ = std::fs::remove_file(&marker);
    let mut child = unsafe {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xsh-test-os-probe"));
        command
            .arg("ready-sleep")
            .arg(&marker)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        command.spawn().expect("spawn new-session helper")
    };
    wait_for_path(&marker, Duration::from_secs(3), &mut child);

    let output = run_script(
        "\
env XSH_LINUX_REAL=1 XSH_LINUX_DRY_RUN=0 {
  linux.kill_all(signal: \"TERM\", except_pid1: true)?
  print \"done\"
} ?
",
    );
    if !output.status.success() && lacks_capability(&output) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(marker);
        return;
    }

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "done\n");
    let status = wait_child_status(&mut child, Duration::from_secs(3));
    assert!(!status.success(), "{status}");

    let mut stderr = Vec::new();
    child
        .stderr
        .take()
        .expect("child stderr")
        .read_to_end(&mut stderr)
        .expect("read child stderr");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
    let _ = std::fs::remove_file(marker);
}

struct LoopGuard {
    device: std::path::PathBuf,
    image: std::path::PathBuf,
}

impl Drop for LoopGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("losetup")
            .arg("-d")
            .arg(&self.device)
            .status();
        let _ = std::fs::remove_file(&self.image);
    }
}
