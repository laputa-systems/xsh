use crate::xsht::cli::timing::{Stage, StageTimings};
use crate::xsht::cli::{
    CliOutput, XshConfig, cancellation_output, collect_configured_xsh_files, collect_xsh_files_below,
    is_path_excluded, load_config, nearest_config_for_file, text_bytes,
};
use crate::xsht::config::{FileToolConfig, config_for_dir};
use crate::xsht::edit::{
    SourceEdit, apply_cst_guarded_edits, apply_cst_guarded_migration_edits, migration_lint_code,
};
use crate::xsht::lint::{LintOptions, Linter, lint_code_selected};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use xsh::diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticFamily, DiagnosticRenderer, Label, Severity,
};
use xsh::frontend::check::CheckOptions;
use xsh::frontend::load::{module_key, parse_load_check_text, resolve_user_module};
use xsh::frontend::source::{SourceId, SourceMap, Span};
use xsh::frontend::symbols::{Name, SymbolOwner};
use xsh::frontend::syntax::arena::{
    ArenaProgram, ArenaProgramBuilder, ArenaRange, ArenaStmtKind, StmtId, UseStmtId,
};
use xsh::frontend::syntax::grouping::grouping_diagnostics;
use xsh::frontend::syntax::parser::Parser;
pub fn lint_files(
    files: &[String],
    fix: bool,
    runless: bool,
    only: Option<Vec<DiagnosticCode>>,
) -> CliOutput {
    lint_files_timed(files, fix, runless, only, &StageTimings::start())
}

/// `xsht lint` over files or directories, recording stage times in `timings`.
pub fn lint_files_timed(
    files: &[String],
    fix: bool,
    runless: bool,
    only: Option<Vec<DiagnosticCode>>,
    timings: &StageTimings,
) -> CliOutput {
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

    let discovery = timings.time(Stage::Discover, || discover_lint_files(files, &cwd_config));
    let mut discovered = match discovery {
        Ok(discovered) => discovered,
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

    discovered.only = only;
    let config_cache = ConfigCache::default();
    let mut results = lint_workspace(
        &discovered,
        fix,
        runless,
        &cwd_config,
        &config_cache,
        timings,
    );
    if let Some(output) = cancellation_output() {
        return output;
    }
    timings.set_files(discovered.files.len());
    results.sort_unstable_by_key(|result| result.index);
    let mut seen_diagnostics = FxHashSet::default();
    let mut written_files = FxHashSet::default();
    for result in results {
        match result.kind {
            LintResultKind::Clean => {}
            LintResultKind::ReadError(message) => {
                status = 2;
                stderr.push_str(&message);
            }
            LintResultKind::FixDiagnostics {
                status: result_status,
                diagnostics,
                stderr: result_stderr,
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
                diagnostics,
                stderr: result_stderr,
            } => {
                if result_status > status {
                    status = result_status;
                }
                for diagnostic in diagnostics {
                    if seen_diagnostics.insert(diagnostic.key) {
                        stderr.push_str(&diagnostic.text);
                    }
                }
                stderr.push_str(&result_stderr);
                if !written_files.insert(file.clone()) {
                    continue;
                }
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

struct LintDiscovery {
    files: Vec<String>,
    explicit_roots: FxHashSet<String>,
    only: Option<Vec<DiagnosticCode>>,
}

fn discover_lint_files(files: &[String], config: &XshConfig) -> Result<LintDiscovery, String> {
    let mut paths = Vec::new();
    let mut explicit_roots = FxHashSet::default();
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
                let dir_config = config_for_dir(path, config)?;
                collect_xsh_files_below(
                    path,
                    &dir_config.config_dir,
                    &dir_config.config.exclude,
                    &mut paths,
                )?;
            } else {
                paths.push(path.to_path_buf());
                explicit_roots.insert(module_key(path));
            }
        }
    }
    paths.sort_unstable();
    paths.dedup();
    let files = paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    Ok(LintDiscovery {
        files,
        explicit_roots,
        only: None,
    })
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
        diagnostics: Vec<RenderedDiagnostic>,
        stderr: String,
    },
    Write {
        file: String,
        text: String,
        status: u8,
        diagnostics: Vec<RenderedDiagnostic>,
        stderr: String,
    },
}

struct RenderedDiagnostic {
    key: String,
    text: String,
}

// The final convergence round has already checked the complete import graph.
// Keep its diagnostics with that exact source so publishing it needs no second check.
struct ValidatedFixedText {
    text: String,
    diagnostics: Vec<RenderedDiagnostic>,
}

// Fix-mode validation can revisit imported modules; keep these failures keyed so the
// command-level aggregation emits one diagnostic per source location.
enum FixedTextValidationError {
    Diagnostics(Vec<RenderedDiagnostic>),
}

#[derive(Clone)]
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

#[derive(Clone)]
struct WorkspaceImport {
    use_id: UseStmtId,
    path: Vec<Name>,
    span: Span,
    target: Option<String>,
}

struct WorkspaceModule {
    key: String,
    path: PathBuf,
    source_id: SourceId,
    text: String,
    statements: ArenaRange,
    imports: Vec<WorkspaceImport>,
    diagnostics: Vec<Diagnostic>,
    module_roots: Vec<PathBuf>,
    config: ResolvedLintConfig,
}

/// Parsed source files and their resolved imports for one lint command.
///
/// The arena is shared by every entry bundle. A bundle changes only its root
/// statement range and reachable module list, so imports are parsed and
/// resolved once even when several roots reach the same module.
struct LintWorkspace {
    sources: SourceMap,
    program: ArenaProgram,
    module_indices: FxHashMap<String, usize>,
    type_program: std::sync::OnceLock<Arc<ArenaProgram>>,
    modules: FxHashMap<String, WorkspaceModule>,
    roots: Vec<String>,
    input_errors: Vec<String>,
}

/// Builds the workspace graph while appending every source to one arena.
/// Dependencies are loaded recursively using the language loader's search
/// order, including configured module roots and `XSH_MODULE_PATH`.
struct WorkspaceLoader {
    sources: SourceMap,
    builder: ArenaProgramBuilder<'static>,
    modules: FxHashMap<String, WorkspaceModule>,
    stack: Vec<String>,
    source_overrides: FxHashMap<String, Vec<u8>>,
}

impl WorkspaceLoader {
    fn new() -> Self {
        Self {
            sources: SourceMap::new(),
            builder: ArenaProgramBuilder::with_token_capacity(4096),
            modules: FxHashMap::default(),
            stack: Vec::new(),
            source_overrides: FxHashMap::default(),
        }
    }

    fn load(
        &mut self,
        path: PathBuf,
        bytes: Vec<u8>,
        module_roots: Vec<PathBuf>,
    ) -> Result<String, String> {
        let key = module_key(&path);
        let bytes = self.source_overrides.get(&key).cloned().unwrap_or(bytes);
        if self.modules.contains_key(&key) {
            return Ok(key);
        }

        let display_path = path.to_string_lossy().into_owned();
        let (source_id, text, mut diagnostics) = match self
            .sources
            .add_file_from_utf8(display_path.clone(), bytes.clone())
        {
            Ok(source_id) => {
                let text = self
                    .sources
                    .get(source_id)
                    .expect("source was just inserted")
                    .text()
                    .to_string();
                (source_id, text, Vec::new())
            }
            Err(error) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let source_id = self.sources.add_file(display_path, text.clone());
                let offset = error.offset.min(text.len());
                let diagnostic = Diagnostic::error("source file is not valid UTF-8")
                    .with_code(DiagnosticCode::SourceInvalidUtf8)
                    .with_label(Label::primary(
                        Span::new(source_id, offset, offset),
                        "invalid UTF-8 starts here",
                    ));
                (source_id, text, vec![diagnostic])
            }
        };

        let fragment = Parser::parse_source_into_arena_builder(source_id, &text, &mut self.builder);
        diagnostics.extend(fragment.diagnostics);
        let imports = self
            .builder
            .statement_ids(fragment.statements)
            .into_iter()
            .filter_map(|statement| {
                let (use_id, path, span) = self.builder.use_stmt_for_statement(statement)?;
                Some(WorkspaceImport {
                    use_id,
                    path,
                    span,
                    target: None,
                })
            })
            .collect::<Vec<_>>();

        let name = self.builder.name(&key);
        self.builder
            .push_arena_module(key.clone(), name, fragment.statements);
        self.modules.insert(
            key.clone(),
            WorkspaceModule {
                key: key.clone(),
                path: path.clone(),
                source_id,
                text,
                statements: fragment.statements,
                imports,
                diagnostics,
                module_roots: module_roots.clone(),
                config: ResolvedLintConfig {
                    lint_options: LintOptions::default(),
                    line_width: 0,
                    module_roots,
                },
            },
        );
        self.stack.push(key.clone());

        let import_count = self
            .modules
            .get(&key)
            .map_or(0, |module| module.imports.len());
        for import_index in 0..import_count {
            let (use_id, path, span, roots, importer) = {
                let module = self.modules.get(&key).expect("module was inserted");
                let import = &module.imports[import_index];
                (
                    import.use_id,
                    import.path.clone(),
                    import.span,
                    module.module_roots.clone(),
                    module.path.clone(),
                )
            };
            match resolve_user_module(&importer, &path, &roots) {
                Ok(None) => {}
                Ok(Some((module_path, module_bytes))) => {
                    let target_key = module_key(&module_path);
                    let cycle = self.stack.contains(&target_key);
                    match self.load(module_path, module_bytes, roots) {
                        Ok(target) => {
                            self.builder
                                .set_use_resolved(use_id, std::sync::Arc::from(target.as_str()));
                            if let Some(module) = self.modules.get_mut(&key) {
                                module.imports[import_index].target = Some(target);
                                if cycle {
                                    module.diagnostics.push(
                                        Diagnostic::error("cyclic module import")
                                            .with_code(DiagnosticCode::ParseModuleCycle)
                                            .with_label(Label::primary(
                                                span,
                                                "module import cycle starts here",
                                            )),
                                    );
                                }
                            }
                        }
                        Err(message) => {
                            if let Some(module) = self.modules.get_mut(&key) {
                                module.diagnostics.push(
                                    Diagnostic::error("failed to load module")
                                        .with_code(DiagnosticCode::ParseModuleLoad)
                                        .with_label(Label::primary(span, message)),
                                );
                            }
                        }
                    }
                }
                Err(message) => {
                    if let Some(module) = self.modules.get_mut(&key) {
                        module.diagnostics.push(
                            Diagnostic::error("failed to read module")
                                .with_code(DiagnosticCode::ParseModuleRead)
                                .with_label(Label::primary(span, message)),
                        );
                    }
                }
            }
        }
        self.stack.pop();
        Ok(key)
    }

    fn finish(self) -> (SourceMap, ArenaProgram, FxHashMap<String, WorkspaceModule>) {
        (self.sources, self.builder.finish(), self.modules)
    }
}

fn lint_workspace(
    discovery: &LintDiscovery,
    fix: bool,
    runless: bool,
    cwd_config: &XshConfig,
    config_cache: &ConfigCache,
    timings: &StageTimings,
) -> Vec<LintResult> {
    let available = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    lint_workspace_with_parallelism(
        discovery,
        fix,
        runless,
        cwd_config,
        config_cache,
        available,
        timings,
    )
}

fn lint_workspace_with_parallelism(
    discovery: &LintDiscovery,
    fix: bool,
    runless: bool,
    cwd_config: &XshConfig,
    config_cache: &ConfigCache,
    available: usize,
    timings: &StageTimings,
) -> Vec<LintResult> {
    let load_started = Instant::now();
    let mut loader = WorkspaceLoader::new();
    let mut input_errors = Vec::new();
    let mut candidate_keys = Vec::new();
    for file in &discovery.files {
        // A pending signal ends the command; its caller reports the signal
        // and discards whatever was loaded.
        if cancellation_output().is_some() {
            break;
        }
        let path = PathBuf::from(file);
        let config = match lint_config_for_file(file, runless, cwd_config, config_cache) {
            Ok(config) => config,
            Err(message) => {
                input_errors.push(format!("xsht: {message}\n"));
                continue;
            }
        };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                input_errors.push(format!("xsht: failed to read '{file}': {error}\n"));
                continue;
            }
        };
        match loader.load(path, bytes, config.module_roots.clone()) {
            Ok(key) => candidate_keys.push(key),
            Err(message) => input_errors.push(format!("xsht: {message}\n")),
        }
    }
    candidate_keys.sort_unstable();
    candidate_keys.dedup();

    let (sources, mut program, mut modules) = loader.finish();
    for module in modules.values_mut() {
        if let Ok(config) = lint_config_for_file(
            &module.path.to_string_lossy(),
            runless,
            cwd_config,
            config_cache,
        ) {
            module.config = config;
            module.config.lint_options.only.clone_from(&discovery.only);
        }
    }
    let explicit_roots = discovery
        .explicit_roots
        .iter()
        .filter(|key| modules.contains_key(*key))
        .cloned()
        .collect::<FxHashSet<_>>();
    let roots = select_lint_roots(&candidate_keys, &modules, &explicit_roots);
    program.modules.shrink_to_fit();
    let mut workspace = LintWorkspace::new(sources, program, modules, roots, input_errors);
    timings.record(Stage::Load, load_started.elapsed());

    let mut results = Vec::new();
    let mut index = 0usize;
    for error in workspace.input_errors.drain(..) {
        results.push(LintResult {
            index,
            kind: LintResultKind::ReadError(error),
        });
        index += 1;
    }
    let linted_modules = Mutex::new(FxHashSet::default());
    let next_root = AtomicUsize::new(0);
    let (tx, rx) = crossbeam_channel::unbounded();
    let workers = worker_count_for_parallelism(workspace.roots.len(), available);
    thread::scope(|scope| {
        for _ in 0..workers {
            let next_root = &next_root;
            let tx = tx.clone();
            let linted_modules = &linted_modules;
            let workspace = &workspace;
            thread::Builder::new()
                .name("xsht-lint".to_string())
                .stack_size(super::FRONTEND_WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    // Each worker checks through its own copy of the program.
                    let (mut bundle, type_program) = timings.time(Stage::Load, || {
                        (workspace.program.clone(), workspace.type_program())
                    });
                    loop {
                        if cancellation_output().is_some() {
                            break;
                        }
                        let root_index = next_root.fetch_add(1, Ordering::Relaxed);
                        let Some(root) = workspace.roots.get(root_index) else {
                            break;
                        };
                        let root_results = lint_workspace_root(
                            workspace,
                            root,
                            fix,
                            &mut bundle,
                            linted_modules,
                            &type_program,
                            timings,
                        );
                        if tx.send((root_index, root_results)).is_err() {
                            break;
                        }
                    }
                })
                .expect("spawn lint worker");
        }
    });
    drop(tx);
    let mut grouped = rx.into_iter().collect::<Vec<_>>();
    grouped.sort_unstable_by_key(|(root_index, _)| *root_index);
    for (_, root_results) in grouped {
        for mut result in root_results {
            result.index += index;
            results.push(result);
            index += 1;
        }
    }
    results
}

// A lint traversal owns one source range. Imported declarations retain their
// contracts in the program, while checked expression and statement facts use
// that source's spans. Ordered ranges avoid walking or cloning other files.
fn source_checked_map<T: Clone>(
    facts: &std::collections::BTreeMap<Span, T>,
    source_id: SourceId,
) -> std::collections::BTreeMap<Span, T> {
    facts
        .range(Span::at(source_id, 0)..)
        .take_while(|(span, _)| span.source_id == source_id)
        .map(|(span, fact)| (*span, fact.clone()))
        .collect()
}

fn source_checked_set(
    facts: &std::collections::BTreeSet<Span>,
    source_id: SourceId,
) -> std::collections::BTreeSet<Span> {
    facts
        .range(Span::at(source_id, 0)..)
        .take_while(|span| span.source_id == source_id)
        .copied()
        .collect()
}

/// The constructor facts `lint.prefer-implicit-message` counts calls with.
/// A check that reported an error did not reach every call, and a count that
/// misses one would delete a payload a call still names.
fn checked_message_payload_constructors(
    checked: &xsh::frontend::check::CheckOutput,
) -> Option<&std::collections::BTreeMap<Span, xsh::frontend::check::MessagePayloadConstructor>> {
    checked
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.severity != Severity::Error)
        .then_some(&checked.message_payload_constructors)
}

fn set_checked_lint_facts_for_source(
    options: &mut LintOptions,
    checked: &xsh::frontend::check::CheckOutput,
    source_id: SourceId,
) {
    options.function_return_types = source_checked_map(&checked.function_return_types, source_id);
    options.expr_types = source_checked_map(&checked.expr_types, source_id);
    options.proven_nonnull_fallback_receivers =
        source_checked_set(&checked.proven_nonnull_fallback_receivers, source_id);
    options.requirement_targets = source_checked_map(&checked.requirement_targets, source_id);
    options.requirement_expected_targets =
        source_checked_map(&checked.requirement_expected_targets, source_id);
    options.statement_positions = source_checked_map(&checked.statement_positions, source_id);
    options.callable_effects = checked.callable_effects.clone();
    options.function_effect_facts = checked
        .function_effect_facts
        .iter()
        .filter(|(id, _)| id.body.source_id == source_id)
        .map(|(id, fact)| (*id, fact.clone()))
        .collect();
    options.function_effect_facts_checked = true;
    options.assertion_effect_spans = source_checked_set(&checked.assertion_effect_spans, source_id);
    options.statement_expression_spans =
        source_checked_set(&checked.statement_expression_spans, source_id);
    options.propagating_statements = source_checked_set(&checked.propagating_statements, source_id);
    options.redundant_condition_propagations =
        source_checked_set(&checked.redundant_condition_propagations, source_id);
    options.implicitly_captured_runs =
        source_checked_set(&checked.implicitly_captured_runs, source_id);
    options.unvalidated_command_vectors =
        source_checked_map(&checked.unvalidated_command_vectors, source_id);
    options.membership_migration_spans =
        source_checked_set(&checked.membership_migration_spans, source_id);
    options.standard_call_spans = source_checked_map(&checked.standard_call_spans, source_id);
    options.definitely_exiting_block_spans =
        source_checked_set(&checked.definitely_exiting_block_spans, source_id);
    options.redundant_variant_qualifiers =
        source_checked_map(&checked.redundant_variant_qualifiers, source_id);
    options.message_payload_constructors = checked_message_payload_constructors(checked)
        .map(|constructors| source_checked_map(constructors, source_id));
    options.statically_resolved_call_spans = if checked.diagnostics.iter().all(spelling_only) {
        source_checked_set(&checked.statically_resolved_call_spans, source_id)
    } else {
        Default::default()
    };
}

fn lint_workspace_root(
    workspace: &LintWorkspace,
    root: &str,
    fix: bool,
    bundle: &mut ArenaProgram,
    linted_modules: &Mutex<FxHashSet<String>>,
    type_program: &Arc<ArenaProgram>,
    timings: &StageTimings,
) -> Vec<LintResult> {
    let reachable = workspace.reachable_modules(root);
    let Some(root_module) = workspace.modules.get(root) else {
        return Vec::new();
    };
    workspace.configure_program_for(root, &reachable, bundle);
    let mut relevant_diagnostics = Vec::new();
    for key in &reachable {
        if let Some(module) = workspace.modules.get(key) {
            relevant_diagnostics.extend(module.diagnostics.iter().cloned());
        }
    }
    if relevant_diagnostics
        .iter()
        .any(|diagnostic| migration_lint_code(diagnostic.code).is_none())
    {
        return vec![LintResult {
            index: 0,
            kind: LintResultKind::Diagnostics {
                status: 2,
                diagnostics: render_diagnostics_with_keys(
                    &relevant_diagnostics,
                    &workspace.sources,
                ),
            },
        }];
    }

    let mut checked = timings.time(Stage::Check, || {
        SymbolOwner::new().with_current(|| {
            xsh::frontend::check::Checker::check_arena_with_options_and_type_program(
                bundle,
                &root_module.text,
                CheckOptions::default(),
                type_program.clone(),
            )
        })
    });
    offer_module_record_require_fixes(&mut checked, &workspace.sources);
    // A check error leaves the checked facts incomplete, so the file is not
    // linted. A check warning does not: it is reported beside the lints.
    let unrelated_check_error = checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error && reported_check_finding(diagnostic)
    });
    if !checked.diagnostics.is_empty()
        && unrelated_check_error
        && (!fix || !relevant_diagnostics.is_empty())
    {
        relevant_diagnostics.extend(checked.diagnostics.iter().cloned());
        return vec![LintResult {
            index: 0,
            kind: LintResultKind::Diagnostics {
                status: 2,
                diagnostics: render_diagnostics_with_keys(
                    &relevant_diagnostics,
                    &workspace.sources,
                ),
            },
        }];
    }

    let only = root_module.config.lint_options.only.as_deref();
    if fix
        && relevant_diagnostics
            .iter()
            .chain(&checked.diagnostics)
            .any(|diagnostic| {
                migration_lint_code(diagnostic.code)
                    .is_some_and(|code| lint_code_selected(only, Some(code)))
            })
    {
        return timings.time(Stage::Fix, || {
            migrate_workspace_syntax(
                workspace,
                root,
                &reachable,
                linted_modules,
                &checked.diagnostics,
            )
        });
    }

    let mut keys = reachable
        .iter()
        .filter(|key| key.as_str() != root)
        .cloned()
        .collect::<Vec<_>>();
    keys.sort_unstable();
    let mut ordered = vec![root.to_string()];
    ordered.extend(keys);
    let mut results = Vec::new();
    for key in ordered {
        // A pending signal ends the command, which then writes no file.
        if cancellation_output().is_some() {
            break;
        }
        if key != root {
            let mut linted_modules = linted_modules
                .lock()
                .expect("linted module set mutex poisoned");
            if !linted_modules.insert(key.clone()) {
                continue;
            }
        }
        let Some(module) = workspace.modules.get(&key) else {
            continue;
        };
        bundle.statements = module.statements;
        if key != root {
            bundle.modules.clear();
        }
        let mut options = module.config.lint_options.clone();
        set_checked_lint_facts_for_source(&mut options, &checked, module.source_id);
        let lint_started = Instant::now();
        let mut linted = if key == root {
            Linter::lint(bundle, &module.text, options)
        } else {
            Linter::lint_module(bundle, &module.text, options)
        };
        if !module.diagnostics.is_empty() {
            linted.diagnostics.clear();
        }
        for diagnostic in &module.diagnostics {
            if let Some(code) = migration_lint_code(diagnostic.code) {
                let mut diagnostic = diagnostic.clone();
                diagnostic.severity = Severity::Warning;
                diagnostic.code = Some(code);
                linted.diagnostics.push(diagnostic);
            }
        }
        for diagnostic in checked
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic_mentions_source(diagnostic, module.source_id))
        {
            if let Some(code) = migration_lint_code(diagnostic.code) {
                let mut diagnostic = diagnostic.clone();
                diagnostic.severity = Severity::Warning;
                diagnostic.code = Some(code);
                linted.diagnostics.push(diagnostic);
            }
        }
        // The bundle check judges only the root's spelling; an imported
        // module's grouping is judged against its own text.
        let module_grouping = if key == root {
            Vec::new()
        } else {
            grouping_diagnostics(bundle, &module.text)
        };
        timings.record(Stage::Lint, lint_started.elapsed());
        if !fix {
            linted.diagnostics.extend(
                checked
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| {
                        (spelling_only(diagnostic) || reported_check_finding(diagnostic))
                            && diagnostic_mentions_source(diagnostic, module.source_id)
                    })
                    .cloned(),
            );
            linted.diagnostics.extend(module_grouping.iter().cloned());
        }
        linted
            .diagnostics
            .retain(|diagnostic| lint_code_selected(only, diagnostic.code));
        // A fix round is judged against every check diagnostic the file had,
        // selected or not: a rewrite may not add one, and one it leaves in
        // place is not new.
        let file_check_diagnostics = checked
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic_mentions_source(diagnostic, module.source_id))
            .chain(&module_grouping)
            .cloned()
            .collect::<Vec<_>>();
        let check_diagnostics = file_check_diagnostics
            .iter()
            .filter(|diagnostic| lint_code_selected(only, diagnostic.code))
            .cloned()
            .collect::<Vec<_>>();
        let result = if fix {
            timings.time(Stage::Fix, || {
                lint_workspace_node_with_fixes(
                    results.len(),
                    module,
                    &linted.diagnostics,
                    &check_diagnostics,
                    &file_check_diagnostics,
                    &workspace.sources,
                    key != root,
                )
            })
        } else if linted.diagnostics.is_empty() {
            LintResult {
                index: results.len(),
                kind: LintResultKind::Clean,
            }
        } else {
            LintResult {
                index: results.len(),
                kind: LintResultKind::Diagnostics {
                    status: if linted.diagnostics.iter().any(|diagnostic| {
                        (diagnostic.severity == Severity::Error
                            || reported_check_finding(diagnostic))
                            && diagnostic
                                .code
                                .is_some_and(|code| code.family() == DiagnosticFamily::Check)
                    }) {
                        2
                    } else {
                        lint_diagnostics_status(&linted.diagnostics)
                    },
                    diagnostics: render_diagnostics_with_keys(
                        &linted.diagnostics,
                        &workspace.sources,
                    ),
                },
            }
        };
        results.push(result);
    }
    results
}

/// The checker builds a `record.require` migration edit only for the root
/// file, whose text it holds. The workspace holds every imported module's
/// text, so the edit for a call in one of them is built here, from the
/// decision the checker published for that call.
fn offer_module_record_require_fixes(
    checked: &mut xsh::frontend::check::CheckOutput,
    sources: &SourceMap,
) {
    if checked.record_require_migrations.is_empty() {
        return;
    }
    for diagnostic in &mut checked.diagnostics {
        if diagnostic.code != Some(DiagnosticCode::CheckRemovedRecordRequire)
            || !diagnostic.fix_hints.is_empty()
        {
            continue;
        }
        // The diagnostic's one label is the call.
        let Some(call) = diagnostic.labels.first().map(|label| label.span) else {
            continue;
        };
        let fix = checked
            .record_require_migrations
            .get(&call)
            .zip(sources.get(call.source_id))
            .and_then(|(migration, file)| migration.fix(call, file.text()));
        diagnostic.fix_hints.extend(fix);
    }
}

/// Validate the complete rewritten import graph before publishing any migration
/// edit. Multiple entries may share a module; each source is emitted once.
fn migrate_workspace_syntax(
    workspace: &LintWorkspace,
    root: &str,
    reachable: &FxHashSet<String>,
    linted_modules: &Mutex<FxHashSet<String>>,
    checked_diagnostics: &[Diagnostic],
) -> Vec<LintResult> {
    let failure = |diagnostics: Vec<RenderedDiagnostic>, stderr: String, status| {
        vec![LintResult {
            index: 0,
            kind: LintResultKind::FixDiagnostics {
                status,
                diagnostics,
                stderr,
            },
        }]
    };
    let mut rewritten = FxHashMap::default();
    let mut loader = WorkspaceLoader::new();
    for key in reachable {
        let module = &workspace.modules[key];
        let mut migration_diagnostics = module.diagnostics.clone();
        migration_diagnostics.extend(
            checked_diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic_mentions_source(diagnostic, module.source_id)
                        && migration_lint_code(diagnostic.code).is_some()
                })
                .cloned(),
        );
        let text = if migration_diagnostics.is_empty() {
            module.text.clone()
        } else {
            let fixes = collect_fix_spans_for_source(&migration_diagnostics, module.source_id);
            let edits = fixes
                .into_iter()
                .map(|(start, end, replacement)| SourceEdit {
                    start,
                    end,
                    replacement,
                })
                .collect::<Vec<_>>();
            match apply_cst_guarded_migration_edits(
                &module.path.to_string_lossy(),
                &module.text,
                &edits,
            ) {
                Ok(Some(text)) => text,
                Ok(None) | Err(_) => {
                    let mut diagnostics = migration_diagnostics;
                    for diagnostic in &mut diagnostics {
                        diagnostic.severity = Severity::Warning;
                        diagnostic.code = migration_lint_code(diagnostic.code);
                    }
                    return failure(
                        render_diagnostics_with_keys(&diagnostics, &workspace.sources),
                        String::new(),
                        1,
                    );
                }
            }
        };
        if text != module.text {
            rewritten.insert(key.clone(), text.clone());
        }
        loader
            .source_overrides
            .insert(key.clone(), text.into_bytes());
    }
    let root_module = &workspace.modules[root];
    if let Err(message) = loader.load(
        root_module.path.clone(),
        root_module.text.as_bytes().to_vec(),
        root_module.module_roots.clone(),
    ) {
        return failure(Vec::new(), message, 2);
    }
    let (sources, program, modules) = loader.finish();
    let candidate = LintWorkspace::new(
        sources,
        program,
        modules,
        vec![root.to_string()],
        Vec::new(),
    );
    let diagnostics = candidate
        .modules
        .values()
        .flat_map(|module| module.diagnostics.iter().cloned())
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        return failure(
            render_diagnostics_with_keys(&diagnostics, &candidate.sources),
            String::new(),
            2,
        );
    }
    let mut program = candidate.program.clone();
    candidate.configure_program_for(root, &candidate.reachable_modules(root), &mut program);
    let checked = xsh::frontend::check::Checker::check_arena_with_options_and_type_program(
        &program,
        &candidate.modules[root].text,
        CheckOptions::default(),
        Arc::new(program.clone()),
    );
    if !checked.diagnostics.is_empty() {
        return failure(
            render_diagnostics_with_keys(&checked.diagnostics, &candidate.sources),
            String::new(),
            2,
        );
    }
    let mut keys = rewritten.keys().cloned().collect::<Vec<_>>();
    keys.sort_unstable();
    let mut emitted = linted_modules
        .lock()
        .expect("linted module set mutex poisoned");
    keys.into_iter()
        .filter_map(|key| {
            if !emitted.insert(key.clone()) {
                return None;
            }
            Some(LintResult {
                index: 0,
                kind: LintResultKind::Write {
                    file: workspace.modules[&key].path.to_string_lossy().into_owned(),
                    text: rewritten.remove(&key).unwrap(),
                    status: 0,
                    diagnostics: Vec::new(),
                    stderr: String::new(),
                },
            })
        })
        .collect()
}

fn worker_count_for_parallelism(file_count: usize, available: usize) -> usize {
    if file_count == 0 {
        return 0;
    }
    available.clamp(1, file_count.min(4))
}

/// Select entry roots from the candidate-file graph. Explicit file arguments
/// are always roots; directory discovery starts at files with no inbound edge,
/// then chooses one stable representative for each otherwise-unreachable
/// cyclic component.
fn select_lint_roots(
    candidates: &[String],
    modules: &FxHashMap<String, WorkspaceModule>,
    explicit: &FxHashSet<String>,
) -> Vec<String> {
    let candidate_set = candidates.iter().cloned().collect::<FxHashSet<_>>();
    let mut roots = explicit.iter().cloned().collect::<FxHashSet<_>>();
    let mut incoming = FxHashMap::<String, usize>::default();
    for key in candidates {
        incoming.entry(key.clone()).or_insert(0);
        if let Some(module) = modules.get(key) {
            for target in module
                .imports
                .iter()
                .filter_map(|import| import.target.as_ref())
            {
                if candidate_set.contains(target) {
                    *incoming.entry(target.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    roots.extend(
        candidates
            .iter()
            .filter(|key| incoming.get(*key).copied().unwrap_or(0) == 0)
            .cloned(),
    );

    let reachable = reachable_keys(&roots, modules);
    let mut remaining = candidates
        .iter()
        .filter(|key| !reachable.contains(*key))
        .cloned()
        .collect::<FxHashSet<_>>();
    while let Some(start) = remaining.iter().next().cloned() {
        let mut component = Vec::new();
        let mut pending = vec![start];
        while let Some(key) = pending.pop() {
            if !remaining.remove(&key) {
                continue;
            }
            component.push(key.clone());
            if let Some(module) = modules.get(&key) {
                for target in module
                    .imports
                    .iter()
                    .filter_map(|import| import.target.as_ref())
                {
                    if remaining.contains(target) {
                        pending.push(target.clone());
                    }
                }
            }
            for other in candidates {
                let Some(module) = modules.get(other) else {
                    continue;
                };
                if module
                    .imports
                    .iter()
                    .filter_map(|import| import.target.as_ref())
                    .any(|target| target == &key)
                    && remaining.contains(other)
                {
                    pending.push(other.clone());
                }
            }
        }
        if let Some(root) = component.into_iter().min() {
            roots.insert(root);
        }
    }
    let mut roots = roots.into_iter().collect::<Vec<_>>();
    roots.sort_unstable();
    roots
}

fn reachable_keys(
    roots: &FxHashSet<String>,
    modules: &FxHashMap<String, WorkspaceModule>,
) -> FxHashSet<String> {
    let mut reachable = FxHashSet::default();
    let mut pending = roots.iter().cloned().collect::<Vec<_>>();
    while let Some(key) = pending.pop() {
        if !reachable.insert(key.clone()) {
            continue;
        }
        if let Some(module) = modules.get(&key) {
            pending.extend(
                module
                    .imports
                    .iter()
                    .filter_map(|import| import.target.clone()),
            );
        }
    }
    reachable
}

impl LintWorkspace {
    fn new(
        sources: SourceMap,
        program: ArenaProgram,
        modules: FxHashMap<String, WorkspaceModule>,
        roots: Vec<String>,
        input_errors: Vec<String>,
    ) -> Self {
        let module_indices = program
            .modules
            .iter()
            .enumerate()
            .map(|(index, module)| (module.key.clone(), index))
            .collect();
        Self {
            sources,
            program,
            module_indices,
            type_program: std::sync::OnceLock::new(),
            modules,
            roots,
            input_errors,
        }
    }

    fn type_program(&self) -> Arc<ArenaProgram> {
        self.type_program
            .get_or_init(|| {
                let mut program = self.program.clone();
                // Type references share the arena; entry bundles own module membership.
                // Keeping every root as a module would qualify its local enum constructors.
                program.modules.clear();
                Arc::new(program)
            })
            .clone()
    }

    fn reachable_modules(&self, root: &str) -> FxHashSet<String> {
        reachable_keys(&[root.to_string()].into_iter().collect(), &self.modules)
    }

    fn configure_program_for(
        &self,
        root: &str,
        reachable: &FxHashSet<String>,
        program: &mut ArenaProgram,
    ) {
        program.statements = self
            .modules
            .get(root)
            .expect("workspace root exists")
            .statements;
        let mut ordered_keys = Vec::new();
        let mut visited = FxHashSet::default();
        order_modules_depth_first(
            root,
            reachable,
            &self.modules,
            &mut visited,
            &mut ordered_keys,
        );
        program.modules = ordered_keys
            .into_iter()
            .filter(|key| key != root)
            .filter_map(|key| self.module_indices.get(&key))
            .map(|index| self.program.modules[*index].clone())
            .collect();
        let allowed_ranges = std::iter::once(program.statements)
            .chain(program.modules.iter().map(|module| module.statements))
            .collect::<Vec<_>>();
        let allowed_statements = allowed_ranges
            .iter()
            .flat_map(|range| program.arena.stmt_ids(*range))
            .collect::<FxHashSet<StmtId>>();
        let allowed_sources = std::iter::once(root)
            .chain(program.modules.iter().map(|module| module.key.as_str()))
            .filter_map(|key| self.modules.get(key).map(|module| module.source_id))
            .collect::<FxHashSet<_>>();
        program.docs = self.program.docs.clone();
        program
            .docs
            .module_ranges
            .retain(|(range, _)| allowed_ranges.contains(range));
        program
            .docs
            .exports
            .retain(|(statement, _)| allowed_statements.contains(statement));
        program
            .docs
            .orphaned
            .retain(|span| allowed_sources.contains(&span.source_id));
        program
            .docs
            .duplicate_modules
            .retain(|span| allowed_sources.contains(&span.source_id));
    }
}

fn order_modules_depth_first(
    key: &str,
    reachable: &FxHashSet<String>,
    modules: &FxHashMap<String, WorkspaceModule>,
    visited: &mut FxHashSet<String>,
    ordered: &mut Vec<String>,
) {
    if !reachable.contains(key) || !visited.insert(key.to_string()) {
        return;
    }
    if let Some(module) = modules.get(key) {
        let mut dependencies = module
            .imports
            .iter()
            .filter_map(|import| import.target.as_ref())
            .filter(|target| reachable.contains(*target))
            .cloned()
            .collect::<Vec<_>>();
        dependencies.sort_unstable();
        for dependency in dependencies {
            order_modules_depth_first(&dependency, reachable, modules, visited, ordered);
        }
    }
    ordered.push(key.to_string());
}

/// Source parentheses that do not change the parse leave every checked fact
/// intact, so they never hide lint diagnostics.
/// A check diagnostic `xsht lint` reports as the checker's own finding: not
/// one a lint code stands for, and not a spelling the lint run judges.
fn reported_check_finding(diagnostic: &Diagnostic) -> bool {
    migration_lint_code(diagnostic.code).is_none()
        && diagnostic.code != Some(DiagnosticCode::CheckRemovedMembership)
        && !spelling_only(diagnostic)
}

fn spelling_only(diagnostic: &Diagnostic) -> bool {
    diagnostic.code == Some(DiagnosticCode::CheckRedundantParens)
}

fn diagnostic_mentions_source(diagnostic: &Diagnostic, source_id: SourceId) -> bool {
    diagnostic
        .span
        .is_some_and(|span| span.source_id == source_id)
        || diagnostic
            .labels
            .iter()
            .any(|label| label.span.source_id == source_id)
        || diagnostic
            .fix_hints
            .iter()
            .any(|hint| hint.span.is_some_and(|span| span.source_id == source_id))
}

fn lint_workspace_node_with_fixes(
    index: usize,
    module: &WorkspaceModule,
    lint_diagnostics: &[Diagnostic],
    check_diagnostics: &[Diagnostic],
    file_check_diagnostics: &[Diagnostic],
    sources: &SourceMap,
    is_module: bool,
) -> LintResult {
    let mut fixes = collect_fix_spans_for_source(lint_diagnostics, module.source_id);
    fixes.extend(collect_fix_spans_for_source(
        check_diagnostics,
        module.source_id,
    ));
    if fixes.is_empty() {
        let mut diagnostics = render_diagnostics_with_keys(check_diagnostics, sources);
        diagnostics.extend(render_diagnostics_with_keys(lint_diagnostics, sources));
        if diagnostics.is_empty() {
            return LintResult {
                index,
                kind: LintResultKind::Clean,
            };
        }
        let status = if check_diagnostics.is_empty() {
            lint_diagnostics_status(lint_diagnostics)
        } else {
            2
        };
        return LintResult {
            index,
            kind: LintResultKind::FixDiagnostics {
                status,
                diagnostics,
                stderr: String::new(),
            },
        };
    }

    let config = &module.config;
    let fixed = match apply_cst_fixes(
        &module.path.to_string_lossy(),
        &module.text,
        &fixes,
        config,
        file_check_diagnostics,
        is_module,
    ) {
        Ok(Some(fixed)) => fixed,
        Ok(None) => {
            return LintResult {
                index,
                kind: LintResultKind::FixDiagnostics {
                    status: 1,
                    diagnostics: render_diagnostics_with_keys(lint_diagnostics, sources),
                    stderr: String::new(),
                },
            };
        }
        Err(stderr) => {
            return LintResult {
                index,
                kind: LintResultKind::FixDiagnostics {
                    status: 2,
                    diagnostics: Vec::new(),
                    stderr,
                },
            };
        }
    };
    let final_text = fixed.text;
    let remaining = fixed.diagnostics;
    if final_text == module.text {
        return LintResult {
            index,
            kind: LintResultKind::FixDiagnostics {
                status: if remaining.is_empty() { 0 } else { 2 },
                diagnostics: remaining,
                stderr: String::new(),
            },
        };
    }
    LintResult {
        index,
        kind: LintResultKind::Write {
            file: module.path.to_string_lossy().into_owned(),
            text: final_text,
            status: if remaining.is_empty() { 0 } else { 2 },
            diagnostics: remaining,
            stderr: String::new(),
        },
    }
}

fn lint_config_for_file(
    file: &str,
    runless: bool,
    cwd_config: &XshConfig,
    config_cache: &ConfigCache,
) -> Result<ResolvedLintConfig, String> {
    let tool_config = FileToolConfig::new(
        config_cache.nearest_config_for_file(Path::new(file))?,
        cwd_config,
    );
    let line_width = tool_config.line_width();
    let module_roots = tool_config.module_roots();
    let configured_return_annotations = tool_config
        .config
        .check
        .annotate
        .as_ref()
        .and_then(|classes| {
            super::check::AnnotationPolicy::from_names(classes.iter().map(String::as_str)).ok()
        })
        .is_some_and(super::check::AnnotationPolicy::annotates_returns);
    let native_test_file = Path::new(file).canonicalize().ok().is_some_and(|file| {
        let roots = if tool_config.config.test_roots.is_empty() {
            vec!["tests".to_owned()]
        } else {
            tool_config.config.test_roots.clone()
        };
        roots.iter().any(|root| {
            tool_config
                .config_dir
                .join(root)
                .canonicalize()
                .ok()
                .is_some_and(|root| file.starts_with(root))
        })
    });
    let lint_options = LintOptions {
        native_test_file,
        prefer_inferred_pure_returns: tool_config.config.lint.prefer_inferred_pure_returns
            && !configured_return_annotations,
        prefer_inferred_private_effects: tool_config.config.lint.prefer_inferred_private_effects,
        prefer_env_string: tool_config.config.lint.prefer_env_string,
        prefer_item_shorthand: tool_config.config.lint.prefer_item_shorthand,
        prefer_tempdir_scope: tool_config.config.lint.prefer_tempdir_scope,
        prefer_inferred_variants: tool_config.config.lint.prefer_inferred_variants,
        prefer_positional_constructors: tool_config.config.lint.prefer_positional_constructors,
        prefer_implicit_messages: tool_config.config.lint.prefer_implicit_messages,
        explicit_missing_ok: tool_config.config.lint.explicit_missing_ok,
        prefer_text_pattern: tool_config.config.lint.prefer_text_pattern,
        // A project that asks `xsht check --annotate` to write returns does
        // not also want them removed.
        prefer_inferred_proc_returns: tool_config.config.lint.prefer_inferred_proc_returns
            && !configured_return_annotations,
        prefer_typed_callables: tool_config.config.lint.prefer_typed_callables,
        prefer_non_empty_argv: tool_config.config.lint.prefer_non_empty_argv,
        return_proof: Some(crate::xsht::lint::ReturnProofContext {
            file: file.to_string(),
            module_roots: module_roots.clone(),
        }),
        runless,
        runless_except: tool_config.config.lint.runless_except,
        interactive_command_replacement: None,
        function_return_types: Default::default(),
        expr_types: Default::default(),
        proven_nonnull_fallback_receivers: Default::default(),
        requirement_targets: Default::default(),
        requirement_expected_targets: Default::default(),
        statement_positions: Default::default(),
        callable_effects: Default::default(),
        function_effect_facts: Default::default(),
        function_effect_facts_checked: false,
        assertion_effect_spans: Default::default(),
        statement_expression_spans: Default::default(),
        propagating_statements: Default::default(),
        redundant_condition_propagations: Default::default(),
        implicitly_captured_runs: Default::default(),
        unvalidated_command_vectors: Default::default(),
        membership_migration_spans: Default::default(),
        standard_call_spans: Default::default(),
        statically_resolved_call_spans: Default::default(),
        definitely_exiting_block_spans: Default::default(),
        redundant_variant_qualifiers: Default::default(),
        message_payload_constructors: None,
        dead_code: !is_path_excluded(
            &tool_config.config_dir,
            Path::new(file),
            &tool_config.config.dead_code.exclude,
        ),
        only: None,
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
                diagnostics: render_diagnostics_with_keys(
                    &checked_program.parsed.diagnostics,
                    &checked_program.sources,
                ),
                stderr: String::new(),
            },
        };
    }
    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    let mut lint_options = config.lint_options.clone();
    lint_options.function_return_types = checked.function_return_types.clone();
    lint_options.expr_types = checked.expr_types.clone();
    lint_options.proven_nonnull_fallback_receivers =
        checked.proven_nonnull_fallback_receivers.clone();
    lint_options.requirement_targets = checked.requirement_targets.clone();
    lint_options.requirement_expected_targets = checked.requirement_expected_targets.clone();
    lint_options.statement_positions = checked.statement_positions.clone();
    lint_options.callable_effects = checked.callable_effects.clone();
    lint_options.function_effect_facts = checked.function_effect_facts.clone();
    lint_options.function_effect_facts_checked = true;
    lint_options.assertion_effect_spans = checked.assertion_effect_spans.clone();
    lint_options.statement_expression_spans = checked.statement_expression_spans.clone();
    lint_options.propagating_statements = checked.propagating_statements.clone();
    lint_options.redundant_condition_propagations =
        checked.redundant_condition_propagations.clone();
    lint_options.implicitly_captured_runs = checked.implicitly_captured_runs.clone();
    lint_options.unvalidated_command_vectors = checked.unvalidated_command_vectors.clone();
    lint_options.membership_migration_spans = checked.membership_migration_spans.clone();
    lint_options.standard_call_spans = checked.standard_call_spans.clone();
    lint_options.definitely_exiting_block_spans = checked.definitely_exiting_block_spans.clone();
    lint_options.redundant_variant_qualifiers = checked.redundant_variant_qualifiers.clone();
    lint_options.message_payload_constructors =
        checked_message_payload_constructors(checked).cloned();
    lint_options.statically_resolved_call_spans = if checked.diagnostics.iter().all(spelling_only) {
        checked.statically_resolved_call_spans.clone()
    } else {
        Default::default()
    };
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
                        diagnostics: render_diagnostics_with_keys(
                            &checked.diagnostics,
                            &checked_program.sources,
                        ),
                        stderr: String::new(),
                    }
                },
            };
        }
        let status = if checked.diagnostics.is_empty() {
            lint_diagnostics_status(&linted.diagnostics)
        } else {
            2
        };
        let mut diagnostics =
            render_diagnostics_with_keys(&checked.diagnostics, &checked_program.sources);
        diagnostics.extend(render_diagnostics_with_keys(
            &linted.diagnostics,
            &checked_program.sources,
        ));
        return LintResult {
            index,
            kind: LintResultKind::FixDiagnostics {
                status,
                diagnostics,
                stderr: String::new(),
            },
        };
    }

    let fixed = match apply_cst_fixes(file, &text, &ast_fixes, config, &checked.diagnostics, false)
    {
        Ok(Some(fixed)) => fixed,
        Ok(None) => {
            return LintResult {
                index,
                kind: LintResultKind::FixDiagnostics {
                    status: 1,
                    diagnostics: render_diagnostics_with_keys(
                        &linted.diagnostics,
                        &checked_program.sources,
                    ),
                    stderr: String::new(),
                },
            };
        }
        Err(stderr) => {
            return LintResult {
                index,
                kind: LintResultKind::FixDiagnostics {
                    status: 2,
                    diagnostics: Vec::new(),
                    stderr,
                },
            };
        }
    };
    let final_text = fixed.text;
    let remaining_check_diagnostics = fixed.diagnostics;

    if final_text == text {
        if remaining_check_diagnostics.is_empty() {
            LintResult {
                index,
                kind: LintResultKind::Clean,
            }
        } else {
            LintResult {
                index,
                kind: LintResultKind::FixDiagnostics {
                    status: 2,
                    diagnostics: remaining_check_diagnostics,
                    stderr: String::new(),
                },
            }
        }
    } else {
        LintResult {
            index,
            kind: LintResultKind::Write {
                file: file.to_string(),
                text: final_text,
                status: if remaining_check_diagnostics.is_empty() {
                    0
                } else {
                    2
                },
                diagnostics: remaining_check_diagnostics,
                stderr: String::new(),
            },
        }
    }
}

fn validate_fixed_text(
    file: &str,
    text: &str,
    config: &ResolvedLintConfig,
    original_check_diagnostics: &[Diagnostic],
) -> Result<Vec<RenderedDiagnostic>, FixedTextValidationError> {
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
        return Err(FixedTextValidationError::Diagnostics(
            render_diagnostics_with_keys(
                &checked_program.parsed.diagnostics,
                &checked_program.sources,
            ),
        ));
    }
    let checked = checked_program
        .checked
        .as_ref()
        .expect("checked program after clean parse");
    if !check_diagnostics_are_preserved(original_check_diagnostics, &checked.diagnostics) {
        let diagnostics = if checked.diagnostics.is_empty() {
            original_check_diagnostics
        } else {
            &checked.diagnostics
        };
        return Err(FixedTextValidationError::Diagnostics(
            render_diagnostics_with_keys(diagnostics, &checked_program.sources),
        ));
    }
    let only = config.lint_options.only.as_deref();
    let selected = checked
        .diagnostics
        .iter()
        .filter(|diagnostic| lint_code_selected(only, diagnostic.code))
        .cloned()
        .collect::<Vec<_>>();
    Ok(render_diagnostics_with_keys(
        &selected,
        &checked_program.sources,
    ))
}

fn apply_cst_fixes(
    file: &str,
    text: &str,
    fixes: &[(usize, usize, String)],
    config: &ResolvedLintConfig,
    original_check_diagnostics: &[Diagnostic],
    is_module: bool,
) -> Result<Option<ValidatedFixedText>, String> {
    let migrating_syntax = Parser::parse_source_arena_only(SourceId::new(0), text)
        .diagnostics
        .iter()
        .any(|diagnostic| migration_lint_code(diagnostic.code).is_some());
    // An `--only` selection applies exactly the selected edits; formatting the
    // whole file would rewrite unrelated code.
    let only = config.lint_options.only.as_deref();
    let mut candidate = text.to_owned();
    let mut fixes = fixes.to_vec();
    let mut seen = FxHashSet::default();
    seen.insert(candidate.clone());
    let mut remaining_diagnostics = None;
    // Outer edits can expose safe inner edits. Every round uses fresh checked
    // facts and source spans; a rejected round never reaches the filesystem.
    for _ in 0..64 {
        // A pending signal ends the command, which then writes no file.
        if cancellation_output().is_some() {
            return Ok(None);
        }
        let edits = fixes
            .iter()
            .map(|(start, end, replacement)| SourceEdit {
                start: *start,
                end: *end,
                replacement: replacement.clone(),
            })
            .collect::<Vec<_>>();
        let next = if only.is_some() {
            apply_cst_guarded_migration_edits(file, &candidate, &edits)?
        } else {
            apply_cst_guarded_edits(file, &candidate, &edits, config.line_width)?
        };
        let Some(next) = next else {
            return Ok(None);
        };
        if next == candidate {
            let diagnostics = match remaining_diagnostics {
                Some(diagnostics) => diagnostics,
                None => {
                    match validate_fixed_text(file, &candidate, config, original_check_diagnostics)
                    {
                        Ok(diagnostics) => diagnostics,
                        Err(FixedTextValidationError::Diagnostics(diagnostics)) => {
                            return Err(diagnostics
                                .into_iter()
                                .map(|diagnostic| diagnostic.text)
                                .collect());
                        }
                    }
                }
            };
            return Ok(Some(ValidatedFixedText {
                text: candidate,
                diagnostics,
            }));
        }
        if !seen.insert(next.clone()) {
            return Err(format!("xsht: safe fixes for {file} do not converge\n"));
        }
        candidate = next;
        let symbols = SymbolOwner::new();
        let program = symbols.with_current(|| {
            parse_load_check_text(
                file,
                candidate.clone(),
                config.module_roots.clone(),
                CheckOptions::default(),
            )
        });
        if !program.parsed.diagnostics.is_empty() {
            return Err(
                DiagnosticRenderer::new().render(&program.parsed.diagnostics, &program.sources)
            );
        }
        let checked = program
            .checked
            .as_ref()
            .expect("checked program after clean parse");
        if !check_diagnostics_are_preserved(original_check_diagnostics, &checked.diagnostics) {
            return Err(DiagnosticRenderer::new().render(&checked.diagnostics, &program.sources));
        }
        // Only the selected codes are reported; the rest of the file's check
        // diagnostics were there before and are not this run's subject.
        let selected_checks = checked
            .diagnostics
            .iter()
            .filter(|diagnostic| lint_code_selected(only, diagnostic.code))
            .cloned()
            .collect::<Vec<_>>();
        remaining_diagnostics = Some(render_diagnostics_with_keys(
            &selected_checks,
            &program.sources,
        ));
        if migrating_syntax {
            return Ok(Some(ValidatedFixedText {
                text: candidate,
                diagnostics: remaining_diagnostics.unwrap(),
            }));
        }
        let mut options = config.lint_options.clone();
        options.function_return_types = checked.function_return_types.clone();
        options.expr_types = checked.expr_types.clone();
        options.proven_nonnull_fallback_receivers =
            checked.proven_nonnull_fallback_receivers.clone();
        options.requirement_targets = checked.requirement_targets.clone();
        options.requirement_expected_targets = checked.requirement_expected_targets.clone();
        options.statement_positions = checked.statement_positions.clone();
        options.callable_effects = checked.callable_effects.clone();
        options.function_effect_facts = checked.function_effect_facts.clone();
        options.function_effect_facts_checked = true;
        options.assertion_effect_spans = checked.assertion_effect_spans.clone();
        options.statement_expression_spans = checked.statement_expression_spans.clone();
        options.propagating_statements = checked.propagating_statements.clone();
        options.redundant_condition_propagations =
            checked.redundant_condition_propagations.clone();
        options.implicitly_captured_runs = checked.implicitly_captured_runs.clone();
        options.unvalidated_command_vectors = checked.unvalidated_command_vectors.clone();
        options.membership_migration_spans = checked.membership_migration_spans.clone();
        options.standard_call_spans = checked.standard_call_spans.clone();
        options.definitely_exiting_block_spans = checked.definitely_exiting_block_spans.clone();
        options.redundant_variant_qualifiers = checked.redundant_variant_qualifiers.clone();
        options.message_payload_constructors =
            checked_message_payload_constructors(checked).cloned();
        options.statically_resolved_call_spans = if checked.diagnostics.iter().all(spelling_only) {
            checked.statically_resolved_call_spans.clone()
        } else {
            Default::default()
        };
        let linted = if is_module {
            Linter::lint_module(&program.parsed.arena, &candidate, options)
        } else {
            Linter::lint(&program.parsed.arena, &candidate, options)
        };
        fixes = collect_fix_spans_for_source(&linted.diagnostics, SourceId::new(0));
        fixes.extend(collect_fix_spans_for_source(
            &selected_checks,
            SourceId::new(0),
        ));
        if fixes.is_empty() {
            return Ok(Some(ValidatedFixedText {
                text: candidate,
                diagnostics: remaining_diagnostics.unwrap(),
            }));
        }
    }
    Err(format!(
        "xsht: safe fixes for {file} exceeded the convergence limit\n"
    ))
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
fn collect_fix_spans(diagnostics: &[Diagnostic]) -> Vec<(usize, usize, String)> {
    collect_fix_spans_by_code(diagnostics, |_| true)
}

fn collect_fix_spans_for_source(
    diagnostics: &[Diagnostic],
    source_id: SourceId,
) -> Vec<(usize, usize, String)> {
    collect_fix_spans_filtered(
        diagnostics,
        |_| true,
        |hint| hint.span.is_some_and(|span| span.source_id == source_id),
    )
}

fn collect_fix_spans_by_code(
    diagnostics: &[Diagnostic],
    include: impl Fn(&Diagnostic) -> bool,
) -> Vec<(usize, usize, String)> {
    collect_fix_spans_filtered(diagnostics, include, |_| true)
}

fn collect_fix_spans_filtered(
    diagnostics: &[Diagnostic],
    include: impl Fn(&Diagnostic) -> bool,
    include_hint: impl Fn(&xsh::diagnostic::FixHint) -> bool,
) -> Vec<(usize, usize, String)> {
    let mut fixes: Vec<_> = diagnostics
        .iter()
        .filter(|d| include(d))
        .flat_map(|d| d.fix_hints.iter())
        .filter(|hint| include_hint(hint))
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
            && diagnostic.code == Some(DiagnosticCode::LintPathConstructor)
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
        diagnostic.code.map_or("", DiagnosticCode::name),
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
    // Removing a pair of parentheses lets the pairs inside it be judged on the
    // next round, so their count may grow while the fixes converge.
    for diagnostic in current
        .iter()
        .filter(|diagnostic| diagnostic.code != Some(DiagnosticCode::CheckRedundantParens))
    {
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

#[cfg(test)]
mod tests {
    use crate::xsht::cli::lint::{
        ConfigCache, LintResultKind, LintWorkspace, ResolvedLintConfig, WorkspaceLoader,
        apply_cst_fixes, collect_fix_spans, discover_lint_files, lint_config_for_file,
        lint_one_file_with_fixes, lint_workspace,
    };
    use crate::xsht::cli::timing::StageTimings;
    use crate::xsht::format::DEFAULT_LINE_WIDTH;
    use crate::xsht::lint::LintOptions;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Severity};
    use xsh::frontend::source::{SourceId, Span};
    use xsh::frontend::symbols::SymbolOwner;

    fn config() -> ResolvedLintConfig {
        ResolvedLintConfig {
            lint_options: LintOptions::default(),
            line_width: DEFAULT_LINE_WIDTH,
            module_roots: Vec::<PathBuf>::new(),
        }
    }

    /// A repaired private clause names exactly its inferred effects, so the
    /// default configuration would go on to delete it. Repair tests stop at
    /// the repaired clause.
    fn repair_config() -> ResolvedLintConfig {
        let mut config = config();
        config.lint_options.prefer_inferred_private_effects = false;
        config
    }

    #[test]
    fn explicit_directory_discovery_does_not_expand_parent_includes() {
        let root = TempDir::new().expect("create temp root");
        let project = root.path().join("project");
        fs::create_dir(&project).expect("create project directory");
        fs::write(root.path().join("xsht-config.ini"), "include = extra\n")
            .expect("write parent config");
        let script = project.join("main.xsh");
        fs::write(&script, "let value = 1\n").expect("write script");

        let discovered = discover_lint_files(
            &[project.to_string_lossy().into_owned()],
            &crate::xsht::cli::XshConfig::default(),
        )
        .expect("discover explicit directory");
        assert_eq!(discovered.files, vec![script.to_string_lossy()]);
    }

    #[test]
    fn lint_config_disables_only_dead_code_for_matching_paths() {
        let root = TempDir::new().expect("create config root");
        let snippet = root.path().join("docs/snippets/api/example.xsh");
        fs::create_dir_all(snippet.parent().expect("snippet parent")).expect("create snippet");
        fs::write(
            root.path().join("xsht-config.ini"),
            "[dead-code]\nexclude = docs/snippets/**/*.xsh\n",
        )
        .expect("write config");

        let config = lint_config_for_file(
            snippet.to_str().expect("utf-8 snippet path"),
            false,
            &crate::xsht::cli::XshConfig::default(),
            &ConfigCache::default(),
        )
        .expect("resolve lint config");

        assert!(!config.lint_options.dead_code);
        assert!(!config.lint_options.runless);
    }

    #[test]
    fn lint_worker_count_respects_available_cpus_roots_and_four_worker_bound() {
        for (roots, available, expected) in [
            (0, 8, 0),
            (1, 8, 1),
            (16, 1, 1),
            (16, 2, 2),
            (16, 4, 4),
            (16, 64, 4),
            (3, 8, 3),
        ] {
            assert_eq!(
                super::worker_count_for_parallelism(roots, available),
                expected,
                "{roots} roots with {available} available CPUs"
            );
        }
    }

    #[test]
    fn lint_four_workers_preserve_shared_import_diagnostics_and_source_bytes() {
        let fixture = TempDir::new().unwrap();
        let module = fixture.path().join("helper.xsh");
        let module_source =
            "##! Helper.\n## Returns one.\nexport pure value() -> Int { return 1 }\n";
        fs::write(&module, module_source).unwrap();
        let mut paths = Vec::new();
        let source = "use helper\nprint (helper.value())\n";
        for index in 0..8 {
            let path = fixture.path().join(format!("entry{index}.xsh"));
            fs::write(&path, source).unwrap();
            paths.push(path.to_string_lossy().into_owned());
        }
        let config = crate::xsht::cli::XshConfig::default();
        let discovery = discover_lint_files(&paths, &config).unwrap();
        let run = |available| {
            let results = super::lint_workspace_with_parallelism(
                &discovery,
                false,
                false,
                &config,
                &ConfigCache::default(),
                available,
                &StageTimings::start(),
            );
            let mut output = Vec::new();
            for result in results {
                match result.kind {
                    LintResultKind::Clean => {}
                    LintResultKind::Diagnostics {
                        status,
                        diagnostics,
                    } => {
                        assert_eq!(status, 1);
                        for diagnostic in diagnostics {
                            output.push((diagnostic.key, diagnostic.text));
                        }
                    }
                    _ => panic!("read-only valid workspace must only return lint diagnostics"),
                }
            }
            output
        };
        let serial = run(1);
        let parallel = run(4);
        assert_eq!(serial.len(), 1, "{serial:?}");
        assert!(serial[0].1.contains("lint.redundant-tail-return"));
        assert_eq!(parallel, serial);
        assert_eq!(fs::read_to_string(&module).unwrap(), module_source);
        for path in paths {
            assert_eq!(fs::read_to_string(path).unwrap(), source);
        }
    }

    #[test]
    fn lint_workspace_copies_only_current_source_checked_facts() {
        let local = SourceId::new(1);
        let foreign = SourceId::new(2);
        let local_span = Span::new(local, 10, 12);
        let mut checked = xsh::frontend::check::CheckOutput::default();
        checked
            .expr_types
            .insert(local_span, xsh::frontend::check::Type::Int);
        checked.statement_positions.insert(
            local_span,
            xsh::frontend::check::StatementPosition::Statement,
        );
        checked.proven_nonnull_fallback_receivers.insert(local_span);
        checked.propagating_statements.insert(local_span);
        for offset in 0..100 {
            let span = Span::new(foreign, offset, offset + 1);
            checked
                .expr_types
                .insert(span, xsh::frontend::check::Type::Str);
            checked
                .statement_positions
                .insert(span, xsh::frontend::check::StatementPosition::Value);
            checked.proven_nonnull_fallback_receivers.insert(span);
            checked.propagating_statements.insert(span);
        }
        let mut options = LintOptions::default();
        super::set_checked_lint_facts_for_source(&mut options, &checked, local);
        assert_eq!(options.expr_types.len(), 1);
        assert_eq!(
            options.expr_types.get(&local_span),
            Some(&xsh::frontend::check::Type::Int)
        );
        assert_eq!(options.statement_positions.len(), 1);
        assert_eq!(options.proven_nonnull_fallback_receivers.len(), 1);
        assert!(options.propagating_statements.contains(&local_span));
        assert_eq!(options.propagating_statements.len(), 1);
        assert!(options.function_effect_facts_checked);
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
export proc map_etcsums(etcsums: List[EtcSum]) [error] -> Result[Map[Str], Error] {
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
    fn lint_fix_half_open_slices_converges_for_nested_calls() {
        let source = "let part = b\"abcdef\".slice(0, 5).slice(0, 2)\nprint part.base64()\n";
        let config = config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed slices");
        };
        assert!(text.contains("b\"abcdef\"[..5][..2]"), "{text}");
        let second = lint_one_file_with_fixes(0, "fixture.xsh", text, &config);
        assert!(matches!(second.kind, LintResultKind::Clean));
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
            &[],
            false,
        )
        .expect("apply fixes");

        assert!(result.is_none());
    }

    #[test]
    fn lint_fix_rewrites_empty_map_initializer_through_ast() {
        for (map_type, key) in [("Map[Int]", "\"x\""), ("Map[Int, Str]", "1")] {
            let source = format!("let counts: {map_type} = map.empty()\nprint ({key} in counts)\n");
            let config = config();
            let result = lint_one_file_with_fixes(0, "fixture.xsh", source, &config);
            let LintResultKind::Write { text, .. } = result.kind else {
                panic!("expected fixed source to be written");
            };

            assert!(
                text.contains(&format!("counts: {map_type} = {{}}")),
                "{text}"
            );
            assert!(text.contains(&format!("print ({key} in counts)")), "{text}");
            assert!(!text.contains("map.empty()"));
            let second = lint_one_file_with_fixes(0, "fixture.xsh", text, &config);
            assert!(matches!(second.kind, LintResultKind::Clean));
        }
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

        assert!(text.contains("const name = \"pkg\""), "{text}");
        assert!(!text.contains(": Str"));
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn lint_fix_checks_the_converged_source_once() {
        xsh::frontend::stdlib_preparation::reset();
        let source = "print tui.red()\nlet target_path = Path(\"/srv/xsh\")\nprint target_path\n";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        let LintResultKind::Write {
            text,
            status,
            diagnostics,
            ..
        } = result.kind
        else {
            panic!("expected the path constructor to be fixed");
        };
        assert_eq!(
            status,
            0,
            "{}",
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.text.as_str())
                .collect::<String>()
        );
        assert!(!text.contains("Path("), "{text}");
        assert!(text.contains("const target_path = /srv/xsh"), "{text}");
        assert_eq!(
            xsh::frontend::stdlib_preparation::parsed_modules(),
            3,
            "prepare each changed source once: constructor, Path literal, then prepared constant"
        );
    }

    #[test]
    fn lint_workspace_migrates_imported_enums_and_refuses_unrelated_import_errors() {
        for (source, expected_error) in [
            (
                "##! Choices.\n## An option.\nexport type Choice = One | Two\n",
                None,
            ),
            (
                "##! Choices.\nuse missing\n## An option.\nexport type Choice = One | Two\n",
                Some("parse.module-read"),
            ),
            (
                "##! Choices.\n## An option.\nexport type Choice = One | Two\nlet broken: Int = \"wrong\"\n",
                Some("check.type-mismatch"),
            ),
        ] {
            let root = TempDir::new().expect("create import migration fixture");
            let module = root.path().join("choice.xsh");
            fs::write(&module, source).expect("write imported enum");
            let entry = root.path().join("entry.xsh");
            fs::write(
                &entry,
                "use choice\nlet selected: choice.Choice = choice.One\n",
            )
            .expect("write entry");
            let config = crate::xsht::cli::XshConfig::default();
            let discovery = discover_lint_files(&[entry.to_string_lossy().into_owned()], &config)
                .expect("discover entry");
            let results = lint_workspace(
            &discovery,
            true,
            false,
            &config,
            &ConfigCache::default(),
            &StageTimings::start(),
        );
            if let Some(expected_error) = expected_error {
                assert!(
                    results
                        .iter()
                        .all(|result| !matches!(result.kind, LintResultKind::Write { .. }))
                );
                assert!(results.iter().any(|result| matches!(&result.kind,
                    LintResultKind::Diagnostics { status: 2, diagnostics }
                    | LintResultKind::FixDiagnostics { status: 2, diagnostics, .. }
                    if diagnostics.iter().any(|diagnostic| diagnostic.text.contains(expected_error)))));
            } else {
                assert!(
                    results.iter().any(|result| matches!(&result.kind,
                    LintResultKind::Write { file, text, status: 0, .. }
                    if file == &module.to_string_lossy() && text.contains("export enum Choice {"))),
                    "{}",
                    results
                        .iter()
                        .map(|result| match &result.kind {
                            LintResultKind::Diagnostics { diagnostics, .. }
                            | LintResultKind::FixDiagnostics { diagnostics, .. } => diagnostics
                                .iter()
                                .map(|diagnostic| diagnostic.text.as_str())
                                .collect::<String>(),
                            LintResultKind::Write { text, .. } => text.clone(),
                            _ => String::new(),
                        })
                        .collect::<String>()
                );
            }
            assert_eq!(
                fs::read_to_string(&module).expect("read imported enum"),
                source
            );
        }
    }

    #[test]
    fn lint_workspace_source_facts_preserve_bundle_diagnostics() {
        let fixture = TempDir::new().expect("create checked fact fixture");
        let module = fixture.path().join("helper.xsh");
        fs::write(&module, "##! Helpers.\n## Echoes an integer.\nexport pure echo(value: Int) -> Int { return value }\n").unwrap();
        let entry = fixture.path().join("entry.xsh");
        fs::write(
            &entry,
            "use helper\nlet value: Int = helper.echo(1)\nprint $value\n",
        )
        .unwrap();
        let mut loader = WorkspaceLoader::new();
        let root = loader
            .load(entry.clone(), fs::read(&entry).unwrap(), Vec::new())
            .unwrap();
        let (sources, program, modules) = loader.finish();
        let workspace =
            LintWorkspace::new(sources, program, modules, vec![root.clone()], Vec::new());
        let reachable = workspace.reachable_modules(&root);
        let mut bundle = workspace.program.clone();
        workspace.configure_program_for(&root, &reachable, &mut bundle);
        let checked =
            xsh::frontend::check::Checker::check_arena(&bundle, &workspace.modules[&root].text);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        for key in reachable {
            let module = &workspace.modules[&key];
            let mut projected = LintOptions::default();
            super::set_checked_lint_facts_for_source(&mut projected, &checked, module.source_id);
            let mut complete = projected.clone();
            complete.function_return_types = checked.function_return_types.clone();
            complete.expr_types = checked.expr_types.clone();
            complete.statement_positions = checked.statement_positions.clone();
            complete.standard_call_spans = checked.standard_call_spans.clone();
            complete.function_effect_facts = checked.function_effect_facts.clone();
            assert!(projected.expr_types.len() < complete.expr_types.len());
            bundle.statements = module.statements;
            if key != root {
                bundle.modules.clear();
            }
            let lint = |options| {
                if key == root {
                    crate::xsht::lint::Linter::lint(&bundle, &module.text, options)
                } else {
                    crate::xsht::lint::Linter::lint_module(&bundle, &module.text, options)
                }
            };
            assert_eq!(
                format!("{:?}", lint(projected).diagnostics),
                format!("{:?}", lint(complete).diagnostics)
            );
        }
    }

    #[test]
    fn lint_workspace_root_configuration_keeps_shared_module_identity_and_docs() {
        let root = TempDir::new().expect("create workspace fixture");
        let sources = [
            (
                "shared.xsh",
                "##! Shared.\n## Shared value.\nexport const value = 1\n",
            ),
            (
                "branch.xsh",
                "##! Branch.\nuse shared\n## Branch value.\nexport const value = shared.value\n",
            ),
            ("left.xsh", "##! Left.\nuse branch\nprint branch.value\n"),
            ("right.xsh", "##! Right.\nuse shared\nprint shared.value\n"),
        ];
        for (name, text) in sources {
            fs::write(root.path().join(name), text).expect("write workspace source");
        }
        let mut loader = WorkspaceLoader::new();
        let mut roots = Vec::new();
        for name in ["left.xsh", "right.xsh"] {
            let file = root.path().join(name);
            roots.push(
                loader
                    .load(file.clone(), fs::read(file).unwrap(), Vec::new())
                    .unwrap(),
            );
        }
        let (sources, program, modules) = loader.finish();
        let workspace = LintWorkspace::new(sources, program, modules, roots, Vec::new());
        let type_program = workspace.type_program();
        assert!(type_program.modules.is_empty());
        assert!(
            std::sync::Arc::ptr_eq(&type_program, &workspace.type_program()),
            "workers share the same immutable arena for type references"
        );
        let mut bundle = workspace.program.clone();
        for (root_index, expected_modules) in [
            (0, vec!["shared", "branch"]),
            (1, vec!["shared"]),
            (0, vec!["shared", "branch"]),
        ] {
            let key = &workspace.roots[root_index];
            let reachable = workspace.reachable_modules(key);
            workspace.configure_program_for(key, &reachable, &mut bundle);
            assert_eq!(bundle.statements, workspace.modules[key].statements);
            assert_eq!(
                bundle
                    .modules
                    .iter()
                    .map(|module| module.key.clone())
                    .collect::<Vec<_>>(),
                expected_modules
                    .iter()
                    .map(|name| xsh::frontend::load::module_key(
                        &root.path().join(format!("{name}.xsh"))
                    ))
                    .collect::<Vec<_>>()
            );
            for module in &bundle.modules {
                assert_eq!(module.statements, workspace.modules[&module.key].statements);
                assert_eq!(module.name.as_str(), module.key.as_str());
            }
            let allowed_sources = reachable
                .iter()
                .map(|key| workspace.modules[key].source_id)
                .collect::<rustc_hash::FxHashSet<_>>();
            assert_eq!(bundle.docs.module_ranges.len(), reachable.len());
            assert!(
                bundle
                    .docs
                    .module_ranges
                    .iter()
                    .all(|(_, span)| allowed_sources.contains(&span.source_id))
            );
            assert!(
                bundle
                    .docs
                    .exports
                    .iter()
                    .all(|(_, span)| allowed_sources.contains(&span.source_id))
            );
            assert_eq!(bundle.docs.exports.len(), expected_modules.len());
        }
    }

    #[test]
    fn lint_workspace_root_enum_contract_matches_entry_checker() {
        for invalid_assignment in [false, true] {
            let root = TempDir::new().expect("create nominal root fixture");
            let file = root.path().join("choice.xsh");
            let mut source = "##! Choices.\n## A selection.\nexport enum Choice { One, Other(Int) }\n## A holder.\nexport type Holder = {choice: Choice}\n## Keep a selection.\nexport pure select(selected: Choice) -> Choice { selected }\nvar selected: Choice = One\nselected = Other(7)\nlet holder = Holder(choice: selected)\nlet _ = select(holder.choice)\nmatch selected { One => print \"one\"; Other(number) => print $number }\n".to_owned();
            if invalid_assignment {
                source.push_str("selected = 1\n");
            }
            fs::write(&file, &source).expect("write nominal root fixture");
            let checked = xsh::frontend::load::parse_load_check_text(
                file.to_str().unwrap(),
                source,
                Vec::new(),
                xsh::frontend::check::CheckOptions::default(),
            );
            assert!(checked.parsed.diagnostics.is_empty());
            let expected = &checked.checked.unwrap().diagnostics;
            assert_eq!(expected.is_empty(), !invalid_assignment, "{expected:?}");
            let config = crate::xsht::cli::XshConfig::default();
            let discovery =
                discover_lint_files(&[file.to_string_lossy().into_owned()], &config).unwrap();
            let results = lint_workspace(
            &discovery,
            false,
            false,
            &config,
            &ConfigCache::default(),
            &StageTimings::start(),
        );
            let errors = results
                .iter()
                .filter_map(|result| match &result.kind {
                    LintResultKind::Diagnostics {
                        status: 2,
                        diagnostics,
                    } => Some(diagnostics),
                    _ => None,
                })
                .flatten()
                .map(|diagnostic| diagnostic.text.as_str())
                .collect::<String>();
            if invalid_assignment {
                assert!(errors.contains("check.type-mismatch"), "{errors}");
            } else {
                assert!(errors.is_empty(), "{errors}");
            }
        }
    }

    #[test]
    fn lint_workspace_legacy_root_enum_helper_migration_rechecks() {
        let root = TempDir::new().expect("create root enum migration fixture");
        let file = root.path().join("target.xsh");
        let source = "##! Targets.\n## A target.\nexport type Target = A | B\n## The selected target.\nexport pure target() -> Target { return A }\n";
        fs::write(&file, source).unwrap();
        let config = crate::xsht::cli::XshConfig::default();
        let discovery =
            discover_lint_files(&[file.to_string_lossy().into_owned()], &config).unwrap();
        let results = lint_workspace(
            &discovery,
            true,
            false,
            &config,
            &ConfigCache::default(),
            &StageTimings::start(),
        );
        let fixed = results
            .into_iter()
            .find_map(|result| match result.kind {
                LintResultKind::Write {
                    text, status: 0, ..
                } => Some(text),
                _ => None,
            })
            .expect("migrate the root enum without false nominal mismatches");
        assert!(fixed.contains("export enum Target {"), "{fixed}");
        let checked = xsh::frontend::load::parse_load_check_text(
            file.to_str().unwrap(),
            fixed,
            Vec::new(),
            xsh::frontend::check::CheckOptions::default(),
        );
        assert!(checked.parsed.diagnostics.is_empty());
        assert!(checked.checked.unwrap().diagnostics.is_empty());
        assert_eq!(fs::read_to_string(file).unwrap(), source);
    }

    #[test]
    fn lint_workspace_preserves_distinct_imported_enum_identities_with_same_file_name() {
        let root = TempDir::new().expect("create distinct nominal import fixture");
        let module = "##! Choices.\n## A choice.\nexport enum Choice { Selected }\n## Consume a choice.\nexport pure consume(selected: Choice) -> Int { 1 }\n";
        for directory in ["left", "right"] {
            fs::create_dir(root.path().join(directory)).unwrap();
            fs::write(root.path().join(directory).join("choice.xsh"), module).unwrap();
        }
        let file = root.path().join("entry.xsh");
        let source = "use left.choice as first\nuse right.choice as second\nlet selected: first.Choice = first.Selected\nlet _ = second.consume(selected)\n";
        fs::write(&file, source).unwrap();
        let checked = xsh::frontend::load::parse_load_check_text(
            file.to_str().unwrap(),
            source.to_owned(),
            Vec::new(),
            xsh::frontend::check::CheckOptions::default(),
        );
        assert!(checked.parsed.diagnostics.is_empty());
        assert!(
            checked
                .checked
                .unwrap()
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch))
        );
        let config = crate::xsht::cli::XshConfig::default();
        let discovery =
            discover_lint_files(&[file.to_string_lossy().into_owned()], &config).unwrap();
        let results = lint_workspace(
            &discovery,
            false,
            false,
            &config,
            &ConfigCache::default(),
            &StageTimings::start(),
        );
        assert!(results.iter().any(|result| matches!(&result.kind,
            LintResultKind::Diagnostics { status: 2, diagnostics }
            if diagnostics.iter().any(|diagnostic| diagnostic.text.contains("check.type-mismatch")))));
    }

    #[test]
    fn lint_fix_keeps_var_annotation_when_reassigned() {
        let source = "\
var build_env: Record = {A: \"1\"}
build_env = {A: \"1\", B: \"2\"}
let _ = \"B\" in build_env
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

        assert!(
            text.contains("  [item for item in left if item in right]"),
            "{text}"
        );
        assert!(!text.contains("var values"));
        assert!(!text.contains("return values"));
    }

    #[test]
    fn lint_fix_preserves_context_for_optional_conditional() {
        let source = "pure choose(flag: Bool, value: Int?) -> Int? {\n  let selected: Int? = if flag { null } else { value }\n  return selected\n}\nprint ${choose(true, 1)}\n";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        if let LintResultKind::FixDiagnostics {
            diagnostics,
            stderr,
            ..
        } = &result.kind
        {
            panic!(
                "{stderr}: {}",
                diagnostics
                    .iter()
                    .map(|d| d.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        assert!(
            matches!(
                result.kind,
                LintResultKind::Write { .. } | LintResultKind::Clean
            ),
            "fixes must preserve the contextual optional type"
        );
    }

    #[test]
    fn lint_fix_preserves_grouped_pipeline_condition() {
        // A one-line condition becomes a guard that keeps the pipeline grouped.
        let source = "proc choose(values: List[Str]) {\n  for value in values {\n    if value == \"\" or (value.split(\"\") |> any { |part| part == \"x\" }) { continue }\n    print ${value}\n  }\n}\nchoose([\"ok\"])\n";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected the guard fix to be written");
        };
        assert!(text.contains("    continue when value == \"\" or (value.split(\"\") |> any { |part| part == \"x\" })\n"), "{text}");
        // The formatter keeps the author's broken pipeline, so the guard would
        // not be a one-liner and the block stays.
        let source = "proc choose(values: List[Str]) {\n  for value in values {\n    if value == \"\" or (value.split(\"\")\n      |> any { |part|\n        part == \"x\"\n      }) {\n      continue\n    }\n    print ${value}\n  }\n}\nchoose([\"ok\"])\n";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        assert!(
            matches!(result.kind, LintResultKind::Clean),
            "grouped pipeline guard must keep its block: {source}"
        );
    }

    #[test]
    fn lint_fix_preserves_conditional_membership_receiver() {
        let source = "pure choose(flag: Bool, value: Str) -> Bool {\n  return !(if flag { \"abc\" } else { \"def\" }).contains(value)\n}\nprint ${choose(true, \"a\")}\n";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        assert!(
            matches!(
                result.kind,
                LintResultKind::Write { .. } | LintResultKind::Clean
            ),
            "conditional receiver grouping must stay parseable"
        );
    }

    #[test]
    fn lint_fix_retains_record_schema_validation() {
        let source = "type Item = { value: Str? }\nproc emit(raw: Record) [error] {\n  let item = raw.require(Item)?\n  print ${item.value ?? \"none\"}\n}\nemit({value: null})?\n";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        match result.kind {
            LintResultKind::Clean => {}
            LintResultKind::Write { text, status, .. } => {
                assert_eq!(status, 0);
                assert!(text.contains("raw.require(Item)"));
            }
            _ => panic!("schema validation must remain valid after fixing"),
        }
    }

    #[test]
    fn lint_fix_retains_record_collection_element_type() {
        let source = "type Item = { parent: Str? }\nproc emit() {\n  var items: List[Item] = []\n  for name in [\"one\"] {\n    items = items.push({parent: null})\n  }\n  items = items.push({parent: \"two\"})\n  print ${items.len()}\n}\nemit()\n";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        let LintResultKind::Write { text, status, .. } = result.kind else {
            panic!("record collection rewrite must remain valid");
        };
        assert_eq!(status, 0);
        assert!(text.contains("items: List[Item]"));
    }

    #[test]
    fn lint_fix_preserves_fallback_return() {
        let source = "pure pick(value: Int?) -> Result[Int] {\n  return Ok(value ?? 1)\n}\nprint ${pick(null)?}\n";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        assert!(matches!(
            result.kind,
            LintResultKind::Clean | LintResultKind::Write { status: 0, .. }
        ));
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
proc load() [fs] {
  let _ = fs.read_text(Path(\"x\"))?
}
";
        let config = repair_config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("proc load() [fs, error]"));
    }

    #[test]
    fn lint_fix_converges_a_too_narrow_private_clause_to_inference() {
        let source = "\
proc load() [fs] {
  let _ = fs.read_text(Path(\"x\"))?
}
";
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config());
        let LintResultKind::Write { text, status, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert_eq!(status, 0);
        assert!(text.starts_with("proc load() {\n"), "{text}");
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

proc stamp() [] -> Int {
  timestamp() + 1
}
";
        let config = repair_config();
        let result = lint_one_file_with_fixes(0, "fixture.xsh", source.to_string(), &config);
        let LintResultKind::Write { text, .. } = result.kind else {
            panic!("expected fixed source to be written");
        };

        assert!(text.contains("proc stamp() [time] -> Int"), "{text}");
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

proc build() [] -> Int {
  ARGV.image_task()
}
";
            let config = repair_config();
            let result = lint_one_file_with_fixes(
                0,
                &entry_path.to_string_lossy(),
                source.to_string(),
                &config,
            );
            let LintResultKind::Write { text, .. } = result.kind else {
                panic!("expected fixed source to be written");
            };

            assert!(text.contains("proc build() [env] -> Int"));
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

proc build() [] -> Int {
  ARGV.image_task()
}
";
            let config = repair_config();
            let result = lint_one_file_with_fixes(
                0,
                &entry_path.to_string_lossy(),
                source.to_string(),
                &config,
            );
            let LintResultKind::Write { text, .. } = result.kind else {
                panic!("expected fixed source to be written");
            };

            assert!(text.contains("proc build() [env] -> Int"));
        });
    }
}
