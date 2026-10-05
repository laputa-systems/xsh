use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaStmtKind};

/// `use a.b` already binds `b`, so `use a.b as b` says the same thing twice.
/// Dropping the alias leaves the binding, and so the program, unchanged.
pub(super) fn lint_redundant_use_aliases(program: &ArenaProgram, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for statement in program.statement_ids() {
        let stmt = program.arena.stmt(statement);
        let ArenaStmtKind::Use(id) = stmt.kind else {
            continue;
        };
        let import = program.arena.use_stmt(id);
        let (Some(alias), Some(bound)) = (import.alias, program.arena.names(import.path).last())
        else {
            continue;
        };
        if alias != bound {
            continue;
        }
        let span = stmt.span;
        let mut diagnostic = Diagnostic::warning(format!(
            "`as {alias}` repeats the name this `use` already binds"
        ))
        .with_code(DiagnosticCode::LintRedundantUseAlias)
        .with_label(Label::primary(span, "the last path segment is the binding"));
        if let Some(alias_clause) = alias_clause_span(source, span, &alias.as_str()) {
            diagnostic = diagnostic
                .with_fix_hint(FixHint::deletion(alias_clause, "drop the redundant alias"));
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}

/// The ` as ALIAS` clause of a `use PATH as ALIAS` statement written on one
/// line with nothing between its words but blanks. Any other layout (a
/// comment or a line break inside the statement) gets the warning without a
/// fix, because deleting text there could drop the comment.
fn alias_clause_span(source: &str, statement: Span, alias: &str) -> Option<Span> {
    let text = source.get(statement.start()..statement.end())?;
    let rest = text.strip_prefix("use")?;
    let path = rest.trim_start_matches([' ', '\t']);
    if path.len() == rest.len() {
        return None;
    }
    let path_len = path.find([' ', '\t'])?;
    let clause = &path[path_len..];
    let after_as = clause.trim_start_matches([' ', '\t']).strip_prefix("as")?;
    let written_alias = after_as.trim_start_matches([' ', '\t']);
    if written_alias.len() == after_as.len() || written_alias.trim_end() != alias {
        return None;
    }
    let clause_start = statement.start() + (text.len() - clause.len());
    let clause_end = statement.start() + (text.len() - written_alias.len()) + alias.len();
    Some(Span::new(statement.source_id, clause_start, clause_end))
}

#[cfg(test)]
mod tests {
    use super::{alias_clause_span, lint_redundant_use_aliases};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::{SourceId, Span};
    use xsh::frontend::syntax::parser::Parser;

    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed
            .arena
            .symbol_owner()
            .with_current(|| lint_redundant_use_aliases(&parsed.arena, source))
    }

    fn apply(diagnostics: &[Diagnostic], source: &str) -> String {
        let mut fixes = diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .collect::<Vec<_>>();
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
        let mut fixed = source.to_owned();
        for fix in fixes {
            fixed.replace_range(
                fix.span.unwrap().range(),
                fix.replacement.as_deref().unwrap_or(""),
            );
        }
        fixed
    }

    #[test]
    fn an_alias_equal_to_the_last_path_segment_is_dropped() {
        let source = "use checks.disk as disk\nuse sshd as sshd\nuse a.b.c  as  c # keep\n\nprint ${disk.usage()}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintRedundantUseAlias))
        );
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "use checks.disk\nuse sshd\nuse a.b.c # keep\n\nprint ${disk.usage()}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    #[test]
    fn a_renaming_alias_and_a_bare_use_are_left_alone() {
        let source =
            "use checks.disk as usage\nuse checks.disk\nuse disk as checks\nuse a.disk as a\n";
        assert!(lint(source).is_empty());
    }

    #[test]
    fn the_fix_is_withheld_unless_only_blanks_separate_the_words() {
        let clause = |text: &str| {
            let span = Span::new(SourceId::new(0), 0, text.len());
            alias_clause_span(text, span, "disk").map(|span| text[span.range()].to_string())
        };
        assert_eq!(
            clause("use checks.disk as disk").as_deref(),
            Some(" as disk")
        );
        assert_eq!(
            clause("use checks.disk\tas\tdisk").as_deref(),
            Some("\tas\tdisk")
        );
        assert_eq!(clause("use checks.disk \\\n  as disk"), None);
        assert_eq!(clause("use checks.disk as\n  disk"), None);
        assert_eq!(clause("use checks.disk as disks"), None);
    }
}
