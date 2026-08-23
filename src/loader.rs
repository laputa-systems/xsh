use crate::diagnostic::{Diagnostic, DiagnosticRenderer, Label};
use crate::modules::api_spec;
use crate::sema::check::{
    CheckOptions, CheckOutput, Checker, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
    ErrorFamilyInfo,
};
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::{Name, QualifiedName};
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaProgram, ArenaProgramBuilder, ArenaRange, ArenaStmtKind, StmtId,
};
use crate::syntax::cst::{LazyCst, SyntaxTree};

use crate::syntax::parser::{ArenaParseOutput, Parser};
use crate::syntax::token::TokenTable;
use rustc_hash::FxHashSet;
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
    let (source_id, invalid_offset) = match sources.add_file_from_utf8(file, bytes.clone()) {
        Ok(source_id) => {
            return EntrySource {
                sources,
                source_id,
                diagnostics: Vec::new(),
            };
        }
        Err(error) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            (sources.add_file(file, text), error.offset)
        }
    };
    let offset = {
        let text = sources
            .get(source_id)
            .expect("source was just inserted")
            .text();
        invalid_offset.min(text.len())
    };
    EntrySource {
        sources,
        source_id,
        diagnostics: vec![
            Diagnostic::error("source file is not valid UTF-8")
                .with_code("source.invalid-utf8")
                .with_label(Label::primary(
                    Span::new(source_id, offset, offset),
                    "invalid UTF-8 starts here",
                )),
        ],
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
            if let crate::syntax::arena::ArenaTypeDefBody::ModuleContract(entries) =
                program.arena.type_def(def).body
            {
                summary.module_contract_entries +=
                    program.arena.module_contract_entries(entries).len();
            }
        }
        ArenaStmtKind::ErrorDef(_) => summary.error_defs += 1,
        ArenaStmtKind::ProcDef(_) => summary.proc_defs += 1,
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
        ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
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
        _ => {}
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
            diagnostic.code.clone(),
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
    let entry_text = sources
        .get(entry_source_id)
        .map(|source| source.text().to_string())
        .unwrap_or_default();
    let checked = parsed
        .diagnostics
        .is_empty()
        .then(|| Checker::check_arena_with_options(&parsed.arena, &entry_text, options));
    CheckedEntry {
        sources,
        entry_source_id,
        parsed,
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

pub fn parse_load_entry_source_arena_only(
    file: &str,
    entry_source: EntrySource,
    module_roots: Vec<PathBuf>,
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
    if diagnostics.is_empty() {
        let mut loader =
            ArenaModuleLoader::new(&mut sources, &mut builder).with_module_roots(module_roots);
        loader.load_uses(Path::new(file), root_statements);
        diagnostics.extend(loader.diagnostics);
    }
    (
        sources,
        ArenaParseOutput {
            arena: builder.finish_with_statements(root_statements),
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
    if diagnostics.is_empty() {
        let mut loader =
            ArenaModuleLoader::new(sources, &mut builder).with_module_roots(module_roots);
        loader.load_uses(Path::new(file), root_statements);
        diagnostics.extend(loader.diagnostics);
    }
    ArenaParseOutput {
        arena: builder.finish_with_statements(root_statements),
        cst,
        diagnostics,
    }
}

struct ArenaModuleLoader<'a, 'b> {
    sources: &'a mut SourceMap,
    arena: &'a mut ArenaProgramBuilder<'b>,
    loaded: FxHashSet<String>,
    stack: Vec<String>,
    diagnostics: Vec<Diagnostic>,
    module_roots: Vec<PathBuf>,
}

impl<'a, 'b> ArenaModuleLoader<'a, 'b> {
    fn new(sources: &'a mut SourceMap, arena: &'a mut ArenaProgramBuilder<'b>) -> Self {
        Self {
            sources,
            arena,
            loaded: FxHashSet::default(),
            stack: Vec::new(),
            diagnostics: Vec::new(),
            module_roots: Vec::new(),
        }
    }

    fn with_module_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.module_roots = roots;
        self
    }

    fn load_uses(&mut self, importer: &Path, statements: ArenaRange) {
        let statements = self.arena.statement_ids(statements);
        for stmt in statements {
            let Some((use_id, path, span)) = self.arena.use_stmt_for_statement(stmt) else {
                continue;
            };
            if path.len() == 1 && api_spec().is_standard_module(&path[0].as_str()) {
                continue;
            }
            if let Some(key) = self.load_module(importer, &path, span) {
                self.arena.set_use_resolved(use_id, Arc::from(key.as_str()));
            }
        }
    }

    fn load_module(&mut self, importer: &Path, path: &[Name], span: Span) -> Option<String> {
        let (module_path, bytes) =
            match read_module_from_candidates(importer, path, &self.module_roots) {
                Ok(found) => found,
                Err(message) => {
                    self.diagnostics.push(
                        Diagnostic::error("failed to read module")
                            .with_code("parse.module-read")
                            .with_label(Label::primary(span, message)),
                    );
                    return None;
                }
            };
        let name = path
            .last()
            .copied()
            .unwrap_or_else(|| crate::symbol::Name::intern("module"));
        self.load_file_bytes(&module_path, bytes, name, span)
    }

    fn load_file_bytes(
        &mut self,
        module_path: &Path,
        bytes: Vec<u8>,
        name: Name,
        span: Span,
    ) -> Option<String> {
        let key = module_key(module_path);
        if self.stack.contains(&key) {
            self.diagnostics.push(
                Diagnostic::error("cyclic module import")
                    .with_code("parse.module-cycle")
                    .with_label(Label::primary(span, "module import cycle starts here")),
            );
            return None;
        }
        if self.loaded.contains(&key) {
            return Some(key);
        }
        let source_name = module_path.to_string_lossy().into_owned();
        let source_id = match self
            .sources
            .add_file_from_utf8(source_name.clone(), bytes.clone())
        {
            Ok(source_id) => source_id,
            Err(error) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let source_id = self.sources.add_file(source_name, text);
                let offset = error.offset.min(
                    self.sources
                        .get(source_id)
                        .map_or(0, crate::source::SourceFile::len),
                );
                self.diagnostics.push(
                    Diagnostic::error("source file is not valid UTF-8")
                        .with_code("source.invalid-utf8")
                        .with_label(Label::primary(
                            Span::new(source_id, offset, offset),
                            "invalid UTF-8 starts here",
                        )),
                );
                return None;
            }
        };
        let text = self
            .sources
            .get(source_id)
            .expect("source was just inserted")
            .text();
        let parsed = Parser::parse_source_into_arena_builder(source_id, text, self.arena);
        if !parsed.diagnostics.is_empty() {
            self.diagnostics.push(
                Diagnostic::error("failed to load module")
                    .with_code("parse.module-load")
                    .with_label(Label::primary(
                        span,
                        format!("`{}` has parse errors", module_path.display()),
                    )),
            );
            self.diagnostics.extend(parsed.diagnostics);
            return None;
        }
        self.stack.push(key.clone());
        self.load_uses(module_path, parsed.statements);
        self.stack.pop();
        self.loaded.insert(key.clone());
        self.arena
            .push_arena_module(key.clone(), name, parsed.statements);
        Some(key)
    }
}

pub fn module_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

#[allow(clippy::single_call_fn)]
fn read_module_from_candidates(
    importer: &Path,
    path: &[Name],
    extra_roots: &[PathBuf],
) -> Result<(PathBuf, Vec<u8>), String> {
    let candidates = resolve_module_path_candidates(importer, path, extra_roots);
    let mut failures = Vec::new();
    for candidate in candidates {
        match fs::read(&candidate) {
            Ok(bytes) => return Ok((candidate, bytes)),
            Err(error) => failures.push(format!("`{}`: {error}", candidate.display())),
        }
    }
    Err(format!(
        "failed to read module; tried {}. Set XSH_MODULE_PATH to add module search roots",
        failures.join(", ")
    ))
}

/// Resolve a user import using the same search order as the runtime loader.
///
/// Tooling that builds a workspace graph must be able to discover imports
/// without constructing a fresh arena-backed loader for every entry file.
/// Standard-library modules are intentionally reported as absent because they
/// do not correspond to user source files.
pub fn resolve_user_module(
    importer: &Path,
    path: &[Name],
    extra_roots: &[PathBuf],
) -> Result<Option<(PathBuf, Vec<u8>)>, String> {
    if path.len() == 1 && api_spec().is_standard_module(&path[0].as_str()) {
        return Ok(None);
    }
    read_module_from_candidates(importer, path, extra_roots).map(Some)
}

#[allow(clippy::single_call_fn)]
fn resolve_module_path_candidates(
    importer: &Path,
    path: &[Name],
    extra_roots: &[PathBuf],
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
    for base in extra_roots {
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
    fn compact_file_unit_wraps_parsed_arena_without_checker_runtime_state() {
        let source = r#"
use fs

type Plugin = module {
  export let name: Str
  export pure label(name: Str) -> Str
  export proc execute() [io] -> Result[Unit]
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
            unit.parse_diagnostics()[0].code.as_deref(),
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
            "error HelperError = Bad(message: Str)\nexport pure value() -> Int {\n  return 1\n}\n",
        )
        .expect("write helper module");

        let (_sources, unit) = parse_load_entry_source_compact_file_unit(
            entry_path.to_str().expect("utf-8 temp path"),
            entry_source_from_text(
                entry_path.to_str().expect("utf-8 temp path"),
                "use helper as h\nexport let answer = 1\n".to_string(),
            ),
            Vec::new(),
        );
        let _ = fs::remove_dir_all(&root);
        assert!(
            unit.parse_diagnostics().is_empty(),
            "{:?}",
            unit.parse_diagnostics()
        );

        let declarations = Checker::check_compact_declarations(unit.program());
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let graph = CompactModuleGraph::from_file_unit(&unit, &declarations);
        unit.program().symbol_owner().with_current(|| {
            let helper = Name::intern("helper");
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
                    .contains_key(&QualifiedName::new(helper, value))
            );
            assert!(
                graph
                    .qualified_error_families()
                    .contains_key(&QualifiedName::new(helper, helper_error))
            );
            assert!(graph.exported_top_level_bindings().contains(&answer));
            assert_eq!(graph.qualified_declaration_count(), 2);
            assert_eq!(graph.source_order().len(), 2);
            assert!(graph.diagnostics().is_empty());
        });
    }
}
