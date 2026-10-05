use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaExprOrRun, ArenaStmtKind, StmtId};

/// `let _ = test.expect(ctx, source, status: 0)?` discards a value the
/// statement may drop by itself: the call's signature is registered as
/// discardable, so `test.expect(ctx, source, status: 0)?` is a statement.
///
/// The checker decides the sites: it records each untyped `let _ = VALUE`
/// whose value comes from such a call and stands where `VALUE` alone would
/// be a statement. A binding at a tail that is read as its block's value is
/// not among them, because there the bare call would become that value.
/// Syntax only selects what the fix can print.
pub(super) fn lint_redundant_discard(linter: &mut super::Linter<'_>, statement: StmtId) {
    let arena = linter.arena;
    let source = linter.source;
    let stmt = arena.stmt(statement);
    if !linter.discardable_bindings.contains(&stmt.span) {
        return;
    }
    let ArenaStmtKind::Let {
        initializer: ArenaExprOrRun::Expr(initializer),
        ..
    } = stmt.kind
    else {
        return;
    };
    let value = arena.expr(initializer).span;
    let binding = Span::new(stmt.span.source_id, stmt.span.start(), value.start());
    let replaced = Span::new(stmt.span.source_id, stmt.span.start(), value.end());
    // A comment between `let` and the call would go with the binding.
    if super::span_may_contain_comment(source, binding) {
        return;
    }
    let Some(call_text) = source.get(value.range()) else {
        return;
    };
    // Without the binding the call is eight columns narrower, so the
    // formatter may lay it out differently; a statement the formatter cannot
    // place is only shortened.
    let fix = match super::lint_prefer_test_expect::formatted_in_place(source, replaced, call_text)
    {
        Some(statement) => FixHint::replacement(replaced, "remove `let _ =`", statement),
        None => FixHint::deletion(binding, "remove `let _ =`"),
    };
    linter.diagnostics.push(
        Diagnostic::warning("`let _ =` discards a value that the statement may drop by itself")
            .with_code(DiagnosticCode::LintRedundantDiscard)
            .with_label(Label::secondary(
                binding,
                "the call's value is discardable once its failure is propagated",
            ))
            .with_fix_hint(fix),
    );
}

#[cfg(test)]
mod tests {
    use super::super::lint_redundant_propagation::tests::lint_rule;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::SourceId;

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintRedundantDiscard)
    }

    /// The fixed source lints clean and is what the formatter prints.
    fn fixed(source: &str, sites: usize) -> String {
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), sites, "{diagnostics:?}");
        let mut fixes = diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .collect::<Vec<_>>();
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
        let mut fixed = source.to_owned();
        for fix in fixes {
            fixed.replace_range(
                fix.span.unwrap().range(),
                fix.replacement.as_deref().unwrap_or_default(),
            );
        }
        assert!(lint(&fixed).is_empty(), "{fixed}");
        let formatted =
            super::super::super::format::Formatter::new().format_source(SourceId::new(0), &fixed);
        assert!(
            formatted.diagnostics.is_empty(),
            "{:?}",
            formatted.diagnostics
        );
        assert_eq!(formatted.formatted, fixed);
        fixed
    }

    #[test]
    fn a_discarded_expectation_becomes_a_statement() {
        let source = "test runs { |ctx|\n  let _ = test.expect(ctx, \"exit 1\", status: 1)?\n  if ctx.name != \"\" {\n    let _ = test.expect(ctx, \"print 1\", status: 0, stdout: [\"1\"])?\n  }\n\n  let _ = test.expect(ctx, \"exit 0\", status: 0)?\n}\n";
        assert_eq!(
            fixed(source, 3),
            "test runs { |ctx|\n  test.expect(ctx, \"exit 1\", status: 1)?\n  if ctx.name != \"\" {\n    test.expect(ctx, \"print 1\", status: 0, stdout: [\"1\"])?\n  }\n\n  test.expect(ctx, \"exit 0\", status: 0)?\n}\n"
        );
    }

    // The call is eight columns narrower without the binding, so a call the
    // formatter broke only for its width goes back on one line, and a call
    // that still does not fit stays broken.
    #[test]
    fn the_call_is_laid_out_as_the_formatter_prints_it() {
        let source = "test runs { |ctx|\n  let _ = test.expect(\n    ctx,\n    \"exit 1\",\n    status: 1,\n    stderr: [\"a fragment of the output that the script prints\", \"more\"],\n  )?\n  let _ = test.expect(\n    ctx,\n    r\"\"\"\nprint 1\nexit 1\n\"\"\",\n    status: 1,\n  )?\n}\n";
        let fixed = fixed(source, 2);
        assert!(fixed.contains("\n  test.expect(\n    ctx,\n    r\"\"\"\nprint 1\nexit 1\n\"\"\",\n    status: 1,\n  )?\n"), "{fixed}");
        assert!(!fixed.contains("let _"), "{fixed}");
    }

    // A binding that is read, a call that is not propagated, a function
    // without the mark, and a tail that is its body's value are left alone.
    #[test]
    fn other_discards_are_left_alone() {
        let source = "test runs { |ctx|\n  let output = test.expect(ctx, \"exit 1\", status: 1)?\n  assert output.status == 1\n  let _ = test.expect(ctx, \"exit 1\", status: 1)\n  let _ = test.run_script(ctx, \"exit 1\")?\n  let attempt = try {\n    let _ = test.expect(ctx, \"exit 0\", status: 0)?\n  }\n  let _ = attempt\n}\n";
        assert!(lint(source).is_empty(), "{:?}", lint(source));
    }
}
