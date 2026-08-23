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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label, Severity};
use xsh::frontend::check::CheckOptions;
use xsh::frontend::load::{module_key, parse_load_check_text, resolve_user_module};
use xsh::frontend::source::{SourceId, SourceMap, Span};
use xsh::frontend::symbols::{Name, SymbolOwner};
use xsh::frontend::syntax::arena::{
    ArenaProgram, ArenaProgramBuilder, ArenaRange, ArenaStmtKind, StmtId, UseStmtId,
};
use xsh::frontend::syntax::parser::Parser;
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

    let config_cache = ConfigCache::default();
    let mut results = lint_workspace(&discovered, fix, runless, &cwd_config, &config_cache);
    if let Some(output) = cancellation_output() {
        return output;
    }
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
                let dir_config = config_for_dir(path, config)?.config;
                collect_configured_xsh_files(path, &dir_config, &mut paths)?;
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
}

impl WorkspaceLoader {
    fn new() -> Self {
        Self {
            sources: SourceMap::new(),
            builder: ArenaProgramBuilder::with_token_capacity(4096),
            modules: FxHashMap::default(),
            stack: Vec::new(),
        }
    }

    fn load(
        &mut self,
        path: PathBuf,
        bytes: Vec<u8>,
        module_roots: Vec<PathBuf>,
    ) -> Result<String, String> {
        let key = module_key(&path);
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
                    .with_code("source.invalid-utf8")
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

        let name = self.builder.symbol_owner().clone().with_current(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(Name::intern)
                .unwrap_or_else(|| Name::intern("module"))
        });
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
                                            .with_code("parse.module-cycle")
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
                                        .with_code("parse.module-load")
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
                                .with_code("parse.module-read")
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
) -> Vec<LintResult> {
    let mut loader = WorkspaceLoader::new();
    let mut input_errors = Vec::new();
    let mut candidate_keys = Vec::new();
    for file in &discovery.files {
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
        }
    }
    let explicit_roots = discovery
        .explicit_roots
        .iter()
        .filter(|key| modules.contains_key(*key))
        .cloned()
        .collect::<FxHashSet<_>>();
    let roots = select_lint_roots(&candidate_keys, &modules, &explicit_roots);
    let mut workspace = LintWorkspace {
        sources,
        program: {
            program.modules.shrink_to_fit();
            program
        },
        modules,
        roots,
        input_errors,
    };

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
    let workers = worker_count(workspace.roots.len());
    thread::scope(|scope| {
        for _ in 0..workers {
            let next_root = &next_root;
            let tx = tx.clone();
            let linted_modules = &linted_modules;
            let workspace = &workspace;
            scope.spawn(move || {
                let mut bundle = workspace.program.clone();
                let type_program = Arc::new(bundle.clone());
                loop {
                    if cancellation_output().is_some() {
                        break;
                    }
                    let root_index = next_root.fetch_add(1, Ordering::Relaxed);
                    let Some(root) = workspace.roots.get(root_index) else {
                        break;
                    };
                    let root_results = lint_workspace_root(
                        &workspace,
                        root,
                        fix,
                        &mut bundle,
                        linted_modules,
                        &type_program,
                    );
                    if tx.send((root_index, root_results)).is_err() {
                        break;
                    }
                }
            });
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

fn lint_workspace_root(
    workspace: &LintWorkspace,
    root: &str,
    fix: bool,
    bundle: &mut ArenaProgram,
    linted_modules: &Mutex<FxHashSet<String>>,
    type_program: &Arc<ArenaProgram>,
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
    if !relevant_diagnostics.is_empty() {
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

    let checked = SymbolOwner::new().with_current(|| {
        xsh::frontend::check::Checker::check_arena_with_options_and_type_program(
            bundle,
            &root_module.text,
            CheckOptions::default(),
            type_program.clone(),
        )
    });
    if !fix && !checked.diagnostics.is_empty() {
        return vec![LintResult {
            index: 0,
            kind: LintResultKind::Diagnostics {
                status: 2,
                diagnostics: render_diagnostics_with_keys(&checked.diagnostics, &workspace.sources),
            },
        }];
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
        options.expr_types = checked.expr_types.clone();
        options.callable_effects = checked.callable_effects.clone();
        options.terminating_call_spans = checked.terminating_call_spans.clone();
        let linted = if key == root {
            Linter::lint(bundle, &module.text, options)
        } else {
            Linter::lint_module(bundle, &module.text, options)
        };
        let check_diagnostics = checked
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic_mentions_source(diagnostic, module.source_id))
            .cloned()
            .collect::<Vec<_>>();
        let result = if fix {
            lint_workspace_node_with_fixes(
                results.len(),
                module,
                &linted.diagnostics,
                &check_diagnostics,
                &workspace.sources,
            )
        } else if linted.diagnostics.is_empty() {
            LintResult {
                index: results.len(),
                kind: LintResultKind::Clean,
            }
        } else {
            LintResult {
                index: results.len(),
                kind: LintResultKind::Diagnostics {
                    status: lint_diagnostics_status(&linted.diagnostics),
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

fn worker_count(file_count: usize) -> usize {
    if file_count == 0 {
        return 0;
    }
    thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, file_count.min(2))
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
        let mut modules_by_key = self
            .program
            .modules
            .iter()
            .cloned()
            .map(|module| (module.key.clone(), module))
            .collect::<FxHashMap<_, _>>();
        program.modules = ordered_keys
            .into_iter()
            .filter(|key| key != root)
            .filter_map(|key| modules_by_key.remove(&key))
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
    sources: &SourceMap,
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
    let final_text =
        match apply_cst_fixes(&module.path.to_string_lossy(), &module.text, &fixes, config) {
            Ok(Some(text)) => text,
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
    let remaining = match validate_fixed_text(
        &module.path.to_string_lossy(),
        &final_text,
        config,
        check_diagnostics,
    ) {
        Ok(diagnostics) => diagnostics,
        Err(FixedTextValidationError::Diagnostics(diagnostics)) => {
            return LintResult {
                index,
                kind: LintResultKind::FixDiagnostics {
                    status: 2,
                    diagnostics,
                    stderr: String::new(),
                },
            };
        }
    };
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
        terminating_call_spans: Default::default(),
        dead_code: !is_path_excluded(
            &tool_config.config_dir,
            Path::new(file),
            &tool_config.config.dead_code.exclude,
        ),
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
    lint_options.expr_types = checked.expr_types.clone();
    lint_options.callable_effects = checked.callable_effects.clone();
    lint_options.terminating_call_spans = checked.terminating_call_spans.clone();
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

    let final_text = match apply_cst_fixes(file, &text, &ast_fixes, config) {
        Ok(Some(text)) => text,
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
    let remaining_check_diagnostics =
        match validate_fixed_text(file, &final_text, config, &checked.diagnostics) {
            Ok(diagnostics) => diagnostics,
            Err(FixedTextValidationError::Diagnostics(diagnostics)) => {
                return LintResult {
                    index,
                    kind: LintResultKind::FixDiagnostics {
                        status: 2,
                        diagnostics,
                        stderr: String::new(),
                    },
                };
            }
        };

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
    Ok(render_diagnostics_with_keys(
        &checked.diagnostics,
        &checked_program.sources,
    ))
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
        ConfigCache, LintResultKind, ResolvedLintConfig, apply_cst_fixes, collect_fix_spans,
        lint_config_for_file, lint_one_file_with_fixes,
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
