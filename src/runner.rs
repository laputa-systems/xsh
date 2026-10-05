use crate::diagnostic::{DiagnosticRenderer, Severity};
use crate::execution::script::{RunOptions, ScriptOutput, XSH_COVERAGE_TRACE_DIR};
use crate::loader::{
    EntrySource, entry_source_from_bytes, parse_load_check_entry_source_with_token_table,
    parse_load_entry_source_arena_only,
};
use crate::mem_track::{self, AllocTraffic, WorkerStageTraffic};
use crate::runtime::eval::Evaluator;
use crate::runtime::process::path_bytes;
use crate::sema::check::{CheckOptions, Checker};
use crate::source::SourceMap;

use crate::syntax::parser::Parser;
use crate::trace::{TraceEvent, TracebackRenderer};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
static COMPACT_RUNNER_SUCCESSES: AtomicUsize = AtomicUsize::new(0);

enum RunAttempt {
    Output(ScriptOutput),
    Diagnostics {
        entry_source: EntrySource,
        // The roots the failed preparation loaded with, so rendering its
        // diagnostics resolves the same modules.
        module_roots: Vec<PathBuf>,
    },
}

struct PreparedRun {
    evaluator: Evaluator,
    plan: crate::runtime::eval::CompactIndexedRunPlan,
    source_id: crate::source::SourceId,
    module_roots: Vec<PathBuf>,
    coverage_trace_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RuntimeAllocationPhases {
    pub construction: AllocTraffic,
    pub controller: AllocTraffic,
    pub worker_stages: Vec<WorkerStageTraffic>,
}

impl PreparedRun {
    fn run(self) -> RunAttempt {
        let output = self
            .evaluator
            .eval_installed_compact_indexed_only(self.plan);
        let output = match output {
            Ok(output) => output,
            Err(evaluator) => {
                return diagnostic_attempt(
                    evaluator.into_sources(),
                    self.source_id,
                    self.module_roots,
                );
            }
        };
        RunAttempt::Output(script_output_from_eval(output, self.coverage_trace_dir))
    }
}

#[cfg(feature = "native-tests")]
pub struct PreparedBenchmarkScript {
    prepared: PreparedRun,
    options: RunOptions,
}

#[cfg(feature = "native-tests")]
impl PreparedBenchmarkScript {
    pub fn run(self) -> ScriptOutput {
        finish_run_attempt(&self.options, self.prepared.run())
    }
}

fn text_bytes(text: impl Into<String>) -> Vec<u8> {
    text.into().into_bytes()
}

fn push_text(buf: &mut Vec<u8>, text: &str) {
    buf.extend_from_slice(text.as_bytes());
}

/// Boot the full interpreter pipeline (parse, check, build the evaluator and
/// register the standard environment) on an empty program, then exit — running no
/// user code. `xsh --startup` exposes this so the interpreter's fixed startup cost
/// can be measured and subtracted as a benchmarking baseline.
pub fn run_startup() -> ScriptOutput {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("<startup>", "");
    let parsed = Parser::parse_source_arena_only(source_id, "");
    let mut evaluator = Evaluator::new_with_sources_and_command(Vec::new(), sources, "xsh".into());
    let plan = evaluator
        .prepare_compact_indexed_only(&parsed.arena, source_id)
        .expect("empty startup program must encode as indexed IR");
    let output = evaluator
        .eval_installed_compact_indexed_only(plan)
        .unwrap_or_else(|_| panic!("verified startup IR must execute"));
    ScriptOutput {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

/// Runs a binary's work where preparing a script has the stack it needs.
///
/// Parsing, checking, and lowering recurse once per level of source nesting,
/// which the parser limits. An optimized build prepares the deepest source
/// the limit allows in under 2 MiB, so `work` runs where it is called: on the
/// main thread, whose stack is 8 MiB on every supported host. An unoptimized
/// build has frames several times larger and needs more than 9 MiB, so there
/// `work` runs on a thread whose stack does not depend on the host's limit.
pub fn on_preparation_stack<R: Send>(work: impl FnOnce() -> R + Send) -> R {
    const UNOPTIMIZED_STACK_BYTES: usize = 64 * 1024 * 1024;
    if !cfg!(debug_assertions) {
        return work();
    }
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(UNOPTIMIZED_STACK_BYTES)
            .spawn_scoped(scope, work)
            .expect("spawn the preparation thread")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

pub fn run_script(options: RunOptions) -> ScriptOutput {
    match try_run_program(&options) {
        Ok(attempt) => finish_run_attempt(&options, attempt),
        Err(err) => read_error_output(&options, err),
    }
}

/// Run one script as this process's program: buffered output is written to
/// the process's stdout and stderr before any child that shares them runs, so
/// script output and child output keep their order.
pub fn run_script_with_shared_stdio(options: RunOptions) -> ScriptOutput {
    let attempt = match try_prepare_program(&options) {
        Ok(Ok(mut prepared)) => {
            prepared.evaluator = prepared.evaluator.with_shared_stdio();
            prepared.run()
        }
        Ok(Err(attempt)) => attempt,
        Err(err) => return read_error_output(&options, err),
    };
    finish_run_attempt(&options, attempt)
}

/// Run one script like [`run_script_with_shared_stdio`] with the `linux` and
/// `unix` modules replaced by the given test fakes. Only the native test
/// harness calls this.
#[cfg(feature = "native-tests")]
pub fn run_script_with_test_fakes(
    options: RunOptions,
    linux: Option<crate::runtime::eval::LinuxFake>,
    unix: Option<crate::runtime::eval::UnixFake>,
) -> ScriptOutput {
    let attempt = match try_prepare_program(&options) {
        Ok(Ok(mut prepared)) => {
            let mut evaluator = prepared.evaluator.with_shared_stdio();
            if let Some(fake) = linux {
                evaluator = evaluator.with_linux_fake(fake);
            }
            if let Some(fake) = unix {
                evaluator = evaluator.with_unix_fake(fake);
            }
            prepared.evaluator = evaluator;
            prepared.run()
        }
        Ok(Err(attempt)) => attempt,
        Err(err) => return read_error_output(&options, err),
    };
    finish_run_attempt(&options, attempt)
}

/// Run one script with construction, controller, and explicitly spawned worker
/// allocation phases. This exists solely for `xsht runtime-stats`; ordinary
/// script execution stays on [`run_script`].
pub(crate) fn run_script_with_allocation_stats(
    options: RunOptions,
) -> (ScriptOutput, RuntimeAllocationPhases) {
    mem_track::begin_stage();
    let prepared = try_prepare_program(&options);
    let construction = mem_track::end_stage();

    mem_track::begin_worker_collection();
    mem_track::begin_stage();
    let output = match prepared {
        Ok(Ok(prepared)) => finish_run_attempt(&options, prepared.run()),
        Ok(Err(attempt)) => finish_run_attempt(&options, attempt),
        Err(err) => read_error_output(&options, err),
    };
    let controller = mem_track::end_stage();
    let worker_stages = mem_track::end_worker_collection();

    (
        output,
        RuntimeAllocationPhases {
            construction,
            controller,
            worker_stages,
        },
    )
}

fn read_error_output(options: &RunOptions, err: std::io::Error) -> ScriptOutput {
    ScriptOutput {
        status: 2,
        stdout: Vec::new(),
        stderr: text_bytes(format!("xsh: failed to read '{}': {err}\n", options.script)),
    }
}

fn finish_run_attempt(options: &RunOptions, attempt: RunAttempt) -> ScriptOutput {
    match attempt {
        RunAttempt::Output(output) => {
            #[cfg(test)]
            COMPACT_RUNNER_SUCCESSES.fetch_add(1, Ordering::Relaxed);
            output
        }
        RunAttempt::Diagnostics {
            entry_source,
            module_roots,
        } => render_checked_diagnostics(
            &options.script,
            entry_source,
            module_roots,
            options.args.clone(),
            options.coverage_trace_dir.clone(),
        ),
    }
}

#[cfg(feature = "native-tests")]
pub fn prepare_benchmark_script(
    options: RunOptions,
) -> Result<PreparedBenchmarkScript, ScriptOutput> {
    match try_prepare_program(&options) {
        Ok(Ok(prepared)) => Ok(PreparedBenchmarkScript { prepared, options }),
        Ok(Err(attempt)) => Err(finish_run_attempt(&options, attempt)),
        Err(err) => Err(ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(format!("xsh: failed to read '{}': {err}\n", options.script)),
        }),
    }
}

#[cfg(test)]
pub(crate) fn try_run_compact_indexed_script(
    options: &RunOptions,
) -> Result<Option<ScriptOutput>, std::io::Error> {
    match try_run_program(options)? {
        RunAttempt::Output(output) => Ok(Some(output)),
        RunAttempt::Diagnostics { .. } => Ok(None),
    }
}

/// A program that parsed cleanly but did not lower to the compact runtime.
/// Run the checker to surface diagnostics (the common case: the program is
/// invalid). If the checker is clean, this is a real lowering gap — report it.
fn render_checked_diagnostics(
    script: &str,
    entry_source: EntrySource,
    module_roots: Vec<PathBuf>,
    args: Vec<String>,
    _coverage_trace_dir: Option<PathBuf>,
) -> ScriptOutput {
    let checked_program = parse_load_check_entry_source_with_token_table(
        script,
        entry_source,
        module_roots,
        CheckOptions::default(),
        None,
    );
    if !checked_program.parsed.diagnostics.is_empty() {
        return ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(checked_program.render_parse_diagnostics()),
        };
    }
    if !checked_program.check_diagnostics().is_empty() {
        return ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(checked_program.render_check_diagnostics()),
        };
    }
    let diagnostics = Evaluator::compact_indexed_diagnostics(
        &checked_program.parsed.arena,
        checked_program.entry_source_id,
        checked_program.sources.clone(),
        args,
        script_command_name(script),
    );
    if !diagnostics.is_empty() {
        return ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(
                DiagnosticRenderer::new().render(&diagnostics, &checked_program.sources),
            ),
        };
    }
    ScriptOutput {
        status: 1,
        stdout: Vec::new(),
        stderr: text_bytes(format!(
            "xsh: indexed execution not available for '{script}'\n"
        )),
    }
}

fn try_run_program(options: &RunOptions) -> Result<RunAttempt, std::io::Error> {
    Ok(match try_prepare_program(options)? {
        Ok(prepared) => prepared.run(),
        Err(attempt) => attempt,
    })
}

fn try_prepare_program(
    options: &RunOptions,
) -> Result<Result<PreparedRun, RunAttempt>, std::io::Error> {
    let bytes = fs::read(&options.script)?;
    let entry_source = entry_source_from_bytes(&options.script, bytes);
    Ok(prepare_entry_source(options, entry_source))
}

fn diagnostic_attempt(
    sources: SourceMap,
    source_id: crate::source::SourceId,
    module_roots: Vec<PathBuf>,
) -> RunAttempt {
    RunAttempt::Diagnostics {
        entry_source: EntrySource {
            sources,
            source_id,
            diagnostics: Vec::new(),
        },
        module_roots,
    }
}

fn prepare_entry_source(
    options: &RunOptions,
    entry_source: EntrySource,
) -> Result<PreparedRun, RunAttempt> {
    let source_id = entry_source.source_id;
    if !entry_source.diagnostics.is_empty() {
        let sources = entry_source.sources;
        return Err(RunAttempt::Output(ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(
                DiagnosticRenderer::new().render(&entry_source.diagnostics, &sources),
            ),
        }));
    }
    // A project config that cannot be read stops the run: skipping it would
    // load a different set of modules than the project declares.
    let module_roots = match crate::project::project_module_roots(Path::new(&options.script)) {
        Ok(roots) => roots,
        Err(message) => {
            return Err(RunAttempt::Output(ScriptOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsh: {message}\n")),
            }));
        }
    };
    let (sources, parsed) =
        parse_load_entry_source_arena_only(&options.script, entry_source, module_roots.clone());
    if !parsed.diagnostics.is_empty() {
        return Err(diagnostic_attempt(sources, source_id, module_roots));
    }
    let crate::syntax::parser::ArenaParseOutput {
        arena,
        cst,
        diagnostics: _,
    } = parsed;
    drop(cst);
    let arena = Arc::new(arena);

    let entry_text = sources
        .get(source_id)
        .map(|source| source.text())
        .unwrap_or("");
    // This one check renders the diagnostics and supplies the lowering facts.
    let check = Checker::check_arena_with_options_and_type_program(
        &arena,
        entry_text,
        CheckOptions {
            embedded_bodies: true,
            ..CheckOptions::default()
        },
        Arc::clone(&arena),
    );
    if check
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(RunAttempt::Output(ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(DiagnosticRenderer::new().render(&check.diagnostics, &sources)),
        }));
    }

    let mut evaluator = Evaluator::new_with_sources_and_command(
        options.args.clone(),
        sources,
        script_command_name(&options.script),
    )
    .with_module_roots(module_roots.clone());
    let coverage_trace_dir = options
        .coverage_trace_dir
        .clone()
        .or_else(|| std::env::var_os(XSH_COVERAGE_TRACE_DIR).map(PathBuf::from));
    if let Some(dir) = &coverage_trace_dir {
        evaluator = evaluator.with_tracing();
        evaluator =
            evaluator.with_env_var(XSH_COVERAGE_TRACE_DIR.as_bytes().to_vec(), path_bytes(dir));
    }
    let Ok(plan) = arena.symbol_owner().with_current(|| {
        let declarations = Checker::compact_declarations(&arena, check);
        evaluator.prepare_checked_compact_indexed_only(&arena, source_id, false, declarations)
    }) else {
        return Err(diagnostic_attempt(
            evaluator.into_sources(),
            source_id,
            module_roots,
        ));
    };
    drop(arena);
    Ok(PreparedRun {
        evaluator,
        plan,
        source_id,
        module_roots,
        coverage_trace_dir,
    })
}

fn script_output_from_eval(
    output: crate::runtime::eval::EvalOutput,
    coverage_trace_dir: Option<PathBuf>,
) -> ScriptOutput {
    let sources = output.sources.clone();
    let coverage_trace_result = write_nested_coverage_trace(&output, coverage_trace_dir.as_ref());
    let mut stderr = output.stderr;

    if let Err(message) = coverage_trace_result {
        push_text(&mut stderr, &format!("xsh: {message}\n"));
        return ScriptOutput {
            status: 4,
            stdout: output.stdout,
            stderr,
        };
    }

    if !output.diagnostics.is_empty() {
        push_text(
            &mut stderr,
            &DiagnosticRenderer::new().render(&output.diagnostics, &sources),
        );
    }

    let status = if let Some(traceback) = output.traceback {
        push_text(
            &mut stderr,
            &TracebackRenderer::new().render(&traceback, &sources),
        );
        3
    } else {
        output.status
    };

    ScriptOutput {
        status,
        stdout: output.stdout,
        stderr,
    }
}

pub fn script_command_name(script: &str) -> String {
    let path = std::path::Path::new(script);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(script);
    let stem = name.strip_suffix(".xsh").unwrap_or(name);
    if stem == "main" {
        return path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or(stem)
            .to_string();
    }
    stem.to_string()
}

#[allow(clippy::single_call_fn)]
pub(crate) fn write_nested_coverage_trace(
    output: &crate::runtime::eval::EvalOutput,
    dir: Option<&PathBuf>,
) -> Result<(), String> {
    let Some(dir) = dir else {
        return Ok(());
    };
    fs::create_dir_all(dir).map_err(|err| {
        format!(
            "failed to create coverage trace directory '{}': {err}",
            dir.display()
        )
    })?;
    let rendered = render_coverage_trace_jsonl(&output.trace_events, &output.sources);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for attempt in 0..100u32 {
        let path = dir.join(format!("xsh-{}-{now}-{attempt}.jsonl", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(rendered.as_bytes()).map_err(|err| {
                    format!("failed to write coverage trace '{}': {err}", path.display())
                })?;
                return Ok(());
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => {
                return Err(format!(
                    "failed to create coverage trace '{}': {err}",
                    path.display()
                ));
            }
        }
    }
    Err(format!(
        "failed to allocate unique coverage trace file in '{}'",
        dir.display()
    ))
}

fn render_coverage_trace_jsonl(events: &[TraceEvent], sources: &SourceMap) -> String {
    let mut output = String::new();
    for file in sources.files() {
        let value = crate::modules::json::raw_json_object([
            (
                "kind".to_string(),
                crate::modules::json::raw_json_string("source.file"),
            ),
            (
                "file".to_string(),
                crate::modules::json::raw_json_string(file.name()),
            ),
            (
                "line_count".to_string(),
                crate::modules::json::raw_json_usize(file.line_count()),
            ),
        ]);
        output.push_str(&crate::modules::json::compact_raw_json(&value));
        output.push('\n');
    }
    for event in events {
        let mut fields = vec![(
            "kind".to_string(),
            crate::modules::json::raw_json_string(event.kind.as_str()),
        )];

        if let Some(api_id) = &event.api_id {
            fields.push((
                "api_id".to_string(),
                crate::modules::json::raw_json_string(api_id),
            ));
        }

        if let Some(name) = &event.name {
            fields.push((
                "name".to_string(),
                crate::modules::json::raw_json_string(name),
            ));
        }

        if let Some(span) = event.source_span
            && let (Some(start), Some(end)) = (
                sources.location(span.source_id, span.start()),
                sources.location(span.source_id, span.end()),
            )
        {
            fields.push((
                "source_span".to_string(),
                crate::modules::json::raw_json_object([
                    (
                        "file".to_string(),
                        crate::modules::json::raw_json_string(start.file),
                    ),
                    (
                        "start_line".to_string(),
                        crate::modules::json::raw_json_usize(start.line),
                    ),
                    (
                        "end_line".to_string(),
                        crate::modules::json::raw_json_usize(end.line),
                    ),
                    (
                        "start_offset".to_string(),
                        crate::modules::json::raw_json_usize(span.start()),
                    ),
                    (
                        "end_offset".to_string(),
                        crate::modules::json::raw_json_usize(span.end()),
                    ),
                ]),
            ));
        }

        if let Some(span) = event.definition_span
            && let (Some(start), Some(end)) = (
                sources.location(span.source_id, span.start()),
                sources.location(span.source_id, span.end()),
            )
        {
            fields.push((
                "definition_span".to_string(),
                crate::modules::json::raw_json_object([
                    (
                        "file".to_string(),
                        crate::modules::json::raw_json_string(start.file),
                    ),
                    (
                        "start_line".to_string(),
                        crate::modules::json::raw_json_usize(start.line),
                    ),
                    (
                        "end_line".to_string(),
                        crate::modules::json::raw_json_usize(end.line),
                    ),
                    (
                        "start_offset".to_string(),
                        crate::modules::json::raw_json_usize(span.start()),
                    ),
                    (
                        "end_offset".to_string(),
                        crate::modules::json::raw_json_usize(span.end()),
                    ),
                ]),
            ));
        }

        let value = crate::modules::json::raw_json_object(fields);
        output.push_str(&crate::modules::json::compact_raw_json(&value));
        output.push('\n');
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_script(name: &str, source: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "xsh-runner-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("create temp script dir");
        let path = dir.join(format!("{name}.xsh"));
        fs::write(&path, source).expect("write temp script");
        path
    }

    #[test]
    fn compact_indexed_runner_attempt_executes_covered_script() {
        let path = temp_script(
            "compact-runner",
            "pure double(n: Int) -> Int {
  return n * 2
}

pure add_one(n: Int) -> Int {
  return double(n) + 1
}

add_one(4)
",
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("covered compact script");
        assert_eq!(output.status, 9);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_coverage_trace_preserves_function_definition_span() {
        let path = temp_script(
            "compact-definition-span",
            "pure double(n: Int) -> Int {\n  return n * 2\n}\n\nproc main() [error] {\n  test.eq(double(4), 8)?\n}\n",
        );
        let trace_dir = path
            .parent()
            .expect("temporary script parent")
            .join("coverage-traces");
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: Some(trace_dir.clone()),
        })
        .expect("compact runner attempt")
        .expect("definition-span script should run");
        assert_eq!(output.status, 0);

        let trace_path = fs::read_dir(&trace_dir)
            .expect("coverage trace directory")
            .map(|entry| entry.expect("coverage trace entry").path())
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "jsonl")
            })
            .expect("coverage trace file");
        let trace = fs::read_to_string(trace_path).expect("read coverage trace");
        let pure_enter = trace
            .lines()
            .find(|line| line.contains("\"kind\":\"pure.enter\""))
            .expect("pure enter event");
        assert!(
            pure_enter.contains("\"definition_span\":") && pure_enter.contains("\"start_line\":1"),
            "{pure_enter}"
        );

        let _ = fs::remove_dir_all(path.parent().expect("temporary script parent"));
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_path_parse_and_print() {
        let path = temp_script(
            "compact-path-print",
            r#"let root = fp"{args[0]}"
print $root
"#,
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec!["/tmp/xsh-compact".to_string()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("path parse and print should be compact-covered");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"/tmp/xsh-compact\n");
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_path_constructor_binding_inference() {
        let path = temp_script(
            "compact-path-constructor",
            r#"let root = Path(args[0])
let child = fp"{root}/child"
print $child
"#,
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec!["/tmp/xsh-compact".to_string()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("Path constructor binding should be compact-covered");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"/tmp/xsh-compact/child\n");
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_path_constructor_param_default() {
        let path = temp_script(
            "compact-path-default",
            r#"proc main(root: Path = Path("/tmp/xsh-compact")) -> Result[Unit] {
  print $root
}
"#,
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("Path constructor default should be compact-covered");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"/tmp/xsh-compact\n");
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_result_unit_fallthrough() {
        let path = temp_script(
            "compact-result-unit-fallthrough",
            r#"proc main() [error] {
  test.eq(1, 1)?
}
"#,
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("Result[Unit] fallthrough should be compact-covered");

        assert_eq!(output.status, 0);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_named_helper_call() {
        let path = temp_script(
            "compact-named-helper-call",
            r#"pure label(value: Str, suffix: Str = "!") -> Str {
  return value + suffix
}

proc main() [error] {
  test.eq(label(value: "ok"), "ok!")?
}
"#,
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("named helper call should be compact-covered");

        assert_eq!(output.status, 0);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_propagates_top_level_result_err() {
        let path = temp_script(
            "compact-top-level-result-err",
            r#"proc fail() [process, error] {
  run false
}

fail()
"#,
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("top-level Result error should be compact-covered");

        assert_eq!(output.status, 3);
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with("err: `false` exited 1\n"),
            "stderr: {stderr}"
        );
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_skips_standard_use() {
        let path = temp_script(
            "compact-standard-use",
            r#"use fs
let root = fp"{args[0]}"
print $root
"#,
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec!["/tmp/xsh-compact-use".to_string()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("standard use should stay on the executable program path");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"/tmp/xsh-compact-use\n");
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_loads_user_module_without_compat_program() {
        let path = temp_script(
            "compact-user-use",
            r#"use compact_user_helper

let value = compact_user_helper.label("ok")
print ${value}
"#,
        );
        let helper = path
            .parent()
            .expect("temp script has parent")
            .join("compact_user_helper.xsh");
        fs::write(
            &helper,
            r#"##! Compact runner helper module.
## Labels a value.
export pure label(value: Str) -> Str {
  return value + "!"
}
"#,
        )
        .expect("write helper module");
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("user module use should stay on the executable program path");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"ok!\n");
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&helper);
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_implicit_rest_main() {
        let path = temp_script(
            "compact-auto-main",
            "proc main(...argv: List[Str]) [error] -> Int {
  return argv.len()
}
",
        );
        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec!["one".to_string(), "two".to_string()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("implicit rest main should be compact-covered");

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn run_script_uses_compact_runner_by_default_for_covered_scripts() {
        let path = temp_script(
            "compact-default-runner",
            r#"let root = fp"{args[0]}"
print $root
"#,
        );
        let before = COMPACT_RUNNER_SUCCESSES.load(Ordering::Relaxed);
        let output = run_script(RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec!["/tmp/xsh-compact-default".to_string()],
            coverage_trace_dir: None,
        });
        let after = COMPACT_RUNNER_SUCCESSES.load(Ordering::Relaxed);

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"/tmp/xsh-compact-default\n");
        assert!(output.stderr.is_empty());
        assert!(
            after > before,
            "covered run_script invocation should return through compact runner"
        );
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn runner_renders_shared_module_diagnostics_once() {
        let script = temp_script(
            "shared-module-docs",
            "use helper
use left
use right
",
        );
        let root = script.parent().expect("temporary script parent");
        fs::write(
            root.join("helper.xsh"),
            "##! Helper module.\n## This comment is not attached to an export.\nlet value = 1\n\n## Exports a value.\nexport let exported: Int = value\n",
        )
        .expect("write helper module");
        fs::write(
            root.join("left.xsh"),
            "##! Left module.\n## Exports a value.\nexport let value: Int = 1\n",
        )
        .expect("write left module");
        fs::write(
            root.join("right.xsh"),
            "##! Right module.\n## Exports a value.\nexport let value: Int = 2\n",
        )
        .expect("write right module");
        let output = run_script(RunOptions {
            script: script.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        });
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert_eq!(output.status, 2);
        assert_eq!(
            stderr.matches("check.orphan-doc-comment").count(),
            1,
            "shared-module diagnostics should render once: {stderr}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn prepared_benchmark_script_matches_normal_execution() {
        let path = temp_script(
            "prepared-benchmark-script",
            r#"let root = fp"{args[0]}"
print $root
"#,
        );
        let options = RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec!["/tmp/xsh-prepared-benchmark".to_string()],
            coverage_trace_dir: None,
        };
        let expected = run_script(options.clone());
        let actual = prepare_benchmark_script(options)
            .expect("prepare benchmark script")
            .run();

        assert_eq!(actual.status, expected.status);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn diagnostic_attempt_reuses_loaded_entry_source() {
        let source = "proc main(...argv: List[Str]) {\n  let _ = argv\n";
        let path = temp_script("diagnostic-attempt-source", source);
        let script = path.to_string_lossy().into_owned();
        let attempt = try_run_program(&RunOptions {
            script: script.clone(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt");
        let RunAttempt::Diagnostics { entry_source, .. } = attempt else {
            panic!("syntax diagnostics should preserve the already loaded source");
        };
        fs::remove_file(&path).expect("remove original script after diagnostics preparation");

        let checked_program = parse_load_check_entry_source_with_token_table(
            &script,
            entry_source,
            Vec::new(),
            CheckOptions::default(),
            None,
        );
        assert!(!checked_program.parsed.diagnostics.is_empty());
        assert_eq!(
            checked_program
                .sources
                .get(checked_program.entry_source_id)
                .unwrap()
                .text(),
            source,
        );
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_extension_count_shape() {
        let path = temp_script(
            "compact-extension-count",
            r#"let root = fp"{args[0]}"

let stats = fs.files(root, gitignore: false, stat: false)
  |> where .ext != ""
  |> count { |entry|
    entry.ext.lower()
  }

let counts = stats.keys()
  |> map { |ext|
    {count: stats.get(ext) ?? 0, ext: ext}
  }
  |> sort-by .count

for row in counts {
  print f"{row.count} {row.ext}"
}
"#,
        );
        let corpus = path.parent().expect("script parent").join("corpus");
        fs::create_dir_all(corpus.join("nested")).expect("create corpus");
        fs::write(corpus.join("one.rs"), b"").expect("write corpus file");
        fs::write(corpus.join("nested").join("two.rs"), b"").expect("write corpus file");
        fs::write(corpus.join("readme.md"), b"").expect("write corpus file");
        fs::write(corpus.join("no_ext"), b"").expect("write corpus file");

        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec![corpus.to_string_lossy().into_owned()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("extension-count shape should be compact-covered");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"1 md\n2 rs\n");
        assert!(output.stderr.is_empty());
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_range_flat_map_sum_collect() {
        let path = temp_script(
            "compact-range-stream",
            "pure expand(seed: Int) -> List[Int] {
  return [seed, seed + 1]
}

let rows = range(0, 4)
  |> flat-map { |seed|
    expand(seed)
  }
  |> collect()

let total = rows |> sum
print $total
",
        );

        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("range/flat-map/sum/collect shape should be compact-covered");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"16\n");
        assert!(output.stderr.is_empty());
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_json_log_rollup_shape() {
        let path = temp_script(
            "compact-json-rollup",
            r#"type LogLine = {service: Str, level: Str, duration_ms: Int}
let root = fp"{args[0]}"
let logs = fp"{root}/logs"

let log_texts = fs.walk(logs, gitignore: false)
  |> where .kind == "file" and .ext == "jsonl"
  |> sort-by .path
  |> map { |entry|
    entry.path.read_text()?
  }

let rows = log_texts.join()
  |> json.lines
  |> map { |line| line.require(LogLine)? }
  |> where .level != "debug"
  |> group-by f"{.service}:{.level}"
  |> map { |bucket|
    {
      key: bucket.key,
      count: bucket.items |> count(),
      duration_ms: bucket.items
        |> map .duration_ms
        |> sum,
    }
  }
  |> sort-by .key

for row in rows {
  print f"{row.key} {row.count} {row.duration_ms}"
}
"#,
        );
        let corpus = path.parent().expect("script parent").join("corpus");
        let logs = corpus.join("logs");
        fs::create_dir_all(&logs).expect("create logs");
        fs::write(
            logs.join("a.jsonl"),
            b"{\"service\":\"api\",\"level\":\"info\",\"duration_ms\":11}\n{\"service\":\"api\",\"level\":\"debug\",\"duration_ms\":99}\n",
        )
        .expect("write log");
        fs::write(
            logs.join("b.jsonl"),
            b"{\"service\":\"worker\",\"level\":\"warn\",\"duration_ms\":3}\n{\"service\":\"api\",\"level\":\"error\",\"duration_ms\":7}\n",
        )
        .expect("write log");
        fs::write(logs.join("ignore.txt"), b"not json").expect("write ignored file");

        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec![corpus.to_string_lossy().into_owned()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("json-log-rollup shape should be compact-covered");

        assert_eq!(output.status, 0);
        assert_eq!(
            output.stdout,
            b"api:error 1 7\napi:info 1 11\nworker:warn 1 3\n"
        );
        assert!(output.stderr.is_empty());
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_manifest_hash_shape() {
        let path = temp_script(
            "compact-manifest-hash",
            r#"let root = fp"{args[0]}"
let pkgroot = fp"{root}/pkgroot"

let manifest = fs.files(pkgroot, gitignore: false)
  |> map { |entry|
    let data = entry.path.read_bytes()?

    {
      path: entry.path.strip_prefix(pkgroot)?.display(),
      sha256: data.sha256().hex(),
      size: data.len(),
      executable: entry.mode % 512 == 493,
    }
  }
  |> sort-by .path

let manifest_json = json.encode(manifest)?

let total_size = manifest
  |> map .size
  |> sum

print ${manifest |> count()} $total_size manifest[0].path manifest[0].sha256 manifest_json.count_lines()
"#,
        );
        let corpus = path.parent().expect("script parent").join("corpus");
        let pkgroot = corpus.join("pkgroot");
        let config = b"name = \"demo\"\n";
        let payload = b"payload";
        fs::create_dir_all(pkgroot.join("etc").join("demo")).expect("create config dir");
        fs::create_dir_all(pkgroot.join("usr").join("share").join("demo"))
            .expect("create payload dir");
        fs::write(pkgroot.join("etc").join("demo").join("config.toml"), config)
            .expect("write config");
        fs::write(
            pkgroot
                .join("usr")
                .join("share")
                .join("demo")
                .join("payload.txt"),
            payload,
        )
        .expect("write payload");

        let digest =
            crate::modules::hash::digest_bytes(crate::modules::hash::HashAlgorithm::Sha256, config);
        let expected = format!(
            "2 {} etc/demo/config.toml {} 1\n",
            config.len() + payload.len(),
            crate::modules::hash::digest_hex(&digest),
        );

        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec![corpus.to_string_lossy().into_owned()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("manifest-hash shape should be compact-covered");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn compact_indexed_runner_attempt_covers_archive_package_shape() {
        let path = temp_script(
            "compact-archive-package",
            r#"let root = fp"{args[0]}"
let pkgroot = fp"{root}/pkgroot"
let work_root = fs.tempdir()?
defer work_root.close()?
let work = work_root.host_path()?
let tarball = fp"{work}/package.tar.gz"
archive.tar_create(tarball, pkgroot, [p"."], compression: "gz", overwrite: true)?
let entries = archive.tar_list(tarball)?.collect()
let extracted = fp"{work}/extracted"
archive.tar_extract(tarball, extracted)?
let config = fp"{extracted}/etc/demo/config.toml".read_text()?
let payload = fp"{extracted}/usr/share/demo/payload.txt".read_bytes()?
print ${entries |> count()} config.count_lines() payload.sha256().hex()
"#,
        );
        let corpus = path.parent().expect("script parent").join("corpus");
        let pkgroot = corpus.join("pkgroot");
        let config = b"name = \"demo\"\n";
        let payload = b"payload";
        fs::create_dir_all(pkgroot.join("etc").join("demo")).expect("create config dir");
        fs::create_dir_all(pkgroot.join("usr").join("share").join("demo"))
            .expect("create payload dir");
        fs::write(pkgroot.join("etc").join("demo").join("config.toml"), config)
            .expect("write config");
        fs::write(
            pkgroot
                .join("usr")
                .join("share")
                .join("demo")
                .join("payload.txt"),
            payload,
        )
        .expect("write payload");

        let digest = crate::modules::hash::digest_bytes(
            crate::modules::hash::HashAlgorithm::Sha256,
            payload,
        );
        let expected = format!("7 1 {}\n", crate::modules::hash::digest_hex(&digest));

        let output = try_run_compact_indexed_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec![corpus.to_string_lossy().into_owned()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("archive-package shape should be compact-covered");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    // A call that spreads a record into named arguments binds the record
    // once and reads each field where it is passed, so the stack it takes to
    // prepare and run does not grow with the record's width. It once bound a
    // slot per field, each binding nested in the one before, and every pass
    // over the lowered program recursed through that nesting: a few thousand
    // fields overflowed the stack. The preparing thread is given a fixed
    // stack here, so the bound holds whatever limit the host gives a main
    // thread.
    #[test]
    fn a_wide_named_spread_is_prepared_within_a_fixed_stack() {
        const FIELDS: usize = 4000;
        let declared = (0..FIELDS)
            .map(|index| format!("f{index}: Int"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = (0..FIELDS)
            .map(|index| format!("f{index}: {index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let path = temp_script(
            "wide-named-spread",
            &format!(
                "type Wide = {{{declared}}}\n\nproc rebuild(wide: Wide) -> Wide {{\n  Wide(...wide)\n}}\n\nlet wide: Wide = {{{values}}}\nlet again = rebuild(wide)\nprint ${{again.f{}}}\n",
                FIELDS - 1
            ),
        );
        let options = RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        };
        let output = std::thread::Builder::new()
            .stack_size(3 * 1024 * 1024)
            .spawn(move || run_script(options))
            .expect("spawn the preparing thread")
            .join()
            .expect("the run finishes");

        assert_eq!(
            output.status,
            0,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, format!("{}\n", FIELDS - 1).as_bytes());
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    /// Source that repeats one nesting construct `levels` times, with the
    /// number of repetitions that reaches the nesting limit exactly.
    fn nested_source(family: &str, levels: usize) -> String {
        let open = |text: &str| text.repeat(levels);
        match family {
            "list" => format!("let x = {}1{}\nprint \"ok\"\n", open("["), open("]")),
            "record" => format!("let x = {}1{}\nprint \"ok\"\n", open("{a: "), open("}")),
            "call" => format!(
                "pure f(v: Int) -> Int {{\n  v + 1\n}}\n\nlet x = {}1{}\nprint \"ok\"\n",
                open("f("),
                open(")")
            ),
            "method" => format!("let x = \" a \"{}\nprint \"ok\"\n", open(".trim()")),
            "sum" => format!("let x = 1{}\nprint \"ok\"\n", open(" + 1")),
            "not" => format!("let x = {}true\nprint \"ok\"\n", open("! ")),
            "type" => format!(
                "let x: {}Int{} = {}{}\nprint \"ok\"\n",
                open("List["),
                open("]"),
                open("["),
                open("]")
            ),
            "pattern" => format!(
                "let v = {}1{}\nmatch v {{\n  {}x{} => print \"ok\"\n  _ => print \"no\"\n}}\n",
                open("["),
                open("]"),
                open("["),
                open("]")
            ),
            "if" => format!("{}print \"ok\"\n{}", open("if true {\n"), open("}\n")),
            "if value" => format!(
                "let x = {}1{}\nprint \"ok\"\n",
                open("if true { "),
                open(" } else { 0 }")
            ),
            "stage block" => format!(
                "let x = {}1{}\nprint \"ok\"\n",
                open("[1] |> map { "),
                open(" }")
            ),
            "interpolation" => {
                let mut value = "1".to_string();
                for _ in 0..levels {
                    value = format!("f\"{{{value}}}\"");
                }
                format!("let x = {value}\nprint \"ok\"\n")
            }
            other => panic!("unknown family {other}"),
        }
    }

    /// Runs `source` the way `xsh` does, preparing it on a thread whose stack
    /// is the 8 MiB of a main thread at most, whatever the host's limit is.
    /// An unoptimized build prepares on the larger stack `xsh` gives it.
    fn run_on_fixed_stack(name: &str, source: &str) -> ScriptOutput {
        let path = temp_script(name, source);
        let options = RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        };
        let output = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(move || on_preparation_stack(move || run_script(options)))
            .expect("spawn the main-thread stand-in")
            .join()
            .expect("the run finishes");
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
        output
    }

    // Each family reaches the limit of 128 levels with the count given here:
    // a script that deep is prepared and evaluated, and one more repetition
    // is the nesting diagnostic. The counts differ because a repetition is
    // one level for a list and several for a block, which holds a statement
    // that holds an expression.
    #[test]
    fn nesting_runs_at_the_limit_and_is_a_diagnostic_one_level_past_it() {
        for (family, at_limit) in [
            ("list", 126),
            ("record", 126),
            ("call", 126),
            ("method", 63),
            ("sum", 126),
            ("not", 126),
            ("type", 126),
            ("pattern", 126),
            ("if", 63),
            ("if value", 62),
            ("stage block", 42),
        ] {
            let name = family.replace(' ', "-");
            let output = run_on_fixed_stack(&name, &nested_source(family, at_limit));
            assert_eq!(
                (output.status, output.stdout.as_slice()),
                (0, b"ok\n".as_slice()),
                "{family} at {at_limit}: {}",
                String::from_utf8_lossy(&output.stderr)
            );

            let output = run_on_fixed_stack(&name, &nested_source(family, at_limit + 1));
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status, 2, "{family} at {}: {stderr}", at_limit + 1);
            assert!(
                stderr.starts_with("err[parse.nesting-depth]: nesting is deeper than 128 levels"),
                "{family} at {}: {stderr}",
                at_limit + 1
            );
            assert_eq!(stderr.matches("err[").count(), 1, "{family}: {stderr}");
            assert!(output.stdout.is_empty(), "{family}");
        }
    }

    // The parser stops at the limit instead of recursing through the whole
    // input, in a string's interpolations as in the file itself, so input far
    // past the limit costs no more stack than input just past it.
    #[test]
    fn nesting_far_past_the_limit_is_the_same_diagnostic() {
        for family in ["list", "call", "if", "sum", "method", "interpolation"] {
            let output =
                run_on_fixed_stack(&format!("far-{family}"), &nested_source(family, 50_000));
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status, 2, "{family}: {stderr}");
            assert!(
                stderr.starts_with("err[parse.nesting-depth]"),
                "{family}: {}",
                &stderr[..stderr.len().min(300)]
            );
            assert_eq!(stderr.matches("err[").count(), 1, "{family}");
        }
    }
}
