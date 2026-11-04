import Xsh.Semantics.Connected.SafetyCallScope

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

inductive ParameterBindingsTyped (solution : Substitution) : Scope → List Value → RuntimeType → Prop where
  | nil : ParameterBindingsTyped solution [] [] .argumentsEmpty
  | cons {name binder raw value values ty types scope} :
      ordinaryValue value = true → monoRuntime (substitute solution raw) = some ty →
      valueType value = some ty → ParameterBindingsTyped solution scope values types →
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

end Xsh.Semantics.Connected
