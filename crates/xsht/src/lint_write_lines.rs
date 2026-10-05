use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};
use xsh::frontend::syntax::node::BinaryOp;

/// `P.write(XS.join("\n") + "\n")` with a checked `List[Str]` is
/// `P.write_lines(XS)`.
///
/// The two agree for every list except the empty one: joining nothing and
/// appending a newline writes one newline, while `write_lines` writes an empty
/// file. The fix is therefore applied only to a list literal with an element
/// that is not a splice; any other list gets the same rewrite as a hint that
/// `--fix` does not apply.
pub(super) fn write_lines(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
) -> Option<Diagnostic> {
    let call = arena.expr(expr);
    let (path, data) = written_path_and_data(arena, call.kind)?;
    let path = arena.expr(path);
    if expr_types.get(&path.span) != Some(&Type::Path) {
        return None;
    }
    let data = arena.expr(data);
    let lines = arena.expr(joined_lines(arena, data.kind)?);
    if expr_types.get(&lines.span) != Some(&Type::List(Box::new(Type::Str))) {
        return None;
    }
    let nonempty = matches!(lines.kind, ArenaExprKind::List(elements)
        if arena.list_elements(elements).any(|element| element.splice_span.is_none()));
    let mut diagnostic =
        Diagnostic::warning("lines are joined and newline-terminated by hand before a write")
            .with_code(DiagnosticCode::LintPreferWriteLines)
            .with_label(Label::secondary(
                call.span,
                "`write_lines` terminates each line of a `List[Str]`",
            ));
    if !nonempty {
        diagnostic = diagnostic.with_note(
            "for an empty list this writes one newline and `write_lines` writes an empty file",
        );
    }
    if let Some(replacement) = replacement(source, call.span, path.kind, path.span, data.span)
    {
        let hint = if nonempty {
            FixHint::replacement(call.span, "write the lines with `write_lines`", replacement)
        } else {
            FixHint::replacement(
                call.span,
                "write the lines with `write_lines` (apply manually: an empty list then writes an empty file)",
                replacement,
            )
            .dangerous()
        };
        diagnostic = diagnostic.with_fix_hint(hint);
    }
    Some(diagnostic)
}

/// The path and data of `path.write(data)` with a positional argument.
fn written_path_and_data(arena: &AstArena, call: ArenaExprKind) -> Option<(ExprId, ExprId)> {
    let ArenaExprKind::Call { callee, args } = call else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if name != "write" {
        return None;
    }
    let [data] = arena.call_args(args) else {
        return None;
    };
    let ArenaCallArgKind::Positional(data) = data.kind else {
        return None;
    };
    Some((base, data))
}

/// The list in `LIST.join("\n") + "\n"`.
fn joined_lines(arena: &AstArena, data: ArenaExprKind) -> Option<ExprId> {
    let ArenaExprKind::Binary {
        op: BinaryOp::Add,
        left,
        right,
    } = data
    else {
        return None;
    };
    if !is_newline(arena, right) {
        return None;
    }
    let ArenaExprKind::Call { callee, args } = arena.expr(left).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    let [separator] = arena.call_args(args) else {
        return None;
    };
    let ArenaCallArgKind::Positional(separator) = separator.kind else {
        return None;
    };
    (name == "join" && is_newline(arena, separator)).then_some(base)
}

fn is_newline(arena: &AstArena, expr: ExprId) -> bool {
    matches!(arena.expr(expr).kind, ArenaExprKind::Str(text) if arena.string_literal(text).as_ref() == "\n")
}

/// `PATH.write_lines(LIST)` spelled from the source of both operands, or
/// `None` when the path cannot stand as a receiver verbatim or the rewrite
/// would drop a comment.
fn replacement(
    source: &str,
    call: Span,
    path_kind: ArenaExprKind,
    path: Span,
    data: Span,
) -> Option<String> {
    if source.get(call.range())?.contains('#') {
        return None;
    }
    let path_text = path_receiver_text(source, path_kind, path)?;
    let data_text = source.get(data.range())?;
    let lines = data_text[..data_text.rfind(".join(")?].trim();
    let lines = unwrapped(lines)?;
    Some(format!("{path_text}.write_lines({lines})"))
}

/// The source of a Path expression that can take a method call verbatim.
///
/// A bare path word such as `./out` would absorb the call into the path, and
/// a lower-precedence expression would need parentheses that a rewrite must
/// not add silently, so both are refused.
fn path_receiver_text(source: &str, kind: ArenaExprKind, span: Span) -> Option<&str> {
    let text = source.get(span.range())?;
    match kind {
        ArenaExprKind::Ident(_)
        | ArenaExprKind::Field { .. }
        | ArenaExprKind::Call { .. }
        | ArenaExprKind::Index { .. } => true,
        ArenaExprKind::PathStr(_) => text.starts_with("p\""),
        ArenaExprKind::PathFmtString(_) => text.starts_with("fp\""),
        _ => false,
    }
    .then_some(text)
}

/// The text without one pair of parentheses that encloses all of it, which a
/// call argument must not keep. `None` when the parentheses do not balance,
/// which means the text is not a whole expression.
fn unwrapped(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut encloses_all = text.starts_with('(');
    let mut quote = None;
    let mut escaped = false;
    for (offset, byte) in text.bytes().enumerate() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == open {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && offset + 1 != text.len() {
                    encloses_all = false;
                }
            }
            _ => {}
        }
    }
    if depth != 0 || quote.is_some() {
        return None;
    }
    Some(if encloses_all {
        text[1..text.len() - 1].trim()
    } else {
        text
    })
}

#[cfg(test)]
mod tests {
    use super::unwrapped;
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
            only: Some(vec![DiagnosticCode::LintPreferWriteLines]),
            ..LintOptions::default()
        };
        Linter::lint(&parsed.arena, source, options).diagnostics
    }

    /// Applies the hints `--fix` applies, or every hint when `manual` is set.
    fn apply(diagnostics: &[Diagnostic], source: &str, manual: bool) -> String {
        let mut fixes = diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .filter(|fix| manual || !fix.dangerous)
            .collect::<Vec<_>>();
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
        let mut fixed = source.to_owned();
        for fix in fixes {
            fixed.replace_range(
                fix.span.unwrap().range(),
                fix.replacement.as_deref().unwrap(),
            );
        }
        fixed
    }

    #[test]
    fn literal_lines_are_rewritten() {
        let source = "proc save(out: Path, name: Str) [fs, error] {\n  out.write([\"a\", name].join(\"\\n\") + \"\\n\")?\n  fp\"{out}.bak\".write([name].join(\"\\n\") + \"\\n\")?\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferWriteLines)
                    && diagnostic.notes.is_empty())
        );
        let fixed = apply(&diagnostics, source, false);
        assert_eq!(
            fixed,
            "proc save(out: Path, name: Str) [fs, error] {\n  out.write_lines([\"a\", name])?\n  fp\"{out}.bak\".write_lines([name])?\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // A list that may be empty writes "\n" before the rewrite and nothing
    // after it, so `--fix` leaves it and the hint is for a person to apply.
    #[test]
    fn possibly_empty_lines_get_a_manual_hint_only() {
        let source = "proc save(out: Path, files: List[Str]) [fs, error] {\n  out.write((files |> sort).join(\"\\n\") + \"\\n\")?\n  out.write([@files].join(\"\\n\") + \"\\n\")?\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.notes.len() == 1)
        );
        assert_eq!(apply(&diagnostics, source, false), source);
        let manual = apply(&diagnostics, source, true);
        assert_eq!(
            manual,
            "proc save(out: Path, files: List[Str]) [fs, error] {\n  out.write_lines(files |> sort)?\n  out.write_lines([@files])?\n}\n"
        );
        assert!(lint(&manual).is_empty());
    }

    #[test]
    fn other_writes_are_left_alone() {
        let source = "proc save(out: Path, files: List[Str], base: Str) [fs, error] {\n  out.write(files.join(\"\\n\"))?\n  out.write(files.join(\",\") + \"\\n\")?\n  out.write(base + files.join(\"\\n\") + \"\\n\")?\n  out.write_atomic(files.join(\"\\n\") + \"\\n\")?\n}\n";
        assert!(lint(source).is_empty());
    }

    // The rewrite copies the receiver verbatim and adds no parentheses.
    #[test]
    fn a_path_that_cannot_stand_as_a_receiver_gets_no_rewrite() {
        let source =
            "proc save(out: Path, other: Path, first: Bool, name: Str) [fs, error] {\n  (if first { out } else { other }).write([name].join(\"\\n\") + \"\\n\")?\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
    }

    #[test]
    fn enclosing_parentheses_are_removed_only_when_they_wrap_everything() {
        assert_eq!(unwrapped("(files |> sort)"), Some("files |> sort"));
        assert_eq!(unwrapped("(a)(b)"), Some("(a)(b)"));
        assert_eq!(unwrapped("f(\")\")"), Some("f(\")\")"));
        assert_eq!(unwrapped("files |> sort)"), None);
    }
}
