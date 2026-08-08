use super::common::*;
use std::collections::BTreeSet;
use xsht::examples::{ExampleCatalog, OutputPolicy, load_catalog, validate_catalog};

fn example_catalog() -> ExampleCatalog {
    let catalog = load_catalog(".").expect("load examples/catalog.json");
    validate_catalog(".", &catalog).expect("validate examples/catalog.json");
    catalog
}

fn example_paths() -> Vec<String> {
    example_catalog()
        .examples
        .into_iter()
        .map(|case| case.path)
        .collect()
}

fn assert_output_policy(policy: &OutputPolicy, actual: &str, script: &str) {
    match policy {
        OutputPolicy::Exact(expected) => assert_eq!(actual, expected, "{script}"),
        OutputPolicy::Contains(expected) => {
            assert!(actual.contains(expected), "{script}: {actual}")
        }
        OutputPolicy::Empty => assert_eq!(actual, "", "{script}"),
        OutputPolicy::Any => {}
    }
}

#[test]
fn example_corpus_runs_with_expected_output() {
    for case in example_catalog().examples {
        if case.requires_net && !cfg!(feature = "net") {
            continue;
        }
        if case.skip {
            continue;
        }
        let mut command = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"));
        command.arg(&case.path);
        if !case.args.is_empty() {
            command.arg("--").args(&case.args);
        }
        let output = command.output().expect("run xsh");

        assert_eq!(
            output.status.code().unwrap_or_default(),
            case.expected_status,
            "{}",
            case.path
        );
        assert_output_policy(&case.stdout, &stdout_text(&output), &case.path);
        assert_output_policy(&case.stderr, &stderr_text(&output), &case.path);
    }
}

#[test]
fn examples_have_timed_trace_output() {
    for case in example_catalog()
        .examples
        .into_iter()
        .filter(|case| case.trace && (cfg!(feature = "net") || !case.requires_net))
    {
        let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
            .arg("trace")
            .arg(&case.path)
            .args(if case.args.is_empty() {
                &[][..]
            } else {
                &["--"][..]
            })
            .args(&case.args)
            .output()
            .expect("run xsht");

        assert!(
            output.status.success(),
            "{}\nstatus={:?}\nstdout:\n{}\nstderr:\n{}",
            case.path,
            output.status.code(),
            stdout_text(&output),
            stderr_text(&output),
        );
        let stderr = stderr_text(&output);
        assert!(stderr.contains("trace summary"), "{}: {stderr}", case.path);
        assert!(
            stderr.contains("script duration"),
            "{}: {stderr}",
            case.path
        );
        assert!(
            stderr.contains("function calls (duration µs)"),
            "{}: {stderr}",
            case.path
        );
        assert!(
            stderr.contains("hot commands (top 10 by total ms)"),
            "{}: {stderr}",
            case.path
        );
        assert!(stderr.contains('┌'), "{}: {stderr}", case.path);
        assert!(stderr.contains('│'), "{}: {stderr}", case.path);
        assert!(
            stderr.contains("p50") || stderr.contains("none"),
            "{}: {stderr}",
            case.path
        );
        assert!(
            !stderr.contains("kind=script.enter"),
            "{}: {stderr}",
            case.path
        );
    }
}

#[test]
fn trace_error_fixture_has_timed_error_trace() {
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .args(["trace", "tests/fixtures/runtime/cli-trace-error.xsh"])
        .output()
        .expect("run xsht");

    assert_exit(&output, 3);
    assert_eq!(stdout_text(&output), "");
    let stderr = stderr_text(&output);
    assert!(stderr.contains("nonzero-exit"));
    assert!(stderr.contains("traceback"));
    assert!(stderr.contains("trace summary"));
    assert!(stderr.contains("script duration"));
}

#[test]
fn example_corpus_is_formatted() {
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", "--check"])
        .args(example_paths())
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn example_corpus_lints_without_warnings() {
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .arg("lint")
        .args(example_paths())
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn example_runtime_cases_cover_every_example_script() {
    let expected: BTreeSet<_> = example_paths().into_iter().collect();
    let discovered: BTreeSet<_> = std::fs::read_dir("examples")
        .expect("read examples")
        .map(|entry| entry.expect("read example entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "xsh"))
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    assert_eq!(discovered, expected);
}

#[test]
fn trace_output_includes_timing() {
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .args(["trace", "--raw", "tests/fixtures/runtime/cli-trace.xsh"])
        .output()
        .expect("run xsht");

    assert_ok(&output);
    let stderr = stderr_text(&output);
    assert!(stderr.contains("kind=script.enter"));
    assert!(stderr.contains("kind=proc.enter"));
    assert!(stderr.contains("kind=core.call"));
    assert!(stderr.contains("start_us="));
    assert!(stderr.contains("duration_us="));
}
