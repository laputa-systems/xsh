// Temporary: delete this directory once the Laputa monorepo is migrated.
//
// Rewrites `${expr}`/`$name` f-strings to `{expr}` f-strings. It must read
// the old syntax with the old lexer and interpolation scanner, so `run.sh`
// builds it inside an export of the last commit before `{expr}` f-strings.
// Rewrites are token based: `${e}` -> `{e}`, `$name.field` -> `{name.field}`,
// `\${` -> `${{`, literal braces doubled, line breaks inside single-line
// interpolations joined. Strings that hold XSH source (raw and plain XSH
// strings, f-string text, Rust string literals including `format!` strings)
// are migrated recursively. Running it twice on a file doubles braces again.
use std::fmt::Write as _;
use xsh::frontend::syntax::lexer::lex_spellings;
use xsh::frontend::syntax::literal::{interpolation_chunks, InterpolationChunk};
use xsh::frontend::syntax::token::TokenTag;

#[derive(Clone, Debug)]
struct Edit {
    start: usize,
    end: usize,
    text: String,
}

struct Report {
    file: String,
    notes: Vec<String>,
    fmt_tokens: usize,
}

impl Report {
    fn note(&mut self, msg: String) {
        self.notes.push(format!("{}: {msg}", self.file));
    }
}

fn has_fmt(text: &str) -> bool {
    // Cheap filter for nested scripts: an f-string prefix after a non-ident byte.
    let b = text.as_bytes();
    for i in 0..b.len() {
        let prev_ok = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'-');
        if prev_ok && b[i] == b'f' {
            if b.get(i + 1) == Some(&b'"') || (b.get(i + 1) == Some(&b'p') && b.get(i + 2) == Some(&b'"')) {
                return true;
            }
        }
    }
    false
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

/// Edits over `src` (old XSH source) that migrate every f-string in it.
fn migrate_xsh(src: &str, rep: &mut Report) -> Vec<Edit> {
    let mut edits = Vec::new();
    let base = src.as_ptr() as usize;
    for (tag, text) in lex_spellings(src) {
        let start = text.as_ptr() as usize - base;
        match tag {
            TokenTag::FmtString | TokenTag::PathFmtString => {
                rep.fmt_tokens += 1;
                edits.extend(migrate_fmt(src, start, text, rep));
            }
            TokenTag::String => {
                edits.extend(migrate_plain_string(start, text, rep));
            }
            _ => {}
        }
    }
    edits
}

fn apply(src: &str, edits: &[Edit]) -> String {
    let mut sorted = edits.to_vec();
    sorted.sort_by_key(|e| (e.start, e.end));
    let mut out = String::new();
    let mut at = 0;
    for e in sorted {
        assert!(e.start >= at, "overlapping edits");
        out.push_str(&src[at..e.start]);
        out.push_str(&e.text);
        at = e.end;
    }
    out.push_str(&src[at..]);
    out
}

fn shift(edits: Vec<Edit>, by: usize) -> Vec<Edit> {
    edits.into_iter().map(|e| Edit { start: e.start + by, end: e.end + by, text: e.text }).collect()
}

struct Lit<'a> {
    prefix_len: usize,
    delim: usize,
    content: &'a str,
    content_start: usize,
}

fn split_lit(text: &str) -> Lit<'_> {
    let quote = text.find('"').unwrap();
    let delim = if text[quote..].starts_with("\"\"\"") && text.len() >= quote + 6 { 3 } else { 1 };
    let content_start = quote + delim;
    let content_end = text.len() - delim;
    Lit { prefix_len: quote, delim, content: &text[content_start..content_end], content_start }
}

/// Migrated text of one old interpolation expression, including its braces.
fn migrate_interp(expr: &str, braced: bool, single_line: bool, rep: &mut Report) -> String {
    let inner = apply(expr, &migrate_xsh(expr, rep));
    check_interp(expr, single_line, rep);
    if !braced {
        return format!("{{{inner}}}");
    }
    if inner.starts_with('{') {
        format!("{{ {inner} }}")
    } else {
        format!("{{{inner}}}")
    }
}

fn migrate_fmt(src: &str, start: usize, text: &str, rep: &mut Report) -> Vec<Edit> {
    let lit = split_lit(text);
    let _ = src;
    if !text.starts_with('f') || text.starts_with("fr") {
        rep.note(format!("unexpected fmt prefix {text:?}"));
        return Vec::new();
    }
    let _ = lit.prefix_len;
    let single_line = lit.delim == 1;
    let Some(chunks) = interpolation_chunks(lit.content, 0) else {
        rep.note(format!("unterminated interpolation in {text:?}"));
        return Vec::new();
    };
    let cs = start + lit.content_start;
    // Virtual text for nested-script detection: decoded text with placeholders.
    let mut decoded_probe = String::new();
    for chunk in &chunks {
        match chunk {
            InterpolationChunk::Text { source, .. } => decoded_probe.push_str(&decode_xsh(source).0),
            InterpolationChunk::Expr { .. } => decoded_probe.push_str("__XSHPH__"),
        }
    }
    if has_fmt(&decoded_probe) {
        return vec![Edit { start: cs, end: cs + lit.content.len(), text: migrate_fmt_nested(lit.content, &chunks, single_line, rep) }];
    }
    let mut edits = Vec::new();
    for chunk in &chunks {
        match *chunk {
            InterpolationChunk::Text { source, offset } => {
                edits.extend(shift(text_edits(source), cs + offset));
            }
            InterpolationChunk::Expr { source, offset } => {
                let braced = lit.content[..offset].ends_with("${");
                check_interp(source, single_line, rep);
                if single_line && source.contains(['\n', '\r']) {
                    let migrated = apply(source, &migrate_xsh(source, rep));
                    edits.push(Edit { start: cs + offset, end: cs + offset + source.len(), text: join_lines(&migrated) });
                } else {
                    edits.extend(shift(migrate_xsh(source, rep), cs + offset));
                }
                let end = cs + offset + source.len();
                if braced {
                    let pad = source.starts_with('{');
                    edits.push(Edit { start: cs + offset - 2, end: cs + offset, text: if pad { "{ ".into() } else { "{".into() } });
                    if pad {
                        edits.push(Edit { start: end, end, text: " ".into() });
                    }
                } else {
                    edits.push(Edit { start: cs + offset - 1, end: cs + offset, text: "{".into() });
                    edits.push(Edit { start: end, end, text: "}".into() });
                }
            }
        }
    }
    edits
}

/// Joins a multi-line call or chain onto one line, dropping trailing commas
/// before closing brackets.
fn join_lines(expr: &str) -> String {
    let base = expr.as_ptr() as usize;
    let tokens: Vec<(TokenTag, &str, usize)> = lex_spellings(expr)
        .into_iter()
        .map(|(tag, text)| (tag, text, text.as_ptr() as usize - base))
        .filter(|(tag, _, _)| *tag != TokenTag::Newline)
        .collect();
    let mut out = String::new();
    let Some(first) = tokens.first() else { return expr.to_string() };
    out.push_str(&expr[..first.2]);
    let mut index = 0;
    while index < tokens.len() {
        let (_, text, start) = tokens[index];
        let end = start + text.len();
        let next = tokens.get(index + 1);
        if text == "," {
            if let Some(next) = next && matches!(next.1, ")" | "]" | "}") && expr[end..next.2].contains('\n') {
                index += 1;
                continue;
            }
        }
        out.push_str(text);
        if let Some(next) = next {
            let gap = &expr[end..next.2];
            if gap.contains(['\n', '\r']) {
                if !(matches!(text, "(" | "[" | "{") || matches!(next.1, ")" | "]" | "}" | ".")) {
                    out.push(' ');
                }
            } else {
                out.push_str(gap);
            }
        } else {
            out.push_str(expr[end..].trim_end_matches(['\n', '\r', ' ']));
        }
        index += 1;
    }
    out
}

fn check_interp(expr: &str, single_line: bool, rep: &mut Report) {
    if lex_spellings(expr).iter().any(|(tag, _)| *tag == TokenTag::Comment) {
        rep.note(format!("comment inside interpolation: {expr:?}"));
    }
    if single_line && expr.contains(['\n', '\r']) && expr.contains("\"\"\"") {
        rep.note(format!("line break inside single-line interpolation: {expr:?}"));
    }
}

/// Respelling of old f-string text: braces doubled, `\${` becomes `${{`,
/// and `\$` stays only before an identifier.
fn text_edits(t: &str) -> Vec<Edit> {
    let mut edits = Vec::new();
    let b = t.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                if b.get(i + 1) == Some(&b'$') {
                    let next = t[i + 2..].chars().next();
                    if !next.is_some_and(is_ident_start) {
                        edits.push(Edit { start: i, end: i + 2, text: "$".into() });
                    }
                    i += 2;
                } else if b.get(i + 1) == Some(&b'u') && b.get(i + 2) == Some(&b'{') {
                    let close = t[i..].find('}').map_or(b.len(), |r| i + r + 1);
                    i = close;
                } else {
                    i += 1;
                    i += t[i..].chars().next().map_or(0, char::len_utf8);
                }
            }
            b'{' => {
                edits.push(Edit { start: i, end: i + 1, text: "{{".into() });
                i += 1;
            }
            b'}' => {
                edits.push(Edit { start: i, end: i + 1, text: "}}".into() });
                i += 1;
            }
            _ => i += 1,
        }
    }
    edits
}

/// One decoded char (or placeholder) with the raw source range it came from.
#[derive(Clone, Debug)]
struct VChar {
    text: String,
    raw: std::ops::Range<usize>,
    placeholder: Option<usize>,
}

fn decode_xsh(t: &str) -> (String, Vec<VChar>) {
    let mut out = String::new();
    let mut v = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let c = t[i..].chars().next().unwrap();
        if c != '\\' {
            v.push(VChar { text: c.to_string(), raw: i..i + c.len_utf8(), placeholder: None });
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let s = i;
        i += 1;
        let Some(e) = t[i..].chars().next() else {
            v.push(VChar { text: "\\".into(), raw: s..i, placeholder: None });
            out.push('\\');
            break;
        };
        i += e.len_utf8();
        let decoded: String = match e {
            '\\' => "\\".into(),
            '"' => "\"".into(),
            '$' => "$".into(),
            'n' => "\n".into(),
            'r' => "\r".into(),
            't' => "\t".into(),
            '0' => "\0".into(),
            'x' => {
                let hex = &t[i..(i + 2).min(t.len())];
                i += hex.len();
                char::from(u8::from_str_radix(hex, 16).unwrap_or(b'?')).to_string()
            }
            'u' if t[i..].starts_with('{') => {
                let close = t[i..].find('}').map_or(t.len(), |r| i + r);
                let digits = &t[i + 1..close];
                i = (close + 1).min(t.len());
                char::from_u32(u32::from_str_radix(digits, 16).unwrap_or(63)).unwrap_or('?').to_string()
            }
            other => format!("\\{other}"),
        };
        out.push_str(&decoded);
        v.push(VChar { text: decoded, raw: s..i, placeholder: None });
    }
    (out, v)
}

fn placeholder(n: usize) -> String {
    format!("__XSHPH{n}__")
}

/// Builds the virtual string and per-byte owner index for `vchars`.
fn virtual_text(vchars: &[VChar]) -> (String, Vec<usize>, Vec<usize>) {
    let mut s = String::new();
    let mut owner = Vec::new();
    let mut starts = Vec::new();
    for (index, vc) in vchars.iter().enumerate() {
        starts.push(s.len());
        let piece = match vc.placeholder {
            Some(n) => placeholder(n),
            None => vc.text.clone(),
        };
        for _ in 0..piece.len() {
            owner.push(index);
        }
        s.push_str(&piece);
    }
    starts.push(s.len());
    (s, owner, starts)
}

/// Maps inner edits on the virtual text to (vchar start index, vchar end index, text).
fn map_edits(edits: &[Edit], starts: &[usize], rep: &mut Report, ctx: &str) -> Option<Vec<(usize, usize, String)>> {
    let mut out = Vec::new();
    for e in edits {
        let a = starts.iter().position(|s| *s == e.start);
        let b = starts.iter().position(|s| *s == e.end);
        match (a, b) {
            (Some(a), Some(b)) => out.push((a, b, e.text.clone())),
            _ => {
                rep.note(format!("edit splits an escape or placeholder in {ctx:?}"));
                return None;
            }
        }
    }
    out.sort_by_key(|x| (x.0, x.1));
    Some(out)
}

fn migrate_fmt_nested(content: &str, chunks: &[InterpolationChunk<'_>], single_line: bool, rep: &mut Report) -> String {
    let mut vchars = Vec::new();
    let mut interps = Vec::new();
    for chunk in chunks {
        match *chunk {
            InterpolationChunk::Text { source, offset } => {
                for mut vc in decode_xsh(source).1 {
                    vc.raw = vc.raw.start + offset..vc.raw.end + offset;
                    vchars.push(vc);
                }
            }
            InterpolationChunk::Expr { source, offset } => {
                let braced = content[..offset].ends_with("${");
                let (open, close) = if braced { (offset - 2, offset + source.len() + 1) } else { (offset - 1, offset + source.len()) };
                let n = interps.len();
                interps.push(migrate_interp(source, braced, single_line, rep));
                vchars.push(VChar { text: String::new(), raw: open..close, placeholder: Some(n) });
            }
        }
    }
    let (virt, _owner, starts) = virtual_text(&vchars);
    let inner = migrate_xsh(&virt, rep);
    let Some(mapped) = map_edits(&inner, &starts, rep, content) else {
        return content.to_string();
    };
    let triple = !single_line;
    let mut out = String::new();
    let mut index = 0;
    let mut pending = mapped.into_iter().peekable();
    // Next virtual char text after a position, for `$` decisions.
    while index <= vchars.len() {
        if let Some((a, _, _)) = pending.peek() && *a == index {
            let (_, b, text) = pending.next().unwrap();
            // Re-encode replacement; the char following it decides `$` spelling.
            if text.contains("__XSHPH") {
                rep.note(format!("outer interpolation inside a rewritten region of {content:?}"));
            }
            let following = vchars.get(b).map(|vc| if vc.placeholder.is_some() { "{".to_string() } else { vc.text.clone() }).unwrap_or_default();
            out.push_str(&encode_fmt_text(&text, &following, triple));
            if b > index {
                index = b;
            }
            continue;
        }
        if index == vchars.len() {
            break;
        }
        let vc = &vchars[index];
        match vc.placeholder {
            Some(n) => out.push_str(&interps[n]),
            None => {
                let raw = &content[vc.raw.clone()];
                let next = vchars.get(index + 1).map(|n| if n.placeholder.is_some() { "{".to_string() } else { n.text.clone() }).unwrap_or_default();
                match raw {
                    "{" => out.push_str("{{"),
                    "}" => out.push_str("}}"),
                    "\\$" => {
                        if next.chars().next().is_some_and(is_ident_start) {
                            out.push_str("\\$")
                        } else {
                            out.push('$')
                        }
                    }
                    _ => out.push_str(raw),
                }
            }
        }
        index += 1;
    }
    out
}

fn encode_fmt_text(text: &str, following: &str, triple: bool) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().chain(following.chars().take(1)).collect();
    let n = text.chars().count();
    for i in 0..n {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' if next.is_some_and(is_ident_start) => out.push_str("\\$"),
            '\n' if !triple => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' if !triple => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn encode_xsh_plain(text: &str, triple: bool, raw: bool) -> Option<String> {
    if raw {
        if (!triple && text.contains('"')) || text.contains("\"\"\"") {
            return None;
        }
        return Some(text.to_string());
    }
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' if chars.get(i + 1) == Some(&'{') => out.push_str("\\$"),
            '\n' if !triple => out.push_str("\\n"),
            '\t' if !triple => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(*c),
        }
    }
    Some(out)
}

fn migrate_plain_string(start: usize, text: &str, rep: &mut Report) -> Vec<Edit> {
    let lit = split_lit(text);
    let raw = text.starts_with('r');
    let triple = lit.delim == 3;
    let cs = start + lit.content_start;
    if raw {
        if !has_fmt(lit.content) {
            return Vec::new();
        }
        let edits = migrate_xsh(lit.content, rep);
        for e in &edits {
            if encode_xsh_plain(&e.text, triple, true).is_none() {
                rep.note(format!("raw string cannot hold migrated text {:?}", e.text));
                return Vec::new();
            }
        }
        return shift(edits, cs);
    }
    let (_, vchars) = decode_xsh(lit.content);
    let (virt, _owner, starts) = virtual_text(&vchars);
    if !has_fmt(&virt) {
        return Vec::new();
    }
    let inner = migrate_xsh(&virt, rep);
    let Some(mapped) = map_edits(&inner, &starts, rep, lit.content) else { return Vec::new() };
    let mut edits = Vec::new();
    for (a, b, t) in mapped {
        let raw_start = vchars.get(a).map_or(lit.content.len(), |vc| vc.raw.start);
        let raw_end = if b == a { raw_start } else { vchars[b - 1].raw.end };
        let Some(encoded) = encode_xsh_plain(&t, triple, false) else { continue };
        edits.push(Edit { start: cs + raw_start, end: cs + raw_end, text: encoded });
    }
    edits
}

// ---------- Rust files ----------

fn rust_string_literals(src: &str) -> Vec<(usize, usize, usize, usize, bool)> {
    // (literal start, content start, content end, literal end, is_raw)
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0;
                while i < b.len() {
                    if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        i += 2;
                    } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'r' | b'b' if (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) => {
                let mut j = i;
                if b[j] == b'b' {
                    j += 1;
                }
                if b.get(j) == Some(&b'r') {
                    j += 1;
                    let hashes_start = j;
                    while b.get(j) == Some(&b'#') {
                        j += 1;
                    }
                    let hashes = j - hashes_start;
                    if b.get(j) == Some(&b'"') {
                        let content_start = j + 1;
                        let closing = format!("\"{}", "#".repeat(hashes));
                        let content_end = content_start + src[content_start..].find(&closing).unwrap();
                        let end = content_end + closing.len();
                        out.push((i, content_start, content_end, end, true));
                        i = end;
                        continue;
                    }
                }
                if b[i] == b'b' && b.get(i + 1) == Some(&b'"') {
                    i += 1;
                    continue;
                }
                // identifier
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
            }
            b'"' => {
                let content_start = i + 1;
                let mut j = content_start;
                while b[j] != b'"' {
                    if b[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                out.push((i, content_start, j, j + 1, false));
                i = j + 1;
            }
            b'\'' => {
                // char literal or lifetime
                if b.get(i + 1) == Some(&b'\\') {
                    let close = src[i + 2..].find('\'').unwrap();
                    i = i + 2 + close + 1;
                } else {
                    let c = src[i + 1..].chars().next().unwrap();
                    let after = i + 1 + c.len_utf8();
                    if b.get(after) == Some(&b'\'') {
                        i = after + 1;
                    } else {
                        i += 1;
                    }
                }
            }
            c if c.is_ascii_alphanumeric() || c == b'_' => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

fn is_format_string(src: &str, literal_start: usize) -> bool {
    let before = src[..literal_start].trim_end();
    for mac in ["format!(", "println!(", "print!(", "eprintln!(", "eprint!(", "panic!(", "format_args!(", "unreachable!(", "todo!("] {
        if before.ends_with(mac) {
            return true;
        }
    }
    // write!(target, "...")
    if let Some(comma) = before.strip_suffix(',') {
        let line_start = comma.rfind(['\n', ';', '{']).map_or(0, |p| p + 1);
        let tail = comma[line_start..].trim_start();
        if (tail.starts_with("write!(") || tail.starts_with("writeln!(")) && !tail[tail.find('(').unwrap() + 1..].contains(['(', '"', ',']) {
            return true;
        }
    }
    false
}

fn decode_rust(t: &str, raw: bool, fmt: bool) -> Vec<VChar> {
    let mut v = Vec::new();
    let mut i = 0;
    let mut ph = 0;
    while i < t.len() {
        let c = t[i..].chars().next().unwrap();
        if fmt && (c == '{' || c == '}') {
            if t[i + 1..].starts_with(c) {
                v.push(VChar { text: c.to_string(), raw: i..i + 2, placeholder: None });
                i += 2;
                continue;
            }
            if c == '{' {
                let close = t[i..].find('}').map(|r| i + r).unwrap();
                v.push(VChar { text: String::new(), raw: i..close + 1, placeholder: Some(1000 + ph) });
                ph += 1;
                i = close + 1;
                continue;
            }
        }
        if raw || c != '\\' {
            v.push(VChar { text: c.to_string(), raw: i..i + c.len_utf8(), placeholder: None });
            i += c.len_utf8();
            continue;
        }
        let s = i;
        i += 1;
        let e = t[i..].chars().next().unwrap();
        i += e.len_utf8();
        let decoded: String = match e {
            '\\' => "\\".into(),
            '"' => "\"".into(),
            '\'' => "'".into(),
            'n' => "\n".into(),
            'r' => "\r".into(),
            't' => "\t".into(),
            '0' => "\0".into(),
            'x' => {
                let hex = &t[i..i + 2];
                i += 2;
                char::from(u8::from_str_radix(hex, 16).unwrap()).to_string()
            }
            'u' => {
                let close = t[i..].find('}').unwrap() + i;
                let digits = &t[i + 1..close];
                i = close + 1;
                char::from_u32(u32::from_str_radix(digits, 16).unwrap()).unwrap().to_string()
            }
            '\n' | '\r' => {
                while i < t.len() && t.as_bytes()[i].is_ascii_whitespace() {
                    i += 1;
                }
                String::new()
            }
            other => panic!("unknown rust escape \\{other}"),
        };
        v.push(VChar { text: decoded, raw: s..i, placeholder: None });
    }
    v
}

fn encode_rust(text: &str, raw: bool, fmt: bool, hashes: &str) -> Option<String> {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '{' if fmt => out.push_str("{{"),
            '}' if fmt => out.push_str("}}"),
            '\\' if !raw => out.push_str("\\\\"),
            '"' if !raw => out.push_str("\\\""),
            '\n' if !raw => out.push_str("\\n"),
            '\t' if !raw => out.push_str("\\t"),
            '\r' if !raw => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    if raw && out.contains(&format!("\"{hashes}")) {
        return None;
    }
    Some(out)
}

fn restore_rust_placeholders(text: &str, vchars: &[VChar], content: &str, raw: bool, fmt: bool, hashes: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("__RSPH") {
        out.push_str(&encode_rust(&rest[..at], raw, fmt, hashes)?);
        let tail = &rest[at + 6..];
        let digits_end = tail.find("__").unwrap();
        let n: usize = tail[..digits_end].parse().unwrap();
        let vc = vchars.iter().find(|vc| vc.placeholder == Some(n)).unwrap();
        out.push_str(&content[vc.raw.clone()]);
        rest = &tail[digits_end + 2..];
    }
    out.push_str(&encode_rust(rest, raw, fmt, hashes)?);
    Some(out)
}

fn rust_placeholder(n: usize) -> String {
    format!("__RSPH{n}__")
}

fn migrate_rust(src: &str, rep: &mut Report) -> Vec<Edit> {
    let mut edits = Vec::new();
    for (lit_start, cs, ce, _end, raw) in rust_string_literals(src) {
        let content = &src[cs..ce];
        let fmt = is_format_string(src, lit_start);
        let vchars = decode_rust(content, raw, fmt);
        let mut virt = String::new();
        let mut starts = Vec::new();
        for vc in &vchars {
            starts.push(virt.len());
            match vc.placeholder {
                Some(n) => virt.push_str(&rust_placeholder(n)),
                None => virt.push_str(&vc.text),
            }
        }
        starts.push(virt.len());
        if !has_fmt(&virt) {
            continue;
        }
        let hashes = if raw { &src[lit_start..cs - 1].trim_start_matches(['b', 'r']) } else { "" };
        let inner = migrate_xsh(&virt, rep);
        if inner.is_empty() {
            continue;
        }
        let Some(mapped) = map_edits(&inner, &starts, rep, content) else { continue };
        for (a, b, t) in mapped {
            let raw_start = vchars.get(a).map_or(content.len(), |vc| vc.raw.start);
            let raw_end = if b == a { raw_start } else { vchars[b - 1].raw.end };
            let restored = restore_rust_placeholders(&t, &vchars, content, raw, fmt, hashes);
            match restored {
                Some(encoded) => edits.push(Edit { start: cs + raw_start, end: cs + raw_end, text: encoded }),
                None => rep.note(format!("raw rust string cannot hold {t:?}")),
            }
        }
    }
    edits
}

fn main() {
    let mut total = 0;
    let mut changed = 0;
    for path in std::env::args().skip(1) {
        let src = std::fs::read_to_string(&path).unwrap();
        let mut rep = Report { file: path.clone(), notes: Vec::new(), fmt_tokens: 0 };
        let edits = if path.ends_with(".rs") { migrate_rust(&src, &mut rep) } else { migrate_xsh(&src, &mut rep) };
        total += rep.fmt_tokens;
        if !edits.is_empty() {
            let out = apply(&src, &edits);
            if out != src {
                changed += 1;
                std::fs::write(&path, out).unwrap();
            }
        }
        for n in rep.notes {
            eprintln!("NOTE {n}");
        }
    }
    let mut summary = String::new();
    let _ = write!(summary, "fmt tokens seen: {total}, files changed: {changed}");
    eprintln!("{summary}");
}
