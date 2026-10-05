use std::collections::{BTreeMap, BTreeSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, ArenaProgram, ExprId};

/// `P.display().starts_with("/")` on a checked Path asks whether the path is
/// absolute, which `P.starts_with(p"/")` answers on the path itself.
///
/// This is the only text test on `.display()` with an equal component test:
/// display text begins with `/` exactly when the path has a root component,
/// because the lossy conversion never produces or removes an ASCII byte. Any
/// longer prefix, and every suffix, differs between the two readings
/// (`"lib/"` is not a text prefix of `lib`, `".a"` is a text suffix of
/// `x/.a`), so those calls are left alone.
pub(super) fn lint_path_text_queries(
    program: &ArenaProgram,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
) -> Vec<Diagnostic> {
    let arena = &program.arena;
    let mut reported = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for index in 0..arena.expr_tags.len() {
        let query = arena.expr(ExprId::from_index(index));
        let ArenaExprKind::Call { callee, args } = query.kind else {
            continue;
        };
        let ArenaExprKind::Field {
            base: display,
            name,
        } = arena.expr(callee).kind
        else {
            continue;
        };
        if name != "starts_with" {
            continue;
        }
        let [argument] = arena.call_args(args) else {
            continue;
        };
        let ArenaCallArgKind::Positional(prefix) = argument.kind else {
            continue;
        };
        let ArenaExprKind::Str(prefix) = arena.expr(prefix).kind else {
            continue;
        };
        if arena.string_literal(prefix).as_ref() != "/" {
            continue;
        }
        let display = arena.expr(display);
        let ArenaExprKind::Call {
            callee: display_callee,
            args: display_args,
        } = display.kind
        else {
            continue;
        };
        let ArenaExprKind::Field {
            base: path,
            name: display_name,
        } = arena.expr(display_callee).kind
        else {
            continue;
        };
        if display_name != "display" || !arena.call_args(display_args).is_empty() {
            continue;
        }
        let path = arena.expr(path).span;
        if expr_types.get(&path) != Some(&Type::Path) || !reported.insert(query.span) {
            continue;
        }
        let mut diagnostic =
            Diagnostic::warning("absolute-path test goes through the Path's display text")
                .with_code(DiagnosticCode::LintPathTextQuery)
                .with_label(Label::secondary(
                    query.span,
                    "`starts_with` on the Path compares components and needs no text conversion",
                ));
        // The receiver is kept verbatim. A rewrite that would drop a comment
        // is left to the author.
        let conversion = Span::new(query.span.source_id, path.end(), display.span.end());
        if query.span.start() == path.start()
            && source.get(conversion.range()) == Some(".display()")
            && source
                .get(display.span.end()..query.span.end())
                .is_some_and(|rest| !rest.contains('#'))
        {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                Span::new(query.span.source_id, path.end(), query.span.end()),
                "ask the Path for its root component",
                ".starts_with(p\"/\")".to_string(),
            ));
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::lint_path_text_queries;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        parsed
            .arena
            .symbol_owner()
            .with_current(|| lint_path_text_queries(&parsed.arena, source, &checked.expr_types))
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
    fn display_root_prefix_test_becomes_a_component_test() {
        let source = "pure rooted(target: Path, entry: FsEntry) -> Bool {\n  target.display().starts_with(\"/\") or ! entry.path.display().starts_with(\"/\")\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPathTextQuery))
        );
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "pure rooted(target: Path, entry: FsEntry) -> Bool {\n  target.starts_with(p\"/\") or ! entry.path.starts_with(p\"/\")\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // A longer prefix, a suffix, and a Str receiver each mean something a
    // component test does not.
    #[test]
    fn text_queries_without_an_equal_component_test_are_left_alone() {
        let source = "pure queries(target: Path, text: Str) -> Bool {\n  target.display().starts_with(\"lib/\") or target.display().ends_with(\".a\") or target.display().ends_with(\"/\") or text.starts_with(\"/\") or target.name().starts_with(\"/\")\n}\n";
        assert!(lint(source).is_empty());
    }

    #[test]
    fn spelling_outside_the_exact_shape_is_reported_without_a_fix() {
        let source = "pure rooted(target: Path) -> Bool {\n  target.display().starts_with(\n    \"/\", # the root\n  )\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(apply(&diagnostics, source), source);
    }
}
