use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaExprKind, AstArena, ExprId};

/// `P.read_text()?.lines()` on a checked Path is `P.read_lines()?`: the same
/// read, the same decoding, the same split, and the same failure propagated
/// from the same place.
///
/// A `for` loop directly over that shape belongs to `lint.prefer-file-lines`,
/// which recommends the lazy `P.lines()?` and cannot fix it (laziness moves a
/// decoding failure into the loop). Rewriting the loop here would silence
/// that advice and report one site twice, so it is skipped; `reported` holds
/// the diagnostics so far, which include that lint's for the loop being
/// visited.
pub(super) fn read_lines(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
    reported: &[Diagnostic],
) -> Option<Diagnostic> {
    let path = read_text_lines_path(arena, expr, expr_types)?;
    let lines = arena.expr(expr);
    let path = arena.expr(path);
    let in_reported_loop = reported
        .iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferFileLines))
        .flat_map(|diagnostic| &diagnostic.labels)
        .any(|label| {
            label.span.source_id == lines.span.source_id
                && label.span.start() <= lines.span.start()
                && lines.span.end() <= label.span.end()
        });
    if in_reported_loop {
        return None;
    }
    let mut diagnostic = Diagnostic::warning("a file is read as text only to be split into lines")
        .with_code(DiagnosticCode::LintPreferReadLines)
        .with_label(Label::secondary(
            lines.span,
            "`read_lines()?` reads and splits in one call with the same failures",
        ));
    let call = source.get(lines.span.range()).unwrap_or("#");
    // Everything before the calls is kept verbatim, so a receiver of any
    // shape stays as written.
    let calls = Span::new(lines.span.source_id, path.span.end(), lines.span.end());
    let rewritable = !call.contains('#')
        && source.get(calls.range()).is_some_and(|calls| {
            calls.split_whitespace().collect::<String>() == ".read_text()?.lines()"
        });
    if rewritable {
        diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
            calls,
            "read the lines with `read_lines`",
            ".read_lines()?",
        ));
    }
    Some(diagnostic)
}

/// The Path that `expr` reads when it is `P.read_text()?.lines()` on a
/// checked Path. Both lints over this shape share the one definition.
pub(super) fn read_text_lines_path(
    arena: &AstArena,
    expr: ExprId,
    expr_types: &BTreeMap<Span, Type>,
) -> Option<ExprId> {
    let lines = arena.expr(expr);
    let ArenaExprKind::Call { callee, args } = lines.kind else {
        return None;
    };
    // `read_text()?.lines()` is a guarded hop that propagates the read's
    // `Err`; `(read_text()?).lines()` propagates first. Both are the shape.
    let read = match arena.expr(callee).kind {
        ArenaExprKind::NullSafeField { base, name } if name == "lines" => base,
        ArenaExprKind::Field { base, name } if name == "lines" => {
            let ArenaExprKind::Try(read) = arena.expr(base).kind else {
                return None;
            };
            read
        }
        _ => return None,
    };
    if !arena.call_args(args).is_empty() {
        return None;
    }
    let path = read_text_path(arena, arena.expr(read).kind)?;
    // The guarded hop must yield the plain list `read_lines()?` yields.
    (expr_types.get(&arena.expr(path).span) == Some(&Type::Path)
        && expr_types.get(&lines.span) == Some(&Type::List(Box::new(Type::Str))))
    .then_some(path)
}

/// The path of `path.read_text()`.
fn read_text_path(arena: &AstArena, read: ArenaExprKind) -> Option<ExprId> {
    let ArenaExprKind::Call { callee, args } = read else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    (name == "read_text" && arena.call_args(args).is_empty()).then_some(base)
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    // The lint runs from the linter's expression traversal, so the tests
    // drive the whole linter restricted to the two codes that share the shape.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let options = LintOptions {
            expr_types: checked.expr_types,
            only: Some(vec![
                DiagnosticCode::LintPreferFileLines,
                DiagnosticCode::LintPreferReadLines,
            ]),
            ..LintOptions::default()
        };
        Linter::lint(&parsed.arena, source, options).diagnostics
    }

    fn apply(diagnostics: &[Diagnostic], source: &str) -> String {
        let mut fixes = diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .collect::<Vec<_>>();
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
        let mut fixed = source.to_owned();
        for fix in fixes {
            fixed.replace_range(
                fix.span.unwrap().range(),
                fix.replacement.as_deref().unwrap(),
            );
        }
        fixed
    }

    #[test]
    fn read_text_then_lines_becomes_read_lines() {
        let source = "proc load(source: Path, root: Path) [fs, error] -> Result[Int] {\n  let direct = source.read_text()?.lines()\n  let joined = fp\"{root}/list\".read_text()?.lines().len()\n  direct.len() + joined\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferReadLines))
        );
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc load(source: Path, root: Path) [fs, error] -> Result[Int] {\n  let direct = source.read_lines()?\n  let joined = fp\"{root}/list\".read_lines()?.len()\n  direct.len() + joined\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // Anything between the read and the split, or a fallback instead of
    // propagation, is a different program.
    #[test]
    fn other_line_splits_are_left_alone() {
        let source = "proc load(source: Path, text: Str) [fs, error] -> Result[Int] {\n  let trimmed = source.read_text()?.trim().lines()\n  let fallback = (source.read_text() ?? \"\").lines()\n  let plain = text.lines()\n  let lazy = source.lines()? |> count\n  trimmed.len() + fallback.len() + plain.len() + lazy\n}\n";
        assert!(lint(source).is_empty());
    }

    // One site, one diagnostic: a loop directly over the lines is advised
    // toward lazy lines and is not rewritten; every other use, including a
    // loop over a list built from them, is rewritten to `read_lines`.
    #[test]
    fn a_line_loop_is_owned_by_the_lazy_lines_lint() {
        let source = "proc load(source: Path) [fs, error] -> Result[Int] {\n  for line in source.read_text()?.lines() {\n    print $line\n  }\n\n  for line in source.read_text()?.lines().push(\"end\") {\n    print $line\n  }\n\n  let kept = source.read_text()?.lines()\n  kept.len()\n}\n";
        let sites = lint(source)
            .iter()
            .map(|diagnostic| {
                let line = source[..diagnostic.labels[0].span.start()]
                    .matches('\n')
                    .count()
                    + 1;
                (diagnostic.code.unwrap(), line, diagnostic.fix_hints.len())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sites,
            [
                (DiagnosticCode::LintPreferFileLines, 2, 0),
                (DiagnosticCode::LintPreferReadLines, 6, 1),
                (DiagnosticCode::LintPreferReadLines, 10, 1),
            ]
        );
    }
}
