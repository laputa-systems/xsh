#![allow(clippy::single_call_fn)]

use super::{Checker, Diagnostic, Label, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::validated::{ValidatedType, Validation, rel_path_failure};
use crate::syntax::arena::{ArenaFmtPart, ArenaListElementRange, ArenaProgram, ArenaRange};

const REL_PATH_RULE: &str =
    "a RelPath is not empty, does not start with `/`, and has no `..` that leaves where it starts";

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
            // No list type carries this validation.
            Validation::RelPath => false,
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

    /// The type of a path literal without interpolation written where
    /// `expected` is wanted: its bytes are the author's, so the validation
    /// is decided here.
    pub(super) fn validated_static_path_literal(
        &mut self,
        expected: &ValidatedType,
        bytes: &[u8],
        span: Span,
    ) -> Type {
        let failure = match expected.validation() {
            Validation::RelPath => rel_path_failure(bytes),
            Validation::NonEmpty => return Type::Invalid,
        };
        let ty = Type::Validated(Box::new(expected.clone()));
        if let Some(failure) = failure {
            self.diagnostics.push(
                Diagnostic::error(format!("this path {failure}, so it is not a {ty}"))
                    .with_code(DiagnosticCode::CheckValidatedLiteral)
                    .with_label(Label::primary(span, REL_PATH_RULE)),
            );
        }
        ty
    }

    /// The type of an interpolating path literal written where `expected` is
    /// wanted. The written text is judged as a static literal is, with each
    /// interpolation standing for a run of whole components that ends no
    /// higher than it starts. That holds for an interpolated value of the
    /// same validated type set off by `/` on both sides, and for nothing
    /// else: text could hold any bytes, and a value glued to its neighbour
    /// forms a component neither of them wrote.
    pub(super) fn validated_interpolated_path_literal(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        expected: &ValidatedType,
        parts: ArenaRange,
        span: Span,
    ) -> Type {
        let Validation::RelPath = expected.validation() else {
            return Type::Invalid;
        };
        let ty = Type::Validated(Box::new(expected.clone()));
        let parts = arena.arena.fmt_parts(parts).collect::<Vec<_>>();
        // The text the literal would be if every interpolation were `.`, the
        // component that leaves the depth where it is.
        let mut written = Vec::new();
        let mut problem = None;
        for (index, part) in parts.iter().enumerate() {
            match part {
                ArenaFmtPart::Text(text) => match arena.arena.text_value(text, source) {
                    Some(text) => written.extend_from_slice(text.as_bytes()),
                    // The text of a literal checked without its own source
                    // cannot be judged; the literal stays a plain path.
                    None => return Type::Path,
                },
                ArenaFmtPart::Expr(value, spec) => {
                    let value_span = arena.arena.expr(*value).span;
                    let value_ty = self.expr_types.get(&value_span).cloned();
                    // An interpolation that failed to check has its own
                    // diagnostic; one whose type is merely unknown is not
                    // known to be confined, and is reported below.
                    if value_ty == Some(Type::Invalid) {
                        return ty;
                    }
                    let confined = value_ty
                        .as_ref()
                        .and_then(Type::validated)
                        .is_some_and(|value| value.validation().implies(Validation::RelPath));
                    let next_starts_component = match parts.get(index + 1) {
                        None => true,
                        Some(ArenaFmtPart::Text(text)) => arena
                            .arena
                            .text_value(text, source)
                            .is_none_or(|text| text.starts_with('/')),
                        Some(ArenaFmtPart::Expr(..)) => false,
                    };
                    if problem.is_none() {
                        if !confined || spec.is_some() {
                            let found = value_ty
                                .map_or_else(|| "a value".to_string(), |ty| format!("a {ty}"));
                            problem = Some((
                                value_span,
                                format!("this path interpolates {found}, so it is not a {ty}"),
                                format!("only an unformatted {ty} keeps an interpolating path a {ty}"),
                            ));
                        } else if !(written.last().is_none_or(|byte| *byte == b'/')
                            && next_starts_component)
                        {
                            problem = Some((
                                value_span,
                                format!("this interpolation is not set off by `/`, so the path is not a {ty}"),
                                "write `/` between an interpolated path and its neighbours".to_string(),
                            ));
                        }
                    }
                    written.push(b'.');
                }
            }
        }
        let problem = problem.or_else(|| {
            rel_path_failure(&written).map(|failure| {
                (
                    span,
                    format!("this path {failure}, so it is not a {ty}"),
                    REL_PATH_RULE.to_string(),
                )
            })
        });
        if let Some((at, message, label)) = problem {
            self.diagnostics.push(
                Diagnostic::error(message)
                    .with_code(DiagnosticCode::CheckValidatedLiteral)
                    .with_label(Label::primary(at, label))
                    .with_note(Validation::RelPath.conversion_note(&ty)),
            );
        }
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
