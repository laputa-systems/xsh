#![allow(clippy::single_call_fn)]

use super::Span;
use super::{
    BinaryOp, Diagnostic, DurationLiteral, FixHint, FloatLiteral, IntLiteral, Keyword, Label, Name,
    Parser, StreamStageKind, TokenKindMatch, TokenTag, UnaryOp, decode_bytes_literal_for, literal,
};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{
    ArenaCallArgInput, ArenaCompQualifier, ArenaExprKind, ArenaListElementInput, ArenaPipeStage,
    ArenaPipeStageKind, ArenaProgramBuilder, ArenaRange, ArenaRecordFieldInput, ArenaStreamStage,
    BlockId, ExprId, RunFormId,
};
use crate::syntax::grammar::{self, LineContinuation, PrimaryForm};
use crate::syntax::grouping;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub(super) struct ArenaOnlyExpr {
    pub(super) id: ExprId,
    pub(super) span: Span,
    bare_ident: Option<Name>,
}

/// Accumulates `a |> b |> c` stages across loop iterations of
/// `parse_precedence_arena_only`, deferring the arena commit until the chain
/// ends (no more `|>` follows). This is cheaper than the old with_arena
/// bridge, which rebuilt and re-lowered the whole accumulated tree on every
/// additional stage. `input` is the already-committed `ExprId` the chain
/// started from; `left.id` is stale while a chain is pending (only `left.span`
/// stays accurate) — every other postfix branch must seal first.
enum ArenaPendingPipeline {
    Value {
        input: ExprId,
    },
    Pipeline {
        input: ExprId,
        stages: Vec<ArenaPipeStage>,
    },
    Structured {
        input: ExprId,
        stages: Vec<ArenaStreamStage>,
    },
}

impl ArenaPendingPipeline {
    fn seal(self, arena: &mut ArenaProgramBuilder<'_>, span: Span) -> ExprId {
        match self {
            ArenaPendingPipeline::Value { input } => input,
            ArenaPendingPipeline::Pipeline { input, stages } => {
                arena.build_pipeline_expr(input, stages, span)
            }
            ArenaPendingPipeline::Structured { input, stages } => {
                arena.build_structured_pipeline_expr(input, stages, span)
            }
        }
    }
}

/// Fold one more `|>` stage into the pending pipeline state. `input` is only
/// used when `pending` is `None` (the very first stage in the chain) — it
/// must be the `ExprId` of the operand the chain started from. A `Structured`
/// (stream-only) pipeline that then receives an `Expr` stage gets sealed into
/// a concrete node and wrapped as the input of a new mixed `Pipeline`,
/// mirroring the old with_arena bridge's `_ => Pipeline { input: Box::new(left), .. }`
/// fallback arm.
fn extend_arena_pending_pipeline(
    arena: &mut ArenaProgramBuilder<'_>,
    pending: Option<ArenaPendingPipeline>,
    input: ExprId,
    prev_span: Span,
    stage_kind: ArenaPipeStageKind,
    stage_span: Span,
) -> ArenaPendingPipeline {
    match (pending, stage_kind) {
        (None, ArenaPipeStageKind::Stream(stream)) => ArenaPendingPipeline::Structured {
            input,
            stages: vec![stream],
        },
        (None, ArenaPipeStageKind::Expr(expr_id)) => {
            if let Some(input) = arena.build_value_pipeline_stage(input, expr_id, stage_span) {
                ArenaPendingPipeline::Value { input }
            } else {
                let stage = arena.build_pipe_stage(ArenaPipeStageKind::Expr(expr_id), stage_span);
                ArenaPendingPipeline::Pipeline {
                    input,
                    stages: vec![stage],
                }
            }
        }
        (
            Some(ArenaPendingPipeline::Structured { input, mut stages }),
            ArenaPipeStageKind::Stream(stream),
        ) => {
            stages.push(stream);
            ArenaPendingPipeline::Structured { input, stages }
        }
        (
            Some(ArenaPendingPipeline::Structured { input, stages }),
            ArenaPipeStageKind::Expr(expr_id),
        ) => {
            let sealed = arena.build_structured_pipeline_expr(input, stages, prev_span);
            if let Some(input) = arena.build_value_pipeline_stage(sealed, expr_id, stage_span) {
                ArenaPendingPipeline::Value { input }
            } else {
                let stage = arena.build_pipe_stage(ArenaPipeStageKind::Expr(expr_id), stage_span);
                ArenaPendingPipeline::Pipeline {
                    input: sealed,
                    stages: vec![stage],
                }
            }
        }
        (Some(ArenaPendingPipeline::Value { input }), ArenaPipeStageKind::Expr(expr_id)) => {
            if let Some(input) = arena.build_value_pipeline_stage(input, expr_id, stage_span) {
                ArenaPendingPipeline::Value { input }
            } else {
                let stage = arena.build_pipe_stage(ArenaPipeStageKind::Expr(expr_id), stage_span);
                ArenaPendingPipeline::Pipeline {
                    input,
                    stages: vec![stage],
                }
            }
        }
        (Some(ArenaPendingPipeline::Value { input }), ArenaPipeStageKind::Stream(stream)) => {
            ArenaPendingPipeline::Structured {
                input,
                stages: vec![stream],
            }
        }
        (Some(ArenaPendingPipeline::Pipeline { input, mut stages }), kind) => {
            stages.push(arena.build_pipe_stage(kind, stage_span));
            ArenaPendingPipeline::Pipeline { input, stages }
        }
    }
}

impl<'a> Parser<'a> {
    fn parse_if_expr_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        self.bump();
        arena.begin_if_expr_branches();
        let Some(condition) = self.parse_condition_arena_only(arena) else {
            arena.discard_if_expr_branches();
            return None;
        };
        let Some((value, mut end)) = self.parse_braced_value_expr_arena_only("if branch", arena)
        else {
            arena.discard_if_expr_branches();
            return None;
        };
        arena.push_if_expr_branch_input(condition.id, value.id);
        let mut else_value = None;
        while let Some(implied_if) = self.consume_else() {
            if implied_if || self.consume_keyword(Keyword::If).is_some() {
                let Some(condition) = self.parse_condition_arena_only(arena) else {
                    arena.discard_if_expr_branches();
                    return None;
                };
                let Some((value, branch_end)) =
                    self.parse_braced_value_expr_arena_only("else-if branch", arena)
                else {
                    arena.discard_if_expr_branches();
                    return None;
                };
                end = branch_end;
                arena.push_if_expr_branch_input(condition.id, value.id);
            } else {
                let Some((value, branch_end)) =
                    self.parse_braced_value_expr_arena_only("else branch", arena)
                else {
                    arena.discard_if_expr_branches();
                    return None;
                };
                end = branch_end;
                else_value = Some(value.id);
                break;
            }
        }
        let else_value = match else_value {
            Some(value) => value,
            None => {
                self.diagnostic_here(
                    "if expressions require an `else` branch",
                    DiagnosticCode::ParseIfExpressionElse,
                );
                arena.push_null_expr(self.current_span())
            }
        };
        let branches = arena.finish_if_expr_branches();
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_if_expr(branches, else_value, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_braced_value_expr_arena_only(
        &mut self,
        _context: &str,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<(ArenaOnlyExpr, usize)> {
        let start = self.current_start();
        let block = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        Some((
            ArenaOnlyExpr {
                id: arena.push_value_block_expr(block, span),
                span,
                bare_ident: None,
            },
            span.end(),
        ))
    }

    fn parse_match_expr_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        self.bump();
        let value = self.parse_head_expr_arena_only(arena)?;
        self.expect(TokenKindMatch::LBrace, "expected `{` to start match arms")?;
        self.skip_separators();
        arena.begin_match_expr_arms();
        let mut else_arm = None;
        self.in_nested_group(|parser| {
            while !parser.at(TokenKindMatch::RBrace) && !parser.at(TokenKindMatch::Eof) {
                if parser
                    .parse_match_expr_arm_arena_only(arena, &mut else_arm)
                    .is_none()
                {
                    parser.recover_match_arm();
                }
                parser.skip_separators();
            }
        });
        let end = self
            .expect(TokenKindMatch::RBrace, "expected `}` to close match")
            .map(|span| span.end())
            .unwrap_or_else(|| self.current_end());
        let span = self.span(start, end);
        let arms = arena.finish_match_expr_arms();
        Some(ArenaOnlyExpr {
            id: arena.push_match_expr(value.id, arms, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_match_expr_arm_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        else_arm: &mut Option<Span>,
    ) -> Option<()> {
        let start = self.current_start();
        let (pattern, guard, spelling) = self.parse_match_arm_head_arena_only(arena, else_arm)?;
        let value = if self.at(TokenKindMatch::LBrace) && !self.brace_starts_record_value() {
            self.parse_braced_value_expr_arena_only("match arm", arena)?
                .0
                .id
        } else {
            let outer_arm_body = self.arm_body_start.replace(self.index);
            let value = self.parse_expr_id_arena_only(arena);
            self.arm_body_start = outer_arm_body;
            value?
        };
        let value_end = self.previous_end();
        if self.consume(TokenKindMatch::Comma).is_some() {
            self.skip_newlines();
        }
        let span = self.span(start, value_end);
        arena.push_match_expr_arm_input_id(pattern, guard, value, spelling, span);
        Some(())
    }

    fn brace_starts_record_value(&self) -> bool {
        self.brace_starts_record(true)
    }

    /// Whether `{` starts a record whose first field is written out as
    /// `name:`, `"key":`, `[key]:`, or `...spread`; a `match` statement arm
    /// body is otherwise a block.
    pub(super) fn brace_starts_field_record(&self) -> bool {
        self.brace_starts_record(false)
    }

    /// `shorthand` also accepts `{}` and a first shorthand field `{name, ...}`.
    fn brace_starts_record(&self, shorthand_or_empty: bool) -> bool {
        let mut offset = 1;
        while matches!(
            self.peek_tag(offset),
            Some(TokenTag::Newline | TokenTag::Comment)
        ) {
            offset += 1;
        }
        if (shorthand_or_empty && self.peek_tag(offset) == Some(TokenTag::RBrace))
            || (self.peek_tag(offset) == Some(TokenTag::Dot)
                && self.peek_tag(offset + 1) == Some(TokenTag::Dot))
        {
            return true;
        }
        if self.peek_tag(offset) == Some(TokenTag::LBracket) {
            let mut depth = 1;
            offset += 1;
            while depth > 0 {
                match self.peek_tag(offset) {
                    Some(TokenTag::LBracket) => depth += 1,
                    Some(TokenTag::RBracket) => depth -= 1,
                    None | Some(TokenTag::Eof) => return false,
                    _ => {}
                }
                offset += 1;
            }
            while matches!(
                self.peek_tag(offset),
                Some(TokenTag::Newline | TokenTag::Comment)
            ) {
                offset += 1;
            }
            return self.peek_tag(offset) == Some(TokenTag::Colon);
        }
        let mut shorthand = shorthand_or_empty && self.peek_tag(offset) == Some(TokenTag::Ident)
            || shorthand_or_empty
                && (self.peek_tag(offset) == Some(TokenTag::Keyword)
                    && !self
                        .peek_keyword(offset)
                        .is_some_and(|keyword| grammar::BLOCK_ONLY_KEYWORDS.contains(&keyword)));
        if self.peek_label_name(offset).is_none() && self.peek_tag(offset) != Some(TokenTag::String)
        {
            return false;
        }
        offset += 1;
        while self.peek_tag(offset) == Some(TokenTag::Dot)
            && self.peek_label_name(offset + 1).is_some()
        {
            shorthand = false;
            offset += 2;
        }
        while matches!(
            self.peek_tag(offset),
            Some(TokenTag::Newline | TokenTag::Comment)
        ) {
            offset += 1;
        }
        self.peek_tag(offset) == Some(TokenTag::Colon)
            || (shorthand
                && matches!(
                    self.peek_tag(offset),
                    Some(TokenTag::Comma | TokenTag::RBrace)
                ))
    }

    /// Shell `[ -f x ]`, `[[ -n $x ]]`, and `[ $x -lt 3 ]` conditions. A list
    /// literal never starts with a flag word or `$name`, so these shapes are
    /// reported once and skipped through the last `]` on the line, along
    /// with a following `; then` or `; do`. A bare `[[` is not enough: a
    /// nested list or comprehension (`[[a, 2] for x in xs]`) starts the same way.
    fn skip_shell_test(&mut self) -> bool {
        if self.current_tag() != TokenTag::LBracket {
            return false;
        }
        let shell_word = |at: usize| {
            (self.peek_tag(at) == Some(TokenTag::Minus)
                && self.peek_tag(at + 1) == Some(TokenTag::Ident)
                && self.peek_start(at + 1) == self.peek_end(at))
                || matches!(
                    self.peek_tag(at),
                    Some(TokenTag::DollarIdent | TokenTag::DollarLBrace)
                )
        };
        let shell_test =
            shell_word(1) || (self.peek_tag(1) == Some(TokenTag::LBracket) && shell_word(2));
        if !shell_test {
            return false;
        }
        let start = self.current_start();
        let mut offset = 0;
        let mut last_close = None;
        while let Some(tag) = self.peek_tag(offset) {
            match tag {
                TokenTag::RBracket => last_close = Some(offset),
                TokenTag::LBrace | TokenTag::Newline | TokenTag::Semicolon | TokenTag::Eof => break,
                _ => {}
            }
            offset += 1;
        }
        let Some(last_close) = last_close else {
            return false;
        };
        let end = self.peek_end(last_close).unwrap_or(start);
        self.diagnostics.push(
            Diagnostic::error(
                "`[ ... ]` is shell test syntax; an XSH condition is a Bool expression",
            )
            .with_code(DiagnosticCode::ParseForeignSyntax)
            .with_label(Label::primary(
                self.span(start, end),
                "write an expression such as `p\"x\".exists()?` or `count < 3`",
            )),
        );
        for _ in 0..=last_close {
            self.bump();
        }
        if self.current_tag() == TokenTag::Semicolon
            && self
                .peek_label_name(1)
                .is_some_and(|name| name == "then" || name == "do")
        {
            self.bump();
            self.bump();
        }
        true
    }

    pub(super) fn parse_condition_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let previous = self.condition_expr;
        self.condition_expr = true;
        let condition = (|| {
            let start = self.current_start();
            if self.skip_shell_test() {
                // The block still parses when it follows; a shell `; then`
                // line ends here and statement recovery takes over.
                if !self.at(TokenKindMatch::LBrace) {
                    return None;
                }
                let span = self.span(start, self.previous_end());
                return Some(ArenaOnlyExpr {
                    id: arena.push_bool_expr(true, span),
                    span,
                    bare_ident: None,
                });
            }
            if self.consume_keyword(Keyword::Let).is_some() {
                self.skip_newlines();
                let (pattern, _) = self.parse_pattern_arena_only(arena)?;
                self.skip_newlines();
                self.expect(
                    TokenKindMatch::Equals,
                    "expected `=` after condition pattern",
                );
                self.skip_newlines();
                let value = self.parse_precedence_arena_only(0, arena)?;
                let span = self.span(start, value.span.end());
                Some(ArenaOnlyExpr {
                    id: arena.push_pattern_condition_expr(value.id, pattern, span),
                    span,
                    bare_ident: None,
                })
            } else {
                self.parse_precedence_arena_only(0, arena)
            }
        })();
        self.condition_expr = previous;
        condition
    }

    /// An expression that a body block follows (a `for` iterable, a `match`
    /// subject, a `with` value), read like a condition: a qualified pattern
    /// test such as `x is E.V {` ends before the block unless the brace holds
    /// a payload.
    pub(super) fn parse_head_expr_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let previous = std::mem::replace(&mut self.condition_expr, true);
        let expr = self.parse_precedence_arena_only(0, arena);
        self.condition_expr = previous;
        expr
    }

    /// Runs `parse` inside a bracket, brace, or argument list. What the
    /// enclosing construct changed about where an expression ends stops at
    /// the delimiter: inside it a `|>` adds a stage even within a pipeline
    /// stage, a spaced `?` before the closer tries the expression even
    /// within a command argument, `,` does not end a statement even within a
    /// `match` arm or `with` binding, and a brace after a qualified pattern
    /// test is its payload even within a condition.
    pub(super) fn in_nested_group<T>(&mut self, parse: impl FnOnce(&mut Self) -> T) -> T {
        let pipe_is_boundary = std::mem::replace(&mut self.pipe_is_boundary, false);
        let trailing_statement_try = std::mem::replace(&mut self.trailing_statement_try, true);
        let comma_is_terminator = std::mem::replace(&mut self.comma_is_terminator, false);
        let condition_expr = std::mem::replace(&mut self.condition_expr, false);
        let result = parse(self);
        self.pipe_is_boundary = pipe_is_boundary;
        self.trailing_statement_try = trailing_statement_try;
        self.comma_is_terminator = comma_is_terminator;
        self.condition_expr = condition_expr;
        result
    }

    pub(super) fn parse_expr_id_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ExprId> {
        self.parse_precedence_arena_only(0, arena)
            .map(|expr| expr.id)
    }

    /// When the precedence loop stops on a token that looks like a C-style
    /// boolean operator (`||`, `&&`, `|`, `&`) or a `then` keyword, emit a
    /// constructive diagnostic that names the offending token and points the
    /// agent at the word-form `or`/`and` operators instead of the block brace
    /// that follows. This turns a ~10-turn operator-spelling discovery into a
    /// one-line fix without changing any valid-program parsing.
    ///
    /// An adjacent `&&` or `||` is unambiguous, so the parse continues as
    /// `and`/`or` (returned with its token count) and the brace that follows
    /// is not reported a second time.
    fn report_unsupported_boolean_operator(&mut self) -> Option<(BinaryOp, usize)> {
        let adjacent = self.peek_start(1) == Some(self.current_end());
        let (unsupported, supported, span, recovered) = match (self.current_tag(), self.peek_tag(1))
        {
            (TokenTag::Pipe, Some(TokenTag::Pipe)) => {
                let span = self.span(self.current_start(), self.peek_end(1).unwrap());
                ("||", "or", span, adjacent.then_some(BinaryOp::Or))
            }
            (TokenTag::Amp, Some(TokenTag::Amp)) => {
                let span = self.span(self.current_start(), self.peek_end(1).unwrap());
                ("&&", "and", span, adjacent.then_some(BinaryOp::And))
            }
            (TokenTag::Pipe, _) => ("|", "or", self.current_span(), None),
            (TokenTag::Amp, _) => ("&", "and", self.current_span(), None),
            (TokenTag::Ident, _) if self.at_ident("then") => {
                let span = self.current_span();
                self.diagnostics.push(
                    Diagnostic::error("the `then` keyword is not used in XSH")
                        .with_code(DiagnosticCode::ParseUnsupportedThen)
                        .with_label(Label::primary(
                            span,
                            "XSH `if`/`while`/`for` heads are followed directly by `{`, not `then`",
                        )),
                );
                return None;
            }
            _ => return None,
        };
        self.diagnostics.push(
            Diagnostic::error(format!(
                "unsupported operator '{unsupported}': XSH boolean operators are the word forms '{supported}'"
            ))
            .with_code(DiagnosticCode::ParseUnsupportedBooleanOperator)
            .with_label(Label::primary(
                span,
                format!("use '{supported}' instead of '{unsupported}'"),
            ))
            .with_fix_hint(FixHint::replacement(span, format!("replace with `{supported}`"), supported)),
        );
        recovered.map(|op| (op, 2))
    }

    /// Report the unsupported integer-division spellings while retaining a
    /// division-shaped AST for parser recovery. Int `/` is the documented
    /// truncating integer-division spelling; `//` and `div` are not operators.
    fn report_unsupported_integer_division(&mut self) -> Option<usize> {
        let (span, replacement, tokens) = if self.current_tag() == TokenTag::Slash
            && self.peek_tag(1) == Some(TokenTag::Slash)
            && self.peek_start(1) == Some(self.current_end())
        {
            (
                self.span(self.current_start(), self.peek_end(1).unwrap()),
                "//",
                2,
            )
        } else if self.current_tag() == TokenTag::Ident && self.at_ident("div") {
            (self.current_span(), "div", 1)
        } else {
            return None;
        };

        self.diagnostics.push(
            Diagnostic::error(format!(
                "unsupported integer-division operator '{replacement}': use `/` on Int operands"
            ))
            .with_code(DiagnosticCode::ParseUnsupportedIntegerDivision)
            .with_label(Label::primary(
                span,
                "use `/` on Int operands; it truncates the result",
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                "replace with integer `/`",
                "/",
            )),
        );
        Some(tokens)
    }

    fn parse_precedence_arena_only(
        &mut self,
        min_prec: u8,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let command_arg_root = self.command_arg_expr;
        if command_arg_root {
            self.command_arg_expr = false;
        }
        let left = self.parse_prefix_arena_only(arena)?;
        self.parse_operators_after_arena_only(left, min_prec, command_arg_root, arena)
    }

    /// Whether a run form that just ended heads a pipeline: `|>` follows it,
    /// directly or on the next line, after an optional `?` that propagates
    /// the run form's failure.
    pub(super) fn at_run_pipeline(&self) -> bool {
        let after_try = self.index + usize::from(self.current_tag() == TokenTag::Question);
        self.token_table.tag_at(after_try) == Some(TokenTag::PipeGt)
            || matches!(
                self.line_continuation_at(after_try),
                Some((LineContinuation::Pipeline, _))
            )
    }

    /// The pipeline that a run form heads in statement or initializer
    /// position, where a run form is otherwise a command: the run form is the
    /// pipeline's first value, and the rest reads as any expression does.
    pub(super) fn parse_run_pipeline_arena_only(
        &mut self,
        run_id: RunFormId,
        run_span: Span,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let head = self.run_value_arena_only(run_id, run_span, arena);
        self.parse_operators_after_arena_only(head, 0, false, arena)
    }

    /// A run form as a value. The `?` before a pipeline belongs to the run
    /// form, as a trailing `?` always does; elsewhere a `?` is left for the
    /// operator loop.
    fn run_value_arena_only(
        &mut self,
        run_id: RunFormId,
        run_span: Span,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> ArenaOnlyExpr {
        let value = ArenaOnlyExpr {
            id: arena.push_run_expr_id(run_id, run_span),
            span: run_span,
            bare_ident: None,
        };
        if !(self.at(TokenKindMatch::Question) && self.at_run_pipeline()) {
            return value;
        }
        self.bump();
        let span = self.span(run_span.start(), self.previous_end());
        ArenaOnlyExpr {
            id: arena.push_try_expr(value.id, span),
            span,
            bare_ident: None,
        }
    }

    /// Applies the postfix forms, binary operators, pattern tests, and
    /// pipeline stages that follow `left`, down to `min_prec`.
    fn parse_operators_after_arena_only(
        &mut self,
        mut left: ArenaOnlyExpr,
        min_prec: u8,
        command_arg_root: bool,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let mut pending_pipeline: Option<ArenaPendingPipeline> = None;
        loop {
            self.skip_postfix_newlines();
            self.skip_pipeline_newlines();
            // A typed command argument ends at whitespace or a line break.
            if command_arg_root
                && (self.current_start() > left.span.end()
                    || matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment))
            {
                break;
            }
            let continues_pipeline = pending_pipeline.is_some()
                && min_prec == 0
                && !self.pipe_is_boundary
                && self.at(TokenKindMatch::PipeGt);
            if !continues_pipeline && let Some(pending) = pending_pipeline.take() {
                left.id = pending.seal(arena, left.span);
            }
            if self.at(TokenKindMatch::Question)
                && self.peek_tag(1) == Some(TokenTag::Dot)
                && self.peek_start(1) == Some(self.current_end())
            {
                let try_end = self.current_end();
                self.bump();
                self.bump();
                let name = self.expect_member_name("expected field name after `?.`")?;
                if name == "require" && self.consume(TokenKindMatch::LParen).is_some() {
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                        self.bump();
                    }
                    let schema = if self.at(TokenKindMatch::RParen) {
                        None
                    } else {
                        Some(self.parse_type_expr(arena)?)
                    };
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                        self.bump();
                    }
                    self.expect(TokenKindMatch::RParen, "expected `)` after require schema");
                    let try_span = self.span(left.span.start(), try_end);
                    let try_id = arena.push_try_expr(left.id, try_span);
                    let span = self.span(left.span.start(), self.previous_end());
                    let id = arena.push_require_expr(try_id, schema, span);
                    left = ArenaOnlyExpr {
                        id,
                        span,
                        bare_ident: None,
                    };
                } else {
                    let span = self.span(left.span.start(), self.previous_end());
                    let id = arena.push_null_safe_field_expr(left.id, name, span);
                    left = ArenaOnlyExpr {
                        id,
                        span,
                        bare_ident: None,
                    };
                }
            } else if self.at(TokenKindMatch::Question)
                && !(self.current_start() == left.span.end()
                    && self.peek_tag(1) == Some(TokenTag::LBracket)
                    && self.peek_start(1) == Some(self.current_end()))
                && (self.current_start() == left.span.end()
                    || (self.trailing_statement_try && self.question_is_trailing_statement_try()))
            {
                self.bump();
                let span = self.span(left.span.start(), self.previous_end());
                let id = arena.push_try_expr(left.id, span);
                left = ArenaOnlyExpr {
                    id,
                    span,
                    bare_ident: None,
                };
            } else if min_prec == 0
                && !self.pipe_is_boundary
                && self.consume(TokenKindMatch::PipeGt).is_some()
            {
                let (stage_kind, stage_span) = self.parse_pipe_stage_arena_only(arena)?;
                let prev_span = left.span;
                let span = self.span(left.span.start(), stage_span.end());
                pending_pipeline = Some(extend_arena_pending_pipeline(
                    arena,
                    pending_pipeline.take(),
                    left.id,
                    prev_span,
                    stage_kind,
                    stage_span,
                ));
                left = ArenaOnlyExpr {
                    id: left.id,
                    span,
                    bare_ident: None,
                };
            } else if self.at(TokenKindMatch::Dot) && self.peek_tag(1) != Some(TokenTag::Dot) {
                self.bump();
                let name = self.expect_member_name("expected field name after `.`")?;
                // These receivers also name user callable fields and removed
                // contract calls; ordinary arguments reach semantic resolution.
                let is_contract_call = left
                    .bare_ident
                    .is_some_and(|module| matches!(module.as_str().as_str(), "record" | "module"));
                if name == "require"
                    && !is_contract_call
                    && self.consume(TokenKindMatch::LParen).is_some()
                {
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                        self.bump();
                    }
                    let schema = if self.at(TokenKindMatch::RParen) {
                        None
                    } else {
                        Some(self.parse_type_expr(arena)?)
                    };
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                        self.bump();
                    }
                    self.expect(TokenKindMatch::RParen, "expected `)` after require schema");
                    let span = self.span(left.span.start(), self.previous_end());
                    let id = arena.push_require_expr(left.id, schema, span);
                    left = ArenaOnlyExpr {
                        id,
                        span,
                        bare_ident: None,
                    };
                } else {
                    let span = self.span(left.span.start(), self.previous_end());
                    let id = arena.push_field_expr(left.id, name, span);
                    left = ArenaOnlyExpr {
                        id,
                        span,
                        bare_ident: None,
                    };
                }
            } else if self.at(TokenKindMatch::LBracket)
                || (self.at(TokenKindMatch::Question)
                    && self.current_start() == left.span.end()
                    && self.peek_tag(1) == Some(TokenTag::LBracket)
                    && self.peek_start(1) == Some(self.current_end()))
            {
                let guarded = self.consume(TokenKindMatch::Question).is_some();
                self.expect(TokenKindMatch::LBracket, "expected `[` after `?`");
                let bounds = self.in_nested_group(
                    |parser| -> Option<(Option<ExprId>, Option<Option<ExprId>>)> {
                        if parser.consume_dot_dot() {
                            let end = if parser.at(TokenKindMatch::RBracket) {
                                None
                            } else {
                                Some(parser.parse_precedence_arena_only(0, arena)?.id)
                            };
                            return Some((None, Some(end)));
                        }
                        let first = parser.parse_precedence_arena_only(0, arena)?;
                        if !parser.consume_dot_dot() {
                            return Some((Some(first.id), None));
                        }
                        let end = if parser.at(TokenKindMatch::RBracket) {
                            None
                        } else {
                            Some(parser.parse_precedence_arena_only(0, arena)?.id)
                        };
                        Some((Some(first.id), Some(end)))
                    },
                )?;
                self.expect(
                    TokenKindMatch::RBracket,
                    "expected `]` after index expression",
                );
                let span = self.span(left.span.start(), self.previous_end());
                let id = match bounds {
                    (Some(index), None) if guarded => {
                        arena.push_guarded_index_expr(left.id, index, span)
                    }
                    (Some(index), None) => arena.push_index_expr(left.id, index, span),
                    (start, end) if guarded => {
                        arena.push_guarded_slice_expr(left.id, start, end.flatten(), span)
                    }
                    (start, end) => arena.push_slice_expr(left.id, start, end.flatten(), span),
                };
                left = ArenaOnlyExpr {
                    id,
                    span,
                    bare_ident: None,
                };
            } else if self.consume(TokenKindMatch::LParen).is_some() {
                let args = self.parse_call_args_arena_only(arena);
                self.expect(TokenKindMatch::RParen, "expected `)` after call arguments");
                let span = self.span(left.span.start(), self.previous_end());
                let id = arena.push_call_expr(left.id, args, span);
                left = ArenaOnlyExpr {
                    id,
                    span,
                    bare_ident: None,
                };
            } else if arena_expr_accepts_builder_block(arena, left.id)
                && self.at(TokenKindMatch::LBrace)
            {
                let block = self.parse_builder_block_arena_only(arena)?;
                let span = self.span(left.span.start(), self.previous_end());
                let id = arena.push_builder_call_expr_id(left.id, block, span);
                left = ArenaOnlyExpr {
                    id,
                    span,
                    bare_ident: None,
                };
            } else {
                if self.current_binary_op().is_none() && self.continuation_binary_op().is_some() {
                    self.skip_line_breaks();
                }
                if self.at_ident("is") {
                    if min_prec > grammar::PATTERN_TEST {
                        break;
                    }
                    if let Some(pending) = pending_pipeline.take() {
                        left.id = pending.seal(arena, left.span);
                    }
                    self.bump();
                    self.skip_newlines();
                    let (pattern, pattern_span) = self.parse_pattern_test_arena_only(arena)?;
                    let span = self.span(left.span.start(), pattern_span.end());
                    let inner_span = arena.expr_span(left.id);
                    let grouped = left.span.start() < inner_span.start()
                        && left.span.end() > inner_span.end();
                    if !grouped
                        && matches!(
                            arena.expr_kind(left.id),
                            ArenaExprKind::ComparisonChain(_)
                                | ArenaExprKind::Binary {
                                    op: BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge,
                                    ..
                                }
                        )
                    {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "group ordering comparisons explicitly when mixing pattern tests",
                            )
                            .with_code(DiagnosticCode::ParseMixedComparison)
                            .with_label(Label::primary(
                                span,
                                "add parentheses around the intended comparison",
                            )),
                        );
                    }
                    left = ArenaOnlyExpr {
                        id: arena.push_pattern_test_expr(left.id, pattern, span),
                        span,
                        bare_ident: None,
                    };
                    continue;
                }
                let unsupported_integer_division = self.report_unsupported_integer_division();
                let (op, prec, tokens) = if let Some(tokens) = unsupported_integer_division {
                    (
                        BinaryOp::Div,
                        grammar::binary_precedence(BinaryOp::Div),
                        tokens,
                    )
                } else if let Some((op, prec, tokens)) = self.current_binary_op() {
                    (op, prec, tokens)
                } else if let Some((op, tokens)) = self.report_unsupported_boolean_operator() {
                    (op, grammar::binary_precedence(op), tokens)
                } else {
                    break;
                };
                if prec < min_prec {
                    break;
                }
                for _ in 0..tokens {
                    self.bump();
                }
                let right_min_prec = grammar::binary_right_operand_precedence(op);
                self.skip_newlines();
                let right = self.parse_precedence_arena_only(right_min_prec, arena)?;
                let span = self.span(left.span.start(), right.span.end());
                if grouping::is_comparison(op) {
                    for operand in [left, right] {
                        let inner_span = arena.expr_span(operand.id);
                        let grouped = operand.span.start() < inner_span.start()
                            && operand.span.end() > inner_span.end();
                        if !grouped
                            && grouping::comparison_family(&arena.expr_kind(operand.id))
                                .is_some_and(|inner_ordering| {
                                    inner_ordering != grouping::is_ordering(op)
                                })
                        {
                            self.diagnostics.push(Diagnostic::error("group ordering comparisons explicitly when mixing equality, membership, or pattern tests")
                                .with_code(DiagnosticCode::ParseMixedComparison)
                                .with_label(Label::primary(span, "add parentheses around the intended comparison")));
                        }
                    }
                }
                let mut id = arena.push_binary_expr(op, left.id, right.id, span);
                if grouping::is_ordering(op) {
                    let mut pairs = vec![id];
                    let mut previous = right;
                    loop {
                        if self.current_binary_op().is_none()
                            && self.continuation_binary_op().is_some()
                        {
                            self.skip_line_breaks();
                        }
                        let Some((next_op, next_prec, next_tokens)) = self.current_binary_op()
                        else {
                            break;
                        };
                        if next_prec != prec || !grouping::is_ordering(next_op) {
                            break;
                        }
                        for _ in 0..next_tokens {
                            self.bump();
                        }
                        self.skip_newlines();
                        let next = self.parse_precedence_arena_only(prec + 1, arena)?;
                        let pair_span = self.span(previous.span.start(), next.span.end());
                        pairs.push(arena.push_binary_expr(
                            next_op,
                            previous.id,
                            next.id,
                            pair_span,
                        ));
                        previous = next;
                    }
                    if pairs.len() > 1 {
                        id = arena.push_comparison_chain_expr(
                            &pairs,
                            self.span(left.span.start(), previous.span.end()),
                        );
                    }
                }
                left = ArenaOnlyExpr {
                    id,
                    span: self.span(span.start(), self.previous_end()),
                    bare_ident: None,
                };
            }
        }
        if let Some(pending) = pending_pipeline.take() {
            left.id = pending.seal(arena, left.span);
        }
        Some(left)
    }

    fn parse_prefix_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let start = self.current_start();
        if self.consume(TokenKindMatch::Bang).is_some() {
            let expr = self.parse_precedence_arena_only(grammar::PREFIX_OPERAND, arena)?;
            let span = self.span(start, expr.span.end());
            let id = arena.push_unary_expr(UnaryOp::Not, expr.id, span);
            return Some(ArenaOnlyExpr {
                id,
                span,
                bare_ident: None,
            });
        }
        if self.consume(TokenKindMatch::Minus).is_some() {
            let expr = self.parse_precedence_arena_only(grammar::PREFIX_OPERAND, arena)?;
            let span = self.span(start, expr.span.end());
            let id = arena.push_unary_expr(UnaryOp::Neg, expr.id, span);
            return Some(ArenaOnlyExpr {
                id,
                span,
                bare_ident: None,
            });
        }
        self.parse_primary_arena_only(arena)
    }

    fn brace_starts_parameter_block(&self) -> bool {
        let mut offset = 1;
        while matches!(
            self.peek_tag(offset),
            Some(TokenTag::Newline | TokenTag::Comment)
        ) {
            offset += 1;
        }
        self.peek_tag(offset) == Some(TokenTag::Pipe)
    }

    pub(super) fn lookahead_is_context_scope(&self) -> bool {
        if !self
            .current_name()
            .is_some_and(|name| name == "cd" || name == "env")
            || self.peek_tag(1) != Some(TokenTag::LParen)
        {
            return false;
        }
        let mut depth = 0;
        let mut offset = 1;
        while let Some(tag) = self.peek_tag(offset) {
            match tag {
                TokenTag::LParen => depth += 1,
                TokenTag::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        offset += 1;
                        while matches!(
                            self.peek_tag(offset),
                            Some(TokenTag::Newline | TokenTag::Comment)
                        ) {
                            offset += 1;
                        }
                        return self.peek_tag(offset) == Some(TokenTag::LBrace);
                    }
                }
                TokenTag::Eof => return false,
                _ => {}
            }
            offset += 1;
        }
        false
    }

    /// `tempdir NAME {` on one line. `tempdir` is contextual: it starts a scope
    /// only before a binder name and a block.
    pub(super) fn lookahead_is_tempdir_scope(&self) -> bool {
        self.current_tag() == TokenTag::Ident
            && self.current_name().is_some_and(|name| name == "tempdir")
            && self.peek_tag(1) == Some(TokenTag::Ident)
            && self.peek_tag(2) == Some(TokenTag::LBrace)
    }

    pub(super) fn parse_tempdir_scope_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        value_body: bool,
    ) -> Option<ArenaOnlyExpr> {
        let start = self.current_start();
        self.bump();
        let name_start = self.current_start();
        let name = self.expect_ident("expected a directory name after `tempdir`")?;
        let bound = crate::syntax::node::BlockParam {
            name,
            span: self.span(name_start, self.previous_end()),
        };
        let block = self.parse_block_with_params_arena_only(arena, Some(bound))?;
        let span = self.span(start, self.previous_end());
        Some(ArenaOnlyExpr {
            id: arena.push_tempdir_scope_expr(block, value_body, span),
            span,
            bare_ident: None,
        })
    }

    pub(super) fn parse_context_scope_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        value_body: bool,
    ) -> Option<ArenaOnlyExpr> {
        let start = self.current_start();
        let kind = if self.current_name()? == "cd" {
            crate::syntax::arena::ContextScopeKind::Cwd
        } else {
            crate::syntax::arena::ContextScopeKind::Env
        };
        self.bump();
        self.expect(
            TokenKindMatch::LParen,
            "expected `(` before scoped context input",
        )?;
        let input = self.in_nested_group(|parser| parser.parse_expr_id_arena_only(arena))?;
        self.expect(
            TokenKindMatch::RParen,
            "expected `)` after scoped context input",
        )?;
        self.skip_separators();
        let block = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        Some(ArenaOnlyExpr {
            id: arena.push_context_scope_expr(kind, input, block, value_body, span),
            span,
            bare_ident: None,
        })
    }

    /// A primary expression that a keyword begins, as the grammar's primary
    /// keyword table dispatches it.
    fn parse_keyword_primary_arena_only(
        &mut self,
        form: PrimaryForm,
        span: Span,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        match form {
            PrimaryForm::Null => {
                self.bump();
                Some(ArenaOnlyExpr {
                    id: arena.push_null_expr(span),
                    span,
                    bare_ident: None,
                })
            }
            PrimaryForm::True => {
                self.bump();
                Some(ArenaOnlyExpr {
                    id: arena.push_bool_expr(true, span),
                    span,
                    bare_ident: None,
                })
            }
            PrimaryForm::False => {
                self.bump();
                Some(ArenaOnlyExpr {
                    id: arena.push_bool_expr(false, span),
                    span,
                    bare_ident: None,
                })
            }
            PrimaryForm::If => self.parse_if_expr_arena_only(span.start(), arena),
            PrimaryForm::Match => self.parse_match_expr_arena_only(span.start(), arena),
            PrimaryForm::Loop => {
                let start = span.start();
                self.bump();
                let block_id = self.parse_block_arena_only(arena)?;
                let span = self.span(start, self.previous_end());
                Some(ArenaOnlyExpr {
                    id: arena.push_loop_expr(block_id, span),
                    span,
                    bare_ident: None,
                })
            }
            PrimaryForm::Try => {
                self.bump();
                // `try run...` is the run form with its `Result` as the value.
                if self.at_keyword(Keyword::Run) {
                    let (run_id, _run_span) = self.parse_run_form_arena_only(arena)?;
                    if arena.run_form_propagates(run_id) || self.at(TokenKindMatch::Question) {
                        self.diagnostic_here(
                            "`try` keeps the run form's failure as a value, and `?` would propagate it; write one of them",
                            DiagnosticCode::ParseExpectedToken,
                        );
                        return None;
                    }
                    arena.set_run_form_captured(run_id);
                    let span = self.span(span.start(), self.previous_end());
                    return Some(ArenaOnlyExpr {
                        id: arena.push_run_expr_id(run_id, span),
                        span,
                        bare_ident: None,
                    });
                }
                let block = self.parse_block_arena_only(arena)?;
                let span = self.span(span.start(), self.previous_end());
                Some(ArenaOnlyExpr {
                    id: arena.push_capture_expr(block, span),
                    span,
                    bare_ident: None,
                })
            }
            PrimaryForm::Retry => self.parse_retry_expr_arena_only(span.start(), arena),
            PrimaryForm::Run => {
                let (run_id, _run_span) = self.parse_run_form_arena_only(arena)?;
                let span = self.span(span.start(), self.previous_end());
                Some(self.run_value_arena_only(run_id, span, arena))
            }
            PrimaryForm::Spawn => self.parse_spawn_expr_arena_only(span.start(), arena),
            PrimaryForm::Wait => self.parse_wait_expr_arena_only(span.start(), arena),
        }
    }

    pub(super) fn parse_primary_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let span = self.current_span();
        if self.current_tag() == TokenTag::Ident && self.lookahead_is_context_scope() {
            return self.parse_context_scope_arena_only(arena, true);
        }
        if self.lookahead_is_tempdir_scope() {
            return self.parse_tempdir_scope_arena_only(arena, true);
        }
        if let Some(form) = self.current_keyword().and_then(grammar::primary_form) {
            return self.parse_keyword_primary_arena_only(form, span, arena);
        }
        match (self.current_tag(), self.current_keyword()) {
            (TokenTag::Ident | TokenTag::ProcIdent, _) if self.lookahead_is_ctx_block() => {
                let start = self.current_start();
                self.bump();
                let previous = self.condition_expr;
                self.condition_expr = true;
                let message = self.parse_precedence_arena_only(0, arena);
                self.condition_expr = previous;
                let message = message?;
                let block = self.parse_block_arena_only(arena)?;
                let span = self.span(start, self.previous_end());
                Some(ArenaOnlyExpr {
                    id: arena.push_error_context_expr(message.id, block, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Ident, _) => {
                let name = self
                    .current_name()
                    .expect("identifier token has name payload");
                let span = self.bump();
                Some(ArenaOnlyExpr {
                    id: arena.push_ident_expr(name, span),
                    span,
                    bare_ident: Some(name),
                })
            }
            (TokenTag::Int, _) => {
                let span = self.bump();
                let value = IntLiteral::from_text(self.span_text(span));
                Some(ArenaOnlyExpr {
                    id: arena.push_int_expr(&value, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Float, _) => {
                let span = self.bump();
                let value = FloatLiteral::from_text(self.span_text(span));
                Some(ArenaOnlyExpr {
                    id: arena.push_float_expr(&value, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Duration, _) => {
                let span = self.bump();
                let value = DurationLiteral::from_text(self.span_text(span));
                Some(ArenaOnlyExpr {
                    id: arena.push_duration_expr(&value, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::String, _) => {
                let flags = self
                    .token_table
                    .string_flags_at(self.index)
                    .expect("string token has flags payload");
                let span = self.bump();
                if flags.has_interpolation {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "expression string literals do not interpolate; use raw strings for literal `$` or formatted strings for interpolation",
                        )
                        .with_code(DiagnosticCode::ParseExprStringInterpolation)
                        .with_label(Label::primary(
                            span,
                            "interpolation is only valid in command words",
                        ))
                        .with_note(
                            "use `r\"\"\"...\"\"\"` for literal `$` characters, or `f\"\"\"...\"\"\"` for intentional interpolation",
                        ),
                    );
                }
                let value: Arc<str> = self.decoded_quoted_text(span, flags.raw_literal);
                Some(ArenaOnlyExpr {
                    id: arena.push_str_expr(&value, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Regex, _) => {
                let span = self.bump();
                let pattern = self.decoded_quoted_text(span, true);
                Some(ArenaOnlyExpr {
                    id: arena.push_regex_expr(&pattern, self.span_text(span), span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::PathString, _) => {
                let span = self.bump();
                self.reject_path_string_interpolation(span);
                let value: Arc<str> = self.decoded_quoted_text(span, false);
                Some(ArenaOnlyExpr {
                    id: arena.push_path_str_expr(&value, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::GlobString, _) => {
                let span = self.bump();
                let value: Arc<str> = self.decoded_quoted_text(span, false);
                Some(ArenaOnlyExpr {
                    id: arena.push_glob_str_expr(&value, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::EnvString, _) => {
                let span = self.bump();
                let name = self.env_string_name(span);
                Some(ArenaOnlyExpr {
                    id: arena.push_env_string_expr(name, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::FmtString, _) => {
                let flags = self
                    .token_table
                    .string_flags_at(self.index)
                    .expect("formatted string token has flags payload");
                let span = self.bump();
                let parts = self.fmt_string_parts_arena_only(arena, span, flags.raw_literal);
                Some(ArenaOnlyExpr {
                    id: arena.push_fmt_string_expr(parts, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::PathFmtString, _) => {
                let span = self.bump();
                let parts = self.fmt_string_parts_arena_only(arena, span, false);
                Some(ArenaOnlyExpr {
                    id: arena.push_path_fmt_string_expr(parts, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Bytes, _) => {
                let span = self.bump();
                let raw = self.quoted_content(span);
                let (bytes, diagnostics) =
                    decode_bytes_literal_for(self.source_id, raw, self.string_content_offset(span));
                self.diagnostics.extend(diagnostics);
                Some(ArenaOnlyExpr {
                    id: arena.push_bytes_expr(&bytes, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Slash, _) => self.parse_bare_path_arena_only(arena),
            (TokenTag::Dot, _) if self.starts_bare_path_literal() => {
                self.parse_bare_path_arena_only(arena)
            }
            (TokenTag::Dot, _) if !self.starts_bare_path_literal() => {
                self.bump();
                let item_id = arena.push_item_expr(span);
                if self.current_member_name().is_some() {
                    let name = self.expect_member_name("expected field name after `.`")?;
                    let span = self.span(span.start(), self.previous_end());
                    Some(ArenaOnlyExpr {
                        id: arena.push_field_expr(item_id, name, span),
                        span,
                        bare_ident: None,
                    })
                } else {
                    Some(ArenaOnlyExpr {
                        id: item_id,
                        span,
                        bare_ident: None,
                    })
                }
            }
            (TokenTag::LastStatus, _) => {
                self.bump();
                Some(ArenaOnlyExpr {
                    id: arena.push_last_status_expr(span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::LBracket, _) => self.parse_list_arena_only(arena),
            (TokenTag::LBrace, _) if self.brace_starts_parameter_block() => {
                let block = self.parse_block_arena_only(arena)?;
                let span = self.span(span.start(), self.previous_end());
                if arena.block_parameter_count(block) == 0 {
                    self.diagnostics.push(
                        Diagnostic::error("error fallback block requires an error parameter")
                            .with_code(DiagnosticCode::ParseFallbackBlockParams)
                            .with_label(Label::primary(
                                span,
                                "use one name or `_` between the pipes",
                            )),
                    );
                }
                Some(ArenaOnlyExpr {
                    id: arena.push_value_block_expr(block, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::LBrace, _) if self.brace_starts_record_value() => {
                self.parse_record_arena_only(arena)
            }
            (TokenTag::LBrace, _) => self
                .parse_braced_value_expr_arena_only("lexical block", arena)
                .map(|(expr, _)| expr),
            (TokenTag::LParen, _) => {
                self.bump();
                self.skip_newlines();
                // A grouped expression owns its closing delimiter, including run argv.
                self.parenthesized_expr_depth += 1;
                let expr =
                    self.in_nested_group(|parser| parser.parse_precedence_arena_only(0, arena));
                self.parenthesized_expr_depth -= 1;
                let expr = expr?;
                self.skip_newlines();
                self.expect(TokenKindMatch::RParen, "expected `)` after expression");
                let span = self.span(span.start(), self.previous_end());
                arena.record_paren_group(expr.id, span);
                Some(ArenaOnlyExpr {
                    span,
                    bare_ident: None,
                    ..expr
                })
            }
            (TokenTag::DollarIdent, _) => {
                let name = self
                    .current_name()
                    .expect("dollar identifier token has name payload");
                let span = self.bump();
                self.diagnostics.push(
                    Diagnostic::error(
                        "`$name` is command-word syntax; in expression context, use `name` directly",
                    )
                    .with_code(DiagnosticCode::ParseExpectedExpression)
                    .with_label(Label::primary(
                        span,
                        format!("use `{name}` here, not `${name}`"),
                    )),
                );
                None
            }
            (TokenTag::DollarLBrace, _) => {
                self.bump();
                self.diagnostics.push(
                    Diagnostic::error(
                        "`${...}` is command-word syntax; in expression context, use the expression directly",
                    )
                    .with_code(DiagnosticCode::ParseExpectedExpression)
                    .with_label(Label::primary(
                        span,
                        "remove `$` and braces in expression context",
                    )),
                );
                None
            }
            _ => {
                self.diagnostic_here(
                    "expected expression",
                    DiagnosticCode::ParseExpectedExpression,
                );
                None
            }
        }
    }

    fn parse_bare_path_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let start = self.current_start();
        let Some(end) = literal::scan_bare_path_at(self.source, start) else {
            self.diagnostic_here(
                "expected path literal",
                DiagnosticCode::ParseExpectedExpression,
            );
            return None;
        };
        let value: Arc<str> = self.source[start..end].into();
        while !self.at(TokenKindMatch::Eof) && self.current_end() <= end {
            self.bump();
        }
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_path_str_expr(&value, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_list_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        self.in_nested_group(|parser| parser.parse_list_inner(arena))
    }

    fn parse_list_inner(&mut self, arena: &mut ArenaProgramBuilder<'_>) -> Option<ArenaOnlyExpr> {
        let start = self.current_start();
        self.bump();
        self.skip_comp_layout();
        if self.at(TokenKindMatch::RBracket) || self.at(TokenKindMatch::Eof) {
            let end = self
                .expect(TokenKindMatch::RBracket, "expected `]` after list")
                .map(|span| span.end())
                .unwrap_or_else(|| self.previous_end());
            let span = self.span(start, end);
            return Some(ArenaOnlyExpr {
                id: arena.push_list_expr_range(ArenaRange::default(), span),
                span,
                bare_ident: None,
            });
        }
        let first_start = self.current_start();
        let first_splice = self.consume(TokenKindMatch::At).is_some();
        self.skip_comp_layout();
        let first = self.parse_precedence_arena_only(0, arena)?;
        self.skip_comp_layout();
        if self.at_keyword(Keyword::For) && !first_splice {
            return self.parse_list_comp_arena_only(arena, start, first.id);
        }
        arena.begin_list_elements();
        arena.push_list_element_input(ArenaListElementInput {
            value: first.id,
            splice_span: first_splice.then(|| self.span(first_start, first.span.end())),
        });
        while self.consume(TokenKindMatch::Comma).is_some() {
            self.skip_comp_layout();
            if self.at(TokenKindMatch::RBracket) || self.at(TokenKindMatch::Eof) {
                break;
            }
            let item_start = self.current_start();
            let splice = self.consume(TokenKindMatch::At).is_some();
            self.skip_comp_layout();
            let Some(item) = self.parse_precedence_arena_only(0, arena) else {
                arena.discard_list_elements();
                return None;
            };
            arena.push_list_element_input(ArenaListElementInput {
                value: item.id,
                splice_span: splice.then(|| self.span(item_start, item.span.end())),
            });
            self.skip_comp_layout();
        }
        self.skip_comp_layout();
        let end = self
            .expect(TokenKindMatch::RBracket, "expected `]` after list")
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        let items = arena.finish_list_elements();
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_list_elements(items, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_list_comp_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        start: usize,
        expr: ExprId,
    ) -> Option<ArenaOnlyExpr> {
        let qualifiers = self.parse_comp_qualifiers_arena_only(arena)?;
        let end = self
            .expect(
                TokenKindMatch::RBracket,
                "expected `]` after list comprehension",
            )
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_list_comp_expr(expr, qualifiers, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_record_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        self.in_nested_group(|parser| parser.parse_record_inner(arena))
    }

    fn parse_record_inner(&mut self, arena: &mut ArenaProgramBuilder<'_>) -> Option<ArenaOnlyExpr> {
        let start = self.current_start();
        self.bump();
        self.skip_comp_layout();
        arena.begin_record_fields();
        // The start of the first entry, once an entry has been read: a
        // comprehension after it would otherwise replace those entries.
        let mut first_entry: Option<usize> = None;
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            let field_start = self.current_start();
            let leading_entries = first_entry.map(|first| {
                let written = self.source[first..field_start]
                    .trim_end()
                    .trim_end_matches(',')
                    .trim_end();
                self.span(first, first + written.len())
            });
            first_entry.get_or_insert(field_start);
            if self.at(TokenKindMatch::Dot)
                && self.peek_tag(1) == Some(TokenTag::Dot)
                && self.peek_tag(2) == Some(TokenTag::Dot)
            {
                self.bump();
                self.bump();
                self.bump();
                let Some(expr) = self.parse_precedence_arena_only(0, arena) else {
                    arena.discard_record_fields();
                    return None;
                };
                arena.push_record_field_input(ArenaRecordFieldInput::Spread {
                    expr: expr.id,
                    span: self.span(field_start, expr.span.end()),
                });
                self.skip_comp_layout();
                if self.consume(TokenKindMatch::Comma).is_none() {
                    break;
                }
                self.skip_comp_layout();
                continue;
            }
            if self.consume(TokenKindMatch::LBracket).is_some() {
                self.skip_comp_layout();
                let Some(key) = self.parse_precedence_arena_only(0, arena) else {
                    arena.discard_record_fields();
                    return None;
                };
                self.skip_comp_layout();
                self.expect(
                    TokenKindMatch::RBracket,
                    "expected `]` after computed map key",
                );
                self.expect(TokenKindMatch::Colon, "expected `:` after computed map key");
                let Some(value) = self.parse_precedence_arena_only(0, arena) else {
                    arena.discard_record_fields();
                    return None;
                };
                self.skip_comp_layout();
                if self.at_keyword(Keyword::For) {
                    arena.discard_record_fields();
                    self.report_map_comprehension_entries(leading_entries);
                    return self.parse_map_comp_tail_arena_only(arena, start, key.id, value.id);
                }
                arena.push_record_field_input(ArenaRecordFieldInput::Computed {
                    key: key.id,
                    value: value.id,
                    span: self.span(field_start, value.span.end()),
                });
                self.skip_comp_layout();
                if self.consume(TokenKindMatch::Comma).is_none() {
                    break;
                }
                self.skip_comp_layout();
                continue;
            }
            let label_tag = self.current_tag();
            let label_span = self.current_span();
            let name = if label_tag == TokenTag::String {
                let flags = self
                    .token_table
                    .string_flags_at(self.index)
                    .expect("record string key has flags payload");
                let name = Name::intern(self.decoded_quoted_text(label_span, flags.raw_literal));
                self.bump();
                name
            } else {
                let Some(name) = self.expect_label_name("expected record field label") else {
                    arena.discard_record_fields();
                    return None;
                };
                name
            };
            let mut key_id =
                arena.push_ident_expr(name, self.span(field_start, self.previous_end()));
            let mut dotted_key = false;
            let mut path = vec![name];
            while self.consume(TokenKindMatch::Dot).is_some() {
                dotted_key = true;
                let Some(field_name) = self.expect_label_name("expected field name") else {
                    break;
                };
                let end = self.previous_end();
                path.push(field_name);
                key_id = arena.push_field_expr(key_id, field_name, self.span(field_start, end));
            }
            if self.consume(TokenKindMatch::Colon).is_some() {
                let Some(value) = self.parse_precedence_arena_only(0, arena) else {
                    arena.discard_record_fields();
                    return None;
                };
                self.skip_comp_layout();
                if self.at_keyword(Keyword::For) {
                    arena.discard_record_fields();
                    self.report_map_comprehension_entries(leading_entries);
                    return self.parse_map_comp_tail_arena_only(arena, start, key_id, value.id);
                }
                if dotted_key {
                    arena.push_record_field_input(ArenaRecordFieldInput::Path {
                        path,
                        value: value.id,
                        span: self.span(field_start, value.span.end()),
                    });
                } else {
                    arena.push_record_field_input(ArenaRecordFieldInput::Named {
                        name,
                        value: value.id,
                        span: self.span(field_start, value.span.end()),
                    });
                }
            } else {
                if dotted_key {
                    self.diagnostic_here(
                        "dotted update paths require `:` and a replacement value",
                        DiagnosticCode::ParseExpectedRecordUpdateValue,
                    );
                    break;
                }
                if !self.require_label_binding_name(label_tag, label_span) {
                    arena.discard_record_fields();
                    return None;
                }
                arena.push_record_field_input(ArenaRecordFieldInput::Shorthand {
                    name,
                    span: self.span(field_start, self.previous_end()),
                });
            }
            self.skip_comp_layout();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_comp_layout();
        }
        self.skip_comp_layout();
        let end = self
            .expect(TokenKindMatch::RBrace, "expected `}` after record")
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        let fields = arena.finish_record_fields();
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_record_expr(fields, span),
            span,
            bare_ident: None,
        })
    }

    /// A map comprehension is the only entry of its braces. Entries before
    /// it are an error rather than silently dropped; the comprehension still
    /// parses so later diagnostics stay accurate.
    fn report_map_comprehension_entries(&mut self, leading_entries: Option<Span>) {
        let Some(leading_entries) = leading_entries else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::error("a map comprehension must be the only entry in its braces")
                .with_code(DiagnosticCode::ParseMapComprehensionEntries)
                .with_label(Label::primary(leading_entries, "these entries come before the comprehension"))
                .with_label(Label::secondary(self.current_span(), "its `for` clause"))
                .with_note("build the comprehension on its own and add the other entries with `.set(key, value)`"),
        );
    }

    fn parse_map_comp_tail_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        start: usize,
        key: ExprId,
        value: ExprId,
    ) -> Option<ArenaOnlyExpr> {
        let qualifiers = self.parse_comp_qualifiers_arena_only(arena)?;
        let end = self
            .expect(
                TokenKindMatch::RBrace,
                "expected `}` after map comprehension",
            )
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_map_comp_expr(key, value, qualifiers, span),
            span,
            bare_ident: None,
        })
    }

    fn skip_call_argument_trivia(&mut self) {
        while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
            self.bump();
        }
    }

    fn skip_comp_layout(&mut self) {
        while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
            self.bump();
        }
    }

    fn parse_comp_qualifiers_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaRange> {
        let mut qualifiers = Vec::new();
        loop {
            self.skip_comp_layout();
            let start = self.current_start();
            if self.consume_keyword(Keyword::For).is_some() {
                let target = self.parse_binding_target_arena_only(
                    "expected binding target in comprehension",
                    arena,
                )?;
                self.expect_keyword(Keyword::In, "expected `in` in comprehension");
                let iter = self.parse_expr_id_arena_only(arena)?;
                qualifiers.push(ArenaCompQualifier::For {
                    target,
                    iter,
                    span: self.span(start, self.previous_end()),
                });
            } else if self.consume_keyword(Keyword::If).is_some() {
                let condition = self.parse_expr_id_arena_only(arena)?;
                qualifiers.push(ArenaCompQualifier::If {
                    condition,
                    span: self.span(start, self.previous_end()),
                });
            } else {
                break;
            }
        }
        Some(arena.push_comp_qualifiers(qualifiers))
    }

    pub(super) fn parse_call_args_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> crate::syntax::arena::ArenaRange {
        self.in_nested_group(|parser| parser.parse_call_args_inner(arena))
    }

    fn parse_call_args_inner(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> crate::syntax::arena::ArenaRange {
        arena.begin_call_args();
        self.skip_call_argument_trivia();
        while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
            if self.at(TokenKindMatch::Dot)
                && self.peek_tag(1) == Some(TokenTag::Dot)
                && self.peek_tag(2) == Some(TokenTag::Dot)
            {
                let start = self.current_start();
                self.bump();
                self.bump();
                self.bump();
                self.skip_call_argument_trivia();
                let Some(value) = self.parse_precedence_arena_only(0, arena) else {
                    break;
                };
                arena.push_call_arg_input(ArenaCallArgInput::NamedSpread {
                    value: value.id,
                    span: self.span(start, value.span.end()),
                });
            } else if self.consume(TokenKindMatch::At).is_some() {
                let start = self.previous_end().saturating_sub(1);
                let Some(value) = self.parse_precedence_arena_only(0, arena) else {
                    break;
                };
                arena.push_call_arg_input(ArenaCallArgInput::Splice {
                    value: value.id,
                    span: self.span(start, value.span.end()),
                });
            } else {
                let named = self.current_label_name().is_some()
                    && self.peek_tag(1) == Some(TokenTag::Colon);
                if named {
                    let start = self.current_start();
                    let name_span = self.current_span();
                    let label_tag = self.current_tag();
                    let name = self.expect_label_name("expected named argument").unwrap();
                    self.bump();
                    let colon_end = self.previous_end();
                    self.skip_call_argument_trivia();
                    let (value, end) =
                        if self.at(TokenKindMatch::Comma) || self.at(TokenKindMatch::RParen) {
                            if !self.require_label_binding_name(label_tag, name_span) {
                                break;
                            }
                            // The implied value is an ordinary lexical identifier; its
                            // span stays on the written name for resolution diagnostics.
                            (arena.push_ident_expr(name, name_span), colon_end)
                        } else {
                            let Some(value) = self.parse_precedence_arena_only(0, arena) else {
                                break;
                            };
                            (value.id, self.previous_end())
                        };
                    arena.push_call_arg_input(ArenaCallArgInput::Named {
                        name,
                        value,
                        span: self.span(start, end),
                    });
                } else if let Some(expr) = self.parse_precedence_arena_only(0, arena) {
                    arena.push_call_arg_input(ArenaCallArgInput::Positional(expr.id));
                } else {
                    break;
                }
            }
            self.skip_call_argument_trivia();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_call_argument_trivia();
        }
        arena.finish_call_args()
    }

    fn parse_pipe_stage_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<(ArenaPipeStageKind, Span)> {
        if self.pipe_stage_is_value_expr() {
            let start = self.current_start();
            let expr_id =
                self.with_pipe_boundary(|parser| parser.parse_expr_id_arena_only(arena))?;
            let span = self.span(start, self.previous_end());
            if let Err(hole_span) = arena.value_pipeline_hole(expr_id) {
                self.diagnostics.push(Diagnostic::new(super::Severity::Error, "a value pipeline call requires exactly one whole argument placeholder")
                    .with_code(DiagnosticCode::ParsePipelineHole)
                    .with_label(Label::primary(hole_span, "place `_` as one positional argument or named argument value of the immediate call")));
            }
            return Some((ArenaPipeStageKind::Expr(expr_id), span));
        }
        let start = self.current_start();
        let stage = self.parse_stream_stage_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        Some((ArenaPipeStageKind::Stream(stage), span))
    }

    fn parse_stream_stage_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaStreamStage> {
        let start = self.current_start();
        let name = self.expect_stage_name()?;
        let member = if grammar::is_stream_stage_namespace(&name.as_str())
            && self.consume(TokenKindMatch::Dot).is_some()
        {
            Some(self.expect_member_name("expected stream stage method")?)
        } else {
            None
        };
        let stage = grammar::stream_stage_named(
            &name.as_str(),
            member.map(|member| member.as_str()).as_deref(),
        )
        .unwrap_or_else(|| {
            self.diagnostic_previous(
                "unknown stream stage",
                DiagnosticCode::ParseUnknownStreamStage,
            );
            grammar::stream_stage(StreamStageKind::Map)
        });
        let kind = stage.kind;
        let mut args = self.parse_legacy_stream_stage_flags_arena_only(arena)?;
        if self.consume(TokenKindMatch::LParen).is_some() {
            args = self.parse_call_args_arena_only(arena);
            self.expect(TokenKindMatch::RParen, "expected `)` after stage arguments");
        }
        let block = if self.at(TokenKindMatch::LBrace) && stage.block {
            Some(self.parse_block_arena_only(arena)?)
        } else if stage.inline && !self.at_pipe_stage_end() {
            Some(self.parse_inline_stream_block_arena_only(arena)?)
        } else {
            None
        };
        let end = self.previous_end();
        let span = self.span(start, end);
        Some(arena.build_stream_stage(kind, block, args, span))
    }

    fn parse_inline_stream_block_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<BlockId> {
        let start = self.current_start();
        let expr_id = self.with_pipe_boundary(|parser| parser.parse_expr_id_arena_only(arena))?;
        let span = self.span(start, self.previous_end());
        arena.begin_block();
        arena.push_expr_statement(expr_id, span);
        Some(arena.finish_block(&[], span))
    }

    fn parse_legacy_stream_stage_flags_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaRange> {
        arena.begin_call_args();
        let migration_start = self.current_start();
        let mut named_arguments = Vec::new();
        let mut migration_end = migration_start;
        while self.at(TokenKindMatch::Minus) && self.peek_tag(1) == Some(TokenTag::Minus) {
            let start = self.current_start();
            self.bump();
            self.bump();
            let Some(name) = self.expect_stream_option_name() else {
                break;
            };
            let mut value_text = "true".to_string();
            let value = if self.consume(TokenKindMatch::Equals).is_some() {
                if self.consume(TokenKindMatch::DollarLBrace).is_some() {
                    let value_start = self.current_start();
                    let Some(value) =
                        self.in_nested_group(|parser| parser.parse_expr_id_arena_only(arena))
                    else {
                        let _ = arena.finish_call_args();
                        return None;
                    };
                    value_text = self.source[value_start..self.previous_end()].to_string();
                    self.expect(
                        TokenKindMatch::RBrace,
                        "expected `}` after option interpolation",
                    );
                    Some(value)
                } else {
                    let value_start = self.current_start();
                    let Some(value) = self.parse_legacy_stream_option_expr_arena_only(arena) else {
                        let _ = arena.finish_call_args();
                        return None;
                    };
                    value_text = self.source[value_start..self.previous_end()].to_string();
                    Some(value)
                }
            } else {
                None
            };
            let end = self.previous_end();
            let value = value.unwrap_or_else(|| arena.push_bool_expr(true, self.span(start, end)));
            arena.push_call_arg_input(ArenaCallArgInput::Named {
                name: Name::intern(name.as_str().replace('-', "_").as_str()),
                value,
                span: self.span(start, end),
            });
            named_arguments.push(format!("{}: {value_text}", name.as_str().replace('-', "_")));
            migration_end = end;
        }
        if !named_arguments.is_empty() {
            let span = self.span(migration_start, migration_end);
            let mut diagnostic =
                Diagnostic::error("structured stream options use ordinary named arguments")
                    .with_code(DiagnosticCode::ParseStreamOptionMigration)
                    .with_label(Label::primary(
                        span,
                        "replace stage flags with named arguments",
                    ));
            if !self.source[span.range()].contains('#') && !self.at(TokenKindMatch::LParen) {
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    "use named stage arguments",
                    format!("({})", named_arguments.join(", ")),
                ));
            }
            self.diagnostics.push(diagnostic);
        }
        Some(arena.finish_call_args())
    }

    // Only migration recovery reads expressions outside an ordinary argument list.
    fn parse_legacy_stream_option_expr_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ExprId> {
        let mut left = self.parse_prefix_arena_only(arena)?;
        loop {
            left = if self.consume(TokenKindMatch::Question).is_some() {
                let span = self.span(left.span.start(), self.previous_end());
                ArenaOnlyExpr {
                    id: arena.push_try_expr(left.id, span),
                    span,
                    bare_ident: None,
                }
            } else if self.at(TokenKindMatch::Dot) && self.peek_tag(1) != Some(TokenTag::Dot) {
                self.bump();
                let name = self.expect_member_name("expected field name after `.`")?;
                let span = self.span(left.span.start(), self.previous_end());
                ArenaOnlyExpr {
                    id: arena.push_field_expr(left.id, name, span),
                    span,
                    bare_ident: None,
                }
            } else if self.consume(TokenKindMatch::LBracket).is_some() {
                let (start, end) = if self.consume_dot_dot() {
                    let end = if self.at(TokenKindMatch::RBracket) {
                        None
                    } else {
                        Some(self.parse_expr_id_arena_only(arena)?)
                    };
                    (None, end)
                } else {
                    let first = self.parse_expr_id_arena_only(arena)?;
                    if self.consume_dot_dot() {
                        let end = if self.at(TokenKindMatch::RBracket) {
                            None
                        } else {
                            Some(self.parse_expr_id_arena_only(arena)?)
                        };
                        (Some(first), end)
                    } else {
                        self.expect(
                            TokenKindMatch::RBracket,
                            "expected `]` after index expression",
                        );
                        let span = self.span(left.span.start(), self.previous_end());
                        left = ArenaOnlyExpr {
                            id: arena.push_index_expr(left.id, first, span),
                            span,
                            bare_ident: None,
                        };
                        continue;
                    }
                };
                self.expect(
                    TokenKindMatch::RBracket,
                    "expected `]` after index expression",
                );
                let span = self.span(left.span.start(), self.previous_end());
                ArenaOnlyExpr {
                    id: arena.push_slice_expr(left.id, start, end, span),
                    span,
                    bare_ident: None,
                }
            } else if self.consume(TokenKindMatch::LParen).is_some() {
                let args = self.parse_call_args_arena_only(arena);
                self.expect(TokenKindMatch::RParen, "expected `)` after call arguments");
                let span = self.span(left.span.start(), self.previous_end());
                ArenaOnlyExpr {
                    id: arena.push_call_expr(left.id, args, span),
                    span,
                    bare_ident: None,
                }
            } else {
                break;
            };
        }
        Some(left.id)
    }

    pub(super) fn pipe_stage_is_value_expr(&self) -> bool {
        let Some(name) = self.stage_name_at(self.index) else {
            return true;
        };
        if let Some((namespace, member, _)) = self.dotted_stage_name_at(self.index) {
            return grammar::stream_stage_named(&namespace.as_str(), Some(&member.as_str()))
                .is_none();
        }
        grammar::stream_stage_named(&name.as_str(), None).is_none()
    }

    pub(super) fn stage_name_at(&self, index: usize) -> Option<Name> {
        matches!(
            self.token_table.tag_at(index)?,
            TokenTag::Ident | TokenTag::ProcIdent
        )
        .then(|| self.token_table.name_at(index))
        .flatten()
    }

    pub(super) fn dotted_stage_name_at(&self, index: usize) -> Option<(Name, Name, usize)> {
        let namespace = self.stage_name_at(index)?;
        if self.token_table.tag_at(index + 1) != Some(TokenTag::Dot)
            || self.start_at(index + 1)? != self.end_at(index)?
        {
            return None;
        }
        if self.start_at(index + 2)? != self.end_at(index + 1)? {
            return None;
        }
        let member = match self.token_table.tag_at(index + 2)? {
            TokenTag::Ident | TokenTag::ProcIdent => self.token_table.name_at(index + 2)?,
            TokenTag::Keyword => Name::intern(self.token_table.keyword_at(index + 2)?.as_str()),
            _ => return None,
        };
        Some((namespace, member, index + 3))
    }

    pub(super) fn with_pipe_boundary<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Option<T> {
        let previous = self.pipe_is_boundary;
        self.pipe_is_boundary = true;
        let result = f(self);
        self.pipe_is_boundary = previous;
        result
    }

    pub(super) fn with_command_arg_expr<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Option<T> {
        let previous_trailing_try = self.trailing_statement_try;
        let previous_command_arg_expr = self.command_arg_expr;
        self.trailing_statement_try = false;
        self.command_arg_expr = true;
        let result = f(self);
        self.command_arg_expr = previous_command_arg_expr;
        self.trailing_statement_try = previous_trailing_try;
        result
    }

    pub(super) fn expect_stage_name(&mut self) -> Option<Name> {
        if !matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
            self.diagnostic_here(
                "expected stream stage name",
                DiagnosticCode::ParseExpectedStreamStage,
            );
            return None;
        }
        let name = self
            .current_name()
            .expect("stream stage name token has payload");
        self.bump();
        Some(name)
    }

    pub(super) fn expect_stream_option_name(&mut self) -> Option<Name> {
        if !matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
            self.diagnostic_here(
                "expected stream stage option name",
                DiagnosticCode::ParseExpectedIdent,
            );
            return None;
        }
        let name = self
            .current_name()
            .expect("stream option name token has payload");
        self.bump();
        Some(name)
    }

    fn parse_retry_expr_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        self.bump();
        self.expect(
            TokenKindMatch::LBracket,
            "expected `[` after `retry` for retry delays",
        )?;
        self.skip_newlines();
        arena.begin_expr_ids();
        while !self.at(TokenKindMatch::RBracket) && !self.at(TokenKindMatch::Eof) {
            let Some(delay) =
                self.in_nested_group(|parser| parser.parse_precedence_arena_only(0, arena))
            else {
                arena.discard_expr_ids();
                return None;
            };
            arena.push_expr_id_input(delay.id);
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        if self
            .expect(TokenKindMatch::RBracket, "expected `]` after retry delays")
            .is_none()
        {
            arena.discard_expr_ids();
            return None;
        }
        let pattern = if self.current_name().is_some_and(|name| name == "on") {
            self.bump();
            let parsed = (|| {
                if !self.at(TokenKindMatch::LParen) {
                    self.expect(TokenKindMatch::LParen, "expected `(` after retry `on`")?;
                }
                self.parse_pattern_test_arena_only(arena)
            })();
            let Some((pattern, _)) = parsed else {
                arena.discard_expr_ids();
                return None;
            };
            Some(pattern)
        } else {
            None
        };
        let Some(block_id) = self.parse_block_arena_only(arena) else {
            arena.discard_expr_ids();
            return None;
        };
        let span = self.span(start, self.previous_end());
        let delays = arena.finish_expr_ids();
        Some(ArenaOnlyExpr {
            id: arena.push_retry_expr(delays, pattern, block_id, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_spawn_expr_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        self.bump();
        if self.at_keyword(Keyword::Run) {
            let (run_id, _run_span) = self.parse_run_form_arena_only(arena)?;
            let span = self.span(start, self.previous_end());
            return Some(ArenaOnlyExpr {
                id: arena.push_spawn_run_expr_id(run_id, span, span),
                span,
                bare_ident: None,
            });
        }
        let command = self.parse_postfix_operand_without_try_arena_only(arena)?;
        let span = self.span(start, command.span.end());
        Some(ArenaOnlyExpr {
            id: arena.push_spawn_command_expr(command.id, span, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_wait_expr_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        self.bump();
        let target = self.parse_postfix_operand_without_try_arena_only(arena)?;
        let span = self.span(start, target.span.end());
        Some(ArenaOnlyExpr {
            id: arena.push_wait_expr(target.id, span, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_postfix_operand_without_try_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let mut left = self.parse_prefix_arena_only(arena)?;
        loop {
            self.skip_postfix_newlines();
            if self.at(TokenKindMatch::Dot) && self.peek_tag(1) != Some(TokenTag::Dot) {
                self.bump();
                let name = self.expect_member_name("expected field name after `.`")?;
                let span = self.span(left.span.start(), self.previous_end());
                left = ArenaOnlyExpr {
                    id: arena.push_field_expr(left.id, name, span),
                    span,
                    bare_ident: None,
                };
            } else if self.consume(TokenKindMatch::LBracket).is_some() {
                let (start, end) = if self.consume_dot_dot() {
                    let end = if self.at(TokenKindMatch::RBracket) {
                        None
                    } else {
                        Some(self.parse_precedence_arena_only(0, arena)?.id)
                    };
                    (None, end)
                } else {
                    let first = self.parse_precedence_arena_only(0, arena)?;
                    if self.consume_dot_dot() {
                        let end = if self.at(TokenKindMatch::RBracket) {
                            None
                        } else {
                            Some(self.parse_precedence_arena_only(0, arena)?.id)
                        };
                        (Some(first.id), end)
                    } else {
                        self.expect(
                            TokenKindMatch::RBracket,
                            "expected `]` after index expression",
                        );
                        let span = self.span(left.span.start(), self.previous_end());
                        left = ArenaOnlyExpr {
                            id: arena.push_index_expr(left.id, first.id, span),
                            span,
                            bare_ident: None,
                        };
                        continue;
                    }
                };
                self.expect(
                    TokenKindMatch::RBracket,
                    "expected `]` after index expression",
                );
                let span = self.span(left.span.start(), self.previous_end());
                left = ArenaOnlyExpr {
                    id: arena.push_slice_expr(left.id, start, end, span),
                    span,
                    bare_ident: None,
                };
            } else if self.consume(TokenKindMatch::LParen).is_some() {
                let args = self.parse_call_args_arena_only(arena);
                self.expect(TokenKindMatch::RParen, "expected `)` after call arguments");
                let span = self.span(left.span.start(), self.previous_end());
                left = ArenaOnlyExpr {
                    id: arena.push_call_expr(left.id, args, span),
                    span,
                    bare_ident: None,
                };
            } else {
                break;
            }
        }
        Some(left)
    }
}

fn arena_expr_accepts_builder_block(arena: &ArenaProgramBuilder<'_>, id: ExprId) -> bool {
    match arena.expr_kind(id) {
        ArenaExprKind::Call { callee, .. } => match arena.expr_kind(callee) {
            ArenaExprKind::Field { base, name } => matches!(
                arena.expr_kind(base),
                ArenaExprKind::Ident(module) if grammar::builder_api_accepts_block(&module.as_str(), &name.as_str())
            ),
            _ => false,
        },
        ArenaExprKind::Field { base, name } => matches!(
            arena.expr_kind(base),
            ArenaExprKind::Ident(module) if grammar::builder_api_accepts_block(&module.as_str(), &name.as_str())
        ),
        _ => false,
    }
}
