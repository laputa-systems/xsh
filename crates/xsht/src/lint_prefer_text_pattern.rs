use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, StmtId,
};
use xsh::frontend::syntax::token::TokenTag;

/// A token of a statement: its tag and its text.
type Token<'a> = (TokenTag, &'a str, usize);

fn tokens(source: &str, span: Span) -> Vec<Token<'_>> {
    let Some(text) = source.get(span.range()) else {
        return Vec::new();
    };
    let table = xsh::frontend::syntax::lexer::Lexer::new(span.source_id, text)
        .lex_compact()
        .token_table;
    (0..table.len())
        .filter_map(|index| {
            let start = table.start_at(index)?;
            let end = table.end_at(index, text)?;
            let tag = table.tag_at(index)?;
            (tag != TokenTag::Eof).then_some((tag, text.get(start..end)?, span.start() + start))
        })
        .collect()
}

/// The text of a plain `"..."` literal that holds no escape, so its bytes
/// are the ones written.
fn plain_literal<'a>(token: &Token<'a>) -> Option<&'a str> {
    let (tag, text, _) = *token;
    (tag == TokenTag::String && !text.contains('\\'))
        .then(|| text.strip_prefix('"')?.strip_suffix('"'))
        .flatten()
        .filter(|inner| !inner.is_empty() && !inner.contains(['"', '{', '}']))
}

/// Two shapes that take text apart by position, where a text pattern names
/// the pieces instead.
///
/// The first is a `let parts = text.split("SEP")` whose pieces later
/// statements of the block read as `parts[0]`, `parts[1]`: the pattern
/// `f"{a}SEP{b}"` binds them and fails to match where an index would be out
/// of range. The second is `text.starts_with("PREFIX")` in a statement that
/// also slices `text` at the prefix's length, which `f"PREFIX{rest}"` says
/// once.
///
/// Neither is rewritten. A pattern matches the whole text and its last hole
/// takes the rest, so `f"{a}:{b}"` keeps a second `:` in `b` where `split`
/// would make a third piece; which the author means is not in the code.
pub(super) fn lint_positional_text(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    for (index, stmt) in stmts.iter().enumerate() {
        if let Some(diagnostic) = split_read_by_position(linter, stmts, index) {
            linter.diagnostics.push(diagnostic);
        }
        let span = linter.arena.stmt(*stmt).span;
        linter
            .diagnostics
            .extend(prefix_test_with_slice(linter.source, span));
    }
}

fn split_read_by_position(
    linter: &super::Linter<'_>,
    stmts: &[StmtId],
    index: usize,
) -> Option<Diagnostic> {
    let arena = linter.arena;
    let stmt = arena.stmt(stmts[index]);
    let ArenaStmtKind::Let {
        target,
        initializer: ArenaExprOrRun::Expr(initializer),
        ..
    } = stmt.kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
        return None;
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(initializer).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name: method } = arena.expr(callee).kind else {
        return None;
    };
    if method.as_str().as_str() != "split"
        || linter.expr_types.get(&arena.expr(base).span) != Some(&Type::Str)
    {
        return None;
    }
    let [separator] = arena.call_args(args) else {
        return None;
    };
    let ArenaCallArgKind::Positional(separator) = separator.kind else {
        return None;
    };
    let separator_span = arena.expr(separator).span;
    let separator = tokens(linter.source, separator_span);
    let [separator] = separator.as_slice() else {
        return None;
    };
    let separator = plain_literal(separator)?;
    // The pieces read by a literal position in the rest of the block.
    let name = name.as_str();
    let mut positions = Vec::new();
    for later in &stmts[index + 1..] {
        let later = tokens(linter.source, arena.stmt(*later).span);
        for window in later.windows(4) {
            if let [(TokenTag::Ident, word, _), (TokenTag::LBracket, ..), (TokenTag::Int, position, _), (TokenTag::RBracket, ..)] =
                window
                && *word == name.as_str()
                && let Ok(position) = position.parse::<usize>()
                && !positions.contains(&position)
            {
                positions.push(position);
            }
        }
    }
    if positions.len() < 2 {
        return None;
    }
    let pieces = positions.iter().max()? + 1;
    let holes = (0..pieces)
        .map(|piece| format!("{{{}}}", (b'a' + (piece % 26) as u8) as char))
        .collect::<Vec<_>>()
        .join(separator);
    Some(
        Diagnostic::new(
            Severity::Warning,
            "pieces of a split are read by position; a text pattern names them",
        )
        .with_code(DiagnosticCode::LintPreferTextPattern)
        .with_label(Label::secondary(
            stmt.span,
            format!("`{name}[N]` is read below; consider `if let f\"{holes}\" = ...`"),
        ))
        .with_note(
            "the pattern must match the whole text, and its last hole takes the rest, including any further separator",
        ),
    )
}

fn prefix_test_with_slice(source: &str, span: Span) -> Vec<Diagnostic> {
    let tokens = tokens(source, span);
    let mut diagnostics = Vec::new();
    // A statement nested in this one is visited with its own block, so only
    // a test outside every brace of this statement is this statement's.
    let mut depth = 0usize;
    for (at, window) in tokens.windows(6).enumerate() {
        match window[0].0 {
            TokenTag::LBrace | TokenTag::DollarLBrace => depth += 1,
            TokenTag::RBrace => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth != 0 {
            continue;
        }
        let [(TokenTag::Ident, subject, start), (TokenTag::Dot, ..), (TokenTag::Ident, "starts_with", _), (TokenTag::LParen, ..), literal, (TokenTag::RParen, _, end)] =
            window
        else {
            continue;
        };
        // A name that is itself a field, as in `entry.name.starts_with`, is
        // not the whole subject.
        if at > 0 && tokens[at - 1].0 == TokenTag::Dot {
            continue;
        }
        let Some(prefix) = plain_literal(literal) else {
            continue;
        };
        let sliced = tokens[at + 6..].windows(5).any(|later| {
            matches!(
                later,
                [(TokenTag::Ident, word, _), (TokenTag::Dot, ..), (TokenTag::Ident, "byte_slice", _), (TokenTag::LParen, ..), (TokenTag::Int, offset, _)]
                    if word == subject && offset.parse::<usize>() == Ok(prefix.len())
            )
        });
        if !sliced {
            continue;
        }
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "a prefix test and a slice at its length say one thing twice; a text pattern says it once",
            )
            .with_code(DiagnosticCode::LintPreferTextPattern)
            .with_label(Label::secondary(
                Span::new(span.source_id, *start, end + 1),
                format!(
                    "`{subject}.byte_slice({}, ...)` follows; consider `if let f\"{prefix}{{rest}}\" = {subject}`",
                    prefix.len()
                ),
            )),
        );
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn notes(source: &str, enabled: bool) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                prefer_text_pattern: enabled,
                ..LintOptions::default()
            },
        )
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferTextPattern))
        .collect()
    }

    const SHAPES: &str = "pure probe(line: Str) -> Str {\n  let parts = line.split(\"=\")\n  let key = parts[0]\n  let value = parts[1]\n  if line.starts_with(\"#define \") {\n    return line.byte_slice(8, line.byte_len() - 8)\n  }\n\n  key + value\n}\n";

    #[test]
    fn positional_text_shapes_are_noted_without_a_fix() {
        let diagnostics = notes(SHAPES, true);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.fix_hints.is_empty())
        );
        let labels: Vec<_> = diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.labels[0].message.clone())
            .collect();
        assert!(labels.iter().any(|label| label.contains("f\"{a}={b}\"")), "{labels:?}");
        assert!(
            labels
                .iter()
                .any(|label| label.contains("f\"#define {rest}\" = line")),
            "{labels:?}"
        );
    }

    #[test]
    fn the_rule_is_opt_in() {
        assert!(notes(SHAPES, false).is_empty());
    }

    /// One piece read, a computed position, a separator with an escape, a
    /// list that is not a split, and a slice at another offset are left
    /// alone.
    #[test]
    fn other_shapes_are_left_alone() {
        let source = "pure probe(line: Str, at: Int, items: List[Str]) -> Str {\n  let first = line.split(\",\")\n  let head = first[0]\n  let second = line.split(\":\")\n  let pick = second[at] + second[at + 1]\n  let third = line.split(\"\\t\")\n  let tabbed = third[0] + third[1]\n  let pair = items[0] + items[1]\n  if line.starts_with(\"ab\") {\n    return line.byte_slice(3, 1)\n  }\n\n  head + pick + tabbed + pair\n}\n";
        assert!(notes(source, true).is_empty(), "{:?}", notes(source, true));
    }
}
