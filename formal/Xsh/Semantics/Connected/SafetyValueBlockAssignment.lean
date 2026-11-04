import Xsh.Semantics.Connected.SafetyValueBlockBinding

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected
open Xsh.Semantics.Environments

theorem assignment_commit_preserves_pending (base : Machine) (binder : BinderId) (value : Value)
    (rest : List StatementNode) (cell : CellId) (next : Store)
    (written : write base.store cell value = .accepted next)
    (pending : PendingBindingsSafe { base with control := .value value, stack := .assign binder rest :: base.stack }) :
    PendingBindingsSafe { base with control := .statements rest, store := next } := by
  have shape : pendingMachineBinders { base with control := .value value, stack := .assign binder rest :: base.stack } =
      pendingMachineBinders { base with control := .statements rest, store := next } := by
    simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders, frameCallerActivation]
  obtain ⟨current, present, _, unchanged⟩ :=
    write_accepted_replaces_only_its_cell (fun _ _ => inferInstance) base.store cell value next written
  constructor
  · simpa [shape] using pending.unique
  · intro binding member
    have oldFresh : base.store ⟨binding.1, binding.2⟩ = none := by
      apply pending.fresh binding
      rw [shape]
      exact member
    have different : CellId.mk binding.1 binding.2 ≠ cell := by
      intro equal
      rw [equal, present] at oldFresh
      cases oldFresh
    change next ⟨binding.1, binding.2⟩ = none
    rw [unchanged _ different]
    exact oldFresh

theorem typed_machine_assignment_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
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
    | assignment found mutable valueTyped restTyped =>
      rename_i name binding
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
        .expression valueTyped valueClosed valueFresh, .cons (.assign found mutable restTyped restFresh) inputClosed oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_assignment_commit (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (binder : BinderId) (value : Value) (rest : List StatementNode)
    (cell : CellId) (next : Store) (activeCell : base.cells binder = some cell)
    (written : write base.store cell value = .accepted next)
    (typed : TypedMachine globals storage types
      { base with control := .value value, stack := .assign binder rest :: base.stack }) :
    TypedMachine globals storage types { base with control := .statements rest, store := next } := by
  have step := Step.assignmentCommit base binder value rest cell next activeCell written
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed callable =>
    rename_i context ty runtimeType
    simp only [TypedDemand.context] at activation
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | assign found mutable restTyped restFresh =>
        rename_i name binding
        have newPending := assignment_commit_preserves_pending base binding.binder value rest cell next written pending
        obtain ⟨scopeAgrees, schemasAgree, schemes, storageAgree⟩ := active
        obtain ⟨scopeAfter, schemesAfter, storageAfter, schemasAfter⟩ :=
          saved_frame_scope_survives_step step context base.cells
            (by simpa [ScopeStateAgrees, TypedDemand.context] using scopeAgrees) schemes storageAgree schemasAgree
        have freshRest : FutureBindingsFresh next context.activation rest := by
          intro nextBinder member
          apply newPending.fresh (context.activation, nextBinder)
          simp [pendingMachineBinders, pendingControlBinders, activation, member]
        have oldStack : StackTyped { base with control := .statements rest, store := next }
            (.block context) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context] using activation.symm
          · rfl
        refine ⟨.block context, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
          ?_, middleClosed, outputClosed, .statements restTyped freshRest, oldStack, newPending⟩
        exact ⟨by simpa [ScopeStateAgrees, TypedDemand.context] using scopeAfter, schemasAfter, schemesAfter, storageAfter⟩

end Xsh.Semantics.Connected
