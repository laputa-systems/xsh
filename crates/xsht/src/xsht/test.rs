#![allow(clippy::single_call_fn)]

use crate::xsht::cli::{
    CliOutput, ConfigCache, CoverageCollector, DiscoveryFor, FRONTEND_WORKER_STACK_BYTES,
    cancellation_output, collect_xsh_files, discover_scripts, load_config,
};
use crate::xsht::config::FileToolConfig;
use crate::xsht::trace::{CoverageTraceRenderer, TracebackRenderer};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{IsTerminal, Write};
use std::os::fd::AsFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use rustix::fs::{AtFlags, CWD, Dir, Mode, OFlags};
use rustix::io::Errno;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticRenderer, Label};
use xsh::execution::evaluator::{
    Evaluator, NativeTestRunKind, NativeTestRunRequest, PreparedTestProgram, TestCancellation,
    NATIVE_TEST_FORCE_FRAMES_ENV,
};
use xsh::execution::script::XSH_COVERAGE_TRACE_DIR;
use xsh::execution::value::{PathValue, RecordMap, ResultValue, RuntimeError, Value};
use xsh::frontend::check::Type;
use xsh::frontend::check::{CheckOptions, CheckSession, Checker};
use xsh::frontend::load::{
    ArenaEntryInput, LoadCancelled, parse_load_entries_shared_arena_only_with_cancellation,
};
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaRange, ArenaStmtKind, FunctionDefId, StmtId};
use xsh::process::{cancellation_escalated_signal, cancellation_requested_signal, path_bytes};

#[derive(Clone, Debug)]
pub(crate) struct TestOptions {
    pub(crate) filter: Option<String>,
    pub(crate) list: bool,
    pub(crate) exact: bool,
    pub(crate) exact_ids: Vec<String>,
    pub(crate) events_jsonl: Option<std::path::PathBuf>,
    pub(crate) nocapture: bool,
    pub(crate) fail_fast: bool,
    pub(crate) keep_temp: bool,
    pub(crate) jobs: Option<usize>,
    /// Per-test time limit; `None` disables timeouts.
    pub(crate) timeout: Option<Duration>,
    pub(crate) coverage: bool,
    pub(crate) api: bool,
    pub(crate) coverage_json_out: Option<String>,
    pub(crate) force_frames: bool,
}

impl TestOptions {
    fn collect_coverage(&self) -> bool {
        self.coverage || self.coverage_json_out.is_some()
    }
}

#[derive(Clone, Debug)]
struct RouteWitnessConfig {
    test_id: String,
    output_dir: PathBuf,
}

fn route_witness_config() -> Result<Option<RouteWitnessConfig>, String> {
    let test_id_name = format!(
        "{}_TEST_ID",
        xsh::execution::evaluator::NATIVE_TEST_ROUTE_WITNESS_HARNESS_ENV_PREFIX
    );
    let output_dir_name = format!(
        "{}_OUTPUT_DIR",
        xsh::execution::evaluator::NATIVE_TEST_ROUTE_WITNESS_HARNESS_ENV_PREFIX
    );
    let test_id = std::env::var_os(test_id_name);
    let output_dir = std::env::var_os(output_dir_name);
    match (test_id, output_dir) {
        (None, None) => Ok(None),
        (Some(test_id), Some(output_dir)) => {
            let test_id = test_id
                .into_string()
                .map_err(|_| "route-witness harness test identity is not UTF-8".to_string())?;
            if test_id.is_empty() {
                return Err("route-witness harness test identity is empty".to_string());
            }
            let output_dir = PathBuf::from(output_dir);
            fs::create_dir_all(&output_dir).map_err(|error| {
                format!("failed to create route-witness output directory: {error}")
            })?;
            Ok(Some(RouteWitnessConfig {
                test_id,
                output_dir,
            }))
        }
        _ => Err("route-witness harness configuration is incomplete".to_string()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestTerminal { Passed, Failed, Skipped, TimedOut, Cancelled, NotExecuted }

impl TestTerminal {
    fn name(self) -> &'static str {
        match self {
            Self::Passed => "passed", Self::Failed => "failed", Self::Skipped => "skipped",
            Self::TimedOut => "timed_out", Self::Cancelled => "cancelled", Self::NotExecuted => "not_executed",
        }
    }
}

enum RunCompletion {
    Success,
    CaseFailure,
    CoverageFailure(String),
    CleanupFailure(String),
    RunnerFailure(String),
    RequiredSkipped,
    Cancelled,
}

impl RunCompletion {
    fn select(cancelled: bool, cleanup: Option<&str>, runner: Option<&str>, coverage: Option<&str>, failed: bool, required_skipped: bool) -> Self {
        if cancelled { Self::Cancelled }
        else if let Some(error) = cleanup { Self::CleanupFailure(error.to_owned()) }
        else if let Some(error) = runner { Self::RunnerFailure(error.to_owned()) }
        else if let Some(error) = coverage { Self::CoverageFailure(error.to_owned()) }
        else if failed { Self::CaseFailure }
        else if required_skipped { Self::RequiredSkipped }
        else { Self::Success }
    }

    fn status(&self) -> u8 {
        match self {
            Self::Success => 0,
            Self::CaseFailure | Self::CoverageFailure(_) => 1,
            Self::CleanupFailure(_) | Self::RunnerFailure(_) | Self::RequiredSkipped => 2,
            Self::Cancelled => 130,
        }
    }

    fn reason(&self) -> xsh::host::json::Value {
        let (kind, detail) = match self {
            Self::CoverageFailure(error) => ("coverage_failure", error.as_str()),
            Self::CleanupFailure(error) => ("cleanup_failure", error.as_str()),
            Self::RunnerFailure(error) => ("runner_failure", error.as_str()),
            Self::Cancelled => ("cancellation", "native test run interrupted"),
            _ => return xsh::host::json::Value::null(),
        };
        json_fields([("kind", json_string(kind)), ("detail", json_string(detail))])
    }
}

fn native_cancellation_output() -> Option<CliOutput> {
    cancellation_output().map(normalize_native_cancellation)
}

fn normalize_native_cancellation(mut output: CliOutput) -> CliOutput {
    output.status = 130;
    output
}

enum CaseLifecycle { Pending, Running, Finished(TestTerminal) }
enum EventOutput { Disabled, Open(fs::File), Failed(String) }

struct TestRunLedger {
    run_id: String,
    cases: BTreeMap<String, CaseLifecycle>,
    output: EventOutput,
    finished: bool,
}

fn json_string(value: impl Into<String>) -> xsh::host::json::Value {
    xsh::host::json::Value::from(value.into())
}
fn json_number(value: u64) -> xsh::host::json::Value {
    xsh::host::json::Value::from(value)
}
fn json_fields(fields: impl IntoIterator<Item = (&'static str, xsh::host::json::Value)>) -> xsh::host::json::Value {
    fields.into_iter().collect()
}
fn duration_ns(duration: Duration) -> Result<xsh::host::json::Value, String> {
    u64::try_from(duration.as_nanos()).map(json_number).map_err(|_| "test duration exceeds the schema-1 nanosecond range".to_owned())
}

impl TestRunLedger {
    fn new(run_id: &str, ids: impl IntoIterator<Item = String>, path: Option<&Path>) -> Result<Self, String> {
        let mut cases = BTreeMap::new();
        for id in ids {
            if cases.insert(id.clone(), CaseLifecycle::Pending).is_some() { return Err(format!("duplicate discovered test ID: {id}")); }
        }
        let output = match path {
            Some(path) => EventOutput::Open(fs::OpenOptions::new().write(true).create_new(true).open(path)
                .map_err(|error| format!("cannot create test events {}: {error}", path.display()))?),
            None => EventOutput::Disabled,
        };
        let selected = cases.keys().cloned().map(json_string).collect::<xsh::host::json::Value>();
        let mut ledger = Self { run_id: run_id.to_owned(), cases, output, finished: false };
        ledger.emit("run_started", [("selected", selected)])?;
        Ok(ledger)
    }

    fn emit(&mut self, event: &'static str, fields: impl IntoIterator<Item = (&'static str, xsh::host::json::Value)>) -> Result<(), String> {
        let mut record = BTreeMap::from([
            ("schema".to_owned(), json_number(1)),
            ("run_id".to_owned(), json_string(&self.run_id)),
            ("event".to_owned(), json_string(event)),
        ]);
        record.extend(fields.into_iter().map(|(key, value)| (key.to_owned(), value)));
        match &mut self.output {
            EventOutput::Disabled => Ok(()),
            EventOutput::Failed(message) => Err(message.clone()),
            EventOutput::Open(file) => {
                let line = xsh::host::json::encode(&record.into_iter().collect())?;
                if let Err(error) = file.write_all(line.as_bytes()).and_then(|()| file.write_all(b"\n")).and_then(|()| file.flush()) {
                    let message = format!("cannot write test events: {error}");
                    self.output = EventOutput::Failed(message.clone());
                    return Err(message);
                }
                Ok(())
            }
        }
    }

    fn start(&mut self, id: &str) -> Result<(), String> {
        if self.finished { return Err("test started after the final summary".into()); }
        let state = self.cases.get_mut(id).ok_or_else(|| format!("unselected test started: {id}"))?;
        if !matches!(state, CaseLifecycle::Pending) { return Err(format!("test started more than once: {id}")); }
        *state = CaseLifecycle::Running;
        self.emit("case_started", [("case_id", json_string(id))])
    }

    fn finish(&mut self, id: &str, outcome: &TestOutcome) -> Result<(), String> {
        let terminal = match outcome.kind {
            TestOutcomeKind::Passed => TestTerminal::Passed,
            TestOutcomeKind::Failed(_) => TestTerminal::Failed,
            TestOutcomeKind::Skipped(_) => TestTerminal::Skipped,
            TestOutcomeKind::TimedOut { .. } => TestTerminal::TimedOut,
            TestOutcomeKind::Cancelled(_) => TestTerminal::Cancelled,
        };
        self.finish_case(id, terminal, outcome.duration, outcome.kind.reason())
    }

    fn finish_case(&mut self, id: &str, terminal: TestTerminal, duration: Duration, reason: xsh::host::json::Value) -> Result<(), String> {
        if self.finished { return Err("test finished after the final summary".into()); }
        let state = self.cases.get_mut(id).ok_or_else(|| format!("unselected test finished: {id}"))?;
        if !matches!(state, CaseLifecycle::Running) && !(matches!(state, CaseLifecycle::Pending) && terminal == TestTerminal::NotExecuted) {
            return Err(format!("test has an invalid terminal transition: {id}"));
        }
        *state = CaseLifecycle::Finished(terminal);
        self.emit("case_finished", [("case_id", json_string(id)), ("outcome", json_string(terminal.name())), ("reason", reason), ("duration_ns", duration_ns(duration)?)])
    }

    fn settle(&mut self, reason: &str) -> Result<(), String> {
        let pending = self.cases.iter().filter_map(|(id, state)| match state {
            CaseLifecycle::Pending => Some((id.clone(), TestTerminal::NotExecuted)),
            CaseLifecycle::Running => Some((id.clone(), TestTerminal::Failed)),
            CaseLifecycle::Finished(_) => None,
        }).collect::<Vec<_>>();
        let mut failure = None;
        for (id, terminal) in pending {
            if let Err(error) = self.finish_case(&id, terminal, Duration::ZERO, json_fields([("kind", json_string(if terminal == TestTerminal::NotExecuted { "not_admitted" } else { "missing_result" })), ("detail", json_string(reason))])) {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn final_summary(&mut self, completion: RunCompletion, elapsed: Duration) -> Result<(), String> {
        if self.finished { return Err("test run emitted more than one final summary".into()); }
        if self.cases.values().any(|state| !matches!(state, CaseLifecycle::Finished(_))) { return Err("test run is missing mandatory terminal results".into()); }
        let mut counts = [0u64; 6];
        for state in self.cases.values() {
            let CaseLifecycle::Finished(terminal) = state else { unreachable!() };
            counts[*terminal as usize] += 1;
        }
        let status = completion.status();
        match &completion {
            RunCompletion::Success if [TestTerminal::Failed, TestTerminal::TimedOut, TestTerminal::Cancelled, TestTerminal::NotExecuted]
                .into_iter().any(|terminal| counts[terminal as usize] != 0) => {
                    return Err("successful test summary contradicts terminal outcomes".into());
                }
            RunCompletion::CaseFailure if counts[TestTerminal::Failed as usize] == 0 && counts[TestTerminal::TimedOut as usize] == 0 => {
                return Err("failed test summary has no failed or timed-out outcome".into());
            }
            _ => {}
        }
        self.finished = true;
        let counts = [TestTerminal::Passed, TestTerminal::Failed, TestTerminal::Skipped, TestTerminal::TimedOut, TestTerminal::Cancelled, TestTerminal::NotExecuted]
            .into_iter().zip(counts).map(|(terminal, count)| (terminal.name(), json_number(count)));
        self.emit("run_finished", [("status", json_number(u64::from(status))), ("reason", completion.reason()), ("selected", json_number(self.cases.len() as u64)), ("completed", json_number(self.cases.len() as u64)), ("counts", json_fields(counts)), ("duration_ns", duration_ns(elapsed)?)])
    }
}

pub(crate) fn test_scripts(options: TestOptions) -> CliOutput {
    if let Some(output) = native_cancellation_output() {
        return output;
    }
    if options.exact && options.filter.is_none() {
        return CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes("xsht: `--exact` requires FILTER\n"),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    if !options.exact_ids.is_empty() && (options.filter.is_some() || options.exact) {
        return test_input_error("`--exact-id` cannot combine with FILTER or `--exact`".into());
    }
    if options.list && options.events_jsonl.is_some() {
        return test_input_error("`--events-jsonl` cannot combine with `--list`".into());
    }
    let mut requested = BTreeSet::new();
    for id in &options.exact_ids {
        if id.is_empty() { return test_input_error("exact test ID must not be empty".into()); }
        if !requested.insert(id) { return test_input_error(format!("duplicate exact test ID: {id}")); }
    }
    let mut cases = Vec::new();
    let mut coverage_source_files: Vec<PathBuf> = Vec::new();
    let mut stdout = String::new();
    let stderr = String::new();

    let route_witness = match route_witness_config() {
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
    let coverage_exclude = config.coverage.exclude.clone();
    let configs = ConfigCache::default();
    if options.collect_coverage() {
        match discover_scripts(&[], &config, &configs, DiscoveryFor::Scripts) {
            Ok(files) => coverage_source_files = files,
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
    let test_roots: Vec<PathBuf> = if config.test_roots.is_empty() {
        vec![PathBuf::from("tests")]
    } else {
        config.test_roots.iter().map(PathBuf::from).collect()
    };
    match discover_native_tests(&test_roots, &configs, &options) {
        Ok(native) => cases.extend(native),
        Err(NativeTestDiscoveryError::Cancelled) => {
            return native_cancellation_output()
                .expect("native test discovery only cancels after a signal request");
        }
        Err(NativeTestDiscoveryError::Failure(message)) => {
            if let Some(output) = native_cancellation_output() {
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
    cases.sort_unstable_by(|left, right| left.id().cmp(right.id()));

    if options.exact && cases.is_empty()
        && let Some(filter) = options.filter.as_deref()
    {
        return CliOutput {
            status: 2,
            stdout: stdout.into_bytes(),
            stderr: text_bytes(format!(
                "xsht: no native tests matched exact filter: {filter}\n"
            )),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    if !options.exact_ids.is_empty() {
        if let Some(TestCase::Invalid { id, message }) = cases.iter().find(|case| matches!(case, TestCase::Invalid { .. })) {
            return test_input_error(format!("invalid selected native test module {id}:\n{message}"));
        }
        let available = cases.iter().map(TestCase::id).collect::<BTreeSet<_>>();
        for id in &options.exact_ids {
            if !available.contains(id.as_str()) {
                return test_input_error(format!("missing exact test ID: {id}"));
            }
        }
    }
    if options.list {
        for case in &cases {
            if let Some(output) = native_cancellation_output() {
                return output;
            }
            stdout.push_str(case.id());
            stdout.push('\n');
        }
        if let Some(output) = native_cancellation_output() { return output; }
        return CliOutput {
            status: 0,
            stdout: stdout.into_bytes(),
            stderr: stderr.into_bytes(),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    let run_id = test_run_id();
    let started = Instant::now();
    let mut ledger = match TestRunLedger::new(&run_id, cases.iter().map(|case| case.id().to_owned()), options.events_jsonl.as_deref()) {
        Ok(ledger) => ledger,
        Err(error) => return test_input_error(error),
    };
    write_test_output(&format!("running {} tests\n", cases.len()));
    let mut lifecycle_error = None;
    let mut cleanup_error = None;
    let mut coverage_error = None;
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut failure_details = Vec::new();
    let mut coverage = options
        .collect_coverage()
        .then(|| CoverageCollector::with_api_and_excludes(options.api, &coverage_exclude));
    if let Some(collector) = coverage.as_mut() {
        collector.register_source_files(&coverage_source_files);
    }
    let mut interrupted = None;
    let run_result = run_test_cases(cases, &run_id, &options, route_witness.as_ref(), |event| {
        let (id, outcome) = match event {
            TestRunEvent::Started(id) => {
                if let Err(error) = ledger.start(&id) { lifecycle_error.get_or_insert(error); return false; }
                return true;
            }
            TestRunEvent::Finished(id, outcome) => (id, outcome),
        };
        if let Err(error) = ledger.finish(&id, &outcome) { lifecycle_error.get_or_insert(error); }
        if let Some(error) = &outcome.cleanup_error { cleanup_error.get_or_insert(error.clone()); }
        if let Some(output) = native_cancellation_output() { interrupted = Some(output); }

        if options.nocapture {
            write_test_output(&bytes_text_lossy(&outcome.stdout));
            write_test_error(&bytes_text_lossy(&outcome.stderr));
        }
        if let Some(collector) = coverage.as_mut()
            && let Some(trace) = &outcome.coverage_trace
            && let Err(message) = collector.ingest_jsonl(trace)
        {
            failed += 1;
            coverage_error.get_or_insert(message.clone());
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
            TestOutcomeKind::Cancelled(message) => {
                write_test_result(&id, "CANCELLED", outcome.duration);
                if options.nocapture && !message.is_empty() { write_test_error(message); }
                true
            }
            TestOutcomeKind::TimedOut { limit, detail } => {
                failed += 1;
                write_test_result(&id, "TIMEOUT", outcome.duration);
                let mut message = format!("TIMEOUT after {}\n", format_test_duration(*limit));
                message.push_str(detail);
                failure_details.push((id.clone(), message, outcome.stdout, outcome.stderr));
                options.fail_fast
            }
        };
        !stop && interrupted.is_none() && lifecycle_error.is_none() && cleanup_error.is_none()
    });
    terminate_child_groups();
    if let Err(error) = run_result { lifecycle_error.get_or_insert(error); }
    if interrupted.is_none() { interrupted = native_cancellation_output(); }
    let remainder_reason = if interrupted.is_some() { "run cancelled" }
        else if lifecycle_error.is_some() { "runner lifecycle failed" }
        else if cleanup_error.is_some() { "worker cleanup failed" }
        else { "stopped after the first failure" };
    if let Err(error) = ledger.settle(remainder_reason) { lifecycle_error.get_or_insert(error); }

    if let Some(path) = &options.coverage_json_out
        && let Some(collector) = coverage.as_ref()
        && let Err(message) = collector.write_json(path)
    {
        failed += 1;
        coverage_error.get_or_insert(message.clone());
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

    if interrupted.is_none() { interrupted = native_cancellation_output(); }
    let status_text = if interrupted.is_some() { "CANCELLED" }
        else if lifecycle_error.is_some() || cleanup_error.is_some() { "INCOMPLETE" }
        else if failed > 0 { "FAILED" }
        else if !options.exact_ids.is_empty() && skipped > 0 { "INCOMPLETE" }
        else { "ok" };
    stdout.push_str(&format!(
        "test result: {status_text}. {passed} passed; {failed} failed; {skipped} skipped\n"
    ));

    let completion = RunCompletion::select(interrupted.is_some(), cleanup_error.as_deref(), lifecycle_error.as_deref(),
        coverage_error.as_deref(), failed > 0, !options.exact_ids.is_empty() && skipped > 0);
    let mut status = completion.status();
    if let Err(error) = ledger.final_summary(completion, started.elapsed()) { lifecycle_error.get_or_insert(error); if status != 130 { status = 2; } }
    let mut stderr = stderr;
    if let Some(output) = interrupted { stderr.push_str(&bytes_text_lossy(&output.stderr)); }
    for error in lifecycle_error.into_iter().chain(cleanup_error) { stderr.push_str(&format!("xsht: {error}\n")); }
    CliOutput {
        status,
        stdout: stdout.into_bytes(),
        stderr: stderr.into_bytes(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

fn test_input_error(message: String) -> CliOutput {
    CliOutput { status: 2, stdout: Vec::new(), stderr: text_bytes(format!("xsht: {message}\n")), trace_text: String::new(), syscall_summary: None }
}

enum TestRunEvent { Started(String), Finished(String, TestOutcome) }

fn run_test_cases<F>(
    cases: Vec<TestCase>, run_id: &str, options: &TestOptions,
    route_witness: Option<&RouteWitnessConfig>, mut on_event: F,
) -> Result<(), String>
where F: FnMut(TestRunEvent) -> bool {
    let stopped = Arc::new(AtomicBool::new(false));
    let jobs = test_jobs(options, cases.len());
    if jobs <= 1 {
        for (index, case) in cases.into_iter().enumerate() {
            if cancellation_requested_signal().is_some() { break; }
            let id = case.id().to_owned();
            if !on_event(TestRunEvent::Started(id.clone())) { break; }
            let outcome = run_test_case(case, index, run_id, options, route_witness, &stopped);
            if !on_event(TestRunEvent::Finished(id, outcome)) { break; }
        }
        return Ok(());
    }
    run_test_cases_parallel(cases, run_id, options, route_witness, jobs, stopped, on_event)
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
    default_test_jobs_for(std::thread::available_parallelism().map_or(1, usize::from))
}

/// Use every visible logical CPU up to a fixed cap to scale large test runs
/// while bounding evaluator stacks and nested test subprocesses.
fn default_test_jobs_for(cpus: usize) -> usize {
    cpus.clamp(1, 32)
}

fn run_test_cases_parallel<F>(
    cases: Vec<TestCase>, run_id: &str, options: &TestOptions,
    route_witness: Option<&RouteWitnessConfig>, jobs: usize, stopped: Arc<AtomicBool>, mut on_event: F,
) -> Result<(), String>
where F: FnMut(TestRunEvent) -> bool {
    let queue = Arc::new((Mutex::new(ParallelTestQueue::new(cases)), Condvar::new()));
    let (tx, rx) = mpsc::sync_channel(jobs * 2);
    let options = Arc::new(options.clone());
    let route_witness = Arc::new(route_witness.cloned());
    let run_id = Arc::new(run_id.to_owned());
    let mut workers = Vec::new();
    let mut failure = None;
    for worker in 0..jobs {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();
        let options = Arc::clone(&options);
        let route_witness = Arc::clone(&route_witness);
        let run_id = Arc::clone(&run_id);
        let worker_stopped = Arc::clone(&stopped);
        match std::thread::Builder::new().name(format!("xsht-test-worker-{worker}")).spawn(move || {
            loop {
                let work = {
                    let (state, available) = &*queue;
                    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    loop {
                        if worker_stopped.load(Ordering::Acquire) || cancellation_requested_signal().is_some() { break None; }
                        if let Some(work) = state.take_next() { break Some(work); }
                        if state.is_complete() { break None; }
                        state = available.wait_timeout(state, Duration::from_millis(50)).unwrap_or_else(|poisoned| poisoned.into_inner()).0;
                    }
                };
                let Some((file_index, index, case)) = work else { break };
                let _lease = ActiveTestLease { queue: &queue, file_index };
                let id = case.id().to_owned();
                if tx.send(TestRunEvent::Started(id.clone())).is_err() { break; }
                let outcome = run_test_case(case, index, &run_id, &options, route_witness.as_ref().as_ref(), &worker_stopped);
                if tx.send(TestRunEvent::Finished(id, outcome)).is_err() { break; }
            }
        }) {
            Ok(handle) => workers.push(handle),
            Err(error) => { failure = Some(format!("cannot start native test worker: {error}")); stopped.store(true, Ordering::Release); break; }
        }
    }
    drop(tx);
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                if !on_event(event) && !stopped.swap(true, Ordering::AcqRel) {
                    queue.1.notify_all();
                    terminate_child_groups();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancellation_escalated_signal().is_some() { unsafe { libc::_exit(130) }; }
                if cancellation_requested_signal().is_some() && !stopped.swap(true, Ordering::AcqRel) {
                    queue.1.notify_all();
                    terminate_child_groups();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    for worker in workers {
        if let Err(panic) = worker.join() { failure.get_or_insert_with(|| format!("native test worker panicked: {}", test_panic_message(panic))); }
    }
    failure.map_or(Ok(()), Err)
}

fn test_panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<String>() { message.clone() }
    else if let Some(message) = panic.downcast_ref::<&str>() { (*message).to_owned() }
    else { "non-string panic payload".to_owned() }
}

/// Limit each file's admitted test cases so a large fixture file cannot starve
/// other files or overwhelm short child-process deadlines.
const MAX_ACTIVE_TESTS_PER_FILE: usize = 8;

struct ActiveTestLease<'queue> {
    queue: &'queue Arc<(Mutex<ParallelTestQueue>, Condvar)>,
    file_index: usize,
}

impl Drop for ActiveTestLease<'_> {
    fn drop(&mut self) {
        let (state, work_available) = &**self.queue;
        state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish(self.file_index);
        work_available.notify_all();
    }
}

struct TestFileQueue {
    cases: VecDeque<(usize, TestCase)>,
    active: usize,
    ready: bool,
}

struct ParallelTestQueue {
    files: Vec<TestFileQueue>,
    ready_files: VecDeque<usize>,
    pending: usize,
    active: usize,
}

impl ParallelTestQueue {
    fn new(cases: Vec<TestCase>) -> Self {
        let mut files: Vec<TestFileQueue> = Vec::new();
        let mut current_file = None;
        for (index, case) in cases.into_iter().enumerate() {
            let file = case
                .id()
                .split_once("::")
                .map_or(case.id(), |(file, _)| file);
            if current_file.as_deref() != Some(file) {
                files.push(TestFileQueue {
                    cases: VecDeque::new(),
                    active: 0,
                    ready: false,
                });
                current_file = Some(file.to_string());
            }
            files
                .last_mut()
                .expect("the current test file queue must exist")
                .cases
                .push_back((index, case));
        }

        let pending = files
            .iter()
            .map(|file| file.cases.len())
            .sum();
        let mut queue = Self {
            files,
            ready_files: VecDeque::new(),
            pending,
            active: 0,
        };
        for file_index in 0..queue.files.len() {
            queue.enqueue_if_ready(file_index);
        }
        queue
    }

    fn take_next(&mut self) -> Option<(usize, usize, TestCase)> {
        let file_index = self.ready_files.pop_front()?;
        let file = &mut self.files[file_index];
        file.ready = false;
        let (index, case) = file
            .cases
            .pop_front()
            .expect("a ready file must have a pending test");
        file.active += 1;
        self.active += 1;
        self.pending -= 1;
        self.enqueue_if_ready(file_index);
        Some((file_index, index, case))
    }

    fn finish(&mut self, file_index: usize) {
        let file = &mut self.files[file_index];
        debug_assert!(file.active > 0);
        file.active -= 1;
        self.active -= 1;
        self.enqueue_if_ready(file_index);
    }

    fn enqueue_if_ready(&mut self, file_index: usize) {
        let file = &mut self.files[file_index];
        if !file.cases.is_empty() && file.active < MAX_ACTIVE_TESTS_PER_FILE && !file.ready {
            file.ready = true;
            self.ready_files.push_back(file_index);
        }
    }

    fn is_complete(&self) -> bool {
        self.pending == 0 && self.active == 0
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
    module_path: Option<Arc<OsString>>,
}

struct TestOutcome {
    kind: TestOutcomeKind,
    duration: Duration,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    coverage_trace: Option<String>,
    cleanup_error: Option<String>,
}

enum TestOutcomeKind {
    Passed,
    Failed(String),
    Skipped(String),
    Cancelled(String),
    TimedOut {
        limit: Duration,
        detail: String,
    },
}

/// Only explicitly registered declarations are harness entrypoints.
fn registered_test_declaration(program: &ArenaProgram, id: StmtId) -> Option<FunctionDefId> {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::ProcDef(def) if program.arena.function_def(def).test_declaration => {
            Some(def)
        }
        _ => None,
    }
}

fn test_id_matches(id: &str, options: &TestOptions) -> bool {
    if !options.exact_ids.is_empty() { return options.exact_ids.iter().any(|selected| selected == id); }
    let Some(filter) = &options.filter else {
        return true;
    };
    if options.exact {
        id == filter
    } else {
        id.contains(filter)
    }
}

fn exact_set_selects_file(file: &str, options: &TestOptions) -> bool {
    options.exact_ids.iter().any(|id| id.strip_prefix(file).is_some_and(|rest| rest.is_empty() || rest.starts_with("::")))
}

/// Whether a filter can select an ID in `file`, decided before parsing it. IDs
/// are `FILE::NAME`; a name has no `:` and appears verbatim in its source.
fn test_file_may_match(file: &Path, file_name: &str, options: &TestOptions) -> bool {
    if !options.exact_ids.is_empty() {
        return exact_set_selects_file(file_name, options);
    }
    let Some(filter) = &options.filter else {
        return true;
    };
    if options.exact {
        return filter
            .strip_prefix(file_name)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"));
    }
    if file_name.contains(filter.as_str()) || file_name.contains(':') {
        return true;
    }
    match filter.split_once(':') {
        Some((before_separator, _)) => file_name.ends_with(before_separator),
        None => fs::read_to_string(file).map_or(true, |text| text.contains(filter.as_str())),
    }
}

fn test_file_matches(
    program: &ArenaProgram,
    statements: ArenaRange,
    file: &str,
    options: &TestOptions,
) -> bool {
    exact_set_selects_file(file, options) || test_id_matches(file, options)
        || program.arena.stmt_ids(statements).any(|id| {
            let kind = match program.arena.stmt(id).kind {
                ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                kind => kind,
            };
            let ArenaStmtKind::ProcDef(def) = kind else {
                return false;
            };
            let def = program.arena.function_def(def);
            (def.test_declaration || def.name.as_str().starts_with("test_"))
                && test_id_matches(&format!("{file}::{}", def.name), options)
        })
}

fn test_top_level_diagnostics(program: &ArenaProgram, statements: ArenaRange) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for id in program.arena.stmt_ids(statements) {
        if !test_top_level_allowed(program, id) {
            let span = program.arena.stmt(id).span;
            diagnostics.push(
                Diagnostic::error(
                    "test files cannot run top-level commands, mutation, or control flow",
                )
                .with_code(DiagnosticCode::CheckTestTopLevel)
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
        | ArenaStmtKind::Const { .. }
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_) => true,
        ArenaStmtKind::Export(inner) => test_top_level_allowed(program, inner),
        ArenaStmtKind::Var { .. }
        | ArenaStmtKind::Assign { .. }
        | ArenaStmtKind::CliMain(_)
        | ArenaStmtKind::SignalHook(_)
        | ArenaStmtKind::Return(_)
        | ArenaStmtKind::Yield(_)
        | ArenaStmtKind::YieldDelegate(_)
        | ArenaStmtKind::Defer(_, _)
        | ArenaStmtKind::If { .. }
        | ArenaStmtKind::While { .. }
        | ArenaStmtKind::For { .. }
        | ArenaStmtKind::With { .. }
        | ArenaStmtKind::Loop { .. }
        | ArenaStmtKind::Guard { .. }
        | ArenaStmtKind::Sugar { .. }
        | ArenaStmtKind::Assert { .. }
        | ArenaStmtKind::Break { .. }
        | ArenaStmtKind::Continue
        | ArenaStmtKind::Exit(_)
        | ArenaStmtKind::Match { .. }
        | ArenaStmtKind::Command(_)
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Expr(_) => false,
    }
}

fn native_test_signature_uses_ctx(
    program: &ArenaProgram,
    id: FunctionDefId,
) -> Result<bool, String> {
    let def = program.arena.function_def(id);
    if !Type::from_arena(&program.arena, def.return_ty).is_result_unit() {
        return Err("native test entrypoint must return Result[Unit]".to_string());
    }
    match program.arena.params(def.params) {
        [] => Ok(false),
        [param]
            if !param.rest
                && param.default.is_none()
                && param
                    .annotation
                    .is_some_and(|ty| program.arena.type_expr_named(ty, "TestContext")) =>
        {
            Ok(true)
        }
        [_] => Err("native test entrypoint parameter must be TestContext".to_string()),
        _ => Err("native test entrypoint accepts at most one TestContext parameter".to_string()),
    }
}

struct NativeTestFile {
    path: PathBuf,
    module_roots: Vec<PathBuf>,
    child_module_path: Option<Arc<OsString>>,
}

enum NativeTestDiscoveryError {
    Cancelled,
    Failure(String),
}

fn discover_native_tests(
    roots: &[PathBuf],
    configs: &ConfigCache,
    options: &TestOptions,
) -> Result<Vec<TestCase>, NativeTestDiscoveryError> {
    let mut files = Vec::new();
    for root in roots {
        if root.exists() {
            collect_xsh_files(root, configs, DiscoveryFor::Scripts, &mut files)
                .map_err(NativeTestDiscoveryError::Failure)?;
        }
    }
    files.sort_unstable();
    files.dedup();

    let mut test_files = Vec::new();
    for file in files {
        if cancellation_requested_signal().is_some() {
            return Err(NativeTestDiscoveryError::Cancelled);
        }
        let file_name = file.to_string_lossy().into_owned();
        if !test_file_may_match(&file, &file_name, options) {
            continue;
        }
        let nearest_config = configs
            .nearest_config_for_file(&file)
            .map_err(NativeTestDiscoveryError::Failure)?;
        let module_roots = FileToolConfig::new(nearest_config).module_roots();
        let child_module_path = child_module_path(&module_roots);
        test_files.push(NativeTestFile {
            path: file,
            module_roots,
            child_module_path,
        });
    }
    if test_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut reports: Vec<Option<Vec<TestCase>>> =
        (0..test_files.len()).map(|_| None).collect();
    let mut inputs = Vec::new();
    let mut entry_file_indices = Vec::new();
    for (file_index, file) in test_files.iter().enumerate() {
        if cancellation_requested_signal().is_some() {
            return Err(NativeTestDiscoveryError::Cancelled);
        }
        let file_name = file.path.to_string_lossy().into_owned();
        let bytes = match fs::read(&file.path) {
            Ok(bytes) => bytes,
            Err(err) => {
                reports[file_index] = Some(if exact_set_selects_file(&file_name, options) || test_id_matches(&file_name, options) {
                    vec![TestCase::Invalid {
                        id: file_name,
                        message: format!("failed to read test file: {err}"),
                    }]
                } else {
                    Vec::new()
                });
                continue;
            }
        };
        inputs.push(ArenaEntryInput {
            file: file_name,
            bytes,
            module_roots: file.module_roots.clone(),
        });
        entry_file_indices.push(file_index);
    }

    let workspace = parse_load_entries_shared_arena_only_with_cancellation(inputs, || {
        cancellation_requested_signal().is_some()
    })
    .map_err(|LoadCancelled| NativeTestDiscoveryError::Cancelled)?;
    let session = CheckSession::new(Arc::clone(&workspace.program), Arc::clone(&workspace.sources));
    assert_eq!(
        workspace.entries.len(),
        entry_file_indices.len(),
        "shared native-test loader returned a different entry count"
    );

    // Loading uses one immutable workspace so roots can share module records.
    // Keep checking bounded and restore sorted file order after workers finish.
    let worker_count = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(32)
        .min(workspace.entries.len());
    let next_entry = AtomicUsize::new(0);
    let mut entry_reports = std::thread::scope(|scope| {
        let (report_tx, report_rx) = crossbeam_channel::unbounded();
        for _ in 0..worker_count {
            let report_tx = report_tx.clone();
            let (workspace, session, files, entry_file_indices, next_entry, options) = (
                &workspace,
                &session,
                &test_files,
                &entry_file_indices,
                &next_entry,
                options,
            );
            std::thread::Builder::new()
                .name("xsht-test-discover".to_string())
                .stack_size(FRONTEND_WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || loop {
                    if native_cancellation_output().is_some() {
                        break;
                    }
                    let entry_index = next_entry.fetch_add(1, Ordering::Relaxed);
                    let Some(entry) = workspace.entries.get(entry_index) else {
                        break;
                    };
                    let file_index = entry_file_indices[entry_index];
                    let cases = discover_native_test_entry(
                        workspace,
                        session,
                        entry,
                        &files[file_index],
                        options,
                    );
                    if report_tx.send((file_index, cases)).is_err() {
                        break;
                    }
                })
                .expect("spawn native test discovery worker");
        }
        drop(report_tx);
        report_rx.iter().collect::<Vec<_>>()
    });
    if native_cancellation_output().is_some() {
        return Err(NativeTestDiscoveryError::Cancelled);
    }
    assert_eq!(
        entry_reports.len(),
        workspace.entries.len(),
        "native test discovery worker panicked"
    );
    entry_reports.sort_unstable_by_key(|(file_index, _)| *file_index);
    for (file_index, cases) in entry_reports {
        reports[file_index] = Some(cases);
    }

    let mut cases = Vec::new();
    for report in reports {
        cases.extend(report.expect("every native test file must produce a report"));
    }
    Ok(cases)
}

fn discover_native_test_entry(
    workspace: &xsh::frontend::load::LoadedArenaWorkspace,
    session: &CheckSession,
    entry: &xsh::frontend::load::LoadedArenaEntry,
    file: &NativeTestFile,
    options: &TestOptions,
) -> Vec<TestCase> {
    let file_name = file.path.to_string_lossy().into_owned();
    let program = workspace.program.as_ref();
    let statements = entry.statements;
    let mut cases = Vec::new();
    if !entry.diagnostics.is_empty() {
        if test_file_matches(program, statements, &file_name, options) {
            cases.push(TestCase::Invalid {
                id: file_name,
                message: DiagnosticRenderer::new()
                    .render(&entry.diagnostics, &workspace.sources),
            });
        }
        return cases;
    }

    let mut top_level_errors = test_top_level_diagnostics(program, statements);
    for stmt_id in program.arena.stmt_ids(statements) {
        let kind = program.arena.stmt(stmt_id).kind;
        let def_id = match kind {
            ArenaStmtKind::ProcDef(def) => Some(def),
            ArenaStmtKind::Export(inner) => match program.arena.stmt(inner).kind {
                ArenaStmtKind::ProcDef(def) => Some(def),
                _ => None,
            },
            _ => None,
        };
        if let Some(def_id) = def_id {
            let def = program.arena.function_def(def_id);
            if !def.test_declaration
                && def.name.as_str().starts_with("test_")
                && native_test_signature_uses_ctx(program, def_id).is_ok()
            {
                top_level_errors.push(
                    Diagnostic::error("legacy native test proc requires migration: replace its signature with `test NAME { |ctx| ... }`; extract callable shared work into an ordinary helper")
                        .with_code(DiagnosticCode::CheckLegacyTestProc)
                        .with_label(Label::primary(program.arena.stmt(stmt_id).span, "keep the exact declared name to preserve the test ID")),
                );
            }
        }
    }

    if !top_level_errors.is_empty() {
        if test_file_matches(program, statements, &file_name, options) {
            cases.push(TestCase::Invalid {
                id: file_name,
                message: DiagnosticRenderer::new()
                    .render(&top_level_errors, &workspace.sources),
            });
        }
        return cases;
    }

    let view = session.view_for_root(statements, entry.source_id);
    let checked = session.check_view(&view, CheckOptions::for_execution());
    if !checked.diagnostics.is_empty() {
        if test_file_matches(view.program(), statements, &file_name, options) {
            cases.push(TestCase::Invalid {
                id: file_name,
                message: DiagnosticRenderer::new()
                    .render(&checked.diagnostics, &workspace.sources),
            });
        }
        return cases;
    }

    let program = view.program();
    let mut matching_tests = Vec::new();
    for stmt_id in view.statement_ids() {
        let Some(def_id) = registered_test_declaration(program, stmt_id) else {
            continue;
        };
        let name = program.arena.function_def(def_id).name;
        let id = format!("{file_name}::{name}");
        if test_id_matches(&id, options) {
            matching_tests.push((id, name.to_string(), def_id));
        }
    }
    if matching_tests.is_empty() {
        return cases;
    }

    let prepared = match Evaluator::new_with_shared_sources(
        Vec::new(),
        Arc::clone(&workspace.sources),
    )
    .with_module_roots(file.module_roots.clone())
    .with_native_test_host({
        let module_path = file.child_module_path.clone();
        Arc::new(move |request| native_test_host(request, module_path.as_deref()))
    })
    .prepare_test_program_for_view(
        &view,
        entry.source_id,
        Checker::compact_declarations(program, checked),
    ) {
        Ok(prepared) => Arc::new(prepared),
        Err(diagnostic) => {
            let message =
                DiagnosticRenderer::new().render(&[diagnostic], &workspace.sources);
            if test_id_matches(&file_name, options) {
                cases.push(TestCase::Invalid {
                    id: file_name,
                    message,
                });
            } else {
                for (id, _, _) in matching_tests {
                    cases.push(TestCase::Invalid {
                        id,
                        message: message.clone(),
                    });
                }
            }
            return cases;
        }
    };
    for (id, name, def_id) in matching_tests {
        match native_test_signature_uses_ctx(program, def_id) {
            Ok(has_ctx) => cases.push(TestCase::Native(NativeTestCase {
                id,
                file: file_name.clone(),
                name,
                prepared: Arc::clone(&prepared),
                has_ctx,
                module_path: file.child_module_path.clone(),
            })),
            Err(message) => cases.push(TestCase::Invalid { id, message }),
        }
    }
    cases
}

fn run_test_case(
    case: TestCase,
    index: usize,
    run_id: &str,
    options: &TestOptions,
    route_witness: Option<&RouteWitnessConfig>,
    stopped: &Arc<AtomicBool>,
) -> TestOutcome {
    let started = Instant::now();
    let mut outcome = match case {
        TestCase::Native(case) => {
            run_native_test_with_timeout(case, index, run_id, options, route_witness, stopped)
        }
        TestCase::Invalid { message, .. } => TestOutcome {
            kind: TestOutcomeKind::Failed(message),
            duration: Duration::ZERO,
            stdout: Vec::new(),
            stderr: Vec::new(),
            coverage_trace: None,
            cleanup_error: None,
        },
    };
    outcome.duration = started.elapsed();
    outcome
}

const TIMED_OUT_TEST_GRACE: Duration = Duration::from_secs(5);
const FORCED_TEST_GRACE: Duration = Duration::from_secs(1);
const TEST_TIMEOUT_POLL: Duration = Duration::from_millis(20);

enum TestInterruption { Timeout(Duration), Cancelled }

fn run_native_test_with_timeout(
    case: NativeTestCase, index: usize, run_id: &str, options: &TestOptions,
    route_witness: Option<&RouteWitnessConfig>, stopped: &Arc<AtomicBool>,
) -> TestOutcome {
    let route_witness_tempdir = match route_witness.filter(|config| config.test_id == case.id) {
        Some(config) => match TempDir::new_in(&config.output_dir) {
            Ok(tempdir) => Some(tempdir),
            Err(error) => return test_failure(format!("failed to create route-witness invocation directory: {error}")),
        },
        None => None,
    };
    let route_witness_output_dir = route_witness_tempdir.as_ref().map(|tempdir| tempdir.path().to_path_buf());
    let cancellation = TestCancellation::default();
    let (tx, rx) = mpsc::sync_channel(1);
    let spawned = {
        let cancellation = cancellation.clone();
        let run_id = run_id.to_owned();
        let options = options.clone();
        let route_witness_output_dir = route_witness_output_dir.clone();
        std::thread::Builder::new().name(format!("xsht-test-{index}")).spawn(move || {
            let outcome = run_native_test(case, index, &run_id, &options, route_witness_output_dir.as_deref(), cancellation);
            let _ = tx.send(native_test_outcome_for_publication(outcome, cancellation_requested_signal()));
        })
    };
    let handle = match spawned { Ok(handle) => handle, Err(error) => return test_failure(format!("failed to start test thread: {error}")) };
    let outcome = wait_native_test(handle, rx, cancellation, options.timeout, stopped);
    finish_route_witness(outcome, route_witness_tempdir)
}

fn native_test_outcome_for_publication(mut outcome: TestOutcome, signal: Option<i32>) -> TestOutcome {
    if signal.is_some() {
        let detail = match outcome.kind {
            TestOutcomeKind::Failed(detail) | TestOutcomeKind::Cancelled(detail) => detail,
            _ => String::new(),
        };
        outcome.kind = TestOutcomeKind::Cancelled(detail);
        outcome.coverage_trace = None;
    }
    outcome
}

fn wait_native_test(
    handle: std::thread::JoinHandle<()>, rx: mpsc::Receiver<TestOutcome>, cancellation: TestCancellation,
    timeout: Option<Duration>, stopped: &AtomicBool,
) -> TestOutcome {
    let started = Instant::now();
    let interrupted = loop {
        match rx.try_recv() {
            Ok(outcome) => {
                if let Err(panic) = handle.join() { return test_failure(format!("native test thread panicked: {}", test_panic_message(panic))); }
                return outcome;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                let detail = match handle.join() { Ok(()) => "test thread exited without a result".to_owned(), Err(panic) => format!("test thread panicked: {}", test_panic_message(panic)) };
                return test_failure(detail);
            }
        }
        if cancellation_escalated_signal().is_some() { unsafe { libc::_exit(130) }; }
        if stopped.load(Ordering::Acquire) || cancellation_requested_signal().is_some() { break TestInterruption::Cancelled; }
        let remaining = timeout.map(|run_limit| cancellation.requested_timeout().unwrap_or(run_limit).saturating_sub(started.elapsed()));
        if remaining.is_some_and(|remaining| remaining.is_zero()) {
            break TestInterruption::Timeout(cancellation.requested_timeout().unwrap_or(timeout.unwrap()));
        }
        match rx.recv_timeout(remaining.map_or(TEST_TIMEOUT_POLL, |remaining| remaining.min(TEST_TIMEOUT_POLL))) {
            Ok(outcome) => {
                if let Err(panic) = handle.join() { return test_failure(format!("native test thread panicked: {}", test_panic_message(panic))); }
                return outcome;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let detail = match handle.join() { Ok(()) => "test thread exited without a result".to_owned(), Err(panic) => format!("test thread panicked: {}", test_panic_message(panic)) };
                return test_failure(detail);
            }
        }
    };
    cancellation.cancel();
    stop_child_groups_owned_by(cancellation.id());
    let unwound = rx.recv_timeout(TIMED_OUT_TEST_GRACE).ok().or_else(|| {
        cancellation.force();
        stop_child_groups_owned_by(cancellation.id());
        rx.recv_timeout(FORCED_TEST_GRACE).ok()
    });
    stop_child_groups_owned_by(cancellation.id());
    let returned = unwound.is_some();
    let mut outcome = match unwound {
        Some(outcome) => outcome,
        None => test_failure("test did not produce a result during bounded cancellation".into()),
    };
    if returned || handle.is_finished() {
        if let Err(panic) = handle.join() { outcome.cleanup_error = Some(format!("test thread panicked during cleanup: {}", test_panic_message(panic))); }
    } else {
        outcome.cleanup_error = Some(format!("native test thread remains running after {} of cancellation; process shutdown is required", format_test_duration(TIMED_OUT_TEST_GRACE + FORCED_TEST_GRACE)));
        drop(handle);
    }
    let detail = match outcome.kind { TestOutcomeKind::Failed(detail) | TestOutcomeKind::Cancelled(detail) => detail, _ => String::new() };
    outcome.kind = match interrupted {
        TestInterruption::Timeout(limit) => TestOutcomeKind::TimedOut { limit, detail },
        TestInterruption::Cancelled => TestOutcomeKind::Cancelled(detail),
    };
    outcome.coverage_trace = None;
    outcome
}

fn test_failure(message: String) -> TestOutcome {
    TestOutcome { kind: TestOutcomeKind::Failed(message), duration: Duration::ZERO, stdout: Vec::new(), stderr: Vec::new(), coverage_trace: None, cleanup_error: None }
}

fn finish_route_witness(
    outcome: TestOutcome,
    route_witness_tempdir: Option<TempDir>,
) -> TestOutcome {
    if matches!(&outcome.kind, TestOutcomeKind::Passed)
        && let Some(tempdir) = route_witness_tempdir
    {
        let _ = tempdir.keep();
    }
    outcome
}

fn run_native_test(
    case: NativeTestCase,
    index: usize,
    run_id: &str,
    options: &TestOptions,
    route_witness_output_dir: Option<&Path>,
    cancellation: TestCancellation,
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
            cleanup_error: None,
        };
    }
    if let Err(error) = clear_inherited_setgid(&temp_root) {
        let cleanup = remove_native_test_directory(&temp_root)
            .err()
            .map_or_else(String::new, |cleanup| format!("; cleanup failed: {cleanup}"));
        return test_failure(format!(
            "failed to normalize native test directory permissions for '{}': {error}{cleanup}",
            temp_root.display()
        ));
    }

    let mut outcome = run_native_test_in_directory(case, options, route_witness_output_dir, cancellation, &temp_root);
    if !options.keep_temp
        && let Err(error) = remove_native_test_directory(&temp_root)
    {
        outcome.cleanup_error = Some(format!("cannot remove native test directory {}: {error}", temp_root.display()));
    }
    outcome
}

fn remove_native_test_directory(root: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(root) {
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            remove_native_test_directory_by_handle(root)
        }
        Err(error) if error.raw_os_error() == Some(libc::ENAMETOOLONG) => {
            remove_native_test_directory_by_handle(root)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn clear_inherited_setgid(directory: &Path) -> std::io::Result<()> {
    const SETGID: u32 = 0o2000;
    let metadata = rustix::fs::statat(CWD, directory, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(std::io::Error::from)?;
    if rustix::fs::FileType::from_raw_mode(metadata.st_mode as _) != rustix::fs::FileType::Directory {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "native test root is not a directory",
        ));
    }
    let mode = metadata.st_mode as u32 & 0o7777;
    if mode & SETGID != 0 {
        set_native_directory_mode_at(CWD, directory.as_os_str(), Mode::from_raw_mode(mode & !SETGID))?;
    }
    Ok(())
}

fn remove_native_test_directory_by_handle(root: &Path) -> std::io::Result<()> {
    let parent_path = root.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "native test directory has no parent",
        )
    })?;
    let name = root.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "native test directory has no name",
        )
    })?;
    let parent = fs::File::open(parent_path)?;
    let root_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let root_fd = match rustix::fs::openat(&parent, name, root_flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(error) if error == Errno::NOENT => return Ok(()),
        Err(error) if error == Errno::ACCESS || error == Errno::PERM => {
            restore_native_directory_access_at(parent.as_fd(), name)?;
            rustix::fs::openat(&parent, name, root_flags, Mode::empty())
                .map_err(std::io::Error::from)?
        }
        Err(error) => return Err(error.into()),
    };
    restore_native_directory_access_at(parent.as_fd(), name)?;
    let mut root_dir = Dir::new(root_fd)?;
    remove_native_directory_entries(&mut root_dir)?;
    match rustix::fs::unlinkat(&parent, name, AtFlags::REMOVEDIR) {
        Err(error) if error == Errno::NOENT => Ok(()),
        result => result.map_err(Into::into),
    }
}

fn remove_native_directory_entries(directory: &mut Dir) -> std::io::Result<()> {
    while let Some(entry) = directory.read() {
        let entry = entry?;
        let name = OsStr::from_bytes(entry.file_name().to_bytes());
        if name == OsStr::new(".") || name == OsStr::new("..") {
            continue;
        }
        let parent = directory.fd()?;
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let child = match rustix::fs::openat(parent, name, flags, Mode::empty()) {
            Ok(child) => child,
            Err(error) if error == Errno::NOENT => continue,
            Err(error) if error == Errno::NOTDIR || error == Errno::LOOP => {
                match rustix::fs::unlinkat(directory.fd()?, name, AtFlags::empty()) {
                    Err(error) if error == Errno::NOENT => {}
                    result => result?,
                }
                continue;
            }
            Err(error) if error == Errno::ACCESS || error == Errno::PERM => {
                restore_native_directory_access_at(parent, name)?;
                rustix::fs::openat(parent, name, flags, Mode::empty())
                    .map_err(std::io::Error::from)?
            }
            Err(error) => return Err(error.into()),
        };
        restore_native_directory_access_at(parent, name)?;
        let mut child = Dir::new(child)?;
        remove_native_directory_entries(&mut child)?;
        rustix::fs::unlinkat(directory.fd()?, name, AtFlags::REMOVEDIR)?;
    }
    Ok(())
}

fn restore_native_directory_access_at(
    parent: rustix::fd::BorrowedFd<'_>,
    name: &OsStr,
) -> std::io::Result<()> {
    const OWNER_ACCESS: u32 = 0o700;
    let metadata = rustix::fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(std::io::Error::from)?;
    if rustix::fs::FileType::from_raw_mode(metadata.st_mode as _) != rustix::fs::FileType::Directory {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "native test cleanup entry is not a directory",
        ));
    }
    let mode = metadata.st_mode as u32 & 0o7777;
    if mode & OWNER_ACCESS == OWNER_ACCESS {
        return Ok(());
    }
    set_native_directory_mode_at(parent, name, Mode::from_raw_mode(mode | OWNER_ACCESS))
}

fn set_native_directory_mode_at(
    parent: impl AsFd,
    name: &OsStr,
    mode: Mode,
) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let pinned = rustix::fs::openat(
            parent,
            name,
            OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        if rustix::fs::FileType::from_raw_mode(rustix::fs::fstat(&pinned).map_err(std::io::Error::from)?.st_mode as _)
            != rustix::fs::FileType::Directory
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "native test directory changed type",
            ));
        }
        rustix::fs::chmod(
            format!("/proc/self/fd/{}", pinned.as_raw_fd()),
            mode,
        )
        .map_err(std::io::Error::from)
    }
    #[cfg(not(target_os = "linux"))]
    {
        rustix::fs::chmodat(parent, name, mode, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(std::io::Error::from)
    }
}

fn run_native_test_in_directory(
    case: NativeTestCase, options: &TestOptions,
    route_witness_output_dir: Option<&Path>, cancellation: TestCancellation, temp_root: &Path,
) -> TestOutcome {
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
                    cleanup_error: None,
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
    if let Some(module_path) = &case.module_path {
        env_overlay.push((
            XSH_MODULE_PATH.as_bytes().to_vec(),
            module_path.as_bytes().to_vec(),
        ));
    }
    if let Some(dir) = &nested_coverage_dir {
        env_overlay.push((XSH_COVERAGE_TRACE_DIR.as_bytes().to_vec(), path_bytes(dir)));
    }
    if let Some(output_dir) = route_witness_output_dir {
        env_overlay.push((
            xsh::execution::evaluator::NATIVE_TEST_ROUTE_WITNESS_TEST_ID_OVERLAY.to_vec(),
            case.id.as_bytes().to_vec(),
        ));
        env_overlay.push((
            xsh::execution::evaluator::NATIVE_TEST_ROUTE_WITNESS_OUTPUT_DIR_OVERLAY.to_vec(),
            path_bytes(output_dir),
        ));
    }
    let evaluated = case.prepared.eval_test(
        &case.name,
        ctx,
        options.collect_coverage(),
        options.force_frames,
        env_overlay,
        cancellation,
    );

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
        cleanup_error: None,
    }
}

impl TestOutcomeKind {
    fn reason(&self) -> xsh::host::json::Value {
        match self {
            Self::Passed => xsh::host::json::Value::null(),
            Self::Failed(detail) => json_fields([("kind", json_string("test_failure")), ("detail", json_string(detail))]),
            Self::Skipped(detail) => json_fields([("kind", json_string("test_skip")), ("detail", json_string(detail))]),
            Self::Cancelled(detail) => json_fields([("kind", json_string("cancellation")), ("detail", json_string(detail))]),
            Self::TimedOut { limit, detail } => json_fields([("kind", json_string("timeout")), ("limit", json_string(format_test_duration(*limit))), ("detail", json_string(detail))]),
        }
    }
}

fn native_test_host(
    request: NativeTestRunRequest,
    module_path: Option<&OsString>,
) -> Result<Value, RuntimeError> {
    if request.env.contains_key(NATIVE_TEST_FORCE_FRAMES_ENV) {
        return Err(RuntimeError::new(
            native_test_error_kind(request.kind),
            "the frame-scheduler test marker is reserved for the test harness",
        )
        .with_span(request.span));
    }
    let script_path = PathBuf::from(OsString::from_vec(request.script_path.bytes.clone()));
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RuntimeError::new(native_test_error_kind(request.kind), error.to_string())
                .with_span(request.span)
        })?;
    }
    fs::write(&script_path, request.source.as_bytes()).map_err(|error| {
        RuntimeError::new(native_test_error_kind(request.kind), error.to_string())
            .with_span(request.span)
    })?;

    let faked = request.linux_fake.is_some() || request.unix_fake.is_some();
    let mut command = match (request.kind, faked) {
        (NativeTestRunKind::Xsh, true) => {
            if !request.tool_args.iter().all(|arg| arg == "--") {
                return Err(RuntimeError::new(
                    native_test_error_kind(request.kind),
                    "xsh options are not supported while a test fake is installed",
                )
                .with_span(request.span));
            }
            let mut command = Command::new(test_binary("xsht"));
            command.arg(crate::xsht::app::TEST_FAKE_RUN);
            if let Some(fake) = &request.linux_fake {
                command.arg("--fake").arg("linux");
                for (key, value) in fake.settings() {
                    command.arg("--fake").arg(format!("linux.{key}={value}"));
                }
            }
            if let Some(fake) = &request.unix_fake {
                command.arg("--fake").arg("unix");
                for (key, value) in fake.settings() {
                    command.arg("--fake").arg(format!("unix.{key}={value}"));
                }
            }
            command.arg(&script_path);
            command
        }
        (NativeTestRunKind::XshtTrace, true) => {
            return Err(RuntimeError::new(
                native_test_error_kind(request.kind),
                "test fakes do not cover `xsht trace` runs",
            )
            .with_span(request.span));
        }
        (NativeTestRunKind::Xsh, false) => {
            let mut command = Command::new(test_binary("xsh"));
            command.args(&request.tool_args);
            command.arg(&script_path);
            command
        }
        (NativeTestRunKind::XshtTrace, false) => {
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
    // Fixture arguments are already script data. Protect a leading `--`
    // from the host CLI's optional compatibility separator.
    if faked
        || request.kind != NativeTestRunKind::Xsh
        || request.tool_args.last().is_none_or(|arg| arg != "--")
    {
        command.arg("--");
    }
    command.args(&request.script_args);
    if let Some(module_path) = module_path {
        command.env(XSH_MODULE_PATH, module_path);
    }
    for (name, value) in &request.env {
        let value: &std::ffi::OsStr = std::os::unix::ffi::OsStrExt::from_bytes(value);
        command.env(name, value);
    }
    let route_witness_harness_prefix =
        xsh::execution::evaluator::NATIVE_TEST_ROUTE_WITNESS_HARNESS_ENV_PREFIX;
    command.env_remove(format!("{route_witness_harness_prefix}_TEST_ID"));
    command.env_remove(format!("{route_witness_harness_prefix}_OUTPUT_DIR"));
    if request.force_frames {
        command.env(NATIVE_TEST_FORCE_FRAMES_ENV, "1");
    } else {
        command.env_remove(NATIVE_TEST_FORCE_FRAMES_ENV);
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if request.stdin.is_empty() {
        command.stdin(Stdio::null());
    } else {
        command.stdin(Stdio::piped());
    }

    let has_route_witness_transport = request
        .attach_route_witness_transport(&mut command)
        .map_err(|error| {
            RuntimeError::new(
                native_test_error_kind(request.kind),
                format!("failed to attach route-witness descriptor: {error}"),
            )
            .with_span(request.span)
        })?;
    if has_route_witness_transport && (request.kind != NativeTestRunKind::Xsh || faked) {
        return Err(RuntimeError::new(
            native_test_error_kind(request.kind),
            "route-witness descriptor is only valid for a direct xsh child without test fakes",
        )
        .with_span(request.span));
    }

    // Each child leads a runner-owned process group so cancellation can stop
    // it and every descendant that stays in its group.
    command.process_group(0);

    let mut child = command.spawn().map_err(|error| {
        RuntimeError::new(native_test_error_kind(request.kind), error.to_string())
            .with_span(request.span)
    })?;
    let _group = ChildGroupRegistration::new(
        child.id() as libc::pid_t,
        request
            .cancellation
            .as_ref()
            .map_or(0, TestCancellation::id),
    );
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

const CHILD_GROUP_SLOTS: usize = 256;
const CHILD_GROUP_GRACE: Duration = Duration::from_secs(1);
const CHILD_GROUP_POLL: Duration = Duration::from_millis(10);
/// Process groups of live test subprocesses. Slots are atomics so the signal
/// handler can read them without locking.
static CHILD_GROUPS: [AtomicI32; CHILD_GROUP_SLOTS] =
    [const { AtomicI32::new(0) }; CHILD_GROUP_SLOTS];
/// `TestCancellation::id` of the test that started each `CHILD_GROUPS` entry.
static CHILD_GROUP_OWNERS: [AtomicUsize; CHILD_GROUP_SLOTS] =
    [const { AtomicUsize::new(0) }; CHILD_GROUP_SLOTS];
static PREVIOUS_SIGNAL_HANDLER: AtomicUsize = AtomicUsize::new(libc::SIG_DFL);

struct ChildGroupRegistration {
    slot: Option<usize>,
}

impl ChildGroupRegistration {
    fn new(pgid: libc::pid_t, owner: usize) -> Self {
        let slot = CHILD_GROUPS.iter().position(|slot| {
            slot.compare_exchange(0, pgid, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        });
        if let Some(slot) = slot {
            CHILD_GROUP_OWNERS[slot].store(owner, Ordering::SeqCst);
        }
        Self { slot }
    }
}

impl Drop for ChildGroupRegistration {
    fn drop(&mut self) {
        if let Some(slot) = self.slot {
            CHILD_GROUP_OWNERS[slot].store(0, Ordering::SeqCst);
            CHILD_GROUPS[slot].store(0, Ordering::SeqCst);
        }
    }
}

/// Signals the registered groups, or only those `owner` started when it is
/// nonzero. Async-signal-safe: uses only atomics and `killpg`.
fn signal_child_groups(signal: i32, owner: usize) -> bool {
    let mut alive = false;
    for (slot, slot_owner) in CHILD_GROUPS.iter().zip(&CHILD_GROUP_OWNERS) {
        let pgid = slot.load(Ordering::SeqCst);
        if pgid > 0
            && (owner == 0 || slot_owner.load(Ordering::SeqCst) == owner)
            && unsafe { libc::killpg(pgid, signal) } == 0
        {
            alive = true;
        }
    }
    alive
}

/// Asks test subprocess groups to stop so `xsh` children can forward the
/// signal to process groups they created, then kills whatever remains.
/// Async-signal-safe: uses only atomics, `killpg`, and `nanosleep`.
fn stop_child_groups(signal: i32, min_grace: Duration, owner: usize) {
    let mut waited = Duration::ZERO;
    let mut alive = signal_child_groups(signal, owner);
    while (alive || waited < min_grace) && waited < CHILD_GROUP_GRACE {
        let pause = libc::timespec {
            tv_sec: 0,
            tv_nsec: CHILD_GROUP_POLL.as_nanos() as libc::c_long,
        };
        unsafe { libc::nanosleep(&pause, std::ptr::null_mut()) };
        waited += CHILD_GROUP_POLL;
        alive = signal_child_groups(0, owner);
    }
    signal_child_groups(libc::SIGKILL, owner);
}

fn terminate_child_groups() {
    stop_child_groups(libc::SIGTERM, Duration::ZERO, 0);
}

fn stop_child_groups_owned_by(owner: usize) {
    stop_child_groups(libc::SIGTERM, Duration::ZERO, owner);
}

extern "C" fn handle_test_cancellation_signal(signal: i32) {
    let previous = PREVIOUS_SIGNAL_HANDLER.load(Ordering::SeqCst);
    if previous != libc::SIG_DFL && previous != libc::SIG_IGN {
        let previous: extern "C" fn(i32) = unsafe { std::mem::transmute(previous) };
        previous(signal);
    }
    signal_child_groups(libc::SIGTERM, 0);
    if cancellation_escalated_signal().is_some() {
        signal_child_groups(libc::SIGKILL, 0);
        unsafe { libc::_exit(130) };
    }
}

/// Installs native-test cancellation handlers. The first signal requests
/// cleanup; a subsequent signal aborts without completing the event stream.
pub(crate) fn install_test_cancellation_signal_handlers() -> std::io::Result<()> {
    for signal in [libc::SIGINT, libc::SIGTERM] {
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = handle_test_cancellation_signal as *const () as usize;
        action.sa_flags = 0;
        let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaddset(&mut action.sa_mask, libc::SIGINT);
            libc::sigaddset(&mut action.sa_mask, libc::SIGTERM);
            if libc::sigaction(signal, &action, &mut previous) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        if previous.sa_sigaction != handle_test_cancellation_signal as *const () as usize {
            PREVIOUS_SIGNAL_HANDLER.store(previous.sa_sigaction, Ordering::SeqCst);
        }
    }
    Ok(())
}

fn native_test_error_kind(kind: NativeTestRunKind) -> &'static str {
    match kind {
        NativeTestRunKind::Xsh => "test-run-xsh",
        NativeTestRunKind::XshtTrace => "test-run-xsht-trace",
    }
}

/// A binary that native tests spawn: the path a Cargo test passes in, else the
/// sibling of the running `xsht`, so children share the runner's build profile.
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
    PathBuf::from(name)
}

const XSH_MODULE_PATH: &str = "XSH_MODULE_PATH";

/// Search path for `xsh` children of tests: the inherited `XSH_MODULE_PATH`
/// followed by the configured module roots as absolute paths, deduplicated.
/// The runner's own environment is unchanged, so lint discovery still ignores
/// these roots.
fn child_module_path(module_roots: &[PathBuf]) -> Option<Arc<OsString>> {
    let mut entries: Vec<PathBuf> = std::env::var_os(XSH_MODULE_PATH)
        .map(|inherited| std::env::split_paths(&inherited).collect())
        .unwrap_or_default();
    for root in module_roots {
        let root: PathBuf = absolute_path(root).components().collect();
        if !entries.contains(&root) {
            entries.push(root);
        }
    }
    if entries.is_empty() {
        return None;
    }
    std::env::join_paths(entries).ok().map(Arc::new)
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
            "FAILED" | "TIMEOUT" => "\x1b[31m",
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
            "test declaration returned {}, expected Result[Unit]",
            value.type_name()
        )),
        None => TestOutcomeKind::Failed("test declaration did not return a value".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Duration, ParallelTestQueue, TestCase, default_test_jobs_for, format_test_duration,
    };

    fn invalid_case(id: &str) -> TestCase {
        TestCase::Invalid {
            id: id.to_string(),
            message: String::new(),
        }
    }

    fn runner_options() -> super::TestOptions {
        super::TestOptions { filter: None, list: false, exact: false, exact_ids: Vec::new(), events_jsonl: None,
            nocapture: false, fail_fast: false, keep_temp: false, jobs: Some(2), timeout: None,
            coverage: false, api: false, coverage_json_out: None, force_frames: false }
    }

    #[test]
    fn test_ledger_rejects_missing_duplicate_and_out_of_order_results() {
        let mut ledger = super::TestRunLedger::new("owned-run", ["a".to_owned(), "b".to_owned()], None).unwrap();
        let outcome = super::test_failure("actual assertion failure".into());
        assert!(ledger.final_summary(super::RunCompletion::Success, Duration::ZERO).is_err());
        assert!(ledger.finish("a", &outcome).is_err());
        assert!(ledger.start("unknown").is_err());
        ledger.start("a").unwrap();
        assert!(ledger.start("a").is_err());
        ledger.finish("a", &outcome).unwrap();
        assert!(ledger.finish("a", &outcome).is_err());
        ledger.settle("fail-fast").unwrap();
        assert!(matches!(ledger.cases["b"], super::CaseLifecycle::Finished(super::TestTerminal::NotExecuted)));
        assert!(ledger.final_summary(super::RunCompletion::Success, Duration::ZERO).is_err());
        ledger.final_summary(super::RunCompletion::CaseFailure, Duration::ZERO).unwrap();
        assert!(ledger.final_summary(super::RunCompletion::Success, Duration::ZERO).is_err());
        assert!(ledger.start("b").is_err());
    }

    #[test]
    fn test_ledger_rejects_success_with_unsuccessful_terminal_outcomes() {
        for terminal in [super::TestTerminal::Failed, super::TestTerminal::TimedOut,
            super::TestTerminal::Cancelled, super::TestTerminal::NotExecuted] {
            let mut ledger = super::TestRunLedger::new("contradictory-summary", ["case".to_owned()], None).unwrap();
            if terminal != super::TestTerminal::NotExecuted { ledger.start("case").unwrap(); }
            ledger.finish_case("case", terminal, Duration::ZERO, xsh::host::json::Value::null()).unwrap();
            assert!(ledger.final_summary(super::RunCompletion::Success, Duration::ZERO).is_err(), "{terminal:?}");
            let completion = match terminal {
                super::TestTerminal::Failed | super::TestTerminal::TimedOut => super::RunCompletion::CaseFailure,
                super::TestTerminal::Cancelled => super::RunCompletion::Cancelled,
                _ => super::RunCompletion::RunnerFailure("not admitted".into()),
            };
            ledger.final_summary(completion, Duration::ZERO).unwrap();
        }
        let mut ledger = super::TestRunLedger::new("legacy-skip", ["case".to_owned()], None).unwrap();
        ledger.start("case").unwrap();
        ledger.finish_case("case", super::TestTerminal::Skipped, Duration::ZERO, xsh::host::json::Value::null()).unwrap();
        assert!(ledger.final_summary(super::RunCompletion::CaseFailure, Duration::ZERO).is_err());
        ledger.final_summary(super::RunCompletion::Success, Duration::ZERO).unwrap();
    }

    #[test]
    fn native_run_completion_keeps_late_cancellation_and_required_failures() {
        let coverage = super::RunCompletion::select(false, None, None, Some("cannot write coverage"), false, true);
        assert_eq!(coverage.status(), 1);
        let mut ledger = super::TestRunLedger::new("coverage-only", ["case".to_owned()], None).unwrap();
        ledger.start("case").unwrap();
        ledger.finish_case("case", super::TestTerminal::Passed, Duration::ZERO, xsh::host::json::Value::null()).unwrap();
        ledger.final_summary(coverage, Duration::ZERO).unwrap();
        assert!(matches!(super::RunCompletion::select(true, Some("cleanup"), None, Some("coverage"), true, true), super::RunCompletion::Cancelled));
        assert!(matches!(super::RunCompletion::select(false, None, None, None, true, true), super::RunCompletion::CaseFailure));
        let output = super::CliOutput { status: 143, stdout: vec![1], stderr: vec![2], trace_text: String::new(), syscall_summary: None };
        let normalized = super::normalize_native_cancellation(output);
        assert_eq!(normalized.status, 130);
        assert_eq!(normalized.stdout, [1]);
        assert_eq!(normalized.stderr, [2]);
    }

    #[test]
    fn event_write_failure_cannot_publish_a_successful_final_summary() {
        let (writer, reader) = std::os::unix::net::UnixStream::pair().unwrap();
        let fd: std::os::fd::OwnedFd = writer.into();
        let mut ledger = super::TestRunLedger::new("broken-output", ["case".to_owned()], None).unwrap();
        ledger.output = super::EventOutput::Open(std::fs::File::from(fd));
        drop(reader);
        assert!(ledger.start("case").is_err());
        assert!(matches!(ledger.output, super::EventOutput::Failed(_)));
        assert!(ledger.settle("event output failed").is_err());
        assert!(ledger.final_summary(super::RunCompletion::Success, Duration::ZERO).is_err());
    }

    #[test]
    fn parallel_runner_drains_fast_results_before_releasing_its_workers() {
        let cases = ["a::one", "a::two", "b::three"].map(invalid_case).into_iter().collect();
        let mut ledger = super::TestRunLedger::new("fast-workers", ["a::one", "a::two", "b::three"].map(str::to_owned), None).unwrap();
        let mut finished = 0;
        super::run_test_cases(cases, "fast-workers", &runner_options(), None, |event| {
            match event {
                super::TestRunEvent::Started(id) => ledger.start(&id).unwrap(),
                super::TestRunEvent::Finished(id, outcome) => { ledger.finish(&id, &outcome).unwrap(); finished += 1; }
            }
            true
        }).unwrap();
        assert_eq!(finished, 3);
        ledger.final_summary(super::RunCompletion::CaseFailure, Duration::ZERO).unwrap();
    }

    #[test]
    fn completed_native_result_precedes_a_later_run_stop() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let mut outcome = super::test_failure(String::new());
            outcome.kind = super::TestOutcomeKind::Passed;
            tx.send(outcome).unwrap();
            finished_tx.send(()).unwrap();
        });
        finished_rx.recv().unwrap();
        let stopped = std::sync::atomic::AtomicBool::new(true);
        let outcome = super::wait_native_test(worker, rx, xsh::execution::evaluator::TestCancellation::default(), None, &stopped);
        assert!(matches!(outcome.kind, super::TestOutcomeKind::Passed));
        assert!(outcome.cleanup_error.is_none());
    }

    #[test]
    fn native_child_failure_publication_retains_cancellation_origin() {
        for signal in [Some(libc::SIGTERM), Some(libc::SIGINT), None] {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
            let worker = std::thread::spawn(move || {
                let outcome = super::test_failure("owned child terminated".into());
                tx.send(super::native_test_outcome_for_publication(outcome, signal)).unwrap();
                finished_tx.send(()).unwrap();
            });
            finished_rx.recv().unwrap();
            let stopped = std::sync::atomic::AtomicBool::new(true);
            let outcome = super::wait_native_test(worker, rx, xsh::execution::evaluator::TestCancellation::default(), None, &stopped);
            let expected = if signal.is_some() { super::TestTerminal::Cancelled } else { super::TestTerminal::Failed };
            let mut ledger = super::TestRunLedger::new("publication", ["case".to_owned()], None).unwrap();
            ledger.start("case").unwrap();
            ledger.finish("case", &outcome).unwrap();
            assert!(matches!(ledger.cases["case"], super::CaseLifecycle::Finished(actual) if actual == expected));
            assert!(outcome.cleanup_error.is_none());
        }
    }

    #[test]
    fn native_test_directory_removal_repairs_restricted_modes_without_following_links() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        let sandbox = tempfile::TempDir::new_in(scratch).unwrap();
        let outside = sandbox.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("kept"), "kept").unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o500)).unwrap();
        let root = sandbox.path().join("root");
        let nested = root.join("read-only").join("sealed");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("file"), "contents").unwrap();
        std::fs::write(root.join("read-only").join("file"), "contents").unwrap();
        symlink(&outside, root.join("read-only").join("link")).unwrap();
        std::fs::set_permissions(&nested, std::fs::Permissions::from_mode(0o000)).unwrap();
        std::fs::set_permissions(root.join("read-only"), std::fs::Permissions::from_mode(0o550)).unwrap();
        super::remove_native_test_directory(&root).unwrap();
        assert!(!root.exists());
        assert_eq!(std::fs::metadata(&outside).unwrap().permissions().mode() & 0o7777, 0o500);
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(std::fs::read_to_string(outside.join("kept")).unwrap(), "kept");
    }

    #[test]
    fn native_test_directory_removal_handles_paths_longer_than_path_max() {
        use std::os::unix::ffi::OsStringExt;
        use std::os::unix::fs::{PermissionsExt, symlink};
        let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        let sandbox = tempfile::TempDir::new_in(scratch).unwrap();
        let sandbox_dir = std::fs::File::open(sandbox.path()).unwrap();
        let outside = sandbox.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("kept"), "kept").unwrap();
        rustix::fs::mkdirat(&sandbox_dir, "root", rustix::fs::Mode::from_raw_mode(0o700)).unwrap();
        let restricted = sandbox.path().join("root/restricted");
        std::fs::create_dir(&restricted).unwrap();
        std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o000)).unwrap();
        let root_fd = rustix::fs::openat(
            &sandbox_dir,
            "root",
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .unwrap();
        symlink(&outside, sandbox.path().join("root/outside-link")).unwrap();
        std::fs::write(
            sandbox.path().join("root").join(std::ffi::OsString::from_vec(b"leaf-\xff".to_vec())),
            "kept",
        )
        .unwrap();
        let flags = rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC;
        let mut parent = root_fd;
        let component = "0123456789".repeat(20);
        for suffix in 0..20 {
            let name = format!("{component}{suffix}");
            rustix::fs::mkdirat(&parent, &name, rustix::fs::Mode::from_raw_mode(0o755)).unwrap();
            parent = rustix::fs::openat(&parent, &name, flags, rustix::fs::Mode::empty()).unwrap();
        }
        drop(parent);

        let root = sandbox.path().join("root");
        super::remove_native_test_directory(&root).unwrap();
        assert!(!root.exists());
        assert_eq!(std::fs::read_to_string(outside.join("kept")).unwrap(), "kept");
    }

    #[test]
    fn native_test_root_clears_inherited_setgid_without_changing_access() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        let sandbox = tempfile::TempDir::new_in(scratch).unwrap();
        let root = sandbox.path().join("root");
        let link = sandbox.path().join("link");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o2755)).unwrap();
        symlink(&root, &link).unwrap();

        assert!(super::clear_inherited_setgid(&link).is_err());
        assert_eq!(std::fs::metadata(&root).unwrap().permissions().mode() & 0o7777, 0o2755);

        super::clear_inherited_setgid(&root).unwrap();

        assert_eq!(std::fs::metadata(&root).unwrap().permissions().mode() & 0o7777, 0o755);
        for mode in [0o2377, 0o2300, 0o377] {
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode)).unwrap();
            super::clear_inherited_setgid(&root).unwrap();
            assert_eq!(std::fs::metadata(&root).unwrap().permissions().mode() & 0o7777, mode & !0o2000);
        }
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn selected_tests_share_preparation_once_per_source_module() {
        let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        let root = tempfile::TempDir::new_in(scratch).unwrap();
        let first = root.path().join("first.xsh");
        let second = root.path().join("second.xsh");
        std::fs::write(&first, "test one { |_| }\ntest two { |_| }\n").unwrap();
        std::fs::write(&second, "test three { |_| }\n").unwrap();
        std::fs::write(root.path().join("unselected.xsh"), "test broken { this is invalid").unwrap();
        let mut options = runner_options();
        options.exact_ids = vec![format!("{}::one", first.display()), format!("{}::two", first.display()), format!("{}::three", second.display())];
        let cases = super::discover_native_tests(&[root.path().to_owned()], &super::ConfigCache::default(), &options).unwrap_or_else(|_| panic!("selected native discovery failed"));
        assert_eq!(cases.len(), 3);
        let mut modules = std::collections::BTreeMap::<String, std::sync::Arc<xsh::execution::evaluator::PreparedTestProgram>>::new();
        for case in cases {
            let super::TestCase::Native(case) = case else { panic!("the selected module must prepare successfully") };
            if let Some(previous) = modules.get(&case.file) { assert!(std::sync::Arc::ptr_eq(previous, &case.prepared)); }
            else { modules.insert(case.file, case.prepared); }
        }
        assert_eq!(modules.len(), 2);
        let programs = modules.values().collect::<Vec<_>>();
        assert!(!std::sync::Arc::ptr_eq(programs[0], programs[1]));
    }

    #[test]
    fn default_test_jobs_use_visible_cpus_up_to_32() {
        assert_eq!(default_test_jobs_for(1), 1);
        assert_eq!(default_test_jobs_for(2), 2);
        assert_eq!(default_test_jobs_for(8), 8);
        assert_eq!(default_test_jobs_for(11), 11);
        assert_eq!(default_test_jobs_for(32), 32);
        assert_eq!(default_test_jobs_for(64), 32);
    }

    #[test]
    fn parallel_queue_fairly_admits_files_with_per_file_cap_and_stable_order() {
        let cases = vec![
            invalid_case("core/a.xsh::test_a1"),
            invalid_case("core/a.xsh::test_a2"),
            invalid_case("core/a.xsh::test_a3"),
            invalid_case("core/a.xsh::test_a4"),
            invalid_case("core/a.xsh::test_a5"),
            invalid_case("core/a.xsh::test_a6"),
            invalid_case("core/a.xsh::test_a7"),
            invalid_case("core/a.xsh::test_a8"),
            invalid_case("core/a.xsh::test_a9"),
            invalid_case("core/a.xsh::test_a10"),
            invalid_case("core/b.xsh::test_b1"),
            invalid_case("core/b.xsh::test_b2"),
            invalid_case("core/b.xsh::test_b3"),
            invalid_case("core/b.xsh::test_b4"),
            invalid_case("core/b.xsh::test_b5"),
            invalid_case("core/b.xsh::test_b6"),
            invalid_case("core/b.xsh::test_b7"),
            invalid_case("core/b.xsh::test_b8"),
            invalid_case("core/b.xsh::test_b9"),
            invalid_case("core/b.xsh::test_b10"),
            invalid_case("core/c.xsh::test_c1"),
            invalid_case("core/c.xsh::test_c2"),
        ];

        let mut queue = ParallelTestQueue::new(cases);
        let mut scheduled = Vec::new();
        while let Some((file_index, index, case)) = queue.take_next() {
            scheduled.push((file_index, index, case.id().to_string()));
        }

        assert_eq!(
            scheduled
                .iter()
                .map(|(_, index, id)| (*index, id.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0, "core/a.xsh::test_a1"),
                (10, "core/b.xsh::test_b1"),
                (20, "core/c.xsh::test_c1"),
                (1, "core/a.xsh::test_a2"),
                (11, "core/b.xsh::test_b2"),
                (21, "core/c.xsh::test_c2"),
                (2, "core/a.xsh::test_a3"),
                (12, "core/b.xsh::test_b3"),
                (3, "core/a.xsh::test_a4"),
                (13, "core/b.xsh::test_b4"),
                (4, "core/a.xsh::test_a5"),
                (14, "core/b.xsh::test_b5"),
                (5, "core/a.xsh::test_a6"),
                (15, "core/b.xsh::test_b6"),
                (6, "core/a.xsh::test_a7"),
                (16, "core/b.xsh::test_b7"),
                (7, "core/a.xsh::test_a8"),
                (17, "core/b.xsh::test_b8"),
            ]
        );
        assert_eq!(scheduled.len(), 18);
        assert!(!queue.is_complete());

        queue.finish(0);
        assert_eq!(
            queue
                .take_next()
                .map(|(_, index, case)| (index, case.id().to_string())),
            Some((8, "core/a.xsh::test_a9".to_string()))
        );
        queue.finish(1);
        assert_eq!(
            queue
                .take_next()
                .map(|(_, index, case)| (index, case.id().to_string())),
            Some((18, "core/b.xsh::test_b9".to_string()))
        );
    }

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
