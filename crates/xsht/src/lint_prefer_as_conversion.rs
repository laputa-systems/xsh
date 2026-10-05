use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::{Conversion, Type};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};

/// The operand of a conversion, the type it converts to, and the source text
/// that follows the operand when the conversion is written as its operation.
struct Operation {
    operand: ExprId,
    target: &'static str,
    conversion: Conversion,
}

/// A propagated operation that `as` names is the conversion:
/// `text.parse_int()?` is `text as Int`, and likewise `parse_uint()?`,
/// `parse_float()?`, `data.utf8()?`, `Path.parse_bytes(data)?`,
/// `Path.parse_bytes(bytes.from_text(text))?`, and `count.require(UInt)?`.
///
/// Only an operation directly under `?` is one: a `Result` kept as a value,
/// as in `text.parse_int() ?? 0` or a `match` on it, is not a conversion.
/// The operand must have exactly the type the conversion table lists, so the
/// rewrite selects the operation it replaces.
///
/// The replacement is grouped; the grouping that the place it lands in does
/// not need is removed when fixes are prepared. An operation written over
/// several lines, with a comment inside, or on the pipeline item is reported
/// without a fix.
pub(super) fn propagated_conversion(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    bytes_is_shadowed: bool,
    expr: ExprId,
) -> Option<Diagnostic> {
    let node = arena.expr(expr);
    let ArenaExprKind::Try(inner) = node.kind else {
        return None;
    };
    let operation = operation(arena, bytes_is_shadowed, inner)?;
    let operand = arena.expr(operation.operand);
    let from = expr_types.get(&operand.span)?;
    let to = operation.conversion.target();
    if Conversion::select(from, &to) != Some(operation.conversion) {
        return None;
    }
    let target = operation.target;
    let diagnostic = Diagnostic::new(
        Severity::Warning,
        format!("prefer `as {target}` over the propagated operation it names"),
    )
    .with_code(DiagnosticCode::LintPreferAsConversion)
    .with_label(Label::secondary(
        node.span,
        format!("this converts {from} to {target} and propagates a failure; write `... as {target}`"),
    ));
    let text = source.get(node.span.range())?;
    // The operand is spelled with whatever groups it; a pipeline stage that
    // names a method has no operand in its own text.
    let inside =
        node.span.start() <= operand.span.start() && operand.span.end() <= node.span.end();
    let spelled = inside
        && match operation.conversion {
            Conversion::BytesToPath | Conversion::TextToPath => true,
            _ => text[operand.span.end() - node.span.start()..]
                .trim_start_matches(')')
                .starts_with('.'),
        };
    let fixable = spelled
        && !text.contains('\n')
        && !super::span_may_contain_comment(source, node.span)
        && !matches!(operand.kind, ArenaExprKind::Item);
    if !fixable {
        return Some(diagnostic);
    }
    let receiver = match operation.conversion {
        // The operand is an argument: its own grouping, if any, is its text.
        Conversion::BytesToPath | Conversion::TextToPath => {
            source[super::widen_over_grouping(source, operand.span).range()].to_owned()
        }
        // The operand is a receiver: everything before the `.` of the call.
        _ => {
            let mut end = operand.span.end();
            while source[end..].starts_with(')') {
                end += 1;
            }
            source[node.span.start()..end].to_owned()
        }
    };
    Some(diagnostic.with_fix_hint(FixHint::replacement(
        node.span,
        format!("convert with `as {target}`"),
        format!("({receiver} as {target})"),
    )))
}

/// The conversion that `call` is the operation of, if it is one.
fn operation(arena: &AstArena, bytes_is_shadowed: bool, call: ExprId) -> Option<Operation> {
    match arena.expr(call).kind {
        ArenaExprKind::Require {
            value,
            schema: Some(schema),
        } if arena.type_expr_named(schema, "UInt") => Some(Operation {
            operand: value,
            target: "UInt",
            conversion: Conversion::IntToUInt,
        }),
        ArenaExprKind::Call { callee, args } => {
            let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
                return None;
            };
            let name = name.as_str();
            let args = arena.call_args(args);
            if matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "Path") {
                let [argument] = args else {
                    return None;
                };
                let ArenaCallArgKind::Positional(bytes) = argument.kind else {
                    return None;
                };
                if name.as_str() != "parse_bytes" {
                    return None;
                }
                // `Path.parse_bytes(bytes.from_text(text))` reads the text.
                if !bytes_is_shadowed
                    && let Some(text) = text_as_bytes(arena, bytes)
                {
                    return Some(Operation {
                        operand: text,
                        target: "Path",
                        conversion: Conversion::TextToPath,
                    });
                }
                return Some(Operation {
                    operand: bytes,
                    target: "Path",
                    conversion: Conversion::BytesToPath,
                });
            }
            if !args.is_empty() {
                return None;
            }
            let (target, conversion) = match name.as_str() {
                "parse_int" => ("Int", Conversion::TextToInt),
                "parse_uint" => ("UInt", Conversion::TextToUInt),
                "parse_float" => ("Float", Conversion::TextToFloat),
                "utf8" => ("Str", Conversion::BytesToText),
                _ => return None,
            };
            Some(Operation {
                operand: base,
                target,
                conversion,
            })
        }
        _ => None,
    }
}

/// The text argument of `bytes.from_text(text)`, where `bytes` names the
/// standard module.
fn text_as_bytes(arena: &AstArena, expr: ExprId) -> Option<ExprId> {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if name.as_str().as_str() != "from_text"
        || !matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "bytes")
    {
        return None;
    }
    let [argument] = arena.call_args(args) else {
        return None;
    };
    let ArenaCallArgKind::Positional(text) = argument.kind else {
        return None;
    };
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn conversions(source: &str) -> Vec<Diagnostic> {
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
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferAsConversion))
        .collect()
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
                fix.replacement.as_deref().unwrap(),
            );
        }
        fixed
    }

    #[test]
    fn every_propagated_operation_of_the_table_becomes_a_conversion() {
        let source = "pure probe(text: Str, raw: Bytes, count: Int) -> Result[Str] {\n  let a = text.parse_int()?\n  let b = text.trim().parse_uint()?\n  let c = text.parse_float()?\n  let d = raw.utf8()?\n  let e = Path.parse_bytes(raw)?\n  let f = Path.parse_bytes(bytes.from_text(text))?\n  let g = count.require(UInt)?\n  Ok(f\"{a} {b} {c} {d} {e} {f} {g}\")\n}\n";
        let diagnostics = conversions(source);
        assert_eq!(diagnostics.len(), 7, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "pure probe(text: Str, raw: Bytes, count: Int) -> Result[Str] {\n  let a = text as Int\n  let b = text.trim() as UInt\n  let c = text as Float\n  let d = raw as Str\n  let e = raw as Path\n  let f = text as Path\n  let g = count as UInt\n  Ok(f\"{a} {b} {c} {d} {e} {f} {g}\")\n}\n"
        );
        // The fixed program checks and is not reported again.
        assert!(conversions(&fixed).is_empty());
    }

    /// The conversion is grouped exactly where the place it lands in needs
    /// it: under a prefix, before a suffix, and where a statement begins
    /// with a name.
    #[test]
    fn the_conversion_keeps_only_the_grouping_its_place_needs() {
        let source = "proc probe(text: Str, parts: List[Str]) -> Result[Int] {\n  let a = -text.parse_int()?\n  let b = text.parse_int()? * 2 + parts[0].parse_int()?\n  let c = (text + \"0\").parse_int()?\n  let d = [text.parse_int()?, 1]\n  let e = parts[(text.parse_int()?)..]\n  let f = f\"{text.parse_int()?}\"\n  let g = try { text.parse_int()? }\n  let h = (text.parse_int()?).float()\n  print $a $b $c ${d.len()} ${e.len()} $f ${g ?? 0} $h (text.parse_int()?)\n  text.parse_int()?\n}\n";
        let diagnostics = conversions(source);
        assert_eq!(diagnostics.len(), 11, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc probe(text: Str, parts: List[Str]) -> Result[Int] {\n  let a = -(text as Int)\n  let b = text as Int * 2 + parts[0] as Int\n  let c = (text + \"0\") as Int\n  let d = [text as Int, 1]\n  let e = parts[(text as Int)..]\n  let f = f\"{text as Int}\"\n  let g = try { (text as Int) }\n  let h = (text as Int).float()\n  print $a $b $c ${d.len()} ${e.len()} $f ${g ?? 0} $h (text as Int)\n  (text as Int)\n}\n"
        );
        assert!(conversions(&fixed).is_empty());
    }

    /// A `Result` kept as a value is not a conversion, and neither is an
    /// operation the table does not list or one on a value of another type.
    #[test]
    fn results_kept_as_values_and_other_operations_are_left_alone() {
        let source = "pure probe(text: Str, dynamic: Any, size: UInt) -> Result[Int] {\n  let a = text.parse_int() ?? 0\n  let b = match text.parse_uint() {\n    Ok(value) => value,\n    Err(_) => 0,\n  }\n  let c = text.parse_int_decimal()?\n  let d = text.parse_uint_positive()?\n  let e = dynamic.require(UInt)?\n  let f = size.require(UInt)?\n  let g = dynamic.require(Int)?\n  let h = text.parse_int()\n  let i = text.parse_float()?.round()?\n  Ok(a + b + c + d + e + f + g + h? + i)\n}\n";
        assert!(conversions(source).is_empty(), "{:?}", conversions(source));
    }

    #[test]
    fn an_operation_that_cannot_be_rewritten_in_place_is_reported_without_a_fix() {
        let source = "pure probe(text: Str, lines: List[Str]) -> Result[Int] {\n  let a = text\n    .trim()\n    .parse_int()?\n  let b = text.trim( # inner\n  ).parse_int()?\n  let c = lines |> map .parse_int()? |> sum()\n  let d = text |> trim() |> parse_int()?\n  Ok(a + b + c + d)\n}\n";
        let diagnostics = conversions(source);
        assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.fix_hints.is_empty()),
            "{diagnostics:?}"
        );
    }
}
