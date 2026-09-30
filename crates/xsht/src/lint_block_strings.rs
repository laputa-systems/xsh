use xsh::diagnostic::{Diagnostic, FixHint, Label};
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::arena::{ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaStmtKind, AstArena, ExprId};
use xsh::frontend::syntax::lexer::Lexer;
use xsh::frontend::syntax::node::FormatSpec;
use xsh::frontend::syntax::parser::Parser;
use xsh::frontend::syntax::token::TokenTag;

pub(super) fn lint_formatted_block_string(arena: &AstArena, source: &str, expr: ExprId) -> Option<Diagnostic> {
    let expression = arena.expr(expr);
    let ArenaExprKind::FmtString(parts) = expression.kind else { return None; };
    let original = source.get(expression.span.range())?;
    if !original.starts_with("f\"") || original.starts_with("f\"\"\"") || !original.ends_with('"')
        || !original.contains("\\n") || !arena.fmt_parts(parts).any(|part| match part {
            ArenaFmtPart::Text(text) => arena.text_value(&text, source).is_some_and(|text| text.contains('\n')),
            ArenaFmtPart::Expr(..) => false,
        })
    { return None; }
    let mut diagnostic = Diagnostic::warning("a formatted escaped-newline literal can use a block string")
        .with_code("lint.prefer-block-string")
        .with_label(Label::secondary(expression.span, "retain each interpolation and its literal text in source order"));
    let line_start = source[..expression.span.start()].rfind(['\r', '\n']).map_or(0, |offset| offset + 1);
    let prefix = &source[line_start..expression.span.start()];
    let suffix = source[expression.span.end()..].split(['\r', '\n']).next().unwrap_or("");
    if prefix.trim_end().ends_with('+') || !suffix.bytes().all(|byte| matches!(byte, b' ' | b'\t')) {
        return Some(diagnostic.with_note("a closing block delimiter needs its own line; concatenation, trailing comments, and expression consumers remain unchanged"));
    }
    let indent: String = prefix.chars().take_while(|ch| matches!(ch, ' ' | '\t')).collect();
    let margin = format!("{indent}  ");
    let line_end = source[expression.span.end()..].find('\n').map(|offset| expression.span.end() + offset);
    let structural_break = if line_end.is_some_and(|offset| offset > 0 && source.as_bytes()[offset - 1] == b'\r') { "\r\n" } else { "\n" };
    let body = &original[2..original.len() - 1];
    let interpolations = arena.fmt_parts(parts).filter_map(|part| match part {
        ArenaFmtPart::Expr(expr, _) => Some(arena.expr(expr).span),
        ArenaFmtPart::Text(_) => None,
    }).collect::<Vec<_>>();
    let Some(content) = expand_literal_newlines(body, expression.span.start() + 2, &interpolations, &margin, expression.span.source_id) else {
        return Some(diagnostic.with_note("the interpolation source boundaries could not be retained exactly"));
    };
    let replacement = format!("f\"\"\"{structural_break}{margin}{content}{structural_break}{margin}\"\"\"");
    // Block layout may split one cooked text part into adjacent parts. Compare
    // their concatenated bytes and every unchanged interpolation source/spec.
    let witness = format!("let migrated = {replacement}\n");
    let parsed = Parser::parse_source_arena_only(expression.span.source_id, &witness);
    let candidate = parsed.arena.statement_ids().next().and_then(|statement| match parsed.arena.arena.stmt(statement).kind {
        ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(value), .. } => Some(value),
        _ => None,
    });
    if !parsed.diagnostics.is_empty() || candidate.and_then(|candidate| normalized_parts(&parsed.arena.arena, &witness, candidate)) != normalized_parts(arena, source, expr) {
        return Some(diagnostic.with_note("the block literal did not reproduce the original decoded text and interpolation sequence"));
    }
    let mut rewritten = source.to_owned();
    rewritten.replace_range(expression.span.range(), &replacement);
    if !Parser::parse_source_arena_only(expression.span.source_id, &rewritten).diagnostics.is_empty() {
        return Some(diagnostic.with_note("the surrounding source cannot retain this literal as a block"));
    }
    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(expression.span, "preserve literal bytes and interpolation evaluation order", replacement));
    Some(diagnostic)
}

fn expand_literal_newlines(body: &str, offset: usize, interpolations: &[xsh::frontend::source::Span], margin: &str, source_id: SourceId) -> Option<String> {
    let mut output = String::new();
    let mut position = 0;
    let mut interpolation = 0;
    let bytes = body.as_bytes();
    while position < body.len() {
        match bytes[position] {
            b'\\' if bytes.get(position + 1) == Some(&b'r') && bytes.get(position + 2..position + 4) == Some(b"\\n") => {
                output.push_str("\r\n"); output.push_str(margin); position += 4;
            }
            b'\\' if bytes.get(position + 1) == Some(&b'n') => {
                output.push('\n'); output.push_str(margin); position += 2;
            }
            b'\\' => {
                let escaped = body.get(position + 1..)?.chars().next()?;
                let end = position + 1 + escaped.len_utf8();
                output.push_str(&body[position..end]); position = end;
            }
            b'$' if bytes.get(position + 1) == Some(&b'{') => {
                let payload = &body[position + 2..];
                // The lexer already owns nested quoted literals and comments;
                // only brace depth is needed to retain the enclosing payload.
                let tokens = Lexer::new(source_id, payload).lex_compact().token_table;
                let mut depth = 0;
                let close = (0..tokens.len()).find_map(|index| match tokens.tag_at(index)? {
                    TokenTag::LBrace | TokenTag::DollarLBrace => { depth += 1; None }
                    TokenTag::RBrace if depth == 0 => tokens.start_at(index),
                    TokenTag::RBrace => { depth -= 1; None }
                    _ => None,
                })?;
                let end = position + 2 + close + 1;
                let expression = *interpolations.get(interpolation)?;
                if expression.start() < offset + position + 2 || expression.end() > offset + end - 1 { return None; }
                output.push_str(&body[position..end]); position = end; interpolation += 1;
            }
            b'$' if bytes.get(position + 1).is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_') => {
                let expression = *interpolations.get(interpolation)?;
                if expression.start() != offset + position + 1 { return None; }
                let end = expression.end().checked_sub(offset)?;
                output.push_str(body.get(position..end)?); position = end; interpolation += 1;
            }
            _ => {
                let ch = body[position..].chars().next()?;
                output.push(ch); position += ch.len_utf8();
            }
        }
    }
    (interpolation == interpolations.len()).then_some(output)
}

#[derive(Debug, Eq, PartialEq)]
enum LiteralPart {
    Text(String),
    Interpolation(String, Option<FormatSpec>),
}

fn normalized_parts(arena: &AstArena, source: &str, expr: ExprId) -> Option<Vec<LiteralPart>> {
    let ArenaExprKind::FmtString(parts) = arena.expr(expr).kind else { return None; };
    let mut result = Vec::new();
    for part in arena.fmt_parts(parts) {
        match part {
            ArenaFmtPart::Text(text) => {
                let value = arena.text_value(&text, source)?;
                if value.is_empty() { continue; }
                if let Some(LiteralPart::Text(previous)) = result.last_mut() { previous.push_str(value); }
                else { result.push(LiteralPart::Text(value.to_owned())); }
            }
            ArenaFmtPart::Expr(expr, spec) => result.push(LiteralPart::Interpolation(source.get(arena.expr(expr).span.range())?.to_owned(), spec)),
        }
    }
    Some(result)
}

#[cfg(test)]
#[path = "lint_block_strings_tests.rs"]
mod tests;
