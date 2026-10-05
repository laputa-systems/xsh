//! `lint.prefer-argument-label`: an argument whose parameter is registered
//! with a label that makes the call read as a sentence is written with that
//! label.
//!
//! ```text
//! src.copy(dest)                  src.copy(to: dest)
//! text.replace("a", "b")          text.replace("a", with: "b")
//! text.replace("a", to: "b")      text.replace("a", with: "b")
//! fs.symlink(target, link)        link.symlink(to: target)
//! ```
//!
//! Which parameters are labeled, and what each was called before, is read
//! from the registry's label rule, so a method gains the migration by
//! declaring the rule. Writing a label changes nothing about the call: the
//! same argument reaches the same parameter. The `fs.symlink` rewrite is the
//! exception, because the method's receiver is the function's second operand:
//! the link is then evaluated before the target, so the fix is offered only
//! where that order cannot be observed.

use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaFmtPart, AstArena, ExprId,
};
use xsh_registry::signature::{LabelRule, MethodReceiver, ParamSig, api_spec};

pub(super) fn unlabeled_arguments(
    arena: &AstArena,
    source: &str,
    expr_types: &BTreeMap<Span, Type>,
    fs_is_shadowed: bool,
    expr: ExprId,
) -> Vec<Diagnostic> {
    let call = arena.expr(expr);
    let ArenaExprKind::Call { callee, args } = call.kind else {
        return Vec::new();
    };
    let (base, method, null_safe) = match arena.expr(callee).kind {
        ArenaExprKind::Field { base, name } => (base, name, false),
        ArenaExprKind::NullSafeField { base, name } => (base, name, true),
        _ => return Vec::new(),
    };
    let args = arena.call_args(args);
    if !fs_is_shadowed
        && !null_safe
        && method == "symlink"
        && matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "fs")
    {
        return symlink_function(arena, source, call.span, args)
            .into_iter()
            .collect();
    }
    let Some(receiver) = expr_types
        .get(&arena.expr(base).span)
        .and_then(|ty| labeled_receiver(ty, null_safe))
    else {
        return Vec::new();
    };
    let method = method.as_str();
    let Some(params) = method_params(receiver, method.as_str()) else {
        return Vec::new();
    };
    let callee_end = arena.expr(callee).span.end();
    params
        .iter()
        .enumerate()
        .filter_map(|(index, param)| {
            let LabelRule::Written { formerly } = param.label else {
                return None;
            };
            let site = Site {
                arena,
                source,
                expr_types,
                method: method.as_str(),
                label: param.name,
            };
            site.argument(args, index, formerly, callee_end)
        })
        .collect()
}

/// The receivers whose methods the registry labels. A receiver with type
/// arguments is not among them; `every_labeled_parameter_is_reachable` fails
/// when the registry labels a parameter this cannot reach.
fn labeled_receiver(ty: &Type, null_safe: bool) -> Option<MethodReceiver> {
    let mut ty = ty;
    if null_safe {
        // `value?.method(...)` calls the method on what a `Result` or an
        // optional holds.
        if let Type::Result(ok, _) = ty {
            ty = ok;
        }
        if let Type::Optional(inner) = ty {
            ty = inner;
        }
    }
    match ty.unvalidated() {
        Type::Str => Some(MethodReceiver::Str),
        Type::Path => Some(MethodReceiver::Path),
        Type::Regex => Some(MethodReceiver::Regex),
        _ => None,
    }
}

/// The parameters of a method that has one overload. Which overload of
/// several a call selects is the checker's knowledge, so an overloaded
/// method is not matched.
fn method_params(receiver: MethodReceiver, method: &str) -> Option<&'static [ParamSig]> {
    let overloads = &api_spec()
        .methods
        .iter()
        .find(|entry| entry.receiver == receiver)?
        .methods
        .iter()
        .find(|candidate| candidate.name == method)?
        .overloads;
    let [only] = overloads.as_slice() else {
        return None;
    };
    Some(&only.sig.params)
}

struct Site<'a> {
    arena: &'a AstArena,
    source: &'a str,
    expr_types: &'a BTreeMap<Span, Type>,
    method: &'a str,
    label: &'static str,
}

impl Site<'_> {
    /// The finding for the argument of parameter `index`, however the call
    /// supplies it.
    fn argument(
        &self,
        args: &[ArenaCallArg],
        index: usize,
        formerly: Option<&'static str>,
        callee_end: usize,
    ) -> Option<Diagnostic> {
        if args.iter().any(
            |arg| matches!(arg.kind, ArenaCallArgKind::Named { name, .. } if name == self.label),
        ) {
            return None;
        }
        if let Some(former) = formerly {
            let by_former_label = args.iter().find_map(|arg| match arg.kind {
                ArenaCallArgKind::Named { name, value, span } if name == former => {
                    Some(self.former_label(former, self.arena.span(span), value))
                }
                ArenaCallArgKind::NamedSpread { value, .. } => {
                    let value = self.arena.expr(value).span;
                    let Some(Type::Record(fields)) = self.expr_types.get(&value) else {
                        return None;
                    };
                    fields
                        .keys()
                        .any(|field| *field == former)
                        .then(|| self.spread_former_label(former, value))
                }
                _ => None,
            });
            if by_former_label.is_some() {
                return by_former_label;
            }
        }
        // A name or a spread before the position leaves it to binding which
        // parameter a later positional argument reaches.
        let leading = args.get(..=index)?;
        let values = leading
            .iter()
            .map(|arg| match arg.kind {
                ArenaCallArgKind::Positional(value) => Some(value),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let boundary = match index.checked_sub(1) {
            Some(previous) => (self.arena.expr(values[previous]).span.end(), ','),
            None => (callee_end, '('),
        };
        Some(self.positional(values[index], boundary))
    }

    fn positional(&self, value: ExprId, boundary: (usize, char)) -> Diagnostic {
        let value = self.arena.expr(value);
        let label = self.label;
        let diagnostic = Diagnostic::warning(format!(
            "`{}` is given its `{label}` argument by position",
            self.method
        ))
        .with_code(DiagnosticCode::LintPreferArgumentLabel)
        .with_label(Label::secondary(
            value.span,
            format!("`{label}:` says which operand this is"),
        ));
        let Some(at) = argument_start(self.source, boundary, value.span.start()) else {
            return diagnostic.with_note(
                "a comment stands before the argument; write the label after it by hand",
            );
        };
        // A value that is the label's own name is the shorthand `label:`.
        let (span, text) = if at == value.span.start()
            && matches!(value.kind, ArenaExprKind::Ident(name) if name == label)
        {
            (value.span, format!("{label}:"))
        } else {
            (
                Span::new(value.span.source_id, at, at),
                format!("{label}: "),
            )
        };
        self.with_fix(diagnostic, span, "write the label", text)
    }

    fn former_label(&self, former: &str, argument: Span, value: ExprId) -> Diagnostic {
        let value = self.arena.expr(value);
        let label = self.label;
        let diagnostic = Diagnostic::warning(format!(
            "`{former}` is the former label of an argument of `{}`",
            self.method
        ))
        .with_code(DiagnosticCode::LintPreferArgumentLabel)
        .with_label(Label::secondary(
            argument,
            format!("the argument is now labeled `{label}:`"),
        ));
        let start = argument.start();
        let written = self.source.get(start..value.span.end()).unwrap_or_default();
        // `former:` alone is the shorthand for `former: former`, whose value
        // keeps its name under the new label.
        let (end, text) = if value.span.start() == start {
            if written != former || !self.source[value.span.end()..].starts_with(':') {
                return diagnostic;
            }
            (value.span.end() + 1, format!("{label}: {former}"))
        } else if !written.starts_with(former) {
            return diagnostic;
        } else if written == format!("{former}: {label}") {
            (value.span.end(), format!("{label}:"))
        } else {
            (start + former.len(), label.to_owned())
        };
        self.with_fix(
            diagnostic,
            Span::new(argument.source_id, start, end),
            "write the current label",
            text,
        )
    }

    fn spread_former_label(&self, former: &str, spread: Span) -> Diagnostic {
        let label = self.label;
        Diagnostic::warning(format!(
            "`{former}` is the former label of an argument of `{}`",
            self.method
        ))
        .with_code(DiagnosticCode::LintPreferArgumentLabel)
        .with_label(Label::secondary(
            spread,
            format!("this record supplies it as the field `{former}`"),
        ))
        .with_note(format!(
            "name the field `{label}` where the record is built; the argument is now labeled `{label}:`"
        ))
    }

    /// Attaches the fix unless it would push a line that fits past the width
    /// at which the formatter breaks the call over several lines.
    fn with_fix(&self, diagnostic: Diagnostic, span: Span, title: &str, text: String) -> Diagnostic {
        if fits_line(self.source, span, &text) {
            diagnostic.with_fix_hint(FixHint::replacement(span, title, text))
        } else {
            diagnostic.with_note(
                "with the label the call no longer fits its line; write the label and break the call",
            )
        }
    }
}

/// Where an argument starts: after the delimiter that follows `boundary.0`
/// (the call's `(` or the `,` of the argument before), past the white space.
/// The value's own parentheses, which its span leaves out, stay inside the
/// argument. Anything else on the way, a comment, gives `None`.
fn argument_start(source: &str, boundary: (usize, char), value_start: usize) -> Option<usize> {
    let (from, delimiter) = boundary;
    let lead = source.get(from..value_start)?;
    let after = lead.find(delimiter)? + delimiter.len_utf8();
    let rest = &lead[after..];
    // Parentheses that close the argument before come ahead of its comma.
    if !lead[..after - delimiter.len_utf8()]
        .chars()
        .all(|ch| ch == ')' || ch.is_whitespace())
    {
        return None;
    }
    let argument = rest.trim_start();
    argument
        .chars()
        .all(|ch| ch == '(' || ch.is_whitespace())
        .then(|| from + after + (rest.len() - argument.len()))
}

/// Whether the line an edit starts on still fits the formatter's width after
/// it, or did not fit before.
fn fits_line(source: &str, span: Span, replacement: &str) -> bool {
    let line_start = source[..span.start()]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    let line_end = source[span.end()..]
        .find('\n')
        .map_or(source.len(), |offset| span.end() + offset);
    let before = source[line_start..line_end].chars().count();
    let removed = source[span.range()].chars().count();
    let after = before - removed + replacement.chars().count();
    let width = super::super::format::DEFAULT_LINE_WIDTH;
    after <= width || before > width
}

/// `fs.symlink(target, link)`, whose operands are in the order of `ln -s` and
/// are both paths, so a swapped pair checks.
fn symlink_function(
    arena: &AstArena,
    source: &str,
    call: Span,
    args: &[ArenaCallArg],
) -> Option<Diagnostic> {
    let mut operands = [None, None];
    for (position, arg) in args.iter().enumerate() {
        let (slot, value) = match arg.kind {
            ArenaCallArgKind::Positional(value) => (position, value),
            ArenaCallArgKind::Named { name, value, .. } if name == "target" => (0, value),
            ArenaCallArgKind::Named { name, value, .. } if name == "path" => (1, value),
            _ => return None,
        };
        let operand = operands.get_mut(slot)?;
        if operand.replace(value).is_some() {
            return None;
        }
    }
    let [Some(target), Some(link)] = operands else {
        return None;
    };
    let diagnostic = Diagnostic::warning("`fs.symlink` takes the target first and the link second")
        .with_code(DiagnosticCode::LintPreferArgumentLabel)
        .with_label(Label::secondary(
            call,
            "`LINK.symlink(to: TARGET)` says which is which",
        ));
    if !reorder_is_unobservable(arena, target, link) {
        return Some(diagnostic.with_note(
            "the method evaluates the link before the target; bind an operand that has an effect to a name first",
        ));
    }
    let written = source.get(call.range())?;
    if written.contains(['#', '\n']) {
        return Some(diagnostic.with_note(
            "the call spans lines or holds a comment; rewrite it as `LINK.symlink(to: TARGET)` by hand",
        ));
    }
    let Some(receiver) = super::lint_path_kind::call_receiver_text(arena, source, link) else {
        return Some(diagnostic.with_note(
            "the link has no spelling a method can follow; bind it to a name first",
        ));
    };
    let target = arena.expr(target);
    let argument = if matches!(target.kind, ArenaExprKind::Ident(name) if name == "to") {
        "to:".to_owned()
    } else {
        format!("to: {}", source.get(target.span.range())?)
    };
    let replacement = format!("{receiver}.symlink({argument})");
    Some(if fits_line(source, call, &replacement) {
        diagnostic.with_fix_hint(FixHint::replacement(
            call,
            "call the method on the link",
            replacement,
        ))
    } else {
        diagnostic.with_note(
            "the method call no longer fits the line; rewrite it as `LINK.symlink(to: TARGET)` and break the call",
        )
    })
}

/// Whether evaluating `second` before `first` gives what evaluating `first`
/// before `second` gives: one of them is a literal, or neither does anything
/// but read.
fn reorder_is_unobservable(arena: &AstArena, first: ExprId, second: ExprId) -> bool {
    is_literal(arena, first)
        || is_literal(arena, second)
        || (only_reads(arena, first) && only_reads(arena, second))
}

fn is_literal(arena: &AstArena, expr: ExprId) -> bool {
    matches!(
        arena.expr(expr).kind,
        ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::PathStr(_)
    )
}

/// A name, a literal, a field path of one, or text interpolating only those:
/// evaluating it runs nothing and fails nowhere.
fn only_reads(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_) | ArenaExprKind::Item => true,
        ArenaExprKind::Field { base, .. } => only_reads(arena, base),
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            arena.fmt_parts(parts).all(|part| match part {
                ArenaFmtPart::Text(_) => true,
                ArenaFmtPart::Expr(value, _) => only_reads(arena, value),
            })
        }
        _ => is_literal(arena, expr),
    }
}

#[cfg(test)]
mod tests {
    use super::super::lint_redundant_propagation::tests::lint_rule;
    use super::{LabelRule, MethodReceiver, api_spec, labeled_receiver, method_params};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Type;

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintPreferArgumentLabel)
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

    fn fixed(source: &str) -> String {
        let diagnostics = lint(source);
        let fixed = apply(&diagnostics, source);
        assert!(lint(&fixed).is_empty(), "{fixed}");
        fixed
    }

    #[test]
    fn a_positional_argument_gets_its_label() {
        let source = "proc stage(src: Path, dest: Path, text: Str, pattern: Regex) [fs, error] {\n  src.copy(dest)\n  src.copy(dest, overwrite: true)\n  src.rename(dest)\n  src.hardlink(dest)\n  print (text.replace(\"a\", \"b\"))\n  print (pattern.replace(text, \"-\"))\n}\n";
        assert_eq!(lint(source).len(), 6);
        assert_eq!(
            fixed(source),
            "proc stage(src: Path, dest: Path, text: Str, pattern: Regex) [fs, error] {\n  src.copy(to: dest)\n  src.copy(to: dest, overwrite: true)\n  src.rename(to: dest)\n  src.hardlink(at: dest)\n  print (text.replace(\"a\", with: \"b\"))\n  print (pattern.replace(text, with: \"-\"))\n}\n"
        );
    }

    #[test]
    fn a_former_label_becomes_the_current_one() {
        let source = "proc stage(src: Path, dest: Path, to: Path, text: Str?) [fs, error] {\n  src.copy(dest: dest)\n  src.copy(overwrite: true, dest: to)\n  src.copy(dest:)\n  src.rename(to)\n  print (text?.replace(from: \"a\", to: \"b\") ?? \"\")\n  print (\"abc\".replace(to: \"b\", from: \"a\"))\n}\n";
        assert_eq!(lint(source).len(), 6);
        assert_eq!(
            fixed(source),
            "proc stage(src: Path, dest: Path, to: Path, text: Str?) [fs, error] {\n  src.copy(to: dest)\n  src.copy(overwrite: true, to:)\n  src.copy(to: dest)\n  src.rename(to:)\n  print (text?.replace(from: \"a\", with: \"b\") ?? \"\")\n  print (\"abc\".replace(with: \"b\", from: \"a\"))\n}\n"
        );
    }

    // The label goes in front of the argument wherever the argument stands.
    #[test]
    fn the_label_is_written_where_the_argument_starts() {
        let source = "proc stage(src: Path, dest: Path, text: Str) [fs, error] {\n  src.copy(\n    dest,\n    overwrite: true,\n  )\n  let replaced = text.replace(\n    \"a\" + \"b\",\n    \"c\" + \"d\",\n  )\n  print $replaced\n}\n";
        assert_eq!(
            fixed(source),
            "proc stage(src: Path, dest: Path, text: Str) [fs, error] {\n  src.copy(\n    to: dest,\n    overwrite: true,\n  )\n  let replaced = text.replace(\n    \"a\" + \"b\",\n    with: \"c\" + \"d\",\n  )\n  print $replaced\n}\n"
        );
    }

    #[test]
    fn a_label_already_written_and_another_receiver_are_left_alone() {
        let source = "proc stage(src: Path, dest: Path, text: Str, root: FsRoot) [fs, error] {\n  src.copy(to: dest)\n  src.hardlink(at: dest)\n  dest.symlink(to: src)\n  print (text.replace(\"a\", with: \"b\"))\n  print (text.translate(\"a\", \"b\"))\n  root.symlink(src, \"link\")\n  root.close()\n}\n";
        assert_eq!(lint(source), []);
    }

    #[test]
    fn a_comment_before_the_argument_and_a_spread_record_have_no_fix() {
        let source = "proc stage(text: Str) [error] {\n  let commented = text.replace(\n    \"a\", # the needle\n    \"b\",\n  )\n  let operands = {from: \"a\", to: \"b\"}\n  print (commented + text.replace(...operands))\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        for diagnostic in &diagnostics {
            assert!(diagnostic.fix_hints.is_empty(), "{diagnostic:?}");
            assert_eq!(diagnostic.notes.len(), 1, "{diagnostic:?}");
        }
        assert_eq!(
            &source[diagnostics[1].labels[0].span.range()],
            "operands"
        );
    }

    #[test]
    fn a_label_that_overflows_the_line_is_reported_without_a_fix() {
        let needle = "n".repeat(83);
        let fits = format!("proc stage(text: Str) [error] {{\n  print (text.replace(\"{needle}\", \"b\"))\n}}\n");
        assert_eq!(lint(&fits)[0].fix_hints.len(), 1);
        let needle = "n".repeat(84);
        let overflows = format!("proc stage(text: Str) [error] {{\n  print (text.replace(\"{needle}\", \"b\"))\n}}\n");
        let diagnostics = lint(&overflows);
        assert!(diagnostics[0].fix_hints.is_empty(), "{diagnostics:?}");
        assert!(diagnostics[0].notes[0].contains("no longer fits"));
    }

    #[test]
    fn fs_symlink_becomes_the_method_where_the_order_cannot_be_observed() {
        let source = "proc stage(root: Path, target: Path, to: Path) [fs, error] {\n  let entry = {link: root}\n  fs.symlink(target, fp\"{root}/link\")\n  fs.symlink(\"../shared\", entry.link)\n  fs.symlink(to, root)\n  fs.symlink(path: root, target: target)\n  fs.symlink(root.parent(), \"literal-link\")\n  fs.symlink(/etc/hosts, root)\n}\n";
        assert_eq!(lint(source).len(), 6);
        assert_eq!(
            fixed(source),
            "proc stage(root: Path, target: Path, to: Path) [fs, error] {\n  let entry = {link: root}\n  fp\"{root}/link\".symlink(to: target)\n  entry.link.symlink(to: \"../shared\")\n  root.symlink(to:)\n  root.symlink(to: target)\n  p\"literal-link\".symlink(to: root.parent())\n  root.symlink(to: /etc/hosts)\n}\n"
        );
    }

    #[test]
    fn fs_symlink_with_an_operand_that_runs_something_has_no_fix() {
        let source = "proc stage(root: Path, target: Path) [fs, error] {\n  fs.symlink(target.resolve()?, root.parent())\n  fs.symlink(\n    target,\n    root,\n  )\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty(), "{diagnostics:?}");
        assert!(diagnostics[0].notes[0].contains("evaluates the link before the target"));
        assert!(diagnostics[1].fix_hints.is_empty(), "{diagnostics:?}");
    }

    // The lint reaches a labeled parameter through `labeled_receiver` and
    // `method_params`; a label rule declared where they do not look would
    // never be migrated.
    #[test]
    fn every_labeled_parameter_is_reachable() {
        let labeled = |params: &[super::ParamSig]| {
            params
                .iter()
                .any(|param| matches!(param.label, LabelRule::Written { .. }))
        };
        for module in &api_spec().modules {
            for function in &module.sig.functions {
                for overload in &function.overloads {
                    assert!(
                        !labeled(&overload.params),
                        "{}.{} labels a parameter; the lint reads methods only",
                        module.name,
                        function.name
                    );
                }
            }
        }
        let reachable = [Type::Str, Type::Path, Type::Regex]
            .iter()
            .filter_map(|ty| labeled_receiver(ty, false))
            .collect::<Vec<MethodReceiver>>();
        for entry in &api_spec().methods {
            for method in &entry.methods {
                if !method.overloads.iter().any(|overload| labeled(&overload.sig.params)) {
                    continue;
                }
                assert!(
                    reachable.contains(&entry.receiver)
                        && method_params(entry.receiver, method.name).is_some(),
                    "{:?}.{} labels a parameter the lint cannot reach",
                    entry.receiver,
                    method.name
                );
            }
        }
    }
}
