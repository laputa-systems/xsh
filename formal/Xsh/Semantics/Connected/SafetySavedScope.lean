import Xsh.Semantics.Connected.SafetyScopeStorage

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Semantics.Environments
open Xsh.Typing.Connected

def SchemeCellsImmutable (cells : ActiveCells) (store : Store) : Prop :=
  ∀ binder declaration scheme, Connected.read store cells binder = some (.schemeFunction declaration scheme) →
    ∃ cell entry, cells binder = some cell ∧ store cell = some entry ∧ entry.mutability = .immutable

theorem scheme_function_type_shape (value : Value) (typed : valueType value = some .schemeFunction) :
    ∃ declaration scheme, value = .schemeFunction declaration scheme := by
  cases value with
  | schemeFunction declaration scheme => exact ⟨declaration, scheme, rfl⟩
  | scalar value => cases value <;> simp [valueType] at typed
  | record fields => simp [valueType] at typed
  | ok value failure | error value failure => simp [valueType] at typed
  | nominal identity base value => cases inner : valueType value <;> simp [valueType, inner] at typed
  | _ => simp [valueType] at typed

theorem ordinary_value_survives_matching_type (before after : Value)
    (ordinary : ordinaryValue before = true) (sameType : valueType after = valueType before) :
    ordinaryValue after = true := by
  cases after with
  | schemeFunction declaration scheme =>
    have typed : valueType before = some .schemeFunction := sameType.symm
    obtain ⟨oldDeclaration, oldScheme, same⟩ := scheme_function_type_shape before typed
    subst before
    simp [ordinaryValue] at ordinary
  | _ => rfl

theorem scheme_cells_survive_stable_store (cells : ActiveCells) (before after : Store)
    (live : ActiveCellsPresent cells before) (schemes : SchemeCellsImmutable cells before)
    (layout : StoreEvolves before after) (stable : ImmutableStable before after) :
    SchemeCellsImmutable cells after := by
  intro binder declaration scheme readAfter
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
      | some ty =>
        cases ty <;> simp_all [HasType, typeFits]
    obtain ⟨oldDeclaration, oldScheme, oldValue⟩ := scheme_function_type_shape old.contents.value oldType
    have oldRead : Connected.read before cells binder = some (.schemeFunction oldDeclaration oldScheme) := by
      simp [Connected.read, active, oldAt, oldValue]
    obtain ⟨oldCell, oldEntry, oldActive, oldPresent, immutable⟩ := schemes binder oldDeclaration oldScheme oldRead
    rw [active] at oldActive
    cases oldActive
    rw [oldAt] at oldPresent
    cases oldPresent
    exact ⟨cell, old, rfl, stable cell old oldAt immutable, immutable⟩

theorem saved_scope_survives_execution (before after : Machine) (events : List Label)
    (scope : Scope) (solution : Substitution) (cells : ActiveCells)
    (execution : Execution before events after)
    (agrees : ScopeStateAgrees scope { before with solution := solution, cells := cells })
    (schemes : SchemeCellsImmutable cells before.store)
    (contract : ScopeWritableContract scope solution cells before.store) :
    ScopeStateAgrees scope { after with solution := solution, cells := cells } ∧
      SchemeCellsImmutable cells after.store := by
  obtain ⟨unique, values, mutableCells, distinct, present⟩ := agrees
  have layout := execution_preserves_store_layout execution
  have stable := execution_preserves_immutable_cells execution
  refine ⟨?_, scheme_cells_survive_stable_store cells before.store after.store present schemes layout stable⟩
  exact ⟨unique, scope_values_survive_writable_contract scope solution cells before.store after.store values contract layout stable,
    mutable_cells_survive_store_evolution scope solution cells before.store after.store layout mutableCells,
    distinct, active_cells_survive_store_evolution cells before.store after.store present layout⟩

theorem parameter_binder_reads_ordinary (solution : Substitution) (activation : Nat)
    (scope : Scope) (values : List Value) (types : RuntimeType) (cells : ActiveCells) (store : Store)
    (typed : ParameterBindingsTyped solution scope values types)
    (agrees : ParameterCellsAgree activation cells store (scope.map Binding.binder) values types)
    (binder : BinderId) (member : binder ∈ scope.map Binding.binder) :
    ∃ value, Connected.read store cells binder = some value ∧ ordinaryValue value = true := by
  induction typed with
  | nil => simp at member
  | @cons name first raw value values ty types scope ordinary closed valueTyped tail ih =>
    cases agrees with
    | cons active present remaining =>
      simp only [List.map_cons, List.mem_cons] at member
      rcases member with same | later
      · subst binder
        obtain ⟨entry, entryAt, _, _, entryValue⟩ := present
        exact ⟨value, by simp [Connected.read, active, entryAt, entryValue], ordinary⟩
      · exact ih remaining later

theorem function_entry_scheme_cells_immutable (state : Machine) (solution : Substitution)
    (parameters : Scope) (values : List Value) (types : RuntimeType) (locals : ActiveCells) (store : Store)
    (typed : ParameterBindingsTyped solution parameters values types)
    (unique : (parameters.map Binding.binder).Nodup)
    (bound : bindParameters state.nextActivation (parameters.map Binding.binder) values types state.store = .ok (locals, store))
    (globalLive : ActiveCellsPresent state.globals state.store)
    (globalSchemes : SchemeCellsImmutable state.globals state.store) :
    SchemeCellsImmutable (withGlobals locals state.globals) store := by
  have parameterCells := bound_parameters_agree state.nextActivation (parameters.map Binding.binder) values types
    state.store store locals (parameter_bindings_have_values solution parameters values types typed) unique bound
  have extension := parameters_preserve_existing state.nextActivation (parameters.map Binding.binder)
    values types state.store store locals bound
  intro binder declaration scheme readMerged
  cases localFound : locals binder with
  | some cell =>
    have member := bound_parameter_cell_is_listed state.nextActivation (parameters.map Binding.binder)
      values types state.store store locals bound binder cell localFound
    obtain ⟨value, localRead, ordinary⟩ := parameter_binder_reads_ordinary solution state.nextActivation
      parameters values types locals store typed parameterCells binder member
    have mergedRead : Connected.read store (withGlobals locals state.globals) binder = some value := by
      simpa [Connected.read, withGlobals, localFound] using localRead
    rw [readMerged] at mergedRead
    cases mergedRead
    simp [ordinaryValue] at ordinary
  | none =>
    cases globalFound : state.globals binder with
    | none => simp [Connected.read, withGlobals, localFound, globalFound] at readMerged
    | some cell =>
      obtain ⟨entry, oldAt⟩ := globalLive binder cell globalFound
      have newAt := extension cell entry oldAt
      have oldRead : Connected.read state.store state.globals binder = some (.schemeFunction declaration scheme) := by
        simpa [Connected.read, withGlobals, localFound, globalFound, oldAt, newAt] using readMerged
      obtain ⟨oldCell, oldEntry, oldActive, oldPresent, immutable⟩ := globalSchemes binder declaration scheme oldRead
      rw [globalFound] at oldActive
      cases oldActive
      rw [oldAt] at oldPresent
      cases oldPresent
      exact ⟨cell, entry, by simp [withGlobals, localFound, globalFound], newAt, immutable⟩

end Xsh.Semantics.Connected
