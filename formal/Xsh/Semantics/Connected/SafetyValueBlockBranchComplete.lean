import Xsh.Semantics.Connected.SafetyValueBlockValues

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem typed_machine_tail_branch_complete (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value) (cells : ActiveCells)
    (typed : TypedMachine globals storage types
      { base with control := .completed value, stack := .continueBlock cells [] :: base.stack }) :
    TypedMachine globals storage types { base with control := .completed value, cells := cells } := by
  have step := Step.tailBranchComplete base value cells
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | completed valueTyped valueClosed accepted callable =>
    rename_i inner ty runtimeType
    simp only [TypedDemand.context] at runtimeSolution activation storageEqual proofSorted
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | continueBlock solutionSame runtimeSame storageSame resultSame activationSame completion scope writable schemes storageAgrees schemas restTyped fresh =>
        rename_i outer
        have newPending : PendingBindingsSafe { base with control := .completed value, cells := cells } := by
          constructor
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation, declaredBinders] using pending.unique
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation, declaredBinders] using pending.fresh
        have oldStack : StackTyped { base with control := .completed value, cells := cells } (.block outer) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context, activationSame] using activation.symm
          · rfl
        have sameCompletion : inner.completion = outer.completion := by simpa using completion
        have outerAccepts : CompletionAccepts outer.completion ty := by
          simpa [sameCompletion] using accepted
        refine ⟨.block outer, output, ?_, ?_, ?_, ?_, runtimeSorted, ?_, middleClosed, outputClosed,
          .completed valueTyped valueClosed outerAccepts callable, oldStack, newPending⟩
        · simpa [TypedDemand.context, runtimeSame] using runtimeSolution
        · simpa [TypedDemand.context, activationSame] using activation
        · simpa [TypedDemand.context, storageSame] using storageEqual
        · simpa [TypedDemand.context, solutionSame] using proofSorted
        · exact ⟨by simpa [ScopeStateAgrees, TypedDemand.context] using scope, writable, schemas, schemes, storageAgrees⟩

theorem typed_machine_non_tail_branch_complete (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value) (cells : ActiveCells) (next : StatementNode) (rest : List StatementNode)
    (typed : TypedMachine globals storage types
      { base with control := .completed value, stack := .continueBlock cells (next :: rest) :: base.stack }) :
    TypedMachine globals storage types { base with control := .statements (next :: rest), cells := cells } := by
  have step := Step.nonTailBranchComplete base value cells next rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | completed valueTyped valueClosed accepted callable =>
    rename_i inner ty runtimeType
    simp only [TypedDemand.context] at runtimeSolution activation storageEqual proofSorted
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | continueBlock solutionSame runtimeSame storageSame resultSame activationSame completion scope writable schemes storageAgrees schemas restTyped fresh =>
        rename_i outer
        have newPending : PendingBindingsSafe { base with control := .statements (next :: rest), cells := cells } := by
          constructor
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.unique
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.fresh
        have oldStack : StackTyped { base with control := .statements (next :: rest), cells := cells } (.block outer) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context, activationSame] using activation.symm
          · rfl
        have restGround := restTyped (by simp)
        refine ⟨.block outer, output, ?_, ?_, ?_, ?_, runtimeSorted, ?_, middleClosed, outputClosed,
          .statements restGround fresh, oldStack, newPending⟩
        · simpa [TypedDemand.context, runtimeSame] using runtimeSolution
        · simpa [TypedDemand.context, activationSame] using activation
        · simpa [TypedDemand.context, storageSame] using storageEqual
        · simpa [TypedDemand.context, solutionSame] using proofSorted
        · exact ⟨by simpa [ScopeStateAgrees, TypedDemand.context] using scope, writable, schemas, schemes, storageAgrees⟩

end Xsh.Semantics.Connected
