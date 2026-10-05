//! `lint.prefer-exit`: `abort(STATUS)` is the statement `exit STATUS`.

use rustc_hash::FxHashSet;
use std::collections::BTreeSet;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};

/// The `abort` calls the linter's statement walker has already reported, so
/// the expression traversal that reaches them next does not report them
/// again.
#[derive(Default)]
pub(super) struct ExitStatements {
    reported: FxHashSet<ExprId>,
}

impl ExitStatements {
    /// Reports the expression statement `expr` when it is `abort(STATUS)`.
    /// `exit STATUS` is defined as that call, so the rewrite is exact.
    pub(super) fn visit_statement(
        &mut self,
        arena: &AstArena,
        source: &str,
        terminating_calls: &BTreeSet<Span>,
        expr: ExprId,
    ) -> Option<Diagnostic> {
        let status = abort_status(arena, terminating_calls, expr)?;
        self.reported.insert(expr);
        let call = arena.expr(expr).span;
        let diagnostic = diagnostic(call);
        let text = source.get(arena.expr(status).span.range())?;
        Some(if text.contains(['#', '\n']) {
            diagnostic
        } else {
            diagnostic.with_fix_hint(FixHint::replacement(
                call,
                "use the `exit` statement",
                format!("exit {text}"),
            ))
        })
    }

    /// Reports `abort(STATUS)` used as a value inside a larger expression.
    /// `exit` is a statement, so that needs a rewrite by hand.
    pub(super) fn visit_expr(
        &mut self,
        arena: &AstArena,
        terminating_calls: &BTreeSet<Span>,
        expr: ExprId,
    ) -> Option<Diagnostic> {
        if self.reported.remove(&expr) {
            return None;
        }
        abort_status(arena, terminating_calls, expr)?;
        Some(diagnostic(arena.expr(expr).span).with_note(
            "`exit` is a statement; give the exit a statement of its own, for example in a block",
        ))
    }
}

fn diagnostic(call: Span) -> Diagnostic {
    Diagnostic::warning("`abort(STATUS)` is written `exit STATUS`")
        .with_code(DiagnosticCode::LintPreferExit)
        .with_label(Label::primary(
            call,
            "a deliberate exit, with deferred cleanup",
        ))
}

/// The status of `expr` when it is the built-in `abort` called with a status
/// and nothing else. `abort(status, force: true)` skips deferred cleanup,
/// which `exit` never does. `terminating_calls` holds the calls the checker
/// resolved to a built-in that never returns.
fn abort_status(
    arena: &AstArena,
    terminating_calls: &BTreeSet<Span>,
    expr: ExprId,
) -> Option<ExprId> {
    let call = arena.expr(expr);
    let ArenaExprKind::Call { callee, args } = call.kind else {
        return None;
    };
    if !matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "abort")
        || !terminating_calls.contains(&call.span)
        || arena.expr_is_synthetic(expr)
    {
        return None;
    }
    match arena.call_args(args) {
        [argument] => match argument.kind {
            ArenaCallArgKind::Positional(status) => Some(status),
            ArenaCallArgKind::Named { name, value, .. } if name == "status" => Some(value),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
#[path = "lint_exit_tests.rs"]
mod tests;
