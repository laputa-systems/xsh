//! `lint.prefer-run-argv`: `process.run(process.command_argv(argv[0], argv))?`
//! rebuilds a command from the vector that already is one; `run.status @argv ?`
//! runs it.

use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};

/// Reports `expr` when it is `process.run(process.command_argv(V[0], V))?`
/// for one binding `V`. The two forms start the same program with the same
/// argv and yield the same `Status`, with a nonzero exit as data and a setup
/// failure propagated by the `?`. They differ only for an empty `V`, which
/// was an index failure and is now `ProcessError.InvalidTarget`.
///
/// `standard_calls` holds the checked standard-library calls by span, so a
/// local binding named `process` is not mistaken for the module.
pub(super) fn run_argv_diagnostic(
    arena: &AstArena,
    source: &str,
    standard_calls: &BTreeMap<Span, (String, String)>,
    expr: ExprId,
) -> Option<Diagnostic> {
    let propagated = arena.expr(expr);
    let ArenaExprKind::Try(run) = propagated.kind else {
        return None;
    };
    let run_args = standard_call_args(arena, standard_calls, run, "run")?;
    let [ArenaCallArgKind::Positional(command)] = run_args.as_slice() else {
        return None;
    };
    let plan_args = standard_call_args(arena, standard_calls, *command, "command_argv")?;
    let [
        ArenaCallArgKind::Positional(target),
        ArenaCallArgKind::Positional(argv),
    ] = plan_args.as_slice()
    else {
        return None;
    };
    let ArenaExprKind::Ident(vector) = arena.expr(*argv).kind else {
        return None;
    };
    let ArenaExprKind::Index {
        base,
        index,
        guarded: false,
    } = arena.expr(*target).kind
    else {
        return None;
    };
    if !matches!(arena.expr(base).kind, ArenaExprKind::Ident(name) if name == vector)
        || !matches!(arena.expr(index).kind, ArenaExprKind::Int(zero) if arena.int_literal(zero).value() == Some(0))
    {
        return None;
    }
    let vector = vector.as_str();
    let replacement = format!("run.status @{} ?", vector.as_str());
    let mut diagnostic = Diagnostic::warning(format!(
        "a command vector is rebuilt to be run; `{replacement}` runs it"
    ))
    .with_code(DiagnosticCode::LintPreferRunArgv)
    .with_label(Label::primary(
        propagated.span,
        "the first element is already the program and `argv[0]`",
    ));
    if is_whole_value(source, propagated.span) {
        diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
            propagated.span,
            "run the vector",
            replacement,
        ));
    } else {
        diagnostic = diagnostic.with_note(
            "a run form is written as the whole value of a binding, assignment, or `return`; bind the status first",
        );
    }
    Some(diagnostic)
}

/// The arguments of `call` when it is the standard call `process.NAME(...)`.
fn standard_call_args(
    arena: &AstArena,
    standard_calls: &BTreeMap<Span, (String, String)>,
    call: ExprId,
    name: &str,
) -> Option<Vec<ArenaCallArgKind>> {
    let expr = arena.expr(call);
    let ArenaExprKind::Call { args, .. } = expr.kind else {
        return None;
    };
    let (module, function) = standard_calls.get(&expr.span)?;
    (module == "process" && function == name).then(|| {
        arena
            .call_args(args)
            .iter()
            .map(|arg| arg.kind.clone())
            .collect()
    })
}

/// Whether `span` is everything after the `=` of a binding or assignment, or
/// after `return`, on one line: the places a run form can replace a call.
fn is_whole_value(source: &str, span: Span) -> bool {
    let (Some(before), Some(after), Some(text)) = (
        source.get(..span.start()),
        source.get(span.end()..),
        source.get(span.range()),
    ) else {
        return false;
    };
    let line = before.rsplit('\n').next().unwrap_or_default().trim_end();
    let rest = after.split('\n').next().unwrap_or_default().trim();
    let introduces_value = line.trim_start() == "return"
        || (line.ends_with('=')
            && !["==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "%="]
                .iter()
                .any(|operator| line.ends_with(operator)));
    introduces_value && rest.is_empty() && !text.contains(['#', '\n'])
}

#[cfg(test)]
#[path = "lint_run_argv_tests.rs"]
mod tests;
