use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaStmtKind, ArenaSugar,
    BindingTargetId, BlockId, ExprId,
};
use xsh::frontend::syntax::parser::Parser;

/// `for _ in range(COUNT) { ... }` only counts: it is the loop that
/// `repeat COUNT times { ... }` is defined to mean, so rewriting the head
/// changes nothing but the spelling. A `range` call with a start, a named
/// argument, or a spread is a different loop and is left alone.
pub(super) fn lint_counted_loop(
    linter: &mut super::Linter<'_>,
    statement: Span,
    target: BindingTargetId,
    iter: ExprId,
    block: BlockId,
) {
    let arena = linter.arena;
    if !matches!(arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "_")
    {
        return;
    }
    let ArenaExprKind::Call { callee, args } = arena.expr(iter).kind else {
        return;
    };
    if !matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "range") {
        return;
    }
    let [argument] = arena.call_args(args) else {
        return;
    };
    let ArenaCallArgKind::Positional(count) = argument.kind else {
        return;
    };
    let body = arena.span(arena.block(block).span);
    let head = Span::new(statement.source_id, statement.start(), body.start());
    let mut diagnostic = Diagnostic::new(
        Severity::Warning,
        "prefer `repeat N times` for a loop that only counts",
    )
    .with_code(DiagnosticCode::LintPreferRepeat)
    .with_label(Label::secondary(
        Span::new(statement.source_id, statement.start(), arena.expr(iter).span.end()),
        "this loop ignores its index; write `repeat N times { ... }`",
    ));
    if let Some(replacement) = repeat_head(linter, head, count) {
        diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
            head,
            "rewrite as `repeat ... times`",
            replacement,
        ));
    }
    linter.diagnostics.push(diagnostic);
}

/// The `repeat COUNT times ` head that replaces `for _ in range(COUNT) `, when
/// the count provably parses to the same expression there. A head with a
/// comment is left to a manual rewrite so the comment is not dropped.
fn repeat_head(linter: &super::Linter<'_>, head: Span, count: ExprId) -> Option<String> {
    let arena = linter.arena;
    let source = linter.source;
    let count_span = arena.expr(count).span;
    let around_count = [
        source.get(head.start()..count_span.start())?,
        source.get(count_span.end()..head.end())?,
    ];
    if around_count.iter().any(|text| text.contains('#')) {
        return None;
    }
    let count_text = source.get(count_span.range())?;
    let expected = super::super::format::canonical_subtree(arena, source, Err(count));
    [
        format!("repeat {count_text} times "),
        format!("repeat ({count_text}) times "),
    ]
    .into_iter()
    .find(|candidate| {
        let rewritten = format!("{candidate}{{\n}}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &rewritten);
        if !parsed.diagnostics.is_empty() {
            return false;
        }
        let mut statements = parsed.arena.statement_ids();
        let (Some(statement), None) = (statements.next(), statements.next()) else {
            return false;
        };
        let ArenaStmtKind::Sugar { form, operands, .. } = parsed.arena.arena.stmt(statement).kind
        else {
            return false;
        };
        let ArenaSugar::Repeat { count, .. } = parsed.arena.arena.sugar(form, operands);
        parsed.arena.symbol_owner().with_current(|| {
            super::super::format::canonical_subtree(&parsed.arena.arena, &rewritten, Err(count))
        }) == expected
    })
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn counted_loops(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(&parsed.arena, source, LintOptions::default())
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferRepeat))
            .collect()
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
    fn counted_loops_become_repeat_and_the_result_is_clean() {
        let source = "proc poll(limit: Int) {\n  var seen = 0\n  for _ in range(limit) {\n    seen += 1\n    for _ in range(seen + 1) { seen += 1 }\n  }\n  print $seen\n}\n";
        let diagnostics = counted_loops(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc poll(limit: Int) {\n  var seen = 0\n  repeat limit times {\n    seen += 1\n    repeat seen + 1 times { seen += 1 }\n  }\n  print $seen\n}\n"
        );
        // The fixed program checks, and its expansion is not reported again.
        assert!(counted_loops(&fixed).is_empty());
    }

    #[test]
    fn a_count_that_begins_with_a_brace_still_converges() {
        let source = "for _ in range({times: 2}.times) {\n  print \"tick\"\n}\n";
        let diagnostics = counted_loops(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "repeat {times: 2}.times times {\n  print \"tick\"\n}\n"
        );
        assert!(counted_loops(&fixed).is_empty(), "{fixed}");
    }

    #[test]
    fn other_loops_over_range_are_left_alone() {
        for source in [
            "for index in range(3) {\n  print $index\n}\n",
            "for _ in range(1, 3) {\n  print \"tick\"\n}\n",
            "for _ in [1, 2, 3] {\n  print \"tick\"\n}\n",
            "repeat 3 times {\n  print \"tick\"\n}\n",
        ] {
            assert!(counted_loops(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn a_comment_in_the_head_keeps_the_report_but_not_the_fix() {
        let source = "for _ in range(\n  3 # attempts\n) {\n  print \"tick\"\n}\n";
        let diagnostics = counted_loops(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
    }
}
