use super::lint_redundant_propagation::PropagationFacts;
use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::{EffectDeclarationId, FunctionEffectFact, StatementPosition, Type};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaPatternKind,
    ArenaStmtKind, AstArena, ExprId, FunctionDefId, StmtId,
};
use xsh::frontend::syntax::node::{AssignOp, Effect};

/// Whether `?` may replace `return Err(e)` in this function without changing
/// its contract. `return Err(e)` needs no effect, while `?` needs `error` in a
/// proc, so the proc's effect contract must already have it; then no
/// signature and no inferred effect set changes.
pub(super) fn propagation_allowed(
    arena: &AstArena,
    effects: &BTreeMap<EffectDeclarationId, FunctionEffectFact>,
    definition: FunctionDefId,
    pure: bool,
) -> bool {
    let body = arena.span(arena.block(arena.function_def(definition).body).span);
    pure || effects.iter().any(|(id, fact)| {
        id.body == body
            && match &fact.effective {
                Some(effective) => effective.contains(&Effect::Error),
                None => !fact.inferred,
            }
    })
}

/// `match f() { Ok(_) => {} Err(e) => return Err(e) }` is `f()?` written out:
/// success continues and the same error leaves the function.
///
/// Only that exact shape is reported: an `Ok(_)` arm with an empty body and an
/// `Err(name)` arm whose whole body is `return Err(name)`, with no guards.
/// `Ok(value) => target = value` beside the same `Err` arm is `target = f()?`
/// written out, where `target` is a plain name: the scrutinee still runs
/// before the assignment, and a target with selectors would run them first.
/// The caller passes only a statement that a proc or pure body reaches
/// through statement blocks alone. Inside a `try`, a `retry`, a stream
/// callback, or a deferred block the two differ, because `return` leaves the
/// function while `?` stops at the nearer boundary.
pub(super) fn repropagating_match(
    arena: &AstArena,
    source: &str,
    facts: &PropagationFacts<'_>,
    statement: StmtId,
    propagation_allowed: bool,
) -> Option<Diagnostic> {
    let statement = arena.stmt(statement);
    let ArenaStmtKind::Match { value, arms } = statement.kind else {
        return None;
    };
    let [first, second] = arena.match_arms(arms) else {
        return None;
    };
    if first.guard.is_some() || second.guard.is_some() {
        return None;
    }
    let constructor = |pattern| match arena.pattern(pattern).kind {
        ArenaPatternKind::Constructor {
            name,
            arg: Some(arg),
        } => Some((name, arena.pattern(arg).kind.clone())),
        _ => None,
    };
    let (ok, error) = match (constructor(first.pattern)?, constructor(second.pattern)?) {
        ((first_name, _), (second_name, _)) if first_name == "Ok" && second_name == "Err" => {
            (first, second)
        }
        ((first_name, _), (second_name, _)) if first_name == "Err" && second_name == "Ok" => {
            (second, first)
        }
        _ => return None,
    };
    let mut ok_body = arena.stmt_ids(arena.block(ok.block).statements);
    let assigned = match (constructor(ok.pattern)?.1, ok_body.next(), ok_body.next()) {
        (ArenaPatternKind::Wildcard, None, None) => None,
        (ArenaPatternKind::Binding(value), Some(only), None) => {
            let ArenaStmtKind::Assign {
                target,
                op: AssignOp::Set,
                value: ArenaExprOrRun::Expr(assigned),
            } = arena.stmt(only).kind
            else {
                return None;
            };
            let ArenaAssignTargetKind::Name(target) = arena.assign_target(target).kind else {
                return None;
            };
            if target == value
                || !matches!(arena.expr(assigned).kind, ArenaExprKind::Ident(name) if name == value)
            {
                return None;
            }
            Some(target)
        }
        _ => return None,
    };
    let (_, ArenaPatternKind::Binding(binding)) = constructor(error.pattern)? else {
        return None;
    };
    let mut error_body = arena.stmt_ids(arena.block(error.block).statements);
    let (Some(only), None) = (error_body.next(), error_body.next()) else {
        return None;
    };
    let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(returned))) = arena.stmt(only).kind else {
        return None;
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(returned).kind else {
        return None;
    };
    let [argument] = arena.call_args(args) else {
        return None;
    };
    let ArenaCallArgKind::Positional(argument) = argument.kind else {
        return None;
    };
    if !matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Err")
        || !matches!(arena.expr(argument).kind, ArenaExprKind::Ident(name) if name == binding)
    {
        return None;
    }
    // The match keeps no value, and its scrutinee is a checked `Result`.
    if facts.statement_positions.get(&statement.span) != Some(&StatementPosition::Statement) {
        return None;
    }
    let scrutinee = arena.expr(value);
    let Some(Type::Result(success, _)) = facts.expr_types.get(&scrutinee.span) else {
        return None;
    };
    let mut diagnostic = Diagnostic::warning("this `match` only returns the `Err` it was given")
        .with_code(DiagnosticCode::LintPreferPropagation)
        .with_label(Label::secondary(
            scrutinee.span,
            "`?` continues on `Ok` and propagates the same `Err`",
        ));
    if let Some(fix) = propagation_fix(
        arena,
        source,
        statement.span,
        value,
        match assigned {
            Some(target) => Kept::Assigned(target),
            None if **success == Type::Unit => Kept::Nothing,
            None => Kept::Dropped,
        },
        propagation_allowed,
    ) {
        diagnostic = diagnostic.with_fix_hint(fix);
    }
    Some(diagnostic)
}

/// What the `Ok` arm did with the success value.
enum Kept {
    /// There is none: the success type is `Unit`.
    Nothing,
    /// `Ok(_)` dropped it.
    Dropped,
    /// `Ok(value) => NAME = value` assigned it.
    Assigned(Name),
}

/// `SCRUTINEE?`, `let _ = SCRUTINEE?` when success carries a value the
/// `Ok(_)` arm dropped, or `NAME = SCRUTINEE?` when the arm assigned it. A
/// scrutinee other than a call may need grouping under
/// `?`, and a comment inside the match has no place in the rewrite; both keep
/// the report and lose the fix.
fn propagation_fix(
    arena: &AstArena,
    source: &str,
    statement: Span,
    value: ExprId,
    kept: Kept,
    propagation_allowed: bool,
) -> Option<FixHint> {
    let scrutinee = arena.expr(value);
    if !propagation_allowed || !matches!(scrutinee.kind, ArenaExprKind::Call { .. }) {
        return None;
    }
    // A statement's span may run through its terminator.
    let text = source.get(statement.range())?.trim_end();
    let scrutinee_text = source.get(scrutinee.span.range())?;
    let before = source.get(statement.start()..scrutinee.span.start())?;
    let after = source.get(scrutinee.span.end()..statement.start() + text.len())?;
    if !text.ends_with('}') || before.contains('#') || after.contains('#') {
        return None;
    }
    let replacement = match kept {
        Kept::Nothing => format!("{scrutinee_text}?"),
        Kept::Dropped => format!("let _ = {scrutinee_text}?"),
        Kept::Assigned(target) => format!("{target} = {scrutinee_text}?"),
    };
    Some(FixHint::replacement(
        Span::new(
            statement.source_id,
            statement.start(),
            statement.start() + text.len(),
        ),
        "propagate with `?`",
        replacement,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::lint_redundant_propagation::tests::{apply, lint_rule};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintPreferPropagation)
    }

    const PRELUDE: &str = "error E = Bad(message: Str)\n\npure step(fail: Bool) -> Result[Unit, E] {\n  if fail { return Err(E.Bad(\"x\")) }\n}\n\npure count() -> Result[Int, E] {\n  1\n}\n\n";

    fn fixed(body: &str, expected: &str) {
        let source = format!("{PRELUDE}{body}");
        let diagnostics = lint(&source);
        assert!(!diagnostics.is_empty(), "{body}");
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintPreferPropagation)
                && diagnostic.fix_hints.len() == 1
        }));
        let after = apply(&diagnostics, &source);
        assert_eq!(after, format!("{PRELUDE}{expected}"));
        // The rewrite checks and leaves nothing to report.
        assert!(lint(&after).is_empty(), "{after}");
    }

    fn unflagged(body: &str) {
        let source = format!("{PRELUDE}{body}");
        let diagnostics = lint(&source);
        assert!(diagnostics.is_empty(), "{body}\n{diagnostics:?}");
    }

    fn reported_without_fix(body: &str) {
        let source = format!("{PRELUDE}{body}");
        let diagnostics = lint(&source);
        assert_eq!(diagnostics.len(), 1, "{body}\n{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn a_repropagating_match_becomes_propagation() {
        fixed(
            "proc work() [error] -> Result[Int, E] {\n  match step(false) {\n    Ok(_) => {}\n    Err(problem) => return Err(problem)\n  }\n  match step(true) {\n    Err(problem) => {\n      return Err(problem)\n    }\n    Ok(_) => {}\n  }\n  1\n}\n",
            "proc work() [error] -> Result[Int, E] {\n  step(false)?\n  step(true)?\n  1\n}\n",
        );
    }

    #[test]
    fn an_assigning_success_arm_becomes_an_assignment() {
        fixed(
            "pure work() -> Result[Int, E] {\n  var total = 0\n  match count() {\n    Ok(value) => total = value\n    Err(problem) => return Err(problem)\n  }\n  total + 1\n}\n",
            "pure work() -> Result[Int, E] {\n  var total = 0\n  total = count()?\n  total + 1\n}\n",
        );
        // A compound assignment, a target with a selector, and an arm that
        // assigns something else are different programs.
        unflagged(
            "pure work() -> Result[Int, E] {\n  var total = 0\n  var totals = [0]\n  match count() {\n    Ok(value) => total += value\n    Err(problem) => return Err(problem)\n  }\n  match count() {\n    Ok(value) => totals[0] = value\n    Err(problem) => return Err(problem)\n  }\n  match count() {\n    Ok(value) => total = value + 1\n    Err(problem) => return Err(problem)\n  }\n  total + totals[0]\n}\n",
        );
    }

    // The `Ok(_)` arm dropped a value, which a bare `count()?` may not do.
    #[test]
    fn a_dropped_success_value_stays_dropped() {
        fixed(
            "pure work() -> Result[Int, E] {\n  match count() {\n    Ok(_) => {}\n    Err(problem) => return Err(problem)\n  }\n  1\n}\n",
            "pure work() -> Result[Int, E] {\n  let _ = count()?\n  1\n}\n",
        );
    }

    #[test]
    fn statement_blocks_and_scopes_are_searched() {
        fixed(
            "proc work(items: List[Bool], dir: Path) [env, error] -> Result[Unit, Error] {\n  for item in items {\n    if item {\n      match step(item) {\n        Ok(_) => {}\n        Err(problem) => return Err(problem)\n      }\n    }\n  }\n  cd $dir {\n    match step(false) {\n      Ok(_) => {}\n      Err(problem) => return Err(problem)\n    }\n  }\n}\n",
            "proc work(items: List[Bool], dir: Path) [env, error] -> Result[Unit, Error] {\n  for item in items {\n    if item {\n      step(item)?\n    }\n  }\n  cd $dir {\n    step(false)?\n  }\n}\n",
        );
    }

    #[test]
    fn arms_that_do_anything_else_are_left_alone() {
        // The success arm acts.
        unflagged(
            "proc work() [io, error] -> Result[Int, E] {\n  match step(false) {\n    Ok(_) => print \"ok\"\n    Err(problem) => return Err(problem)\n  }\n  1\n}\n",
        );
        // The error is replaced, wrapped, or not returned.
        unflagged(
            "proc work() [error] -> Result[Int, E] {\n  match step(false) {\n    Ok(_) => {}\n    Err(problem) => return Err(E.Bad(problem.message))\n  }\n  match step(false) {\n    Ok(_) => {}\n    Err(_) => return Err(E.Bad(\"other\"))\n  }\n  match step(false) {\n    Ok(_) => {}\n    Err(problem) => return Ok(problem.message.byte_len())\n  }\n  1\n}\n",
        );
        // A guard makes the arm conditional.
        unflagged(
            "proc work(strict: Bool) [error] -> Result[Int, E] {\n  match step(false) {\n    Ok(_) => {}\n    Err(problem) if strict => return Err(problem)\n    Err(_) => {}\n  }\n  1\n}\n",
        );
    }

    // `return` leaves the function; `?` would stop at the capture.
    #[test]
    fn a_match_inside_a_capture_boundary_is_left_alone() {
        unflagged(
            "proc work() [time, error] -> Result[Int, E] {\n  let captured = try {\n    match step(true) {\n      Ok(_) => {}\n      Err(problem) => return Err(problem)\n    }\n    1\n  }\n  let retried = retry [1ms] {\n    match step(true) {\n      Ok(_) => {}\n      Err(problem) => return Err(problem)\n    }\n    2\n  }\n  (captured ?? 0) + (retried ?? 0)\n}\n",
        );
    }

    // `?` needs the `error` effect this proc's contract does not have.
    #[test]
    fn a_proc_without_the_error_effect_gets_no_rewrite() {
        reported_without_fix(
            "proc work() [fs] -> Result[Int, E] {\n  match step(false) {\n    Ok(_) => {}\n    Err(problem) => return Err(problem)\n  }\n  1\n}\n",
        );
    }

    #[test]
    fn a_comment_or_a_non_call_scrutinee_gets_no_rewrite() {
        reported_without_fix(
            "proc work() [error] -> Result[Int, E] {\n  match step(false) {\n    # nothing to do\n    Ok(_) => {}\n    Err(problem) => return Err(problem)\n  }\n  1\n}\n",
        );
        reported_without_fix(
            "proc work() [error] -> Result[Int, E] {\n  let outcome = step(false)\n  match outcome {\n    Ok(_) => {}\n    Err(problem) => return Err(problem)\n  }\n  1\n}\n",
        );
    }
}
