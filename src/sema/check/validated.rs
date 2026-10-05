#![allow(clippy::single_call_fn)]

use super::{Checker, Diagnostic, Label, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::validated::{ValidatedType, Validation};
use crate::syntax::arena::{ArenaListElementRange, ArenaProgram};

/// Validated types get their values in three places only: a literal judged
/// here, the explicit conversion (`.require(T)`, a type test, or a type
/// pattern), and the operations the validation's registry receiver lists.
/// Every other operation reads its operand through `Type::unvalidated`.
impl Checker {
    /// The type of a list literal written where `expected` is wanted. The
    /// literal was checked against the base and is `actual`; whether it
    /// passes the validation is decided here, from the literal alone.
    pub(super) fn validated_list_literal(
        &mut self,
        arena: &ArenaProgram,
        expected: &ValidatedType,
        range: ArenaListElementRange,
        actual: Type,
        span: Span,
    ) -> Type {
        let passes = match expected.validation() {
            // An element written out is there whatever the splices hold, and
            // a splice of a non-empty list contributes at least one.
            Validation::NonEmpty => arena.arena.list_elements(range).any(|item| {
                item.splice_span.is_none()
                    || self
                        .expr_types
                        .get(&arena.arena.expr(item.value).span)
                        .and_then(Type::validated)
                        .is_some_and(|spread| spread.validation().implies(Validation::NonEmpty))
            }),
        };
        let Ok(validated) = ValidatedType::new(expected.validation(), actual) else {
            return Type::Invalid;
        };
        let ty = Type::Validated(Box::new(validated));
        if !passes {
            let message = if range.is_empty() {
                format!("an empty list literal is not a {ty}")
            } else {
                format!("this list literal may be empty, so it is not a {ty}")
            };
            self.diagnostics.push(
                Diagnostic::error(message)
                    .with_code(DiagnosticCode::CheckValidatedLiteral)
                    .with_label(Label::primary(
                        span,
                        "write at least one element outside an `@` splice",
                    )),
            );
        }
        // The literal keeps the expected type after a failure, so the
        // binding it initializes is not reported a second time.
        ty
    }

    /// `left + right` keeps a validation that survives concatenation when
    /// either operand carries it.
    pub(super) fn concatenation_type(
        operands: [Option<Validation>; 2],
        concatenated: Type,
    ) -> Type {
        let kept = operands
            .into_iter()
            .flatten()
            .find(|validation| validation.survives_concatenation());
        match kept.map(|validation| ValidatedType::new(validation, concatenated.clone())) {
            Some(Ok(validated)) => Type::Validated(Box::new(validated)),
            _ => concatenated,
        }
    }

    /// For a mismatch between a validated type and a value of its base that
    /// has not been validated: how the value is validated.
    pub(super) fn validated_mismatch_note(expected: &Type, actual: &Type) -> Option<String> {
        let expected = expected.optional_inner().unwrap_or(expected);
        let wanted = expected.validated()?;
        (actual.validated().is_none()
            && !actual.is_recovery()
            && actual.matches_expected(wanted.base()))
        .then(|| wanted.validation().conversion_note(expected))
    }
}
