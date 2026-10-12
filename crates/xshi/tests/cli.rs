#[macro_use]
#[path = "../../../tests/test_binary.rs"]
mod test_binary;

use std::os::unix::ffi::OsStringExt;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::io::Write;
use std::process::{Command, Output, Stdio};

/// Runs `xshi` against a private HOME so no test reads or writes the
/// developer's history, trust, or configuration.
fn xshi(args: &[&str]) -> (Output, tempfile::TempDir) {
    let home = tempfile::tempdir().expect("temporary HOME");
    let output = Command::new(test_bin!("xshi"))
        .args(args)
        .env("HOME", home.path())
        .env("USER", "testuser")
        .current_dir(home.path())
        .output()
        .expect("run xshi");
    (output, home)
}

#[test]
fn xshi_reports_non_utf8_command_without_panicking() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let raw_command = std::ffi::OsString::from_vec(b"print \"\xff\"".to_vec());
    let output = Command::new(test_bin!("xshi"))
        .arg("-c")
        .arg(raw_command)
        .env("HOME", home.path())
        .output()
        .expect("run xshi");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "xshi: argument 2 is not valid UTF-8\n"
    );
}

#[test]
fn script_mode_is_refused_like_the_reference_shell() {
    let (output, _home) = xshi(&["script.sh"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("interactive-only"), "{stderr}");
    assert!(stderr.contains("usage: xshi"), "{stderr}");
    assert!(output.stdout.is_empty());
}

#[test]
fn version_and_help_need_no_terminal() {
    let (output, _home) = xshi(&["-V"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("xshi {}\n", env!("CARGO_PKG_VERSION"))
    );
    let (output, _home) = xshi(&["--version"]);
    assert!(output.status.success());

    for flag in ["-h", "--help"] {
        let (output, _home) = xshi(&[flag]);
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("usage: xshi"), "{stdout}");
        assert!(stdout.contains("--no-config"), "{stdout}");
    }
}

#[test]
fn missing_option_arguments_are_usage_errors() {
    for args in [&["-c"][..], &["--config"][..]] {
        let (output, _home) = xshi(args);
        assert_eq!(output.status.code(), Some(2));
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn command_mode_exit_builtin_sets_process_status() {
    let (output, _home) = xshi(&["-c", "exit 3"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn deep_type_diagnostic_survives_a_small_process_main_stack() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let source = format!(
        "let value = {}1{}; print $value",
        "[".repeat(120),
        "]".repeat(120)
    );
    let mut command = Command::new(test_bin!("xshi"));
    command
        .args(["--no-config", "-c", &source])
        .env("HOME", home.path())
        .current_dir(home.path());
    // The submitted program stays below XSH's nesting limit but forces the
    // checker to render a deeply nested value type before reporting the error.
    unsafe {
        command.pre_exec(|| {
            let stack_limit = libc::rlimit {
                rlim_cur: 256 * 1024,
                rlim_max: 256 * 1024,
            };
            if libc::setrlimit(libc::RLIMIT_STACK, &stack_limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let output = command.output().expect("run xshi with a small main stack");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("err[check.display-conversion]"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn interactive_session_preserves_its_exit_status_on_the_worker() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let mut child = Command::new(test_bin!("xshi"))
        .args(["--no-config"])
        .env("HOME", home.path())
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .current_dir(home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run xshi session");
    child
        .stdin
        .take()
        .expect("session stdin")
        .write_all(b"exit\n")
        .expect("submit exit command");
    let output = child.wait_with_output().expect("wait for xshi session");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
}

#[test]
fn explicit_config_replaces_the_default_and_warns_when_missing() {
    let home = tempfile::tempdir().expect("temporary HOME");
    std::fs::create_dir_all(home.path().join(".config/xshi")).unwrap();
    std::fs::write(
        home.path().join(".config/xshi/config.ish"),
        "alias probe echo from-default\n",
    )
    .unwrap();
    let custom = home.path().join("custom.ish");
    std::fs::write(&custom, "alias probe echo from-custom\n").unwrap();

    let run = |args: &[&str]| {
        Command::new(test_bin!("xshi"))
            .args(args)
            .env("HOME", home.path())
            .output()
            .expect("run xshi")
    };
    let default = run(&["-c", "probe"]);
    assert_eq!(String::from_utf8(default.stdout).unwrap(), "from-default\n");

    let explicit = run(&["--config", custom.to_str().unwrap(), "-c", "probe"]);
    assert_eq!(String::from_utf8(explicit.stdout).unwrap(), "from-custom\n");

    let none = run(&["--no-config", "-c", "probe"]);
    assert_eq!(none.status.code(), Some(127));

    let missing = run(&["--config", "/nonexistent/config.ish", "-c", "true"]);
    let stderr = String::from_utf8(missing.stderr).unwrap();
    assert!(stderr.contains("/nonexistent/config.ish"), "{stderr}");
    assert!(
        missing.status.success(),
        "a missing config is a warning, not fatal"
    );
}
