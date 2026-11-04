import Xsh.Semantics.Connected.SafetyValueBlockInvoke

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem typed_return_stack_progress (state : Machine) (input output : TypedDemand)
    (stack : List Frame) (value : Value) (control : ControlTyped state input (.returned value))
    (typedStack : StackTyped state input output stack) :
    stack = [] ∨ ∃ next, Step { state with control := .returned value, stack := stack } .internal next := by
  cases typedStack with
  | nil => exact Or.inl rfl
  | @cons input middle output frame rest typedFrame closed remaining =>
    right
    cases frame <;> try (exact ⟨_, by simpa using Step.returnUnwind { state with stack := rest } value _ rfl⟩)
    case caller cells activation solution result =>
      cases typedFrame with
      | caller sameStorage proofSorted runtimeSorted savedActivation completion resultClosed callClosed scope writable schemes storage schemas =>
        rename_i inner outer callType
        cases control with
        | returned typed valueClosed callables =>
          have sameType : HasType result value := by
            simp only [TypedDemand.context] at valueClosed
            rw [resultClosed] at valueClosed
            cases valueClosed
            exact typed
          exact ⟨_, by simpa using Step.returnedToCaller { state with stack := rest } value cells activation outer.runtimeSolution result sameType⟩

theorem typed_machine_return_progress (state : Machine) (globals : Scope) (storage : BindingStorageMap)
    (types : CellTypes) (value : Value) (invariant : TypedMachine globals storage types state)
    (current : state.control = .returned value) :
    state.stack = [] ∨ ∃ next, Step state .internal next := by
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
  rw [current] at control
  rcases typed_return_stack_progress state input output state.stack value control stack with empty | ⟨next, step⟩
  · exact Or.inl empty
  · have sameState : { state with control := .returned value, stack := state.stack } = state := by
      cases state
      simp_all
    rw [sameState] at step
    exact Or.inr ⟨next, step⟩

theorem typed_completed_stack_progress (state : Machine) (input output : TypedDemand)
    (stack : List Frame) (value : Value) (control : ControlTyped state input (.completed value))
    (typedStack : StackTyped state input output stack) :
    stack = [] ∨ ∃ next, Step { state with control := .completed value, stack := stack } .internal next := by
  cases control with
  | completed typed valueClosed accepted callables =>
    rename_i context ty runtimeType
    cases typedStack with
    | nil => exact Or.inl rfl
    | @cons input middle output frame rest typedFrame closed remaining =>
      right
      cases typedFrame with
      | continueBlock solutionSame runtimeSame storageSame resultSame activationSame completion scope writable schemes storage schemas restTyped fresh =>
        rename_i outer cells following
        cases following with
        | nil => exact ⟨_, by simpa using Step.tailBranchComplete { state with stack := rest } value cells⟩
        | cons next tail => exact ⟨_, by simpa using Step.nonTailBranchComplete { state with stack := rest } value cells next tail⟩
      | valueBlock solutionSame runtimeSame storageSame resultSame activationSame completion scope writable schemes storage schemas =>
        rename_i outer cells blockType
        exact ⟨_, by simpa using Step.valueBlockComplete { state with stack := rest } value cells⟩
      | caller storageSame proofSorted runtimeSorted activationSame completion resultClosed callClosed scope writable schemes storage schemas =>
        rename_i outer cells activation result callType
        have sameType : HasType result value := by
          have sameMono : ty = context.result := by
            rcases accepted with absent | present
            · rw [completion] at absent
              cases absent
            · rw [completion] at present
              exact (Option.some.inj present).symm
          rw [sameMono] at valueClosed
          rw [resultClosed] at valueClosed
          cases valueClosed
          exact typed
        exact ⟨_, by simpa using Step.completedToCaller { state with stack := rest } value cells activation outer.runtimeSolution result sameType⟩

theorem typed_machine_completed_progress (state : Machine) (globals : Scope) (storage : BindingStorageMap)
    (types : CellTypes) (value : Value) (invariant : TypedMachine globals storage types state)
    (current : state.control = .completed value) :
    state.stack = [] ∨ ∃ next, Step state .internal next := by
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
  rw [current] at control
  rcases typed_completed_stack_progress state input output state.stack value control stack with empty | ⟨next, step⟩
  · exact Or.inl empty
  · have sameState : { state with control := .completed value, stack := state.stack } = state := by
      cases state
      simp_all
    rw [sameState] at step
    exact Or.inr ⟨next, step⟩

theorem typed_propagated_stack_progress (state : Machine) (input output : TypedDemand)
    (stack : List Frame) (error : Value) (oldType : RuntimeType)
    (control : ControlTyped state input (.propagated (.error error oldType)))
    (typedStack : StackTyped state input output stack) :
    stack = [] ∨ ∃ next, Step { state with control := .propagated (.error error oldType), stack := stack } .internal next := by
  cases typedStack with
  | nil => exact Or.inl rfl
  | @cons input middle output frame rest typedFrame closed remaining =>
    right
    cases frame <;> try (exact ⟨_, by simpa using Step.propagateUnwind { state with stack := rest } (.error error oldType) _ rfl⟩)
    case caller cells activation solution resultType =>
      cases typedFrame with
      | caller sameStorage proofSorted runtimeSorted savedActivation completion resultClosed callClosed scope writable schemes storage schemas =>
        rename_i inner outer callType
        cases control with
        | propagated matching errorTyped failureClosed callable =>
          rename_i success failure runtimeFailure
          rw [show inner.result = .result success failure from matching] at resultClosed
          cases successClosed : monoRuntime success with
          | none => simp [monoRuntime, successClosed, failureClosed] at resultClosed
          | some runtimeSuccess =>
            simp [monoRuntime, successClosed, failureClosed] at resultClosed
            subst resultType
            exact ⟨_, by simpa using Step.propagatedToCaller { state with stack := rest } error oldType runtimeSuccess runtimeFailure cells activation outer.runtimeSolution errorTyped⟩

theorem typed_machine_propagated_progress (state : Machine) (globals : Scope) (storage : BindingStorageMap)
    (types : CellTypes) (error : Value) (oldType : RuntimeType) (invariant : TypedMachine globals storage types state)
    (current : state.control = .propagated (.error error oldType)) :
    state.stack = [] ∨ ∃ next, Step state .internal next := by
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
  rw [current] at control
  rcases typed_propagated_stack_progress state input output state.stack error oldType control stack with empty | ⟨next, step⟩
  · exact Or.inl empty
  · have sameState : { state with control := .propagated (.error error oldType), stack := state.stack } = state := by
      cases state
      simp_all
    rw [sameState] at step
    exact Or.inr ⟨next, step⟩

end Xsh.Semantics.Connected
