//! `lint.command-signature`: a statically named external command's flags are
//! compared with a signature the project configured for it.
//!
//! Checking reads only the source and the configuration. A signature is
//! reviewed for one tool version and the project pins another; unless they
//! match, and unless the signature is complete, a finding is advisory.
//! A flag list stops at the first argument that is not a literal word. An
//! applet's own `cli main` contract judges a call only when every argument is
//! literal. Either way a spliced or computed argument can never turn a valid
//! command invalid.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use xsh::frontend::check::{CheckOptions, CheckedCliEntryContract};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaRange, ArenaRunSegment, ArenaRunTailOperand, AstArena, RunFormId};

#[derive(Clone, Debug)]
pub enum SignatureShape {
    Flags { flags: Vec<String>, value_flags: Vec<String> },
    Applet(Arc<CheckedCliEntryContract>),
}

#[derive(Clone, Debug)]
pub struct CommandSignature {
    pub name: String,
    pub reviewed_version: String,
    pub pinned_version: String,
    pub complete: bool,
    pub shape: SignatureShape,
}

/// The checked `cli main` contract of the XSH script at `path`. The script is
/// read, parsed and checked and nothing in it runs.
pub fn load_applet_contract(path: &Path, module_roots: &[PathBuf]) -> Result<Arc<CheckedCliEntryContract>, String> {
    let file = path.to_str().ok_or_else(|| format!("{}: path is not UTF-8", path.display()))?;
    let contracts = xsh::frontend::symbols::SymbolOwner::new().with_current(|| {
        let entry = xsh::frontend::load::parse_load_check_file(file, module_roots.to_vec(), CheckOptions::for_check_cli())
            .map_err(|error| format!("{file}: {error}"))?;
        let checked = entry.checked.as_ref().filter(|checked| entry.parsed.diagnostics.is_empty() && checked.diagnostics.is_empty())
            .ok_or_else(|| format!("{file}: the applet does not check cleanly"))?;
        checked.checked_declarations.cli_entry_contracts(&entry.parsed.arena, &entry.sources)
            .map_err(|error| format!("{file}: {error:?}"))
    })?;
    let mut mains = contracts.into_iter().filter(|contract| contract.path().is_empty());
    match (mains.next(), mains.next()) {
        (Some(contract), None) => Ok(Arc::new(contract)),
        _ => Err(format!("{file}: expected exactly one `cli main` entry without a subcommand path")),
    }
}

impl CommandSignature {
    fn finding(&self, message: String, span: Span, rejection: String) -> Diagnostic {
        let hard = self.complete && self.reviewed_version == self.pinned_version;
        let (severity, code) = if hard {
            (Severity::Error, DiagnosticCode::LintCommandSignature)
        } else {
            (Severity::Warning, DiagnosticCode::LintCommandSignatureAdvisory)
        };
        let advice = if !self.complete {
            "the signature is partial".to_owned()
        } else if self.reviewed_version != self.pinned_version {
            format!(
                "the signature was reviewed for version {} but version {} is pinned",
                self.reviewed_version, self.pinned_version
            )
        } else {
            rejection
        };
        Diagnostic::new(severity, message).with_code(code).with_label(Label::primary(span, advice))
    }

    fn unknown_flag(&self, flag: &str, span: Span) -> Diagnostic {
        self.finding(
            format!("`{}` has no listed flag `{flag}`", self.name),
            span,
            format!("version {} has no such flag", self.reviewed_version),
        )
    }
}

fn lint_flags(
    arena: &AstArena,
    source: &str,
    signature: &CommandSignature,
    flags: &[String],
    value_flags: &[String],
    tail: ArenaRange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let lists = |flag: &str| flags.iter().chain(value_flags).any(|listed| listed == flag);
    let takes_value = |flag: &str| value_flags.iter().any(|listed| listed == flag);
    let mut value_pending = false;
    for operand in arena.run_tail_operands(tail) {
        let ArenaRunTailOperand::Argument(id) = operand else {
            continue;
        };
        let argument = arena.command_arg(*id);
        let Some(word) = super::literal_command_word(arena, source, argument) else {
            break;
        };
        if std::mem::take(&mut value_pending) {
            continue;
        }
        if word == "--" {
            break;
        }
        let span = arena.span(argument.span);
        if let Some(long) = word.strip_prefix("--") {
            let (flag, attached) = match long.split_once('=') {
                Some((flag, _)) => (format!("--{flag}"), true),
                None => (word.clone(), false),
            };
            if !lists(&flag) {
                diagnostics.push(signature.unknown_flag(&flag, span));
            } else {
                value_pending = takes_value(&flag) && !attached;
            }
        } else if let Some(cluster) = word.strip_prefix('-').filter(|cluster| !cluster.is_empty()) {
            for (offset, letter) in cluster.char_indices() {
                let flag = format!("-{letter}");
                if !lists(&flag) {
                    diagnostics.push(signature.unknown_flag(&flag, span));
                } else if takes_value(&flag) {
                    value_pending = offset + letter.len_utf8() == cluster.len();
                    break;
                }
            }
        }
    }
}

fn lint_applet(
    arena: &AstArena,
    source: &str,
    signature: &CommandSignature,
    contract: &CheckedCliEntryContract,
    segment: &ArenaRunSegment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut argv = Vec::new();
    let mut span = None;
    for operand in arena.run_tail_operands(segment.tail) {
        let ArenaRunTailOperand::Argument(id) = operand else {
            continue;
        };
        let argument = arena.command_arg(*id);
        let Some(word) = super::literal_command_word(arena, source, argument) else {
            return;
        };
        span.get_or_insert_with(|| arena.span(argument.span));
        argv.push(word);
    }
    if let Some(rejection) = contract.argument_error(&argv) {
        let span = span.unwrap_or_else(|| arena.span(segment.span));
        diagnostics.push(signature.finding(
            format!("`{}` rejects these arguments: {rejection}", signature.name),
            span,
            format!("the `cli main` contract of `{}` rejects them", signature.name),
        ));
    }
}

pub(super) fn lint_run(
    arena: &AstArena,
    source: &str,
    signatures: &[CommandSignature],
    run: RunFormId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let form = arena.run_form(run).clone();
    for segment in arena.run_segments(form.segments) {
        let Some(head) = super::literal_command_word(arena, source, &segment.target) else {
            continue;
        };
        let Some(signature) = signatures.iter().find(|signature| signature.name == head) else {
            continue;
        };
        match &signature.shape {
            SignatureShape::Flags { flags, value_flags } => {
                lint_flags(arena, source, signature, flags, value_flags, segment.tail, diagnostics);
            }
            SignatureShape::Applet(contract) => {
                lint_applet(arena, source, signature, contract, segment, diagnostics);
            }
        }
    }
}

#[cfg(test)]
#[path = "lint_command_signature_tests.rs"]
mod tests;
