//! `Set[T]`: the constructors and the set operators.
//!
//! A set holds distinct elements of one type a map key can have, so every
//! rule here reduces to an element type: a constructor's list has one,
//! the operators require the same one on both sides, and membership compares
//! against it.

use super::Checker;
use super::args::call_arg_span_arena;
use crate::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use crate::sema::types::Type;
use crate::source::Span;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ExprId};
use crate::syntax::node::BinaryOp;

/// The set type a context expects, through an optional.
pub(super) fn expected_set_element(expected: Option<&Type>) -> Option<&Type> {
    match expected.map(|ty| ty.optional_inner().unwrap_or(ty)) {
        Some(Type::Set(element)) => Some(element.as_ref()),
        _ => None,
    }
}

impl Checker {
    pub(super) fn require_set_element_type(&mut self, element: &Type, span: Span) {
        if element.is_recovery() { return; }
        match self.type_constraints.require_map_key(element, span) {
            Ok(()) => {}
            Err(crate::sema::constraints::MapKeyConstraintError::Resolution(error)) => {
                let message = match error {
                    crate::sema::constraints::ConstraintResolutionError::ForeignVariable => "Set element inference does not belong to this checking problem",
                    crate::sema::constraints::ConstraintResolutionError::TypeDepth => "Set element inference exceeds the supported nesting depth",
                };
                self.error(span, message, DiagnosticCode::CheckLocalInference);
            }
            Err(crate::sema::constraints::MapKeyConstraintError::Invalid(actual)) => {
                self.error(span, &format!("Set elements require Str, Int, UInt, Bool, Bytes, Path, or Duration, not {actual}"), DiagnosticCode::CheckSetElementType);
            }
        }
    }

    /// `set.empty()` and `set.from(items)`. The element type is the expected
    /// set's; without one, `set.from` takes the list's and `set.empty()`
    /// has none to take. `None` for any other spelling of the two calls,
    /// which the registered signature then rejects.
    pub(super) fn check_set_constructor_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
        expected: Option<&Type>,
    ) -> Option<Type> {
        let element = expected_set_element(expected).cloned();
        match (name, args) {
            ("empty", []) => {
                let Some(element) = element else {
                    self.diagnostics.push(
                        Diagnostic::error("the empty set needs an element type")
                            .with_code(DiagnosticCode::CheckLocalInference)
                            .with_label(Label::primary(
                                span,
                                "annotate the binding as `Set[T]`, or write this where a `Set[T]` is expected",
                            )),
                    );
                    return Some(Type::Unknown);
                };
                Some(Type::Set(Box::new(element)))
            }
            // A positional argument only: lowering reads the call's one
            // positional argument as the list.
            ("from", [items]) if matches!(items.kind, ArenaCallArgKind::Positional(_)) => {
                let items_span = call_arg_span_arena(arena, &items.kind);
                if let Some(element) = element {
                    let list = Type::List(Box::new(element.clone()));
                    let actual = self.check_call_arg_arena(arena, source, &items.kind, Some(&list));
                    self.expect_type(&list, &actual, items_span);
                    return Some(Type::Set(Box::new(element)));
                }
                match self
                    .check_call_arg_arena(arena, source, &items.kind, None)
                    .into_unvalidated()
                {
                    Type::List(item) => {
                        // An element is stored as its base type.
                        let item = item.into_unvalidated();
                        self.require_set_element_type(&item, items_span);
                        Some(Type::Set(Box::new(item)))
                    }
                    other => {
                        if !other.is_recovery() {
                            self.error(
                                items_span,
                                &format!("`set.from` requires a list, not {other}"),
                                DiagnosticCode::CheckTypeMismatch,
                            );
                        }
                        Some(Type::Unknown)
                    }
                }
            }
            _ => None,
        }
    }

    /// `set.add(set, item)` and `set.remove(set, item)` were functions over
    /// a map of `true`; a set has both as methods. The arguments are checked
    /// so that each has its type and its own reports.
    pub(super) fn check_removed_set_function_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        let mut diagnostic = Diagnostic::error(format!("`set.{name}` was removed"))
            .with_code(DiagnosticCode::CheckRemovedSetFunction)
            .with_label(Label::primary(
                span,
                format!("`set.{name}` is no longer a function"),
            ))
            .with_note(format!(
                "a `Set[T]` has the method `.{name}(item)`, which returns the new set"
            ));
        let mut receiver_ty = Type::Unknown;
        for (index, arg) in args.iter().enumerate() {
            let ty = self.check_call_arg_arena(arena, source, &arg.kind, None);
            if index == 0 {
                receiver_ty = ty;
            }
        }
        // The rewrite is offered where the receiver is a set and is spelled
        // as something a method can follow without grouping.
        if let [
            ArenaCallArg {
                kind: ArenaCallArgKind::Positional(receiver),
                ..
            },
            ArenaCallArg {
                kind: ArenaCallArgKind::Positional(item),
                ..
            },
        ] = args
            && matches!(receiver_ty, Type::Set(_))
            && matches!(
                arena.arena.expr(*receiver).kind,
                ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. } | ArenaExprKind::Call { .. }
            )
            && let Some(receiver) = source.get(arena.arena.expr(*receiver).span.range())
            && let Some(item) = source.get(arena.arena.expr(*item).span.range())
        {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                format!("call the `.{name}` method"),
                format!("{receiver}.{name}({item})"),
            ));
        }
        self.diagnostics.push(diagnostic);
        match receiver_ty {
            Type::Set(_) => receiver_ty,
            _ => Type::Unknown,
        }
    }

    pub(super) fn check_set_operator_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
        expected: Option<&Type>,
    ) -> Type {
        let left_span = arena.arena.expr(left).span;
        let right_span = arena.arena.expr(right).span;
        let expected = expected.filter(|ty| matches!(ty, Type::Set(_)));
        let left_ty = self
            .check_expr_arena(arena, source, left, expected)
            .into_unvalidated();
        let right_expected = match &left_ty {
            Type::Set(_) => Some(&left_ty),
            _ => expected,
        };
        let right_ty = self
            .check_expr_arena(arena, source, right, right_expected)
            .into_unvalidated();
        let (symbol, word) = match op {
            BinaryOp::Union => ("|", "or"),
            _ => ("&", "and"),
        };
        if !matches!(left_ty, Type::Set(_)) {
            if !left_ty.is_recovery() {
                let message = if left_ty == Type::Bool {
                    format!("`{symbol}` joins two sets; the boolean operator is `{word}`")
                } else {
                    format!("`{symbol}` requires two sets, not {left_ty}")
                };
                self.error(left_span, &message, DiagnosticCode::CheckSetOperator);
            }
            return Type::Unknown;
        }
        if !matches!(right_ty, Type::Set(_)) {
            if !right_ty.is_recovery() {
                self.error(
                    right_span,
                    &format!("`{symbol}` requires two sets, not {right_ty}"),
                    DiagnosticCode::CheckSetOperator,
                );
            }
            return left_ty;
        }
        if let Some(result) = left_ty.set_operator_result(op, &right_ty) { return result; }
        self.expect_type(&left_ty, &right_ty, right_span);
        left_ty
    }
}
