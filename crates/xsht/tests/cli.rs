#![allow(clippy::single_call_fn)]

use std::fs;
use std::process::Command;
use std::sync::Mutex;
use tempfile::TempDir;

static SIGNAL_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn xsht_top_level_help_lists_subcommands_only() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .arg("-h")
        .output()
        .expect("run xsht help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("lint"));
    assert!(stdout.contains("Run `xsht COMMAND --help`"));
    assert!(!stdout.contains("--runless"));
    assert!(!stdout.contains("--trace-format"));
}

#[test]
fn xsht_lint_help_is_subcommand_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["lint", "--help"])
        .output()
        .expect("run xsht lint help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage:\n  xsht lint"));
    assert!(stdout.contains("--fix"));
    assert!(stdout.contains("--runless"));
    assert!(!stdout.contains("xsht trace"));
}

#[test]
fn xsht_lint_short_help_is_accepted() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["lint", "-h"])
        .output()
        .expect("run xsht lint short help");

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("xsht lint [--fix] [--runless] [FILE...]")
    );
}

#[test]
fn fmt_uses_nearest_xsht_config_line_width() {
    let root = TempDir::new().expect("create temp root");
    let narrow = root.path().join("narrow");
    fs::create_dir_all(&narrow).expect("create narrow dir");
    fs::write(
        root.path().join("xsht-config.ini"),
        "[format]\nline-width = 120\n",
    )
    .expect("write root config");
    fs::write(
        narrow.join("xsht-config.ini"),
        "[format]\nline-width = 60\n",
    )
    .expect("write narrow config");
    let script = narrow.join("main.xsh");
    fs::write(
        &script,
        "let values = [\"alpha\", \"beta\", \"gamma\", \"delta\", \"epsilon\", \"zeta\"]\n",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", script.to_str().unwrap()])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let formatted = fs::read_to_string(&script).expect("read formatted script");
    assert_eq!(
        formatted,
        "let values = [\n  \"alpha\",\n  \"beta\",\n  \"gamma\",\n  \"delta\",\n  \"epsilon\",\n  \"zeta\",\n]\n"
    );
}

#[test]
fn fmt_reports_invalid_xsht_config_line_width() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("xsht-config.ini"),
        "[format]\nline-width = nope\n",
    )
    .expect("write config");
    let script = root.path().join("main.xsh");
    fs::write(&script, "let value = 1\n").expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", "--check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("format.line-width"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fmt_ignores_legacy_config_ini() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("config.ini"),
        "[format]\nline-width = 60\n",
    )
    .expect("write legacy config");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "let values = [\"alpha\", \"beta\", \"gamma\", \"delta\", \"epsilon\", \"zeta\"]\n",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", "--check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_annotate_uses_xsht_config_line_width() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("xsht-config.ini"),
        "[format]\nline-width = 60\n",
    )
    .expect("write config");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc local(input = Path(\".\"), source = Path(\".\"), destination = Path(\".\")) {}\n",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "--annotate", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let annotated = fs::read_to_string(&script).expect("read annotated script");
    assert_eq!(
        annotated,
        "proc local(\n  input: Path = Path(\".\"),\n  source: Path = Path(\".\"),\n  destination: Path = Path(\".\"),\n) {}\n"
    );
}

#[test]
fn lint_returns_interrupted_status_for_pending_sigint() {
    let _lock = SIGNAL_TEST_LOCK.lock().unwrap();
    let _guard = xsh::runtime::process::install_cancellation_signal_handlers()
        .expect("install cancellation signal handlers");
    xsh::runtime::process::clear_cancellation_request();
    let kill_result = unsafe { libc::kill(libc::getpid(), libc::SIGINT) };
    assert_eq!(kill_result, 0);

    let output = xsht::cli::lint_files(&["unused.xsh".to_string()], false, false);
    xsh::runtime::process::clear_cancellation_request();

    assert_eq!(output.status, 130);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("interrupted by SIGINT")
    );
}
