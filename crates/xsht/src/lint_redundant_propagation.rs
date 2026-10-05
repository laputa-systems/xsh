use std::collections::{BTreeMap, BTreeSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::{StatementPosition, Type};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCommand, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, AstArena, ExprId, RunFormId,
    StmtId,
};
use xsh::frontend::syntax::node::RunKind;
use xsh::frontend::syntax::parser::Parser;

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
/// body, or the direct tail of a `Result[Unit]` function), and the operand is
/// a `Result[Unit]`. A tail whose body yields anything else is excluded by the
/// second fact: there the operand would become the body's value, which
/// changes a `try` block's type.
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

/// `run make ?` as a statement spells propagation twice: a plain `run`
/// statement, and a byte pipeline that begins with one, already fails on an
/// unsuccessful command. The `?` is redundant only where the statement keeps
/// no value; as the tail of a body that yields its `Status`, `run make` is
/// data and `run make ?` is not.
pub(super) fn redundant_run_propagation(
    arena: &AstArena,
    source: &str,
    facts: &PropagationFacts<'_>,
    statement: StmtId,
) -> Option<Diagnostic> {
    let statement = arena.stmt(statement);
    let ArenaStmtKind::Command(command) = statement.kind else {
        return None;
    };
    let ArenaCommand::Run(run) = arena.command_stmt(command).command else {
        return None;
    };
    let run = arena.run_form(run);
    if !run.propagation_written
        || arena.run_segments(run.segments).first()?.kind != RunKind::Plain
        || facts.statement_positions.get(&statement.span) != Some(&StatementPosition::Statement)
    {
        return None;
    }
    let form = arena.span(run.span);
    // A command statement's span runs through its terminator.
    let after = source.get(form.end()..statement.span.end())?;
    let propagation = after.trim_start_matches([' ', '\t']);
    let blanks = after.len() - propagation.len();
    if !propagation
        .strip_prefix('?')
        .is_some_and(|rest| rest.trim_matches([' ', '\t', '\r', '\n', ';']).is_empty())
    {
        return None;
    }
    let removal = Span::new(form.source_id, form.end(), form.end() + blanks + 1);
    Some(
        Diagnostic::warning("`?` on a `run` statement that already fails with its command")
            .with_code(DiagnosticCode::LintRedundantPropagation)
            .with_label(Label::secondary(
                removal,
                "a plain `run` statement propagates a failed command without `?`",
            ))
            .with_fix_hint(FixHint::deletion(removal, "remove `?`")),
    )
}

/// `defer root.close()?` spells propagation twice: a deferred `Result[Unit]`
/// fails its action with or without `?`, at the same place and with the same
/// error. Only a call operand is reported, for the reason given on
/// `redundant_propagation`; a deferred block's statements are expression
/// statements and are reported there.
pub(super) fn redundant_defer_propagation(
    arena: &AstArena,
    source: &str,
    facts: &PropagationFacts<'_>,
    statement: StmtId,
) -> Option<Diagnostic> {
    let ArenaStmtKind::Defer(ArenaExprOrRun::Expr(action), _) = arena.stmt(statement).kind else {
        return None;
    };
    let propagation = arena.expr(action);
    let ArenaExprKind::Try(operand) = propagation.kind else {
        return None;
    };
    let operand_span = arena.expr(operand).span;
    if !matches!(arena.expr(operand).kind, ArenaExprKind::Call { .. })
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
    source
        .get(removal.range())
        .is_some_and(|text| text.trim_matches([' ', '\t']) == "?")
        .then(|| {
            Diagnostic::warning("`?` on a deferred action that already fails with its `Result`")
                .with_code(DiagnosticCode::LintRedundantPropagation)
                .with_label(Label::secondary(
                    removal,
                    "a deferred `Result[Unit]` fails its action without `?`",
                ))
                .with_fix_hint(FixHint::deletion(removal, "remove `?`"))
        })
}

/// Whether the run form is one that captures or streams its output. Such a
/// form fails the enclosing function on a failed command unless `try`
/// captures it, so a `?` after it says nothing.
fn captures_output(arena: &AstArena, run: RunFormId) -> bool {
    let form = arena.run_form(run);
    !form.captured
        && arena
            .run_segments(form.segments)
            .first()
            .is_some_and(|head| !matches!(head.kind, RunKind::Plain | RunKind::Status))
}

/// The report for a `?` after a capturing run form, or `None` where the `?`
/// is also what ends the form.
///
/// A run form reads words to the end of its line, a `;`, a `}`, a `|>`, or
/// the `)` of the parentheses that group it. Before anything else, such as
/// the `,` or `)` of a call's arguments, an operator, or a postfix guard,
/// the `?` ends the form and stays. A `)` closes a group only if the source
/// still parses without the `?`.
fn redundant_capture_diagnostic(source: &str, removal: Span) -> Option<Diagnostic> {
    let after = source.get(removal.end()..)?.trim_start_matches([' ', '\t']);
    let ends_form = after.is_empty()
        || after.starts_with(['\n', '\r', ';', '}', '#'])
        || after.starts_with("|>")
        || (after.starts_with(')') && {
            let mut candidate = source.to_owned();
            candidate.replace_range(removal.range(), "");
            Parser::parse_source_arena_only(removal.source_id, &candidate)
                .diagnostics
                .is_empty()
        });
    ends_form.then(|| {
        Diagnostic::warning("`?` on a run form that already fails with its command")
            .with_code(DiagnosticCode::LintRedundantPropagation)
            .with_label(Label::secondary(
                removal,
                "a capturing run form propagates a failed command without `?`; `try run...` keeps the failure as a value",
            ))
            .with_fix_hint(FixHint::deletion(removal, "remove `?`"))
    })
}

/// `let text = run.text git describe ?` spells propagation twice: a
/// capturing run form already fails with its command.
pub(super) fn redundant_capture_propagation(
    arena: &AstArena,
    source: &str,
    run: RunFormId,
) -> Option<Diagnostic> {
    let form = arena.run_form(run);
    if !form.propagation_written || !captures_output(arena, run) {
        return None;
    }
    let span = arena.span(form.span);
    let after = source.get(span.end()..)?;
    let propagation = after.trim_start_matches([' ', '\t']);
    let blanks = after.len() - propagation.len();
    if !propagation.starts_with('?') {
        return None;
    }
    redundant_capture_diagnostic(
        source,
        Span::new(span.source_id, span.end(), span.end() + blanks + 1),
    )
}

/// `(run.text git describe ?).trim()` and `run.stream --text git log ? |>
/// take(2)` spell it twice the same way, with the `?` as an operator on the
/// run form. A `?` after a closing parenthesis is left alone: only one
/// written directly after the form's words is removed.
pub(super) fn redundant_capture_try(
    arena: &AstArena,
    source: &str,
    expression: ExprId,
) -> Option<Diagnostic> {
    let propagation = arena.expr(expression);
    let ArenaExprKind::Try(operand) = propagation.kind else {
        return None;
    };
    let ArenaExprKind::Run(run) = arena.expr(operand).kind else {
        return None;
    };
    if !captures_output(arena, run) {
        return None;
    }
    let form = arena.span(arena.run_form(run).span);
    if form.end() > propagation.span.end() {
        return None;
    }
    let removal = Span::new(form.source_id, form.end(), propagation.span.end());
    if source.get(removal.range())?.trim_matches([' ', '\t']) != "?" {
        return None;
    }
    redundant_capture_diagnostic(source, removal)
}

/// `if fs.exists(path)? { ... }` spells propagation twice: a `Result[Bool]`
/// in a control position of a condition already propagates. The checker
/// decided which `?` those are; this only finds the text to remove.
///
/// A `?` whose operand is the whole content of a pair of parentheses keeps
/// its report and loses the fix, because the parentheses would be left
/// around a bare operand.
pub(super) fn redundant_condition_propagation(
    arena: &AstArena,
    source: &str,
    redundant: &BTreeSet<Span>,
    expression: ExprId,
) -> Option<Diagnostic> {
    if redundant.is_empty() {
        return None;
    }
    let propagation = arena.expr(expression);
    let ArenaExprKind::Try(operand) = propagation.kind else {
        return None;
    };
    if !redundant.contains(&propagation.span) {
        return None;
    }
    let operand_span = arena.expr(operand).span;
    let removal = Span::new(
        propagation.span.source_id,
        operand_span.end(),
        propagation.span.end().max(operand_span.end()),
    );
    let mut diagnostic =
        Diagnostic::warning("`?` on a condition that already propagates its failure")
            .with_code(DiagnosticCode::LintRedundantPropagation)
            .with_label(Label::secondary(
                removal,
                "a `Result[Bool]` condition propagates without `?`",
            ));
    let grouped = source[..propagation.span.start()].trim_end().ends_with('(')
        && source[propagation.span.end()..].trim_start().starts_with(')');
    if !grouped
        && source
            .get(removal.range())
            .is_some_and(|text| text.trim_matches([' ', '\t']) == "?")
    {
        diagnostic = diagnostic.with_fix_hint(FixHint::deletion(removal, "remove `?`"));
    }
    Some(diagnostic)
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
                redundant_condition_propagations: checked.redundant_condition_propagations,
                unvalidated_command_vectors: checked.unvalidated_command_vectors,
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

    // The direct tail of a `Result[Unit]` function is the function's result
    // and a propagating statement at once.
    #[test]
    fn a_result_unit_function_tail_propagates() {
        fixed(
            "proc work() -> Result[Unit, E] {\n  step(false)?\n  step(true)?\n}\n",
            "proc work() -> Result[Unit, E] {\n  step(false)\n  step(true)\n}\n",
        );
        fixed(
            "proc work() [error] -> Result[Unit] {\n  step(false)?\n}\n",
            "proc work() [error] -> Result[Unit] {\n  step(false)\n}\n",
        );
    }

    // A proc without a return annotation returns `Result[Unit]` under either
    // spelling of its tail.
    #[test]
    fn an_unannotated_proc_tail_propagates() {
        fixed(
            "proc work() {\n  step(false)?\n}\n",
            "proc work() {\n  step(false)\n}\n",
        );
    }

    #[test]
    fn propagation_inside_a_larger_expression_stays() {
        unflagged(
            "proc pair() -> Result[Unit, E] {\n  Ok()\n}\n\nproc work() -> Result[Int, E] {\n  let unit = step(false)?\n  let both = [step(false)?, pair()?]\n  let _ = unit\n  both.len()\n}\n",
        );
    }

    // A deferred `Result[Unit]` fails its action under either spelling.
    #[test]
    fn a_deferred_call_loses_its_propagation() {
        fixed(
            "proc work() -> Result[Int, E] {\n  defer step(false)?\n  errdefer step(true) ?\n  defer {\n    step(false)?\n  }\n  1\n}\n",
            "proc work() -> Result[Int, E] {\n  defer step(false)\n  errdefer step(true)\n  defer {\n    step(false)\n  }\n  1\n}\n",
        );
        fixed("defer step(false)?\nprint \"done\"\n", "defer step(false)\nprint \"done\"\n");
    }

    // A deferred value other than `Unit` is rejected without its `?`, and a
    // bare name without `?` would be a command word.
    #[test]
    fn a_deferred_action_that_needs_its_propagation_keeps_it() {
        unflagged(
            "proc work() -> Result[Int, E] {\n  let outcome = step(false)\n  defer outcome?\n  1\n}\n",
        );
    }

    #[test]
    fn a_capturing_run_form_loses_its_propagation() {
        let source = "proc work() [process, error] -> Result[Int] {\n  let text = run.text echo hi ?\n  var raw = run.bytes echo hi ?\n  raw = run.bytes echo ho ?\n  let words = (run.text echo a b ?).split(\" \")\n  let lines = run.stream --text echo log ? |> take(2) |> collect()\n  let both = run.text echo a | run cat ?\n  run.text echo discarded ?\n  print $text ${raw.len()} ${words.len()} ${lines.len()} $both\n  return Ok((run.capture --text echo hi ?).stdout.byte_len())\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 8, "{diagnostics:?}");
        let after = apply(&diagnostics, source);
        assert_eq!(
            after,
            "proc work() [process, error] -> Result[Int] {\n  let text = run.text echo hi\n  var raw = run.bytes echo hi\n  raw = run.bytes echo ho\n  let words = (run.text echo a b).split(\" \")\n  let lines = run.stream --text echo log |> take(2) |> collect()\n  let both = run.text echo a | run cat\n  run.text echo discarded\n  print $text ${raw.len()} ${words.len()} ${lines.len()} $both\n  return Ok((run.capture --text echo hi).stdout.byte_len())\n}\n"
        );
        // The fixed program checks, with every type it had, and is clean.
        assert!(lint(&after).is_empty(), "{after}");
        let types = |source: &str| {
            published_types(source)
                .into_iter()
                .filter(|(text, _)| !text.contains("run"))
                .collect::<Vec<_>>()
        };
        assert_eq!(types(source), types(&after));
    }

    // In a call's or a list's arguments and before an operator, the `?` is
    // also what ends the run form.
    #[test]
    fn a_propagation_that_ends_its_run_form_stays() {
        unflagged(
            "pure both(text: Str, tail: Str) -> Str {\n  text + tail\n}\n\nproc work() [process, error] -> Result[Int] {\n  let joined = both(run.text echo a ?, \"!\")\n  let last = both(\"!\", run.text echo a ?)\n  let listed = [run.text echo b ?, \"c\"]\n  assert run.text echo d? == \"d\"\n  print $joined $last ${listed.len()}\n  1\n}\n",
        );
    }

    // A captured form has no `?`, a status form's `?` is its only
    // propagation, a `?` after a closing parenthesis is an operator on the
    // group, and a guard after the `?` would become arguments without it.
    #[test]
    fn a_run_form_that_needs_its_propagation_keeps_it() {
        unflagged(
            "proc work(ready: Bool) [process, error] -> Result[Int] {\n  let kept = try run.text echo hi\n  let status = run.status echo hi ?\n  let plain = run echo hi ?\n  let grouped = (run.text echo hi)?\n  var text = \"\"\n  text = run.text echo hi ? when ready\n  return Ok(text.byte_len()) when kept is Ok(_)\n  print $grouped ${status.success} ${plain.success}\n  1\n}\n",
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

    const KNOWN: &str = "pure known(name: Str) -> Result[Bool] {\n  Ok(name != \"\")\n}\n\npure count(name: Str) -> Result[Int] {\n  name.parse_int()\n}\n\n";

    #[test]
    fn a_condition_in_a_control_position_loses_its_propagation() {
        let source = format!(
            "{KNOWN}pure pick(name: Str) -> Result[Int] {{\n  if known(name)? {{\n    return Ok(1)\n  }} else if ! known(name)? and (known(name)? or count(name)? > 1) {{\n    return Ok(2)\n  }}\n\n  while known(name)? {{\n    break\n  }}\n\n  guard known(name)? else {{\n    return Ok(3)\n  }}\n\n  return Ok(4) unless known(name)?\n  let label = if known(name)? {{ 5 }} else {{ 6 }}\n  Ok(label)\n}}\n"
        );
        let diagnostics = lint(&source);
        assert_eq!(diagnostics.len(), 7, "{diagnostics:?}");
        let after = apply(&diagnostics, &source);
        assert_eq!(
            after,
            format!(
                "{KNOWN}pure pick(name: Str) -> Result[Int] {{\n  if known(name) {{\n    return Ok(1)\n  }} else if ! known(name) and (known(name) or count(name)? > 1) {{\n    return Ok(2)\n  }}\n\n  while known(name) {{\n    break\n  }}\n\n  guard known(name) else {{\n    return Ok(3)\n  }}\n\n  return Ok(4) unless known(name)\n  let label = if known(name) {{ 5 }} else {{ 6 }}\n  Ok(label)\n}}\n"
            )
        );
        // The fixed program checks and has nothing left to report.
        assert!(lint(&after).is_empty(), "{after}");
    }

    #[test]
    fn a_result_that_is_data_keeps_its_propagation() {
        for body in [
            // An operand of a comparison, a call argument, a binding, and a
            // match arm's guard are not control positions.
            "  if known(name)? == true {\n    return Ok(1)\n  }\n\n  Ok(0)\n",
            "  if show(known(name)?) {\n    return Ok(1)\n  }\n\n  Ok(0)\n",
            "  let flag = ! known(name)?\n  Ok(if flag { 1 } else { 0 })\n",
            "  match name {\n    \"x\" if known(name)? => Ok(1)\n    else => Ok(0)\n  }\n",
            // Another `Result` in a condition is not a `Result[Bool]`.
            "  if count(name)? > 1 {\n    return Ok(1)\n  }\n\n  Ok(0)\n",
        ] {
            let source = format!(
                "{KNOWN}pure show(flag: Bool) -> Bool {{\n  flag\n}}\n\npure pick(name: Str) -> Result[Int] {{\n{body}}}\n"
            );
            assert!(lint(&source).is_empty(), "{body}");
        }
    }

    #[test]
    fn a_plain_run_statement_loses_its_propagation() {
        let source = "proc build(target: Str) -> Result[Status] {\n  run make clean ?\n  run make $target | run tee build.log ?\n  if target == \"all\" {\n    run make install ?\n  }\n\n  run.status make check ?\n  let listed = run.text make --version ?\n  print $listed\n  let status = run make $target\n  Ok(status)\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
        let after = apply(&diagnostics, source);
        // `run.status` discards its status, and fails only by its `?`.
        assert_eq!(
            after,
            "proc build(target: Str) -> Result[Status] {\n  run make clean\n  run make $target | run tee build.log\n  if target == \"all\" {\n    run make install\n  }\n\n  run.status make check ?\n  let listed = run.text make --version\n  print $listed\n  let status = run make $target\n  Ok(status)\n}\n"
        );
        assert!(lint(&after).is_empty(), "{after}");
    }

    #[test]
    fn a_run_statement_that_ends_a_unit_body_is_a_statement_too() {
        // The checker reads the last statement of a `Unit` body, and of a
        // `try` block that is one, as a statement: the bare form asserts.
        let source = "proc build() {\n  run make all ?\n}\n\nproc both() -> Result[Unit] {\n  try {\n    run make all ?\n  }\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        let after = apply(&diagnostics, source);
        assert_eq!(
            after,
            "proc build() {\n  run make all\n}\n\nproc both() -> Result[Unit] {\n  try {\n    run make all\n  }\n}\n"
        );
        assert!(lint(&after).is_empty(), "{after}");
    }
}
