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
        let enclosing = self.boundary.excluded_effects.len();
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
            self.boundary.excluded_effects.push(ExcludedEffect {
                effect,
                head: bound.head,
            });
        }
        enclosing
    }

    pub(super) fn constrain_callable_effect_exclusions(&mut self, contract: &crate::sema::effects::EffectContract<crate::sema::constraints::EffectVariableId>, name: &str, span: Span) {
        for excluded in self.boundary.excluded_effects.clone() {
            let predicate = crate::sema::effects::EffectPredicate::Excluded(contract.clone(), crate::sema::effects::EffectSet::singleton(excluded.effect.clone()));
            if let Err(error) = self.type_constraints.constrain_effect(predicate) {
                match error {
                    crate::sema::constraints::EffectConstraintError::Mismatch => self.report_excluded(format!("effect relationship required by `{name}` is excluded by `without {}`", excluded.effect.as_str()), span, &excluded),
                    _ => self.error(span, "callable effect exclusion escaped its declaration inference scope", DiagnosticCode::CheckLocalInference),
                }
            }
        }
    }

    pub(super) fn validate_symbolic_callable_effect_exclusions(&mut self, contract: &crate::sema::effects::EffectContract<crate::sema::constraints::EffectVariableId>, span: Span) {
        for excluded in self.boundary.excluded_effects.clone() {
            if !self.owns_effect_predicate(contract, span, |contract| crate::sema::effects::EffectPredicate::Excluded(contract, crate::sema::effects::EffectSet::singleton(excluded.effect))) {
                self.error(span, "callable effect exclusion is outside its checked scheme", DiagnosticCode::CheckLocalInference);
            }
        }
    }

    pub(super) fn leave_block_effect_bound(&mut self, enclosing: usize) {
        self.boundary.excluded_effects.truncate(enclosing);
    }

    fn excluding_bound(&self, required: &Effect) -> Option<ExcludedEffect> {
        self.boundary.excluded_effects
            .iter()
            .rev()
            .find(|excluded| {
                crate::sema::effects::effect_covers(required, &excluded.effect)
            })
            .cloned()
    }

    /// Whether the proc's own clause already rejects `required` here, in
    /// which case that report stands and the local bound adds nothing.
    fn clause_rejects(&self, required: &Effect) -> bool {
        self.boundary.current_effects
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
        if self.defer_effect_diagnostics {
            return;
        }
        if self.collecting_effects || self.boundary.in_pure || self.clause_rejects(required) {
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
        if self.defer_effect_diagnostics {
            self.defer_callable_effects(
                callee_effects.clone(),
                unknown_chain.to_vec(),
                callee_name,
                span,
            );
            return;
        }
        if self.collecting_effects || self.boundary.in_pure {
            return;
        }
        let Some(innermost) = self.boundary.excluded_effects.last().cloned() else {
            return;
        };
        let Some(callee_effects) = callee_effects else {
            // A restricted caller has already been told the contract is
            // unknown; an unrestricted one hears it because of the bound.
            if self.boundary.current_effects.is_some() {
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
