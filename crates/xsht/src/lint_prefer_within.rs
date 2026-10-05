use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCommand, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, AstArena, ExprId, RunFormId,
    StmtId,
};

/// A statement list whose `run` forms all carry the same `--timeout` repeats
/// one limit on every command. `within LIMIT { ... }` states a limit once,
/// for the list as a whole:
///
/// ```text
/// run --timeout=30s make fetch
/// let rev = run.text --timeout=30s git rev-parse HEAD ?
/// ```
///
/// The two are not the same program, so this is a note with no rewrite: each
/// `--timeout` allows its own command the limit and fails as a `ProcessError`,
/// while the scope allows all of them the limit together, also bounds what
/// runs between them, and reports a `Timeout` as its `Result`.
///
/// Only the statements of the list itself count, each of them a `run` form
/// alone or the initializer of a binding; a list needs two to be reported.
pub(super) fn lint_repeated_timeouts(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    let arena = linter.arena;
    let source = linter.source;
    let mut runs = stmts.iter().filter_map(|stmt| run_statement(arena, *stmt));
    let Some(first) = runs.next() else {
        return;
    };
    let Some(limit) = shared_timeout(arena, source, first, None) else {
        return;
    };
    let mut count = 1;
    for run in runs {
        if shared_timeout(arena, source, run, Some(limit)).is_none() {
            return;
        }
        count += 1;
    }
    if count < 2 {
        return;
    }
    let span = arena.span(arena.run_form(first).span);
    linter.diagnostics.push(
        Diagnostic::new(
            Severity::Warning,
            format!("every `run` in this block carries `--timeout={limit}`"),
        )
        .with_code(DiagnosticCode::LintPreferWithin)
        .with_label(Label::primary(
            span,
            format!("the first of {count} commands with the same limit"),
        ))
        .with_note(format!(
            "`within {limit} {{ ... }}` states the limit once, but means something else: it allows the commands {limit} together instead of {limit} each, and reports the deadline as its `Result`"
        )),
    );
}

/// The `run` form a statement is: alone, under `?`, or bound by `let` or
/// `var`.
fn run_statement(arena: &AstArena, stmt: StmtId) -> Option<RunFormId> {
    let run_expr = |expr: ExprId| {
        let expr = match arena.expr(expr).kind {
            ArenaExprKind::Try(inner) => inner,
            _ => expr,
        };
        match arena.expr(expr).kind {
            ArenaExprKind::Run(run) => Some(run),
            _ => None,
        }
    };
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Command(command) => match arena.command_stmt(command).command {
            ArenaCommand::Run(run) => Some(run),
            _ => None,
        },
        ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Var { initializer, .. } => {
            match initializer {
                ArenaExprOrRun::Run(run) => Some(run),
                ArenaExprOrRun::Expr(expr) => run_expr(expr),
            }
        }
        ArenaStmtKind::Expr(expr) => run_expr(expr),
        _ => None,
    }
}

/// The text of the `--timeout` every segment of `run` carries, when it is
/// one text and, given `expected`, that one.
fn shared_timeout<'a>(
    arena: &AstArena,
    source: &'a str,
    run: RunFormId,
    expected: Option<&'a str>,
) -> Option<&'a str> {
    let mut shared = expected;
    let segments = arena.run_segments(arena.run_form(run).segments);
    if segments.is_empty() {
        return None;
    }
    for segment in segments {
        let span: Span = arena.expr(segment.timeout?).span;
        let text = source.get(span.range())?.trim();
        if shared.is_some_and(|shared| shared != text) {
            return None;
        }
        shared = Some(text);
    }
    shared
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn repeated_timeouts(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Linter::lint(&parsed.arena, source, LintOptions::default())
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferWithin))
            .collect()
    }

    #[test]
    fn a_block_whose_runs_share_one_timeout_is_noted_without_a_rewrite() {
        let source = "proc sync() [process, error] -> Result[Str] {\n  print \"syncing\"\n  run --timeout=30s git fetch\n  let rev = run.text --timeout=30s git rev-parse HEAD ?\n  run --timeout=30s git gc ?\n  rev\n}\n";
        let diagnostics = repeated_timeouts(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "every `run` in this block carries `--timeout=30s`"
        );
        let label = diagnostics[0].labels[0].span;
        assert!(source[label.range()].starts_with("run --timeout=30s git fetch"));
        assert!(diagnostics[0].labels[0].message.as_deref().unwrap().contains("3 commands"));
        assert!(diagnostics[0].notes[0].contains("within 30s { ... }"));
        assert!(diagnostics[0].fix_hints.is_empty());
    }

    #[test]
    fn a_named_limit_and_the_statements_of_a_file_count() {
        let source = "let limit = 5s\nrun --timeout=limit true\nrun --timeout=limit false\n";
        let diagnostics = repeated_timeouts(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].message.contains("--timeout=limit"));
    }

    #[test]
    fn other_blocks_are_left_alone() {
        for source in [
            // One command, different limits, and a command without one.
            "run --timeout=30s git fetch\n",
            "run --timeout=30s git fetch\nrun --timeout=10s git gc\n",
            "run --timeout=30s git fetch\nrun git gc\n",
            // A pipeline whose segments do not all carry the limit.
            "run --timeout=30s git log | run wc -l\nrun --timeout=30s git gc\n",
            // Commands in other blocks are other lists.
            "run --timeout=30s git fetch\nif true {\n  run --timeout=30s git gc\n}\n",
        ] {
            assert!(repeated_timeouts(source).is_empty(), "{source}");
        }
    }
}
