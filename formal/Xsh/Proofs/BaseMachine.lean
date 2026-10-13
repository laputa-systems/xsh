import Xsh.Semantics.BaseMachine

namespace Xsh.Proofs.BaseMachine

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Semantics.Environments
open Xsh.Semantics.BaseMachine

theorem accepted_initial_expression_is_wellFormed_fragment
    (names : LexicalEnvironment) (activations : ActivationEnvironment) (store : BaseStore)
    (expression : BaseExpr)
    (accepted : checkBaseExpr names activations store expression = true) :
    wellFormed (initialConfig names activations store expression) := by
  exact ⟨accepted, rfl⟩

theorem machine_progress_fragment (config : MachineConfig) :
    terminalOutcome config ≠ none ∨ ∃ next, step config = some next := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression =>
      cases expression <;> simp [terminalOutcome, step]
    | returned value =>
      cases continuation with
      | nil => simp [terminalOutcome]
      | cons frame rest =>
        cases frame <;> simp [terminalOutcome, step]
    | failed fault => simp [terminalOutcome]

theorem wellFormed_preserved_fragment {config next : MachineConfig}
    (wellFormedConfig : wellFormed config) (takesStep : step config = some next) :
    wellFormed next := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression =>
      cases expression with
      | integerLiteral value =>
        simp only [step] at takesStep
        cases takesStep
        cases checkedInt value <;>
          simp_all [wellFormed, updateState, controlOfIntegerResult]
      | identifier name =>
        simp only [step] at takesStep
        cases takesStep
        cases lookupInteger names activations store name <;>
          simp_all [wellFormed, updateState]
      | negate operand =>
        simp only [step] at takesStep
        cases takesStep
        simp_all [wellFormed, updateState, continuationChecked, checkBaseExpr]
      | binary operator left right =>
        simp only [step] at takesStep
        cases takesStep
        simp_all [wellFormed, updateState, continuationChecked, checkBaseExpr]
    | returned value =>
      cases continuation with
      | nil => simp [step] at takesStep
      | cons frame rest =>
        cases frame with
        | negate =>
          simp only [step] at takesStep
          cases takesStep
          cases IntegerOps.negate value <;>
            simp_all [wellFormed, updateState, controlOfIntegerResult, continuationChecked]
        | binaryLeft operator right =>
          simp only [step] at takesStep
          cases takesStep
          simp_all [wellFormed, updateState, continuationChecked]
        | binaryRight operator left =>
          simp only [step] at takesStep
          cases takesStep
          cases applyBinary operator left value <;>
            simp_all [wellFormed, updateState, controlOfIntegerResult, continuationChecked]
    | failed fault => simp [step] at takesStep

theorem machine_runner_never_stuck_fragment {fuel : Nat} {config : MachineConfig}
    (wellFormedConfig : wellFormed config) :
    runFuel fuel config ≠ .internalStuck := by
  induction fuel generalizing config with
  | zero =>
    unfold runFuel
    cases terminalOutcome config <;> simp
  | succ fuel inductionHypothesis =>
    unfold runFuel
    cases terminal : terminalOutcome config with
    | some outcome => simp
    | none =>
      have notFinal : ¬ terminalOutcome config ≠ none := by simp [terminal]
      obtain ⟨next, takesStep⟩ :=
        (machine_progress_fragment config).resolve_left notFinal
      rw [takesStep]
      exact inductionHypothesis (wellFormed_preserved_fragment wellFormedConfig takesStep)

theorem runner_soundness_fragment {fuel : Nat} {config : MachineConfig}
    {outcome : BaseOutcome}
    (completed : runFuel fuel config = .completed outcome) :
    ∃ used final, used ≤ fuel ∧ MachineSteps used config final ∧
      terminalOutcome final = some outcome := by
  induction fuel generalizing config outcome with
  | zero =>
    unfold runFuel at completed
    cases terminal : terminalOutcome config with
    | some result =>
      simp [terminal] at completed
      subst outcome
      exact ⟨0, config, Nat.zero_le _, .refl config, terminal⟩
    | none => simp [terminal] at completed
  | succ fuel inductionHypothesis =>
    unfold runFuel at completed
    cases terminal : terminalOutcome config with
    | some result =>
      simp [terminal] at completed
      subst outcome
      exact ⟨0, config, Nat.zero_le _, .refl config, terminal⟩
    | none =>
      cases takesStep : step config with
      | none => simp [terminal, takesStep] at completed
      | some next =>
        simp [terminal, takesStep] at completed
        obtain ⟨used, final, withinFuel, steps, isFinal⟩ := inductionHypothesis completed
        exact ⟨used + 1, final, Nat.succ_le_succ withinFuel, .step takesStep steps, isFinal⟩

private def exampleSmall (value : Int)
    (inRange : signed64Min ≤ value ∧ value < signed64Limit) : IntValue :=
  ⟨value, inRange⟩

private def exampleTwo : IntValue := exampleSmall 2 (by unfold signed64Min signed64Limit; omega)
private def exampleThree : IntValue := exampleSmall 3 (by unfold signed64Min signed64Limit; omega)
private def exampleFive : IntValue := exampleSmall 5 (by unfold signed64Min signed64Limit; omega)
private def exampleSix : IntValue := exampleSmall 6 (by unfold signed64Min signed64Limit; omega)
private def exampleEleven : IntValue := exampleSmall 11 (by unfold signed64Min signed64Limit; omega)
private def exampleTwentyTwo : IntValue := exampleSmall 22 (by unfold signed64Min signed64Limit; omega)

def acceptedArithmeticExample : BaseExpr :=
  .binary .add (.integerLiteral 2) (.integerLiteral 3)

theorem integer_checker_accepts_supported_arithmetic_fragment :
    checkBaseExpr [] [] (fun _ => none) acceptedArithmeticExample = true := by
  decide

theorem integer_checker_rejects_out_of_range_literal_fragment :
    checkBaseExpr [] [] (fun _ => none) (.integerLiteral signed64Limit) = false := by
  decide

private def missingName : NameId := ⟨⟨19, by decide⟩⟩

theorem integer_checker_rejects_unbound_name_fragment :
    checkBaseExpr [] [] (fun _ => none) (.identifier missingName) = false := by
  decide

theorem supported_arithmetic_runs_to_exact_value_fragment :
    runFuel 5 (initialConfig [] [] (fun _ => none) acceptedArithmeticExample) =
      .completed (.value exampleFive) := by
  decide

def overflowExample : BaseExpr :=
  .binary .multiply (.integerLiteral intMax.val) (.integerLiteral 2)

theorem checked_multiplication_overflow_is_explicit_fragment :
    runFuel 5 (initialConfig [] [] (fun _ => none) overflowExample) =
      .completed (.fault (.arithmetic .overflow)) := by
  decide

def divisionByZeroExample : BaseExpr :=
  .binary .divide (.integerLiteral 1) (.integerLiteral 0)

theorem checked_division_by_zero_is_explicit_fragment :
    runFuel 5 (initialConfig [] [] (fun _ => none) divisionByZeroExample) =
      .completed (.fault (.arithmetic .divisionByZero)) := by
  decide

def arithmeticMutantMultiply (left right : IntValue) : IntegerResult :=
  applyBinary .add left right

theorem multiplication_mutant_witness_fragment :
    applyBinary .multiply exampleTwo exampleThree = .value exampleSix ∧
      arithmeticMutantMultiply exampleTwo exampleThree = .value exampleFive ∧
      exampleSix ≠ exampleFive := by
  decide

private def redirectedName : NameId := ⟨⟨7, by decide⟩⟩
private def declaredBinder : BinderId := ⟨⟨11, by decide⟩⟩
private def redirectedBinder : BinderId := ⟨⟨12, by decide⟩⟩
private def declaredCell : CellId := ⟨0, declaredBinder⟩
private def redirectedCell : CellId := ⟨0, redirectedBinder⟩

private def integerCell (value : IntValue) : StoreCell BaseType CompositeValue hasBaseType := {
  contents := {
    typeTag := .integer
    value := .integer value
    wellTyped := trivial
  }
  mutability := .mutable
}

private def redirectionStore : BaseStore := fun cell =>
  if cell = declaredCell then some (integerCell exampleEleven)
  else if cell = redirectedCell then some (integerCell exampleTwentyTwo)
  else none

private def redirectionActivations : ActivationEnvironment :=
  bindCell (bindCell [] declaredBinder declaredCell) redirectedBinder redirectedCell

private def declaredNames : LexicalEnvironment :=
  bind emptyLexicalEnvironment redirectedName declaredBinder

private def mutatedNames : LexicalEnvironment :=
  bind emptyLexicalEnvironment redirectedName redirectedBinder

private def redirectionExpression : BaseExpr := .identifier redirectedName

theorem binder_redirection_changes_identifier_result_fragment :
    checkBaseExpr declaredNames redirectionActivations redirectionStore
      redirectionExpression = true ∧
    checkBaseExpr mutatedNames redirectionActivations redirectionStore
      redirectionExpression = true ∧
    runFuel 1 (initialConfig declaredNames redirectionActivations redirectionStore
      redirectionExpression) = .completed (.value exampleEleven) ∧
    runFuel 1 (initialConfig mutatedNames redirectionActivations redirectionStore
      redirectionExpression) = .completed (.value exampleTwentyTwo) ∧
    exampleEleven ≠ exampleTwentyTwo := by
  decide

end Xsh.Proofs.BaseMachine
