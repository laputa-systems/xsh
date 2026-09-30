use xsh::diagnostic::{Diagnostic, FixHint, Label};
use xsh::frontend::check::Checker;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaProgram, ArenaStmtKind, FunctionDefId};
use xsh::frontend::syntax::parser::Parser;

pub(super) fn lint_callable_aliases(program: &ArenaProgram, source: &str) -> Vec<Diagnostic> {
    lint_callable_aliases_with_check(program, source, || Checker::check_arena(program, source))
}

fn lint_callable_aliases_with_check(
    program: &ArenaProgram,
    source: &str,
    check_original: impl FnOnce() -> xsh::frontend::check::CheckOutput,
) -> Vec<Diagnostic> {
    let before = std::cell::LazyCell::new(check_original);
    let mut diagnostics = Vec::new();
    for statement in program.statement_ids() {
        let outer = program.arena.stmt(statement);
        let (inner, exported) = match outer.kind {
            ArenaStmtKind::Export(inner) => (program.arena.stmt(inner), true),
            _ => (outer.clone(), false),
        };
        let (definition, pure) = match inner.kind {
            ArenaStmtKind::PureDef(id) => (id, true),
            ArenaStmtKind::ProcDef(id) => (id, false),
            _ => continue,
        };
        let wrapper = program.arena.function_def(definition);
        if wrapper.test_declaration || wrapper.name == "main" || wrapper.return_ty_defaulted { continue; }
        let body = program.arena.block(wrapper.body);
        let statements = program.arena.stmt_ids(body.statements).collect::<Vec<_>>();
        let [statement] = statements.as_slice() else { continue; };
        let call = match program.arena.stmt(*statement).kind {
            ArenaStmtKind::Expr(expression) | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expression))) => expression,
            _ => continue,
        };
        let ArenaExprKind::Call { callee, args } = program.arena.expr(call).kind else { continue; };
        let ArenaExprKind::Ident(name) = program.arena.expr(callee).kind else { continue; };
        if name == wrapper.name { continue; }
        let target = program.statement_ids().find_map(|statement| {
            let kind = match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                kind => kind,
            };
            match kind {
                ArenaStmtKind::PureDef(id) if pure && program.arena.function_def(id).name == name => Some(id),
                ArenaStmtKind::ProcDef(id) if !pure && program.arena.function_def(id).name == name => Some(id),
                _ => None,
            }
        });
        let Some(target) = target else { continue; };
        let params = program.arena.params(wrapper.params);
        let arguments = program.arena.call_args(args);
        if params.len() != arguments.len() || !params.iter().zip(arguments).all(|(parameter, argument)| {
            let expression = match argument.kind {
                ArenaCallArgKind::Positional(expression) if !parameter.rest => expression,
                ArenaCallArgKind::Named { name, value, .. } if !parameter.rest && name == parameter.name => value,
                ArenaCallArgKind::Splice { value, .. } if parameter.rest => value,
                _ => return false,
            };
            matches!(program.arena.expr(expression).kind, ArenaExprKind::Ident(name) if name == parameter.name)
        }) { continue; }
        // Preparing the complete program is needed only after a local wrapper
        // has forwarded every parameter unchanged to a callable of the same kind.
        if !before.diagnostics.is_empty() { return Vec::new(); }
        if !same_signature(program, source, &before.prepared_constants, definition, target) { continue; }
        let mut diagnostic = Diagnostic::warning("an exact forwarding callable can preserve its signature through an immutable alias")
            .with_code("lint.prefer-callable-alias")
            .with_label(Label::secondary(inner.span, "the alias executes the original callable without a wrapper traceback frame"));
        let text = source.get(outer.span.range()).unwrap_or_default();
        if !text.contains('#') {
            let replacement = format!("{}let {} = {}", if exported { "export " } else { "" }, wrapper.name, name);
            let mut rewritten = source.to_owned();
            rewritten.replace_range(outer.span.range(), &replacement);
            let parsed = Parser::parse_source_arena_only(outer.span.source_id, &rewritten);
            if parsed.diagnostics.is_empty() {
                let checked = Checker::check_arena(&parsed.arena, &rewritten);
                let offset = replacement.len() as isize - (outer.span.end() - outer.span.start()) as isize;
                let equivalent = before.expr_types.iter().filter(|(span, _)| span.end() <= outer.span.start() || span.start() >= outer.span.end())
                    .all(|(span, ty)| {
                        let new_span = if span.start() >= outer.span.end() {
                            xsh::frontend::source::Span::new(span.source_id, span.start().checked_add_signed(offset).unwrap(), span.end().checked_add_signed(offset).unwrap())
                        } else { *span };
                        let shape = program.symbol_owner().with_current(|| super::checked_return_type_shape(ty));
                        checked.expr_types.get(&new_span).is_some_and(|new_type| parsed.arena.symbol_owner().with_current(|| super::checked_return_type_shape(new_type)) == shape)
                    });
                if checked.diagnostics.is_empty() && equivalent {
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(outer.span, "preserve the original callable signature", replacement));
                }
            }
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}

fn same_signature(program: &ArenaProgram, source: &str, constants: &xsh::frontend::check::PreparedConstants, left: FunctionDefId, right: FunctionDefId) -> bool {
    let left = program.arena.function_def(left);
    let right = program.arena.function_def(right);
    if right.return_ty_defaulted || left.effects.map(|range| program.arena.effects(range).collect::<Vec<_>>())
        != right.effects.map(|range| program.arena.effects(range).collect::<Vec<_>>()) { return false; }
    let annotation = |id| source.get(program.arena.type_expr_span(id).range());
    if annotation(left.return_ty) != annotation(right.return_ty) { return false; }
    let left = program.arena.params(left.params);
    let right = program.arena.params(right.params);
    left.len() == right.len() && left.iter().zip(right).all(|(left, right)|
        left.name == right.name && left.rest == right.rest && annotation(left.ty) == annotation(right.ty)
        && match (left.default, right.default) {
            (None, None) => true,
            (Some(left), Some(right)) => constants.analyze_expression(&program.arena, left).is_some_and(|left| Some(left) == constants.analyze_expression(&program.arena, right)),
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use xsh::frontend::source::SourceId;

    fn lint_with_preparation_count(source: &str) -> (Vec<Diagnostic>, usize) {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let preparations = Cell::new(0);
        let diagnostics = parsed.arena.symbol_owner().with_current(|| {
            lint_callable_aliases_with_check(&parsed.arena, source, || {
                preparations.set(preparations.get() + 1);
                Checker::check_arena(&parsed.arena, source)
            })
        });
        (diagnostics, preparations.get())
    }

    #[test]
    fn callable_aliases_skip_original_preparation_without_exact_local_forwarding() {
        for source in [
            "let prepared = rx\"ready\"\n",
            "pure render(value: Str) -> Str { value }\nproc format(value: Str) [] -> Str { render(value) }\n",
            "pure render(left: Str, right: Str) -> Str { left + right }\npure format(left: Str, right: Str) -> Str { render(right, left) }\n",
            "pure render(value: Str) -> Str { value }\npure format(value: Str) -> Str { render(value.trim()) }\n",
        ] {
            let (diagnostics, preparations) = lint_with_preparation_count(source);
            assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
            assert_eq!(preparations, 0, "{source}");
        }
    }

    #[test]
    fn callable_aliases_cache_original_preparation_across_forwarding_candidates() {
        let source = "pure render(value: Str) -> Str { value }\npure first(value: Str) -> Str { render(value) }\npure second(value: Str) -> Str { render(value) }\n";
        let (diagnostics, preparations) = lint_with_preparation_count(source);
        assert_eq!(preparations, 1);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.fix_hints.len() == 1));
    }
}
