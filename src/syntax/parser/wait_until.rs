//! `wait until CONDITION within LIMIT`: test a condition again and again
//! until it holds or a deadline passes.
//!
//! The statement is sugar. Its meaning is the expansion `expand_wait_until`
//! builds, under the rules every expansion keeps (`super::sugar`).

use super::{Keyword, Name, Parser, TokenTag};
use crate::diagnostic::DiagnosticCode;
use crate::source::Span;
use crate::syntax::arena::{
    ArenaCallArgInput, ArenaExprOrRun, ArenaProgramBuilder, ArenaSugarOperand, ContextScopeKind,
    ExprId, StmtId, SugarForm,
};
use crate::syntax::node::{AssignOp, BinaryOp, DurationLiteral, IntLiteral};

/// The word after `wait` that begins the statement. It stays an ordinary
/// identifier everywhere else.
const UNTIL_WORD: &str = "until";
/// The word between the condition and the limit, in this statement and in
/// the head of a `retry backoff`.
pub(super) const WITHIN_WORD: &str = "within";
/// The word before a fixed interval.
const EVERY_WORD: &str = "every";
/// The word before a doubling interval, in this statement and after `retry`.
pub(super) const BACKOFF_WORD: &str = "backoff";
/// The interval between two tests when the statement names none.
const DEFAULT_INTERVAL: &str = "100ms";
/// The local that holds the interval the next sleep takes. No identifier can
/// spell it, so the condition cannot see or change it.
const DELAY_LOCAL: &str = "%delay";
/// The local that holds the largest interval of a backoff.
const CAP_LOCAL: &str = "%cap";

/// A duration operand of the head with the text it was written as.
#[derive(Clone, Copy)]
pub(super) struct DurationOperand {
    pub(super) expr: ExprId,
    pub(super) span: Span,
}

impl Parser<'_> {
    /// Whether the cursor is on `wait until`. `wait` is reserved, and the
    /// name `until` directly after it always begins this statement: a handle
    /// that is named `until` is waited for as `wait (until)`.
    pub(super) fn lookahead_is_wait_until(&self) -> bool {
        self.at_keyword(Keyword::Wait)
            && self.peek_tag(1) == Some(TokenTag::Ident)
            && self.peek_name(1).is_some_and(|name| name == UNTIL_WORD)
    }

    /// A duration written in a head that words follow: a duration literal,
    /// or a name with any `.field`s. Nothing longer is read, so the word or
    /// the `..` after it is never taken for part of it.
    pub(super) fn parse_duration_operand_arena_only(
        &mut self,
        after: &str,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<DurationOperand> {
        let start = self.current_start();
        match self.current_tag() {
            TokenTag::Duration => {
                let span = self.bump();
                let value = DurationLiteral::from_text(self.span_text(span));
                Some(DurationOperand {
                    expr: arena.push_duration_expr(&value, span),
                    span,
                })
            }
            TokenTag::Ident => {
                let name = self.current_name().expect("a name token has a name");
                let mut span = self.bump();
                let mut expr = arena.push_ident_expr(name, span);
                while self.current_tag() == TokenTag::Dot
                    && self.peek_tag(1) == Some(TokenTag::Ident)
                {
                    self.bump();
                    let field = self.current_name().expect("a name token has a name");
                    self.bump();
                    span = self.span(start, self.previous_end());
                    expr = arena.push_field_expr(expr, field, span);
                }
                Some(DurationOperand { expr, span })
            }
            _ => {
                self.diagnostic_here(
                    &format!(
                        "expected a duration literal or a name after `{after}`; bind any other expression to a name first"
                    ),
                    DiagnosticCode::ParseExpectedExpression,
                );
                None
            }
        }
    }

    /// The `..` between the two intervals of a backoff: two dots with
    /// nothing between them.
    pub(super) fn expect_backoff_range(&mut self) -> Option<()> {
        if self.current_tag() == TokenTag::Dot
            && self.peek_tag(1) == Some(TokenTag::Dot)
            && self.peek_start(1) == Some(self.current_end())
        {
            self.bump();
            self.bump();
            return Some(());
        }
        self.diagnostic_here(
            "expected `..` between the first and the largest interval of `backoff`",
            DiagnosticCode::ParseExpectedToken,
        );
        None
    }

    /// The `within` word of a head, which must be there.
    pub(super) fn expect_within_word(&mut self, after: &str) -> Option<Span> {
        if self.current_tag() == TokenTag::Ident
            && self.current_name().is_some_and(|name| name == WITHIN_WORD)
        {
            return Some(self.bump());
        }
        self.diagnostic_here(
            &format!("expected `within LIMIT` after {after}"),
            DiagnosticCode::ParseExpectedKeyword,
        );
        None
    }

    pub(super) fn parse_wait_until_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let wait = self.bump();
        let until = self.bump();
        let condition = self.parse_head_expr_arena_only(arena)?.id;
        let condition_span = arena.expr_span(condition);
        let within = self.expect_within_word("the condition of `wait until`")?;
        let limit = self.parse_duration_operand_arena_only(WITHIN_WORD, arena)?;
        let word = (self.current_tag() == TokenTag::Ident)
            .then(|| self.current_name())
            .flatten();
        let pace = if word.is_some_and(|name| name == EVERY_WORD) {
            let every = self.bump();
            let interval = self.parse_duration_operand_arena_only(EVERY_WORD, arena)?;
            Pace::Every { every, interval }
        } else if word.is_some_and(|name| name == BACKOFF_WORD) {
            let backoff = self.bump();
            let first = self.parse_duration_operand_arena_only(BACKOFF_WORD, arena)?;
            self.expect_backoff_range()?;
            let cap = self.parse_duration_operand_arena_only("..", arena)?;
            Pace::Backoff {
                backoff,
                first,
                cap,
            }
        } else {
            Pace::Default
        };
        let end = self.expect_terminator();
        let span = self.span(start, end);
        let operands = WaitUntilOperands {
            wait,
            until,
            condition,
            condition_span,
            within,
            limit,
            pace,
        };
        let mut written = Vec::with_capacity(4);
        match pace {
            Pace::Default => {}
            Pace::Every { interval, .. } => written.push(ArenaSugarOperand::Expr(interval.expr)),
            Pace::Backoff { first, cap, .. } => {
                written.push(ArenaSugarOperand::Expr(first.expr));
                written.push(ArenaSugarOperand::Expr(cap.expr));
            }
        }
        written.push(ArenaSugarOperand::Expr(limit.expr));
        written.push(ArenaSugarOperand::Expr(condition));
        arena.push_sugar(SugarForm::WaitUntil, &written, span, |arena| {
            expand_wait_until(arena, operands, span)
        });
        Some(())
    }
}

#[derive(Clone, Copy)]
enum Pace {
    Default,
    Every {
        /// The `every` word.
        every: Span,
        interval: DurationOperand,
    },
    Backoff {
        /// The `backoff` word.
        backoff: Span,
        first: DurationOperand,
        cap: DurationOperand,
    },
}

#[derive(Clone, Copy)]
struct WaitUntilOperands {
    /// The `wait` word.
    wait: Span,
    /// The `until` word.
    until: Span,
    condition: ExprId,
    condition_span: Span,
    /// The `within` word.
    within: Span,
    limit: DurationOperand,
    pace: Pace,
}

/// `wait until CONDITION within LIMIT every INTERVAL` is a `within` scope
/// around a loop that tests the condition and sleeps:
///
/// ```text
/// {
///   let %delay: Duration = INTERVAL
///   within LIMIT {
///     loop {
///       if CONDITION { break }
///       time.sleep(%delay)
///     }
///   }?
/// }
/// ```
///
/// With no `every`, the interval is `100ms`. With `backoff FIRST..CAP` the
/// interval is a variable that doubles after each sleep until it is the cap:
///
/// ```text
/// {
///   var %delay: Duration = FIRST
///   let %cap: Duration = CAP
///   within LIMIT {
///     loop {
///       if CONDITION { break }
///       time.sleep(%delay)
///       %delay = if %delay > %cap / 2 { %cap } else { %delay * 2 }
///     }
///   }?
/// }
/// ```
///
/// The comparison is against half the cap so that doubling never overflows.
/// The `?` makes a passed deadline the statement's failure wherever the
/// statement stands: without it the scope's `Result` would be the value of
/// the block, which the tail of a `try` block keeps as data.
///
/// The scope sits on `wait until CONDITION within LIMIT`, so the timeout a
/// user sees points at what was waited for, and its propagation on
/// `within LIMIT`. The nodes that can carry no diagnostic of their own take
/// distinct prefixes and suffixes of the head's words.
fn expand_wait_until(
    arena: &mut ArenaProgramBuilder<'_>,
    operands: WaitUntilOperands,
    span: Span,
) -> StmtId {
    let WaitUntilOperands {
        wait,
        until,
        condition,
        condition_span,
        within,
        limit,
        pace,
    } = operands;
    let part = |start: usize, end: usize| Span::new(span.source_id, start, end);
    // `wait` has four bytes and `within` six, so each of these is a proper
    // part of its word and distinct from the others.
    let wait_prefix = |len: usize| part(wait.start(), wait.start() + len);
    let wait_suffix = |skip: usize| part(wait.start() + skip, wait.end());
    let within_prefix = |len: usize| part(within.start(), within.start() + len);
    let within_suffix = |skip: usize| part(within.start() + skip, within.end());
    let delay = Name::intern(DELAY_LOCAL);
    let cap_local = Name::intern(CAP_LOCAL);
    let duration = Name::intern("Duration");

    arena.begin_block();

    // The interval is read once, before the scope is entered.
    let (first_interval, pace_span, backoff_cap) = match pace {
        Pace::Default => {
            let literal = DurationLiteral::from_text(DEFAULT_INTERVAL);
            (arena.push_duration_expr(&literal, wait_prefix(1)), wait, None)
        }
        Pace::Every { every, interval } => (
            interval.expr,
            part(every.start(), interval.span.end()),
            None,
        ),
        Pace::Backoff {
            backoff,
            first,
            cap,
        } => (
            first.expr,
            part(backoff.start(), first.span.end()),
            Some((backoff, first, cap)),
        ),
    };
    let delay_type = arena.push_named_type_expr(duration, pace_span);
    let delay_target = arena.push_binding_target_name(delay);
    arena.push_binding_parts(
        backoff_cap.is_none(),
        delay_target,
        Some(delay_type),
        ArenaExprOrRun::Expr(first_interval),
        pace_span,
    );
    if let Some((backoff, _, cap)) = backoff_cap {
        let cap_type = arena.push_named_type_expr(duration, cap.span);
        let cap_target = arena.push_binding_target_name(cap_local);
        arena.push_binding_parts(
            true,
            cap_target,
            Some(cap_type),
            ArenaExprOrRun::Expr(cap.expr),
            part(backoff.start(), cap.span.end()),
        );
    }

    // The body of the scope: one loop.
    arena.begin_block();
    arena.begin_block();

    arena.begin_block();
    arena.push_break(None, until);
    let leave = arena.finish_block(&[], until);
    let tested = part(until.start(), condition_span.end());
    arena.push_if(&[(condition, leave)], None, tested);

    let module = arena.push_ident_expr(Name::intern("time"), wait_prefix(2));
    let callee = arena.push_field_expr(module, Name::intern("sleep"), wait_prefix(3));
    let interval = arena.push_ident_expr(delay, wait_suffix(1));
    arena.begin_call_args();
    arena.push_call_arg_input(ArenaCallArgInput::Positional(interval));
    let args = arena.finish_call_args();
    let sleep = arena.push_call_expr(callee, args, wait_suffix(2));
    arena.push_expr_statement(sleep, wait_suffix(1));

    if let Some((_, first, cap)) = backoff_cap {
        let range = part(first.span.start(), cap.span.end());
        let current = arena.push_ident_expr(delay, within_prefix(1));
        let largest = arena.push_ident_expr(cap_local, within_prefix(2));
        let two = arena.push_int_expr(&IntLiteral::from_text("2"), within_prefix(3));
        let half = arena.push_binary_expr(BinaryOp::Div, largest, two, within_prefix(4));
        let capped = arena.push_binary_expr(BinaryOp::Gt, current, half, within_prefix(5));
        let largest = arena.push_ident_expr(cap_local, within_suffix(1));
        let current = arena.push_ident_expr(delay, within_suffix(2));
        let two = arena.push_int_expr(&IntLiteral::from_text("2"), within_suffix(3));
        let doubled = arena.push_binary_expr(BinaryOp::Mul, current, two, within_suffix(4));
        // Each branch of a written `if` expression is a block of its own.
        arena.begin_block();
        arena.push_expr_statement(largest, within_prefix(1));
        let at_cap = arena.finish_block(&[], within);
        let at_cap = arena.push_value_block_expr(at_cap, within);
        arena.begin_block();
        arena.push_expr_statement(doubled, within_prefix(2));
        let twice = arena.finish_block(&[], part(within.start(), limit.span.end()));
        let twice = arena.push_value_block_expr(twice, within_suffix(5));
        arena.begin_if_expr_branches();
        arena.push_if_expr_branch_input(capped, at_cap);
        let branches = arena.finish_if_expr_branches();
        let next = arena.push_if_expr(branches, twice, range);
        let target = arena.push_assign_target_name(delay);
        arena.push_assignment(target, AssignOp::Set, ArenaExprOrRun::Expr(next), range);
    }

    let iteration = arena.finish_block(&[], tested);
    arena.push_loop(iteration, wait_suffix(2));
    let scope_span = part(wait.start(), limit.span.end());
    let waited = arena.finish_block(&[], scope_span);
    let scope =
        arena.push_context_scope_expr(ContextScopeKind::Within, limit.expr, waited, true, scope_span);
    let propagate_span = part(within.start(), limit.span.end());
    let propagate = arena.push_try_expr(scope, propagate_span);
    arena.push_expr_statement(propagate, propagate_span);

    // The block starts at `until`: the whole statement's span may already
    // belong to a block, since a `match` arm that is one statement is a block
    // with that statement's span.
    let outer_span = part(until.start(), span.end());
    let block = arena.finish_block(&[], outer_span);
    let outer = arena.push_value_block_expr(block, outer_span);
    arena.push_expr_statement(outer, span)
}
