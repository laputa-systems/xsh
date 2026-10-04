pub(in crate::syntax::parser) use crate::diagnostic::{Diagnostic, FixHint, Label, Severity};
pub(in crate::syntax::parser) use crate::source::{SourceId, Span};
pub(in crate::syntax::parser) use crate::symbol::Name;
use crate::syntax::arena::{ArenaProgram, ArenaProgramBuilder, ArenaRange, TypeExprId};
use crate::syntax::cst::LazyCst;
use crate::syntax::grammar::{self, LineContinuation};
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
pub(crate) mod expr;
mod literals;
mod pattern;
mod stmt;
mod types;

pub(in crate::syntax::parser) use self::literals::{
    decode_bytes_literal_for, decode_interpolation_text_for,
    parse_interpolation_expr_arena_only_for,
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
    condition_expr: bool,
    block_depth: usize,
    parenthesized_expr_depth: usize,
    diagnostics: Vec<Diagnostic>,
}

/// The binary operator a token spells (with the keyword after it, for
/// `not in`), its precedence, and its token count, from the grammar's
/// operator table.
fn binary_op_for_token(tag: TokenTag, keyword: Option<Keyword>, next_keyword: Option<Keyword>) -> Option<(BinaryOp, u8, usize)> {
    grammar::binary_operator_at(tag, keyword, next_keyword)
        .map(|operator| (operator.op, operator.precedence, 1 + usize::from(operator.second.is_some())))
}

impl<'a> Parser<'a> {
    pub fn parse_source_arena_only(source_id: SourceId, source: &'a str) -> ArenaParseOutput {
        let symbols = crate::symbol::SymbolOwner::new();
        symbols.clone().with_current(|| {
            let lexed = Lexer::new_with_symbols(source_id, source, symbols.clone()).lex_compact();
            let mut parser = Self::new_with_token_table(source_id, source, lexed.token_table);
            parser.diagnostics.extend(lexed.diagnostics);
            let cst = LazyCst::new(parser.source_id, parser.source, parser.token_table.clone());
            let mut arena = ArenaProgramBuilder::with_source_and_token_capacity_and_symbols(
                parser.source,
                parser.token_table.len(),
                symbols,
            );
            parser.skip_separators();
            while !parser.at(TokenKindMatch::Eof) {
                if parser.parse_statement_arena_only(&mut arena).is_none() {
                    parser.recover_statement();
                }
                parser.skip_separators();
            }
            let mut program = arena.finish();
            program.attach_doc_comments(parser.source_id, parser.source);
            ArenaParseOutput {
                arena: program,
                cst,
                diagnostics: parser.diagnostics,
            }
        })
    }

    pub fn parse_source_into_arena_builder(
        source_id: SourceId,
        source: &'a str,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> ArenaParseFragment {
        let symbols = arena.symbol_owner().clone();
        symbols.with_current(|| {
            let lexed = Lexer::new_with_symbols(source_id, source, symbols.clone()).lex_compact();
            let mut parser = Self::new_with_token_table(source_id, source, lexed.token_table);
            parser.diagnostics.extend(lexed.diagnostics);
            parser.parse_into_arena_builder(arena)
        })
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
            condition_expr: false,
            block_depth: 0,
            parenthesized_expr_depth: 0,
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
        arena.attach_doc_comments_for_statements(self.source_id, self.source, statements);
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
        let mut program = arena.finish();
        program.attach_doc_comments(self.source_id, self.source);
        program
    }

    pub(in crate::syntax::parser) fn current_binary_op(&self) -> Option<(BinaryOp, u8, usize)> {
        binary_op_for_token(self.current_tag(), self.current_keyword(), self.peek_keyword(1))
    }

    /// The line continuation that the first token after the line breaks and
    /// comments at `index` begins, with that token's index.
    fn line_continuation_at(&self, index: usize) -> Option<(LineContinuation, usize)> {
        let mut first = index;
        while matches!(self.token_table.tag_at(first), Some(TokenTag::Newline | TokenTag::Comment)) {
            first += 1;
        }
        if first == index {
            return None;
        }
        let continuation = grammar::line_continuation(
            self.token_table.tag_at(first)?,
            self.token_table.keyword_at(first),
            self.token_table.tag_at(first + 1),
            self.token_table.keyword_at(first + 1),
        )?;
        Some((continuation, first))
    }

    /// If the current token is a newline/comment and the next line starts with
    /// a binary operator that continues lines, return the binary op info, so
    /// the expression continues across the line break.
    pub(in crate::syntax::parser) fn continuation_binary_op(
        &self,
    ) -> Option<(BinaryOp, u8, usize)> {
        let (LineContinuation::Operator, first) = self.line_continuation_at(self.index)? else {
            return None;
        };
        binary_op_for_token(self.token_table.tag_at(first)?, self.token_table.keyword_at(first), self.token_table.keyword_at(first + 1))
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
            || (self.parenthesized_expr_depth > 0 && self.at(TokenKindMatch::RParen))
    }

    pub(in crate::syntax::parser) fn at_pipe_stage_end(&mut self) -> bool {
        self.skip_comments();
        self.at_terminator() || self.at(TokenKindMatch::Eof) || self.at(TokenKindMatch::PipeGt)
            || self.at(TokenKindMatch::RParen) || self.at(TokenKindMatch::RBracket)
            || self.at(TokenKindMatch::Comma)
    }

    pub(in crate::syntax::parser) fn is_word_part_start(&self) -> bool {
        if self.parenthesized_expr_depth > 0 && self.current_tag() == TokenTag::RParen {
            return false;
        }
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

    pub(in crate::syntax::parser) fn lookahead_is_ctx_block(&self) -> bool {
        if !self.at_ident("ctx") || self.peek_start(1) == Some(self.current_end()) { return false; }
        let mut depth = 0usize;
        for offset in 1.. {
            match self.peek_tag(offset) {
                Some(TokenTag::LParen | TokenTag::LBracket) => depth += 1,
                Some(TokenTag::RParen | TokenTag::RBracket) if depth > 0 => depth -= 1,
                Some(TokenTag::LBrace) if depth == 0 => return true,
                Some(TokenTag::Newline | TokenTag::Semicolon | TokenTag::Equals | TokenTag::RBrace) | None if depth == 0 => return false,
                Some(TokenTag::Dot) if offset == 1 => return false,
                None => return false,
                _ => {}
            }
        }
        unreachable!()
    }

    pub(in crate::syntax::parser) fn lookahead_is_assignment(&self) -> bool {
        let mut offset = 1;
        loop {
            match self.peek_tag(offset) {
                Some(TokenTag::Dot) if self.peek_tag(offset + 1) != Some(TokenTag::Dot) => {
                    if self.peek_label_name(offset + 1).is_some()
                        || self.peek_tag(offset + 1) == Some(TokenTag::ProcIdent) {
                        offset += 2;
                    } else { return false; }
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
                            TokenTag::Eof => {
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
                && !matches!(tag, TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Eof | TokenTag::PipeGt)
                && grammar::binary_operator_at(tag, self.token_table.keyword_at(index + 1), self.token_table.keyword_at(index + 2)).is_none()
                && !(tag == TokenTag::Ident && self.token_table.name_at(index + 1).is_some_and(|name| name == "is"))
        })
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

    /// Whether the name at the start of a statement is followed by a binary
    /// operator, so the statement is an expression rather than a command. A
    /// `not` counts even without `in`, and `-` counts only when it is spaced
    /// like an operator rather than written as a flag.
    pub(in crate::syntax::parser) fn lookahead_is_expr_binary(&self) -> bool {
        self.peek_tag(1).is_some_and(|tag| {
            tag != TokenTag::Minus
                && (self.peek_keyword(1) == Some(Keyword::Not) || grammar::binary_operator_at(tag, self.peek_keyword(1), self.peek_keyword(2)).is_some())
        }) || (self.peek_tag(1) == Some(TokenTag::Minus)
            && (self.peek_start(1) == Some(self.current_end())
                || self.peek_start(2) != self.peek_end(1)))
            || self.peek_name(1).is_some_and(|name| name == "is")
            || self.lookahead_past_newlines_is_pipe_gt()
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
        if let Some((LineContinuation::Pipeline, first)) = self.line_continuation_at(self.index) {
            self.index = first;
        }
    }

    /// Skips the newlines and comment lines between an expression and the
    /// line that continues it.
    pub(in crate::syntax::parser) fn skip_line_breaks(&mut self) {
        while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
            self.bump();
        }
    }

    /// Only `.name` continues: `./path` and `../path` begin bare paths.
    pub(in crate::syntax::parser) fn skip_postfix_newlines(&mut self) {
        if let Some((LineContinuation::Member, first)) = self.line_continuation_at(self.index) {
            self.index = first;
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
        } else if let Some(diagnostic) = self.foreign_statement_continuation() {
            let end = self.previous_end();
            self.diagnostics.push(diagnostic);
            self.recover_statement();
            end
        } else {
            self.diagnostic_here("expected statement terminator", "parse.expected-terminator");
            self.current_start()
        }
    }

    /// Constructs from other languages that look like a complete XSH
    /// statement followed by junk: `} catch e { ... }` after a `try` block,
    /// and `cond ? a : b`, which XSH reads as `cond?` followed by `a`. The
    /// caller skips the rest of the statement, so it is reported once.
    fn foreign_statement_continuation(&self) -> Option<Diagnostic> {
        let previous = self.token_table.tag_at(self.index.checked_sub(1)?)?;
        if previous == TokenTag::RBrace && self.current_name().is_some_and(|name| name == "catch" || name == "except") {
            return Some(
                Diagnostic::error("XSH has no `catch`: `try { ... }` produces a Result")
                    .with_code("parse.foreign-syntax")
                    .with_label(Label::primary(self.current_span(), "handle the Result with `??`, `match`, or `if let Err(error) = ...`")),
            );
        }
        let mut offset = 0;
        let at_question = self.current_tag() == TokenTag::Question;
        let ternary = (previous == TokenTag::Question || at_question) && loop {
            match self.peek_tag(offset) {
                Some(TokenTag::Colon) => break true,
                None | Some(TokenTag::Newline | TokenTag::Semicolon | TokenTag::LBrace | TokenTag::RBrace | TokenTag::Eof) => break false,
                _ => offset += 1,
            }
        };
        ternary.then(|| {
            Diagnostic::error("XSH has no `? :` conditional operator")
                .with_code("parse.foreign-syntax")
                .with_label(Label::primary(if at_question { self.current_span() } else { self.previous_span() }, "write `if condition { a } else { b }`"))
        })
    }

    /// Skip the rest of a statement that failed to parse. A `{` opened on the
    /// skipped line is skipped through its matching `}`, so the body of an
    /// unparseable head (`function f() {`, `} elif c {`) is not reparsed as
    /// statements and its closing brace is not reported as a stray token.
    pub(in crate::syntax::parser) fn recover_statement(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.current_tag() {
                TokenTag::Eof => return,
                TokenTag::LBrace => depth += 1,
                TokenTag::RBrace if depth > 0 => depth -= 1,
                _ if depth == 0 && self.at_terminator() => break,
                _ => {}
            }
            self.bump();
        }
        self.bump();
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

    /// Explicit labels retain their token spelling without declaring lexical names.
    pub(in crate::syntax::parser) fn current_label_name(&self) -> Option<Name> {
        self.peek_label_name(0)
    }

    pub(in crate::syntax::parser) fn peek_label_name(&self, distance: usize) -> Option<Name> {
        self.token_table.label_text_at(self.index + distance).map(Name::intern)
    }

    pub(in crate::syntax::parser) fn expect_label_name(&mut self, message: &str) -> Option<Name> {
        let Some(name) = self.current_label_name() else {
            self.diagnostic_here(message, "parse.expected-label");
            return None;
        };
        self.bump();
        Some(name)
    }

    pub(in crate::syntax::parser) fn require_label_binding_name(&mut self, tag: TokenTag, span: Span) -> bool {
        if tag == TokenTag::Ident { return true; }
        self.diagnostics.push(Diagnostic::error("field labels cannot declare a keyword binding")
            .with_code("parse.keyword-label-binding")
            .with_label(Label::primary(span, "supply an explicit value or rename this field to a legal binding name")));
        false
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
        if self.current_tag() == TokenTag::ProcIdent { self.current_name() }
        else { self.current_label_name() }
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

    pub(in crate::syntax::parser) fn peek_end(&self, distance: usize) -> Option<usize> {
        self.index
            .checked_add(distance)
            .and_then(|i| self.end_at(i))
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
        if self.follows_invalid_source() {
            return;
        }
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(self.current_span(), message)),
        );
    }

    /// The lexer emits no token for source it rejects (a `'...'` string or a
    /// `$(...)` substitution), and its diagnostic already names the mistake.
    /// A parse error at the token right after that gap is only its echo.
    fn follows_invalid_source(&self) -> bool {
        let gap_start = if self.index == 0 { 0 } else { self.previous_end() };
        let gap_end = self.current_start();
        self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref().is_some_and(|code| code.starts_with("lex."))
                && diagnostic.labels.first().is_some_and(|label| label.span.start() >= gap_start && label.span.end() <= gap_end)
        })
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
    fn rejects_interpolation_in_path_strings() {
        let output = Parser::parse_source_arena_only(SourceId::new(0), "let x = p\"${name}\"\n");

        let diagnostic = output
            .diagnostics
            .iter()
            .find(|diag| diag.code.as_deref() == Some("parse.path-string-interpolation"))
            .expect("expected p-string interpolation diagnostic");
        assert!(diagnostic.message.contains("do not interpolate"));
        assert!(diagnostic.notes.iter().any(|note| note.contains("literal")));

        let escaped = Parser::parse_source_arena_only(SourceId::new(0), "let x = p\"\\${name}\"\n");
        assert!(
            escaped.diagnostics.is_empty(),
            "escaped interpolation marker should remain literal: {:?}",
            escaped.diagnostics
        );
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
