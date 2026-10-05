use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaRecordFieldKind, AstArena, ExprId,
};

/// `.display()` on a checked Path that is handed straight to an operating
/// system byte sink, where the Path itself is accepted and arrives as its
/// native bytes:
///
/// - `bytes.from_text(P.display())`, which is `P.bytes()`;
/// - the target of `process.command_argv`, and the items of an argv list
///   written in the call;
/// - the values of an `env` record written in a call to
///   `process.command_argv`, `test.run_script`, `test.run_xsh`, or
///   `test.run_xsht_trace`.
///
/// For every path that is valid UTF-8 the sink receives the same bytes with
/// and without the conversion. For any other path the conversion substitutes
/// U+FFFD, so the child or the hash sees a name that does not exist; dropping
/// it is the correction, as it is for a Path interpolated into a command
/// word. Only operands written in the call are rewritten: a list or record
/// bound to a name first has a type of its own that other uses may rely on.
pub(super) fn path_display_sinks(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
) -> Vec<Diagnostic> {
    let call = arena.expr(expr);
    let ArenaExprKind::Call { callee, args } = call.kind else {
        return Vec::new();
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return Vec::new();
    };
    let ArenaExprKind::Ident(module) = arena.expr(base).kind else {
        return Vec::new();
    };
    let args = arena.call_args(args);
    let displayed = |operand: ExprId| displayed_path(arena, source, expr_types, operand);
    if module == "bytes" && name == "from_text" {
        let [argument] = args else {
            return Vec::new();
        };
        let ArenaCallArgKind::Positional(text) = argument.kind else {
            return Vec::new();
        };
        let Some((path, _)) = displayed(text) else {
            return Vec::new();
        };
        let mut diagnostic = sink_diagnostic(
            call.span,
            "`bytes()` gives the Path's native bytes without a text conversion",
        );
        if let Some(receiver) = source.get(path.range())
            && source
                .get(call.span.range())
                .is_some_and(|text| !text.contains('#'))
            && arena.expr(text).span.start() == path.start()
        {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                call.span,
                "take the Path's bytes",
                format!("{receiver}.bytes()"),
            ));
        }
        return vec![diagnostic];
    }
    // Positions of the operands that are byte sinks, by function. `env` may
    // also be passed by name.
    let (target, argv, env) = match (module.as_str().as_str(), name.as_str().as_str()) {
        ("process", "command_argv") => (Some(0), Some(1), 3),
        ("test", "run_script") => (None, None, 3),
        ("test", "run_xsh" | "run_xsht_trace") => (None, None, 4),
        _ => return Vec::new(),
    };
    let mut operands = Vec::new();
    for (position, argument) in args.iter().enumerate() {
        match argument.kind {
            ArenaCallArgKind::Positional(value) if Some(position) == target => {
                operands.push(value);
            }
            ArenaCallArgKind::Positional(value) if Some(position) == argv => {
                if let ArenaExprKind::List(items) = arena.expr(value).kind {
                    operands.extend(
                        arena
                            .list_elements(items)
                            .filter(|item| item.splice_span.is_none())
                            .map(|item| item.value),
                    );
                }
            }
            ArenaCallArgKind::Positional(value) if position == env => {
                operands.extend(record_values(arena, value));
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "env" => {
                operands.extend(record_values(arena, value));
            }
            _ => {}
        }
    }
    operands
        .into_iter()
        .filter_map(|operand| {
            let (_, conversion) = displayed(operand)?;
            let mut diagnostic = sink_diagnostic(
                arena.expr(operand).span,
                "this sink takes the Path itself and passes its native bytes",
            );
            if let Some(conversion) = conversion {
                diagnostic = diagnostic
                    .with_fix_hint(FixHint::deletion(conversion, "pass the Path itself"));
            }
            Some(diagnostic)
        })
        .collect()
}

fn sink_diagnostic(span: Span, label: &'static str) -> Diagnostic {
    Diagnostic::warning("a Path is converted to display text on its way to a byte sink")
        .with_code(DiagnosticCode::LintPathDisplaySink)
        .with_label(Label::secondary(span, label))
        .with_note("display text replaces bytes that are not UTF-8, which the sink would otherwise receive unchanged")
}

/// The values of a record literal's `name: value` fields.
fn record_values(arena: &AstArena, record: ExprId) -> Vec<ExprId> {
    let ArenaExprKind::Record(fields) = arena.expr(record).kind else {
        return Vec::new();
    };
    arena
        .record_fields(fields)
        .iter()
        .filter_map(|field| match field.kind {
            ArenaRecordFieldKind::Named { value, .. } => Some(value),
            _ => None,
        })
        .collect()
}

/// For `P.display()` on a checked Path: the span of `P`, and the span of the
/// `.display()` after it when that is spelled exactly so.
fn displayed_path(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    operand: ExprId,
) -> Option<(Span, Option<Span>)> {
    let display = arena.expr(operand);
    let ArenaExprKind::Call { callee, args } = display.kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if name != "display" || !arena.call_args(args).is_empty() {
        return None;
    }
    let path = arena.expr(base).span;
    if expr_types.get(&path) != Some(&Type::Path) {
        return None;
    }
    let conversion = Span::new(path.source_id, path.end(), display.span.end());
    Some((
        path,
        (source.get(conversion.range()) == Some(".display()")).then_some(conversion),
    ))
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
            only: Some(vec![DiagnosticCode::LintPathDisplaySink]),
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
    fn display_before_a_byte_sink_is_dropped() {
        let source = "proc launch(exe: Path, root: Path, flags: List[Str]) [process, error] -> Result[Bytes] {\n  let plan = process.command_argv(exe.display(), [exe.display(), \"--root\", root.display(), @flags], env: {ROOT: root.display(), MODE: \"fast\"})\n  let _ = process.run(plan)?\n  bytes.from_text(root.display())\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 5, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc launch(exe: Path, root: Path, flags: List[Str]) [process, error] -> Result[Bytes] {\n  let plan = process.command_argv(exe, [exe, \"--root\", root, @flags], env: {ROOT: root, MODE: \"fast\"})\n  let _ = process.run(plan)?\n  root.bytes()\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // A list or record bound to a name has its own type, and a Str parameter
    // is a text boundary.
    #[test]
    fn display_outside_a_sink_operand_is_left_alone() {
        let source = "pure label(text: Str) -> Str {\n  text\n}\n\nproc launch(exe: Path, root: Path, text: Str) [process, error] -> Result[Bytes] {\n  let argv = [exe.display(), root.display()]\n  let overlay = {ROOT: root.display()}\n  let plan = process.command_argv(exe, argv, env: overlay)\n  let _ = process.run(plan)?\n  print (label(root.display()))\n  bytes.from_text(text)\n}\n";
        assert!(lint(source).is_empty());
    }
}
