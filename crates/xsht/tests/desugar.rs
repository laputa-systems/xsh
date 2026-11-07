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
use std::thread;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
                "target" | ".git" | ".claude" | ".work" | "node_modules"
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
            if path.file_name().is_some_and(|name| name == ".work") {
                continue;
            }
            xsh_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "xsh") {
            output.push(path);
        }
    }
}

#[cfg(unix)]
struct RestoreDirectoryPermissions(PathBuf);

#[cfg(unix)]
impl Drop for RestoreDirectoryPermissions {
    fn drop(&mut self) {
        fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700))
            .expect("restore scratch directory permissions");
    }
}

#[cfg(unix)]
#[test]
fn workspace_corpus_skips_work_tree() {
    let scratch = TempDir::new().expect("scratch directory");
    let source = scratch.path().join("workspace");
    let copy = scratch.path().join("copy");
    fs::create_dir_all(&source).expect("create fake workspace");
    let corpus_source = source.join("source.xsh");
    fs::write(&corpus_source, "print \"source\"\n").expect("write corpus source");
    let blocked = source.join(".work/tmp/blocked");
    fs::create_dir_all(&blocked).expect("create unreadable scratch child");
    fs::write(blocked.join("scratch.xsh"), "print \"scratch\"\n")
        .expect("write scratch source");
    let _restore = RestoreDirectoryPermissions(blocked.clone());
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o0))
        .expect("make scratch child unreadable");

    copy_workspace(&source, &copy);
    assert!(!copy.join(".work").exists());
    assert_eq!(
        fs::read_to_string(copy.join("source.xsh")).expect("read copied corpus source"),
        "print \"source\"\n"
    );

    let mut paths = Vec::new();
    xsh_files(&source, &mut paths);
    paths.sort();
    assert_eq!(paths, vec![corpus_source]);
}

/// Whether the file parses and holds at least one sugar statement.
fn holds_sugar(path: &Path) -> bool {
    let source = fs::read_to_string(path).expect("read corpus file");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
    parsed.diagnostics.is_empty() && !parsed.arena.arena.sugar_expansions.is_empty()
}

fn xsht(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(test_bin!("xsht"))
        .args(args)
        .current_dir(root)
        .env_remove("XSH_MODULE_PATH")
        .output()
        .expect("run xsht")
}

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
    assert!(
        output.status.success() || !report.is_empty(),
        "xsht check failed with exit status {:?} and no diagnostics:\n{stderr}",
        output.status.code()
    );
    report
}

struct TestReport {
    outcomes: Vec<String>,
    stdout: String,
}

struct BatchReport {
    files: BTreeMap<String, TestReport>,
    stderr: String,
}

fn test_jobs() -> String {
    thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, 8)
        .to_string()
}

fn test_id_file(id: &str) -> &str {
    id.split_once("::").map_or(id, |(file, _)| file)
}

fn batch_test_report(root: &Path, files: &[String]) -> BatchReport {
    let listed = xsht(root, &["test", "--list-json"]);
    assert!(
        listed.status.success(),
        "xsht test --list-json failed:\n{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let listed = String::from_utf8(listed.stdout).expect("UTF-8 test list");
    let ids = deser_json::from_str::<Vec<String>>(&listed)
        .expect("parse test list")
        .into_iter()
        .filter(|id| files.iter().any(|file| file == test_id_file(id)))
        .collect::<Vec<_>>();
    let scratch = TempDir::new().expect("create test id directory");
    let ids_file = scratch.path().join("ids.json");
    fs::write(&ids_file, deser_json::to_string(&ids).expect("encode test ids"))
        .expect("write test ids");

    let output = xsht(
        root,
        &[
            "test",
            "--jobs",
            &test_jobs(),
            "--exact-ids-file",
            &ids_file.to_string_lossy(),
        ],
    );
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 test report");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("test result:"));
    assert!(
        summary.is_some(),
        "xsht test produced no test summary with exit status {:?}:\n{stderr}",
        output.status.code(),
    );
    let skipped_only = output.status.code() == Some(2)
        && summary.is_some_and(|line| line.starts_with("test result: INCOMPLETE"))
        && !stderr.lines().any(|line| line.starts_with("xsht: "));
    assert!(
        matches!(output.status.code(), Some(0 | 1)) || skipped_only,
        "xsht test did not finish normally with exit status {:?}:\n{stderr}",
        output.status.code(),
    );
    assert_eq!(
        output.status.code() == Some(1),
        summary.is_some_and(|line| line.starts_with("test result: FAILED")),
        "xsht test exit status disagrees with its summary: {summary:?}\n{stderr}",
    );

    let mut reports = files
        .iter()
        .map(|file| {
            (
                file.clone(),
                TestReport {
                    outcomes: Vec::new(),
                    stdout: String::new(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut in_failures = false;
    let mut owner: Option<String> = None;
    for line in stdout.lines() {
        if line.starts_with("test result:") {
            break;
        }
        if line == "failures:" {
            in_failures = true;
            owner = None;
            continue;
        }
        let id = if in_failures {
            line.strip_prefix("---- ")
                .and_then(|rest| rest.strip_suffix(" ----"))
        } else {
            line.split_once(" ... ").map(|(id, _)| id)
        };
        if let Some(id) = id {
            owner = reports
                .contains_key(test_id_file(id))
                .then(|| test_id_file(id).to_string());
        } else if !in_failures {
            owner = None;
        }
        let Some(report) = owner.as_deref().and_then(|file| reports.get_mut(file)) else {
            continue;
        };
        report.stdout.push_str(line);
        report.stdout.push('\n');
        if !in_failures && let Some((name, outcome)) = line.split_once(" ... ") {
            report.outcomes.push(format!(
                "{name} ... {}",
                outcome.split_whitespace().next().unwrap_or("")
            ));
        }
    }
    for report in reports.values_mut() {
        report.outcomes.sort();
    }
    BatchReport {
        files: reports,
        stderr,
    }
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

    eprintln!("desugared corpus: selected {} files", sugared.len());
    eprintln!(
        "desugared corpus: checking written diagnostics ({} files)",
        sugared.len()
    );
    let written_check = check_report(root, &sugared);
    eprintln!(
        "desugared corpus: checking written tests ({} files, {} jobs)",
        sugared.len(),
        test_jobs()
    );
    let written_tests = batch_test_report(root, &sugared);
    assert!(
        written_tests
            .files
            .values()
            .map(|report| report.outcomes.len())
            .sum::<usize>()
            > sugared.len(),
        "the sugared corpus files ran almost no tests"
    );

    eprintln!(
        "desugared corpus: desugaring all {} files",
        sugared.len()
    );
    let mut sources = BTreeMap::new();
    for file in &sugared {
        let written = fs::read(root.join(file)).expect("read written file");
        let output = xsht(root, &["desugar", file]);
        assert!(
            output.status.success(),
            "xsht desugar {file}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(root.join(file), &output.stdout).expect("write desugared file");
        sources.insert(file.clone(), (written, output.stdout));
        assert!(
            !holds_sugar(&root.join(file)),
            "xsht desugar {file} left sugar in its output"
        );
    }

    eprintln!(
        "desugared corpus: comparing check reports for {} files",
        sugared.len()
    );
    assert_eq!(
        written_check,
        check_report(root, &sugared),
        "the desugared corpus checks differently"
    );
    eprintln!(
        "desugared corpus: checking desugared tests ({} files, {} jobs)",
        sugared.len(),
        test_jobs()
    );
    let desugared_tests = batch_test_report(root, &sugared);
    let differing = sugared
        .iter()
        .filter(|file| written_tests.files[*file].outcomes != desugared_tests.files[*file].outcomes)
        .cloned()
        .collect::<Vec<_>>();
    if differing.is_empty() {
        return;
    }

    eprintln!(
        "desugared corpus: rerunning {} differing files: {differing:?}",
        differing.len()
    );
    for file in &differing {
        fs::write(root.join(file), &sources[file].0).expect("restore written file");
    }
    let written_rerun = batch_test_report(root, &differing);
    for file in &differing {
        fs::write(root.join(file), &sources[file].1).expect("write desugared file");
    }
    let desugared_rerun = batch_test_report(root, &differing);
    for file in &differing {
        assert_eq!(
            &written_rerun.files[file].outcomes,
            &desugared_rerun.files[file].outcomes,
            "{file} tests differently once desugared, on both attempts\n\
             first attempt written outcomes: {:?}\nfirst attempt desugared outcomes: {:?}\n\
             first attempt written stdout:\n{}\nfirst attempt written stderr:\n{}\n\
             first attempt desugared stdout:\n{}\nfirst attempt desugared stderr:\n{}\n\
             rerun written stdout:\n{}\nrerun written stderr:\n{}\n\
             rerun desugared stdout:\n{}\nrerun desugared stderr:\n{}",
            written_tests.files[file].outcomes,
            desugared_tests.files[file].outcomes,
            written_tests.files[file].stdout,
            written_tests.stderr,
            desugared_tests.files[file].stdout,
            desugared_tests.stderr,
            written_rerun.files[file].stdout,
            written_rerun.stderr,
            desugared_rerun.files[file].stdout,
            desugared_rerun.stderr
        );
    }
}
