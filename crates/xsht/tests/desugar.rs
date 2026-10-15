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
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
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
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0))
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
    stderr: String,
}

fn test_report(root: &Path, file: &str) -> TestReport {
    let output = xsht(root, &["test", file]);
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 test report");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut report = stdout
        .lines()
        .filter(|line| line.contains(" ... ") || line.starts_with("test result:"))
        .map(|line| {
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
    let summary = report
        .iter()
        .find(|line| line.starts_with("test result:"));
    assert!(
        summary.is_some(),
        "xsht test {file} produced no test summary with exit status {:?}:\n{}",
        output.status.code(),
        stderr
    );
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "xsht test {file} did not finish normally with exit status {:?}:\n{}",
        output.status.code(),
        stderr
    );
    assert_eq!(
        output.status.code() == Some(1),
        summary.is_some_and(|line| line.starts_with("test result: FAILED")),
        "xsht test {file} exit status disagrees with its summary: {summary:?}\n{}",
        stderr
    );
    TestReport {
        outcomes: report,
        stdout,
        stderr,
    }
}

fn report_worker_count() -> usize {
    (thread::available_parallelism().map_or(1, usize::from) / 8).clamp(1, 4)
}

fn parallel_map_ordered<T, R, F>(items: &[T], worker_count: usize, map: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(usize, &T) -> R + Sync,
{
    if items.is_empty() {
        return Vec::new();
    }

    let worker_count = worker_count.clamp(1, items.len());
    let next = AtomicUsize::new(0);
    let cancelled = AtomicBool::new(false);
    let results = Mutex::new(
        std::iter::repeat_with(|| None)
            .take(items.len())
            .collect::<Vec<Option<R>>>(),
    );

    thread::scope(|scope| {
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            workers.push(scope.spawn(|| {
                loop {
                    if cancelled.load(Ordering::Acquire) {
                        break None;
                    }
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        break None;
                    };
                    let mapped = catch_unwind(AssertUnwindSafe(|| map(index, item)));
                    let value = match mapped {
                        Ok(value) => value,
                        Err(payload) => {
                            cancelled.store(true, Ordering::Release);
                            break Some(payload);
                        }
                    };
                    let mut results = results
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    results[index] = Some(value);
                }
            }));
        }

        let mut failure = None;
        for worker in workers {
            match worker.join() {
                Ok(Some(payload)) | Err(payload) => {
                    if failure.is_none() {
                        failure = Some(payload);
                    }
                }
                Ok(None) => {}
            }
        }
        if let Some(payload) = failure {
            resume_unwind(payload);
        }
    });

    results
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            result.unwrap_or_else(|| panic!("report worker skipped item {index}"))
        })
        .collect()
}

#[test]
fn report_workers_preserve_order_and_visit_each_file_once() {
    let files = ["first", "second", "third", "fourth"];
    let calls = files
        .iter()
        .map(|_| AtomicUsize::new(0))
        .collect::<Vec<_>>();

    let reports = parallel_map_ordered(&files, 3, |index, file| {
        calls[index].fetch_add(1, Ordering::Relaxed);
        format!("{file}:{index}")
    });

    assert_eq!(
        reports,
        vec![
            "first:0".to_string(),
            "second:1".to_string(),
            "third:2".to_string(),
            "fourth:3".to_string()
        ]
    );
    assert!(calls.iter().all(|count| count.load(Ordering::Relaxed) == 1));
}

#[test]
fn report_workers_join_started_items_before_propagating_a_failure() {
    let files = [0, 1, 2, 3];
    let rendezvous = std::sync::Barrier::new(2);
    let started = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);

    let result = catch_unwind(AssertUnwindSafe(|| {
        parallel_map_ordered(&files, 2, |_, file| {
            started.fetch_add(1, Ordering::Relaxed);
            if *file < 2 {
                rendezvous.wait();
            }
            assert_ne!(*file, 0, "report worker failure");
            finished.fetch_add(1, Ordering::Relaxed);
            *file
        })
    }));

    assert!(result.is_err());
    assert_eq!(
        started.load(Ordering::Relaxed),
        finished.load(Ordering::Relaxed) + 1
    );
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

    let workers = report_worker_count();
    eprintln!("desugared corpus: selected {} files", sugared.len());
    eprintln!(
        "desugared corpus: checking written diagnostics ({} files)",
        sugared.len()
    );
    let written_check = check_report(root, &sugared);
    eprintln!(
        "desugared corpus: checking written tests ({} files, {workers} workers)",
        sugared.len()
    );
    let written_tests = parallel_map_ordered(&sugared, workers, |_, file| {
        test_report(root, file)
    });
    assert!(
        written_tests
            .iter()
            .map(|report| report.outcomes.len())
            .sum::<usize>()
            > sugared.len(),
        "the sugared corpus files ran almost no tests"
    );

    eprintln!(
        "desugared corpus: desugaring all {} files",
        sugared.len()
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
        "desugared corpus: checking desugared tests ({} files, {workers} workers)",
        sugared.len()
    );
    let _ = parallel_map_ordered(&sugared, workers, |index, file| {
        let desugared = test_report(root, file);
        assert_eq!(
            &written_tests[index].outcomes,
            &desugared.outcomes,
            "{file} tests differently once desugared\nwritten stdout:\n{}\nwritten stderr:\n{}\ndesugared stdout:\n{}\ndesugared stderr:\n{}",
            written_tests[index].stdout,
            written_tests[index].stderr,
            desugared.stdout,
            desugared.stderr
        );
    });
}
