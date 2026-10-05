//! `value as TARGET`: the conversion table.
//!
//! A conversion is never a new operation. Each pair of types the table
//! accepts names an operation the language already has, one that returns a
//! `Result`, and the expression is that operation under `?`. A pair the table
//! does not accept is a check error, so no conversion can lose information
//! without saying so.

use super::Checker;
use crate::diagnostic::{Diagnostic, DiagnosticCode, Label};
use crate::sema::types::Type;
use crate::source::Span;
use crate::syntax::arena::{ArenaProgram, ExprId, TypeExprId};

/// The operation one accepted conversion performs. The checker selects it
/// from the operand's type and the target type; lowering builds the operation
/// from this value alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Conversion {
    /// `Str as Int`: `text.parse_int()`.
    TextToInt,
    /// `Str as UInt`: `text.parse_uint()`.
    TextToUInt,
    /// `Str as Float`: `text.parse_float()`.
    TextToFloat,
    /// `Str as Path`: `Path.parse_bytes(bytes.from_text(text))`, which
    /// rejects NUL as a failure the caller can handle.
    TextToPath,
    /// `Bytes as Str`: `bytes.utf8()`.
    BytesToText,
    /// `Bytes as Path`: `Path.parse_bytes(bytes)`.
    BytesToPath,
    /// `Int as UInt`: `value.require(UInt)`.
    IntToUInt,
}

impl Conversion {
    /// Every accepted pair, in the order the language reference lists them.
    /// A validated type joins the table by adding a row here and an arm
    /// where lowering builds the operation.
    pub const TABLE: [(Type, Type, Conversion); 7] = [
        (Type::Str, Type::Int, Conversion::TextToInt),
        (Type::Str, Type::UInt, Conversion::TextToUInt),
        (Type::Str, Type::Float, Conversion::TextToFloat),
        (Type::Str, Type::Path, Conversion::TextToPath),
        (Type::Bytes, Type::Str, Conversion::BytesToText),
        (Type::Bytes, Type::Path, Conversion::BytesToPath),
        (Type::Int, Type::UInt, Conversion::IntToUInt),
    ];

    /// The conversion from exactly `from` to exactly `to`.
    pub fn select(from: &Type, to: &Type) -> Option<Self> {
        Self::TABLE
            .iter()
            .find(|(source, target, _)| source == from && target == to)
            .map(|(_, _, conversion)| *conversion)
    }

    /// The type of the converted value.
    pub fn target(self) -> Type {
        Self::TABLE
            .iter()
            .find(|(_, _, conversion)| *conversion == self)
            .map(|(_, target, _)| target.clone())
            .expect("every conversion has a table row")
    }
}

/// What to write instead of a pair the table rejects, when the language has
/// a spelling for it.
fn rejected_pair_note(from: &Type, to: &Type) -> Option<&'static str> {
    Some(match (from, to) {
        _ if from == to => "the value already has this type",
        (Type::UInt, Type::Int) => "a `UInt` is already an `Int` wherever one is expected",
        (Type::Float, Type::Int | Type::UInt) => {
            "choose the rounding: `.floor()`, `.ceil()`, or `.round()` return a `Result[Int]`"
        }
        (Type::Int | Type::UInt, Type::Float) => {
            "use `.float()`, which is exact only up to 9007199254740992"
        }
        (Type::Path, Type::Str) => {
            "use `.display()`, which replaces bytes that are not UTF-8, or `.bytes().utf8()` to fail on them"
        }
        (Type::Str, Type::Bytes) => "use `bytes.from_text(text)`, which cannot fail",
        (Type::Path, Type::Bytes) => "use `.bytes()`, which cannot fail",
        (Type::Any, _) => "validate a dynamic value with `.require(TYPE)`",
        (Type::Optional(_), _) => "handle `null` first, with `??`, `if let`, or `guard let`",
        (Type::Result(_, _), _) => "propagate or handle the `Result` first",
        (_, Type::Optional(_)) => {
            "`T?` is the optional type; a conversion already propagates its failure, so write the plain type"
        }
        (_, Type::Str) => "format the value with an f-string",
        _ => return None,
    })
}

impl Checker {
    /// Checks `value as TARGET`: selects the conversion for the pair of
    /// types, publishes it for lowering, and propagates its failure exactly
    /// as `?` on the operation would.
    pub(super) fn check_conversion_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        target: TypeExprId,
        span: Span,
    ) -> Type {
        self.conversions.remove(&span);
        let from = self.check_expr_arena(arena, source, value, None);
        let to = self.type_from_arena(arena, target);
        if matches!(from, Type::Unknown | Type::Invalid) || matches!(to, Type::Unknown | Type::Invalid)
        {
            return to;
        }
        let Some(conversion) = Conversion::select(&from, &to) else {
            let mut diagnostic = Diagnostic::error(format!("no conversion from {from} to {to}"))
                .with_code(DiagnosticCode::CheckConversion)
                .with_label(Label::primary(
                    span,
                    format!("`as` does not convert {from} to {to}"),
                ));
            if let Some(note) = rejected_pair_note(&from, &to) {
                diagnostic = diagnostic.with_note(note);
            }
            self.diagnostics.push(diagnostic);
            return Type::Invalid;
        };
        self.conversions.insert(span, conversion);
        let reported = self.diagnostics.len();
        let result = Type::Result(Box::new(conversion.target()), Box::new(Type::Error));
        let ty = self.check_propagation(&result, span);
        // The propagation rules are worded for `?`; say which form this is.
        for diagnostic in &mut self.diagnostics[reported..] {
            *diagnostic = diagnostic.clone().with_note(
                "`as` fails the enclosing function when the conversion fails, as `?` does",
            );
        }
        ty
    }
}
