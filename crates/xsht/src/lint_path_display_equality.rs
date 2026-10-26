use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaExprKind, AstArena, ExprId};
use xsh::frontend::syntax::node::BinaryOp;

/// `P.display() == "literal"` and `!=`, in either operand order, on a checked
/// Path are `P == "literal"`: beside a Path the literal is itself a Path.
///
/// The two agree unless the literal contains U+FFFD. Display text replaces
/// each byte sequence that is not UTF-8 with that character, so only such a
/// literal can equal the display of a path whose bytes differ from its own;
/// every other literal equals the display text exactly when it equals the
/// bytes. A literal with NUL is no Path at all. Both are left alone.
pub(super) fn path_display_equality(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
) -> Option<Diagnostic> {
    let comparison = arena.expr(expr);
    let ArenaExprKind::Binary {
        op: BinaryOp::Eq | BinaryOp::Ne,
        left,
        right,
    } = comparison.kind
    else {
        return None;
    };
    let (display, literal) = [(left, right), (right, left)]
        .into_iter()
        .find(|(_, literal)| matches!(arena.expr(*literal).kind, ArenaExprKind::Str(_)))?;
    let ArenaExprKind::Str(text) = arena.expr(literal).kind else {
        return None;
    };
    if arena.string_literal(text).contains(['\u{fffd}', '\0']) {
        return None;
    }
    let display = arena.expr(display);
    let ArenaExprKind::Call { callee, args } = display.kind else {
        return None;
    };
    let ArenaExprKind::Field { base: path, name } = arena.expr(callee).kind else {
        return None;
    };
    if name != "display" || !arena.call_args(args).is_empty() {
        return None;
    }
    let path = arena.expr(path).span;
    if expr_types.get(&path) != Some(&Type::Path) {
        return None;
    }
    let mut diagnostic =
        Diagnostic::warning("a Path is converted to text to compare it with a string literal")
            .with_code(DiagnosticCode::LintPathDisplayEquality)
            .with_label(Label::secondary(
                comparison.span,
                "beside a Path the literal is a Path, so the comparison needs no conversion",
            ));
    // Only the conversion is removed; the receiver and the literal stay as
    // written.
    let conversion = Span::new(path.source_id, path.end(), display.span.end());
    if source.get(conversion.range()) == Some(".display()") {
        diagnostic =
            diagnostic.with_fix_hint(FixHint::deletion(conversion, "compare the Path itself"));
    }
    Some(diagnostic)
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    // The lint runs from the linter's expression traversal, so the tests
    // drive the whole linter restricted to this code.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let options = LintOptions {
            expr_types: checked.expr_types,
            only: Some(vec![DiagnosticCode::LintPathDisplayEquality]),
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
                fix.replacement.as_deref().unwrap_or_default(),
            );
        }
        fixed
    }

    #[test]
    fn display_compared_with_a_literal_becomes_a_path_comparison() {
        let source = "pure special(target: Path, entry: FsEntry) -> Bool {\n  target.display() == \"/repo/x\" or \"etc\" != entry.path.display() or target.display() != \"\"\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "pure special(target: Path, entry: FsEntry) -> Bool {\n  target == \"/repo/x\" or \"etc\" != entry.path or target != \"\"\n}\n"
        );
        // The rewritten comparisons check: each literal is now a Path.
        assert!(lint(&fixed).is_empty());
    }

    // A replacement-character literal can equal the display of a path with
    // other bytes, a Str operand is a text comparison, and a name is not a
    // literal.
    #[test]
    fn comparisons_that_mean_text_are_left_alone() {
        let source = "pure special(target: Path, text: Str) -> Bool {\n  target.display() == \"bad\\u{fffd}name\" or target.display() == text or text == \"x\" or target.name() == \"x\"\n}\n";
        assert!(lint(source).is_empty());
    }

    // A string literal typed as a Path owes that to its annotation, so the
    // annotation is never reported as needless and removed.
    #[test]
    fn an_annotation_that_makes_a_literal_a_path_is_not_needless() {
        let source = "const roots: List[Path] = [\"/a\", \"/b\"]\nconst prefix: Path = \"/usr\"\n\nproc show() {\n  let bound: Path = \"a/b\"\n  let many: List[Path] = [\"x/1\", bound]\n  print $bound $prefix (many.len() + roots.len())\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let options = LintOptions {
            expr_types: checked.expr_types,
            only: Some(vec![DiagnosticCode::LintNeedlessAnnotation]),
            ..LintOptions::default()
        };
        let diagnostics = Linter::lint(&parsed.arena, source, options).diagnostics;
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}
