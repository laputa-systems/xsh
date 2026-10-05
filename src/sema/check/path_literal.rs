#![allow(clippy::single_call_fn)]

//! String literals in `Path` position: a `"..."` literal takes the `Path`
//! type exactly where the expected type is `Path`, and nowhere else.
//!
//! This is the whole rule. Every context that accepts a literal for a path (a
//! typed binding, a parameter, an operand compared with a path, a member of a
//! collection of paths, a map key) does so by checking the literal against the
//! `Path` it expects, never by tolerating a `Str` where a `Path` belongs. The
//! literals that took the type are published, and lowering builds a `Path`
//! constant for exactly those, so no runtime `Str` reaches a path operation.
//! A value that is not a literal never converts: its bytes are not known to
//! be the bytes the author wrote.

use super::{Checker, Name, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::validated::ValidatedType;
use crate::syntax::arena::{ArenaExprKind, ArenaProgram, ExprId, StringLiteralId};

impl Checker {
    /// The type of a string literal under `expected`.
    ///
    /// A literal can be checked more than once (overload selection checks an
    /// argument against each candidate), so the published fact follows the
    /// latest check instead of accumulating.
    pub(super) fn check_string_literal(
        &mut self,
        arena: &ArenaProgram,
        span: Span,
        literal: StringLiteralId,
        expected: Option<&Type>,
    ) -> Type {
        if !expected.is_some_and(|expected| self.expects_path(expected)) {
            self.path_literals.remove(&span);
            return Type::Str;
        }
        if arena.arena.string_literal(literal).contains('\0') {
            self.error(
                span,
                "a Path cannot contain NUL, so this string literal cannot be a Path",
                DiagnosticCode::CheckTypeMismatch,
            );
        }
        self.path_literals.insert(span);
        match expected.and_then(|expected| self.validated_path_expectation(expected)) {
            Some(validated) => {
                let text = arena.arena.string_literal(literal).clone();
                self.validated_static_path_literal(&validated, text.as_bytes(), span)
            }
            None => Type::Path,
        }
    }

    /// The validated path type `expected` asks for, such as `RelPath`. A
    /// path literal written there is judged against the validation.
    pub(super) fn validated_path_expectation(&self, expected: &Type) -> Option<ValidatedType> {
        match expected {
            Type::Validated(validated) if *validated.base() == Type::Path => {
                Some(validated.as_ref().clone())
            }
            Type::Optional(inner) => self.validated_path_expectation(inner),
            Type::Inference(_) => self
                .type_constraints
                .resolve(expected)
                .ok()
                .filter(|resolved| !resolved.contains_inference())
                .and_then(|resolved| self.validated_path_expectation(&resolved)),
            _ => None,
        }
    }

    /// Whether `expected` asks for a `Path`. An optional path asks for one;
    /// `Any` and an unsolved type ask for nothing in particular, and there a
    /// literal stays text.
    fn expects_path(&self, expected: &Type) -> bool {
        match expected {
            Type::Path => true,
            // A validated path is a path first; the literal that takes the
            // type is then judged against the validation.
            Type::Validated(validated) => *validated.base() == Type::Path,
            Type::Optional(inner) => self.expects_path(inner),
            Type::Inference(_) => self
                .type_constraints
                .resolve(expected)
                .is_ok_and(|resolved| !resolved.contains_inference() && self.expects_path(&resolved)),
            _ => false,
        }
    }

    /// The `Path` to expect of `operand` when it is a string literal that
    /// must agree with a value of type `other`, as in `path == "literal"`.
    /// Any other operand is checked on its own terms.
    pub(super) fn path_literal_expectation(
        &self,
        arena: &ArenaProgram,
        operand: ExprId,
        other: &Type,
    ) -> Option<Type> {
        (matches!(arena.arena.expr(operand).kind, ArenaExprKind::Str(_))
            && self.expects_path(other))
        .then_some(Type::Path)
    }

    /// The key type of a quoted label in a map literal. A label is literal
    /// text, so it is a `Path` key where the map's keys are paths; lowering
    /// reads that from the literal's checked map type.
    pub(super) fn map_label_key_type(
        &mut self,
        label: Name,
        expected_key: Option<&Type>,
        span: Span,
    ) -> Type {
        if !expected_key.is_some_and(|expected| self.expects_path(expected)) {
            return Type::Str;
        }
        if label.as_str().contains('\0') {
            self.error(
                span,
                "a Path cannot contain NUL, so this label cannot be a Path key",
                DiagnosticCode::CheckTypeMismatch,
            );
        }
        Type::Path
    }
}
