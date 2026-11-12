import Xsh.Semantics.Connected.SafetyActivation
import Xsh.Semantics.Connected.SafetyWorld

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Semantics.Environments
open Xsh.Syntax.Foundation
open Xsh.Typing.Connected

structure MachineMemoryInvariant (types : CellTypes) (state : Machine) : Prop where
  activation : ActivationInvariant state
  globals : GlobalStoreInvariant types state

theorem step_preserves_machine_memory_invariant {before after : Machine} {event : Label} {types : CellTypes}
    (step : Step before event after) (invariant : MachineMemoryInvariant types before) :
    MachineMemoryInvariant types after :=
  ⟨step_preserves_activation_invariant step invariant.activation,
    step_preserves_global_store_invariant step invariant.globals⟩

theorem execution_preserves_machine_memory_invariant {before after : Machine} {events : List Label}
    {types : CellTypes} (execution : Execution before events after)
    (invariant : MachineMemoryInvariant types before) : MachineMemoryInvariant types after := by
  induction execution with
  | done => exact invariant
  | next step _ ih => exact ih (step_preserves_machine_memory_invariant step invariant)

theorem live_cell_precedes_next_activation (state : Machine) (types : CellTypes)
    (invariant : MachineMemoryInvariant types state) (binder : BinderId) (cell : CellId)
    (active : state.globals binder = some cell) : cell.activation < state.nextActivation := by
  obtain ⟨entry, present⟩ := invariant.globals.live binder cell active
  exact invariant.activation.store cell entry present

theorem parameter_cells_survive_globals (activation : Nat) (cells globals : ActiveCells) (store : Store)
    (binders : List BinderId) (values : List Value) (types : RuntimeType)
    (agrees : ParameterCellsAgree activation cells store binders values types) :
    ParameterCellsAgree activation (withGlobals cells globals) store binders values types := by
  induction agrees with
  | nil => exact .nil
  | cons active present tail ih =>
    exact .cons (by simp [withGlobals, active]) present ih

theorem bound_parameters_leave_other_globals_visible (activation : Nat) (binders : List BinderId)
    (values : List Value) (types : RuntimeType) (store next : Store) (cells globals : ActiveCells)
    (bound : bindParameters activation binders values types store = .ok (cells, next))
    (binder : BinderId) (notParameter : binder ∉ binders) :
    withGlobals cells globals binder = globals binder := by
  have absent : cells binder = none := by
    cases active : cells binder with
    | none => rfl
    | some cell =>
      exact False.elim (notParameter (bound_parameter_cell_is_listed activation binders values types store next cells bound binder cell active))
  simp [withGlobals, absent]

theorem typed_function_entry_preserves_memory (state : Machine) (globalTypes : CellTypes)
    (declaration : DeclarationId) (arguments : List Value) (parameters result : RuntimeType)
    (captured : Substitution) (typeArguments normalized : List (Variable × MonoType)) (function : Function)
    (invariant : MachineMemoryInvariant globalTypes state)
    (found : state.functions declaration = some (.function function))
    (signature : instantiateFunction captured function.scheme typeArguments = some (parameters, result, normalized))
    (typed : ParameterValuesTyped function.parameters arguments parameters) (unique : function.parameters.Nodup) :
    ∃ cells store,
      let entered : Machine := { state with control := .statements function.body, cells := withGlobals cells state.globals, store := store, activation := state.nextActivation, nextActivation := state.nextActivation + 1, solution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration), stack := .caller state.cells state.activation state.solution result :: state.stack }
      Step { state with control := .invoke (.instantiatedFunction declaration parameters result captured typeArguments) arguments }
        .internal entered ∧ MachineMemoryInvariant globalTypes entered ∧
        ParameterCellsAgree state.nextActivation (withGlobals cells state.globals) store function.parameters arguments parameters := by
  have fresh : ∀ binder ∈ function.parameters, state.store ⟨state.nextActivation, binder⟩ = none := by
    intro binder _
    cases occupied : state.store ⟨state.nextActivation, binder⟩ with
    | none => rfl
    | some entry =>
      have impossible := invariant.activation.store ⟨state.nextActivation, binder⟩ entry occupied
      exact False.elim (Nat.lt_irrefl _ impossible)
  obtain ⟨cells, store, bound⟩ := fresh_typed_parameters_bind state.nextActivation function.parameters arguments
    parameters state.store typed unique fresh
  have step := Step.callFunction state declaration arguments parameters result captured typeArguments normalized function cells store found signature bound
  refine ⟨cells, store, step, ?_, ?_⟩
  · apply step_preserves_machine_memory_invariant step
    exact ⟨⟨invariant.activation.active, invariant.activation.saved, invariant.activation.store⟩,
      ⟨invariant.globals.typed, invariant.globals.live⟩⟩
  · exact parameter_cells_survive_globals state.nextActivation cells state.globals store function.parameters arguments parameters
      (bound_parameters_agree state.nextActivation function.parameters arguments parameters state.store store cells typed unique bound)

end Xsh.Semantics.Connected
