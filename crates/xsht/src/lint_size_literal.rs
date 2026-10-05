//! `lint.prefer-size-literal`: a byte count written as a product of integer
//! literals and powers of 1024, such as `64 * 1024 * 1024`, is the size
//! literal `64MiB`.

use rustc_hash::{FxHashMap, FxHashSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaExprKind, ArenaExprOrRun, ArenaProgram, ArenaStmtKind, AstArena, ExprId, StmtId,
};
use xsh::frontend::syntax::node::BinaryOp;

/// The binary unit for a product with this many factors of 1024.
const BINARY_UNITS: [(&str, i64); 3] = [("KiB", 1 << 10), ("MiB", 1 << 20), ("GiB", 1 << 30)];

pub(super) fn lint_size_products(program: &ArenaProgram, source: &str) -> Vec<Diagnostic> {
    let arena = &program.arena;
    // The arena can also hold modules parsed from other sources, whose spans
    // do not index `source`.
    let Some(source_id) = program
        .statement_ids()
        .next()
        .map(|id| arena.stmt(id).span.source_id)
    else {
        return Vec::new();
    };
    let expressions = || (0..arena.expr_tags.len()).map(ExprId::from_index);

    // A size literal is `UInt` where the product is `Int`. The rewrite cannot
    // change what a program does where that type does not escape: under an
    // arithmetic or comparison operator, whose result is the same for both,
    // and in a binding whose type is written.
    let mut operator_above: FxHashMap<ExprId, BinaryOp> = FxHashMap::default();
    for id in expressions() {
        if let ArenaExprKind::Binary { op, left, right } = arena.expr(id).kind {
            operator_above.insert(left, op);
            operator_above.insert(right, op);
        }
    }
    let mut typed_initializers: FxHashSet<ExprId> = FxHashSet::default();
    for statement in (0..arena.stmt_tags.len()).map(StmtId::from_index) {
        if let ArenaStmtKind::Let {
            ty: Some(_),
            initializer: ArenaExprOrRun::Expr(value),
            ..
        }
        | ArenaStmtKind::Const {
            ty: Some(_),
            initializer: ArenaExprOrRun::Expr(value),
            ..
        }
        | ArenaStmtKind::Var {
            ty: Some(_),
            initializer: ArenaExprOrRun::Expr(value),
            ..
        } = arena.stmt(statement).kind
        {
            typed_initializers.insert(value);
        }
    }

    let mut diagnostics = Vec::new();
    for id in expressions() {
        let expr = arena.expr(id);
        if !matches!(
            expr.kind,
            ArenaExprKind::Binary {
                op: BinaryOp::Mul,
                ..
            }
        ) {
            continue;
        }
        // Only a whole product: `mb * 1024 * 1024` has a factor that is not
        // a literal, and its literal part is not a size on its own.
        let above = operator_above.get(&id).copied();
        if above == Some(BinaryOp::Mul) || expr.span.source_id != source_id {
            continue;
        }
        let Some(text) = source.get(expr.span.range()) else {
            continue;
        };
        let mut factors = Vec::new();
        if !literal_factors(arena, source, id, &mut factors) {
            continue;
        }
        let Some(literal) = size_literal(&factors) else {
            continue;
        };
        let mut diagnostic = Diagnostic::warning(format!(
            "a byte count written as a product is the size literal `{literal}`"
        ))
        .with_code(DiagnosticCode::LintPreferSizeLiteral)
        .with_label(Label::primary(expr.span, format!("write `{literal}`")));
        let type_stays_local = typed_initializers.contains(&id)
            || above.is_some_and(|op| {
                matches!(
                    op,
                    BinaryOp::Add
                        | BinaryOp::Sub
                        | BinaryOp::Div
                        | BinaryOp::Rem
                        | BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                )
            });
        if type_stays_local && !text.contains(['#', '\n']) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                with_grouping_parens(source, expr.span),
                "use the size literal",
                literal,
            ));
        } else {
            diagnostic = diagnostic.with_note(
                "a size literal is `UInt` and the product is `Int`; this value's type is inferred where it is used, so check that nothing negative is stored there before rewriting it",
            );
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}

/// `span` widened over the parentheses that group it, if any. The product is
/// an operand or an initializer here, so a `(` directly before it and a `)`
/// directly after it group the product alone, and a literal needs no group.
fn with_grouping_parens(source: &str, span: Span) -> Span {
    let before = source[..span.start()].trim_end_matches([' ', '\t']);
    let after = source[span.end()..].trim_start_matches([' ', '\t']);
    if before.ends_with('(') && after.starts_with(')') {
        Span::new(
            span.source_id,
            before.len() - 1,
            source.len() - after.len() + 1,
        )
    } else {
        span
    }
}

/// Collects the factors of the product at `id` when every one is a plain
/// decimal integer literal.
fn literal_factors(arena: &AstArena, source: &str, id: ExprId, factors: &mut Vec<i64>) -> bool {
    let expr = arena.expr(id);
    match expr.kind {
        ArenaExprKind::Binary {
            op: BinaryOp::Mul,
            left,
            right,
        } => {
            literal_factors(arena, source, left, factors)
                && literal_factors(arena, source, right, factors)
        }
        ArenaExprKind::Int(literal) => {
            let decimal = source
                .get(expr.span.range())
                .is_some_and(|text| text.bytes().all(|byte| byte.is_ascii_digit()));
            match arena.int_literal(literal).value() {
                Some(value) if decimal => {
                    factors.push(value);
                    true
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// The size literal for a product of `factors`: the factors of 1024 name the
/// unit, and at most one other factor counts it. A product with no factor of
/// 1024, or with several other factors, is not recognizably a byte count.
fn size_literal(factors: &[i64]) -> Option<String> {
    let powers = factors.iter().filter(|factor| **factor == 1024).count();
    let mut counts = factors.iter().filter(|factor| **factor != 1024);
    let count = counts.next().copied().unwrap_or(1);
    if powers == 0 || counts.next().is_some() || count < 1 {
        return None;
    }
    let (unit, bytes) = BINARY_UNITS[powers.min(BINARY_UNITS.len()) - 1];
    let total = factors
        .iter()
        .try_fold(1i64, |total, factor| total.checked_mul(*factor))?;
    Some(format!("{}{unit}", total / bytes))
}

#[cfg(test)]
#[path = "lint_size_literal_tests.rs"]
mod tests;
