use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use xsht::examples::{ExampleCatalog, OutputPolicy, load_catalog, validate_catalog};

fn example_catalog() -> ExampleCatalog {
    let root = super::workspace_root();
    let catalog = load_catalog(&root).expect("load examples/catalog.json");
    validate_catalog(&root, &catalog).expect("validate examples/catalog.json");
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

fn lint_diagnostics(stderr: &str) -> &str {
    let Some(body) = stderr.strip_suffix('\n') else {
        return stderr;
    };
    let (diagnostics, timing) = body.rfind('\n').map_or(("", body), |index| {
        body.split_at(index + 1)
    });
    let Some(timing) = timing.strip_prefix("xsht lint: ") else {
        return stderr;
    };
    let Some((count, timing)) = timing.split_once(' ') else {
        return stderr;
    };
    if count.parse::<usize>().is_err() {
        return stderr;
    }
    let Some(timing) = timing
        .strip_prefix("files")
        .or_else(|| timing.strip_prefix("file"))
    else {
        return stderr;
    };
    if !timing.starts_with(" in ") || !timing.contains(" (thread time by stage: ") {
        return stderr;
    }
    diagnostics
}

fn xsh_binary() -> PathBuf {
    let binary = Path::new(test_bin!("xsht")).with_file_name("xsh");
    assert!(
        binary.is_file(),
        "missing sibling xsh binary at {}; build xsh in the same profile before running examples",
        binary.display()
    );
    binary
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout UTF-8")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr UTF-8")
}

fn assert_ok(output: &Output) {
    assert!(
        output.status.success(),
        "status={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout_text(output),
        stderr_text(output)
    );
}

#[test]
fn example_corpus_runs_with_expected_output() {
    let root = super::workspace_root();
    for case in example_catalog().examples {
        if case.skip {
            continue;
        }
        let mut command = Command::new(xsh_binary());
        command.current_dir(&root).arg(&case.path);
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
    let root = super::workspace_root();
    for case in example_catalog()
        .examples
        .into_iter()
        .filter(|case| case.trace)
    {
        let output = Command::new(test_bin!("xsht"))
            .arg("trace")
            .arg(&case.path)
            .args(if case.args.is_empty() {
                &[][..]
            } else {
                &["--"][..]
            })
            .args(&case.args)
            .current_dir(&root)
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
fn example_corpus_is_formatted() {
    let root = super::workspace_root();
    let output = Command::new(test_bin!("xsht"))
        .args(["fmt", "--check"])
        .args(example_paths())
        .current_dir(&root)
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn example_corpus_lints_without_warnings() {
    let root = super::workspace_root();
    let output = Command::new(test_bin!("xsht"))
        .arg("lint")
        .args(example_paths())
        .current_dir(&root)
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(lint_diagnostics(&stderr_text(&output)), "");
}

#[test]
fn example_runtime_cases_cover_every_example_script() {
    let root = super::workspace_root();
    let expected: BTreeSet<_> = example_paths().into_iter().collect();
    let discovered: BTreeSet<_> = std::fs::read_dir(root.join("examples"))
        .expect("read examples")
        .map(|entry| entry.expect("read example entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "xsh"))
        .map(|path| {
            path.strip_prefix(&root)
                .expect("example is under the workspace root")
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    assert_eq!(discovered, expected);
}
