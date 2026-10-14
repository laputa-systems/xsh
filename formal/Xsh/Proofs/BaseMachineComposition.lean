import Xsh.Proofs.BaseHostTrace
import Xsh.Proofs.Foundation.ObservedMachine

namespace Xsh.Proofs.BaseMachineComposition

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Syntax
open Xsh.Semantics.Environments
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace
open Xsh.Proofs.BaseHostTrace

private def sourceName : RawName := ⟨⟨7, by decide⟩⟩
private def sourceBinder : BinderId := ⟨⟨11, by decide⟩⟩
private def sourceCell : CellId := ⟨1, sourceBinder⟩

private def four : IntValue :=
  ⟨4, by unfold signed64Min signed64Limit; omega⟩

private def three : IntValue :=
  ⟨3, by unfold signed64Min signed64Limit; omega⟩

private def seven : IntValue :=
  ⟨7, by unfold signed64Min signed64Limit; omega⟩

private def one : IntValue :=
  ⟨1, by unfold signed64Min signed64Limit; omega⟩

private def sourceNames : LexicalEnvironment :=
  [fun name => if name = sourceName then some sourceBinder else none]

private def sourceActivations : ActivationEnvironment :=
  [fun binder => if binder = sourceBinder then some sourceCell else none]

private def sourceCellValue : StoreCell BaseType CompositeValue hasBaseType :=
  ⟨typedInteger four, .mutable⟩

private def sourceStore : BaseStore := fun cell =>
  if cell = sourceCell then some sourceCellValue else none

private def sourceExpression : BaseExpr :=
  .binary .add (.identifier sourceName) .timeNow

private def sourceMachine : MachineConfig :=
  initialConfig sourceNames sourceActivations sourceStore sourceExpression

private def clockResponse : BaseHostResponse := ⟨typedInteger three⟩

private def sourceFinal : MachineConfig :=
  updateState sourceMachine (.returned (typedInteger seven)) []

private def sourceEvents : List BaseHostEvent :=
  [.internal,
   .internal,
   .internal,
   .request BaseHostRequest.timeNow,
   .response BaseHostRequest.timeNow clockResponse,
   .internal]

private theorem sourceRunner_reduces :
    runBaseHostFuel 6 [clockResponse] sourceMachine = ⟨sourceFinal, sourceEvents⟩ := by
  rfl

/-- The checked name resolves through its active cell, then the runner composes that stored
integer with a typed clock response while preserving the observed-machine invariant. -/
theorem checked_binding_and_host_run_compose :
    checkBaseExpr sourceNames sourceActivations sourceStore sourceExpression = true ∧
    lookupBaseValue sourceNames sourceActivations sourceStore sourceName =
      .ok (typedInteger four) ∧
    runBaseHostFuel 6 [clockResponse] sourceMachine = ⟨sourceFinal, sourceEvents⟩ ∧
    BaseHostTrace sourceMachine sourceEvents sourceFinal ∧
    Xsh.Semantics.BaseMachine.wellFormed sourceFinal ∧
    Xsh.Semantics.Foundation.wellFormed
      ⟨sourceMachine, sourceFinal, sourceEvents⟩ ∧
    terminalOutcome sourceFinal = some (.value (typedInteger seven)) := by
  have accepted :
      checkBaseExpr sourceNames sourceActivations sourceStore sourceExpression = true := by
    decide
  have bindingResolves :
      lookupBaseValue sourceNames sourceActivations sourceStore sourceName =
        .ok (typedInteger four) := by
    rfl
  have initialWellFormed :=
    Xsh.Proofs.BaseMachine.accepted_initial_expression_is_wellFormed_fragment
      sourceNames sourceActivations sourceStore sourceExpression accepted
  have trace := runBaseHostFuel_trace 6 [clockResponse] sourceMachine
  have finalWellFormed :=
    runBaseHostFuel_preserves_wellFormed 6 [clockResponse] sourceMachine initialWellFormed
  rw [sourceRunner_reduces] at trace finalWellFormed
  have observedWellFormed : Xsh.Semantics.Foundation.wellFormed
      ⟨sourceMachine, sourceFinal, sourceEvents⟩ :=
    ⟨⟨sourceExpression, rfl⟩, initialWellFormed, finalWellFormed, trace⟩
  exact ⟨accepted, bindingResolves, sourceRunner_reduces, trace, finalWellFormed,
    observedWellFormed, rfl⟩

private def subtractedExpression : BaseExpr :=
  .binary .subtract (.identifier sourceName) .timeNow

private def subtractedMachine : MachineConfig :=
  initialConfig sourceNames sourceActivations sourceStore subtractedExpression

private def subtractedFinal : MachineConfig :=
  updateState subtractedMachine (.returned (typedInteger one)) []

/-- Changing only the arithmetic operator preserves checker acceptance but changes the final
machine result. -/
theorem arithmetic_operator_mutation_changes_the_result :
    checkBaseExpr sourceNames sourceActivations sourceStore sourceExpression = true ∧
    checkBaseExpr sourceNames sourceActivations sourceStore subtractedExpression = true ∧
    (runBaseHostFuel 6 [clockResponse] sourceMachine).finalConfig = sourceFinal ∧
    (runBaseHostFuel 6 [clockResponse] subtractedMachine).finalConfig = subtractedFinal ∧
    sourceFinal.control ≠ subtractedFinal.control := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩
  · decide
  · decide
  · rw [sourceRunner_reduces]
  · rfl
  · intro sameControl
    injection sameControl with sameValue
    have samePayload := congrArg (fun value : BaseTypedValue => value.value) sameValue
    injection samePayload with sameInteger
    have sameNumber := congrArg Subtype.val sameInteger
    have differentNumbers : (7 : Int) ≠ 1 := by decide
    change (7 : Int) = 1 at sameNumber
    exact differentNumbers sameNumber

private def staleBinder : BinderId := ⟨⟨12, by decide⟩⟩

private def mutatedNames : LexicalEnvironment :=
  [fun name => if name = sourceName then some staleBinder else none]

private def bindingMutationMachine : MachineConfig :=
  initialConfig mutatedNames sourceActivations sourceStore sourceExpression

private def bindingMutationFinal : MachineConfig :=
  updateState bindingMutationMachine (.failed (.inactiveBinder staleBinder))
    [.binaryLeft .add .timeNow]

/-- Redirecting the source name to a binder with no active cell is rejected and reaches the
specified inactive-binding fault before the clock request can occur. -/
theorem inactive_binding_mutation_is_rejected_and_observable :
    checkBaseExpr mutatedNames sourceActivations sourceStore sourceExpression = false ∧
    runBaseHostFuel 2 [] bindingMutationMachine =
      ⟨bindingMutationFinal, [.internal, .internal]⟩ ∧
    terminalOutcome bindingMutationFinal =
      some (.fault (.inactiveBinder staleBinder)) := by
  refine ⟨?_, ?_, rfl⟩
  · decide
  · rfl

private def wrongClockResponse : BaseHostResponse :=
  ⟨⟨.boolean, .boolean false, trivial⟩⟩

private def wrongResponseFinal : MachineConfig :=
  updateState sourceMachine (.failed .hostResponseTypeMismatch)
    [.binaryRight .add four]

private def wrongResponseEvents : List BaseHostEvent :=
  [.internal,
   .internal,
   .internal,
   .request BaseHostRequest.timeNow,
   .response BaseHostRequest.timeNow wrongClockResponse]

/-- A response with the wrong type is recorded as a host event and becomes the machine's
specified failure while retaining the checked continuation. -/
theorem wrong_host_response_mutation_is_rejected_in_the_run :
    checkBaseExpr sourceNames sourceActivations sourceStore sourceExpression = true ∧
    checkedHostResponse? BaseHostRequest.timeNow wrongClockResponse = none ∧
    runBaseHostFuel 5 [wrongClockResponse] sourceMachine =
      ⟨wrongResponseFinal, wrongResponseEvents⟩ ∧
    BaseHostTrace sourceMachine wrongResponseEvents wrongResponseFinal ∧
    Xsh.Semantics.BaseMachine.wellFormed wrongResponseFinal ∧
    terminalOutcome wrongResponseFinal = some (.fault .hostResponseTypeMismatch) := by
  have accepted :
      checkBaseExpr sourceNames sourceActivations sourceStore sourceExpression = true := by
    decide
  have initialWellFormed :=
    Xsh.Proofs.BaseMachine.accepted_initial_expression_is_wellFormed_fragment
      sourceNames sourceActivations sourceStore sourceExpression accepted
  have trace := runBaseHostFuel_trace 5 [wrongClockResponse] sourceMachine
  have finalWellFormed :=
    runBaseHostFuel_preserves_wellFormed 5 [wrongClockResponse] sourceMachine initialWellFormed
  rw [show runBaseHostFuel 5 [wrongClockResponse] sourceMachine =
      ⟨wrongResponseFinal, wrongResponseEvents⟩ by rfl] at trace finalWellFormed
  exact ⟨accepted, rfl, rfl, trace, finalWellFormed, rfl⟩

end Xsh.Proofs.BaseMachineComposition
