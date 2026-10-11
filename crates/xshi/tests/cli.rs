#[macro_use]
#[path = "../../../tests/release_binary.rs"]
mod release_binary;

use std::os::unix::ffi::OsStringExt;
use std::process::{Command, Output};

/// Runs `xshi` against a private HOME so no test reads or writes the
/// developer's history, trust, or configuration.
fn xshi(args: &[&str]) -> (Output, tempfile::TempDir) {
    let home = tempfile::tempdir().expect("temporary HOME");
    let output = Command::new(release_bin!("xshi"))
        .args(args)
        .env("HOME", home.path())
        .env("USER", "testuser")
        .output()
        .expect("run xshi");
    (output, home)
}

#[test]
fn xshi_reports_non_utf8_command_without_panicking() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let raw_command = std::ffi::OsString::from_vec(b"print \"\xff\"".to_vec());
    let output = Command::new(release_bin!("xshi"))
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
        Command::new(release_bin!("xshi"))
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
