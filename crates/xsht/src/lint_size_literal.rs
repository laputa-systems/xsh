//! `lint.prefer-size-literal`: a byte count written as a product of integer
//! literals and powers of 1024, such as `64 * 1024 * 1024`, is the size
//! literal `64MiB`.

use rustc_hash::{FxHashMap, FxHashSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaExprKind, AstArena, ExprId};
use xsh::frontend::syntax::node::BinaryOp;

/// The binary unit for a product with this many factors of 1024.
const BINARY_UNITS: [(&str, i64); 3] = [("KiB", 1 << 10), ("MiB", 1 << 20), ("GiB", 1 << 30)];

/// What the linter's traversal has seen above the products it has not reached
/// yet. A size literal is `UInt` where the product is `Int`, and the rewrite
/// cannot change what a program does where that type does not escape: under
/// an arithmetic or comparison operator, whose result is the same for both,
/// and in a binding whose type is written.
#[derive(Default)]
pub(super) struct SizeProducts {
    /// The operator directly above a product.
    operator_above: FxHashMap<ExprId, BinaryOp>,
    /// Products that initialize a binding with a written type.
    typed_initializers: FxHashSet<ExprId>,
}

impl SizeProducts {
    /// Records that `value` initializes a binding whose type is written.
    /// Called before the traversal reaches `value`.
    pub(super) fn typed_initializer(&mut self, arena: &AstArena, value: ExprId) {
        if is_product(arena, value) {
            self.typed_initializers.insert(value);
        }
    }

    /// Visits `id` before its operands, and reports it when it is a whole
    /// product of literals that spells a byte count.
    pub(super) fn visit(
        &mut self,
        arena: &AstArena,
        source: &str,
        id: ExprId,
    ) -> Option<Diagnostic> {
        let expr = arena.expr(id);
        let ArenaExprKind::Binary { op, left, right } = expr.kind else {
            return None;
        };
        for operand in [left, right] {
            if is_product(arena, operand) {
                self.operator_above.insert(operand, op);
            }
        }
        if op != BinaryOp::Mul {
            return None;
        }
        let above = self.operator_above.remove(&id);
        let typed_initializer = self.typed_initializers.remove(&id);
        // Only a whole product: `mb * 1024 * 1024` has a factor that is not
        // a literal, and its literal part is not a size on its own.
        if above == Some(BinaryOp::Mul) {
            return None;
        }
        let text = source.get(expr.span.range())?;
        let mut factors = Vec::new();
        if !literal_factors(arena, source, id, &mut factors) {
            return None;
        }
        let literal = size_literal(&factors)?;
        let diagnostic = Diagnostic::warning(format!(
            "a byte count written as a product is the size literal `{literal}`"
        ))
        .with_code(DiagnosticCode::LintPreferSizeLiteral)
        .with_label(Label::primary(expr.span, format!("write `{literal}`")));
        let type_stays_local = typed_initializer
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
        Some(if type_stays_local && !text.contains(['#', '\n']) {
            diagnostic.with_fix_hint(FixHint::replacement(
                with_grouping_parens(source, expr.span),
                "use the size literal",
                literal,
            ))
        } else {
            diagnostic.with_note(
                "a size literal is `UInt` and the product is `Int`; this value's type is inferred where it is used, so check that nothing negative is stored there before rewriting it",
            )
        })
    }
}

fn is_product(arena: &AstArena, id: ExprId) -> bool {
    matches!(
        arena.expr(id).kind,
        ArenaExprKind::Binary {
            op: BinaryOp::Mul,
            ..
        }
    )
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
