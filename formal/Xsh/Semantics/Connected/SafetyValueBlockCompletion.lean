import Xsh.Semantics.Connected.SafetyValueBlockAssignment

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem typed_machine_value_block_complete (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value) (cells : ActiveCells)
    (typed : TypedMachine globals storage types
      { base with control := .completed value, stack := .valueBlock cells :: base.stack }) :
    TypedMachine globals storage types { base with control := .value value, cells := cells } := by
  have step := Step.valueBlockComplete base value cells
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | completed valueTyped valueClosed accepted callable =>
    rename_i inner ty runtimeType
    simp only [TypedDemand.context] at runtimeSolution activation storageEqual
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | valueBlock solutionSame runtimeSame storageSame resultSame activationSame completion scope schemes storageAgrees schemas =>
        rename_i outer blockType
        have sameType : ty = blockType := by
          rcases accepted with absent | present
          · rw [completion] at absent
            cases absent
          · rw [completion] at present
            exact (Option.some.inj present).symm
        subst ty
        have newPending : PendingBindingsSafe { base with control := .value value, cells := cells } := by
          constructor
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.unique
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.fresh
        have oldStack : StackTyped { base with control := .value value, cells := cells }
            (.expression outer blockType) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context, activationSame] using activation.symm
          · rfl
        refine ⟨.expression outer blockType, output, ?_, ?_, ?_, ?_, runtimeSorted,
          ?_, middleClosed, outputClosed, .value valueTyped valueClosed callable, oldStack, newPending⟩
        · simpa [runtimeSame, TypedDemand.context] using runtimeSolution
        · simpa [activationSame, TypedDemand.context] using activation
        · simpa [storageSame, TypedDemand.context] using storageEqual
        · simpa [solutionSame, TypedDemand.context] using proofSorted
        · exact ⟨by simpa [ScopeStateAgrees, TypedDemand.context] using scope, schemas, schemes, storageAgrees⟩

end Xsh.Semantics.Connected
