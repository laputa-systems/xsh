use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::{BindingId, BindingKind, CheckOutput};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, AstArena,
    DeferTrigger, ExprId, StmtId,
};

/// Record checked local uses as the ordinary expression descent reaches them.
pub(super) fn record_binding_use(linter: &mut super::Linter<'_>, expr: ExprId) {
    if !linter
        .rule_selection
        .accepts(Some(DiagnosticCode::LintRedundantClose))
    {
        return;
    }
    let Some(checked) = linter.checked_output.as_deref() else {
        return;
    };
    if let Some(&binding_id) = checked.resolved_bindings.expressions.get(&expr) {
        linter
            .checked_binding_uses
            .entry(binding_id)
            .or_default()
            .insert(expr);
    }
    let ArenaExprKind::Call { callee, .. } = linter.arena.expr(expr).kind else {
        return;
    };
    let ArenaExprKind::Field { base, name } = linter.arena.expr(callee).kind else {
        return;
    };
    if !is_borrowed_root_method(name.as_str().as_str()) {
        return;
    }
    if let Some(&binding_id) = checked.resolved_bindings.expressions.get(&base) {
        linter
            .borrowed_root_receivers
            .entry(binding_id)
            .or_default()
            .insert(base);
    }
}

/// The scope releases a locally created root after its deferred actions.
/// Removing the explicit defer is safe only when that close is the last
/// registered cleanup and every other use remains a borrowed receiver.
pub(super) fn lint_deferred_root_closes(
    linter: &mut super::Linter<'_>,
    stmts: &[StmtId],
) -> Vec<Diagnostic> {
    let code = DiagnosticCode::LintRedundantClose;
    if !linter.rule_selection.accepts(Some(code)) {
        return Vec::new();
    }
    let Some(checked) = linter.checked_output.as_deref() else {
        return Vec::new();
    };
    let Some(&first) = stmts.first() else {
        return Vec::new();
    };
    let Some(&last) = stmts.last() else {
        return Vec::new();
    };
    let arena = linter.arena;
    let scope_span = Span::new(
        arena.stmt(first).span.source_id,
        arena.stmt(first).span.start(),
        arena.stmt(last).span.end(),
    );
    if check_has_errors_in_source(checked, scope_span.source_id) {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for (defer_index, &defer_stmt) in stmts.iter().enumerate() {
        let Some((name, receiver)) = deferred_close(arena, defer_stmt) else {
            continue;
        };
        if stmts[..defer_index]
            .iter()
            .any(|stmt| matches!(arena.stmt(*stmt).kind, ArenaStmtKind::Defer(..)))
        {
            continue;
        }
        let Some((binding_stmt, binding_id)) = local_root_binding(
            arena,
            checked,
            &stmts[..defer_index],
            name,
            receiver,
        ) else {
            continue;
        };
        if !uses_stay_borrowed(
            linter,
            scope_span,
            binding_id,
            receiver,
        ) {
            continue;
        }

        let root_name = name.as_str();
        let span = arena.stmt(defer_stmt).span;
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("`{root_name}.close()` is redundant at scope exit"),
            )
            .with_code(code)
            .with_label(Label::primary(
                span,
                "scope cleanup closes this local root after its deferred actions",
            ))
            .with_label(Label::secondary(
                arena.stmt(binding_stmt).span,
                "this root is registered for scope cleanup",
            ))
            .with_note("Scope cleanup skips a root that was already explicitly closed.")
            .with_fix_hint(FixHint::replacement(
                span,
                "remove the explicit close and let scope cleanup release the root",
                String::new(),
            )),
        );
    }
    diagnostics
}

fn check_has_errors_in_source(
    checked: &CheckOutput,
    source_id: xsh::frontend::source::SourceId,
) -> bool {
    checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && (diagnostic.labels.is_empty()
                || diagnostic
                    .labels
                    .iter()
                    .any(|label| label.span.source_id == source_id))
    })
}

fn deferred_close(arena: &AstArena, stmt: StmtId) -> Option<(Name, ExprId)> {
    let ArenaStmtKind::Defer(ArenaExprOrRun::Expr(action), DeferTrigger::Exit) =
        arena.stmt(stmt).kind
    else {
        return None;
    };
    let call = match arena.expr(action).kind {
        ArenaExprKind::Try(inner) => inner,
        _ => action,
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(call).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    (name.as_str() == "close"
        && arena.call_args(args).is_empty()
        && matches!(arena.expr(base).kind, ArenaExprKind::Ident(_)))
    .then(|| {
        let ArenaExprKind::Ident(name) = arena.expr(base).kind else {
            unreachable!();
        };
        (name, base)
    })
}

fn local_root_binding(
    arena: &AstArena,
    checked: &CheckOutput,
    preceding: &[StmtId],
    name: Name,
    receiver: ExprId,
) -> Option<(StmtId, BindingId)> {
    for &stmt in preceding.iter().rev() {
        let node = arena.stmt(stmt);
        let ArenaStmtKind::Let {
            target,
            initializer,
            ..
        } = node.kind
        else {
            continue;
        };
        let ArenaBindingTargetKind::Name(binding_name) = arena.binding_target(target).kind else {
            continue;
        };
        if binding_name != name {
            continue;
        }
        if checked.local_binding_types.get(&node.span) != Some(&super::Type::FsRoot)
            || !initializes_local_root(arena, checked, initializer)
        {
            return None;
        }
        let binding_id = checked
            .resolved_bindings
            .declaration_sites
            .get(&(node.span, name))
            .copied()?;
        let declaration = checked.resolved_bindings.declarations.get(&binding_id)?;
        if declaration.kind != BindingKind::Runtime
            || declaration.global
            || declaration.exported
            || checked.resolved_bindings.expressions.get(&receiver) != Some(&binding_id)
        {
            return None;
        }
        return Some((stmt, binding_id));
    }
    None
}

fn initializes_local_root(
    arena: &AstArena,
    checked: &CheckOutput,
    initializer: ArenaExprOrRun,
) -> bool {
    let ArenaExprOrRun::Expr(initializer) = initializer else {
        return false;
    };
    let ArenaExprKind::Try(call) = arena.expr(initializer).kind else {
        return false;
    };
    let ArenaExprKind::Call { callee, .. } = arena.expr(call).kind else {
        return false;
    };
    let call_span = arena.expr(call).span;
    if checked.standard_call_spans.get(&call_span).is_some_and(|(module, name)| {
        module == "fs"
            && matches!(
                name.as_str(),
                "open_root" | "tempdir" | "tempdir_in" | "project_root" | "user_root"
            )
    }) {
        return true;
    }
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    name.as_str() == "open_root"
        && checked.expr_types.get(&arena.expr(base).span) == Some(&super::Type::FsRoot)
}

fn uses_stay_borrowed(
    linter: &super::Linter<'_>,
    scope_span: Span,
    binding_id: BindingId,
    deferred_receiver: ExprId,
) -> bool {
    let arena = linter.arena;
    linter
        .checked_binding_uses
        .get(&binding_id)
        .into_iter()
        .flatten()
        .filter(|expr| within(arena.expr(**expr).span, scope_span))
        .all(|expr| {
            *expr == deferred_receiver
                || linter
                    .borrowed_root_receivers
                    .get(&binding_id)
                    .is_some_and(|receivers| receivers.contains(expr))
        })
}

/// A later root call must not release the root or create another managed root.
fn is_borrowed_root_method(name: &str) -> bool {
    matches!(
        name,
        "host_path"
            | "read_bytes"
            | "read_result"
            | "filesystem_stats"
            | "children"
            | "read_text"
            | "write"
            | "write_atomic"
            | "metadata"
            | "stat"
            | "exists"
            | "mkdir"
            | "remove"
            | "readlink"
            | "readlink_result"
            | "symlink"
            | "chmod"
    )
}

fn within(span: Span, scope: Span) -> bool {
    span.source_id == scope.source_id
        && span.start() >= scope.start()
        && span.end() <= scope.end()
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn redundant_closes(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Linter::lint(&parsed.arena, source, LintOptions::default())
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintRedundantClose))
            .collect()
    }

    #[test]
    fn a_locally_opened_root_closed_by_the_scope_gets_a_fix() {
        let source = "proc check(dir: Path) [fs, error] -> Result[Bool] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  root.exists(p\"data\")\n}\n";
        let diagnostics = redundant_closes(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, xsh::diagnostic::Severity::Warning);
        assert_eq!(diagnostics[0].fix_hints.len(), 1);
        assert!(diagnostics[0].notes[0].contains("already explicitly closed"));
    }

    #[test]
    fn selected_rule_checks_bindings_even_when_effect_facts_are_precomputed() {
        let source = "proc check(dir: Path) [fs, error] -> Result[Bool] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  root.exists(p\".\")\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert!(!checked.function_effect_facts.is_empty());

        let function_effect_facts = checked.function_effect_facts.clone();
        let options = || LintOptions {
            only: Some(vec![DiagnosticCode::LintRedundantClose]),
            function_effect_facts: function_effect_facts.clone(),
            function_effect_facts_checked: true,
            ..LintOptions::default()
        };
        let mut checker_calls = 0;
        let discovered = Linter::lint_internal_with_effect_check(
            &parsed.arena,
            source,
            options(),
            true,
            || {
                checker_calls += 1;
                xsh::frontend::check::Checker::check_arena(&parsed.arena, source)
            },
        );
        assert_eq!(checker_calls, 1);
        assert!(discovered.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintRedundantClose)
        }));

        let shared = std::sync::Arc::new(checked);
        let mut duplicate_checker_calls = 0;
        let reused = Linter::lint_internal_with_effect_check(
            &parsed.arena,
            source,
            LintOptions {
                checked_output: Some(std::sync::Arc::clone(&shared)),
                ..options()
            },
            true,
            || {
                duplicate_checker_calls += 1;
                xsh::frontend::check::Checker::check_arena(&parsed.arena, source)
            },
        );
        assert_eq!(duplicate_checker_calls, 0);
        assert!(reused.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintRedundantClose)
        }));
    }

    #[test]
    fn later_registered_deferred_reads_run_before_the_implicit_close() {
        let source = "proc check(dir: Path) [fs, error] -> Result[Bool] {\n  let root = fs.tempdir()?\n  defer root.close()?\n  defer { let _ = root.exists(p\"data\")? }\n  root.exists(p\"data\")\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let linted = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                checked_output: Some(std::sync::Arc::new(checked)),
                ..LintOptions::default()
            },
        );
        let redundant = linted
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintRedundantClose))
            .count();
        assert_eq!(redundant, 1, "{:?}", linted.diagnostics);
    }

    #[test]
    fn locks_prior_defers_and_other_close_paths_are_not_rewritten() {
        for source in [
            "proc check(path: Path) [fs, error] {\n  let held = fs.lock(path)?\n  defer fs.unlock(held)\n}\n",
            "proc check(dir: Path) [fs, error] {\n  defer { print \"before\" }\n  let root = fs.open_root(dir)?\n  defer root.close()\n}\n",
            "proc close_it(root: FsRoot) [fs, error] { root.close()? }\nproc check(dir: Path) [fs, error] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  close_it(root)?\n}\n",
            "proc check(dir: Path) [fs, error] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  return root\n}\n",
            "proc check(dir: Path) [fs, error] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  root.close()?\n}\n",
            "proc check(dir: Path) [fs, error] -> Result[Bool] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  let child = root.open_root(p\".\")?\n  child.exists(p\".\")\n}\n",
        ] {
            assert!(redundant_closes(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn a_shadowed_name_does_not_share_the_outer_binding_identity() {
        let source = "proc check(outer: Path, inner: Path) [fs, error] {\n  let root = fs.open_root(outer)?\n  defer root.close()\n  {\n    let root = fs.open_root(inner)?\n    defer root.close()\n  }\n}\n";
        assert_eq!(redundant_closes(source).len(), 2);
    }
}
