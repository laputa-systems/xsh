import Xsh.Semantics.Connected.Invariant

namespace Xsh.Semantics.Connected

structure GlobalStoreInvariant (types : CellTypes) (state : Machine) : Prop where
  typed : CellsTyped state.globals types state.store
  live : CellsLive state.globals state.store

theorem step_preserves_global_context {before after : Machine} {event : Label}
    (step : Step before event after) :
    after.functions = before.functions ∧ after.schemas = before.schemas ∧ after.globals = before.globals := by
  cases step <;> exact ⟨rfl, rfl, rfl⟩

theorem step_preserves_global_store_invariant {before after : Machine} {event : Label} {types : CellTypes}
    (step : Step before event after) (invariant : GlobalStoreInvariant types before) :
    GlobalStoreInvariant types after := by
  obtain ⟨_, _, sameGlobals⟩ := step_preserves_global_context step
  have evolves := step_preserves_store_layout step
  constructor
  · rw [sameGlobals]
    exact store_evolution_preserves_typed_cells evolves invariant.typed
  · rw [sameGlobals]
    exact store_evolution_preserves_live_cells evolves invariant.live

theorem execution_preserves_global_store_invariant {before after : Machine} {events : List Label}
    {types : CellTypes} (execution : Execution before events after)
    (invariant : GlobalStoreInvariant types before) : GlobalStoreInvariant types after := by
  induction execution with
  | done => exact invariant
  | next step _ ih => exact ih (step_preserves_global_store_invariant step invariant)

end Xsh.Semantics.Connected
