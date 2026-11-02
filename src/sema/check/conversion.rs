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
    /// An `Int` or `UInt` as a bounded integer type: `value.require(T)`.
    /// The target is the type the conversion names, so it has no row in the
    /// table of fixed pairs.
    IntToBounded,
    /// `value as Path` where the operand is a generic parameter that must be
    /// `Str` or `Bytes`: the callable is shared by every instantiation, so
    /// the value, not the checker, picks `TextToPath` or `BytesToPath`.
    SourceToPath,
}

#[derive(Clone)]
pub(super) struct PendingConversion {
    owner: Option<super::EffectDeclarationId>,
    from: Type,
    to: Type,
    span: Span,
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
        if to.int_range().is_some() && matches!(from, Type::Int | Type::UInt) {
            return Some(Self::IntToBounded);
        }
        Self::TABLE
            .iter()
            .find(|(source, target, _)| source == from && target == to)
            .map(|(_, _, conversion)| *conversion)
    }

    /// The type of the converted value, for a conversion between a fixed
    /// pair of types. A conversion to a bounded integer yields the type it
    /// names and has no target of its own.
    pub fn target(self) -> Option<Type> {
        if self == Self::SourceToPath { return Some(Type::Path); }
        Self::TABLE
            .iter()
            .find(|(_, _, conversion)| *conversion == self)
            .map(|(_, target, _)| target.clone())
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
    fn publish_conversion(&mut self, from: &Type, to: &Type, span: Span) -> bool {
        let from = from.clone().into_unvalidated();
        let Some(conversion) = Conversion::select(&from, to) else {
            let mut diagnostic = Diagnostic::error(format!("no conversion from {from} to {to}"))
                .with_code(DiagnosticCode::CheckConversion)
                .with_label(Label::primary(span, format!("`as` does not convert {from} to {to}")));
            if let Some(note) = rejected_pair_note(&from, to) { diagnostic = diagnostic.with_note(note); }
            self.diagnostics.push(diagnostic);
            return false;
        };
        self.conversions.insert(span, conversion);
        true
    }

    pub(super) fn finish_pending_conversions(&mut self, arena: &ArenaProgram, start: usize, body: Span) {
        let pending = self.local_inference.conversions.drain(start..).collect::<Vec<_>>();
        if pending.is_empty() { return; }
        if let Err(error) = self.type_constraints.solve_callable_applications() {
            self.error(body, &format!("conversion constraints failed: {error}"), DiagnosticCode::CheckLocalInference);
        }
        let indexed_iterations = match self.type_constraints.indexed_iteration_evidence() {
            Ok(evidence) => evidence,
            Err(error) => { self.reject_host_inference_resolution(error, body); return; }
        };
        for conversion in &pending {
            assert_eq!(conversion.owner, self.stream_origin_owner);
            let from = match self.type_constraints.resolve(&conversion.from) {
                Ok(from) => from,
                Err(error) => { self.reject_host_inference_resolution(error, conversion.span); continue; }
            };
            let Type::Inference(item) = from else { continue; };
            if conversion.to.int_range().is_some() { continue; }
            let mut sources = Conversion::TABLE.iter().filter(|(_, target, _)| target == &conversion.to).map(|(source, _, _)| source);
            if let Some(source) = sources.next() {
                if sources.next().is_none() {
                    if let Err(conflict) = self.type_constraints.constrain(&Type::Inference(item), source, conversion.span) {
                        self.report_inferred_type_conflict(conflict);
                    }
                } else if conversion.to == Type::Path && self.boundary.current_return.is_some() {
                    self.require_value_conversion(&Type::Inference(item), crate::sema::schemes::ValueConversion::PathOperand, conversion.span);
                }
            }
            let Some(owner) = conversion.owner else { continue; };
            let scope = self.constraint_scope_at(owner, conversion.span);
            for (receiver, key, result, span) in indexed_iterations.get(&(scope, item)).into_iter().flatten() {
                let key = match self.type_constraints.resolve(key) {
                    Ok(key) => key,
                    Err(error) => { self.reject_host_inference_resolution(error, *span); continue; }
                };
                let result = match self.type_constraints.resolve(result) {
                    Ok(result) => result,
                    Err(error) => { self.reject_host_inference_resolution(error, *span); continue; }
                };
                if key.contains_inference() || key.contains_any() || key.contains_recovery() || !key.matches_expected(&Type::Int)
                    || result.contains_inference() || result.contains_any() || result.contains_recovery()
                    || Conversion::select(result.unvalidated(), &conversion.to).is_none() { continue; }
                if let Err(conflict) = self.type_constraints.constrain(receiver, &Type::List(Box::new(result)), *span) {
                    self.report_inferred_type_conflict(conflict);
                }
            }
        }
        if let Err(error) = self.type_constraints.solve_callable_applications() {
            self.error(body, &format!("conversion constraints failed: {error}"), DiagnosticCode::CheckLocalInference);
        }
        if let Err(error) = self.solve_host_obligations(arena) { self.reject_host_inference_resolution(error, body); }
        if let Err(error) = self.type_constraints.solve_callable_applications() {
            self.error(body, &format!("conversion constraints failed: {error}"), DiagnosticCode::CheckLocalInference);
        }
        for conversion in pending {
            match self.type_constraints.resolve(&conversion.from) {
                Ok(Type::Inference(item)) if conversion.to == Type::Path
                    && self.type_constraints.requires_conversion(&Type::Inference(item), crate::sema::schemes::ValueConversion::PathOperand) => {
                    self.conversions.insert(conversion.span, Conversion::SourceToPath);
                }
                Ok(from) => { self.publish_conversion(&from, &conversion.to, conversion.span); }
                Err(error) => { self.reject_host_inference_resolution(error, conversion.span); }
            }
        }
    }

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
        if matches!(from, Type::Unknown | Type::Invalid)
            || matches!(to, Type::Unknown | Type::Invalid)
        {
            return to;
        }
        // The operand is read as its base type: a bounded integer converts
        // as the integer it is.
        let from = match self.type_constraints.resolve(&from) {
            Ok(from) => from,
            Err(error) => return self.reject_host_inference_resolution(error, span),
        };
        if matches!(from, Type::Inference(_)) && self.collecting_local_constraints() {
            let pending = PendingConversion { owner: self.stream_origin_owner, from, to: to.clone(), span };
            if let Some(existing) = self.local_inference.conversions.iter_mut().find(|existing| existing.owner == pending.owner && existing.span == span) { *existing = pending; }
            else { self.local_inference.conversions.push(pending); }
        } else if !self.publish_conversion(&from, &to, span) {
            return Type::Invalid;
        }
        let reported = self.diagnostics.len();
        let deferred = self.deferred_effect_checks.len();
        let result = Type::Result(
            Box::new(to),
            Box::new(Type::Error),
        );
        let ty = self.check_propagation(&result, span);
        // The propagation rules are worded for `?`; say which form this is.
        // A missing `error` effect may be reported later, from a deferred
        // check, so its note is attached to that check as well.
        let note = "`as` fails the enclosing function when the conversion fails, as `?` does";
        for diagnostic in &mut self.diagnostics[reported..] {
            *diagnostic = diagnostic.clone().with_note(note);
        }
        self.add_deferred_required_effect_note(deferred, span, note.into());
        ty
    }
}
