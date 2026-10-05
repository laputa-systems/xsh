//! Local negative effect bounds: `without EFFECT, ... { BODY }`.
//!
//! A bound subtracts effects from what a lexical region may do. It sits beside
//! the enclosing proc's clause, not inside it: the clause says what the proc
//! may do at all, and each enclosing `without` removes effects from that for
//! its region. Every effect requirement the checker records (a host
//! operation, or a callee's contract) is tested against both.
//!
//! The bound is a static claim only. It adds nothing to the effects inferred
//! for the enclosing proc and nothing to the lowered program.

use super::{Checker, Diagnostic, Effect, Label, Span};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaProgram, BlockId};

/// One effect an enclosing `without` excludes, and the head that says so.
#[derive(Clone, Debug)]
pub(super) struct ExcludedEffect {
    effect: Effect,
    head: Span,
}

impl Checker {
    /// Enters `block`'s `without` bound, if it has one. The result restores
    /// the enclosing bound through [`Checker::leave_block_effect_bound`].
    pub(super) fn enter_block_effect_bound(
        &mut self,
        arena: &ArenaProgram,
        block: BlockId,
    ) -> usize {
        let enclosing = self.excluded_effects.len();
        let Some(bound) = arena.arena.block_effect_bound(block) else {
            return enclosing;
        };
        for effect in arena.arena.effects(bound.effects) {
            if effect == Effect::Error {
                // Whether an error leaves a region is control flow, which
                // `try` already bounds; subtracting it here would be a second
                // spelling of the same thing.
                self.error(
                    bound.head,
                    "`without` subtracts host effects; bound errors with `try { ... }`",
                    DiagnosticCode::CheckWithoutEffect,
                );
                continue;
            }
            self.excluded_effects.push(ExcludedEffect {
                effect,
                head: bound.head,
            });
        }
        enclosing
    }

    pub(super) fn leave_block_effect_bound(&mut self, enclosing: usize) {
        self.excluded_effects.truncate(enclosing);
    }

    /// The innermost enclosing bound that rules out `required`. `io` implies
    /// `fs`, `net`, `process`, and `env`, so excluding any of those excludes a
    /// requirement for `io`; excluding `io` excludes only `io` itself.
    fn excluding_bound(&self, required: &Effect) -> Option<ExcludedEffect> {
        self.excluded_effects
            .iter()
            .rev()
            .find(|excluded| {
                excluded.effect == *required
                    || (*required == Effect::Io
                        && matches!(
                            excluded.effect,
                            Effect::Fs | Effect::Net | Effect::Process | Effect::Env
                        ))
            })
            .cloned()
    }

    /// Whether the proc's own clause already rejects `required` here, in
    /// which case that report stands and the local bound adds nothing.
    fn clause_rejects(&self, required: &Effect) -> bool {
        self.current_effects
            .as_ref()
            .is_some_and(|clause| !Self::effects_covers(clause, required))
    }

    fn report_excluded(&mut self, message: String, span: Span, excluded: &ExcludedEffect) {
        self.diagnostics.push(
            Diagnostic::error(message.clone())
                .with_code(DiagnosticCode::CheckEffectViolation)
                .with_label(Label::primary(span, message))
                .with_label(Label::secondary(
                    excluded.head,
                    format!("this region excludes `{}`", excluded.effect.as_str()),
                )),
        );
    }

    /// Tests a host operation's own effect requirement against the enclosing
    /// `without` bounds. `subject` names the operation, as in the clause
    /// report it parallels.
    pub(super) fn check_effect_not_excluded(
        &mut self,
        required: &Effect,
        span: Span,
        subject: &str,
    ) {
        if self.collecting_effects || self.in_pure || self.clause_rejects(required) {
            return;
        }
        let Some(excluded) = self.excluding_bound(required) else {
            return;
        };
        let denied = excluded.effect.as_str();
        let message = if excluded.effect == *required {
            format!(
                "{subject} requires the `{denied}` effect, which `without {denied}` excludes here"
            )
        } else {
            format!(
                "{subject} requires the `{}` effect, which implies `{denied}`; `without {denied}` excludes it here",
                required.as_str()
            )
        };
        self.report_excluded(message, span, &excluded);
    }

    /// Tests a callee's effect contract against the enclosing `without`
    /// bounds. `unknown_chain` is the call chain to the dependency that makes
    /// an inferred callee's effects unknown, when there is one.
    pub(super) fn check_callee_not_excluded(
        &mut self,
        callee_effects: &Option<Vec<Effect>>,
        unknown_chain: &[String],
        callee_name: &str,
        span: Span,
    ) {
        if self.collecting_effects || self.in_pure {
            return;
        }
        let Some(innermost) = self.excluded_effects.last().cloned() else {
            return;
        };
        let Some(callee_effects) = callee_effects else {
            // A restricted caller has already been told the contract is
            // unknown; an unrestricted one hears it because of the bound.
            if self.current_effects.is_some() {
                return;
            }
            let denied = innermost.effect.as_str();
            let message = if unknown_chain.is_empty() {
                format!(
                    "callable `{callee_name}` has an unknown or unrestricted effect contract, so `without {denied}` cannot hold; call a named callable with checked effects"
                )
            } else {
                format!(
                    "proc `{callee_name}` has an unknown effect summary: {}; `without {denied}` cannot hold",
                    unknown_chain.join(" -> ")
                )
            };
            self.report_excluded(message, span, &innermost);
            return;
        };
        for required in callee_effects {
            if self.clause_rejects(required) {
                continue;
            }
            let Some(excluded) = self.excluding_bound(required) else {
                continue;
            };
            let denied = excluded.effect.as_str();
            let message = if excluded.effect == *required {
                format!(
                    "effect `{denied}` required by `{callee_name}` is excluded by `without {denied}`"
                )
            } else {
                format!(
                    "effect `{}` required by `{callee_name}` implies `{denied}`, which `without {denied}` excludes",
                    required.as_str()
                )
            };
            self.report_excluded(message, span, &excluded);
        }
    }
}
