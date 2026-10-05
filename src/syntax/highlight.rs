//! Syntax highlighting from the real lexer.
//!
//! Highlighting reads the same token boundaries the parser reads, including
//! f-string interpolation boundaries, so a renderer never disagrees with the
//! language about where a string or an expression ends. Classification beyond
//! the token tag uses only neighboring tokens; it never parses.

use crate::source::SourceId;
use crate::syntax::lexer::Lexer;
use crate::syntax::literal::{
    InterpolationChunk, QuotedLiteralKind, QuotedScan, fmt_chunks, scan_bare_path_at,
    scan_quoted_literal,
};
use crate::syntax::token::TokenTag;

/// What a run of source text is, as a closed vocabulary renderers map to
/// their own styles.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Kind {
    /// Whitespace, line breaks, bytes the lexer rejects, and identifiers that
    /// carry no further meaning.
    #[default]
    Plain,
    Comment,
    /// A `##` or `##!` comment.
    DocComment,
    Keyword,
    /// `true`, `false`, and `null`.
    Constant,
    /// A capitalized name: a type, `Ok`/`Err`, or an enum or error variant.
    Type,
    /// A called name, a declared `proc`/`pure`/`stream`/`test` name, a pipeline
    /// stage, or the program word of a `run` command.
    Function,
    /// A name after `.` that is not called, or a field or argument label.
    Property,
    /// `$name`, `${`...`}`, `$?`, and an `@name` splice.
    Variable,
    /// A string or bytes literal, including the text parts of an f-string.
    String,
    /// A path, glob, or bare path literal.
    Path,
    Regex,
    Number,
    Operator,
    Punctuation,
    /// The braces of an f-string interpolation and its `:>N` width spec.
    Interpolation,
}

impl Kind {
    pub const ALL: [Kind; 16] = [
        Kind::Plain,
        Kind::Comment,
        Kind::DocComment,
        Kind::Keyword,
        Kind::Constant,
        Kind::Type,
        Kind::Function,
        Kind::Property,
        Kind::Variable,
        Kind::String,
        Kind::Path,
        Kind::Regex,
        Kind::Number,
        Kind::Operator,
        Kind::Punctuation,
        Kind::Interpolation,
    ];

    /// The stable spelling machine-readable output uses.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Comment => "comment",
            Self::DocComment => "doc-comment",
            Self::Keyword => "keyword",
            Self::Constant => "constant",
            Self::Type => "type",
            Self::Function => "function",
            Self::Property => "property",
            Self::Variable => "variable",
            Self::String => "string",
            Self::Path => "path",
            Self::Regex => "regex",
            Self::Number => "number",
            Self::Operator => "operator",
            Self::Punctuation => "punctuation",
            Self::Interpolation => "interpolation",
        }
    }
}

/// A byte range of the source with one kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Run {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

/// Splits `source` into runs that tile it exactly: no gaps, no overlaps, and
/// no two adjacent runs of the same kind. Source the lexer rejects stays
/// `Plain`, so a half-typed program still renders.
pub fn highlight(source: &str) -> Vec<Run> {
    let mut kinds = vec![Kind::Plain; source.len()];
    paint(source, 0, &mut kinds);

    let mut runs: Vec<Run> = Vec::new();
    for (offset, kind) in kinds.into_iter().enumerate() {
        match runs.last_mut() {
            Some(run) if run.kind == kind => run.end = offset + 1,
            _ => runs.push(Run {
                kind,
                start: offset,
                end: offset + 1,
            }),
        }
    }
    runs
}

#[derive(Clone, Copy)]
struct Token {
    tag: TokenTag,
    start: usize,
    end: usize,
}

/// The tokens of `source`, and the offset of each `\` that continues a
/// command onto the next line: the lexer reads those as whitespace.
fn lex(source: &str) -> (Vec<Token>, Vec<usize>) {
    let table = Lexer::new(SourceId::new(0), source)
        .lex_compact()
        .token_table;
    let continuations = table
        .line_continuations()
        .iter()
        .map(|offset| *offset as usize)
        .collect();
    let tokens = (0..table.len())
        .filter_map(|index| {
            let tag = table.tag_at(index)?;
            let span = table.span_at(index, SourceId::new(0), source)?;
            (tag != TokenTag::Eof).then(|| Token {
                tag,
                start: span.start(),
                end: span.end(),
            })
        })
        .collect();
    (tokens, continuations)
}

fn fill(kinds: &mut [Kind], base: usize, start: usize, end: usize, kind: Kind) {
    kinds[base + start..base + end].fill(kind);
}

/// Paints the kinds of `source`, which sits at `base` in the whole text, into
/// `kinds`. An f-string interpolation recurses with its expression's offset.
fn paint(source: &str, base: usize, kinds: &mut [Kind]) {
    let (tokens, continuations) = lex(source);
    for offset in continuations {
        fill(kinds, base, offset, offset + 1, Kind::Punctuation);
    }
    // Where a bare path literal or `@name` splice ends, so the tokens inside
    // are not classified on their own.
    let mut skip_until = 0;
    // A token already painted by looking ahead from the `run` before it.
    let mut claimed: Option<usize> = None;
    // Open braces, so a `${` is closed by a variable-colored `}`.
    let mut braces: Vec<bool> = Vec::new();
    let mut in_use = false;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        index += 1;
        if token.start < skip_until || claimed == Some(token.start) {
            continue;
        }
        let text = &source[token.start..token.end];
        let previous = index.checked_sub(2).map(|at| tokens[at]);
        let next = tokens.get(index).copied();
        let adjacent_after = next.filter(|next| next.start == token.end);
        let paint_token = |kinds: &mut [Kind], kind: Kind| {
            fill(kinds, base, token.start, token.end, kind);
        };

        if matches!(token.tag, TokenTag::Newline | TokenTag::Semicolon) {
            in_use = false;
        }

        match token.tag {
            TokenTag::Comment => paint_token(
                kinds,
                if text.starts_with("##") {
                    Kind::DocComment
                } else {
                    Kind::Comment
                },
            ),
            TokenTag::Keyword => {
                let kind = match text {
                    "true" | "false" | "null" => Kind::Constant,
                    _ if previous.is_some_and(|previous| previous.tag == TokenTag::Dot) => {
                        Kind::Property
                    }
                    _ => Kind::Keyword,
                };
                in_use = text == "use" && kind == Kind::Keyword;
                paint_token(kinds, kind);
                if kind == Kind::Keyword {
                    match text {
                        "proc" | "pure" | "stream" => {
                            if let Some(name) = next.filter(is_name) {
                                fill(kinds, base, name.start, name.end, Kind::Function);
                                skip_until = name.end;
                            }
                        }
                        "run" => claimed = paint_run_command(source, base, &tokens, index, kinds),
                        _ => {}
                    }
                }
            }
            TokenTag::Ident | TokenTag::ProcIdent => {
                let kind = classify_name(source, &tokens, index - 1, in_use);
                paint_token(kinds, kind);
                if kind == Kind::Keyword
                    && matches!(text, "cli" | "test")
                    && let Some(name) = next.filter(is_name)
                {
                    fill(kinds, base, name.start, name.end, Kind::Function);
                    skip_until = name.end;
                }
            }
            TokenTag::Int | TokenTag::Float | TokenTag::Duration => {
                paint_token(kinds, Kind::Number);
            }
            TokenTag::String | TokenTag::Bytes => paint_token(kinds, Kind::String),
            TokenTag::PathString | TokenTag::GlobString => paint_token(kinds, Kind::Path),
            TokenTag::Regex => paint_token(kinds, Kind::Regex),
            TokenTag::FmtString => {
                paint_token(kinds, Kind::String);
                paint_interpolations(source, base, token, kinds);
            }
            TokenTag::PathFmtString => {
                paint_token(kinds, Kind::Path);
                paint_interpolations(source, base, token, kinds);
            }
            TokenTag::DollarIdent | TokenTag::LastStatus | TokenTag::EnvString => {
                paint_token(kinds, Kind::Variable)
            }
            TokenTag::DollarLBrace => {
                braces.push(true);
                paint_token(kinds, Kind::Variable);
            }
            TokenTag::LBrace => {
                braces.push(false);
                paint_token(kinds, Kind::Punctuation);
            }
            TokenTag::RBrace => {
                let kind = if braces.pop() == Some(true) {
                    Kind::Variable
                } else {
                    Kind::Punctuation
                };
                paint_token(kinds, kind);
            }
            TokenTag::At => {
                // `@name` splices a list, so the name is part of the sigil.
                match adjacent_after.filter(is_name) {
                    Some(name) => {
                        fill(kinds, base, token.start, name.end, Kind::Variable);
                        skip_until = name.end;
                    }
                    None => paint_token(kinds, Kind::Operator),
                }
            }
            TokenTag::Slash | TokenTag::Dot => {
                if let Some(end) = bare_path_end(source, token, previous) {
                    fill(kinds, base, token.start, end, Kind::Path);
                    skip_until = end;
                } else {
                    let kind = if token.tag == TokenTag::Dot {
                        Kind::Punctuation
                    } else {
                        Kind::Operator
                    };
                    paint_token(kinds, kind);
                }
            }
            TokenTag::LParen
            | TokenTag::RParen
            | TokenTag::LBracket
            | TokenTag::RBracket
            | TokenTag::Comma
            | TokenTag::Colon
            | TokenTag::Semicolon => paint_token(kinds, Kind::Punctuation),
            TokenTag::Newline | TokenTag::Eof => {}
            TokenTag::Question
            | TokenTag::QuestionQuestion
            | TokenTag::Arrow
            | TokenTag::FatArrow
            | TokenTag::Equals
            | TokenTag::EqEq
            | TokenTag::Bang
            | TokenTag::BangEq
            | TokenTag::Lt
            | TokenTag::Le
            | TokenTag::Gt
            | TokenTag::Ge
            | TokenTag::Plus
            | TokenTag::Minus
            | TokenTag::Star
            | TokenTag::Percent
            | TokenTag::Pipe
            | TokenTag::PipeGt
            | TokenTag::Amp
            | TokenTag::GtGt
            | TokenTag::ErrorGt
            | TokenTag::ErrorGtGt => paint_token(kinds, Kind::Operator),
        }
    }
}

fn is_name(token: &Token) -> bool {
    matches!(token.tag, TokenTag::Ident | TokenTag::ProcIdent)
}

/// Whether a token ends a value, so a `/` right after it divides instead of
/// starting a path.
fn ends_value(tag: TokenTag) -> bool {
    matches!(
        tag,
        TokenTag::Ident
            | TokenTag::ProcIdent
            | TokenTag::Int
            | TokenTag::Float
            | TokenTag::Duration
            | TokenTag::String
            | TokenTag::PathString
            | TokenTag::GlobString
            | TokenTag::EnvString
            | TokenTag::FmtString
            | TokenTag::PathFmtString
            | TokenTag::Bytes
            | TokenTag::Regex
            | TokenTag::DollarIdent
            | TokenTag::LastStatus
            | TokenTag::RParen
            | TokenTag::RBracket
            | TokenTag::RBrace
            | TokenTag::Question
    )
}

/// The end of the bare path literal starting at `token`, if one does. A path
/// starts where a value cannot end, or after a space with no space following
/// (`run ls /var/log`); `a / b` and `a/b` divide.
fn bare_path_end(source: &str, token: Token, previous: Option<Token>) -> Option<usize> {
    let rest = &source[token.start..];
    if token.tag == TokenTag::Dot && !(rest.starts_with("./") || rest.starts_with("../")) {
        return None;
    }
    let end = scan_bare_path_at(source, token.start)?;
    if end <= token.start + 1 {
        return None;
    }
    let value_before = previous.is_some_and(|previous| ends_value(previous.tag));
    let glued = previous.is_some_and(|previous| previous.end == token.start);
    (!value_before || !glued).then_some(end)
}

fn is_screaming(name: &str) -> bool {
    name.len() > 1
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

/// The kind of the identifier at `tokens[at]`, from its neighbors alone.
fn classify_name(source: &str, tokens: &[Token], at: usize, in_use: bool) -> Kind {
    let token = tokens[at];
    let text = &source[token.start..token.end];
    let previous = at.checked_sub(1).map(|before| tokens[before]);
    let next = tokens.get(at + 1).copied();
    let adjacent_after = next.filter(|next| next.start == token.end);
    let after_dot = previous.is_some_and(|previous| previous.tag == TokenTag::Dot);
    let called = next.is_some_and(|next| next.tag == TokenTag::LParen);

    if in_use && text == "as" {
        return Kind::Keyword;
    }
    // `cli`, `test`, and `error` are ordinary names except where they open a
    // declaration: first in a statement and followed by the declared name.
    if matches!(text, "cli" | "test" | "error")
        && statement_start(previous, source)
        && next.is_some_and(|next| is_name(&next))
    {
        return Kind::Keyword;
    }
    // `repeat` and `times` are ordinary names except in the head of a
    // `repeat COUNT times {` statement.
    if matches!(text, "repeat" | "times") && repeat_head_word(source, tokens, at) {
        return Kind::Keyword;
    }
    // `exit` is an ordinary name except where it begins `exit STATUS`: first
    // in a statement, with the status after a space on the same line. `fail`
    // begins `fail MESSAGE` under the same rule.
    if matches!(text, "exit" | "fail") && operand_statement_word(source, tokens, at) {
        return Kind::Keyword;
    }
    // After the `else` of a `guard`, `fail` is the statement without a block.
    if text == "fail" && guard_fail_word(source, tokens, at) {
        return Kind::Keyword;
    }
    // `because` is an ordinary name except between the failure and the cause
    // of a `fail` statement.
    if text == "because" && fail_because_word(source, tokens, at) {
        return Kind::Keyword;
    }
    // `without` is an ordinary name except where it opens a
    // `without EFFECT, ... {` statement.
    if text == "without" && without_head_word(source, tokens, at, previous) {
        return Kind::Keyword;
    }
    // `tempdir` and `at` are ordinary names except in the head of a
    // `tempdir NAME at PATH {` scope.
    if matches!(text, "tempdir" | "at") && tempdir_head_word(source, tokens, at) {
        return Kind::Keyword;
    }
    // `atomically`, `replace`, and `as` are ordinary names except in the head
    // of an `atomically replace DEST as NAME {` statement.
    if matches!(text, "atomically" | "replace" | "as") && atomically_head_word(source, tokens, at) {
        return Kind::Keyword;
    }
    // `print` is a statement form, not a reserved word, so only a bare use
    // reads as one.
    if text == "print" && !after_dot && !called {
        return Kind::Keyword;
    }
    // `cd DIR { ... }` and `env NAME=value { ... }` are statement forms; `env`
    // is also a module, so it needs a word after it rather than `.` or `(`.
    if matches!(text, "cd" | "env")
        && statement_start(previous, source)
        && next.is_some_and(|next| !matches!(next.tag, TokenTag::Dot | TokenTag::LParen))
    {
        return Kind::Keyword;
    }
    // `within DURATION { ... }` opens a scope wherever an expression may
    // start; the duration is a literal or a dotted name.
    if text == "within" && !after_dot && within_head(tokens, at) {
        return Kind::Keyword;
    }
    // `until`, `within`, `every`, and `backoff` are ordinary names except in
    // the head of a `wait until` statement or a `retry backoff`.
    if matches!(text, "until" | "within" | "every" | "backoff")
        && !after_dot
        && paced_head_word(source, tokens, at)
    {
        return Kind::Keyword;
    }
    // `tempdir NAME { ... }` opens a scope wherever an expression may start.
    if text == "tempdir"
        && !after_dot
        && next.is_some_and(|next| next.tag == TokenTag::Ident)
        && tokens
            .get(at + 2)
            .is_some_and(|brace| brace.tag == TokenTag::LBrace)
    {
        return Kind::Keyword;
    }
    if text.as_bytes()[0].is_ascii_uppercase() {
        if after_dot && is_screaming(text) {
            return Kind::Property;
        }
        // `LC_ALL=C` in an `env` block names a variable; it is not a type.
        if is_screaming(text) && adjacent_after.is_some_and(|next| next.tag == TokenTag::Equals) {
            return Kind::Property;
        }
        return Kind::Type;
    }
    if called || token.tag == TokenTag::ProcIdent {
        return Kind::Function;
    }
    if previous.is_some_and(|previous| previous.tag == TokenTag::PipeGt) {
        return Kind::Function;
    }
    if after_dot {
        return Kind::Property;
    }
    // A label (`limit: UInt`, `width: 4`, `file:`) but not the name in a
    // `let x: T` binding.
    if adjacent_after.is_some_and(|next| next.tag == TokenTag::Colon)
        && !is_binding_name(&tokens[..at], source)
    {
        return Kind::Property;
    }
    Kind::Plain
}

fn is_binding_name(before: &[Token], source: &str) -> bool {
    before.last().is_some_and(|last| {
        last.tag == TokenTag::Keyword && matches!(&source[last.start..last.end], "let" | "var" | "const")
    })
}

/// Whether `tokens[at]` is the `repeat` or the `times` of a statement head
/// `repeat COUNT times {` written on one line.
fn repeat_head_word(source: &str, tokens: &[Token], at: usize) -> bool {
    let text = |token: &Token| &source[token.start..token.end];
    let line_start = tokens[..at]
        .iter()
        .rposition(|token| matches!(token.tag, TokenTag::Newline | TokenTag::Semicolon))
        .map_or(0, |separator| separator + 1);
    let Some(first) = (line_start..=at).find(|&index| {
        text(&tokens[index]) == "repeat"
            && statement_start(index.checked_sub(1).map(|before| tokens[before]), source)
    }) else {
        return false;
    };
    let times = tokens[first + 1..]
        .iter()
        .take_while(|token| token.tag != TokenTag::Newline)
        .position(|token| token.tag == TokenTag::Ident && text(token) == "times")
        .map(|offset| first + 1 + offset)
        .filter(|&times| {
            times > first + 1
                && tokens
                    .get(times + 1)
                    .is_some_and(|next| next.tag == TokenTag::LBrace)
        });
    times.is_some_and(|times| at == first || at == times)
}

/// Whether the word at `tokens[at]` begins a statement that reads an operand
/// after it, as `exit STATUS` and `fail MESSAGE` do: first in a statement,
/// with the operand after a space on the same line and not an operator that
/// would make the word a name.
fn operand_statement_word(source: &str, tokens: &[Token], at: usize) -> bool {
    let token = tokens[at];
    statement_start(at.checked_sub(1).map(|before| tokens[before]), source)
        && tokens.get(at + 1).is_some_and(|next| {
            next.start > token.end
                && !matches!(
                    next.tag,
                    TokenTag::Newline
                        | TokenTag::Semicolon
                        | TokenTag::RBrace
                        | TokenTag::Comment
                        | TokenTag::Equals
                        | TokenTag::PipeGt
                        | TokenTag::Plus
                        | TokenTag::Star
                        | TokenTag::Slash
                        | TokenTag::Percent
                        | TokenTag::EqEq
                        | TokenTag::BangEq
                        | TokenTag::Lt
                        | TokenTag::Le
                        | TokenTag::Gt
                        | TokenTag::Ge
                        | TokenTag::QuestionQuestion
                )
        })
}

/// Whether the word at `tokens[at]` follows the keyword `else` with its
/// failure after it on the line, as in `guard COND else fail MESSAGE`. No
/// other statement puts a name after `else`.
fn guard_fail_word(source: &str, tokens: &[Token], at: usize) -> bool {
    at.checked_sub(1).is_some_and(|before| {
        let before = tokens[before];
        before.tag == TokenTag::Keyword && &source[before.start..before.end] == "else"
    }) && tokens.get(at + 1).is_some_and(|next| {
        !matches!(
            next.tag,
            TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Comment
        )
    })
}

/// Whether `tokens[at]` is the `because` of a statement that begins with the
/// word `fail` on the same line: outside every bracket the failure opens,
/// and not a field name.
fn fail_because_word(source: &str, tokens: &[Token], at: usize) -> bool {
    let text = |token: &Token| &source[token.start..token.end];
    let first = tokens[..at]
        .iter()
        .rposition(|token| matches!(token.tag, TokenTag::Newline | TokenTag::Semicolon))
        .map_or(0, |separator| separator + 1);
    // A match arm's statement follows `=>` on the line.
    let first = tokens[first..at]
        .iter()
        .rposition(|token| token.tag == TokenTag::FatArrow)
        .map_or(first, |arrow| first + arrow + 1);
    // So does the `fail` of a `guard`, after `else`.
    let first = tokens[first..at]
        .iter()
        .rposition(|token| token.tag == TokenTag::Keyword && text(token) == "else")
        .map_or(first, |otherwise| first + otherwise + 1);
    if at < first + 2
        || text(&tokens[first]) != "fail"
        || !(operand_statement_word(source, tokens, first)
            || guard_fail_word(source, tokens, first))
        || tokens[at - 1].tag == TokenTag::Dot
    {
        return false;
    }
    let mut depth = 0usize;
    for index in first + 1..at {
        let token = &tokens[index];
        match token.tag {
            TokenTag::LParen | TokenTag::LBracket | TokenTag::LBrace | TokenTag::DollarLBrace => {
                depth += 1
            }
            TokenTag::RParen | TokenTag::RBracket | TokenTag::RBrace => {
                let Some(outer) = depth.checked_sub(1) else {
                    return false;
                };
                depth = outer;
            }
            // An earlier `because` is the word, so this one is in the cause.
            TokenTag::Ident
                if depth == 0
                    && index > first + 1
                    && tokens[index - 1].tag != TokenTag::Dot
                    && text(token) == "because" =>
            {
                return false;
            }
            _ => {}
        }
    }
    depth == 0
}

/// Whether `tokens[at]` is the `without` of a statement head
/// `without EFFECT, ... {` written on one line.
fn without_head_word(source: &str, tokens: &[Token], at: usize, previous: Option<Token>) -> bool {
    use std::str::FromStr;
    if !statement_start(previous, source) {
        return false;
    }
    let mut index = at + 1;
    loop {
        let is_effect = tokens.get(index).is_some_and(|token| {
            token.tag == TokenTag::Ident
                && crate::syntax::node::Effect::from_str(&source[token.start..token.end]).is_ok()
        });
        if !is_effect {
            return false;
        }
        match tokens.get(index + 1).map(|token| token.tag) {
            Some(TokenTag::Comma) => index += 2,
            Some(TokenTag::LBrace) => return true,
            _ => return false,
        }
    }
}

/// Whether `tokens[at]` is the `tempdir` or the `at` of a scope that begins
/// `tempdir NAME at`. The scope opens wherever an expression may start, and
/// nothing else puts these three words in a row.
fn tempdir_head_word(source: &str, tokens: &[Token], at: usize) -> bool {
    let text = |token: &Token| &source[token.start..token.end];
    let is_head = |first: usize| {
        !first
            .checked_sub(1)
            .is_some_and(|before| tokens[before].tag == TokenTag::Dot)
            && matches!(
                tokens.get(first..first + 3),
                Some([keyword, name, word])
                    if text(keyword) == "tempdir"
                        && name.tag == TokenTag::Ident
                        && word.tag == TokenTag::Ident
                        && text(word) == "at"
            )
    };
    is_head(at) || at.checked_sub(2).is_some_and(is_head)
}

/// Whether `tokens[at]` is the `atomically`, the `replace`, or the `as` of a
/// statement that begins `atomically replace`. The `as` is the one directly
/// before the name and the `{` that end the head, on the line the statement
/// starts on.
fn atomically_head_word(source: &str, tokens: &[Token], at: usize) -> bool {
    let text = |token: &Token| &source[token.start..token.end];
    let is_head = |first: usize| {
        statement_start(first.checked_sub(1).map(|before| tokens[before]), source)
            && matches!(
                tokens.get(first..first + 2),
                Some([keyword, word])
                    if text(keyword) == "atomically"
                        && word.tag == TokenTag::Ident
                        && text(word) == "replace"
            )
    };
    if text(&tokens[at]) != "as" {
        return is_head(at) || at.checked_sub(1).is_some_and(is_head);
    }
    let ends_head = matches!(
        tokens.get(at + 1..at + 3),
        Some([name, brace]) if name.tag == TokenTag::Ident && brace.tag == TokenTag::LBrace
    );
    let line_start = tokens[..at]
        .iter()
        .rposition(|token| token.tag == TokenTag::Newline)
        .map_or(0, |newline| newline + 1);
    // The destination sits between `replace` and this word.
    ends_head && at >= line_start + 3 && is_head(line_start)
}

/// Whether `tokens[at]` is a word of the head of `wait until CONDITION within
/// LIMIT every INTERVAL` or of `retry backoff FIRST..CAP within LIMIT`:
/// the word directly after `wait` or `retry`, or a later word of that line
/// that a duration literal or a name follows.
fn paced_head_word(source: &str, tokens: &[Token], at: usize) -> bool {
    let text = |token: &Token| &source[token.start..token.end];
    let begins = |first: usize| {
        matches!(
            tokens.get(first..first + 2),
            Some([keyword, word])
                if word.tag == TokenTag::Ident
                    && matches!(
                        (text(keyword), text(word)),
                        ("wait", "until") | ("retry", "backoff")
                    )
        )
    };
    if at.checked_sub(1).is_some_and(begins) {
        return true;
    }
    if !matches!(
        tokens.get(at + 1).map(|token| token.tag),
        Some(TokenTag::Duration | TokenTag::Ident)
    ) {
        return false;
    }
    let line_start = tokens[..at]
        .iter()
        .rposition(|token| token.tag == TokenTag::Newline)
        .map_or(0, |newline| newline + 1);
    let head = (line_start..at).rev().find(|first| begins(*first));
    match head {
        // `every` belongs to `wait until` only.
        Some(first) => text(&tokens[at]) != "every" || text(&tokens[first]) == "wait",
        None => false,
    }
}

/// Whether `tokens[at]` is the `within` of `within DURATION {`.
fn within_head(tokens: &[Token], at: usize) -> bool {
    let tag = |index: usize| tokens.get(index).map(|token| token.tag);
    let mut next = match tag(at + 1) {
        Some(TokenTag::Duration) => at + 2,
        Some(TokenTag::Ident) => at + 2,
        _ => return false,
    };
    if tag(at + 1) == Some(TokenTag::Ident) {
        while tag(next) == Some(TokenTag::Dot) && tag(next + 1) == Some(TokenTag::Ident) {
            next += 2;
        }
    }
    tag(next) == Some(TokenTag::LBrace)
}

/// Whether the token after `previous` starts a statement.
fn statement_start(previous: Option<Token>, source: &str) -> bool {
    match previous {
        None => true,
        Some(previous) => match previous.tag {
            // A match arm's body is a statement.
            TokenTag::Newline | TokenTag::Semicolon | TokenTag::LBrace | TokenTag::FatArrow => true,
            TokenTag::Keyword => &source[previous.start..previous.end] == "export",
            _ => false,
        },
    }
}

/// Paints the program word of a `run` command as a function, after the
/// optional `.mode` and any `--flag` words, and returns its start so the
/// caller does not classify it again. A flag runs to the next space outside
/// brackets (`--accept=[0, 1]`).
fn paint_run_command(
    source: &str,
    base: usize,
    tokens: &[Token],
    mut index: usize,
    kinds: &mut [Kind],
) -> Option<usize> {
    if let [dot, mode, ..] = tokens.get(index..)?
        && dot.tag == TokenTag::Dot
        && (is_name(mode) || mode.tag == TokenTag::Keyword)
    {
        index += 2;
    }
    while tokens.get(index)?.tag == TokenTag::Minus {
        let mut depth = 0usize;
        let mut last_end = tokens[index].start;
        while let Some(token) = tokens.get(index) {
            if (depth == 0 && token.start != last_end) || token.tag == TokenTag::Newline {
                break;
            }
            match token.tag {
                TokenTag::LParen | TokenTag::LBracket | TokenTag::LBrace => depth += 1,
                TokenTag::RParen | TokenTag::RBracket | TokenTag::RBrace => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            last_end = token.end;
            index += 1;
        }
    }
    let word = *tokens.get(index)?;
    let called = tokens
        .get(index + 1)
        .is_some_and(|next| next.tag == TokenTag::LParen);
    (is_name(&word) && !called && !source.as_bytes()[word.start].is_ascii_uppercase()).then(|| {
        fill(kinds, base, word.start, word.end, Kind::Function);
        word.start
    })
}

/// Paints the braces and `:>N` specs of an f-string and highlights each
/// interpolated expression. Text, including `{{` and `}}` escapes, keeps the
/// literal's own kind.
fn paint_interpolations(source: &str, base: usize, token: Token, kinds: &mut [Kind]) {
    let Some(QuotedScan::Terminated(literal)) = scan_quoted_literal(source, token.start, false)
    else {
        return;
    };
    if literal.end != token.end
        || literal.raw
        || !matches!(
            literal.kind,
            QuotedLiteralKind::Fmt | QuotedLiteralKind::PathFmt
        )
    {
        return;
    }
    let (chunks, _) = fmt_chunks(source, literal);
    for chunk in chunks {
        let InterpolationChunk::Expr {
            source: expression,
            offset,
        } = chunk
        else {
            continue;
        };
        let code_len = spec_start(expression).unwrap_or(expression.len());
        fill(kinds, base, offset - 1, offset, Kind::Interpolation);
        fill(
            kinds,
            base,
            offset + code_len,
            offset + expression.len() + 1,
            Kind::Interpolation,
        );
        // The expression is code, so the literal's color must not show
        // through the spaces between its tokens.
        fill(kinds, base, offset, offset + code_len, Kind::Plain);
        paint(&expression[..code_len], base + offset, kinds);
    }
}

/// Where a `:` width spec starts in an interpolation body: the first colon
/// outside every bracket, since an expression never has a bare one.
fn spec_start(expression: &str) -> Option<usize> {
    let mut depth = 0usize;
    for token in lex(expression).0 {
        match token.tag {
            TokenTag::LParen | TokenTag::LBracket | TokenTag::LBrace | TokenTag::DollarLBrace => {
                depth += 1;
            }
            TokenTag::RParen | TokenTag::RBracket | TokenTag::RBrace => {
                depth = depth.saturating_sub(1);
            }
            TokenTag::Colon if depth == 0 => return Some(token.start),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Kind, Run, highlight};
    use std::path::Path;

    /// The `(kind, text)` pairs of `source`.
    fn runs(source: &str) -> Vec<(Kind, &str)> {
        highlight(source)
            .into_iter()
            .map(|run| (run.kind, &source[run.start..run.end]))
            .collect()
    }

    /// The kind of the first byte of the first occurrence of `text`.
    fn kind_of(source: &str, text: &str) -> Kind {
        let at = source
            .find(text)
            .unwrap_or_else(|| panic!("no `{text}` in {source:?}"));
        highlight(source)
            .into_iter()
            .find(|run| run.start <= at && at < run.end)
            .map(|run| run.kind)
            .expect("runs tile the source")
    }

    fn assert_tiles(source: &str, runs: &[Run]) {
        let mut end = 0;
        for run in runs {
            assert_eq!(run.start, end, "gap or overlap before {run:?}");
            assert!(run.end > run.start, "empty run {run:?}");
            end = run.end;
        }
        assert_eq!(end, source.len());
        for pair in runs.windows(2) {
            assert_ne!(pair[0].kind, pair[1].kind, "uncoalesced {pair:?}");
        }
    }

    fn corpus() -> Vec<std::path::PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        for dir in ["docs/snippets/tour", "tests/xsh", "tests/xsh/stdlib", "dev"] {
            for entry in std::fs::read_dir(root.join(dir)).expect("corpus directory") {
                let path = entry.expect("directory entry").path();
                if path.extension().is_some_and(|ext| ext == "xsh") {
                    files.push(path);
                }
            }
        }
        assert!(files.len() > 100, "corpus unexpectedly small");
        files
    }

    #[test]
    fn empty_source_has_no_runs() {
        assert!(highlight("").is_empty());
    }

    #[test]
    fn runs_tile_every_corpus_file() {
        for path in corpus() {
            let source = std::fs::read_to_string(&path).expect("utf-8 corpus file");
            assert_tiles(&source, &highlight(&source));
        }
    }

    #[test]
    fn runs_tile_malformed_source() {
        for source in [
            "let x = \"open",
            "print f\"{",
            "print f\"{a:>}\" ~ ü '",
            "a ${ b",
            "/",
            "run",
            "run --",
            "f\"{{}}\"",
            "# é",
        ] {
            assert_tiles(source, &highlight(source));
        }
    }

    #[test]
    fn every_kind_appears() {
        let source = "## doc\n# note\nlet n = 5s ?? null\nproc f(a: Path) {\n  run.text --accept=[0, 1] grep -c x ${a} $?\n  print f\"{n:>4}\" @xs\n}\nconst r = rx\"a\"\nconst p = /etc/hosts\nconst t = p\"x\"\nlet o = Ok(1).field\n";
        let seen: Vec<Kind> = highlight(source).iter().map(|run| run.kind).collect();
        for kind in Kind::ALL {
            assert!(seen.contains(&kind), "{} missing from {:?}", kind.name(), runs(source));
        }
    }

    #[test]
    fn continued_command_lines_keep_command_kinds() {
        let source = "run.text --timeout=5s \\\n  grep -c \\\n  $pattern \\\n  | run sort ?\nprint done\n";
        assert_eq!(kind_of(source, "\\"), Kind::Punctuation);
        assert_eq!(kind_of(source, "grep"), Kind::Function);
        // A word on a continued line is an argument, not the start of a
        // statement.
        assert_eq!(kind_of(source, "sort"), Kind::Function);
        assert_eq!(kind_of(source, "$pattern"), Kind::Variable);
        assert_eq!(kind_of(source, "print"), kind_of("print done\n", "print"));
        assert_eq!(
            kind_of("print one \\\n  two\n", "two"),
            kind_of("print one two\n", "two")
        );
    }

    #[test]
    fn kind_names_are_unique() {
        let mut names: Vec<_> = Kind::ALL.iter().map(|kind| kind.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Kind::ALL.len());
    }

    #[test]
    fn doc_comments_differ_from_comments() {
        assert_eq!(kind_of("# a\n## b\n##! c\n", "# a"), Kind::Comment);
        assert_eq!(kind_of("# a\n## b\n##! c\n", "## b"), Kind::DocComment);
        assert_eq!(kind_of("# a\n## b\n##! c\n", "##! c"), Kind::DocComment);
    }

    #[test]
    fn constants_are_not_keywords() {
        let source = "let a = true or false and null";
        assert_eq!(kind_of(source, "let"), Kind::Keyword);
        assert_eq!(kind_of(source, "or"), Kind::Keyword);
        for constant in ["true", "false", "null"] {
            assert_eq!(kind_of(source, constant), Kind::Constant);
        }
    }

    #[test]
    fn calls_properties_and_types_are_told_apart() {
        let source = "let v = Path.parse(x).len + conf.name() + point.x + Ok(y)\n";
        assert_eq!(kind_of(source, "Path"), Kind::Type);
        assert_eq!(kind_of(source, "parse"), Kind::Function);
        assert_eq!(kind_of(source, "len"), Kind::Property);
        assert_eq!(kind_of(source, "name"), Kind::Function);
        assert_eq!(kind_of(source, "x"), Kind::Plain);
        assert_eq!(kind_of(source, "Ok"), Kind::Type);
        assert_eq!(kind_of("a.b.x", "x"), Kind::Property);
    }

    #[test]
    fn declarations_name_functions_and_labels_are_properties() {
        let source = "proc backup(src: Path, dest: Path) [fs] {\n}\ncli main(root: Path) {\n}\ntest it_works {\n}\nerror E = A(file: Path) | B\nlet x: Int = 1\n";
        assert_eq!(kind_of(source, "backup"), Kind::Function);
        assert_eq!(kind_of(source, "src"), Kind::Property);
        assert_eq!(kind_of(source, "cli"), Kind::Keyword);
        assert_eq!(kind_of(source, "main"), Kind::Function);
        assert_eq!(kind_of(source, "test"), Kind::Keyword);
        assert_eq!(kind_of(source, "it_works"), Kind::Function);
        assert_eq!(kind_of(source, "error"), Kind::Keyword);
        assert_eq!(kind_of(source, "x"), Kind::Plain);
    }

    #[test]
    fn without_is_a_keyword_only_in_a_without_statement_head() {
        let source = "without net, fs {\n}\nlet without = [net]\nwithout = []\nprint ${without.len()}\n";
        assert_eq!(kind_of(source, "without net"), Kind::Keyword);
        assert_eq!(kind_of(source, "without = [net]"), Kind::Plain);
        assert_eq!(kind_of(source, "without = []"), Kind::Plain);
        assert_eq!(kind_of(source, "without.len"), Kind::Plain);
    }

    #[test]
    fn repeat_head_words_are_keywords_only_in_a_repeat_statement() {
        let source = "repeat n times {\n}\nlet times = xs |> repeat(count: 2)\nlet repeat = times\n";
        assert_eq!(kind_of(source, "repeat n"), Kind::Keyword);
        assert_eq!(kind_of(source, "times {"), Kind::Keyword);
        assert_eq!(kind_of(source, "times ="), Kind::Plain);
        assert_eq!(kind_of(source, "repeat(count"), Kind::Function);
        assert_eq!(kind_of(source, "repeat = times"), Kind::Plain);
    }

    #[test]
    fn fail_words_are_keywords_only_in_a_fail_statement() {
        let source = "fail \"no\" because problem\nlet because = test.fail(\"x\")\nfail = because\nfail f(because) because because.fail\n";
        assert_eq!(kind_of(source, "fail \"no\""), Kind::Keyword);
        assert_eq!(kind_of(source, "because problem"), Kind::Keyword);
        assert_eq!(kind_of(source, "because = test"), Kind::Plain);
        assert_eq!(kind_of(source, "fail(\"x\")"), Kind::Function);
        assert_eq!(kind_of(source, "fail = because"), Kind::Plain);
        assert_eq!(kind_of(source, "because\nfail f"), Kind::Plain);
        assert_eq!(kind_of(source, "because) because"), Kind::Plain);
        assert_eq!(kind_of(source, "because because.fail"), Kind::Keyword);
        assert_eq!(kind_of(source, "because.fail"), Kind::Plain);
    }

    #[test]
    fn fail_after_the_else_of_a_guard_is_the_statement() {
        let source = "guard ready else fail \"no\" because problem\nlet kept = if ready { fail } else { fail }\nguard ready else fail(\"x\")\n";
        assert_eq!(kind_of(source, "fail \"no\""), Kind::Keyword);
        assert_eq!(kind_of(source, "because problem"), Kind::Keyword);
        assert_eq!(kind_of(source, "fail } else"), Kind::Plain);
        assert_eq!(kind_of(source, "fail }\n"), Kind::Plain);
        assert_eq!(kind_of(source, "fail(\"x\")"), Kind::Keyword);
    }

    #[test]
    fn a_match_arm_statement_begins_with_its_statement_word() {
        let source = "match code {\n  0 => exit 3\n  1 => fail \"one\" because problem\n  _ => exit(code)\n}\n";
        assert_eq!(kind_of(source, "exit 3"), Kind::Keyword);
        assert_eq!(kind_of(source, "fail \"one\""), Kind::Keyword);
        assert_eq!(kind_of(source, "because problem"), Kind::Keyword);
        assert_eq!(kind_of(source, "exit(code)"), Kind::Function);
    }

    #[test]
    fn atomically_head_words_are_keywords_only_in_an_atomically_statement() {
        let source = "atomically replace as as replace {\n}\nlet atomically = replace\nlet replace = text.replace(a, b)\nuse a as b\n";
        assert_eq!(kind_of(source, "atomically replace"), Kind::Keyword);
        assert_eq!(kind_of(source, "replace as"), Kind::Keyword);
        assert_eq!(kind_of(source, "as as"), Kind::Plain);
        assert_eq!(kind_of(source, "as replace {"), Kind::Keyword);
        assert_eq!(kind_of(source, "replace {"), Kind::Plain);
        assert_eq!(kind_of(source, "atomically ="), Kind::Plain);
        assert_eq!(kind_of(source, "replace = text"), Kind::Plain);
    }

    #[test]
    fn tempdir_head_words_are_keywords_only_in_a_tempdir_head() {
        let source = "tempdir at at tempdir {\n}\nlet tempdir = at\nlet at = fs.tempdir()\n";
        assert_eq!(kind_of(source, "tempdir at"), Kind::Keyword);
        assert_eq!(kind_of(source, "at tempdir"), Kind::Keyword);
        let value = "let n = tempdir dir at root { 1 }?\n";
        assert_eq!(kind_of(value, "tempdir dir"), Kind::Keyword);
        assert_eq!(kind_of(value, "at root"), Kind::Keyword);
        assert_eq!(kind_of(source, "tempdir {"), Kind::Plain);
        assert_eq!(kind_of(source, "tempdir ="), Kind::Plain);
        assert_eq!(kind_of(source, "at = fs"), Kind::Plain);
    }

    #[test]
    fn contextual_names_stay_plain_elsewhere() {
        let source = "match r {\n  Err(error) => print error.message\n}\nlet test = 1\n";
        assert_eq!(kind_of(source, "error"), Kind::Plain);
        assert_eq!(kind_of(source, "test"), Kind::Plain);
    }

    #[test]
    fn scope_statements_are_keywords_but_the_env_module_is_not() {
        let source = "cd $dir {\n  env LC_ALL=C {\n    let x = env.get_or(\"A\", \"\")\n  }\n}\n";
        assert_eq!(kind_of(source, "cd"), Kind::Keyword);
        assert_eq!(kind_of(source, "env LC"), Kind::Keyword);
        assert_eq!(kind_of(source, "LC_ALL"), Kind::Property);
        assert_eq!(kind_of(source, "env.get_or"), Kind::Plain);
        assert_eq!(kind_of(source, "get_or"), Kind::Function);
    }

    #[test]
    fn the_words_of_a_paced_head_are_keywords_only_there() {
        let source = "wait until ready() within 5s every cfg.step\nlet r = retry backoff 1s..5s within limit { 1 }\nlet until = every + backoff\nwait (until)\n";
        assert_eq!(kind_of(source, "until ready"), Kind::Keyword);
        assert_eq!(kind_of(source, "within 5s"), Kind::Keyword);
        assert_eq!(kind_of(source, "every cfg"), Kind::Keyword);
        assert_eq!(kind_of(source, "backoff 1s"), Kind::Keyword);
        assert_eq!(kind_of(source, "within limit"), Kind::Keyword);
        assert_eq!(kind_of(source, "until = every"), Kind::Plain);
        assert_eq!(kind_of(source, "every + backoff"), Kind::Plain);
        assert_eq!(kind_of(source, "backoff\nwait"), Kind::Plain);
        assert_eq!(kind_of(source, "until)"), Kind::Plain);
    }

    #[test]
    fn within_is_a_keyword_only_before_a_duration_and_block() {
        let source = "let r = within 5s { 1 }\nwithin cfg.limit { }\nlet within = 2\nif within (x) { }\n";
        assert_eq!(kind_of(source, "within 5s"), Kind::Keyword);
        assert_eq!(kind_of(source, "within cfg"), Kind::Keyword);
        assert_eq!(kind_of(source, "within = 2"), Kind::Plain);
        assert_ne!(kind_of(source, "within (x)"), Kind::Keyword);
    }

    #[test]
    fn tempdir_is_a_keyword_only_before_a_name_and_block() {
        let source = "let n = tempdir dir { 1 }?\nlet tempdir = 2\n";
        assert_eq!(kind_of(source, "tempdir dir"), Kind::Keyword);
        assert_eq!(kind_of(source, "tempdir = 2"), Kind::Plain);
    }

    #[test]
    fn spaces_inside_an_interpolation_are_not_string() {
        let all = runs("f\"{a ?? 80}\"");
        assert!(all.contains(&(Kind::Plain, "a ")), "{all:?}");
        assert!(all.contains(&(Kind::Plain, " ")), "{all:?}");
    }

    #[test]
    fn use_alias_and_pipeline_stages() {
        let source = "use stage as stages\nlet a = xs |> sort-by .size |> take(3) |> collect\n";
        assert_eq!(kind_of(source, "as"), Kind::Keyword);
        assert_eq!(kind_of(source, "sort-by"), Kind::Function);
        assert_eq!(kind_of(source, "take"), Kind::Function);
        assert_eq!(kind_of(source, "collect"), Kind::Function);
    }

    #[test]
    fn bare_paths_are_paths_and_division_is_not() {
        let source = "const a = /etc/hosts\nconst b = ./x/../y\nlet c = n / 2\nlet d = m/2\nrun ls /var/log\n";
        assert_eq!(kind_of(source, "/etc/hosts"), Kind::Path);
        assert_eq!(kind_of(source, "./x/../y"), Kind::Path);
        assert_eq!(kind_of(source, "/var/log"), Kind::Path);
        assert!(runs(source).iter().filter(|(kind, _)| *kind == Kind::Path).count() == 3);
    }

    #[test]
    fn command_words_variables_and_splices() {
        let source = "run.text --accept=[0, 1] grep -c ERROR $log @flags ${x} $?\n";
        assert_eq!(kind_of(source, "grep"), Kind::Function);
        assert_eq!(kind_of(source, "$log"), Kind::Variable);
        assert_eq!(kind_of(source, "@flags"), Kind::Variable);
        assert_eq!(kind_of(source, "$?"), Kind::Variable);
        assert_eq!(kind_of(source, "text"), Kind::Property);
        let braces: Vec<_> = runs(source)
            .into_iter()
            .filter(|(kind, _)| *kind == Kind::Variable)
            .map(|(_, text)| text)
            .collect();
        assert!(braces.contains(&"${"), "{braces:?}");
    }

    #[test]
    fn fstring_interpolation_is_highlighted_as_code() {
        let source = "print f\"{n}: {entry.size:>12} {{x}}\"";
        assert_eq!(
            runs(source),
            vec![
                (Kind::Keyword, "print"),
                (Kind::Plain, " "),
                (Kind::String, "f\""),
                (Kind::Interpolation, "{"),
                (Kind::Plain, "n"),
                (Kind::Interpolation, "}"),
                (Kind::String, ": "),
                (Kind::Interpolation, "{"),
                (Kind::Plain, "entry"),
                (Kind::Punctuation, "."),
                (Kind::Property, "size"),
                (Kind::Interpolation, ":>12}"),
                (Kind::String, " {{x}}\""),
            ]
        );
    }

    #[test]
    fn nested_fstrings_recurse() {
        let source = "f\"good: {read_port(fp\"{dir}/good\")?}\"";
        let all = runs(source);
        assert_eq!(kind_of(source, "read_port"), Kind::Function);
        assert!(all.contains(&(Kind::Path, "fp\"")), "{all:?}");
        assert!(all.contains(&(Kind::Path, "/good\"")), "{all:?}");
        assert!(all.contains(&(Kind::Operator, "?")), "{all:?}");
        assert_eq!(all.last(), Some(&(Kind::String, "\"")));
    }

    #[test]
    fn spec_colon_ignores_colons_inside_brackets() {
        let source = "f\"{f(width: 4):05}\"";
        let all = runs(source);
        assert!(all.contains(&(Kind::Property, "width")), "{all:?}");
        assert!(all.contains(&(Kind::Interpolation, ":05}")), "{all:?}");
    }

    #[test]
    fn literals_have_their_own_kinds() {
        let source = "let a = [1, 2.5, 90s, \"s\", b\"x\", p\"/x\", g\"*.c\", rx\"^a\"]\n";
        assert_eq!(kind_of(source, "1"), Kind::Number);
        assert_eq!(kind_of(source, "2.5"), Kind::Number);
        assert_eq!(kind_of(source, "90s"), Kind::Number);
        assert_eq!(kind_of(source, "\"s\""), Kind::String);
        assert_eq!(kind_of(source, "b\"x\""), Kind::String);
        assert_eq!(kind_of(source, "p\"/x\""), Kind::Path);
        assert_eq!(kind_of(source, "g\"*.c\""), Kind::Path);
        assert_eq!(kind_of(source, "rx\"^a\""), Kind::Regex);
    }
}
