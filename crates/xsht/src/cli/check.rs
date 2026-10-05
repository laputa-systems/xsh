use crate::xsht::cli::timing::{Stage, StageTimings};
use crate::xsht::cli::{
    CliOutput, ConfigCache, DiscoveryFor, XshConfig, cancellation_output, discover_scripts,
    load_config, text_bytes,
};
use crate::xsht::config::config_for_file;
use crate::xsht::format::Formatter;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticRenderer, Label, LabelStyle};
use xsh::execution::evaluator::Evaluator;
use xsh::frontend::check::{AnnotationFact, AnnotationFactKind, CheckOptions, Checker};
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
    pub(crate) fn annotates_returns(self) -> bool {
        self.returns
    }

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
    check_one_script(script, None, &[], XshConfig::default().format.line_width)
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
    check_paths_timed(paths, annotation_selection, summary, &StageTimings::start())
}

/// `xsht check` over files or directories, recording stage times in `timings`.
pub fn check_paths_timed(
    paths: &[String],
    annotation_selection: Option<AnnotationSelection>,
    summary: bool,
    timings: &StageTimings,
) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }

    let discover_started = Instant::now();

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
    let mut files = match discover_scripts(paths, &config, &ConfigCache::default(), DiscoveryFor::Scripts) {
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
    let mut summary_counts = CheckSummary::default();
    let mut status = 0;
    let mut stderr = String::new();
    let mut checked_files: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    files.retain(|file| {
        let canonical = file
            .canonicalize()
            .unwrap_or_else(|_| file.clone())
            .to_string_lossy()
            .into_owned();
        checked_files.insert(canonical)
    });
    timings.record(Stage::Discover, discover_started.elapsed());

    // Annotation rewrites files that later entries import, so each entry must
    // finish before the next one loads.
    let worker_count = if annotation_selection.is_some() {
        1
    } else {
        std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .min(files.len().max(1))
    };
    let next_file = AtomicUsize::new(0);
    let mut reports = std::thread::scope(|scope| {
        let (report_tx, report_rx) = crossbeam_channel::unbounded();
        for _ in 0..worker_count {
            let report_tx = report_tx.clone();
            let (files, next_file) = (&files, &next_file);
            std::thread::Builder::new()
                .name("xsht-check".to_string())
                .stack_size(super::FRONTEND_WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    loop {
                        if cancellation_output().is_some() {
                            break;
                        }
                        let file_index = next_file.fetch_add(1, Ordering::Relaxed);
                        let Some(file) = files.get(file_index) else {
                            break;
                        };
                        let report =
                            check_entry(&file.to_string_lossy(), annotation_selection, timings);
                        if report_tx.send((file_index, report)).is_err() {
                            break;
                        }
                    }
                })
                .expect("spawn checker worker");
        }
        drop(report_tx);
        report_rx.iter().collect::<Vec<_>>()
    });
    if let Some(output) = cancellation_output() {
        return output;
    }
    assert_eq!(reports.len(), files.len(), "checker worker panicked");

    // Entries are checked in any order and reported in file order, so the
    // output and the cross-file deduplication never depend on scheduling.
    reports.sort_unstable_by_key(|(file_index, _)| *file_index);
    let mut seen_diagnostics: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    let mut write_new_diagnostics =
        |stderr: &mut String, diagnostics: &[Diagnostic], sources: &SourceMap| {
            let new_diags: Vec<_> = diagnostics
                .iter()
                .filter(|d| seen_diagnostics.insert(diagnostic_key(d, sources)))
                .cloned()
                .collect();
            if !new_diags.is_empty() {
                stderr.push_str(&DiagnosticRenderer::new().render(&new_diags, sources));
                summary_counts.observe_diagnostics(&new_diags, sources);
            }
        };
    for (_, report) in &reports {
        for part in &report.output {
            match part {
                EntryOutput::Text(text) => stderr.push_str(text),
                EntryOutput::Diagnostics(diagnostics) => {
                    write_new_diagnostics(&mut stderr, diagnostics, &report.sources);
                }
            }
        }
        if let Some(entry_status) = report.status {
            status = entry_status;
        }
    }
    for (_, report) in &reports {
        if !report.lowering_diagnostics.is_empty() {
            write_new_diagnostics(&mut stderr, &report.lowering_diagnostics, &report.sources);
            status = 2;
        }
    }

    if summary {
        summary_counts.write_to(&mut stderr);
    }
    timings.set_files(checked_files.len());

    CliOutput {
        status,
        stdout: Vec::new(),
        stderr: text_bytes(stderr),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

/// One part of an entry's stderr, in the order the entry produced it.
enum EntryOutput {
    Text(String),
    /// Rendered unless an earlier entry already reported the same diagnostic.
    Diagnostics(Vec<Diagnostic>),
}

/// What checking one entry file contributes to `xsht check`. Its spans index
/// `sources`, which holds only this entry and the modules it loads.
struct EntryReport {
    output: Vec<EntryOutput>,
    /// Reported after every entry's own output.
    lowering_diagnostics: Vec<Diagnostic>,
    /// The exit status this entry asks for; a later entry's request replaces it.
    status: Option<u8>,
    sources: SourceMap,
}

impl EntryReport {
    fn failed(mut self, status: u8, output: EntryOutput) -> Self {
        self.output.push(output);
        self.status = Some(status);
        self
    }
}

/// Load, check, and lower one entry file. With an annotation selection, also
/// write the inferred annotations back to the file; a selection that defers
/// to configuration reads `[check] annotate` from the file's own config.
fn check_entry(
    path_str: &str,
    annotation_selection: Option<AnnotationSelection>,
    timings: &StageTimings,
) -> EntryReport {
    let mut report = EntryReport {
        output: Vec::new(),
        lowering_diagnostics: Vec::new(),
        status: None,
        sources: SourceMap::new(),
    };
    let file_config = match config_for_file(path_str) {
        Ok(file_config) => file_config,
        Err(message) => {
            return report.failed(2, EntryOutput::Text(format!("xsht: {message}\n")));
        }
    };
    let annotation_policy = match annotation_selection {
        None => None,
        Some(AnnotationSelection::Policy(policy)) => Some(policy),
        Some(AnnotationSelection::Configured) => {
            match configured_annotation_policy(&file_config.config) {
                Ok(policy) => Some(policy),
                Err(message) => {
                    return report.failed(2, EntryOutput::Text(format!("xsht: {message}\n")));
                }
            }
        }
    };
    let line_width = file_config.line_width();
    let module_roots = file_config.module_roots();

    let load_started = Instant::now();
    let bytes = match fs::read(path_str) {
        Ok(bytes) => bytes,
        Err(err) => {
            return report.failed(
                2,
                EntryOutput::Text(format!("xsh: failed to read '{path_str}': {err}\n")),
            );
        }
    };
    let source_id = match report
        .sources
        .add_file_from_utf8(path_str.to_string(), bytes.clone())
    {
        Ok(id) => id,
        Err(error) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let sid = report.sources.add_file(path_str.to_string(), text);
            let offset = error
                .offset
                .min(report.sources.get(sid).map_or(0, |s| s.len()));
            let diagnostics = vec![
                Diagnostic::error("source file is not valid UTF-8")
                    .with_code(DiagnosticCode::SourceInvalidUtf8)
                    .with_label(Label::primary(
                        Span::new(sid, offset, offset),
                        "invalid UTF-8 starts here",
                    )),
            ];
            let rendered = DiagnosticRenderer::new().render(&diagnostics, &report.sources);
            return report.failed(2, EntryOutput::Text(rendered));
        }
    };
    drop(bytes);

    let parsed = loader::parse_load_entry_source_shared_arena_only(
        path_str,
        source_id,
        &mut report.sources,
        module_roots,
    );
    timings.record(Stage::Load, load_started.elapsed());
    if !parsed.diagnostics.is_empty() {
        return report.failed(2, EntryOutput::Diagnostics(parsed.diagnostics));
    }

    // This one check renders the diagnostics and supplies the lowering facts.
    let arena = Arc::new(parsed.arena);
    let entry_text = report.sources.get(source_id).map(|s| s.text()).unwrap_or("");
    let mut checked = timings.time(Stage::Check, || {
        Checker::check_arena_with_options_and_type_program(
            &arena,
            entry_text,
            CheckOptions {
                interactive_commands: None,
                reveal_types: true,
                migration_diagnostics: true,
                embedded_bodies: true,
            },
            Arc::clone(&arena),
        )
    });
    if !checked.diagnostics.is_empty() {
        return report.failed(2, EntryOutput::Diagnostics(checked.diagnostics));
    }

    let mut type_stderr = DiagnosticRenderer::new().render(&checked.reveal_types, &report.sources);
    let annotation_facts = std::mem::take(&mut checked.annotation_facts);
    let lowering_diagnostics = timings.time(Stage::Lower, || {
        Evaluator::compact_lowerability_diagnostics_with_parts(
            &arena,
            source_id,
            report.sources.clone(),
            Checker::compact_declarations(&arena, checked),
            Vec::new(),
            xsh::execution::script::script_command_name(path_str),
        )
    });

    if let Some(annotation_policy) = annotation_policy {
        if !lowering_diagnostics.is_empty() {
            return report.failed(2, EntryOutput::Diagnostics(lowering_diagnostics));
        }
        let Some(original) = report.sources.get(source_id).map(|s| s.text().to_string()) else {
            return report.failed(2, EntryOutput::Text("xsht: missing script source\n".into()));
        };
        let edits = annotation_edits(&annotation_facts, annotation_policy, source_id, &original);
        if !edits.is_empty() {
            let mut annotated = original.clone();
            for (start, end, replacement) in edits {
                annotated.replace_range(start..end, &replacement);
            }

            let mut fmt_sources = SourceMap::new();
            let fmt_id = fmt_sources.add_file(path_str, annotated.clone());
            let reformatted = Formatter::new()
                .with_line_width(line_width)
                .format_source(fmt_id, &annotated);
            if !reformatted.diagnostics.is_empty() {
                let rendered =
                    DiagnosticRenderer::new().render(&reformatted.diagnostics, &fmt_sources);
                return report.failed(2, EntryOutput::Text(rendered));
            }
            if reformatted.formatted != original
                && let Err(err) = fs::write(path_str, &reformatted.formatted)
            {
                return report.failed(
                    4,
                    EntryOutput::Text(format!("xsht: failed to write '{path_str}': {err}\n")),
                );
            }
        }
    } else {
        report.lowering_diagnostics = lowering_diagnostics;
    }

    if !type_stderr.is_empty() && !type_stderr.ends_with('\n') {
        type_stderr.push('\n');
    }
    report.output.push(EntryOutput::Text(type_stderr));
    report
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
                .map_or("diagnostic.uncoded", DiagnosticCode::name)
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
    let failed = |message: String| CliOutput {
        status: 2,
        stdout: Vec::new(),
        stderr: text_bytes(format!("xsht: {message}\n")),
        trace_text: String::new(),
        syscall_summary: None,
    };
    let file_config = match config_for_file(script) {
        Ok(file_config) => file_config,
        Err(message) => return failed(message),
    };
    let annotation_policy = if annotate {
        match configured_annotation_policy(&file_config.config) {
            Ok(policy) => Some(policy),
            Err(message) => return failed(message),
        }
    } else {
        None
    };
    check_one_script(
        script,
        annotation_policy,
        &file_config.module_roots(),
        file_config.line_width(),
    )
}

fn check_one_script(
    script: &str,
    annotation_policy: Option<AnnotationPolicy>,
    module_roots: &[PathBuf],
    line_width: usize,
) -> CliOutput {
    // This one check renders the diagnostics and supplies the lowering facts.
    let mut checked_program = match parse_load_check_file(
        script,
        module_roots.to_vec(),
        CheckOptions {
            interactive_commands: None,
            reveal_types: true,
            migration_diagnostics: true,
            embedded_bodies: true,
        },
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

    if !checked_program.check_diagnostics().is_empty() {
        return CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(checked_program.render_check_diagnostics()),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }
    let mut checked = checked_program
        .checked
        .take()
        .expect("checked program after clean parse");

    let mut stderr =
        DiagnosticRenderer::new().render(&checked.reveal_types, &checked_program.sources);
    let annotation_facts = std::mem::take(&mut checked.annotation_facts);

    let declarations = Checker::compact_declarations(&checked_program.parsed.arena, checked);
    let diagnostics = Evaluator::compact_lowerability_diagnostics_with_parts(
        &checked_program.parsed.arena,
        checked_program.entry_source_id,
        checked_program.sources.clone(),
        declarations,
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
            &annotation_facts,
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
                && let Err(err) = fs::write(script, &reformatted.formatted)
            {
                return CliOutput {
                    status: 4,
                    stdout: Vec::new(),
                    stderr: text_bytes(format!("xsht: failed to write '{script}': {err}\n")),
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
            diagnostic.code.map_or("", DiagnosticCode::name),
            diagnostic.message,
            loc.file,
            span.start(),
            span.end()
        ),
        None => format!(
            "{:?}:{}:{}",
            diagnostic.severity,
            diagnostic.code.map_or("", DiagnosticCode::name),
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

#[allow(clippy::single_call_fn)]
fn annotation_edits(
    facts: &[AnnotationFact],
    policy: AnnotationPolicy,
    target_source: SourceId,
    source: &str,
) -> Vec<(usize, usize, String)> {
    let mut edits = Vec::new();
    for fact in facts {
        if matches!(
            fact.kind,
            AnnotationFactKind::Binding { .. } | AnnotationFactKind::DefaultedParam { .. }
        ) && matches!(fact.ty, xsh::frontend::check::Type::Unit)
        {
            continue;
        }
        let Some(ty) = fact.ty.annotation_source() else {
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
            AnnotationFactKind::InferredPureReturn { body }
            | AnnotationFactKind::ExportedProcReturn { body } => {
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
