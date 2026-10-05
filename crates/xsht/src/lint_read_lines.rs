use super::lint_write_lines::path_receiver_text;
use std::collections::{BTreeMap, BTreeSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaProgram, AstArena, ExprId,
};

/// `P.read_text()?.lines()` and `fs.read_text(P)?.lines()` on a checked Path
/// are `P.read_lines()?`: the same read, the same decoding, the same split,
/// and the same failure propagated from the same place.
///
/// A line-by-line loop over that shape is skipped. `lint.prefer-file-lines`
/// already reports it and recommends the lazy `P.lines()?`, and rewriting it
/// here would silence that advice; `loops` holds the spans it reported.
pub(super) fn lint_read_lines(
    program: &ArenaProgram,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    loops: &[Span],
) -> Vec<Diagnostic> {
    let arena = &program.arena;
    let mut reported = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for index in 0..arena.expr_tags.len() {
        let lines = arena.expr(ExprId::from_index(index));
        let ArenaExprKind::Call { callee, args } = lines.kind else {
            continue;
        };
        // `read_text()?.lines()` is a guarded hop that propagates the read's
        // `Err`; `(read_text()?).lines()` propagates first. Both are the shape.
        let read = match arena.expr(callee).kind {
            ArenaExprKind::NullSafeField { base, name } if name == "lines" => base,
            ArenaExprKind::Field { base, name } if name == "lines" => {
                let ArenaExprKind::Try(read) = arena.expr(base).kind else {
                    continue;
                };
                read
            }
            _ => continue,
        };
        if !arena.call_args(args).is_empty() {
            continue;
        }
        let Some((path, method)) = read_text_path(arena, arena.expr(read).kind) else {
            continue;
        };
        let path = arena.expr(path);
        // The guarded hop must yield the plain list `read_lines()?` yields.
        if expr_types.get(&path.span) != Some(&Type::Path)
            || expr_types.get(&lines.span) != Some(&Type::List(Box::new(Type::Str)))
            || loops.iter().any(|iteration| {
                iteration.source_id == lines.span.source_id
                    && iteration.start() <= lines.span.start()
                    && lines.span.end() <= iteration.end()
            })
            || !reported.insert(lines.span)
        {
            continue;
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
        diagnostics.push(diagnostic);
    }
    diagnostics
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
    use super::lint_read_lines;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::{SourceId, Span};
    use xsh::frontend::syntax::parser::Parser;

    fn lint_outside(source: &str, loops: &[Span]) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        parsed
            .arena
            .symbol_owner()
            .with_current(|| lint_read_lines(&parsed.arena, source, &checked.expr_types, loops))
    }

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_outside(source, &[])
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

    #[test]
    fn a_loop_already_reported_for_lazy_lines_is_skipped() {
        let source = "proc load(source: Path) [fs, error] {\n  for line in source.read_text()?.lines() {\n    print $line\n  }\n}\n";
        let start = source.find("source.read_text").unwrap();
        let end = source.find(" {\n    print").unwrap();
        assert_eq!(lint(source).len(), 1);
        let iteration = Span::new(SourceId::new(0), start, end);
        assert!(lint_outside(source, &[iteration]).is_empty());
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
