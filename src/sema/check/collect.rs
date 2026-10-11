//! `collect { ... }`: a block whose `yield`s append to a list that is the
//! expression's value.

use super::expr::{expr_or_run_span_arena, merge_list_literal_item_ty};
use super::{Checker, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaExprOrRun, ArenaProgram, BlockId, ExprId};

/// The `collect` expression whose block is being checked. A `yield` in the
/// block belongs to it, and so does one in any block nested in that block; a
/// function or a stage block starts over with none.
#[derive(Clone, Debug)]
pub(super) struct CollectScope {
    /// The expression's span, which names it in the fact lowering reads.
    span: Span,
    /// The item type the expression's context states, if it states one.
    expected: Option<Type>,
    /// The item type the yields checked so far give.
    item: Type,
    /// Whether the block has a `yield` of its own.
    yields: bool,
}

impl Checker {
    /// Whether a `yield` here appends to a `collect` expression.
    pub(super) fn in_collect(&self) -> bool {
        self.boundary.collect_scope.is_some()
    }

    pub(super) fn check_collect_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block: BlockId,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let expected = match expected.map(|ty| ty.optional_inner().unwrap_or(ty)) {
            Some(Type::List(item)) => Some((**item).clone()),
            _ => None,
        };
        let outer = self.boundary.collect_scope.replace(CollectScope {
            span,
            item: expected.clone().unwrap_or(Type::Unknown),
            expected,
            yields: false,
        });
        // Each attempt of an enclosing `retry` builds a list of its own, so
        // only a `retry` inside the block stands between a yield and its list.
        let outer_retry = std::mem::replace(&mut self.boundary.retry_block_depth, 0);
        self.check_block_arena(arena, source, block);
        self.boundary.retry_block_depth = outer_retry;
        let scope = std::mem::replace(&mut self.boundary.collect_scope, outer)
            .expect("the collect scope entered above is still the current one");
        if !scope.yields && scope.expected.is_none() {
            self.error(
                span,
                "cannot infer the item type of a `collect` block that never yields; annotate the binding as `List[T]`",
                DiagnosticCode::CheckCollectItem,
            );
            return Type::List(Box::new(Type::Invalid));
        }
        Type::List(Box::new(scope.item))
    }

    /// `yield VALUE` in a `collect` block: one more item of its list.
    pub(super) fn check_collect_yield_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        let expected = self.begin_collect_yield(span);
        let actual = self.check_expr_or_run_arena(arena, source, value, expected.as_ref());
        let value_span = expr_or_run_span_arena(arena, value);
        if matches!(actual, Type::Stream(_)) {
            self.error(
                value_span,
                "`yield` does not accept a stream; in a `collect` block use `yield @stream.collect()`",
                DiagnosticCode::CheckYieldStream,
            );
            return;
        }
        self.finish_collect_yield(actual, value_span);
    }

    /// `yield @VALUE` in a `collect` block: every item of a list.
    pub(super) fn check_collect_yield_delegation_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        span: Span,
    ) {
        let expected = self
            .begin_collect_yield(span)
            .map(|item| Type::List(Box::new(item)));
        let actual = self.check_expr_arena(arena, source, value, expected.as_ref());
        let value_span = arena.arena.expr(value).span;
        match actual.into_unvalidated() {
            Type::List(item) => self.finish_collect_yield(*item, value_span),
            Type::Unknown => {}
            Type::Stream(_) => self.error(
                value_span,
                "`yield @` in a `collect` block takes a List; collect the stream first with `.collect()`",
                DiagnosticCode::CheckYieldDelegation,
            ),
            _ => self.error(
                value_span,
                "yield delegation requires a List or Stream; handle Results explicitly",
                DiagnosticCode::CheckYieldDelegation,
            ),
        }
    }

    /// Records the yield as the current expression's and returns the item
    /// type its context states.
    fn begin_collect_yield(&mut self, span: Span) -> Option<Type> {
        // A failed attempt would leave its items in the list and the next
        // attempt would add them again.
        if self.boundary.retry_block_depth > 0 {
            self.error(
                span,
                "`yield` is not allowed inside a retry attempt",
                DiagnosticCode::CheckYield,
            );
        }
        let scope = self
            .boundary.collect_scope
            .as_mut()
            .expect("a collect yield is checked inside a collect expression");
        scope.yields = true;
        let (collect, expected) = (scope.span, scope.expected.clone());
        self.collect_yields.insert(span, collect);
        expected
    }

    fn finish_collect_yield(&mut self, actual: Type, value_span: Span) {
        if !self.boundary.context_scope_depths.is_empty() && !actual.can_escape_context_scope() {
            self.error(
                value_span,
                "a live producer or host handle cannot escape through yield",
                DiagnosticCode::CheckContextScopeEscape,
            );
        }
        let scope = self
            .boundary.collect_scope
            .as_ref()
            .expect("a collect yield is checked inside a collect expression");
        let (expected, item) = (scope.expected.clone(), scope.item.clone());
        let item = match expected {
            Some(expected) => {
                self.expect_type(&expected, &actual, value_span);
                expected
            }
            None if item == Type::Unknown => actual,
            None => match merge_list_literal_item_ty(&item, &actual) {
                Some(merged) => merged,
                None => {
                    self.expect_type(&item, &actual, value_span);
                    item
                }
            },
        };
        if let Some(scope) = self.boundary.collect_scope.as_mut() {
            scope.item = item;
        }
    }
}
