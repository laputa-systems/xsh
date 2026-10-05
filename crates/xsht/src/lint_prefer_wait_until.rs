use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind,
    ArenaExprOrRun, ArenaStmtKind, ArenaSugar, AstArena, BlockId, ExprId, StmtId, SugarForm,
};
use xsh::frontend::syntax::node::{AssignOp, BinaryOp, UnaryOp};

/// A `while` loop that sleeps with `time.sleep` and counts its rounds in a
/// local is a hand-written poll with a limit:
///
/// ```text
/// var tries = 50
/// while tries > 0 {
///   return when path.exists()
///   time.sleep(100ms)
///   tries -= 1
/// }
/// fail "the device did not appear"
/// ```
///
/// `wait until path.exists() within 5000ms every 100ms` states the condition
/// and the limit. It is never the same program, so this is a note and
/// `--fix` never applies it:
/// the statement fails with `Timeout` where the loop fails with the error it
/// wrote, and its limit is elapsed time where the loop's is a count of
/// sleeps. The rewrite is shown for the two plain shapes that fail when they
/// give up. A loop that falls through to the code after it gets no rewrite
/// at all: the statement would fail where that loop goes on.
///
/// Only the statements of the loop body itself are read: the sleep and the
/// count must both be among them.
pub(super) fn lint_polling_loops(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    let arena = linter.arena;
    let source = linter.source;
    for (index, stmt) in stmts.iter().enumerate() {
        let ArenaStmtKind::While { condition, block } = arena.stmt(*stmt).kind else {
            continue;
        };
        let body: Vec<StmtId> = arena.stmt_ids(arena.block(block).statements).collect();
        let Some(poll) = polling_body(arena, &body) else {
            continue;
        };
        let loop_span = arena.stmt(*stmt).span;
        let head = Span::new(
            loop_span.source_id,
            loop_span.start(),
            arena.expr(condition).span.end(),
        );
        let rewrite = rewrite(arena, source, stmts, index, condition, &body, &poll);
        let fails_after = stmts
            .get(index + 1)
            .is_some_and(|next| statement_fails(arena, *next));
        let fails_inside = body.iter().any(|stmt| match arena.stmt(*stmt).kind {
            ArenaStmtKind::If { branches, else_block: None } => arena
                .if_branches(branches)
                .iter()
                .any(|branch| block_fails(arena, branch.block)),
            _ => false,
        });
        let mut diagnostic = Diagnostic::new(
            Severity::Note,
            "this loop polls with `time.sleep` and a count; `wait until CONDITION within LIMIT every INTERVAL` states the condition and the limit",
        )
        .with_code(DiagnosticCode::LintPreferWaitUntil)
        .with_label(Label::primary(head, "a polling loop with a limit"));
        diagnostic = match rewrite {
            Some(rewrite) => diagnostic
                .with_note(
                    "`--fix` does not apply the rewrite, because it is not the same program: the statement fails with `Timeout` in place of the failure written here, and its limit is elapsed time, not a count of sleeps",
                )
                .with_fix_hint(
                    FixHint::replacement(rewrite.span, "rewrite as `wait until`", rewrite.text)
                        .dangerous(),
                ),
            None if fails_after || fails_inside => diagnostic.with_note(
                "no rewrite is shown: the loop is not the plain test, sleep, and count with literal limits. The statement fails with `Timeout` in place of the failure written here",
            ),
            None => diagnostic.with_note(
                "no rewrite is shown: when this loop gives up it goes on to the code after it, and `wait until` fails with `Timeout` instead, so the rewrite would change what a timeout does",
            ),
        };
        linter.diagnostics.push(diagnostic);
    }
}

/// The sleep and the count of a polling loop's body.
struct Poll {
    /// Where the `time.sleep` statement is in the body.
    sleep: usize,
    /// The sleep's argument.
    interval: ExprId,
    /// Where the statement that counts a round is in the body.
    count: usize,
    counter: xsh::frontend::symbols::Name,
    /// The count goes down.
    down: bool,
}

/// The body's one `time.sleep(INTERVAL)` statement and its one
/// `COUNTER += 1` or `COUNTER -= 1` statement, when it has exactly those.
fn polling_body(arena: &AstArena, body: &[StmtId]) -> Option<Poll> {
    let mut sleep = None;
    let mut count = None;
    for (index, stmt) in body.iter().enumerate() {
        match arena.stmt(*stmt).kind {
            ArenaStmtKind::Expr(expr) => {
                if let Some(interval) = sleep_interval(arena, expr) {
                    if sleep.is_some() {
                        return None;
                    }
                    sleep = Some((index, interval));
                }
            }
            ArenaStmtKind::Assign {
                target,
                op: op @ (AssignOp::Add | AssignOp::Sub),
                value: ArenaExprOrRun::Expr(value),
            } => {
                let ArenaAssignTargetKind::Name(name) = arena.assign_target(target).kind else {
                    continue;
                };
                if int_value(arena, value) != Some(1) {
                    continue;
                }
                if count.is_some() {
                    return None;
                }
                count = Some((index, name, op == AssignOp::Sub));
            }
            _ => {}
        }
    }
    let (sleep, interval) = sleep?;
    let (count, counter, down) = count?;
    Some(Poll {
        sleep,
        interval,
        count,
        counter,
        down,
    })
}

/// The argument of `time.sleep(ARG)` or `time.sleep(ARG)?`.
fn sleep_interval(arena: &AstArena, expr: ExprId) -> Option<ExprId> {
    let expr = match arena.expr(expr).kind {
        ArenaExprKind::Try(inner) => inner,
        _ => expr,
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if name != "sleep" || !matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "time")
    {
        return None;
    }
    let [argument] = arena.call_args(args) else {
        return None;
    };
    match argument.kind {
        ArenaCallArgKind::Positional(interval) => Some(interval),
        _ => None,
    }
}

fn int_value(arena: &AstArena, expr: ExprId) -> Option<i64> {
    match arena.expr(expr).kind {
        ArenaExprKind::Int(literal) => arena.int_literal(literal).value(),
        _ => None,
    }
}

fn is_name(arena: &AstArena, expr: ExprId, name: xsh::frontend::symbols::Name) -> bool {
    matches!(arena.expr(expr).kind, ArenaExprKind::Ident(found) if found == name)
}

/// Whether the statement states a failure: `fail ...`, `return Err(...)`, or
/// a bare `Err(...)`.
fn statement_fails(arena: &AstArena, stmt: StmtId) -> bool {
    let is_err = |value: ArenaExprOrRun| {
        let ArenaExprOrRun::Expr(expr) = value else {
            return false;
        };
        matches!(
            arena.expr(expr).kind,
            ArenaExprKind::Call { callee, .. }
                if matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Err")
        )
    };
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Sugar {
            form: SugarForm::Fail,
            ..
        } => true,
        ArenaStmtKind::Return(Some(value)) => is_err(value),
        ArenaStmtKind::Expr(expr) => is_err(ArenaExprOrRun::Expr(expr)),
        _ => false,
    }
}

/// Whether the block's last statement states a failure.
fn block_fails(arena: &AstArena, block: BlockId) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .last()
        .is_some_and(|last| statement_fails(arena, last))
}

struct Rewrite {
    span: Span,
    text: String,
}

/// The statement one of the two plain shapes is, when every limit is a
/// literal and no comment would be lost.
fn rewrite(
    arena: &AstArena,
    source: &str,
    stmts: &[StmtId],
    index: usize,
    condition: ExprId,
    body: &[StmtId],
    poll: &Poll,
) -> Option<Rewrite> {
    let [first, second, third] = body else {
        return None;
    };
    if poll.sleep != 1 || poll.count != 2 {
        return None;
    }
    let _ = (second, third);
    let declared = declared_count(arena, *stmts.get(index.checked_sub(1)?)?, poll.counter)?;
    if !matches!(arena.expr(poll.interval).kind, ArenaExprKind::Duration(_)) {
        return None;
    }
    let text = |expr: ExprId| source.get(arena.expr(expr).span.range());
    let loop_span = arena.stmt(stmts[index]).span;
    let start = arena.stmt(stmts[index - 1]).span.start();
    let (rounds, waited, end, tail) = if poll.down {
        // var N = ROUNDS; while N > 0 { return when C; sleep; N -= 1 }; FAIL
        let ArenaExprKind::Binary {
            op: BinaryOp::Gt,
            left,
            right,
        } = arena.expr(condition).kind
        else {
            return None;
        };
        if !is_name(arena, left, poll.counter) || int_value(arena, right) != Some(0) {
            return None;
        }
        let waited = returns_when(arena, *first)?;
        let after = *stmts.get(index + 1)?;
        if !statement_fails(arena, after) {
            return None;
        }
        (declared, waited, arena.stmt(after).span.end(), "\nreturn")
    } else {
        // var N = 0; while ! C { if N >= ROUNDS { FAIL }; sleep; N += 1 }
        if declared != 0 {
            return None;
        }
        let ArenaExprKind::Unary {
            op: UnaryOp::Not,
            expr: waited,
        } = arena.expr(condition).kind
        else {
            return None;
        };
        let ArenaStmtKind::If {
            branches,
            else_block: None,
        } = arena.stmt(*first).kind
        else {
            return None;
        };
        let [branch] = arena.if_branches(branches) else {
            return None;
        };
        let ArenaExprKind::Binary {
            op: BinaryOp::Ge,
            left,
            right,
        } = arena.expr(branch.condition).kind
        else {
            return None;
        };
        if !is_name(arena, left, poll.counter) {
            return None;
        }
        let gave_up: Vec<StmtId> = arena.stmt_ids(arena.block(branch.block).statements).collect();
        let [only] = gave_up[..] else {
            return None;
        };
        if !statement_fails(arena, only) {
            return None;
        }
        (int_value(arena, right)?, waited, loop_span.end(), "")
    };
    // A statement's span may take in the line break that ends it.
    let end = start + source.get(start..end)?.trim_end().len();
    // The limit is the interval that many times, in the interval's unit.
    let interval = text(poll.interval)?;
    let unit = interval.trim_start_matches(|digit: char| digit.is_ascii_digit());
    let amount: u64 = interval[..interval.len() - unit.len()].parse().ok()?;
    let limit = amount.checked_mul(u64::try_from(rounds).ok()?)?;
    let span = Span::new(loop_span.source_id, start, end);
    // A comment in the loop would be dropped, and the count must not be read
    // by anything the rewrite keeps.
    if source.get(span.range())?.contains('#') {
        return None;
    }
    let counter = poll.counter.as_str();
    let rest = source.get(end..arena.stmt(*stmts.last()?).span.end())?;
    if mentions(text(waited)?, &counter) || mentions(rest, &counter) {
        return None;
    }
    let indent = line_indent(source, start);
    let tail = tail.replace('\n', &format!("\n{indent}"));
    Some(Rewrite {
        span,
        text: format!(
            "wait until {} within {limit}{unit} every {interval}{tail}",
            text(waited)?
        ),
    })
}

/// The whitespace that begins the line of `offset`.
fn line_indent(source: &str, offset: usize) -> &str {
    let line = source[..offset].rfind('\n').map_or(0, |newline| newline + 1);
    let text = &source[line..offset];
    &text[..text.len() - text.trim_start().len()]
}

/// The integer `var NAME = INT` declares.
fn declared_count(arena: &AstArena, stmt: StmtId, name: xsh::frontend::symbols::Name) -> Option<i64> {
    let ArenaStmtKind::Var {
        target,
        initializer: ArenaExprOrRun::Expr(value),
        ..
    } = arena.stmt(stmt).kind
    else {
        return None;
    };
    match arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(found) if found == name => int_value(arena, value),
        _ => None,
    }
}

/// The condition of `return when CONDITION`.
fn returns_when(arena: &AstArena, stmt: StmtId) -> Option<ExprId> {
    let ArenaStmtKind::Sugar {
        form: form @ SugarForm::When,
        operands,
        ..
    } = arena.stmt(stmt).kind
    else {
        return None;
    };
    match arena.sugar(form, operands) {
        ArenaSugar::Guarded {
            stmt: guarded,
            negate: false,
            condition,
        } if matches!(arena.stmt(guarded).kind, ArenaStmtKind::Return(None)) => Some(condition),
        _ => None,
    }
}

/// Whether `text` holds `name` as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    text.match_indices(name).any(|(at, _)| {
        let before = at.checked_sub(1).map(|index| text.as_bytes()[index]);
        let after = text.as_bytes().get(at + name.len()).copied();
        !before.is_some_and(is_word) && !after.is_some_and(is_word)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn polling_loops(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Linter::lint(&parsed.arena, source, LintOptions::default())
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferWaitUntil))
            .collect()
    }

    fn rewritten(source: &str) -> String {
        let diagnostics = polling_loops(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let [hint] = &diagnostics[0].fix_hints[..] else {
            panic!("no rewrite: {diagnostics:?}");
        };
        assert!(hint.dangerous, "`--fix` must not apply the rewrite");
        let span = hint.span.unwrap();
        let mut text = source.to_string();
        text.replace_range(span.range(), hint.replacement.as_deref().unwrap());
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &text);
        assert!(parsed.diagnostics.is_empty(), "{text}\n{:?}", parsed.diagnostics);
        text
    }

    #[test]
    fn the_rule_reports_a_note_by_default() {
        let source = "proc settle(path: Path) [fs, time, error] {\n  var tries = 50\n  while tries > 0 {\n    return when path.exists()\n    time.sleep(100ms)\n    tries -= 1\n  }\n  fail \"missing\"\n}\n";
        let diagnostics = polling_loops(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, xsh::diagnostic::Severity::Note);
    }

    #[test]
    fn a_countdown_that_fails_after_the_loop_shows_its_statement() {
        let source = "proc settle(path: Path) [fs, time, error] {\n  var tries = 50\n\n  while tries > 0 {\n    return when path.exists()\n\n    time.sleep(100ms)\n    tries -= 1\n  }\n\n  return Err(error.failure(\"missing\"))\n}\n";
        assert_eq!(
            rewritten(source),
            "proc settle(path: Path) [fs, time, error] {\n  wait until path.exists() within 5000ms every 100ms\n  return\n}\n"
        );
        let diagnostics = polling_loops(source);
        assert!(diagnostics[0].notes[0].contains("`--fix` does not apply the rewrite"));
        assert!(diagnostics[0].notes[0].contains("`Timeout`"));
    }

    #[test]
    fn a_count_up_that_fails_inside_the_loop_shows_its_statement() {
        let source = "proc settle(ready: Proc) [time, error] {\n  var waited = 0\n  while ! ready.call() {\n    if waited >= 600 {\n      fail \"the mirror did not start\"\n    }\n    time.sleep(1s)?\n    waited += 1\n  }\n  print \"up\"\n}\n";
        assert_eq!(
            rewritten(source),
            "proc settle(ready: Proc) [time, error] {\n  wait until ready.call() within 600s every 1s\n  print \"up\"\n}\n"
        );
    }

    #[test]
    fn a_loop_that_falls_through_gets_no_rewrite_and_says_why() {
        // The countdown ends without a failure, and the code after it runs.
        let source = "proc settle(path: Path) [fs, time] {\n  var tries = 50\n  while tries > 0 {\n    return when path.exists()\n    time.sleep(100ms)\n    tries -= 1\n  }\n  print \"gave up\"\n}\n";
        let diagnostics = polling_loops(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
        assert!(diagnostics[0].notes[0].contains("goes on to the code after it"));
        assert!(diagnostics[0].notes[0].contains("change what a timeout does"));
    }

    #[test]
    fn a_loud_loop_that_is_not_plain_gets_no_rewrite() {
        for source in [
            // A limit that is a name has no literal product.
            "proc settle(path: Path, seconds: Int) [fs, time, error] {\n  var waited = 0\n  while ! path.exists() {\n    if waited >= seconds {\n      fail \"missing\"\n    }\n    time.sleep(1s)\n    waited += 1\n  }\n}\n",
            // More happens when it gives up than the failure.
            "proc settle(path: Path) [fs, time, error] {\n  var waited = 0\n  while ! path.exists() {\n    if waited >= 5 {\n      print \"late\"\n      fail \"missing\"\n    }\n    time.sleep(1s)\n    waited += 1\n  }\n}\n",
            // A comment would be dropped.
            "proc settle(path: Path) [fs, time, error] {\n  var tries = 5\n  while tries > 0 {\n    return when path.exists()\n    time.sleep(1s) # settle\n    tries -= 1\n  }\n  fail \"missing\"\n}\n",
            // The count is read afterwards.
            "proc settle(path: Path) [fs, time, error] -> Result[Int] {\n  var waited = 0\n  while ! path.exists() {\n    if waited >= 5 {\n      fail \"missing\"\n    }\n    time.sleep(1s)\n    waited += 1\n  }\n  Ok(waited)\n}\n",
        ] {
            let diagnostics = polling_loops(source);
            assert_eq!(diagnostics.len(), 1, "{source}\n{diagnostics:?}");
            assert!(diagnostics[0].fix_hints.is_empty(), "{source}");
            assert!(diagnostics[0].notes[0].contains("not the plain test"), "{source}");
        }
    }

    #[test]
    fn loops_that_do_not_poll_with_a_count_are_left_alone() {
        for source in [
            // An idle loop has no limit.
            "while true {\n  time.sleep(60s)\n}\n",
            // A count without a sleep is not a poll.
            "var n = 0\nwhile n < 5 {\n  n += 1\n}\n",
            // The sleep is not a statement of the loop body.
            "var n = 0\nwhile n < 5 {\n  if n > 2 {\n    time.sleep(1s)\n  }\n  n += 1\n}\n",
        ] {
            assert!(polling_loops(source).is_empty(), "{source}");
        }
    }
}
