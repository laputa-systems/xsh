import Xsh.Semantics.Connected.SafetySavedScope

namespace Xsh.Semantics.Connected

open Xsh.Syntax.Foundation
open Xsh.Typing.Connected

theorem typed_function_call_enters_scope (state : Machine) (globalTypes : CellTypes)
    (declaration : DeclarationId) (arguments : List Value) (parameters globals : Scope)
    (parameterTypes result : RuntimeType) (captured : Substitution)
    (typeArguments normalized : List (Variable × MonoType)) (function : Function)
    (invariant : MachineMemoryInvariant globalTypes state)
    (found : state.functions declaration = some (.function function))
    (signature : instantiateFunction captured function.scheme typeArguments = some (parameterTypes, result, normalized))
    (binders : function.parameters = parameters.map Binding.binder)
    (typed : ParameterBindingsTyped
      (compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration))
      parameters arguments parameterTypes)
    (unique : ((parameters ++ globals).map Binding.binder).Nodup)
    (globalValues : ScopeValuesAgree globals
      (compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration))
      state.globals state.store)
    (globalMutable : MutableCellsAgree globals
      (compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration))
      state.globals state.store)
    (globalDistinct : DistinctActiveCells state.globals)
    (globalSchemes : SchemeCellsImmutable state.globals state.store) :
    ∃ locals store,
      let entered : Machine := { state with control := .statements function.body, cells := withGlobals locals state.globals, store := store, activation := state.nextActivation, nextActivation := state.nextActivation + 1, solution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration), stack := .caller state.cells state.activation state.solution result :: state.stack }
      Step { state with control := .invoke (.instantiatedFunction declaration parameterTypes result captured typeArguments) arguments }
        .internal entered ∧ MachineMemoryInvariant globalTypes entered ∧ ScopeStateAgrees (parameters ++ globals) entered ∧
        SchemeCellsImmutable (withGlobals locals state.globals) store := by
  let solution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration)
  have uniqueParameters : (parameters.map Binding.binder).Nodup := by
    exact (List.nodup_append.mp (by simpa using unique)).1
  have fresh : ∀ binder ∈ parameters.map Binding.binder, state.store ⟨state.nextActivation, binder⟩ = none := by
    intro binder _
    cases occupied : state.store ⟨state.nextActivation, binder⟩ with
    | none => rfl
    | some entry =>
      have impossible := invariant.activation.store ⟨state.nextActivation, binder⟩ entry occupied
      exact False.elim (Nat.lt_irrefl _ impossible)
  obtain ⟨locals, store, bound⟩ := fresh_typed_parameters_bind state.nextActivation (parameters.map Binding.binder)
    arguments parameterTypes state.store (parameter_bindings_have_values solution parameters arguments parameterTypes typed)
    uniqueParameters fresh
  have actualBound : bindParameters state.nextActivation function.parameters arguments parameterTypes state.store = .ok (locals, store) := by
    simpa [binders] using bound
  have step := Step.callFunction state declaration arguments parameterTypes result captured typeArguments normalized function locals store found signature actualBound
  refine ⟨locals, store, step, ?_, ?_, ?_⟩
  · apply step_preserves_machine_memory_invariant step
    exact ⟨⟨invariant.activation.active, invariant.activation.saved, invariant.activation.store⟩,
      ⟨invariant.globals.typed, invariant.globals.live⟩⟩
  · exact parameter_global_scope_agrees state globalTypes solution parameters globals arguments parameterTypes locals store
      invariant typed bound unique globalValues globalMutable globalDistinct
  · exact function_entry_scheme_cells_immutable state solution parameters arguments parameterTypes locals store
      typed uniqueParameters bound invariant.globals.live globalSchemes

end Xsh.Semantics.Connected
