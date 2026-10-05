//! `lint.prefer-inferred-proc-return`: drop a private proc's return
//! annotation when the checker infers exactly the type it states.
//!
//! "Exactly" is proved, not estimated. The file is checked again with the
//! annotation removed, through the same loader and module roots as the real
//! check, and the fix is offered only when the second check is clean and every
//! checked fact of the file is unchanged: each expression type, statement
//! position, function return type, parameter type, and callable effect. That
//! one test covers the cases that must keep their annotation without naming
//! them: a recursive proc (inference refuses it), a body that needs the
//! annotation as an expected type (`Err(.Variant(...))`, `.require()`, an
//! empty collection), and an annotation wider or narrower than the body.

use super::{
    ArenaTypeExprKind, CheckedReturnRemovalFacts, Linter, checked_return_type_shape,
    scan_before_arrow, standalone_annotation_imports_available, type_expr_kind,
};
use std::path::PathBuf;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::{CheckOptions, CheckOutput, Checker};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::symbols::SymbolOwner;
use xsh::frontend::syntax::arena::FunctionDefId;
use xsh::frontend::syntax::node::Effect;

/// Where the linted text lives, so a proof check resolves its imports as the
/// real check did. Without one, only a file with no user imports is provable.
#[derive(Clone, Debug)]
pub struct ReturnProofContext {
    pub file: String,
    pub module_roots: Vec<PathBuf>,
}

/// A private proc's return annotation that the traversal found removable in
/// form; the proof decides whether it is removable in fact.
pub(super) struct ProcReturnCandidate {
    /// The annotation's type.
    ty: Span,
    /// ` -> TYPE`, the text a fix deletes.
    deletion: Span,
}

impl Linter<'_> {
    /// Called from the traversal for each proc definition of the linted file.
    pub(super) fn note_proc_return_candidate(&mut self, id: FunctionDefId, exported: bool) {
        let def = self.arena.function_def(id);
        if !self.prefer_inferred_proc_returns
            || exported
            || def.return_ty_defaulted
            || def.test_declaration
            || def.name == "main"
            // A subcommand `cli main` entry is named `main WORD...`.
            || def.name.as_str().starts_with("main ")
        {
            return;
        }
        // `lint.redundant-result-unit` owns the annotation every statement
        // body already means.
        if broad_result_unit(self.arena, def.return_ty) {
            return;
        }
        let ty = self.arena.type_expr_span(def.return_ty);
        let start = scan_before_arrow(self.source, ty.start());
        // A comment inside the annotation would be deleted with it.
        if self
            .source
            .get(start..ty.end())
            .is_none_or(|annotation| annotation.contains('#'))
        {
            return;
        }
        self.proc_return_candidates.push(ProcReturnCandidate {
            ty,
            deletion: Span::new(ty.source_id, start, ty.end()),
        });
    }

    /// Proves the candidates the traversal collected and reports the proved.
    pub(super) fn lint_inferred_proc_returns(&mut self) {
        let candidates = std::mem::take(&mut self.proc_return_candidates);
        let Some(first) = candidates.first() else {
            return;
        };
        let source_id = first.ty.source_id;
        let context = self.return_proof.as_ref();
        let Some(before) = proof_facts(self.source, source_id, &[], context) else {
            return;
        };
        let proves = |removed: &[&ProcReturnCandidate]| {
            let mut rewritten = self.source.to_string();
            let mut ranges = Vec::with_capacity(removed.len());
            for candidate in removed.iter().rev() {
                rewritten.replace_range(candidate.deletion.range(), "");
            }
            for candidate in removed {
                ranges.push((
                    candidate.deletion.start(),
                    candidate.deletion.end() - candidate.deletion.start(),
                ));
            }
            proof_facts(&rewritten, source_id, &ranges, context).as_ref() == Some(&before)
        };
        // An annotation only adds information to the inference of other
        // definitions, so when the file is unchanged with every candidate
        // gone it is unchanged with any one of them gone. That makes the
        // common case two checks per file; a file where some annotation is
        // needed proves each candidate on its own.
        let all = candidates.iter().collect::<Vec<_>>();
        let proved = if proves(&all) {
            all
        } else if candidates.len() == 1 {
            Vec::new()
        } else {
            candidates
                .iter()
                .filter(|candidate| proves(&[candidate]))
                .collect()
        };
        for candidate in proved {
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "private proc return type is inferred exactly",
                )
                .with_code(DiagnosticCode::LintPreferInferredProcReturn)
                .with_label(Label::secondary(
                    candidate.ty,
                    "every checked type in this file stays the same without this annotation",
                ))
                .with_fix_hint(FixHint::deletion(
                    candidate.deletion,
                    "infer the private proc return",
                )),
            );
        }
    }
}

/// Everything a proof compares between the file and the file without an
/// annotation.
#[derive(Eq, PartialEq)]
struct ProofFacts {
    checked: CheckedReturnRemovalFacts,
    /// Each warning's place, code, and message.
    warnings: Vec<(Option<(usize, usize)>, Option<&'static str>, String)>,
}

/// `Result[Unit]` with no error type, or with the `Error` it defaults to.
pub(super) fn broad_result_unit(
    arena: &xsh::frontend::syntax::arena::AstArena,
    ty: xsh::frontend::syntax::arena::TypeExprId,
) -> bool {
    let ArenaTypeExprKind::Result { ok, err } = type_expr_kind(arena, ty) else {
        return false;
    };
    matches!(type_expr_kind(arena, ok), ArenaTypeExprKind::Named(name) if name == "Unit")
        && err.is_none_or(|err| {
            matches!(type_expr_kind(arena, err), ArenaTypeExprKind::Named(name) if name == "Error")
        })
}

/// The checked facts of `source`, or `None` when it does not check clean.
/// `removed` lists the `(start, length)` ranges, in the original text and in
/// order, that `source` lacks; facts are reported at original offsets so two
/// versions compare directly.
fn proof_facts(
    source: &str,
    source_id: SourceId,
    removed: &[(usize, usize)],
    context: Option<&ReturnProofContext>,
) -> Option<ProofFacts> {
    #[cfg(test)]
    tests::record_proof();
    let (checked, entry_source, symbols): (CheckOutput, SourceId, SymbolOwner) = match context {
        Some(context) => {
            let entry = xsh::frontend::load::parse_load_check_text(
                &context.file,
                source.to_string(),
                context.module_roots.clone(),
                CheckOptions::default(),
            );
            if !entry.parsed.diagnostics.is_empty() {
                return None;
            }
            let symbols = entry.parsed.arena.symbol_owner().clone();
            (entry.checked?, entry.entry_source_id, symbols)
        }
        None => {
            let parsed =
                xsh::frontend::syntax::parser::Parser::parse_source_arena_only(source_id, source);
            if !parsed.diagnostics.is_empty()
                || !standalone_annotation_imports_available(&parsed.arena)
            {
                return None;
            }
            let symbols = parsed.arena.symbol_owner().clone();
            (
                Checker::check_arena(&parsed.arena, source),
                source_id,
                symbols,
            )
        }
    };
    // A warning the file already has does not disqualify it; the warnings
    // themselves must then be the same before and after.
    if checked
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return None;
    }
    let original = |offset: usize| {
        removed.iter().fold(offset, |offset, &(start, length)| {
            if offset >= start {
                offset + length
            } else {
                offset
            }
        })
    };
    // Imported modules are not edited; only the linted file's facts can move.
    let here = |span: &Span| span.source_id == entry_source;
    let mut warnings = checked
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let span = diagnostic
                .labels
                .first()
                .map(|label| label.span)
                .or(diagnostic.span)
                .filter(|span| here(span));
            (
                span.map(|span| (original(span.start()), original(span.end()))),
                diagnostic.code.map(DiagnosticCode::name),
                diagnostic.message.clone(),
            )
        })
        .collect::<Vec<_>>();
    warnings.sort();
    let checked_facts = symbols.with_current(|| {
        Some(CheckedReturnRemovalFacts {
            expressions: checked
                .expr_types
                .iter()
                .filter(|(span, _)| here(span))
                .map(|(span, ty)| {
                    (
                        original(span.start()),
                        original(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            statements: checked
                .statement_positions
                .iter()
                .filter(|(span, _)| here(span))
                .map(|(span, position)| (original(span.start()), original(span.end()), *position))
                .collect(),
            returns: checked
                .function_return_types
                .iter()
                .filter(|(span, _)| here(span))
                .map(|(span, ty)| {
                    (
                        original(span.start()),
                        original(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            parameters: checked
                .parameter_types
                .iter()
                .filter(|(span, _)| here(span))
                .map(|(span, ty)| {
                    (
                        original(span.start()),
                        original(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            effects: checked
                .callable_effects
                .into_iter()
                .map(|(name, effects)| {
                    let effects = effects.map(|mut effects| {
                        effects.sort_by_key(Effect::as_str);
                        effects.dedup();
                        effects
                    });
                    (name, effects)
                })
                .collect(),
        })
    })?;
    Some(ProofFacts {
        checked: checked_facts,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use crate::xsht::lint::{LintOptions, Linter};
    use std::cell::Cell;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    thread_local! {
        static PROOFS: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn record_proof() {
        PROOFS.with(|proofs| proofs.set(proofs.get() + 1));
    }

    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                prefer_inferred_proc_returns: true,
                only: Some(vec![DiagnosticCode::LintPreferInferredProcReturn]),
                ..LintOptions::default()
            },
        )
        .diagnostics
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
                fix.replacement.as_deref().unwrap_or(""),
            );
        }
        fixed
    }

    #[test]
    fn an_exactly_inferred_private_return_is_dropped() {
        let source = "type Disk = {mount: Str, used: Int}\n\nproc disk(mount: Str) -> Result[Disk] {\n  Disk(mount:, used: mount.byte_len())\n}\n\nproc used(mount: Str) [error] -> Result[Int] {\n  disk(mount)?.used\n}\n\nprint ${used(\"/\")?}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "type Disk = {mount: Str, used: Int}\n\nproc disk(mount: Str) {\n  Disk(mount:, used: mount.byte_len())\n}\n\nproc used(mount: Str) [error] {\n  disk(mount)?.used\n}\n\nprint ${used(\"/\")?}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    #[test]
    fn exports_main_and_statement_bodies_are_not_candidates() {
        let source = "##! Module.\n\n## Counts.\nexport proc count() -> Result[Int, Error] {\n  1\n}\n\nproc log(line: Str) -> Result[Unit] {\n  print $line\n}\n\nproc main() -> Result[Unit] {\n  log(\"x\")?\n}\n";
        PROOFS.with(|proofs| proofs.set(0));
        assert!(lint(source).is_empty());
        assert_eq!(PROOFS.with(Cell::get), 0, "no candidate needs a proof");
    }

    #[test]
    fn a_recursive_proc_keeps_its_annotation() {
        let source = "proc depth(n: Int) [error] -> Result[Int] {\n  if n <= 0 {\n    return 0\n  }\n\n  depth(n - 1)? + 1\n}\n\nprint ${depth(3)?}\n";
        assert!(lint(source).is_empty());
    }

    #[test]
    fn an_inferred_variant_keeps_the_family_typed_annotation() {
        let source = "error Fam {\n  Bad(message: Str)\n}\n\nproc pick(n: Int) -> Result[Int, Fam] {\n  if n < 0 {\n    return Err(.Bad(message: \"negative\"))\n  }\n\n  n\n}\n\nprint ${pick(1) ?? 0}\n";
        assert!(lint(source).is_empty());
    }

    #[test]
    fn an_annotation_the_body_depends_on_or_differs_from_is_kept() {
        for source in [
            // The annotation is the expected type of the empty list.
            "proc names() -> Result[List[Str]] {\n  []\n}\n\nprint ${names()?.len()}\n",
            // The annotation is wider than the body's error type.
            "error Fam {\n  Bad(message: Str)\n}\n\nproc pick(n: Int) -> Result[Int] {\n  if n < 0 {\n    return Err(Fam.Bad(message: \"negative\"))\n  }\n\n  n\n}\n\nprint ${pick(1) ?? 0}\n",
            // The annotation narrows an integer literal.
            "proc port() -> Result[UInt] {\n  8080\n}\n\nprint ${port()?}\n",
        ] {
            assert!(lint(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn one_needed_annotation_does_not_hide_the_removable_ones() {
        let source = "proc names() -> Result[List[Str]] {\n  []\n}\n\nproc count() [error] -> Result[Int] {\n  names()?.len()\n}\n\nprint ${count()?}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            apply(&diagnostics, source),
            "proc names() -> Result[List[Str]] {\n  []\n}\n\nproc count() [error] {\n  names()?.len()\n}\n\nprint ${count()?}\n"
        );
    }

    #[test]
    fn the_lint_is_off_unless_the_project_opts_in() {
        let source = "proc one() -> Result[Int] {\n  1\n}\n\nprint ${one()?}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let diagnostics = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                only: Some(vec![DiagnosticCode::LintPreferInferredProcReturn]),
                ..LintOptions::default()
            },
        )
        .diagnostics;
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}
