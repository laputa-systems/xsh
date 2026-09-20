//! A19 — repeated program preparation and teardown leave no stale symbol
//! ownership or caller state.
//!
//! This lives in its own test binary because it samples a process-global
//! counter: any other test preparing a program at the same time would perturb
//! the reading, and a single-test binary has no in-process concurrency.

use std::io::Write;
use std::path::Path;

use xsh::execution::script::{RunOptions, run_script};
use xsh::frontend::symbols::dynamic_symbol_stats;

fn write_script(dir: &Path, name: &str, source: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).expect("create script");
    file.write_all(source.as_bytes()).expect("write script");
    path
}

#[test]
fn repeated_preparation_returns_to_a_stable_symbol_plateau() {
    let dir = std::env::temp_dir().join(format!("xsh-symbol-plateau-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let script = write_script(
        &dir,
        "plateau.xsh",
        "proc main() [io] {\n  print shlex.quote(\"a b\")\n  print bytes.human(2048)\n}\n",
    );

    // Warm up so one-time process state is not counted as growth.
    for _ in 0..3 {
        let output = run_script(RunOptions {
            script: script.display().to_string(),
            args: Vec::new(),
            coverage_trace_dir: None,
        });
        assert_eq!(output.status, 0);
    }
    let (baseline, _) = dynamic_symbol_stats();

    for _ in 0..20 {
        let output = run_script(RunOptions {
            script: script.display().to_string(),
            args: Vec::new(),
            coverage_trace_dir: None,
        });
        assert_eq!(output.status, 0);
    }
    let (after, _) = dynamic_symbol_stats();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        after <= baseline,
        "twenty prepared runs left {after} live dynamic symbols against a plateau of {baseline}"
    );
}

