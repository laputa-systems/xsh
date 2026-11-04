import Xsh.Semantics.Connected.SafetyParameterScope

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Semantics.Environments
open Xsh.Typing.Connected

def ScopeWritableContract (scope : Scope) (solution : Substitution) (cells : ActiveCells) (store : Store) : Prop :=
  ∀ name binding cell entry, scope.find? (fun item => decide (item.name = name)) = some binding →
    binding.kind ≠ .schema → cells binding.binder = some cell → store cell = some entry →
    entry.mutability = .mutable → ∀ value, HasType entry.contents.typeTag value →
      OrdinaryBindingValueAgrees solution binding value

theorem scope_writable_contract_survives (scope : Scope) (solution : Substitution) (cells : ActiveCells)
    (before after : Store) (live : ActiveCellsPresent cells before)
    (contract : ScopeWritableContract scope solution cells before) (layout : StoreEvolves before after) :
    ScopeWritableContract scope solution cells after := by
  intro name binding cell current found notSchema active present mutable value typed
  obtain ⟨old, oldAt⟩ := live binding.binder cell active
  obtain ⟨next, nextAt, tag, mutability⟩ := layout cell old oldAt
  rw [present] at nextAt
  cases nextAt
  exact contract name binding cell old found notSchema active oldAt (mutability.symm.trans mutable)
    value (by simpa [tag] using typed)

theorem scope_values_survive_writable_contract (scope : Scope) (solution : Substitution) (cells : ActiveCells)
    (before after : Store) (values : ScopeValuesAgree scope solution cells before)
    (contract : ScopeWritableContract scope solution cells before) (layout : StoreEvolves before after)
    (stable : ImmutableStable before after) : ScopeValuesAgree scope solution cells after := by
  intro name binding found notSchema
  obtain ⟨oldValue, oldRead, agrees⟩ := values name binding found notSchema
  cases active : cells binding.binder with
  | none => simp [Connected.read, active] at oldRead
  | some cell =>
    cases present : before cell with
    | none => simp [Connected.read, active, present] at oldRead
    | some old =>
      cases kind : old.mutability with
      | immutable =>
        have retained := stable cell old present kind
        exact ⟨oldValue, by simpa [Connected.read, active, present, retained] using oldRead,
          agrees⟩
      | mutable =>
        obtain ⟨next, nextAt, tag, _⟩ := layout cell old present
        refine ⟨next.contents.value, by simp [Connected.read, active, nextAt], Or.inr ?_⟩
        exact contract name binding cell old found notSchema active present kind next.contents.value
          (by simpa [tag] using next.contents.wellTyped)

theorem scope_state_survives_writable_contract (before after : Machine) (scope : Scope)
    (solution : Substitution) (cells : ActiveCells)
    (agrees : ScopeStateAgrees scope { before with solution := solution, cells := cells })
    (contract : ScopeWritableContract scope solution cells before.store)
    (layout : StoreEvolves before.store after.store) (stable : ImmutableStable before.store after.store) :
    ScopeStateAgrees scope { after with solution := solution, cells := cells } ∧
      ScopeWritableContract scope solution cells after.store := by
  obtain ⟨unique, values, mutableCells, distinct, live⟩ := agrees
  exact ⟨⟨unique, scope_values_survive_writable_contract scope solution cells before.store after.store
    values contract layout stable,
    mutable_cells_survive_store_evolution scope solution cells before.store after.store layout mutableCells,
    distinct, active_cells_survive_store_evolution cells before.store after.store live layout⟩,
    scope_writable_contract_survives scope solution cells before.store after.store live contract layout⟩

end Xsh.Semantics.Connected
