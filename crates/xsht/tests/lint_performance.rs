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

/// Whether every finding in a lint report is a note. A note is advice with no
/// safe rewrite and leaves the corpus clean; a rendered finding starts its
/// line with the severity and a bracketed code, or a colon.
fn reports_only_notes(report: &str) -> bool {
    report.lines().all(|line| {
        !["warn", "err", "info"].iter().any(|severity| {
            line.strip_prefix(severity)
                .is_some_and(|rest| rest.starts_with(['[', ':']))
        })
    })
}

fn require_clean_lint(run: &LintRun) -> Result<(), String> {
    if run.status.success()
        && run.stdout.is_empty()
        && reports_only_notes(crate::stderr_before_timing_line("lint", &run.stderr))
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
