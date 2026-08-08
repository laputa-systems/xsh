#![allow(clippy::single_call_fn)]

use super::Span;
use super::{
    BinaryOp, Diagnostic, DurationLiteral, FixHint, FloatLiteral, IntLiteral, Keyword, Label, Name,
    Parser,
    StreamStageKind, TokenKindMatch, TokenTag, UnaryOp, decode_bytes_literal_for, literal,
};
use crate::syntax::arena::{
    ArenaCallArgInput, ArenaExprKind, ArenaPipeStage, ArenaPipeStageKind, ArenaProgramBuilder,
    ArenaRange, ArenaRecordFieldInput, ArenaStreamStage, BlockId, ExprId,
};
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
        let Some(condition) = self.parse_precedence_arena_only(0, arena) else {
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
        while self.consume_keyword(Keyword::Else).is_some() {
            if self.consume_keyword(Keyword::If).is_some() {
                let Some(condition) = self.parse_precedence_arena_only(0, arena) else {
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
                    "parse.if-expression-else",
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
        context: &str,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<(ArenaOnlyExpr, usize)> {
        self.expect(
            TokenKindMatch::LBrace,
            &format!("expected `{{` to start {context}"),
        )?;
        self.skip_separators();
        let value = self.parse_precedence_arena_only(0, arena)?;
        self.skip_separators();
        let end = self
            .expect(
                TokenKindMatch::RBrace,
                &format!("expected `}}` to close {context}"),
            )
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        Some((value, end))
    }

    fn parse_match_expr_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        self.bump();
        let value = self.parse_precedence_arena_only(0, arena)?;
        self.expect(TokenKindMatch::LBrace, "expected `{` to start match arms")?;
        self.skip_separators();
        arena.begin_match_expr_arms();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            if self.parse_match_expr_arm_arena_only(arena).is_none() {
                self.recover_match_arm();
            }
            self.skip_separators();
        }
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
    ) -> Option<()> {
        let start = self.current_start();
        let (pattern, _pattern_span) = self.parse_pattern_arena_only(arena)?;
        let guard = if self.consume_keyword(Keyword::If).is_some() {
            Some(self.parse_expr_id_arena_only(arena)?)
        } else {
            None
        };
        self.expect(TokenKindMatch::FatArrow, "expected `=>` in match arm");
        let value = self.parse_expr_id_arena_only(arena)?;
        let value_end = self.previous_end();
        if self.consume(TokenKindMatch::Comma).is_some() {
            self.skip_newlines();
        }
        let span = self.span(start, value_end);
        arena.push_match_expr_arm_input_id(pattern, guard, value, span);
        Some(())
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
    fn report_unsupported_boolean_operator(&mut self) {
        let (unsupported, supported, span) = match (self.current_tag(), self.peek_tag(1)) {
            (TokenTag::Pipe, Some(TokenTag::Pipe)) => {
                let span = self.span(self.current_start(), self.peek_end(1).unwrap());
                ("||", "'or'", span)
            }
            (TokenTag::Amp, Some(TokenTag::Amp)) => {
                let span = self.span(self.current_start(), self.peek_end(1).unwrap());
                ("&&", "'and'", span)
            }
            (TokenTag::Pipe, _) => ("|", "'or'", self.current_span()),
            (TokenTag::Amp, _) => ("&", "'and'", self.current_span()),
            (TokenTag::Ident, _) if self.at_ident("then") => {
                let span = self.current_span();
                self.diagnostics.push(
                    Diagnostic::error("the `then` keyword is not used in XSH")
                        .with_code("parse.unsupported-then")
                        .with_label(Label::primary(
                            span,
                            "XSH `if`/`while`/`for` heads are followed directly by `{`, not `then`",
                        )),
                );
                return;
            }
            _ => return,
        };
        self.diagnostics.push(
            Diagnostic::error(format!(
                "unsupported operator '{unsupported}': XSH boolean operators are the word forms {supported}"
            ))
            .with_code("parse.unsupported-boolean-operator")
            .with_label(Label::primary(
                span,
                format!("use {supported} instead of '{unsupported}'"),
            )),
        );
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
            .with_code("parse.unsupported-integer-division")
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
        let mut left = self.parse_prefix_arena_only(arena)?;
        let mut pending_pipeline: Option<ArenaPendingPipeline> = None;
        loop {
            self.skip_postfix_newlines();
            self.skip_pipeline_newlines();
            if command_arg_root && self.current_start() > left.span.end() {
                break;
            }
            let continues_pipeline = pending_pipeline.is_some()
                && min_prec == 0
                && !self.pipe_is_boundary
                && self.at(TokenKindMatch::PipeGt);
            if !continues_pipeline && let Some(pending) = pending_pipeline.take() {
                left.id = pending.seal(arena, left.span);
            }
            if self.at(TokenKindMatch::Question) && self.peek_tag(1) == Some(TokenTag::Dot) {
                let try_end = self.current_end();
                self.bump();
                self.bump();
                let name = self.expect_member_name("expected field name after `?.`")?;
                if name == "require" && self.consume(TokenKindMatch::LParen).is_some() {
                    let schema = self.parse_type_expr(arena)?;
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
                let is_compat_module_call = left
                    .bare_ident
                    .is_some_and(|module| matches!(module.as_str().as_str(), "record" | "module"));
                if name == "require"
                    && !is_compat_module_call
                    && self.consume(TokenKindMatch::LParen).is_some()
                {
                    let schema = self.parse_type_expr(arena)?;
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
                        let id = arena.push_index_expr(left.id, first.id, span);
                        left = ArenaOnlyExpr {
                            id,
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
                let id = arena.push_slice_expr(left.id, start, end, span);
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
                    self.skip_newlines();
                }
                let unsupported_integer_division =
                    self.report_unsupported_integer_division();
                let (op, prec, tokens) = if let Some(tokens) = unsupported_integer_division {
                    (BinaryOp::Div, 6, tokens)
                } else if let Some((op, prec, tokens)) = self.current_binary_op() {
                    (op, prec, tokens)
                } else {
                    self.report_unsupported_boolean_operator();
                    break;
                };
                if prec < min_prec {
                    break;
                }
                for _ in 0..tokens {
                    self.bump();
                }
                let right_min_prec = if op == BinaryOp::ResultFallback {
                    prec
                } else {
                    prec + 1
                };
                self.skip_newlines();
                let right = self.parse_precedence_arena_only(right_min_prec, arena)?;
                let span = self.span(left.span.start(), right.span.end());
                let id = arena.push_binary_expr(op, left.id, right.id, span);
                left = ArenaOnlyExpr {
                    id,
                    span,
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
            let expr = self.parse_precedence_arena_only(8, arena)?;
            let span = self.span(start, expr.span.end());
            let id = arena.push_unary_expr(UnaryOp::Not, expr.id, span);
            return Some(ArenaOnlyExpr {
                id,
                span,
                bare_ident: None,
            });
        }
        if self.consume(TokenKindMatch::Minus).is_some() {
            let expr = self.parse_precedence_arena_only(8, arena)?;
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

    pub(super) fn parse_primary_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let span = self.current_span();
        match (self.current_tag(), self.current_keyword()) {
            (TokenTag::Keyword, Some(Keyword::Null)) => {
                self.bump();
                Some(ArenaOnlyExpr {
                    id: arena.push_null_expr(span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Keyword, Some(Keyword::True)) => {
                self.bump();
                Some(ArenaOnlyExpr {
                    id: arena.push_bool_expr(true, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Keyword, Some(Keyword::False)) => {
                self.bump();
                Some(ArenaOnlyExpr {
                    id: arena.push_bool_expr(false, span),
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
            (TokenTag::Keyword, Some(Keyword::If)) => {
                self.parse_if_expr_arena_only(span.start(), arena)
            }
            (TokenTag::Keyword, Some(Keyword::Match)) => {
                self.parse_match_expr_arena_only(span.start(), arena)
            }
            (TokenTag::Keyword, Some(Keyword::Loop)) => {
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
            (TokenTag::Keyword, Some(Keyword::Retry)) => {
                self.parse_retry_expr_arena_only(span.start(), arena)
            }
            (TokenTag::Keyword, Some(Keyword::Run)) => {
                let (run_id, _run_span) = self.parse_run_form_arena_only(arena)?;
                let span = self.span(span.start(), self.previous_end());
                Some(ArenaOnlyExpr {
                    id: arena.push_run_expr_id(run_id, span),
                    span,
                    bare_ident: None,
                })
            }
            (TokenTag::Keyword, Some(Keyword::Spawn)) => {
                self.parse_spawn_expr_arena_only(span.start(), arena)
            }
            (TokenTag::Keyword, Some(Keyword::Wait)) => {
                self.parse_wait_expr_arena_only(span.start(), arena)
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
                        .with_code("parse.expr-string-interpolation")
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
            (TokenTag::LBrace, _) => self.parse_record_arena_only(arena),
            (TokenTag::LParen, _) => {
                self.bump();
                let expr = self.parse_precedence_arena_only(0, arena)?;
                self.skip_newlines();
                self.expect(TokenKindMatch::RParen, "expected `)` after expression");
                Some(ArenaOnlyExpr {
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
                    .with_code("parse.expected-expression")
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
                    .with_code("parse.expected-expression")
                    .with_label(Label::primary(
                        span,
                        "remove `$` and braces in expression context",
                    )),
                );
                None
            }
            _ => {
                self.diagnostic_here("expected expression", "parse.expected-expression");
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
            self.diagnostic_here("expected path literal", "parse.expected-expression");
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
        let start = self.current_start();
        self.bump();
        self.skip_newlines();
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
        let first = self.parse_precedence_arena_only(0, arena)?;
        self.skip_newlines();
        if self.at_keyword(Keyword::For) {
            return self.parse_list_comp_arena_only(arena, start, first.id);
        }
        arena.begin_expr_ids();
        arena.push_expr_id_input(first.id);
        while self.consume(TokenKindMatch::Comma).is_some() {
            self.skip_newlines();
            if self.at(TokenKindMatch::RBracket) || self.at(TokenKindMatch::Eof) {
                break;
            }
            let Some(item) = self.parse_precedence_arena_only(0, arena) else {
                arena.discard_expr_ids();
                return None;
            };
            arena.push_expr_id_input(item.id);
            self.skip_newlines();
        }
        self.skip_newlines();
        let end = self
            .expect(TokenKindMatch::RBracket, "expected `]` after list")
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        let items = arena.finish_expr_ids();
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_list_expr_range(items, span),
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
        self.bump(); // consume `for`
        let target = self.parse_binding_target_arena_only(
            "expected binding target in list comprehension",
            arena,
        )?;
        self.expect_keyword(Keyword::In, "expected `in` in list comprehension");
        let iter = self.parse_expr_id_arena_only(arena)?;
        self.skip_newlines();
        let condition = if self.consume_keyword(Keyword::If).is_some() {
            Some(self.parse_expr_id_arena_only(arena)?)
        } else {
            None
        };
        self.skip_newlines();
        let end = self
            .expect(
                TokenKindMatch::RBracket,
                "expected `]` after list comprehension",
            )
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_list_comp_expr(expr, target, iter, condition, span),
            span,
            bare_ident: None,
        })
    }

    fn parse_record_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaOnlyExpr> {
        let start = self.current_start();
        self.bump();
        self.skip_newlines();
        arena.begin_record_fields();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            let field_start = self.current_start();
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
                self.skip_newlines();
                if self.consume(TokenKindMatch::Comma).is_none() {
                    break;
                }
                self.skip_newlines();
                continue;
            }
            let name = match self.current_tag() {
                TokenTag::Ident => self.current_name().expect("record key token has payload"),
                TokenTag::String => {
                    let flags = self
                        .token_table
                        .string_flags_at(self.index)
                        .expect("record string key has flags payload");
                    Name::intern(self.decoded_quoted_text(self.current_span(), flags.raw_literal))
                }
                _ => {
                    self.diagnostic_here("expected record field", "parse.expected-record-field");
                    break;
                }
            };
            self.bump();
            let mut key_id =
                arena.push_ident_expr(name, self.span(field_start, self.previous_end()));
            let mut dotted_key = false;
            while self.consume(TokenKindMatch::Dot).is_some() {
                dotted_key = true;
                let Some(field_name) = self.expect_ident("expected field name") else {
                    break;
                };
                let end = self.previous_end();
                key_id = arena.push_field_expr(key_id, field_name, self.span(field_start, end));
            }
            if self.consume(TokenKindMatch::Colon).is_some() {
                let Some(value) = self.parse_precedence_arena_only(0, arena) else {
                    arena.discard_record_fields();
                    return None;
                };
                self.skip_newlines();
                if self.at_keyword(Keyword::For) {
                    arena.discard_record_fields();
                    return self.parse_map_comp_tail_arena_only(arena, start, key_id, value.id);
                }
                if dotted_key {
                    self.diagnostic_here(
                        "dotted record fields are only valid in map comprehensions",
                        "parse.expected-map-comprehension",
                    );
                    break;
                }
                arena.push_record_field_input(ArenaRecordFieldInput::Named {
                    name,
                    value: value.id,
                    span: self.span(field_start, value.span.end()),
                });
            } else {
                if dotted_key {
                    self.diagnostic_here(
                        "dotted record fields are only valid in map comprehensions",
                        "parse.expected-map-comprehension",
                    );
                    break;
                }
                arena.push_record_field_input(ArenaRecordFieldInput::Shorthand {
                    name,
                    span: self.span(field_start, self.previous_end()),
                });
            }
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        self.skip_newlines();
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

    fn parse_map_comp_tail_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        start: usize,
        key: ExprId,
        value: ExprId,
    ) -> Option<ArenaOnlyExpr> {
        self.expect_keyword(Keyword::For, "expected `for` in map comprehension");
        let target = self.parse_binding_target_arena_only(
            "expected binding target in map comprehension",
            arena,
        )?;
        self.expect_keyword(Keyword::In, "expected `in` in map comprehension");
        let iter = self.parse_expr_id_arena_only(arena)?;
        self.skip_newlines();
        let condition = if self.consume_keyword(Keyword::If).is_some() {
            Some(self.parse_expr_id_arena_only(arena)?)
        } else {
            None
        };
        self.skip_newlines();
        let end = self
            .expect(
                TokenKindMatch::RBrace,
                "expected `}` after map comprehension",
            )
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        let span = self.span(start, end);
        Some(ArenaOnlyExpr {
            id: arena.push_map_comp_expr(key, value, target, iter, condition, span),
            span,
            bare_ident: None,
        })
    }

    pub(super) fn parse_call_args_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> crate::syntax::arena::ArenaRange {
        arena.begin_call_args();
        self.skip_newlines();
        while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
            if self.consume(TokenKindMatch::At).is_some() {
                let start = self.previous_start();
                let Some(value) = self.parse_precedence_arena_only(0, arena) else {
                    break;
                };
                arena.push_call_arg_input(ArenaCallArgInput::Splice {
                    value: value.id,
                    span: self.span(start, value.span.end()),
                });
            } else {
                let named = self.current_tag() == TokenTag::Ident
                    && self.peek_tag(1) == Some(TokenTag::Colon);
                if named {
                    let start = self.current_start();
                    let name = self.expect_ident("expected named argument").unwrap();
                    self.bump();
                    let Some(value) = self.parse_precedence_arena_only(0, arena) else {
                        break;
                    };
                    arena.push_call_arg_input(ArenaCallArgInput::Named {
                        name,
                        value: value.id,
                        span: self.span(start, value.span.end()),
                    });
                } else if let Some(expr) = self.parse_precedence_arena_only(0, arena) {
                    arena.push_call_arg_input(ArenaCallArgInput::Positional(expr.id));
                } else {
                    break;
                }
            }
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
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
        let member = if matches!(name.as_str().as_str(), "table" | "text" | "bytes" | "json")
            && self.consume(TokenKindMatch::Dot).is_some()
        {
            Some(self.expect_member_name("expected stream stage method")?)
        } else {
            None
        };
        let kind = stream_stage_kind_from_names(name, member).unwrap_or_else(|| {
            self.diagnostic_previous("unknown stream stage", "parse.unknown-stream-stage");
            StreamStageKind::Map
        });
        let options = self.parse_stream_stage_options_arena_only(arena)?;
        let mut args = ArenaRange::default();
        if self.consume(TokenKindMatch::LParen).is_some() {
            args = self.parse_call_args_arena_only(arena);
            self.expect(TokenKindMatch::RParen, "expected `)` after stage arguments");
        }
        let block = if self.at(TokenKindMatch::LBrace) && stream_stage_accepts_block(&kind) {
            Some(self.parse_block_arena_only(arena)?)
        } else if stream_stage_accepts_inline_expr(&kind) && !self.at_pipe_stage_end() {
            Some(self.parse_inline_stream_block_arena_only(arena)?)
        } else {
            None
        };
        let end = self.previous_end();
        let span = self.span(start, end);
        Some(arena.build_stream_stage(kind, options, block, args, span))
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

    fn parse_stream_stage_options_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaRange> {
        arena.begin_stream_stage_options();
        while self.at(TokenKindMatch::Minus) && self.peek_tag(1) == Some(TokenTag::Minus) {
            let start = self.current_start();
            self.bump();
            self.bump();
            let Some(name) = self.expect_stream_option_name() else {
                break;
            };
            let value = if self.consume(TokenKindMatch::Equals).is_some() {
                if self.consume(TokenKindMatch::DollarLBrace).is_some() {
                    let Some(value) = self.parse_expr_id_arena_only(arena) else {
                        arena.discard_stream_stage_options();
                        return None;
                    };
                    self.expect(
                        TokenKindMatch::RBrace,
                        "expected `}` after option interpolation",
                    );
                    Some(value)
                } else {
                    let Some(value) = self.parse_stream_option_expr_arena_only(arena) else {
                        arena.discard_stream_stage_options();
                        return None;
                    };
                    Some(value)
                }
            } else {
                None
            };
            let end = self.previous_end();
            arena.push_stream_stage_option_input(name, value, self.span(start, end));
        }
        Some(arena.finish_stream_stage_options())
    }

    fn parse_stream_option_expr_arena_only(
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
            return stream_stage_kind_from_names(namespace, Some(member)).is_none();
        }
        stream_stage_kind_from_names(name, None).is_none()
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
            self.diagnostic_here("expected stream stage name", "parse.expected-stream-stage");
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
            self.diagnostic_here("expected stream stage option name", "parse.expected-ident");
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
            let Some(delay) = self.parse_precedence_arena_only(0, arena) else {
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
        let Some(block_id) = self.parse_block_arena_only(arena) else {
            arena.discard_expr_ids();
            return None;
        };
        let span = self.span(start, self.previous_end());
        let delays = arena.finish_expr_ids();
        Some(ArenaOnlyExpr {
            id: arena.push_retry_expr(delays, block_id, span),
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

pub(in crate::syntax::parser) fn stream_stage_accepts_block(kind: &StreamStageKind) -> bool {
    matches!(
        kind,
        StreamStageKind::Where
            | StreamStageKind::Map
            | StreamStageKind::ParMap
            | StreamStageKind::Each
            | StreamStageKind::Batch
            | StreamStageKind::SortBy
            | StreamStageKind::UniqueBy
            | StreamStageKind::Tee
            | StreamStageKind::GroupBy
            | StreamStageKind::FlatMap
            | StreamStageKind::Fold
            | StreamStageKind::Reduce
            | StreamStageKind::Any
            | StreamStageKind::All
            | StreamStageKind::Count
            | StreamStageKind::Collect
            | StreamStageKind::ReduceBy
    )
}

pub(in crate::syntax::parser) fn stream_stage_accepts_inline_expr(kind: &StreamStageKind) -> bool {
    matches!(
        kind,
        StreamStageKind::Where
            | StreamStageKind::Map
            | StreamStageKind::ParMap
            | StreamStageKind::Each
            | StreamStageKind::SortBy
            | StreamStageKind::UniqueBy
            | StreamStageKind::Tee
            | StreamStageKind::GroupBy
            | StreamStageKind::FlatMap
            | StreamStageKind::Any
            | StreamStageKind::All
    )
}

pub(in crate::syntax::parser) fn stream_stage_kind_from_names(
    name: Name,
    member: Option<Name>,
) -> Option<StreamStageKind> {
    match member {
        None => Some(match name.as_str().as_str() {
            "where" => StreamStageKind::Where,
            "map" => StreamStageKind::Map,
            "par-map" => StreamStageKind::ParMap,
            "each" => StreamStageKind::Each,
            "batch" => StreamStageKind::Batch,
            "sort" => StreamStageKind::Sort,
            "sort-by" => StreamStageKind::SortBy,
            "take" => StreamStageKind::Take,
            "drop" => StreamStageKind::Drop,
            "first" => StreamStageKind::First,
            "last" => StreamStageKind::Last,
            "unique-by" => StreamStageKind::UniqueBy,
            "enumerate" => StreamStageKind::Enumerate,
            "zip" => StreamStageKind::Zip,
            "range" => StreamStageKind::Range,
            "repeat" => StreamStageKind::Repeat,
            "tee" => StreamStageKind::Tee,
            "sum" => StreamStageKind::Sum,
            "min" => StreamStageKind::Min,
            "max" => StreamStageKind::Max,
            "group-by" => StreamStageKind::GroupBy,
            "fold" => StreamStageKind::Fold,
            "reduce" => StreamStageKind::Reduce,
            "flat-map" => StreamStageKind::FlatMap,
            "any" => StreamStageKind::Any,
            "all" => StreamStageKind::All,
            "shuffle" => StreamStageKind::Shuffle,
            "count" => StreamStageKind::Count,
            "collect" => StreamStageKind::Collect,
            "reduce-by" => StreamStageKind::ReduceBy,
            _ => return None,
        }),
        Some(member) => Some(match (name.as_str().as_str(), member.as_str().as_str()) {
            ("table", "print") => StreamStageKind::TablePrint,
            ("text", "lines") => StreamStageKind::TextStreamLines,
            ("bytes", "chunks") => StreamStageKind::BytesChunks,
            ("json", "lines") => StreamStageKind::JsonLines,
            ("json", "stream") => StreamStageKind::JsonStream,
            _ => return None,
        }),
    }
}

pub(in crate::syntax::parser) fn builder_api_accepts_block(module: &str, name: &str) -> bool {
    matches!((module, name), ("process", "command"))
}

fn arena_expr_accepts_builder_block(arena: &ArenaProgramBuilder<'_>, id: ExprId) -> bool {
    match arena.expr_kind(id) {
        ArenaExprKind::Call { callee, .. } => match arena.expr_kind(callee) {
            ArenaExprKind::Field { base, name } => matches!(
                arena.expr_kind(base),
                ArenaExprKind::Ident(module) if builder_api_accepts_block(&module.as_str(), &name.as_str())
            ),
            _ => false,
        },
        ArenaExprKind::Field { base, name } => matches!(
            arena.expr_kind(base),
            ArenaExprKind::Ident(module) if builder_api_accepts_block(&module.as_str(), &name.as_str())
        ),
        _ => false,
    }
}
