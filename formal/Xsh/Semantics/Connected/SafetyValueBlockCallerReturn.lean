import Xsh.Semantics.Connected.SafetyValueBlockUnwinding

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem typed_machine_caller_resume (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (control : Control) (value : Value) (cells : ActiveCells) (activation : Nat)
    (saved : Substitution) (result : RuntimeType) (event : Label)
    (step : Step { base with control := control, stack := .caller cells activation saved result :: base.stack } event
      { base with control := .value value, cells := cells, activation := activation, solution := saved })
    (valueTyped : HasType result value)
    (callable : ValueCallables base.functions value)
    (typed : TypedMachine globals storage types
      { base with control := control, stack := .caller cells activation saved result :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .value value, cells := cells, activation := activation, solution := saved } := by
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activeActivation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, controlTyped, stack, pending⟩ := typed.focus
  cases stack with
  | cons frameTyped middleClosed remaining =>
    cases frameTyped with
    | caller storageSame outerSorted savedSorted activationSame completion resultClosed callClosed scope writable schemes storageAgrees schemas =>
      rename_i inner outer callType
      have unique := pending.unique
      have fresh := pending.fresh
      simp only [pendingMachineBinders, pendingStackBinders, pendingFrameBinders, frameCallerActivation,
        List.map_nil, List.nil_append] at unique fresh
      have newPending : PendingBindingsSafe
          { base with control := .value value, cells := cells, activation := activation, solution := outer.runtimeSolution } := by
        constructor
        · simpa [pendingMachineBinders, pendingControlBinders] using (List.nodup_append.mp unique).2.1
        · intro binding member
          apply fresh binding
          apply List.mem_append_right
          simpa [pendingMachineBinders, pendingControlBinders] using member
      have oldStack : StackTyped
          { base with control := .value value, cells := cells, activation := activation, solution := outer.runtimeSolution }
          (.expression outer callType) output base.stack := by
        apply stack_typing_survives_with_pending step remaining newPending
        · simpa [TypedDemand.context] using activationSame.symm
        · rfl
      refine ⟨.expression outer callType, output, ?_, ?_, ?_, outerSorted, savedSorted,
        ?_, middleClosed, outputClosed, .value valueTyped callClosed callable, oldStack, newPending⟩
      · rfl
      · simpa [TypedDemand.context] using activationSame
      · simpa [TypedDemand.context, storageSame] using storageEqual
      · exact ⟨by simpa [ScopeStateAgrees, TypedDemand.context] using scope, writable, schemas, schemes, storageAgrees⟩

theorem typed_machine_return_to_caller (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value) (cells : ActiveCells) (activation : Nat) (saved : Substitution) (result : RuntimeType)
    (valueTyped : HasType result value)
    (typed : TypedMachine globals storage types
      { base with control := .returned value, stack := .caller cells activation saved result :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .value value, cells := cells, activation := activation, solution := saved } := by
  obtain ⟨_, _, _, _, _, _, _, _, _, _, control, _, _⟩ := typed.focus
  cases control with
  | returned valueTyped' closed callable =>
    exact typed_machine_caller_resume globals storage types base (.returned value) value cells activation saved result .internal
      (Step.returnedToCaller base value cells activation saved result valueTyped) valueTyped callable typed

theorem typed_machine_completion_to_caller (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value) (cells : ActiveCells) (activation : Nat) (saved : Substitution) (result : RuntimeType)
    (valueTyped : HasType result value)
    (typed : TypedMachine globals storage types
      { base with control := .completed value, stack := .caller cells activation saved result :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .value value, cells := cells, activation := activation, solution := saved } := by
  obtain ⟨_, _, _, _, _, _, _, _, _, _, control, _, _⟩ := typed.focus
  cases control with
  | completed valueTyped' closed accepted callable =>
    exact typed_machine_caller_resume globals storage types base (.completed value) value cells activation saved result .internal
      (Step.completedToCaller base value cells activation saved result valueTyped) valueTyped callable typed

theorem typed_machine_propagation_to_caller (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (error : Value) (oldType successType errorType : RuntimeType)
    (cells : ActiveCells) (activation : Nat) (saved : Substitution) (errorTyped : HasType errorType error)
    (typed : TypedMachine globals storage types
      { base with control := .propagated (.error error oldType), stack := .caller cells activation saved (.result successType errorType) :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .value (.error error successType), cells := cells, activation := activation, solution := saved } := by
  obtain ⟨_, _, _, _, _, _, _, _, _, _, control, _, _⟩ := typed.focus
  cases control with
  | propagated matching actualType closed callable =>
    have valueTyped : HasType (.result successType errorType) (.error error successType) :=
      compatible_error_value error successType errorType errorTyped
    exact typed_machine_caller_resume globals storage types base (.propagated (.error error oldType))
      (.error error successType) cells activation saved (.result successType errorType) .internal
      (Step.propagatedToCaller base error oldType successType errorType cells activation saved errorTyped)
      valueTyped (.error callable) typed

end Xsh.Semantics.Connected
