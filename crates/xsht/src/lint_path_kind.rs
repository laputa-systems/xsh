use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};
use xsh::frontend::syntax::node::BinaryOp;

/// A path's kind read out of its metadata and compared with a literal:
///
/// ```text
/// out.metadata()?.kind == "dir"
/// fs.metadata(out)?.kind != "file"
/// ```
///
/// is `out.is_dir()?` and `! out.is_file()?`. The predicate makes the same
/// lookup as `metadata`, does not follow a symlink either, and fails the same
/// way on a path that does not exist, so the rewrite changes no result and no
/// failure. Only `"dir"`, `"file"`, and `"symlink"` have a predicate, and only
/// a comparison of the propagated call itself is matched: the `kind` of an
/// entry that is already at hand (one from `fs.walk`, say) costs no lookup and
/// is left alone.
///
/// The negated call binds tighter than any operator the comparison could have
/// been an operand of, and parentheses around the comparison stay where they
/// are, so the rewrite reads the same in every position.
pub(super) fn path_kind_comparison(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    fs_is_shadowed: bool,
    expr: ExprId,
) -> Option<Diagnostic> {
    let comparison = arena.expr(expr);
    let ArenaExprKind::Binary {
        op: op @ (BinaryOp::Eq | BinaryOp::Ne),
        left,
        right,
    } = comparison.kind
    else {
        return None;
    };
    let (kind, literal) = [(left, right), (right, left)]
        .into_iter()
        .find(|(_, literal)| matches!(arena.expr(*literal).kind, ArenaExprKind::Str(_)))?;
    let ArenaExprKind::Str(text) = arena.expr(literal).kind else {
        return None;
    };
    let predicate = match &**arena.string_literal(text) {
        "dir" => "is_dir",
        "file" => "is_file",
        "symlink" => "is_symlink",
        _ => return None,
    };
    // `LOOKUP?.kind` is written with the same characters as a null-safe
    // field access and is parsed as one; on a `Result` it propagates.
    let (lookup, name) = match arena.expr(kind).kind {
        ArenaExprKind::NullSafeField { base, name } => (base, name),
        ArenaExprKind::Field { base, name } => match arena.expr(base).kind {
            ArenaExprKind::Try(lookup) => (lookup, name),
            _ => return None,
        },
        _ => return None,
    };
    if name != "kind" {
        return None;
    }
    let ArenaExprKind::Call { callee, args } = arena.expr(lookup).kind else {
        return None;
    };
    let ArenaExprKind::Field {
        base: receiver,
        name: function,
    } = arena.expr(callee).kind
    else {
        return None;
    };
    if function != "metadata" {
        return None;
    }
    let is_path = |expr: ExprId| expr_types.get(&arena.expr(expr).span) == Some(&Type::Path);
    // The text the predicate is called on, ending in the `.` of the call.
    let subject = match arena.call_args(args) {
        [] if is_path(receiver) => source
            .get(arena.expr(callee).span.range())
            .and_then(|callee| callee.strip_suffix("metadata"))
            .map(str::to_owned),
        [argument]
            if !fs_is_shadowed
                && matches!(arena.expr(receiver).kind, ArenaExprKind::Ident(module) if module == "fs") =>
        {
            let ArenaCallArgKind::Positional(path) = argument.kind else {
                return None;
            };
            if !is_path(path) {
                return None;
            }
            call_receiver_text(arena, source, path).map(|path| format!("{path}."))
        }
        _ => return None,
    };
    let mut diagnostic = Diagnostic::warning("a path's kind is read from its metadata to test it")
        .with_code(DiagnosticCode::LintPreferPathKind)
        .with_label(Label::secondary(
            comparison.span,
            format!("`{predicate}()` asks the same question with the same lookup"),
        ));
    if let Some(subject) = subject
        && source
            .get(comparison.span.range())
            .is_some_and(|text| !text.contains('#'))
    {
        let negation = if op == BinaryOp::Ne { "! " } else { "" };
        diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
            comparison.span,
            format!("ask with `{predicate}()`"),
            format!("{negation}{subject}{predicate}()?"),
        ));
    }
    Some(diagnostic)
}

/// An operand of static type `Path`, spelled so that `.method(...)` can
/// follow it and the whole still evaluates the operand exactly as written:
///
/// - a name, a field, an index, a call, or a propagation is used as it is
///   (`resolve()?.copy(...)` propagates and then calls, as `LOOKUP?.kind`
///   does);
/// - a bare path literal such as `/etc/hosts` would read `.method` as more
///   of the path, and a string literal is text until something expects a
///   path, so both are written `p"..."`, which is the same path;
/// - any other expression is parenthesized.
///
/// A literal with no `p"..."` spelling that is certainly the same path (a
/// bare path that needs quoting inside quotes, a `$`, a triple-quoted or raw
/// string) gives `None`.
pub(super) fn call_receiver_text(arena: &AstArena, source: &str, operand: ExprId) -> Option<String> {
    let operand = arena.expr(operand);
    let text = source.get(operand.span.range())?;
    let quoted = |value: &str, written: &str| {
        (value == written && !written.contains(['"', '\\', '\n'])).then(|| format!("p\"{written}\""))
    };
    match operand.kind {
        ArenaExprKind::Ident(_)
        | ArenaExprKind::Field { .. }
        | ArenaExprKind::NullSafeField { .. }
        | ArenaExprKind::Call { .. }
        | ArenaExprKind::Index { .. }
        | ArenaExprKind::Try(_) => Some(text.to_owned()),
        ArenaExprKind::PathStr(_) if text.starts_with("p\"") => Some(text.to_owned()),
        ArenaExprKind::PathStr(literal) => quoted(arena.string_literal(literal), text),
        // `p"..."` decodes the escapes `"..."` does, so a one-line string
        // keeps its text between the quotes, escapes included. Two things
        // differ and are left alone: `${` is an error in a path literal
        // only, and a triple-quoted string has layout a path literal lacks.
        ArenaExprKind::Str(_) => {
            let written = text.strip_prefix('"')?.strip_suffix('"')?;
            (!written.starts_with('"') && !written.contains(['$', '\n']))
                .then(|| format!("p\"{written}\""))
        }
        ArenaExprKind::PathFmtString(_) => text.starts_with("fp\"").then(|| text.to_owned()),
        // A format string would need its prefix changed, not parentheses.
        ArenaExprKind::FmtString(_) => None,
        _ => Some(format!("({text})")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    // The lint runs from the linter's expression traversal, so the tests
    // drive the whole linter restricted to this code.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let options = LintOptions {
            expr_types: checked.expr_types,
            only: Some(vec![DiagnosticCode::LintPreferPathKind]),
            ..LintOptions::default()
        };
        Linter::lint(&parsed.arena, source, options).diagnostics
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

    #[test]
    fn a_kind_comparison_becomes_the_predicate() {
        let source = "proc classify(out: Path, root: Path) [fs, error] -> Result[Bool] {\n  if out.metadata()?.kind == \"dir\" and fs.metadata(out)?.kind != \"symlink\" {\n    return Ok(true)\n  }\n  let plain = \"file\" == fp\"{root}/a\".metadata()?.kind\n  let absent = fs.metadata(root.parent())?.kind != \"dir\"\n  Ok(plain and ! absent)\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc classify(out: Path, root: Path) [fs, error] -> Result[Bool] {\n  if out.is_dir()? and ! out.is_symlink()? {\n    return Ok(true)\n  }\n  let plain = fp\"{root}/a\".is_file()?\n  let absent = ! root.parent().is_dir()?\n  Ok(plain and ! absent)\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // An entry at hand, another kind, a kind that was bound first, and a
    // lookup that is not propagated are not this comparison.
    #[test]
    fn other_kind_tests_are_left_alone() {
        let source = "proc classify(out: Path) [fs, error] -> Result[Bool] {\n  let entry = out.metadata()?\n  let found = entry.kind == \"dir\"\n  let other = out.metadata()?.kind == \"other\"\n  let kind = out.metadata()?.kind\n  let size = out.metadata()?.size == 0\n  Ok(found or other or kind == \"file\" or size)\n}\n";
        assert!(lint(source).is_empty(), "{:?}", lint(source));
    }

    // An operand that cannot stand before `.is_dir()` as written is respelled:
    // parentheses around an expression, and a quoted path for a bare path
    // literal, which would otherwise swallow the method name.
    #[test]
    fn an_operand_that_is_not_a_receiver_is_respelled() {
        let source = "proc classify(out: Path, fallback: Path, first: Bool) [fs, error] -> Result[Bool] {\n  let chosen = fs.metadata(if first { out } else { fallback })?.kind == \"dir\"\n  let real = fs.metadata(out.resolve()?)?.kind != \"symlink\"\n  Ok(chosen and real and fs.metadata(/etc/hosts)?.kind == \"file\")\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc classify(out: Path, fallback: Path, first: Bool) [fs, error] -> Result[Bool] {\n  let chosen = (if first { out } else { fallback }).is_dir()?\n  let real = ! out.resolve()?.is_symlink()?\n  Ok(chosen and real and p\"/etc/hosts\".is_file()?)\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }
}
