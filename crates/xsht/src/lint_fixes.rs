use super::{ArenaFunctionDef, ArenaProgram, AstArena, Diagnostic, Span};

/// Whether replacing `span` could drop a comment. Unlexable text counts as
/// commented so callers stay conservative.
pub(super) fn span_may_contain_comment(source: &str, span: Span) -> bool {
    let Some(text) = source.get(span.range()) else {
        return true;
    };
    let lexed = xsh::frontend::syntax::lexer::Lexer::new(span.source_id, text).lex_compact();
    !lexed.diagnostics.is_empty()
        || (0..lexed.token_table.len()).any(|index| {
            lexed.token_table.tag_at(index) == Some(xsh::frontend::syntax::token::TokenTag::Comment)
        })
}

/// Fixes build replacements with conservative grouping; removes each pair
/// that `check.redundant-parens` would reject where the fix lands, so applied
/// fixes keep exactly the parentheses the parser needs.
pub(super) fn minimize_fix_grouping(program: &ArenaProgram, source: &str, diagnostics: &mut [Diagnostic]) {
    let Some(source_id) = program
        .statement_ids()
        .next()
        .map(|id| program.arena.stmt(id).span.source_id)
    else {
        return;
    };
    // One fix's edits: the diagnostic, the hint, where it lands, and its text.
    let mut pending: Vec<Vec<GroupedEdit>> = Vec::new();
    for (diagnostic_index, diagnostic) in diagnostics.iter().enumerate() {
        if !diagnostic.fix_hints.iter().any(|hint| {
            hint.replacement
                .as_deref()
                .is_some_and(|text| text.contains('('))
        }) {
            continue;
        }
        let mut edits = Vec::new();
        for (hint_index, hint) in diagnostic.fix_hints.iter().enumerate() {
            let (Some(span), Some(replacement)) = (hint.span, hint.replacement.as_ref()) else {
                continue;
            };
            if span.source_id == source_id && source.get(span.range()).is_some() {
                edits.push(GroupedEdit {
                    diagnostic: diagnostic_index,
                    hint: hint_index,
                    span,
                    replacement: replacement.clone(),
                });
            }
        }
        edits.sort_by_key(|edit| (edit.span.start(), edit.span.end()));
        if edits
            .windows(2)
            .any(|pair| pair[1].span.start() < pair[0].span.end())
        {
            continue;
        }
        pending.push(edits);
    }
    // Judging a pair of parentheses takes a parse of the whole file with the
    // fix in place. Fixes that touch separate text are judged in one parse:
    // each sweep takes the fixes that stay clear of those already taken and
    // leaves the rest, such as a fix nested in another, for the next sweep.
    pending.sort_by_key(|edits| edits.first().map(|edit| edit.span.start()));
    while !pending.is_empty() {
        let mut taken: Vec<GroupedEdit> = Vec::new();
        let mut taken_fixes = 0;
        let mut rest = Vec::new();
        for edits in pending {
            let clear = edits.iter().all(|edit| {
                taken.iter().all(|other| {
                    edit.span.start() > other.span.end() || other.span.start() > edit.span.end()
                })
            });
            if clear {
                taken.extend(edits);
                taken_fixes += 1;
            } else {
                rest.push(edits);
            }
        }
        pending = rest;
        taken.sort_by_key(|edit| edit.span.start());
        if minimize_grouped_edits(source, &taken, diagnostics) || taken_fixes == 1 {
            continue;
        }
        // One of these fixes leaves text that does not parse, which hides
        // the others' grouping; judge each fix alone.
        let mut alone: Vec<Vec<GroupedEdit>> = Vec::new();
        for edit in taken {
            match alone.last_mut() {
                Some(fix) if fix[0].diagnostic == edit.diagnostic => fix.push(edit),
                _ => alone.push(vec![edit]),
            }
        }
        for fix in &alone {
            minimize_grouped_edits(source, fix, diagnostics);
        }
    }
}

/// One edit of a fix whose grouping `minimize_fix_grouping` judges.
pub(super) struct GroupedEdit {
    diagnostic: usize,
    hint: usize,
    span: Span,
    replacement: String,
}

/// Applies `edits`, which are in source order and clear of each other, drops
/// the redundant parentheses inside them, and stores each minimal
/// replacement. Reports whether the edited text parsed.
pub(super) fn minimize_grouped_edits(
    source: &str,
    edits: &[GroupedEdit],
    diagnostics: &mut [Diagnostic],
) -> bool {
    let mut text = String::with_capacity(source.len());
    let mut ranges = Vec::with_capacity(edits.len());
    let mut cursor = 0;
    for edit in edits {
        text.push_str(&source[cursor..edit.span.start()]);
        ranges.push(text.len()..text.len() + edit.replacement.len());
        text.push_str(&edit.replacement);
        cursor = edit.span.end();
    }
    text.push_str(&source[cursor..]);
    let Some((minimal, ranges)) =
        xsh::frontend::syntax::grouping::try_remove_redundant_parens(&text, &ranges)
    else {
        return false;
    };
    for (edit, range) in edits.iter().zip(ranges) {
        diagnostics[edit.diagnostic].fix_hints[edit.hint].replacement =
            Some(minimal[range].to_owned());
    }
    true
}

/// Widens `span` over grouping parentheses that enclose exactly it, so an edit
/// that yields an atomic literal does not depend on redundant source grouping.
/// Call and index parentheses, and groups holding comments, are never widened.
pub(super) fn widen_over_grouping(source: &str, mut span: Span) -> Span {
    use xsh::frontend::syntax::token::TokenTag;
    let tokens = xsh::frontend::syntax::lexer::Lexer::new(span.source_id, source)
        .lex_compact()
        .token_table;
    let tag = |index: usize| tokens.tag_at(index);
    loop {
        let Some(first) =
            (0..tokens.len()).find(|&index| tokens.start_at(index) == Some(span.start()))
        else {
            return span;
        };
        let Some(last) =
            (first..tokens.len()).find(|&index| tokens.end_at(index, source) == Some(span.end()))
        else {
            return span;
        };
        let Some(open) = (0..first)
            .rev()
            .find(|&index| tag(index) != Some(TokenTag::Newline))
        else {
            return span;
        };
        let Some(close) =
            (last + 1..tokens.len()).find(|&index| tag(index) != Some(TokenTag::Newline))
        else {
            return span;
        };
        if tag(open) != Some(TokenTag::LParen) || tag(close) != Some(TokenTag::RParen) {
            return span;
        }
        let callee = (0..open)
            .rev()
            .find(|&index| tag(index) != Some(TokenTag::Newline))
            .and_then(tag);
        if matches!(
            callee,
            Some(
                TokenTag::Ident
                    | TokenTag::ProcIdent
                    | TokenTag::DollarIdent
                    | TokenTag::RParen
                    | TokenTag::RBracket
                    | TokenTag::RBrace
                    | TokenTag::Question
                    | TokenTag::String
                    | TokenTag::FmtString
                    | TokenTag::PathString
                    | TokenTag::PathFmtString
            )
        ) {
            return span;
        }
        let (Some(start), Some(end)) = (tokens.start_at(open), tokens.end_at(close, source)) else {
            return span;
        };
        span = Span::new(span.source_id, start, end);
    }
}

/// The span of a declaration's `[effects]` clause: the bracket that directly
/// follows the closing parenthesis of the parameter list. Scanning tokens
/// keeps comments and bracketed parameter types or defaults from being
/// mistaken for the clause.
pub(super) fn scan_effect_list_span(
    arena: &AstArena,
    def: &ArenaFunctionDef,
    stmt_span: Span,
    source: &str,
) -> Option<Span> {
    use xsh::frontend::syntax::token::TokenTag;
    let base = stmt_span.start();
    let signature = source.get(base..arena.span(arena.block(def.body).span).start())?;
    let tokens = xsh::frontend::syntax::lexer::Lexer::new(stmt_span.source_id, signature)
        .lex_compact()
        .token_table;
    let mut depth = 0usize;
    let mut parameters_closed = false;
    let mut open = None;
    for index in 0..tokens.len() {
        let tag = tokens.tag_at(index)?;
        if let Some(open) = open {
            match tag {
                TokenTag::RBracket => {
                    return Some(Span::new(
                        stmt_span.source_id,
                        base + open,
                        base + tokens.end_at(index, signature)?,
                    ));
                }
                _ => continue,
            }
        }
        if matches!(tag, TokenTag::Comment | TokenTag::Newline) {
            continue;
        }
        if parameters_closed {
            if tag != TokenTag::LBracket {
                return None;
            }
            open = Some(tokens.start_at(index)?);
            continue;
        }
        match tag {
            TokenTag::LParen => depth += 1,
            TokenTag::RParen => {
                depth = depth.checked_sub(1)?;
                parameters_closed = depth == 0;
            }
            _ => {}
        }
    }
    None
}

/// Scan backward from `ty_start` past whitespace and the `->` arrow to find the
/// start of the ` -> TypeName` annotation so it can be deleted in one span.
pub(super) fn scan_before_arrow(source: &str, ty_start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = ty_start;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    if i >= 2 && bytes[i - 2] == b'-' && bytes[i - 1] == b'>' {
        i -= 2;
        while i > 0 && bytes[i - 1] == b' ' {
            i -= 1;
        }
    }
    i
}

pub(super) fn shift_after_deletion(offset: usize, deletion: Span) -> usize {
    if offset >= deletion.end() {
        offset - deletion.range().len()
    } else {
        offset
    }
}

pub(super) fn scan_before_colon(source: &str, ty_start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = ty_start;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    if i > 0 && bytes[i - 1] == b':' {
        i -= 1;
        while i > 0 && bytes[i - 1] == b' ' {
            i -= 1;
        }
    }
    i
}

pub(super) fn scan_after_type(_source: &str, ty_end: usize) -> usize {
    ty_end
}

pub(super) fn scan_run_propagate_deletion_span(source: &str, run_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut end = run_span.end();
    while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'?' {
        end += 1;
        Span::new(run_span.source_id, run_span.end(), end)
    } else {
        Span::new(run_span.source_id, run_span.end(), run_span.end())
    }
}

pub(super) fn span_end_after_following_newlines(source: &str, mut end: usize) -> usize {
    let bytes = source.as_bytes();
    while end < bytes.len() && matches!(bytes[end], b'\r' | b'\n') {
        end += 1;
    }
    end
}

/// Compute a deletion span for a bare `return` statement covering the full source
/// line: leading indentation, the keyword, and the trailing newline.
pub(super) fn scan_return_stmt_span(source: &str, stmt_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut start = stmt_span.start();
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    let mut end = stmt_span.end();
    while end < bytes.len() && bytes[end] == b' ' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'\r' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'\n' {
        end += 1;
    }
    Span::new(stmt_span.source_id, start, end)
}

/// Scan backward from `pos` over any spaces, to include the separator whitespace
/// before a token in its deletion span.
pub(super) fn scan_back_space(source: &str, pos: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = pos;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    i
}

pub(super) fn scan_pipe_stage_deletion_span(source: &str, stage_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut start = stage_span.start();
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    if start >= 2 && bytes[start - 2] == b'|' && bytes[start - 1] == b'>' {
        start -= 2;
        while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
            start -= 1;
        }
    }
    Span::new(stage_span.source_id, start, stage_span.end())
}

/// An unbraced match arm ends at a comma, which `assert` would read as its
/// message separator, so such an arm becomes a braced block.
pub(super) fn assert_statement(condition: &str, comma_terminated: bool) -> String {
    if comma_terminated {
        format!("{{ assert {condition} }}")
    } else {
        format!("assert {condition}")
    }
}

/// Whether the statement whose expression ends at `end` is terminated by a
/// comma (an unbraced match arm).
pub(super) fn comma_terminated(source: &str, end: usize) -> bool {
    source
        .get(end..)
        .is_some_and(|rest| rest.trim_start_matches([' ', '\t']).starts_with(','))
}
