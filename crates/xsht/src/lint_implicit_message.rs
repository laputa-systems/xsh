//! `lint.prefer-implicit-message`: a variant declared `Variant(message: Str)`
//! restates the message every error carries. The migration is two edits that
//! each preserve behavior on their own: a call that names the field passes it
//! positionally (which binds the same sole field), and then the declaration
//! drops its payload (which keeps the `message` field, every positional
//! construction, and every pattern).

use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::Checker;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaStmtKind};

pub(super) fn lint_implicit_messages(program: &ArenaProgram, source: &str) -> Vec<Diagnostic> {
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
    let checked = Checker::check_arena(program, source);
    // Constructor facts of a program with check errors do not cover every call.
    if checked
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Vec::new();
    }
    let constructors = checked
        .message_payload_constructors
        .iter()
        .filter(|(call, _)| call.source_id == source_id)
        .map(|(_, constructor)| constructor)
        .collect::<Vec<_>>();

    let mut diagnostics = Vec::new();
    for constructor in &constructors {
        let Some((argument, value)) = constructor.named_message else {
            continue;
        };
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
            let named_calls = constructors.iter().any(|constructor| {
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
                // Calls in importing files are not visible here, and one that
                // names `message:` stops checking once the payload is gone.
                diagnostic = diagnostic.with_note(format!(
                    "`{}` is exported: once every `{}(message: ...)` call in the files that import it passes the message positionally, delete `(message: Str)` here",
                    family.name, variant.name
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
