#![allow(clippy::single_call_fn)]

use crate::xsht::cli::{
    CliOutput, CoverageCollector, cancellation_output, collect_configured_xsh_files,
    collect_xsh_files, load_config,
};
use crate::xsht::trace::{CoverageTraceRenderer, TracebackRenderer};
use std::fs;
use std::io::{IsTerminal, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label};
use xsh::execution::evaluator::{
    Evaluator, NativeTestRunKind, NativeTestRunRequest, PreparedTestProgram,
};
use xsh::execution::script::XSH_COVERAGE_TRACE_DIR;
use xsh::execution::value::{PathValue, RecordMap, ResultValue, RuntimeError, Value};
use xsh::frontend::check::Checker;
use xsh::frontend::check::Type;
use xsh::frontend::load::parse_script_with_module_roots;
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaStmtKind, FunctionDefId, StmtId};
use xsh::process::{cancellation_escalated_signal, cancellation_requested_signal, path_bytes};

#[derive(Clone, Debug)]
pub(crate) struct TestOptions {
    pub(crate) filter: Option<String>,
    pub(crate) list: bool,
    pub(crate) exact: bool,
    pub(crate) nocapture: bool,
    pub(crate) fail_fast: bool,
    pub(crate) keep_temp: bool,
    pub(crate) jobs: Option<usize>,
    pub(crate) coverage: bool,
    pub(crate) api: bool,
    pub(crate) coverage_json_out: Option<String>,
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
    let mut coverage_source_files = Vec::new();
    let coverage_module_roots;
    let coverage_exclude;
    let mut stdout = String::new();
    let stderr = String::new();

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
    coverage_module_roots = module_roots.clone();
    coverage_exclude = config.coverage.exclude.clone();
    if options.collect_coverage()
        && let Err(message) =
            collect_configured_xsh_files(Path::new("."), &config, &mut coverage_source_files)
    {
        return CliOutput {
            status: 2,
            stdout: stdout.into_bytes(),
            stderr: text_bytes(format!("xsht: {message}\n")),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }
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

    write_test_output(&format!("running {} tests\n", cases.len()));

    let run_id = test_run_id();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut failure_details = Vec::new();
    let mut coverage = options
        .collect_coverage()
        .then(|| CoverageCollector::with_api_and_excludes(options.api, &coverage_exclude));
    if let Some(collector) = coverage.as_mut() {
        collector.register_source_files(&coverage_source_files, &coverage_module_roots);
    }
    let mut interrupted = None;
    run_test_cases(cases, &run_id, &options, |id, outcome| {
        if let Some(output) = cancellation_output() {
            interrupted = Some(output);
            return false;
        }

        if options.nocapture {
            write_test_output(&bytes_text_lossy(&outcome.stdout));
            write_test_error(&bytes_text_lossy(&outcome.stderr));
        }
        if let Some(collector) = coverage.as_mut()
            && let Some(trace) = &outcome.coverage_trace
            && let Err(message) = collector.ingest_jsonl(trace)
        {
            failed += 1;
            write_test_output("test coverage ... FAILED\n");
            failure_details.push(("coverage".to_string(), message, Vec::new(), Vec::new()));
            if options.fail_fast {
                return false;
            }
        }
        let stop = match &outcome.kind {
            TestOutcomeKind::Passed => {
                passed += 1;
                write_test_result(&id, "ok", outcome.duration);
                false
            }
            TestOutcomeKind::Skipped(message) => {
                skipped += 1;
                let status = if message.is_empty() {
                    "skipped".to_string()
                } else {
                    format!("skipped: {message}")
                };
                write_test_result(&id, &status, outcome.duration);
                false
            }
            TestOutcomeKind::Failed(message) => {
                failed += 1;
                write_test_result(&id, "FAILED", outcome.duration);
                failure_details.push((id.clone(), message.clone(), outcome.stdout, outcome.stderr));
                options.fail_fast
            }
        };
        !stop
    });
    if interrupted.is_none()
        && let Some(output) = cancellation_output()
    {
        return output;
    }
    if let Some(output) = interrupted {
        return output;
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

fn run_test_cases<F>(cases: Vec<TestCase>, run_id: &str, options: &TestOptions, mut on_outcome: F)
where
    F: FnMut(String, TestOutcome) -> bool,
{
    let jobs = test_jobs(options, cases.len());
    if jobs <= 1 {
        for (index, case) in cases.into_iter().enumerate() {
            let id = case.id().to_string();
            let outcome = run_test_case(case, index, run_id, options);
            if !on_outcome(id, outcome) {
                break;
            }
        }
        return;
    }

    run_test_cases_parallel(cases, run_id, options, jobs, on_outcome);
}

fn test_jobs(options: &TestOptions, cases_len: usize) -> usize {
    if cases_len <= 1 || options.fail_fast || options.nocapture {
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

fn run_test_cases_parallel<F>(
    cases: Vec<TestCase>,
    run_id: &str,
    options: &TestOptions,
    jobs: usize,
    mut on_outcome: F,
) where
    F: FnMut(String, TestOutcome) -> bool,
{
    let queue = Arc::new(Mutex::new(
        cases
            .into_iter()
            .enumerate()
            .collect::<std::collections::VecDeque<_>>(),
    ));
    let (tx, rx) = mpsc::channel();

    let options = Arc::new(options.clone());
    let run_id = Arc::new(run_id.to_string());
    for _ in 0..jobs {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();
        let options = Arc::clone(&options);
        let run_id = Arc::clone(&run_id);
        std::thread::spawn(move || {
            loop {
                if cancellation_requested_signal().is_some() {
                    break;
                }
                let Some((index, case)) = queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .pop_front()
                else {
                    break;
                };
                let id = case.id().to_string();
                let outcome = run_test_case(case, index, &run_id, &options);
                if tx.send((index, id, outcome)).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok((_, id, outcome)) => {
                if !on_outcome(id, outcome) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancellation_escalated_signal().is_some() {
                    unsafe { libc::_exit(130) };
                }
                if cancellation_requested_signal().is_some() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

enum TestCase {
    Native(NativeTestCase),
    Invalid { id: String, message: String },
}

impl TestCase {
    fn id(&self) -> &str {
        match self {
            Self::Native(case) => &case.id,
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

struct TestOutcome {
    kind: TestOutcomeKind,
    duration: Duration,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    coverage_trace: Option<String>,
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
            .map(xsh::frontend::source::SourceFile::id)
            .unwrap_or_else(|| xsh::frontend::source::SourceId::new(0));
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

fn run_test_case(case: TestCase, index: usize, run_id: &str, options: &TestOptions) -> TestOutcome {
    let started = Instant::now();
    let mut outcome = match case {
        TestCase::Native(case) => run_native_test(case, index, run_id, options),
        TestCase::Invalid { message, .. } => TestOutcome {
            kind: TestOutcomeKind::Failed(message),
            duration: Duration::ZERO,
            stdout: Vec::new(),
            stderr: Vec::new(),
            coverage_trace: None,
        },
    };
    outcome.duration = started.elapsed();
    outcome
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
            duration: Duration::ZERO,
            stdout: Vec::new(),
            stderr: Vec::new(),
            coverage_trace: None,
        };
    }

    let xsh_binary = absolute_path(&test_binary("xsh"));
    let ctx = if case.has_ctx {
        match test_context_value(&case.id, &case.file, &temp_root, &xsh_binary) {
            Ok(ctx) => ctx,
            Err(message) => {
                return TestOutcome {
                    kind: TestOutcomeKind::Failed(message),
                    duration: Duration::ZERO,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    coverage_trace: None,
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
    env_overlay.push((b"CARGO_BIN_EXE_xsh".to_vec(), path_bytes(&xsh_binary)));
    if let Some(tool_dir) = xsh_binary.parent() {
        let mut path_entries = vec![tool_dir.to_path_buf()];
        if let Some(path) = std::env::var_os("PATH") {
            path_entries.extend(std::env::split_paths(&path));
        }
        if let Ok(path) = std::env::join_paths(path_entries) {
            env_overlay.push((b"PATH".to_vec(), path.into_vec()));
        }
    }
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
        let mut trace = CoverageTraceRenderer::new()
            .render_events(&evaluated.output.trace_events, &evaluated.output.sources);
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
        duration: Duration::ZERO,
        stdout: evaluated.output.stdout,
        stderr: evaluated.output.stderr,
        coverage_trace,
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

fn write_test_output(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

fn write_test_error(text: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(text.as_bytes());
    let _ = stderr.flush();
}

fn write_test_result(id: &str, status: &str, duration: Duration) {
    let status = if test_color_enabled() {
        let color = match status {
            "ok" => "\x1b[32m",
            "FAILED" => "\x1b[31m",
            _ => "\x1b[33m",
        };
        format!("{color}{status}\x1b[0m")
    } else {
        status.to_string()
    };
    let duration_text = format_test_duration(duration);
    let duration_text = if test_color_enabled() {
        let color = if duration < Duration::from_millis(500) {
            "\x1b[90m"
        } else if duration < Duration::from_secs(1) {
            "\x1b[33m"
        } else {
            "\x1b[31m"
        };
        format!("{color}{duration_text}\x1b[0m")
    } else {
        duration_text
    };
    write_test_output(&format!("{id} ... {status} {duration_text}\n"));
}

fn test_color_enabled() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn format_test_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1_000 {
        return format!("{millis}ms");
    }

    let seconds = millis / 1_000;
    if seconds < 60 {
        if millis.is_multiple_of(1_000) {
            return format!("{seconds}s");
        }
        return format!("{:.1}s", millis as f64 / 1_000.0);
    }

    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    if minutes < 60 {
        return if remaining_seconds == 0 {
            format!("{minutes}m")
        } else {
            format!("{minutes}m{remaining_seconds}s")
        };
    }

    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;
    if remaining_minutes == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h{remaining_minutes}m")
    }
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

fn test_context_value(
    id: &str,
    file: &str,
    temp_root: &Path,
    xsh_binary: &Path,
) -> Result<Value, String> {
    let file = PathValue::from_text(file)
        .map_err(|error| format!("invalid test file path: {}", error.message))?;
    let temp_root = PathValue::from_text(temp_root.to_string_lossy())
        .map_err(|error| format!("invalid temp root path: {}", error.message))?;
    let core_dir = Path::new(&file.display())
        .parent()
        .and_then(Path::parent)
        .map(absolute_path)
        .unwrap_or_else(|| absolute_path(Path::new(".")));
    let core_dir = PathValue::from_text(core_dir.to_string_lossy())
        .map_err(|error| format!("invalid core directory path: {}", error.message))?;
    let xsh_bin = PathValue::from_text(xsh_binary.to_string_lossy())
        .map_err(|error| format!("invalid xsh binary path: {}", error.message))?;
    Ok(Value::Record(RecordMap::from([
        (Arc::from("name"), Value::Str(id.into())),
        (Arc::from("file"), Value::Path(file)),
        (Arc::from("temp_root"), Value::Path(temp_root)),
        (Arc::from("core_dir"), Value::Path(core_dir)),
        (Arc::from("xsh_bin"), Value::Path(xsh_bin)),
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

#[cfg(test)]
mod tests {
    use super::{Duration, format_test_duration};

    #[test]
    fn formats_test_durations_for_humans() {
        assert_eq!(format_test_duration(Duration::from_millis(5)), "5ms");
        assert_eq!(format_test_duration(Duration::from_millis(1_500)), "1.5s");
        assert_eq!(
            format_test_duration(Duration::from_secs(2 * 60 + 5)),
            "2m5s"
        );
    }
}
