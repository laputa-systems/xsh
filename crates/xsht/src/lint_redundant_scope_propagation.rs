use super::lint_redundant_propagation::{PropagationFacts, redundant_statement_try};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::StatementPosition;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCommand, ArenaExprKind, ArenaStmtKind, AstArena, StmtId};
use xsh::frontend::syntax::node::CoreCommand;

/// `cd dir { ... }?` and `env (overlay) { ... }?` as statements spell
/// propagation twice. A block-valued form that is a statement-position
/// `Result[Unit]` propagates by itself, whether it is a `cd` or `env` scope, a
/// `try` capture, or a `retry`.
///
/// The parenthesized scopes, `try`, and `retry` are expressions and use the
/// same checked facts as a call statement. The bare `cd PATH { }` and
/// `env NAME=value { }` forms are command statements: the checker gives them
/// `Result[Unit]` always and propagates it in every position, so a trailing
/// `?` on one never changes the program.
pub(super) fn redundant_scope_propagation(
    arena: &AstArena,
    source: &str,
    facts: &PropagationFacts<'_>,
    statement: StmtId,
) -> Option<Diagnostic> {
    let removal = match redundant_statement_try(arena, source, facts, statement) {
        Some(found) => matches!(
            arena.expr(found.operand).kind,
            ArenaExprKind::ContextScope { .. }
                | ArenaExprKind::Capture(_)
                | ArenaExprKind::Retry { .. }
        )
        .then_some(found.removal)?,
        None => command_scope_propagation(arena, source, facts, statement)?,
    };
    Some(
        Diagnostic::warning("`?` on a statement scope that already propagates its failure")
            .with_code(DiagnosticCode::LintRedundantScopePropagation)
            .with_label(Label::secondary(
                removal,
                "a statement-position `Result[Unit]` propagates without `?`",
            ))
            .with_fix_hint(FixHint::deletion(removal, "remove `?`")),
    )
}

/// The `?` after the block of a `cd PATH { }` or `env NAME=value { }`
/// command statement, with any blanks before it.
fn command_scope_propagation(
    arena: &AstArena,
    source: &str,
    facts: &PropagationFacts<'_>,
    statement: StmtId,
) -> Option<Span> {
    let statement = arena.stmt(statement);
    let ArenaStmtKind::Command(command) = statement.kind else {
        return None;
    };
    let command = arena.command_stmt(command);
    let ArenaCommand::Core {
        name: CoreCommand::Cd | CoreCommand::Env,
        block: Some(block),
        ..
    } = command.command
    else {
        return None;
    };
    if !command.propagate
        || facts.statement_positions.get(&statement.span) != Some(&StatementPosition::Statement)
    {
        return None;
    }
    let block = arena.span(arena.block(block).span);
    // A command statement's span runs through its terminator.
    let after_block = source.get(block.end()..statement.span.end())?;
    let propagation = after_block.trim_start_matches([' ', '\t']);
    let blanks = after_block.len() - propagation.len();
    propagation
        .strip_prefix('?')
        .is_some_and(|rest| rest.trim_matches([' ', '\t', '\r', '\n', ';']).is_empty())
        .then(|| {
            Span::new(
                statement.span.source_id,
                block.end(),
                block.end() + blanks + 1,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::super::lint_redundant_propagation::tests::{apply, lint_rule};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintRedundantScopePropagation)
    }

    const PRELUDE: &str =
        "proc step() -> Result[Unit] {\n}\n\nproc count() -> Result[Int] {\n  1\n}\n\n";

    fn fixed(body: &str, expected: &str) {
        let source = format!("{PRELUDE}{body}");
        let diagnostics = lint(&source);
        assert!(!diagnostics.is_empty(), "{body}");
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintRedundantScopePropagation)
                && diagnostic.fix_hints.len() == 1
        }));
        let after = apply(&diagnostics, &source);
        assert_eq!(after, format!("{PRELUDE}{expected}"));
        // The fixed program checks and has nothing left to report.
        assert!(lint(&after).is_empty(), "{after}");
    }

    fn unflagged(body: &str) {
        let source = format!("{PRELUDE}{body}");
        let diagnostics = lint(&source);
        assert!(diagnostics.is_empty(), "{body}\n{diagnostics:?}");
    }

    #[test]
    fn statement_scopes_lose_their_redundant_propagation() {
        fixed(
            "proc work(dir: Path) [env, error] -> Result[Int] {\n  cd $dir {\n    step()?\n  }?\n  cd (dir) {\n    step()?\n  } ?\n  env MODE=fast {\n    step()?\n  }?\n  env ({MODE: \"fast\"}) {\n    step()?\n  }?\n  1\n}\n",
            "proc work(dir: Path) [env, error] -> Result[Int] {\n  cd $dir {\n    step()?\n  }\n  cd (dir) {\n    step()?\n  }\n  env MODE=fast {\n    step()?\n  }\n  env ({MODE: \"fast\"}) {\n    step()?\n  }\n  1\n}\n",
        );
    }

    #[test]
    fn statement_captures_and_retries_lose_their_redundant_propagation() {
        fixed(
            "proc work() [time, error] -> Result[Int] {\n  try {\n    step()?\n  }?\n  retry [1ms] {\n    step()?\n  }?\n  1\n}\n",
            "proc work() [time, error] -> Result[Int] {\n  try {\n    step()?\n  }\n  retry [1ms] {\n    step()?\n  }\n  1\n}\n",
        );
    }

    // A command scope propagates in every position, so its tail `?` goes too.
    #[test]
    fn a_command_scope_at_a_unit_result_tail_loses_its_propagation() {
        fixed(
            "proc work(dir: Path) [env, error] -> Result[Unit] {\n  cd $dir {\n    step()?\n  }?\n}\n",
            "proc work(dir: Path) [env, error] -> Result[Unit] {\n  cd $dir {\n    step()?\n  }\n}\n",
        );
    }

    #[test]
    fn a_scope_whose_value_is_used_keeps_its_propagation() {
        unflagged(
            "proc work(dir: Path) [env, error] -> Result[Int] {\n  let inside = cd (dir) {\n    count()?\n  }?\n  let captured = try {\n    count()?\n  }?\n  inside + captured\n}\n",
        );
        // A value-producing scope is not a `Result[Unit]`.
        unflagged(
            "proc work(dir: Path) [env, error] -> Result[Int] {\n  cd (dir) {\n    count()?\n  }?\n}\n",
        );
    }

    // Without `?` an expression scope at the tail would be the function's
    // value rather than a propagating statement.
    #[test]
    fn an_expression_scope_at_a_result_tail_keeps_its_propagation() {
        unflagged(
            "proc work(dir: Path) [env, error] -> Result[Unit] {\n  cd (dir) {\n    step()?\n  }?\n}\n",
        );
        unflagged("proc work() [error] -> Result[Unit] {\n  try {\n    step()?\n  }?\n}\n");
    }

    #[test]
    fn a_scope_without_propagation_is_left_alone() {
        unflagged(
            "proc work(dir: Path) [env, error] -> Result[Int] {\n  cd $dir {\n    step()?\n  }\n  env ({MODE: \"fast\"}) {\n    step()?\n  }\n  1\n}\n",
        );
    }
}
