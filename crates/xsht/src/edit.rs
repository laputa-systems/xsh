use crate::xsht::format::Formatter;
use xsh::diagnostic::DiagnosticRenderer;
use xsh::frontend::source::{SourceMap, Span};
use xsh::frontend::syntax::parser::Parser;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

pub(crate) fn migration_lint_code(code: Option<&str>) -> Option<&'static str> {
    match code {
        Some("parse.block-header-migration") => Some("lint.block-header"),
        Some("parse.stream-option-migration") => Some("lint.stream-options"),
        Some("parse.enum-migration") => Some("lint.enum-declaration"),
        Some("check.removed-record-require") => Some("lint.removed-record-require"),
        Some("parse.compatibility-vocabulary" | "check.compatibility-vocabulary") => Some("lint.compatibility-vocabulary"),
        Some("parse.env-scope-migration") => Some("lint.env-scope"),
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
            migration_lint_code(diagnostic.code.as_deref()).is_some()
                && diagnostic.fix_hints.iter().any(|hint| {
                    hint.span.is_some_and(|span| edits.iter().any(|edit| {
                        edit.start == span.start() && edit.end == span.end()
                            && hint.replacement.as_ref() == Some(&edit.replacement)
                    }))
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
        if parsed.cst.get().contains_comment(span) {
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
