//! `lint.prefer-inferred-variant` for patterns: a qualified variant pattern
//! whose matched value already has that enum or error family type is the
//! same pattern as `.Name`. A match arm head is exempt, because a line that
//! begins with `.name` continues the line before it; the linter's pattern
//! traversal knows which patterns those are and does not report them.

use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;

/// The diagnostic for a pattern whose `qualifier` the matched type makes
/// redundant.
pub(super) fn redundant_pattern_qualifier(source: &str, qualifier: Span) -> Diagnostic {
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
    diagnostic
}
