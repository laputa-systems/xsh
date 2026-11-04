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

theorem binding_commit_preserves_pending (base : Machine) (binder : BinderId) (kind : BindingKind)
    (value : Value) (rest : List StatementNode) (next : Store)
    (allocated : allocate base.store ⟨base.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value = some next)
    (pending : PendingBindingsSafe { base with control := .value value, stack := .bind binder kind rest :: base.stack }) :
    PendingBindingsSafe { base with control := .statements rest, store := next, cells := bindActive base.cells binder ⟨base.activation, binder⟩ } := by
  have shape : pendingMachineBinders { base with control := .value value, stack := .bind binder kind rest :: base.stack } =
      (base.activation, binder) :: pendingMachineBinders { base with control := .statements rest, store := next, cells := bindActive base.cells binder ⟨base.activation, binder⟩ } := by
    simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders, frameCallerActivation]
  have unique := pending.unique
  rw [shape, List.nodup_cons] at unique
  constructor
  · exact unique.2
  · intro binding member
    have notAllocated : binding ≠ (base.activation, binder) := by
      intro equal
      subst binding
      exact unique.1 member
    have cellDifferent : Xsh.Semantics.Environments.CellId.mk binding.1 binding.2 ≠ ⟨base.activation, binder⟩ := by
      intro equal
      apply notAllocated
      cases binding with
      | mk activation binder =>
        cases equal
        rfl
    change next ⟨binding.1, binding.2⟩ = none
    rw [allocate_other_cell base.store next _ _ _ value cellDifferent allocated]
    apply pending.fresh binding
    rw [shape]
    exact List.mem_cons_of_mem _ member

theorem binding_commit_preserves_other_reads (base : Machine) (binder other : BinderId) (kind : BindingKind)
    (value : Value) (next : Store) (different : other ≠ binder)
    (allocated : allocate base.store ⟨base.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value = some next)
    (live : ActiveCellsPresent base.cells base.store) :
    Connected.read next (bindActive base.cells binder ⟨base.activation, binder⟩) other =
      Connected.read base.store base.cells other := by
  cases active : base.cells other with
  | none => simp [Connected.read, bindActive, different, active]
  | some cell =>
    obtain ⟨entry, oldAt⟩ := live other cell active
    have nextAt := allocate_preserves_existing base.store next ⟨base.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value allocated cell entry oldAt
    simp [Connected.read, bindActive, different, active, oldAt, nextAt]

theorem binding_commit_preserves_scheme_state (base : Machine) (binder : BinderId) (kind : BindingKind)
    (value : Value) (runtimeType : RuntimeType) (next : Store) (storage : BindingStorageMap)
    (allocated : allocate base.store ⟨base.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value = some next)
    (fresh : base.store ⟨base.activation, binder⟩ = none)
    (typed : valueType value = some runtimeType) (ordinary : ordinaryValue value = true)
    (ordinaryStorage : storage binder = .ordinary) (live : ActiveCellsPresent base.cells base.store)
    (schemes : SchemeCellsImmutable base.cells base.store) (agreement : StorageAgreement base.cells base.store storage) :
    SchemeCellsImmutable (bindActive base.cells binder ⟨base.activation, binder⟩) next ∧
      StorageAgreement (bindActive base.cells binder ⟨base.activation, binder⟩) next storage := by
  obtain ⟨built, entry, builtAt, present, _, _, entryValue⟩ :=
    allocate_fresh_typed_cell base.store ⟨base.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value runtimeType fresh typed
  rw [allocated] at builtAt
  cases builtAt
  have newRead : Connected.read next (bindActive base.cells binder ⟨base.activation, binder⟩) binder = some value := by
    simp [Connected.read, bindActive, present, entryValue]
  have unchanged : ∀ other, other ≠ binder →
      Connected.read next (bindActive base.cells binder ⟨base.activation, binder⟩) other = Connected.read base.store base.cells other := by
    intro other different
    exact binding_commit_preserves_other_reads base binder other kind value next different allocated live
  constructor
  · intro other declaration scheme found
    by_cases same : other = binder
    · subst other
      rw [newRead] at found
      cases found
      simp [ordinaryValue] at ordinary
    · have oldRead := found
      rw [unchanged other same] at oldRead
      obtain ⟨cell, old, active, oldAt, immutable⟩ := schemes other declaration scheme oldRead
      have nextAt := allocate_preserves_existing base.store next ⟨base.activation, binder⟩
        (if kind = .mutable then .mutable else .immutable) value allocated cell old oldAt
      exact ⟨cell, old, by simpa [bindActive, same] using active, nextAt, immutable⟩
  · constructor
    · intro other declaration scheme found
      by_cases same : other = binder
      · subst other
        rw [newRead] at found
        cases found
        simp [ordinaryValue] at ordinary
      · rw [unchanged other same] at found
        exact agreement.1 other declaration scheme found
    · intro other isScheme
      have different : other ≠ binder := by
        intro same
        subst other
        rw [ordinaryStorage] at isScheme
        cases isScheme
      obtain ⟨declaration, scheme, oldRead⟩ := agreement.2 other isScheme
      exact ⟨declaration, scheme, by simpa [unchanged other different] using oldRead⟩

theorem schema_matches_after_binding (scope : Scope) (solution : Substitution) (schemas : SchemaTable)
    (name : NameId) (binder : BinderId) (kind : BindingKind) (raw : MonoType)
    (valid : kind = .immutable ∨ kind = .mutable) (schemasAgree : SchemaMatches scope solution schemas) :
    SchemaMatches (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: scope) solution schemas := by
  intro sought binding identity row found isSchema body
  by_cases same : name = sought
  · simp [same] at found
    subst binding
    rcases valid with immutable | mutable <;> simp_all
  · have tailFound : scope.find? (fun item => decide (item.name = sought)) = some binding := by
      simpa [same] using found
    exact schemasAgree sought binding identity row tailFound isSchema body

theorem binding_commit_preserves_scope_with_allocation (state : Machine) (scope : Scope)
    (name : NameId) (binder : BinderId) (kind : BindingKind) (raw : MonoType)
    (value : Value) (runtimeType : RuntimeType) (rest : List StatementNode) (next : Store)
    (agrees : ScopeStateAgrees scope state) (binderFresh : ∀ binding ∈ scope, binding.binder ≠ binder)
    (cellFresh : state.store ⟨state.activation, binder⟩ = none) (ordinary : ordinaryValue value = true)
    (closed : monoRuntime (substitute state.solution raw) = some runtimeType) (typed : valueType value = some runtimeType)
    (allocated : allocate state.store ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value = some next) :
    ScopeStateAgrees (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: scope)
      { state with control := .statements rest, store := next, cells := bindActive state.cells binder ⟨state.activation, binder⟩ } := by
  obtain ⟨unique, values, mutableCells, distinct, activePresent⟩ := agrees
  obtain ⟨built, builtAllocation, _, valuesAfter⟩ := binding_commit_preserves_scope_values state scope name binder kind raw
    value runtimeType rest values binderFresh cellFresh ordinary closed typed
  rw [allocated] at builtAllocation
  cases builtAllocation
  obtain ⟨constructed, current, built, present, _, _, _⟩ := allocate_fresh_typed_cell state.store
    ⟨state.activation, binder⟩ (if kind = .mutable then .mutable else .immutable) value runtimeType cellFresh typed
  rw [allocated] at built
  cases built
  have newUnique : ((⟨name, binder, kind, ⟨[], raw, []⟩⟩ : Binding) :: scope).map Binding.binder |>.Nodup := by
    simp only [List.map_cons, List.nodup_cons]
    refine ⟨?_, unique⟩
    intro member
    obtain ⟨old, oldIn, same⟩ := List.mem_map.mp member
    exact binderFresh old oldIn same
  refine ⟨newUnique, valuesAfter, ?_, ?_, ?_⟩
  · exact binding_commit_preserves_mutable_cells state scope name binder kind raw value runtimeType next
      mutableCells binderFresh cellFresh closed typed allocated
  · exact fresh_binding_preserves_distinct_cells state.cells state.store binder ⟨state.activation, binder⟩ distinct activePresent cellFresh
  · exact fresh_binding_preserves_active_cells state.cells state.store next binder ⟨state.activation, binder⟩
      activePresent (allocate_preserves_layout state.store next _ _ value allocated) ⟨current, present⟩

theorem typed_machine_binding_commit (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (binder : BinderId) (kind : BindingKind) (value : Value) (rest : List StatementNode)
    (next : Store) (allocated : allocate base.store ⟨base.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value = some next)
    (typed : TypedMachine globals storage types
      { base with control := .value value, stack := .bind binder kind rest :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .statements rest, store := next, cells := bindActive base.cells binder ⟨base.activation, binder⟩ } := by
  have step := Step.bindingCommit base binder kind value rest next allocated
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
      | bind valid ordinary scopeFresh binderFresh restTyped restFresh =>
        rename_i name raw
        have newPending := binding_commit_preserves_pending base binder kind value rest next allocated pending
        have ordinaryValue := mono_typed_value_is_ordinary value _ runtimeType valueClosed valueTyped
        obtain ⟨scopeAgrees, schemasAgree, schemes, storageAgree⟩ := active
        have baseFresh : base.store ⟨base.activation, binder⟩ = none := by simpa [activation] using binderFresh
        obtain ⟨newSchemes, newStorage⟩ := binding_commit_preserves_scheme_state base binder kind value runtimeType next
          context.storage allocated baseFresh valueTyped ordinaryValue ordinary (by simpa [ScopeStateAgrees] using scopeAgrees.2.2.2.2)
          schemes storageAgree
        have scopeAfter := binding_commit_preserves_scope_with_allocation
          { base with solution := context.solution } context.scope name binder kind raw value runtimeType rest next
          (by simpa [ScopeStateAgrees, TypedDemand.context] using scopeAgrees) scopeFresh (by simpa [activation] using binderFresh)
          ordinaryValue valueClosed valueTyped allocated
        have schemasAfter := schema_matches_after_binding context.scope context.solution base.schemas name binder kind raw valid schemasAgree
        have freshRest : FutureBindingsFresh next context.activation rest := by
          intro nextBinder member
          apply newPending.fresh (context.activation, nextBinder)
          simp [pendingMachineBinders, pendingControlBinders, activation, member]
        have oldStack : StackTyped
            { base with control := .statements rest, store := next, cells := bindActive base.cells binder ⟨base.activation, binder⟩ }
            (.block { context with scope := ⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: context.scope }) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context] using activation.symm
          · rfl
        refine ⟨.block { context with scope := ⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: context.scope }, output,
          runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, ?_, middleClosed, outputClosed,
          .statements restTyped freshRest, oldStack, newPending⟩
        exact ⟨by simpa [ScopeStateAgrees, activation, TypedDemand.context] using scopeAfter, schemasAfter, newSchemes, newStorage⟩

end Xsh.Semantics.Connected
