import Xsh.Semantics.Connected.SafetyValueBlockPendingProof
import Xsh.Semantics.Connected.SafetyValueBlockScopeTransport
import Xsh.Semantics.Connected.SafetyValueBlockGround

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem compatible_binding_start_reservations_permute (base : Machine) (binder : BinderId) (kind : BindingKind)
    (rawStorage : MonoType) (storageType : RuntimeType) (initializer : Node) (rest : List StatementNode) :
    (pendingMachineBinders { base with control := .statements (.binding binder kind rawStorage initializer :: rest) }).Perm
      (pendingMachineBinders { base with control := .expression initializer, stack := .bind binder kind storageType rest :: base.stack }) := by
  simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
    frameCallerActivation, declaredBinders, List.map_cons, List.map_append, List.append_assoc]
  exact List.perm_middle.symm

theorem compatible_machine_binding_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (binder : BinderId) (kind : BindingKind) (rawStorage : MonoType)
    (storageType : RuntimeType) (initializer : Node) (rest : List StatementNode)
    (storageClosed : monoRuntime (substitute base.solution rawStorage) = some storageType)
    (typed : TypedMachine globals storage types
      { base with control := .statements (.binding binder kind rawStorage initializer :: rest) }) :
    TypedMachine globals storage types
      { base with control := .expression initializer, stack := .bind binder kind storageType rest :: base.stack } := by
  have step := Step.bindingStart base binder kind rawStorage storageType initializer rest storageClosed
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | statements ground fresh =>
    rename_i context
    simp only [TypedDemand.context] at activation
    cases ground with
    | binding initializerTyped compatible closed runtimeClosed valid ordinary scopeFresh restTyped =>
      rename_i name initializerType certifiedStorage
      have sameStorage : certifiedStorage = storageType := by
        simp only [TypedDemand.context] at runtimeSolution
        rw [runtimeSolution] at runtimeClosed
        exact Option.some.inj (runtimeClosed.symm.trans storageClosed)
      subst certifiedStorage
      let raw := rawStorage
      let extended := { context with scope := ⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: context.scope }
      have newPending : PendingBindingsSafe
          { base with control := .expression initializer, stack := .bind binder kind storageType rest :: base.stack } := by
        constructor
        · exact (compatible_binding_start_reservations_permute base binder kind rawStorage storageType initializer rest).nodup pending.unique
        · intro binding member
          apply pending.fresh binding
          exact (compatible_binding_start_reservations_permute base binder kind rawStorage storageType initializer rest).mem_iff.mpr member
      have initializerFresh : FutureNodeBindingsFresh base.store context.activation initializer := by
        intro next member
        apply fresh next
        simp [declaredBinders, member]
      have binderFresh : base.store ⟨context.activation, binder⟩ = none := by
        apply fresh binder
        simp [declaredBinders]
      have restFresh : FutureBindingsFresh base.store context.activation rest := by
        intro next member
        apply fresh next
        simp [declaredBinders, member]
      obtain ⟨nextOutput, nextClosed, reindexed⟩ :=
        stack_block_scope_reindex stack outputClosed extended.scope
      have oldStack : StackTyped
          { base with control := .expression initializer, stack := .bind binder kind storageType rest :: base.stack }
          (.block extended) nextOutput base.stack := by
        apply stack_typing_survives_pending step reindexed
        intro binding member
        apply newPending.fresh binding
        simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
        right
        right
        simpa [activation, extended, frameCallerActivation, TypedDemand.context] using member
      have extendedClosed : DemandClosed (.block extended) := by
        simpa [DemandClosed, extended] using inputClosed
      obtain ⟨initialRuntime, initialClosed⟩ := ground_node_is_closed initializerTyped
      refine ⟨.expression context initializerType, nextOutput, runtimeSolution,
        activation, storageEqual, proofSorted, runtimeSorted, ?_, ⟨initialRuntime, initialClosed⟩,
        nextClosed, .expression initializerTyped initialClosed initializerFresh,
        .cons (.bind compatible closed valid ordinary scopeFresh binderFresh restTyped restFresh) extendedClosed oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem compatible_machine_assignment_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (binder : BinderId) (value : Node) (rest : List StatementNode)
    (typed : TypedMachine globals storage types
      { base with control := .statements (.assignment binder value :: rest) }) :
    TypedMachine globals storage types
      { base with control := .expression value, stack := .assign binder rest :: base.stack } := by
  have step := Step.assignmentStart base binder value rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | statements ground fresh =>
    rename_i context
    simp only [TypedDemand.context] at activation
    cases ground with
    | assignment found mutable valueTyped compatible restTyped =>
      rename_i name binding valueType
      have newPending : PendingBindingsSafe
          { base with control := .expression value, stack := .assign binding.binder rest :: base.stack } := by
        constructor
        · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, declaredBinders, List.map_append, List.append_assoc] using pending.unique
        · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, declaredBinders, List.map_append, List.append_assoc] using pending.fresh
      have valueFresh : FutureNodeBindingsFresh base.store context.activation value := by
        intro next member
        apply fresh next
        simp [declaredBinders, member]
      have restFresh : FutureBindingsFresh base.store context.activation rest := by
        intro next member
        apply fresh next
        simp [declaredBinders, member]
      have oldStack : StackTyped
          { base with control := .expression value, stack := .assign binding.binder rest :: base.stack }
          (.block context) output base.stack := by
        apply stack_typing_survives_pending step stack
        intro binding member
        apply newPending.fresh binding
        simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
        right
        right
        simpa [activation, frameCallerActivation, TypedDemand.context] using member
      obtain ⟨valueRuntime, valueClosed⟩ := ground_node_is_closed valueTyped
      refine ⟨.expression context _, output, runtimeSolution, activation, storageEqual,
        proofSorted, runtimeSorted, ?_, ⟨valueRuntime, valueClosed⟩, outputClosed,
        .expression valueTyped valueClosed valueFresh, .cons (.assign compatible found mutable restTyped restFresh) inputClosed oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
