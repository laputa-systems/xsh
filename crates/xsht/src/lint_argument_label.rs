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

/// The edit that replaces `call` with `replacement` as the formatter prints
/// the result: the span it covers and its text.
///
/// The method spelling can be wider than the function's, and a call that no
/// longer fits its line is one the formatter breaks. How it breaks depends on
/// the statement around the call, so the formatter is asked for the whole
/// file with the call replaced, and the edit is the part of its answer that
/// differs from the source, widened to hold the call.
///
/// That is the formatter's rendering of this call only in a file the
/// formatter leaves alone. In any other file, and where the formatter
/// rejects the result, the call is replaced where it stands and the next
/// format lays it out.
fn formatted_replacement(source: &str, call: Span, replacement: String) -> (Span, String) {
    let in_place = (call, replacement.clone());
    // A call that still fits its line stays on it; only one that does not is
    // worth formatting the file for.
    if fits_line(source, call, &replacement) {
        return in_place;
    }
    let format = |text: &str| {
        let output =
            super::super::format::Formatter::new().format_source(call.source_id, text);
        output.diagnostics.is_empty().then_some(output.formatted)
    };
    let mut rewritten = source.to_owned();
    rewritten.replace_range(call.range(), &replacement);
    let Some(formatted) = format(&rewritten) else {
        return in_place;
    };
    if formatted == rewritten || format(source).as_deref() != Some(source) {
        return in_place;
    }
    // Everything before the call and everything after it that the formatter
    // kept is outside the edit.
    let prefix = source
        .bytes()
        .zip(formatted.bytes())
        .take_while(|(old, new)| old == new)
        .count()
        .min(call.start());
    let suffix = source[prefix..]
        .bytes()
        .rev()
        .zip(formatted[prefix..].bytes().rev())
        .take_while(|(old, new)| old == new)
        .count()
        .min(source.len() - call.end());
    let (old_end, new_end) = (source.len() - suffix, formatted.len() - suffix);
    if !source.is_char_boundary(prefix)
        || !source.is_char_boundary(old_end)
        || !formatted.is_char_boundary(new_end)
    {
        return in_place;
    }
    (
        Span::new(call.source_id, prefix, old_end),
        formatted[prefix..new_end].to_owned(),
    )
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
    let Some(receiver) = xsh::frontend::check::call_receiver_text(arena, source, link) else {
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
    let (span, replacement) =
        formatted_replacement(source, call, format!("{receiver}.symlink({argument})"));
    Some(diagnostic.with_fix_hint(FixHint::replacement(
        span,
        "call the method on the link",
        replacement,
    )))
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

    /// A statement of `width` columns that links a literal, whose method
    /// spelling `p"..."` is one column wider than the function's `"..."`.
    fn literal_link_statement(width: usize) -> String {
        let fixed = "  fs.symlink(root.parent(), \"\")".len();
        format!(
            "proc stage(root: Path) [fs, error] {{\n  fs.symlink(root.parent(), \"{}\")\n}}\n",
            "l".repeat(width - fixed)
        )
    }

    // The method call is one column wider than the function call here. Where
    // that column is the first past the formatter's width the fix is the
    // broken call the formatter prints, and one column earlier it is the
    // call on its line.
    #[test]
    fn a_call_the_label_pushes_past_the_width_is_broken_as_the_formatter_breaks_it() {
        let width = super::super::super::format::DEFAULT_LINE_WIDTH;
        let format = |text: &str| {
            let output = super::super::super::format::Formatter::new()
                .format_source(xsh::frontend::source::SourceId::new(0), text);
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            output.formatted
        };
        for (statement_width, broken) in [(width - 1, false), (width, true)] {
            let source = literal_link_statement(statement_width);
            assert_eq!(format(&source), source, "the fixture is formatted");
            let diagnostics = lint(&source);
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].fix_hints.len(), 1, "{diagnostics:?}");
            let fixed = fixed(&source);
            assert_eq!(format(&fixed), fixed, "the fix prints what the formatter prints");
            assert_eq!(fixed.contains(".symlink(\n    to: root.parent(),\n  )\n"), broken, "{fixed}");
            assert!(fixed.contains("\n  p\"l") && fixed.contains("l\".symlink("), "{fixed}");
        }
    }

    // In a file the formatter would change elsewhere, the formatter's
    // rendering of the whole file is not the fix: the call is rewritten in
    // place and left to the next format.
    #[test]
    fn a_call_in_an_unformatted_file_is_rewritten_in_place() {
        let width = super::super::super::format::DEFAULT_LINE_WIDTH;
        let source = literal_link_statement(width).replace("proc stage(root: Path)", "proc stage( root: Path )");
        let fixed = fixed(&source);
        assert!(fixed.starts_with("proc stage( root: Path ) [fs, error] {\n  p\""), "{fixed}");
        assert!(fixed.ends_with("\".symlink(to: root.parent())\n}\n"), "{fixed}");
    }
}
