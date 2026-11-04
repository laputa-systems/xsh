import Xsh.Semantics.Connected.SafetyValueBlockBranchEntry

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem typed_machine_empty_expression_branch (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (otherwise : Node)
    (typed : TypedMachine globals storage types { base with control := .expression (.conditional [] otherwise) }) :
    TypedMachine globals storage types { base with control := .expression otherwise } := by
  have step := Step.conditionalEmpty base otherwise
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | expression ground closed fresh =>
    rename_i context ty runtimeType
    cases ground with
    | conditional branches fallback valueClosed =>
      have newPending : PendingBindingsSafe { base with control := .expression otherwise } := by
        constructor
        · simpa [pendingMachineBinders, pendingControlBinders, nodeDeclaredBinders, expressionBranchBinders] using pending.unique
        · simpa [pendingMachineBinders, pendingControlBinders, nodeDeclaredBinders, expressionBranchBinders] using pending.fresh
      have fallbackFresh : FutureNodeBindingsFresh base.store context.activation otherwise := by
        simpa [FutureNodeBindingsFresh, nodeDeclaredBinders, expressionBranchBinders] using fresh
      have oldStack : StackTyped { base with control := .expression otherwise } (.expression context ty) output base.stack := by
        apply stack_typing_survives_with_pending step stack newPending
        · simpa [TypedDemand.context] using activation.symm
        · rfl
      refine ⟨.expression context ty, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
        ?_, inputClosed, outputClosed, .expression fallback closed fallbackFresh, oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_expression_branch (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (choice : Bool) (condition branch : Node) (rest : List (Node × Node)) (otherwise : Node)
    (typed : TypedMachine globals storage types
      { base with control := .value (.scalar (.boolean choice)), stack := .conditional ((condition, branch) :: rest) otherwise :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .expression (if choice then branch else .conditional rest otherwise) } := by
  have step : Step
      { base with control := .value (.scalar (.boolean choice)), stack := .conditional ((condition, branch) :: rest) otherwise :: base.stack }
      .internal { base with control := .expression (if choice then branch else .conditional rest otherwise) } := by
    cases choice with
    | false => exact Step.conditionalFalse base condition branch rest otherwise
    | true => exact Step.conditionalTrue base condition branch rest otherwise
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed callable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | conditional nonempty branches fallback =>
        rename_i context runtimeType result
        simp only [TypedDemand.context] at runtimeSolution activation storageEqual proofSorted
        cases branches with
        | cons conditionTyped branchTyped restTyped =>
          have subset : List.Sublist
              (pendingMachineBinders { base with control := .expression (if choice then branch else .conditional rest otherwise) })
              (pendingMachineBinders { base with control := .value (.scalar (.boolean choice)), stack := .conditional ((condition, branch) :: rest) otherwise :: base.stack }) := by
            cases choice with
            | false =>
              simp only [Bool.false_eq_true, ↓reduceIte, pendingMachineBinders, pendingControlBinders, pendingStackBinders,
                pendingFrameBinders, frameCallerActivation, nodeDeclaredBinders, List.map_nil, List.nil_append, List.map_append, List.append_assoc]
              exact List.sublist_append_right _ _
            | true =>
              simp only [↓reduceIte, pendingMachineBinders, pendingControlBinders, pendingStackBinders,
                pendingFrameBinders, frameCallerActivation, List.map_nil, List.nil_append, List.map_append, List.append_assoc]
              apply List.Sublist.append_left
              simp
          have newPending := pending_safe_of_sublist
            { base with control := .value (.scalar (.boolean choice)), stack := .conditional ((condition, branch) :: rest) otherwise :: base.stack }
            { base with control := .expression (if choice then branch else .conditional rest otherwise) } rfl subset pending
          have selectedFresh : FutureNodeBindingsFresh base.store context.activation (if choice then branch else .conditional rest otherwise) := by
            intro binder member
            apply newPending.fresh (context.activation, binder)
            simp [pendingMachineBinders, pendingControlBinders, activation, member]
          obtain ⟨resultType, resultClosed⟩ := show ∃ runtimeType, monoRuntime result = some runtimeType from middleClosed
          have selectedTyped : GroundNode context.solution context.runtimeSolution context.storage context.scope context.result
              (if choice then branch else .conditional rest otherwise) result := by
            cases choice with
            | false => exact .conditional restTyped fallback resultClosed
            | true => exact branchTyped
          have oldStack : StackTyped
              { base with control := .expression (if choice then branch else .conditional rest otherwise) }
              (.expression context result) output base.stack := by
            apply stack_typing_survives_with_pending step remaining newPending
            · simpa [TypedDemand.context] using activation.symm
            · rfl
          refine ⟨.expression context result, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, middleClosed, outputClosed, .expression selectedTyped resultClosed selectedFresh, oldStack, newPending⟩
          simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
