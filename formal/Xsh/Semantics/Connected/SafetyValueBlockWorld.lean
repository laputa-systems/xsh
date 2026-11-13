import Xsh.Semantics.Connected.SafetyValueBlockInvoke

namespace Xsh.Semantics.Connected

open Xsh.Semantics.Environments
open Xsh.Typing.Connected

theorem allocate_preserves_store_callables (functions : FunctionTable) (store next : Store)
    (cell : CellId) (mutability : Mutability) (ty : RuntimeType) (value : Value)
    (prior : StoreCallables functions store) (safe : ValueCallables functions value)
    (allocated : allocate store cell mutability ty value = some next) : StoreCallables functions next := by
  obtain ⟨current, atCell, _, _, valueEq⟩ :=
    allocate_accepted_at_cell (fun _ _ => inferInstance) store next cell mutability ty value allocated
  intro other entry present
  by_cases same : other = cell
  · subst other
    rw [atCell] at present
    cases present
    simpa [valueEq] using safe
  · rw [allocate_other_cell store next cell other mutability ty value same allocated] at present
    exact prior other entry present


theorem write_preserves_store_callables (functions : FunctionTable) (store next : Store)
    (cell : CellId) (value : Value) (prior : StoreCallables functions store)
    (safe : ValueCallables functions value) (written : write store cell value = .accepted next) :
    StoreCallables functions next := by
  obtain ⟨current, present, ⟨replacement, atCell, _, newValue, _⟩, unchanged⟩ :=
    write_accepted_replaces_only_its_cell (fun _ _ => inferInstance) store cell value next written
  intro other entry found
  by_cases same : other = cell
  · subst other
    rw [atCell] at found
    cases found
    rw [newValue]
    exact safe
  · rw [unchanged other same] at found
    exact prior other entry found

theorem parameter_binding_preserves_callables (functions : FunctionTable) (activation : Nat)
    (binders : List Xsh.Foundation.BinderId) (values : List Value) (types : RuntimeType)
    (store next : Store) (cells : ActiveCells) (prior : StoreCallables functions store)
    (safe : ∀ value ∈ values, ValueCallables functions value)
    (bound : bindParameters activation binders values types store = .ok (cells, next)) :
    StoreCallables functions next := by
  induction binders generalizing values types store next cells with
  | nil =>
    cases values with
    | nil =>
      cases types with
      | argumentsEmpty => cases bound; exact prior
      | _ => cases bound
    | cons _ _ => cases types <;> cases bound
  | cons binder rest ih =>
    cases values with
    | nil => cases types <;> cases bound
    | cons value values =>
      cases types with
      | argument ty types =>
        have headSafe : ValueCallables functions value := safe value (by simp)
        have tailSafe : ∀ candidate ∈ values, ValueCallables functions candidate := by
          intro candidate member
          exact safe candidate (List.mem_cons_of_mem _ member)
        by_cases typed : HasType ty value
        · cases allocated : allocate store ⟨activation, binder⟩ .immutable ty value with
          | none => simp [bindParameters, typed, allocated] at bound
          | some intermediate =>
            simp [bindParameters, typed, allocated, except_bind_ok] at bound
            obtain ⟨priorCells, finalStore, recursive, finished⟩ := bound
            split at finished
            · cases finished
            · cases finished
              exact ih values types intermediate next priorCells
                (allocate_preserves_store_callables functions store intermediate _ _ ty value prior headSafe allocated) tailSafe recursive
        · simp only [bindParameters, typed, ↓reduceIte] at bound
          cases bound
      | _ => cases bound

theorem parameter_binding_preserves_other_cell (activation : Nat)
    (binders : List Xsh.Foundation.BinderId) (values : List Value) (types : RuntimeType)
    (store next : Store) (cells : ActiveCells) (cell : CellId)
    (outside : cell.activation ≠ activation ∨ cell.binder ∉ binders)
    (bound : bindParameters activation binders values types store = .ok (cells, next)) :
    next cell = store cell := by
  induction binders generalizing values types store next cells with
  | nil =>
    cases values with
    | nil =>
      cases types with
      | argumentsEmpty => cases bound; rfl
      | _ => cases bound
    | cons _ _ => cases types <;> cases bound
  | cons binder rest ih =>
    cases values with
    | nil => cases types <;> cases bound
    | cons value values =>
      cases types with
      | argument ty types =>
        have different : cell ≠ ⟨activation, binder⟩ := by
          intro same
          cases outside with
          | inl activationDifferent => exact activationDifferent (congrArg CellId.activation same)
          | inr binderAbsent =>
            have sameBinder := congrArg CellId.binder same
            apply binderAbsent
            simp [sameBinder]
        have outsideTail : cell.activation ≠ activation ∨ cell.binder ∉ rest := by
          rcases outside with activationDifferent | binderAbsent
          · exact Or.inl activationDifferent
          · exact Or.inr (fun member => binderAbsent (List.mem_cons_of_mem _ member))
        by_cases typed : HasType ty value
        · cases allocated : allocate store ⟨activation, binder⟩ .immutable ty value with
          | none => simp [bindParameters, typed, allocated] at bound
          | some intermediate =>
            simp [bindParameters, typed, allocated, except_bind_ok] at bound
            obtain ⟨priorCells, finalStore, recursive, finished⟩ := bound
            split at finished
            · cases finished
            · cases finished
              exact (ih values types intermediate next priorCells outsideTail recursive).trans
                (allocate_other_cell store intermediate _ cell .immutable ty value different allocated)
        · simp only [bindParameters, typed, ↓reduceIte] at bound
          cases bound
      | _ => cases bound

theorem runtime_arguments_are_callable (functions : FunctionTable) (values : List Value) (types : RuntimeType)
    (typed : RuntimeArguments functions values types) : ∀ value ∈ values, ValueCallables functions value := by
  induction typed with
  | nil => intro value member; cases member
  | cons valueType callable rest ih =>
    intro value member
    rcases List.mem_cons.mp member with same | later
    · subst value
      exact callable
    · exact ih value later

def ControlValuesSafe (functions : FunctionTable) : Control → Prop
  | .value value => ValueCallables functions value
  | .invoke target values => ValueCallables functions target ∧ ∀ value ∈ values, ValueCallables functions value
  | _ => True

theorem typed_control_values_safe (state : Machine) (input : TypedDemand)
    (typed : ControlTyped state input state.control) : ControlValuesSafe state.functions state.control := by
  cases state
  cases typed <;> try trivial
  case mk.invoke =>
    constructor
    · assumption
    · exact runtime_arguments_are_callable _ _ _ (by assumption)

theorem step_preserves_store_callables {before after : Machine} {event : Label}
    (step : Step before event after) (prior : StoreCallables before.functions before.store)
    (safe : ControlValuesSafe before.functions before.control) : StoreCallables after.functions after.store := by
  cases step <;> try exact prior
  case bindingCommit state binder kind ty value rest next allocated =>
    exact allocate_preserves_store_callables state.functions state.store next _ _ ty value prior safe allocated
  case assignmentCommit state binder value rest cell next active written =>
    exact write_preserves_store_callables state.functions state.store next cell value prior safe written
  case assignmentRejected state binder value rest cell next active rejected =>
    have same := rejected_assignment_does_not_change_store state.store next cell value rejected
    subst next
    exact prior
  case callFunction state declaration arguments parameterTypes resultType captured typeArguments normalized function cells store found signature bound =>
    exact parameter_binding_preserves_callables state.functions state.nextActivation function.parameters arguments
      parameterTypes state.store store cells prior safe.2 bound

theorem step_preserves_function_table_certificate {before after : Machine} {event : Label}
    (globals : Scope) (storage : BindingStorageMap) (step : Step before event after)
    (certified : FunctionTableCertified globals storage before)
    (_schemes : SchemeCellsImmutable before.globals before.store)
    (live : ActiveCellsPresent before.globals before.store) :
    FunctionTableCertified globals storage after := by
  obtain ⟨sameFunctions, sameSchemas, sameGlobals⟩ := step_preserves_global_context step
  have layout := step_preserves_store_layout step
  have stable := step_preserves_immutable_cells step
  intro declaration function found captured typeArguments parameters result normalized sorted signature
  rw [sameFunctions] at found
  obtain ⟨extension, parameterScope, bodyResult, extensionSorted, runtimeSorted, proofSorted,
    globalValues, globalWritable, globalMutable, globalSchemas, parametersOrdinary, localsOrdinary, binders, unique,
    parameterTypesTyped, bodyTyped, resultClosed, bodyUnique, disjoint, propagation⟩ :=
      certified declaration function found captured typeArguments parameters result normalized sorted signature
  refine ⟨extension, parameterScope, bodyResult, extensionSorted, runtimeSorted, proofSorted, ?_, ?_, ?_, ?_,
    parametersOrdinary, localsOrdinary, binders, unique, parameterTypesTyped, bodyTyped, resultClosed,
    bodyUnique, disjoint, propagation⟩
  · rw [sameGlobals]
    exact scope_values_survive_writable_contract globals _ before.globals before.store after.store
      globalValues globalWritable layout stable
  · rw [sameGlobals]
    exact scope_writable_contract_survives globals _ before.globals before.store after.store live globalWritable layout
  · rw [sameGlobals]
    exact mutable_cells_survive_store_evolution globals _ before.globals before.store after.store layout globalMutable
  · rw [sameSchemas]
    exact globalSchemas

theorem stable_store_preserves_scheme_read (cells : ActiveCells) (before after : Store)
    (schemes : SchemeCellsImmutable cells before) (stable : ImmutableStable before after)
    (binder : Xsh.Foundation.BinderId) (declaration : Xsh.Syntax.Foundation.DeclarationId) (scheme : Scheme)
    (readBefore : Connected.read before cells binder = some (.schemeFunction declaration scheme)) :
    Connected.read after cells binder = some (.schemeFunction declaration scheme) := by
  obtain ⟨cell, entry, active, present, immutable⟩ := schemes binder declaration scheme readBefore
  have retained := stable cell entry present immutable
  simpa [Connected.read, active, present, retained] using readBefore

theorem stable_store_reflects_scheme_read (cells : ActiveCells) (before after : Store)
    (live : ActiveCellsPresent cells before) (schemes : SchemeCellsImmutable cells before)
    (layout : StoreEvolves before after) (stable : ImmutableStable before after)
    (binder : Xsh.Foundation.BinderId) (declaration : Xsh.Syntax.Foundation.DeclarationId) (scheme : Scheme)
    (readAfter : Connected.read after cells binder = some (.schemeFunction declaration scheme)) :
    Connected.read before cells binder = some (.schemeFunction declaration scheme) := by
  cases active : cells binder with
  | none => simp [Connected.read, active] at readAfter
  | some cell =>
    obtain ⟨old, oldAt⟩ := live binder cell active
    obtain ⟨next, nextAt, sameTag, _⟩ := layout cell old oldAt
    have nextValue : next.contents.value = .schemeFunction declaration scheme := by
      simpa [Connected.read, active, nextAt] using readAfter
    have nextTag : next.contents.typeTag = .schemeFunction := by
      have typed := next.contents.wellTyped
      rw [nextValue] at typed
      cases tag : next.contents.typeTag <;> simp_all [HasType, valueType, typeFits]
    have oldType : valueType old.contents.value = some .schemeFunction := by
      have typed := old.contents.wellTyped
      have oldTag : old.contents.typeTag = .schemeFunction := sameTag.symm.trans nextTag
      rw [oldTag] at typed
      cases actual : valueType old.contents.value with
      | none => simp [HasType, actual] at typed
      | some ty => cases ty <;> simp_all [HasType, typeFits]
    obtain ⟨oldDeclaration, oldScheme, oldValue⟩ := scheme_function_type_shape old.contents.value oldType
    have oldRead : Connected.read before cells binder = some (.schemeFunction oldDeclaration oldScheme) := by
      simp [Connected.read, active, oldAt, oldValue]
    obtain ⟨oldCell, oldEntry, oldActive, oldPresent, immutable⟩ := schemes binder oldDeclaration oldScheme oldRead
    rw [active] at oldActive
    cases oldActive
    rw [oldAt] at oldPresent
    cases oldPresent
    have retained := stable cell old oldAt immutable
    rw [retained] at nextAt
    cases nextAt
    simp [Connected.read, active, oldAt, nextValue]

theorem storage_agreement_survives_stable_store (cells : ActiveCells) (storage : BindingStorageMap)
    (before after : Store) (live : ActiveCellsPresent cells before) (schemes : SchemeCellsImmutable cells before)
    (agreement : StorageAgreement cells before storage) (layout : StoreEvolves before after)
    (stable : ImmutableStable before after) : StorageAgreement cells after storage := by
  constructor
  · intro binder declaration scheme found
    exact agreement.1 binder declaration scheme
      (stable_store_reflects_scheme_read cells before after live schemes layout stable binder declaration scheme found)
  · intro binder storedAsScheme
    obtain ⟨declaration, scheme, found⟩ := agreement.2 binder storedAsScheme
    exact ⟨declaration, scheme,
      stable_store_preserves_scheme_read cells before after schemes stable binder declaration scheme found⟩

theorem typed_machine_world_preserved {before after : Machine} {event : Label}
    (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (invariant : TypedMachine globals storage types before) (step : Step before event after) :
    MachineMemoryInvariant types after ∧ FunctionTableCertified globals storage after ∧
      DistinctActiveCells after.globals ∧ SchemeCellsImmutable after.globals after.store ∧
      StorageAgreement after.globals after.store storage ∧ StoreCallables after.functions after.store := by
  obtain ⟨sameFunctions, sameSchemas, sameGlobals⟩ := step_preserves_global_context step
  have layout := step_preserves_store_layout step
  have stable := step_preserves_immutable_cells step
  have controlSafe : ControlValuesSafe before.functions before.control := by
    obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
      inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
    exact typed_control_values_safe before input control
  refine ⟨step_preserves_machine_memory_invariant step invariant.memory,
    step_preserves_function_table_certificate globals storage step invariant.functions invariant.globalSchemes invariant.memory.globals.live,
    ?_, ?_, ?_, step_preserves_store_callables step invariant.storedValues controlSafe⟩
  · rw [sameGlobals]
    exact invariant.globalDistinct
  · rw [sameGlobals]
    exact scheme_cells_survive_stable_store before.globals before.store after.store
      invariant.memory.globals.live invariant.globalSchemes layout stable
  · rw [sameGlobals]
    exact storage_agreement_survives_stable_store before.globals storage before.store after.store
      invariant.memory.globals.live invariant.globalSchemes invariant.globalStorageAgrees layout stable

end Xsh.Semantics.Connected
