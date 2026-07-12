use super::{
    CoreCommand, Diagnostic, DurationLiteral, FixHint, IntLiteral, Keyword, Label, Name, Parser,
    RedirectionKind, RunKind, Severity, Span, TokenKindMatch, TokenTag,
    decode_interpolation_text_for, dollar_shorthand_end, interpolation_diagnostic, is_ident_start,
    literal, parse_interpolation_expr_arena_only_for,
};
use crate::syntax::arena::{
    ArenaCommand, ArenaCommandArg, ArenaEnvAssignmentValue, ArenaProgramBuilder, ArenaRange,
    ArenaRedirectionTarget, ExprId, RunFormId,
};
use std::sync::Arc;

impl<'a> Parser<'a> {
    pub(super) fn parse_command_statement_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let (arena_command, command_propagate) = if self.at_keyword(Keyword::Run) {
            let (run_id, _span) = self.parse_run_form_arena_only(arena)?;
            let propagate = self.consume(TokenKindMatch::Question).is_some();
            if propagate {
                arena.set_run_form_propagate(run_id, true);
            }
            (ArenaCommand::Run(run_id), false)
        } else {
            let arena_command = self.parse_command_arena_only(arena)?;
            let propagate = self.consume(TokenKindMatch::Question).is_some();
            (arena_command, propagate)
        };
        let end = self.expect_terminator();
        let span = self.span(start, end);
        arena.push_command_statement(arena_command, command_propagate, span);
        Some(())
    }

    pub(super) fn parse_command_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaCommand> {
        match (self.current_tag(), self.current_keyword()) {
            (TokenTag::Keyword, Some(Keyword::Run)) => {
                let (run_id, _span) = self.parse_run_form_arena_only(arena)?;
                Some(ArenaCommand::Run(run_id))
            }
            (TokenTag::Keyword, Some(Keyword::True)) => {
                self.bump();
                let args = self.parse_command_args_arena_only(false, arena);
                Some(ArenaCommand::Proc {
                    name: Name::intern("true"),
                    args,
                })
            }
            (TokenTag::Keyword, Some(Keyword::False)) => {
                self.bump();
                let args = self.parse_command_args_arena_only(false, arena);
                Some(ArenaCommand::Proc {
                    name: Name::intern("false"),
                    args,
                })
            }
            (TokenTag::Ident | TokenTag::ProcIdent, _) => {
                let name = self
                    .current_name()
                    .expect("command name token has name payload");
                if let Some(command) = CoreCommand::from_name(name.as_str())
                    && !self.current_name_has_contiguous_dot()
                    && (command != CoreCommand::Env || self.command_line_has_block())
                {
                    self.parse_core_command_arena_only(command, arena)
                } else {
                    let name = self.parse_command_name()?;
                    let args = self.parse_command_args_arena_only(false, arena);
                    Some(ArenaCommand::Proc { name, args })
                }
            }
            _ => {
                self.diagnostic_here("expected command", "parse.expected-command");
                None
            }
        }
    }

    pub(super) fn parse_command_name(&mut self) -> Option<Name> {
        let first = self.expect_proc_ident("expected command name")?;
        if self.consume(TokenKindMatch::Dot).is_none() {
            return Some(first);
        }
        let field = self.expect_proc_ident("expected command name after `.`")?;
        let mut name = String::with_capacity(first.as_str().len() + field.as_str().len() + 1);
        name.push_str(first.as_str());
        name.push('.');
        name.push_str(field.as_str());
        while self.consume(TokenKindMatch::Dot).is_some() {
            let field = self.expect_proc_ident("expected command name after `.`")?;
            name.push('.');
            name.push_str(field.as_str());
        }
        Some(Name::intern(name))
    }

    pub(super) fn current_name_has_contiguous_dot(&self) -> bool {
        self.peek_tag(1) == Some(TokenTag::Dot) && self.peek_start(1) == Some(self.current_end())
    }

    fn command_line_has_block(&self) -> bool {
        let mut index = self.index + 1;
        while let Some(tag) = self.token_table.tag_at(index) {
            match tag {
                TokenTag::LBrace => return true,
                TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Eof => {
                    return false;
                }
                _ => {}
            }
            index += 1;
        }
        false
    }

    pub(super) fn lookahead_is_keyword_builtin_command(&self) -> bool {
        matches!(
            self.peek_tag(1),
            Some(
                TokenTag::Newline
                    | TokenTag::Semicolon
                    | TokenTag::RBrace
                    | TokenTag::Eof
                    | TokenTag::Question
            )
        )
    }

    pub(super) fn parse_core_command_arena_only(
        &mut self,
        name: CoreCommand,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaCommand> {
        self.bump();
        let mut args = empty_command_arg_range(arena);
        let mut env = empty_env_assignment_range(arena);
        let mut block = None;
        match name {
            CoreCommand::Cd => {
                let (parsed_args, parsed_count) =
                    self.parse_command_args_arena_only_limit(true, 1, arena);
                args = parsed_args;
                if parsed_count > 1 {
                    self.diagnostic_previous("`cd` accepts one path argument", "parse.cd-arity");
                }
                let id = self.parse_block_arena_only(arena)?;
                block = Some(id);
            }
            CoreCommand::Env => {
                if self.lookahead_is_env_expr_assignment_block() {
                    env = self.parse_env_expr_assignment_block_arena_only(arena)?;
                } else {
                    arena.begin_env_assignments();
                    while !self.at(TokenKindMatch::LBrace) && !self.at(TokenKindMatch::Eof) {
                        if self.parse_env_assignment_arena_only(arena).is_none() {
                            self.diagnostic_here(
                                "`env` blocks accept NAME=value assignments",
                                "parse.env-assignment",
                            );
                            break;
                        }
                    }
                    env = arena.finish_env_assignments();
                }
                let id = self.parse_block_arena_only(arena)?;
                block = Some(id);
            }
            CoreCommand::Print | CoreCommand::Eprint => {
                args = self.parse_command_args_arena_only(false, arena);
            }
        }
        Some(ArenaCommand::Core {
            name,
            args,
            env,
            block,
        })
    }

    pub(super) fn parse_run_form_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<(RunFormId, Span)> {
        let start = self.current_start();
        arena.begin_run_segments();
        let Some(first_span) = self.parse_run_segment_arena_only(arena) else {
            arena.discard_run_segments();
            return None;
        };
        let mut end = first_span.end();
        while self.consume(TokenKindMatch::Pipe).is_some() {
            if !self.at_keyword(Keyword::Run) {
                self.diagnostic_here(
                    "byte pipeline segments must start with `run`",
                    "parse.pipeline-run",
                );
                break;
            }
            let Some(seg_span) = self.parse_run_segment_arena_only(arena) else {
                arena.discard_run_segments();
                return None;
            };
            end = seg_span.end();
        }
        let span = self.span(start, end);
        let id = arena.finish_run_form(false, span);
        Some((id, span))
    }

    fn parse_run_segment_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<Span> {
        let start = self.current_start();
        self.bump();
        let mut kind = RunKind::Plain;
        let mut builtin = false;
        if self.consume(TokenKindMatch::Dot).is_some() {
            let name = self.expect_member_name("expected run form after `run.`")?;
            if name == "builtin" {
                builtin = true;
                if self.consume(TokenKindMatch::Dot).is_some() {
                    let name =
                        self.expect_member_name("expected run builtin form after `run.builtin.`")?;
                    kind = self.parse_run_kind_after_dot(&name)?;
                }
            } else {
                kind = self.parse_run_kind_after_dot(&name)?;
            }
        }
        let (timeout_id, cpu_max_id) = self.parse_run_options_arena_only(arena);
        let env_range = self.parse_env_assignments_arena_only(arena);
        let grouped = self.at(TokenKindMatch::LParen)
            && self
                .peek_tag(1)
                .is_some_and(|tag| matches!(tag, TokenTag::Newline | TokenTag::Comment));
        if grouped {
            self.bump();
            self.skip_newlines();
        }
        let target = self.parse_command_arg_arena_only(arena)?;
        arena.begin_command_args();
        arena.begin_redirections();
        if grouped {
            self.parse_grouped_run_tail_arena_only(arena);
            self.expect(
                TokenKindMatch::RParen,
                "expected `)` after grouped run invocation",
            );
        } else {
            while !self.at_run_segment_end() {
                if self.parse_redirection_arena_only(arena).is_some() {
                } else if let Some(arg) = self.parse_command_arg_arena_only(arena) {
                    arena.push_command_arg_input(arg);
                } else {
                    break;
                }
            }
        }
        let args_range = arena.finish_command_args();
        let redirections_range = arena.finish_redirections();
        let span = self.span(start, self.previous_end());
        arena.push_run_segment_parts(
            kind,
            builtin,
            timeout_id,
            cpu_max_id,
            env_range,
            grouped,
            target,
            args_range,
            redirections_range,
            span,
        );
        Some(span)
    }

    fn parse_run_kind_after_dot(&mut self, name: &str) -> Option<RunKind> {
        Some(match name {
            "status" => RunKind::Status,
            "text" => RunKind::CaptureText,
            "bytes" => RunKind::CaptureBytes,
            "capture" => {
                let mode = self.parse_command_word_text()?;
                match mode.as_str() {
                    "--text" => RunKind::CaptureTextRecord,
                    "--bytes" => RunKind::CaptureBytesRecord,
                    _ => {
                        self.diagnostic_previous(
                            "expected `--text` or `--bytes` capture mode",
                            "parse.capture-mode",
                        );
                        RunKind::CaptureTextRecord
                    }
                }
            }
            "stream" => {
                let mode = self.parse_command_word_text()?;
                match mode.as_str() {
                    "--text" => RunKind::StreamText,
                    "--bytes" => RunKind::StreamBytes,
                    _ => {
                        self.diagnostic_previous(
                            "expected `--text` or `--bytes` stream mode",
                            "parse.stream-mode",
                        );
                        RunKind::StreamText
                    }
                }
            }
            _ => {
                self.diagnostic_previous("unknown run form", "parse.unknown-run-form");
                RunKind::Plain
            }
        })
    }

    fn parse_grouped_run_tail_arena_only(&mut self, arena: &mut ArenaProgramBuilder<'_>) {
        loop {
            self.skip_newlines();
            self.skip_comments();
            if self.at(TokenKindMatch::RParen) || self.at(TokenKindMatch::Eof) {
                break;
            }
            if self.parse_redirection_arena_only(arena).is_some() {
            } else if let Some(arg) = self.parse_command_arg_arena_only(arena) {
                arena.push_command_arg_input(arg);
            } else {
                break;
            }
        }
    }

    fn parse_run_options_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> (Option<ExprId>, Option<ExprId>) {
        let mut timeout_id = None;
        let mut cpu_max_id = None;
        loop {
            let save = self.index;
            if !(self.at(TokenKindMatch::Minus) && self.peek_tag(1) == Some(TokenTag::Minus)) {
                break;
            }
            self.bump();
            self.bump();
            let Some(name) = self.expect_ident("expected run option name after `--`") else {
                self.index = save;
                break;
            };
            match name.as_str() {
                "timeout" => {
                    self.expect(TokenKindMatch::Equals, "expected `=` after `--timeout`");
                    if timeout_id.is_some() {
                        self.diagnostic_previous(
                            "duplicate `--timeout` option",
                            "parse.run-option",
                        );
                    }
                    if let Some(id) = self.parse_run_option_expr_arena_only(arena) {
                        timeout_id = Some(id);
                    }
                }
                "cpumax" => {
                    self.expect(TokenKindMatch::Equals, "expected `=` after `--cpumax`");
                    if cpu_max_id.is_some() {
                        self.diagnostic_previous("duplicate `--cpumax` option", "parse.run-option");
                    }
                    if let Some(id) = self.parse_run_option_expr_arena_only(arena) {
                        cpu_max_id = Some(id);
                    }
                }
                _ => {
                    self.index = save;
                    break;
                }
            }
        }
        (timeout_id, cpu_max_id)
    }

    fn parse_run_option_expr_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ExprId> {
        let span = self.current_span();
        match self.current_tag() {
            TokenTag::Int => {
                self.bump();
                let value = IntLiteral::from_text(self.span_text(span));
                Some(arena.push_int_expr(&value, span))
            }
            TokenTag::Duration => {
                self.bump();
                let value = DurationLiteral::from_text(self.span_text(span));
                Some(arena.push_duration_expr(&value, span))
            }
            TokenTag::Ident if !self.run_option_ident_continues_expr(span.end()) => {
                let value = self
                    .current_name()
                    .expect("identifier token has name payload");
                self.bump();
                Some(arena.push_ident_expr(value, span))
            }
            _ => self.parse_expr_id_arena_only(arena),
        }
    }

    fn run_option_ident_continues_expr(&self, end: usize) -> bool {
        self.peek_start(1) == Some(end)
            && matches!(
                self.peek_tag(1),
                Some(TokenTag::Dot | TokenTag::LParen | TokenTag::LBracket)
            )
    }

    pub(super) fn parse_env_assignments_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> ArenaRange {
        arena.begin_env_assignments();
        while self.parse_env_assignment_arena_only(arena).is_some() {}
        arena.finish_env_assignments()
    }

    pub(super) fn parse_env_assignment_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        if self.current_tag() != TokenTag::Ident {
            return None;
        }
        let name = self
            .current_name()
            .expect("identifier token has name payload");
        if self.peek_tag(1) != Some(TokenTag::Equals) {
            return None;
        }
        let start = self.current_start();
        self.bump();
        self.bump();
        let value = self.parse_command_arg_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_env_assignment_input(name, ArenaEnvAssignmentValue::CommandArg(value), span);
        Some(())
    }

    pub(super) fn parse_env_expr_assignment_block_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaRange> {
        self.expect(
            TokenKindMatch::LBrace,
            "expected `{` to start env assignments",
        )?;
        self.skip_separators();
        arena.begin_env_assignments();
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            let start = self.current_start();
            let Some(name) = self.expect_ident("expected environment name") else {
                arena.discard_env_assignments();
                return None;
            };
            self.expect(TokenKindMatch::Equals, "expected `=` in env assignment");
            let Some(value_id) = self.parse_expr_id_arena_only(arena) else {
                arena.discard_env_assignments();
                return None;
            };
            let end = self.expect_terminator();
            arena.push_env_assignment_input(
                name,
                ArenaEnvAssignmentValue::Expr(value_id),
                self.span(start, end),
            );
            self.skip_separators();
        }
        self.expect(TokenKindMatch::RBrace, "expected `}` after env assignments");
        Some(arena.finish_env_assignments())
    }

    pub(super) fn parse_redirection_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let start = self.current_start();
        let kind = if self.consume_legacy_error_redirection(true) {
            RedirectionKind::StderrAppend
        } else if self.consume_legacy_error_redirection(false) {
            RedirectionKind::StderrWrite
        } else if self.consume_stderr_redirection(true) {
            RedirectionKind::StderrAppend
        } else if self.consume_stderr_redirection(false) {
            RedirectionKind::StderrWrite
        } else if self.consume(TokenKindMatch::GtGt).is_some() {
            RedirectionKind::StdoutAppend
        } else if self.at(TokenKindMatch::Gt) && self.peek_tag(1) == Some(TokenTag::Amp) {
            self.bump();
            self.bump();
            RedirectionKind::StdoutDup
        } else if self.consume(TokenKindMatch::Gt).is_some() {
            RedirectionKind::StdoutWrite
        } else if self.at(TokenKindMatch::Lt) && self.peek_tag(1) == Some(TokenTag::Amp) {
            self.bump();
            self.bump();
            RedirectionKind::StdinDup
        } else if self.consume(TokenKindMatch::Lt).is_some() {
            RedirectionKind::StdinRead
        } else {
            return None;
        };

        let target_arg = self.parse_command_arg_arena_only(arena)?;
        let arena_target = match kind {
            RedirectionKind::StdoutDup | RedirectionKind::StdinDup => {
                ArenaRedirectionTarget::Fd(target_arg)
            }
            RedirectionKind::StdoutWrite
            | RedirectionKind::StdoutAppend
            | RedirectionKind::StdinRead
            | RedirectionKind::StderrWrite
            | RedirectionKind::StderrAppend => ArenaRedirectionTarget::Path(target_arg),
        };
        let span = self.span(start, self.previous_end());
        arena.push_redirection_input(kind, arena_target, span);
        Some(())
    }

    pub(super) fn consume_legacy_error_redirection(&mut self, append: bool) -> bool {
        let matched = if append {
            self.consume(TokenKindMatch::ErrorGtGt).is_some()
        } else {
            self.consume(TokenKindMatch::ErrorGt).is_some()
        };
        if matched {
            let replacement = if append { "2>>" } else { "2>" };
            self.diagnostic_previous(
                &format!("stderr redirection uses `{replacement}`"),
                "parse.legacy-stderr-redirection",
            );
        }
        matched
    }

    pub(super) fn consume_stderr_redirection(&mut self, append: bool) -> bool {
        if self.current_tag() != TokenTag::Int {
            return false;
        }
        let span = self.current_span();
        if self.span_text(span) != "2" {
            return false;
        }
        if self.peek_start(1) != Some(span.end()) {
            return false;
        }
        let matches_op = if append {
            self.peek_tag(1) == Some(TokenTag::GtGt)
        } else {
            self.peek_tag(1) == Some(TokenTag::Gt)
        };
        if !matches_op {
            return false;
        }
        self.bump();
        self.bump();
        true
    }

    pub(super) fn parse_command_args_arena_only(
        &mut self,
        stop_before_block: bool,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> ArenaRange {
        self.parse_command_args_arena_only_limit(stop_before_block, usize::MAX, arena)
            .0
    }

    fn parse_command_args_arena_only_limit(
        &mut self,
        stop_before_block: bool,
        direct_limit: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> (ArenaRange, usize) {
        let mut count = 0usize;
        arena.begin_command_args();
        while !self.at_command_end(stop_before_block) {
            if count < direct_limit {
                if let Some(arg) = self.parse_command_arg_arena_only(arena) {
                    arena.push_command_arg_input(arg);
                    count += 1;
                } else {
                    break;
                }
            } else if self.parse_command_arg_arena_only(arena).is_some() {
                count += 1;
            } else {
                break;
            }
        }
        (arena.finish_command_args(), count)
    }

    /// True when the current token starts an expression chain ending in
    /// `()` (call) or `[]` (index). These are unambiguous in command-arg
    /// position — unlike bare field access (`a.b`) which could be a filename.
    fn at_call_or_index_chain(&self) -> bool {
        if !matches!(self.current_tag(), TokenTag::Ident | TokenTag::String) {
            return false;
        }
        let mut pos = self.index + 1;
        loop {
            let tag = match self.token_table.tag_at(pos) {
                Some(tag) => tag,
                None => return false,
            };
            match tag {
                TokenTag::Dot => {
                    if self.start_at(pos + 1) != self.end_at(pos)
                        || self.token_table.tag_at(pos + 1) != Some(TokenTag::Ident)
                    {
                        return false;
                    }
                    pos += 2;
                }
                TokenTag::LParen | TokenTag::LBracket => {
                    return self.start_at(pos) == self.end_at(pos - 1);
                }
                _ => return false,
            }
        }
    }

    pub(super) fn parse_command_arg_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<ArenaCommandArg> {
        if self.at_call_or_index_chain() {
            let start = self.current_start();
            let expr_id =
                self.with_command_arg_expr(|parser| parser.parse_expr_id_arena_only(arena))?;
            let span = self.span(start, self.previous_end());
            return Some(arena.typed_command_arg(expr_id, span));
        }

        if self.consume(TokenKindMatch::At).is_some() {
            let start = self.previous_start();
            if self.current_tag() == TokenTag::Ident {
                let name = self.current_name().expect("splice name token has payload");
                let end = self.current_end();
                self.bump();
                return Some(arena.splice_name_command_arg(name, self.span(start, end)));
            }
            if self.current_tag() == TokenTag::GlobString {
                let span = self.current_span();
                let value: Arc<str> = self.decoded_quoted_text(span, false);
                let expr_id = arena.push_glob_str_expr(&value, span);
                self.bump();
                return Some(arena.splice_expr_command_arg(expr_id, self.span(start, span.end())));
            }
            self.expect(TokenKindMatch::LParen, "expected splice name or expression");
            let expr_id = self.parse_expr_id_arena_only(arena)?;
            self.expect(
                TokenKindMatch::RParen,
                "expected `)` after splice expression",
            );
            let end = self.previous_end();
            return Some(arena.splice_expr_command_arg(expr_id, self.span(start, end)));
        }

        if self.consume(TokenKindMatch::LParen).is_some() {
            let start = self.previous_start();
            let expr_id = self.parse_expr_id_arena_only(arena)?;
            self.expect(
                TokenKindMatch::RParen,
                "expected `)` after typed command argument",
            );
            let end = self.previous_end();
            return Some(arena.typed_command_arg(expr_id, self.span(start, end)));
        }

        if matches!(
            self.current_tag(),
            TokenTag::PathString
                | TokenTag::GlobString
                | TokenTag::PathFmtString
                | TokenTag::FmtString
        ) {
            let start = self.current_start();
            let expr_id =
                self.with_command_arg_expr(|parser| parser.parse_expr_id_arena_only(arena))?;
            return Some(arena.typed_command_arg(expr_id, self.span(start, self.previous_end())));
        }

        let start = self.current_start();
        let mut end = start;
        let mut first = true;
        let mut bare_text = String::new();
        arena.begin_word_parts();
        while self.is_word_part_start() {
            if !first && self.current_start() != end {
                break;
            }
            first = false;
            match self.current_tag() {
                TokenTag::String => {
                    let flags = self
                        .token_table
                        .string_flags_at(self.index)
                        .expect("command string has flags payload");
                    let span = self.current_span();
                    self.bump();
                    let mut search_from = span.start();
                    self.command_string_parts_arena_only(
                        arena,
                        span,
                        flags.raw_literal,
                        &mut search_from,
                    );
                    end = span.end();
                }
                TokenTag::DollarIdent => {
                    let name = self
                        .current_name()
                        .expect("dollar ident token has name payload");
                    let span = self.current_span();
                    self.bump();
                    let expr_id =
                        self.parse_dollar_command_interpolation_arena_only(name, span, arena);
                    end = self.previous_end();
                    arena.push_shorthand_word_part_expr(expr_id);
                }
                TokenTag::DollarLBrace => {
                    self.bump();
                    let expr_id = self.parse_expr_id_arena_only(arena)?;
                    self.expect(TokenKindMatch::RBrace, "expected `}` after interpolation");
                    end = self.previous_end();
                    arena.push_interpolation_word_part_expr(expr_id);
                }
                _ => {
                    let span = self.current_span();
                    bare_text.push_str(self.span_text(span));
                    arena.push_bare_word_part_source_span(span);
                    end = span.end();
                    self.bump();
                }
            }
        }

        if first {
            arena.discard_word_parts();
            self.diagnostic_here("expected command argument", "parse.expected-command-arg");
            return None;
        }

        if self.current_tag() == TokenTag::LParen && self.current_start() == end {
            let message = if bare_text.contains('.') {
                format!(
                    "command args cannot contain call expressions; try `${bare_text}()` or bind to a let first"
                )
            } else {
                format!(
                    "command args cannot contain call expressions; try `${bare_text}()` or wrap in `({bare_text} ...)`"
                )
            };
            let diag_span = self.span(start, end);
            let paren_start = self.current_start();
            self.bump();
            let mut depth = 1u32;
            while depth > 0 {
                match self.current_tag() {
                    TokenTag::LParen => depth += 1,
                    TokenTag::RParen => depth -= 1,
                    TokenTag::Eof => break,
                    _ => {}
                }
                self.bump();
            }
            let fix_span = self.span(start, self.previous_end());
            let call_text = self
                .source
                .get(paren_start..self.previous_end())
                .unwrap_or("()");
            let fix = format!("${bare_text}{call_text}");
            self.diagnostics.push(
                Diagnostic::new(Severity::Error, &message)
                    .with_code("parse.command-call-expr")
                    .with_label(Label::primary(diag_span, &message))
                    .with_fix_hint(FixHint::replacement(fix_span, "use `$` shorthand", fix)),
            );
        }

        let span = self.span(start, end);
        let parts = arena.finish_word_parts();
        Some(arena.word_command_arg(parts, span))
    }

    pub(super) fn parse_dollar_command_interpolation_arena_only(
        &mut self,
        name: Name,
        span: Span,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> ExprId {
        let start = span.start();
        let mut expr_id = arena.push_ident_expr(name, span);
        let mut end = span.end();
        loop {
            if self.current_start() == end {
                if self.current_tag() == TokenTag::Dot {
                    if self.peek_start(1) != Some(self.current_end()) {
                        break;
                    }
                    let Some(name) = self
                        .peek_name(1)
                        .filter(|_| self.peek_tag(1) == Some(TokenTag::Ident))
                    else {
                        break;
                    };
                    self.bump();
                    self.bump();
                    end = self.previous_end();
                    let next_span = self.span(start, end);
                    expr_id = arena.push_field_expr(expr_id, name, next_span);
                    continue;
                }
                if self.current_tag() == TokenTag::LParen {
                    self.bump();
                    let args = self.parse_call_args_arena_only(arena);
                    self.expect(TokenKindMatch::RParen, "expected `)` after call arguments");
                    end = self.previous_end();
                    let next_span = self.span(start, end);
                    expr_id = arena.push_call_expr(expr_id, args, next_span);
                    continue;
                }
            }
            break;
        }
        expr_id
    }

    pub(super) fn parse_command_word_text(&mut self) -> Option<String> {
        let span = self.current_span();
        let text = match self.current_tag() {
            TokenTag::Minus
                if self.peek_tag(1) == Some(TokenTag::Minus)
                    && matches!(
                        self.peek_tag(2),
                        Some(TokenTag::Ident | TokenTag::ProcIdent)
                    ) =>
            {
                self.bump();
                self.bump();
                let name = self
                    .current_name()
                    .expect("capture mode option token has payload");
                self.bump();
                return Some(format!("--{name}"));
            }
            TokenTag::Ident | TokenTag::ProcIdent => self.span_text(span).to_string(),
            TokenTag::String => {
                let raw = self.quoted_content(span).to_string();
                if raw.contains('$') {
                    self.diagnostics.push(
                        Diagnostic::error("capture mode cannot be interpolated")
                            .with_code("parse.capture-mode-interpolation")
                            .with_label(Label::primary(span, "expected literal capture mode")),
                    );
                }
                let (text, diagnostics) = decode_interpolation_text_for(
                    self.source_id,
                    &raw,
                    span,
                    self.string_content_offset(span),
                );
                self.diagnostics.extend(diagnostics);
                text
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error("expected literal command word")
                        .with_code("parse.expected-word")
                        .with_label(Label::primary(span, "expected word")),
                );
                return None;
            }
        };
        self.bump();
        Some(text)
    }

    /// Pushes this command-word string token's parts directly into the
    /// caller's already-open `begin_word_parts()` scope (a bare command word
    /// is built from a sequence of tokens — string, `$ident`, `${...}`,
    /// bare-text — all flattened into one word-parts list, not one list per
    /// token), so this does not manage its own begin/finish. `search_from`
    /// is threaded by the caller across that whole token sequence.
    pub(super) fn command_string_parts_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        span: Span,
        raw_literal: bool,
        search_from: &mut usize,
    ) {
        let source_id = self.source_id;
        let raw = self.quoted_content(span);
        if raw_literal || !raw.contains('$') {
            let text = if raw_literal {
                Arc::from(raw)
            } else {
                let (text, decode_diagnostics) = decode_interpolation_text_for(
                    source_id,
                    raw,
                    span,
                    self.string_content_offset(span),
                );
                self.diagnostics.extend(decode_diagnostics);
                Arc::from(text)
            };
            arena.push_quoted_word_part_text(&text, span, search_from, span.end());
            return;
        }

        let mut diagnostics = Vec::new();
        let mut any_part = false;
        let content_offset = self.string_content_offset(span);
        let mut rest_start = 0usize;
        let mut search_start = 0usize;
        let bytes = raw.as_bytes();
        while let Some(relative) = raw[search_start..].find('$') {
            let dollar = search_start + relative;
            if literal::is_escaped(bytes, dollar) {
                search_start = dollar + 1;
                continue;
            }
            let Some(next) = bytes.get(dollar + 1).copied() else {
                break;
            };
            if next == b'{' {
                if dollar > rest_start {
                    let (text, decode_diagnostics) = decode_interpolation_text_for(
                        source_id,
                        &raw[rest_start..dollar],
                        span,
                        content_offset + rest_start,
                    );
                    diagnostics.extend(decode_diagnostics);
                    arena.push_quoted_word_part_text(
                        &Arc::from(text),
                        span,
                        search_from,
                        span.end(),
                    );
                }
                let expr_start = dollar + 2;
                let Some(close) = literal::interpolation_close(raw, expr_start) else {
                    diagnostics.push(interpolation_diagnostic(
                        span,
                        "unterminated string interpolation",
                        "interpolation starts in this string",
                    ));
                    // Unlike the old recursive-AST path (which discards
                    // whatever parts it accumulated for this token and
                    // replaces them with one part covering the whole raw
                    // content), this pushes directly into the caller's
                    // already-open word-parts list alongside any earlier
                    // tokens, so it can't retroactively discard — push one
                    // more part for just the unparsed remainder instead.
                    let (text, decode_diagnostics) = decode_interpolation_text_for(
                        source_id,
                        &raw[dollar..],
                        span,
                        content_offset + dollar,
                    );
                    diagnostics.extend(decode_diagnostics);
                    arena.push_quoted_word_part_text(
                        &Arc::from(text),
                        span,
                        search_from,
                        span.end(),
                    );
                    self.diagnostics.extend(diagnostics);
                    return;
                };
                let (expr_id, parse_diagnostics) = parse_interpolation_expr_arena_only_for(
                    source_id,
                    &raw[expr_start..close],
                    content_offset + expr_start,
                    arena,
                );
                diagnostics.extend(parse_diagnostics);
                if let Some(expr_id) = expr_id {
                    any_part = true;
                    arena.push_interpolation_word_part_expr(expr_id);
                }
                rest_start = close + 1;
                search_start = rest_start;
                continue;
            }

            if is_ident_start(next) {
                if dollar > rest_start {
                    let (text, decode_diagnostics) = decode_interpolation_text_for(
                        source_id,
                        &raw[rest_start..dollar],
                        span,
                        content_offset + rest_start,
                    );
                    diagnostics.extend(decode_diagnostics);
                    any_part = true;
                    arena.push_quoted_word_part_text(
                        &Arc::from(text),
                        span,
                        search_from,
                        span.end(),
                    );
                }
                let expr_start = dollar + 1;
                let shorthand_end = dollar_shorthand_end(raw, expr_start);
                let (expr_id, parse_diagnostics) = parse_interpolation_expr_arena_only_for(
                    source_id,
                    &raw[expr_start..shorthand_end],
                    content_offset + expr_start,
                    arena,
                );
                diagnostics.extend(parse_diagnostics);
                if let Some(expr_id) = expr_id {
                    any_part = true;
                    arena.push_shorthand_word_part_expr(expr_id);
                }
                rest_start = shorthand_end;
                search_start = rest_start;
                continue;
            }

            search_start = dollar + 1;
        }
        if rest_start < raw.len() {
            let (text, decode_diagnostics) = decode_interpolation_text_for(
                source_id,
                &raw[rest_start..],
                span,
                content_offset + rest_start,
            );
            diagnostics.extend(decode_diagnostics);
            any_part = true;
            arena.push_quoted_word_part_text(&Arc::from(text), span, search_from, span.end());
        }
        self.diagnostics.extend(diagnostics);
        if !any_part {
            arena.push_quoted_word_part_text(&Arc::from(""), span, search_from, span.end());
        }
    }
}

fn empty_command_arg_range(arena: &mut ArenaProgramBuilder<'_>) -> ArenaRange {
    arena.begin_command_args();
    arena.finish_command_args()
}

fn empty_env_assignment_range(arena: &mut ArenaProgramBuilder<'_>) -> ArenaRange {
    arena.begin_env_assignments();
    arena.finish_env_assignments()
}
