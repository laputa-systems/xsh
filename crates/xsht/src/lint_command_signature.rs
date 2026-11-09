//! `lint.command-signature`: a statically named external command's flags are
//! compared with a signature the project configured for it.
//!
//! Checking reads only the source and the configuration. A signature is
//! reviewed for one tool version and the project pins another; unless they
//! match, and unless the signature is complete, a finding is advisory.
//! Checking stops at the first argument that is not a literal word, so a
//! spliced or computed argument can never turn a valid command invalid.

use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaRunTailOperand, AstArena, RunFormId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSignature {
    pub name: String,
    pub reviewed_version: String,
    pub pinned_version: String,
    pub complete: bool,
    pub flags: Vec<String>,
    pub value_flags: Vec<String>,
}

impl CommandSignature {
    fn lists(&self, flag: &str) -> bool {
        self.flags.iter().chain(&self.value_flags).any(|listed| listed == flag)
    }

    fn takes_value(&self, flag: &str) -> bool {
        self.value_flags.iter().any(|listed| listed == flag)
    }

    fn hard_failures(&self) -> bool {
        self.complete && self.reviewed_version == self.pinned_version
    }

    fn unknown_flag(&self, flag: &str, span: Span) -> Diagnostic {
        let (severity, code) = if self.hard_failures() {
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
            format!("version {} has no such flag", self.reviewed_version)
        };
        Diagnostic::new(severity, format!("`{}` has no listed flag `{flag}`", self.name))
            .with_code(code)
            .with_label(Label::primary(span, advice))
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
        let mut value_pending = false;
        for operand in arena.run_tail_operands(segment.tail) {
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
                if !signature.lists(&flag) {
                    diagnostics.push(signature.unknown_flag(&flag, span));
                } else {
                    value_pending = signature.takes_value(&flag) && !attached;
                }
            } else if let Some(cluster) = word.strip_prefix('-').filter(|cluster| !cluster.is_empty()) {
                for (offset, letter) in cluster.char_indices() {
                    let flag = format!("-{letter}");
                    if !signature.lists(&flag) {
                        diagnostics.push(signature.unknown_flag(&flag, span));
                    } else if signature.takes_value(&flag) {
                        value_pending = offset + letter.len_utf8() == cluster.len();
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "lint_command_signature_tests.rs"]
mod tests;
