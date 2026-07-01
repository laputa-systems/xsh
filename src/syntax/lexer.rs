use crate::diagnostic::{Diagnostic, Label};
use crate::source::{SourceId, Span};
use crate::symbol::Name;
use crate::syntax::literal::{self, QuotedLiteralKind, QuotedScan};
use crate::syntax::token::{Keyword, TokenKind, TokenTable, TokenTableBuilder};

#[derive(Clone, Debug, Default)]
pub struct CompactLexerOutput {
    pub token_table: TokenTable,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct Lexer<'a> {
    source_id: SourceId,
    source: &'a str,
    offset: usize,
    token_builder: TokenTableBuilder,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringLiteralKind {
    Str,
    Bytes,
    Path,
    Glob,
}

impl<'a> Lexer<'a> {
    pub fn new(source_id: SourceId, source: &'a str) -> Self {
        Self {
            source_id,
            source,
            offset: 0,
            token_builder: TokenTableBuilder::default(),
            diagnostics: Vec::new(),
        }
    }

    pub fn lex_compact(mut self) -> CompactLexerOutput {
        self.lex_source();
        CompactLexerOutput {
            token_table: std::mem::take(&mut self.token_builder).finish(),
            diagnostics: self.diagnostics,
        }
    }

    fn lex_source(&mut self) {
        while !self.is_eof() {
            let start = self.offset;
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
                Some(b'"') => self.lex_string(StringLiteralKind::Str, false, start),
                Some(b'b') if self.peek_next_byte() == Some(b'"') => {
                    self.offset += 1;
                    self.lex_string(StringLiteralKind::Bytes, false, start);
                }
                Some(b'p') if self.peek_next_byte() == Some(b'"') => {
                    self.offset += 1;
                    self.lex_string(StringLiteralKind::Path, false, start);
                }
                Some(b'g') if self.peek_next_byte() == Some(b'"') => {
                    self.offset += 1;
                    self.lex_string(StringLiteralKind::Glob, false, start);
                }
                Some(b'r') if self.peek_next_byte() == Some(b'"') => {
                    self.offset += 1;
                    self.lex_string(StringLiteralKind::Str, true, start);
                }
                Some(b'f')
                    if self.peek_next_byte() == Some(b'p')
                        && self.source.as_bytes().get(self.offset + 2) == Some(&b'"') =>
                {
                    self.lex_path_fmt_string()
                }
                Some(b'f') if self.peek_next_byte() == Some(b'"') => self.lex_fmt_string(),
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
                        _ => self.diagnostics.push(
                            Diagnostic::error("unexpected character")
                                .with_code("lex.unexpected-character")
                                .with_label(Label::primary(
                                    self.span(start, self.offset),
                                    "not valid in source",
                                )),
                        ),
                    }
                }
                None => break,
            }
        }

        self.push(TokenKind::Eof, self.offset, self.offset);
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
                        .with_code("lex.invalid-octal")
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "octal literals use digits 0 through 7",
                        )),
                );
            }
            if self.offset == digits_start {
                self.diagnostics.push(
                    Diagnostic::error("invalid octal integer literal")
                        .with_code("lex.invalid-octal")
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
                        .with_code("lex.invalid-float")
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
        if self.source.as_bytes().get(self.offset..self.offset + 2) == Some(b"ms") {
            self.offset += 2;
            self.push(TokenKind::Duration, start, self.offset);
            return;
        }
        if matches!(self.peek_byte(), Some(b's' | b'm' | b'h')) {
            self.offset += 1;
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
                        .with_code("lex.unterminated-fmt-string")
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "fmt string literal starts here",
                        )),
                );
            }
            None => {
                self.diagnostics.push(
                    Diagnostic::error("unterminated fmt string literal")
                        .with_code("lex.unterminated-fmt-string")
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
                        .with_code("lex.unterminated-fmt-string")
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
                        .with_code("lex.unterminated-path-fmt-string")
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
                        .with_code("lex.unterminated-path-fmt-string")
                        .with_label(Label::primary(
                            self.span(start, self.offset),
                            "path fmt string literal starts here",
                        )),
                );
            }
        }
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
                            "lex.unterminated-bytes"
                        } else {
                            "lex.unterminated-string"
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
                        .with_code("lex.unterminated-string")
                        .with_label(Label::primary(
                            self.span(literal_start.min(quote_start), self.offset),
                            "string literal starts here",
                        )),
                );
                return;
            }
        };
        let mut has_interpolation = false;
        let mut decoded = Vec::new();

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
                    StringLiteralKind::Bytes => unreachable!(),
                },
                Err(_) => self.diagnostics.push(
                    Diagnostic::error("string literal is not valid UTF-8")
                        .with_code("lex.invalid-string")
                        .with_label(Label::primary(
                            self.span(literal_start, self.offset),
                            "invalid string literal",
                        )),
                ),
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
            b'u' if bytes => self.diagnostics.push(
                Diagnostic::error("unicode escapes are not valid in bytes literals")
                    .with_code("lex.invalid-bytes-escape")
                    .with_label(Label::primary(
                        self.span(escape_start, self.offset),
                        "bytes literals use byte escapes only",
                    )),
            ),
            _ => self.invalid_escape(escape_start, self.offset),
        }
    }

    fn invalid_escape(&mut self, start: usize, end: usize) {
        self.diagnostics.push(
            Diagnostic::error("invalid escape sequence")
                .with_code("lex.invalid-escape")
                .with_label(Label::primary(
                    self.span(start, end.max(start + 1)),
                    "unsupported escape sequence",
                )),
        );
    }

    fn push(&mut self, kind: TokenKind, start: usize, _end: usize) {
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

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::{Keyword, Lexer, SourceId};
    use crate::symbol::Name;
    use crate::syntax::token::TokenTag;

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
        assert_eq!(table.name_at(1), Some(Name::intern("name")));
        assert_eq!(table.keyword_at(6), Some(Keyword::Run));
        assert_eq!(table.name_at(7), Some(Name::intern("make")));
        assert_eq!(
            table
                .string_flags_at(3)
                .map(|flags| (flags.has_interpolation, flags.raw_literal)),
            Some((false, false))
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
            output.diagnostics[0].code.as_deref(),
            Some("lex.invalid-bytes-escape")
        );
    }
}
