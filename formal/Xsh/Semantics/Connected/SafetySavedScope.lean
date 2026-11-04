import Xsh.Semantics.Connected.SafetyFunctionEntry

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

theorem scope_values_survive_stable_store (scope : Scope) (solution : Substitution) (cells : ActiveCells)
    (before after : Store) (values : ScopeValuesAgree scope solution cells before)
    (schemes : SchemeCellsImmutable cells before) (layout : StoreEvolves before after)
    (stable : ImmutableStable before after) : ScopeValuesAgree scope solution cells after := by
  intro name binding found notSchema
  obtain ⟨oldValue, oldRead, oldAgrees⟩ := values name binding found notSchema
  cases active : cells binding.binder with
  | none => simp [Connected.read, active] at oldRead
  | some cell =>
    cases present : before cell with
    | none => simp [Connected.read, active, present] at oldRead
    | some current =>
      have oldValueEq : current.contents.value = oldValue := by simpa [Connected.read, active, present] using oldRead
      rcases oldAgrees with ⟨declaration, parameters, result, schemeValue, schemeBody⟩ | ordinary
      · have oldSchemeRead : Connected.read before cells binding.binder = some (.schemeFunction declaration binding.scheme) := by
          simpa [schemeValue] using oldRead
        obtain ⟨schemeCell, schemeEntry, schemeActive, schemePresent, immutable⟩ :=
          schemes binding.binder declaration binding.scheme oldSchemeRead
        rw [active] at schemeActive
        cases schemeActive
        rw [present] at schemePresent
        cases schemePresent
        have retained := stable cell current present immutable
        exact ⟨oldValue, by simp [Connected.read, active, retained, oldValueEq],
          Or.inl ⟨declaration, parameters, result, schemeValue, schemeBody⟩⟩
      · obtain ⟨next, nextAt, sameTag, _⟩ := layout cell current present
        have sameType : valueType next.contents.value = valueType oldValue := by
          rw [← oldValueEq]
          exact next.contents.wellTyped.trans (sameTag ▸ current.contents.wellTyped.symm)
        refine ⟨next.contents.value, by simp [Connected.read, active, nextAt], Or.inr ?_⟩
        refine ⟨ordinary_value_survives_matching_type oldValue next.contents.value ordinary.1 sameType, ?_⟩
        intro arguments raw requirements runtimeType instantiated solved closed
        exact sameType.trans (ordinary.2 arguments raw requirements runtimeType instantiated solved closed)

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
    have oldType : valueType old.contents.value = some .schemeFunction := by
      have typed := next.contents.wellTyped
      rw [nextValue] at typed
      have oldTyped := old.contents.wellTyped
      rw [← sameTag] at oldTyped
      exact oldTyped.trans typed.symm
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
    (schemes : SchemeCellsImmutable cells before.store) :
    ScopeStateAgrees scope { after with solution := solution, cells := cells } ∧
      SchemeCellsImmutable cells after.store := by
  obtain ⟨unique, values, mutableCells, distinct, present⟩ := agrees
  have layout := execution_preserves_store_layout execution
  have stable := execution_preserves_immutable_cells execution
  refine ⟨?_, scheme_cells_survive_stable_store cells before.store after.store present schemes layout stable⟩
  exact ⟨unique, scope_values_survive_stable_store scope solution cells before.store after.store values schemes layout stable,
    mutable_cells_survive_store_evolution scope solution cells before.store after.store layout mutableCells,
    distinct, active_cells_survive_store_evolution cells before.store after.store present layout⟩

end Xsh.Semantics.Connected
