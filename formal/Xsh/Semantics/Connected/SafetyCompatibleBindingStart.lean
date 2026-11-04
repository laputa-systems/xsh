import Xsh.Semantics.Connected.SafetyValueBlockPendingProof
import Xsh.Semantics.Connected.SafetyValueBlockScopeTransport
import Xsh.Semantics.Connected.SafetyValueBlockGround

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected
open Xsh.Semantics.Environments

theorem writable_contract_after_binding (state : Machine) (scope : Scope) (name : NameId)
    (binder : BinderId) (kind : BindingKind) (raw : MonoType) (value : Value)
    (runtimeType : RuntimeType) (next : Store)
    (contract : ScopeWritableContract scope state.solution state.cells state.store)
    (live : ActiveCellsPresent state.cells state.store)
    (binderFresh : ∀ binding ∈ scope, binding.binder ≠ binder)
    (cellFresh : state.store ⟨state.activation, binder⟩ = none)
    (closed : monoRuntime (substitute state.solution raw) = some runtimeType)
    (typed : HasType runtimeType value)
    (allocated : allocate state.store ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) runtimeType value = some next) :
    ScopeWritableContract (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: scope) state.solution
      (bindActive state.cells binder ⟨state.activation, binder⟩) next := by
  obtain ⟨built, entry, builtAt, present, _, tag, _⟩ :=
    allocate_fresh_typed_cell state.store ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value runtimeType cellFresh typed
  rw [allocated] at builtAt
  cases builtAt
  intro sought binding cell current found notSchema active currentAt mutable replacement replacementTyped
  by_cases same : name = sought
  · subst sought
    simp at found
    cases found
    simp [bindActive] at active
    subst cell
    rw [present] at currentAt
    cases currentAt
    apply monomorphic_value_agrees state.solution name binder kind raw replacement runtimeType
    · exact mono_typed_value_is_ordinary replacement (substitute state.solution raw) runtimeType closed
        (by simpa [tag] using replacementTyped)
    · exact closed
    · simpa [tag] using replacementTyped
  · have tailFound : scope.find? (fun item => decide (item.name = sought)) = some binding := by
      simpa [same] using found
    have distinct : binding.binder ≠ binder := binderFresh binding (List.mem_of_find?_eq_some tailFound)
    have oldActive : state.cells binding.binder = some cell := by simpa [bindActive, distinct] using active
    obtain ⟨old, oldAt⟩ := live binding.binder cell oldActive
    have oldRetained := allocate_preserves_existing state.store next ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) runtimeType value allocated cell old oldAt
    rw [currentAt] at oldRetained
    cases oldRetained
    exact contract sought binding cell current tailFound notSchema oldActive oldAt mutable replacement replacementTyped

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

theorem compatible_assignment_commit_preserves_pending (base : Machine) (binder : BinderId) (value : Value)
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

theorem compatible_machine_assignment_commit (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
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
      | assign compatible found mutable restTyped restFresh =>
        rename_i name binding
        have newPending := compatible_assignment_commit_preserves_pending base binding.binder value rest cell next written pending
        obtain ⟨scopeAgrees, writable, schemasAgree, schemes, storageAgree⟩ := active
        obtain ⟨scopeAfter, writableAfter, schemesAfter, storageAfter, schemasAfter⟩ :=
          saved_frame_scope_survives_step step context base.cells
            (by simpa [ScopeStateAgrees, TypedDemand.context] using scopeAgrees) writable schemes storageAgree schemasAgree
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
        exact ⟨by simpa [ScopeStateAgrees, TypedDemand.context] using scopeAfter, writableAfter, schemasAfter, schemesAfter, storageAfter⟩

end Xsh.Semantics.Connected
