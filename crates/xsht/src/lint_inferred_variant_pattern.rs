//! `lint.prefer-inferred-variant` for patterns: a qualified variant pattern
//! whose matched value already has that enum or error family type is the
//! same pattern as `.Name`. A match arm head is exempt, because a line that
//! begins with `.name` continues the line before it.

use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaExprKind, ArenaProgram, ExprId};

pub(super) fn lint_inferred_variant_patterns(
    program: &ArenaProgram,
    source: &str,
    redundant_qualifiers: &BTreeMap<Span, Span>,
) -> Vec<Diagnostic> {
    if redundant_qualifiers.is_empty() {
        return Vec::new();
    }
    let arena = &program.arena;
    let pattern_start = |pattern| {
        let span = arena.span(arena.pattern(pattern).span);
        (span.source_id, span.start())
    };
    // Pattern tests and conditions store their pattern as a one-arm match
    // expression; only the arms of a real `match` are arm heads.
    let mut arm_heads: FxHashSet<_> = arena
        .match_arms
        .iter()
        .map(|arm| pattern_start(arm.pattern))
        .collect();
    for index in 0..arena.expr_tags.len() {
        if let ArenaExprKind::Match { arms, .. } = arena.expr(ExprId::from_index(index)).kind {
            arm_heads.extend(
                arena
                    .match_expr_arms(arms)
                    .iter()
                    .map(|arm| pattern_start(arm.pattern)),
            );
        }
    }
    let mut reported = FxHashSet::default();
    let mut diagnostics = Vec::new();
    for pattern in &arena.patterns {
        let span = arena.span(pattern.span);
        let Some(qualifier) = redundant_qualifiers.get(&span).copied() else {
            continue;
        };
        if arm_heads.contains(&(qualifier.source_id, qualifier.start())) || !reported.insert(span)
        {
            continue;
        }
        let mut diagnostic =
            Diagnostic::warning("the matched value's type already selects this variant")
                .with_code(DiagnosticCode::LintPreferInferredVariant)
                .with_label(Label::secondary(
                    qualifier,
                    "qualifier the matched type makes redundant",
                ));
        // Only a qualifier written as plain dotted names, directly before the
        // variant's dot, is removed.
        let plain = source.get(qualifier.range()).is_some_and(|text| {
            text.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        }) && source[qualifier.end()..].starts_with('.');
        if plain {
            diagnostic = diagnostic.with_fix_hint(FixHint::deletion(
                qualifier,
                "select the variant from the matched type",
            ));
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}
