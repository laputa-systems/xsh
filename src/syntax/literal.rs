#![allow(clippy::single_call_fn)]

use crate::syntax::grammar;
use crate::syntax::lexer::{InterpolationEnd, interpolation_end};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuotedLiteralKind {
    Str,
    Bytes,
    Regex,
    Path,
    Glob,
    Env,
    Fmt,
    PathFmt,
}

/// Whether `name` can be written inside `e"..."`: an ASCII identifier, the
/// rule environment overlays apply to the names they set.
pub fn is_env_string_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct QuotedLiteral {
    pub kind: QuotedLiteralKind,
    pub raw: bool,
    pub delimiter_len: usize,
    pub content_start: usize,
    pub content_end: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuotedScan {
    Terminated(QuotedLiteral),
    Unterminated { end: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InterpolationChunk<'a> {
    Text { source: &'a str, offset: usize },
    Expr { source: &'a str, offset: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EscapeIssueKind {
    Invalid,
    BytesUnicode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EscapeIssue {
    pub start: usize,
    pub end: usize,
    pub kind: EscapeIssueKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DecodedText {
    pub bytes: Vec<u8>,
    pub issues: Vec<EscapeIssue>,
}

#[derive(Clone, Copy)]
struct QuotePrefix {
    len: usize,
    raw: bool,
    kind: QuotedLiteralKind,
}

pub(crate) fn scan_quoted_literal(
    source: &str,
    start: usize,
    stop_on_newline: bool,
) -> Option<QuotedScan> {
    let bytes = source.as_bytes();
    let prefix = quote_prefix_at(bytes, start)?;
    let quote = start + prefix.len;
    let delimiter_len = if bytes.get(quote..quote + 3) == Some(b"\"\"\"") {
        3
    } else {
        1
    };
    let content_start = quote + delimiter_len;
    let mut offset = content_start;
    while offset < bytes.len() {
        if delimiter_len == 3 && bytes.get(offset..offset + 3) == Some(b"\"\"\"") {
            return Some(QuotedScan::Terminated(QuotedLiteral {
                kind: prefix.kind,
                raw: prefix.raw,
                delimiter_len,
                content_start,
                content_end: offset,
                end: offset + 3,
            }));
        }
        if delimiter_len == 1 && bytes[offset] == b'"' {
            return Some(QuotedScan::Terminated(QuotedLiteral {
                kind: prefix.kind,
                raw: prefix.raw,
                delimiter_len,
                content_start,
                content_end: offset,
                end: offset + 1,
            }));
        }
        if fmt_interpolates(prefix.kind, prefix.raw) && matches!(bytes[offset], b'{' | b'}') {
            offset += if bytes.get(offset + 1) == Some(&bytes[offset]) {
                2
            } else if bytes[offset] == b'{'
                && let InterpolationEnd::Close(close) = interpolation_end(source, offset + 1)
            {
                close + 1 - offset
            } else {
                1
            };
            continue;
        }
        if command_string_interpolates(prefix.kind, prefix.raw)
            && bytes[offset] == b'$'
            && bytes.get(offset + 1) == Some(&b'{')
        {
            if let Some(close) = interpolation_close(source, offset + 2) {
                offset = close + 1;
            } else {
                offset += 2;
            }
            continue;
        }
        if stop_on_newline && delimiter_len == 1 && matches!(bytes[offset], b'\n' | b'\r') {
            return Some(QuotedScan::Unterminated { end: offset });
        }
        if bytes[offset] == b'\\' && !prefix.raw {
            offset = escape_end(bytes, offset);
        } else {
            offset += 1;
        }
    }
    Some(QuotedScan::Unterminated { end: offset })
}

// Skips a quoted literal in an expression context without recursing into its interpolations.
// Used by interpolation_close so that a } inside a nested display string doesn't confuse
// the outer ${...} scanner, while still preventing same-quote nesting.
fn skip_string_in_expr(source: &str, start: usize) -> Option<usize> {
    match scan_quoted_literal(source, start, false)? {
        QuotedScan::Terminated(literal) => Some(literal.end),
        QuotedScan::Unterminated { .. } => None,
    }
}

pub(crate) fn interpolation_close(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = start;
    let mut depth = 0usize;
    while offset < bytes.len() {
        if quote_prefix_at(bytes, offset).is_some() {
            offset = skip_string_in_expr(source, offset)?;
            continue;
        }
        match bytes[offset] {
            b'#' => {
                while offset < bytes.len() && !matches!(bytes[offset], b'\r' | b'\n') {
                    offset += 1;
                }
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                offset += 1;
            }
            b')' | b']' => {
                depth = depth.saturating_sub(1);
                offset += 1;
            }
            b'}' if depth == 0 => return Some(offset),
            b'}' => {
                depth -= 1;
                offset += 1;
            }
            _ => offset += 1,
        }
    }
    None
}

pub(crate) fn interpolation_chunks(
    raw: &str,
    content_offset: usize,
) -> Option<Vec<InterpolationChunk<'_>>> {
    let mut chunks = Vec::new();
    let mut rest_start = 0;
    let mut search_start = 0;
    while search_start < raw.len() {
        let Some(relative) = raw[search_start..].find('$') else {
            break;
        };
        let dollar = search_start + relative;
        if is_escaped(raw.as_bytes(), dollar) {
            search_start = dollar + 1;
            continue;
        }
        let bytes = raw.as_bytes();
        let Some(next) = bytes.get(dollar + 1).copied() else {
            break;
        };
        let (expr_start, close) = if next == b'{' {
            let expr_start = dollar + 2;
            (expr_start, interpolation_close(raw, expr_start)?)
        } else if is_ident_start(next) {
            let close = shorthand_end(bytes, dollar + 1);
            (dollar + 1, close)
        } else {
            search_start = dollar + 1;
            continue;
        };
        if dollar > rest_start {
            chunks.push(InterpolationChunk::Text {
                source: &raw[rest_start..dollar],
                offset: content_offset + rest_start,
            });
        }
        chunks.push(InterpolationChunk::Expr {
            source: &raw[expr_start..close],
            offset: content_offset + expr_start,
        });
        rest_start = if next == b'{' { close + 1 } else { close };
        search_start = rest_start;
    }
    if rest_start < raw.len() {
        chunks.push(InterpolationChunk::Text {
            source: &raw[rest_start..],
            offset: content_offset + rest_start,
        });
    }
    Some(chunks)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FmtIssueKind {
    /// A `}` in text that is not part of a `}}` escape.
    LoneCloseBrace,
    /// A `{` whose interpolation never closes.
    Unclosed,
    /// A `#` comment inside `{...}`.
    Comment,
    /// A line break inside `{...}` of a single-line f-string.
    LineBreak,
    /// `{}` with nothing but whitespace inside.
    Empty,
    /// `${`, the command-word interpolation marker, in f-string text.
    DollarBrace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FmtIssue {
    pub kind: FmtIssueKind,
    pub start: usize,
    pub end: usize,
}

/// Splits the body of an `f"..."` or `fp"..."` literal into text and `{expr}`
/// chunks with source offsets. Text chunks are exact source slices: a `{{` or
/// `}}` escape ends one chunk after its first brace and starts the next after
/// its second, so escape decoding and diagnostic columns stay byte-exact.
/// An expression chunk is everything between `{` and its closing `}`,
/// including any `:spec`. After a comment or an unclosed `{`, the rest of the
/// literal is text so one mistake reports once.
pub(crate) fn fmt_chunks(
    source: &str,
    literal: QuotedLiteral,
) -> (Vec<InterpolationChunk<'_>>, Vec<FmtIssue>) {
    let bytes = source.as_bytes();
    let end = literal.content_end;
    let mut chunks = Vec::new();
    let mut issues = Vec::new();
    let mut text_start = literal.content_start;
    let mut offset = text_start;
    let push_text = |chunks: &mut Vec<_>, from: usize, to: usize| {
        if to > from {
            chunks.push(InterpolationChunk::Text {
                source: &source[from..to],
                offset: from,
            });
        }
    };
    while offset < end {
        match bytes[offset] {
            b'\\' => offset = escape_end(bytes, offset).min(end),
            brace @ (b'{' | b'}') if offset + 1 < end && bytes[offset + 1] == brace => {
                push_text(&mut chunks, text_start, offset + 1);
                offset += 2;
                text_start = offset;
            }
            b'}' => {
                issues.push(FmtIssue {
                    kind: FmtIssueKind::LoneCloseBrace,
                    start: offset,
                    end: offset + 1,
                });
                offset += 1;
            }
            b'$' if bytes.get(offset + 1) == Some(&b'{')
                && bytes.get(offset + 2) != Some(&b'{') =>
            {
                issues.push(FmtIssue {
                    kind: FmtIssueKind::DollarBrace,
                    start: offset,
                    end: offset + 2,
                });
                push_text(&mut chunks, text_start, offset);
                offset += 1;
                text_start = offset;
            }
            b'{' => {
                let open = offset;
                match interpolation_end(source, open + 1) {
                    InterpolationEnd::Close(close) if close < end => {
                        push_text(&mut chunks, text_start, open);
                        let expression = &source[open + 1..close];
                        if expression.trim().is_empty() {
                            issues.push(FmtIssue {
                                kind: FmtIssueKind::Empty,
                                start: open,
                                end: close + 1,
                            });
                        } else {
                            if literal.delimiter_len == 1
                                && let Some(at) = expression.find(['\n', '\r'])
                            {
                                issues.push(FmtIssue {
                                    kind: FmtIssueKind::LineBreak,
                                    start: open + 1 + at,
                                    end: open + 2 + at,
                                });
                            }
                            chunks.push(InterpolationChunk::Expr {
                                source: expression,
                                offset: open + 1,
                            });
                        }
                        offset = close + 1;
                        text_start = offset;
                    }
                    InterpolationEnd::Comment(at) if at < end => {
                        let line_end = source[at..end]
                            .find(['\n', '\r'])
                            .map_or(end, |relative| at + relative);
                        issues.push(FmtIssue {
                            kind: FmtIssueKind::Comment,
                            start: at,
                            end: line_end,
                        });
                        break;
                    }
                    _ => {
                        issues.push(FmtIssue {
                            kind: FmtIssueKind::Unclosed,
                            start: open,
                            end: open + 1,
                        });
                        break;
                    }
                }
            }
            _ => offset += 1,
        }
    }
    push_text(&mut chunks, text_start, end);
    (chunks, issues)
}

/// The `{name}` and `{name.field}` interpolations a plain `"..."` or
/// `p"..."` literal would have with an `f` prefix, as `(name, byte range in
/// literal_source)`. `None` unless the literal would then be a valid f-string
/// whose every interpolation is such a dotted name, so adding the prefix
/// never changes any other text.
pub fn dotted_names_if_formatted(
    literal_source: &str,
) -> Option<Vec<(&str, std::ops::Range<usize>)>> {
    let Some(QuotedScan::Terminated(literal)) = scan_quoted_literal(literal_source, 0, false)
    else {
        return None;
    };
    if literal.raw
        || literal.end != literal_source.len()
        || !matches!(
            literal.kind,
            QuotedLiteralKind::Str | QuotedLiteralKind::Path
        )
    {
        return None;
    }
    let (chunks, issues) = fmt_chunks(literal_source, literal);
    if !issues.is_empty() {
        return None;
    }
    let mut names = Vec::new();
    for chunk in chunks {
        let InterpolationChunk::Expr { source, offset } = chunk else {
            continue;
        };
        let dotted = source.split('.').all(|part| {
            part.bytes().next().is_some_and(is_ident_start) && part.bytes().all(is_ident_continue)
        });
        if !dotted {
            return None;
        }
        let name = source.split('.').next().unwrap_or(source);
        names.push((name, offset..offset + source.len()));
    }
    (!names.is_empty()).then_some(names)
}

/// The offset just past the escape starting with the backslash at `offset`.
/// A `\u{HEX}` escape is one unit, so its braces never read as f-string
/// interpolation.
fn escape_end(bytes: &[u8], offset: usize) -> usize {
    if bytes.get(offset + 1) == Some(&b'u') && bytes.get(offset + 2) == Some(&b'{') {
        let digits = bytes[offset + 3..]
            .iter()
            .take_while(|byte| byte.is_ascii_hexdigit())
            .count();
        let close = offset + 3 + digits;
        return if bytes.get(close) == Some(&b'}') {
            close + 1
        } else {
            close
        };
    }
    (offset + 2).min(bytes.len())
}

/// Byte ranges of each unescaped `$name` in the text of an f-string body,
/// covering the `$` and the identifier.
pub(crate) fn fmt_text_dollar_names(
    source: &str,
    literal: QuotedLiteral,
) -> Vec<std::ops::Range<usize>> {
    let mut names = Vec::new();
    for chunk in fmt_chunks(source, literal).0 {
        let InterpolationChunk::Text {
            source: text,
            offset,
        } = chunk
        else {
            continue;
        };
        let bytes = text.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index += 2,
                b'$' if bytes
                    .get(index + 1)
                    .is_some_and(|byte| is_ident_start(*byte)) =>
                {
                    let mut name_end = index + 2;
                    while bytes
                        .get(name_end)
                        .is_some_and(|byte| is_ident_continue(*byte))
                    {
                        name_end += 1;
                    }
                    names.push(offset + index..offset + name_end);
                    index = name_end;
                }
                _ => index += 1,
            }
        }
    }
    names
}

fn shorthand_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while bytes.get(end).is_some_and(|byte| is_ident_continue(*byte)) {
        end += 1;
    }
    while bytes.get(end) == Some(&b'.')
        && bytes.get(end + 1).is_some_and(|byte| is_ident_start(*byte))
    {
        end += 1;
        while bytes.get(end).is_some_and(|byte| is_ident_continue(*byte)) {
            end += 1;
        }
    }
    end
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

pub(crate) fn decode_string_text(
    raw: &str,
    base_offset: usize,
    allow_unicode: bool,
) -> DecodedText {
    let mut output = Vec::new();
    let mut issues = Vec::new();
    let mut offset = 0usize;
    while offset < raw.len() {
        let ch = raw[offset..]
            .chars()
            .next()
            .expect("offset is inside a UTF-8 string");
        if ch != '\\' {
            push_utf8(ch, &mut output);
            offset += ch.len_utf8();
            continue;
        }

        let escape_start = offset;
        offset += 1;
        let Some(escaped) = raw[offset..].chars().next() else {
            issues.push(EscapeIssue {
                start: base_offset + escape_start,
                end: base_offset + raw.len(),
                kind: EscapeIssueKind::Invalid,
            });
            output.push(b'\\');
            break;
        };
        offset += escaped.len_utf8();
        match escaped {
            '\\' => output.push(b'\\'),
            '"' => output.push(b'"'),
            '$' => output.push(b'$'),
            'n' => output.push(b'\n'),
            'r' => output.push(b'\r'),
            't' => output.push(b'\t'),
            '0' => output.push(b'\0'),
            'x' => {
                let hex_start = offset;
                let hex_end = (offset + 2).min(raw.len());
                if offset + 2 <= raw.len() {
                    let hex = &raw[offset..offset + 2];
                    if hex.as_bytes().iter().all(u8::is_ascii_hexdigit)
                        && let Ok(value) = u8::from_str_radix(hex, 16)
                    {
                        output.push(value);
                        offset += 2;
                        continue;
                    }
                }
                issues.push(EscapeIssue {
                    start: base_offset + escape_start,
                    end: base_offset + hex_end.max(hex_start),
                    kind: EscapeIssueKind::Invalid,
                });
                offset = hex_end;
            }
            'u' if allow_unicode && raw.as_bytes().get(offset) == Some(&b'{') => {
                offset += 1;
                let digits_start = offset;
                while offset < raw.len() && raw.as_bytes()[offset] != b'}' {
                    let ch = raw[offset..]
                        .chars()
                        .next()
                        .expect("offset is inside a UTF-8 string");
                    offset += ch.len_utf8();
                }
                let terminated = raw.as_bytes().get(offset) == Some(&b'}');
                let digits = &raw[digits_start..offset];
                if terminated {
                    offset += 1;
                }
                if !digits.is_empty()
                    && terminated
                    && digits.as_bytes().iter().all(u8::is_ascii_hexdigit)
                    && let Ok(value) = u32::from_str_radix(digits, 16)
                    && let Some(ch) = char::from_u32(value)
                {
                    push_utf8(ch, &mut output);
                } else {
                    issues.push(EscapeIssue {
                        start: base_offset + escape_start,
                        end: base_offset + offset,
                        kind: EscapeIssueKind::Invalid,
                    });
                }
            }
            'u' if allow_unicode => {
                issues.push(EscapeIssue {
                    start: base_offset + escape_start,
                    end: base_offset + offset,
                    kind: EscapeIssueKind::Invalid,
                });
            }
            'u' => {
                issues.push(EscapeIssue {
                    start: base_offset + escape_start,
                    end: base_offset + offset,
                    kind: EscapeIssueKind::BytesUnicode,
                });
            }
            _ => {
                issues.push(EscapeIssue {
                    start: base_offset + escape_start,
                    end: base_offset + offset,
                    kind: EscapeIssueKind::Invalid,
                });
                push_utf8(escaped, &mut output);
            }
        }
    }
    DecodedText {
        bytes: output,
        issues,
    }
}

pub(crate) fn scan_bare_path_at(source: &str, start: usize) -> Option<usize> {
    let rest = source.get(start..)?;
    if !(rest.starts_with('/') || rest.starts_with("./") || rest.starts_with("../")) {
        return None;
    }
    let mut end = start;
    for (offset, ch) in rest.char_indices() {
        if !is_bare_path_literal_char(ch) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    (end > start).then_some(end)
}

pub fn can_be_bare_path_literal(value: &str) -> bool {
    scan_bare_path_at(value, 0).is_some_and(|end| end == value.len())
}

pub(crate) fn is_escaped(bytes: &[u8], offset: usize) -> bool {
    let mut index = offset;
    let mut count = 0usize;
    while index > 0 && bytes[index - 1] == b'\\' {
        count += 1;
        index -= 1;
    }
    count % 2 == 1
}

fn quote_prefix_at(bytes: &[u8], start: usize) -> Option<QuotePrefix> {
    grammar::quoted_literal_at(bytes, start).map(|form| QuotePrefix {
        len: form.prefix.len(),
        raw: form.raw,
        kind: form.kind,
    })
}

// A quoted command word interpolates `${expr}` and `$name`.
fn command_string_interpolates(kind: QuotedLiteralKind, raw: bool) -> bool {
    !raw && kind == QuotedLiteralKind::Str
}

fn fmt_interpolates(kind: QuotedLiteralKind, raw: bool) -> bool {
    !raw && matches!(kind, QuotedLiteralKind::Fmt | QuotedLiteralKind::PathFmt)
}

fn push_utf8(ch: char, output: &mut Vec<u8>) {
    let mut buffer = [0; 4];
    output.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
}

pub(crate) fn is_bare_path_literal_char(ch: char) -> bool {
    !ch.is_whitespace()
        && !matches!(
            ch,
            '"' | '\''
                | '\\'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
                | ';'
                | ','
                | '?'
                | '$'
                | '|'
                | '&'
                | '<'
                | '>'
                | '#'
        )
}

/// Removes only source layout from text slices, retaining original expression
/// slices and offsets. Interpolated values and code never participate in margins.
pub(crate) fn block_string_chunks<'a>(
    source: &'a str,
    literal: QuotedLiteral,
    chunks: Vec<InterpolationChunk<'a>>,
) -> (Vec<InterpolationChunk<'a>>, Vec<std::ops::Range<usize>>) {
    if literal.delimiter_len != 3
        || !matches!(
            literal.kind,
            QuotedLiteralKind::Str | QuotedLiteralKind::Fmt
        )
    {
        return (chunks, Vec::new());
    }
    let bytes = source.as_bytes();
    let opening = line_break_len(bytes, literal.content_start);
    if opening == 0 {
        return (chunks, Vec::new());
    }
    let closing_line = source[..literal.content_end]
        .rfind(['\r', '\n'])
        .map_or(0, |offset| offset + 1);
    let margin = &bytes[closing_line..literal.content_end];
    let suffix = source[literal.end..]
        .split(['\r', '\n'])
        .next()
        .unwrap_or("");
    if !margin.iter().all(|byte| matches!(byte, b' ' | b'\t'))
        || !suffix.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
    {
        return (chunks, Vec::new());
    }
    let start = literal.content_start + opening;
    let closing_break = if closing_line > 1 && bytes[closing_line - 2..closing_line] == *b"\r\n" {
        closing_line - 2
    } else {
        closing_line.saturating_sub(1)
    };
    let end = closing_break.max(start);
    let expressions: Vec<_> = chunks
        .iter()
        .filter_map(|chunk| match chunk {
            InterpolationChunk::Expr {
                source: expression,
                offset,
            } => {
                // An f-string expression sits between `{` and `}`; a quoted
                // command word writes `${expr}` or `$name`.
                let (open, close) = if literal.kind == QuotedLiteralKind::Fmt {
                    (1, 1)
                } else if source[..*offset].ends_with("${") {
                    (2, 1)
                } else {
                    (1, 0)
                };
                Some(offset - open..offset + expression.len() + close)
            }
            _ => None,
        })
        .collect();
    let mut removed = Vec::new();
    let mut issues = Vec::new();
    let mut position = start;
    let mut line_start = true;
    let mut expression_index = 0;
    while position < end {
        while expressions
            .get(expression_index)
            .is_some_and(|range| range.end <= position)
        {
            expression_index += 1;
        }
        if line_start {
            let line_end = source[position..end]
                .find(['\r', '\n'])
                .map_or(end, |relative| position + relative);
            let blank = source[position..line_end].chars().all(char::is_whitespace);
            let matching = margin
                .iter()
                .zip(&bytes[position..line_end])
                .take_while(|(expected, actual)| expected == actual)
                .count();
            if !blank && matching != margin.len() {
                issues.push(position..(position + margin.len().max(1)).min(line_end));
            } else if matching > 0 {
                removed.push(position..position + matching);
                position += matching;
            }
            line_start = false;
            if position >= end {
                break;
            }
        }
        if let Some(expression) = expressions.get(expression_index)
            && expression.start == position
        {
            position = expression.end;
            expression_index += 1;
            continue;
        }
        let newline = line_break_len(bytes, position);
        if newline > 0 {
            position += newline;
            line_start = true;
        } else {
            position += 1;
        }
    }
    let mut result = Vec::new();
    for chunk in chunks {
        match chunk {
            InterpolationChunk::Expr { .. } => result.push(chunk),
            InterpolationChunk::Text {
                source: text,
                offset,
            } => {
                let mut cursor = offset.max(start);
                let limit = (offset + text.len()).min(end);
                for range in &removed {
                    if range.start >= limit || range.end <= cursor {
                        continue;
                    }
                    if cursor < range.start {
                        result.push(InterpolationChunk::Text {
                            source: &source[cursor..range.start],
                            offset: cursor,
                        });
                    }
                    cursor = cursor.max(range.end);
                }
                if cursor < limit {
                    result.push(InterpolationChunk::Text {
                        source: &source[cursor..limit],
                        offset: cursor,
                    });
                }
            }
        }
    }
    (result, issues)
}

fn line_break_len(bytes: &[u8], offset: usize) -> usize {
    match bytes.get(offset) {
        Some(b'\r') if bytes.get(offset + 1) == Some(&b'\n') => 2,
        Some(b'\r' | b'\n') => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod block_string_tests {
    use super::*;

    fn chunks(source: &str) -> (Vec<InterpolationChunk<'_>>, Vec<std::ops::Range<usize>>) {
        let Some(QuotedScan::Terminated(quoted)) = scan_quoted_literal(source, 0, false) else {
            panic!("quoted literal");
        };
        let raw = &source[quoted.content_start..quoted.content_end];
        let chunks = if quoted.kind == QuotedLiteralKind::Fmt {
            let (chunks, issues) = fmt_chunks(source, quoted);
            assert!(issues.is_empty(), "{issues:?}");
            chunks
        } else {
            vec![InterpolationChunk::Text {
                source: raw,
                offset: quoted.content_start,
            }]
        };
        block_string_chunks(source, quoted, chunks)
    }

    fn text(source: &str) -> String {
        let (chunks, issues) = chunks(source);
        assert!(issues.is_empty(), "{issues:?}");
        chunks
            .into_iter()
            .filter_map(|chunk| match chunk {
                InterpolationChunk::Text { source, .. } => Some(source),
                _ => None,
            })
            .collect()
    }

    // Physical newline bytes and whitespace prefixes cannot be expressed by
    // a source file with one fixed checkout line-ending convention.
    #[test]
    fn block_string_layout_preserves_crlf_tabs_blank_prefixes_and_shared_breaks() {
        assert_eq!(text("\"\"\"\r\n\t α\r\n\t \r\n\t \"\"\""), "α\r\n");
        assert_eq!(
            text("\"\"\"\n\t first\n\t\n\t  \n\t last\n\t \"\"\""),
            "first\n\n \nlast"
        );
        assert_eq!(text("\"\"\"\n\"\"\""), "");
        assert_eq!(text("\"\"\"\r\n\t\"\"\""), "");
        assert_eq!(text("\"\"\"\n\n\"\"\""), "");
        assert_eq!(text("\"\"\"\n\n\n\"\"\""), "\n");
        assert_eq!(text("\"\"\"\r  first\r  second\r  \"\"\""), "first\rsecond");
        assert_eq!(
            text("\"\"\"\n  first\n\u{a0}\n  last\n  \"\"\""),
            "first\n\u{a0}\nlast"
        );
    }

    #[test]
    fn block_string_layout_reports_original_byte_range_for_missing_margin() {
        let source = "\"\"\"\n  café\n bad\n  \"\"\"";
        let (_, issues) = chunks(source);
        assert_eq!(issues.len(), 1);
        assert_eq!(&source[issues[0].clone()], " b");
        let (_, issues) = chunks("\"\"\"\n\tgood\n good\n\t\"\"\"");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn block_string_layout_never_rewrites_interpolation_code_or_nested_literals() {
        let source = "f\"\"\"\n  {if true {\n\"}\"\nr\"\"\"nested\n unindented\"\"\"\n} else { \"\" }}\n  after\n  \"\"\"";
        let (parts, issues) = chunks(source);
        assert!(issues.is_empty(), "{issues:?}");
        let expressions: Vec<_> = parts
            .iter()
            .filter_map(|part| match part {
                InterpolationChunk::Expr { source, offset } => Some((*source, *offset)),
                _ => None,
            })
            .collect();
        assert_eq!(expressions.len(), 1);
        assert_eq!(
            expressions[0].0,
            "if true {\n\"}\"\nr\"\"\"nested\n unindented\"\"\"\n} else { \"\" }"
        );
        assert_eq!(
            &source[expressions[0].1..expressions[0].1 + expressions[0].0.len()],
            expressions[0].0
        );
        assert_eq!(text(source), "\nafter");
        for part in parts {
            if let InterpolationChunk::Text {
                source: piece,
                offset,
            } = part
            {
                assert_eq!(&source[offset..offset + piece.len()], piece);
            }
        }
    }

    #[test]
    fn block_string_layout_requires_both_structural_boundaries_and_str_domain() {
        for source in [
            "\"\"\"inline\n  \"\"\"",
            "\"\"\"\n  first\n  \"\"\")",
            "b\"\"\"\n  first\n  \"\"\"",
            "p\"\"\"\n  first\n  \"\"\"",
            "g\"\"\"\n  first\n  \"\"\"",
            "rx\"\"\"\n  first\n  \"\"\"",
            "fp\"\"\"\n  first\n  \"\"\"",
        ] {
            let Some(QuotedScan::Terminated(quoted)) = scan_quoted_literal(source, 0, false) else {
                panic!("quoted literal");
            };
            assert_eq!(
                text(source),
                &source[quoted.content_start..quoted.content_end]
            );
        }
    }
}

#[cfg(test)]
mod block_string_migration_tests {
    use super::*;

    fn decode_hex(text: &str) -> Vec<u8> {
        text.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    // This disk-backed inventory pins old text bytes and interpolation pieces
    // independently of the new layout preparation. Archived migrated literals
    // keep this byte evidence stable when executable script contracts change.
    #[test]
    fn block_string_corpus_migrations_preserve_old_text_and_interpolation_boundaries() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let inventory =
            std::fs::read_to_string(root.join("tests/fixtures/syntax/block-string-migration.tsv"))
                .unwrap();
        let migrated_corpus =
            std::fs::read_to_string(root.join("tests/fixtures/syntax/block-string-migrated.txt"))
                .unwrap();
        let mut count = 0;
        for row in inventory.lines() {
            let columns = row.split('\t').collect::<Vec<_>>();
            let old = String::from_utf8(decode_hex(columns[3])).unwrap();
            let Some(QuotedScan::Terminated(literal)) = scan_quoted_literal(&old, 0, false) else {
                panic!("old literal");
            };
            let body = &old[literal.content_start..literal.content_end];
            let escaped = if literal.raw {
                body.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('$', "\\$")
            } else {
                body.to_string()
            };
            let opening = line_break_len(escaped.as_bytes(), 0);
            let explicit_break = match &escaped[..opening] {
                "\r\n" => "\\r\\n",
                "\r" => "\\r",
                "\n" => "\\n",
                _ => panic!("structural break"),
            };
            let prefix = if literal.raw {
                ""
            } else {
                &old[..literal.content_start - 3]
            };
            let migrated = format!(
                "{prefix}\"\"\"{explicit_break}{}\"\"\"",
                &escaped[opening..]
            );
            assert!(
                migrated_corpus.contains(&migrated),
                "review migrated literal bytes originally from {}",
                columns[0]
            );
            let Some(QuotedScan::Terminated(quoted)) = scan_quoted_literal(&migrated, 0, false)
            else {
                panic!("migrated literal");
            };
            let raw = &migrated[quoted.content_start..quoted.content_end];
            let chunks = interpolation_chunks(raw, quoted.content_start).unwrap();
            let (chunks, issues) = block_string_chunks(&migrated, quoted, chunks);
            assert!(issues.is_empty());
            let pieces = chunks
                .into_iter()
                .map(|chunk| match chunk {
                    InterpolationChunk::Text { source, offset } => {
                        ('T', decode_string_text(source, offset, true).bytes)
                    }
                    InterpolationChunk::Expr { source, .. } => ('E', source.as_bytes().to_vec()),
                })
                .collect::<Vec<_>>();
            let expected = columns[4]
                .split(';')
                .map(|piece| {
                    let (kind, value) = piece.split_once(':').unwrap();
                    (kind.chars().next().unwrap(), decode_hex(value))
                })
                .collect::<Vec<_>>();
            assert_eq!(pieces, expected, "old pieces in {}", columns[0]);
            count += 1;
        }
        assert_eq!(count, 8);
    }
}

#[cfg(test)]
mod fmt_chunk_tests {
    use super::*;

    fn scan(source: &str) -> (QuotedLiteral, Vec<FmtIssueKind>) {
        let Some(QuotedScan::Terminated(literal)) = scan_quoted_literal(source, 0, true) else {
            panic!("terminated literal in {source:?}")
        };
        (
            literal,
            fmt_chunks(source, literal)
                .1
                .into_iter()
                .map(|issue| issue.kind)
                .collect(),
        )
    }

    // The literal's end is the token boundary that every later lexer and
    // parser stage relies on, so it is pinned below the parser.
    #[test]
    fn fmt_literal_ends_after_nested_same_quote_strings_and_braces() {
        for source in [
            "f\"{f\"{\"}\"}\"}\" tail",
            "f\"{ {a: \"}\"}.a }\" tail",
            "f\"\\u{7b}{1}\" tail",
            "fp\"{{\" tail",
        ] {
            let (literal, issues) = scan(source);
            assert_eq!(literal.end, source.find(" tail").unwrap(), "{source:?}");
            assert!(issues.is_empty(), "{source:?}: {issues:?}");
        }
    }

    #[test]
    fn fmt_chunks_report_one_issue_per_malformed_brace() {
        assert_eq!(scan("f\"a } b\"").1, [FmtIssueKind::LoneCloseBrace]);
        assert_eq!(scan("f\"a { b\"").1, [FmtIssueKind::Unclosed]);
        assert_eq!(scan("f\"{ }\"").1, [FmtIssueKind::Empty]);
        assert_eq!(scan("f\"{x # }\"").1, [FmtIssueKind::Comment]);
        assert_eq!(scan("f\"${x}\"").1, [FmtIssueKind::DollarBrace]);
        assert_eq!(scan("f\"${{x}}\"").1, []);
    }
}
