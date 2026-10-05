#![allow(clippy::single_call_fn)]

use super::{
    AssignOp, BlockParam, Diagnostic, DurationLiteral, Effect, FixHint, IntLiteral, Keyword, Label,
    Name, Parser, SignalHookOptions, TokenKindMatch, TokenTag, result_unit_type_expr,
    unknown_type_expr,
};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{
    ArenaBuilderEntryKind, ArenaErrorVariant, ArenaExprOrRun, ArenaModuleContractEntryKind,
    ArenaProgramBuilder, ArenaTypeDefBody, BindingTargetId, BuilderBlockId, DeferTrigger, ExprId,
    TypeExprId,
};
use crate::syntax::grammar::{self, StatementForm};
use std::str::FromStr;

impl<'a> Parser<'a> {
    pub(super) fn parse_statement_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.skip_comments();
        let start = self.current_start();
        if let Some(form) = self.current_keyword().and_then(grammar::statement_form) {
            return match form {
                StatementForm::Binding => {
                    self.parse_binding_arena_only(start, !self.at_keyword(Keyword::Var), arena)
                }
                StatementForm::Run => self.parse_command_statement_arena_only(start, arena),
                StatementForm::Assert => self.parse_assert_arena_only(start, arena),
                StatementForm::If => self.parse_if_arena_only(start, arena),
                StatementForm::While => self.parse_while_arena_only(start, arena),
                StatementForm::For => self.parse_for_arena_only(start, arena),
                StatementForm::Loop => self.parse_loop_arena_only(start, arena),
                StatementForm::Return => self.parse_return_arena_only(start, arena),
                StatementForm::Yield => self.parse_yield_arena_only(start, arena),
                StatementForm::Defer => self.parse_defer_arena_only(start, arena),
                StatementForm::Break => self.parse_loop_control_arena_only(start, true, arena),
                StatementForm::Continue => self.parse_loop_control_arena_only(start, false, arena),
                StatementForm::Match => self.parse_match_arena_only(start, arena),
                StatementForm::Proc => self.parse_function_arena_only(start, true, arena),
                StatementForm::Pure => self.parse_function_arena_only(start, false, arena),
                StatementForm::Stream => self.parse_stream_function_arena_only(start, arena),
                StatementForm::Use => self.parse_use_arena_only(start, arena),
                StatementForm::Guard => self.parse_guard_arena_only(start, arena),
                StatementForm::With => self.parse_with_arena_only(start, arena),
                StatementForm::Enum => self.parse_enum_def_arena_only(start, arena),
                StatementForm::Type => self.parse_type_def_arena_only(start, arena),
                StatementForm::Export => self.parse_export_arena_only(start, arena),
            };
        }
        match (self.current_tag(), self.current_keyword()) {
            (TokenTag::Ident | TokenTag::ProcIdent, _) => {
                if self.lookahead_is_repeat() {
                    return self.parse_repeat_arena_only(start, arena);
                }
                if self.lookahead_is_without() {
                    return self.parse_without_arena_only(start, arena);
                }
                if self.lookahead_is_tempdir() {
                    return self.parse_tempdir_arena_only(start, arena);
                }
                if self.lookahead_is_atomically() {
                    return self.parse_atomically_arena_only(start, arena);
                }
                if self.current_name().is_some_and(|name| name == "env")
                    && self.peek_tag(1) == Some(TokenTag::LBrace)
                {
                    let saved = self.index;
                    self.bump();
                    let legacy = self.lookahead_is_env_expr_assignment_block();
                    self.index = saved;
                    if legacy {
                        let scope = self.parse_legacy_env_scope_arena_only(arena)?;
                        let value = if self.consume(TokenKindMatch::Question).is_some() {
                            arena.push_try_expr(scope, self.span(start, self.previous_end()))
                        } else {
                            scope
                        };
                        let end = self.expect_terminator();
                        arena.push_expr_statement(value, self.span(start, end));
                        return Some(());
                    }
                }
                if self
                    .current_name()
                    .is_some_and(|name| name == "env" || name == "cd")
                    && self.lookahead_is_context_scope()
                {
                    let scope = self.parse_context_scope_arena_only(arena, false)?;
                    let value = if self.consume(TokenKindMatch::Question).is_some() {
                        arena.push_try_expr(scope.id, self.span(start, self.previous_end()))
                    } else {
                        scope.id
                    };
                    let end = self.expect_terminator();
                    arena.push_expr_statement(value, self.span(start, end));
                    return Some(());
                }
                if self.lookahead_is_tempdir_scope() {
                    let scope = self.parse_tempdir_scope_arena_only(arena, false)?;
                    let value = if self.consume(TokenKindMatch::Question).is_some() {
                        arena.push_try_expr(scope.id, self.span(start, self.previous_end()))
                    } else {
                        scope.id
                    };
                    let end = self.expect_terminator();
                    arena.push_expr_statement(value, self.span(start, end));
                    return Some(());
                }
                if self.current_name().is_some_and(|name| name == "cli")
                    && self.peek_tag(1) == Some(TokenTag::Ident)
                    && self.peek_tag(2) == Some(TokenTag::LParen)
                {
                    self.parse_function_arena_only(start, true, arena)?;
                    arena.mark_last_function_as_cli_main();
                    if self.block_depth != 0 {
                        self.diagnostic_at(
                            self.span(start, self.previous_end()),
                            "`cli main` must be declared at the entry module's top level",
                            DiagnosticCode::ParseCliEntryScope,
                        );
                    }
                    Some(())
                } else if self.current_name().is_some_and(|name| name == "test")
                    && matches!(
                        self.peek_tag(1),
                        Some(TokenTag::Ident | TokenTag::ProcIdent)
                    )
                {
                    self.parse_test_declaration_arena_only(start, arena)
                } else if self.lookahead_is_ctx_block() {
                    self.parse_expr_statement_arena_only(start, arena)
                } else if self.lookahead_is_error_def() {
                    self.parse_error_def_arena_only(start, arena)
                } else if self.current_name().is_some_and(|name| name == "on")
                    && self.lookahead_is_signal_hook()
                {
                    self.parse_signal_hook_arena_only(start, arena)
                } else if let Some(operator) = self.lookahead_increment() {
                    let operator_span = self.span(self.peek_start(1)?, self.peek_end(2)?);
                    let replacement = if operator == "++" { " += 1" } else { " -= 1" };
                    self.diagnostics.push(
                        Diagnostic::error(format!("XSH has no `{operator}` operator"))
                            .with_code(DiagnosticCode::ParseForeignSyntax)
                            .with_label(Label::primary(
                                operator_span,
                                format!("write `{}{replacement}`", self.current_name()?),
                            ))
                            .with_fix_hint(FixHint::replacement(
                                operator_span,
                                format!("use `{}`", replacement.trim()),
                                replacement,
                            )),
                    );
                    None
                } else if let Some(spelling) = self.shell_declaration_keyword() {
                    let span = self.current_span();
                    self.diagnostics.push(
                        Diagnostic::error(format!(
                            "XSH declares variables with `let` or `var`, not `{spelling}`"
                        ))
                        .with_code(DiagnosticCode::ParseForeignSyntax)
                        .with_label(Label::primary(
                            span,
                            "write `let name = value`, or `var` to allow reassignment",
                        ))
                        .with_fix_hint(FixHint::replacement(span, "replace with `let`", "let")),
                    );
                    self.parse_binding_arena_only(start, true, arena)
                } else if let Some(spelling) = self.foreign_function_keyword() {
                    self.report_foreign_function_keyword(spelling);
                    None
                } else if self.lookahead_is_assignment() {
                    self.parse_assignment_arena_only(start, arena)
                } else if self.lookahead_is_dotted_command()
                    // `cd /tmp { ... }`: a bare path before the block is the
                    // scope's directory, not a division.
                    || (self.current_name().is_some_and(|name| name == "cd") && (self.command_line_has_block() || self.lookahead_is_blockless_cd()))
                {
                    self.parse_command_statement_arena_only(start, arena)
                } else if self.lookahead_is_expr_call_or_postfix()
                    || self.lookahead_is_expr_binary()
                {
                    self.parse_expr_statement_arena_only(start, arena)
                } else if self.lookahead_is_exit() {
                    self.parse_exit_arena_only(start, arena)
                } else if self.lookahead_is_fail() {
                    self.parse_fail_arena_only(start, arena)
                } else {
                    self.parse_command_statement_arena_only(start, arena)
                }
            }
            (TokenTag::EnvString, _) if self.lookahead_is_assignment() => {
                self.parse_assignment_arena_only(start, arena)
            }
            _ => self.parse_expr_statement_arena_only(start, arena),
        }
    }

    fn parse_assert_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let condition = self.parse_expr_id_arena_only(arena)?;
        let message = match self.consume(TokenKindMatch::Comma) {
            Some(_) => Some(self.parse_expr_id_arena_only(arena)?),
            None => None,
        };
        let end = self.expect_terminator();
        arena.push_assert(condition, message, self.span(start, end));
        Some(())
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
        let exported = self
            .current_keyword()
            .filter(|keyword| grammar::EXPORTABLE_KEYWORDS.contains(keyword))
            .and_then(grammar::statement_form);
        match (self.current_tag(), exported) {
            (_, Some(StatementForm::Binding)) => {
                self.parse_binding_arena_only(start, true, arena)?
            }
            (_, Some(StatementForm::Proc)) => self.parse_function_arena_only(start, true, arena)?,
            (_, Some(StatementForm::Pure)) => {
                self.parse_function_arena_only(start, false, arena)?
            }
            (_, Some(StatementForm::Stream)) => {
                self.parse_stream_function_arena_only(start, arena)?
            }
            (_, Some(StatementForm::Enum)) => self.parse_enum_def_arena_only(start, arena)?,
            (_, Some(StatementForm::Type)) => self.parse_type_def_arena_only(start, arena)?,
            (TokenTag::Ident, _)
                if self.current_name().is_some_and(|name| name == "on")
                    && self.lookahead_is_signal_hook() =>
            {
                self.parse_signal_hook_arena_only(start, arena)?
            }
            (TokenTag::Ident, _) if self.lookahead_is_error_def() => {
                self.parse_error_def_arena_only(start, arena)?
            }
            _ => {
                let message = "`export` applies only to const, let, proc, pure, stream, type, enum, or error definitions";
                let mut diagnostic = Diagnostic::error(message)
                    .with_code(DiagnosticCode::ParseExportTarget)
                    .with_label(Label::primary(self.current_span(), message));
                if matches!(self.current_tag(), TokenTag::Ident)
                    && self.peek_tag(1) == Some(TokenTag::Equals)
                {
                    diagnostic = diagnostic.with_note("XSH `export` publishes module definitions; set an environment variable for commands with `env NAME=value { ... }`");
                }
                self.diagnostics.push(diagnostic);
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
        let introducer_start = self.current_start();
        self.bump();
        let name = self.expect_ident("expected type name")?;
        let mut parameters = Vec::new();
        if self.consume(TokenKindMatch::LBracket).is_some() {
            parameters.push(self.expect_ident("expected record type parameter")?);
            while self.consume(TokenKindMatch::Comma).is_some() {
                parameters.push(self.expect_ident("expected record type parameter")?);
            }
            self.expect(
                TokenKindMatch::RBracket,
                "expected `]` after record type parameters",
            );
        }
        self.expect(TokenKindMatch::Equals, "expected `=` in type definition");
        let body_start = self.index;
        // `exact` is a word only here, directly before `module`; anywhere
        // else it stays an ordinary name, so `type T = exact` aliases a type.
        let exact = self.at_ident("exact")
            && self.peek_tag(1) == Some(TokenTag::Ident)
            && self.peek_name(1).is_some_and(|name| name == "module");
        let body = if exact || self.at_ident("module") {
            if exact {
                self.bump();
            }
            self.bump();
            ArenaTypeDefBody::ModuleContract {
                entries: self.parse_module_contract_arena_only(arena)?,
                exact,
            }
        } else if self.at(TokenKindMatch::LBrace) {
            ArenaTypeDefBody::RecordSchema(self.parse_record_schema_arena_only(arena)?)
        } else if let Some(variants) = self.recover_legacy_tag_union_arena_only(arena) {
            let body_end = self.previous_end();
            let mut diagnostic = Diagnostic::error(
                "tagged unions use `enum Name { A, B }`; replace this `type` declaration",
            )
            .with_code(DiagnosticCode::ParseEnumMigration)
            .with_label(Label::primary(
                self.span(start, body_end),
                "use an explicit enum declaration",
            ))
            .with_fix_hint(FixHint::replacement(
                self.span(introducer_start, introducer_start + 4),
                "use enum",
                "enum",
            ));
            for index in body_start.saturating_sub(1)..self.index {
                let tag = self.token_table.tag_at(index);
                if tag == Some(TokenTag::Equals) || tag == Some(TokenTag::Pipe) {
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                        self.span_at(index)
                            .expect("migration delimiter token exists"),
                        "use enum delimiters",
                        if tag == Some(TokenTag::Equals) {
                            "{"
                        } else {
                            ","
                        },
                    ));
                }
            }
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                self.span(body_end, body_end),
                "close enum body",
                " }",
            ));
            self.diagnostics.push(diagnostic);
            ArenaTypeDefBody::TagUnion(variants)
        } else {
            ArenaTypeDefBody::Alias(self.parse_type_expr(arena)?)
        };
        let end = self.expect_terminator();
        let span = self.span(start, end);
        arena.push_parameterized_type_def(name, parameters, body, span);
        Some(())
    }

    fn skip_enum_trivia(&mut self) {
        while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
            self.bump();
        }
    }

    fn parse_enum_def_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let name = self.expect_ident("expected enum name")?;
        let wire_backed = self.consume(TokenKindMatch::Colon).is_some();
        if wire_backed {
            let backing = self.expect_ident("expected Str enum backing type")?;
            if backing != "Str" {
                self.diagnostic_previous(
                    "wire enums support only Str backing",
                    DiagnosticCode::ParseEnumBacking,
                );
            }
        }
        self.expect(TokenKindMatch::LBrace, "expected `{` after enum name")?;
        self.skip_enum_trivia();
        let mut variants = Vec::new();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            let variant_start = self.current_start();
            let variant_name = self.expect_ident("expected enum variant name")?;
            let mut fields = Vec::new();
            if self.consume(TokenKindMatch::LParen).is_some() {
                self.skip_enum_trivia();
                while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
                    fields.push(self.parse_type_expr(arena)?);
                    self.skip_enum_trivia();
                    if self.consume(TokenKindMatch::Comma).is_none() {
                        break;
                    }
                    self.skip_enum_trivia();
                }
                self.expect(
                    TokenKindMatch::RParen,
                    "expected `)` after enum payload types",
                )?;
            }
            let wire_value = if wire_backed {
                if !fields.is_empty() {
                    self.diagnostic_previous(
                        "Str-backed enum variants cannot have payload fields",
                        DiagnosticCode::ParseEnumWirePayload,
                    );
                }
                self.expect(
                    TokenKindMatch::Equals,
                    "every Str-backed enum variant requires `= constant_string`",
                )?;
                Some(self.parse_expr_id_arena_only(arena)?)
            } else {
                None
            };
            let mut variant = arena.build_tag_variant(
                variant_name,
                &fields,
                self.span(variant_start, self.previous_end()),
            );
            variant.wire_value = wire_value;
            variants.push(variant);
            self.skip_enum_trivia();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_enum_trivia();
        }
        self.expect(TokenKindMatch::RBrace, "expected `}` after enum variants")?;
        if variants.is_empty() {
            self.diagnostic_previous(
                "an enum requires at least one variant",
                DiagnosticCode::ParseEmptyEnum,
            );
        }
        let variants = arena.push_tag_variant_range(variants);
        let end = self.expect_terminator();
        arena.push_type_def(
            name,
            ArenaTypeDefBody::TagUnion(variants),
            self.span(start, end),
        );
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
            let name = self.expect_label_name("expected schema field label")?;
            self.expect(TokenKindMatch::Colon, "expected `:` after schema field");
            let ty_id = self.parse_type_expr(arena)?;
            let default = if self.consume(TokenKindMatch::Equals).is_some() {
                Some(self.parse_expr_id_arena_only(arena)?)
            } else {
                None
            };
            let span = self.span(start, self.previous_end());
            fields.push(arena.build_schema_field(name, ty_id, default, span));
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        self.expect(TokenKindMatch::RBrace, "expected `}` after schema");
        Some(arena.push_schema_field_range(fields))
    }

    // Recover only pipe-separated declarations so aliases retain their identity.
    // The recovered rows support checked migration edits; the accompanying parse
    // diagnostic prevents the declaration from reaching execution.
    fn recover_legacy_tag_union_arena_only(
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
        self.skip_enum_trivia();
        loop {
            self.skip_enum_trivia();
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
                self.skip_enum_trivia();
                let mut field_ids = Vec::new();
                while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
                    let Some(ty) = self.parse_type_expr(arena) else {
                        break;
                    };
                    field_ids.push(ty);
                    self.skip_enum_trivia();
                    if self.consume(TokenKindMatch::Comma).is_none() {
                        break;
                    }
                    self.skip_enum_trivia();
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
                self.skip_enum_trivia();
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
        if self.at(TokenKindMatch::LBracket) {
            // Recover past the parameter list so the variants still parse.
            let parameters_start = self.current_start();
            while !self.at(TokenKindMatch::Equals)
                && !self.at(TokenKindMatch::LBrace)
                && !self.at(TokenKindMatch::Eof)
            {
                self.bump();
            }
            self.diagnostic_at(
                self.span(parameters_start, self.previous_end()),
                "generic error families are not supported; declare a concrete error family and give payload fields concrete types",
                DiagnosticCode::ParseGenericErrorFamily,
            );
        }
        let variants = if self.at(TokenKindMatch::LBrace) {
            self.parse_error_variant_block_arena_only(arena)?
        } else {
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
                let Some(variant) = self.parse_error_variant_arena_only(arena)? else {
                    if variants.is_empty() {
                        self.diagnostic_here(
                            "expected error variant",
                            DiagnosticCode::ParseErrorVariant,
                        );
                    }
                    break;
                };
                variants.push(variant);
                if self.consume(TokenKindMatch::Pipe).is_none() {
                    break;
                }
                self.skip_newlines();
            }
            variants
        };
        let end = self.expect_terminator();
        let span = self.span(start, end);
        arena.push_error_def(name, variants, span);
        Some(())
    }

    /// The braced variant list of an error family: one variant per line.
    /// A newline is the only separator, because `,` already separates the
    /// facets that may end a variant's line.
    fn parse_error_variant_block_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<Vec<ArenaErrorVariant>> {
        self.expect(
            TokenKindMatch::LBrace,
            "expected `{` after error family name",
        )?;
        let mut variants = Vec::new();
        loop {
            self.skip_enum_trivia();
            if self.at(TokenKindMatch::RBrace) || self.at(TokenKindMatch::Eof) {
                break;
            }
            let Some(variant) = self.parse_error_variant_arena_only(arena)? else {
                self.diagnostic_here("expected error variant", DiagnosticCode::ParseErrorVariant);
                break;
            };
            variants.push(variant);
            if !matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment)
                && !self.at(TokenKindMatch::RBrace)
                && !self.at(TokenKindMatch::Eof)
            {
                self.diagnostic_here(
                    "each error variant in braces is on its own line, with no separator",
                    DiagnosticCode::ParseErrorVariant,
                );
                // Step over one stray separator so the remaining variants
                // are still declared and checked.
                if self.consume(TokenKindMatch::Comma).is_none()
                    && self.consume(TokenKindMatch::Pipe).is_none()
                    && !matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent)
                {
                    break;
                }
            }
        }
        self.expect(TokenKindMatch::RBrace, "expected `}` after error variants")?;
        if variants.is_empty() {
            self.diagnostic_previous(
                "an error family requires at least one variant",
                DiagnosticCode::ParseErrorVariant,
            );
        }
        Some(variants)
    }

    /// One variant with its payload fields and facets, or `None` when the
    /// current token cannot begin a variant.
    fn parse_error_variant_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<Option<ArenaErrorVariant>> {
        if !matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
            return Some(None);
        }
        let variant_start = self.current_start();
        let variant_name = self
            .current_name()
            .expect("error variant name token has payload");
        self.bump();
        let mut fields = Vec::new();
        if self.consume(TokenKindMatch::LParen).is_some() {
            self.skip_newlines();
            while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
                let field_start = self.current_start();
                let field_name = self.expect_label_name("expected error payload field")?;
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
        let span = self.span(variant_start, self.previous_end());
        Some(Some(arena.build_error_variant(
            variant_name,
            fields,
            &facets,
            span,
        )))
    }

    fn parse_binding_arena_only(
        &mut self,
        start: usize,
        immutable: bool,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let constant = self.at_keyword(Keyword::Const);
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
        if constant {
            arena.push_const_binding_parts(target, ty, initializer, self.span(start, end));
        } else {
            arena.push_binding_parts(immutable, target, ty, initializer, self.span(start, end));
        }
        Some(())
    }

    fn parse_expr_or_run_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaExprOrRun> {
        if self.at_keyword(Keyword::Run) {
            let (run_id, span) = self.parse_run_form_arena_only(arena)?;
            if self.at_run_pipeline() {
                return Some(ArenaExprOrRun::Expr(
                    self.parse_run_pipeline_arena_only(run_id, span, arena)?.id,
                ));
            }
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
        if self.current_tag() == TokenTag::EnvString {
            let span = self.bump();
            let name = self.env_string_name(span);
            return Some(arena.push_assign_target_env(name));
        }
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
                self.diagnostic_here(
                    "expected assignment operator",
                    DiagnosticCode::ParseExpectedToken,
                );
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
        let mut reported_untyped = false;
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
                                DiagnosticCode::ParseRestDefault,
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
                            DiagnosticCode::ParseRestDefault,
                        );
                    }
                    let default_start = self.current_start();
                    let Some(default_id) = self.parse_expr_id_arena_only(arena) else {
                        break;
                    };
                    let default_span = self.span(default_start, self.previous_end());
                    let ty_id = unknown_type_expr(arena, default_span);
                    (ty_id, true, Some(default_id), default_span.end())
                } else if matches!(self.current_tag(), TokenTag::Comma | TokenTag::RParen) {
                    // An untyped parameter list (`proc add(a, b)`) is one
                    // mistake: report its first parameter and keep parsing
                    // the signature and body.
                    let name_span = self.previous_span();
                    if !reported_untyped {
                        reported_untyped = true;
                        self.diagnostics.push(
                            Diagnostic::error("expected `:` or default value after parameter name")
                                .with_code(DiagnosticCode::ParseExpectedParamType)
                                .with_label(Label::primary(
                                    name_span,
                                    format!("parameters declare their types: write `{name}: Type`"),
                                )),
                        );
                    }
                    (
                        unknown_type_expr(arena, name_span),
                        false,
                        None,
                        name_span.end(),
                    )
                } else {
                    self.diagnostic_here(
                        "expected `:` or default value after parameter name",
                        DiagnosticCode::ParseExpectedParamType,
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

    fn parse_test_declaration_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let name = self.expect_ident("expected test name")?;
        if self.block_depth != 0 {
            self.diagnostic_here(
                "test declarations must be top-level",
                DiagnosticCode::ParseTestNested,
            );
        }
        let effects = self.parse_effect_list();
        let body = self.parse_block_arena_only(arena)?;
        if arena.block_parameter_count(body) > 1 {
            self.diagnostic_here(
                "test declarations accept at most one immutable TestContext parameter",
                DiagnosticCode::ParseTestParams,
            );
        }
        let effects = effects
            .as_deref()
            .map(|effects| arena.push_effects(effects));
        let span = self.span(start, self.previous_end());
        arena.push_test_declaration(name, effects, body, span);
        Some(())
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
                DiagnosticCode::ParseRequiredSignature,
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
            (
                arena.push_named_type_expr(Name::intern("Unit"), self.current_span()),
                true,
            )
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
                DiagnosticCode::ParseRequiredSignature,
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
                DiagnosticCode::ParseRequiredReturn,
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

    /// `error Name =`, `error Name {`, or either after `[...]`; the bracketed
    /// form is parsed only to report that generic error families are
    /// unsupported.
    fn lookahead_is_error_def(&self) -> bool {
        if self.current_name() != Some(Name::intern("error"))
            || !matches!(
                self.peek_tag(1),
                Some(TokenTag::Ident | TokenTag::ProcIdent)
            )
        {
            return false;
        }
        let mut index = self.index + 2;
        if self.token_table.tag_at(index) == Some(TokenTag::LBracket) {
            let mut depth = 0usize;
            loop {
                match self.token_table.tag_at(index) {
                    Some(TokenTag::LBracket) => depth += 1,
                    Some(TokenTag::RBracket) => {
                        depth -= 1;
                        if depth == 0 {
                            index += 1;
                            break;
                        }
                    }
                    Some(TokenTag::Newline | TokenTag::Eof) | None => return false,
                    Some(_) => {}
                }
                index += 1;
            }
        }
        matches!(
            self.token_table.tag_at(index),
            Some(TokenTag::Equals | TokenTag::LBrace)
        )
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
                self.diagnostic_here(
                    "expected signal name after `on`",
                    DiagnosticCode::ParseSignalHook,
                );
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
                    self.diagnostic_here(
                        "expected signal hook option name",
                        DiagnosticCode::ParseSignalHook,
                    );
                    break;
                }
            };
            match grammar::SignalHookOption::named(&name.as_str()) {
                Some(grammar::SignalHookOption::PreCancel) => {
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
                            DiagnosticCode::ParseSignalHook,
                        ),
                    }
                }
                _ => {
                    self.diagnostic_at(
                        option_span,
                        &format!("unknown signal hook option `--{name}`"),
                        DiagnosticCode::ParseSignalHook,
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
            self.diagnostic_here(
                "signal hooks require an effect list",
                DiagnosticCode::ParseSignalHook,
            );
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
                        DiagnosticCode::ParseUnknownEffect,
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
        let keyword = self.bump(); // consume `guard`
        if !self.at_keyword(Keyword::Let) {
            return self.parse_boolean_guard_arena_only(start, keyword, arena);
        }
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
        let else_block = self.parse_error_handler_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_guard(target, ty, initializer, else_block, span);
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
        let condition = self.parse_condition_arena_only(arena)?.id;
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
        let iter = self.parse_head_expr_arena_only(arena)?.id;
        let block_id = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_for_id(target, iter, block_id, span);
        Some(())
    }

    /// `name++` or `name--` as a whole statement.
    fn lookahead_increment(&self) -> Option<&'static str> {
        let operator = match (self.peek_tag(1)?, self.peek_tag(2)?) {
            (TokenTag::Plus, TokenTag::Plus) => "++",
            (TokenTag::Minus, TokenTag::Minus) => "--",
            _ => return None,
        };
        let adjacent =
            self.peek_start(1)? == self.current_end() && self.peek_start(2)? == self.peek_end(1)?;
        (adjacent
            && matches!(
                self.peek_tag(3),
                Some(
                    TokenTag::Newline
                        | TokenTag::Semicolon
                        | TokenTag::RBrace
                        | TokenTag::Eof
                        | TokenTag::Comment
                )
            ))
        .then_some(operator)
    }

    /// Shell `cd DIR` on its own line. The command form reports the missing
    /// block instead of reading `cd /tmp` as a division of two names.
    fn lookahead_is_blockless_cd(&self) -> bool {
        matches!(
            self.peek_tag(1),
            Some(
                TokenTag::Slash
                    | TokenTag::Dot
                    | TokenTag::PathString
                    | TokenTag::String
                    | TokenTag::DollarIdent
            )
        ) && !matches!(self.peek_tag(2), Some(TokenTag::Equals))
    }

    /// `local x=1` and friends declare a shell variable; parsing continues
    /// as the binding they stand for, so later uses of the name resolve.
    fn shell_declaration_keyword(&self) -> Option<&'static str> {
        let name = self.current_name()?;
        let spelling = ["local", "declare", "readonly", "typeset"]
            .into_iter()
            .find(|spelling| name == *spelling)?;
        (self.peek_tag(1) == Some(TokenTag::Ident) && self.peek_tag(2) == Some(TokenTag::Equals))
            .then_some(spelling)
    }

    /// `function name() {` (shell) and `def name(` (Python) at the start of a
    /// statement. Neither word is reserved, so only the declaration shape,
    /// a name followed by `(` or `{`, is treated as the foreign keyword.
    fn foreign_function_keyword(&self) -> Option<&'static str> {
        let name = self.current_name()?;
        let spelling = ["function", "def", "fn", "func"]
            .into_iter()
            .find(|spelling| name == *spelling)?;
        let named = matches!(
            self.peek_tag(1),
            Some(TokenTag::Ident | TokenTag::ProcIdent)
        );
        (named && matches!(self.peek_tag(2), Some(TokenTag::LParen | TokenTag::LBrace)))
            .then_some(spelling)
    }

    /// The statement is reported once and skipped with its body by statement
    /// recovery; its parameters would need XSH types anyway.
    fn report_foreign_function_keyword(&mut self, spelling: &str) {
        let span = self.current_span();
        self.diagnostics.push(
            Diagnostic::error(format!(
                "XSH declares functions with `proc` (or `pure`), not `{spelling}`"
            ))
            .with_code(DiagnosticCode::ParseForeignSyntax)
            .with_label(Label::primary(
                span,
                "write `proc name(param: Type) { ... }`",
            ))
            .with_fix_hint(FixHint::replacement(span, "replace with `proc`", "proc")),
        );
    }

    fn parse_if_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let condition = self.parse_condition_arena_only(arena)?.id;
        let block_id = self.parse_block_arena_only(arena)?;
        let mut branch_ids = vec![(condition, block_id)];
        let mut else_block_id = None;
        while let Some(implied_if) = self.consume_else() {
            if implied_if || self.consume_keyword(Keyword::If).is_some() {
                let condition = self.parse_condition_arena_only(arena)?.id;
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

    /// Consume the `else` that continues an `if`, returning whether it also
    /// stands for `if` (a misspelled `elif`). A statement never starts with
    /// `else`, so an `else` on the line after the closing `}` can only belong
    /// to this `if`: it is reported with a fix that joins the lines, and the
    /// parse continues as if it were written there. `else =>` is the one
    /// exception: it heads the catch-all arm of the match this `if` is an arm
    /// body of, and never continues the `if`.
    pub(super) fn consume_else(&mut self) -> Option<bool> {
        let mut offset = 0;
        while matches!(
            self.peek_tag(offset),
            Some(TokenTag::Newline | TokenTag::Comment)
        ) {
            offset += 1;
        }
        let is_else = self.peek_tag(offset) == Some(TokenTag::Keyword)
            && self.peek_keyword(offset) == Some(Keyword::Else)
            && self.peek_tag(offset + 1) != Some(TokenTag::FatArrow);
        let is_elif = self.peek_tag(offset) == Some(TokenTag::Ident)
            && self
                .peek_label_name(offset)
                .is_some_and(|name| name == "elif")
            && !matches!(
                self.peek_tag(offset + 1),
                Some(TokenTag::Equals | TokenTag::FatArrow | TokenTag::Newline | TokenTag::Eof)
            );
        if !is_else && !is_elif {
            return None;
        }
        if offset > 0 {
            let gap = self.span(self.previous_end(), self.peek_start(offset)?);
            let keyword = self.span(self.peek_start(offset)?, self.peek_end(offset)?);
            let mut diagnostic = Diagnostic::error(
                "`else` must be on the same line as the `}` that closes the `if` block",
            )
            .with_code(DiagnosticCode::ParseDetachedElse)
            .with_label(Label::primary(
                keyword,
                "a newline before `else` ends the `if` statement",
            ));
            if self.source[gap.range()].trim().is_empty() {
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    gap,
                    "join `else` to the closing `}`",
                    " ",
                ));
            }
            self.diagnostics.push(diagnostic);
            for _ in 0..offset {
                self.bump();
            }
        }
        if is_elif {
            let span = self.current_span();
            self.diagnostics.push(
                Diagnostic::error("XSH spells `elif` as `else if`")
                    .with_code(DiagnosticCode::ParseForeignSyntax)
                    .with_label(Label::primary(span, "write `else if`"))
                    .with_fix_hint(FixHint::replacement(
                        span,
                        "replace with `else if`",
                        "else if",
                    )),
            );
        }
        self.bump();
        Some(is_elif)
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
        let start = self.current_start();
        self.bump();
        while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
            self.bump();
        }
        let mut rest = false;
        arena.begin_destructure_fields();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            if self.at(TokenKindMatch::Dot) && self.peek_tag(1) == Some(TokenTag::Dot) {
                self.bump();
                self.bump();
                rest = true;
            } else {
                let start = self.current_start();
                let label_tag = self.current_tag();
                let label_span = self.current_span();
                let Some(name) = self.expect_label_name("expected destructured field label") else {
                    arena.discard_destructure_fields();
                    return None;
                };
                let target = if self.consume(TokenKindMatch::Colon).is_some() {
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                        self.bump();
                    }
                    let Some(target) = self.parse_binding_target_arena_only(
                        "expected binding name or record target",
                        arena,
                    ) else {
                        arena.discard_destructure_fields();
                        return None;
                    };
                    target
                } else {
                    if !self.require_label_binding_name(label_tag, label_span) {
                        arena.discard_destructure_fields();
                        return None;
                    }
                    arena.push_binding_target_name(name)
                };
                arena.push_destructure_field(name, target, self.span(start, self.previous_end()));
            }
            while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                self.bump();
            }
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                self.bump();
            }
        }
        self.expect(
            TokenKindMatch::RBrace,
            "expected `}` after destructuring target",
        );
        let fields = arena.finish_destructure_fields();
        Some(arena.push_binding_target_record(fields, rest, self.span(start, self.previous_end())))
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
        if self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless) {
            let inner = arena.push_return(Some(value), self.span(start, self.previous_end()));
            return self.parse_guarded_stmt_arena_only(start, inner, arena);
        }
        let end = self.expect_terminator();
        arena.push_return(Some(value), self.span(start, end));
        Some(())
    }

    /// Whether the command statement at the cursor is `exit STATUS`. `exit`
    /// is not reserved: it begins the statement only where a command named
    /// `exit` would be read, with its status on the same line.
    fn lookahead_is_exit(&self) -> bool {
        self.current_name().is_some_and(|name| name == "exit")
            && !matches!(
                self.peek_tag(1),
                None | Some(
                    TokenTag::Newline
                        | TokenTag::Semicolon
                        | TokenTag::RBrace
                        | TokenTag::Comment
                        | TokenTag::Eof
                )
            )
    }

    fn parse_exit_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let status = self.parse_expr_id_arena_only(arena)?;
        if self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless) {
            let inner = arena.push_exit(status, self.span(start, self.previous_end()));
            return self.parse_guarded_stmt_arena_only(start, inner, arena);
        }
        let end = self.expect_terminator();
        arena.push_exit(status, self.span(start, end));
        Some(())
    }

    fn parse_yield_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        if self.at_terminator() {
            self.diagnostic_here(
                "`yield` requires a value",
                DiagnosticCode::ParseRequiredValue,
            );
            self.expect_terminator();
            return None;
        }
        if self.consume(TokenKindMatch::At).is_some() {
            let value = self.parse_expr_id_arena_only(arena)?;
            if self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless) {
                let inner = arena.push_yield_delegate(value, self.span(start, self.previous_end()));
                return self.parse_guarded_stmt_arena_only(start, inner, arena);
            }
            let end = self.expect_terminator();
            arena.push_yield_delegate(value, self.span(start, end));
            return Some(());
        }
        let value = self.parse_expr_or_run_arena_only(arena)?;
        if self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless) {
            let inner = arena.push_yield(value, self.span(start, self.previous_end()));
            return self.parse_guarded_stmt_arena_only(start, inner, arena);
        }
        let end = self.expect_terminator();
        arena.push_yield(value, self.span(start, end));
        Some(())
    }

    fn parse_defer_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let trigger = if self.current_keyword() == Some(Keyword::Errdefer) {
            DeferTrigger::Error
        } else {
            DeferTrigger::Exit
        };
        self.bump();
        let value = if self.at(TokenKindMatch::LBrace) {
            let block_start = self.current_start();
            let block = self.parse_block_arena_only(arena)?;
            ArenaExprOrRun::Expr(
                arena.push_value_block_expr(block, self.span(block_start, self.previous_end())),
            )
        } else {
            self.parse_expr_or_run_arena_only(arena)?
        };
        let end = self.expect_terminator();
        arena.push_defer(value, trigger, self.span(start, end));
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
        if self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless) {
            let inner = arena.push_break(Some(value), self.span(start, self.previous_end()));
            return self.parse_guarded_stmt_arena_only(start, inner, arena);
        }
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
            let initializer = self.parse_head_expr_arena_only(arena).map(|expr| expr.id);
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
        let else_block = self.parse_error_handler_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        let bindings_range = arena.push_with_bindings(&bindings);
        arena.push_with(bindings_range, body, else_block, span);
        Some(())
    }

    fn parse_match_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let value = self.parse_head_expr_arena_only(arena)?.id;
        self.expect(TokenKindMatch::LBrace, "expected `{` to start match arms")?;
        self.skip_separators();
        arena.begin_match_arms();
        let mut else_arm = None;
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            if self
                .parse_match_arm_arena_only(arena, &mut else_arm)
                .is_none()
            {
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

    fn parse_match_arm_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        else_arm: &mut Option<crate::source::Span>,
    ) -> Option<()> {
        let start = self.current_start();
        let (pattern, guard, spelling) = self.parse_match_arm_head_arena_only(arena, else_arm)?;
        let block_id = if self.at(TokenKindMatch::LBrace) && !self.brace_starts_field_record() {
            self.parse_block_arena_only(arena)?
        } else {
            let stmt_start = self.current_start();
            let previous = self.comma_is_terminator;
            self.comma_is_terminator = true;
            let outer_arm_body = self.arm_body_start.replace(self.index);
            arena.begin_block();
            let stmt = self.parse_statement_arena_only(arena);
            self.arm_body_start = outer_arm_body;
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
        arena.push_match_arm_input_id(pattern, guard, block_id, spelling, span);
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

    fn parse_error_handler_block_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<crate::syntax::arena::BlockId> {
        self.skip_newlines();
        if !self.at(TokenKindMatch::Pipe) {
            return self.parse_block_arena_only(arena);
        }
        let start = self.current_start();
        let params = self.parse_block_params();
        let header_end = self.previous_end();
        self.skip_newlines();
        let brace_start = self.current_start();
        let brace_end = self.current_end();
        let mut diagnostic = Diagnostic::error(
            "put error-handler parameters inside the block: `else { |failure| ... }`",
        )
        .with_code(DiagnosticCode::ParseBlockHeaderMigration)
        .with_label(Label::primary(
            self.span(start, header_end),
            "move this header after `{`",
        ));
        if self.at(TokenKindMatch::LBrace) && !self.source[start..brace_end].contains('#') {
            let between = self.source[header_end..brace_start].trim_end_matches([' ', '\t']);
            let replacement = format!("{{ {}{between}", &self.source[start..header_end]);
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                self.span(start, brace_end),
                "move the header inside the block",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
        let block = self.parse_block_arena_only(arena)?;
        if arena.block_parameter_count(block) == 0 {
            arena.recover_block_parameters(block, &params);
        } else {
            self.diagnostics.push(
                Diagnostic::error("an error handler cannot have two parameter headers")
                    .with_code(DiagnosticCode::ParseBlockParams)
                    .with_label(Label::primary(
                        self.span(start, brace_end),
                        "remove the outside header",
                    )),
            );
        }
        Some(block)
    }

    pub(super) fn parse_block_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<crate::syntax::arena::BlockId> {
        self.parse_block_with_params_arena_only(arena, None)
    }

    /// `bound` is a parameter the surrounding form already named (the binder
    /// of `tempdir NAME { ... }`); such a block cannot also write `|...|`.
    pub(super) fn parse_block_with_params_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        bound: Option<BlockParam>,
    ) -> Option<crate::syntax::arena::BlockId> {
        let start = self.expect(TokenKindMatch::LBrace, "expected `{` to start block")?;
        self.skip_separators();
        let params = match bound {
            None => self.parse_block_params(),
            Some(bound) => {
                if self.at(TokenKindMatch::Pipe) {
                    let pipe = self.current_span();
                    self.parse_block_params();
                    self.diagnostic_at(
                        self.span(pipe.start(), self.previous_end()),
                        "this block's parameter is the name before it",
                        DiagnosticCode::ParseBlockParams,
                    );
                }
                vec![bound]
            }
        };
        arena.begin_block();
        self.block_depth += 1;
        self.skip_separators();
        self.in_nested_group(|parser| {
            while !parser.at(TokenKindMatch::RBrace) && !parser.at(TokenKindMatch::Eof) {
                if parser.parse_statement_arena_only(arena).is_none() {
                    parser.recover_statement();
                }
                parser.skip_separators();
            }
        });
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
        self.in_nested_group(|parser| {
            while !parser.at(TokenKindMatch::RBrace) && !parser.at(TokenKindMatch::Eof) {
                if parser.parse_builder_entry_arena_only(arena).is_none() {
                    parser.recover_statement();
                }
                parser.skip_separators();
            }
        });
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
            (TokenTag::Keyword, Some(keyword))
                if grammar::BUILDER_STATEMENT_KEYWORDS.contains(&keyword) =>
            {
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
                self.diagnostic_here(
                    "expected builder entry",
                    DiagnosticCode::ParseExpectedBuilderEntry,
                );
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
