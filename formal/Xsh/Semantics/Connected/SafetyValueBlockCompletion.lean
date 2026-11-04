import Xsh.Semantics.Connected.SafetyBindingPreservation

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
      | valueBlock solutionSame runtimeSame storageSame resultSame activationSame completion scope writable schemes storageAgrees schemas =>
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
        · exact ⟨by simpa [ScopeStateAgrees, TypedDemand.context] using scope, writable, schemas, schemes, storageAgrees⟩

theorem typed_machine_value_block_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (completion : MonoType) (body : List StatementNode)
    (typed : TypedMachine globals storage types { base with control := .expression (.valueBlock completion body) }) :
    TypedMachine globals storage types
      { base with control := .statements body, stack := .valueBlock base.cells :: base.stack } := by
  have step := Step.valueBlockStart base completion body
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | expression ground closed fresh =>
    rename_i context ty runtimeType
    simp only [TypedDemand.context] at runtimeSolution activation storageEqual proofSorted
    cases ground with
    | valueBlock bodyTyped valueClosed =>
      let inner : TypingContext := { context with completion := some (substitute context.solution completion) }
      have newPending : PendingBindingsSafe
          { base with control := .statements body, stack := .valueBlock base.cells :: base.stack } := by
        constructor
        · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, nodeDeclaredBinders] using pending.unique
        · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, nodeDeclaredBinders] using pending.fresh
      have bodyFresh : FutureBindingsFresh base.store context.activation body := by
        simpa [FutureNodeBindingsFresh, FutureBindingsFresh, nodeDeclaredBinders] using fresh
      obtain ⟨scope, writable, schemas, schemes, storageAgrees⟩ := active
      have oldStack : StackTyped { base with control := .statements body, stack := .valueBlock base.cells :: base.stack }
          (.expression context (substitute context.solution completion)) output base.stack := by
        apply stack_typing_survives_pending step stack
        intro binding member
        apply newPending.fresh binding
        simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
          frameCallerActivation, List.map_nil, List.nil_append, List.mem_append]
        right
        simpa [TypedDemand.context, activation] using member
      have frame : FrameTyped { base with control := .statements body, stack := .valueBlock base.cells :: base.stack }
          (.block inner) (.expression context (substitute context.solution completion)) (.valueBlock base.cells) := by
        exact .valueBlock rfl rfl rfl rfl rfl rfl (by simpa [ScopeStateAgrees, TypedDemand.context] using scope)
          writable schemes storageAgrees schemas
      have innerClosed : DemandClosed (.block inner) := by
        refine ⟨context.resultClosed, ?_⟩
        intro ty equal
        simp [inner] at equal
        subst ty
        exact ⟨_, valueClosed⟩
      refine ⟨.block inner, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
        ?_, innerClosed, outputClosed, .statements bodyTyped bodyFresh, .cons frame inputClosed oldStack, newPending⟩
      exact ⟨by simpa [inner, ScopeStateAgrees, TypedDemand.context] using scope, writable, schemas, schemes, storageAgrees⟩

end Xsh.Semantics.Connected
