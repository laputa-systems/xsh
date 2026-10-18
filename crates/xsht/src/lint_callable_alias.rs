use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::{CheckBundleView, CheckOptions};
#[cfg(test)]
use xsh::frontend::check::Checker;
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaProgram, ArenaStmtKind, FunctionDefId,
};
#[cfg(test)]
use xsh::frontend::syntax::parser::Parser;

pub(super) fn lint_callable_aliases(
    view: &CheckBundleView<'_>,
    source: &str,
    check_original: Option<std::sync::Arc<xsh::frontend::check::CheckOutput>>,
) -> Vec<Diagnostic> {
    lint_callable_aliases_with_check(view, source, check_original)
}

fn lint_callable_aliases_with_check(
    view: &CheckBundleView<'_>,
    source: &str,
    check_original: Option<std::sync::Arc<xsh::frontend::check::CheckOutput>>,
) -> Vec<Diagnostic> {
    let program = view.program();
    let Some(before) = check_original else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();
    for statement in view.statement_ids() {
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
        if wrapper.test_declaration || wrapper.name == "main" || wrapper.return_ty_defaulted {
            continue;
        }
        let body = program.arena.block(wrapper.body);
        let statements = program.arena.stmt_ids(body.statements).collect::<Vec<_>>();
        let [statement] = statements.as_slice() else {
            continue;
        };
        let call = match program.arena.stmt(*statement).kind {
            ArenaStmtKind::Expr(expression)
            | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expression))) => expression,
            _ => continue,
        };
        let ArenaExprKind::Call { callee, args } = program.arena.expr(call).kind else {
            continue;
        };
        let ArenaExprKind::Ident(name) = program.arena.expr(callee).kind else {
            continue;
        };
        if name == wrapper.name {
            continue;
        }
        let target = view.statement_ids().find_map(|statement| {
            let kind = match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                kind => kind,
            };
            match kind {
                ArenaStmtKind::PureDef(id)
                    if pure && program.arena.function_def(id).name == name =>
                {
                    Some(id)
                }
                ArenaStmtKind::ProcDef(id)
                    if !pure && program.arena.function_def(id).name == name =>
                {
                    Some(id)
                }
                _ => None,
            }
        });
        let Some(target) = target else {
            continue;
        };
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
        if before.diagnostics.iter().any(|diagnostic| {
            diagnostic.code != Some(DiagnosticCode::CheckRedundantParens)
        }) {
            return Vec::new();
        }
        if !same_signature(
            program,
            source,
            &before.prepared_constants,
            definition,
            target,
        ) {
            continue;
        }
        let mut diagnostic = Diagnostic::warning(
            "an exact forwarding callable can preserve its signature through an immutable alias",
        )
        .with_code(DiagnosticCode::LintPreferCallableAlias)
        .with_label(Label::secondary(
            inner.span,
            "the alias executes the original callable without a wrapper traceback frame",
        ));
        let text = source.get(outer.span.range()).unwrap_or_default();
        if !text.contains('#') {
            let replacement = format!(
                "{}let {} = {}",
                if exported { "export " } else { "" },
                wrapper.name,
                name
            );
            let mut rewritten = source.to_owned();
            rewritten.replace_range(outer.span.range(), &replacement);
            let snapshot = super::probes::LintProbeTransport::standalone(
                outer.span.source_id,
                &rewritten,
                CheckOptions::default(),
            );
            if snapshot.parse_diagnostics.is_empty() {
                let Some(checked) = snapshot.checked.as_ref() else {
                    continue;
                };
                let offset =
                    replacement.len() as isize - (outer.span.end() - outer.span.start()) as isize;
                let equivalent = before
                    .expr_types
                    .iter()
                    .filter(|(span, _)| {
                        span.end() <= outer.span.start() || span.start() >= outer.span.end()
                    })
                    .all(|(span, ty)| {
                        let new_span = if span.start() >= outer.span.end() {
                            xsh::frontend::source::Span::new(
                                span.source_id,
                                span.start().checked_add_signed(offset).unwrap(),
                                span.end().checked_add_signed(offset).unwrap(),
                            )
                        } else {
                            *span
                        };
                        let shape = program
                            .symbol_owner()
                            .with_current(|| super::checked_return_type_shape(ty));
                        checked.expr_types.get(&new_span).is_some_and(|new_type| {
                            snapshot
                                .program
                                .symbol_owner()
                                .with_current(|| super::checked_return_type_shape(new_type))
                                == shape
                        })
                    });
                if checked.diagnostics.iter().all(|diagnostic| {
                    diagnostic.code == Some(DiagnosticCode::CheckRedundantParens)
                }) && equivalent {
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                        outer.span,
                        "preserve the original callable signature",
                        replacement,
                    ));
                }
            }
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}

fn same_signature(
    program: &ArenaProgram,
    source: &str,
    constants: &xsh::frontend::check::PreparedConstants,
    left: FunctionDefId,
    right: FunctionDefId,
) -> bool {
    let left = program.arena.function_def(left);
    let right = program.arena.function_def(right);
    if right.return_ty_defaulted
        || left
            .effects
            .map(|range| program.arena.effects(range).collect::<Vec<_>>())
            != right
                .effects
                .map(|range| program.arena.effects(range).collect::<Vec<_>>())
    {
        return false;
    }
    let annotation = |id| source.get(program.arena.type_expr_span(id).range());
    if annotation(left.return_ty) != annotation(right.return_ty) {
        return false;
    }
    let left = program.arena.params(left.params);
    let right = program.arena.params(right.params);
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name
                && left.rest == right.rest
                && left.annotation.map(annotation) == right.annotation.map(annotation)
                && match (left.default, right.default) {
                    (None, None) => true,
                    (Some(left), Some(right)) => constants
                        .analyze_expression(&program.arena, left)
                        .is_some_and(|left| {
                            Some(left) == constants.analyze_expression(&program.arena, right)
                        }),
                    _ => false,
                }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use xsh::frontend::source::SourceId;

    fn lint(source: &str, include_checked_output: bool) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let view = CheckBundleView::for_root(
                &parsed.arena,
                parsed.arena.statements,
                None,
                Some(SourceId::new(0)),
            );
            let checked = include_checked_output
                .then(|| std::sync::Arc::new(Checker::check_arena(&parsed.arena, source)));
            lint_callable_aliases_with_check(&view, source, checked)
        })
    }

    #[test]
    fn callable_aliases_preserve_findings_and_fixes_with_redundant_parentheses() {
        for expression in ["value + 1", "(value) + (1)"] {
            let source = format!(
                "pure target(value: Int) -> Int {{ {expression} }}\npure wrapper(value: Int) -> Int {{ target(value) }}\n"
            );
            let diagnostics = lint(&source, true);
            assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
            assert_eq!(diagnostics[0].fix_hints.len(), 1, "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn callable_aliases_reject_semantic_checker_errors() {
        let source = "pure target(value: Int) -> Int { value + missing }\npure wrapper(value: Int) -> Int { target(value) }\n";
        assert!(lint(source, true).is_empty());
    }

    #[test]
    fn callable_aliases_need_explicit_original_checked_facts() {
        let source = "pure render(value: Str) -> Str { value }\npure format(value: Str) -> Str { render(value) }\n";
        assert!(lint(source, false).is_empty());
        let diagnostics = lint(source, true);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn callable_aliases_skip_non_forwarding_candidates_without_baseline_facts() {
        for source in [
            "let prepared = rx\"ready\"\n",
            "pure render(value: Str) -> Str { value }\nproc format(value: Str) [] -> Str { render(value) }\n",
            "pure render(left: Str, right: Str) -> Str { left + right }\npure format(left: Str, right: Str) -> Str { render(right, left) }\n",
            "pure render(value: Str) -> Str { value }\npure format(value: Str) -> Str { render(value.trim()) }\n",
        ] {
            assert!(lint(source, false).is_empty(), "{source}");
        }
    }

    #[test]
    fn callable_aliases_use_shared_original_facts_across_forwarding_candidates() {
        let source = "pure render(value: Str) -> Str { value }\npure first(value: Str) -> Str { render(value) }\npure second(value: Str) -> Str { render(value) }\n";
        let diagnostics = lint(source, true);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.fix_hints.len() == 1)
        );
    }
}
