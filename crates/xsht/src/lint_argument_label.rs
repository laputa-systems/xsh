//! `lint.prefer-argument-label`: `fs.symlink(target, link)` takes two paths
//! in the order of `ln -s`, so a swapped pair checks. The method names the
//! one that is not its receiver:
//!
//! ```text
//! fs.symlink(target, link)        link.symlink(to: target)
//! ```
//!
//! The method's receiver is the function's second operand, so the link is
//! then evaluated before the target, and the fix is offered only where that
//! order cannot be observed.

use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaFmtPart, AstArena, ExprId,
};

pub(super) fn positional_symlink(
    arena: &AstArena,
    source: &str,
    fs_is_shadowed: bool,
    expr: ExprId,
) -> Option<Diagnostic> {
    let call = arena.expr(expr);
    let ArenaExprKind::Call { callee, args } = call.kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if fs_is_shadowed
        || name != "symlink"
        || !matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "fs")
    {
        return None;
    }
    symlink_function(arena, source, call.span, arena.call_args(args))
}

/// Whether the line an edit starts on still fits the formatter's width after
/// it, or did not fit before.
fn fits_line(source: &str, span: Span, replacement: &str) -> bool {
    let line_start = source[..span.start()]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    let line_end = source[span.end()..]
        .find('\n')
        .map_or(source.len(), |offset| span.end() + offset);
    let before = source[line_start..line_end].chars().count();
    let removed = source[span.range()].chars().count();
    let after = before - removed + replacement.chars().count();
    let width = super::super::format::DEFAULT_LINE_WIDTH;
    after <= width || before > width
}

/// `fs.symlink(target, link)`, whose operands are in the order of `ln -s` and
/// are both paths, so a swapped pair checks.
fn symlink_function(
    arena: &AstArena,
    source: &str,
    call: Span,
    args: &[ArenaCallArg],
) -> Option<Diagnostic> {
    let mut operands = [None, None];
    for (position, arg) in args.iter().enumerate() {
        let (slot, value) = match arg.kind {
            ArenaCallArgKind::Positional(value) => (position, value),
            ArenaCallArgKind::Named { name, value, .. } if name == "target" => (0, value),
            ArenaCallArgKind::Named { name, value, .. } if name == "path" => (1, value),
            _ => return None,
        };
        let operand = operands.get_mut(slot)?;
        if operand.replace(value).is_some() {
            return None;
        }
    }
    let [Some(target), Some(link)] = operands else {
        return None;
    };
    let diagnostic = Diagnostic::warning("`fs.symlink` takes the target first and the link second")
        .with_code(DiagnosticCode::LintPreferArgumentLabel)
        .with_label(Label::secondary(
            call,
            "`LINK.symlink(to: TARGET)` says which is which",
        ));
    if !reorder_is_unobservable(arena, target, link) {
        return Some(diagnostic.with_note(
            "the method evaluates the link before the target; bind an operand that has an effect to a name first",
        ));
    }
    let written = source.get(call.range())?;
    if written.contains(['#', '\n']) {
        return Some(diagnostic.with_note(
            "the call spans lines or holds a comment; rewrite it as `LINK.symlink(to: TARGET)` by hand",
        ));
    }
    let Some(receiver) = super::lint_path_kind::call_receiver_text(arena, source, link) else {
        return Some(diagnostic.with_note(
            "the link has no spelling a method can follow; bind it to a name first",
        ));
    };
    let target = arena.expr(target);
    let argument = if matches!(target.kind, ArenaExprKind::Ident(name) if name == "to") {
        "to:".to_owned()
    } else {
        format!("to: {}", source.get(target.span.range())?)
    };
    let replacement = format!("{receiver}.symlink({argument})");
    Some(if fits_line(source, call, &replacement) {
        diagnostic.with_fix_hint(FixHint::replacement(
            call,
            "call the method on the link",
            replacement,
        ))
    } else {
        diagnostic.with_note(
            "the method call no longer fits the line; rewrite it as `LINK.symlink(to: TARGET)` and break the call",
        )
    })
}

/// Whether evaluating `second` before `first` gives what evaluating `first`
/// before `second` gives: one of them is a literal, or neither does anything
/// but read.
fn reorder_is_unobservable(arena: &AstArena, first: ExprId, second: ExprId) -> bool {
    is_literal(arena, first)
        || is_literal(arena, second)
        || (only_reads(arena, first) && only_reads(arena, second))
}

fn is_literal(arena: &AstArena, expr: ExprId) -> bool {
    matches!(
        arena.expr(expr).kind,
        ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::PathStr(_)
    )
}

/// A name, a literal, a field path of one, or text interpolating only those:
/// evaluating it runs nothing and fails nowhere.
fn only_reads(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_) | ArenaExprKind::Item => true,
        ArenaExprKind::Field { base, .. } => only_reads(arena, base),
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            arena.fmt_parts(parts).all(|part| match part {
                ArenaFmtPart::Text(_) => true,
                ArenaFmtPart::Expr(value, _) => only_reads(arena, value),
            })
        }
        _ => is_literal(arena, expr),
    }
}

#[cfg(test)]
mod tests {
    use super::super::lint_redundant_propagation::tests::lint_rule;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintPreferArgumentLabel)
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
                fix.replacement.as_deref().unwrap_or_default(),
            );
        }
        fixed
    }

    fn fixed(source: &str) -> String {
        let diagnostics = lint(source);
        let fixed = apply(&diagnostics, source);
        assert!(lint(&fixed).is_empty(), "{fixed}");
        fixed
    }

    #[test]
    fn fs_symlink_becomes_the_method_where_the_order_cannot_be_observed() {
        let source = "proc stage(root: Path, target: Path, to: Path) [fs, error] {\n  let entry = {link: root}\n  fs.symlink(target, fp\"{root}/link\")\n  fs.symlink(\"../shared\", entry.link)\n  fs.symlink(to, root)\n  fs.symlink(path: root, target: target)\n  fs.symlink(root.parent(), \"literal-link\")\n  fs.symlink(/etc/hosts, root)\n}\n";
        assert_eq!(lint(source).len(), 6);
        assert_eq!(
            fixed(source),
            "proc stage(root: Path, target: Path, to: Path) [fs, error] {\n  let entry = {link: root}\n  fp\"{root}/link\".symlink(to: target)\n  entry.link.symlink(to: \"../shared\")\n  root.symlink(to:)\n  root.symlink(to: target)\n  p\"literal-link\".symlink(to: root.parent())\n  root.symlink(to: /etc/hosts)\n}\n"
        );
    }

    #[test]
    fn fs_symlink_with_an_operand_that_runs_something_has_no_fix() {
        let source = "proc stage(root: Path, target: Path) [fs, error] {\n  fs.symlink(target.resolve()?, root.parent())\n  fs.symlink(\n    target,\n    root,\n  )\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty(), "{diagnostics:?}");
        assert!(diagnostics[0].notes[0].contains("evaluates the link before the target"));
        assert!(diagnostics[1].fix_hints.is_empty(), "{diagnostics:?}");
    }
}
