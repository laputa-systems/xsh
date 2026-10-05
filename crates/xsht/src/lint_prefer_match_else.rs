use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaArmSpelling, ArenaPatternKind, AstArena, ExprId, PatternId,
};

/// A last arm written `_ =>` is the catch-all that `else =>` names. Both
/// parse to the same unguarded wildcard arm, so replacing the `_` changes
/// only the spelling.
///
/// Only the last arm qualifies: `else` must be last, and a `_ =>` arm with
/// arms after it already draws `check.unreachable-match-arm`. A guarded
/// `_ if cond =>` is not a catch-all and has no `else` spelling.
///
/// This is the report for the last arm of one `match` statement or
/// expression the linter visits. A pattern test or condition also stores
/// arms, with a synthetic wildcard; only a written `match` has an arm to
/// respell.
pub(super) fn catch_all_arm_report(
    arena: &AstArena,
    source: &str,
    pattern: PatternId,
    guard: Option<ExprId>,
    spelling: ArenaArmSpelling,
) -> Option<Diagnostic> {
    if guard.is_some() || spelling != ArenaArmSpelling::Pattern {
        return None;
    }
    let wildcard = written_wildcard(arena, source, pattern)?;
    Some(
        Diagnostic::warning("write the catch-all match arm as `else =>`")
            .with_code(DiagnosticCode::LintPreferMatchElse)
            .with_label(Label::primary(
                wildcard,
                "a whole-arm `_` is the catch-all `else`",
            ))
            .with_fix_hint(FixHint::replacement(
                wildcard,
                "replace `_` with `else`",
                "else",
            )),
    )
}

/// The span of an arm pattern that is exactly the token `_`. A grouped
/// `(_)`, an alias, or an arm an expansion synthesized is left alone.
fn written_wildcard(arena: &AstArena, source: &str, pattern: PatternId) -> Option<Span> {
    let pattern = arena.pattern(pattern);
    if !matches!(pattern.kind, ArenaPatternKind::Wildcard) {
        return None;
    }
    let span = arena.span(pattern.span);
    (source.get(span.range())? == "_").then_some(span)
}

#[cfg(test)]
mod tests {
    use crate::xsht::lint::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Linter::lint(&parsed.arena, source, LintOptions::default())
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferMatchElse))
            .collect()
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
    fn a_last_wildcard_arm_becomes_else_in_statements_and_expressions() {
        let source = "match level {\n  Info => print \"info\"\n  _ => {}\n}\nlet label = match level { Info => \"info\", _ => \"other\" }\nmatch pair {\n  [_, _] => if ready { go } \n  _ => match inner {\n    Some(_) => 1\n    _ => 2 # nested\n  }\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferMatchElse))
        );
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "match level {\n  Info => print \"info\"\n  else => {}\n}\nlet label = match level { Info => \"info\", else => \"other\" }\nmatch pair {\n  [_, _] => if ready { go } \n  else => match inner {\n    Some(_) => 1\n    else => 2 # nested\n  }\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    #[test]
    fn other_wildcards_are_left_alone() {
        let source = "match level {\n  Fault(_) => 1\n  _ if quiet => 2\n  (_) => 3\n}\nmatch level {\n  _ => 1\n  Info => 2\n}\nmatch level {\n  _ | Info => 1\n}\nlet known = level is _\nif let Fault(_) = level { print \"fault\" }\n";
        assert!(lint(source).is_empty(), "{:?}", lint(source));
    }
}
