use super::{applet, Running};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;

fn copied(command: &mut std::process::Command) {
    let output = Running::spawn(command).finish();
    assert!(output.status.success(), "cp failed: {}", String::from_utf8_lossy(&output.stderr));
}

// origin: uutils test_cp::test_closes_file_descriptors
#[cfg(target_os = "linux")]
#[test]
fn test_closes_file_descriptors() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("dir_with_10_files");
    std::fs::create_dir(&source).unwrap();
    for number in 0..10 {
        std::fs::write(source.join(number.to_string()), b"").unwrap();
    }
    // Other tests can own sockets and pipes concurrently. The child budget
    // includes the parent's open descriptors as well as nine copy descriptors.
    let limit = std::fs::read_dir(format!("/proc/{}/fd", std::process::id())).unwrap().count() as libc::rlim_t + 9;
    let mut command = applet("cp", directory.path());
    command.args(["-r", "--reflink=auto", "dir_with_10_files/", "dir_with_10_files_new/"]);
    unsafe {
        command.pre_exec(move || {
            let resource = libc::rlimit { rlim_cur: limit, rlim_max: limit };
            if libc::setrlimit(libc::RLIMIT_NOFILE, &resource) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    copied(&mut command);
}

fn deleted_cwd(target_exists: bool) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("src");
    let target = directory.path().join("dst");
    let removed = directory.path().join("deleted-cwd");
    std::fs::create_dir_all(source.join("sub")).unwrap();
    std::fs::write(source.join("sub/file"), b"contents").unwrap();
    std::fs::create_dir(&removed).unwrap();
    if target_exists { std::fs::create_dir(&target).unwrap(); }
    let removed_bytes = CString::new(removed.as_os_str().as_bytes()).unwrap();
    let mut command = applet("cp", &removed);
    command.args(["-Ra", "--no-preserve=ownership"]).arg(&source).arg(&target);
    // Removing only the child's already selected cwd preserves the parent's
    // working directory and exercises exec with no resolvable cwd pathname.
    unsafe {
        command.pre_exec(move || {
            if libc::rmdir(removed_bytes.as_ptr()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    copied(&mut command);
    let relative = if target_exists { "src/sub/file" } else { "sub/file" };
    assert_eq!(std::fs::read(target.join(relative)).unwrap(), b"contents");
}

// origin: uutils test_cp::test_cp_absolute_paths_from_deleted_cwd::case_1_existing_target
#[test]
fn test_cp_absolute_paths_from_deleted_cwd_existing_target() {
    deleted_cwd(true);
}

// origin: uutils test_cp::test_cp_absolute_paths_from_deleted_cwd::case_2_new_target
#[test]
fn test_cp_absolute_paths_from_deleted_cwd_new_target() {
    deleted_cwd(false);
}

#[cfg(all(target_os = "linux", feature = "linux-priv-tests"))]
mod privileged {
    use super::*;
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    fn device(flag: &str, character: bool) {
        assert_eq!(unsafe { libc::geteuid() }, 0, "BLOCKED: device preservation requires root and CAP_MKNOD");
        let directory = tempfile::tempdir().unwrap();
        let (source, target, kind, mode, major, minor) = if character {
            ("null", "null2", libc::S_IFCHR, 0o640, 1, 3)
        } else {
            ("sda", "sda2", libc::S_IFBLK, 0o600, 8, 0)
        };
        let source_path = directory.path().join(source);
        let bytes = CString::new(source_path.as_os_str().as_bytes()).unwrap();
        let result = unsafe { libc::mknod(bytes.as_ptr(), kind | mode, libc::makedev(major, minor)) };
        assert_eq!(result, 0, "BLOCKED: create device fixture (CAP_MKNOD): {}", std::io::Error::last_os_error());
        // Set the intended source mode independently of the test runner's umask.
        std::fs::set_permissions(&source_path, std::fs::Permissions::from_mode(mode)).unwrap();
        let mut command = applet("cp", directory.path());
        command.args([flag, source, target]);
        copied(&mut command);
        let original = std::fs::metadata(source_path).unwrap();
        let duplicate = std::fs::metadata(directory.path().join(target)).unwrap();
        if character {
            assert!(duplicate.file_type().is_char_device());
            assert_eq!(duplicate.permissions().mode() & 0o777, 0o640);
        } else {
            assert!(duplicate.file_type().is_block_device());
        }
        assert_eq!(duplicate.rdev(), original.rdev());
    }

    // origin: uutils test_cp::test_cp_recursive_char_device::case_1_recursive
    #[test]
    fn test_cp_recursive_char_device_recursive() { device("-R", true); }

    // origin: uutils test_cp::test_cp_recursive_char_device::case_2_archive
    #[test]
    fn test_cp_recursive_char_device_archive() { device("-a", true); }

    // origin: uutils test_cp::test_cp_recursive_block_device::case_1_recursive
    #[test]
    fn test_cp_recursive_block_device_recursive() { device("-R", false); }

    // origin: uutils test_cp::test_cp_recursive_block_device::case_2_archive
    #[test]
    fn test_cp_recursive_block_device_archive() { device("-a", false); }

    // Mounts belong to a private child namespace. A failed assertion can never
    // leave a mount in the runner's namespace or propagate one to the host.
    fn mount_worker(name: &str, fixture: impl FnOnce()) {
        if std::env::var("XSH_CP_MOUNT_WORKER").as_deref() == Ok(name) {
            fixture();
            return;
        }
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", name, "--nocapture"])
            .env("XSH_CP_MOUNT_WORKER", name)
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        unsafe {
            command.pre_exec(|| {
                if libc::unshare(libc::CLONE_NEWNS) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::mount(std::ptr::null(), c"/".as_ptr(), std::ptr::null(),
                    libc::MS_REC | libc::MS_PRIVATE, std::ptr::null()) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().expect("BLOCKED: private mount namespace requires CAP_SYS_ADMIN");
        let output = Running(child).finish();
        assert!(output.status.success(), "private mount worker failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"), "mount worker did not run {name}");
    }

    struct Mount(PathBuf);

    impl Mount {
        fn unmount(self) {
            let bytes = CString::new(self.0.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::umount(bytes.as_ptr()) }, 0,
                "unmount owned fixture: {}", std::io::Error::last_os_error());
        }
    }

    impl Drop for Mount {
        fn drop(&mut self) {
            let bytes = CString::new(self.0.as_os_str().as_bytes()).unwrap();
            // Detach on unwinding before TempDir cleanup can traverse a mount.
            unsafe { libc::umount2(bytes.as_ptr(), libc::MNT_DETACH); }
        }
    }

    fn assert_tree(source: &Path, target: &Path) {
        assert!(target.is_dir(), "missing copied directory {}", target.display());
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let copied = target.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                assert_tree(&entry.path(), &copied);
            } else {
                assert!(entry.file_type().unwrap().is_file());
                assert!(copied.is_file(), "missing copied file {}", copied.display());
            }
        }
    }

    // origin: uutils test_cp::test_cp_one_file_system
    #[test]
    fn test_cp_one_file_system() {
        mount_worker("cp::privileged::test_cp_one_file_system", || {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("dir_with_mount");
            let target = directory.path().join("copy_to_folder_new");
            std::fs::create_dir_all(source.join("copy_me")).unwrap();
            for relative in ["copy_me.txt", "copy_me/copy_me.txt"] {
                std::fs::write(source.join(relative), b"").unwrap();
            }
            let mountpoint = source.join("mount");
            std::fs::create_dir(&mountpoint).unwrap();
            let bytes = CString::new(mountpoint.as_os_str().as_bytes()).unwrap();
            let result = unsafe { libc::mount(c"tmpfs".as_ptr(), bytes.as_ptr(), c"tmpfs".as_ptr(), 0, std::ptr::null()) };
            assert_eq!(result, 0, "BLOCKED: mount tmpfs: {}", std::io::Error::last_os_error());
            let mount = Mount(mountpoint.clone());
            std::fs::write(mountpoint.join("DO_NOT_copy_me.txt"), b"").unwrap();
            let mut command = applet("cp", directory.path());
            command.args(["-rx", "dir_with_mount", "copy_to_folder_new"]);
            copied(&mut command);
            mount.unmount();
            assert!(!target.join("mount/DO_NOT_copy_me.txt").exists());
            assert_tree(&source, &target);
        });
    }

    fn fixture_command(name: &str, directory: &Path, args: &[&str]) {
        let mut command = Command::new(name);
        command.current_dir(directory).args(args)
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = command.spawn().unwrap_or_else(|error| panic!("BLOCKED: fixture tool {name}: {error}"));
        let output = Running(child).finish();
        assert!(output.status.success(), "BLOCKED: fixture tool {name}: {}", String::from_utf8_lossy(&output.stderr));
    }

    // origin: uutils test_cp::test_cp_reflink_always_override
    #[test]
    fn test_cp_reflink_always_override() {
        mount_worker("cp::privileged::test_cp_reflink_always_override", || {
            let directory = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(directory.path().join("disk_root/dir")).unwrap();
            std::fs::File::create(directory.path().join("disk.img")).unwrap().set_len(128 * 1024 * 1024).unwrap();
            fixture_command("mkfs.btrfs", directory.path(), &["--rootdir", "disk_root/", "disk.img"]);
            let mountpoint = directory.path().join("mountpoint");
            std::fs::create_dir(&mountpoint).unwrap();
            fixture_command("mount", directory.path(), &["-o", "loop", "disk.img", "mountpoint/"]);
            let mount = Mount(mountpoint.clone());
            std::fs::write(mountpoint.join("dir/src1"), [0x64; 8192]).unwrap();
            std::fs::write(mountpoint.join("dir/src2"), b"other data").unwrap();
            for source in ["mountpoint/dir/src1", "mountpoint/dir/src2"] {
                let mut command = applet("cp", directory.path());
                command.args(["--reflink=always", source, "mountpoint/dir/dst"]);
                copied(&mut command);
            }
            mount.unmount();
        });
    }
}
