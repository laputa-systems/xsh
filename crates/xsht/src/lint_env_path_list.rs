use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaFmtPart, ArenaRecordFieldKind, AstArena, ExprId,
};
use xsh::frontend::syntax::node::BinaryOp;

/// A record field that extends a search-path variable by formatting text:
///
/// ```text
/// PATH: f"{root}/usr/bin:{root}/bin:{e"PATH" ?? ""}"
/// ```
///
/// The field is named for the variable its last entry reads, with `""` as
/// the fallback. As a `List[Path]` the same value is
///
/// ```text
/// PATH: [fp"{root}/usr/bin", fp"{root}/bin", @env.PathList.PATH ?? []]
/// ```
///
/// The two are the same bytes whenever the variable is set, is valid UTF-8,
/// and no entry contains `:`. They differ otherwise, in the list's favor, so
/// the rewrite is offered for manual application and never applied by
/// `--fix`:
///
/// - an unset variable leaves the string with a trailing empty entry, which
///   a search reads as the current directory, and the list with none;
/// - a variable that is not UTF-8 is dropped by the string's fallback and
///   kept by the list, and an interpolated Path keeps its bytes instead of
///   its display text;
/// - an entry that contains `:` is two entries in the string and a failure
///   in the list.
pub(super) fn env_path_lists(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
) -> Vec<Diagnostic> {
    let ArenaExprKind::Record(fields) = arena.expr(expr).kind else {
        return Vec::new();
    };
    arena
        .record_fields(fields)
        .iter()
        .filter_map(|field| {
            let ArenaRecordFieldKind::Named { name, value, .. } = field.kind else {
                return None;
            };
            let name = name.as_str();
            formatted_search_path(arena, source, expr_types, name.as_str(), value)
        })
        .collect()
}

/// One piece of a `:`-separated entry: literal text, or the span of an
/// interpolated expression.
enum Piece<'a> {
    Text(&'a str),
    Expr(Span),
}

fn formatted_search_path(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    variable: &str,
    value: ExprId,
) -> Option<Diagnostic> {
    let literal = arena.expr(value);
    let ArenaExprKind::FmtString(parts) = literal.kind else {
        return None;
    };
    // A single-line `f"..."` only: its text between interpolations is then
    // exactly the source between them.
    let body = source
        .get(literal.span.range())?
        .strip_prefix("f\"")?
        .strip_suffix('"')?;
    if body.starts_with('"') {
        return None;
    }
    let body_end = literal.span.end() - 1;
    let mut cursor = literal.span.start() + 2;
    let mut entries: Vec<Vec<Piece<'_>>> = vec![Vec::new()];
    let mut last_expr = None;
    for part in arena.fmt_parts(parts) {
        let ArenaFmtPart::Expr(interpolated, spec) = part else {
            continue;
        };
        if spec.is_some() {
            return None;
        }
        let span = arena.expr(interpolated).span;
        let open = span.start().checked_sub(1)?;
        if source.as_bytes().get(open) != Some(&b'{')
            || source.as_bytes().get(span.end()) != Some(&b'}')
        {
            return None;
        }
        push_text(&mut entries, source.get(cursor..open)?)?;
        entries.last_mut()?.push(Piece::Expr(span));
        cursor = span.end() + 1;
        last_expr = Some(interpolated);
    }
    // The variable's old value is the whole last entry.
    if cursor != body_end || entries.len() < 2 {
        return None;
    }
    let tail = entries.pop()?;
    if !matches!(tail.as_slice(), [Piece::Expr(_)])
        || !reads_variable_or_empty(arena, last_expr?, variable)
        || entries.iter().any(Vec::is_empty)
    {
        return None;
    }

    let mut diagnostic = Diagnostic::note(format!(
        "`{variable}` is extended by formatting a `:`-separated string"
    ))
    .with_code(DiagnosticCode::LintPreferEnvPathList)
    .with_label(Label::secondary(
        literal.span,
        "a `List[Path]` environment value joins its entries and keeps their bytes",
    ))
    .with_note(format!(
        "as a list this value differs in three cases: when `{variable}` is unset there is no trailing empty entry, which a search reads as the current directory; bytes that are not UTF-8, in `{variable}` or in an interpolated Path, are kept; and an entry that contains `:` fails instead of becoming two entries"
    ));
    let rendered = entries
        .iter()
        .map(|entry| entry_text(source, expr_types, entry))
        .collect::<Option<Vec<_>>>();
    if let Some(rendered) = rendered {
        diagnostic = diagnostic.with_fix_hint(
            FixHint::replacement(
                literal.span,
                "write the search path as a list (apply manually: see the note for what changes)",
                format!("[{}, @env.PathList.{variable} ?? []]", rendered.join(", ")),
            )
            .dangerous(),
        );
    }
    Some(diagnostic)
}

/// Appends literal text to the entries, starting a new entry at each `:`.
/// Text with an escape, a brace, or a quote is not what a path literal with
/// the same spelling would hold, so it ends the match.
fn push_text<'a>(entries: &mut Vec<Vec<Piece<'a>>>, text: &'a str) -> Option<()> {
    if text.contains(['\\', '{', '}', '"', '\n']) {
        return None;
    }
    for (index, chunk) in text.split(':').enumerate() {
        if index > 0 {
            entries.push(Vec::new());
        }
        if !chunk.is_empty() {
            entries.last_mut()?.push(Piece::Text(chunk));
        }
    }
    Some(())
}

/// `e"NAME" ?? ""` or `env.get("NAME") ?? ""` for the given name.
fn reads_variable_or_empty(arena: &AstArena, expr: ExprId, variable: &str) -> bool {
    let ArenaExprKind::Binary {
        op: BinaryOp::ResultFallback,
        left,
        right,
    } = arena.expr(expr).kind
    else {
        return false;
    };
    let is_literal = |expr: ExprId, text: &str| {
        matches!(arena.expr(expr).kind, ArenaExprKind::Str(literal)
            if &**arena.string_literal(literal) == text)
    };
    if !is_literal(right, "") {
        return false;
    }
    match arena.expr(left).kind {
        ArenaExprKind::EnvString(name) => name == variable,
        ArenaExprKind::Call { callee, args } => {
            let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
                return false;
            };
            name == "get"
                && matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "env")
                && matches!(arena.call_args(args), [argument]
                    if matches!(argument.kind, ArenaCallArgKind::Positional(key)
                        if is_literal(key, variable)))
        }
        _ => false,
    }
}

/// One entry as a path expression: a Path as itself, literal text as a path
/// literal, and anything interpolated as a path interpolation, which holds
/// the same bytes as the formatted text for a `Str`.
fn entry_text(
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    entry: &[Piece<'_>],
) -> Option<String> {
    if let [Piece::Expr(span)] = entry
        && expr_types.get(span) == Some(&Type::Path)
    {
        return source.get(span.range()).map(str::to_owned);
    }
    let mut body = String::new();
    let mut interpolates = false;
    for piece in entry {
        match piece {
            Piece::Text(text) => body.push_str(text),
            Piece::Expr(span) => {
                if !matches!(expr_types.get(span), Some(Type::Str | Type::Path)) {
                    return None;
                }
                interpolates = true;
                body.push('{');
                body.push_str(source.get(span.range())?);
                body.push('}');
            }
        }
    }
    let prefix = if interpolates { "fp" } else { "p" };
    Some(format!("{prefix}\"{body}\""))
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
            only: Some(vec![DiagnosticCode::LintPreferEnvPathList]),
            ..LintOptions::default()
        };
        Linter::lint(&parsed.arena, source, options).diagnostics
    }

    fn replacements(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .map(|fix| fix.replacement.as_deref().unwrap())
            .collect()
    }

    #[test]
    fn a_formatted_search_path_is_offered_as_a_list() {
        let source = "proc build(root: Path, bin: Path, tools: Str) [process, env, error] {\n  env ({PATH: f\"{root}/usr/bin:{bin}:/opt/bin:{tools}:{e\"PATH\" ?? \"\"}\", LD_LIBRARY_PATH: f\"/usr/lib:{env.get(\"LD_LIBRARY_PATH\") ?? \"\"}\"}) {\n    run make\n  }\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert_eq!(
            replacements(&diagnostics),
            [
                "[fp\"{root}/usr/bin\", bin, p\"/opt/bin\", fp\"{tools}\", @env.PathList.PATH ?? []]",
                "[p\"/usr/lib\", @env.PathList.LD_LIBRARY_PATH ?? []]",
            ]
        );
        // The list is not the same value when the variable is unset, so
        // `--fix` must not apply it.
        assert!(
            diagnostics
                .iter()
                .flat_map(|diagnostic| &diagnostic.fix_hints)
                .all(|fix| fix.dangerous)
        );
        assert!(diagnostics[0].notes[0].contains("no trailing empty entry"));

        // The offered text is itself accepted and clean.
        let mut fixed = source.to_owned();
        for diagnostic in diagnostics.iter().rev() {
            let fix = &diagnostic.fix_hints[0];
            fixed.replace_range(
                fix.span.unwrap().range(),
                fix.replacement.as_deref().unwrap(),
            );
        }
        assert!(lint(&fixed).is_empty());
    }

    // A different variable, a fallback that is not empty, a leading or empty
    // entry, and a value that is not the last entry are other values.
    #[test]
    fn other_strings_are_left_alone() {
        let source = "proc build(root: Path) [process, env, error] {\n  let overlay = {\n    PATH: f\"{root}/bin:{e\"HOME\" ?? \"\"}\",\n    MANPATH: f\"{root}/man:{e\"MANPATH\" ?? \"/usr/share/man\"}\",\n    CDPATH: f\"{e\"CDPATH\" ?? \"\"}:{root}\",\n    INFOPATH: f\"{root}/info::{e\"INFOPATH\" ?? \"\"}\",\n    LABEL: f\"{root}:{e\"LABEL\"?}\",\n    ONLY: f\"{e\"ONLY\" ?? \"\"}\",\n  }\n  env (overlay) {\n    run make\n  }\n}\n";
        assert!(lint(source).is_empty());
    }

    // An interpolation that is not text or a path has no path spelling, so
    // the report carries no rewrite.
    #[test]
    fn an_entry_without_a_path_spelling_is_reported_without_a_rewrite() {
        let source = "proc build(jobs: Int) [process, env, error] {\n  env ({PATH: f\"/opt/{jobs}:{e\"PATH\" ?? \"\"}\"}) {\n    run make\n  }\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
    }
}
