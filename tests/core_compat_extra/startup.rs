use super::*;

fn close_standard(command: &mut Command, fd: i32) {
    // SAFETY: the close runs only in the child after Cargo's pipe setup.
    unsafe { command.pre_exec(move || { libc::close(fd); Ok(()) }); }
}

#[test]
fn gnu_misc_close_stdout_preserves_initial_absence() {
    let directory = tempfile::tempdir().unwrap();
    for (name, args, expected) in [
        ("true", vec![], 0),
        ("false", vec![], 1),
        ("echo", vec!["output"], 1),
        ("dd", vec!["--help"], 1),
    ] {
        let mut command = applet(name, directory.path());
        command.args(args);
        close_standard(&mut command, 1);
        let output = Running::spawn(&mut command).finish();
        assert_eq!(output.status.code(), Some(expected), "{name}");
        assert!(output.stdout.is_empty(), "{name}");
    }
}

#[test]
fn gnu_dd_misc_preserves_initial_stderr_absence() {
    let directory = tempfile::tempdir().unwrap();
    for (args, expected) in [(vec!["--help"], 0), (vec![], 1)] {
        let mut command = applet("dd", directory.path());
        command.args(args);
        close_standard(&mut command, 2);
        let output = Running::spawn(&mut command).finish();
        assert_eq!(output.status.code(), Some(expected));
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn gnu_touch_dash_closed_stdout() {
    let directory = tempfile::tempdir().unwrap();
    for (args, expected) in [
        (vec!["-c", "-"], 0), (vec!["-cm", "-"], 0),
        (vec!["-ca", "-"], 0), (vec!["-h", "-"], 1),
        (vec!["-ch", "-"], 0),
    ] {
        let mut command = applet("touch", directory.path());
        command.args(args);
        close_standard(&mut command, 1);
        let output = Running::spawn(&mut command).finish();
        assert_eq!(output.status.code(), Some(expected));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn gnu_misc_no_fork_runs_initial_evaluation_when_threads_are_forbidden() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("env", directory.path());
    command.arg("/bin/true");
    // RLIMIT_NPROC counts threads for the real UID. Drop root in this child
    // only, because the kernel exempts privileged processes from that limit.
    unsafe { command.pre_exec(|| {
        if libc::geteuid() == 0 {
            if libc::setgid(1000) == -1 || libc::setuid(1000) == -1 {
                return Err(std::io::Error::last_os_error());
            }
        }
        let limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if libc::setrlimit(libc::RLIMIT_NPROC, &limit) == -1 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }); }
    // The unprivileged child must traverse the fixture's cwd.
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = Running::spawn(&mut command).finish();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
}

#[test]
fn absent_stdin_does_not_alias_loaded_source_and_can_be_reopened() {
    if std::env::var_os("XSH_CORE_COMPAT_REFERENCE_DIR").is_some() { return; }
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("stdin.xsh");
    let input = directory.path().join("input");
    std::fs::write(&input, b"reopened").unwrap();
    // An external child must close fd 0 before exec; native captured input
    // cannot express the descriptor state that Rust initializes at startup.
    std::fs::write(&script, "assert io.stdin_bytes() is Err(is HostIo)\nunix.redirect_fd(0, Path(args[0]))?\nio.write_stdout_bytes(io.stdin_bytes()?)?\nio.flush_stdout()?\n").unwrap();
    let mut command = Command::new(release_bin!("xsh"));
    command.arg(script).arg("--").arg(input)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    close_standard(&mut command, 0);
    let output = Running::spawn(&mut command).finish();
    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"reopened");
}

#[test]
fn absent_stdout_can_be_redirected_and_inherited_by_exec() {
    if std::env::var_os("XSH_CORE_COMPAT_REFERENCE_DIR").is_some() { return; }
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("stdout.xsh");
    let log = directory.path().join("output");
    // Startup descriptor absence and exec inheritance require a real process;
    // the script only supplies the typed descriptor replacement operation.
    std::fs::write(&script, "unix.redirect_fd(1, Path(args[0]), write: true)?\nunix.dup_fd(1, 2)?\nunix.exec(process.command_argv(\"sh\", [\"sh\", \"-c\", \"printf output; printf error >&2\"]))?\n").unwrap();
    let mut command = Command::new(release_bin!("xsh"));
    command.arg(script).arg("--").arg(&log)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    close_standard(&mut command, 1);
    close_standard(&mut command, 2);
    let output = Running::spawn(&mut command).finish();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(std::fs::read(log).unwrap(), b"outputerror");
}
