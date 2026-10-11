use super::Running;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

// The creation mask belongs to the child process. Native command plans cannot
// set it, so an owned launcher supplies that inherited descriptor constraint.
fn run_descriptor_script(directory: &Path, source: &str, mask: libc::mode_t) -> Output {
    let script = directory.join("descriptor.xsh");
    std::fs::write(&script, source).expect("write descriptor script");
    let mut command = Command::new(release_bin!("xsh"));
    command.arg(&script).current_dir(directory);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    unsafe {
        command.pre_exec(move || {
            libc::umask(mask);
            Ok(())
        });
    }
    Running::spawn(&mut command).finish()
}

#[test]
fn exact_descriptor_creation_mode_ignores_umask_and_preserves_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    let existing = directory.path().join("existing");
    std::fs::write(&existing, b"prefix").unwrap();
    std::fs::set_permissions(&existing, std::fs::Permissions::from_mode(0o660)).unwrap();
    let output = run_descriptor_script(directory.path(), r#"
unix.redirect_fd(100, p"new", write: true, append: true, mode: 0o600, exact_create_mode: true)?
assert unix.write_fd(100, b"new")? == 3
unix.close_fd(100)?
unix.redirect_fd(100, p"existing", write: true, append: true, mode: 0o600, exact_create_mode: true)?
assert unix.write_fd(100, b"suffix")? == 6
unix.close_fd(100)?
unix.redirect_fd(100, p"ordinary", write: true)?
unix.close_fd(100)?
"#, 0o600);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (name, mode) in [("new", 0o600), ("existing", 0o660), ("ordinary", 0o066)] {
        assert_eq!(std::fs::metadata(directory.path().join(name)).unwrap().permissions().mode() & 0o7777, mode, "{name}");
    }
    assert_eq!(std::fs::read(directory.path().join("new")).unwrap(), b"new");
    assert_eq!(std::fs::read(existing).unwrap(), b"prefixsuffix");
}

#[cfg(target_os = "linux")]
#[test]
fn saved_descriptor_is_closed_on_exec() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_descriptor_script(directory.path(), r#"
let saved = unix.duplicate_fd(2, min_fd: 100)?
assert unix.write_fd(saved, b"saved")? == 5
unix.exec(process.command_argv(p"/bin/sh", ["sh", "-c", "test ! -e /proc/self/fd/$1", "sh", f"{saved}"]))?
"#, 0o022);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stderr, b"saved");
}
