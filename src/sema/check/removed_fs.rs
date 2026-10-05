//! Calls of the `fs` functions that were removed because a `Path` method of
//! the same name is the one spelling of the operation.
//!
//! `fs.OP(PATH, REST)` is reported with the rewrite to `PATH.OP(REST)`. The
//! rewrite moves the first argument in front of the call and leaves the rest
//! as written, so operands are still evaluated left to right.

use super::{
    Checker, Diagnostic, FixHint, Label, MethodReceiver, ModuleFnSig, Span, Type, api_spec,
    call_arg_span_arena,
};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, AstArena, ExprId,
};
use xsh_registry::signature::{LabelRule, REMOVED_FS_PATH_FUNCTIONS};

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
pub fn call_receiver_text(arena: &AstArena, source: &str, operand: ExprId) -> Option<String> {
    let operand = arena.expr(operand);
    let text = source.get(operand.span.range())?;
    let quoted = |value: &str, written: &str| {
        (value == written && !written.contains(['"', '\\', '\n']))
            .then(|| format!("p\"{written}\""))
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

/// Whether `fs.NAME` named a function that a `Path` method replaced.
pub(super) fn is_removed_fs_function(name: &str) -> bool {
    REMOVED_FS_PATH_FUNCTIONS.contains(&name)
}

impl Checker {
    /// Checks `fs.NAME(PATH, REST)` for a removed `NAME` and reports it.
    ///
    /// The path is checked as the `Path` the function took, and the rest
    /// against the method's own overloads, which is how the rewritten call
    /// will be checked. The result is the method's, so the code around the
    /// call is checked as it will be after the rewrite.
    ///
    /// `fs.executable(mode: Int)` is still a function, so `executable`
    /// arrives here only through [`Self::check_fs_executable_argument`].
    pub(super) fn check_removed_fs_function_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        let methods = api_spec()
            .method_overloads(MethodReceiver::Path, name)
            .expect("a removed fs function has its Path method");
        // The function took every operand by position, so a label the method
        // requires is not asked of this call: the rewrite writes it.
        let labeled = methods.iter().any(|method| {
            method
                .sig
                .params
                .first()
                .is_some_and(|param| param.label == LabelRule::Required)
        });
        let overloads = methods
            .iter()
            .map(|method| {
                let mut sig = method.sig.clone();
                for param in &mut sig.params {
                    param.label = LabelRule::Free;
                }
                sig
            })
            .collect::<Vec<ModuleFnSig>>();
        let Some((path, rest)) = args.split_first() else {
            self.report_removed_fs_function(arena, source, name, None, None, span);
            return overloads[0].return_ty.clone();
        };
        let path_ty = self.check_leading_argument(arena, source, &path.kind, &Type::Path);
        let sig = if let [sig] = overloads.as_slice() {
            self.check_module_sig_args_arena(arena, source, rest, sig, span);
            sig
        } else {
            self.check_module_overload_args_arena(arena, source, "fs", name, rest, &overloads, span)
        };
        // A parameter whose label is part of the method's spelling had no
        // label as the function's second operand.
        let label = match (rest.first().map(|arg| &arg.kind), sig.params.first()) {
            (Some(ArenaCallArgKind::Positional(_)), Some(param)) if labeled => Some(param.name),
            _ => None,
        };
        self.report_removed_fs_function(arena, source, name, Some((path, &path_ty)), label, span);
        sig.return_ty.clone()
    }

    /// Checks the one positional argument of `fs.executable(ARG)` against
    /// the `Int` the remaining overload takes. `Some` is the result of the
    /// removed `Path` overload, reported; `None` means the argument was
    /// checked as the mode and the call is the function that still exists.
    pub(super) fn check_fs_executable_argument(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: &ArenaCallArg,
        span: Span,
    ) -> Option<Type> {
        let actual = self.check_leading_argument(arena, source, &arg.kind, &Type::Int);
        if !matches!(actual.unvalidated(), Type::Path | Type::Str) {
            self.expect_type(&Type::Int, &actual, call_arg_span_arena(arena, &arg.kind));
            return None;
        }
        self.report_removed_fs_function(
            arena,
            source,
            "executable",
            Some((arg, &actual)),
            None,
            span,
        );
        Some(Type::Result(Box::new(Type::Bool), Box::new(Type::Error)))
    }

    /// Checks one argument as a standard API parameter of type `expected`
    /// and returns its type without comparing the two.
    fn check_leading_argument(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: &ArenaCallArgKind,
        expected: &Type,
    ) -> Type {
        let previous = self
            .expected_schema
            .replace(crate::sema::constants::SchemaExpectation::default());
        let actual = self.check_call_arg_arena(arena, source, arg, Some(expected));
        self.expected_schema = previous;
        actual
    }

    /// Reports a call of the removed `fs.NAME`. The rewrite is offered when
    /// the path argument is statically a `Path` or a string literal, which
    /// the function read as one; other text has no `Path` method, so the
    /// caller converts it first.
    fn report_removed_fs_function(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        path: Option<(&ArenaCallArg, &Type)>,
        label: Option<&str>,
        span: Span,
    ) {
        let mut diagnostic = removed_fs_function_diagnostic(name, span);
        if let Some((arg, ty)) = path
            && let ArenaCallArgKind::Positional(operand) = arg.kind
        {
            let literal = matches!(arena.arena.expr(operand).kind, ArenaExprKind::Str(_));
            if *ty.unvalidated() == Type::Path || literal {
                if let Some(fix) =
                    method_call_head(&arena.arena, source, span, operand, name, label)
                {
                    diagnostic = diagnostic.with_fix_hint(fix);
                }
            } else if *ty.unvalidated() == Type::Str {
                diagnostic = diagnostic.with_note(format!(
                    "`{name}` is a method of `Path`, and this argument is a `Str`; convert it first with `Path(text)`"
                ));
            }
        }
        self.diagnostics.push(diagnostic);
    }

    /// Reports the command form `fs.NAME WORD ...` of a removed function. A
    /// command word has no receiver spelling, so the statement is rewritten
    /// by hand as the method call.
    pub(super) fn report_removed_fs_command(&mut self, name: &str, span: Span) {
        self.diagnostics.push(
            removed_fs_function_diagnostic(name, span).with_note(format!(
                "a method has no command form; write the call `PATH.{name}(...)`"
            )),
        );
    }
}

fn removed_fs_function_diagnostic(name: &str, span: Span) -> Diagnostic {
    let (message, label) = if name == "executable" {
        (
            "`fs.executable` no longer takes a path; call the `Path` method `executable`"
                .to_string(),
            "`fs.executable` takes only a mode".to_string(),
        )
    } else {
        (
            format!("`fs.{name}` was removed; call the `Path` method `{name}`"),
            format!("`fs.{name}` is no longer a function"),
        )
    };
    Diagnostic::error(message)
        .with_code(DiagnosticCode::CheckRemovedFsFunction)
        .with_label(Label::primary(span, label))
}

/// Replaces `fs.OP(PATH, ` (or `fs.OP(PATH`) with `PATH.OP(`, followed by
/// `label: ` when the next argument now needs one. The next argument takes
/// the place of the path, after whatever whitespace stood before the path,
/// so a call written one argument to a line stays so. A comment beside the
/// path is left for a manual rewrite.
fn method_call_head(
    arena: &AstArena,
    source: &str,
    call: Span,
    path: ExprId,
    function: &str,
    label: Option<&str>,
) -> Option<FixHint> {
    let receiver = call_receiver_text(arena, source, path)?;
    let path = arena.expr(path).span;
    let open = source
        .get(call.start()..path.start())?
        .strip_prefix("fs.")?
        .strip_prefix(function)?;
    if !open.starts_with('(') || !open[1..].trim().is_empty() {
        return None;
    }
    let rest = source.get(path.end()..call.end())?;
    let comma = usize::from(rest.starts_with(','));
    let after = &rest[comma..];
    let gap = after.len() - after.trim_start().len();
    let label = label.map(|label| format!("{label}: ")).unwrap_or_default();
    let replacement = match after[gap..].chars().next()? {
        ')' => format!("{receiver}.{function}("),
        '#' => return None,
        _ if comma == 1 => format!("{receiver}.{function}{open}{label}"),
        _ => return None,
    };
    Some(FixHint::replacement(
        Span::new(call.source_id, call.start(), path.end() + comma + gap),
        format!("call the `Path` method `{function}`"),
        replacement,
    ))
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{Diagnostic, DiagnosticCode};
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn check(source: &str) -> Vec<Diagnostic> {
        let program = Parser::parse_source_arena_only(SourceId::new(0), source).arena;
        program
            .symbol_owner()
            .with_current(|| Checker::check_arena(&program, source).diagnostics)
    }

    fn codes(diagnostics: &[Diagnostic]) -> Vec<Option<DiagnosticCode>> {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
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
                fix.replacement.as_deref().unwrap_or_default(),
            );
        }
        fixed
    }

    // A call inside a command argument's own delimiters is rewritten like any
    // other, and the rewritten program has nothing left to report.
    #[test]
    fn a_call_in_any_position_becomes_the_method_call() {
        let source = "proc show(log: Path, names: List[Path], dest: Path) [fs, process, error] {\n  print (fs.exists(log)?) ${fs.read_text(log)?}\n  run ls @([fs.read_text(names[0])?])\n  fs.copy(\n    log,\n    dest,\n    overwrite: true,\n  )\n  fs.rename(log, to: dest)\n}\n";
        let diagnostics = check(source);
        assert_eq!(
            codes(&diagnostics),
            [Some(DiagnosticCode::CheckRemovedFsFunction); 5],
            "{diagnostics:?}"
        );
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc show(log: Path, names: List[Path], dest: Path) [fs, process, error] {\n  print (log.exists()?) ${log.read_text()?}\n  run ls @([names[0].read_text()?])\n  log.copy(\n    to: dest,\n    overwrite: true,\n  )\n  log.rename(to: dest)\n}\n"
        );
        assert!(check(&fixed).is_empty(), "{:?}", check(&fixed));
    }

    // A triple-quoted string has layout a path literal lacks, and a path
    // passed by name has no receiver position to move to.
    #[test]
    fn a_path_with_no_receiver_spelling_is_reported_without_a_rewrite() {
        let source = "proc publish(out: Path, text: Str) [fs, error] {\n  fs.write(\"\"\"notes.txt\"\"\", text)\n  fs.write(path: out, data: text)\n  fs.mkdir()\n}\n";
        let diagnostics = check(source);
        assert_eq!(
            codes(&diagnostics),
            [Some(DiagnosticCode::CheckRemovedFsFunction); 3],
            "{diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.fix_hints.is_empty())
        );
    }

    // The arguments after the path are checked as the method's, so a mistake
    // among them is reported once, beside the removal.
    #[test]
    fn the_remaining_arguments_are_checked_as_the_method_arguments() {
        let diagnostics =
            check("proc publish(out: Path) [fs, error] {\n  fs.chmod(out, \"rw\")\n}\n");
        assert_eq!(
            codes(&diagnostics),
            [
                Some(DiagnosticCode::CheckTypeMismatch),
                Some(DiagnosticCode::CheckRemovedFsFunction),
            ],
            "{diagnostics:?}"
        );
    }

    // The mode test is still a function: a mode checks, and a value that is
    // neither a mode nor a path is a mismatch with the `Int` it takes.
    #[test]
    fn executable_still_takes_a_mode() {
        let source = "pure runnable(mode: Int) -> Bool {\n  fs.executable(mode) and fs.executable(0o755)\n}\n";
        assert!(check(source).is_empty(), "{:?}", check(source));
        let diagnostics = check("let wrong = fs.executable(true)\n");
        assert_eq!(
            codes(&diagnostics),
            [Some(DiagnosticCode::CheckTypeMismatch)],
            "{diagnostics:?}"
        );
    }
}
