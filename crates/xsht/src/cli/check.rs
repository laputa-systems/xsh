use crate::xsht::cli::{
    CliOutput, XshConfig, cancellation_output, collect_configured_xsh_files, collect_xsh_files,
    load_config, text_bytes,
};
use crate::xsht::config::{config_for_dir, config_for_file};
use crate::xsht::format::Formatter;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label, LabelStyle};
use xsh::execution::evaluator::Evaluator;
use xsh::frontend::check::{
    AnnotationFactKind, CheckOptions, CheckOutput, Checker, CompactBodyProbeOutput,
};
use xsh::frontend::load::{self as loader, parse_load_check_file};
use xsh::frontend::source::{SourceId, SourceMap, Span};
use xsh::frontend::syntax::parser::Parser;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnnotationPolicy {
    params: bool,
    returns: bool,
    exports: bool,
    locals: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnotationSelection {
    Configured,
    Policy(AnnotationPolicy),
}

impl AnnotationPolicy {
    pub(crate) fn annotates_returns(self) -> bool { self.returns }

    pub fn defaults() -> Self {
        Self {
            params: true,
            returns: true,
            exports: true,
            locals: false,
        }
    }

    pub fn signatures() -> Self {
        Self {
            params: true,
            returns: true,
            exports: false,
            locals: false,
        }
    }

    pub fn with_locals() -> Self {
        Self {
            locals: true,
            ..Self::defaults()
        }
    }

    pub fn all() -> Self {
        Self {
            params: true,
            returns: true,
            exports: true,
            locals: true,
        }
    }

    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let mut policy = Self {
            params: false,
            returns: false,
            exports: false,
            locals: false,
        };
        for name in names {
            match name.trim() {
                "" => {}
                "default" | "defaults" => policy = Self::defaults(),
                "signature" | "signatures" => policy = Self::signatures(),
                "all" => policy = Self::all(),
                "none" => {
                    policy = Self {
                        params: false,
                        returns: false,
                        exports: false,
                        locals: false,
                    };
                }
                "params" | "parameters" => policy.params = true,
                "returns" | "return" => policy.returns = true,
                "exports" | "exported" => policy.exports = true,
                "locals" | "local-bindings" => policy.locals = true,
                other => {
                    return Err(format!(
                        "unknown annotation class '{other}' (expected params, returns, exports, locals, default, signatures, all, or none)"
                    ));
                }
            }
        }
        Ok(policy)
    }

    pub fn from_arg(value: &str) -> Result<Self, String> {
        if value == "locals" {
            return Ok(Self::with_locals());
        }
        Self::from_names(value.split(','))
    }
}

pub fn check_script(script: &str) -> CliOutput {
    check_one_script(
        script,
        None,
        &[],
        XshConfig::default().format.line_width,
    )
}

pub fn check_paths_with_options(
    paths: &[String],
    annotation_selection: Option<AnnotationSelection>,
) -> CliOutput {
    check_paths_with_summary_options(paths, annotation_selection, false)
}

pub fn check_paths_with_summary_options(
    paths: &[String],
    annotation_selection: Option<AnnotationSelection>,
    summary: bool,
) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }

    let config = match load_config() {
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
    let annotation_policy = match annotation_selection {
        None => None,
        Some(AnnotationSelection::Configured) => match configured_annotation_policy(&config) {
            Ok(policy) => Some(policy),
            Err(message) => {
                return CliOutput {
                    status: 2,
                    stdout: Vec::new(),
                    stderr: text_bytes(format!("xsht: {message}\n")),
                    trace_text: String::new(),
                    syscall_summary: None,
                };
            }
        },
        Some(AnnotationSelection::Policy(policy)) => Some(policy),
    };
    let mut files = Vec::new();
    if paths.is_empty() {
        if let Err(message) = collect_configured_xsh_files(Path::new("."), &config, &mut files) {
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
    } else {
        for path in paths {
            let path = Path::new(path);
            if path.is_dir() {
                let dir_config = match config_for_dir(path, &config) {
                    Ok(tool_config) => tool_config.config,
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
                if let Err(message) = collect_xsh_files(path, &dir_config.exclude, &mut files) {
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
            } else {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort_unstable();
    files.dedup();

    let check_options = CheckOptions {
        interactive_commands: None,
        reveal_types: true,
        migration_diagnostics: true,
    };

    let mut sources = SourceMap::new();
    let mut source_ids: rustc_hash::FxHashMap<String, SourceId> = rustc_hash::FxHashMap::default();

    let mut summary_counts = CheckSummary::default();
    let mut status = 0;
    let mut stderr = String::new();
    let mut seen_diagnostics: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    let mut checked_files: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    std::thread::scope(|scope| {
        let worker_count = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .min(files.len().max(1));
        let (job_tx, job_rx) = crossbeam_channel::unbounded();
        let (result_tx, result_rx) = crossbeam_channel::unbounded();
        for _ in 0..worker_count {
            let job_rx = job_rx.clone();
            let result_tx = result_tx.clone();
            std::thread::Builder::new()
                .name("xsht-check".to_string())
                .stack_size(super::FRONTEND_WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || {
                while let Ok((
                    file_index,
                    program,
                    source_id,
                    sources,
                    declarations,
                    bodies,
                    command_name,
                )) = job_rx.recv()
                {
                    let diagnostics = Evaluator::compact_lowerability_diagnostics_with_parts(
                        &program,
                        source_id,
                        sources,
                        declarations,
                        bodies,
                        Vec::new(),
                        command_name,
                    );
                    if result_tx.send((file_index, diagnostics)).is_err() {
                        break;
                    }
                }
            }).expect("spawn checker worker");
        }
        drop(result_tx);
        let mut submitted = 0;
        for (file_index, file) in files.into_iter().enumerate() {
            if cancellation_output().is_some() {
                break;
            }
            let path_str = file.to_string_lossy().into_owned();

            let canonical = file
                .canonicalize()
                .unwrap_or_else(|_| file.clone())
                .to_string_lossy()
                .into_owned();
            if !checked_files.insert(canonical) {
                continue;
            }

            let file_config = match config_for_file(&path_str, &config) {
                Ok(file_config) => file_config,
                Err(message) => {
                    status = 2;
                    stderr.push_str(&format!("xsht: {message}\n"));
                    continue;
                }
            };
            let line_width = file_config.line_width();
            let module_roots = file_config.module_roots();

            let source_id = match source_ids.get(&path_str) {
                Some(&id) => id,
                None => {
                    let bytes = match fs::read(&path_str) {
                        Ok(b) => b,
                        Err(err) => {
                            status = 2;
                            stderr.push_str(&format!("xsh: failed to read '{path_str}': {err}\n"));
                            continue;
                        }
                    };
                    let id = match sources.add_file_from_utf8(path_str.clone(), bytes.clone()) {
                        Ok(id) => id,
                        Err(error) => {
                            let text = String::from_utf8_lossy(&bytes).into_owned();
                            let sid = sources.add_file(path_str.clone(), text);
                            let offset = error.offset.min(sources.get(sid).map_or(0, |s| s.len()));
                            let diagnostics = vec![
                                Diagnostic::error("source file is not valid UTF-8")
                                    .with_code("source.invalid-utf8")
                                    .with_label(Label::primary(
                                        Span::new(sid, offset, offset),
                                        "invalid UTF-8 starts here",
                                    )),
                            ];
                            stderr.push_str(
                                &DiagnosticRenderer::new().render(&diagnostics, &sources),
                            );
                            status = 2;
                            source_ids.insert(path_str.clone(), sid);
                            continue;
                        }
                    };
                    source_ids.insert(path_str.clone(), id);
                    id
                }
            };

            let parsed = loader::parse_load_entry_source_shared_arena_only(
                &path_str,
                source_id,
                &mut sources,
                module_roots.clone(),
            );

            if !parsed.diagnostics.is_empty() {
                let new_diags: Vec<_> = parsed
                    .diagnostics
                    .iter()
                    .filter(|d| seen_diagnostics.insert(diagnostic_key(d, &sources)))
                    .cloned()
                    .collect();
                if !new_diags.is_empty() {
                    stderr.push_str(&DiagnosticRenderer::new().render(&new_diags, &sources));
                    summary_counts.observe_diagnostics(&new_diags, &sources);
                }
                status = 2;
                continue;
            }

            let entry_text = sources.get(source_id).map(|s| s.text()).unwrap_or("");
            let checked =
                Checker::check_arena_with_options(&parsed.arena, entry_text, check_options);
            if !checked.diagnostics.is_empty() {
                let new_diags: Vec<_> = checked
                    .diagnostics
                    .iter()
                    .filter(|d| seen_diagnostics.insert(diagnostic_key(d, &sources)))
                    .cloned()
                    .collect();
                if !new_diags.is_empty() {
                    stderr.push_str(&DiagnosticRenderer::new().render(&new_diags, &sources));
                    summary_counts.observe_diagnostics(&new_diags, &sources);
                }
                status = 2;
                continue;
            }

            let mut type_stderr = DiagnosticRenderer::new().render(&checked.reveal_types, &sources);

            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = if declarations.diagnostics.is_empty() {
                Checker::probe_compact_bodies(&parsed.arena, &declarations)
            } else {
                CompactBodyProbeOutput::default()
            };

            if annotation_policy.is_some() {
                let diagnostics = Evaluator::compact_lowerability_diagnostics_with_parts(
                    &parsed.arena,
                    source_id,
                    sources.clone(),
                    declarations,
                    bodies,
                    Vec::new(),
                    xsh::execution::script::script_command_name(&path_str),
                );
                if !diagnostics.is_empty() {
                    let new_diags: Vec<_> = diagnostics
                        .iter()
                        .filter(|d| seen_diagnostics.insert(diagnostic_key(d, &sources)))
                        .cloned()
                        .collect();
                    if !new_diags.is_empty() {
                        stderr.push_str(&DiagnosticRenderer::new().render(&new_diags, &sources));
                        summary_counts.observe_diagnostics(&new_diags, &sources);
                    }
                    status = 2;
                    continue;
                }
            } else {
                let source_snapshot = sources.clone();
                let command_name = xsh::execution::script::script_command_name(&path_str);
                job_tx
                    .send((
                        file_index,
                        parsed.arena,
                        source_id,
                        source_snapshot,
                        declarations,
                        bodies,
                        command_name,
                    ))
                    .expect("lowerability worker pool disconnected");
                submitted += 1;
            }

            if let Some(annotation_policy) = annotation_policy {
                let Some(original) = sources.get(source_id).map(|s| s.text().to_string()) else {
                    status = 2;
                    stderr.push_str("xsht: missing script source\n");
                    continue;
                };
                let edits = annotation_edits(
                    &checked,
                    annotation_policy,
                    source_id,
                    &original,
                );
                if !edits.is_empty() {
                    let mut annotated = original.clone();
                    for (start, end, replacement) in edits {
                        annotated.replace_range(start..end, &replacement);
                    }

                    let mut fmt_sources = SourceMap::new();
                    let fmt_id = fmt_sources.add_file(&path_str, annotated.clone());
                    let reformatted = Formatter::new()
                        .with_line_width(line_width)
                        .format_source(fmt_id, &annotated);
                    if !reformatted.diagnostics.is_empty() {
                        stderr.push_str(
                            &DiagnosticRenderer::new()
                                .render(&reformatted.diagnostics, &fmt_sources),
                        );
                        status = 2;
                        continue;
                    }
                    if reformatted.formatted != original
                        && let Err(err) = write_checked_annotation(&path_str, &reformatted.formatted, source_id, &sources, &module_roots, check_options)
                    {
                        stderr.push_str(&err.message(&path_str));
                        status = err.status();
                        continue;
                    }
                }
            }

            if !type_stderr.is_empty() && !type_stderr.ends_with('\n') {
                type_stderr.push('\n');
            }
            stderr.push_str(&type_stderr);
        }
        drop(job_tx);
        let mut lowerability_results = Vec::with_capacity(submitted);
        for _ in 0..submitted {
            lowerability_results.push(result_rx.recv().expect("lowerability worker panicked"));
        }
        lowerability_results.sort_unstable_by_key(|(index, _)| *index);
        for (_, diagnostics) in lowerability_results {
            if diagnostics.is_empty() {
                continue;
            }
            let new_diags: Vec<_> = diagnostics
                .iter()
                .filter(|d| seen_diagnostics.insert(diagnostic_key(d, &sources)))
                .cloned()
                .collect();
            if !new_diags.is_empty() {
                stderr.push_str(&DiagnosticRenderer::new().render(&new_diags, &sources));
                summary_counts.observe_diagnostics(&new_diags, &sources);
            }
            status = 2;
        }
    });
    if let Some(output) = cancellation_output() {
        return output;
    }

    if summary {
        summary_counts.write_to(&mut stderr);
    }

    CliOutput {
        status,
        stdout: Vec::new(),
        stderr: text_bytes(stderr),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

#[derive(Clone, Debug, Default)]
struct CheckSummary {
    by_code: BTreeMap<String, CheckSummaryEntry>,
}

#[derive(Clone, Debug)]
struct CheckSummaryEntry {
    count: usize,
    first: String,
}

impl CheckSummary {
    fn observe_diagnostics(&mut self, diagnostics: &[Diagnostic], sources: &SourceMap) {
        for diagnostic in diagnostics {
            let code = diagnostic
                .code
                .as_deref()
                .unwrap_or("diagnostic.uncoded")
                .to_string();
            let first = diagnostic_summary_location(diagnostic, sources);
            self.by_code
                .entry(code)
                .and_modify(|entry| entry.count += 1)
                .or_insert(CheckSummaryEntry { count: 1, first });
        }
    }

    fn write_to(&self, stderr: &mut String) {
        if !stderr.is_empty() && !stderr.ends_with('\n') {
            stderr.push('\n');
        }
        stderr.push_str("xsht check summary:\n");
        if self.by_code.is_empty() {
            stderr.push_str("  no diagnostics\n");
            return;
        }
        for (code, entry) in &self.by_code {
            stderr.push_str(&format!(
                "  {code}: {} (first: {})\n",
                entry.count, entry.first
            ));
        }
    }
}

fn diagnostic_summary_location(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let span = diagnostic
        .labels
        .iter()
        .find(|label| matches!(label.style, LabelStyle::Primary))
        .map(|label| label.span)
        .or_else(|| diagnostic.labels.first().map(|label| label.span))
        .or(diagnostic.span);
    let Some(span) = span else {
        return diagnostic.message.clone();
    };
    let Some(location) = sources.location(span.source_id, span.start()) else {
        return diagnostic.message.clone();
    };
    format!(
        "{}:{}:{} {}",
        location.file, location.line, location.column, diagnostic.message
    )
}

pub fn check_script_with_options(script: &str, annotate: bool) -> CliOutput {
    let config = match load_config() {
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
    let module_roots: Vec<PathBuf> = config.module_path.iter().map(PathBuf::from).collect();
    let annotation_policy = if annotate {
        match configured_annotation_policy(&config) {
            Ok(policy) => Some(policy),
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
    } else {
        None
    };
    let line_width = match formatter_line_width_for_script(script, &config) {
        Ok(line_width) => line_width,
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
    check_one_script(
        script,
        annotation_policy,
        &module_roots,
        line_width,
    )
}

fn check_one_script(
    script: &str,
    annotation_policy: Option<AnnotationPolicy>,
    module_roots: &[PathBuf],
    line_width: usize,
) -> CliOutput {
    let check_options = CheckOptions {
        interactive_commands: None,
        reveal_types: true,
        migration_diagnostics: true,
    };
    let checked_program = match parse_load_check_file(
        script,
        module_roots.to_vec(),
        check_options,
    ) {
        Ok(source) => source,
        Err(err) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsh: failed to read '{script}': {err}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };

    if !checked_program.parsed.diagnostics.is_empty() {
        return CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(checked_program.render_parse_diagnostics()),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    if !checked.diagnostics.is_empty() {
        return CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(checked_program.render_check_diagnostics()),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    let mut stderr =
        DiagnosticRenderer::new().render(&checked.reveal_types, &checked_program.sources);

    let declarations = Checker::compact_declarations_from_checked(&checked_program.parsed.arena, checked);
    let bodies = if declarations.diagnostics.is_empty() {
        Checker::probe_compact_bodies(&checked_program.parsed.arena, &declarations)
    } else {
        CompactBodyProbeOutput::default()
    };
    let diagnostics = Evaluator::compact_lowerability_diagnostics_with_parts(
        &checked_program.parsed.arena,
        checked_program.entry_source_id,
        checked_program.sources.clone(),
        declarations,
        bodies,
        Vec::new(),
        xsh::execution::script::script_command_name(script),
    );
    if !diagnostics.is_empty() {
        return CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(
                DiagnosticRenderer::new().render(&diagnostics, &checked_program.sources),
            ),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    if let Some(annotation_policy) = annotation_policy {
        let Some(original) = checked_program.entry_source_text() else {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes("xsht: missing script source\n"),
                trace_text: String::new(),
                syscall_summary: None,
            };
        };
        let edits = annotation_edits(
            checked,
            annotation_policy,
            checked_program.entry_source_id,
            original,
        );
        if !edits.is_empty() {
            let mut annotated = original.to_string();
            for (start, end, replacement) in edits {
                annotated.replace_range(start..end, &replacement);
            }

            let mut fmt_sources = SourceMap::new();
            let fmt_id = fmt_sources.add_file(script, annotated.clone());
            let reformatted = Formatter::new()
                .with_line_width(line_width)
                .format_source(fmt_id, &annotated);
            if !reformatted.diagnostics.is_empty() {
                return CliOutput {
                    status: 2,
                    stdout: Vec::new(),
                    stderr: text_bytes(
                        DiagnosticRenderer::new().render(&reformatted.diagnostics, &fmt_sources),
                    ),
                    trace_text: String::new(),
                    syscall_summary: None,
                };
            }
            if reformatted.formatted != original
                && let Err(err) = write_checked_annotation(script, &reformatted.formatted, checked_program.entry_source_id, &checked_program.sources, module_roots, check_options)
            {
                return CliOutput {
                    status: err.status(),
                    stdout: Vec::new(),
                    stderr: text_bytes(err.message(script)),
                    trace_text: String::new(),
                    syscall_summary: None,
                };
            }
        }
    }

    if !stderr.is_empty() && !stderr.ends_with('\n') {
        stderr.push('\n');
    }
    CliOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: text_bytes(stderr),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

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

fn configured_annotation_policy(
    config: &crate::xsht::cli::XshConfig,
) -> Result<AnnotationPolicy, String> {
    let Some(classes) = &config.check.annotate else {
        return Ok(AnnotationPolicy::defaults());
    };
    AnnotationPolicy::from_names(classes.iter().map(String::as_str))
        .map_err(|message| format!("invalid xsht-config.ini check.annotate: {message}"))
}

#[derive(Debug)]
enum AnnotationWriteError {
    Rejected(String),
    Write(std::io::Error),
}

impl AnnotationWriteError {
    fn status(&self) -> u8 {
        match self {
            Self::Rejected(_) => 2,
            Self::Write(_) => 4,
        }
    }

    fn message(&self, script: &str) -> String {
        match self {
            Self::Rejected(diagnostics) => diagnostics.clone(),
            Self::Write(error) => format!("xsht: failed to write '{script}': {error}\n"),
        }
    }
}

fn write_checked_annotation(
    script: &str,
    replacement: &str,
    source_id: SourceId,
    sources: &SourceMap,
    module_roots: &[PathBuf],
    options: CheckOptions,
) -> Result<(), AnnotationWriteError> {
    // Rewrites are new source. Keep the entry's source identity and import
    // resolution context, then validate semantic and execution boundaries before
    // writing any bytes. Facts from the original text cannot validate new spans.
    let mut rewritten_sources = SourceMap::new();
    for source in sources.files() {
        let text = if source.id() == source_id { replacement } else { source.text() };
        let retained_id = rewritten_sources.add_file(source.name(), text);
        debug_assert_eq!(retained_id, source.id());
    }
    let parsed = loader::parse_load_entry_source_shared_arena_only(script, source_id, &mut rewritten_sources, module_roots.to_vec());
    if !parsed.diagnostics.is_empty() {
        return Err(AnnotationWriteError::Rejected(DiagnosticRenderer::new().render(&parsed.diagnostics, &rewritten_sources)));
    }
    let checked = Checker::check_arena_with_options(&parsed.arena, replacement, options);
    if !checked.diagnostics.is_empty() {
        return Err(AnnotationWriteError::Rejected(DiagnosticRenderer::new().render(&checked.diagnostics, &rewritten_sources)));
    }
    let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    let diagnostics = Evaluator::compact_lowerability_diagnostics_with_parts(
        &parsed.arena,
        source_id,
        rewritten_sources.clone(),
        declarations,
        bodies,
        Vec::new(),
        xsh::execution::script::script_command_name(script),
    );
    if !diagnostics.is_empty() {
        return Err(AnnotationWriteError::Rejected(DiagnosticRenderer::new().render(&diagnostics, &rewritten_sources)));
    }
    fs::write(script, replacement).map_err(AnnotationWriteError::Write)
}

fn formatter_line_width_for_script(
    script: &str,
    fallback_config: &XshConfig,
) -> Result<usize, String> {
    Ok(config_for_file(script, fallback_config)?.line_width())
}

#[allow(clippy::single_call_fn)]
fn annotation_edits(
    checked: &CheckOutput,
    policy: AnnotationPolicy,
    target_source: SourceId,
    source: &str,
) -> Vec<(usize, usize, String)> {
    let mut edits = Vec::new();
    let query = xsh::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    for fact in &checked.annotation_facts {
        if matches!(
            fact.kind,
            AnnotationFactKind::Binding { .. } | AnnotationFactKind::DefaultedParam { .. }
        ) && matches!(fact.ty, xsh::frontend::check::Type::Unit)
        {
            continue;
        }
        let Ok(view) = query.type_view(&fact.ty, None) else {
            continue;
        };
        let Some(ty) = view.annotation_source() else {
            continue;
        };
        match &fact.kind {
            AnnotationFactKind::Binding {
                span,
                initializer,
                exported,
            } => {
                if (*exported && !policy.exports) || (!*exported && !policy.locals) {
                    continue;
                }
                if span.source_id != target_source || initializer.source_id != target_source {
                    continue;
                }
                let end = initializer.start().min(source.len());
                let start = span.start().min(end);
                if let Some(offset) = source[start..end].rfind('=').map(|offset| start + offset) {
                    edits.push((offset, offset, format!(": {ty} ")));
                }
            }
            AnnotationFactKind::DefaultedParam { span, default } => {
                if !policy.params {
                    continue;
                }
                if span.source_id != target_source || default.source_id != target_source {
                    continue;
                }
                let end = default.start().min(source.len());
                let start = span.start().min(end);
                if let Some(offset) = source[start..end].rfind('=').map(|offset| start + offset) {
                    edits.push((offset, offset, format!(": {ty} ")));
                }
            }
            AnnotationFactKind::InferredPureReturn { body } | AnnotationFactKind::ExportedProcReturn { body } => {
                if policy.returns && body.source_id == target_source {
                    edits.push((body.start(), body.start(), format!(" -> {ty} ")));
                }
            }
        }
    }
    edits.sort_unstable_by_key(|(start, end, _)| Reverse((*start, *end)));
    edits.dedup_by_key(|(start, end, _)| (*start, *end));
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotation_edits_accept_closed_nested_graph_leaves_and_refuse_generic_graphs() {
        use xsh::frontend::check::Type;
        let source = "pure identity(value) { value }\nlet result = [identity(7)]\n";
        let source_id = SourceId::new(2);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        checked.annotation_facts.retain(|fact| matches!(fact.kind, AnnotationFactKind::Binding { .. }));
        assert_eq!(checked.annotation_facts.len(), 1);
        let call = *checked.solved.calls.keys().next().unwrap();
        let ground = checked.solved.expressions[&call];
        checked.annotation_facts[0].ty = Type::List(Box::new(Type::Graph(ground)));
        let edits = annotation_edits(&checked, AnnotationPolicy::with_locals(), source_id, source);
        assert_eq!(edits.len(), 1, "{edits:?}");
        assert_eq!(edits[0].2, ": List[Int] ");
        let mut rewritten = source.to_string();
        for (start, end, replacement) in edits { rewritten.replace_range(start..end, &replacement); }
        let reparsed = Parser::parse_source_arena_only(source_id, &rewritten);
        assert!(reparsed.diagnostics.is_empty(), "{:?}", reparsed.diagnostics);
        let rewritten_check = Checker::check_arena(&reparsed.arena, &rewritten);
        assert!(rewritten_check.diagnostics.is_empty(), "{:?}", rewritten_check.diagnostics);
        let generic = checked.solved.declarations.values().next().expect("generic identity declaration").signature;
        for unsupported in [Type::List(Box::new(Type::Graph(generic))), Type::Any, Type::Record(BTreeMap::new())] {
            checked.annotation_facts[0].ty = unsupported;
            assert!(annotation_edits(&checked, AnnotationPolicy::with_locals(), source_id, source).is_empty());
        }
    }

    #[test]
    fn annotation_write_rejects_semantically_invalid_source_without_changing_bytes() {
        let root = std::env::temp_dir().join(format!("xsh-invalid-annotation-{}", std::process::id()));
        fs::create_dir_all(&root).expect("create annotation fixture");
        let script = root.join("entry.xsh");
        let path = script.to_str().unwrap();
        let original = "let value = 42\n";
        fs::write(&script, original).unwrap();
        let mut sources = SourceMap::new();
        sources.add_file("<preceding-source>", "let unrelated = true\n");
        let source_id = sources.add_file(path, original);
        let outcome = write_checked_annotation(path, "let value: Bool = 42\n", source_id, &sources, &[], CheckOptions::default());
        let unchanged = fs::read_to_string(&script).unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(matches!(outcome, Err(AnnotationWriteError::Rejected(_))), "{outcome:?}");
        assert_eq!(unchanged, original);
    }

    #[test]
    fn annotation_write_retains_entry_source_identity_and_configured_module_roots() {
        let root = std::env::temp_dir().join(format!("xsh-imported-annotation-{}", std::process::id()));
        let modules = root.join("modules");
        fs::create_dir_all(&modules).unwrap();
        fs::write(modules.join("helper.xsh"), "##! Text helper.\n## Render a value.\nexport pure render(value: Str = \"module\") -> Str { value }\n").unwrap();
        let script = root.join("entry.xsh");
        let path = script.to_str().unwrap();
        let original = "use helper\nlet value = helper.render()\n";
        let replacement = "use helper\nlet value: Str = helper.render()\n";
        fs::write(&script, original).unwrap();
        let mut sources = SourceMap::new();
        sources.add_file("<preceding-source>", "let unrelated = true\n");
        let source_id = sources.add_file(path, original);
        let outcome = write_checked_annotation(path, replacement, source_id, &sources, &[modules], CheckOptions { migration_diagnostics: true, ..CheckOptions::default() });
        let written = fs::read_to_string(&script).unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(written, replacement);
    }
}
