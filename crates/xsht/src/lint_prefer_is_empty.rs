use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaExprKind, AstArena, ExprId};
use xsh::frontend::syntax::node::BinaryOp;

/// A length compared with zero only asks whether the value is empty:
/// `items.len() == 0` is `items.is_empty()`, and `items.len() > 0` and
/// `items.len() != 0` are `! items.is_empty()`, as are the mirrored
/// `0 == items.len()`, `0 != items.len()`, and `0 < items.len()`.
///
/// The length is `len()` of a `Bytes`, `List`, `Map`, or `Set`, or `byte_len()` or
/// `count_chars()` of a `Str`, each of which is zero exactly when the value
/// is empty. The receiver is evaluated once either way, and a negation binds
/// tighter than any operator the comparison could have been an operand of, so
/// the rewrite needs no parentheses of its own. A comparison with a comment
/// inside is reported without a fix.
pub(super) fn length_compared_with_zero(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
) -> Option<Diagnostic> {
    let node = arena.expr(expr);
    let ArenaExprKind::Binary { op, left, right } = node.kind else {
        return None;
    };
    let is_zero = |operand: ExprId| {
        matches!(arena.expr(operand).kind, ArenaExprKind::Int(_))
            && source.get(arena.expr(operand).span.range()) == Some("0")
    };
    // Whether the comparison holds for an empty value, with the length on
    // the left or on the right of the zero.
    let (length, empty) = match op {
        BinaryOp::Eq if is_zero(right) => (left, true),
        BinaryOp::Eq if is_zero(left) => (right, true),
        BinaryOp::Ne if is_zero(right) => (left, false),
        BinaryOp::Ne if is_zero(left) => (right, false),
        BinaryOp::Gt if is_zero(right) => (left, false),
        BinaryOp::Lt if is_zero(left) => (right, false),
        _ => return None,
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(length).kind else {
        return None;
    };
    if !arena.call_args(args).is_empty() {
        return None;
    }
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    let name = name.as_str();
    let name = name.as_str();
    let counts_elements = match expr_types.get(&arena.expr(base).span)? {
        Type::Bytes | Type::List(_) | Type::Map(_, _) | Type::Set(_) => name == "len",
        Type::Str => matches!(name, "byte_len" | "count_chars"),
        _ => false,
    };
    if !counts_elements {
        return None;
    }
    let spelling = if empty {
        "`.is_empty()`"
    } else {
        "`! ... .is_empty()`"
    };
    let diagnostic = Diagnostic::new(
        Severity::Warning,
        "prefer `is_empty()` over comparing a length with zero",
    )
    .with_code(DiagnosticCode::LintPreferIsEmpty)
    .with_label(Label::secondary(
        node.span,
        format!("this only asks whether the value is empty; write {spelling}"),
    ));
    // The call is rewritten in place: its receiver text stays, and its
    // method name and empty argument list become `is_empty()`.
    let call = source.get(arena.expr(length).span.range())?;
    let Some(receiver) = call.strip_suffix(&format!("{name}()")) else {
        return Some(diagnostic);
    };
    if super::span_may_contain_comment(source, node.span) {
        return Some(diagnostic);
    }
    let test = |empty: bool| {
        let negation = if empty { "" } else { "! " };
        format!("{negation}{receiver}is_empty()")
    };
    // A comparison needs grouping where an emptiness test does not, so the
    // parentheses around one are rewritten with it. Under a `!` the two
    // negations cancel; elsewhere the group is kept in the replacement, and
    // dropped from it when the result no longer needs one.
    let grouped = source[..node.span.start()].trim_end().ends_with('(')
        && source[node.span.end()..].trim_start().starts_with(')');
    let group = if grouped {
        super::widen_over_grouping(source, node.span)
    } else {
        node.span
    };
    let (span, replacement) = if group == node.span {
        (node.span, test(empty))
    } else if let Some(not) = source[..group.start()].strip_suffix('!') {
        (
            Span::new(group.source_id, not.len(), group.end()),
            test(!empty),
        )
    } else {
        (group, format!("({})", test(empty)))
    };
    Some(diagnostic.with_fix_hint(FixHint::replacement(
        span,
        "test emptiness directly",
        replacement,
    )))
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn length_tests(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        )
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferIsEmpty))
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
    fn every_zero_comparison_of_a_length_becomes_an_emptiness_test() {
        let source = "pure probe(text: Str, raw: Bytes, items: List[Int], table: Map[Str, Int]) -> Bool {\n  let a = items.len() == 0\n  let b = 0 == raw.len()\n  let c = table.len() != 0 and items.len() > 0\n  let d = 0 < text.byte_len() or 0 != text.count_chars()\n  let e = !(text.trim().split(\",\").len() > 0)\n  a and b and c and d and e\n}\n";
        let diagnostics = length_tests(source);
        assert_eq!(diagnostics.len(), 7, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "pure probe(text: Str, raw: Bytes, items: List[Int], table: Map[Str, Int]) -> Bool {\n  let a = items.is_empty()\n  let b = raw.is_empty()\n  let c = ! table.is_empty() and ! items.is_empty()\n  let d = ! text.is_empty() or ! text.is_empty()\n  let e = text.trim().split(\",\").is_empty()\n  a and b and c and d and e\n}\n"
        );
        // The fixed program checks and is not reported again.
        assert!(length_tests(&fixed).is_empty());
    }

    #[test]
    fn a_set_length_compared_with_zero_becomes_an_emptiness_test() {
        let source = "pure probe(tags: Set[Str]) -> Bool {\n  let a = tags.len() == 0\n  let b = 0 < (tags | {\"x\",}).len()\n  a and b\n}\n";
        let diagnostics = length_tests(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "pure probe(tags: Set[Str]) -> Bool {\n  let a = tags.is_empty()\n  let b = ! (tags | {\"x\",}).is_empty()\n  a and b\n}\n"
        );
        assert!(length_tests(&fixed).is_empty());
    }

    /// A length compared with anything but zero, a count that is not a
    /// length, and arithmetic on a length are left alone.
    #[test]
    fn other_comparisons_are_left_alone() {
        let source = "pure probe(text: Str, items: List[Int]) -> Bool {\n  let a = items.len() == 1\n  let b = items.len() >= 0\n  let c = text.count_lines() == 0\n  let d = items.len() - 1 == 0\n  let e = items.len() < 0\n  a and b and c and d and e\n}\n";
        assert!(length_tests(source).is_empty());
    }

    #[test]
    fn a_comment_inside_the_comparison_leaves_the_rewrite_to_the_author() {
        let source = "pure probe(items: List[Int]) -> Bool {\n  let empty = items.push(\n    1, # one\n  ).len() == 0\n  empty\n}\n";
        let diagnostics = length_tests(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
    }
}
