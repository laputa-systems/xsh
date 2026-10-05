use crate::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use crate::source::{SourceId, Span};
use crate::symbol::{Name, SymbolOwner};
use crate::syntax::grammar;
use crate::syntax::literal::{self, QuotedLiteralKind, QuotedScan};
use crate::syntax::token::{Keyword, TokenKind, TokenTable, TokenTableBuilder, TokenTag};

#[derive(Clone, Debug, Default)]
pub struct CompactLexerOutput {
    pub token_table: TokenTable,
    pub diagnostics: Vec<Diagnostic>,
    _symbols: SymbolOwner,
}

pub struct Lexer<'a> {
    source_id: SourceId,
    source: &'a str,
    offset: usize,
    token_builder: TokenTableBuilder,
    diagnostics: Vec<Diagnostic>,
    symbols: SymbolOwner,
    // A scanner only walks token boundaries: it records no tokens and interns
    // no names, so it can run without a symbol owner.
    scan_only: bool,
}

/// How the tokens after an f-string `{` end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InterpolationEnd {
    /// The offset of the `}` that closes the interpolation.
    Close(usize),
    /// A `#` comment starts at this offset before the closing `}`.
    Comment(usize),
    Unclosed,
}

// The lexer validates a literal it decodes itself, and the parser decodes the
// text of interpolating literals and command words. Both report the same
// condition through these constructors, so one code names it.

pub(crate) fn invalid_escape_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::error("invalid escape sequence")
        .with_code(DiagnosticCode::LexInvalidEscape)
        .with_label(Label::primary(span, "unsupported escape sequence"))
}

pub(crate) fn bytes_unicode_escape_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::error("unicode escapes are not valid in bytes literals")
        .with_code(DiagnosticCode::LexInvalidBytesEscape)
        .with_label(Label::primary(span, "bytes literals use byte escapes only"))
}

pub(crate) fn invalid_utf8_string_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::error("string literal is not valid UTF-8")
        .with_code(DiagnosticCode::LexInvalidString)
        .with_label(Label::primary(span, "invalid string literal"))
}

/// Finds the end of the f-string interpolation whose expression starts at
/// `start`, just after its `{`, by lexing real tokens: the closing `}` is the
/// first one outside every bracket, string, and nested f-string the lexer
/// sees, so the expression parser and this boundary always agree.
pub(crate) fn interpolation_end(source: &str, start: usize) -> InterpolationEnd {
    let mut lexer = Lexer {
        source_id: SourceId::new(0),
        source,
        offset: start,
        token_builder: TokenTableBuilder::default(),
        diagnostics: Vec::new(),
        symbols: SymbolOwner::new(),
        scan_only: true,
    };
    let mut depth = 0usize;
    loop {
        lexer.lex_whitespace();
        let token_start = lexer.offset;
        match lexer.peek_byte() {
            None => return InterpolationEnd::Unclosed,
            Some(b'#') => return InterpolationEnd::Comment(token_start),
            _ => {}
        }
        lexer.lex_token();
        if lexer.offset == token_start {
            lexer.offset += 1;
        }
        match &source[token_start..lexer.offset] {
            "(" | "[" | "{" | "${" => depth += 1,
            ")" | "]" => depth = depth.saturating_sub(1),
            "}" if depth == 0 => return InterpolationEnd::Close(token_start),
            "}" => depth -= 1,
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringLiteralKind {
    Str,
    Bytes,
    Regex,
    Path,
    Glob,
    Env,
}

impl<'a> Lexer<'a> {
    pub fn new(source_id: SourceId, source: &'a str) -> Self {
        Self::new_with_symbols(source_id, source, SymbolOwner::new())
    }

    pub fn new_with_symbols(source_id: SourceId, source: &'a str, symbols: SymbolOwner) -> Self {
        // Shell-like source averages ~4 bytes/token across the checked-in corpus;
        // sizing the token table up front avoids repeated doubling reallocations
        // for every file lexed (`TokenTableBuilder::default()` starts at capacity 0).
        let estimated_tokens = source.len() / 3 + 1;
        Self {
            source_id,
            source,
            offset: 0,
            token_builder: TokenTableBuilder::with_capacity(estimated_tokens),
            diagnostics: Vec::new(),
            symbols,
            scan_only: false,
        }
    }

    pub fn lex_compact(mut self) -> CompactLexerOutput {
        let symbols = self.symbols.clone();
        symbols.with_current(|| self.lex_source());
        CompactLexerOutput {
            token_table: std::mem::take(&mut self.token_builder).finish(),
            diagnostics: self.diagnostics,
            _symbols: self.symbols,
        }
    }

    fn lex_source(&mut self) {
        while !self.is_eof() {
            self.lex_token();
        }

        self.push(TokenKind::Eof, self.offset, self.offset);
    }

    fn lex_token(&mut self) {
        {
            let start = self.offset;
            let quoted = grammar::quoted_literal_at(self.source.as_bytes(), start);
            match self.peek_byte() {
                Some(b' ' | b'\t') => self.lex_whitespace(),
                Some(b'\r') if self.peek_next_byte() == Some(b'\n') => {
                    self.offset += 2;
                    self.push(TokenKind::Newline, start, self.offset);
                }
                Some(b'\n') => {
                    self.offset += 1;
                    self.push(TokenKind::Newline, start, self.offset);
                }
                Some(b'#') => self.lex_comment(),
                Some(_) if let Some(form) = quoted => {
                    self.offset += form.prefix.len();
                    match form.kind {
                        QuotedLiteralKind::Fmt => {
                            self.offset = start;
                            self.lex_fmt_string();
                        }
                        QuotedLiteralKind::PathFmt => {
                            self.offset = start;
                            self.lex_path_fmt_string();
                        }
                        QuotedLiteralKind::Str => {
                            self.lex_string(StringLiteralKind::Str, form.raw, start)
                        }
                        QuotedLiteralKind::Bytes => {
                            self.lex_string(StringLiteralKind::Bytes, form.raw, start)
                        }
                        QuotedLiteralKind::Regex => {
                            self.lex_string(StringLiteralKind::Regex, form.raw, start)
                        }
                        QuotedLiteralKind::Path => {
                            self.lex_string(StringLiteralKind::Path, form.raw, start)
                        }
                        QuotedLiteralKind::Glob => {
                            self.lex_string(StringLiteralKind::Glob, form.raw, start)
                        }
                        QuotedLiteralKind::Env => {
                            self.lex_string(StringLiteralKind::Env, form.raw, start)
                        }
                    }
                }
                Some(b'E')
                    if self.peek_next_byte() == Some(b'>')
                        && self.source.as_bytes().get(self.offset + 2) == Some(&b'>') =>
                {
                    self.offset += 3;
                    self.push(TokenKind::ErrorGtGt, start, self.offset);
                }
                Some(b'E') if self.peek_next_byte() == Some(b'>') => {
                    self.offset += 2;
                    self.push(TokenKind::ErrorGt, start, self.offset);
                }
                Some(byte) if is_ident_start(byte) => self.lex_ident_or_keyword(),
                Some(byte) if byte.is_ascii_digit() => self.lex_number(),
                Some(b'$') if self.peek_next_byte() == Some(b'?') => {
                    self.offset += 2;
                    self.push(TokenKind::LastStatus, start, self.offset);
                }
                Some(b'$') if self.peek_next_byte() == Some(b'{') => {
                    self.offset += 2;
                    self.push(TokenKind::DollarLBrace, start, self.offset);
                }
                Some(b'$') if self.peek_next_byte().is_some_and(is_ident_start) => {
                    self.offset += 2;
                    while matches!(self.peek_byte(), Some(byte) if is_ident_continue(byte)) {
                        self.offset += 1;
                    }
                    if self.scan_only {
                        return;
                    }
                    self.push(
                        TokenKind::DollarIdent(Name::intern(&self.source[start + 1..self.offset])),
                        start,
                        self.offset,
                    );
                }
                Some(b'-') if self.peek_next_byte() == Some(b'>') => {
                    self.offset += 2;
                    self.push(TokenKind::Arrow, start, self.offset);
                }
                Some(b'=') if self.peek_next_byte() == Some(b'>') => {
                    self.offset += 2;
                    self.push(TokenKind::FatArrow, start, self.offset);
                }
                Some(b'=') if self.peek_next_byte() == Some(b'=') => {
                    self.offset += 2;
                    self.push(TokenKind::EqEq, start, self.offset);
                }
                Some(b'!') if self.peek_next_byte() == Some(b'=') => {
                    self.offset += 2;
                    self.push(TokenKind::BangEq, start, self.offset);
                }
                Some(b'<') if self.peek_next_byte() == Some(b'=') => {
                    self.offset += 2;
                    self.push(TokenKind::Le, start, self.offset);
                }
                Some(b'>') if self.peek_next_byte() == Some(b'=') => {
                    self.offset += 2;
                    self.push(TokenKind::Ge, start, self.offset);
                }
                Some(b'>') if self.peek_next_byte() == Some(b'>') => {
                    self.offset += 2;
                    self.push(TokenKind::GtGt, start, self.offset);
                }
                Some(b'?') if self.peek_next_byte() == Some(b'?') => {
                    self.offset += 2;
                    self.push(TokenKind::QuestionQuestion, start, self.offset);
                }
                Some(b'|') if self.peek_next_byte() == Some(b'>') => {
                    self.offset += 2;
                    self.push(TokenKind::PipeGt, start, self.offset);
                }
                Some(byte) => {
                    self.offset += 1;
                    match byte {
                        b'(' => self.push(TokenKind::LParen, start, self.offset),
                        b')' => self.push(TokenKind::RParen, start, self.offset),
                        b'{' => self.push(TokenKind::LBrace, start, self.offset),
                        b'}' => self.push(TokenKind::RBrace, start, self.offset),
                        b'[' => self.push(TokenKind::LBracket, start, self.offset),
                        b']' => self.push(TokenKind::RBracket, start, self.offset),
                        b',' => self.push(TokenKind::Comma, start, self.offset),
                        b':' => self.push(TokenKind::Colon, start, self.offset),
                        b';' => self.push(TokenKind::Semicolon, start, self.offset),
                        b'.' => self.push(TokenKind::Dot, start, self.offset),
                        b'@' => self.push(TokenKind::At, start, self.offset),
                        b'?' => self.push(TokenKind::Question, start, self.offset),
                        b'=' => self.push(TokenKind::Equals, start, self.offset),
                        b'!' => self.push(TokenKind::Bang, start, self.offset),
                        b'<' => self.push(TokenKind::Lt, start, self.offset),
                        b'>' => self.push(TokenKind::Gt, start, self.offset),
                        b'+' => self.push(TokenKind::Plus, start, self.offset),
                        b'-' => self.push(TokenKind::Minus, start, self.offset),
                        b'*' => self.push(TokenKind::Star, start, self.offset),
                        b'/' => self.push(TokenKind::Slash, start, self.offset),
                        b'%' => self.push(TokenKind::Percent, start, self.offset),
                        b'|' => self.push(TokenKind::Pipe, start, self.offset),
                        b'&' => self.push(TokenKind::Amp, start, self.offset),
                        b'\'' if self.report_single_quoted_text(start) => {}
                        b'$' if self.report_command_substitution(start) => {}
                        _ => {
                            // Skip the whole character so the span stays on a
                            // character boundary and a multi-byte character is
                            // reported once.
                            self.offset = start
                                + self
                                    .source
                                    .get(start..)
                                    .and_then(|rest| rest.chars().next())
                                    .map_or(1, char::len_utf8);
                            self.diagnostics.push(
                                Diagnostic::error("unexpected character")
                                    .with_code(DiagnosticCode::LexUnexpectedCharacter)
                                    .with_label(Label::primary(
                                        self.span(start, self.offset),
                                        "not valid in source",
                                    )),
                            );
                        }
                    }
                }
                None => {}
            }
        }
    }

    fn lex_whitespace(&mut self) {
        while matches!(self.peek_byte(), Some(b' ' | b'\t')) {
            self.offset += 1;
        }
    }

    fn lex_comment(&mut self) {
        let start = self.offset;
        self.offset += 1;
        while !matches!(self.peek_byte(), None | Some(b'\n' | b'\r')) {
            self.offset += 1;
        }
        self.push(TokenKind::Comment, start, self.offset);
    }

    fn lex_ident_or_keyword(&mut self) {
        let start = self.offset;
        self.offset += 1;
        while matches!(self.peek_byte(), Some(byte) if is_ident_continue(byte) || byte == b'-') {
            self.offset += 1;
        }
        let text = &self.source[start..self.offset];
        if self.scan_only {
            return;
        }
        if let Some(keyword) = Keyword::from_ident(text) {
            self.push(TokenKind::Keyword(keyword), start, self.offset);
        } else if text.contains('-') {
            self.push(TokenKind::ProcIdent(Name::intern(text)), start, self.offset);
        } else {
            self.push(TokenKind::Ident(Name::intern(text)), start, self.offset);
        }
    }

    fn lex_number(&mut self) {
        let start = self.offset;
        if self.peek_byte() == Some(b'0') && self.peek_next_byte() == Some(b'o') {
            self.offset += 2;
            let digits_start = self.offset;
            while matches!(self.peek_byte(), Some(b'0'..=b'7')) {
                self.offset += 1;
            }
            if matches!(self.peek_byte(), Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_')
            {
                self.offset += 1;
                while matches!(self.peek_byte(), Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_')
                {
                    self.offset += 1;
                }
                self.diagnostics.push(
                    Diagnostic::error("invalid octal integer literal")
                        .with_code(DiagnosticCode::LexInvalidOctal)
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "octal literals use digits 0 through 7",
                        )),
                );
            }
            if self.offset == digits_start {
                self.diagnostics.push(
                    Diagnostic::error("invalid octal integer literal")
                        .with_code(DiagnosticCode::LexInvalidOctal)
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "expected octal digits after 0o",
                        )),
                );
            }
            self.push(TokenKind::Int, start, self.offset);
            return;
        }

        while matches!(self.peek_byte(), Some(byte) if byte.is_ascii_digit()) {
            self.offset += 1;
        }
        let mut is_float = false;
        if self.peek_byte() == Some(b'.')
            && matches!(self.peek_next_byte(), Some(byte) if byte.is_ascii_digit())
        {
            is_float = true;
            self.offset += 1;
            while matches!(self.peek_byte(), Some(byte) if byte.is_ascii_digit()) {
                self.offset += 1;
            }
        }
        if matches!(self.peek_byte(), Some(b'e' | b'E')) {
            is_float = true;
            self.offset += 1;
            if matches!(self.peek_byte(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            let digits_start = self.offset;
            while matches!(self.peek_byte(), Some(byte) if byte.is_ascii_digit()) {
                self.offset += 1;
            }
            if self.offset == digits_start {
                self.diagnostics.push(
                    Diagnostic::error("invalid float literal")
                        .with_code(DiagnosticCode::LexInvalidFloat)
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "expected exponent digits",
                        )),
                );
            }
        }
        if is_float {
            self.push(TokenKind::Float, start, self.offset);
            return;
        }
        if let Some(suffix) = grammar::duration_suffix_at(self.source.as_bytes(), self.offset) {
            self.offset += suffix.len();
            self.push(TokenKind::Duration, start, self.offset);
            return;
        }
        self.push(TokenKind::Int, start, self.offset);
    }

    fn lex_fmt_string(&mut self) {
        let start = self.offset;
        match literal::scan_quoted_literal(self.source, start, true) {
            Some(QuotedScan::Terminated(literal)) if literal.kind == QuotedLiteralKind::Fmt => {
                self.offset = literal.end;
                self.push(
                    TokenKind::FmtString {
                        raw_literal: literal.raw,
                    },
                    start,
                    self.offset,
                );
            }
            Some(QuotedScan::Unterminated { end }) => {
                self.offset = end;
                self.diagnostics.push(
                    Diagnostic::error("unterminated fmt string literal")
                        .with_code(DiagnosticCode::LexUnterminatedFmtString)
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "fmt string literal starts here",
                        )),
                );
            }
            None => {
                self.diagnostics.push(
                    Diagnostic::error("unterminated fmt string literal")
                        .with_code(DiagnosticCode::LexUnterminatedFmtString)
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "fmt string literal starts here",
                        )),
                );
            }
            Some(QuotedScan::Terminated(_)) => {
                self.offset += 1;
                self.diagnostics.push(
                    Diagnostic::error("unterminated fmt string literal")
                        .with_code(DiagnosticCode::LexUnterminatedFmtString)
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "fmt string literal starts here",
                        )),
                );
            }
        }
    }

    fn lex_path_fmt_string(&mut self) {
        let start = self.offset;
        match literal::scan_quoted_literal(self.source, start, true) {
            Some(QuotedScan::Terminated(literal)) if literal.kind == QuotedLiteralKind::PathFmt => {
                self.offset = literal.end;
                self.push(TokenKind::PathFmtString, start, self.offset);
            }
            Some(QuotedScan::Unterminated { end }) => {
                self.offset = end;
                self.diagnostics.push(
                    Diagnostic::error("unterminated path fmt string literal")
                        .with_code(DiagnosticCode::LexUnterminatedPathFmtString)
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "path fmt string literal starts here",
                        )),
                );
            }
            None | Some(QuotedScan::Terminated(_)) => {
                self.offset += 1;
                self.diagnostics.push(
                    Diagnostic::error("unterminated path fmt string literal")
                        .with_code(DiagnosticCode::LexUnterminatedPathFmtString)
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "path fmt string literal starts here",
                        )),
                );
            }
        }
    }

    /// Shell and Python habit: `'text'`. The whole quoted run on one line is
    /// skipped as one invalid region with a double-quote fix, when the text
    /// means the same inside `"..."`. A lone apostrophe is left to the
    /// generic unexpected-character report.
    fn report_single_quoted_text(&mut self, start: usize) -> bool {
        let rest = &self.source[start + 1..];
        let Some(close) = rest
            .find(['\'', '\n'])
            .filter(|&index| rest.as_bytes()[index] == b'\'')
        else {
            return false;
        };
        let text = &rest[..close];
        self.offset = start + 1 + close + 1;
        let span = self.span(start, self.offset);
        let mut diagnostic = Diagnostic::error("single quotes do not delimit strings in XSH")
            .with_code(DiagnosticCode::LexUnexpectedCharacter)
            .with_label(Label::primary(span, "XSH strings use double quotes"));
        if !text.contains(['"', '\\', '$', '{', '}']) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "use double quotes",
                format!("\"{text}\""),
            ));
        }
        self.diagnostics.push(diagnostic);
        true
    }

    /// Shell habit: `$(command)`. XSH runs commands with `run`, so the whole
    /// substitution on one line is skipped as one invalid region.
    fn report_command_substitution(&mut self, start: usize) -> bool {
        if self.source.as_bytes().get(start + 1) != Some(&b'(') {
            return false;
        }
        let mut depth = 0usize;
        let mut end = None;
        for (index, byte) in self.source.as_bytes()[start + 1..].iter().enumerate() {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + 1 + index + 1);
                        break;
                    }
                }
                b'\n' => break,
                _ => {}
            }
        }
        self.offset = end.unwrap_or(start + 1);
        self.diagnostics.push(
            Diagnostic::error("`$(...)` command substitution is shell syntax")
                .with_code(DiagnosticCode::LexUnexpectedCharacter)
                .with_label(Label::primary(
                    self.span(start, self.offset),
                    "capture a command's output with `run.text COMMAND ?`",
                )),
        );
        true
    }

    fn lex_string(&mut self, kind: StringLiteralKind, raw_literal: bool, literal_start: usize) {
        let quote_start = self.offset;
        let literal = match literal::scan_quoted_literal(self.source, literal_start, true) {
            Some(QuotedScan::Terminated(literal)) => literal,
            Some(QuotedScan::Unterminated { end }) => {
                self.offset = end;
                self.diagnostics.push(
                    Diagnostic::error("unterminated string literal")
                        .with_code(if kind == StringLiteralKind::Bytes {
                            DiagnosticCode::LexUnterminatedBytes
                        } else {
                            DiagnosticCode::LexUnterminatedString
                        })
                        .with_label(Label::primary(
                            self.span(literal_start.min(quote_start), self.offset),
                            "string literal starts here",
                        )),
                );
                return;
            }
            None => {
                self.offset = quote_start;
                self.diagnostics.push(
                    Diagnostic::error("unterminated string literal")
                        .with_code(DiagnosticCode::LexUnterminatedString)
                        .with_label(Label::primary(
                            self.span(literal_start.min(quote_start), self.offset),
                            "string literal starts here",
                        )),
                );
                return;
            }
        };
        let mut has_interpolation = false;
        // Escapes only shrink and interpolation markers only copy verbatim, so the
        // raw content span is always a safe upper bound on the decoded length.
        let mut decoded = Vec::with_capacity(literal.content_end - literal.content_start);

        self.offset = literal.content_start;
        while self.offset < literal.content_end {
            let byte = self
                .peek_byte()
                .expect("offset is inside scanned string literal content");
            match byte {
                b'$' if self.peek_next_byte() == Some(b'{')
                    && kind == StringLiteralKind::Str
                    && !raw_literal =>
                {
                    has_interpolation = true;
                    if let Some(end) = literal::interpolation_close(self.source, self.offset + 2)
                        && end < literal.content_end
                    {
                        decoded.extend_from_slice(&self.source.as_bytes()[self.offset..end + 1]);
                        self.offset = end + 1;
                    } else {
                        decoded.extend_from_slice(b"${");
                        self.offset += 2;
                    }
                }
                b'\\' if !raw_literal => {
                    let escape_start = self.offset;
                    self.offset += 1;
                    self.decode_escape(
                        kind == StringLiteralKind::Bytes,
                        escape_start,
                        &mut decoded,
                    );
                }
                byte => {
                    decoded.push(byte);
                    self.offset += 1;
                }
            }
        }

        self.offset = literal.end;
        if kind == StringLiteralKind::Bytes {
            self.push(TokenKind::Bytes, literal_start, self.offset);
        } else {
            match String::from_utf8(decoded) {
                Ok(value) => match kind {
                    StringLiteralKind::Str => self.push(
                        TokenKind::String {
                            has_interpolation,
                            raw_literal,
                        },
                        literal_start,
                        self.offset,
                    ),
                    StringLiteralKind::Path => {
                        let _ = value;
                        self.push(TokenKind::PathString, literal_start, self.offset);
                    }
                    StringLiteralKind::Glob => {
                        let _ = value;
                        self.push(TokenKind::GlobString, literal_start, self.offset);
                    }
                    StringLiteralKind::Env => {
                        self.push(TokenKind::EnvString, literal_start, self.offset)
                    }
                    StringLiteralKind::Regex => {
                        self.push(TokenKind::Regex, literal_start, self.offset)
                    }
                    StringLiteralKind::Bytes => unreachable!(),
                },
                Err(_) => self.diagnostics.push(invalid_utf8_string_diagnostic(
                    self.span(literal_start, self.offset),
                )),
            }
        }
    }

    fn decode_escape(&mut self, bytes: bool, escape_start: usize, decoded: &mut Vec<u8>) {
        let Some(byte) = self.peek_byte() else {
            self.invalid_escape(escape_start, self.offset);
            return;
        };
        self.offset += 1;
        match byte {
            b'\\' => decoded.push(b'\\'),
            b'"' => decoded.push(b'"'),
            b'$' if !bytes => decoded.push(b'$'),
            b'n' => decoded.push(b'\n'),
            b'r' => decoded.push(b'\r'),
            b't' => decoded.push(b'\t'),
            b'0' => decoded.push(0),
            b'x' => {
                let hex_start = self.offset;
                if self.offset + 2 <= self.source.len() {
                    let hex = &self.source[self.offset..self.offset + 2];
                    if let Ok(value) = u8::from_str_radix(hex, 16) {
                        decoded.push(value);
                        self.offset += 2;
                        return;
                    }
                }
                self.invalid_escape(escape_start, hex_start);
            }
            b'u' if !bytes && self.peek_byte() == Some(b'{') => {
                self.offset += 1;
                let digits_start = self.offset;
                while matches!(self.peek_byte(), Some(byte) if byte.is_ascii_hexdigit()) {
                    self.offset += 1;
                }
                if self.peek_byte() == Some(b'}') {
                    let digits = &self.source[digits_start..self.offset];
                    self.offset += 1;
                    if let Ok(value) = u32::from_str_radix(digits, 16)
                        && let Some(ch) = char::from_u32(value)
                    {
                        let mut buf = [0; 4];
                        decoded.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        return;
                    }
                }
                self.invalid_escape(escape_start, self.offset);
            }
            b'u' if bytes => self.diagnostics.push(bytes_unicode_escape_diagnostic(
                self.span(escape_start, self.offset),
            )),
            _ => self.invalid_escape(escape_start, self.offset),
        }
    }

    fn invalid_escape(&mut self, start: usize, end: usize) {
        self.diagnostics
            .push(invalid_escape_diagnostic(self.span(start, end.max(start + 1))));
    }

    fn push(&mut self, kind: TokenKind, start: usize, _end: usize) {
        if self.scan_only {
            return;
        }
        self.token_builder.push_kind(&kind, start);
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.source_id, start, end)
    }

    fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset).copied()
    }

    fn peek_next_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset + 1).copied()
    }

    fn is_eof(&self) -> bool {
        self.offset >= self.source.len()
    }
}

/// The tokens of `source`, without the end-of-file token, with their text.
pub fn lex_spellings(source: &str) -> Vec<(TokenTag, &str)> {
    let table = Lexer::new(SourceId::new(0), source)
        .lex_compact()
        .token_table;
    (0..table.len())
        .filter_map(|index| {
            let tag = table.tag_at(index)?;
            let span = table.span_at(index, SourceId::new(0), source)?;
            (tag != TokenTag::Eof).then(|| (tag, &source[span.range()]))
        })
        .collect()
}

/// Whether `left` written directly before `right` still lexes as the tokens
/// of `left` followed by the tokens of `right`. Printers that join tokens
/// without whitespace consult this instead of per-token rules.
pub fn tokens_stay_separate(left: &str, right: &str) -> bool {
    let joined = format!("{left}{right}");
    let mut expected = lex_spellings(left);
    expected.extend(lex_spellings(right));
    lex_spellings(&joined) == expected
}

/// `right` appended to `left`, separated by a space when written directly
/// after `left` its tokens would merge with `left`'s last token.
pub fn join_tokens(left: &str, right: &str) -> String {
    let last = lex_spellings(left).last().map_or("", |(_, text)| *text);
    let first = lex_spellings(right).first().map_or("", |(_, text)| *text);
    if tokens_stay_separate(last, first) {
        format!("{left}{right}")
    } else {
        format!("{left} {right}")
    }
}

/// Source spellings that cover every token kind and every lexer decision
/// that depends on the bytes around a token: each fixed spelling, each
/// keyword, an identifier for every identifier-start byte (so the `b"`,
/// `p"`, `g"`, `r"`, `rx"`, `f"`, `fp"`, `E>`, exponent, and duration-suffix
/// rules all meet a neighbor), and each literal form.
pub fn representative_token_texts() -> Vec<(TokenTag, String)> {
    let mut texts: Vec<(TokenTag, String)> = TokenTag::ALL
        .iter()
        .filter_map(|tag| {
            tag.fixed_text()
                .filter(|text| !text.is_empty())
                .map(|text| (*tag, text.to_owned()))
        })
        .collect();
    texts.extend(
        Keyword::ALL
            .iter()
            .map(|keyword| (TokenTag::Keyword, keyword.as_str().to_owned())),
    );
    for byte in (0u8..128).filter(|byte| is_ident_start(*byte)) {
        texts.push((TokenTag::Ident, char::from(byte).to_string()));
    }
    for (tag, text) in [
        (TokenTag::Ident, "rx"),
        (TokenTag::Ident, "fp"),
        (TokenTag::Ident, "name1"),
        (TokenTag::ProcIdent, "proc-name"),
        (TokenTag::Int, "1"),
        (TokenTag::Int, "0"),
        (TokenTag::Int, "0o7"),
        (TokenTag::Float, "1.5"),
        (TokenTag::Float, "1e3"),
        (TokenTag::Duration, "1s"),
        (TokenTag::Duration, "1ms"),
        (TokenTag::String, "\"s\""),
        (TokenTag::String, "r\"s\""),
        (TokenTag::String, "\"\"\"s\"\"\""),
        (TokenTag::PathString, "p\"s\""),
        (TokenTag::GlobString, "g\"s\""),
        (TokenTag::EnvString, "e\"S\""),
        (TokenTag::FmtString, "f\"s\""),
        (TokenTag::PathFmtString, "fp\"s\""),
        (TokenTag::Bytes, "b\"s\""),
        (TokenTag::Regex, "rx\"s\""),
        (TokenTag::Comment, "# c"),
        (TokenTag::Newline, "\n"),
        (TokenTag::DollarIdent, "$name"),
    ] {
        texts.push((tag, text.to_owned()));
    }
    texts
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::{Keyword, Lexer, SourceId, TokenTag};
    use crate::diagnostic::DiagnosticCode;

    #[test]
    fn tokenizes_keywords_identifiers_strings_comments_and_eof() {
        let source_id = SourceId::new(0);
        let output = Lexer::new(source_id, "let name = \"x\" # comment\nrun make\n").lex_compact();

        assert!(output.diagnostics.is_empty());
        let table = output.token_table;
        assert_eq!(
            (0..table.len())
                .filter_map(|index| table.tag_at(index))
                .collect::<Vec<_>>(),
            vec![
                TokenTag::Keyword,
                TokenTag::Ident,
                TokenTag::Equals,
                TokenTag::String,
                TokenTag::Comment,
                TokenTag::Newline,
                TokenTag::Keyword,
                TokenTag::Ident,
                TokenTag::Newline,
                TokenTag::Eof,
            ]
        );
        assert_eq!(table.keyword_at(0), Some(Keyword::Let));
        assert!(table.name_at(1).is_some_and(|name| name == "name"));
        assert_eq!(table.keyword_at(6), Some(Keyword::Run));
        assert!(table.name_at(7).is_some_and(|name| name == "make"));
        assert_eq!(
            table
                .string_flags_at(3)
                .map(|flags| (flags.has_interpolation, flags.raw_literal)),
            Some((false, false))
        );
    }

    #[test]
    fn regex_tokens_preserve_raw_contents_and_full_delimiter_spans() {
        let source = "rx\"\\d+\\$\\{name\\}\" rx\"\"\"(?x)\n[a-z]+ # flags\n\"\"\" rx";
        let output = Lexer::new(SourceId::new(0), source).lex_compact();
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let table = output.token_table;
        assert_eq!(table.tag_at(0), Some(TokenTag::Regex));
        assert_eq!(table.tag_at(1), Some(TokenTag::Regex));
        assert_eq!(table.tag_at(2), Some(TokenTag::Ident));
        let first = table.span_at(0, SourceId::new(0), source).unwrap();
        let second = table.span_at(1, SourceId::new(0), source).unwrap();
        assert_eq!(&source[first.range()], r#"rx"\d+\$\{name\}""#);
        assert_eq!(
            &source[second.range()],
            "rx\"\"\"(?x)\n[a-z]+ # flags\n\"\"\""
        );
    }

    #[test]
    fn tokenizes_command_interpolation_boundaries() {
        let output = Lexer::new(SourceId::new(0), "run make -j${cpu.count()}\n").lex_compact();

        assert!(output.diagnostics.is_empty());
        assert!(
            (0..output.token_table.len())
                .any(|index| output.token_table.tag_at(index) == Some(TokenTag::DollarLBrace))
        );
    }

    #[test]
    fn rejects_invalid_bytes_unicode_escape() {
        let output = Lexer::new(SourceId::new(0), "let b = b\"\\u{41}\"\n").lex_compact();

        assert_eq!(output.diagnostics.len(), 1);
        assert_eq!(
            output.diagnostics[0].code.map(DiagnosticCode::name),
            Some("lex.invalid-bytes-escape")
        );
    }
}
