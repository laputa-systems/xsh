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

end Xsh.Semantics.Connected
