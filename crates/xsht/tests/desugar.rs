//! A program and the program `xsht desugar` prints for it must mean the same.
//!
//! Sugar reaches the checker and the runtime only as its expansion, but a
//! semantic walker that matches statement kinds with a wildcard arm can miss
//! the sugar node and treat the statement as inert. Nothing in the type
//! system catches that. This test does: every file of the native test corpus
//! that holds a sugar statement is checked and run as written and again after
//! desugaring, in a copy of the workspace, and the two must report the same
//! diagnostics (by file, code, and message; positions move) and the same
//! result for every test.
//!
//! It spawns `xsht`, so it runs on release binaries only. The unit tests in
//! `crates/xsht/src/desugar_tests.rs` hold a handful of inline programs to
//! the checking half of the same contract.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

/// The native test corpus: the `test_roots` of the workspace configuration.
const CORPUS_ROOTS: [&str; 4] = ["tests/xsh", "core/tests", "dev/tests", "showcase/tests"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Copies the workspace without build output and repository state. Tests
/// read fixtures and modules by relative path, so everything else comes.
fn copy_workspace(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create workspace copy directory");
    for entry in fs::read_dir(from).expect("read workspace directory") {
        let entry = entry.expect("read workspace entry");
        let name = entry.file_name();
        let target = to.join(&name);
        let kind = entry.file_type().expect("workspace entry type");
        if kind.is_dir() {
            if matches!(
                &*name.to_string_lossy(),
                "target" | ".git" | ".claude" | "node_modules"
            ) {
                continue;
            }
            copy_workspace(&entry.path(), &target);
        } else if kind.is_file() {
            fs::copy(entry.path(), &target).expect("copy workspace file");
        }
    }
}

fn xsh_files(root: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("read corpus directory") {
        let path = entry.expect("read corpus entry").path();
        if path.is_dir() {
            xsh_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "xsh") {
            output.push(path);
        }
    }
}

/// Whether the file parses and holds at least one sugar statement.
fn holds_sugar(path: &Path) -> bool {
    let source = fs::read_to_string(path).expect("read corpus file");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
    parsed.diagnostics.is_empty() && !parsed.arena.arena.sugar_expansions.is_empty()
}

fn xsht(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(release_bin!("xsht"))
        .args(args)
        .current_dir(root)
        .env_remove("XSH_MODULE_PATH")
        .output()
        .expect("run xsht")
}

/// The diagnostics `xsht check` reports for `files`, as `FILE: HEADER` with
/// its count. A report prints each diagnostic as a `severity[code]: message`
/// line followed by an indented `FILE:LINE:COLUMN` line; the line and column
/// are dropped because desugaring moves them.
fn check_report(root: &Path, files: &[String]) -> BTreeMap<String, usize> {
    let mut args = vec!["check"];
    args.extend(files.iter().map(String::as_str));
    let output = xsht(root, &args);
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 check report");
    let mut report = BTreeMap::new();
    let mut lines = stderr.lines();
    while let Some(line) = lines.next() {
        if !(line.starts_with("err[") || line.starts_with("warn[")) {
            continue;
        }
        let location = lines.next().unwrap_or("").trim();
        let file = location.rsplitn(3, ':').last().unwrap_or(location);
        *report.entry(format!("{file}: {line}")).or_default() += 1;
    }
    report
}

/// The result line of every test `xsht test` runs for `file`, without its
/// duration, and the summary line.
fn test_report(root: &Path, file: &str) -> Vec<String> {
    let output = xsht(root, &["test", file]);
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 test report");
    let mut report = stdout
        .lines()
        .filter(|line| line.contains(" ... ") || line.starts_with("test result:"))
        .map(|line| {
            // `NAME ... ok 12ms`: the outcome is the word after ` ... `.
            match line.split_once(" ... ") {
                Some((name, outcome)) => format!(
                    "{name} ... {}",
                    outcome.split_whitespace().next().unwrap_or("")
                ),
                None => line.to_string(),
            }
        })
        .collect::<Vec<_>>();
    report.sort();
    report
}

#[test]
fn the_desugared_corpus_checks_and_tests_like_the_corpus() {
    let copy = TempDir::new().expect("create workspace copy");
    let root = copy.path();
    copy_workspace(&workspace_root(), root);

    let mut files = Vec::new();
    for corpus in CORPUS_ROOTS {
        xsh_files(&root.join(corpus), &mut files);
    }
    files.sort();
    let sugared = files
        .iter()
        .filter(|path| holds_sugar(path))
        .map(|path| {
            path.strip_prefix(root)
                .expect("corpus file under the copy")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        sugared.len() >= 10,
        "the corpus holds sugar in only {} files",
        sugared.len()
    );

    let written_check = check_report(root, &sugared);
    let written_tests = sugared
        .iter()
        .map(|file| test_report(root, file))
        .collect::<Vec<_>>();
    assert!(
        written_tests.iter().flatten().count() > sugared.len(),
        "the sugared corpus files ran almost no tests"
    );

    for file in &sugared {
        let output = xsht(root, &["desugar", file]);
        assert!(
            output.status.success(),
            "xsht desugar {file}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(root.join(file), &output.stdout).expect("write desugared file");
        assert!(
            !holds_sugar(&root.join(file)),
            "xsht desugar {file} left sugar in its output"
        );
    }

    assert_eq!(
        written_check,
        check_report(root, &sugared),
        "the desugared corpus checks differently"
    );
    for (file, written) in sugared.iter().zip(&written_tests) {
        assert_eq!(
            written,
            &test_report(root, file),
            "{file} tests differently once desugared"
        );
    }
}
