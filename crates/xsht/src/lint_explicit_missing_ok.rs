use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};

/// A removal that relies on the default for a missing path:
///
/// ```text
/// stale.remove()
/// fs.remove(stale)
/// ```
///
/// With no `missing_ok` argument a missing path is an error. This rule writes
/// that default out, `stale.remove(missing_ok: false)`, and changes nothing
/// about what the call does. It is a migration aid and is off unless asked
/// for: once every call that relies on the default says so, the default can
/// become `true` without changing any existing call.
///
/// Only the `Path` method and the `fs` function are matched. A rooted
/// `remove` takes other parameters, and a map's `remove` is another
/// operation.
pub(super) fn implicit_missing_ok(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
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
    if name != "remove" {
        return None;
    }
    let args = arena.call_args(args);
    // The number of arguments the call has when it leaves the default alone.
    let bare = if !fs_is_shadowed
        && matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "fs")
    {
        1
    } else if expr_types.get(&arena.expr(base).span) == Some(&Type::Path) {
        0
    } else {
        return None;
    };
    if args.len() != bare {
        return None;
    }
    let last = match args.last() {
        Some(argument) => match argument.kind {
            ArenaCallArgKind::Positional(path) => Some(arena.expr(path).span),
            _ => return None,
        },
        None => None,
    };
    let mut diagnostic = Diagnostic::warning("`remove` relies on the default for a missing path")
        .with_code(DiagnosticCode::LintExplicitMissingOk)
        .with_label(Label::secondary(
            call.span,
            "a missing path is an error here; `missing_ok: false` says so",
        ));
    if let Some(fix) = explicit_argument(source, call.span, last) {
        diagnostic = diagnostic.with_fix_hint(fix);
    }
    Some(diagnostic)
}

/// Inserts `missing_ok: false` as the last argument. In a call written one
/// argument to a line it takes a line of its own with the same indentation.
/// A comment after the last argument is left for a manual rewrite.
fn explicit_argument(source: &str, call: Span, last: Option<Span>) -> Option<FixHint> {
    const ARGUMENT: &str = "missing_ok: false";
    let hint = |at: usize, text: String| {
        FixHint::replacement(
            Span::new(call.source_id, at, at),
            "write the default out",
            text,
        )
    };
    let Some(last) = last else {
        return source
            .get(..call.end())?
            .ends_with("()")
            .then(|| hint(call.end() - 1, ARGUMENT.to_owned()));
    };
    let rest = source.get(last.end()..call.end())?;
    if rest == ")" {
        return Some(hint(last.end(), format!(", {ARGUMENT}")));
    }
    let after_comma = rest.strip_prefix(',')?;
    if !after_comma.strip_suffix(')')?.trim().is_empty() || !after_comma.contains('\n') {
        return None;
    }
    let line_start = source.get(..last.start())?.rfind('\n')? + 1;
    let indent = source.get(line_start..last.start())?;
    indent
        .trim()
        .is_empty()
        .then(|| hint(last.end() + 1, format!("\n{indent}{ARGUMENT},")))
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn lint_with(source: &str, options: LintOptions) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let options = LintOptions {
            expr_types: checked.expr_types,
            ..options
        };
        Linter::lint(&parsed.arena, source, options)
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintExplicitMissingOk))
            .collect()
    }

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_with(
            source,
            LintOptions {
                explicit_missing_ok: true,
                ..LintOptions::default()
            },
        )
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

    const SOURCE: &str = "proc clean(stale: Path, root: Path) [fs, error] {\n  stale.remove()\n  fs.remove(fp\"{root}/cache\")\n  fs.remove(\n    root,\n  )\n  let _ = stale.remove()\n}\n";

    #[test]
    fn a_removal_that_relies_on_the_default_says_so() {
        let diagnostics = lint(SOURCE);
        assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
        let fixed = apply(&diagnostics, SOURCE);
        assert_eq!(
            fixed,
            "proc clean(stale: Path, root: Path) [fs, error] {\n  stale.remove(missing_ok: false)\n  fs.remove(fp\"{root}/cache\", missing_ok: false)\n  fs.remove(\n    root,\n    missing_ok: false,\n  )\n  let _ = stale.remove(missing_ok: false)\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // The rule is a migration aid: it reports nothing until it is asked for,
    // by its setting or by name.
    #[test]
    fn the_rule_is_off_unless_asked_for() {
        assert!(lint_with(SOURCE, LintOptions::default()).is_empty());
        let by_name = LintOptions {
            only: Some(vec![DiagnosticCode::LintExplicitMissingOk]),
            ..LintOptions::default()
        };
        assert_eq!(lint_with(SOURCE, by_name).len(), 4);
    }

    // A call that already chooses, a rooted removal, and a map's `remove`
    // do not rely on this default.
    #[test]
    fn other_removals_are_left_alone() {
        let source = "proc clean(stale: Path, root: FsRoot, seen: Map[Int]) [fs, error] {\n  var counts = seen\n  stale.remove(missing_ok: true)\n  fs.remove(stale, missing_ok: false)\n  root.remove(p\"cache\")\n  counts.remove(\"stale\")\n}\n";
        assert!(lint(source).is_empty(), "{:?}", lint(source));
    }
}
