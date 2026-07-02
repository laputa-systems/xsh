use crate::diagnostic::DiagnosticRenderer;
use crate::loader::{
    EntrySource, entry_source_from_bytes, parse_load_check_entry_source_with_token_table,
    parse_load_entry_source_arena_only,
};
use crate::runtime::eval::Evaluator;
use crate::runtime::process::path_bytes;
use crate::sema::check::{CheckOptions, Checker};
use crate::source::SourceMap;

use crate::syntax::parser::Parser;
use crate::trace::{TraceEvent, TracebackRenderer};
use std::fs;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const XSH_COVERAGE_TRACE_DIR: &str = "XSH_COVERAGE_TRACE_DIR";
pub const XSH_DISABLE_COMPACT_RUNNER: &str = "XSH_DISABLE_COMPACT_RUNNER";
// XSH_ALLOW_LEGACY_FALLBACK is removed; compact is the only runtime path.

#[cfg(test)]
static COMPACT_RUNNER_SUCCESSES: AtomicUsize = AtomicUsize::new(0);

enum CompactRunAttempt {
    Output(ScriptOutput),
    Fallback { entry_source: EntrySource },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunOptions {
    pub script: String,
    pub args: Vec<String>,
    pub coverage_trace_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ScriptOutput {
    pub status: u8,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
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
    let _ = Checker::check_compact_declarations(&parsed.arena);
    let output = Evaluator::new_with_sources_and_command(Vec::new(), sources, "xsh".into())
        .eval(&parsed.arena, source_id);
    ScriptOutput {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

pub fn run_script(options: RunOptions) -> ScriptOutput {
    let compact_disabled = std::env::var_os(XSH_DISABLE_COMPACT_RUNNER).is_some()
        || options.coverage_trace_dir.is_some()
        || std::env::var_os(XSH_COVERAGE_TRACE_DIR).is_some();

    if !compact_disabled {
        match try_run_compact_lowered_script_with_fallback(&options) {
            Ok(CompactRunAttempt::Output(output)) => {
                #[cfg(test)]
                COMPACT_RUNNER_SUCCESSES.fetch_add(1, Ordering::Relaxed);
                return output;
            }
            Ok(CompactRunAttempt::Fallback { entry_source }) => {
                // The program parsed cleanly but did not lower to the compact
                // runtime. At 407/407 corpus coverage this means the program is
                // invalid; run the (still-present) checker to surface its
                // diagnostics. A clean check here would mean a genuine lowering
                // gap — report it so it is investigated rather than silently run.
                return run_checked_fallback(&options.script, entry_source);
            }
            Err(err) => {
                return ScriptOutput {
                    status: 2,
                    stdout: Vec::new(),
                    stderr: text_bytes(format!(
                        "xsh: failed to read '{}': {err}\n",
                        options.script
                    )),
                };
            }
        }
    }

    // Coverage / explicitly-disabled compact: still run the compact path, but
    // with tracing enabled so coverage traces are emitted. We parse into the
    // arena and check declarations only (the full check happens via the arena
    // checker during lowering installation).
    let bytes = match fs::read(&options.script) {
        Ok(bytes) => bytes,
        Err(err) => {
            return ScriptOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsh: failed to read '{}': {err}\n", options.script)),
            };
        }
    };
    let entry_source = entry_source_from_bytes(&options.script, bytes);
    if !entry_source.diagnostics.is_empty() {
        let sources = entry_source.sources;
        return ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(
                DiagnosticRenderer::new().render(&entry_source.diagnostics, &sources),
            ),
        };
    }
    let entry_source_id = entry_source.source_id;
    let (sources, parsed) =
        parse_load_entry_source_arena_only(&options.script, entry_source, Vec::new());
    if !parsed.diagnostics.is_empty() {
        return ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources)),
        };
    }

    let command_name = script_command_name(&options.script);
    let mut evaluator =
        Evaluator::new_with_sources_and_command(options.args, sources, command_name);
    let coverage_trace_dir = options
        .coverage_trace_dir
        .or_else(|| std::env::var_os(XSH_COVERAGE_TRACE_DIR).map(PathBuf::from));
    if let Some(dir) = &coverage_trace_dir {
        evaluator = evaluator.with_tracing();
        evaluator =
            evaluator.with_env_var(XSH_COVERAGE_TRACE_DIR.as_bytes().to_vec(), path_bytes(dir));
    }
    let output = evaluator.eval(&parsed.arena, entry_source_id);
    script_output_from_eval(output, coverage_trace_dir)
}

#[cfg(test)]
pub(crate) fn try_run_compact_lowered_script(
    options: &RunOptions,
) -> Result<Option<ScriptOutput>, std::io::Error> {
    match try_run_compact_lowered_script_with_fallback(options)? {
        CompactRunAttempt::Output(output) => Ok(Some(output)),
        CompactRunAttempt::Fallback { .. } => Ok(None),
    }
}

/// A program that parsed cleanly but did not lower to the compact runtime.
/// Run the checker to surface diagnostics (the common case: the program is
/// invalid). If the checker is clean, this is a real lowering gap — report it.
fn run_checked_fallback(script: &str, entry_source: EntrySource) -> ScriptOutput {
    let checked_program = parse_load_check_entry_source_with_token_table(
        script,
        entry_source,
        Vec::new(),
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
    ScriptOutput {
        status: 1,
        stdout: Vec::new(),
        stderr: text_bytes(format!(
            "xsh: compact lowering not available for '{script}'\n"
        )),
    }
}

fn try_run_compact_lowered_script_with_fallback(
    options: &RunOptions,
) -> Result<CompactRunAttempt, std::io::Error> {
    let bytes = fs::read(&options.script)?;
    let entry_source = entry_source_from_bytes(&options.script, bytes);
    Ok(try_run_compact_lowered_entry_source(options, entry_source))
}

fn compact_fallback(sources: SourceMap, source_id: crate::source::SourceId) -> CompactRunAttempt {
    CompactRunAttempt::Fallback {
        entry_source: EntrySource {
            sources,
            source_id,
            diagnostics: Vec::new(),
        },
    }
}

fn try_run_compact_lowered_entry_source(
    options: &RunOptions,
    entry_source: EntrySource,
) -> CompactRunAttempt {
    let source_id = entry_source.source_id;
    if !entry_source.diagnostics.is_empty() {
        let sources = entry_source.sources;
        return CompactRunAttempt::Output(ScriptOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(
                DiagnosticRenderer::new().render(&entry_source.diagnostics, &sources),
            ),
        });
    }
    let (sources, parsed) =
        parse_load_entry_source_arena_only(&options.script, entry_source, Vec::new());
    if !parsed.diagnostics.is_empty() {
        return compact_fallback(sources, source_id);
    }
    let crate::syntax::parser::ArenaParseOutput {
        arena,
        cst,
        diagnostics: _,
    } = parsed;
    drop(cst);

    let mut evaluator = Evaluator::new_with_sources_and_command(
        options.args.clone(),
        sources,
        script_command_name(&options.script),
    );
    let Some(plan) = evaluator.prepare_compact_lowered_only(&arena, source_id) else {
        return compact_fallback(evaluator.into_sources(), source_id);
    };
    drop(arena);
    let output = match evaluator.eval_installed_compact_lowered_only(plan) {
        Ok(output) => output,
        Err(evaluator) => {
            return compact_fallback(evaluator.into_sources(), source_id);
        }
    };
    CompactRunAttempt::Output(script_output_from_eval(output, None))
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

fn script_command_name(script: &str) -> String {
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
    let rendered = render_coverage_trace_jsonl(&output.trace_events);
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

pub fn render_coverage_trace_jsonl(events: &[TraceEvent]) -> String {
    let mut output = String::new();
    for event in events {
        let Some(api_id) = &event.api_id else {
            continue;
        };
        let value = crate::modules::json::raw_json_object([
            (
                "kind".to_string(),
                crate::modules::json::raw_json_string(event.kind.as_str()),
            ),
            (
                "api_id".to_string(),
                crate::modules::json::raw_json_string(api_id),
            ),
        ]);
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
    fn compact_lowered_runner_attempt_executes_covered_script() {
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
        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_path_parse_and_print() {
        let path = temp_script(
            "compact-path-print",
            r#"let root = fp"${args[0]}"
print $root
"#,
        );
        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_path_constructor_binding_inference() {
        let path = temp_script(
            "compact-path-constructor",
            r#"let root = Path(args[0])
let child = fp"${root}/child"
print $child
"#,
        );
        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_path_constructor_param_default() {
        let path = temp_script(
            "compact-path-default",
            r#"proc main(root: Path = Path("/tmp/xsh-compact")) -> Result[Unit] {
  print $root
}
"#,
        );
        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_result_unit_fallthrough() {
        let path = temp_script(
            "compact-result-unit-fallthrough",
            r#"proc main() [error] {
  test.eq(1, 1)?
}
"#,
        );
        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_named_helper_call() {
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
        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_propagates_top_level_result_err() {
        let path = temp_script(
            "compact-top-level-result-err",
            r#"proc fail() [process] {
  run false
}

fail()
"#,
        );
        let output = try_run_compact_lowered_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("top-level Result error should be compact-covered");

        assert_eq!(output.status, 3);
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("nonzero-exit"));
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_lowered_runner_attempt_skips_standard_use() {
        let path = temp_script(
            "compact-standard-use",
            r#"use fs
let root = fp"${args[0]}"
print $root
"#,
        );
        let output = try_run_compact_lowered_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: vec!["/tmp/xsh-compact-use".to_string()],
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("standard use should not require compatibility Program");

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"/tmp/xsh-compact-use\n");
        assert!(output.stderr.is_empty());
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_lowered_runner_attempt_loads_user_module_without_compat_program() {
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
            r#"export pure label(value: Str) -> Str {
  return value + "!"
}
"#,
        )
        .expect("write helper module");
        let output = try_run_compact_lowered_script(&RunOptions {
            script: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt")
        .expect("user module use should not require compatibility Program");

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
    fn compact_lowered_runner_attempt_covers_implicit_rest_main() {
        let path = temp_script(
            "compact-auto-main",
            "proc main(...argv: List[Str]) [error] -> Int {
  return argv.len()
}
",
        );
        let output = try_run_compact_lowered_script(&RunOptions {
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
            r#"let root = fp"${args[0]}"
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
    fn compact_runner_fallback_reuses_loaded_entry_source() {
        let path = temp_script(
            "compact-fallback-source",
            "proc main(...argv: List[Str]) {
  let _ = argv
  with value = Ok(\"ok\") {
    let _seen = value
  } else |err| {
    let _err = err
  }
}
",
        );
        let script = path.to_string_lossy().into_owned();
        let attempt = try_run_compact_lowered_script_with_fallback(&RunOptions {
            script: script.clone(),
            args: Vec::new(),
            coverage_trace_dir: None,
        })
        .expect("compact runner attempt");
        let CompactRunAttempt::Fallback { entry_source } = attempt else {
            panic!("unsupported with-statement script should fall back with its loaded source");
        };
        fs::remove_file(&path).expect("remove original script after compact fallback");

        let checked_program = parse_load_check_entry_source_with_token_table(
            &script,
            entry_source,
            Vec::new(),
            CheckOptions::default(),
            None,
        );
        assert!(checked_program.parsed.diagnostics.is_empty());
        assert!(checked_program.check_diagnostics().is_empty());
        assert_eq!(checked_program.parsed.arena.statement_ids().count(), 1);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn compact_lowered_runner_attempt_covers_extension_count_shape() {
        let path = temp_script(
            "compact-extension-count",
            r#"let root = fp"${args[0]}"

let stats = fs.files(root, gitignore: false, stat: false)
  |> where .ext != ""
  |> count { |entry|
    entry.ext.lower()
  }

let counts = stats.keys()
  |> map { |ext|
    {count: stats.get(ext, 0), ext: ext}
  }
  |> sort-by .count

for row in counts {
  print f"${row.count} ${row.ext}"
}
"#,
        );
        let corpus = path.parent().expect("script parent").join("corpus");
        fs::create_dir_all(corpus.join("nested")).expect("create corpus");
        fs::write(corpus.join("one.rs"), b"").expect("write corpus file");
        fs::write(corpus.join("nested").join("two.rs"), b"").expect("write corpus file");
        fs::write(corpus.join("readme.md"), b"").expect("write corpus file");
        fs::write(corpus.join("no_ext"), b"").expect("write corpus file");

        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_range_flat_map_sum_collect() {
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

        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_json_log_rollup_shape() {
        let path = temp_script(
            "compact-json-rollup",
            r#"let root = fp"${args[0]}"
let logs = fp"${root}/logs"

let log_texts = fs.walk(logs, gitignore: false)
  |> where .kind == "file" and .ext == "jsonl"
  |> sort-by .path
  |> map { |entry|
    entry.path.read_text()?
  }

let rows = log_texts.join()
  |> json.lines
  |> where .level != "debug"
  |> group-by f"${.service}:${.level}"
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
  print f"${row.key} ${row.count} ${row.duration_ms}"
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

        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_manifest_hash_shape() {
        let path = temp_script(
            "compact-manifest-hash",
            r#"let root = fp"${args[0]}"
let pkgroot = fp"${root}/pkgroot"

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

        let output = try_run_compact_lowered_script(&RunOptions {
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
    fn compact_lowered_runner_attempt_covers_archive_package_shape() {
        let path = temp_script(
            "compact-archive-package",
            r#"let root = fp"${args[0]}"
let pkgroot = fp"${root}/pkgroot"
let work_root = fs.tempdir()?
defer fs.close_root(work_root)?
let work = fs.root_path(work_root)?
let tarball = fp"${work}/package.tar.gz"
archive.tar_create(tarball, pkgroot, [p"."], compression: "gz", overwrite: true)?
let entries = archive.tar_list(tarball)?
let extracted = fp"${work}/extracted"
archive.tar_extract(tarball, extracted)?
let config = fp"${extracted}/etc/demo/config.toml".read_text()?
let payload = fp"${extracted}/usr/share/demo/payload.txt".read_bytes()?
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

        let output = try_run_compact_lowered_script(&RunOptions {
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
}
