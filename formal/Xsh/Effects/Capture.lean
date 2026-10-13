import Xsh.Effects.Lexical
import Xsh.Effects.Solver

/-!
An algebraic capture fragment over the effect lattice and equation solver.

Capture filters only the outward `error` fact. It leaves other named effects
and unresolved work intact. The closure law below applies to the known effect
set because `io` closure does not derive `error`; unresolved work is retained
separately and is not an effect-lattice member. These are facts about the
represented summaries and edges, not a proof that the Rust checker classifies
or solves every source construct this way.
-/

namespace Xsh.Semantics.Effects

open Xsh.Syntax
open Xsh.Semantics.Effects.Solver

/-- Remove only the propagated `error` member from a known effect set. -/
def captureEffectSet (effects : FiniteEffectSet) : FiniteEffectSet := fun effect =>
  if effect = .error then False else effects effect

theorem capture_effect_set_removes_error (effects : FiniteEffectSet) :
    ¬ captureEffectSet effects .error := by
  simp [captureEffectSet]

theorem capture_effect_set_preserves_other_effect (effects : FiniteEffectSet)
    (effect : Effect) (notError : effect ≠ .error) :
    captureEffectSet effects effect = effects effect := by
  simp [captureEffectSet, notError]

theorem capture_effect_set_idempotent (effects : FiniteEffectSet) :
    captureEffectSet (captureEffectSet effects) = captureEffectSet effects := by
  funext effect
  by_cases isError : effect = .error <;> simp [captureEffectSet, isError]

/-- `io` closure implies only host effects, so it never restores captured error. -/
theorem capture_commutes_with_effect_closure (effects : FiniteEffectSet) :
    captureEffectSet (closure effects) = closure (captureEffectSet effects) := by
  funext effect
  cases effect <;> simp [captureEffectSet, closure, ioComponent]

/-- Membership in a finite effect clause is independent of list ordering. -/
theorem ofList_permutation_invariant {first second : EffectSet}
    (same : first.Perm second) : ofList first = ofList second := by
  funext effect
  apply propext
  exact same.mem_iff

/-- Repeating one clause member cannot change its set interpretation. -/
theorem ofList_duplicate_invariant (effects : EffectSet) (effect : Effect) :
    ofList (effects ++ [effect, effect]) = ofList (effects ++ [effect]) := by
  funext required
  apply propext
  simp [ofList]

theorem captured_ofList_permutation_invariant {first second : EffectSet}
    (same : first.Perm second) :
    captureEffectSet (ofList first) = captureEffectSet (ofList second) := by
  rw [ofList_permutation_invariant same]

theorem captured_ofList_duplicate_invariant (effects : EffectSet) (effect : Effect) :
    captureEffectSet (ofList (effects ++ [effect, effect])) =
      captureEffectSet (ofList (effects ++ [effect])) := by
  rw [ofList_duplicate_invariant]

/-- A faulty capture mutation that discards every named effect. -/
def mutationDropNamedEffects (summary : EffectSummary) : EffectSummary :=
  { known := bottom, unresolved := summary.unresolved }

/-- A faulty capture mutation that treats unresolved work as resolved. -/
def mutationDropUnresolved (summary : EffectSummary) : EffectSummary :=
  { known := captureEffectSet summary.known, unresolved := false }

def networkOnlySummary : EffectSummary :=
  { known := ofList [.net], unresolved := false }

def unresolvedOnlySummary : EffectSummary :=
  { known := bottom, unresolved := true }

/-- Correct capture keeps a non-error effect that still violates an empty bound. -/
theorem capture_rejects_network_under_empty_bound :
    ¬ restrictedCallPermitted bottom
      (contractOfSummary (captureErrorSummary networkOnlySummary)) := by
  classical
  intro permitted
  have noUnresolved :
      (captureErrorSummary networkOnlySummary).unresolved = false := by
    rw [capture_error_summary_preserves_unresolved]
    rfl
  have checked : contractOfSummary (captureErrorSummary networkOnlySummary) =
      .checked (captureErrorSummary networkOnlySummary).known := by
    simp [contractOfSummary, noUnresolved]
  have factsPermitted :
      le (captureErrorSummary networkOnlySummary).known (closure bottom) := by
    simpa [restrictedCallPermitted, checked] using permitted
  have network : (captureErrorSummary networkOnlySummary).known .net := by
    rw [capture_error_summary_preserves_other_effect _ .net (by decide)]
    simp [networkOnlySummary, ofList]
  have impossible : False := by
    have present := factsPermitted .net network
    simp [closure, bottom] at present
  exact impossible.elim

/-- Erasing named effects makes the same network call look incorrectly safe. -/
theorem mutation_dropping_named_effects_is_observable :
    restrictedCallPermitted bottom
      (contractOfSummary (mutationDropNamedEffects networkOnlySummary)) ∧
    ¬ restrictedCallPermitted bottom
      (contractOfSummary (captureErrorSummary networkOnlySummary)) := by
  constructor
  · simp [restrictedCallPermitted, contractOfSummary, mutationDropNamedEffects,
      networkOnlySummary, le, bottom]
  · exact capture_rejects_network_under_empty_bound

/-- Correct capture keeps unresolved work, so it cannot justify a restricted call. -/
theorem capture_rejects_unresolved_under_empty_bound :
    ¬ restrictedCallPermitted bottom
      (contractOfSummary (captureErrorSummary unresolvedOnlySummary)) := by
  simp [restrictedCallPermitted, contractOfSummary, captureErrorSummary,
    unresolvedOnlySummary, toEffectSummary, captureErrorFacts, summaryFacts]

/-- Dropping unresolved work would incorrectly accept that same unknown call. -/
theorem mutation_dropping_unresolved_is_observable :
    restrictedCallPermitted bottom
      (contractOfSummary (mutationDropUnresolved unresolvedOnlySummary)) ∧
    ¬ restrictedCallPermitted bottom
      (contractOfSummary (captureErrorSummary unresolvedOnlySummary)) := by
  constructor
  · simp [restrictedCallPermitted, contractOfSummary, mutationDropUnresolved,
      unresolvedOnlySummary, le, captureEffectSet, bottom, closure]
  · exact capture_rejects_unresolved_under_empty_bound

/-- A captured contract edge must retain the solver's explicit unknown fact. -/
def capturedMissingContractGraph : Graph 2 where
  node source :=
    if source = 0 then
      { direct := emptyFacts, declared := none,
        edges := [{ target := 1, capturesError := true, readsContract := true }] }
    else
      { direct := emptyFacts, declared := none, edges := [] }

theorem capture_keeps_unknown_contract_unresolved :
    solve capturedMissingContractGraph 0 none = true ∧
      solve capturedMissingContractGraph 0 (some Effect.error) = false := by
  decide

end Xsh.Semantics.Effects
