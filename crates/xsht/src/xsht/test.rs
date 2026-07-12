#![allow(clippy::single_call_fn)]

use crate::xsht::cli::{
    CliOutput, CoverageCollector, TraceFormat, TraceOptions, cancellation_output,
    collect_xsh_files, load_config, trace_script,
};
use crate::xsht::docs::{OutputPolicy, load_example_catalog};
use miniserde::json::{Object, Value as JsonValue};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};
use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label};
use xsh::parse_script_with_module_roots;
use xsh::runner::{RunOptions, XSH_COVERAGE_TRACE_DIR, render_coverage_trace_jsonl, run_script};
use xsh::runtime::eval::{Evaluator, NativeTestRunKind, NativeTestRunRequest, PreparedTestProgram};
use xsh::runtime::process::path_bytes;
use xsh::runtime::value::{PathValue, RecordMap, ResultValue, RuntimeError, Value};
use xsh::sema::check::Checker;
use xsh::sema::types::Type;
use xsh::syntax::arena::{ArenaProgram, ArenaStmtKind, FunctionDefId, StmtId};
use xsh::trace::{SyscallSummary, TracebackRenderer};

#[derive(Clone, Debug)]
pub(crate) struct TestOptions {
    pub(crate) filter: Option<String>,
    pub(crate) native: bool,
    pub(crate) examples: bool,
    pub(crate) list: bool,
    pub(crate) exact: bool,
    pub(crate) nocapture: bool,
    pub(crate) fail_fast: bool,
    pub(crate) keep_temp: bool,
    pub(crate) strict_lower: bool,
    pub(crate) jobs: Option<usize>,
    pub(crate) coverage: bool,
    pub(crate) coverage_json_out: Option<String>,
    pub(crate) trace_top_syscalls: Option<usize>,
    /// If set, write per-test syscall summaries to this file as a JSON baseline.
    pub(crate) syscall_json_out: Option<String>,
    /// Per-test syscall budgets: map from test id to map from syscall name (or "total") to limit.
    pub(crate) syscall_budgets: Option<BTreeMap<String, BTreeMap<String, u64>>>,
}

impl TestOptions {
    fn collect_coverage(&self) -> bool {
        self.coverage || self.coverage_json_out.is_some()
    }
}
pub(crate) fn test_scripts(options: TestOptions) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }

    let mut cases = Vec::new();
    let mut stdout = String::new();
    let mut stderr = String::new();

    if options.native {
        let config = match load_config() {
            Ok(config) => config,
            Err(message) => {
                return CliOutput {
                    status: 2,
                    stdout: stdout.into_bytes(),
                    stderr: text_bytes(format!("xsht: {message}\n")),
                    trace_text: String::new(),
                    syscall_summary: None,
                };
            }
        };
        let module_roots: Vec<PathBuf> = config.module_path.iter().map(PathBuf::from).collect();
        let test_roots: Vec<PathBuf> = if config.test_roots.is_empty() {
            vec![PathBuf::from("tests")]
        } else {
            config.test_roots.iter().map(PathBuf::from).collect()
        };
        for root in &test_roots {
            match discover_native_tests(root, &config.exclude, &module_roots, &options) {
                Ok(native) => cases.extend(native),
                Err(message) => {
                    if let Some(output) = cancellation_output() {
                        return output;
                    }
                    return CliOutput {
                        status: 2,
                        stdout: stdout.into_bytes(),
                        stderr: text_bytes(format!("xsht: {message}\n")),
                        trace_text: String::new(),
                        syscall_summary: None,
                    };
                }
            }
        }
    }
    if options.examples {
        if let Some(output) = cancellation_output() {
            return output;
        }
        match discover_example_tests(&options) {
            Ok(examples) => cases.extend(examples),
            Err(message) => {
                return CliOutput {
                    status: 2,
                    stdout: stdout.into_bytes(),
                    stderr: text_bytes(format!("xsht: {message}\n")),
                    trace_text: String::new(),
                    syscall_summary: None,
                };
            }
        }
    }

    cases.sort_unstable_by(|left, right| left.id().cmp(right.id()));

    if options.list {
        for case in &cases {
            if let Some(output) = cancellation_output() {
                return output;
            }
            stdout.push_str(case.id());
            stdout.push('\n');
        }
        return CliOutput {
            status: 0,
            stdout: stdout.into_bytes(),
            stderr: stderr.into_bytes(),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    stdout.push_str(&format!("running {} tests\n", cases.len()));

    let run_id = test_run_id();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut failure_details = Vec::new();
    let mut coverage = options.collect_coverage().then(CoverageCollector::new);
    let mut baseline_entries: Vec<(String, SyscallSummary)> = Vec::new();

    let ordered_outcomes = run_test_cases(cases, &run_id, &options);

    for (id, mut outcome) in ordered_outcomes {
        if let Some(output) = cancellation_output() {
            return output;
        }

        // Budget checking: fail if any syscall count exceeds the configured budget.
        if let Some(summary) = &outcome.syscall_summary
            && let Some(budgets) = options.syscall_budgets.as_ref().and_then(|b| b.get(&id))
        {
            let budget_failures = check_syscall_budget(summary, budgets);
            if !budget_failures.is_empty() {
                let msg = budget_failures.join("; ");
                // Downgrade to Failed if currently Passed, or append to existing failure.
                outcome.kind = match outcome.kind {
                    TestOutcomeKind::Passed => {
                        TestOutcomeKind::Failed(format!("syscall budget exceeded: {msg}"))
                    }
                    TestOutcomeKind::Failed(existing) => TestOutcomeKind::Failed(format!(
                        "{existing}; syscall budget exceeded: {msg}"
                    )),
                    other => other,
                };
            }
        }

        // Collect baseline data.
        if options.syscall_json_out.is_some()
            && let Some(summary) = outcome.syscall_summary.clone()
        {
            baseline_entries.push((id.clone(), summary));
        }

        if options.nocapture {
            stdout.push_str(&bytes_text_lossy(&outcome.stdout));
            stderr.push_str(&bytes_text_lossy(&outcome.stderr));
            if options.trace_top_syscalls.is_some() && !outcome.trace_text.is_empty() {
                stderr.push_str(&format!("# test: {id}\n"));
                stderr.push_str(&outcome.trace_text);
            }
        }
        if let Some(collector) = coverage.as_mut()
            && let Some(trace) = &outcome.coverage_trace
            && let Err(message) = collector.ingest_jsonl(outcome.coverage_scope, trace)
        {
            failed += 1;
            stdout.push_str("test coverage ... FAILED\n");
            failure_details.push(("coverage".to_string(), message, Vec::new(), Vec::new()));
            if options.fail_fast {
                break;
            }
        }
        match outcome.kind {
            TestOutcomeKind::Passed => {
                passed += 1;
                stdout.push_str(&format!("test {id} ... ok\n"));
            }
            TestOutcomeKind::Skipped(message) => {
                skipped += 1;
                if message.is_empty() {
                    stdout.push_str(&format!("test {id} ... skipped\n"));
                } else {
                    stdout.push_str(&format!("test {id} ... skipped: {message}\n"));
                }
            }
            TestOutcomeKind::Failed(message) => {
                failed += 1;
                stdout.push_str(&format!("test {id} ... FAILED\n"));
                failure_details.push((id.clone(), message, outcome.stdout, outcome.stderr));
                if options.fail_fast {
                    break;
                }
            }
        }
    }

    // Write baseline JSON if requested.
    if let Some(path) = &options.syscall_json_out {
        match write_syscall_baseline(&baseline_entries, path) {
            Ok(()) => {}
            Err(message) => {
                failed += 1;
                stdout.push_str("test syscall-baseline ... FAILED\n");
                failure_details.push((
                    "syscall-baseline".to_string(),
                    message,
                    Vec::new(),
                    Vec::new(),
                ));
            }
        }
    }

    if let Some(path) = &options.coverage_json_out
        && let Some(collector) = coverage.as_ref()
        && let Err(message) = collector.write_json(path)
    {
        failed += 1;
        stdout.push_str("test coverage-json ... FAILED\n");
        failure_details.push(("coverage-json".to_string(), message, Vec::new(), Vec::new()));
    }

    if options.coverage
        && let Some(collector) = coverage.as_mut()
    {
        stdout.push('\n');
        stdout.push_str(&collector.render());
    }

    if !failure_details.is_empty() {
        stdout.push_str("\nfailures:\n\n");
        for (id, message, captured_stdout, captured_stderr) in failure_details {
            stdout.push_str(&format!("---- {id} ----\n"));
            if !captured_stdout.is_empty() && !options.nocapture {
                stdout.push_str("stdout:\n");
                stdout.push_str(&bytes_text_lossy(&captured_stdout));
                if !captured_stdout.ends_with(b"\n") {
                    stdout.push('\n');
                }
            }
            if !captured_stderr.is_empty() && !options.nocapture {
                stdout.push_str("stderr:\n");
                stdout.push_str(&bytes_text_lossy(&captured_stderr));
                if !captured_stderr.ends_with(b"\n") {
                    stdout.push('\n');
                }
            }
            stdout.push_str(&message);
            if !message.ends_with('\n') {
                stdout.push('\n');
            }
            stdout.push('\n');
        }
    }

    let status_text = if failed == 0 { "ok" } else { "FAILED" };
    stdout.push_str(&format!(
        "test result: {status_text}. {passed} passed; {failed} failed; {skipped} skipped\n"
    ));

    CliOutput {
        status: if failed == 0 { 0 } else { 1 },
        stdout: stdout.into_bytes(),
        stderr: stderr.into_bytes(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

fn run_test_cases(
    cases: Vec<TestCase>,
    run_id: &str,
    options: &TestOptions,
) -> Vec<(String, TestOutcome)> {
    let jobs = test_jobs(options, cases.len());
    if jobs <= 1 {
        return cases
            .into_iter()
            .enumerate()
            .map(|(index, case)| {
                let id = case.id().to_string();
                let outcome = run_test_case(case, index, run_id, options);
                (id, outcome)
            })
            .collect();
    }

    run_test_cases_parallel(cases, run_id, options, jobs)
}

fn test_jobs(options: &TestOptions, cases_len: usize) -> usize {
    if cases_len <= 1
        || options.fail_fast
        || options.nocapture
        || options.trace_top_syscalls.is_some()
    {
        return 1;
    }
    options
        .jobs
        .unwrap_or_else(default_test_jobs)
        .min(cases_len)
}

fn default_test_jobs() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .clamp(1, 8)
}

fn run_test_cases_parallel(
    cases: Vec<TestCase>,
    run_id: &str,
    options: &TestOptions,
    jobs: usize,
) -> Vec<(String, TestOutcome)> {
    let total = cases.len();
    let queue = Arc::new(Mutex::new(
        cases
            .into_iter()
            .enumerate()
            .collect::<std::collections::VecDeque<_>>(),
    ));
    let (tx, rx) = mpsc::channel();

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            scope.spawn(move || {
                loop {
                    let Some((index, case)) = queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .pop_front()
                    else {
                        break;
                    };
                    let id = case.id().to_string();
                    let outcome = run_test_case(case, index, run_id, options);
                    if tx.send((index, id, outcome)).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(tx);

    let mut outcomes = Vec::with_capacity(total);
    for item in rx {
        outcomes.push(item);
    }
    outcomes.sort_by_key(|(index, _, _)| *index);
    outcomes
        .into_iter()
        .map(|(_, id, outcome)| (id, outcome))
        .collect()
}

enum TestCase {
    Native(NativeTestCase),
    Example(ExampleTestCase),
    Invalid { id: String, message: String },
}

impl TestCase {
    fn id(&self) -> &str {
        match self {
            Self::Native(case) => &case.id,
            Self::Example(case) => &case.id,
            Self::Invalid { id, .. } => id,
        }
    }
}

#[derive(Clone)]
struct NativeTestCase {
    id: String,
    file: String,
    name: String,
    prepared: Arc<PreparedTestProgram>,
    has_ctx: bool,
}

#[derive(Clone)]
struct ExampleTestCase {
    id: String,
    path: String,
    args: Vec<String>,
    expected_status: i32,
    stdout: OutputPolicy,
    stderr: OutputPolicy,
}

struct TestOutcome {
    kind: TestOutcomeKind,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    trace_text: String,
    syscall_summary: Option<SyscallSummary>,
    coverage_trace: Option<String>,
    coverage_scope: &'static str,
}

enum TestOutcomeKind {
    Passed,
    Failed(String),
    Skipped(String),
}

/// The proc def a top-level statement exposes as a test, unwrapping `export`.
fn exported_test_proc(program: &ArenaProgram, id: StmtId) -> Option<FunctionDefId> {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => exported_test_proc(program, inner),
        ArenaStmtKind::ProcDef(def) => Some(def),
        _ => None,
    }
}

fn test_id_matches(id: &str, options: &TestOptions) -> bool {
    let Some(filter) = &options.filter else {
        return true;
    };
    if options.exact {
        id == filter
    } else {
        id.contains(filter)
    }
}

fn test_top_level_diagnostics(program: &ArenaProgram) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for id in program.statement_ids() {
        if !test_top_level_allowed(program, id) {
            let span = program.arena.stmt(id).span;
            diagnostics.push(
                Diagnostic::error(
                    "test files cannot run top-level commands, mutation, or control flow",
                )
                .with_code("check.test-top-level")
                .with_label(Label::primary(span, "not allowed at test file top level")),
            );
        }
    }
    diagnostics
}

fn test_top_level_allowed(program: &ArenaProgram, id: StmtId) -> bool {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Use(_)
        | ArenaStmtKind::Let { .. }
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::PureDef(_) => true,
        ArenaStmtKind::Export(inner) => test_top_level_allowed(program, inner),
        _ => false,
    }
}

fn native_test_signature_uses_ctx(
    program: &ArenaProgram,
    id: FunctionDefId,
) -> Result<bool, String> {
    let def = program.arena.function_def(id);
    if !Type::from_arena(&program.arena, def.return_ty).is_result_unit() {
        return Err("test proc must return Result[Unit]".to_string());
    }
    match program.arena.params(def.params) {
        [] => Ok(false),
        [param]
            if !param.rest
                && param.default.is_none()
                && program.arena.type_expr_named(param.ty, "TestContext") =>
        {
            Ok(true)
        }
        [_] => Err("test proc parameter must be `ctx: TestContext`".to_string()),
        _ => Err("test proc accepts at most one TestContext parameter".to_string()),
    }
}

fn discover_native_tests(
    root: &Path,
    excludes: &[String],
    module_roots: &[PathBuf],
    options: &TestOptions,
) -> Result<Vec<TestCase>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_xsh_files(root, excludes, &mut files)?;
    files.sort_unstable();

    let mut cases = Vec::new();
    for file in files {
        let file_name = file.to_string_lossy().into_owned();
        let (sources, parsed) = match parse_script_with_module_roots(&file_name, module_roots) {
            Ok(parsed) => parsed,
            Err(err) => {
                let id = file_name.clone();
                if test_id_matches(&id, options) {
                    cases.push(TestCase::Invalid {
                        id,
                        message: format!("failed to read test file: {err}"),
                    });
                }
                continue;
            }
        };
        if !parsed.diagnostics.is_empty() {
            let id = file_name.clone();
            if test_id_matches(&id, options) {
                cases.push(TestCase::Invalid {
                    id,
                    message: DiagnosticRenderer::new().render(&parsed.diagnostics, &sources),
                });
            }
            continue;
        }

        let top_level_errors = test_top_level_diagnostics(&parsed.arena);
        if !top_level_errors.is_empty() {
            let id = file_name.clone();
            if test_id_matches(&id, options) {
                cases.push(TestCase::Invalid {
                    id,
                    message: DiagnosticRenderer::new().render(&top_level_errors, &sources),
                });
            }
            continue;
        }

        // The entry file is always the first source added to a fresh source map
        // during parsing; modules are appended after it.
        let source_id = sources
            .files()
            .first()
            .map(xsh::source::SourceFile::id)
            .unwrap_or_else(|| xsh::source::SourceId::new(0));
        let entry_text = sources
            .get(source_id)
            .map(|source| source.text().to_string())
            .unwrap_or_default();
        let checked = Checker::check_arena(&parsed.arena, &entry_text);
        if !checked.diagnostics.is_empty() {
            let id = file_name.clone();
            if test_id_matches(&id, options) {
                cases.push(TestCase::Invalid {
                    id,
                    message: DiagnosticRenderer::new().render(&checked.diagnostics, &sources),
                });
            }
            continue;
        }

        let arena = Arc::new(parsed.arena);
        let sources = Arc::new(sources);
        let prepared = Arc::new(
            Evaluator::new_with_shared_sources(Vec::new(), Arc::clone(&sources))
                .with_native_test_host(Arc::new(native_test_host))
                .prepare_test_program(Arc::clone(&arena), source_id),
        );
        for stmt_id in arena.statement_ids() {
            let Some(def_id) = exported_test_proc(&arena, stmt_id) else {
                continue;
            };
            let name = arena.arena.function_def(def_id).name;
            if !name.as_str().starts_with("test_") {
                continue;
            }
            let id = format!("{file_name}::{name}");
            if !test_id_matches(&id, options) {
                continue;
            }
            match native_test_signature_uses_ctx(&arena, def_id) {
                Ok(has_ctx) => cases.push(TestCase::Native(NativeTestCase {
                    id,
                    file: file_name.clone(),
                    name: name.to_string(),
                    prepared: Arc::clone(&prepared),
                    has_ctx,
                })),
                Err(message) => cases.push(TestCase::Invalid { id, message }),
            }
        }
    }

    Ok(cases)
}

fn discover_example_tests(options: &TestOptions) -> Result<Vec<TestCase>, String> {
    let catalog = load_example_catalog(".")?;
    let cases = catalog
        .examples
        .into_iter()
        .filter_map(|case| {
            let id = format!("examples::{}", case.include_id);
            test_id_matches(&id, options).then_some(TestCase::Example(ExampleTestCase {
                id,
                path: case.path,
                args: case.args,
                expected_status: case.expected_status,
                stdout: case.stdout,
                stderr: case.stderr,
            }))
        })
        .collect();
    Ok(cases)
}

fn run_test_case(case: TestCase, index: usize, run_id: &str, options: &TestOptions) -> TestOutcome {
    match case {
        TestCase::Native(case) => run_native_test(case, index, run_id, options),
        TestCase::Example(case) => run_example_test(case, index, run_id, options),
        TestCase::Invalid { message, .. } => TestOutcome {
            kind: TestOutcomeKind::Failed(message),
            stdout: Vec::new(),
            stderr: Vec::new(),
            trace_text: String::new(),
            syscall_summary: None,
            coverage_trace: None,
            coverage_scope: "tests",
        },
    }
}

fn run_native_test(
    case: NativeTestCase,
    index: usize,
    run_id: &str,
    options: &TestOptions,
) -> TestOutcome {
    let temp_root = std::env::temp_dir().join(format!(
        "xsh-test-{run_id}-{index}-{}",
        sanitize_test_id(&case.name)
    ));
    if let Err(err) = fs::create_dir_all(&temp_root) {
        return TestOutcome {
            kind: TestOutcomeKind::Failed(format!(
                "failed to create temp root '{}': {err}",
                temp_root.display()
            )),
            stdout: Vec::new(),
            stderr: Vec::new(),
            trace_text: String::new(),
            syscall_summary: None,
            coverage_trace: None,
            coverage_scope: "tests",
        };
    }

    let ctx = if case.has_ctx {
        match test_context_value(&case.id, &case.file, &temp_root) {
            Ok(ctx) => ctx,
            Err(message) => {
                return TestOutcome {
                    kind: TestOutcomeKind::Failed(message),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    trace_text: String::new(),
                    syscall_summary: None,
                    coverage_trace: None,
                    coverage_scope: "tests",
                };
            }
        }
    } else {
        Value::Unit
    };

    let nested_coverage_dir = options
        .collect_coverage()
        .then(|| temp_root.join("coverage-traces"));
    let mut env_overlay = Vec::new();
    let xsh_binary = absolute_path(&test_binary("xsh"));
    let core_dir = Path::new(&case.file)
        .parent()
        .and_then(Path::parent)
        .map(absolute_path)
        .unwrap_or_else(|| absolute_path(Path::new(".")));
    env_overlay.push((b"CARGO_BIN_EXE_xsh".to_vec(), path_bytes(&xsh_binary)));
    env_overlay.push((b"XSH_CORE_DIR".to_vec(), path_bytes(&core_dir)));
    if let Some(dir) = &nested_coverage_dir {
        env_overlay.push((XSH_COVERAGE_TRACE_DIR.as_bytes().to_vec(), path_bytes(dir)));
    }
    let evaluated =
        case.prepared
            .eval_test(&case.name, ctx, options.collect_coverage(), env_overlay);

    let mut detail = String::new();
    if !evaluated.output.diagnostics.is_empty() {
        detail.push_str(
            &DiagnosticRenderer::new()
                .render(&evaluated.output.diagnostics, &evaluated.output.sources),
        );
    }
    if let Some(traceback) = &evaluated.output.traceback {
        detail.push_str(&TracebackRenderer::new().render(traceback, &evaluated.output.sources));
    }

    let coverage_trace = if options.collect_coverage() {
        let mut trace =
            render_coverage_trace_jsonl(&evaluated.output.trace_events, &evaluated.output.sources);
        if let Some(dir) = &nested_coverage_dir {
            match read_nested_coverage_traces(dir) {
                Ok(nested) => trace.push_str(&nested),
                Err(message) => detail.push_str(&format!("coverage: {message}\n")),
            }
        }
        Some(trace)
    } else {
        None
    };

    if !options.keep_temp {
        let _ = fs::remove_dir_all(&temp_root);
    }

    let kind = if !detail.is_empty() {
        TestOutcomeKind::Failed(detail)
    } else {
        classify_native_test_result(evaluated.result)
    };

    TestOutcome {
        kind,
        stdout: evaluated.output.stdout,
        stderr: evaluated.output.stderr,
        trace_text: String::new(),
        syscall_summary: None,
        coverage_trace,
        coverage_scope: "tests",
    }
}

fn native_test_host(request: NativeTestRunRequest) -> Result<Value, RuntimeError> {
    let script_path = PathBuf::from(std::ffi::OsString::from_vec(
        request.script_path.bytes.clone(),
    ));
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RuntimeError::new(native_test_error_kind(request.kind), error.to_string())
                .with_span(request.span)
        })?;
    }
    fs::write(&script_path, request.source).map_err(|error| {
        RuntimeError::new(native_test_error_kind(request.kind), error.to_string())
            .with_span(request.span)
    })?;

    let mut command = match request.kind {
        NativeTestRunKind::Xsh => {
            let mut command = Command::new(test_binary("xsh"));
            command.args(&request.tool_args);
            command.arg(&script_path);
            command
        }
        NativeTestRunKind::XshtTrace => {
            let mut command = Command::new(test_binary("xsht"));
            command.arg("trace");
            command.args(
                request
                    .tool_args
                    .iter()
                    .filter(|arg| arg.as_str() != "--trace"),
            );
            command.arg(&script_path);
            command
        }
    };
    command.args(&request.script_args);
    command.envs(&request.env);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if request.stdin.is_empty() {
        command.stdin(Stdio::null());
    } else {
        command.stdin(Stdio::piped());
    }

    let mut child = command.spawn().map_err(|error| {
        RuntimeError::new(native_test_error_kind(request.kind), error.to_string())
            .with_span(request.span)
    })?;
    if !request.stdin.is_empty()
        && let Some(mut child_stdin) = child.stdin.take()
    {
        child_stdin.write_all(&request.stdin).map_err(|error| {
            RuntimeError::new(native_test_error_kind(request.kind), error.to_string())
                .with_span(request.span)
        })?;
    }
    let output = child.wait_with_output().map_err(|error| {
        RuntimeError::new(native_test_error_kind(request.kind), error.to_string())
            .with_span(request.span)
    })?;
    let status = output
        .status
        .code()
        .or_else(|| output.status.signal().map(|signal| 128 + signal))
        .unwrap_or(-1) as i64;

    Ok(Value::Record(RecordMap::from([
        (Arc::from("success"), Value::Bool(output.status.success())),
        (Arc::from("status"), Value::Int(status)),
        (
            Arc::from("stdout"),
            Value::Str(String::from_utf8_lossy(&output.stdout).into()),
        ),
        (
            Arc::from("stderr"),
            Value::Str(String::from_utf8_lossy(&output.stderr).into()),
        ),
        (Arc::from("stdout_bytes"), Value::Bytes(output.stdout)),
        (Arc::from("stderr_bytes"), Value::Bytes(output.stderr)),
    ])))
}

fn native_test_error_kind(kind: NativeTestRunKind) -> &'static str {
    match kind {
        NativeTestRunKind::Xsh => "test-run-xsh",
        NativeTestRunKind::XshtTrace => "test-run-xsht-trace",
    }
}

fn test_binary(name: &str) -> PathBuf {
    let env_name = format!("CARGO_BIN_EXE_{name}");
    if let Some(path) = std::env::var_os(env_name) {
        return PathBuf::from(path);
    }
    if let Ok(current) = std::env::current_exe()
        && let Some(dir) = current.parent()
    {
        let sibling = dir.join(name);
        if sibling.exists() {
            return sibling;
        }
    }
    let target_debug = PathBuf::from("target/debug").join(name);
    if target_debug.exists() {
        return target_debug;
    }
    PathBuf::from(name)
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn text_bytes(text: impl Into<String>) -> Vec<u8> {
    text.into().into_bytes()
}

fn bytes_text_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn test_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("{}-{nanos}", std::process::id())
}

fn sanitize_test_id(id: &str) -> String {
    id.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn test_context_value(id: &str, file: &str, temp_root: &Path) -> Result<Value, String> {
    let file = PathValue::from_text(file)
        .map_err(|error| format!("invalid test file path: {}", error.message))?;
    let temp_root = PathValue::from_text(temp_root.to_string_lossy())
        .map_err(|error| format!("invalid temp root path: {}", error.message))?;
    Ok(Value::Record(RecordMap::from([
        (Arc::from("name"), Value::Str(id.into())),
        (Arc::from("file"), Value::Path(file)),
        (Arc::from("temp_root"), Value::Path(temp_root)),
    ])))
}

fn read_nested_coverage_traces(dir: &Path) -> Result<String, String> {
    if !dir.exists() {
        return Ok(String::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(|err| {
        format!(
            "failed to read nested coverage directory '{}': {err}",
            dir.display()
        )
    })? {
        let entry = entry.map_err(|err| format!("failed to read nested coverage entry: {err}"))?;
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort_unstable();

    let mut output = String::new();
    for file in files {
        let text = fs::read_to_string(&file).map_err(|err| {
            format!(
                "failed to read nested coverage trace '{}': {err}",
                file.display()
            )
        })?;
        output.push_str(&text);
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok(output)
}

fn run_example_test(
    case: ExampleTestCase,
    index: usize,
    run_id: &str,
    options: &TestOptions,
) -> TestOutcome {
    let syscalls = options.trace_top_syscalls.is_some();
    let coverage_trace_dir = options.collect_coverage().then(|| {
        std::env::temp_dir().join(format!(
            "xsh-test-{run_id}-{index}-{}-coverage-traces",
            sanitize_test_id(&case.id)
        ))
    });
    let output: CliOutput = if syscalls {
        trace_script(TraceOptions {
            script: case.path.clone(),
            args: case.args,
            raw: false,
            format: TraceFormat::Text,
            file: None,
            syscalls: true,
            top_syscalls: options.trace_top_syscalls.unwrap_or(8),
        })
    } else {
        run_script(RunOptions {
            script: case.path.clone(),
            args: case.args,
            coverage_trace_dir: coverage_trace_dir.clone(),
            strict_lower: options.strict_lower,
        })
        .into()
    };
    let mut failures = Vec::new();
    let coverage_trace = if let Some(dir) = &coverage_trace_dir {
        match read_nested_coverage_traces(dir) {
            Ok(trace) => Some(trace),
            Err(message) => {
                failures.push(format!("coverage: {message}"));
                None
            }
        }
    } else {
        None
    };
    if let Some(dir) = &coverage_trace_dir
        && !options.keep_temp
    {
        let _ = fs::remove_dir_all(dir);
    }
    if i32::from(output.status) != case.expected_status {
        failures.push(format!(
            "expected status {}, found {}",
            case.expected_status, output.status
        ));
    }
    if let Err(message) = output_policy_error(&case.stdout, &output.stdout, "stdout") {
        failures.push(message);
    }
    if let Err(message) = output_policy_error(&case.stderr, &output.stderr, "stderr") {
        failures.push(message);
    }
    let kind = if failures.is_empty() {
        TestOutcomeKind::Passed
    } else {
        TestOutcomeKind::Failed(format!("{}: {}", case.path, failures.join("; ")))
    };
    TestOutcome {
        kind,
        stdout: output.stdout,
        stderr: output.stderr,
        trace_text: output.trace_text,
        syscall_summary: output.syscall_summary,
        coverage_trace,
        coverage_scope: "examples",
    }
}

fn classify_native_test_result(result: Option<Value>) -> TestOutcomeKind {
    match result {
        Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit) => {
            TestOutcomeKind::Passed
        }
        Some(Value::Result(ResultValue::Err(error))) => {
            let kind = error.error_kind().unwrap_or("error").to_string();
            let message = error.error_message().unwrap_or("").to_string();
            if kind == "test-skip" {
                TestOutcomeKind::Skipped(message)
            } else {
                TestOutcomeKind::Failed(format!("{kind}: {message}"))
            }
        }
        Some(Value::Unit) => TestOutcomeKind::Passed,
        Some(value) => TestOutcomeKind::Failed(format!(
            "test proc returned {}, expected Result[Unit]",
            value.type_name()
        )),
        None => TestOutcomeKind::Failed("test proc did not return a value".to_string()),
    }
}

fn output_policy_error(policy: &OutputPolicy, actual: &[u8], stream: &str) -> Result<(), String> {
    let actual_text = bytes_text_lossy(actual);
    match policy {
        OutputPolicy::Exact(expected) if actual == expected.as_bytes() => Ok(()),
        OutputPolicy::Contains(expected) if actual_text.contains(expected) => Ok(()),
        OutputPolicy::Empty if actual.is_empty() => Ok(()),
        OutputPolicy::Any => Ok(()),
        OutputPolicy::Exact(expected) => Err(format!(
            "{stream} did not match exactly (expected {} bytes, found {} bytes)",
            expected.len(),
            actual.len()
        )),
        OutputPolicy::Contains(expected) => Err(format!(
            "{stream} did not contain expected text `{expected}`"
        )),
        OutputPolicy::Empty => Err(format!("{stream} was not empty")),
    }
}

fn check_syscall_budget(summary: &SyscallSummary, budgets: &BTreeMap<String, u64>) -> Vec<String> {
    let mut failures = Vec::new();
    for (key, &limit) in budgets {
        let actual = if key == "total" {
            summary.syscall_count
        } else {
            summary
                .by_syscall
                .iter()
                .find(|row| row.syscall == *key)
                .map_or(0, |row| row.calls)
        };
        if actual > limit {
            failures.push(format!("{key}={actual} > budget {limit}"));
        }
    }
    failures
}

fn write_syscall_baseline(entries: &[(String, SyscallSummary)], path: &str) -> Result<(), String> {
    let mut tests = Object::new();
    for (id, summary) in entries {
        let mut by_syscall = Object::new();
        for row in &summary.by_syscall {
            by_syscall.insert(row.syscall.clone(), json_u64(row.calls));
        }
        let mut entry = Object::new();
        entry.insert("syscall_count".to_string(), json_u64(summary.syscall_count));
        if let Some(wall_ns) = summary.wall_time_ns {
            let wall_ms = wall_ns / 1_000_000;
            entry.insert("wall_ms".to_string(), json_u64(wall_ms));
        }
        entry.insert("by_syscall".to_string(), JsonValue::Object(by_syscall));
        tests.insert(id.clone(), JsonValue::Object(entry));
    }

    let mut root = Object::new();
    root.insert("version".to_string(), json_u64(1));
    root.insert("tests".to_string(), JsonValue::Object(tests));

    let json = miniserde::json::to_string(&JsonValue::Object(root));
    fs::write(path, json).map_err(|e| format!("failed to write '{path}': {e}"))?;
    Ok(())
}

fn json_u64(value: u64) -> JsonValue {
    JsonValue::Number(miniserde::json::Number::U64(value))
}
