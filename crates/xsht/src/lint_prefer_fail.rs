//! `lint.prefer-fail`: a message-only error family may be simplified manually
//! to `fail MESSAGE` when its nominal identity is not part of the contract.
//!
//! A family that code matches on stays. The lint proves that nothing in the
//! file does by counting: every place the source spells the family's name
//! must be its declaration or a constructor call this module recognizes, and
//! the variant must never be written in its leading-dot form, which names no
//! family. A pattern, a type annotation, a string, or a comment that mentions
//! the family therefore leaves it alone. An exported family can be matched in
//! a file this lint does not see, so it is reported without a fix.
//!
//! Replacing a nominal constructor with a string failure changes the family
//! reported by an uncaught failure, even when callers only inspect its message.
//! Such findings explain the manual choice and never offer a replacement.
//! An unused declaration can be deleted independently of constructor rewrites;
//! comments inside or beside it require deletion by hand.
//!
//! The lint also respells `return Err(.Variant(...))`, with or without a
//! `cause:`, as the `fail .Variant(...)` that is defined to mean it.

use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaProgram, ArenaStmtKind, AstArena, ExprId,
    StmtId, SugarForm,
};
use xsh::frontend::syntax::parser::Parser;

/// A family declared as one variant that carries only its message.
struct Family {
    name: Name,
    variant: Name,
    /// The declaration statement, with its `export` when it has one.
    declaration: Span,
    exported: bool,
}

/// `Family.Variant(MESSAGE)`, as the linter's traversal reached it.
struct Constructor {
    family: usize,
    call: ExprId,
}

/// `Err(VALUE)` or `Err(VALUE, cause: CAUSE)`.
struct ErrCall {
    call: ExprId,
    value: ExprId,
    cause: Option<ExprId>,
}

/// `return VALUE`, by where the statement starts.
struct Return {
    start: usize,
    value: ExprId,
}

/// The message-only families of one file and the places that build them.
#[derive(Default)]
pub(super) struct Candidates {
    families: Vec<Family>,
    constructors: Vec<Constructor>,
    err_calls: Vec<ErrCall>,
    returns: Vec<Return>,
}

impl Candidates {
    pub(super) fn collect(program: &ArenaProgram, source: &str) -> Self {
        let mut candidates = Self::default();
        // Every family is declared with the word `error`.
        if !source.contains("error ") {
            return candidates;
        }
        let arena = &program.arena;
        for statement in program.statement_ids() {
            let outer = arena.stmt(statement);
            let (inner, exported) = match outer.kind {
                ArenaStmtKind::Export(inner) => (arena.stmt(inner), true),
                _ => (outer.clone(), false),
            };
            let ArenaStmtKind::ErrorDef(id) = inner.kind else {
                continue;
            };
            let family = arena.error_def(id);
            let [variant] = arena.error_variants(family.variants) else {
                continue;
            };
            // A facet is something callers can test without naming the family.
            if variant.facets.len != 0 {
                continue;
            }
            let message_only = match arena.error_fields(variant.fields) {
                [] => true,
                [field] => {
                    field.name == "message"
                        && source.get(arena.type_expr_span(field.ty).range()) == Some("Str")
                }
                _ => false,
            };
            if !message_only {
                continue;
            }
            candidates.families.push(Family {
                name: family.name,
                variant: variant.name,
                declaration: outer.span,
                exported,
            });
        }
        candidates
    }

    pub(super) fn visit_stmt(&mut self, arena: &AstArena, statement: StmtId) {
        let stmt = arena.stmt(statement);
        if let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) = stmt.kind
            && is_err_call(arena, value)
        {
            self.returns.push(Return {
                start: stmt.span.start(),
                value,
            });
        }
    }

    pub(super) fn visit_expr(&mut self, arena: &AstArena, expr: ExprId) {
        let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
            return;
        };
        if is_err_call(arena, expr) {
            let (value, cause) = match arena.call_args(args) {
                [value] => (value, None),
                [value, cause] => match cause.kind {
                    ArenaCallArgKind::Named {
                        name, value: cause, ..
                    } if name == "cause" => (value, Some(cause)),
                    _ => return,
                },
                _ => return,
            };
            let ArenaCallArgKind::Positional(value) = value.kind else {
                return;
            };
            // Without a candidate family, only a leading-dot error matters.
            if !self.families.is_empty() || is_leading_dot_variant(arena, value) {
                self.err_calls.push(ErrCall {
                    call: expr,
                    value,
                    cause,
                });
            }
            return;
        }
        if self.families.is_empty() {
            return;
        }
        let [argument] = arena.call_args(args) else {
            return;
        };
        match arena.expr(callee).kind {
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(family) = arena.expr(base).kind else {
                    return;
                };
                let Some(index) = self
                    .families
                    .iter()
                    .position(|candidate| candidate.name == family && candidate.variant == name)
                else {
                    return;
                };
                if !matches!(argument.kind, ArenaCallArgKind::Positional(_))
                    && !matches!(argument.kind, ArenaCallArgKind::Named { name, .. } if name == "message")
                {
                    return;
                }
                self.constructors.push(Constructor {
                    family: index,
                    call: expr,
                });
            }
            _ => {}
        }
    }

    pub(super) fn finish(self, arena: &AstArena, source: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (index, family) in self.families.iter().enumerate() {
            let name = family.name.as_str();
            let variant = family.variant.as_str();
            if family.exported {
                diagnostics.push(
                    Diagnostic::warning(format!("error family `{name}` only carries a message"))
                        .with_code(DiagnosticCode::LintPreferFail)
                        .with_label(Label::secondary(
                            family.declaration,
                            "a failure that callers only report needs no family",
                        ))
                        .with_note(format!(
                            "`{name}` is exported: if no file that imports it matches on `{name}`, return its failures with `fail MESSAGE` and delete the declaration"
                        )),
                );
                continue;
            }
            let constructors = self
                .constructors
                .iter()
                .filter(|constructor| constructor.family == index)
                .collect::<Vec<_>>();
            let qualified = format!("{name}.{variant}");
            if words(source, &name).count() != constructors.len() + 1
                || words(source, &qualified).count() != constructors.len()
                || dotted_words(source, &variant).count() != constructors.len()
            {
                continue;
            }
            if constructors.is_empty() {
                let mut diagnostic =
                    Diagnostic::warning(format!("error family `{name}` is never constructed"))
                        .with_code(DiagnosticCode::LintPreferFail)
                        .with_label(Label::secondary(
                            family.declaration,
                            "nothing in this file names it; delete the declaration",
                        ));
                diagnostic = match declaration_lines(source, family.declaration) {
                    Ok(lines) => diagnostic
                        .with_fix_hint(FixHint::deletion(lines, "delete the unused error family")),
                    // The report then says what the author has to do, since
                    // `--fix` leaves the declaration behind.
                    Err(kept) => diagnostic.with_note(kept.hand_edit()),
                };
                diagnostics.push(diagnostic);
                continue;
            }
            for constructor in constructors {
                let call = arena.expr(constructor.call).span;
                let diagnostic = Diagnostic::warning(format!(
                    "`{qualified}` only carries a message; report it with `fail`"
                ))
                .with_code(DiagnosticCode::LintPreferFail)
                .with_label(Label::secondary(
                    call,
                    format!("nothing in this file matches on `{name}`"),
                ))
                .with_note(format!(
                    "rewrite this failure by hand only if changing its nominal error family is intended; `fail MESSAGE` reports `validation` instead of `{qualified}`"
                ));
                diagnostics.push(diagnostic);
            }
        }
        for err in &self.err_calls {
            if !is_leading_dot_variant(arena, err.value) {
                continue;
            }
            let Some(statement) = self.returned(arena, err) else {
                continue;
            };
            let mut diagnostic =
                Diagnostic::warning("return an error written `.Variant(...)` with `fail`")
                    .with_code(DiagnosticCode::LintPreferFail)
                    .with_label(Label::secondary(
                        statement,
                        "`fail .Variant(...)` means this `return Err(...)`",
                    ));
            if let Some(replacement) = self.fail_text(arena, source, statement, err, err.value) {
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    statement,
                    "return the error with `fail`",
                    replacement,
                ));
            }
            diagnostics.push(diagnostic);
        }
        diagnostics.sort_by_key(|diagnostic| diagnostic.labels[0].span.start());
        diagnostics
    }

    /// The `return Err(...)` statement, up to the end of the `Err(...)`, that
    /// returns exactly this call.
    fn returned(&self, arena: &AstArena, err: &ErrCall) -> Option<Span> {
        let statement = self.returns.iter().find(|ret| ret.value == err.call)?;
        let call = arena.expr(err.call).span;
        Some(Span::new(call.source_id, statement.start, call.end()))
    }

    /// `fail FAILURE` or `fail FAILURE because CAUSE` for a returned `Err`,
    /// where `failure` is the part of its first argument that `fail` takes.
    /// There is none when the statement holds a comment the rewrite would
    /// drop, or when the text reads differently after the word `fail`.
    fn fail_text(
        &self,
        arena: &AstArena,
        source: &str,
        statement: Span,
        err: &ErrCall,
        failure: ExprId,
    ) -> Option<String> {
        let failure = arena.expr(failure).span;
        let text = written(source, failure)?;
        let mut kept_end = failure.end();
        let mut replacement = format!("fail {text}");
        if source
            .get(statement.start()..failure.start())?
            .contains('#')
        {
            return None;
        }
        if let Some(cause) = err.cause {
            let cause = arena.expr(cause).span;
            if source.get(kept_end..cause.start())?.contains('#') {
                return None;
            }
            replacement.push_str(" because ");
            replacement.push_str(written(source, cause)?);
            kept_end = cause.end();
        }
        (!source.get(kept_end..statement.end())?.contains('#') && is_fail_statement(&replacement))
            .then_some(replacement)
    }
}

fn is_err_call(arena: &AstArena, expr: ExprId) -> bool {
    matches!(
        arena.expr(expr).kind,
        ArenaExprKind::Call { callee, .. }
            if matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Err")
    )
}

/// Whether the expression is `.Name` or `.Name(...)`, the shape a `fail`
/// statement returns as it stands.
fn is_leading_dot_variant(arena: &AstArena, expr: ExprId) -> bool {
    let constructor = match arena.expr(expr).kind {
        ArenaExprKind::Call { callee, .. } => callee,
        _ => expr,
    };
    matches!(
        arena.expr(constructor).kind,
        ArenaExprKind::Field { base, .. } if matches!(arena.expr(base).kind, ArenaExprKind::Item)
    )
}

/// The text of an argument's value. A pun (`message:`) is the name with its
/// colon.
fn written(source: &str, value: Span) -> Option<&str> {
    let text = source.get(value.range())?.trim_end_matches(':').trim();
    (!text.is_empty()).then_some(text)
}

/// Whether this text is a `fail` statement, alone and before a postfix guard.
/// Text that reads differently after the word `fail` than it did as an
/// argument keeps its `Err(...)`.
fn is_fail_statement(statement: &str) -> bool {
    [format!("{statement}\n"), format!("{statement} when true\n")]
        .iter()
        .all(|candidate| {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), candidate);
            if !parsed.diagnostics.is_empty() {
                return false;
            }
            let mut statements = parsed.arena.statement_ids();
            let (Some(statement), None) = (statements.next(), statements.next()) else {
                return false;
            };
            let arena = &parsed.arena.arena;
            let fails = |id: StmtId| {
                matches!(
                    arena.stmt(id).kind,
                    ArenaStmtKind::Sugar {
                        form: SugarForm::Fail,
                        ..
                    }
                )
            };
            match arena.stmt(statement).kind {
            ArenaStmtKind::Sugar {
                form: SugarForm::When,
                operands,
                ..
            } => arena.sugar_operands(operands).iter().any(|operand| {
                matches!(
                    operand,
                    xsh::frontend::syntax::arena::ArenaSugarOperand::Stmt(inner) if fails(*inner)
                )
            }),
            _ => fails(statement),
        }
        })
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Where `word` stands in the source as a whole name.
fn words<'a>(source: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    let bytes = source.as_bytes();
    source.match_indices(word).filter_map(move |(start, _)| {
        let end = start + word.len();
        let before = start.checked_sub(1).map(|index| bytes[index]);
        let after = bytes.get(end).copied();
        (!before.is_some_and(is_word_byte) && !after.is_some_and(is_word_byte)).then_some(start)
    })
}

/// Where `.word` stands in the source, qualified or in its leading-dot form.
fn dotted_words<'a>(source: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    let bytes = source.as_bytes();
    words(source, word)
        .filter(move |start| start.checked_sub(1).map(|index| bytes[index]) == Some(b'.'))
}

/// The lines of a declaration that stands alone on them, with the blank line
/// after it when one also stands before it (or the file starts with it), so
/// the deletion leaves the spacing the formatter writes. A comment above the
/// declaration is not part of it and keeps the blank line that follows. A
/// declaration with a comment inside or beside it has no such lines: an edit
/// never removes a comment.
fn declaration_lines(source: &str, declaration: Span) -> Result<Span, KeptDeclaration> {
    let text = source
        .get(declaration.range())
        .ok_or(KeptDeclaration::SharedLine)?
        .trim_end();
    let line_start = source[..declaration.start()]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let text_end = declaration.start() + text.len();
    let line_end = source[text_end..]
        .find('\n')
        .map_or(source.len(), |at| text_end + at + 1);
    if text.contains('#') {
        return Err(KeptDeclaration::CommentInside);
    }
    let after = source[text_end..line_end].trim();
    if after.starts_with('#') {
        return Err(KeptDeclaration::CommentBeside);
    }
    if !source[line_start..declaration.start()].trim().is_empty() || !after.is_empty() {
        return Err(KeptDeclaration::SharedLine);
    }
    let blank_before = source[..line_start]
        .strip_suffix('\n')
        .is_none_or(|before| {
            before
                .rsplit('\n')
                .next()
                .unwrap_or(before)
                .trim()
                .is_empty()
        });
    let end = if source[line_end..].starts_with('\n') && blank_before {
        line_end + 1
    } else {
        line_end
    };
    Ok(Span::new(declaration.source_id, line_start, end))
}

/// Why no edit deletes an unused family's declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeptDeclaration {
    /// A comment is written between its tokens.
    CommentInside,
    /// A comment follows it on its last line.
    CommentBeside,
    /// Other code is written on its lines.
    SharedLine,
}

impl KeptDeclaration {
    /// What the author does instead of `--fix`.
    fn hand_edit(self) -> &'static str {
        match self {
            Self::CommentInside => {
                "`--fix` leaves this declaration: it has a comment inside it, and a fix never removes a comment. Delete the declaration by hand, with the comment if it describes the family"
            }
            Self::CommentBeside => {
                "`--fix` leaves this declaration: it has a comment beside it, and a fix never removes a comment. Delete the declaration by hand, with the comment if it describes the family"
            }
            Self::SharedLine => {
                "`--fix` leaves this declaration: it shares its line with other code. Delete the declaration by hand"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use super::KeptDeclaration;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(&parsed.arena, source, LintOptions::default())
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferFail))
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

    /// Lints and applies fixes until a pass has none, as `xsht lint --fix`
    /// does, and returns the text of each pass that changed it.
    fn passes(source: &str) -> Vec<String> {
        let mut passes: Vec<String> = Vec::new();
        loop {
            let current = passes.last().map_or(source, String::as_str);
            let next = apply(&lint(current), current);
            if next == current {
                return passes;
            }
            passes.push(next);
            assert!(passes.len() < 4, "fixes do not converge: {passes:?}");
        }
    }

    fn converged(source: &str) -> String {
        passes(source).pop().unwrap_or_else(|| source.to_owned())
    }

    #[test]
    fn a_message_only_family_is_reported_without_changing_its_errors() {
        let source = "use system_report as report\n\nerror LoadError = Failed(message: Str)\n\nproc load(name: Str, ready: Bool) -> Result[Int] {\n  return Err(LoadError.Failed(\"not ready\")) unless ready\n  if name == \"\" {\n    return Err(LoadError.Failed(message: f\"no {name}\"))\n  }\n  let fallback = Err(LoadError.Failed(\"no fallback\"))\n  Ok(1) ?? fallback?\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default())
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferFail))
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.fix_hints.is_empty()));
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.notes.iter().any(|note| note.contains("nominal error family"))));
        assert_eq!(apply(&diagnostics, source), source);
    }

    /// A fixer applies the edits of every rule together and drops one that
    /// overlaps an earlier one. Whatever subset survives, a file that
    /// checked still checks.
    #[test]
    fn the_surviving_edits_of_every_rule_leave_a_program_that_checks() {
        let source = "error AppError = Failed(message: Str)\n\nproc validate(argv: List[Str]) [] -> Result[Unit] {\n  if argv.len() > 4 {\n    return Err(AppError.Failed(\"too many\"))\n  }\n\n  if argv.len() > 3 {\n    return Err(AppError.Failed(\"this message is long enough that the one-line guard passes the column cap\"))\n  }\n\n  for arg in argv {\n    if arg == \"\" {\n      return Err(AppError.Failed(\"empty\"))\n    }\n  }\n}\n";
        let mut current = source.to_owned();
        for _ in 0..6 {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &current);
            assert!(
                parsed.diagnostics.is_empty(),
                "{current}\n{:?}",
                parsed.diagnostics
            );
            let checked = Checker::check_arena(&parsed.arena, &current);
            assert!(
                checked.diagnostics.is_empty(),
                "{current}\n{:?}",
                checked.diagnostics
            );
            let diagnostics =
                Linter::lint(&parsed.arena, &current, LintOptions::default()).diagnostics;
            let mut edits = diagnostics
                .iter()
                .flat_map(|diagnostic| &diagnostic.fix_hints)
                .filter_map(|hint| Some((hint.span?, hint.replacement.clone()?)))
                .collect::<Vec<_>>();
            edits.sort_by_key(|(span, _)| (span.start(), std::cmp::Reverse(span.end())));
            let mut kept = Vec::new();
            let mut kept_end = 0;
            for (span, replacement) in edits {
                if span.start() >= kept_end {
                    kept_end = span.end().max(span.start() + 1);
                    kept.push((span, replacement));
                }
            }
            if kept.is_empty() {
                // Other rules may rewrite control flow while the nominal error stays.
                assert!(current.contains("error AppError = Failed(message: Str)"), "{current}");
                assert!(current.contains("AppError.Failed("), "{current}");
                assert!(lint(&current).iter().all(|diagnostic| diagnostic.fix_hints.is_empty()));
                return;
            }
            for (span, replacement) in kept.into_iter().rev() {
                current.replace_range(span.range(), &replacement);
            }
        }
        panic!("fixes do not converge:\n{current}");
    }

    #[test]
    fn a_family_at_the_start_or_beside_another_declaration_leaves_formatted_text() {
        let first = "error LoadError = Failed(message: Str)\n\nproc load() -> Result[Int] {\n  return Err(LoadError.Failed(\"no\"))\n}\n";
        assert_eq!(converged(first), first);
        let grouped = "error LoadError = Failed(message: Str)\nerror Kept = Missing(path: Path) | Busy\n\nproc load() -> Result[Int] {\n  return Err(LoadError.Failed(\"no\"))\n}\n\nprint ${Kept.Busy().message}\n";
        assert_eq!(converged(grouped), grouped);
        let unused = "error LoadError = Failed(message: Str)\n\nprint \"done\"\n";
        assert_eq!(converged(unused), "print \"done\"\n");
        let unused_grouped = "error LoadError = Failed(message: Str)\nerror Kept = Missing(path: Path) | Busy\n\nprint ${Kept.Busy().message}\n";
        assert_eq!(converged(unused_grouped), "error Kept = Missing(path: Path) | Busy\n\nprint ${Kept.Busy().message}\n");
    }

    #[test]
    fn a_family_that_code_names_any_other_way_is_left_alone() {
        for source in [
            // Matched on.
            "error LoadError = Failed(message: Str)\n\nproc load() -> Result[Int, LoadError] {\n  return Err(LoadError.Failed(\"no\"))\n}\n\nmatch load() {\n  Err(LoadError.Failed {message}) => print $message\n  Ok(_) => print \"ok\"\n}\n",
            // Named by a return type, and built in the leading-dot form.
            "error LoadError = Failed\n\nproc load() -> Result[Int, LoadError] {\n  Err(.Failed(\"no\"))\n}\n",
            // Named only in a comment.
            "error LoadError = Failed(message: Str)\n\n# LoadError is what the installer greps for.\nproc load() -> Result[Int] {\n  return Err(LoadError.Failed(\"no\"))\n}\n",
            // More than a message, more than one variant, or a facet.
            "error LoadError = Failed(path: Path)\n\nproc load() -> Result[Int] {\n  return Err(LoadError.Failed(path: p\"/\"))\n}\n",
            "error LoadError = Failed(message: Str) | Missing\n\nproc load() -> Result[Int] {\n  return Err(LoadError.Failed(\"no\"))\n}\n",
            "error LoadError = Failed(message: Str) : NotFound\n\nproc load() -> Result[Int] {\n  return Err(LoadError.Failed(\"no\"))\n}\n",
        ] {
            let diagnostics = lint(source);
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
        }
    }

    #[test]
    fn an_exported_family_is_reported_without_a_fix() {
        let source = "export error LoadError = Failed(message: Str)\n\nexport proc load() -> Result[Int, Error] {\n  return Err(LoadError.Failed(\"no\"))\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
        assert!(
            diagnostics[0]
                .notes
                .iter()
                .any(|note| note.contains("is exported"))
        );
    }

    #[test]
    fn commented_constructors_and_unused_declarations_keep_their_comments() {
        let source = "error LoadError = Failed(message: Str)\n\nproc load(ready: Bool) -> Result[Int] {\n  return Err(LoadError.Failed(\"not ready\")) unless ready\n  return Err(LoadError.Failed( # why\n    \"no\",\n  ))\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.fix_hints.is_empty()));
        assert_eq!(apply(&diagnostics, source), source);

        let beside = "error LoadError = Failed(message: Str) # legacy\n\nproc load() -> Result[Int] {\n  fail \"no\"\n}\n";
        assert_eq!(converged(beside), beside);
        let unused = lint(beside);
        assert_eq!(unused.len(), 1, "{unused:?}");
        assert!(unused[0].fix_hints.is_empty());
        assert_eq!(unused[0].notes, [KeptDeclaration::CommentBeside.hand_edit()]);

        let inside = "error LoadError {\n  # legacy\n  Failed(message: Str)\n}\n\nproc load() -> Result[Int] {\n  fail \"no\"\n}\n";
        let unused = lint(inside);
        assert_eq!(unused.len(), 1, "{unused:?}");
        assert!(unused[0].fix_hints.is_empty());
        assert_eq!(
            unused[0].notes,
            [KeptDeclaration::CommentInside.hand_edit()]
        );

        let shared = "error LoadError = Failed(message: Str); const limit = 3\n\nproc load() -> Result[Int] {\n  fail f\"over {limit}\"\n}\n";
        let unused = lint(shared);
        assert_eq!(unused.len(), 1, "{unused:?}");
        assert!(unused[0].fix_hints.is_empty());
        assert_eq!(unused[0].notes, [KeptDeclaration::SharedLine.hand_edit()]);
    }

    #[test]
    fn a_comment_above_the_declaration_stays_where_it_is() {
        let source = "# Retries a command.\n# Usage: retry COMMAND\nerror RetryError = Failed(message: Str)\n\nproc attempt() -> Result[Int] {\n  return Err(RetryError.Failed(\"no\"))\n}\n";
        assert_eq!(converged(source), source);
        let unused = "# Retries a command.\n# Usage: retry COMMAND\nerror RetryError = Failed(message: Str)\n\nproc attempt() -> Result[Int] {\n  fail \"no\"\n}\n";
        assert_eq!(converged(unused), "# Retries a command.\n# Usage: retry COMMAND\n\nproc attempt() -> Result[Int] {\n  fail \"no\"\n}\n");
    }

    #[test]
    fn nominal_errors_keep_their_causes() {
        let source = "error LoadError = Failed(message: Str)\n\nproc load(inner: Result[Int]) -> Result[Int] {\n  match inner {\n    Ok(value) => Ok(value)\n    Err(error) => return Err(LoadError.Failed(\"outer\"), cause: error)\n  }\n}\n\nproc wrap(inner: Result[Int]) -> Result[Int] {\n  match inner {\n    Ok(value) => Ok(value)\n    Err(error) => Err(LoadError.Failed(\"tail\"), cause: error)\n  }\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.fix_hints.is_empty()));
        assert_eq!(converged(source), source);
    }

    #[test]
    fn a_returned_leading_dot_error_becomes_fail() {
        let source = "error LoadError = Missing(path: Path) | Busy\n\nproc load(target: Path, inner: Result[Int]) -> Result[Int, LoadError] {\n  return Err(.Busy()) when target == p\"/\"\n  match inner {\n    Ok(value) => Ok(value)\n    Err(problem) => return Err(.Missing(path: target), cause: problem)\n  }\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "error LoadError = Missing(path: Path) | Busy\n\nproc load(target: Path, inner: Result[Int]) -> Result[Int, LoadError] {\n  fail .Busy() when target == p\"/\"\n  match inner {\n    Ok(value) => Ok(value)\n    Err(problem) => fail .Missing(path: target) because problem\n  }\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    #[test]
    fn other_returned_errors_keep_their_return() {
        for source in [
            // A qualified constructor, an error that already exists, and an
            // `Err` that is a value and not a `return`.
            "error LoadError = Missing(path: Path) | Busy\n\nproc load(inner: Result[Int, LoadError]) -> Result[Int, LoadError] {\n  match inner {\n    Ok(0) => return Err(LoadError.Busy())\n    Ok(value) => Ok(value)\n    Err(problem) => return Err(problem)\n  }\n}\n\nproc tail() -> Result[Int, LoadError] {\n  Err(.Busy())\n}\n",
        ] {
            let diagnostics = lint(source);
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
        }
    }
}
