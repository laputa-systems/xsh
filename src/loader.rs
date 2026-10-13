use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticRenderer, Label};
use crate::modules::api_spec;
use crate::sema::check::{
    CheckOptions, CheckOutput, Checker, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
    ErrorFamilyInfo,
};
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::{Name, QualifiedName};
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaProgram, ArenaProgramBuilder, ArenaRange, ArenaStmtKind, StmtId, UseStmtId,
};
use crate::syntax::cst::{LazyCst, SyntaxTree};

use crate::syntax::parser::{ArenaParseOutput, Parser};
use crate::syntax::token::TokenTable;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct EntrySource {
    pub sources: SourceMap,
    pub source_id: SourceId,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug)]
pub struct CheckedEntry {
    pub sources: SourceMap,
    pub entry_source_id: SourceId,
    pub parsed: ArenaParseOutput,
    pub checked: Option<CheckOutput>,
}

#[derive(Clone, Debug)]
pub struct CompactFileUnit {
    source_id: SourceId,
    display_path: String,
    parsed: ArenaParseOutput,
    imports: Vec<CompactFileImport>,
    exports: Vec<CompactFileExport>,
    declaration_summary: CompactFileDeclarationSummary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactFileImport {
    pub statement: StmtId,
    pub path: Vec<Name>,
    pub alias: Option<Name>,
    pub resolved: Option<Arc<str>>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactFileExport {
    pub statement: StmtId,
    pub exported: StmtId,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompactFileDeclarationSummary {
    pub type_defs: usize,
    pub error_defs: usize,
    pub proc_defs: usize,
    pub pure_defs: usize,
    pub stream_defs: usize,
    pub module_contract_entries: usize,
}

#[derive(Clone, Debug)]
pub struct CompactModuleGraph {
    import_edges: Vec<CompactModuleImportEdge>,
    module_aliases: BTreeMap<Name, String>,
    qualified_procs: BTreeMap<QualifiedName, CompactFunctionSig>,
    qualified_pures: BTreeMap<QualifiedName, CompactFunctionSig>,
    qualified_streams: BTreeMap<QualifiedName, CompactFunctionSig>,
    qualified_error_families: BTreeMap<QualifiedName, ErrorFamilyInfo>,
    type_defs: BTreeMap<Name, CompactTypeDefInfo>,
    exported_top_level_bindings: BTreeSet<Name>,
    diagnostics: Vec<Diagnostic>,
    source_order: Vec<(SourceId, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactModuleImportEdge {
    pub importer: SourceId,
    pub statement: StmtId,
    pub path: Vec<Name>,
    pub alias: Option<Name>,
    pub resolved: Option<Arc<str>>,
    pub span: Span,
}

impl CompactFileUnit {
    pub fn new(
        display_path: impl Into<String>,
        source_id: SourceId,
        parsed: ArenaParseOutput,
    ) -> Self {
        let imports = collect_compact_file_imports(&parsed.arena);
        let exports = collect_compact_file_exports(&parsed.arena);
        let declaration_summary = compact_file_declaration_summary(&parsed.arena);
        Self {
            source_id,
            display_path: display_path.into(),
            parsed,
            imports,
            exports,
            declaration_summary,
        }
    }

    pub fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub fn display_path(&self) -> &str {
        &self.display_path
    }

    pub fn parsed(&self) -> &ArenaParseOutput {
        &self.parsed
    }

    pub fn into_parsed(self) -> ArenaParseOutput {
        self.parsed
    }

    pub fn program(&self) -> &ArenaProgram {
        &self.parsed.arena
    }

    pub fn cst(&self) -> &SyntaxTree {
        self.parsed.cst.get()
    }

    pub fn token_table(&self) -> &TokenTable {
        self.parsed.cst.token_table()
    }

    pub fn parse_diagnostics(&self) -> &[Diagnostic] {
        &self.parsed.diagnostics
    }

    pub fn imports(&self) -> &[CompactFileImport] {
        &self.imports
    }

    pub fn exports(&self) -> &[CompactFileExport] {
        &self.exports
    }

    pub fn declaration_summary(&self) -> CompactFileDeclarationSummary {
        self.declaration_summary
    }

    pub fn root_statements(&self) -> impl Iterator<Item = StmtId> + '_ {
        self.parsed.arena.statement_ids()
    }

    pub fn module_statements(&self) -> impl Iterator<Item = (Name, StmtId)> + '_ {
        self.parsed.arena.modules.iter().flat_map(|module| {
            let name = module.name;
            self.parsed
                .arena
                .module_statements(module)
                .map(move |stmt| (name, stmt))
        })
    }
}

impl CompactModuleGraph {
    pub fn from_file_unit(file: &CompactFileUnit, declarations: &CompactDeclOutput) -> Self {
        let import_edges = file
            .imports()
            .iter()
            .map(|import| CompactModuleImportEdge {
                importer: file.source_id(),
                statement: import.statement,
                path: import.path.clone(),
                alias: import.alias,
                resolved: import.resolved.clone(),
                span: import.span,
            })
            .collect::<Vec<_>>();
        let module_aliases = import_edges
            .iter()
            .filter_map(|edge| {
                let alias = edge.alias.or_else(|| edge.path.last().copied())?;
                let resolved = edge
                    .resolved
                    .as_deref()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| {
                        edge.path
                            .iter()
                            .map(|name| name.as_str().to_string())
                            .collect::<Vec<_>>()
                            .join(".")
                    });
                Some((alias, resolved))
            })
            .collect::<BTreeMap<_, _>>();
        let mut diagnostics = declarations.diagnostics.clone();
        diagnostics.extend(file.parse_diagnostics().iter().cloned());
        sort_diagnostics_by_source(&mut diagnostics);

        Self {
            import_edges,
            module_aliases,
            qualified_procs: declarations.qualified_procs.clone().into_iter().collect(),
            qualified_pures: declarations.qualified_pures.clone().into_iter().collect(),
            qualified_streams: declarations.qualified_streams.clone().into_iter().collect(),
            qualified_error_families: declarations
                .qualified_error_families
                .clone()
                .into_iter()
                .collect(),
            type_defs: declarations.types.clone().into_iter().collect(),
            exported_top_level_bindings: collect_exported_top_level_binding_names(file.program()),
            diagnostics,
            source_order: compact_module_source_order(file),
        }
    }

    pub fn import_edges(&self) -> &[CompactModuleImportEdge] {
        &self.import_edges
    }

    pub fn module_aliases(&self) -> &BTreeMap<Name, String> {
        &self.module_aliases
    }

    pub fn qualified_procs(&self) -> &BTreeMap<QualifiedName, CompactFunctionSig> {
        &self.qualified_procs
    }

    pub fn qualified_pures(&self) -> &BTreeMap<QualifiedName, CompactFunctionSig> {
        &self.qualified_pures
    }

    pub fn qualified_streams(&self) -> &BTreeMap<QualifiedName, CompactFunctionSig> {
        &self.qualified_streams
    }

    pub fn qualified_error_families(&self) -> &BTreeMap<QualifiedName, ErrorFamilyInfo> {
        &self.qualified_error_families
    }

    pub fn type_defs(&self) -> &BTreeMap<Name, CompactTypeDefInfo> {
        &self.type_defs
    }

    pub fn exported_top_level_bindings(&self) -> &BTreeSet<Name> {
        &self.exported_top_level_bindings
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn source_order(&self) -> &[(SourceId, String)] {
        &self.source_order
    }

    pub fn qualified_declaration_count(&self) -> usize {
        self.qualified_procs.len()
            + self.qualified_pures.len()
            + self.qualified_streams.len()
            + self.qualified_error_families.len()
    }
}

impl CheckedEntry {
    pub fn entry_source_text(&self) -> Option<&str> {
        self.sources
            .get(self.entry_source_id)
            .map(|source| source.text())
    }

    pub fn check_diagnostics(&self) -> &[Diagnostic] {
        self.checked
            .as_ref()
            .map(|checked| checked.diagnostics.as_slice())
            .unwrap_or(&[])
    }

    pub fn render_parse_diagnostics(&self) -> String {
        DiagnosticRenderer::new().render(&self.parsed.diagnostics, &self.sources)
    }

    pub fn render_check_diagnostics(&self) -> String {
        DiagnosticRenderer::new().render(self.check_diagnostics(), &self.sources)
    }
}

pub fn parse_script(script: &str) -> Result<(SourceMap, ArenaParseOutput), std::io::Error> {
    parse_script_with_module_roots(script, &[])
}

pub fn parse_script_with_module_roots(
    script: &str,
    module_roots: &[PathBuf],
) -> Result<(SourceMap, ArenaParseOutput), std::io::Error> {
    let bytes = fs::read(script)?;
    Ok(parse_load_entry_source_arena_only(
        script,
        entry_source_from_bytes(script, bytes),
        module_roots.to_vec(),
    ))
}

pub fn entry_source_from_text(file: &str, text: String) -> EntrySource {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(file, text);
    EntrySource {
        sources,
        source_id,
        diagnostics: Vec::new(),
    }
}

pub fn entry_source_from_bytes(file: &str, bytes: Vec<u8>) -> EntrySource {
    let mut sources = SourceMap::new();
    let (source_id, diagnostics) = add_source_bytes(&mut sources, file, bytes);
    EntrySource { sources, source_id, diagnostics }
}

/// Add source bytes without crossing an invalid UTF-8 boundary silently.
/// Lossy text is retained only so the diagnostic can point at the bad bytes.
pub fn add_source_bytes(
    sources: &mut SourceMap,
    file: &str,
    bytes: Vec<u8>,
) -> (SourceId, Vec<Diagnostic>) {
    match sources.add_file_from_utf8(file, bytes.clone()) {
        Ok(source_id) => (source_id, Vec::new()),
        Err(error) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let offset = error.offset.min(text.len());
            let source_id = sources.add_file(file, text);
            (source_id, vec![
                Diagnostic::error("source file is not valid UTF-8")
                    .with_code(DiagnosticCode::SourceInvalidUtf8)
                    .with_label(Label::primary(
                        Span::new(source_id, offset, offset),
                        "invalid UTF-8 starts here",
                    )),
            ])
        }
    }
}

fn compact_file_statement_ids(program: &ArenaProgram) -> Vec<StmtId> {
    let mut statements = program.statement_ids().collect::<Vec<_>>();
    for module in &program.modules {
        statements.extend(program.module_statements(module));
    }
    statements
}

fn collect_compact_file_imports(program: &ArenaProgram) -> Vec<CompactFileImport> {
    compact_file_statement_ids(program)
        .into_iter()
        .filter_map(|statement| {
            let stmt = program.arena.stmt(statement);
            let ArenaStmtKind::Use(use_id) = stmt.kind else {
                return None;
            };
            let use_stmt = program.arena.use_stmt(use_id);
            Some(CompactFileImport {
                statement,
                path: program.arena.names(use_stmt.path).collect(),
                alias: use_stmt.alias,
                resolved: use_stmt.resolved.clone(),
                span: stmt.span,
            })
        })
        .collect()
}

fn collect_compact_file_exports(program: &ArenaProgram) -> Vec<CompactFileExport> {
    compact_file_statement_ids(program)
        .into_iter()
        .filter_map(|statement| {
            let stmt = program.arena.stmt(statement);
            let ArenaStmtKind::Export(exported) = stmt.kind else {
                return None;
            };
            Some(CompactFileExport {
                statement,
                exported,
                span: stmt.span,
            })
        })
        .collect()
}

fn compact_file_declaration_summary(program: &ArenaProgram) -> CompactFileDeclarationSummary {
    let mut summary = CompactFileDeclarationSummary::default();
    for statement in compact_file_statement_ids(program) {
        add_compact_file_declaration(program, statement, &mut summary);
    }
    summary
}

fn add_compact_file_declaration(
    program: &ArenaProgram,
    statement: StmtId,
    summary: &mut CompactFileDeclarationSummary,
) {
    match program.arena.stmt(statement).kind {
        ArenaStmtKind::Export(inner) => add_compact_file_declaration(program, inner, summary),
        ArenaStmtKind::TypeDef(def) => {
            summary.type_defs += 1;
            if let crate::syntax::arena::ArenaTypeDefBody::ModuleContract { entries, .. } =
                program.arena.type_def(def).body
            {
                summary.module_contract_entries +=
                    program.arena.module_contract_entries(entries).len();
            }
        }
        ArenaStmtKind::ErrorDef(_) => summary.error_defs += 1,
        ArenaStmtKind::ProcDef(_) | ArenaStmtKind::CliMain(_) => summary.proc_defs += 1,
        ArenaStmtKind::PureDef(_) => summary.pure_defs += 1,
        ArenaStmtKind::StreamDef(_) => summary.stream_defs += 1,
        _ => {}
    }
}

fn collect_exported_top_level_binding_names(program: &ArenaProgram) -> BTreeSet<Name> {
    let mut names = BTreeSet::new();
    for statement in program.statement_ids() {
        collect_exported_top_level_binding_name(program, statement, &mut names);
    }
    names
}

fn compact_module_source_order(file: &CompactFileUnit) -> Vec<(SourceId, String)> {
    let mut sources = vec![(file.source_id(), file.display_path().to_string())];
    for module in &file.program().modules {
        let Some(first_statement) = file.program().module_statements(module).next() else {
            continue;
        };
        sources.push((
            file.program().arena.stmt(first_statement).span.source_id,
            module.key.clone(),
        ));
    }
    sources.sort_by_key(|(source_id, display)| (*source_id, display.clone()));
    sources
}

fn collect_exported_top_level_binding_name(
    program: &ArenaProgram,
    statement: StmtId,
    names: &mut BTreeSet<Name>,
) {
    let ArenaStmtKind::Export(inner) = program.arena.stmt(statement).kind else {
        return;
    };
    match program.arena.stmt(inner).kind {
        ArenaStmtKind::Let { target, .. }
        | ArenaStmtKind::Const { target, .. }
        | ArenaStmtKind::Var { target, .. } => {
            if let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind {
                names.insert(name);
            }
        }
        ArenaStmtKind::ProcDef(def)
        | ArenaStmtKind::PureDef(def)
        | ArenaStmtKind::StreamDef(def) => {
            names.insert(program.arena.function_def(def).name);
        }
        ArenaStmtKind::TypeDef(def) => {
            names.insert(program.arena.type_def(def).name);
        }
        ArenaStmtKind::ErrorDef(def) => {
            names.insert(program.arena.error_def(def).name);
        }
        ArenaStmtKind::Use(_)
        | ArenaStmtKind::Export(_)
        | ArenaStmtKind::Assign { .. }
        | ArenaStmtKind::CliMain(_)
        | ArenaStmtKind::SignalHook(_)
        | ArenaStmtKind::Return(_)
        | ArenaStmtKind::Yield(_)
        | ArenaStmtKind::YieldDelegate(_)
        | ArenaStmtKind::Defer(..)
        | ArenaStmtKind::If { .. }
        | ArenaStmtKind::While { .. }
        | ArenaStmtKind::For { .. }
        | ArenaStmtKind::With { .. }
        | ArenaStmtKind::Loop { .. }
        | ArenaStmtKind::Guard { .. }
        | ArenaStmtKind::Sugar { .. }
        | ArenaStmtKind::Assert { .. }
        | ArenaStmtKind::Break { .. }
        | ArenaStmtKind::Continue
        | ArenaStmtKind::Exit(_)
        | ArenaStmtKind::Match { .. }
        | ArenaStmtKind::Command(_)
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Expr(_) => {}
    }
}

fn sort_diagnostics_by_source(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by_key(|diagnostic| {
        let span = diagnostic_primary_span(diagnostic);
        (
            span.map(|span| span.source_id)
                .unwrap_or_else(|| SourceId::new(u32::MAX as usize)),
            span.map(|span| span.start()).unwrap_or(usize::MAX),
            span.map(|span| span.end()).unwrap_or(usize::MAX),
            diagnostic.code.map(DiagnosticCode::name),
            diagnostic.message.clone(),
        )
    });
}

fn diagnostic_primary_span(diagnostic: &Diagnostic) -> Option<Span> {
    diagnostic
        .span
        .or_else(|| diagnostic.labels.first().map(|label| label.span))
}

pub fn parse_load_check_text(
    file: &str,
    text: String,
    module_roots: Vec<PathBuf>,
    options: CheckOptions,
) -> CheckedEntry {
    parse_load_check_entry_source(
        file,
        entry_source_from_text(file, text),
        module_roots,
        options,
    )
}

pub fn parse_load_check_bytes(
    file: &str,
    bytes: Vec<u8>,
    module_roots: Vec<PathBuf>,
    options: CheckOptions,
) -> CheckedEntry {
    parse_load_check_entry_source(
        file,
        entry_source_from_bytes(file, bytes),
        module_roots,
        options,
    )
}

pub fn parse_load_check_file(
    file: &str,
    module_roots: Vec<PathBuf>,
    options: CheckOptions,
) -> Result<CheckedEntry, std::io::Error> {
    let bytes = fs::read(file)?;
    Ok(parse_load_check_bytes(file, bytes, module_roots, options))
}

pub fn parse_load_check_entry_source(
    file: &str,
    entry_source: EntrySource,
    module_roots: Vec<PathBuf>,
    options: CheckOptions,
) -> CheckedEntry {
    parse_load_check_entry_source_with_token_table(file, entry_source, module_roots, options, None)
}

pub fn parse_load_check_entry_source_with_token_table(
    file: &str,
    entry_source: EntrySource,
    module_roots: Vec<PathBuf>,
    options: CheckOptions,
    token_table: Option<TokenTable>,
) -> CheckedEntry {
    let _ = token_table;
    let entry_source_id = entry_source.source_id;
    let (sources, parsed) = parse_load_entry_source_arena_only(file, entry_source, module_roots);
    sources
        .get(entry_source_id)
        .expect("entry source id must exist in the source map");
    let ArenaParseOutput {
        arena,
        cst,
        diagnostics,
    } = parsed;
    let type_program = Arc::new(arena);
    let checked = diagnostics.is_empty().then(|| {
        Checker::check_arena_with_sources_options_and_type_program(
            &type_program,
            &sources,
            entry_source_id,
            options,
            Arc::clone(&type_program),
        )
    });
    let arena = match Arc::try_unwrap(type_program) {
        Ok(arena) => arena,
        Err(_) => panic!("checker retained the loader's type program"),
    };
    CheckedEntry {
        sources,
        entry_source_id,
        parsed: ArenaParseOutput {
            arena,
            cst,
            diagnostics,
        },
        checked,
    }
}

pub fn parse_load_entry_source_compact_file_unit(
    file: &str,
    entry_source: EntrySource,
    module_roots: Vec<PathBuf>,
) -> (SourceMap, CompactFileUnit) {
    let entry_source_id = entry_source.source_id;
    let (sources, parsed) = parse_load_entry_source_arena_only(file, entry_source, module_roots);
    (sources, CompactFileUnit::new(file, entry_source_id, parsed))
}

/// How an entry source links standard-library implementation modules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StdlibLinkage {
    /// Attach every embedded implementation module the program can reach, and
    /// lower its standard calls to those prepared functions.
    Prepare,
    /// Leave embedded sources out of this program and lower its standard calls
    /// to implementations a loading program already prepared.
    ///
    /// Used for dynamically loaded user modules: the loading program prepared
    /// the complete applicable set before execution, so the loaded module must
    /// not reparse standard-library source to reach the same functions.
    LinkToPrepared,
}

pub fn parse_load_entry_source_arena_only(
    file: &str,
    entry_source: EntrySource,
    module_roots: Vec<PathBuf>,
) -> (SourceMap, ArenaParseOutput) {
    parse_load_entry_source_arena_only_with_linkage(
        file,
        entry_source,
        module_roots,
        StdlibLinkage::Prepare,
    )
}

pub fn parse_load_entry_source_arena_only_with_linkage(
    file: &str,
    entry_source: EntrySource,
    module_roots: Vec<PathBuf>,
    linkage: StdlibLinkage,
) -> (SourceMap, ArenaParseOutput) {
    let EntrySource {
        mut sources,
        source_id,
        diagnostics,
    } = entry_source;
    if !diagnostics.is_empty() {
        return (
            sources,
            ArenaParseOutput {
                arena: Default::default(),
                cst: LazyCst::empty(source_id),
                diagnostics,
            },
        );
    }
    let text = sources
        .get(source_id)
        .expect("source was just inserted")
        .text();
    let token_capacity = text.len() / 4 + 1;
    let mut builder = ArenaProgramBuilder::with_token_capacity(token_capacity);
    let root = Parser::parse_source_into_arena_builder(source_id, text, &mut builder);
    let cst = root.cst;
    let mut diagnostics = root.diagnostics;
    let root_statements = root.statements;
    let mut loaded = FxHashSet::default();
    if diagnostics.is_empty() {
        let mut loader = ModuleLoader::new(&mut sources, &mut builder);
        let imports = loader.load_imports(Path::new(file), root_statements, &module_roots);
        runtime_import_diagnostics(&imports, &loader.finish(), &mut loaded, &mut diagnostics);
        if linkage == StdlibLinkage::Prepare {
            let mut embedded = EmbeddedModuleLoader::new(&mut sources, &mut builder);
            embedded.load_stdlib_modules(source_id);
            diagnostics.extend(embedded.diagnostics);
        }
    }
    let mut arena = builder.finish_with_statements(root_statements);
    arena.modules.retain(|module| module.internal || loaded.contains(&module.key));
    (
        sources,
        ArenaParseOutput {
            arena,
            cst,
            diagnostics,
        },
    )
}

/// Arena-only load into a shared source map (no old `Program`). Used by tooling
/// that checks several entry files against one `SourceMap` (e.g. `xsht check`).
pub fn parse_load_entry_source_shared_arena_only(
    file: &str,
    source_id: SourceId,
    sources: &mut SourceMap,
    module_roots: Vec<PathBuf>,
) -> ArenaParseOutput {
    let text = sources
        .get(source_id)
        .expect("source must be in shared map")
        .text();
    let token_capacity = text.len() / 4 + 1;
    let mut builder = ArenaProgramBuilder::with_token_capacity(token_capacity);
    let root = Parser::parse_source_into_arena_builder(source_id, text, &mut builder);
    let cst = root.cst;
    let mut diagnostics = root.diagnostics;
    let root_statements = root.statements;
    let mut loaded = FxHashSet::default();
    if diagnostics.is_empty() {
        let mut loader = ModuleLoader::new(sources, &mut builder);
        let imports = loader.load_imports(Path::new(file), root_statements, &module_roots);
        runtime_import_diagnostics(&imports, &loader.finish(), &mut loaded, &mut diagnostics);
        // Tooling that checks or lints a workspace must see the same standard
        // implementations the runner would prepare, or a script-backed call
        // would look unlowerable to the checker while running fine.
        let mut embedded = EmbeddedModuleLoader::new(sources, &mut builder);
        embedded.load_stdlib_modules(source_id);
        diagnostics.extend(embedded.diagnostics);
    }
    let mut arena = builder.finish_with_statements(root_statements);
    arena.modules.retain(|module| module.internal || loaded.contains(&module.key));
    ArenaParseOutput {
        arena,
        cst,
        diagnostics,
    }
}

/// Source bytes and import roots for one entry in a shared tooling workspace.
pub struct ArenaEntryInput {
    pub file: String,
    pub bytes: Vec<u8>,
    pub module_roots: Vec<PathBuf>,
}

/// One entry's root range and source-local loading result in a shared arena.
pub struct LoadedArenaEntry {
    pub file: String,
    pub source_id: SourceId,
    pub statements: ArenaRange,
    pub cst: LazyCst,
    pub diagnostics: Vec<Diagnostic>,
}

/// Several entry roots and their common imports in one immutable arena/session.
pub struct LoadedArenaWorkspace {
    pub sources: Arc<SourceMap>,
    pub program: Arc<ArenaProgram>,
    pub entries: Vec<LoadedArenaEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadCancelled;

/// Parse entry roots into one arena and resolve shared imports once.
///
/// Each entry keeps its own loading diagnostics while dependencies are added
/// only once to the common arena. Callers create a new workspace after any
/// source edit so cached semantic facts remain tied to these exact identities.
pub fn parse_load_entries_shared_arena_only(
    inputs: Vec<ArenaEntryInput>,
) -> LoadedArenaWorkspace {
    parse_load_entries_shared_arena_only_with_cancellation(inputs, || false)
        .expect("unconditional shared workspace loading cannot be cancelled")
}

pub fn parse_load_entries_shared_arena_only_with_cancellation(
    inputs: Vec<ArenaEntryInput>,
    mut cancellation_requested: impl FnMut() -> bool,
) -> Result<LoadedArenaWorkspace, LoadCancelled> {
    check_load_cancellation(&mut cancellation_requested)?;
    let mut sources = SourceMap::new();
    let token_capacity = inputs
        .iter()
        .map(|input| input.bytes.len() / 4 + 1)
        .sum();
    let mut builder = ArenaProgramBuilder::with_token_capacity(token_capacity);
    let mut entries = Vec::with_capacity(inputs.len());
    let mut entry_loads = Vec::with_capacity(inputs.len());
    for input in inputs {
        check_load_cancellation(&mut cancellation_requested)?;
        let (source_id, mut diagnostics) = add_source_bytes(&mut sources, &input.file, input.bytes);
        let source = sources
            .get(source_id)
            .expect("entry source was just inserted")
            .text();
        let parsed = if diagnostics.is_empty() {
            Parser::parse_source_into_arena_builder(source_id, source, &mut builder)
        } else {
            crate::syntax::parser::ArenaParseFragment {
                statements: ArenaRange::default(),
                cst: LazyCst::empty(source_id),
                diagnostics: Vec::new(),
            }
        };
        check_load_cancellation(&mut cancellation_requested)?;
        diagnostics.extend(parsed.diagnostics);
        entry_loads.push((input.file.clone(), input.module_roots, diagnostics.is_empty()));
        entries.push(LoadedArenaEntry {
            file: input.file,
            source_id,
            statements: parsed.statements,
            cst: parsed.cst,
            diagnostics,
        });
    }

    let mut loaded = FxHashSet::default();
    {
        let mut loader = ModuleLoader::new_shared_workspace(&mut sources, &mut builder);
        let mut root_imports = Vec::with_capacity(entries.len());
        for ((file, module_roots, load_imports), entry) in
            entry_loads.iter().zip(&entries)
        {
            root_imports.push(if *load_imports {
                loader.load_imports_with_cancellation(
                    Path::new(file),
                    entry.statements,
                    module_roots,
                    &mut cancellation_requested,
                )?
            } else {
                Vec::new()
            });
        }
        let modules = loader.finish();
        for (entry, imports) in entries.iter_mut().zip(&root_imports) {
            runtime_import_diagnostics(imports, &modules, &mut loaded, &mut entry.diagnostics);
            loaded.clear();
        }
    }

    if let Some(entry) = entries.first() {
        check_load_cancellation(&mut cancellation_requested)?;
        let mut embedded = EmbeddedModuleLoader::new(&mut sources, &mut builder);
        embedded.load_stdlib_modules(entry.source_id);
        for entry in &mut entries {
            entry.diagnostics.extend(embedded.diagnostics.iter().cloned());
        }
    }

    check_load_cancellation(&mut cancellation_requested)?;

    let root_statements = entries
        .first()
        .map_or_else(ArenaRange::default, |entry| entry.statements);
    let program = builder.finish_with_statements(root_statements);
    Ok(LoadedArenaWorkspace {
        sources: Arc::new(sources),
        program: Arc::new(program),
        entries,
    })
}

fn check_load_cancellation(
    cancellation_requested: &mut impl FnMut() -> bool,
) -> Result<(), LoadCancelled> {
    if cancellation_requested() {
        Err(LoadCancelled)
    } else {
        Ok(())
    }
}

/// A parsed file and its imports in one shared source map and arena.
/// Parser recovery is retained for syntax migration; execution checks only
/// files reachable through imports whose source parsed successfully.
pub struct LoadedModule {
    pub key: String,
    pub path: PathBuf,
    pub source_id: SourceId,
    pub text: String,
    pub statements: ArenaRange,
    pub imports: Vec<LoadedImport>,
    pub diagnostics: Vec<Diagnostic>,
    pub module_roots: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct LoadedImport {
    pub use_id: UseStmtId,
    pub path: Vec<Name>,
    pub span: Span,
    pub target: Option<String>,
    pub diagnostic: Option<Diagnostic>,
}

/// Loads each user file once, including files shared by several entry roots.
/// The caller keeps the source map and arena so entry parsing and embedded
/// implementations use the same source and syntax identities.
pub struct ModuleLoader<'a, 'b> {
    sources: &'a mut SourceMap,
    arena: &'a mut ArenaProgramBuilder<'b>,
    modules: FxHashMap<String, LoadedModule>,
    stack: Vec<String>,
    source_overrides: FxHashMap<String, Vec<u8>>,
    resolution_scoped: bool,
    primary_contexts: FxHashMap<String, Vec<PathBuf>>,
    contextual_modules: FxHashMap<(String, Vec<PathBuf>), String>,
    context_ids: FxHashMap<Vec<PathBuf>, usize>,
    next_context_id: usize,
}

impl<'a, 'b> ModuleLoader<'a, 'b> {
    pub fn new(sources: &'a mut SourceMap, arena: &'a mut ArenaProgramBuilder<'b>) -> Self {
        Self::new_with_resolution_scope(sources, arena, false)
    }

    fn new_shared_workspace(
        sources: &'a mut SourceMap,
        arena: &'a mut ArenaProgramBuilder<'b>,
    ) -> Self {
        Self::new_with_resolution_scope(sources, arena, true)
    }

    fn new_with_resolution_scope(
        sources: &'a mut SourceMap,
        arena: &'a mut ArenaProgramBuilder<'b>,
        resolution_scoped: bool,
    ) -> Self {
        Self {
            sources,
            arena,
            modules: FxHashMap::default(),
            stack: Vec::new(),
            source_overrides: FxHashMap::default(),
            resolution_scoped,
            primary_contexts: FxHashMap::default(),
            contextual_modules: FxHashMap::default(),
            context_ids: FxHashMap::default(),
            next_context_id: 1,
        }
    }

    /// Reparse edited bytes in a fresh loader during a syntax-fix round.
    pub fn set_source_override(&mut self, key: String, bytes: Vec<u8>) {
        self.source_overrides.insert(key, bytes);
    }

    pub fn load(&mut self, path: PathBuf, bytes: Vec<u8>, module_roots: Vec<PathBuf>) -> String {
        let mut never_cancel = || false;
        self.load_with_cancellation(path, bytes, module_roots, &mut never_cancel)
            .expect("unconditional module loading cannot be cancelled")
    }

    fn load_with_cancellation(
        &mut self,
        path: PathBuf,
        bytes: Vec<u8>,
        module_roots: Vec<PathBuf>,
        cancellation_requested: &mut impl FnMut() -> bool,
    ) -> Result<String, LoadCancelled> {
        check_load_cancellation(cancellation_requested)?;
        let physical_key = module_key(&path);
        let key = self.identity_key(&physical_key, &module_roots);
        if self.modules.contains_key(&key) {
            return Ok(key);
        }
        let bytes = self
            .source_overrides
            .get(&physical_key)
            .cloned()
            .unwrap_or(bytes);
        let (source_id, mut diagnostics) =
            add_source_bytes(self.sources, &path.to_string_lossy(), bytes);
        let text = self
            .sources
            .get(source_id)
            .expect("source was just inserted")
            .text()
            .to_string();
        let statements = if diagnostics.is_empty() {
            let fragment = Parser::parse_source_into_arena_builder(source_id, &text, self.arena);
            diagnostics.extend(fragment.diagnostics);
            fragment.statements
        } else {
            ArenaRange::default()
        };
        check_load_cancellation(cancellation_requested)?;
        self.modules.insert(
            key.clone(),
            LoadedModule {
                key: key.clone(),
                path: path.clone(),
                source_id,
                text,
                statements,
                imports: Vec::new(),
                diagnostics,
                module_roots: module_roots.clone(),
            },
        );
        self.stack.push(key.clone());
        let imports = self.load_imports_with_cancellation(
            &path,
            statements,
            &module_roots,
            cancellation_requested,
        );
        self.stack.pop();
        self.modules.get_mut(&key).expect("module was inserted").imports = imports?;
        let name = self.arena.name(&physical_key);
        self.arena.push_arena_module(key.clone(), name, statements);
        Ok(key)
    }

    fn identity_key(&mut self, physical_key: &str, module_roots: &[PathBuf]) -> String {
        if !self.resolution_scoped {
            return physical_key.to_string();
        }

        let identity = (physical_key.to_string(), module_roots.to_vec());
        if let Some(key) = self.contextual_modules.get(&identity) {
            return key.clone();
        }

        let key = if let Some(primary_context) = self.primary_contexts.get(physical_key) {
            debug_assert_ne!(primary_context, module_roots);
            let context_id = if let Some(context_id) = self.context_ids.get(module_roots) {
                *context_id
            } else {
                let context_id = self.next_context_id;
                self.next_context_id += 1;
                self.context_ids
                    .insert(module_roots.to_vec(), context_id);
                context_id
            };
            format!("{physical_key}\0{context_id}")
        } else {
            self.primary_contexts
                .insert(physical_key.to_string(), module_roots.to_vec());
            physical_key.to_string()
        };
        self.contextual_modules.insert(identity, key.clone());
        key
    }

    fn load_imports(
        &mut self,
        importer: &Path,
        statements: ArenaRange,
        module_roots: &[PathBuf],
    ) -> Vec<LoadedImport> {
        let mut never_cancel = || false;
        self.load_imports_with_cancellation(importer, statements, module_roots, &mut never_cancel)
            .expect("unconditional module loading cannot be cancelled")
    }

    fn load_imports_with_cancellation(
        &mut self,
        importer: &Path,
        statements: ArenaRange,
        module_roots: &[PathBuf],
        cancellation_requested: &mut impl FnMut() -> bool,
    ) -> Result<Vec<LoadedImport>, LoadCancelled> {
        let mut imports = Vec::new();
        for stmt in self.arena.statement_ids(statements) {
            check_load_cancellation(cancellation_requested)?;
            let Some((use_id, path, span)) = self.arena.use_stmt_for_statement(stmt) else {
                continue;
            };
            let mut import = LoadedImport {
                use_id,
                path,
                span,
                target: None,
                diagnostic: None,
            };
            match resolve_user_module(importer, &import.path, module_roots) {
                Ok(UserModuleResolution::Standard) => {}
                Ok(UserModuleResolution::Missing(paths)) => {
                    self.arena.set_use_searched_candidates(use_id, paths);
                }
                Ok(UserModuleResolution::Found { path, bytes }) => {
                    let target = self.identity_key(&module_key(&path), module_roots);
                    if self.stack.contains(&target) {
                        import.diagnostic = Some(
                            Diagnostic::error("cyclic module import")
                                .with_code(DiagnosticCode::ParseModuleCycle)
                                .with_label(Label::primary(span, "module import cycle starts here")),
                        );
                    } else {
                        self.load_with_cancellation(
                            path,
                            bytes,
                            module_roots.to_vec(),
                            cancellation_requested,
                        )?;
                    }
                    self.arena.set_use_resolved(use_id, Arc::from(target.as_str()));
                    import.target = Some(target);
                }
                Err(message) => {
                    import.diagnostic = Some(
                        Diagnostic::error("failed to read module")
                            .with_code(DiagnosticCode::ParseModuleRead)
                            .with_label(Label::primary(span, message)),
                    );
                }
            }
            imports.push(import);
        }
        Ok(imports)
    }

    pub fn finish(self) -> FxHashMap<String, LoadedModule> {
        self.modules
    }
}

/// Runtime loading stops reporting below a source that failed to parse.
/// Recovery files remain available to tooling without changing that boundary.
fn runtime_import_diagnostics(
    imports: &[LoadedImport],
    modules: &FxHashMap<String, LoadedModule>,
    loaded: &mut FxHashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for import in imports {
        if let Some(diagnostic) = &import.diagnostic {
            diagnostics.push(diagnostic.clone());
            continue;
        }
        let Some(target) = &import.target else {
            continue;
        };
        let module = modules.get(target).expect("resolved import was loaded");
        if !module.diagnostics.is_empty() {
            if !module
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::SourceInvalidUtf8))
            {
                diagnostics.push(
                    Diagnostic::error("failed to load module")
                        .with_code(DiagnosticCode::ParseModuleLoad)
                        .with_label(Label::primary(
                            import.span,
                            format!("`{}` has parse errors", module.path.display()),
                        )),
                );
            }
            diagnostics.extend(module.diagnostics.iter().cloned());
        } else if loaded.insert(module.key.clone()) {
            runtime_import_diagnostics(&module.imports, modules, loaded, diagnostics);
        }
    }
}

struct EmbeddedModuleLoader<'a, 'b> {
    sources: &'a mut SourceMap,
    arena: &'a mut ArenaProgramBuilder<'b>,
    diagnostics: Vec<Diagnostic>,
    loaded_stdlib: FxHashSet<&'static str>,
}

impl<'a, 'b> EmbeddedModuleLoader<'a, 'b> {
    fn new(sources: &'a mut SourceMap, arena: &'a mut ArenaProgramBuilder<'b>) -> Self {
        Self {
            sources,
            arena,
            diagnostics: Vec::new(),
            loaded_stdlib: FxHashSet::default(),
        }
    }

    /// Parse and attach every embedded implementation module the program can
    /// reach.
    ///
    /// Runs once per preparation, after the entry and its static user-module
    /// graph are parsed, so the decision sees every reference the program
    /// contains. Embedded sources never import user or project source, so
    /// attaching them here cannot pull in a file.
    fn load_stdlib_modules(&mut self, entry_source_id: SourceId) {
        let span = Span::new(entry_source_id, 0, 0);
        for identity in crate::stdlib::required_modules(self.arena.ast_arena()) {
            self.load_stdlib_module(identity, span);
        }
    }

    fn load_stdlib_module(&mut self, identity: &'static str, span: Span) {
        if !self.loaded_stdlib.insert(identity) {
            return;
        }
        #[cfg(feature = "native-tests")]
        crate::stdlib::counters::record_parsed_module();
        let Some(module) = crate::stdlib::find(identity) else {
            self.diagnostics.push(
                Diagnostic::error("embedded standard-library module is missing")
                    .with_code(DiagnosticCode::ParseStdlibCatalog)
                    .with_label(Label::primary(
                        span,
                        format!("the implementation catalog has no `{identity}` module"),
                    )),
            );
            return;
        };
        let source_name = module.label.to_string();
        let source_id = match self
            .sources
            .add_file_from_utf8(source_name.clone(), module.source.as_bytes().to_vec())
        {
            Ok(source_id) => source_id,
            Err(error) => {
                self.diagnostics.push(
                    Diagnostic::error("embedded standard-library source is not valid UTF-8")
                        .with_code(DiagnosticCode::SourceInvalidUtf8)
                        .with_label(Label::primary(
                            Span::new(span.source_id, error.offset, error.offset),
                            module.label,
                        )),
                );
                return;
            }
        };
        let namespace = self.arena.name(&crate::stdlib::namespace_text(identity));
        let text = self
            .sources
            .get(source_id)
            .expect("embedded source was just inserted")
            .text();
        let parsed = self.arena.with_internal_source(|arena| {
            Parser::parse_source_into_arena_builder(source_id, text, arena)
        });
        if !parsed.diagnostics.is_empty() {
            self.diagnostics.extend(parsed.diagnostics);
            return;
        }
        self.arena.push_internal_arena_module(
            module.label.to_string(),
            namespace,
            parsed.statements,
        );
    }

}

/// Prepare one embedded implementation module for catalog validation.
///
/// The module is parsed exactly as preparation parses it — as an internal
/// implementation module attached to an otherwise empty program, never as a
/// user entry — so the catalog gate validates the bytes the runtime will see.
/// A module that references another embedded module's entry pulls that one in
/// through the same selection the ordinary preparation uses.
pub fn prepare_stdlib_catalog_module(identity: &str) -> Option<(SourceMap, ArenaParseOutput)> {
    let module = crate::stdlib::find(identity)?;
    let mut sources = SourceMap::new();
    let entry_source_id = sources.add_file("<catalog-validation>", String::new());
    let mut builder = ArenaProgramBuilder::with_token_capacity(0);
    let mut diagnostics = Vec::new();
    {
        let mut loader = EmbeddedModuleLoader::new(&mut sources, &mut builder);
        loader.load_stdlib_module(module.identity, Span::new(entry_source_id, 0, 0));
        loader.load_stdlib_modules(entry_source_id);
        diagnostics.extend(loader.diagnostics);
    }
    let arena = builder.finish();
    Some((
        sources,
        ArenaParseOutput {
            arena,
            cst: LazyCst::empty(entry_source_id),
            diagnostics,
        },
    ))
}

pub fn module_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// The filesystem outcome of resolving one import. Absence remains checker
/// input; a candidate that cannot be read is a loading error.
pub enum UserModuleResolution {
    Standard,
    Found { path: PathBuf, bytes: Vec<u8> },
    Missing(Arc<[PathBuf]>),
}

/// Resolve a user import using the language's search order. A standard module
/// has no source file, and missing candidates retain their actual search paths.
pub fn resolve_user_module(
    importer: &Path,
    path: &[Name],
    extra_roots: &[PathBuf],
) -> Result<UserModuleResolution, String> {
    if path.len() == 1 && api_spec().is_standard_module(&path[0].as_str()) {
        return Ok(UserModuleResolution::Standard);
    }
    let candidates = resolve_module_path_candidates(importer, path, extra_roots);
    for candidate in &candidates {
        match fs::read(candidate) {
            Ok(bytes) => return Ok(UserModuleResolution::Found { path: candidate.clone(), bytes }),
            Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => {}
            Err(error) => return Err(format!("`{}`: {error}", candidate.display())),
        }
    }
    Ok(UserModuleResolution::Missing(candidates.into()))
}

/// Where a `use` may load from, in search order: beside the importing file,
/// then each `XSH_MODULE_PATH` directory, then each project module root.
/// Every loader, in every binary, searches in this order.
#[allow(clippy::single_call_fn)]
fn resolve_module_path_candidates(
    importer: &Path,
    path: &[Name],
    project_roots: &[PathBuf],
) -> Vec<PathBuf> {
    let mut candidates = vec![module_path_from_base(
        importer.parent().unwrap_or_else(|| Path::new(".")),
        path,
    )];
    if let Some(paths) = std::env::var_os("XSH_MODULE_PATH") {
        for base in std::env::split_paths(&paths) {
            candidates.push(module_path_from_base(&base, path));
        }
    }
    for base in project_roots {
        candidates.push(module_path_from_base(base, path));
    }
    let mut seen = FxHashSet::default();
    let mut output = Vec::new();
    for path in candidates {
        let key = path.to_string_lossy().into_owned();
        if seen.insert(key) {
            output.push(path);
        }
    }
    output
}

fn module_path_from_base(base: &Path, path: &[Name]) -> PathBuf {
    let mut base = base.to_path_buf();
    for segment in path {
        base.push(segment.as_str());
    }
    base.set_extension("xsh");
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_entry_loader_shares_imports_and_keeps_root_ranges() {
        let root = tempfile::TempDir::new().expect("create module fixture");
        let first = root.path().join("first.xsh");
        let second = root.path().join("second.xsh");
        let shared = root.path().join("shared.xsh");
        fs::write(&shared, "export pure value() -> Int { 1 }\n")
            .expect("write shared module");

        let loaded = parse_load_entries_shared_arena_only(vec![
            ArenaEntryInput {
                file: first.to_string_lossy().into_owned(),
                bytes: b"use shared\nconst first = 1\n".to_vec(),
                module_roots: Vec::new(),
            },
            ArenaEntryInput {
                file: second.to_string_lossy().into_owned(),
                bytes: b"use shared\nconst second = 2\n".to_vec(),
                module_roots: Vec::new(),
            },
        ]);

        assert_eq!(loaded.entries.len(), 2);
        assert_ne!(loaded.entries[0].source_id, loaded.entries[1].source_id);
        assert_ne!(loaded.entries[0].statements, loaded.entries[1].statements);
        assert!(loaded.entries.iter().all(|entry| {
            entry.diagnostics.is_empty()
                && loaded.sources.get(entry.source_id).is_some_and(|source| {
                    source.text().contains("use shared")
                })
        }));
        assert_eq!(loaded.program.modules.len(), 1);
        assert_eq!(loaded.program.modules[0].key, module_key(&shared));
    }

    // Search provenance is exposed through the Rust frontend, so the host
    // test compares loaded imports and imports parsed without resolution.
    #[test]
    fn only_loaded_missing_imports_record_filesystem_candidates() {
        let root = tempfile::TempDir::new().expect("create module fixture");
        let entry = root.path().join("entry.xsh");
        let module_root = root.path().join("modules");
        let source = "use unavailable_module\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let parsed_use = parsed.arena.arena.use_stmt(
            match parsed.arena.arena.stmt(parsed.arena.statement_ids().next().unwrap()).kind {
                ArenaStmtKind::Use(id) => id,
                _ => panic!("expected import"),
            },
        );
        assert!(parsed_use.searched_candidates.is_none());

        let (_, loaded) = parse_load_entry_source_arena_only(
            entry.to_str().expect("fixture path"),
            entry_source_from_text(entry.to_str().unwrap(), source.to_string()),
            vec![module_root.clone()],
        );
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        let loaded_use = loaded.arena.arena.use_stmt(
            match loaded.arena.arena.stmt(loaded.arena.statement_ids().next().unwrap()).kind {
                ArenaStmtKind::Use(id) => id,
                _ => panic!("expected import"),
            },
        );
        assert!(loaded_use.resolved.is_none());
        let searched = loaded_use.searched_candidates.as_deref().expect("actual search paths");
        assert_eq!(searched.first(), Some(&root.path().join("unavailable_module.xsh")));
        assert!(searched.contains(&module_root.join("unavailable_module.xsh")));
    }

    #[test]
    fn workspace_loader_parses_shared_import_once_for_two_roots() {
        let root = tempfile::TempDir::new().expect("create shared-module fixture");
        let shared = root.path().join("shared.xsh");
        std::fs::write(&shared, "export const value = 1\n").unwrap();
        let mut sources = SourceMap::new();
        let mut arena = ArenaProgramBuilder::with_token_capacity(128);
        let mut loader = ModuleLoader::new(&mut sources, &mut arena);
        let roots = ["left.xsh", "right.xsh"].map(|name| {
            loader.load(root.path().join(name), b"use shared\n".to_vec(), Vec::new())
        });
        let modules = loader.finish();
        assert_eq!(modules.len(), 3);
        assert_eq!(sources.files().len(), 3);
        let shared_key = module_key(&shared);
        for key in roots {
            let module = &modules[&key];
            assert!(module.diagnostics.is_empty());
            assert_eq!(module.imports[0].target.as_deref(), Some(shared_key.as_str()));
        }
        let program = arena.finish();
        let shared_module = &modules[&shared_key];
        assert_eq!(
            program.modules.iter().filter(|module| module.key == shared_key).count(),
            1,
        );
        assert_eq!(
            sources.get(shared_module.source_id).unwrap().name(),
            shared.to_string_lossy(),
        );
    }

    #[test]
    fn shared_workspace_reuses_same_context_and_separates_import_graphs() {
        let root = tempfile::TempDir::new().expect("create shared-module fixture");
        let first_context = root.path().join("first_context");
        let second_context = root.path().join("second_context");
        let common = root.path().join("common");
        let first_modules = root.path().join("first_modules");
        let second_modules = root.path().join("second_modules");
        for directory in [
            &first_context,
            &second_context,
            &common,
            &first_modules,
            &second_modules,
        ] {
            fs::create_dir_all(directory).expect("create module root");
        }
        let shared = common.join("shared.xsh");
        let first_nested = first_modules.join("nested.xsh");
        let second_nested = second_modules.join("nested.xsh");
        fs::write(&shared, "use nested\n").expect("write shared module");
        fs::write(&first_nested, "export const value = 1\n").expect("write first dependency");
        fs::write(&second_nested, "export const value = 2\n").expect("write second dependency");

        let inputs = [
            (first_context.join("one.xsh"), vec![common.clone(), first_modules.clone()]),
            (first_context.join("two.xsh"), vec![common.clone(), first_modules.clone()]),
            (second_context.join("one.xsh"), vec![common, second_modules]),
        ]
        .into_iter()
        .map(|(file, module_roots)| ArenaEntryInput {
            file: file.to_string_lossy().into_owned(),
            bytes: b"use shared\n".to_vec(),
            module_roots,
        })
        .collect();
        let loaded = parse_load_entries_shared_arena_only(inputs);
        let program = &loaded.program;
        let imported_module = |statements| {
            program
                .arena
                .stmt_ids(statements)
                .find_map(|statement| match program.arena.stmt(statement).kind {
                    ArenaStmtKind::Use(use_id) => program
                        .arena
                        .use_stmt(use_id)
                        .resolved
                        .as_deref()
                        .map(str::to_owned),
                    _ => None,
                })
                .expect("entry and module import resolve")
        };
        let first_shared = imported_module(loaded.entries[0].statements);
        assert_eq!(imported_module(loaded.entries[1].statements), first_shared);
        let second_shared = imported_module(loaded.entries[2].statements);
        assert_ne!(first_shared, second_shared);

        let shared_key = module_key(&shared);
        let shared_modules = program
            .modules
            .iter()
            .filter(|module| module.name.as_str().as_str() == shared_key)
            .collect::<Vec<_>>();
        assert_eq!(shared_modules.len(), 2);
        assert!(shared_modules
            .iter()
            .all(|module| module.name.as_str().as_str() == shared_key));
        assert!(shared_modules.iter().any(|module| module.key == shared_key));
        assert!(shared_modules.iter().any(|module| module.key.contains('\0')));
        let mut keys = BTreeSet::new();
        for module in &program.modules {
            assert!(keys.insert(module.key.as_str()), "duplicate module key: {:?}", module.key);
        }

        let first_dependency = imported_module(
            program
                .modules
                .iter()
                .find(|module| module.key == first_shared)
                .expect("first shared module is present")
                .statements,
        );
        let second_dependency = imported_module(
            program
                .modules
                .iter()
                .find(|module| module.key == second_shared)
                .expect("second shared module is present")
                .statements,
        );
        assert_eq!(first_dependency, module_key(&first_nested));
        assert_eq!(second_dependency, module_key(&second_nested));
        assert_ne!(first_dependency, second_dependency);
    }

    #[test]
    fn shared_workspace_loading_propagates_cancellation_during_recursive_imports() {
        let root = tempfile::TempDir::new().expect("create cancellation fixture");
        let entry = root.path().join("entry.xsh");
        fs::write(root.path().join("shared.xsh"), "use nested\n")
            .expect("write shared module");
        fs::write(root.path().join("nested.xsh"), "const value = 1\n")
            .expect("write nested module");
        let mut polls = 0;
        let result = parse_load_entries_shared_arena_only_with_cancellation(
            vec![ArenaEntryInput {
                file: entry.to_string_lossy().into_owned(),
                bytes: b"use shared\n".to_vec(),
                module_roots: Vec::new(),
            }],
            || {
                polls += 1;
                polls == 7
            },
        );

        assert!(matches!(result, Err(LoadCancelled)));
        assert_eq!(polls, 7, "cancellation is polled at the nested import");
    }

    #[test]
    fn compact_file_unit_wraps_parsed_arena_without_checker_runtime_state() {
        let source = r#"
use fs

type Plugin = module {
  export let name: Str
  export pure label(name: Str) -> Str
  export proc execute() [io] -> Result[Unit, Error]
}

type User = {name: Str}
error AppError = Bad(message: Str)

export pure label(name: Str) -> Str {
  return name
}

proc main() [io] {
  print "ok"
}
"#;

        let (_sources, unit) = parse_load_entry_source_compact_file_unit(
            "entry.xsh",
            entry_source_from_text("entry.xsh", source.to_string()),
            Vec::new(),
        );

        assert!(
            unit.parse_diagnostics().is_empty(),
            "{:?}",
            unit.parse_diagnostics()
        );
        assert_eq!(unit.display_path(), "entry.xsh");
        assert_eq!(unit.source_id(), SourceId::new(0));
        assert_eq!(unit.cst().source_id(), SourceId::new(0));
        assert_eq!(unit.token_table().len(), unit.cst().token_table().len());
        assert!(unit.program().modules.is_empty());
        assert_eq!(unit.imports().len(), 1);
        assert_eq!(
            unit.imports()[0]
                .path
                .iter()
                .map(|name| name.as_str())
                .collect::<Vec<_>>(),
            vec!["fs"]
        );
        assert_eq!(unit.exports().len(), 1);
        assert!(unit.root_statements().count() > 0);
        assert_eq!(unit.module_statements().count(), 0);

        let summary = unit.declaration_summary();
        assert_eq!(summary.type_defs, 2);
        assert_eq!(summary.error_defs, 1);
        assert_eq!(summary.pure_defs, 1);
        assert_eq!(summary.proc_defs, 1);
        assert_eq!(summary.stream_defs, 0);
        assert_eq!(summary.module_contract_entries, 3);
    }

    #[test]
    fn compact_file_unit_keeps_parse_diagnostics_attached() {
        let (_sources, unit) = parse_load_entry_source_compact_file_unit(
            "bad.xsh",
            entry_source_from_bytes("bad.xsh", vec![0xff]),
            Vec::new(),
        );

        assert_eq!(unit.display_path(), "bad.xsh");
        assert_eq!(unit.source_id(), SourceId::new(0));
        assert_eq!(unit.parse_diagnostics().len(), 1);
        assert_eq!(
            unit.parse_diagnostics()[0].code.map(DiagnosticCode::name),
            Some("source.invalid-utf8")
        );
        assert_eq!(unit.program().statement_ids().count(), 0);
        assert_eq!(unit.cst().source_id(), SourceId::new(0));
    }

    #[test]
    fn compact_file_unit_exposes_loaded_module_statements() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "xsh-compact-file-unit-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create temp module root");
        let entry_path = root.join("entry.xsh");
        let module_path = root.join("helper.xsh");
        fs::write(
            &module_path,
            "export pure value() -> Int {\n  return 1\n}\n",
        )
        .expect("write helper module");

        let (_sources, unit) = parse_load_entry_source_compact_file_unit(
            entry_path.to_str().expect("utf-8 temp path"),
            entry_source_from_text(
                entry_path.to_str().expect("utf-8 temp path"),
                "use helper\nlet value = 1\n".to_string(),
            ),
            Vec::new(),
        );
        let _ = fs::remove_dir_all(&root);

        assert!(
            unit.parse_diagnostics().is_empty(),
            "{:?}",
            unit.parse_diagnostics()
        );
        assert_eq!(unit.imports().len(), 1);
        assert!(unit.imports()[0].resolved.is_some());
        assert_eq!(unit.program().modules.len(), 1);
        assert_eq!(unit.module_statements().count(), 1);
        assert_eq!(unit.exports().len(), 1);
        assert_eq!(unit.declaration_summary().pure_defs, 1);
    }

    #[test]
    fn compact_module_graph_collects_resolved_declarations_before_evaluator_installation() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "xsh-compact-module-graph-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create temp module root");
        let entry_path = root.join("entry.xsh");
        let module_path = root.join("helper.xsh");
        fs::write(
            &module_path,
            "##! Helper values.\nerror HelperError = Bad(message: Str)\n## Returns one.\nexport pure value() -> Int {\n  return 1\n}\n",
        )
        .expect("write helper module");

        let (sources, unit) = parse_load_entry_source_compact_file_unit(
            entry_path.to_str().expect("utf-8 temp path"),
            entry_source_from_text(
                entry_path.to_str().expect("utf-8 temp path"),
                "##! Entry values.\nuse helper as h\n## The answer.\nexport let answer = 1\n"
                    .to_string(),
            ),
            Vec::new(),
        );
        let _ = fs::remove_dir_all(&root);
        assert!(
            unit.parse_diagnostics().is_empty(),
            "{:?}",
            unit.parse_diagnostics()
        );

        let declarations = Checker::check_compact_declarations(
            unit.program(),
            &sources,
            unit.source_id(),
        );
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let graph = CompactModuleGraph::from_file_unit(&unit, &declarations);
        unit.program().symbol_owner().with_current(|| {
            let resolved_helper = Name::intern(
                graph.import_edges()[0]
                    .resolved
                    .as_deref()
                    .expect("resolved helper import"),
            );
            let value = Name::intern("value");
            let helper_error = Name::intern("HelperError");
            let answer = Name::intern("answer");
            let alias = Name::intern("h");

            assert_eq!(graph.import_edges().len(), 1);
            assert_eq!(graph.import_edges()[0].alias, Some(alias));
            assert!(graph.import_edges()[0].resolved.is_some());
            assert!(graph.module_aliases().contains_key(&alias));
            assert!(
                graph
                    .qualified_pures()
                    .contains_key(&QualifiedName::new(resolved_helper, value))
            );
            assert!(
                graph
                    .qualified_error_families()
                    .contains_key(&QualifiedName::new(resolved_helper, helper_error))
            );
            assert!(graph.exported_top_level_bindings().contains(&answer));
            assert_eq!(graph.qualified_declaration_count(), 2);
            assert_eq!(graph.source_order().len(), 2);
            assert!(graph.diagnostics().is_empty());
        });
    }
}
