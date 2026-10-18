use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::syntax::arena::{
    ArenaChild, ArenaExprKind, ArenaExprOrRun, ArenaFunctionDef, ArenaStmtKind, SugarView,
};
use xsh::frontend::syntax::parser::Parser;
use xsh::frontend::syntax::token::TokenTag;

struct Site {
    project: String,
    revision: String,
    path: String,
    kind: String,
    declaration_kind: String,
    visibility: String,
    owner: String,
    initializer: String,
    start: usize,
    end: usize,
    edit_start: usize,
    edit_end: usize,
    line: usize,
    annotation: String,
    edit: String,
}

fn line_at(source: &str, offset: usize) -> usize {
    source[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1
}

fn edit_span(source: &str, kind: &str, span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut prefix_end = span.start();
    while prefix_end > 0 && bytes[prefix_end - 1].is_ascii_whitespace() {
        prefix_end -= 1;
    }
    let prefix: &[u8] = match kind {
        "return" => b"->",
        "effect" => return span,
        _ => b":",
    };
    let prefix_start = prefix_end.checked_sub(prefix.len()).unwrap_or_else(|| {
        panic!("missing annotation delimiter before byte {}", span.start())
    });
    assert_eq!(
        &bytes[prefix_start..prefix_end],
        prefix,
        "missing annotation delimiter before byte {}",
        span.start()
    );
    Span::new(span.source_id, prefix_start, span.end())
}

fn escaped(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

fn local_initializer_class(
    arena: &xsh::frontend::syntax::arena::AstArena,
    source: &str,
    ty: Span,
    initializer: ArenaExprOrRun,
) -> String {
    let ty_text = &source[ty.start()..ty.end()];
    let compact_ty: String = ty_text.chars().filter(|ch| !ch.is_whitespace()).collect();
    let ArenaExprOrRun::Expr(expr) = initializer else {
        return if compact_ty.starts_with("Map[") {
            "map-typed".to_string()
        } else {
            "other".to_string()
        };
    };
    let expression = arena.expr(expr);
    if let ArenaExprKind::Call { callee, args } = &expression.kind {
        if args.is_empty() {
            let callee = arena.expr(*callee);
            if let ArenaExprKind::Field { name, .. } = &callee.kind {
                if name.to_string() == "require" {
                    return "require-target".to_string();
                }
            }
        }
    }
    if compact_ty.starts_with("Map[") {
        "map-typed".to_string()
    } else {
        "other".to_string()
    }
}

fn effect_clause_span(
    arena: &xsh::frontend::syntax::arena::AstArena,
    def: &ArenaFunctionDef,
    statement: Span,
    source: &str,
) -> Option<Span> {
    let base = statement.start();
    let body_start = arena.span(arena.block(def.body).span).start();
    let signature = source.get(base..body_start)?;
    let table = xsh::frontend::syntax::lexer::Lexer::new(statement.source_id, signature)
        .lex_compact()
        .token_table;
    let mut depth = 0usize;
    let mut parameters_closed = false;
    let mut opening = None;
    let mut clause = None;
    for index in 0..table.len() {
        let tag = table.tag_at(index)?;
        if matches!(tag, TokenTag::Comment | TokenTag::Newline) {
            continue;
        }
        if parameters_closed {
            match tag {
                TokenTag::LBracket => {
                    if depth == 0 {
                        opening = Some(table.start_at(index)?);
                    }
                    depth += 1;
                }
                TokenTag::RBracket => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        clause = Some(Span::new(
                            statement.source_id,
                            base + opening.take()?,
                            base + table.end_at(index, signature)?,
                        ));
                    }
                }
                _ => {}
            }
            continue;
        }
        match tag {
            TokenTag::LParen => depth += 1,
            TokenTag::RParen => {
                depth = depth.checked_sub(1)?;
                parameters_closed = depth == 0;
            }
            _ => {}
        }
    }
    clause
}

fn add_type_site(
    sites: &mut Vec<Site>,
    project: &str,
    revision: &str,
    path: &str,
    source: &str,
    kind: &str,
    declaration_kind: &str,
    visibility: &str,
    owner: String,
    initializer: &str,
    span: Span,
) {
    let edit_span = edit_span(source, kind, span);
    sites.push(Site {
        project: project.to_string(),
        revision: revision.to_string(),
        path: path.to_string(),
        kind: kind.to_string(),
        declaration_kind: declaration_kind.to_string(),
        visibility: visibility.to_string(),
        owner,
        initializer: initializer.to_string(),
        start: span.start(),
        end: span.end(),
        edit_start: edit_span.start(),
        edit_end: edit_span.end(),
        line: line_at(source, span.start()),
        annotation: escaped(&source[span.start()..span.end()]),
        edit: escaped(&source[edit_span.start()..edit_span.end()]),
    });
}

fn inventory_child(
    sites: &mut Vec<Site>,
    project: &str,
    revision: &str,
    relative: &str,
    source: &str,
    arena: &xsh::frontend::syntax::arena::AstArena,
    child: ArenaChild,
) {
    match child {
        ArenaChild::Stmt(id) => inventory_statement(
            sites,
            project,
            revision,
            relative,
            source,
            arena,
            id,
            "private",
        ),
        ArenaChild::Block(id) => {
            for child in arena.stmt_ids(arena.block(id).statements) {
                inventory_statement(
                    sites,
                    project,
                    revision,
                    relative,
                    source,
                    arena,
                    child,
                    "private",
                );
            }
        }
        ArenaChild::Expr(id) => arena.for_each_expr_child(id, |child| {
            inventory_child(sites, project, revision, relative, source, arena, child)
        }),
        ArenaChild::Pattern(id) => arena.for_each_pattern_child(id, |child| {
            inventory_child(sites, project, revision, relative, source, arena, child)
        }),
        ArenaChild::BindingTarget(id) => arena.for_each_binding_target_child(id, |child| {
            inventory_child(sites, project, revision, relative, source, arena, child)
        }),
        ArenaChild::AssignTarget(id) => arena.for_each_assign_target_child(id, |child| {
            inventory_child(sites, project, revision, relative, source, arena, child)
        }),
        ArenaChild::TypeExpr(id) => arena.for_each_type_expr_child(id, |child| {
            inventory_child(sites, project, revision, relative, source, arena, child)
        }),
        ArenaChild::BuilderBlock(id) => arena.for_each_builder_child(id, |child| {
            inventory_child(sites, project, revision, relative, source, arena, child)
        }),
    }
}

fn inventory_statement(
    sites: &mut Vec<Site>,
    project: &str,
    revision: &str,
    relative: &str,
    source: &str,
    arena: &xsh::frontend::syntax::arena::AstArena,
    id: xsh::frontend::syntax::arena::StmtId,
    visibility: &str,
) {
    let statement = arena.stmt(id);
    if let ArenaStmtKind::Export(inner) = statement.kind.clone() {
        inventory_statement(
            sites,
            project,
            revision,
            relative,
            source,
            arena,
            inner,
            "exported",
        );
        return;
    }
    if arena.stmt_is_synthetic(id) {
        return;
    }
    if let ArenaStmtKind::ProcDef(definition)
    | ArenaStmtKind::CliMain(definition)
    | ArenaStmtKind::PureDef(definition)
    | ArenaStmtKind::StreamDef(definition) = statement.kind.clone()
    {
        if arena.function_def(definition).test_declaration {
            return;
        }
    }
    match statement.kind.clone() {
        ArenaStmtKind::Let {
            target,
            ty: Some(ty),
            initializer,
        }
        | ArenaStmtKind::Var {
            target,
            ty: Some(ty),
            initializer,
        }
        | ArenaStmtKind::Const {
            target,
            ty: Some(ty),
            initializer,
        } => {
            let target = arena.binding_target(target);
            let owner = match &target.kind {
                xsh::frontend::syntax::arena::ArenaBindingTargetKind::Name(name) => {
                    name.to_string()
                }
                xsh::frontend::syntax::arena::ArenaBindingTargetKind::Record { .. } => {
                    "<record>".to_string()
                }
            };
            let span = arena.type_expr_span(ty);
            let initializer = local_initializer_class(arena, source, span, initializer);
            add_type_site(
                sites,
                project,
                revision,
                relative,
                source,
                "local",
                "local",
                visibility,
                owner,
                &initializer,
                span,
            );
        }
        ArenaStmtKind::Guard {
            target,
            ty: Some(ty),
            initializer,
            ..
        } => {
            let target = arena.binding_target(target);
            let owner = match &target.kind {
                xsh::frontend::syntax::arena::ArenaBindingTargetKind::Name(name) => {
                    name.to_string()
                }
                xsh::frontend::syntax::arena::ArenaBindingTargetKind::Record { .. } => {
                    "<record>".to_string()
                }
            };
            let span = arena.type_expr_span(ty);
            let initializer = local_initializer_class(arena, source, span, initializer);
            add_type_site(
                sites,
                project,
                revision,
                relative,
                source,
                "local",
                "guard-local",
                visibility,
                owner,
                &initializer,
                span,
            );
        }
        kind @ (ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::CliMain(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_)) => {
            let (definition, declaration_kind) = match kind {
                ArenaStmtKind::ProcDef(definition) => (definition, "proc"),
                ArenaStmtKind::CliMain(definition) => (definition, "entry-proc"),
                ArenaStmtKind::PureDef(definition) => (definition, "pure"),
                ArenaStmtKind::StreamDef(definition) => (definition, "stream"),
                _ => unreachable!(),
            };
            let def = arena.function_def(definition);
            let visibility = if declaration_kind == "entry-proc" {
                "entry"
            } else {
                visibility
            };
            for param in arena.params(def.params) {
                if let Some(ty) = param.annotation {
                    let span = arena.type_expr_span(ty);
                    add_type_site(
                        sites,
                        project,
                        revision,
                        relative,
                        source,
                        "parameter",
                        declaration_kind,
                        visibility,
                        format!("{declaration_kind} {} parameter {}", def.name, param.name),
                        "none",
                        span,
                    );
                }
            }
            if !def.return_ty_defaulted {
                let span = arena.type_expr_span(def.return_ty);
                add_type_site(
                    sites,
                    project,
                    revision,
                    relative,
                    source,
                    "return",
                    declaration_kind,
                    visibility,
                    format!("{declaration_kind} {} return", def.name),
                    "none",
                    span,
                );
            }
            if def.effects.is_some() {
                let span = effect_clause_span(arena, def, statement.span, source)
                    .unwrap_or_else(|| {
                        panic!(
                            "failed to find effect clause for {} {}",
                            declaration_kind, def.name
                        )
                    });
                add_type_site(
                    sites,
                    project,
                    revision,
                    relative,
                    source,
                    "effect",
                    declaration_kind,
                    visibility,
                    format!("{declaration_kind} {} effect clause", def.name),
                    "pending",
                    span,
                );
            }
        }
        _ => {}
    }
    arena.for_each_stmt_child(id, SugarView::Source, |child| {
        inventory_child(sites, project, revision, relative, source, arena, child)
    });
}

fn inventory(project: &str, revision: &str, root: &Path, relative: &str) -> Vec<Site> {
    let path = root.join(relative);
    let source = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    });
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{} has parser diagnostics: {:?}",
        path.display(),
        parsed.diagnostics
    );
    let program = parsed.arena;
    let source_id = SourceId::new(0);
    let mut sites = Vec::new();
    for id in program.arena.stmt_ids(source_id) {
        inventory_statement(
            &mut sites,
            project,
            revision,
            relative,
            &source,
            &program.arena,
            id,
            "private",
        );
    }
    sites
}

fn main() {
    let mut args = env::args().skip(1);
    let mut sites = Vec::new();
    while let Some(project) = args.next() {
        let root = PathBuf::from(args.next().expect("missing root"));
        let revision = args.next().expect("missing source revision");
        let relative = args.next().expect("missing source path");
        sites.extend(inventory(&project, &revision, &root, &relative));
    }
    sites.sort_by(|left, right| {
        (&left.project, &left.path, left.start, &left.kind)
            .cmp(&(&right.project, &right.path, right.start, &right.kind))
    });
    println!("site\tproject\tsource_commit\tpath\tkind\tdeclaration_kind\tvisibility\towner\tinitializer\tstart\tend\tedit_start\tedit_end\tline\tannotation\tedit");
    for (index, site) in sites.into_iter().enumerate() {
        println!(
            "{index}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            site.project,
            site.revision,
            site.path,
            site.kind,
            site.declaration_kind,
            site.visibility,
            escaped(&site.owner),
            site.initializer,
            site.start,
            site.end,
            site.edit_start,
            site.edit_end,
            site.line,
            site.annotation,
            site.edit,
        );
    }
}
