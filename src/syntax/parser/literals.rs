#![allow(clippy::single_call_fn)]

use super::{Diagnostic, EscapeIssueKind, InterpolationChunk, Label, Lexer, Parser, Span, literal};
use crate::syntax::arena::{ArenaProgramBuilder, ArenaRange, ExprId};
use std::sync::Arc;

impl<'a> Parser<'a> {
    pub(super) fn starts_bare_path_literal(&self) -> bool {
        literal::scan_bare_path_at(self.source, self.current_start()).is_some()
    }

    pub(super) fn fmt_string_parts_arena_only(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
        span: Span,
        raw_literal: bool,
    ) -> ArenaRange {
        let source_id = self.source_id;
        arena.begin_fmt_parts();
        let mut diagnostics = Vec::new();
        let mut any_part = false;
        let content_offset = self.string_content_offset(span);
        let raw = self.quoted_content(span);
        let chunks = match literal::interpolation_chunks(raw, content_offset) {
            Some(chunks) => chunks,
            None => {
                diagnostics.push(interpolation_diagnostic(
                    span,
                    "unterminated fmt string interpolation",
                    "interpolation starts in this string",
                ));
                let (text, decode_diagnostics) =
                    decode_interpolation_text_for(source_id, raw, span, content_offset);
                diagnostics.extend(decode_diagnostics);
                self.diagnostics.extend(diagnostics);
                arena.push_fmt_text_part_cooked(&Arc::from(text));
                return arena.finish_fmt_parts();
            }
        };
        for chunk in chunks {
            match chunk {
                InterpolationChunk::Text { source, offset } => {
                    any_part = true;
                    if raw_literal {
                        arena.push_fmt_text_part_cooked(&Arc::from(source));
                    } else {
                        let (text, decode_diagnostics) =
                            decode_interpolation_text_for(source_id, source, span, offset);
                        diagnostics.extend(decode_diagnostics);
                        arena.push_fmt_text_part_cooked(&Arc::from(text));
                    }
                }
                InterpolationChunk::Expr { source, offset } => {
                    let (expr_source, spec) = split_fmt_spec(source);
                    let (expr_id, parse_diagnostics) = parse_interpolation_expr_arena_only_for(
                        source_id,
                        expr_source,
                        offset,
                        arena,
                    );
                    diagnostics.extend(parse_diagnostics);
                    if let Some(expr_id) = expr_id {
                        any_part = true;
                        arena.push_fmt_expr_part(expr_id, spec);
                    }
                }
            }
        }
        self.diagnostics.extend(diagnostics);
        if !any_part {
            arena.push_fmt_text_part_cooked(&Arc::from(""));
        }
        arena.finish_fmt_parts()
    }

    pub(super) fn quoted_content(&self, span: Span) -> &str {
        match literal::scan_quoted_literal(self.source, span.start(), true) {
            Some(literal::QuotedScan::Terminated(literal)) => {
                &self.source[literal.content_start..literal.content_end]
            }
            _ => "",
        }
    }

    pub(super) fn decoded_quoted_text(&mut self, span: Span, raw_literal: bool) -> Arc<str> {
        let source_id = self.source_id;
        let (text, diagnostics) = {
            let raw = self.quoted_content(span);
            if raw_literal {
                (Arc::from(raw), Vec::new())
            } else {
                let (text, diagnostics) = decode_interpolation_text_for(
                    source_id,
                    raw,
                    span,
                    self.string_content_offset(span),
                );
                (Arc::from(text), diagnostics)
            }
        };
        self.diagnostics.extend(diagnostics);
        text
    }

    pub(super) fn string_content_offset(&self, span: Span) -> usize {
        match literal::scan_quoted_literal(self.source, span.start(), true) {
            Some(literal::QuotedScan::Terminated(literal)) => literal.content_start,
            _ => {
                let literal = &self.source[span.start()..span.end()];
                let quote = literal.find('"').unwrap_or(0);
                span.start()
                    + quote
                    + if literal[quote..].starts_with("\"\"\"") {
                        3
                    } else {
                        1
                    }
            }
        }
    }
}

pub(in crate::syntax::parser) fn interpolation_diagnostic(
    span: Span,
    message: &str,
    label: &str,
) -> Diagnostic {
    Diagnostic::error(message)
        .with_code("parse.unterminated-interpolation")
        .with_label(Label::primary(span, label))
}

pub(in crate::syntax::parser) fn decode_interpolation_text_for(
    source_id: crate::source::SourceId,
    raw: &str,
    span: Span,
    offset: usize,
) -> (String, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let decoded = literal::decode_string_text(raw, offset, true);
    for issue in decoded.issues {
        let message = match issue.kind {
            EscapeIssueKind::Invalid => "invalid escape sequence",
            EscapeIssueKind::BytesUnicode => "unicode escapes are not valid in bytes literals",
        };
        let label = match issue.kind {
            EscapeIssueKind::Invalid => "unsupported string escape",
            EscapeIssueKind::BytesUnicode => "bytes literals use byte escapes only",
        };
        diagnostics.push(
            Diagnostic::error(message)
                .with_code("parse.invalid-string-escape")
                .with_label(Label::primary(
                    Span::new(source_id, issue.start, issue.end.max(issue.start + 1)),
                    label,
                )),
        );
    }
    let value = match String::from_utf8(decoded.bytes) {
        Ok(value) => value,
        Err(err) => {
            diagnostics.push(
                Diagnostic::error("string literal is not valid UTF-8")
                    .with_code("parse.invalid-string")
                    .with_label(Label::primary(span, "invalid string literal")),
            );
            String::from_utf8_lossy(err.as_bytes()).into_owned()
        }
    };
    (value, diagnostics)
}

pub(in crate::syntax::parser) fn decode_bytes_literal_for(
    source_id: crate::source::SourceId,
    raw: &str,
    offset: usize,
) -> (Arc<[u8]>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let decoded = literal::decode_string_text(raw, offset, false);
    for issue in decoded.issues {
        let message = match issue.kind {
            EscapeIssueKind::Invalid => "invalid escape sequence",
            EscapeIssueKind::BytesUnicode => "unicode escapes are not valid in bytes literals",
        };
        let label = match issue.kind {
            EscapeIssueKind::Invalid => "unsupported string escape",
            EscapeIssueKind::BytesUnicode => "bytes literals use byte escapes only",
        };
        diagnostics.push(
            Diagnostic::error(message)
                .with_code("parse.invalid-string-escape")
                .with_label(Label::primary(
                    Span::new(source_id, issue.start, issue.end.max(issue.start + 1)),
                    label,
                )),
        );
    }
    (Arc::from(decoded.bytes), diagnostics)
}

pub(in crate::syntax::parser) fn parse_interpolation_expr_arena_only_for(
    source_id: crate::source::SourceId,
    source: &str,
    offset: usize,
    arena: &mut ArenaProgramBuilder<'_>,
) -> (Option<ExprId>, Vec<Diagnostic>) {
    let lexed = Lexer::new(source_id, source).lex_compact();
    let mut parser = Parser::new_with_token_table(source_id, source, lexed.token_table);
    parser.diagnostics.extend(lexed.diagnostics);
    let marks = arena.span_marks();
    let expr_id = parser.parse_expr_id_arena_only(arena);
    if expr_id.is_some() {
        arena.shift_spans_since(marks, offset);
    }
    (expr_id, parser.diagnostics)
}

pub(in crate::syntax::parser) fn dollar_shorthand_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut end = scan_ident_end(bytes, start);
    while bytes.get(end) == Some(&b'.')
        && bytes.get(end + 1).is_some_and(|byte| is_ident_start(*byte))
    {
        end = scan_ident_end(bytes, end + 1);
    }
    end
}

/// Split `source` into the expression part and an optional trailing format spec `:<>N` or `:0N`.
///
/// Examples:
///   `"count:>4"`  → `("count", Some(RightAlign { width: 4 }))`
///   `"count:<10"` → `("count", Some(LeftAlign  { width: 10 }))`
///   `"count:04"`  → `("count", Some(ZeroPad    { width: 4 }))`
///   `"count"`     → `("count", None)`
pub(in crate::syntax::parser) fn split_fmt_spec(
    source: &str,
) -> (&str, Option<crate::syntax::node::FormatSpec>) {
    use crate::syntax::node::{FormatSpec, FormatSpecKind};
    // Walk right-to-left: find the last `:` that is immediately followed by
    // a valid spec character (`>`, `<`, or a digit for zero-pad).
    let bytes = source.as_bytes();
    // Find rightmost `:` that starts a valid spec.
    let mut colon = None;
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b':' {
            let rest = &source[i + 1..];
            // Must start with `>`, `<`, or `0` and be followed only by digits.
            let spec_start = rest.as_bytes().first().copied();
            if matches!(spec_start, Some(b'>' | b'<' | b'0'..=b'9')) {
                let digits = if matches!(spec_start, Some(b'>' | b'<')) {
                    &rest[1..]
                } else {
                    rest
                };
                if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                    colon = Some(i);
                    break;
                }
            }
        }
    }
    let Some(colon) = colon else {
        return (source, None);
    };
    let expr_part = &source[..colon];
    let spec_str = &source[colon + 1..];
    let (kind, width_str) = if let Some(stripped) = spec_str.strip_prefix('>') {
        (FormatSpecKind::RightAlign, stripped)
    } else if let Some(stripped) = spec_str.strip_prefix('<') {
        (FormatSpecKind::LeftAlign, stripped)
    } else {
        (FormatSpecKind::ZeroPad, spec_str)
    };
    let Ok(width) = width_str.parse::<usize>() else {
        return (source, None);
    };
    if width == 0 {
        return (source, None);
    }
    (expr_part, Some(FormatSpec { kind, width }))
}

pub(in crate::syntax::parser) fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(in crate::syntax::parser) fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

pub(in crate::syntax::parser) fn scan_ident_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while bytes.get(end).is_some_and(|byte| is_ident_continue(*byte)) {
        end += 1;
    }
    end
}
