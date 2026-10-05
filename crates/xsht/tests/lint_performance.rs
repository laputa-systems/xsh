use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

// The budget covers process startup, configured discovery, checking, and linting
// with the release `xsht`. Cargo compilation and fixture setup finish before the
// clock starts. Debug builds are not measured.
const REPOSITORY_LINT_BUDGET: Duration = Duration::from_secs(15);

struct LintRun {
    elapsed: Duration,
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn run_lint(root: &Path, budget: Duration) -> Result<LintRun, String> {
    let capture = TempDir::new().expect("lint output directory");
    let stdout_path = capture.path().join("stdout");
    let stderr_path = capture.path().join("stderr");
    let stdout = File::create(&stdout_path).expect("lint stdout capture");
    let stderr = File::create(&stderr_path).expect("lint stderr capture");
    let started = Instant::now();
    let mut child = Command::new(release_bin!("xsht"))
        .arg("lint")
        .current_dir(root)
        .env_remove("XSH_MODULE_PATH")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .expect("start xsht lint");
    let status = loop {
        if started.elapsed() >= budget {
            let _ = child.kill();
            let status = child.wait().expect("reap timed-out xsht lint");
            return Err(format!(
                "xsht lint in {} exceeded {:.3}s wall budget ({:.3}s elapsed); terminated and reaped ({status})\nstdout:\n{}\nstderr:\n{}",
                root.display(),
                budget.as_secs_f64(),
                started.elapsed().as_secs_f64(),
                String::from_utf8_lossy(&fs::read(&stdout_path).expect("read lint stdout")),
                String::from_utf8_lossy(&fs::read(&stderr_path).expect("read lint stderr")),
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("poll xsht lint in {}: {error}", root.display()));
            }
        }
    };
    Ok(LintRun {
        elapsed: started.elapsed(),
        status,
        stdout: fs::read(stdout_path).expect("read lint stdout"),
        stderr: fs::read(stderr_path).expect("read lint stderr"),
    })
}

fn require_clean_lint(run: &LintRun) -> Result<(), String> {
    if run.status.success()
        && run.stdout.is_empty()
        && crate::stderr_before_timing_line("lint", &run.stderr).is_empty()
    {
        return Ok(());
    }
    Err(format!(
        "xsht lint failed the clean corpus gate ({}, {:.3}s elapsed)\nstdout:\n{}\nstderr:\n{}",
        run.status,
        run.elapsed.as_secs_f64(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    ))
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only performance gate: run `cargo dev check lint` or `cargo test --release`"
)]
fn repository_lint_is_clean_within_wall_budget() {
    let run = run_lint(&workspace_root(), REPOSITORY_LINT_BUDGET)
        .unwrap_or_else(|failure| panic!("{failure}"));
    require_clean_lint(&run).unwrap_or_else(|failure| panic!("{failure}"));
    eprintln!(
        "repository xsht lint: {:.3}s / {:.3}s wall budget",
        run.elapsed.as_secs_f64(),
        REPOSITORY_LINT_BUDGET.as_secs_f64(),
    );
}

#[test]
fn lint_gate_rejects_imported_diagnostics_without_writing_sources() {
    let root = TempDir::new().expect("isolated lint workspace");
    let sources = [
        ("xsht-config.ini", "module_path = .\n"),
        ("main.xsh", "use helper\nprint helper.value\n"),
        (
            "helper.xsh",
            "##! Helper module.\n## Exports a value.\nexport let value = 1\n\npure unused() -> Int {\n  return 1\n}\n",
        ),
    ];
    for (path, source) in sources {
        fs::write(root.path().join(path), source).expect("write lint source");
    }
    let run =
        run_lint(root.path(), Duration::from_secs(5)).unwrap_or_else(|failure| panic!("{failure}"));
    assert_eq!(
        run.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let failure = require_clean_lint(&run).expect_err("diagnostics must reject the lint gate");
    assert!(failure.contains("lint.unused-callable"), "{failure}");
    for (path, source) in sources {
        assert_eq!(
            fs::read_to_string(root.path().join(path)).expect("read lint source"),
            source,
            "read-only lint changed {path}",
        );
    }
}

#[test]
fn lint_gate_obeys_configured_diagnostic_fixture_exclusions() {
    let root = TempDir::new().expect("isolated lint discovery workspace");
    let config = root.path().join("xsht-config.ini");
    fs::write(&config, "exclude = fixtures/**/*.xsh\n").expect("write lint discovery config");
    fs::write(root.path().join("main.xsh"), "print \"ready\"\n")
        .expect("write maintained lint source");
    fs::create_dir(root.path().join("fixtures")).expect("create diagnostic fixture directory");
    let fixture = root.path().join("fixtures/invalid.xsh");
    let invalid_source = "let value: Int = \"wrong\"\n";
    fs::write(&fixture, invalid_source).expect("write diagnostic lint fixture");

    let clean =
        run_lint(root.path(), Duration::from_secs(5)).unwrap_or_else(|failure| panic!("{failure}"));
    require_clean_lint(&clean).unwrap_or_else(|failure| panic!("{failure}"));

    fs::write(&config, "").expect("include diagnostic fixtures in lint discovery");
    let rejected =
        run_lint(root.path(), Duration::from_secs(5)).unwrap_or_else(|failure| panic!("{failure}"));
    assert_eq!(
        rejected.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    let failure =
        require_clean_lint(&rejected).expect_err("checker errors must reject the lint gate");
    assert!(failure.contains("check.type-mismatch"), "{failure}");
    assert_eq!(
        fs::read_to_string(fixture).expect("read diagnostic fixture"),
        invalid_source
    );
}

#[test]
fn lint_gate_terminates_and_reaps_process_at_wall_deadline() {
    let root = TempDir::new().expect("isolated lint deadline workspace");
    fs::write(root.path().join("main.xsh"), "print \"ready\"\n")
        .expect("write lint deadline source");
    let failure = run_lint(root.path(), Duration::ZERO)
        .err()
        .expect("zero wall budget must reject lint");
    assert!(failure.contains("exceeded 0.000s wall budget"), "{failure}");
    assert!(failure.contains("terminated and reaped"), "{failure}");
}
