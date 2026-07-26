#![allow(clippy::single_call_fn)]

use super::{
    AssignOp, BlockParam, DurationLiteral, Effect, IntLiteral, Keyword, Name, Parser,
    SignalHookOptions, TokenKindMatch, TokenTag, result_unit_type_expr, unknown_type_expr,
};
use crate::syntax::arena::{
    ArenaBuilderEntryKind, ArenaExprOrRun, ArenaModuleContractEntryKind, ArenaProgramBuilder,
    ArenaTypeDefBody, BindingTargetId, BuilderBlockId, ExprId, TypeExprId,
};
use std::str::FromStr;

impl<'a> Parser<'a> {
    pub(super) fn parse_statement_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.skip_comments();
        let start = self.current_start();
        match (self.current_tag(), self.current_keyword()) {
            (TokenTag::Keyword, Some(Keyword::Let)) => {
                self.parse_binding_arena_only(start, true, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Var)) => {
                self.parse_binding_arena_only(start, false, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Run)) if self.lookahead_is_run_stream() => {
                self.parse_expr_statement_arena_only(start, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Run)) => {
                self.parse_command_statement_arena_only(start, arena)
            }
            (TokenTag::Keyword, Some(Keyword::True | Keyword::False))
                if self.block_depth == 0 && self.lookahead_is_keyword_builtin_command() =>
            {
                self.parse_command_statement_arena_only(start, arena)
            }
            (TokenTag::Keyword, Some(Keyword::If)) => self.parse_if_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::While)) => self.parse_while_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::For)) => self.parse_for_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::Loop)) => self.parse_loop_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::Return)) => {
                self.parse_return_arena_only(start, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Yield)) => self.parse_yield_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::Defer)) => self.parse_defer_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::Break)) => {
                self.parse_loop_control_arena_only(start, true, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Continue)) => {
                self.parse_loop_control_arena_only(start, false, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Match)) => self.parse_match_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::Proc)) => {
                self.parse_function_arena_only(start, true, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Pure)) => {
                self.parse_function_arena_only(start, false, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Stream)) => {
                self.parse_stream_function_arena_only(start, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Use)) => self.parse_use_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::Guard)) => self.parse_guard_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::With)) => self.parse_with_arena_only(start, arena),
            (TokenTag::Keyword, Some(Keyword::Type)) => {
                self.parse_type_def_arena_only(start, arena)
            }
            (TokenTag::Keyword, Some(Keyword::Export)) => {
                self.parse_export_arena_only(start, arena)
            }
            (TokenTag::Ident | TokenTag::ProcIdent, _) => {
                if self.current_name().is_some_and(|name| name == "error")
                    && matches!(
                        self.peek_tag(1),
                        Some(TokenTag::Ident | TokenTag::ProcIdent)
                    )
                    && self.peek_tag(2) == Some(TokenTag::Equals)
                {
                    self.parse_error_def_arena_only(start, arena)
                } else if self.current_name().is_some_and(|name| name == "on")
                    && self.lookahead_is_signal_hook()
                {
                    self.parse_signal_hook_arena_only(start, arena)
                } else if self.lookahead_is_assignment() {
                    self.parse_assignment_arena_only(start, arena)
                } else if self.lookahead_is_dotted_command() {
                    self.parse_command_statement_arena_only(start, arena)
                } else if self.lookahead_is_expr_call_or_postfix()
                    || self.lookahead_is_expr_binary()
                {
                    self.parse_expr_statement_arena_only(start, arena)
                } else {
                    self.parse_command_statement_arena_only(start, arena)
                }
            }
            _ => self.parse_expr_statement_arena_only(start, arena),
        }
    }

    fn parse_use_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let mut path = Vec::new();
        let first = self.expect_module_path_segment("expected module name after `use`")?;
        path.push(first);
        while self.consume(TokenKindMatch::Dot).is_some() {
            if let Some(name) =
                self.expect_module_path_segment("expected module path segment after `.`")
            {
                path.push(name);
            } else {
                break;
            }
        }
        let alias = if self.at_ident("as") {
            self.bump();
            Some(self.expect_ident("expected module alias after `as`")?)
        } else {
            None
        };
        let end = self.expect_terminator();
        arena.push_use(&path, alias, self.span(start, end));
        Some(())
    }

    fn parse_export_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        match (self.current_tag(), self.current_keyword()) {
            (TokenTag::Keyword, Some(Keyword::Let)) => {
                self.parse_binding_arena_only(start, true, arena)?
            }
            (TokenTag::Keyword, Some(Keyword::Proc)) => {
                self.parse_function_arena_only(start, true, arena)?
            }
            (TokenTag::Keyword, Some(Keyword::Pure)) => {
                self.parse_function_arena_only(start, false, arena)?
            }
            (TokenTag::Keyword, Some(Keyword::Stream)) => {
                self.parse_stream_function_arena_only(start, arena)?
            }
            (TokenTag::Keyword, Some(Keyword::Type)) => {
                self.parse_type_def_arena_only(start, arena)?
            }
            (TokenTag::Ident, _)
                if self.current_name().is_some_and(|name| name == "on")
                    && self.lookahead_is_signal_hook() =>
            {
                self.parse_signal_hook_arena_only(start, arena)?
            }
            (TokenTag::Ident, _)
                if self.current_name().is_some_and(|name| name == "error")
                    && matches!(
                        self.peek_tag(1),
                        Some(TokenTag::Ident | TokenTag::ProcIdent)
                    )
                    && self.peek_tag(2) == Some(TokenTag::Equals) =>
            {
                self.parse_error_def_arena_only(start, arena)?
            }
            _ => {
                self.diagnostic_here(
                    "`export` applies only to let, proc, pure, stream, type, or error definitions",
                    "parse.export-target",
                );
                return None;
            }
        };
        let inner = arena
            .last_current_statement_id()
            .expect("export wraps a just-registered statement");
        let span = self.span(start, self.previous_end());
        arena.push_export(inner, span);
        Some(())
    }

    fn parse_type_def_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let name = self.expect_ident("expected type name")?;
        self.expect(TokenKindMatch::Equals, "expected `=` in type definition");
        let body = if self.at_ident("module") {
            self.bump();
            ArenaTypeDefBody::ModuleContract(self.parse_module_contract_arena_only(arena)?)
        } else if self.at(TokenKindMatch::LBrace) {
            ArenaTypeDefBody::RecordSchema(self.parse_record_schema_arena_only(arena)?)
        } else if let Some(variants) = self.try_parse_tag_union_arena_only(arena) {
            ArenaTypeDefBody::TagUnion(variants)
        } else {
            ArenaTypeDefBody::Alias(self.parse_type_expr(arena)?)
        };
        let end = self.expect_terminator();
        let span = self.span(start, end);
        arena.push_type_def(name, body, span);
        Some(())
    }

    fn parse_record_schema_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<crate::syntax::arena::ArenaRange> {
        self.bump();
        self.skip_newlines();
        let mut fields = Vec::new();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            let start = self.current_start();
            let name = self.expect_ident("expected schema field name")?;
            self.expect(TokenKindMatch::Colon, "expected `:` after schema field");
            let ty_id = self.parse_type_expr(arena)?;
            let ty_end = self.previous_end();
            let span = self.span(start, ty_end);
            fields.push(arena.build_schema_field(name, ty_id, span));
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        self.expect(TokenKindMatch::RBrace, "expected `}` after schema");
        Some(arena.push_schema_field_range(fields))
    }

    fn try_parse_tag_union_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<crate::syntax::arena::ArenaRange> {
        let mut base = 0usize;
        while matches!(
            self.peek_tag(base),
            Some(TokenTag::Newline | TokenTag::Comment)
        ) {
            base += 1;
        }
        let is_ident = matches!(
            self.peek_tag(base),
            Some(TokenTag::Ident | TokenTag::ProcIdent)
        );
        if !is_ident {
            return None;
        }
        let next = self.peek_tag_skip_newlines(base + 1);
        let looks_like_tag_union = match next {
            Some(TokenTag::Pipe) => true,
            Some(TokenTag::LParen) => {
                let mut depth = 0usize;
                let mut i = 1;
                loop {
                    match self.peek_tag(i) {
                        Some(TokenTag::LParen) => {
                            depth += 1;
                            i += 1;
                        }
                        Some(TokenTag::RParen) => {
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                            i += 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        Some(TokenTag::Eof) | None => break,
                        _ => {
                            i += 1;
                        }
                    }
                }
                matches!(self.peek_tag_skip_newlines(i), Some(TokenTag::Pipe))
            }
            _ => false,
        };
        if !looks_like_tag_union {
            return None;
        }
        let mut variants = Vec::new();
        self.skip_newlines();
        loop {
            let variant_start = self.current_start();
            let variant_name =
                if matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
                    let name = self
                        .current_name()
                        .expect("tag variant name token has payload");
                    self.bump();
                    name
                } else {
                    break;
                };
            let fields = if self.consume(TokenKindMatch::LParen).is_some() {
                let mut field_ids = Vec::new();
                while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
                    let Some(ty) = self.parse_type_expr(arena) else {
                        break;
                    };
                    field_ids.push(ty);
                    if self.consume(TokenKindMatch::Comma).is_none() {
                        break;
                    }
                }
                self.expect(TokenKindMatch::RParen, "expected `)` after variant fields");
                field_ids
            } else {
                Vec::new()
            };
            let variant_end = self.previous_end();
            let span = self.span(variant_start, variant_end);
            variants.push(arena.build_tag_variant(variant_name, &fields, span));
            if self.peeked_pipe_after_newlines() {
                self.skip_newlines();
                self.bump();
            } else if self.consume(TokenKindMatch::Pipe).is_none() {
                break;
            }
        }
        if variants.len() < 2 {
            return None;
        }
        Some(arena.push_tag_variant_range(variants))
    }

    fn parse_module_contract_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<crate::syntax::arena::ArenaRange> {
        self.expect(TokenKindMatch::LBrace, "expected `{` after `module`");
        let mut entries = Vec::new();
        self.skip_newlines();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            let start = self.current_start();
            self.expect_keyword(Keyword::Export, "expected `export` in module contract")?;
            let optional = if self.at_ident("optional") {
                self.bump();
                true
            } else {
                false
            };
            if self.consume_keyword(Keyword::Proc).is_some() {
                let name = self.expect_proc_ident("expected exported proc name")?;
                self.expect(TokenKindMatch::LParen, "expected `(` after proc name");
                let params = self.parse_params_arena_only(arena);
                self.expect(TokenKindMatch::RParen, "expected `)` after parameters");
                let params = arena.push_params(&params);
                let effects = self
                    .parse_effect_list()
                    .map(|effects| arena.push_effects(&effects));
                let return_ty = if self.consume(TokenKindMatch::Arrow).is_some() {
                    self.parse_type_expr(arena)?
                } else {
                    result_unit_type_expr(arena, self.current_span())
                };
                let end = self.previous_end();
                let span = self.span(start, end);
                entries.push(arena.build_module_contract_entry(
                    name,
                    optional,
                    ArenaModuleContractEntryKind::Proc {
                        params,
                        effects,
                        return_ty,
                    },
                    span,
                ));
                self.skip_module_contract_separator();
                continue;
            } else if self.consume_keyword(Keyword::Pure).is_some() {
                let name = self.expect_ident("expected exported pure function name")?;
                self.expect(
                    TokenKindMatch::LParen,
                    "expected `(` after pure function name",
                );
                let params = self.parse_params_arena_only(arena);
                self.expect(TokenKindMatch::RParen, "expected `)` after parameters");
                let params = arena.push_params(&params);
                self.expect(
                    TokenKindMatch::Arrow,
                    "expected `->` after pure function parameters",
                );
                let return_ty = self.parse_type_expr(arena)?;
                let end = self.previous_end();
                let span = self.span(start, end);
                entries.push(arena.build_module_contract_entry(
                    name,
                    optional,
                    ArenaModuleContractEntryKind::Pure { params, return_ty },
                    span,
                ));
                self.skip_module_contract_separator();
                continue;
            } else {
                self.consume_keyword(Keyword::Let);
                let name = self.expect_ident("expected exported value name")?;
                self.expect(
                    TokenKindMatch::Colon,
                    "expected `:` after exported value name",
                );
                let ty_id = self.parse_type_expr(arena)?;
                let end = self.previous_end();
                let span = self.span(start, end);
                entries.push(arena.build_module_contract_entry(
                    name,
                    optional,
                    ArenaModuleContractEntryKind::Value(ty_id),
                    span,
                ));
                self.skip_module_contract_separator();
                continue;
            };
        }
        self.expect(TokenKindMatch::RBrace, "expected `}` after module contract");
        Some(arena.push_module_contract_entry_range(entries))
    }

    fn skip_module_contract_separator(&mut self) {
        self.skip_separators();
        if self.consume(TokenKindMatch::Comma).is_some() {
            self.skip_separators();
        }
    }

    fn parse_error_def_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let name = self.expect_ident("expected error family name")?;
        self.expect(TokenKindMatch::Equals, "expected `=` in error definition");
        self.skip_newlines();
        let mut variants = Vec::new();
        loop {
            if self.consume(TokenKindMatch::Pipe).is_some() {
                self.skip_newlines();
            }
            if self.at(TokenKindMatch::Eof) {
                break;
            }
            let variant_start = self.current_start();
            let variant_name =
                if matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
                    let name = self
                        .current_name()
                        .expect("error variant name token has payload");
                    self.bump();
                    name
                } else {
                    if variants.is_empty() {
                        self.diagnostic_here("expected error variant", "parse.error-variant");
                    }
                    break;
                };
            let mut fields = Vec::new();
            if self.consume(TokenKindMatch::LParen).is_some() {
                self.skip_newlines();
                while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
                    let field_start = self.current_start();
                    let field_name = self.expect_ident("expected error payload field")?;
                    self.expect(
                        TokenKindMatch::Colon,
                        "expected `:` after error payload field",
                    );
                    let ty_id = self.parse_type_expr(arena)?;
                    let ty_end = self.previous_end();
                    let span = self.span(field_start, ty_end);
                    fields.push(arena.build_error_field(field_name, ty_id, span));
                    self.skip_newlines();
                    if self.consume(TokenKindMatch::Comma).is_none() {
                        break;
                    }
                    self.skip_newlines();
                }
                self.expect(
                    TokenKindMatch::RParen,
                    "expected `)` after error payload fields",
                );
            }
            let mut facets = Vec::new();
            if self.consume(TokenKindMatch::Colon).is_some() {
                loop {
                    facets.push(self.expect_ident("expected error facet")?);
                    if self.consume(TokenKindMatch::Comma).is_none() {
                        break;
                    }
                }
            }
            let variant_end = self.previous_end();
            let span = self.span(variant_start, variant_end);
            variants.push(arena.build_error_variant(variant_name, fields, &facets, span));
            if self.consume(TokenKindMatch::Pipe).is_none() {
                break;
            }
            self.skip_newlines();
        }
        let end = self.expect_terminator();
        let span = self.span(start, end);
        arena.push_error_def(name, variants, span);
        Some(())
    }

    fn parse_binding_arena_only(
        &mut self,
        start: usize,
        immutable: bool,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let target = self.parse_binding_target_arena_only("expected binding name", arena)?;
        let ty = if self.consume(TokenKindMatch::Colon).is_some() {
            Some(self.parse_type_expr(arena)?)
        } else {
            None
        };
        self.expect(TokenKindMatch::Equals, "expected `=` in binding");
        let initializer = self.parse_expr_or_run_arena_only(arena)?;
        let end = self.expect_terminator();
        arena.push_binding_parts(immutable, target, ty, initializer, self.span(start, end));
        Some(())
    }

    fn parse_expr_or_run_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaExprOrRun> {
        if self.at_keyword(Keyword::Run) && self.lookahead_is_run_stream() {
            let expr_id = self.parse_expr_id_arena_only(arena)?;
            return Some(ArenaExprOrRun::Expr(expr_id));
        }
        if self.at_keyword(Keyword::Run) {
            let (run_id, _span) = self.parse_run_form_arena_only(arena)?;
            let propagate = self.consume(TokenKindMatch::Question).is_some();
            if propagate {
                arena.set_run_form_propagate(run_id, true);
            }
            return Some(arena.run_expr_or_run(run_id));
        }
        let expr_id = self.parse_expr_id_arena_only(arena)?;
        Some(ArenaExprOrRun::Expr(expr_id))
    }

    fn parse_assignment_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let target_id = self.parse_assign_target_arena_only(arena)?;
        let op = self.parse_assign_op();
        let value = self.parse_expr_or_run_arena_only(arena)?;
        let end = self.expect_terminator();
        arena.push_assignment(target_id, op, value, self.span(start, end));
        Some(())
    }

    fn parse_assign_target_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<crate::syntax::arena::AssignTargetId> {
        let name = self.expect_ident("expected assignment target")?;
        let mut target_id = arena.push_assign_target_name(name);
        loop {
            if self.at(TokenKindMatch::Dot) && self.peek_tag(1) != Some(TokenTag::Dot) {
                self.bump();
                let name = self.expect_member_name("expected field name after `.`")?;
                target_id = arena.push_assign_target_field(target_id, name);
            } else if self.consume(TokenKindMatch::LBracket).is_some() {
                let index_id = self.parse_expr_id_arena_only(arena)?;
                self.expect(TokenKindMatch::RBracket, "expected `]` after index");
                target_id = arena.push_assign_target_index(target_id, index_id);
            } else {
                break;
            }
        }
        Some(target_id)
    }

    pub(super) fn parse_assign_op(&mut self) -> AssignOp {
        if self.consume(TokenKindMatch::Equals).is_some() {
            return AssignOp::Set;
        }
        let op = match self.current_tag() {
            TokenTag::Plus => AssignOp::Add,
            TokenTag::Minus => AssignOp::Sub,
            TokenTag::Star => AssignOp::Mul,
            TokenTag::Slash => AssignOp::Div,
            TokenTag::Percent => AssignOp::Rem,
            _ => {
                self.diagnostic_here("expected assignment operator", "parse.expected-token");
                return AssignOp::Set;
            }
        };
        self.bump();
        self.expect(TokenKindMatch::Equals, "expected `=` in assignment");
        op
    }

    fn parse_params_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Vec<(
        Name,
        TypeExprId,
        bool,
        Option<ExprId>,
        bool,
        crate::source::Span,
    )> {
        let mut params = Vec::new();
        self.skip_newlines();
        while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
            let start = self.current_start();
            let rest = self.consume_rest_marker();
            let Some(name) = self.expect_ident("expected parameter name") else {
                break;
            };
            let (ty_id, ty_defaulted, default_id, end) =
                if self.consume(TokenKindMatch::Colon).is_some() {
                    let Some(ty_id) = self.parse_type_expr(arena) else {
                        break;
                    };
                    let (default_id, end) = if self.consume(TokenKindMatch::Equals).is_some() {
                        if rest {
                            self.diagnostic_previous(
                                "rest parameters cannot have default values",
                                "parse.rest-default",
                            );
                        }
                        match self.parse_expr_id_arena_only(arena) {
                            Some(id) => (Some(id), self.previous_end()),
                            None => break,
                        }
                    } else {
                        (None, self.previous_end())
                    };
                    (ty_id, false, default_id, end)
                } else if self.consume(TokenKindMatch::Equals).is_some() {
                    if rest {
                        self.diagnostic_previous(
                            "rest parameters cannot have default values",
                            "parse.rest-default",
                        );
                    }
                    let default_start = self.current_start();
                    let Some(default_id) = self.parse_expr_id_arena_only(arena) else {
                        break;
                    };
                    let default_span = self.span(default_start, self.previous_end());
                    let ty_id = match arena.infer_param_type_name(default_id) {
                        Some(type_name) => arena.push_named_type_expr(type_name, default_span),
                        None => {
                            self.diagnostic_at(
                                default_span,
                                "defaulted parameter needs an explicit type",
                                "parse.inferred-param-type",
                            );
                            unknown_type_expr(arena, default_span)
                        }
                    };
                    (ty_id, true, Some(default_id), default_span.end())
                } else {
                    self.diagnostic_here(
                        "expected `:` or default value after parameter name",
                        "parse.expected-param-type",
                    );
                    break;
                };
            params.push((
                name,
                ty_id,
                ty_defaulted,
                default_id,
                rest,
                self.span(start, end),
            ));
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        params
    }

    fn parse_function_arena_only(
        &mut self,
        start: usize,
        proc_def: bool,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let name = if proc_def {
            self.expect_proc_ident("expected proc name")?
        } else {
            self.expect_ident("expected pure function name")?
        };
        if self.consume(TokenKindMatch::LParen).is_none() {
            self.diagnostic_here(
                "function signatures are required",
                "parse.required-signature",
            );
            return None;
        }
        let params = self.parse_params_arena_only(arena);
        self.expect(TokenKindMatch::RParen, "expected `)` after parameters");
        let effects = if proc_def {
            self.parse_effect_list()
        } else {
            None
        };
        let (return_ty, return_ty_defaulted) = if self.consume(TokenKindMatch::Arrow).is_some() {
            (self.parse_type_expr(arena)?, false)
        } else if proc_def {
            (result_unit_type_expr(arena, self.current_span()), true)
        } else {
            self.diagnostic_here(
                "pure function return annotations are required",
                "parse.required-return",
            );
            (result_unit_type_expr(arena, self.current_span()), true)
        };
        let body_id = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        let params_range = arena.push_params(&params);
        let effects_range = effects
            .as_deref()
            .map(|effects| arena.push_effects(effects));
        arena.push_function_def_parts(
            name,
            params_range,
            effects_range,
            return_ty,
            return_ty_defaulted,
            body_id,
            proc_def,
            span,
        );
        Some(())
    }

    fn parse_stream_function_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let name = self.expect_ident("expected stream producer name")?;
        if self.consume(TokenKindMatch::LParen).is_none() {
            self.diagnostic_here(
                "stream producer signatures are required",
                "parse.required-signature",
            );
            return None;
        }
        let params = self.parse_params_arena_only(arena);
        self.expect(TokenKindMatch::RParen, "expected `)` after parameters");
        let effects = self.parse_effect_list();
        let return_ty = if self.consume(TokenKindMatch::Arrow).is_some() {
            self.parse_type_expr(arena)?
        } else {
            self.diagnostic_here(
                "stream producer return annotations are required",
                "parse.required-return",
            );
            unknown_type_expr(arena, self.current_span())
        };
        let body_id = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        let params_range = arena.push_params(&params);
        let effects_range = effects
            .as_deref()
            .map(|effects| arena.push_effects(effects));
        arena.push_stream_function_def_parts(
            name,
            params_range,
            effects_range,
            return_ty,
            false,
            body_id,
            span,
        );
        Some(())
    }

    fn lookahead_is_signal_hook(&self) -> bool {
        if self.current_tag() != TokenTag::Ident || self.current_name() != Some(Name::intern("on"))
        {
            return false;
        }
        let mut index = self.index + 1;
        if matches!(
            self.token_table.tag_at(index),
            Some(TokenTag::Ident | TokenTag::ProcIdent | TokenTag::Int)
        ) {
            index += 1;
        } else {
            return false;
        }

        while matches!(
            (
                self.token_table.tag_at(index),
                self.token_table.tag_at(index + 1),
            ),
            (Some(TokenTag::Minus), Some(TokenTag::Minus))
        ) {
            index += 2;
            if matches!(
                self.token_table.tag_at(index),
                Some(TokenTag::Ident | TokenTag::ProcIdent)
            ) {
                index += 1;
            }
            if self.token_table.tag_at(index) == Some(TokenTag::Equals) {
                index += 1;
                if matches!(
                    self.token_table.tag_at(index),
                    Some(
                        TokenTag::Duration | TokenTag::Int | TokenTag::Ident | TokenTag::ProcIdent
                    )
                ) {
                    index += 1;
                }
            }
        }

        matches!(
            self.token_table.tag_at(index),
            Some(
                TokenTag::LBracket
                    | TokenTag::LBrace
                    | TokenTag::Newline
                    | TokenTag::Semicolon
                    | TokenTag::Eof
            )
        )
    }

    fn parse_signal_hook_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let signal = match self.current_tag() {
            TokenTag::Ident | TokenTag::ProcIdent => {
                let name = self
                    .current_name()
                    .expect("signal hook name token has payload");
                self.bump();
                name
            }
            TokenTag::Int => {
                let span = self.bump();
                Name::intern(IntLiteral::from_text(self.span_text(span)).to_text())
            }
            _ => {
                self.diagnostic_here("expected signal name after `on`", "parse.signal-hook");
                return None;
            }
        };
        let mut options = SignalHookOptions::default();
        while self.at(TokenKindMatch::Minus) && self.peek_tag(1) == Some(TokenTag::Minus) {
            let option_span = self.current_span();
            self.bump();
            self.bump();
            let name = match self.current_tag() {
                TokenTag::Ident | TokenTag::ProcIdent => {
                    let name = self
                        .current_name()
                        .expect("signal hook option token has payload");
                    self.bump();
                    name
                }
                _ => {
                    self.diagnostic_here("expected signal hook option name", "parse.signal-hook");
                    break;
                }
            };
            match name.as_str().as_str() {
                "pre-cancel" => {
                    self.expect(TokenKindMatch::Equals, "expected `=` after `--pre-cancel`");
                    match self.current_tag() {
                        TokenTag::Duration => {
                            let span = self.bump();
                            options.pre_cancel.replace(
                                DurationLiteral::from_text(self.span_text(span)).to_text(),
                            );
                        }
                        _ => self.diagnostic_here(
                            "`--pre-cancel` expects a duration literal",
                            "parse.signal-hook",
                        ),
                    }
                }
                _ => {
                    self.diagnostic_at(
                        option_span,
                        &format!("unknown signal hook option `--{name}`"),
                        "parse.signal-hook",
                    );
                    if self.consume(TokenKindMatch::Equals).is_some()
                        && !self.at_terminator()
                        && !self.at(TokenKindMatch::LBracket)
                        && !self.at(TokenKindMatch::LBrace)
                    {
                        self.bump();
                    }
                }
            }
        }
        let effects = if self.at(TokenKindMatch::LBracket) {
            self.parse_effect_list().unwrap_or_default()
        } else {
            self.diagnostic_here("signal hooks require an effect list", "parse.signal-hook");
            Vec::new()
        };
        let effects = arena.push_effects(&effects);
        let body = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_signal_hook(signal, options, effects, body, span);
        Some(())
    }

    pub(super) fn parse_effect_list(&mut self) -> Option<Vec<Effect>> {
        self.consume(TokenKindMatch::LBracket)?;
        let mut effects = Vec::new();
        self.skip_newlines();
        while !self.at(TokenKindMatch::RBracket) && !self.at(TokenKindMatch::Eof) {
            let span = self.current_span();
            if let Some(name) = self.expect_ident("expected effect name") {
                match Effect::from_str(&name.as_str()) {
                    Ok(effect) => effects.push(effect),
                    Err(()) => self.diagnostic_at(
                        span,
                        &format!("unknown effect `{name}`"),
                        "parse.unknown-effect",
                    ),
                }
            } else {
                break;
            }
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        self.expect(TokenKindMatch::RBracket, "expected `]` after effects");
        Some(effects)
    }

    pub(super) fn consume_rest_marker(&mut self) -> bool {
        if self.at(TokenKindMatch::Dot)
            && self.peek_tag(1) == Some(TokenTag::Dot)
            && self.peek_tag(2) == Some(TokenTag::Dot)
        {
            self.bump();
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    fn parse_guard_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump(); // consume `guard`
        self.expect_keyword(Keyword::Let, "expected `let` after `guard`");
        let target = self.parse_binding_target_arena_only("expected binding name", arena)?;
        let ty = if self.consume(TokenKindMatch::Colon).is_some() {
            Some(self.parse_type_expr(arena)?)
        } else {
            None
        };
        self.expect(TokenKindMatch::Equals, "expected `=` in guard binding");
        let initializer = self.parse_expr_or_run_arena_only(arena)?;
        self.expect_keyword(Keyword::Else, "expected `else` in guard statement");
        let else_param = if self.consume(TokenKindMatch::Pipe).is_some() {
            let param = self.expect_ident("expected parameter name in `else |param|`");
            self.expect(TokenKindMatch::Pipe, "expected `|` after else parameter");
            param
        } else {
            None
        };
        let else_block = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_guard(target, ty, initializer, else_param, else_block, span);
        Some(())
    }

    fn parse_loop_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let block_id = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_loop(block_id, span);
        Some(())
    }

    fn parse_while_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let condition = self.parse_expr_id_arena_only(arena)?;
        let block_id = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_while(condition, block_id, span);
        Some(())
    }

    fn parse_for_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let target = self.parse_binding_target_arena_only("expected loop binding name", arena)?;
        self.expect_keyword(Keyword::In, "expected `in` in for loop");
        let iter = self.parse_expr_id_arena_only(arena)?;
        let block_id = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_for_id(target, iter, block_id, span);
        Some(())
    }

    fn parse_if_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let condition = self.parse_expr_id_arena_only(arena)?;
        let block_id = self.parse_block_arena_only(arena)?;
        let mut branch_ids = vec![(condition, block_id)];
        let mut else_block_id = None;
        while self.consume_keyword(Keyword::Else).is_some() {
            if self.consume_keyword(Keyword::If).is_some() {
                let condition = self.parse_expr_id_arena_only(arena)?;
                let block_id = self.parse_block_arena_only(arena)?;
                branch_ids.push((condition, block_id));
            } else {
                else_block_id = Some(self.parse_block_arena_only(arena)?);
                break;
            }
        }
        let span = self.span(start, self.previous_end());
        arena.push_if(&branch_ids, else_block_id, span);
        Some(())
    }

    pub(super) fn parse_binding_target_arena_only(
        &mut self,
        message: &str,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<BindingTargetId> {
        if self.at(TokenKindMatch::LBrace) {
            return self.parse_destructure_target_arena_only(arena);
        }
        let name = self.expect_ident(message)?;
        Some(arena.push_binding_target_name(name))
    }

    pub(super) fn parse_destructure_target_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<BindingTargetId> {
        self.bump();
        self.skip_newlines();
        let mut rest = false;
        arena.begin_destructure_fields();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            if self.at(TokenKindMatch::Dot) && self.peek_tag(1) == Some(TokenTag::Dot) {
                self.bump();
                self.bump();
                rest = true;
            } else {
                let start = self.current_start();
                let Some(name) = self.expect_ident("expected destructured field name") else {
                    arena.discard_destructure_fields();
                    return None;
                };
                arena.push_destructure_field(name, self.span(start, self.previous_end()));
            }
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        self.expect(
            TokenKindMatch::RBrace,
            "expected `}` after destructuring target",
        );
        let fields = arena.finish_destructure_fields();
        Some(arena.push_binding_target_record(fields, rest))
    }

    fn parse_guarded_stmt_arena_only(
        &mut self,
        start: usize,
        inner: crate::syntax::arena::StmtId,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let negate = self.consume_keyword(Keyword::Unless).is_some();
        if !negate {
            self.expect_keyword(Keyword::When, "expected `when` or `unless`");
        }
        let condition = self.parse_expr_id_arena_only(arena)?;
        let end = self.expect_terminator();
        arena.push_guarded_stmt(inner, negate, condition, self.span(start, end));
        Some(())
    }

    fn parse_return_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        if self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless) {
            let inner = arena.push_return(None, self.span(start, self.previous_end()));
            return self.parse_guarded_stmt_arena_only(start, inner, arena);
        }
        if self.at_terminator() {
            let end = self.expect_terminator();
            arena.push_return(None, self.span(start, end));
            return Some(());
        }
        let value = self.parse_expr_or_run_arena_only(arena)?;
        let end = self.expect_terminator();
        arena.push_return(Some(value), self.span(start, end));
        Some(())
    }

    fn parse_yield_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        if self.at_terminator() {
            self.diagnostic_here("`yield` requires a value", "parse.required-value");
            self.expect_terminator();
            return None;
        }
        let value = self.parse_expr_or_run_arena_only(arena)?;
        let end = self.expect_terminator();
        arena.push_yield(value, self.span(start, end));
        Some(())
    }

    fn parse_defer_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let value = self.parse_expr_or_run_arena_only(arena)?;
        let end = self.expect_terminator();
        arena.push_defer(value, self.span(start, end));
        Some(())
    }

    fn parse_loop_control_arena_only(
        &mut self,
        start: usize,
        is_break: bool,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        if !is_break {
            if self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless) {
                let inner = arena.push_continue(self.span(start, self.previous_end()));
                return self.parse_guarded_stmt_arena_only(start, inner, arena);
            }
            let end = self.expect_terminator();
            arena.push_continue(self.span(start, end));
            return Some(());
        }
        if self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless) {
            let inner = arena.push_break(None, self.span(start, self.previous_end()));
            return self.parse_guarded_stmt_arena_only(start, inner, arena);
        }
        if self.at_terminator() {
            let end = self.expect_terminator();
            arena.push_break(None, self.span(start, end));
            return Some(());
        }
        let value = self.parse_expr_id_arena_only(arena)?;
        let end = self.expect_terminator();
        arena.push_break(Some(value), self.span(start, end));
        Some(())
    }

    fn parse_with_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump(); // consume `with`
        self.skip_newlines();
        let mut bindings = Vec::new();
        loop {
            if self.at(TokenKindMatch::LBrace) {
                break;
            }
            let binding_start = self.current_start();
            let name = self.expect_ident("expected binding name in `with`")?;
            self.expect(
                TokenKindMatch::Equals,
                "expected `=` after binding name in `with`",
            );
            let prev_comma = self.comma_is_terminator;
            self.comma_is_terminator = true;
            let initializer = self.parse_expr_id_arena_only(arena);
            self.comma_is_terminator = prev_comma;
            let initializer = initializer?;
            bindings.push((
                name,
                initializer,
                self.span(binding_start, self.previous_end()),
            ));
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        let body = self.parse_block_arena_only(arena)?;
        self.skip_newlines();
        self.expect_keyword(Keyword::Else, "expected `else` after `with` body")?;
        let else_param = if self.consume(TokenKindMatch::Pipe).is_some() {
            let param = self.expect_ident("expected parameter name in `else |param|`");
            self.expect(TokenKindMatch::Pipe, "expected `|` after else parameter");
            param
        } else {
            None
        };
        let else_block = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        let bindings_range = arena.push_with_bindings(&bindings);
        arena.push_with(bindings_range, body, else_param, else_block, span);
        Some(())
    }

    fn parse_match_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let value = self.parse_expr_id_arena_only(arena)?;
        self.expect(TokenKindMatch::LBrace, "expected `{` to start match arms")?;
        self.skip_separators();
        arena.begin_match_arms();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            if self.parse_match_arm_arena_only(arena).is_none() {
                self.recover_match_arm();
            }
            self.skip_separators();
        }
        let end = self
            .expect(TokenKindMatch::RBrace, "expected `}` to close match")
            .map(|span| span.end())
            .unwrap_or_else(|| self.current_end());
        let span = self.span(start, end);
        let arms = arena.finish_match_arms();
        arena.push_match(value, arms, span);
        Some(())
    }

    fn parse_match_arm_arena_only(&mut self, arena: &mut ArenaProgramBuilder<'_>) -> Option<()> {
        let start = self.current_start();
        let (pattern, _pattern_span) = self.parse_pattern_arena_only(arena)?;
        let guard = if self.consume_keyword(Keyword::If).is_some() {
            Some(self.parse_expr_id_arena_only(arena)?)
        } else {
            None
        };
        self.expect(TokenKindMatch::FatArrow, "expected `=>` in match arm");
        let block_id = if self.at(TokenKindMatch::LBrace) {
            self.parse_block_arena_only(arena)?
        } else {
            let stmt_start = self.current_start();
            let previous = self.comma_is_terminator;
            self.comma_is_terminator = true;
            arena.begin_block();
            let stmt = self.parse_statement_arena_only(arena);
            self.comma_is_terminator = previous;
            if stmt.is_none() {
                arena.discard_block();
                return None;
            }
            let block_span = self.span(stmt_start, self.previous_end());
            if let Some(name) = arena.current_block_tail_bare_ident_name() {
                arena.mark_current_tail_bare_ident(name);
            }
            arena.finish_block(&[], block_span)
        };
        let arm_end = self.previous_end();
        if self.consume(TokenKindMatch::Comma).is_some() {
            self.skip_newlines();
        }
        let span = self.span(start, arm_end);
        arena.push_match_arm_input_id(pattern, guard, block_id, span);
        Some(())
    }

    fn parse_expr_statement_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let expr_id = self.parse_expr_id_arena_only(arena)?;
        let end = self.expect_terminator();
        arena.push_expr_statement(expr_id, self.span(start, end));
        Some(())
    }

    pub(super) fn parse_block_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<crate::syntax::arena::BlockId> {
        let start = self.expect(TokenKindMatch::LBrace, "expected `{` to start block")?;
        let params = self.parse_block_params();
        arena.begin_block();
        self.block_depth += 1;
        self.skip_separators();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            if self.parse_statement_arena_only(arena).is_none() {
                self.recover_statement();
            }
            self.skip_separators();
        }
        self.block_depth -= 1;
        let end = self
            .expect(TokenKindMatch::RBrace, "expected `}` to close block")
            .map(|span| span.end())
            .unwrap_or_else(|| self.current_end());
        let span = self.span(start.start(), end);
        if let Some(name) = arena.current_block_tail_bare_ident_name() {
            arena.mark_current_tail_bare_ident(name);
        }
        Some(arena.finish_block(&params, span))
    }

    pub(super) fn parse_block_params(&mut self) -> Vec<BlockParam> {
        if self.consume(TokenKindMatch::Pipe).is_none() {
            return Vec::new();
        }

        let mut params = Vec::new();
        self.skip_newlines();
        while !self.at(TokenKindMatch::Pipe) && !self.at(TokenKindMatch::Eof) {
            let start = self.current_start();
            let Some(name) = self.expect_ident("expected block parameter name") else {
                break;
            };
            params.push(BlockParam {
                name,
                span: self.span(start, self.previous_end()),
            });
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        self.expect(TokenKindMatch::Pipe, "expected `|` after block parameters");
        params
    }

    pub(super) fn parse_builder_block_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<BuilderBlockId> {
        let start = self.expect(
            TokenKindMatch::LBrace,
            "expected `{` to start builder block",
        )?;
        arena.begin_builder_entries();
        self.skip_separators();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            if self.parse_builder_entry_arena_only(arena).is_none() {
                self.recover_statement();
            }
            self.skip_separators();
        }
        let end = self
            .expect(
                TokenKindMatch::RBrace,
                "expected `}` to close builder block",
            )
            .map(|span| span.end())
            .unwrap_or_else(|| self.current_end());
        Some(arena.finish_builder_block(self.span(start.start(), end)))
    }

    fn parse_builder_entry_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.skip_comments();
        let start = self.current_start();
        match (self.current_tag(), self.current_keyword()) {
            (
                TokenTag::Keyword,
                Some(
                    Keyword::Let
                    | Keyword::Var
                    | Keyword::Return
                    | Keyword::Defer
                    | Keyword::If
                    | Keyword::While
                    | Keyword::For
                    | Keyword::Loop
                    | Keyword::Guard
                    | Keyword::Break
                    | Keyword::Continue
                    | Keyword::Match
                    | Keyword::Run,
                ),
            ) => {
                self.parse_statement_arena_only(arena)?;
                let stmt_id = arena.pop_last_statement();
                let span = self.span(start, self.previous_end());
                let entry = arena.build_builder_entry(ArenaBuilderEntryKind::Stmt(stmt_id), span);
                arena.push_builder_entry_input(entry);
                return Some(());
            }
            (TokenTag::Ident | TokenTag::ProcIdent, _)
                if self.current_name().is_some_and(|name| name == "task")
                    && matches!(
                        self.peek_tag(1),
                        Some(TokenTag::Ident | TokenTag::ProcIdent)
                    ) =>
            {
                self.bump();
                let name = self.expect_proc_ident("expected task name")?;
                if self.consume(TokenKindMatch::LParen).is_some() {
                    while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
                        self.bump();
                    }
                    self.expect(TokenKindMatch::RParen, "expected `)` after task signature");
                }
                let block = self.parse_block_arena_only(arena)?;
                let span = self.span(start, self.previous_end());
                let entry =
                    arena.build_builder_entry(ArenaBuilderEntryKind::Task { name, block }, span);
                arena.push_builder_entry_input(entry);
                return Some(());
            }
            (TokenTag::Ident, _) if self.lookahead_is_assignment() => {
                let name = self.expect_ident("expected builder field name")?;
                self.expect(TokenKindMatch::Equals, "expected `=` in builder field");
                let value = self.parse_expr_id_arena_only(arena)?;
                let end = self.expect_terminator();
                let span = self.span(start, end);
                let entry =
                    arena.build_builder_entry(ArenaBuilderEntryKind::Field { name, value }, span);
                arena.push_builder_entry_input(entry);
                return Some(());
            }
            (TokenTag::Ident | TokenTag::ProcIdent, _) => {}
            _ => {
                self.diagnostic_here("expected builder entry", "parse.expected-builder-entry");
                return None;
            }
        }

        let name = self.parse_command_name()?;
        let args = self.parse_command_args_arena_only(true, arena);
        let block = if self.at(TokenKindMatch::LBrace) {
            Some(self.parse_builder_block_arena_only(arena)?)
        } else {
            None
        };
        let end = if block.is_some() {
            self.previous_end()
        } else {
            self.expect_terminator()
        };
        let span = self.span(start, end);
        let entry =
            arena.build_builder_entry(ArenaBuilderEntryKind::Entry { name, args, block }, span);
        arena.push_builder_entry_input(entry);
        Some(())
    }
}
