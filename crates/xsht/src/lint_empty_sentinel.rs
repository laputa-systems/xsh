use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, AstArena, ExprId, StmtId,
};
use xsh::frontend::syntax::node::BinaryOp;

/// An empty-string fallback whose binding has not been tested yet.
struct Fallback {
    name: Name,
    /// The statements of the block after the binding.
    later: std::ops::Range<usize>,
    /// The `""` after `??`.
    literal: Span,
    /// How a binding that keeps the missing case apart is written.
    binding: String,
}

/// The empty-string fallbacks of the blocks being visited, each waiting for
/// the first emptiness test of its binding among the later statements of its
/// block. The test is found as the expressions of those statements are
/// visited, so how the test is grouped or broken over lines does not matter.
#[derive(Default)]
pub(super) struct EmptyFallbacks {
    waiting: Vec<Fallback>,
}

fn is_empty_literal(arena: &AstArena, source: &str, expr: ExprId) -> bool {
    let expr = arena.expr(expr);
    matches!(expr.kind, ArenaExprKind::Str(_)) && source.get(expr.span.range()) == Some("\"\"")
}

/// The binding that `expr` tests for emptiness: `name == ""`, `name != ""`,
/// either mirrored, or `name.is_empty()`. A field of a binding is another
/// value.
fn tested_name(arena: &AstArena, source: &str, expr: ExprId) -> Option<Name> {
    let named = |expr: ExprId| match arena.expr(expr).kind {
        ArenaExprKind::Ident(name) => Some(name),
        _ => None,
    };
    match arena.expr(expr).kind {
        ArenaExprKind::Binary {
            op: BinaryOp::Eq | BinaryOp::Ne,
            left,
            right,
        } => {
            if is_empty_literal(arena, source, right) {
                named(left)
            } else if is_empty_literal(arena, source, left) {
                named(right)
            } else {
                None
            }
        }
        ArenaExprKind::Call { callee, args } if arena.call_args(args).is_empty() => {
            match arena.expr(callee).kind {
                ArenaExprKind::Field { base, name } if name.as_str().as_str() == "is_empty" => {
                    named(base)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

impl EmptyFallbacks {
    /// Reports each waiting fallback whose binding `expr` tests.
    pub(super) fn visit_expr(
        &mut self,
        arena: &AstArena,
        source: &str,
        expr: ExprId,
    ) -> Vec<Diagnostic> {
        if self.waiting.is_empty() {
            return Vec::new();
        }
        let Some(name) = tested_name(arena, source, expr) else {
            return Vec::new();
        };
        let test = arena.expr(expr).span;
        let (tested, waiting) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|fallback| {
                fallback.name == name
                    && fallback.literal.source_id == test.source_id
                    && fallback.later.start <= test.start()
                    && test.end() <= fallback.later.end
            });
        self.waiting = waiting;
        let tested: Vec<Fallback> = tested;
        tested
            .into_iter()
            .map(|fallback| {
                let name = name.as_str();
                let name = name.as_str();
                Diagnostic::note(format!(
                    "`{name}` is `\"\"` both when there is no value and when the value is empty"
                ))
                .with_code(DiagnosticCode::LintEmptySentinel)
                .with_label(Label::primary(
                    fallback.literal,
                    "a missing value becomes the empty string here",
                ))
                .with_label(Label::secondary(
                    test,
                    format!(
                        "and is tested here; `{} {{ ... }}` keeps the missing case apart",
                        fallback.binding
                    ),
                ))
                .with_note(
                    "not rewritten: the value may itself be `\"\"`, and an optional binding would then take the present branch",
                )
            })
            .collect()
    }
}

/// An optional or `Result` text bound through `?? ""` whose binding a later
/// statement of the block tests for emptiness:
///
/// ```text
/// let name = lookup(key) ?? ""
/// if name == "" { ... }
/// ```
///
/// The binding makes "no value" and "an empty value" one value, and the
/// test then cannot tell them apart. `if let name = lookup(key) { ... }`
/// (`if let Ok(name) = ...` for a `Result`) or `guard let` keeps the missing
/// case as its own branch.
///
/// Nothing is rewritten: the source may itself yield `""`, and whether that
/// belongs with the absent case is the author's to say.
///
/// This records the fallback; the note is made where the test is visited.
pub(super) fn lint_empty_fallback_then_test(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    let arena = linter.arena;
    for (index, stmt) in stmts.iter().enumerate() {
        let stmt = arena.stmt(*stmt);
        let (ArenaStmtKind::Let {
            target,
            initializer: ArenaExprOrRun::Expr(initializer),
            ..
        }
        | ArenaStmtKind::Var {
            target,
            initializer: ArenaExprOrRun::Expr(initializer),
            ..
        }) = stmt.kind
        else {
            continue;
        };
        let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
            continue;
        };
        let ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left,
            right,
        } = arena.expr(initializer).kind
        else {
            continue;
        };
        if !is_empty_literal(arena, linter.source, right) {
            continue;
        }
        let (Some(first), Some(last)) = (stmts.get(index + 1), stmts.last()) else {
            continue;
        };
        let source_span = arena.expr(left).span;
        // How a binding that keeps the missing case apart opens.
        let pattern = match linter.expr_types.get(&source_span) {
            Some(Type::Optional(inner)) if **inner == Type::Str => name.as_str().to_string(),
            Some(Type::Result(ok, _)) if **ok == Type::Str => format!("Ok({})", name.as_str()),
            _ => continue,
        };
        let Some(source_text) = linter.source.get(source_span.range()) else {
            continue;
        };
        let binding = if source_text.contains('\n') {
            format!("if let {pattern} = ...")
        } else {
            format!("if let {pattern} = {source_text}")
        };
        linter.empty_fallbacks.waiting.push(Fallback {
            name,
            later: arena.stmt(*first).span.start()..arena.stmt(*last).span.end(),
            literal: arena.expr(right).span,
            binding,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode, Severity};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn notes(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        // Grouping an operand is a layout the findings must not depend on.
        assert!(
            checked
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckRedundantParens)),
            "{:?}",
            checked.diagnostics
        );
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        )
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintEmptySentinel))
        .collect()
    }

    fn probe(body: &str) -> String {
        format!("pure probe(names: Map[Str], key: Str) -> Str {{\n{body}}}\n")
    }

    #[test]
    fn empty_fallback_tested_for_emptiness_is_noted_without_a_fix() {
        for test in [
            "name == \"\"",
            "name != \"\"",
            "\"\" == name",
            "name.is_empty()",
            "! name.is_empty()",
        ] {
            let source = probe(&format!(
                "  let name = names.get(key) ?? \"\"\n  if {test} {{\n    return \"none\"\n  }}\n\n  name\n"
            ));
            let diagnostics = notes(&source);
            assert_eq!(diagnostics.len(), 1, "{test}: {diagnostics:?}");
            let diagnostic = &diagnostics[0];
            assert_eq!(diagnostic.severity, Severity::Note);
            assert!(diagnostic.fix_hints.is_empty(), "{test}");
            assert_eq!(&source[diagnostic.labels[0].span.range()], "\"\"");
            let tested = &source[diagnostic.labels[1].span.range()];
            assert!(test.ends_with(tested), "{test}: {tested}");
            assert!(
                diagnostic.labels[1]
                    .message
                    .as_deref()
                    .is_some_and(|label| label.contains("`if let Ok(name) = names.get(key) { ... }`")),
                "{diagnostic:?}"
            );
        }
    }

    /// The same finding however the statements are laid out.
    #[test]
    fn layout_does_not_change_the_finding() {
        let source = probe(
            "  let name: Str = names.get(\n    key,\n  )\n    ?? \"\"\n  if name ==\n    \"\" {\n    return \"none\"\n  }\n\n  name\n",
        );
        assert_eq!(notes(&source).len(), 1);
        let written = "  let name = names.get(key) ?? \"\"\n  if name == \"\" {\n    return \"none\"\n  }\n\n  name\n";
        for (fallback, test) in [
            ("(names.get(key)) ??\n(\"\")", "(name) ==\n(\"\")"),
            ("names.get(key) ?? (\"\")", "(\"\") != (name)"),
            ("names.get(key) ?? \"\"", "(name).is_empty()"),
            ("names.get(key) ?? \"\"", "(name == \"\")"),
        ] {
            let grouped = written
                .replace("names.get(key) ?? \"\"", fallback)
                .replace("name == \"\"", test);
            assert_ne!(grouped, written);
            let found = |body: &str| {
                let source = probe(body);
                notes(&source)
                    .iter()
                    .map(|diagnostic| {
                        diagnostic
                            .labels
                            .iter()
                            .map(|label| source[label.span.range()].to_string())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(found(written).len(), 1);
            assert_eq!(found(&grouped).len(), 1, "{grouped}");
            assert_eq!(found(&grouped)[0][0], "\"\"", "{grouped}");
        }
    }

    /// A binding that is never tested, another fallback, a test of a field
    /// of that name, and a test against other text are left alone.
    #[test]
    fn optional_source_is_bound_by_name() {
        let source = "pure probe(name: Str?) -> Str {\n  let text = name ?? \"\"\n  if text.is_empty() {\n    return \"none\"\n  }\n\n  text\n}\n";
        let diagnostics = notes(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            diagnostics[0].labels[1]
                .message
                .as_deref()
                .is_some_and(|label| label.contains("`if let text = name { ... }`")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn other_shapes_are_left_alone() {
        for body in [
            "  let name = names.get(key) ?? \"\"\n  name\n",
            "  let name = names.get(key) ?? \"-\"\n  if name == \"\" {\n    return \"none\"\n  }\n\n  name\n",
            "  let name = names.get(key) ?? \"\"\n  let other = {name: key}\n  if other.name == \"\" {\n    return \"none\"\n  }\n\n  name\n",
            "  let name = names.get(key) ?? \"\"\n  if name == \"x\" {\n    return \"none\"\n  }\n\n  name\n",
        ] {
            let source = probe(body);
            assert!(notes(&source).is_empty(), "{body}");
        }
    }
}
