use std::collections::BTreeSet;
use xsh::diagnostic::{Diagnostic, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaStmtKind, StmtId};

pub(super) const CODE: &str = "lint.explicit-assert";

/// Prefix every statement the checker classified as a Boolean assertion with
/// `assert`. Checked assertion spans, not syntax, decide membership, so
/// parenthesized, multi-line, Unit-tail, and bare-identifier forms are covered.
pub(super) fn lint_explicit_asserts(program: &ArenaProgram, source: &str, assertion_spans: &BTreeSet<Span>) -> Vec<Diagnostic> {
    let arena = &program.arena;
    // A workspace arena also holds imported modules; edit only this source.
    let Some(source_id) = program.statement_ids().next().map(|id| arena.stmt(id).span.source_id) else { return Vec::new(); };
    let mut starts = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for index in 0..arena.stmt_tags.len() {
        let stmt = arena.stmt(StmtId::from_index(index));
        if stmt.span.source_id != source_id { continue; }
        let checked = match stmt.kind {
            ArenaStmtKind::Expr(expr) => arena.expr(expr).span,
            ArenaStmtKind::TailBareIdent(_) => stmt.span,
            ArenaStmtKind::Command(command) => arena.span(arena.command_stmt(command).span),
            _ => continue,
        };
        if !assertion_spans.contains(&checked) || !starts.insert(stmt.span.start()) { continue; }
        let mut diagnostic = Diagnostic::warning("use an explicit `assert` for a Boolean statement")
            .with_code(CODE)
            .with_label(Label::primary(stmt.span, "this statement asserts its Boolean value"));
        if let Some(fix) = explicit_assert_fix(source, stmt.span) { diagnostic = diagnostic.with_fix_hint(fix); }
        diagnostics.push(diagnostic);
    }
    diagnostics.sort_by_key(|diagnostic| diagnostic.labels.first().map(|label| label.span.start()));
    diagnostics
}

fn explicit_assert_fix(source: &str, statement: Span) -> Option<FixHint> {
    let text = source.get(statement.range())?.trim_end();
    let Some(content) = text.strip_suffix(',') else {
        return Some(FixHint::replacement(Span::at(statement.source_id, statement.start()), "insert `assert`", "assert "));
    };
    let content = content.trim_end();
    if content.contains('#') { return None; }
    Some(FixHint::replacement(
        Span::new(statement.source_id, statement.start(), statement.start() + content.len()),
        "insert `assert` in a braced match arm",
        assert_statement(content, true),
    ))
}

/// An unbraced match arm ends at a comma, which `assert` would read as its
/// message separator, so such an arm becomes a braced block.
pub(super) fn assert_statement(condition: &str, comma_terminated: bool) -> String {
    if comma_terminated { format!("{{ assert {condition} }}") } else { format!("assert {condition}") }
}

/// Whether the statement whose expression ends at `end` is terminated by a
/// comma (an unbraced match arm).
pub(super) fn comma_terminated(source: &str, end: usize) -> bool {
    source.get(end..).is_some_and(|rest| rest.trim_start_matches([' ', '\t']).starts_with(','))
}
