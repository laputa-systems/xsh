#![allow(clippy::single_call_fn)]

use super::{Diagnostic, EscapeIssueKind, InterpolationChunk, Label, Lexer, Parser, Span, literal};
use crate::diagnostic::FixHint;
use crate::syntax::literal::FmtIssueKind;
use crate::syntax::node::{FormatSpec, FormatSpecKind};
use crate::syntax::token::TokenTag;
use crate::syntax::arena::{ArenaProgramBuilder, ArenaRange, ExprId};
use std::sync::Arc;

impl<'a> Parser<'a> {
    pub(super) fn reject_path_string_interpolation(&mut self, span: Span) {
        let raw = self.quoted_content(span);
        let has_interpolation =
            literal::interpolation_chunks(raw, self.string_content_offset(span)).is_some_and(
                |chunks| {
                    chunks
                        .iter()
                        .any(|chunk| matches!(chunk, InterpolationChunk::Expr { .. }))
                },
            );
        if has_interpolation {
            self.diagnostics.push(
                Diagnostic::error("p-strings do not interpolate")
                    .with_code("parse.path-string-interpolation")
                    .with_label(Label::primary(
                        span,
                        "use `fp\"...\"` for an interpolated path",
                    ))
                    .with_note(
                        "`fp\"...{expr}...\"` interpolates; write `\\${` for a literal `${` in a p-string",
                    ),
            );
        }
    }

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
        // Adjacent text chunks (split by `{{`/`}}` escapes and block layout)
        // form one text part.
        let mut text: Option<String> = None;
        let (chunks, chunk_diagnostics) = self.quoted_text_chunks(span, true);
        diagnostics.extend(chunk_diagnostics);
        for chunk in chunks {
            match chunk {
                InterpolationChunk::Text { source, offset } => {
                    let pending = text.get_or_insert_with(String::new);
                    if raw_literal {
                        pending.push_str(source);
                    } else {
                        let (decoded, decode_diagnostics) =
                            decode_interpolation_text_for(source_id, source, span, offset);
                        diagnostics.extend(decode_diagnostics);
                        pending.push_str(&decoded);
                    }
                }
                InterpolationChunk::Expr { source, offset } => {
                    let (expr_id, spec, parse_diagnostics) =
                        parse_fmt_interpolation_for(source_id, source, offset, arena);
                    diagnostics.extend(parse_diagnostics);
                    if let Some(expr_id) = expr_id {
                        if let Some(pending) = text.take() {
                            arena.push_fmt_text_part_cooked(&Arc::from(pending));
                        }
                        any_part = true;
                        arena.push_fmt_expr_part(expr_id, spec);
                    }
                }
            }
        }
        if let Some(pending) = text {
            any_part = true;
            arena.push_fmt_text_part_cooked(&Arc::from(pending));
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
        let (chunks, mut diagnostics) = self.quoted_text_chunks(span, false);
        let mut value = String::new();
        for chunk in chunks {
            if let InterpolationChunk::Text { source, offset } = chunk {
                if raw_literal { value.push_str(source); }
                else {
                    let (text, decode_diagnostics) = decode_interpolation_text_for(self.source_id, source, span, offset);
                    diagnostics.extend(decode_diagnostics);
                    value.push_str(&text);
                }
            }
        }
        self.diagnostics.extend(diagnostics);
        Arc::from(value)
    }

    pub(super) fn quoted_text_chunks(&self, span: Span, interpolates: bool) -> (Vec<InterpolationChunk<'a>>, Vec<Diagnostic>) {
        let Some(literal::QuotedScan::Terminated(quoted)) = literal::scan_quoted_literal(self.source, span.start(), true) else {
            return (Vec::new(), Vec::new());
        };
        let raw = &self.source[quoted.content_start..quoted.content_end];
        let mut diagnostics = Vec::new();
        let chunks = if !interpolates || quoted.raw {
            vec![InterpolationChunk::Text { source: raw, offset: quoted.content_start }]
        } else if matches!(quoted.kind, literal::QuotedLiteralKind::Fmt | literal::QuotedLiteralKind::PathFmt) {
            let (chunks, issues) = literal::fmt_chunks(self.source, quoted);
            diagnostics.extend(issues.into_iter().map(|issue| self.fmt_issue_diagnostic(issue, quoted)));
            chunks
        } else {
            // A quoted command word interpolates `${expr}` and `$name`.
            literal::interpolation_chunks(raw, quoted.content_start).unwrap_or_else(|| {
                diagnostics.push(Diagnostic::error("unterminated string interpolation")
                    .with_code("parse.unterminated-interpolation")
                    .with_label(Label::primary(span, "interpolation starts in this string")));
                vec![InterpolationChunk::Text { source: raw, offset: quoted.content_start }]
            })
        };
        let (chunks, issues) = literal::block_string_chunks(self.source, quoted, chunks);
        for issue in issues {
            diagnostics.push(Diagnostic::error("block string line does not start with the closing delimiter's exact indentation")
                .with_code("parse.block-string-margin")
                .with_label(Label::primary(Span::new(self.source_id, issue.start, issue.end), "required space/tab prefix is missing")));
        }
        (chunks, diagnostics)
    }

    fn fmt_issue_diagnostic(&self, issue: literal::FmtIssue, quoted: literal::QuotedLiteral) -> Diagnostic {
        let span = Span::new(self.source_id, issue.start, issue.end);
        match issue.kind {
            FmtIssueKind::LoneCloseBrace => {
                let diagnostic = Diagnostic::error("unmatched `}` in f-string")
                    .with_code("parse.fmt-lone-brace")
                    .with_label(Label::primary(span, "a literal brace is written `}}`"))
                    .with_fix_hint(FixHint::replacement(span, "write `}}`", "}}"));
                // `{{` is always an escape, so `f"{{a: 1}.a}"` reaches here.
                if self.source[quoted.content_start..issue.start].contains("{{") {
                    diagnostic.with_note("an interpolation that starts with `{` needs a space: `{ {a: 1}.a }`")
                } else {
                    diagnostic
                }
            }
            FmtIssueKind::Unclosed => Diagnostic::error("unclosed `{` in f-string")
                .with_code("parse.unterminated-interpolation")
                .with_label(Label::primary(span, "this `{` has no matching `}`"))
                .with_note("a literal brace is written `{{`"),
            FmtIssueKind::Comment => Diagnostic::error("comments are not allowed inside an f-string interpolation")
                .with_code("parse.fmt-interpolation-comment")
                .with_label(Label::primary(span, "comment inside `{...}`")),
            FmtIssueKind::LineBreak => Diagnostic::error("line break inside an interpolation of a single-line f-string")
                .with_code("parse.fmt-interpolation-line-break")
                .with_label(Label::primary(span, "the interpolation continues on the next line"))
                .with_note("bind the value first, or use a block `f\"\"\"...\"\"\"` string"),
            FmtIssueKind::Empty => Diagnostic::error("empty interpolation in f-string")
                .with_code("parse.fmt-empty-interpolation")
                .with_label(Label::primary(span, "expected an expression inside `{}`"))
                .with_note("literal braces are written `{{}}`"),
            FmtIssueKind::DollarBrace => Diagnostic::error("f-strings interpolate with `{expr}`, not `${expr}`")
                .with_code("parse.fmt-dollar-interpolation")
                .with_label(Label::primary(span, "`${` is command-word interpolation"))
                .with_fix_hint(FixHint::replacement(span, "write `{`", "{"))
                .with_note("a literal `$` before a brace is written `${{`"),
        }
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
    let symbols = arena.symbol_owner().clone();
    symbols.with_current(|| {
        let lexed = Lexer::new_with_symbols(source_id, source, symbols.clone()).lex_compact();
        let mut parser = Parser::new_with_token_table(source_id, source, lexed.token_table);
        parser.diagnostics.extend(lexed.diagnostics);
        let marks = arena.span_marks();
        let expr_id = parser.parse_expr_id_arena_only(arena);
        arena.shift_spans_since(marks, offset);
        let shift = |span: Span| Span::new(source_id, span.start() + offset, span.end() + offset);
        for diagnostic in &mut parser.diagnostics {
            diagnostic.span = diagnostic.span.map(shift);
            for label in &mut diagnostic.labels { label.span = shift(label.span); }
            for hint in &mut diagnostic.fix_hints { hint.span = hint.span.map(shift); }
        }
        (expr_id, parser.diagnostics)
    })
}

/// Parses one f-string interpolation body: an expression, then optionally a
/// `:` width spec. XSH expressions never contain a bare top-level `:`, so
/// the spec starts exactly where the expression parser stops.
pub(in crate::syntax::parser) fn parse_fmt_interpolation_for(
    source_id: crate::source::SourceId,
    source: &str,
    offset: usize,
    arena: &mut ArenaProgramBuilder<'_>,
) -> (Option<ExprId>, Option<FormatSpec>, Vec<Diagnostic>) {
    let symbols = arena.symbol_owner().clone();
    symbols.with_current(|| {
        let lexed = Lexer::new_with_symbols(source_id, source, symbols.clone()).lex_compact();
        let mut parser = Parser::new_with_token_table(source_id, source, lexed.token_table);
        let marks = arena.span_marks();
        parser.skip_newlines();
        let expr_id = parser.parse_expr_id_arena_only(arena);
        parser.skip_newlines();
        let mut spec = None;
        let mut expression_end = source.len();
        if expr_id.is_some() && parser.diagnostics.is_empty() {
            match parser.current_tag() {
                TokenTag::Eof => {}
                TokenTag::Colon => {
                    expression_end = parser.current_start();
                    let text = source[expression_end + 1..].trim_end();
                    spec = parse_format_spec(text);
                    if spec.is_none() {
                        parser.diagnostics.push(
                            Diagnostic::error("invalid f-string format spec")
                                .with_code("parse.fmt-spec")
                                .with_label(Label::primary(
                                    Span::new(source_id, expression_end, expression_end + 1 + text.len()),
                                    "expected `:>N`, `:<N`, or `:0N` with a width of at least 1",
                                )),
                        );
                    }
                }
                _ => {
                    let span = parser.current_span();
                    parser.diagnostics.push(
                        Diagnostic::error("unexpected token in f-string interpolation")
                            .with_code("parse.fmt-interpolation-trailing")
                            .with_label(Label::primary(span, "expected `}` or a `:` width spec")),
                    );
                }
            }
        }
        let mut diagnostics: Vec<Diagnostic> = lexed
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.span.is_none_or(|span| span.start() < expression_end))
            .collect();
        diagnostics.append(&mut parser.diagnostics);
        arena.shift_spans_since(marks, offset);
        let shift = |span: Span| Span::new(source_id, span.start() + offset, span.end() + offset);
        for diagnostic in &mut diagnostics {
            diagnostic.span = diagnostic.span.map(shift);
            for label in &mut diagnostic.labels { label.span = shift(label.span); }
            for hint in &mut diagnostic.fix_hints { hint.span = hint.span.map(shift); }
        }
        (expr_id, spec, diagnostics)
    })
}

/// Parses `>N`, `<N`, or `0N` with `N >= 1`.
fn parse_format_spec(text: &str) -> Option<FormatSpec> {
    let (kind, digits) = if let Some(digits) = text.strip_prefix('>') {
        (FormatSpecKind::RightAlign, digits)
    } else if let Some(digits) = text.strip_prefix('<') {
        (FormatSpecKind::LeftAlign, digits)
    } else {
        (FormatSpecKind::ZeroPad, text.strip_prefix('0')?)
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let width = digits.parse::<usize>().ok().filter(|width| *width > 0)?;
    Some(FormatSpec { kind, width })
}
