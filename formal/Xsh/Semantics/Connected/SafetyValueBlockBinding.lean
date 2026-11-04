import Xsh.Semantics.Connected.SafetyValueBlockPendingProof
import Xsh.Semantics.Connected.SafetyValueBlockScopeTransport
import Xsh.Semantics.Connected.SafetyValueBlockGround

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem binding_start_reservations_permute (base : Machine) (binder : BinderId) (kind : BindingKind)
    (initializer : Node) (rest : List StatementNode) :
    (pendingMachineBinders { base with control := .statements (.binding binder kind initializer :: rest) }).Perm
      (pendingMachineBinders { base with control := .expression initializer, stack := .bind binder kind rest :: base.stack }) := by
  simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
    frameCallerActivation, declaredBinders, List.map_cons, List.map_append, List.append_assoc]
  exact List.perm_middle.symm

theorem typed_machine_binding_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (binder : BinderId) (kind : BindingKind) (initializer : Node) (rest : List StatementNode)
    (typed : TypedMachine globals storage types
      { base with control := .statements (.binding binder kind initializer :: rest) }) :
    TypedMachine globals storage types
      { base with control := .expression initializer, stack := .bind binder kind rest :: base.stack } := by
  have step := Step.bindingStart base binder kind initializer rest
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
    | binding initializerTyped valid ordinary scopeFresh restTyped =>
      rename_i name raw
      let extended := { context with scope := ⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: context.scope }
      have newPending : PendingBindingsSafe
          { base with control := .expression initializer, stack := .bind binder kind rest :: base.stack } := by
        constructor
        · exact (binding_start_reservations_permute base binder kind initializer rest).nodup pending.unique
        · intro binding member
          apply pending.fresh binding
          exact (binding_start_reservations_permute base binder kind initializer rest).mem_iff.mpr member
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
          { base with control := .expression initializer, stack := .bind binder kind rest :: base.stack }
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
      refine ⟨.expression context (substitute context.solution raw), nextOutput, runtimeSolution,
        activation, storageEqual, proofSorted, runtimeSorted, ?_, ⟨initialRuntime, initialClosed⟩,
        nextClosed, .expression initializerTyped initialClosed initializerFresh,
        .cons (.bind valid ordinary scopeFresh binderFresh restTyped restFresh) extendedClosed oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
