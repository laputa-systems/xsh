//! `lint.prefer-implicit-message`: a variant declared `Variant(message: Str)`
//! restates the message every error carries. The migration is two edits that
//! each preserve behavior on their own: a call that names the field passes it
//! positionally (which binds the same sole field), and then the declaration
//! drops its payload (which keeps the `message` field, every positional
//! construction, and every pattern).
//!
//! The one thing the second edit takes away is the named call: a variant
//! without a payload takes no named argument. Calls in the linted file are
//! counted, so a private family is fixed here. An exported family can be
//! constructed by name in a file this lint does not see, so its declaration
//! is reported without a fix.
//!
//! A declaration deleted by a safe `lint.prefer-fail` fix needs no payload
//! edit. Findings without a fix still permit message edits that retain the family.

use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::MessagePayloadConstructor;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaStmtKind};

/// `constructors` are the checked constructor calls of the program the linted
/// file was checked in, and `reported` the diagnostics the other rules have
/// produced for the file.
pub(super) fn lint_implicit_messages(
    program: &ArenaProgram,
    source: &str,
    constructors: &BTreeMap<Span, MessagePayloadConstructor>,
    reported: &[Diagnostic],
) -> Vec<Diagnostic> {
    // Every reported site spells the field name, in a declaration or a call.
    if !source.contains("message") {
        return Vec::new();
    }
    let Some(source_id) = program
        .statement_ids()
        .next()
        .map(|id| program.arena.stmt(id).span.source_id)
    else {
        return Vec::new();
    };
    // A file the linted one imports can declare a family of the same name.
    let constructors = constructors
        .iter()
        .filter(|(call, _)| call.source_id == source_id)
        .collect::<Vec<_>>();
    // Only an offered fix supersedes a message edit at the same site.
    let fail_sites = reported
        .iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferFail)
            && !diagnostic.fix_hints.is_empty())
        .filter_map(|diagnostic| diagnostic.labels.first().map(|label| label.span))
        .collect::<FxHashSet<_>>();

    let mut diagnostics = Vec::new();
    for (call, constructor) in &constructors {
        let Some((argument, value)) = constructor.named_message else {
            continue;
        };
        if fail_sites.contains(*call) {
            continue;
        }
        let mut diagnostic = Diagnostic::warning(format!(
            "`{}` takes its message positionally",
            constructor.variant
        ))
        .with_code(DiagnosticCode::LintPreferImplicitMessage)
        .with_label(Label::secondary(
            argument,
            "`message` is this variant's only field",
        ));
        // A pun (`message:`) ends after its value; anything else between the
        // label and the value, such as a comment, stays as written.
        if let (Some(lead), Some(text), Some(tail)) = (
            source.get(argument.start()..value.start()),
            source.get(value.range()),
            source.get(value.end()..argument.end()),
        ) && !lead.contains('#')
            && matches!(tail.trim(), "" | ":")
        {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                argument,
                "pass the message positionally",
                text,
            ));
        }
        diagnostics.push(diagnostic);
    }

    for statement in program.statement_ids() {
        let outer = program.arena.stmt(statement);
        let (inner, exported) = match outer.kind {
            ArenaStmtKind::Export(inner) => (program.arena.stmt(inner), true),
            _ => (outer.clone(), false),
        };
        let ArenaStmtKind::ErrorDef(id) = inner.kind else {
            continue;
        };
        let family = program.arena.error_def(id);
        if !exported
            && (fail_sites.contains(&outer.span)
                || constructors.iter().any(|(call, constructor)| {
                    constructor.family == family.name && fail_sites.contains(*call)
                }))
        {
            continue;
        }
        for variant in program.arena.error_variants(family.variants) {
            let [field] = program.arena.error_fields(variant.fields) else {
                continue;
            };
            let field_type = program.arena.type_expr_span(field.ty);
            if field.name != "message" || source.get(field_type.range()) != Some("Str") {
                continue;
            }
            let variant_span = program.arena.span(variant.span);
            let name_end = variant_span.start() + variant.name.as_str().len();
            let Some(payload_end) = source
                .get(field_type.end()..variant_span.end())
                .and_then(|rest| rest.find(')'))
                .map(|offset| field_type.end() + offset + 1)
            else {
                continue;
            };
            let payload = Span::new(source_id, name_end, payload_end);
            let named_calls = constructors.iter().any(|(_, constructor)| {
                constructor.family == family.name
                    && constructor.variant == variant.name
                    && constructor.named_message.is_some()
            });
            let mut diagnostic = Diagnostic::warning(format!(
                "variant `{}` restates the message every error carries",
                variant.name
            ))
            .with_code(DiagnosticCode::LintPreferImplicitMessage)
            .with_label(Label::secondary(
                payload,
                format!(
                    "declare `{}` without a payload; its constructor takes the message positionally",
                    variant.name
                ),
            ));
            if exported {
                diagnostic = diagnostic.with_note(format!(
                    "`{family}` is exported, and a `{variant}(message: ...)` call in a file that imports it stops checking once the payload is gone (`check.error-constructor`); positional calls, patterns, and `.message` are unchanged. Run this rule over the importing files, which passes each message positionally, then delete `(message: Str)` here",
                    family = family.name,
                    variant = variant.name
                ));
            } else if named_calls {
                diagnostic = diagnostic.with_note(format!(
                    "first pass the message positionally at each `{}(message: ...)` call in this file",
                    variant.name
                ));
            } else if source
                .get(payload.range())
                .is_some_and(|text| !text.contains('#'))
            {
                diagnostic = diagnostic.with_fix_hint(FixHint::deletion(
                    payload,
                    "declare the variant without a payload",
                ));
            }
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::DiagnosticCode;
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    #[test]
    fn a_nominal_error_finding_keeps_safe_message_fixes_available() {
        let source = "error E = Failed(message: Str)\n\nproc load() -> Result[Int] {\n  return Err(E.Failed(message: \"no\"))\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let found = Linter::lint(&parsed.arena, source, LintOptions {
            message_payload_constructors: Some(checked.message_payload_constructors),
            ..LintOptions::default()
        }).diagnostics;
        let nominal = found.iter().filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferFail)).collect::<Vec<_>>();
        assert_eq!(nominal.len(), 1);
        assert!(nominal[0].fix_hints.is_empty());
        let messages = found.iter().filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferImplicitMessage)).collect::<Vec<_>>();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages.iter().flat_map(|diagnostic| &diagnostic.fix_hints).count(), 1);
    }

    #[test]
    fn a_deleted_unused_family_needs_no_payload_fix() {
        let source = "error E = Failed(message: Str)\n\nprint \"done\"\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(parsed.diagnostics.is_empty() && checked.diagnostics.is_empty());
        let found = Linter::lint(&parsed.arena, source, LintOptions {
            message_payload_constructors: Some(checked.message_payload_constructors),
            ..LintOptions::default()
        }).diagnostics;
        assert!(found.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferFail) && diagnostic.fix_hints.len() == 1));
        assert!(found.iter().all(|diagnostic| diagnostic.code != Some(DiagnosticCode::LintPreferImplicitMessage)));
    }
}
