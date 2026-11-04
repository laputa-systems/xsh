import Xsh.Semantics.Connected.SafetyCallScope

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Semantics.Environments
open Xsh.Typing.Connected

inductive ParameterBindingsTyped (solution : Substitution) : Scope → List Value → RuntimeType → Prop where
  | nil : ParameterBindingsTyped solution [] [] .argumentsEmpty
  | cons {name binder raw value values ty types scope} :
      ordinaryValue value = true → monoRuntime (substitute solution raw) = some ty →
      HasType ty value → ParameterBindingsTyped solution scope values types →
      ParameterBindingsTyped solution (⟨name, binder, .immutable, ⟨[], raw, []⟩⟩ :: scope)
        (value :: values) (.argument ty types)

theorem parameter_bindings_scope_values (solution : Substitution) (activation : Nat)
    (scope : Scope) (values : List Value) (types : RuntimeType) (cells : ActiveCells) (store : Store)
    (typed : ParameterBindingsTyped solution scope values types)
    (agrees : ParameterCellsAgree activation cells store (scope.map Binding.binder) values types) :
    ScopeValuesAgree scope solution cells store := by
  induction typed with
  | nil =>
    intro name binding found
    simp at found
  | @cons name binder raw value values ty types scope ordinary closed valueTyped tail ih =>
    cases agrees with
    | cons active present remaining =>
      intro sought binding found notSchema
      by_cases same : name = sought
      · subst sought
        simp at found
        cases found
        obtain ⟨entry, entryAt, _, _, entryValue⟩ := present
        refine ⟨value, ?_, Or.inr ?_⟩
        · simp [Connected.read, active, entryAt, entryValue]
        · exact monomorphic_value_agrees solution name binder .immutable raw value ty ordinary closed valueTyped
      · have tailFound : scope.find? (fun item => decide (item.name = sought)) = some binding := by
          simpa [same] using found
        exact ih remaining sought binding tailFound notSchema

theorem parameter_bindings_have_no_mutable_cells (solution : Substitution)
    (scope : Scope) (values : List Value) (types : RuntimeType) (cells : ActiveCells) (store : Store)
    (typed : ParameterBindingsTyped solution scope values types) :
    MutableCellsAgree scope solution cells store := by
  induction typed with
  | nil => intro name binding found; simp at found
  | @cons name binder raw value values ty types scope ordinary closed valueTyped tail ih =>
    intro sought binding found mutable
    by_cases same : name = sought
    · subst sought
      simp at found
      cases found
      cases mutable
    · have tailFound : scope.find? (fun item => decide (item.name = sought)) = some binding := by
        simpa [same] using found
      exact ih sought binding tailFound mutable

theorem parameter_bindings_have_values (solution : Substitution) (scope : Scope)
    (values : List Value) (types : RuntimeType) (typed : ParameterBindingsTyped solution scope values types) :
    ParameterValuesTyped (scope.map Binding.binder) values types := by
  induction typed with
  | nil => exact .nil
  | cons ordinary closed valueTyped tail ih => exact .cons valueTyped ih

theorem parameter_and_global_scope_values (state : Machine) (solution : Substitution)
    (parameters globals : Scope) (values : List Value) (types : RuntimeType) (locals : ActiveCells) (store : Store)
    (typed : ParameterBindingsTyped solution parameters values types)
    (bound : bindParameters state.nextActivation (parameters.map Binding.binder) values types state.store = .ok (locals, store))
    (unique : (parameters.map Binding.binder).Nodup)
    (globalValues : ScopeValuesAgree globals solution state.globals state.store)
    (disjoint : ∀ binding ∈ globals, binding.binder ∉ parameters.map Binding.binder) :
    ScopeValuesAgree (parameters ++ globals) solution (withGlobals locals state.globals) store := by
  have parameterCells := bound_parameters_agree state.nextActivation (parameters.map Binding.binder) values types
    state.store store locals (parameter_bindings_have_values solution parameters values types typed) unique bound
  have parameterValues := parameter_bindings_scope_values solution state.nextActivation parameters values types
    (withGlobals locals state.globals) store typed
    (parameter_cells_survive_globals state.nextActivation locals state.globals store
      (parameters.map Binding.binder) values types parameterCells)
  intro name binding found notSchema
  cases localFound : parameters.find? (fun item => decide (item.name = name)) with
  | some localBinding =>
    have same : localBinding = binding := by
      simpa [List.find?_append, localFound] using found
    subst localBinding
    exact parameterValues name binding localFound notSchema
  | none =>
    have globalFound : globals.find? (fun item => decide (item.name = name)) = some binding := by
      simpa [List.find?_append, localFound] using found
    obtain ⟨value, readGlobal, agrees⟩ := globalValues name binding globalFound notSchema
    have notParameter := disjoint binding (List.mem_of_find?_eq_some globalFound)
    have unshadowed : locals binding.binder = none := by
      cases active : locals binding.binder with
      | none => rfl
      | some cell =>
        exact False.elim (notParameter (bound_parameter_cell_is_listed state.nextActivation
          (parameters.map Binding.binder) values types state.store store locals bound binding.binder cell active))
    exact ⟨value, parameter_binding_preserves_unshadowed_global state locals store
      (parameters.map Binding.binder) values types binding.binder value bound unshadowed readGlobal, agrees⟩

theorem parameter_and_global_mutable_cells (state : Machine) (solution : Substitution)
    (parameters globals : Scope) (values : List Value) (types : RuntimeType) (locals : ActiveCells) (store : Store)
    (typed : ParameterBindingsTyped solution parameters values types)
    (bound : bindParameters state.nextActivation (parameters.map Binding.binder) values types state.store = .ok (locals, store))
    (globalMutable : MutableCellsAgree globals solution state.globals state.store)
    (disjoint : ∀ binding ∈ globals, binding.binder ∉ parameters.map Binding.binder) :
    MutableCellsAgree (parameters ++ globals) solution (withGlobals locals state.globals) store := by
  have parameterMutable := parameter_bindings_have_no_mutable_cells solution parameters values types
    (withGlobals locals state.globals) store typed
  have globalRetained := mutable_cells_survive_store_evolution globals solution state.globals state.store store
    (parameters_preserve_layout state.nextActivation (parameters.map Binding.binder) values types state.store store locals bound)
    globalMutable
  intro name binding found mutable
  cases localFound : parameters.find? (fun item => decide (item.name = name)) with
  | some localBinding =>
    have same : localBinding = binding := by
      simpa [List.find?_append, localFound] using found
    subst localBinding
    exact parameterMutable name binding localFound mutable
  | none =>
    have globalFound : globals.find? (fun item => decide (item.name = name)) = some binding := by
      simpa [List.find?_append, localFound] using found
    obtain ⟨quantified, requirements, cell, current, runtimeType, active, present, writable, sameType, closed⟩ :=
      globalRetained name binding globalFound mutable
    have notParameter := disjoint binding (List.mem_of_find?_eq_some globalFound)
    have localAbsent : locals binding.binder = none := by
      cases localActive : locals binding.binder with
      | none => rfl
      | some cell =>
        exact False.elim (notParameter (bound_parameter_cell_is_listed state.nextActivation
          (parameters.map Binding.binder) values types state.store store locals bound binding.binder cell localActive))
    exact ⟨quantified, requirements, cell, current, runtimeType,
      by simpa [withGlobals, localAbsent] using active, present, writable, sameType, closed⟩

theorem parameter_cell_for_binder (activation : Nat) (cells : ActiveCells) (store : Store)
    (binders : List BinderId) (values : List Value) (types : RuntimeType)
    (agrees : ParameterCellsAgree activation cells store binders values types)
    (binder : BinderId) (member : binder ∈ binders) :
    cells binder = some ⟨activation, binder⟩ ∧ ∃ entry, store ⟨activation, binder⟩ = some entry := by
  induction agrees with
  | nil => simp at member
  | @cons first rest value values ty types active present tail ih =>
    simp only [List.mem_cons] at member
    rcases member with same | later
    · subst binder
      obtain ⟨entry, present, _, _, _⟩ := present
      exact ⟨active, entry, present⟩
    · exact ih later

theorem parameter_local_cell (state : Machine) (solution : Substitution) (parameters : Scope) (values : List Value)
    (types : RuntimeType) (locals : ActiveCells) (store : Store)
    (typed : ParameterBindingsTyped solution parameters values types)
    (unique : (parameters.map Binding.binder).Nodup)
    (bound : bindParameters state.nextActivation (parameters.map Binding.binder) values types state.store = .ok (locals, store))
    (binder : BinderId) (cell : CellId) (active : locals binder = some cell) :
    cell = ⟨state.nextActivation, binder⟩ ∧ ∃ entry, store cell = some entry := by
  have member := bound_parameter_cell_is_listed state.nextActivation (parameters.map Binding.binder)
    values types state.store store locals bound binder cell active
  have agrees := bound_parameters_agree state.nextActivation (parameters.map Binding.binder) values types
    state.store store locals (parameter_bindings_have_values solution parameters values types typed) unique bound
  obtain ⟨expected, entry, present⟩ := parameter_cell_for_binder state.nextActivation locals store
    (parameters.map Binding.binder) values types agrees binder member
  have same : cell = ⟨state.nextActivation, binder⟩ := Option.some.inj (active.symm.trans expected)
  subst cell
  exact ⟨rfl, entry, present⟩

theorem merge_fresh_locals_distinct_and_live (globals locals : ActiveCells) (before after : Store) (activation : Nat)
    (localCells : ∀ binder cell, locals binder = some cell →
      cell = ⟨activation, binder⟩ ∧ ∃ entry, after cell = some entry)
    (globalLive : CellsLive globals before) (globalDistinct : DistinctActiveCells globals)
    (below : StoreBelowActivation before activation) (extension : StoreExtends before after) :
    DistinctActiveCells (withGlobals locals globals) ∧ ActiveCellsPresent (withGlobals locals globals) after := by
  have globalBelow : ∀ binder cell, globals binder = some cell → cell.activation < activation := by
    intro binder cell active
    obtain ⟨entry, present⟩ := globalLive binder cell active
    exact below cell entry present
  constructor
  · intro first second cell firstAt secondAt
    cases localFirst : locals first with
    | some firstCell =>
      have firstEq : firstCell = cell := by simpa [withGlobals, localFirst] using firstAt
      subst firstCell
      have ⟨firstEq, _⟩ := localCells first cell localFirst
      cases localSecond : locals second with
      | some secondCell =>
        have secondEq : secondCell = cell := by simpa [withGlobals, localSecond] using secondAt
        subst secondCell
        have ⟨secondEq, _⟩ := localCells second cell localSecond
        exact congrArg CellId.binder (firstEq.symm.trans secondEq)
      | none =>
        have globalSecond : globals second = some cell := by simpa [withGlobals, localSecond] using secondAt
        have prior := globalBelow second cell globalSecond
        rw [firstEq] at prior
        exact False.elim (Nat.lt_irrefl _ prior)
    | none =>
      have globalFirst : globals first = some cell := by simpa [withGlobals, localFirst] using firstAt
      cases localSecond : locals second with
      | some secondCell =>
        have secondEq : secondCell = cell := by simpa [withGlobals, localSecond] using secondAt
        subst secondCell
        have ⟨secondEq, _⟩ := localCells second cell localSecond
        have prior := globalBelow first cell globalFirst
        rw [secondEq] at prior
        exact False.elim (Nat.lt_irrefl _ prior)
      | none =>
        have globalSecond : globals second = some cell := by simpa [withGlobals, localSecond] using secondAt
        exact globalDistinct first second cell globalFirst globalSecond
  · intro binder cell active
    cases localFound : locals binder with
    | some localCell =>
      have same : localCell = cell := by simpa [withGlobals, localFound] using active
      subst localCell
      exact (localCells binder cell localFound).2
    | none =>
      have globalFound : globals binder = some cell := by simpa [withGlobals, localFound] using active
      obtain ⟨entry, present⟩ := globalLive binder cell globalFound
      exact ⟨entry, extension cell entry present⟩

theorem parameter_global_scope_agrees (state : Machine) (globalTypes : CellTypes) (solution : Substitution)
    (parameters globals : Scope) (values : List Value) (types : RuntimeType) (locals : ActiveCells) (store : Store)
    (invariant : MachineMemoryInvariant globalTypes state)
    (typed : ParameterBindingsTyped solution parameters values types)
    (bound : bindParameters state.nextActivation (parameters.map Binding.binder) values types state.store = .ok (locals, store))
    (unique : ((parameters ++ globals).map Binding.binder).Nodup)
    (globalValues : ScopeValuesAgree globals solution state.globals state.store)
    (globalMutable : MutableCellsAgree globals solution state.globals state.store)
    (globalDistinct : DistinctActiveCells state.globals) :
    ScopeStateAgrees (parameters ++ globals)
      { state with solution := solution, cells := withGlobals locals state.globals, store := store } := by
  have splitUnique : (parameters.map Binding.binder).Nodup ∧ (globals.map Binding.binder).Nodup ∧
      ∀ first ∈ parameters.map Binding.binder, ∀ second ∈ globals.map Binding.binder, first ≠ second := by
    exact List.nodup_append.mp (by simpa using unique)
  have disjoint : ∀ binding ∈ globals, binding.binder ∉ parameters.map Binding.binder := by
    intro binding member paramMember
    apply splitUnique.2.2 binding.binder paramMember binding.binder
      (List.mem_map.mpr ⟨binding, member, rfl⟩)
    rfl
  have localCells := parameter_local_cell state solution parameters values types locals store typed splitUnique.1 bound
  have both := merge_fresh_locals_distinct_and_live state.globals locals state.store store state.nextActivation
    localCells invariant.globals.live globalDistinct invariant.activation.store
    (parameters_preserve_existing state.nextActivation (parameters.map Binding.binder) values types state.store store locals bound)
  exact ⟨unique, parameter_and_global_scope_values state solution parameters globals values types locals store
      typed bound splitUnique.1 globalValues disjoint,
    parameter_and_global_mutable_cells state solution parameters globals values types locals store
      typed bound globalMutable disjoint, both.1, both.2⟩

end Xsh.Semantics.Connected
