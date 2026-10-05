use crate::xsht::format::Formatter;
use xsh::diagnostic::{DiagnosticCode, DiagnosticRenderer};
use xsh::frontend::source::{SourceMap, Span};
use xsh::frontend::syntax::cst::TriviaKind;
use xsh::frontend::syntax::lexer::Lexer;
use xsh::frontend::syntax::parser::Parser;
use xsh::frontend::syntax::token::TokenTag;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// The lint code that reports a parser or checker migration diagnostic, whose
/// fix `xsht lint` applies as an ordinary lint fix.
pub(crate) fn migration_lint_code(code: Option<DiagnosticCode>) -> Option<DiagnosticCode> {
    match code? {
        DiagnosticCode::ParseBlockHeaderMigration => Some(DiagnosticCode::LintBlockHeader),
        DiagnosticCode::ParseStreamOptionMigration => Some(DiagnosticCode::LintStreamOptions),
        DiagnosticCode::ParseEnumMigration => Some(DiagnosticCode::LintEnumDeclaration),
        DiagnosticCode::CheckRemovedRecordRequire => Some(DiagnosticCode::LintRemovedRecordRequire),
        DiagnosticCode::ParseCompatibilityVocabulary
        | DiagnosticCode::CheckCompatibilityVocabulary => {
            Some(DiagnosticCode::LintCompatibilityVocabulary)
        }
        DiagnosticCode::ParseEnvScopeMigration => Some(DiagnosticCode::LintEnvScope),
        DiagnosticCode::CheckPublicResultError => Some(DiagnosticCode::LintPublicResultError),
        DiagnosticCode::CheckPositionalErrorArguments => {
            Some(DiagnosticCode::LintPositionalErrorArguments)
        }
        _ => None,
    }
}

pub(crate) fn apply_cst_guarded_edits(
    file: &str,
    text: &str,
    edits: &[SourceEdit],
    line_width: usize,
) -> Result<Option<String>, String> {
    apply_cst_edits(file, text, edits, Some(line_width))
}

/// Removed vocabulary and `xsht lint --only` fixes change exact tokens.
/// Preserve unrelated literal bytes, line endings and layout while the caller
/// rechecks the result.
pub(crate) fn apply_cst_guarded_migration_edits(
    file: &str,
    text: &str,
    edits: &[SourceEdit],
) -> Result<Option<String>, String> {
    apply_cst_edits(file, text, edits, None)
}

/// Whether replacing `span` loses none of its comments. A replacement that
/// spans statements (a declaration through its last use) carries the text
/// between its changes along, comments included, and is applied; one that
/// would drop or reword a comment is not.
fn replacement_keeps_comments(
    cst: &xsh::frontend::syntax::cst::SyntaxTree,
    text: &str,
    span: Span,
    replacement: &str,
) -> bool {
    let comments = cst
        .trivia_in_span(span)
        .into_iter()
        .map(|id| cst.trivia(id))
        .filter(|trivia| trivia.kind == TriviaKind::Comment)
        .map(|trivia| text.get(trivia.span.range()))
        .collect::<Option<Vec<_>>>();
    let Some(comments) = comments else {
        return false;
    };
    if comments.is_empty() {
        return true;
    }
    // The replacement is a fragment, so it is read as tokens only, and has to
    // read cleanly for its comments to be known.
    let lexed = Lexer::new(span.source_id, replacement).lex_compact();
    if !lexed.diagnostics.is_empty() {
        return false;
    }
    let kept = (0..lexed.token_table.len())
        .filter(|index| lexed.token_table.tag_at(*index) == Some(TokenTag::Comment))
        .map(|index| {
            let token = lexed
                .token_table
                .span_at(index, span.source_id, replacement)?;
            replacement.get(token.range())
        })
        .collect::<Option<Vec<_>>>();
    kept.is_some_and(|kept| kept == comments)
}

fn apply_cst_edits(
    file: &str,
    text: &str,
    edits: &[SourceEdit],
    line_width: Option<usize>,
) -> Result<Option<String>, String> {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(file, text);
    let parsed = Parser::parse_source_arena_only(source_id, text);
    // Only exact diagnosed migration edits may repair this rejected syntax. All
    // other parser failures remain errors, and rewritten source parses normally.
    let migrating_syntax = !parsed.diagnostics.is_empty()
        && parsed.diagnostics.iter().all(|diagnostic| {
            migration_lint_code(diagnostic.code).is_some()
                && diagnostic.fix_hints.iter().any(|hint| {
                    hint.span.is_some_and(|span| {
                        edits.iter().any(|edit| {
                            edit.start == span.start()
                                && edit.end == span.end()
                                && hint.replacement.as_ref() == Some(&edit.replacement)
                        })
                    })
                })
        });
    if !parsed.diagnostics.is_empty() && !migrating_syntax {
        return Err(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources));
    }

    let mut applied = false;
    let mut rewritten = text.to_string();
    // Lint and check fixes arrive as separate lists; apply the earliest of
    // overlapping edits and leave the rest for the next round.
    let mut ordered = edits.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|edit| (edit.start, std::cmp::Reverse(edit.end)));
    let mut kept_end = 0;
    ordered.retain(|edit| {
        let keep = edit.start >= kept_end;
        if keep {
            kept_end = edit.end.max(edit.start + 1);
        }
        keep
    });
    for edit in ordered.into_iter().rev() {
        if edit.start > edit.end
            || edit.end > text.len()
            || !text.is_char_boundary(edit.start)
            || !text.is_char_boundary(edit.end)
        {
            continue;
        }
        let span = Span::new(source_id, edit.start, edit.end);
        if !replacement_keeps_comments(parsed.cst.get(), text, span, &edit.replacement) {
            continue;
        }
        rewritten.replace_range(edit.start..edit.end, &edit.replacement);
        applied = true;
    }

    if !applied {
        return Ok(None);
    }

    if migrating_syntax || line_width.is_none() {
        let rewritten_parse = Parser::parse_source_arena_only(source_id, &rewritten);
        if !rewritten_parse.diagnostics.is_empty() {
            return Err(DiagnosticRenderer::new().render(&rewritten_parse.diagnostics, &sources));
        }
        return Ok(Some(rewritten));
    }

    let formatted = Formatter::new()
        .with_line_width(line_width.expect("formatted edit width"))
        .format_source(source_id, &rewritten);
    if !formatted.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new().render(&formatted.diagnostics, &sources));
    }
    Ok(Some(formatted.formatted))
}
