//! `lint.prefer-fail`: an error family with one variant that carries only a
//! message, declared so a function has something to return, is what
//! `fail MESSAGE` replaces.
//!
//! A family that code matches on stays. The lint proves that nothing in the
//! file does by counting: every place the source spells the family's name
//! must be its declaration or a constructor call this module rewrites, and
//! the variant must never be written in its leading-dot form, which names no
//! family. A pattern, a type annotation, a string, or a comment that mentions
//! the family therefore leaves it alone. An exported family can be matched in
//! a file this lint does not see, so it is reported without a fix.
//!
//! The migration is two edits. Each constructor becomes `fail MESSAGE` where
//! it is the whole value of a `return Err(...)`, and `error.failure(MESSAGE)`
//! anywhere else; once none is left, the declaration is deleted. Callers see
//! the same `Err` with the same message. What changes is the name an
//! uncaught failure is reported under: `validation` instead of the family and
//! variant.

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
    message: ExprId,
}

/// `Err(VALUE)` with nothing but its one positional argument.
struct ErrCall {
    call: ExprId,
    value: ExprId,
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
        if self.families.is_empty() {
            return;
        }
        let stmt = arena.stmt(statement);
        if let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) = stmt.kind {
            self.returns.push(Return {
                start: stmt.span.start(),
                value,
            });
        }
    }

    pub(super) fn visit_expr(&mut self, arena: &AstArena, expr: ExprId) {
        if self.families.is_empty() {
            return;
        }
        let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
            return;
        };
        let [argument] = arena.call_args(args) else {
            return;
        };
        match arena.expr(callee).kind {
            ArenaExprKind::Ident(name) if name == "Err" => {
                if let ArenaCallArgKind::Positional(value) = argument.kind {
                    self.err_calls.push(ErrCall { call: expr, value });
                }
            }
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
                let message = match argument.kind {
                    ArenaCallArgKind::Positional(value) => value,
                    ArenaCallArgKind::Named { name, value, .. } if name == "message" => value,
                    _ => return,
                };
                self.constructors.push(Constructor {
                    family: index,
                    call: expr,
                    message,
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
                if let Some(lines) = declaration_lines(source, family.declaration) {
                    diagnostic = diagnostic
                        .with_fix_hint(FixHint::deletion(lines, "delete the unused error family"));
                }
                diagnostics.push(diagnostic);
                continue;
            }
            for constructor in constructors {
                let call = arena.expr(constructor.call).span;
                let mut diagnostic = Diagnostic::warning(format!(
                    "`{qualified}` only carries a message; report it with `fail`"
                ))
                .with_code(DiagnosticCode::LintPreferFail)
                .with_label(Label::secondary(
                    call,
                    format!("nothing in this file matches on `{name}`"),
                ))
                .with_note(format!(
                    "an uncaught failure is then reported as `validation` instead of `{qualified}`"
                ));
                if let Some(fix) = self.rewrite(arena, source, constructor) {
                    diagnostic = diagnostic.with_fix_hint(fix);
                }
                diagnostics.push(diagnostic);
            }
        }
        diagnostics.sort_by_key(|diagnostic| diagnostic.labels[0].span.start());
        diagnostics
    }

    /// The edit that replaces one constructor: the whole `return Err(...)`
    /// that holds it by `fail MESSAGE`, or the constructor alone by
    /// `error.failure(MESSAGE)`.
    fn rewrite(&self, arena: &AstArena, source: &str, constructor: &Constructor) -> Option<FixHint> {
        let call = arena.expr(constructor.call).span;
        let message = arena.expr(constructor.message).span;
        // A pun (`message:`) is the name with its colon.
        let text = source.get(message.range())?.trim_end_matches(':').trim();
        // A comment between the pieces would be dropped with them.
        if text.is_empty()
            || source.get(call.start()..message.start())?.contains('#')
            || source.get(message.end()..call.end())?.contains('#')
        {
            return None;
        }
        let returned = self
            .err_calls
            .iter()
            .find(|err| err.value == constructor.call)
            .and_then(|err| {
                let statement = self.returns.iter().find(|ret| ret.value == err.call)?;
                Some(Span::new(
                    call.source_id,
                    statement.start,
                    arena.expr(err.call).span.end(),
                ))
            })
            .filter(|statement| {
                source
                    .get(statement.start()..call.start())
                    .is_some_and(|text| !text.contains('#'))
                    && source
                        .get(call.end()..statement.end())
                        .is_some_and(|text| !text.contains('#'))
                    && is_fail_statement(text)
            });
        Some(match returned {
            Some(statement) => FixHint::replacement(
                statement,
                "return the failure with `fail`",
                format!("fail {text}"),
            ),
            None => FixHint::replacement(
                call,
                "build the error with `error.failure`",
                format!("error.failure({text})"),
            ),
        })
    }
}

/// Whether `fail MESSAGE` with this message text is a `fail` statement, alone
/// and before a postfix guard. A message that reads differently after the
/// word `fail` than it did as an argument keeps its `Err(...)`.
fn is_fail_statement(message: &str) -> bool {
    [
        format!("fail {message}\n"),
        format!("fail {message} when true\n"),
    ]
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
    words(source, word).filter(move |start| start.checked_sub(1).map(|index| bytes[index]) == Some(b'.'))
}

/// The lines of a declaration that stands alone on them, with the blank line
/// after it when one also stands before it (or the file starts with it), so
/// the deletion leaves the spacing the formatter writes. A declaration with a
/// comment above it or beside it is left for the author to delete.
fn declaration_lines(source: &str, declaration: Span) -> Option<Span> {
    let text = source.get(declaration.range())?.trim_end();
    let line_start = source[..declaration.start()].rfind('\n').map_or(0, |at| at + 1);
    let text_end = declaration.start() + text.len();
    let line_end = source[text_end..]
        .find('\n')
        .map_or(source.len(), |at| text_end + at + 1);
    if !source[line_start..declaration.start()].trim().is_empty()
        || !source[text_end..line_end].trim().is_empty()
        || text.contains('#')
    {
        return None;
    }
    let before = source[..line_start].trim_end_matches(' ');
    let previous_line = before
        .strip_suffix('\n')
        .map(|rest| rest.rsplit('\n').next().unwrap_or(rest).trim());
    if previous_line.is_some_and(|line| line.starts_with('#')) {
        return None;
    }
    let after = &source[line_end..];
    let blank_after = after.starts_with('\n');
    let end = if blank_after && previous_line.is_none_or(str::is_empty) {
        line_end + 1
    } else {
        line_end
    };
    Some(Span::new(declaration.source_id, line_start, end))
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
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

    #[test]
    fn a_message_only_family_becomes_fail_and_then_goes() {
        let source = "error LoadError = Failed(message: Str)\n\nproc load(name: Str, ready: Bool) -> Result[Int] {\n  return Err(LoadError.Failed(\"not ready\")) unless ready\n  if name == \"\" {\n    return Err(LoadError.Failed(message: f\"no {name}\"))\n  }\n  let fallback = Err(LoadError.Failed(\"no fallback\"))\n  Ok(1) ?? fallback?\n}\n";
        let first = lint(source);
        assert_eq!(first.len(), 3, "{first:?}");
        let rewritten = apply(&first, source);
        assert_eq!(
            rewritten,
            "error LoadError = Failed(message: Str)\n\nproc load(name: Str, ready: Bool) -> Result[Int] {\n  fail \"not ready\" unless ready\n  if name == \"\" {\n    fail f\"no {name}\"\n  }\n  let fallback = Err(error.failure(\"no fallback\"))\n  Ok(1) ?? fallback?\n}\n"
        );
        // With no constructor left, the declaration is the one report.
        let second = lint(&rewritten);
        assert_eq!(second.len(), 1, "{second:?}");
        assert!(second[0].message.contains("never constructed"), "{second:?}");
        let fixed = apply(&second, &rewritten);
        assert_eq!(
            fixed,
            "proc load(name: Str, ready: Bool) -> Result[Int] {\n  fail \"not ready\" unless ready\n  if name == \"\" {\n    fail f\"no {name}\"\n  }\n  let fallback = Err(error.failure(\"no fallback\"))\n  Ok(1) ?? fallback?\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    #[test]
    fn a_family_that_code_names_any_other_way_is_left_alone() {
        for source in [
            // Matched on.
            "error LoadError = Failed(message: Str)\n\nproc load() -> Result[Int, LoadError] {\n  return Err(LoadError.Failed(\"no\"))\n}\n\nmatch load() {\n  Err(LoadError.Failed {message}) => print $message\n  Ok(_) => print \"ok\"\n}\n",
            // Named by a return type, and built in the leading-dot form.
            "error LoadError = Failed\n\nproc load() -> Result[Int, LoadError] {\n  return Err(.Failed(\"no\"))\n}\n",
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
        assert!(diagnostics[0].notes.iter().any(|note| note.contains("is exported")));
    }

    #[test]
    fn a_comment_keeps_the_report_but_not_the_edit_that_would_drop_it() {
        let source = "error LoadError = Failed(message: Str)\n\nproc load() -> Result[Int] {\n  return Err(LoadError.Failed( # why\n    \"no\",\n  ))\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());

        let documented = "# Raised by nothing.\nerror LoadError = Failed(message: Str)\n\nprint \"ok\"\n";
        let diagnostics = lint(documented);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
    }

    #[test]
    fn a_cause_keeps_its_err_and_only_the_error_changes() {
        let source = "error LoadError = Failed(message: Str)\n\nproc load(inner: Result[Int]) -> Result[Int] {\n  match inner {\n    Ok(value) => Ok(value)\n    Err(error) => return Err(LoadError.Failed(\"outer\"), cause: error)\n  }\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert!(
            fixed.contains("Err(error) => return Err(error.failure(\"outer\"), cause: error)\n"),
            "{fixed}"
        );
        // The fixed program still checks: the call names the module even
        // where `error` is the caught error.
        assert_eq!(lint(&fixed).len(), 1);
    }
}
