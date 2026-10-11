#![cfg(all(target_os = "linux", feature = "linux-priv-tests"))]

use super::{applet, Running};
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt, symlink};
use std::path::Path;
use std::process::Output;

fn require_capability(bit: u32, name: &str) {
    let status = fs::read_to_string("/proc/self/status").expect("read effective Linux capabilities");
    let mask = status.lines().find_map(|line| line.strip_prefix("CapEff:\t"))
        .expect("Linux process status contains CapEff");
    let mask = u64::from_str_radix(mask.trim(), 16).expect("parse effective capability mask");
    assert!(mask & (1u64 << bit) != 0, "privileged compatibility fixture requires {name}");
}

fn require_chroot() {
    require_capability(18, "CAP_SYS_CHROOT");
    assert_eq!(unsafe { libc::geteuid() }, 0, "chroot fixtures require root UID");
    assert_eq!(unsafe { libc::getegid() }, 0, "chroot fixtures require root GID");
}

fn run(name: &str, directory: &Path, args: &[&str]) -> Output {
    Running::spawn(applet(name, directory).args(args)).finish()
}

fn success(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn stdout_only(output: Output, expected: &[u8]) {
    success(&output);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
}

// The pinned Linux image's BusyBox supplies real command execution inside an
// owned jail. An explicit fixture directory supplies that same binary and
// loader when the reference applet runs in an image with a different libc.
fn jail() -> tempfile::TempDir {
    require_chroot();
    let directory = tempfile::tempdir().expect("create owned chroot fixture");
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();
    for subdirectory in ["CHROOT_DIR", "CHROOT_DIR/bin", "CHROOT_DIR/lib", "CHROOT_DIR/etc"] {
        fs::create_dir(directory.path().join(subdirectory)).unwrap();
    }
    let root = directory.path().join("CHROOT_DIR");
    let tools = std::env::var_os("XSH_CORE_COMPAT_JAIL_TOOLS");
    let binary_directory = tools.as_deref().map(Path::new).unwrap_or(Path::new("/bin"));
    let loader_directory = tools.as_deref().map(Path::new).unwrap_or(Path::new("/lib"));
    fs::copy(binary_directory.join("busybox"), root.join("bin/busybox"))
        .expect("jail fixture must provide the pinned image's busybox");
    for tool in ["whoami", "id", "pwd"] {
        symlink("busybox", root.join("bin").join(tool)).unwrap();
    }
    let mut loaders = 0;
    for entry in fs::read_dir(loader_directory).expect("read jail fixture loader directory") {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if name_text.starts_with("ld-musl-") && name_text.ends_with(".so.1") {
            fs::copy(entry.path(), root.join("lib").join(name)).unwrap();
            loaders += 1;
        }
    }
    assert!(loaders > 0, "populated jail requires the pinned image's musl loader");
    fs::write(root.join("etc/passwd"), b"root:x:0:0:root:/:/bin/sh\nsync:x:3:65534:sync:/:/bin/sh\n").unwrap();
    fs::write(root.join("etc/group"), b"root:x:0:\nnogroup:x:65534:sync\n").unwrap();
    directory
}

fn chroot(directory: &Path, args: &[&str]) -> Output {
    let mut command = applet("chroot", directory);
    command.args(args).env("PATH", "/bin");
    Running::spawn(&mut command).finish()
}

// origin: uutils test_chown::test_chown_only_user_id_nonexistent_user
#[test]
fn test_chown_only_user_id_nonexistent_user() {
    require_capability(0, "CAP_CHOWN");
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("f"), []).unwrap();
    stdout_only(run("chown", directory.path(), &["12345", "f"]), b"");
}

// origin: uutils test_chown::test_chown_only_group_id_nonexistent_group
#[test]
fn test_chown_only_group_id_nonexistent_group() {
    require_capability(0, "CAP_CHOWN");
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("f"), []).unwrap();
    stdout_only(run("chown", directory.path(), &[":12345", "f"]), b"");
}

// origin: uutils test_mknod::test_mknod_mode_permissions
#[test]
fn test_mknod_mode_permissions() {
    require_capability(27, "CAP_MKNOD");
    for mode in [0o666, 0o000, 0o444, 0o004, 0o040, 0o400, 0o644] {
        let directory = tempfile::tempdir().unwrap();
        let mode_arg = format!("{mode:04o}");
        let filename = format!("null_file-{mode_arg}");
        let output = run("mknod", directory.path(), &["--mode", &mode_arg, &filename, "c", "1", "3"]);
        success(&output);
        assert!(output.stdout.is_empty());
        let metadata = fs::metadata(directory.path().join(filename)).unwrap();
        assert!(metadata.file_type().is_char_device());
        assert_eq!(metadata.permissions().mode() & 0o777, mode);
        assert_eq!(metadata.rdev(), libc::makedev(1, 3));
    }
}

// origin: uutils test_ls::test_ls_capabilities
#[test]
fn test_ls_capabilities() {
    require_capability(31, "CAP_SETFCAP");
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("test/dir")).unwrap();
    for filename in ["test/cap_pos.txt", "test/dir/cap_neg.txt", "test/dir/cap_pos.txt"] {
        fs::write(directory.path().join(filename), []).unwrap();
    }
    // Linux v2 file capabilities: effective flag and CAP_NET_BIND_SERVICE in
    // the low permitted word, with no inheritable or high capability bits.
    let words = [0x02000001u32, 1 << 10, 0, 0, 0];
    let capability: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    for filename in ["test/cap_pos.txt", "test/dir/cap_pos.txt"] {
        let filename = CString::new(directory.path().join(filename).as_os_str().as_bytes()).unwrap();
        let result = unsafe {
            libc::setxattr(filename.as_ptr(), c"security.capability".as_ptr(), capability.as_ptr().cast(), capability.len(), 0)
        };
        assert_eq!(result, 0, "install owned security.capability: {}", std::io::Error::last_os_error());
    }
    let mut command = applet("ls", directory.path());
    command.env("LS_COLORS", "di=:ca=30;41").args(["--color=always", "test/cap_pos.txt", "test/dir"]);
    let output = Running::spawn(&mut command).finish();
    success(&output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\x1b[30;41mtest/cap_pos.txt"));
    assert!(stdout.contains("\x1b[30;41mcap_pos.txt"));
    assert!(!stdout.contains("0;41mcap_neg.txt"));
    let mut command = applet("ls", directory.path());
    command.env("LS_COLORS", "di=:no=30;41:*.txt=31;41").args(["--color=always", "test/cap_pos.txt"]);
    let output = Running::spawn(&mut command).finish();
    success(&output);
    assert!(String::from_utf8(output.stdout).unwrap().contains("\x1b[31;41mtest/cap_pos.txt"));
}

// origin: uutils test_chroot::test_chroot
#[test]
fn test_chroot() {
    let directory = jail();
    stdout_only(chroot(directory.path(), &["CHROOT_DIR", "whoami"]), b"root\n");
    stdout_only(chroot(directory.path(), &["CHROOT_DIR", "pwd"]), b"/\n");
}

// origin: uutils test_chroot::test_chroot_retains_uid_gid
#[test]
fn test_chroot_retains_uid_gid() {
    let directory = jail();
    for flag in ["-u", "-g"] {
        stdout_only(chroot(directory.path(), &["CHROOT_DIR", "id", flag]), b"0\n");
    }
}

// origin: uutils test_chroot::test_chroot_command_not_found_error
#[test]
fn test_chroot_command_not_found_error() {
    let directory = jail();
    let output = chroot(directory.path(), &["CHROOT_DIR", "definitely_missing_command"]);
    assert_eq!(output.status.code(), Some(127));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("failed to run command 'definitely_missing_command'"));
    assert!(stderr.contains("No such file or directory"));
}

// origin: uutils test_chroot::test_chroot_command_permission_denied_error
#[test]
fn test_chroot_command_permission_denied_error() {
    let directory = jail();
    let script = directory.path().join("CHROOT_DIR/noexec.sh");
    fs::write(&script, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(script, fs::Permissions::from_mode(0o644)).unwrap();
    let output = chroot(directory.path(), &["CHROOT_DIR", "/noexec.sh"]);
    assert_eq!(output.status.code(), Some(126));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("failed to run command '/noexec.sh'"));
    assert!(stderr.contains("Permission denied"));
}

// origin: uutils test_chroot::test_default_shell
#[test]
fn test_default_shell() {
    let directory = jail();
    let mut command = applet("chroot", directory.path());
    command.arg("CHROOT_DIR").env("SHELL", "/bin/sh");
    let output = Running::spawn(&mut command).finish();
    assert!(String::from_utf8(output.stderr).unwrap().contains("chroot: failed to run command '/bin/sh': No such file or directory"));
}

// origin: uutils test_chroot::test_invalid_user_spec
#[test]
fn test_invalid_user_spec() {
    let directory = jail();
    for (spec, error) in [("--userspec=ARABA:", "invalid user"), ("--userspec=ARABA:ARABA", "invalid user"), ("--userspec=:ARABA", "invalid group")] {
        let output = chroot(directory.path(), &[spec, "CHROOT_DIR"]);
        assert_eq!(output.status.code(), Some(125));
        assert_eq!(output.stderr, format!("chroot: {error}\n").as_bytes());
    }
}

// origin: uutils test_chroot::test_invalid_user
#[test]
fn test_invalid_user() {
    require_capability(6, "CAP_SETGID");
    require_capability(7, "CAP_SETUID");
    let directory = jail();
    stdout_only(chroot(directory.path(), &["CHROOT_DIR", "whoami"]), b"root\n");
    // Account resolution can use the outer database when nobody is absent
    // inside the jail; the '+' prefix forces a numeric group ID.
    stdout_only(chroot(directory.path(), &["--user=nobody:+65535", "CHROOT_DIR", "pwd"]), b"/\n");
}

// origin: uutils test_chroot::test_multiple_group_args
#[test]
fn test_multiple_group_args() {
    require_capability(6, "CAP_SETGID");
    let directory = jail();
    stdout_only(chroot(directory.path(), &["--groups=invalid ignored", "--groups=", "CHROOT_DIR", "id", "-G"]), b"0\n");
}

// origin: uutils test_chroot::test_chroot_userspec_unknown_uid
#[test]
fn test_chroot_userspec_unknown_uid() {
    let directory = jail();
    let output = chroot(directory.path(), &["--userspec=99999", "--groups=root", "CHROOT_DIR", "id", "-g"]);
    assert_eq!(output.status.code(), Some(125));
    assert_eq!(output.stderr, b"chroot: no group specified for unknown uid: 99999\n");
}

// origin: uutils test_chroot::test_chroot_userspec_does_not_set_gid_with_uid
#[test]
fn test_chroot_userspec_does_not_set_gid_with_uid() {
    require_capability(6, "CAP_SETGID");
    require_capability(7, "CAP_SETUID");
    let directory = jail();
    stdout_only(chroot(directory.path(), &["--userspec=sync", "CHROOT_DIR", "id", "-g"]), b"65534\n");
}

// origin: uutils test_chroot::test_chroot_extra_arg
#[test]
fn test_chroot_extra_arg() {
    let directory = jail();
    stdout_only(chroot(directory.path(), &["CHROOT_DIR", "pwd", "-P"]), b"/\n");
}

// origin: uutils test_chroot::test_chroot_skip_chdir
#[test]
fn test_chroot_skip_chdir() {
    require_chroot();
    let directory = tempfile::tempdir().unwrap();
    symlink("/", directory.path().join("isroot")).unwrap();
    let cwd = fs::canonicalize(directory.path()).unwrap();
    let mut expected = cwd.as_os_str().as_bytes().to_vec();
    expected.push(b'\n');
    for root in ["/", "/.", "/..", "isroot"] {
        stdout_only(chroot(directory.path(), &["--skip-chdir", root, "/bin/pwd", "-P"]), &expected);
    }
}
