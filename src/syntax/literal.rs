#![allow(clippy::single_call_fn)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuotedLiteralKind {
    Str,
    Bytes,
    Path,
    Glob,
    Fmt,
    PathFmt,
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
        if literal_interpolates(prefix.kind, prefix.raw)
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
            offset += 1;
            if offset < bytes.len() {
                offset += 1;
            }
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
    while let Some(relative) = raw[search_start..].find("${") {
        let open = search_start + relative;
        if is_escaped(raw.as_bytes(), open) {
            search_start = open + 1;
            continue;
        }
        if open > rest_start {
            chunks.push(InterpolationChunk::Text {
                source: &raw[rest_start..open],
                offset: content_offset + rest_start,
            });
        }
        let expr_start = open + 2;
        let close = interpolation_close(raw, expr_start)?;
        chunks.push(InterpolationChunk::Expr {
            source: &raw[expr_start..close],
            offset: content_offset + expr_start,
        });
        rest_start = close + 1;
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
    let rest = bytes.get(start..)?;
    match rest {
        [b'"', ..] => Some(QuotePrefix {
            len: 0,
            raw: false,
            kind: QuotedLiteralKind::Str,
        }),
        [b'b', b'"', ..] => Some(QuotePrefix {
            len: 1,
            raw: false,
            kind: QuotedLiteralKind::Bytes,
        }),
        [b'p', b'"', ..] => Some(QuotePrefix {
            len: 1,
            raw: false,
            kind: QuotedLiteralKind::Path,
        }),
        [b'g', b'"', ..] => Some(QuotePrefix {
            len: 1,
            raw: false,
            kind: QuotedLiteralKind::Glob,
        }),
        [b'f', b'"', ..] => Some(QuotePrefix {
            len: 1,
            raw: false,
            kind: QuotedLiteralKind::Fmt,
        }),
        [b'f', b'p', b'"', ..] => Some(QuotePrefix {
            len: 2,
            raw: false,
            kind: QuotedLiteralKind::PathFmt,
        }),
        [b'r', b'"', ..] => Some(QuotePrefix {
            len: 1,
            raw: true,
            kind: QuotedLiteralKind::Str,
        }),
        [b'r', b'f', b'"', ..] | [b'f', b'r', b'"', ..] => Some(QuotePrefix {
            len: 2,
            raw: true,
            kind: QuotedLiteralKind::Fmt,
        }),
        _ => None,
    }
}

fn literal_interpolates(kind: QuotedLiteralKind, raw: bool) -> bool {
    !raw && matches!(
        kind,
        QuotedLiteralKind::Str | QuotedLiteralKind::Fmt | QuotedLiteralKind::PathFmt
    )
}

fn push_utf8(ch: char, output: &mut Vec<u8>) {
    let mut buffer = [0; 4];
    output.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
}

fn is_bare_path_literal_char(ch: char) -> bool {
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
