use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, StmtId,
};
use xsh::frontend::syntax::node::BinaryOp;
use xsh::frontend::syntax::token::TokenTag;

/// A token of a statement: its tag, its text, and where it starts.
type Token<'a> = (TokenTag, &'a str, usize);

fn tokens(source: &str, span: Span) -> Vec<Token<'_>> {
    let Some(text) = source.get(span.range()) else {
        return Vec::new();
    };
    let table = xsh::frontend::syntax::lexer::Lexer::new(span.source_id, text)
        .lex_compact()
        .token_table;
    (0..table.len())
        .filter_map(|index| {
            let start = table.start_at(index)?;
            let end = table.end_at(index, text)?;
            let tag = table.tag_at(index)?;
            // A line break or a comment inside a test does not change it.
            (!matches!(tag, TokenTag::Eof | TokenTag::Newline | TokenTag::Comment))
                .then_some((tag, text.get(start..end)?, span.start() + start))
        })
        .collect()
}

fn is_empty_literal(token: &Token<'_>) -> bool {
    matches!(token, (TokenTag::String, "\"\"", _))
}

fn is_equality(token: &Token<'_>) -> bool {
    matches!(token.0, TokenTag::EqEq | TokenTag::BangEq)
}

/// Where the tokens of a statement test `name` for emptiness: `name == ""`,
/// `name != ""`, either mirrored, or `name.is_empty()`. A `name` that is a
/// field of something else is another value.
fn emptiness_test(tokens: &[Token<'_>], name: &str) -> Option<std::ops::Range<usize>> {
    let is_name = |at: usize| {
        matches!(tokens.get(at), Some((TokenTag::Ident, word, _)) if *word == name)
            && !matches!(at.checked_sub(1).and_then(|before| tokens.get(before)), Some((TokenTag::Dot, ..)))
    };
    let span = |from: usize, to: usize| {
        let (_, text, start) = tokens[to];
        tokens[from].2..start + text.len()
    };
    for at in 0..tokens.len() {
        if is_name(at) {
            if let [Some(operator), Some(literal)] = [tokens.get(at + 1), tokens.get(at + 2)]
                && is_equality(operator)
                && is_empty_literal(literal)
            {
                return Some(span(at, at + 2));
            }
            if let [Some((TokenTag::Dot, ..)), Some((TokenTag::Ident, "is_empty", _)), Some((TokenTag::LParen, ..)), Some((TokenTag::RParen, ..))] = [
                tokens.get(at + 1),
                tokens.get(at + 2),
                tokens.get(at + 3),
                tokens.get(at + 4),
            ] {
                return Some(span(at, at + 4));
            }
        }
        if is_empty_literal(&tokens[at])
            && tokens.get(at + 1).is_some_and(is_equality)
            && is_name(at + 2)
            && !matches!(tokens.get(at + 3), Some((TokenTag::Dot, ..)))
        {
            return Some(span(at, at + 2));
        }
    }
    None
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
        let fallback = arena.expr(right);
        if !matches!(fallback.kind, ArenaExprKind::Str(_))
            || linter.source.get(fallback.span.range()) != Some("\"\"")
        {
            continue;
        }
        let source_span = arena.expr(left).span;
        // How a binding that keeps the missing case apart opens.
        let pattern = |name: &str| match linter.expr_types.get(&source_span) {
            Some(Type::Optional(inner)) if **inner == Type::Str => Some(name.to_string()),
            Some(Type::Result(ok, _)) if **ok == Type::Str => Some(format!("Ok({name})")),
            _ => None,
        };
        let name = name.as_str();
        let name = name.as_str();
        let mut test = None;
        for later in &stmts[index + 1..] {
            let later = arena.stmt(*later);
            let later_tokens = tokens(linter.source, later.span);
            test = emptiness_test(&later_tokens, name)
                .map(|range| Span::new(later.span.source_id, range.start, range.end));
            if test.is_some() {
                break;
            }
        }
        let Some(test) = test else {
            continue;
        };
        let Some(source_text) = linter.source.get(source_span.range()) else {
            continue;
        };
        let Some(pattern) = pattern(name) else {
            continue;
        };
        let binding = if source_text.contains('\n') {
            format!("if let {pattern} = ...")
        } else {
            format!("if let {pattern} = {source_text}")
        };
        linter.diagnostics.push(
            Diagnostic::note(format!(
                "`{name}` is `\"\"` both when there is no value and when the value is empty"
            ))
            .with_code(DiagnosticCode::LintEmptySentinel)
            .with_label(Label::primary(
                fallback.span,
                "a missing value becomes the empty string here",
            ))
            .with_label(Label::secondary(
                test,
                format!("and is tested here; `{binding} {{ ... }}` keeps the missing case apart"),
            ))
            .with_note(
                "not rewritten: the value may itself be `\"\"`, and an optional binding would then take the present branch",
            ),
        );
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
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
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
