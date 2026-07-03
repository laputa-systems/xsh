use crate::xsht::format::Formatter;
use xsh::diagnostic::DiagnosticRenderer;
use xsh::source::{SourceMap, Span};
use xsh::syntax::parser::Parser;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

pub(crate) fn apply_cst_guarded_edits(
    file: &str,
    text: &str,
    edits: &[SourceEdit],
    line_width: usize,
) -> Result<Option<String>, String> {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(file, text);
    let parsed = Parser::parse_source_arena_only(source_id, text);
    if !parsed.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources));
    }

    let mut applied = false;
    let mut rewritten = text.to_string();
    for edit in edits.iter().rev() {
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

    let formatted = Formatter::new()
        .with_line_width(line_width)
        .format_source(source_id, &rewritten);
    if !formatted.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new().render(&formatted.diagnostics, &sources));
    }
    Ok(Some(formatted.formatted))
}
