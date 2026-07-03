use crate::xsht::cli::{
    CliOutput, XshConfig, cancellation_output, collect_configured_xsh_files, collect_xsh_files,
    load_config, text_bytes,
};
use crate::xsht::config::config_for_file;
use crate::xsht::format::Formatter;
use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};
use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label};
use xsh::loader::{self, parse_load_check_file};
use xsh::runtime::eval::Evaluator;
use xsh::sema::check::{AnnotationFact, AnnotationFactKind, CheckOptions, Checker};
use xsh::source::{SourceId, SourceMap, Span};
use xsh::syntax::parser::Parser;

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
        false,
        None,
        &[],
        XshConfig::default().format.line_width,
    )
}

pub fn check_paths_with_options(
    paths: &[String],
    strict_dynamic: bool,
    annotation_selection: Option<AnnotationSelection>,
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
    let module_roots: Vec<PathBuf> = config.module_path.iter().map(PathBuf::from).collect();
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
                if let Err(message) = collect_xsh_files(path, &config.exclude, &mut files) {
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
        strict_dynamic,
        reveal_types: true,
        migration_diagnostics: true,
    };

    let mut sources = SourceMap::new();
    let mut source_ids: rustc_hash::FxHashMap<String, SourceId> = rustc_hash::FxHashMap::default();

    let mut status = 0;
    let mut stderr = String::new();
    let mut seen_diagnostics: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    let mut checked_files: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    for file in files {
        if let Some(output) = cancellation_output() {
            return output;
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

        let line_width = match formatter_line_width_for_script(&path_str, &config) {
            Ok(line_width) => line_width,
            Err(message) => {
                status = 2;
                stderr.push_str(&format!("xsht: {message}\n"));
                continue;
            }
        };

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
                        stderr.push_str(&DiagnosticRenderer::new().render(&diagnostics, &sources));
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
            }
            status = 2;
            continue;
        }

        let entry_text = sources.get(source_id).map(|s| s.text()).unwrap_or("");
        let checked = Checker::check_arena_with_options(&parsed.arena, entry_text, check_options);
        if !checked.diagnostics.is_empty() {
            let new_diags: Vec<_> = checked
                .diagnostics
                .iter()
                .filter(|d| seen_diagnostics.insert(diagnostic_key(d, &sources)))
                .cloned()
                .collect();
            if !new_diags.is_empty() {
                stderr.push_str(&DiagnosticRenderer::new().render(&new_diags, &sources));
            }
            status = 2;
            continue;
        }

        let mut type_stderr = DiagnosticRenderer::new().render(&checked.reveal_types, &sources);

        let diagnostics = Evaluator::compact_lowerability_diagnostics(
            &parsed.arena,
            source_id,
            sources.clone(),
            Vec::new(),
            xsh::runner::script_command_name(&path_str),
        );
        if !diagnostics.is_empty() {
            let new_diags: Vec<_> = diagnostics
                .iter()
                .filter(|d| seen_diagnostics.insert(diagnostic_key(d, &sources)))
                .cloned()
                .collect();
            if !new_diags.is_empty() {
                stderr.push_str(&DiagnosticRenderer::new().render(&new_diags, &sources));
            }
            status = 2;
            continue;
        }

        if let Some(annotation_policy) = annotation_policy {
            let Some(original) = sources.get(source_id).map(|s| s.text().to_string()) else {
                status = 2;
                stderr.push_str("xsht: missing script source\n");
                continue;
            };
            let edits = annotation_edits(
                &checked.annotation_facts,
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
                        &DiagnosticRenderer::new().render(&reformatted.diagnostics, &fmt_sources),
                    );
                    status = 2;
                    continue;
                }
                if reformatted.formatted != original
                    && let Err(err) = fs::write(&path_str, &reformatted.formatted)
                {
                    stderr.push_str(&format!("xsht: failed to write '{path_str}': {err}\n"));
                    status = 4;
                    continue;
                }
            }
        }

        if !type_stderr.is_empty() && !type_stderr.ends_with('\n') {
            type_stderr.push('\n');
        }
        stderr.push_str(&type_stderr);
    }

    CliOutput {
        status,
        stdout: Vec::new(),
        stderr: text_bytes(stderr),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

pub fn check_script_with_options(script: &str, strict_dynamic: bool, annotate: bool) -> CliOutput {
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
        strict_dynamic,
        annotation_policy,
        &module_roots,
        line_width,
    )
}

fn check_one_script(
    script: &str,
    strict_dynamic: bool,
    annotation_policy: Option<AnnotationPolicy>,
    module_roots: &[PathBuf],
    line_width: usize,
) -> CliOutput {
    let checked_program = match parse_load_check_file(
        script,
        module_roots.to_vec(),
        CheckOptions {
            interactive_commands: None,
            strict_dynamic,
            reveal_types: true,
            migration_diagnostics: true,
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

    let diagnostics = Evaluator::compact_lowerability_diagnostics(
        &checked_program.parsed.arena,
        checked_program.entry_source_id,
        checked_program.sources.clone(),
        Vec::new(),
        xsh::runner::script_command_name(script),
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
            &checked.annotation_facts,
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

fn formatter_line_width_for_script(
    script: &str,
    fallback_config: &XshConfig,
) -> Result<usize, String> {
    Ok(config_for_file(script, fallback_config)?.line_width())
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
        ) && matches!(fact.ty, xsh::sema::types::Type::Unit)
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
            AnnotationFactKind::ExportedProcReturn { body } => {
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
