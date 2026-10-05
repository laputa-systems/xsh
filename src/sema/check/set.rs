//! `Set[T]`: literals, comprehensions, and the set operators.
//!
//! A set holds distinct elements of one type a map key can have, so every
//! rule here reduces to an element type: a literal's elements agree on one,
//! the operators require the same one on both sides, and membership compares
//! against it.

use super::Checker;
use super::args::call_arg_span_arena;
use crate::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use crate::sema::types::Type;
use crate::source::Span;
use crate::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaListElementRange, ArenaProgram,
    ArenaRange, ArenaRecordFieldKind, ArenaStmtKind, BlockId, ExprId,
};
use crate::syntax::node::BinaryOp;

/// The set type a context expects, through an optional.
pub(super) fn expected_set_element(expected: Option<&Type>) -> Option<&Type> {
    match expected.map(|ty| ty.optional_inner().unwrap_or(ty)) {
        Some(Type::Set(element)) => Some(element.as_ref()),
        _ => None,
    }
}

impl Checker {
    /// Reports an element type a set cannot hold. The types are the ones a
    /// map key can have, since a set is ordered and compared the same way.
    pub(super) fn require_set_element_type(&mut self, element: &Type, span: Span) {
        if !element.is_map_key() && !element.is_recovery() && !element.contains_inference() {
            self.error(
                span,
                &format!(
                    "Set elements require Str, Int, UInt, Bool, Bytes, Path, or Duration, not {element}"
                ),
                DiagnosticCode::CheckSetElementType,
            );
        }
    }

    /// One element of a set literal or the body of a set comprehension,
    /// folded into the element type the earlier elements agreed on.
    fn check_set_element(
        &mut self,
        actual: Type,
        span: Span,
        expected_element: Option<&Type>,
        inferred: &mut Type,
    ) {
        // An element is stored as its base type.
        let actual = actual.into_unvalidated();
        if let Some(expected_element) = expected_element {
            self.expect_type(expected_element, &actual, span);
        } else if *inferred == Type::Unknown {
            self.require_set_element_type(&actual, span);
            *inferred = actual;
        } else {
            self.expect_type(inferred, &actual, span);
        }
    }

    /// `{a, "b"}`: braces with an element that is not a bare name.
    pub(super) fn check_set_literal_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        range: ArenaListElementRange,
        expected: Option<&Type>,
    ) -> Type {
        let expected_element = expected_set_element(expected);
        let mut inferred = expected_element.cloned().unwrap_or(Type::Unknown);
        for item in arena.arena.list_elements(range) {
            let span = arena.arena.expr(item.value).span;
            let actual = self.check_expr_arena(arena, source, item.value, expected_element);
            self.check_set_element(actual, span, expected_element, &mut inferred);
        }
        Type::Set(Box::new(inferred))
    }

    /// `{a, b}` written where a set is expected: the names are elements, not
    /// punned fields. `None` when the braces hold anything but bare names,
    /// which leaves them a record for the caller to report.
    pub(super) fn check_set_of_names_arena(
        &mut self,
        arena: &ArenaProgram,
        range: ArenaRange,
        expected: Option<&Type>,
        span: Span,
    ) -> Option<Type> {
        let expected_element = expected_set_element(expected)?;
        let fields = arena.arena.record_fields(range);
        if fields.is_empty() {
            self.diagnostics.push(
                Diagnostic::error("`{}` is the empty record or map, not a set")
                    .with_code(DiagnosticCode::CheckTypeMismatch)
                    .with_label(Label::primary(
                        span,
                        format!("expected Set[{expected_element}]; the empty set is `set.empty()`"),
                    ))
                    .with_fix_hint(FixHint::replacement(span, "use `set.empty()`", "set.empty()")),
            );
            return Some(Type::Set(Box::new(expected_element.clone())));
        }
        if !fields
                .iter()
                .all(|field| matches!(field.kind, ArenaRecordFieldKind::Shorthand { .. }))
        {
            return None;
        }
        let mut inferred = expected_element.clone();
        for field in fields {
            let ArenaRecordFieldKind::Shorthand { name, span } = &field.kind else {
                continue;
            };
            let span = arena.arena.span(*span);
            let actual = self.lookup_record_shorthand(*name, span);
            self.check_set_element(actual, span, Some(expected_element), &mut inferred);
        }
        Some(Type::Set(Box::new(inferred)))
    }

    /// The one expression of braces written where a set is expected, when
    /// that expression is a literal: `{"a"}` is a block, since one entry
    /// without a comma always is, and here it was meant as a set.
    pub(super) fn block_written_for_set(
        &self,
        arena: &ArenaProgram,
        block: BlockId,
        expected: Option<&Type>,
    ) -> Option<ExprId> {
        expected_set_element(expected)?;
        let block = arena.arena.block(block);
        if !block.params.is_empty() {
            return None;
        }
        let mut statements = arena.arena.stmt_ids(block.statements);
        let (Some(only), None) = (statements.next(), statements.next()) else {
            return None;
        };
        let ArenaStmtKind::Expr(value) = arena.arena.stmt(only).kind else {
            return None;
        };
        matches!(
            arena.arena.expr(value).kind,
            ArenaExprKind::Str(_)
                | ArenaExprKind::PathStr(_)
                | ArenaExprKind::FmtString(_)
                | ArenaExprKind::PathFmtString(_)
                | ArenaExprKind::Int(_)
                | ArenaExprKind::Bool(_)
                | ArenaExprKind::Bytes(_)
                | ArenaExprKind::Duration(_)
        )
        .then_some(value)
    }

    /// Reports `{"a"}` where a set is expected, with the comma that makes it
    /// one, and checks the element so that it has no second report.
    pub(super) fn check_block_written_for_set_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block: BlockId,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let (Some(value), Some(element)) = (
            self.block_written_for_set(arena, block, expected),
            expected_set_element(expected).cloned(),
        ) else {
            return Type::Unknown;
        };
        let value_span = arena.arena.expr(value).span;
        let actual = self.check_expr_arena(arena, source, value, Some(&element));
        self.expect_type(&element, &actual.into_unvalidated(), value_span);
        let end = Span::new(value_span.source_id, value_span.end(), value_span.end());
        self.diagnostics.push(
            Diagnostic::error("braces around one expression are a block, not a set")
                .with_code(DiagnosticCode::CheckTypeMismatch)
                .with_label(Label::primary(
                    span,
                    format!("expected Set[{element}]; a one-element set has a comma after its element"),
                ))
                .with_fix_hint(FixHint::replacement(end, "add the comma", ",")),
        );
        Type::Set(Box::new(element))
    }

    /// `{f(x) for x in xs}`.
    pub(super) fn check_set_comp_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        body: ExprId,
        qualifiers: ArenaRange,
        expected: Option<&Type>,
    ) -> Type {
        let scopes = self.check_comp_qualifiers_arena(arena, source, qualifiers, false);
        let expected_element = expected_set_element(expected);
        let mut inferred = expected_element.cloned().unwrap_or(Type::Unknown);
        let actual = self.check_expr_arena(arena, source, body, expected_element);
        self.check_set_element(
            actual,
            arena.arena.expr(body).span,
            expected_element,
            &mut inferred,
        );
        for _ in 0..scopes {
            self.pop_scope();
        }
        Type::Set(Box::new(inferred))
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

    /// `left | right` and `left & right`. Both operands are sets of one
    /// element type, which is the result's.
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
        self.expect_type(&left_ty, &right_ty, right_span);
        left_ty
    }
}
