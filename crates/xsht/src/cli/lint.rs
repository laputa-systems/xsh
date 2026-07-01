use crate::xsht::cli::{
    CliOutput, XshConfig, cancellation_output, collect_configured_xsh_files, is_path_excluded,
    load_config, nearest_config_for_file, text_bytes,
};
use crate::xsht::config::FileToolConfig;
use crate::xsht::edit::{SourceEdit, apply_cst_guarded_edits};
use crate::xsht::lint::{LintOptions, Linter};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label};
use xsh::loader::{parse_load_check_bytes, parse_load_check_text};
use xsh::sema::check::CheckOptions;
use xsh::source::{SourceId, SourceMap, Span};
pub fn lint_files(files: &[String], fix: bool, runless: bool) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }

    let mut stderr = String::new();
    let mut status = 0;

    let cwd_config = match load_config() {
        Ok(config) => config,
        Err(message) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht: {message}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };

    let discovered;
    let files: &[String] = if files.is_empty() {
        let mut paths = Vec::new();
        if let Err(message) = collect_configured_xsh_files(Path::new("."), &cwd_config, &mut paths)
        {
            if let Some(output) = cancellation_output() {
                return output;
            }
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht: {message}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
        let mut filtered = Vec::new();
        let config_cache = ConfigCache::default();
        for path in paths {
            match excluded_by_nearest_config(&path, &cwd_config, &config_cache) {
                Ok(true) => {}
                Ok(false) => filtered.push(path.to_string_lossy().into_owned()),
                Err(message) => {
                    return CliOutput {
                        status: 2,
                        stdout: Vec::new(),
                        stderr: text_bytes(format!("xsht: {message}\n")),
                        trace_text: String::new(),
                        syscall_summary: None,
                    };
                }
            }
        }
        discovered = filtered;
        &discovered
    } else {
        files
    };

    let config_cache = ConfigCache::default();
    let mut results = lint_files_parallel(files, fix, runless, &cwd_config, &config_cache);
    if let Some(output) = cancellation_output() {
        return output;
    }
    results.sort_unstable_by_key(|result| result.index);
    let mut seen_diagnostics = FxHashSet::default();
    for result in results {
        match result.kind {
            LintResultKind::Clean => {}
            LintResultKind::ReadError(message) => {
                status = 2;
                stderr.push_str(&message);
            }
            LintResultKind::FixDiagnostics {
                status: result_status,
                stderr: result_stderr,
            } => {
                if result_status == 1 {
                    if status == 0 {
                        status = 1;
                    }
                } else {
                    status = result_status;
                }
                stderr.push_str(&result_stderr);
            }
            LintResultKind::Diagnostics {
                status: result_status,
                diagnostics,
            } => {
                if result_status == 1 {
                    if status == 0 {
                        status = 1;
                    }
                } else {
                    status = result_status;
                }
                for diagnostic in diagnostics {
                    if seen_diagnostics.insert(diagnostic.key) {
                        stderr.push_str(&diagnostic.text);
                    }
                }
            }
            LintResultKind::Write { file, text } => {
                if let Err(err) = fs::write(&file, &text) {
                    status = 4;
                    stderr.push_str(&format!("xsht: failed to write '{file}': {err}\n"));
                }
            }
        }
    }

    CliOutput {
        status,
        stdout: Vec::new(),
        stderr: stderr.into_bytes(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

struct LintResult {
    index: usize,
    kind: LintResultKind,
}

enum LintResultKind {
    Clean,
    ReadError(String),
    Diagnostics {
        status: u8,
        diagnostics: Vec<RenderedDiagnostic>,
    },
    FixDiagnostics {
        status: u8,
        stderr: String,
    },
    Write {
        file: String,
        text: String,
    },
}

struct RenderedDiagnostic {
    key: String,
    text: String,
}

struct ResolvedLintConfig {
    lint_options: LintOptions,
    line_width: usize,
    module_roots: Vec<PathBuf>,
}

type CachedConfig = Result<Option<(PathBuf, XshConfig)>, String>;

#[derive(Default)]
struct ConfigCache {
    nearest: Mutex<FxHashMap<PathBuf, CachedConfig>>,
}

impl ConfigCache {
    fn nearest_config_for_file(&self, file: &Path) -> CachedConfig {
        let parent = file.parent().unwrap_or_else(|| Path::new("."));
        let key = if parent.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            parent.to_path_buf()
        };
        if let Some(cached) = self
            .nearest
            .lock()
            .expect("config cache mutex poisoned")
            .get(&key)
            .cloned()
        {
            return cached;
        }
        let resolved = nearest_config_for_file(file);
        self.nearest
            .lock()
            .expect("config cache mutex poisoned")
            .insert(key, resolved.clone());
        resolved
    }
}

fn lint_config_for_file(
    file: &str,
    runless: bool,
    cwd_config: &XshConfig,
    config_cache: &ConfigCache,
) -> Result<ResolvedLintConfig, String> {
    let (config_dir, config) = config_cache
        .nearest_config_for_file(Path::new(file))?
        .unwrap_or_else(|| (PathBuf::from("."), cwd_config.clone()));
    let tool_config = FileToolConfig { config_dir, config };
    let line_width = tool_config.line_width();
    let module_roots = tool_config.module_roots();
    let lint_options = LintOptions {
        runless,
        runless_except: tool_config.config.lint.runless_except,
        interactive_command_replacement: None,
        expr_types: Default::default(),
        callable_effects: Default::default(),
    };
    Ok(ResolvedLintConfig {
        lint_options,
        line_width,
        module_roots,
    })
}

fn excluded_by_nearest_config(
    path: &Path,
    cwd_config: &XshConfig,
    config_cache: &ConfigCache,
) -> Result<bool, String> {
    let (config_dir, config) = config_cache
        .nearest_config_for_file(path)?
        .unwrap_or_else(|| (PathBuf::from("."), cwd_config.clone()));
    Ok(is_path_excluded(&config_dir, path, &config.exclude))
}

#[allow(clippy::single_call_fn)]
fn lint_files_parallel(
    files: &[String],
    fix: bool,
    runless: bool,
    cwd_config: &XshConfig,
    config_cache: &ConfigCache,
) -> Vec<LintResult> {
    if files.is_empty() {
        return Vec::new();
    }
    let next = AtomicUsize::new(0);
    let (tx, rx) = crossbeam_channel::unbounded();
    let workers = worker_count(files.len());

    thread::scope(|scope| {
        for _ in 0..workers {
            let next = &next;
            let tx = tx.clone();
            scope.spawn(move || {
                loop {
                    if cancellation_output().is_some() {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = files.get(index) else {
                        break;
                    };
                    let result = lint_one_file(index, file, fix, runless, cwd_config, config_cache);
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(tx);
    rx.into_iter().collect()
}

#[allow(clippy::single_call_fn)]
fn lint_one_file(
    index: usize,
    file: &str,
    fix: bool,
    runless: bool,
    cwd_config: &XshConfig,
    config_cache: &ConfigCache,
) -> LintResult {
    let bytes = match fs::read(file) {
        Ok(bytes) => bytes,
        Err(err) => {
            return LintResult {
                index,
                kind: LintResultKind::ReadError(format!("xsht: failed to read '{file}': {err}\n")),
            };
        }
    };

    let config = match lint_config_for_file(file, runless, cwd_config, config_cache) {
        Ok(config) => config,
        Err(message) => {
            return LintResult {
                index,
                kind: LintResultKind::ReadError(format!("xsht: {message}\n")),
            };
        }
    };

    if fix {
        let mut sources = SourceMap::new();
        let source_id = match sources.add_file_from_utf8(file, bytes.clone()) {
            Ok(source_id) => source_id,
            Err(error) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let source_id = sources.add_file(file, text.clone());
                let offset = error.offset.min(text.len());
                let span = xsh::source::Span::new(source_id, offset, offset);
                let diagnostics = vec![
                    Diagnostic::error("source file is not valid UTF-8")
                        .with_code("source.invalid-utf8")
                        .with_label(Label::primary(span, "invalid UTF-8 starts here")),
                ];
                return LintResult {
                    index,
                    kind: LintResultKind::FixDiagnostics {
                        status: 2,
                        stderr: DiagnosticRenderer::new().render(&diagnostics, &sources),
                    },
                };
            }
        };
        let text = sources
            .get(source_id)
            .expect("source was just inserted")
            .text()
            .to_string();
        return lint_one_file_with_fixes(index, file, text, &config);
    }

    let mut checked_program = parse_load_check_bytes(
        file,
        bytes,
        config.module_roots.clone(),
        CheckOptions::default(),
    );
    if !checked_program.parsed.diagnostics.is_empty() {
        return LintResult {
            index,
            kind: LintResultKind::Diagnostics {
                status: 2,
                diagnostics: render_diagnostics_with_keys(
                    &checked_program.parsed.diagnostics,
                    &checked_program.sources,
                ),
            },
        };
    }
    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    if !checked.diagnostics.is_empty() {
        return LintResult {
            index,
            kind: LintResultKind::Diagnostics {
                status: 2,
                diagnostics: render_diagnostics_with_keys(
                    &checked.diagnostics,
                    &checked_program.sources,
                ),
            },
        };
    }

    let checked = checked_program
        .checked
        .take()
        .expect("checked program after clean parse");
    let mut lint_options = config.lint_options;
    lint_options.expr_types = checked.expr_types;
    lint_options.callable_effects = checked.callable_effects;
    let text = checked_program.entry_source_text().unwrap_or("");
    let linted = Linter::lint(&checked_program.parsed.arena, text, lint_options);
    if linted.diagnostics.is_empty() {
        return LintResult {
            index,
            kind: LintResultKind::Clean,
        };
    }
    LintResult {
        index,
        kind: LintResultKind::Diagnostics {
            status: 1,
            diagnostics: render_diagnostics_with_keys(
                &linted.diagnostics,
                &checked_program.sources,
            ),
        },
    }
}

#[allow(clippy::single_call_fn)]
fn lint_one_file_with_fixes(
    index: usize,
    file: &str,
    text: String,
    config: &ResolvedLintConfig,
) -> LintResult {
    let checked_program = parse_load_check_text(
        file,
        text.clone(),
        config.module_roots.clone(),
        CheckOptions::default(),
    );
    if !checked_program.parsed.diagnostics.is_empty() {
        return LintResult {
            index,
            kind: LintResultKind::FixDiagnostics {
                status: 2,
                stderr: checked_program.render_parse_diagnostics(),
            },
        };
    }
    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    let check_diagnostics_are_entry_effect_violations =
        entry_check_diagnostics_are_effect_violations(
            &checked.diagnostics,
            checked_program.entry_source_id,
        );
    let fixable_check_diagnostics: Vec<_> = checked
        .diagnostics
        .iter()
        .filter(|d| d.fix_hints.iter().any(|h| h.replacement.is_some()))
        .collect();
    if !checked.diagnostics.is_empty()
        && !check_diagnostics_are_entry_effect_violations
        && fixable_check_diagnostics.is_empty()
    {
        return LintResult {
            index,
            kind: LintResultKind::FixDiagnostics {
                status: 2,
                stderr: checked_program.render_check_diagnostics(),
            },
        };
    }
    let mut lint_options = config.lint_options.clone();
    lint_options.expr_types = checked.expr_types.clone();
    lint_options.callable_effects = checked.callable_effects.clone();
    let linted = Linter::lint(&checked_program.parsed.arena, &text, lint_options);

    let ast_fixes = if checked.diagnostics.is_empty() {
        collect_fix_spans(&linted.diagnostics)
    } else {
        let mut fixes = collect_fix_spans_for_codes(&linted.diagnostics, &["lint.missing-effects"]);
        fixes.extend(collect_fix_spans(&checked.diagnostics));
        fixes
    };
    if ast_fixes.is_empty() {
        if linted.diagnostics.is_empty() {
            return LintResult {
                index,
                kind: LintResultKind::Clean,
            };
        }
        return LintResult {
            index,
            kind: LintResultKind::FixDiagnostics {
                status: 1,
                stderr: DiagnosticRenderer::new()
                    .render(&linted.diagnostics, &checked_program.sources),
            },
        };
    }

    let final_text = match apply_cst_fixes(file, &text, &ast_fixes, config) {
        Ok(Some(text)) => text,
        Ok(None) => {
            return LintResult {
                index,
                kind: LintResultKind::FixDiagnostics {
                    status: 1,
                    stderr: DiagnosticRenderer::new()
                        .render(&linted.diagnostics, &checked_program.sources),
                },
            };
        }
        Err(stderr) => {
            return LintResult {
                index,
                kind: LintResultKind::FixDiagnostics { status: 2, stderr },
            };
        }
    };
    if let Err(stderr) = validate_fixed_text(
        file,
        &final_text,
        config,
        check_diagnostics_are_entry_effect_violations,
    ) {
        return LintResult {
            index,
            kind: LintResultKind::FixDiagnostics { status: 2, stderr },
        };
    }

    if final_text == text {
        LintResult {
            index,
            kind: LintResultKind::Clean,
        }
    } else {
        LintResult {
            index,
            kind: LintResultKind::Write {
                file: file.to_string(),
                text: final_text,
            },
        }
    }
}

fn validate_fixed_text(
    file: &str,
    text: &str,
    config: &ResolvedLintConfig,
    allow_non_entry_check_diagnostics: bool,
) -> Result<(), String> {
    let checked_program = parse_load_check_text(
        file,
        text.to_string(),
        config.module_roots.clone(),
        CheckOptions::default(),
    );
    if !checked_program.parsed.diagnostics.is_empty() {
        return Err(checked_program.render_parse_diagnostics());
    }
    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    if !checked.diagnostics.is_empty() {
        if allow_non_entry_check_diagnostics
            && checked.diagnostics.iter().all(|diagnostic| {
                !diagnostic_is_from_source(diagnostic, checked_program.entry_source_id)
            })
        {
            return Ok(());
        }
        return Err(checked_program.render_check_diagnostics());
    }
    Ok(())
}

fn apply_cst_fixes(
    file: &str,
    text: &str,
    fixes: &[(usize, usize, String)],
    config: &ResolvedLintConfig,
) -> Result<Option<String>, String> {
    let edits = fixes
        .iter()
        .map(|(start, end, replacement)| SourceEdit {
            start: *start,
            end: *end,
            replacement: replacement.clone(),
        })
        .collect::<Vec<_>>();
    apply_cst_guarded_edits(file, text, &edits, config.line_width)
}

fn render_diagnostics_with_keys(
    diagnostics: &[Diagnostic],
    sources: &SourceMap,
) -> Vec<RenderedDiagnostic> {
    diagnostics
        .iter()
        .map(|diagnostic| RenderedDiagnostic {
            key: diagnostic_key(diagnostic, sources),
            text: DiagnosticRenderer::new().render(std::slice::from_ref(diagnostic), sources),
        })
        .collect()
}

#[allow(clippy::single_call_fn)]
fn worker_count(file_count: usize) -> usize {
    thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, file_count.min(2))
}

#[allow(clippy::single_call_fn)]
fn collect_fix_spans(diagnostics: &[Diagnostic]) -> Vec<(usize, usize, String)> {
    collect_fix_spans_by_code(diagnostics, |_| true)
}

fn collect_fix_spans_for_codes(
    diagnostics: &[Diagnostic],
    allowed_codes: &[&str],
) -> Vec<(usize, usize, String)> {
    collect_fix_spans_by_code(diagnostics, |diagnostic| {
        diagnostic
            .code
            .as_deref()
            .is_some_and(|code| allowed_codes.contains(&code))
    })
}

fn collect_fix_spans_by_code(
    diagnostics: &[Diagnostic],
    include: impl Fn(&Diagnostic) -> bool,
) -> Vec<(usize, usize, String)> {
    let mut fixes: Vec<_> = diagnostics
        .iter()
        .filter(|d| include(d))
        .flat_map(|d| d.fix_hints.iter())
        .filter(|h| !h.dangerous)
        .filter_map(|h| {
            let span = h.span?;
            let repl = h.replacement.as_ref()?.clone();
            Some((span.start(), span.end(), repl))
        })
        .collect();
    fixes.sort_unstable_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    let mut non_overlapping: Vec<(usize, usize, String)> = Vec::with_capacity(fixes.len());
    for fix in fixes {
        if non_overlapping
            .last()
            .is_some_and(|(_, end, _)| fix.0 < *end)
        {
            continue;
        }
        non_overlapping.push(fix);
    }
    non_overlapping
}

fn entry_check_diagnostics_are_effect_violations(
    diagnostics: &[Diagnostic],
    entry_source_id: SourceId,
) -> bool {
    let mut saw_entry_diagnostic = false;
    for diagnostic in diagnostics {
        if !diagnostic_is_from_source(diagnostic, entry_source_id) {
            continue;
        }
        saw_entry_diagnostic = true;
        if diagnostic.code.as_deref() != Some("check.effect-violation") {
            return false;
        }
    }
    saw_entry_diagnostic
}

fn diagnostic_is_from_source(diagnostic: &Diagnostic, source_id: SourceId) -> bool {
    diagnostic
        .span
        .is_some_and(|span| span.source_id == source_id)
        || diagnostic
            .labels
            .iter()
            .any(|label| label.span.source_id == source_id)
}

#[allow(clippy::single_call_fn)]
fn diagnostic_key(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let span = diagnostic
        .labels
        .first()
        .map(|label| label.span)
        .or(diagnostic.span);
    let location = span.and_then(|span| {
        sources
            .location(span.source_id, span.start())
            .map(|loc| (span, loc))
    });
    match location {
        Some((span, loc)) => format!(
            "{:?}:{}:{}:{}:{}:{}",
            diagnostic.severity,
            diagnostic.code.as_deref().unwrap_or(""),
            diagnostic.message,
            loc.file,
            span.start(),
            span.end()
        ),
        None => format!(
            "{:?}:{}:{}",
            diagnostic.severity,
            diagnostic.code.as_deref().unwrap_or(""),
            diagnostic.message
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::xsht::cli::lint::{
        LintResultKind, ResolvedLintConfig, apply_cst_fixes, collect_fix_spans,
        lint_one_file_with_fixes,
    };
    use crate::xsht::format::DEFAULT_LINE_WIDTH;
    use crate::xsht::lint::LintOptions;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use xsh::diagnostic::{Diagnostic, FixHint, Severity};
    use xsh::source::{SourceId, Span};

    fn config() -> ResolvedLintConfig {
        ResolvedLintConfig {
            lint_options: LintOptions::default(),
            line_width: DEFAULT_LINE_WIDTH,
            module_roots: Vec::<PathBuf>::new(),
        }
    }

    #[test]
    fn collect_fix_spans_drops_nested_replacements() {
        let source_id = SourceId::new(0);
        let outer = Diagnostic::new(Severity::Warning, "outer").with_fix_hint(
            FixHint::replacement(Span::new(source_id, 10, 50), "outer", "large"),
        );
        let inner = Diagnostic::new(Severity::Warning, "inner").with_fix_hint(
            FixHint::replacement(Span::new(source_id, 20, 31), "inner", "small"),
        );

        let fixes = collect_fix_spans(&[inner, outer]);

        assert_eq!(fixes, vec![(10, 50, "large".to_string())]);
    }

    #[test]
    fn lint_fix_handles_nested_map_fixes_without_corrupting_source() {
        let source = "\
type EtcSum = {path: Str, sha256: Str}

export proc map_etcsums(etcsums: List[EtcSum]) [error] -> Result[Map[Str]] {
  var mapped: Map[Str] = map.empty()

  for entry in etcsums {
    mapped[entry.path] = entry.sha256
  }

  mapped
}
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("var mapped = {entry.path: entry.sha256 for entry in etcsums}"));
        assert!(text.contains("\n  mapped\n"));
        assert!(!text.contains("}d"));
        assert!(!text.contains("map.empty()"));
    }

    #[test]
    fn lint_fix_declines_comment_bearing_spans() {
        let source = "\
let value = 1
# keep this attached to the next statement
print ${value}
";
        let config = config();
        let result = apply_cst_fixes(
            "fixture.xsh",
            source,
            &[(
                0,
                source.len(),
                "let value = 2\nprint ${value}\n".to_string(),
            )],
            &config,
        )
        .expect("apply fixes");

        assert_eq!(result, None);
    }

    #[test]
    fn lint_fix_rewrites_empty_map_initializer_through_ast() {
        let source = r#"
let counts: Map[Int] = map.empty()
print ${counts.has("x")}
"#;
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("let counts: Map[Int] = {}"));
        assert!(text.contains("print counts.has(\"x\")"));
        assert!(!text.contains("map.empty()"));
    }

    #[test]
    fn lint_fix_rewrites_tail_return_binding_through_ast() {
        let source = "\
proc overlap(left: List[Str], right: List[Str]) -> List[Str] {
  var values = [item for item in left if right.contains(item)]
  return values
}
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("  [item for item in left if right.contains(item)]"));
        assert!(!text.contains("var values"));
        assert!(!text.contains("return values"));
    }

    #[test]
    fn lint_fix_rewrites_tail_ok_return_through_ast() {
        let source = "\
proc parsed(value: Int) -> Result[Int] {
  return Ok(value + 1)
}
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("  value + 1"));
        assert!(!text.contains("return Ok"));
    }

    #[test]
    fn lint_fix_rewrites_typed_empty_list_return_binding_through_ast() {
        let source = "\
pure empty() -> List[Str] {
  let values: List[Str] = []
  return values
}
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("  []"));
        assert!(!text.contains("let values"));
        assert!(!text.contains("return values"));
    }

    #[test]
    fn lint_fix_repairs_missing_effect_annotations_after_check_error() {
        let source = "\
proc main() [fs] {
  let _ = fs.read_text(Path(\"x\"))?
}
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("proc main() [fs, error]"));
    }

    #[test]
    fn lint_fix_repairs_missing_effects_from_called_restricted_proc() {
        let source = "\
proc timestamp() [time] -> Int {
  time.now()
}

proc main() [] -> Int {
  timestamp()
}
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("proc main() [time] -> Int"));
    }

    #[test]
    fn lint_fix_repairs_missing_effects_from_imported_module_proc() {
        let temp = TempDir::new().expect("tempdir");
        let module_path = temp.path().join("kbuild.xsh");
        fs::write(
            &module_path,
            "\
export proc image_task() [env] -> Int {
  1
}
",
        )
        .expect("write module");
        let entry_path = temp.path().join("main.xsh");
        let source = "\
use kbuild

proc main() [] -> Int {
  kbuild.image_task()
}
";
        let config = config();
        let result = lint_one_file_with_fixes(
            0,
            &entry_path.to_string_lossy(),
            source.to_string(),
            &config,
        );
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("proc main() [env] -> Int"));
    }

    #[test]
    fn lint_fix_repairs_entry_effects_with_unrelated_module_check_error() {
        let temp = TempDir::new().expect("tempdir");
        let module_path = temp.path().join("kbuild.xsh");
        fs::write(
            &module_path,
            "\
export proc image_task() [env] -> Int {
  1
}

export proc unrelated_bad() {
  1()
}
",
        )
        .expect("write module");
        let entry_path = temp.path().join("main.xsh");
        let source = "\
use kbuild

proc main() [] -> Int {
  kbuild.image_task()
}
";
        let config = config();
        let result = lint_one_file_with_fixes(
            0,
            &entry_path.to_string_lossy(),
            source.to_string(),
            &config,
        );
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("proc main() [env] -> Int"));
    }
}
