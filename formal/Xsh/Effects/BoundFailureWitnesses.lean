import Xsh.Effects.ExpressionBridge

/-!
Rejected classified obligations retain their source expression and required
effect in this finite bound-failure model. A failure names whether the clause
does not cover the requirement or an enclosing `without` entry blocks it.
This fragment does not model Rust diagnostics or close the full G3 obligation.
-/

namespace Xsh.Semantics.Effects

open Xsh.Syntax

inductive BoundFailureKind where
  | missingClause
  | withoutEntry
  deriving DecidableEq, Repr

/-- The source facts needed to explain one rejected classified obligation. -/
structure BoundFailureWitness where
  source : Expr
  required : Effect
  kind : BoundFailureKind

/-- Classify a rejected requirement while retaining both classified fields. -/
def classifyBoundFailure (context : LexicalContext) (obligation : EffectObligation) :
    Option BoundFailureWitness :=
  if clauseCoversBool context.clause obligation.required then
    if blockedByBool context.excluded obligation.required then
      some {
        source := obligation.source
        required := obligation.required
        kind := .withoutEntry
      }
    else none
  else
    some {
      source := obligation.source
      required := obligation.required
      kind := .missingClause
    }

/-- A successful classification cannot erase or substitute either input field. -/
theorem classified_failure_retains_site_and_effect (context : LexicalContext)
    (obligation : EffectObligation) (witness : BoundFailureWitness)
    (classified : classifyBoundFailure context obligation = some witness) :
    witness.source = obligation.source ∧ witness.required = obligation.required := by
  unfold classifyBoundFailure at classified
  cases clauseResult : clauseCoversBool context.clause obligation.required with
  | false =>
      simp [clauseResult] at classified
      cases classified
      exact ⟨rfl, rfl⟩
  | true =>
      cases blockedResult : blockedByBool context.excluded obligation.required with
      | false => simp [clauseResult, blockedResult] at classified
      | true =>
          simp [clauseResult, blockedResult] at classified
          cases classified
          exact ⟨rfl, rfl⟩

/-- The failure reason records which bound check rejected the requirement. -/
theorem classified_failure_reason_is_sound (context : LexicalContext)
    (obligation : EffectObligation) (witness : BoundFailureWitness)
    (classified : classifyBoundFailure context obligation = some witness) :
    (witness.kind = .missingClause ∧
      ¬ clauseCovers context.clause obligation.required) ∨
    (witness.kind = .withoutEntry ∧
      clauseCovers context.clause obligation.required ∧
      blockedBy context.excluded obligation.required) := by
  unfold classifyBoundFailure at classified
  cases clauseResult : clauseCoversBool context.clause obligation.required with
  | false =>
      simp [clauseResult] at classified
      cases classified
      apply Or.inl
      constructor
      · rfl
      · intro covered
        have coveredBool := (clauseCoversBool_correct context.clause
          obligation.required).mpr covered
        rw [clauseResult] at coveredBool
        cases coveredBool
  | true =>
      cases blockedResult : blockedByBool context.excluded obligation.required with
      | false => simp [clauseResult, blockedResult] at classified
      | true =>
          simp [clauseResult, blockedResult] at classified
          cases classified
          apply Or.inr
          constructor
          · rfl
          · exact ⟨(clauseCoversBool_correct context.clause
                obligation.required).mp clauseResult,
              (blockedByBool_correct context.excluded obligation.required).mp
                blockedResult⟩

/-- Every denied requirement has a classified witness carrying its site and effect. -/
theorem rejected_classified_obligation_retains_site_and_effect
    (context : LexicalContext) (obligation : EffectObligation)
    (rejected : ¬ permits context obligation.required) :
    ∃ witness, classifyBoundFailure context obligation = some witness ∧
      witness.source = obligation.source ∧ witness.required = obligation.required := by
  cases clauseResult : clauseCoversBool context.clause obligation.required with
  | false =>
      refine ⟨{
        source := obligation.source
        required := obligation.required
        kind := .missingClause
      }, ?_, rfl, rfl⟩
      simp [classifyBoundFailure, clauseResult]
  | true =>
      cases blockedResult : blockedByBool context.excluded obligation.required with
      | false =>
          have permitted : permitsBool context obligation.required = true := by
            simp [permitsBool, clauseResult, blockedResult]
          exact False.elim (rejected
            ((permitsBool_correct context obligation.required).mp permitted))
      | true =>
          refine ⟨{
            source := obligation.source
            required := obligation.required
            kind := .withoutEntry
          }, ?_, rfl, rfl⟩
          simp [classifyBoundFailure, clauseResult, blockedResult]

private def missingClauseContext : LexicalContext := ⟨[], []⟩

/-- A network requirement with no clause is classified as a missing-clause failure. -/
theorem missing_clause_network_witness_retains_obligation :
    classifyBoundFailure missingClauseContext exampleNetworkObligation =
      some {
        source := exampleNetworkObligation.source
        required := exampleNetworkObligation.required
        kind := .missingClause
      } := by
  rfl

/-- `without net` is reported as a blocking entry when the clause covers `net`. -/
theorem without_net_network_witness_retains_obligation :
    classifyBoundFailure exampleInsideWithoutNet exampleNetworkObligation =
      some {
        source := exampleNetworkObligation.source
        required := exampleNetworkObligation.required
        kind := .withoutEntry
      } := by
  rfl

def networkWithoutNetWitness : BoundFailureWitness := {
  source := exampleNetworkObligation.source
  required := .net
  kind := .withoutEntry
}

/-- Dropping the source site changes the concrete network rejection witness. -/
def mutationDropFailureSite (witness : BoundFailureWitness) : BoundFailureWitness :=
  { witness with source := .literal .null }

/-- Dropping the required effect changes the concrete network rejection witness. -/
def mutationDropFailureEffect (witness : BoundFailureWitness) : BoundFailureWitness :=
  { witness with required := .fs }

theorem mutation_dropping_failure_site_is_detected :
    mutationDropFailureSite networkWithoutNetWitness ≠ networkWithoutNetWitness := by
  intro changed
  have sourceChanged := congrArg
    (fun witness : BoundFailureWitness => witness.source) changed
  simp [mutationDropFailureSite, networkWithoutNetWitness,
    exampleNetworkObligation, exampleNetworkBody] at sourceChanged

theorem mutation_dropping_failure_effect_is_detected :
    mutationDropFailureEffect networkWithoutNetWitness ≠ networkWithoutNetWitness := by
  intro changed
  have effectChanged := congrArg
    (fun witness : BoundFailureWitness => witness.required) changed
  simp [mutationDropFailureEffect, networkWithoutNetWitness] at effectChanged

end Xsh.Semantics.Effects
