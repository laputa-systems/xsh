use crate::xsht::cli::{
    CliOutput, XshConfig, cancellation_output, collect_configured_xsh_files, is_path_excluded,
    load_config, nearest_config_for_file, text_bytes,
};
use crate::xsht::config::{FileToolConfig, config_for_dir};
use crate::xsht::edit::{SourceEdit, apply_cst_guarded_edits};
use crate::xsht::lint::{LintOptions, Linter};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label, Severity};
use xsh::frontend::check::CheckOptions;
use xsh::frontend::load::{parse_load_check_bytes, parse_load_check_text};
use xsh::frontend::source::SourceMap;
use xsh::frontend::symbols::SymbolOwner;
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

    let discovered = match discover_lint_files(files, &cwd_config) {
        Ok(files) => files,
        Err(message) => {
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
    };

    let config_cache = ConfigCache::default();
    let mut results = lint_files_parallel(&discovered, fix, runless, &cwd_config, &config_cache);
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
            LintResultKind::Write {
                file,
                text,
                status: result_status,
                stderr: result_stderr,
            } => {
                if result_status > status {
                    status = result_status;
                }
                stderr.push_str(&result_stderr);
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

fn discover_lint_files(files: &[String], config: &XshConfig) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    if files.is_empty() {
        collect_configured_xsh_files(Path::new("."), config, &mut paths)?;
        let config_cache = ConfigCache::default();
        let mut filtered = Vec::with_capacity(paths.len());
        for path in paths {
            if !excluded_by_nearest_config(&path, config, &config_cache)? {
                filtered.push(path);
            }
        }
        paths = filtered;
    } else {
        for file in files {
            let path = Path::new(file);
            if path.is_dir() {
                let dir_config = config_for_dir(path, config)?.config;
                collect_configured_xsh_files(path, &dir_config, &mut paths)?;
            } else {
                paths.push(path.to_path_buf());
            }
        }
    }
    paths.sort_unstable();
    paths.dedup();
    Ok(paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
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
        status: u8,
        stderr: String,
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
                let span = xsh::frontend::source::Span::new(source_id, offset, offset);
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

    let symbols = SymbolOwner::new();
    let mut checked_program = symbols.with_current(|| {
        parse_load_check_bytes(
            file,
            bytes,
            config.module_roots.clone(),
            CheckOptions::default(),
        )
    });
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
            status: lint_diagnostics_status(&linted.diagnostics),
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
    let symbols = SymbolOwner::new();
    let checked_program = symbols.with_current(|| {
        parse_load_check_text(
            file,
            text.clone(),
            config.module_roots.clone(),
            CheckOptions::default(),
        )
    });
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
    let mut lint_options = config.lint_options.clone();
    lint_options.expr_types = checked.expr_types.clone();
    lint_options.callable_effects = checked.callable_effects.clone();
    let linted = Linter::lint(&checked_program.parsed.arena, &text, lint_options);

    let mut ast_fixes = collect_fix_spans(&linted.diagnostics);
    ast_fixes.extend(collect_fix_spans(&checked.diagnostics));
    if ast_fixes.is_empty() {
        if linted.diagnostics.is_empty() {
            return LintResult {
                index,
                kind: if checked.diagnostics.is_empty() {
                    LintResultKind::Clean
                } else {
                    LintResultKind::FixDiagnostics {
                        status: 2,
                        stderr: checked_program.render_check_diagnostics(),
                    }
                },
            };
        }
        let mut stderr = String::new();
        let status = if checked.diagnostics.is_empty() {
            lint_diagnostics_status(&linted.diagnostics)
        } else {
            2
        };
        if !checked.diagnostics.is_empty() {
            stderr.push_str(&checked_program.render_check_diagnostics());
            stderr.push('\n');
        }
        stderr.push_str(
            &DiagnosticRenderer::new().render(&linted.diagnostics, &checked_program.sources),
        );
        return LintResult {
            index,
            kind: LintResultKind::FixDiagnostics { status, stderr },
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
    let remaining_check_stderr =
        match validate_fixed_text(file, &final_text, config, &checked.diagnostics) {
            Ok(stderr) => stderr,
            Err(stderr) => {
                return LintResult {
                    index,
                    kind: LintResultKind::FixDiagnostics { status: 2, stderr },
                };
            }
        };

    if final_text == text {
        if remaining_check_stderr.is_empty() {
            LintResult {
                index,
                kind: LintResultKind::Clean,
            }
        } else {
            LintResult {
                index,
                kind: LintResultKind::FixDiagnostics {
                    status: 2,
                    stderr: remaining_check_stderr,
                },
            }
        }
    } else {
        LintResult {
            index,
            kind: LintResultKind::Write {
                file: file.to_string(),
                text: final_text,
                status: if remaining_check_stderr.is_empty() {
                    0
                } else {
                    2
                },
                stderr: remaining_check_stderr,
            },
        }
    }
}

fn validate_fixed_text(
    file: &str,
    text: &str,
    config: &ResolvedLintConfig,
    original_check_diagnostics: &[Diagnostic],
) -> Result<String, String> {
    let symbols = SymbolOwner::new();
    let checked_program = symbols.with_current(|| {
        parse_load_check_text(
            file,
            text.to_string(),
            config.module_roots.clone(),
            CheckOptions::default(),
        )
    });
    if !checked_program.parsed.diagnostics.is_empty() {
        return Err(checked_program.render_parse_diagnostics());
    }
    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    if !check_diagnostics_are_preserved(original_check_diagnostics, &checked.diagnostics) {
        return Err(checked_program.render_check_diagnostics());
    }
    Ok(if checked.diagnostics.is_empty() {
        String::new()
    } else {
        checked_program.render_check_diagnostics()
    })
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

fn lint_diagnostics_status(diagnostics: &[Diagnostic]) -> u8 {
    if diagnostics.iter().all(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic.code.as_deref() == Some("lint.path-constructor")
    }) {
        0
    } else {
        1
    }
}

fn check_diagnostic_signature(diagnostic: &Diagnostic) -> String {
    format!(
        "{:?}:{}:{}",
        diagnostic.severity,
        diagnostic.code.as_deref().unwrap_or(""),
        diagnostic.message
    )
}

fn check_diagnostics_are_preserved(original: &[Diagnostic], current: &[Diagnostic]) -> bool {
    let mut remaining = FxHashMap::default();
    for diagnostic in original {
        *remaining
            .entry(check_diagnostic_signature(diagnostic))
            .or_insert(0usize) += 1;
    }
    for diagnostic in current {
        let Some(count) = remaining.get_mut(&check_diagnostic_signature(diagnostic)) else {
            return false;
        };
        if *count == 0 {
            return false;
        }
        *count -= 1;
    }
    true
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
    use xsh::frontend::source::{SourceId, Span};
    use xsh::frontend::symbols::SymbolOwner;

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
##! Lint fixture module.
type EtcSum = {path: Str, sha256: Str}

## Builds a map from etcsum records.
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

        assert!(text.contains("let counts = {}"));
        assert!(text.contains("print counts.has(\"x\")"));
        assert!(!text.contains("map.empty()"));
    }

    #[test]
    fn lint_fix_rewrites_needless_annotation_through_ast() {
        let source = "\
let name: Str = \"pkg\"
print ${name}
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("let name = \"pkg\""));
        assert!(!text.contains(": Str"));
    }

    #[test]
    fn lint_fix_keeps_var_annotation_when_reassigned() {
        let source = "\
var build_env: Record = {A: \"1\"}
build_env = {A: \"1\", B: \"2\"}
let _ = build_env.has(\"B\")
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);

        assert!(matches!(result.kind, LintResultKind::Clean));
    }

    #[test]
    fn lint_fix_removes_run_status_propagation_through_ast() {
        let source = "\
run test -f p\"missing\" ?
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert_eq!(text, "run test -f p\"missing\"\n");
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
    fn lint_fix_applies_safe_lints_with_unrelated_check_errors() {
        let source = "\
proc main(names: List[Str]) {
  let path = Path(\"/srv/xsh\")
  if ! names.contains(\"factory/tools\") {
    print $path
  }
}

let unresolved = missing
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected safe lint fixes to be written");
        };

        assert!(!text.contains("Path(\"/srv/xsh\")"));
        assert!(text.contains("not in"));
        assert!(text.contains("let unresolved = missing"));
    }

    #[test]
    fn lint_fix_does_not_create_orphan_docs_from_multiline_strings() {
        let source = "\
proc main() {
  let target = Path(\"/srv/xsh\")
  let report = \"# Manager\\n\\n## North-star impact\\n\\nfixture\\n\\n## task-tags\\n\"
  print $target
}
";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write {
            text,
            status,
            stderr,
            ..
        } = result.kind
        else {
            panic!("expected safe lint fix to be written");
        };

        assert_eq!(status, 0, "unexpected diagnostics: {stderr}");
        assert!(stderr.is_empty());
        assert!(text.contains("## North-star impact"));
        assert!(text.contains("## task-tags"));
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
        SymbolOwner::new().with_current(|| {
            let temp = TempDir::new().expect("tempdir");
            let module_path = temp.path().join("ARGV.xsh");
            fs::write(
                &module_path,
                "\
##! Kbuild fixture module.
## Returns a task status with an environment effect.
export proc image_task() [env] -> Int {
  1
}
",
            )
            .expect("write module");
            let entry_path = temp.path().join("main.xsh");
            let source = "\
use ARGV

proc main() [] -> Int {
  ARGV.image_task()
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
        });
    }

    #[test]
    fn lint_fix_repairs_entry_effects_with_unrelated_module_check_error() {
        SymbolOwner::new().with_current(|| {
            let temp = TempDir::new().expect("tempdir");
            let module_path = temp.path().join("ARGV.xsh");
            fs::write(
                &module_path,
                "\
##! Kbuild fixture module.
## Returns a task status with an environment effect.
export proc image_task() [env] -> Int {
  1
}

## Deliberately contains an unrelated module error.
export proc unrelated_bad() {
  1()
}
",
            )
            .expect("write module");
            let entry_path = temp.path().join("main.xsh");
            let source = "\
use ARGV

proc main() [] -> Int {
  ARGV.image_task()
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
        });
    }
}
