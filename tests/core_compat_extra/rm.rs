#![cfg(all(target_os = "linux", feature = "linux-priv-tests"))]

use super::{applet, Running};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

// The real root identity is essential to these tests. A child gets a private
// recursive bind of that root, with every submount made read-only before exec.
// It also drops root credentials so the tested process cannot undo protection.
// Setup errors abort spawning; the dangerous operands never reach an applet
// unless the kernel has successfully enforced the entire read-only tree.
fn preserve_root(operand: &str, nested: bool, bind_root: bool) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("/", directory.path().join("rootlink")).unwrap();
    if nested {
        std::os::unix::fs::symlink("rootlink", directory.path().join("rootlink2")).unwrap();
    }
    let mountpoint = directory.path().join("rootbind");
    if bind_root { std::fs::create_dir(&mountpoint).unwrap(); }
    let target = if bind_root { mountpoint.as_os_str().to_owned() } else { operand.into() };
    let rm = applet("rm", directory.path());
    let mut command = Command::new("strace");
    // Following threads is necessary: the applet's filesystem work can occur
    // outside the main thread. Decoded descriptor paths identify root descent.
    // Tracer exit also kills tracees when the bounded process guard times out.
    command.args(["--kill-on-exit", "-f", "-yy", "-e", "trace=getdents64", "--"])
        .arg(rm.get_program()).args(rm.get_args())
        .args([if bind_root { "-ri" } else { "-rf" }, "--preserve-root"])
        .arg(target).current_dir(directory.path())
        .env("LC_ALL", "C").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let cwd = CString::new(directory.path().as_os_str().as_bytes()).unwrap();
    let mountpoint_bytes = CString::new(mountpoint.as_os_str().as_bytes()).unwrap();
    unsafe {
        command.pre_exec(move || {
            if libc::unshare(libc::CLONE_NEWNS) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::mount(std::ptr::null(), c"/".as_ptr(), std::ptr::null(),
                libc::MS_PRIVATE | libc::MS_REC, std::ptr::null()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if bind_root && libc::mount(c"/".as_ptr(), mountpoint_bytes.as_ptr(), std::ptr::null(),
                libc::MS_BIND | libc::MS_REC, std::ptr::null()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::mount(c"/".as_ptr(), c"/".as_ptr(), std::ptr::null(),
                libc::MS_BIND | libc::MS_REC, std::ptr::null()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let attributes = libc::mount_attr {
                attr_set: libc::MOUNT_ATTR_RDONLY, attr_clr: 0, propagation: 0, userns_fd: 0,
            };
            if libc::syscall(libc::SYS_mount_setattr, libc::AT_FDCWD, c"/".as_ptr(),
                libc::AT_RECURSIVE, &attributes, std::mem::size_of::<libc::mount_attr>()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // Command selected its cwd before pre_exec. Resolve it again after
            // the root bind so no relative operand retains a writable old mount.
            if libc::chdir(c"/".as_ptr()) != 0 || libc::chdir(cwd.as_ptr()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::setgroups(0, std::ptr::null()) != 0 || libc::setgid(1000) != 0 || libc::setuid(1000) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().expect("BLOCKED: strace and private recursively read-only root require root, CAP_SYS_ADMIN, and mount_setattr");
    let output = Running(child).finish();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let expected = if bind_root || operand != "/" {
        "it is dangerous to operate recursively on"
    } else {
        "it is dangerous to operate recursively on '/'"
    };
    assert!(stderr.contains(expected), "{stderr}");
    if bind_root || operand != "/" {
        assert!(stderr.contains("(same as '/')"), "{stderr}");
    } else {
        assert!(stderr.contains("use --no-preserve-root to override this failsafe"), "{stderr}");
    }
    let bind_descriptor = format!("<{}>", mountpoint.display());
    assert!(!stderr.lines().any(|line| line.contains("getdents64(") &&
        (line.contains("</>") || line.contains(&bind_descriptor))),
        "rm descended into root before rejecting it: {stderr}");
    assert_eq!(std::fs::read_link(directory.path().join("rootlink")).unwrap(), std::path::Path::new("/"));
    if nested {
        assert_eq!(std::fs::read_link(directory.path().join("rootlink2")).unwrap(), std::path::Path::new("rootlink"));
    }
}

// origin: uutils test_rm::test_preserve_root_literal_root
#[test]
fn test_preserve_root_literal_root() { preserve_root("/", false, false); }

// origin: uutils test_rm::test_preserve_root_symlink_to_root
#[test]
fn test_preserve_root_symlink_to_root() { preserve_root("rootlink/", false, false); }

// origin: uutils test_rm::test_preserve_root_nested_symlink_to_root
#[test]
fn test_preserve_root_nested_symlink_to_root() { preserve_root("rootlink2/", true, false); }

// origin: uutils test_rm::test_preserve_root_bind_mount_of_root
#[test]
fn test_preserve_root_bind_mount_of_root() { preserve_root("", false, true); }
