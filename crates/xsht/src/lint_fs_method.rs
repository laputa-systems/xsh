use std::collections::BTreeMap;
use std::sync::OnceLock;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};
use xsh_registry::signature::{MethodReceiver, api_spec};
use xsh_registry::types::Type as RegistryType;

/// `fs.OP(PATH, ...)` where `PATH.OP(...)` is the same operation:
///
/// ```text
/// fs.write(out, text)?
/// fs.remove(stale, missing_ok: true)?
/// ```
///
/// is `out.write(text)?` and `stale.remove(missing_ok: true)?`.
///
/// Which operations have both spellings is read from the registry, never
/// listed here: an `fs` function is paired with the `Path` method of the same
/// name when the method runs the same operation and takes exactly the
/// parameters that follow the function's leading `Path` one. The method call
/// then selects the overload the function call selected and passes it the
/// same values.
///
/// The rewrite moves the first argument in front of the call and leaves the
/// rest as written, so operands are still evaluated left to right. It is
/// offered only when that argument is statically a `Path` (a `Str` there is
/// converted by the function, and a `Str` has no such method) and can be
/// written before `.OP(` unchanged.
pub(super) fn fs_function_with_a_path_method(
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
    if fs_is_shadowed
        || !matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "fs")
    {
        return None;
    }
    let function = name.as_str();
    let function = function.as_str();
    if !operations_with_a_path_method().contains(&function) {
        return None;
    }
    let ArenaCallArgKind::Positional(path) = arena.call_args(args).first()?.kind else {
        return None;
    };
    let path_span = arena.expr(path).span;
    if expr_types.get(&path_span) != Some(&Type::Path) {
        return None;
    }
    let mut diagnostic = Diagnostic::warning(format!(
        "`fs.{function}` is called on a path that has the method `{function}`"
    ))
    .with_code(DiagnosticCode::LintPreferPathMethod)
    .with_label(Label::secondary(
        call.span,
        "the `Path` method is the one spelling of this operation",
    ));
    if let Some(fix) = method_call_head(arena, source, call.span, callee, path, function) {
        diagnostic = diagnostic.with_fix_hint(fix);
    }
    Some(diagnostic)
}

/// Replaces `fs.OP(PATH, ` (or `fs.OP(PATH`) with `PATH.OP(`. The next
/// argument takes the place of the path, after whatever whitespace stood
/// before the path, so a call written one argument to a line stays so. A
/// comment beside the path is left for a manual rewrite.
fn method_call_head(
    arena: &AstArena,
    source: &str,
    call: Span,
    callee: ExprId,
    path: ExprId,
    function: &str,
) -> Option<FixHint> {
    let receiver = super::lint_path_kind::call_receiver_text(arena, source, path)?;
    let path = arena.expr(path).span;
    let open = source.get(arena.expr(callee).span.end()..path.start())?;
    if !open.starts_with('(') || !open[1..].trim().is_empty() {
        return None;
    }
    let rest = source.get(path.end()..call.end())?;
    let comma = usize::from(rest.starts_with(','));
    let after = &rest[comma..];
    let gap = after.len() - after.trim_start().len();
    let replacement = match after[gap..].chars().next()? {
        ')' => format!("{receiver}.{function}("),
        '#' => return None,
        _ if comma == 1 => format!("{receiver}.{function}{open}"),
        _ => return None,
    };
    Some(FixHint::replacement(
        Span::new(call.source_id, call.start(), path.end() + comma + gap),
        format!("call the `Path` method `{function}`"),
        replacement,
    ))
}

/// The `fs` functions that a `Path` method replaces, in registry order.
pub(super) fn operations_with_a_path_method() -> &'static [&'static str] {
    static OPERATIONS: OnceLock<Vec<&'static str>> = OnceLock::new();
    OPERATIONS.get_or_init(|| {
        let spec = api_spec();
        let Some(fs) = spec.modules.iter().find(|module| module.name == "fs") else {
            return Vec::new();
        };
        let Some(path) = spec
            .methods
            .iter()
            .find(|entry| entry.receiver == MethodReceiver::Path)
        else {
            return Vec::new();
        };
        fs.sig
            .functions
            .iter()
            .filter(|function| {
                let Some(method) = path
                    .methods
                    .iter()
                    .find(|method| method.name == function.name)
                else {
                    return false;
                };
                // Every overload that takes a path first has its method, so
                // whichever one a call selects can be rewritten.
                let mut path_first = function
                    .overloads
                    .iter()
                    .filter(|overload| {
                        overload
                            .params
                            .first()
                            .is_some_and(|param| param.ty == RegistryType::Path && !param.defaulted)
                    })
                    .peekable();
                path_first.peek().is_some()
                    && path_first.all(|overload| {
                        method.overloads.iter().any(|candidate| {
                            candidate.sig.op == overload.op
                                && candidate.sig.return_ty == overload.return_ty
                                && candidate.sig.params.len() + 1 == overload.params.len()
                                && candidate
                                    .sig
                                    .params
                                    .iter()
                                    .zip(&overload.params[1..])
                                    .all(|(method, function)| {
                                        method.name == function.name
                                            && method.ty == function.ty
                                            && method.defaulted == function.defaulted
                                    })
                        })
                    })
            })
            .map(|function| function.name)
            .collect()
    })
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
            only: Some(vec![DiagnosticCode::LintPreferPathMethod]),
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

    // The operations are derived from the registry. This list is the
    // contract a later removal of the `fs` spellings works from: a change
    // here is a change to which calls the lint rewrites.
    #[test]
    fn the_operations_with_both_spellings_are_the_ones_listed() {
        assert_eq!(
            super::operations_with_a_path_method(),
            [
                "metadata",
                "read_text",
                "write",
                "write_atomic",
                "exists",
                "executable",
                "copy",
                "rename",
                "mkdir",
                "remove",
                "chmod",
            ]
        );
    }

    #[test]
    fn a_function_call_on_a_path_becomes_the_method_call() {
        let source = "proc publish(out: Path, root: Path, text: Str) [fs, error] -> Result[Bool] {\n  fs.mkdir(fp\"{root}/cache\", parents: true)\n  fs.write(out, text)\n  fs.remove(root.parent(), missing_ok: true)\n  fs.write(\n    fp\"{root}/notes\",\n    text,\n  )\n  fs.mkdir(\n    root,\n  )\n  let size = fs.read_text(out)?.byte_len()\n  Ok(fs.exists(p\"/tmp/marker\")? and size > 0)\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 7, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc publish(out: Path, root: Path, text: Str) [fs, error] -> Result[Bool] {\n  fp\"{root}/cache\".mkdir(parents: true)\n  out.write(text)\n  root.parent().remove(missing_ok: true)\n  fp\"{root}/notes\".write(\n    text,\n  )\n  root.mkdir()\n  let size = out.read_text()?.byte_len()\n  Ok(p\"/tmp/marker\".exists()? and size > 0)\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // A function whose method takes other parameters, a function with no
    // path operand, and the mode test that shares a name with the path test
    // are not the same call.
    #[test]
    fn other_calls_are_left_alone() {
        let source = "proc publish(out: Path, mode: Int) [fs, error] -> Result[Path] {\n  fs.symlink(out, fp\"{out}.link\")\n  Ok(if fs.executable(mode) { fs.cwd()? } else { out })\n}\n";
        assert!(lint(source).is_empty(), "{:?}", lint(source));
    }

    // An operand that cannot stand before `.OP(` as written is respelled: a
    // propagated call is used directly, an expression is parenthesized, and a
    // bare path literal or a string literal read as a path is written
    // `p"..."`. In each the operand is still evaluated first.
    #[test]
    fn an_operand_that_is_not_a_receiver_is_respelled() {
        let source = "proc publish(out: Path, other: Path, first: Bool, text: Str) [fs, error] {\n  fs.write(if first { out } else { other }, text)\n  fs.write(/tmp/marker, text)\n  fs.write(\"marker.txt\", text)\n  fs.copy(out.resolve()?, other)\n  let found = fs.exists(\"marker.txt\")?\n  assert found\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 5, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc publish(out: Path, other: Path, first: Bool, text: Str) [fs, error] {\n  (if first { out } else { other }).write(text)\n  p\"/tmp/marker\".write(text)\n  p\"marker.txt\".write(text)\n  out.resolve()?.copy(other)\n  let found = p\"marker.txt\".exists()?\n  assert found\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // An expression between a command argument's own delimiters is linted
    // like any other: `(EXPR)`, `${EXPR}`, and `@(EXPR)`.
    #[test]
    fn a_call_inside_a_command_argument_is_rewritten() {
        let source = "proc show(log: Path, names: List[Path]) [fs, process, error] {\n  print (fs.exists(log)?) ${fs.read_text(log)?}\n  run ls @([fs.read_text(names[0])?])\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        assert_eq!(
            apply(&diagnostics, source),
            "proc show(log: Path, names: List[Path]) [fs, process, error] {\n  print (log.exists()?) ${log.read_text()?}\n  run ls @([names[0].read_text()?])\n}\n"
        );
    }

    // The formatter writes a command argument's expression in parentheses
    // and an f-string argument without them. Either way it is the same
    // argument, so each spelling has the same findings; only the rewrite
    // waits for the parentheses.
    #[test]
    fn a_command_argument_has_the_same_findings_however_it_is_laid_out() {
        let program = |argument: &str| {
            format!("proc show(log: Path) [fs, error] {{\n  print {argument}\n}}\n")
        };
        let bare = lint(&program("fs.read_text(log)?"));
        let wrapped = lint(&program("(fs.read_text(log)?)"));
        assert_eq!(bare.len(), 1, "{bare:?}");
        assert_eq!(wrapped.len(), 1, "{wrapped:?}");
        assert_eq!(bare[0].message, wrapped[0].message);
        assert!(bare[0].fix_hints.is_empty());
        assert_eq!(wrapped[0].fix_hints.len(), 1);

        let bare_text = lint(&program("f\"{fs.read_text(log)?}\""));
        let wrapped_text = lint(&program("(f\"{fs.read_text(log)?}\")"));
        assert_eq!(bare_text.len(), wrapped_text.len());
    }

    // A comment beside the path would be lost, and a string that a path
    // literal reads differently (`${`, a triple-quoted or raw string) has no
    // quoted path spelling here; each is reported without a rewrite.
    #[test]
    fn a_call_that_cannot_be_rewritten_in_place_is_reported_without_a_rewrite() {
        let source = "proc publish(out: Path, text: Str) [fs, error] {\n  fs.write(\n    out, # the target\n    text,\n  )\n  fs.write(\"cost-$5.txt\", text)\n  fs.write(\"\"\"notes.txt\"\"\", text)\n  fs.write(r\"raw\\name.txt\", text)\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.fix_hints.is_empty()));
    }

    // A path literal decodes the escapes a string does, so an escaped
    // one-line string keeps its text between the quotes.
    #[test]
    fn a_string_with_escapes_becomes_the_path_literal_with_the_same_escapes() {
        let source = "proc publish(text: Str) [fs, error] {\n  fs.write(\"tab\\there.txt\", text)\n  fs.write(\"quote\\\"here \\\\ \\u{e9}.txt\", text)\n  fs.write(\"\", text)\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc publish(text: Str) [fs, error] {\n  p\"tab\\there.txt\".write(text)\n  p\"quote\\\"here \\\\ \\u{e9}.txt\".write(text)\n  p\"\".write(text)\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }
}
