use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaArmSpelling, ArenaExprKind, ArenaPatternKind, ArenaProgram, ArenaStmtKind, ExprId,
    PatternId, StmtId,
};

/// A last arm written `_ =>` is the catch-all that `else =>` names. Both
/// parse to the same unguarded wildcard arm, so replacing the `_` changes
/// only the spelling.
///
/// Only the last arm qualifies: `else` must be last, and a `_ =>` arm with
/// arms after it already draws `check.unreachable-match-arm`. A guarded
/// `_ if cond =>` is not a catch-all and has no `else` spelling.
pub(super) fn lint_wildcard_catch_all_arms(program: &ArenaProgram, source: &str) -> Vec<Diagnostic> {
    let arena = &program.arena;
    let mut last_arms = Vec::new();
    for index in 0..arena.stmt_tags.len() {
        if let ArenaStmtKind::Match { arms, .. } = arena.stmt(StmtId::from_index(index)).kind
            && let Some(arm) = arena.match_arms(arms).last()
        {
            last_arms.push((arm.pattern, arm.guard, arm.spelling));
        }
    }
    for index in 0..arena.expr_tags.len() {
        // A pattern test or condition also stores arms, with a synthetic
        // wildcard; only a written `match` has an arm to respell.
        if let ArenaExprKind::Match { arms, .. } = arena.expr(ExprId::from_index(index)).kind
            && let Some(arm) = arena.match_expr_arms(arms).last()
        {
            last_arms.push((arm.pattern, arm.guard, arm.spelling));
        }
    }
    let mut diagnostics = Vec::new();
    for (pattern, guard, spelling) in last_arms {
        if guard.is_some() || spelling != ArenaArmSpelling::Pattern {
            continue;
        }
        let Some(wildcard) = written_wildcard(program, source, pattern) else {
            continue;
        };
        diagnostics.push(
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
        );
    }
    diagnostics.sort_by_key(|diagnostic| diagnostic.labels[0].span.start());
    diagnostics
}

/// The span of an arm pattern that is exactly the token `_`. A grouped
/// `(_)`, an alias, or an arm an expansion synthesized is left alone.
fn written_wildcard(program: &ArenaProgram, source: &str, pattern: PatternId) -> Option<Span> {
    let pattern = program.arena.pattern(pattern);
    if !matches!(pattern.kind, ArenaPatternKind::Wildcard) {
        return None;
    }
    let span = program.arena.span(pattern.span);
    (source.get(span.range())? == "_").then_some(span)
}

#[cfg(test)]
mod tests {
    use super::lint_wildcard_catch_all_arms;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed
            .arena
            .symbol_owner()
            .with_current(|| lint_wildcard_catch_all_arms(&parsed.arena, source))
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
