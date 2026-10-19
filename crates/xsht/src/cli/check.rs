use crate::xsht::cli::timing::{Stage, StageTimings};
use crate::xsht::cli::{
    CliOutput, ConfigCache, DiscoveryFor, cancellation_output, discover_scripts,
    load_config, text_bytes,
};
use crate::xsht::config::config_for_file;
use crate::xsht::format::Formatter;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticRenderer, Label, LabelStyle};
use xsh::execution::evaluator::Evaluator;
use xsh::frontend::check::{
    AnnotationFact, AnnotationFactKind, CheckOptions, CheckOutput, CheckSession, Checker,
};
use xsh::frontend::load as loader;
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
    let mut files = match discover_scripts(
        paths,
        &config,
        &ConfigCache::default(),
        DiscoveryFor::Scripts,
    ) {
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
    let shared_workspace = if annotation_selection.is_none() {
        let load_started = Instant::now();
        let workspace = load_check_workspace(&files);
        timings.record(Stage::Load, load_started.elapsed());
        if let Some(output) = cancellation_output() {
            return output;
        }
        workspace
    } else {
        None
    };
    let next_file = AtomicUsize::new(0);
    let mut reports = std::thread::scope(|scope| {
        let (report_tx, report_rx) = crossbeam_channel::unbounded();
        for _ in 0..worker_count {
            let report_tx = report_tx.clone();
            let (files, next_file) = (&files, &next_file);
            let shared_workspace = shared_workspace.as_ref();
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
                        let report = check_entry(
                            &file.to_string_lossy(),
                            annotation_selection,
                            timings,
                            shared_workspace,
                        );
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

struct CheckWorkspace {
    sources: Arc<SourceMap>,
    program: Arc<xsh::frontend::syntax::arena::ArenaProgram>,
    session: CheckSession,
    entries: rustc_hash::FxHashMap<String, loader::LoadedArenaEntry>,
}

fn load_check_workspace(files: &[std::path::PathBuf]) -> Option<CheckWorkspace> {
    let mut inputs = Vec::new();
    for file in files {
        let path = file.to_string_lossy().into_owned();
        let Ok(config) = config_for_file(&path) else {
            continue;
        };
        let Ok(bytes) = fs::read(file) else {
            continue;
        };
        inputs.push(loader::ArenaEntryInput {
            file: path,
            bytes,
            module_roots: config.module_roots(),
        });
    }
    if inputs.is_empty() {
        return None;
    }
    let loaded = loader::parse_load_entries_shared_arena_only(inputs);
    let session = CheckSession::new(Arc::clone(&loaded.program), Arc::clone(&loaded.sources));
    Some(CheckWorkspace {
        sources: loaded.sources,
        program: loaded.program,
        session,
        entries: loaded
            .entries
            .into_iter()
            .map(|entry| (entry.file.clone(), entry))
            .collect(),
    })
}

/// What checking one entry file contributes to `xsht check`. Its spans index
/// `sources`, which holds only this entry and the modules it loads.
struct EntryReport {
    output: Vec<EntryOutput>,
    /// Reported after every entry's own output.
    lowering_diagnostics: Vec<Diagnostic>,
    /// The exit status this entry asks for; a later entry's request replaces it.
    status: Option<u8>,
    sources: Arc<SourceMap>,
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
    workspace: Option<&CheckWorkspace>,
) -> EntryReport {
    let report_sources = workspace
        .map(|workspace| Arc::clone(&workspace.sources))
        .unwrap_or_else(|| Arc::new(SourceMap::new()));
    let mut report = EntryReport {
        output: Vec::new(),
        lowering_diagnostics: Vec::new(),
        status: None,
        sources: report_sources,
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
    let loaded_entry = workspace.and_then(|workspace| workspace.entries.get(path_str));
    let (arena, source_id, statements) = if let Some(entry) = loaded_entry {
        if !entry.diagnostics.is_empty() {
            return report.failed(2, EntryOutput::Diagnostics(entry.diagnostics.clone()));
        }
        (
            Arc::clone(&workspace.expect("loaded entry has a workspace").program),
            entry.source_id,
            entry.statements,
        )
    } else {
        let bytes = match fs::read(path_str) {
            Ok(bytes) => bytes,
            Err(err) => {
                return report.failed(
                    2,
                    EntryOutput::Text(format!("xsh: failed to read '{path_str}': {err}\n")),
                );
            }
        };
        let mut sources = SourceMap::new();
        let (source_id, source_diagnostics) =
            loader::add_source_bytes(&mut sources, path_str, bytes);
        if !source_diagnostics.is_empty() {
            let rendered = DiagnosticRenderer::new().render(&source_diagnostics, &sources);
            report.sources = Arc::new(sources);
            return report.failed(2, EntryOutput::Text(rendered));
        }
        let parsed = loader::parse_load_entry_source_shared_arena_only(
            path_str,
            source_id,
            &mut sources,
            module_roots,
        );
        report.sources = Arc::new(sources);
        if !parsed.diagnostics.is_empty() {
            return report.failed(2, EntryOutput::Diagnostics(parsed.diagnostics));
        }
        let arena = Arc::new(parsed.arena);
        let statements = arena.statements;
        (arena, source_id, statements)
    };
    if loaded_entry.is_none() {
        timings.record(Stage::Load, load_started.elapsed());
    }

    // This one check renders the diagnostics and supplies the lowering facts.
    let local_session = (workspace.is_none() || loaded_entry.is_none())
        .then(|| CheckSession::new(Arc::clone(&arena), Arc::clone(&report.sources)));
    let session = local_session
        .as_ref()
        .or_else(|| workspace.map(|workspace| &workspace.session))
        .expect("a check session is available for the entry");
    let view = session.view_for_root(statements, source_id);
    let mut checked = timings.time(Stage::Check, || {
        session.check_view(&view, CheckOptions::for_check_cli())
    });
    if !checked.diagnostics.is_empty() {
        return report.failed(2, EntryOutput::Diagnostics(checked.diagnostics));
    }

    let mut type_stderr = DiagnosticRenderer::new().render(&checked.reveal_types, &report.sources);
    let annotation_facts = std::mem::take(&mut checked.annotation_facts);
    let annotation_plan = annotation_policy.map(|policy| {
        report.sources.get(source_id).map(|source| {
            let original = source.text().to_string();
            let edits = annotation_edits(
                &checked,
                &annotation_facts,
                policy,
                source_id,
                &original,
            );
            (original, edits)
        })
    });
    let lowering_diagnostics = timings.time(Stage::Lower, || {
        Evaluator::compact_lowerability_diagnostics_with_view(
            &view,
            source_id,
            Arc::clone(&report.sources),
            Checker::compact_declarations(view.program(), checked),
            Vec::new(),
            xsh::execution::script::script_command_name(path_str),
        )
    });

    if annotation_policy.is_some() {
        if !lowering_diagnostics.is_empty() {
            return report.failed(2, EntryOutput::Diagnostics(lowering_diagnostics));
        }
        let Some((original, edits)) = annotation_plan.flatten() else {
            return report.failed(2, EntryOutput::Text("xsht: missing script source\n".into()));
        };
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
    checked: &CheckOutput,
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
        let annotation_span = match &fact.kind {
            AnnotationFactKind::Binding { initializer, .. } => Some(*initializer),
            AnnotationFactKind::DefaultedParam { default, .. } => Some(*default),
            AnnotationFactKind::InferredPureReturn { .. }
            | AnnotationFactKind::ExportedProcReturn { .. } => None,
        };
        let ty = if let Some(span) = annotation_span {
            if let Some(contract) = checked.callable_contract_at(span) {
                let Some(annotation) = contract.source_annotation() else {
                    continue;
                };
                annotation
            } else {
                let Some(annotation) = fact.ty.annotation_source() else {
                    continue;
                };
                annotation
            }
        } else {
            let Some(annotation) = fact.ty.annotation_source() else {
                continue;
            };
            annotation
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
