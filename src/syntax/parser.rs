pub(in crate::syntax::parser) use crate::diagnostic::{Diagnostic, FixHint, Label, Severity};
pub(in crate::syntax::parser) use crate::source::{SourceId, Span};
pub(in crate::syntax::parser) use crate::symbol::Name;
use crate::syntax::arena::{ArenaProgram, ArenaProgramBuilder, ArenaRange, TypeExprId};
use crate::syntax::cst::LazyCst;
pub(in crate::syntax::parser) use crate::syntax::lexer::Lexer;
pub(in crate::syntax::parser) use crate::syntax::literal::{
    self, EscapeIssueKind, InterpolationChunk,
};
pub(in crate::syntax::parser) use crate::syntax::node::{
    AssignOp, BinaryOp, BlockParam, CoreCommand, DurationLiteral, Effect, FloatLiteral, IntLiteral,
    RedirectionKind, RunKind, SignalHookOptions, StreamStageKind, UnaryOp,
};
pub(in crate::syntax::parser) use crate::syntax::token::{Keyword, TokenTable, TokenTag};
mod command;
mod expr;
mod literals;
mod pattern;
mod stmt;
mod types;

pub(in crate::syntax::parser) use self::literals::{
    decode_bytes_literal_for, decode_interpolation_text_for, dollar_shorthand_end,
    interpolation_diagnostic, is_ident_start, parse_interpolation_expr_arena_only_for,
};
pub(in crate::syntax::parser) use self::types::{result_unit_type_expr, unknown_type_expr};

#[derive(Clone, Debug, Default)]
pub struct ArenaParseOutput {
    pub arena: ArenaProgram,
    pub cst: LazyCst,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Default)]
pub struct ArenaParseFragment {
    pub statements: ArenaRange,
    pub cst: LazyCst,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct Parser<'a> {
    source_id: SourceId,
    source: &'a str,
    token_table: TokenTable,
    index: usize,
    comma_is_terminator: bool,
    pipe_is_boundary: bool,
    trailing_statement_try: bool,
    command_arg_expr: bool,
    block_depth: usize,
    diagnostics: Vec<Diagnostic>,
}

/// Map a single token kind to its binary op, precedence, and token count.
/// The `peek` closure is used to check the next token for two-token ops (e.g. `not in`).
fn binary_op_for_token(
    tag: TokenTag,
    keyword: Option<Keyword>,
    peek_keyword: impl Fn(usize) -> Option<Keyword>,
) -> Option<(BinaryOp, u8, usize)> {
    Some(match (tag, keyword) {
        (TokenTag::QuestionQuestion, _) => (BinaryOp::ResultFallback, 1, 1),
        (TokenTag::Keyword, Some(Keyword::Or)) => (BinaryOp::Or, 1, 1),
        (TokenTag::Keyword, Some(Keyword::And)) => (BinaryOp::And, 2, 1),
        (TokenTag::EqEq, _) => (BinaryOp::Eq, 3, 1),
        (TokenTag::BangEq, _) => (BinaryOp::Ne, 3, 1),
        (TokenTag::Lt, _) => (BinaryOp::Lt, 4, 1),
        (TokenTag::Le, _) => (BinaryOp::Le, 4, 1),
        (TokenTag::Gt, _) => (BinaryOp::Gt, 4, 1),
        (TokenTag::Ge, _) => (BinaryOp::Ge, 4, 1),
        (TokenTag::Keyword, Some(Keyword::In)) => (BinaryOp::In, 4, 1),
        (TokenTag::Keyword, Some(Keyword::Not)) if peek_keyword(1) == Some(Keyword::In) => {
            (BinaryOp::NotIn, 4, 2)
        }
        (TokenTag::Plus, _) => (BinaryOp::Add, 5, 1),
        (TokenTag::Minus, _) => (BinaryOp::Sub, 5, 1),
        (TokenTag::Star, _) => (BinaryOp::Mul, 6, 1),
        (TokenTag::Slash, _) => (BinaryOp::Div, 6, 1),
        (TokenTag::Percent, _) => (BinaryOp::Rem, 6, 1),
        _ => return None,
    })
}

impl<'a> Parser<'a> {
    pub fn parse_source_arena_only(source_id: SourceId, source: &'a str) -> ArenaParseOutput {
        let lexed = Lexer::new(source_id, source).lex_compact();
        let mut parser = Self::new_with_token_table(source_id, source, lexed.token_table);
        parser.diagnostics.extend(lexed.diagnostics);
        parser.parse_arena_only()
    }

    pub fn parse_source_into_arena_builder(
        source_id: SourceId,
        source: &'a str,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> ArenaParseFragment {
        let lexed = Lexer::new(source_id, source).lex_compact();
        let mut parser = Self::new_with_token_table(source_id, source, lexed.token_table);
        parser.diagnostics.extend(lexed.diagnostics);
        parser.parse_into_arena_builder(arena)
    }

    pub fn new_with_token_table(
        source_id: SourceId,
        source: &'a str,
        token_table: TokenTable,
    ) -> Self {
        Self {
            source_id,
            source,
            token_table,
            index: 0,
            comma_is_terminator: false,
            pipe_is_boundary: false,
            trailing_statement_try: true,
            command_arg_expr: false,
            block_depth: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn parse_arena_only(mut self) -> ArenaParseOutput {
        let cst = LazyCst::new(self.source_id, self.source, self.token_table.clone());
        let arena = self.parse_program_arena_only();
        ArenaParseOutput {
            arena,
            cst,
            diagnostics: self.diagnostics,
        }
    }

    pub fn parse_into_arena_builder(
        mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> ArenaParseFragment {
        let cst = LazyCst::new(self.source_id, self.source, self.token_table.clone());
        let start = arena.root_statement_count();
        self.skip_separators();
        while !self.at(TokenKindMatch::Eof) {
            if self.parse_statement_arena_only(arena).is_none() {
                self.recover_statement();
            }
            self.skip_separators();
        }
        let statements = arena.finish_root_statements_from(start);
        ArenaParseFragment {
            statements,
            cst,
            diagnostics: self.diagnostics,
        }
    }

    fn parse_program_arena_only(&mut self) -> ArenaProgram {
        let mut arena = ArenaProgramBuilder::with_source_and_token_capacity(
            self.source,
            self.token_table.len(),
        );
        self.skip_separators();
        while !self.at(TokenKindMatch::Eof) {
            if self.parse_statement_arena_only(&mut arena).is_none() {
                self.recover_statement();
            }
            self.skip_separators();
        }
        arena.finish()
    }

    pub(in crate::syntax::parser) fn current_binary_op(&self) -> Option<(BinaryOp, u8, usize)> {
        binary_op_for_token(self.current_tag(), self.current_keyword(), |n| {
            self.peek_keyword(n)
        })
    }

    /// If the current token is a newline/comment and the next non-newline token
    /// is a binary operator, return the binary op info. This lets expression
    /// parsing continue across newlines when a binary operator follows.
    pub(in crate::syntax::parser) fn continuation_binary_op(
        &self,
    ) -> Option<(BinaryOp, u8, usize)> {
        let mut offset = 0usize;
        while matches!(
            self.peek_tag(offset),
            Some(TokenTag::Newline | TokenTag::Comment)
        ) {
            offset += 1;
        }
        if offset == 0 {
            return None;
        }
        binary_op_for_token(self.peek_tag(offset)?, self.peek_keyword(offset), |n| {
            self.peek_keyword(offset + n)
        })
    }

    /// Like `peek_tag(n)` but skips intervening newlines and comments.
    pub(in crate::syntax::parser) fn peek_tag_skip_newlines(&self, n: usize) -> Option<TokenTag> {
        let mut offset = n;
        loop {
            match self.peek_tag(offset) {
                Some(TokenTag::Newline | TokenTag::Comment) => offset += 1,
                other => return other,
            }
        }
    }

    pub(in crate::syntax::parser) fn question_is_trailing_statement_try(&self) -> bool {
        if self.current_tag() != TokenTag::Question {
            return false;
        }
        let mut offset = 1usize;
        while self.peek_tag(offset) == Some(TokenTag::Comment) {
            offset += 1;
        }
        matches!(
            self.peek_tag(offset),
            Some(
                TokenTag::Newline
                    | TokenTag::Semicolon
                    | TokenTag::Comma
                    | TokenTag::RParen
                    | TokenTag::RBracket
                    | TokenTag::RBrace
                    | TokenTag::Eof
            )
        )
    }

    /// Peek past newlines and return true if the next non-newline token is `|`.
    pub(in crate::syntax::parser) fn peeked_pipe_after_newlines(&self) -> bool {
        let mut offset = 0usize;
        while matches!(
            self.peek_tag(offset),
            Some(TokenTag::Newline | TokenTag::Comment)
        ) {
            offset += 1;
        }
        self.peek_tag(offset) == Some(TokenTag::Pipe)
    }

    pub(in crate::syntax::parser) fn at_command_end(&mut self, stop_before_block: bool) -> bool {
        self.skip_comments();
        self.at_terminator()
            || (self.comma_is_terminator && self.at(TokenKindMatch::Comma))
            || self.at(TokenKindMatch::Eof)
            || self.at(TokenKindMatch::Question)
            || (stop_before_block && self.at(TokenKindMatch::LBrace))
    }

    pub(in crate::syntax::parser) fn at_run_segment_end(&mut self) -> bool {
        self.skip_comments();
        self.at_terminator()
            || self.at(TokenKindMatch::Eof)
            || self.at(TokenKindMatch::Question)
            || self.at(TokenKindMatch::LBrace)
            || self.at(TokenKindMatch::Pipe)
            || self.at(TokenKindMatch::PipeGt)
    }

    pub(in crate::syntax::parser) fn at_pipe_stage_end(&mut self) -> bool {
        self.skip_comments();
        self.at_terminator() || self.at(TokenKindMatch::Eof) || self.at(TokenKindMatch::PipeGt)
    }

    pub(in crate::syntax::parser) fn is_word_part_start(&self) -> bool {
        !matches!(
            self.current_tag(),
            TokenTag::Eof
                | TokenTag::Newline
                | TokenTag::Comment
                | TokenTag::Semicolon
                | TokenTag::RBrace
                | TokenTag::At
                | TokenTag::Question
                | TokenTag::Pipe
                | TokenTag::PipeGt
                | TokenTag::Amp
                | TokenTag::LParen
        )
    }

    pub(in crate::syntax::parser) fn lookahead_is_assignment(&self) -> bool {
        let mut offset = 1;
        loop {
            match self.peek_tag(offset) {
                Some(TokenTag::Dot) if self.peek_tag(offset + 1) != Some(TokenTag::Dot) => {
                    match self.peek_tag(offset + 1) {
                        Some(TokenTag::Ident | TokenTag::ProcIdent) => offset += 2,
                        _ => return false,
                    }
                }
                Some(TokenTag::LBracket) => {
                    offset += 1;
                    let mut depth = 1usize;
                    while let Some(tag) = self.peek_tag(offset) {
                        match tag {
                            TokenTag::LBracket => depth += 1,
                            TokenTag::RBracket => {
                                depth -= 1;
                                if depth == 0 {
                                    offset += 1;
                                    break;
                                }
                            }
                            TokenTag::Eof | TokenTag::Newline | TokenTag::Semicolon => {
                                return false;
                            }
                            _ => {}
                        }
                        offset += 1;
                    }
                    if depth != 0 {
                        return false;
                    }
                }
                _ => break,
            }
        }
        match self.peek_tag(offset) {
            Some(TokenTag::Equals) => true,
            Some(
                TokenTag::Plus
                | TokenTag::Minus
                | TokenTag::Star
                | TokenTag::Slash
                | TokenTag::Percent,
            ) => self.peek_tag(offset + 1) == Some(TokenTag::Equals),
            _ => false,
        }
    }

    pub(in crate::syntax::parser) fn lookahead_is_expr_call_or_postfix(&self) -> bool {
        let current_end = self.current_end();
        self.peek_tag(1).is_some_and(|tag| {
            self.peek_start(1) == Some(current_end)
                && matches!(
                    tag,
                    TokenTag::LParen | TokenTag::Dot | TokenTag::LBracket | TokenTag::Question
                )
        })
    }

    pub(in crate::syntax::parser) fn lookahead_is_dotted_command(&self) -> bool {
        let mut index = self.index;
        let mut end = self.current_end();
        let mut saw_dot = false;
        while self
            .token_table
            .tag_at(index + 1)
            .is_some_and(|tag| self.start_at(index + 1) == Some(end) && tag == TokenTag::Dot)
            && matches!(
                self.token_table.tag_at(index + 2),
                Some(TokenTag::Ident | TokenTag::ProcIdent)
            )
        {
            saw_dot = true;
            index += 2;
            end = self.end_at(index).unwrap_or(end);
        }
        if !saw_dot {
            return false;
        }
        self.token_table.tag_at(index + 1).is_some_and(|tag| {
            self.start_at(index + 1).is_some_and(|start| start > end)
                && !matches!(
                    tag,
                    TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Eof
                )
        })
    }

    pub(in crate::syntax::parser) fn lookahead_is_run_stream(&self) -> bool {
        self.peek_tag(1) == Some(TokenTag::Dot)
            && self
                .peek_name(2)
                .is_some_and(|name| self.peek_tag(2) == Some(TokenTag::Ident) && name == "stream")
    }

    pub(in crate::syntax::parser) fn lookahead_is_env_expr_assignment_block(&self) -> bool {
        if self.current_tag() != TokenTag::LBrace {
            return false;
        }
        let mut index = self.index + 1;
        while matches!(
            self.token_table.tag_at(index),
            Some(TokenTag::Newline | TokenTag::Semicolon | TokenTag::Comment)
        ) {
            index += 1;
        }
        self.token_table.tag_at(index) == Some(TokenTag::Ident)
            && self.token_table.tag_at(index + 1) == Some(TokenTag::Equals)
    }

    pub(in crate::syntax::parser) fn lookahead_is_expr_binary(&self) -> bool {
        self.peek_tag(1).is_some_and(|tag| {
            matches!(
                (tag, self.peek_keyword(1)),
                (TokenTag::EqEq, _)
                    | (TokenTag::BangEq, _)
                    | (TokenTag::Lt, _)
                    | (TokenTag::Le, _)
                    | (TokenTag::Gt, _)
                    | (TokenTag::Ge, _)
                    | (TokenTag::Plus, _)
                    | (TokenTag::Star, _)
                    | (TokenTag::Slash, _)
                    | (TokenTag::Percent, _)
                    | (TokenTag::Keyword, Some(Keyword::And))
                    | (TokenTag::Keyword, Some(Keyword::Or))
                    | (TokenTag::Keyword, Some(Keyword::In))
                    | (TokenTag::Keyword, Some(Keyword::Not))
            )
        }) || self.lookahead_past_newlines_is_pipe_gt()
    }

    pub(in crate::syntax::parser) fn lookahead_past_newlines_is_pipe_gt(&self) -> bool {
        let mut i = self.index + 1;
        while let Some(tag) = self.token_table.tag_at(i) {
            match tag {
                TokenTag::Newline => i += 1,
                TokenTag::PipeGt => return true,
                _ => return false,
            }
        }
        false
    }

    pub(in crate::syntax::parser) fn span_text(&self, span: Span) -> &str {
        &self.source[span.start()..span.end()]
    }

    pub(in crate::syntax::parser) fn skip_separators(&mut self) {
        while matches!(
            self.current_tag(),
            TokenTag::Newline | TokenTag::Semicolon | TokenTag::Comment
        ) {
            self.bump();
        }
    }

    pub(in crate::syntax::parser) fn skip_newlines(&mut self) {
        while self.current_tag() == TokenTag::Newline {
            self.bump();
        }
    }

    pub(in crate::syntax::parser) fn skip_pipeline_newlines(&mut self) {
        let mut index = self.index;
        while matches!(
            self.token_table.tag_at(index),
            Some(TokenTag::Newline | TokenTag::Comment)
        ) {
            index += 1;
        }
        if self.token_table.tag_at(index) == Some(TokenTag::PipeGt) {
            self.index = index;
        }
    }

    pub(in crate::syntax::parser) fn skip_comments(&mut self) {
        while self.current_tag() == TokenTag::Comment {
            self.bump();
        }
    }

    pub(in crate::syntax::parser) fn at_terminator(&self) -> bool {
        matches!(
            self.current_tag(),
            TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Eof
        ) || self.current_comment_is_line_terminator()
            || (self.comma_is_terminator && self.current_tag() == TokenTag::Comma)
    }

    fn current_comment_is_line_terminator(&self) -> bool {
        self.current_tag() == TokenTag::Comment
            && matches!(
                self.peek_tag(1),
                Some(TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Eof)
            )
    }

    pub(in crate::syntax::parser) fn expect_terminator(&mut self) -> usize {
        if matches!(self.current_tag(), TokenTag::Newline | TokenTag::Semicolon)
            || (self.comma_is_terminator && self.current_tag() == TokenTag::Comma)
        {
            let end = self.current_end();
            self.bump();
            end
        } else if self.current_comment_is_line_terminator() {
            let end = self.current_start();
            self.bump();
            if matches!(self.current_tag(), TokenTag::Newline | TokenTag::Semicolon) {
                self.bump();
            }
            end
        } else if matches!(self.current_tag(), TokenTag::RBrace | TokenTag::Eof) {
            self.previous_end()
        } else {
            self.diagnostic_here("expected statement terminator", "parse.expected-terminator");
            self.current_start()
        }
    }

    pub(in crate::syntax::parser) fn recover_statement(&mut self) {
        while !self.at_terminator() && !self.at(TokenKindMatch::Eof) {
            self.bump();
        }
        if !self.at(TokenKindMatch::Eof) {
            self.bump();
        }
    }

    pub(in crate::syntax::parser) fn recover_match_arm(&mut self) {
        while !matches!(
            self.current_tag(),
            TokenTag::Newline | TokenTag::Comma | TokenTag::RBrace | TokenTag::Eof
        ) {
            self.bump();
        }
        if matches!(self.current_tag(), TokenTag::Comma | TokenTag::Newline) {
            self.bump();
        }
    }

    pub(in crate::syntax::parser) fn expect_ident(&mut self, message: &str) -> Option<Name> {
        if self.current_tag() != TokenTag::Ident {
            self.diagnostic_here(message, "parse.expected-ident");
            return None;
        }
        let name = self
            .current_name()
            .expect("identifier token has name payload");
        self.bump();
        Some(name)
    }

    pub(in crate::syntax::parser) fn expect_member_name(&mut self, message: &str) -> Option<Name> {
        let Some(name) = self.current_member_name() else {
            self.diagnostic_here(message, "parse.expected-ident");
            return None;
        };
        self.bump();
        Some(name)
    }

    pub(in crate::syntax::parser) fn current_member_name(&self) -> Option<Name> {
        match self.current_tag() {
            TokenTag::Ident | TokenTag::ProcIdent => self.current_name(),
            TokenTag::Keyword => self
                .current_keyword()
                .map(|keyword| Name::intern(keyword.as_str())),
            _ => None,
        }
    }

    pub(in crate::syntax::parser) fn expect_proc_ident(&mut self, message: &str) -> Option<Name> {
        if !matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
            self.diagnostic_here(message, "parse.expected-ident");
            return None;
        }
        let name = self
            .current_name()
            .expect("identifier token has name payload");
        self.bump();
        Some(name)
    }

    pub(in crate::syntax::parser) fn expect_module_path_segment(
        &mut self,
        message: &str,
    ) -> Option<Name> {
        if !matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
            self.diagnostic_here(message, "parse.expected-ident");
            return None;
        }
        let name = self
            .current_name()
            .expect("identifier token has name payload");
        self.bump();
        Some(name)
    }

    pub(in crate::syntax::parser) fn expect_keyword(
        &mut self,
        keyword: Keyword,
        message: &str,
    ) -> Option<Span> {
        if self.at_keyword(keyword) {
            Some(self.bump())
        } else {
            self.diagnostic_here(message, "parse.expected-keyword");
            None
        }
    }

    pub(in crate::syntax::parser) fn consume_keyword(&mut self, keyword: Keyword) -> Option<Span> {
        if self.at_keyword(keyword) {
            Some(self.bump())
        } else {
            None
        }
    }

    pub(in crate::syntax::parser) fn at_keyword(&self, keyword: Keyword) -> bool {
        self.current_keyword() == Some(keyword)
    }

    pub(in crate::syntax::parser) fn at_ident(&self, ident: &str) -> bool {
        self.current_tag() == TokenTag::Ident
            && self.current_name().is_some_and(|name| name == ident)
    }

    pub(in crate::syntax::parser) fn expect(
        &mut self,
        kind: TokenKindMatch,
        message: &str,
    ) -> Option<Span> {
        if self.at(kind) {
            Some(self.bump())
        } else {
            self.diagnostic_here(message, "parse.expected-token");
            None
        }
    }

    pub(in crate::syntax::parser) fn consume(&mut self, kind: TokenKindMatch) -> Option<Span> {
        if self.at(kind) {
            Some(self.bump())
        } else {
            None
        }
    }

    /// Consume `..` (two adjacent Dot tokens). Used for slice syntax inside brackets.
    pub(in crate::syntax::parser) fn consume_dot_dot(&mut self) -> bool {
        if self.at(TokenKindMatch::Dot) && self.peek_tag(1) == Some(TokenTag::Dot) {
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    pub(in crate::syntax::parser) fn at(&self, kind: TokenKindMatch) -> bool {
        kind.matches(self.current_tag())
    }

    pub(in crate::syntax::parser) fn current_tag(&self) -> TokenTag {
        self.peek_tag(0)
            .expect("parser index always points at EOF token")
    }

    pub(in crate::syntax::parser) fn peek_tag(&self, distance: usize) -> Option<TokenTag> {
        self.token_table.tag_at(self.index + distance)
    }

    pub(in crate::syntax::parser) fn current_name(&self) -> Option<Name> {
        self.peek_name(0)
    }

    pub(in crate::syntax::parser) fn peek_name(&self, distance: usize) -> Option<Name> {
        self.token_table.name_at(self.index + distance)
    }

    pub(in crate::syntax::parser) fn current_keyword(&self) -> Option<Keyword> {
        self.peek_keyword(0)
    }

    pub(in crate::syntax::parser) fn peek_keyword(&self, distance: usize) -> Option<Keyword> {
        self.token_table.keyword_at(self.index + distance)
    }

    pub(in crate::syntax::parser) fn current_span(&self) -> Span {
        self.span_at(self.index)
            .expect("parser index always points at EOF token")
    }

    pub(in crate::syntax::parser) fn previous_span(&self) -> Span {
        self.span_at(self.index.saturating_sub(1))
            .expect("previous parser index points at a token")
    }

    pub(in crate::syntax::parser) fn current_start(&self) -> usize {
        self.start_at(self.index)
            .expect("parser index always points at EOF token")
    }

    pub(in crate::syntax::parser) fn current_end(&self) -> usize {
        self.end_at(self.index)
            .expect("parser index always points at EOF token")
    }

    pub(in crate::syntax::parser) fn previous_end(&self) -> usize {
        self.end_at(self.index.saturating_sub(1))
            .expect("previous parser index points at a token")
    }

    pub(in crate::syntax::parser) fn previous_start(&self) -> usize {
        self.start_at(self.index.saturating_sub(1))
            .expect("previous parser index points at a token")
    }

    pub(in crate::syntax::parser) fn peek_start(&self, distance: usize) -> Option<usize> {
        self.start_at(self.index + distance)
    }

    pub(in crate::syntax::parser) fn start_at(&self, index: usize) -> Option<usize> {
        self.token_table.start_at(index)
    }

    pub(in crate::syntax::parser) fn end_at(&self, index: usize) -> Option<usize> {
        self.token_table.end_at(index, self.source)
    }

    pub(in crate::syntax::parser) fn span_at(&self, index: usize) -> Option<Span> {
        self.token_table.span_at(index, self.source_id, self.source)
    }

    pub(in crate::syntax::parser) fn bump(&mut self) -> Span {
        let span = self.current_span();
        if self.current_tag() != TokenTag::Eof {
            self.index += 1;
        }
        span
    }

    pub(in crate::syntax::parser) fn diagnostic_here(&mut self, message: &str, code: &str) {
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(self.current_span(), message)),
        );
    }

    pub(in crate::syntax::parser) fn diagnostic_previous(&mut self, message: &str, code: &str) {
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(self.previous_span(), message)),
        );
    }

    pub(in crate::syntax::parser) fn diagnostic_at(
        &mut self,
        span: Span,
        message: &str,
        code: &str,
    ) {
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }

    pub(in crate::syntax::parser) fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.source_id, start, end)
    }
}

#[derive(Clone, Copy)]
enum TokenKindMatch {
    Eof,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    At,
    Question,
    Bang,
    DollarLBrace,
    Equals,
    Arrow,
    FatArrow,
    Minus,
    Lt,
    Gt,
    Pipe,
    PipeGt,
    GtGt,
    ErrorGt,
    ErrorGtGt,
}

impl TokenKindMatch {
    fn matches(self, tag: TokenTag) -> bool {
        matches!(
            (self, tag),
            (Self::Eof, TokenTag::Eof)
                | (Self::LParen, TokenTag::LParen)
                | (Self::RParen, TokenTag::RParen)
                | (Self::LBrace, TokenTag::LBrace)
                | (Self::RBrace, TokenTag::RBrace)
                | (Self::LBracket, TokenTag::LBracket)
                | (Self::RBracket, TokenTag::RBracket)
                | (Self::Comma, TokenTag::Comma)
                | (Self::Colon, TokenTag::Colon)
                | (Self::Dot, TokenTag::Dot)
                | (Self::At, TokenTag::At)
                | (Self::Question, TokenTag::Question)
                | (Self::Bang, TokenTag::Bang)
                | (Self::DollarLBrace, TokenTag::DollarLBrace)
                | (Self::Equals, TokenTag::Equals)
                | (Self::Arrow, TokenTag::Arrow)
                | (Self::FatArrow, TokenTag::FatArrow)
                | (Self::Minus, TokenTag::Minus)
                | (Self::Lt, TokenTag::Lt)
                | (Self::Gt, TokenTag::Gt)
                | (Self::Pipe, TokenTag::Pipe)
                | (Self::PipeGt, TokenTag::PipeGt)
                | (Self::GtGt, TokenTag::GtGt)
                | (Self::ErrorGt, TokenTag::ErrorGt)
                | (Self::ErrorGtGt, TokenTag::ErrorGtGt)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Parser, SourceId};
    use crate::syntax::arena::{ArenaCommand, ArenaCommandArgKind, ArenaStmtKind};

    #[test]
    fn parses_run_compound_word_and_result_operator() {
        let output =
            Parser::parse_source_arena_only(SourceId::new(0), "run make -j${cpu.count()} ?\n");

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let arena = &output.arena.arena;
        let root: Vec<_> = output.arena.statement_ids().collect();
        let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
            panic!("expected command");
        };
        let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
            panic!("expected run");
        };
        let form = arena.run_form(*run_id);
        assert!(form.propagate);
        let segments = arena.run_segments(form.segments);
        let args = arena.command_args(segments[0].args);
        assert_eq!(args.len(), 1);
        let ArenaCommandArgKind::Word(parts) = &args[0].kind else {
            panic!("expected word");
        };
        assert_eq!(arena.word_parts(*parts).len(), 2);
    }

    #[test]
    fn parses_bare_command_as_proc_not_run() {
        let output = Parser::parse_source_arena_only(SourceId::new(0), "make -j4\n");

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let arena = &output.arena.arena;
        let root: Vec<_> = output.arena.statement_ids().collect();
        let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
            panic!("expected command");
        };
        assert!(matches!(
            arena.command_stmt(cmd_id).command,
            ArenaCommand::Proc { .. }
        ));
    }

    #[test]
    fn rejects_proc_without_signature() {
        let output = Parser::parse_source_arena_only(SourceId::new(0), "proc build { }\n");

        assert!(
            output
                .diagnostics
                .iter()
                .any(|diag| diag.code.as_deref() == Some("parse.required-signature"))
        );
    }

    #[test]
    fn parses_declarations_control_flow_and_records() {
        let source = r#"
use fs
proc main(args: List[Str]) -> Result[Unit] {
  let pkg = { name: "x", version }
  if true { print "ok" } else { return Err(Error(kind: "x")) }
  for arg in args { print ${arg} }
}
"#;
        let output = Parser::parse_source_arena_only(SourceId::new(0), source);

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.arena.statement_ids().count(), 2);
    }

    #[test]
    fn rejects_expression_string_interpolation() {
        let output = Parser::parse_source_arena_only(SourceId::new(0), "let x = \"${name}\"\n");

        let diagnostic = output
            .diagnostics
            .iter()
            .find(|diag| diag.code.as_deref() == Some("parse.expr-string-interpolation"))
            .expect("expected interpolation diagnostic");
        assert!(diagnostic.message.contains("raw strings"));
        assert!(diagnostic.notes.iter().any(|note| note.contains("r\"\"\"")));
    }

    #[test]
    fn explains_dollar_names_in_expression_context() {
        let output =
            Parser::parse_source_arena_only(SourceId::new(0), "env { FOO = $foo } { print ok }\n");

        let diagnostic = output
            .diagnostics
            .iter()
            .find(|diag| diag.code.as_deref() == Some("parse.expected-expression"))
            .expect("expected expression diagnostic");
        assert!(diagnostic.message.contains("command-word syntax"));
        assert!(diagnostic.message.contains("use `name` directly"));
    }
}
