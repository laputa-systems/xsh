use super::lint_write_lines::path_receiver_text;
use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};

/// `P.read_text()?.lines()` and `fs.read_text(P)?.lines()` on a checked Path
/// are `P.read_lines()?`: the same read, the same decoding, the same split,
/// and the same failure propagated from the same place.
///
/// A line-by-line loop over that shape is skipped. `lint.prefer-file-lines`
/// reports it and recommends the lazy `P.lines()?`, and rewriting it here
/// would silence that advice; `reported` holds the diagnostics so far, which
/// include that lint's for the loop being visited.
pub(super) fn read_lines(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
    reported: &[Diagnostic],
) -> Option<Diagnostic> {
    let (path, method) = read_text_lines_path(arena, expr, expr_types)?;
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
    let hint = if call.contains('#') {
        None
    } else if method {
        // Everything before the calls is kept verbatim, so a receiver of
        // any shape stays as written.
        let calls = Span::new(lines.span.source_id, path.span.end(), lines.span.end());
        source
            .get(calls.range())
            .filter(|calls| {
                calls.split_whitespace().collect::<String>() == ".read_text()?.lines()"
            })
            .map(|_| (calls, ".read_lines()?".to_string()))
    } else {
        path_receiver_text(source, path.kind, path.span)
            .map(|receiver| (lines.span, format!("{receiver}.read_lines()?")))
    };
    if let Some((span, replacement)) = hint {
        diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
            span,
            "read the lines with `read_lines`",
            replacement,
        ));
    }
    Some(diagnostic)
}

/// The Path that `expr` reads when it is `P.read_text()?.lines()` or
/// `fs.read_text(P)?.lines()` on a checked Path, and whether the read is
/// spelled as a method.
pub(super) fn read_text_lines_path(
    arena: &AstArena,
    expr: ExprId,
    expr_types: &BTreeMap<Span, Type>,
) -> Option<(ExprId, bool)> {
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
    let (path, method) = read_text_path(arena, arena.expr(read).kind)?;
    // The guarded hop must yield the plain list `read_lines()?` yields.
    (expr_types.get(&arena.expr(path).span) == Some(&Type::Path)
        && expr_types.get(&lines.span) == Some(&Type::List(Box::new(Type::Str))))
    .then_some((path, method))
}

/// The path of `path.read_text()` (with `true`) or `fs.read_text(path)`.
fn read_text_path(arena: &AstArena, read: ArenaExprKind) -> Option<(ExprId, bool)> {
    let ArenaExprKind::Call { callee, args } = read else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if name != "read_text" {
        return None;
    }
    let module_call =
        matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "fs");
    match arena.call_args(args) {
        [] if !module_call => Some((base, true)),
        [argument] if module_call => match argument.kind {
            ArenaCallArgKind::Positional(path) => Some((path, false)),
            _ => None,
        },
        _ => None,
    }
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
    fn read_text_then_lines_becomes_read_lines_for_both_spellings() {
        let source = "proc load(source: Path, root: Path) [fs, error] -> Result[Int] {\n  let direct = source.read_text()?.lines()\n  let joined = fp\"{root}/list\".read_text()?.lines().len()\n  let by_module = fs.read_text(source)?.lines()\n  direct.len() + joined + by_module.len()\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferReadLines))
        );
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc load(source: Path, root: Path) [fs, error] -> Result[Int] {\n  let direct = source.read_lines()?\n  let joined = fp\"{root}/list\".read_lines()?.len()\n  let by_module = source.read_lines()?\n  direct.len() + joined + by_module.len()\n}\n"
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

    // A bare path word would absorb `.read_lines()` into the path.
    #[test]
    fn a_module_call_on_a_bare_path_word_gets_no_rewrite() {
        let source = "proc load() [fs, error] -> Result[Int] {\n  fs.read_text(./list.txt)?.lines().len()\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
    }
}
