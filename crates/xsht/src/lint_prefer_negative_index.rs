use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaExprKind, AstArena, ExprId};
use xsh::frontend::syntax::node::BinaryOp;

/// `items[items.len() - N]` with a literal `N` of one or more reads the item
/// `N` from the end, which `items[-N]` says directly.
///
/// The two are the same read. The list is a binding or a field path, so
/// naming it once instead of twice yields the same value and has no effect,
/// and a list shorter than `N` fails with `index-out-of-range` either way:
/// the subtraction gives a negative index, which is never in range. Only a
/// read is rewritten; an assignment target does not count from the end.
pub(super) fn length_minus_literal_index(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
) -> Option<Diagnostic> {
    let node = arena.expr(expr);
    let ArenaExprKind::Index {
        base,
        index,
        guarded: false,
    } = node.kind
    else {
        return None;
    };
    let ArenaExprKind::Binary {
        op: BinaryOp::Sub,
        left,
        right,
    } = arena.expr(index).kind
    else {
        return None;
    };
    let ArenaExprKind::Int(literal) = arena.expr(right).kind else {
        return None;
    };
    let distance = arena.int_literal(literal).value()?;
    let distance_text = source.get(arena.expr(right).span.range())?;
    if distance < 1
        || u32::try_from(distance).is_err()
        || !distance_text.chars().all(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    let ArenaExprKind::Call { callee, args } = arena.expr(left).kind else {
        return None;
    };
    if !arena.call_args(args).is_empty() {
        return None;
    }
    let ArenaExprKind::Field {
        base: measured,
        name,
    } = arena.expr(callee).kind
    else {
        return None;
    };
    if name != "len"
        || !is_name_path(arena, base)
        || !same_name_path(arena, base, measured)
        || !matches!(expr_types.get(&arena.expr(base).span), Some(Type::List(_)))
    {
        return None;
    }
    let index_span = arena.expr(index).span;
    let diagnostic = Diagnostic::new(
        Severity::Warning,
        "prefer a negative index for an item counted from the end",
    )
    .with_code(DiagnosticCode::LintPreferNegativeIndex)
    .with_label(Label::secondary(
        index_span,
        format!("this is the item {distance} from the end; write `-{distance}`"),
    ));
    if super::span_may_contain_comment(source, index_span) {
        return Some(diagnostic);
    }
    Some(diagnostic.with_fix_hint(FixHint::replacement(
        index_span,
        "count from the end",
        format!("-{distance_text}"),
    )))
}

/// A binding or a path of fields from one: reading it has no effect.
fn is_name_path(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_) => true,
        ArenaExprKind::Field { base, .. } => is_name_path(arena, base),
        _ => false,
    }
}

fn same_name_path(arena: &AstArena, left: ExprId, right: ExprId) -> bool {
    match (arena.expr(left).kind, arena.expr(right).kind) {
        (ArenaExprKind::Ident(left), ArenaExprKind::Ident(right)) => left == right,
        (
            ArenaExprKind::Field {
                base: left_base,
                name: left_name,
            },
            ArenaExprKind::Field {
                base: right_base,
                name: right_name,
            },
        ) => left_name == right_name && same_name_path(arena, left_base, right_base),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn end_indexes(source: &str) -> Vec<Diagnostic> {
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
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferNegativeIndex))
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
    fn a_length_minus_a_literal_becomes_a_negative_index() {
        let source = "type Table = {rows: List[List[Int]]}\n\npure probe(items: List[Int], table: Table) -> Int {\n  let last = items[items.len() - 1]\n  let row = table.rows[table.rows.len() - 2]\n  last + row[row.len() - 1]\n}\n";
        let diagnostics = end_indexes(source);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "type Table = {rows: List[List[Int]]}\n\npure probe(items: List[Int], table: Table) -> Int {\n  let last = items[-1]\n  let row = table.rows[-2]\n  last + row[-1]\n}\n"
        );
        assert!(end_indexes(&fixed).is_empty());
    }

    /// Another list's length, a computed distance, a distance of zero, a
    /// receiver that is evaluated, and an assignment target are left alone.
    #[test]
    fn other_indexes_are_left_alone() {
        let source = "pure rest(items: List[Int]) -> List[Int] {\n  items[1..]\n}\n\npure probe(items: List[Int], other: List[Int], step: Int) -> Int {\n  var copy = items\n  copy[copy.len() - 1] = 0\n  let a = items[other.len() - 1]\n  let b = items[items.len() - step]\n  let c = items[items.len() - 0]\n  let d = rest(items)[rest(items).len() - 1]\n  let e = items[items.len() - 1 - step]\n  a + b + c + d + e + copy.len()\n}\n";
        assert!(end_indexes(source).is_empty());
    }
}
