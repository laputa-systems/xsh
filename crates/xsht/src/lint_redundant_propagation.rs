use std::collections::{BTreeMap, BTreeSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::{StatementPosition, Type};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaExprKind, ArenaStmtKind, AstArena, ExprId, StmtId};

/// The checked facts that decide whether a statement's `?` is redundant.
pub(super) struct PropagationFacts<'a> {
    pub expr_types: &'a BTreeMap<Span, Type>,
    pub statement_positions: &'a BTreeMap<Span, StatementPosition>,
    pub propagating_statements: &'a BTreeSet<Span>,
}

/// An expression statement `OPERAND?` whose `?` repeats what the statement
/// does anyway.
pub(super) struct RedundantTry {
    pub operand: ExprId,
    /// The `?` and any blanks before it.
    pub removal: Span,
}

/// `statement` as `OPERAND?` where `OPERAND` alone would be a
/// statement-position `Result[Unit]`, which propagates its failure to the same
/// boundary.
///
/// Three checked facts carry the proof, and syntax only selects the shape:
/// the statement keeps no value, the checker treats a `Result[Unit]` at this
/// statement as propagating (a non-tail statement, or the tail of a `Unit`
/// body), and the operand is a `Result[Unit]`. A tail whose body yields
/// anything else is excluded by the second fact: there the operand would
/// become the body's value, which changes a `try` block's type.
pub(super) fn redundant_statement_try(
    arena: &AstArena,
    source: &str,
    facts: &PropagationFacts<'_>,
    statement: StmtId,
) -> Option<RedundantTry> {
    let statement = arena.stmt(statement);
    let ArenaStmtKind::Expr(expression) = statement.kind else {
        return None;
    };
    let propagation = arena.expr(expression);
    let ArenaExprKind::Try(operand) = propagation.kind else {
        return None;
    };
    let operand_span = arena.expr(operand).span;
    if facts.statement_positions.get(&statement.span) != Some(&StatementPosition::Statement)
        || !facts.propagating_statements.contains(&statement.span)
        || !facts
            .expr_types
            .get(&operand_span)
            .is_some_and(Type::is_result_unit)
        || operand_span.end() > propagation.span.end()
    {
        return None;
    }
    let removal = Span::new(
        propagation.span.source_id,
        operand_span.end(),
        propagation.span.end(),
    );
    // The operand's own text is kept, so only a plain `?` is removable.
    source
        .get(removal.range())
        .is_some_and(|text| text.trim_matches([' ', '\t']) == "?")
        .then_some(RedundantTry { operand, removal })
}

/// `fs.mkdir(tmp)?` as a statement spells propagation twice: a
/// statement-position `Result[Unit]` already propagates. Only a call operand
/// is reported here. A bare name would become a command word without its `?`,
/// and block-valued operands belong to `lint.redundant-scope-propagation`.
pub(super) fn redundant_propagation(
    arena: &AstArena,
    source: &str,
    facts: &PropagationFacts<'_>,
    statement: StmtId,
) -> Option<Diagnostic> {
    let found = redundant_statement_try(arena, source, facts, statement)?;
    matches!(arena.expr(found.operand).kind, ArenaExprKind::Call { .. }).then(|| {
        Diagnostic::warning("`?` on a statement that already propagates its failure")
            .with_code(DiagnosticCode::LintRedundantPropagation)
            .with_label(Label::secondary(
                found.removal,
                "a statement-position `Result[Unit]` propagates without `?`",
            ))
            .with_fix_hint(FixHint::deletion(found.removal, "remove `?`"))
    })
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::{CheckOutput, Checker};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::arena::ArenaProgram;
    use xsh::frontend::syntax::parser::Parser;

    pub(in super::super) fn checked(source: &str) -> (ArenaProgram, CheckOutput) {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(
            checked.diagnostics.is_empty(),
            "{source}\n{:?}",
            checked.diagnostics
        );
        (parsed.arena, checked)
    }

    /// The diagnostics of one rule from a whole lint of `source`, so a rule
    /// is exercised through the traversal that calls it.
    pub(in super::super) fn lint_rule(source: &str, rule: DiagnosticCode) -> Vec<Diagnostic> {
        let (program, checked) = checked(source);
        lint_checked(&program, source, checked, rule)
    }

    fn lint_checked(
        program: &ArenaProgram,
        source: &str,
        checked: CheckOutput,
        rule: DiagnosticCode,
    ) -> Vec<Diagnostic> {
        Linter::lint(
            program,
            source,
            LintOptions {
                function_return_types: checked.function_return_types,
                expr_types: checked.expr_types,
                statement_positions: checked.statement_positions,
                propagating_statements: checked.propagating_statements,
                function_effect_facts: checked.function_effect_facts,
                function_effect_facts_checked: true,
                only: Some(vec![rule]),
                ..LintOptions::default()
            },
        )
        .diagnostics
    }

    pub(in super::super) fn apply(diagnostics: &[Diagnostic], source: &str) -> String {
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

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintRedundantPropagation)
    }

    /// Every type the checker publishes, by source text, so a fix can be shown
    /// to leave the rest of the program's types alone.
    fn published_types(source: &str) -> Vec<(String, String)> {
        let (program, checked) = checked(source);
        program.symbol_owner().with_current(|| {
            let mut types = checked
                .expr_types
                .iter()
                .map(|(span, ty)| (source[span.range()].to_owned(), ty.to_string()))
                .chain(
                    checked
                        .function_return_types
                        .values()
                        .map(|ty| ("return".to_owned(), ty.to_string())),
                )
                .collect::<Vec<_>>();
            types.sort();
            types
        })
    }

    const PRELUDE: &str = "error E = Bad(message: Str)\n\nproc step(fail: Bool) -> Result[Unit, E] {\n  if fail { return Err(E.Bad(\"x\")) }\n}\n\nproc count() -> Result[Int, E] {\n  1\n}\n\n";

    fn fixed(body: &str, expected: &str) {
        let source = format!("{PRELUDE}{body}");
        let diagnostics = lint(&source);
        assert!(!diagnostics.is_empty(), "{body}");
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintRedundantPropagation)
                && diagnostic.fix_hints.len() == 1
        }));
        let after = apply(&diagnostics, &source);
        assert_eq!(after, format!("{PRELUDE}{expected}"));
        // The fixed program checks, reports nothing more, and every expression
        // other than the removed `step(..)?` keeps its type.
        assert!(lint(&after).is_empty(), "{after}");
        let without_propagation =
            |(text, ty): (String, String)| (text.replace([' ', '?'], ""), ty);
        let after_types = published_types(&after)
            .into_iter()
            .map(without_propagation)
            .collect::<Vec<_>>();
        for entry in published_types(&source) {
            assert!(
                entry.0.ends_with('?') || after_types.contains(&without_propagation(entry.clone())),
                "{body}: lost {entry:?}"
            );
        }
    }

    fn unflagged(body: &str) {
        let source = format!("{PRELUDE}{body}");
        let diagnostics = lint(&source);
        assert!(diagnostics.is_empty(), "{body}\n{diagnostics:?}");
    }

    #[test]
    fn a_statement_call_loses_its_redundant_propagation() {
        fixed(
            "proc work() -> Result[Int, E] {\n  step(false)?\n  step(true) ?\n  1\n}\n",
            "proc work() -> Result[Int, E] {\n  step(false)\n  step(true)\n  1\n}\n",
        );
        // Top-level statements other than the last are statements too.
        fixed("step(false)?\nprint \"done\"\n", "step(false)\nprint \"done\"\n");
    }

    #[test]
    fn statement_blocks_propagate_from_their_last_statement() {
        fixed(
            "proc work(items: List[Bool]) -> Result[Int, E] {\n  for item in items {\n    step(item)?\n  }\n  if items.len() > 1 {\n    step(false)?\n  } else {\n    step(true)?\n  }\n  1\n}\n",
            "proc work(items: List[Bool]) -> Result[Int, E] {\n  for item in items {\n    step(item)\n  }\n  if items.len() > 1 {\n    step(false)\n  } else {\n    step(true)\n  }\n  1\n}\n",
        );
        // A tail `if` of a `Result[Unit]` body is a statement; its branches
        // are statement blocks.
        fixed(
            "proc work(flag: Bool) -> Result[Unit, E] {\n  if flag {\n    step(false)?\n  }\n}\n",
            "proc work(flag: Bool) -> Result[Unit, E] {\n  if flag {\n    step(false)\n  }\n}\n",
        );
    }

    #[test]
    fn capture_and_retry_boundaries_keep_their_types() {
        fixed(
            "proc work() [time] -> Int {\n  let once = try {\n    step(true)?\n    1\n  }\n  let again = retry [1ms] {\n    step(true)?\n    2\n  }\n  (once ?? 0) + (again ?? 0)\n}\n",
            "proc work() [time] -> Int {\n  let once = try {\n    step(true)\n    1\n  }\n  let again = retry [1ms] {\n    step(true)\n    2\n  }\n  (once ?? 0) + (again ?? 0)\n}\n",
        );
    }

    #[test]
    fn a_unit_body_tail_propagates() {
        fixed(
            "proc work() -> Int {\n  let outcome: Result[Unit, E] = try {\n    step(true)?\n  }\n  match outcome {\n    Ok(_) => 0\n    Err(_) => 1\n  }\n}\n",
            "proc work() -> Int {\n  let outcome: Result[Unit, E] = try {\n    step(true)\n  }\n  match outcome {\n    Ok(_) => 0\n    Err(_) => 1\n  }\n}\n",
        );
    }

    #[test]
    fn a_value_producing_result_keeps_its_propagation() {
        unflagged("proc work() -> Result[Int, E] {\n  let _ = count()?\n  count()?\n}\n");
    }

    // Without `?` the operand would be the block's value: `try { step(true) }`
    // is a `Result[Result[Unit, E]]`.
    #[test]
    fn a_tail_whose_value_is_consumed_keeps_its_propagation() {
        unflagged(
            "proc work() -> Int {\n  let outcome = try {\n    step(true)?\n  }\n  match outcome {\n    Ok(_) => 0\n    Err(_) => 1\n  }\n}\n",
        );
        unflagged(
            "proc work() [time] -> Int {\n  let outcome = retry [1ms] {\n    step(true)?\n  }\n  match outcome {\n    Ok(_) => 0\n    Err(_) => 1\n  }\n}\n",
        );
    }

    // The direct tail of a `Result[Unit]` function would be returned as a
    // value, and the traceback would lose the `?` that propagated it.
    #[test]
    fn a_result_unit_function_tail_keeps_its_propagation() {
        unflagged("proc work() -> Result[Unit, E] {\n  step(false)?\n}\n");
        unflagged("proc work() [error] {\n  step(false)?\n}\n");
    }

    #[test]
    fn propagation_inside_a_larger_expression_stays() {
        unflagged(
            "proc pair() -> Result[Unit, E] {\n  Ok()\n}\n\nproc work() -> Result[Int, E] {\n  let unit = step(false)?\n  let both = [step(false)?, pair()?]\n  let _ = unit\n  both.len()\n}\n",
        );
    }

    // `defer` takes the expression itself; it is not an expression statement.
    #[test]
    fn a_deferred_expression_keeps_its_propagation() {
        unflagged("proc work() -> Result[Int, E] {\n  defer step(false)?\n  1\n}\n");
    }

    // A separated `?` after a command belongs to the whole command form.
    #[test]
    fn command_forms_keep_their_propagation() {
        unflagged(
            "proc work(dir: Path) [process, env, error] -> Result[Int] {\n  run true ?\n  let text = run.text echo hi ?\n  print $text\n  1\n}\n",
        );
    }

    // A bare name without `?` is a command word, not the binding.
    #[test]
    fn a_non_call_operand_is_left_alone() {
        unflagged(
            "proc work() -> Result[Int, E] {\n  let outcome = step(false)\n  outcome?\n  1\n}\n",
        );
    }

    // Without the checker's facts nothing is known about position.
    #[test]
    fn unchecked_source_reports_nothing() {
        let source = format!("{PRELUDE}proc work() -> Result[Int, E] {{\n  step(false)?\n  1\n}}\n");
        let (program, mut checked) = checked(&source);
        checked.propagating_statements.clear();
        let diagnostics = lint_checked(
            &program,
            &source,
            checked,
            DiagnosticCode::LintRedundantPropagation,
        );
        assert!(diagnostics.is_empty());
    }
}
