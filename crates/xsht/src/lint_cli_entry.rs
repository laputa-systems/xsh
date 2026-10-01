use std::collections::BTreeMap;
use rustc_hash::FxHashMap;
use xsh::diagnostic::{Diagnostic, FixHint, Label, Severity};
use xsh::frontend::check::{Checker, LiteralConstant, Type};
use xsh::frontend::syntax::arena::{ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind,
    ArenaExprOrRun, ArenaProgram, ArenaRecordFieldKind, ArenaStmtKind};
use xsh::frontend::syntax::parser::Parser;

/// Only defaulted options can preserve the explicit schema's complete argv
/// policy: ordinary positional descriptors also accept their long spelling.
/// Exact generated help and an initializer-free entry keep preflight observable
/// output and timing unchanged.
pub(super) fn signature_cli_migration_with_checked<'checked>(program: &ArenaProgram, source: &str, check_original: impl FnOnce() -> &'checked xsh::frontend::check::CheckOutput) -> Vec<Diagnostic> {
    program.symbol_owner().with_current(|| signature_cli_migration_inner(program, source, check_original))
}

fn signature_cli_migration_inner<'checked>(program: &ArenaProgram, source: &str, check_original: impl FnOnce() -> &'checked xsh::frontend::check::CheckOutput) -> Vec<Diagnostic> {
    let before = std::cell::LazyCell::new(check_original);
    if !program.modules.is_empty() || program.module_doc_for(program.statements).is_some() { return Vec::new(); }
    let roots = program.statement_ids().collect::<Vec<_>>();
    if !roots.iter().all(|id| matches!(program.arena.stmt(*id).kind,
        ArenaStmtKind::ProcDef(_) | ArenaStmtKind::PureDef(_) | ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_))) {
        return Vec::new();
    }
    let arena = &program.arena;
    let mut diagnostics = Vec::new();
    for statement in roots {
        let declaration = arena.stmt(statement);
        let ArenaStmtKind::ProcDef(definition) = declaration.kind else { continue; };
        let function = arena.function_def(definition);
        let [argv] = arena.params(function.params) else { continue; };
        if function.name != "main" || !argv.rest || Type::from_arena(arena, argv.ty) != Type::List(Box::new(Type::Str)) { continue; }
        let Some(first) = arena.stmt_ids(arena.block(function.body).statements).next() else { continue; };
        let binding = arena.stmt(first);
        let ArenaStmtKind::Let { target, ty: Some(annotation), initializer: ArenaExprOrRun::Expr(value) } = binding.kind else { continue; };
        let ArenaBindingTargetKind::Record { fields: bindings, rest: false } = arena.binding_target(target).kind else { continue; };
        let Type::Record(expected) = before.record_constructors.resolve_type(arena, annotation, None) else { continue; };
        let ArenaExprKind::Try(call) = arena.expr(value).kind else { continue; };
        let ArenaExprKind::Call { callee, args } = arena.expr(call).kind else { continue; };
        let ArenaExprKind::Field { base: target, name } = arena.expr(callee).kind else { continue; };
        if name != "parse" || !matches!(arena.expr(target).kind, ArenaExprKind::Ident(name) if name == "cli") { continue; }
        let [input, schema] = arena.call_args(args) else { continue; };
        let (ArenaCallArgKind::Positional(input), ArenaCallArgKind::Positional(schema)) = (input.kind.clone(), schema.kind.clone()) else { continue; };
        if !matches!(arena.expr(input).kind, ArenaExprKind::Ident(name) if name == argv.name) { continue; }
        let ArenaExprKind::Record(fields) = arena.expr(schema).kind else { continue; };
        let mut parameters = BTreeMap::new();
        for field in arena.record_fields(fields) {
            let ArenaRecordFieldKind::Named { name, value, .. } = field.kind else { parameters.clear(); break; };
            let Some(LiteralConstant::Record(descriptor)) = LiteralConstant::analyze(arena, value, &FxHashMap::default()) else { parameters.clear(); break; };
            if descriptor.len() != 3 { parameters.clear(); break; }
            let Some(LiteralConstant::Str(kind)) = descriptor.get(&xsh::frontend::symbols::Name::intern("kind")) else { parameters.clear(); break; };
            let Some(default) = descriptor.get(&xsh::frontend::symbols::Name::intern("default")) else { parameters.clear(); break; };
            let Some(LiteralConstant::Str(help)) = descriptor.get(&xsh::frontend::symbols::Name::intern("help")) else { parameters.clear(); break; };
            let Some(default_text) = scalar_default_help(default) else { parameters.clear(); break; };
            if !matches!(kind.as_ref(), "Str" | "Int" | "UInt" | "Bool" | "Path" | "Duration")
                || help.as_ref() != format!("{kind}, default: {default_text}")
                || expected.get(&name) != Type::builtin_from_name(kind).as_ref()
                || !default.matches_data_type(expected.get(&name).unwrap()) {
                parameters.clear(); break;
            }
            let ArenaExprKind::Record(entries) = arena.expr(value).kind else { parameters.clear(); break; };
            let default_expr = arena.record_fields(entries).iter().find_map(|entry| match entry.kind {
                ArenaRecordFieldKind::Named { name, value, .. } if name == "default" => Some(value), _ => None,
            }).unwrap();
            let Some(default_source) = source.get(arena.expr(default_expr).span.range()) else { parameters.clear(); break; };
            if parameters.insert(name, format!("{name}: {kind} = {default_source}")).is_some() { parameters.clear(); break; }
        }
        if parameters.is_empty() || parameters.len() != expected.len() || parameters.len() != bindings.len() { continue; }
        if !arena.destructure_fields(bindings).iter().all(|field| parameters.contains_key(&field.name)
            && matches!(arena.binding_target(field.target).kind, ArenaBindingTargetKind::Name(name) if name == field.name)) { continue; }
        let Some(tail) = source.get(binding.span.end()..declaration.span.end()) else { continue; };
        if tail.contains(argv.name.as_str().as_str()) { continue; }
        let Some(header) = source.get(declaration.span.start()..binding.span.start()) else { continue; };
        let (Some(open), Some(close)) = (header.find('('), header.find(')')) else { continue; };
        if close < open { continue; }
        let replacement = format!("cli main({}){}{}", parameters.values().cloned().collect::<Vec<_>>().join(", "), header[close + 1..].trim_end(), tail);
        let mut diagnostic = Diagnostic::new(Severity::Warning, "this literal CLI schema can be represented by its entry signature")
            .with_code("lint.prefer-signature-cli")
            .with_label(Label::secondary(declaration.span, "parameter defaults and generated help match the strict option schema"));
        if source.get(declaration.span.range()).is_some_and(|text| text.contains('#'))
            || source.get(..declaration.span.start()).and_then(|text| text.trim_end().lines().last()).is_some_and(|line| line.trim_start().starts_with("##")) {
            diagnostic = diagnostic.with_note("no automatic fix: the entry contains documentation or comments");
        } else {
            let mut candidate = source.to_string();
            candidate.replace_range(declaration.span.range(), &replacement);
            let parsed = Parser::parse_source_arena_only(declaration.span.source_id, &candidate);
            if !parsed.diagnostics.is_empty() { continue; }
            let after = Checker::check_arena(&parsed.arena, &candidate);
            if !before.diagnostics.is_empty() || !after.diagnostics.is_empty() { continue; }
            let old_prefix = declaration.span.start()..binding.span.end();
            let delta = replacement.len() as isize - declaration.span.range().len() as isize;
            let new_prefix = old_prefix.start..old_prefix.end.checked_add_signed(delta).unwrap();
            let facts = |checked: &xsh::frontend::check::CheckOutput, prefix: std::ops::Range<usize>, shift: isize| {
                checked.expr_types.iter().filter_map(|(span, ty)| {
                    if span.start() < prefix.end && span.end() > prefix.start { return None; }
                    let offset = if span.start() >= prefix.end { shift } else { 0 };
                    Some((span.start().checked_add_signed(offset)?, span.end().checked_add_signed(offset)?, super::checked_return_type_shape(ty)))
                }).collect::<Vec<_>>()
            };
            let original_facts = program.symbol_owner().with_current(|| facts(&before, old_prefix, delta));
            let replacement_facts = parsed.arena.symbol_owner().with_current(|| facts(&after, new_prefix, 0));
            if original_facts != replacement_facts { continue; }
            let position_facts = |checked: &xsh::frontend::check::CheckOutput, prefix: std::ops::Range<usize>, shift: isize| {
                checked.statement_positions.iter().filter_map(|(span, position)| {
                    if span.start() < prefix.end && span.end() > prefix.start { return None; }
                    let offset = if span.start() >= prefix.end { shift } else { 0 };
                    Some((span.start().checked_add_signed(offset)?, span.end().checked_add_signed(offset)?, *position))
                }).collect::<Vec<_>>()
            };
            if position_facts(&before, declaration.span.start()..binding.span.end(), delta)
                != position_facts(&after, declaration.span.start()..binding.span.end().checked_add_signed(delta).unwrap(), 0) { continue; }
            let return_facts = |checked: &xsh::frontend::check::CheckOutput| checked.function_return_types.values()
                .map(super::checked_return_type_shape).collect::<Vec<_>>();
            if program.symbol_owner().with_current(|| return_facts(&before))
                != parsed.arena.symbol_owner().with_current(|| return_facts(&after)) { continue; }
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(declaration.span, "derive the CLI from the signature", replacement));
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}

fn scalar_default_help(value: &LiteralConstant) -> Option<String> {
    Some(match value {
        LiteralConstant::Bool(value) => value.to_string(),
        LiteralConstant::Int(value) => value.to_string(),
        LiteralConstant::Str(value) => format!("{value:?}"),
        LiteralConstant::Path(value) => value.to_string(),
        LiteralConstant::Duration(millis) => format!("{millis}ms"),
        _ => return None,
    })
}
