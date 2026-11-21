//! Workspace-wide unused-export analysis.
//!
//! An export's references can live in any module of the project, so this rule
//! cannot run while one file is walked. It reads the loaded workspace instead:
//! declarations from the shared arena, and references from the token stream of
//! every module. Tokens are used because every form that can name a module
//! member (calls, fields, types, patterns, signatures) spells it
//! `namespace.Name`, and a token scan cannot miss a new expression form the
//! way a hand-written syntax walk would. It can over-count a reference where a
//! local binding shadows a namespace alias, which keeps a live export from
//! being reported. Modules reached through a literal `module.load(p"PATH")`
//! are exempt, because their exports are used at run time.

use rustc_hash::{FxHashMap, FxHashSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use xsh::frontend::load::LoadedImport;
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaRange,
    ArenaStmtKind, ArenaTypeDefBody, AstArena, TypeDefId,
};
use xsh::frontend::syntax::lexer::Lexer;
use xsh::frontend::syntax::token::TokenTag;

/// One loaded module as the analysis sees it.
pub(crate) struct ExportModule<'a> {
    pub key: &'a str,
    pub path: &'a std::path::Path,
    pub source_id: SourceId,
    pub text: &'a str,
    pub statements: ArenaRange,
    pub imports: &'a [LoadedImport],
    /// A module under a configured test root: its references count as test use.
    pub test_file: bool,
    /// `[dead-code] public-api`: its exports are a published interface.
    pub public_api: bool,
    /// Whether this module's own unused exports are reported.
    pub enabled: bool,
}

#[derive(Clone, Copy, Default)]
struct RefStats {
    production: usize,
    test: usize,
}

struct ExportDecl {
    module: usize,
    name: String,
    span: Span,
    members: Vec<String>,
    /// A proc named `main` is invoked by the runner, not imported.
    entry: bool,
}

/// One export referenced nowhere is a warning in every run; an export only
/// tests or only its own module use is advice, emitted when the rule is named
/// so a default run stays readable.
pub(crate) fn unused_export_diagnostics(
    program: &ArenaProgram,
    modules: &[ExportModule<'_>],
    include_reduced_use: bool,
) -> FxHashMap<String, Vec<Diagnostic>> {
    let arena = &program.arena;
    let module_index = modules
        .iter()
        .enumerate()
        .map(|(index, module)| (module.key, index))
        .collect::<FxHashMap<_, _>>();

    let mut declarations = Vec::new();
    let mut by_name = FxHashMap::default();
    let mut member_owner = FxHashMap::default();
    // A module contract names the surface of a module loaded at run time. It
    // does not name the module that implements it, so an export with a
    // contract's name is a dynamic interface name rather than dead code.
    let mut contract_surface = FxHashSet::default();
    for (index, module) in modules.iter().enumerate() {
        for statement in arena.stmt_ids(module.statements) {
            let stmt = arena.stmt(statement);
            if let ArenaStmtKind::TypeDef(id) = stmt.kind {
                collect_contract_names(arena, id, &mut contract_surface);
            }
            let ArenaStmtKind::Export(inner) = stmt.kind else {
                continue;
            };
            let inner_stmt = arena.stmt(inner);
            let (name, members, entry) = match inner_stmt.kind {
                ArenaStmtKind::TypeDef(id) => {
                    let def = arena.type_def(id);
                    collect_contract_names(arena, id, &mut contract_surface);
                    let members = match &def.body {
                        ArenaTypeDefBody::TagUnion(variants) => arena
                            .tag_variants(*variants)
                            .iter()
                            .map(|variant| variant.name.as_str().to_string())
                            .collect(),
                        _ => Vec::new(),
                    };
                    (def.name.as_str().to_string(), members, false)
                }
                ArenaStmtKind::ErrorDef(id) => {
                    let def = arena.error_def(id);
                    let members = arena
                        .error_variants(def.variants)
                        .iter()
                        .map(|variant| variant.name.as_str().to_string())
                        .collect();
                    (def.name.as_str().to_string(), members, false)
                }
                ArenaStmtKind::ProcDef(id) => {
                    let name = arena.function_def(id).name.as_str().to_string();
                    let entry = name == "main";
                    (name, Vec::new(), entry)
                }
                ArenaStmtKind::PureDef(id) | ArenaStmtKind::StreamDef(id) => {
                    (arena.function_def(id).name.as_str().to_string(), Vec::new(), false)
                }
                ArenaStmtKind::Const { target, .. } | ArenaStmtKind::Let { target, .. } => {
                    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind
                    else {
                        continue;
                    };
                    (name.as_str().to_string(), Vec::new(), false)
                }
                _ => continue,
            };
            if name.starts_with('_') {
                continue;
            }
            let index_decl = declarations.len();
            by_name.insert((index, name.clone()), index_decl);
            for member in &members {
                if !member.starts_with('_') {
                    member_owner.insert((index, member.clone()), index_decl);
                }
            }
            declarations.push(ExportDecl {
                module: index,
                name,
                span: stmt.span,
                members,
                entry,
            });
        }
    }

    let mut stats = vec![RefStats::default(); declarations.len()];
    let mut own_occurrences: Vec<FxHashMap<String, Vec<usize>>> =
        (0..modules.len()).map(|_| FxHashMap::default()).collect();
    // A module loaded through `module.load(p"PATH")` is reached at run time,
    // so a literal path names a dynamic surface rather than a dead one.
    let mut dynamically_loaded = FxHashSet::default();
    // An unresolved import hides the references its module makes, so any
    // exported name it mentions cannot be judged unused.
    let mut unresolved_names = FxHashSet::default();
    for (index, module) in modules.iter().enumerate() {
        let aliases = module_aliases(program, module);
        let has_unresolved_import = module.imports.iter().any(|import| {
            import.target.is_none()
                && (import.diagnostic.is_some()
                    || program
                        .arena
                        .use_stmt(import.use_id)
                        .searched_candidates
                        .is_some())
        });
        let use_spans = use_statement_spans(program, module.statements);
        let lexed = Lexer::new(module.source_id, module.text).lex_compact();
        let table = &lexed.token_table;
        let mut next_use = 0usize;
        for token in 0..table.len() {
            let Some(start) = table.start_at(token) else {
                continue;
            };
            while next_use < use_spans.len() && start >= use_spans[next_use].1 {
                next_use += 1;
            }
            if next_use < use_spans.len()
                && start >= use_spans[next_use].0
                && start < use_spans[next_use].1
            {
                continue;
            }
            let tag = table.tag_at(token);
            if !matches!(tag, Some(TokenTag::Ident) | Some(TokenTag::ProcIdent)) {
                continue;
            }
            let Some(name) = table.name_at(token) else {
                continue;
            };
            let name = name.as_str().to_string();
            if has_unresolved_import {
                unresolved_names.insert(name.clone());
            }
            // A member after `.` is qualified, and a label before `:` binds or
            // names a field rather than reading a top-level name.
            let qualified_member = token > 0 && table.tag_at(token - 1) == Some(TokenTag::Dot);
            if !qualified_member && table.tag_at(token + 1) != Some(TokenTag::Colon) {
                own_occurrences[index]
                    .entry(name.clone())
                    .or_default()
                    .push(start);
            }
            if table.tag_at(token + 1) != Some(TokenTag::Dot) {
                continue;
            }
            let Some(member) = table.tag_at(token + 2).and_then(|member_tag| {
                matches!(member_tag, TokenTag::Ident | TokenTag::ProcIdent)
                    .then(|| table.name_at(token + 2))
                    .flatten()
            }) else {
                continue;
            };
            let Some(target) = aliases.get(&name) else {
                continue;
            };
            let Some(&target_index) = module_index.get(target.as_str()) else {
                continue;
            };
            if target_index == index {
                continue;
            }
            let member = member.as_str().to_string();
            let Some(&declaration) = by_name
                .get(&(target_index, member.clone()))
                .or_else(|| member_owner.get(&(target_index, member)))
            else {
                continue;
            };
            let stats = &mut stats[declaration];
            if module.test_file {
                stats.test += 1;
            } else {
                stats.production += 1;
            }
        }

        // An f-string interpolation is one token to the lexer, so the names in
        // it never reach the token pass. The arena holds each interpolation as
        // an ordinary expression; reading the source's expressions covers it.
        // The token pass counts most of the same references again, which only
        // strengthens "referenced at least once".
        let rows = program.arena.source_rows();
        let field_bases = rows
            .expr_ids(module.source_id)
            .filter_map(|expression| match program.arena.expr(expression).kind {
                ArenaExprKind::Field { base, .. }
                | ArenaExprKind::NullSafeField { base, .. } => Some(base),
                _ => None,
            })
            .collect::<FxHashSet<_>>();
        for expression in rows.expr_ids(module.source_id) {
            let expr = program.arena.expr(expression);
            match expr.kind {
                ArenaExprKind::Ident(name) if !field_bases.contains(&expression) => {
                    let name = name.as_str().to_string();
                    if has_unresolved_import {
                        unresolved_names.insert(name.clone());
                    }
                    own_occurrences[index]
                        .entry(name)
                        .or_default()
                        .push(expr.span.start());
                }
                ArenaExprKind::Call { callee, args } => {
                    let ArenaExprKind::Field { base, name } = program.arena.expr(callee).kind
                    else {
                        continue;
                    };
                    let ArenaExprKind::Ident(namespace) = program.arena.expr(base).kind else {
                        continue;
                    };
                    if namespace.as_str().as_str() != "module"
                        || name.as_str().as_str() != "load"
                    {
                        continue;
                    }
                    for arg in program.arena.call_args(args) {
                        let ArenaCallArgKind::Positional(value) = arg.kind else {
                            continue;
                        };
                        let ArenaExprKind::PathStr(literal) = program.arena.expr(value).kind
                        else {
                            continue;
                        };
                        let literal = program.arena.string_literal(literal);
                        if let Some(file) = std::path::Path::new(literal.as_ref())
                            .file_name()
                            .and_then(|file| file.to_str())
                        {
                            dynamically_loaded.insert(file.to_string());
                        }
                    }
                }
                ArenaExprKind::Field { base, name }
                | ArenaExprKind::NullSafeField { base, name } => {
                    let ArenaExprKind::Ident(alias) = program.arena.expr(base).kind else {
                        continue;
                    };
                    if has_unresolved_import {
                        unresolved_names.insert(alias.as_str().to_string());
                        unresolved_names.insert(name.as_str().to_string());
                    }
                    let Some(target) = aliases.get(alias.as_str().as_str()) else {
                        continue;
                    };
                    let Some(&target_index) = module_index.get(target.as_str()) else {
                        continue;
                    };
                    if target_index == index {
                        continue;
                    }
                    let member = name.as_str().to_string();
                    let Some(&declaration) = by_name
                        .get(&(target_index, member.clone()))
                        .or_else(|| member_owner.get(&(target_index, member)))
                    else {
                        continue;
                    };
                    let stats = &mut stats[declaration];
                    if module.test_file {
                        stats.test += 1;
                    } else {
                        stats.production += 1;
                    }
                }
                _ => {}
            }
        }
    }

    let mut diagnostics: FxHashMap<String, Vec<Diagnostic>> = FxHashMap::default();
    for (index, declaration) in declarations.iter().enumerate() {
        let module = &modules[declaration.module];
        let loaded_dynamically = module
            .path
            .file_name()
            .and_then(|file| file.to_str())
            .is_some_and(|file| dynamically_loaded.contains(file));
        if !module.enabled
            || module.public_api
            || loaded_dynamically
            || declaration.entry
            || contract_surface.contains(&declaration.name)
            || unresolved_names.contains(&declaration.name)
        {
            continue;
        }
        let reference = stats[index];
        let own_uses = own_occurrences[declaration.module]
            .get(&declaration.name)
            .map_or(0, |uses| outside(uses, declaration.span))
            + declaration
                .members
                .iter()
                .map(|member| {
                    own_occurrences[declaration.module]
                        .get(member)
                        .map_or(0, |uses| outside(uses, declaration.span))
                })
                .sum::<usize>();
        let diagnostic = if reference.production > 0 {
            continue;
        } else if reference.test > 0 {
            if !include_reduced_use {
                continue;
            }
            Diagnostic::new(
                Severity::Note,
                format!("export `{}` is used only by tests", declaration.name),
            )
            .with_code(DiagnosticCode::LintUnusedExport)
            .with_label(Label::secondary(
                declaration.span,
                "only test modules reference this export",
            ))
        } else if own_uses > 0 {
            if !include_reduced_use {
                continue;
            }
            Diagnostic::new(
                Severity::Note,
                format!(
                    "export `{}` is used only inside its module",
                    declaration.name
                ),
            )
            .with_code(DiagnosticCode::LintUnusedExport)
            .with_label(Label::secondary(
                declaration.span,
                "no other module references this export",
            ))
        } else {
            Diagnostic::new(
                Severity::Warning,
                format!("unused export `{}`", declaration.name),
            )
            .with_code(DiagnosticCode::LintUnusedExport)
            .with_label(Label::secondary(
                declaration.span,
                "no module references this export",
            ))
        };
        diagnostics
            .entry(module.key.to_string())
            .or_default()
            .push(diagnostic);
    }
    diagnostics
}

fn collect_contract_names(arena: &AstArena, id: TypeDefId, names: &mut FxHashSet<String>) {
    if let ArenaTypeDefBody::ModuleContract { entries, .. } = &arena.type_def(id).body {
        for entry in arena.module_contract_entries(*entries) {
            names.insert(entry.name.as_str().to_string());
        }
    }
}

fn module_aliases(program: &ArenaProgram, module: &ExportModule<'_>) -> FxHashMap<String, String> {
    let arena = &program.arena;
    let mut aliases = FxHashMap::default();
    for import in module.imports {
        let Some(target) = import.target.as_ref() else {
            continue;
        };
        let alias = arena
            .use_stmt(import.use_id)
            .alias
            .or_else(|| import.path.last().copied());
        if let Some(alias) = alias {
            aliases.insert(alias.as_str().to_string(), target.to_string());
        }
    }
    aliases
}

fn use_statement_spans(program: &ArenaProgram, statements: ArenaRange) -> Vec<(usize, usize)> {
    let mut spans = program
        .arena
        .stmt_ids(statements)
        .filter(|statement| {
            matches!(program.arena.stmt(*statement).kind, ArenaStmtKind::Use(_))
        })
        .map(|statement| {
            let span = program.arena.stmt(statement).span;
            (span.start(), span.end())
        })
        .collect::<Vec<_>>();
    spans.sort_unstable();
    spans
}

fn outside(uses: &[usize], span: Span) -> usize {
    uses.iter()
        .filter(|&&offset| offset < span.start() || offset >= span.end())
        .count()
}
