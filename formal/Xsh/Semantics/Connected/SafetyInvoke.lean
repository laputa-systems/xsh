import Xsh.Semantics.Connected.SafetyParameterCells

namespace Xsh.Semantics.Connected

open Xsh.Syntax.Foundation
open Xsh.Semantics.Environments
open Xsh.Typing.Connected
open Xsh.Values

def StoreBelowActivation (store : Store) (limit : Nat) : Prop :=
  ∀ cell entry, store cell = some entry → cell.activation < limit

inductive InvocationReady (state : Machine) : Value → List Value → RuntimeType → Prop where
  | function {declaration arguments parameters result captured typeArguments normalized function} :
      state.functions declaration = some (.function function) →
      instantiateFunction captured function.scheme typeArguments = some (parameters, result, normalized) →
      ParameterValuesTyped function.parameters arguments parameters → function.parameters.Nodup →
      StoreBelowActivation state.store state.nextActivation →
      InvocationReady state (.instantiatedFunction declaration parameters result captured typeArguments) arguments result
  | ok {declaration value success failure} :
      state.functions declaration = some .resultOk → valueType value = some success →
      InvocationReady state (.function declaration (.argument success .argumentsEmpty) (.result success failure))
        [value] (.result success failure)
  | error {declaration value success failure} :
      state.functions declaration = some .resultError → valueType value = some failure →
      InvocationReady state (.function declaration (.argument failure .argumentsEmpty) (.result success failure))
        [value] (.result success failure)
  | okInstantiated {declaration value success failure captured typeArguments} :
      state.functions declaration = some .resultOk → valueType value = some success →
      InvocationReady state (.instantiatedFunction declaration (.argument success .argumentsEmpty)
        (.result success failure) captured typeArguments) [value] (.result success failure)
  | errorInstantiated {declaration value success failure captured typeArguments} :
      state.functions declaration = some .resultError → valueType value = some failure →
      InvocationReady state (.instantiatedFunction declaration (.argument failure .argumentsEmpty)
        (.result success failure) captured typeArguments) [value] (.result success failure)
  | time {declaration} : state.functions declaration = some .timeNow →
      InvocationReady state (.function declaration .argumentsEmpty .integer) [] .integer
  | timeInstantiated {declaration captured typeArguments} : state.functions declaration = some .timeNow →
      InvocationReady state (.instantiatedFunction declaration .argumentsEmpty .integer captured typeArguments) [] .integer

inductive InvocationEntered (result : RuntimeType) : Machine → Prop where
  | value {state : Machine} {value : Value} : valueType value = some result → InvocationEntered result { state with control := .value value }
  | function {state : Machine} {function : Function} {arguments : List Value} {parameters resultType : RuntimeType} {captured : Substitution} {typeArguments : List (Variable × MonoType)} {cells : ActiveCells} {store : Store} :
      result = resultType →
      ParameterCellsAgree state.nextActivation cells store function.parameters arguments parameters →
      InvocationEntered result { state with control := .statements function.body, cells := withGlobals cells state.globals, store := store, activation := state.nextActivation, nextActivation := state.nextActivation + 1, solution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration), stack := .caller state.cells state.activation state.solution resultType :: state.stack }

theorem ready_invocation_steps (state : Machine) (target : Value) (arguments : List Value)
    (result : RuntimeType) (response : IntValue) (ready : InvocationReady state target arguments result) :
    ∃ event next, Step { state with control := .invoke target arguments } event next ∧ InvocationEntered result next := by
  cases ready with
  | function found signature typed unique below =>
    rename_i declaration parameters captured typeArguments normalized function
    have fresh : ∀ binder ∈ function.parameters, state.store ⟨state.nextActivation, binder⟩ = none := by
      intro binder _
      cases occupied : state.store ⟨state.nextActivation, binder⟩ with
      | none => rfl
      | some entry =>
        have impossible := below ⟨state.nextActivation, binder⟩ entry occupied
        exact False.elim (Nat.lt_irrefl _ impossible)
    obtain ⟨cells, store, bound⟩ := fresh_typed_parameters_bind state.nextActivation function.parameters arguments
      parameters state.store typed unique fresh
    refine ⟨.internal, _, .callFunction state declaration arguments parameters result captured typeArguments normalized function cells store found signature bound, ?_⟩
    exact .function rfl (bound_parameters_agree state.nextActivation function.parameters arguments parameters state.store store cells typed unique bound)
  | ok found typed => exact ⟨.internal, _, .callOk state _ _ _ _ found typed, .value (by simp [valueType, typed])⟩
  | error found typed => exact ⟨.internal, _, .callError state _ _ _ _ found typed, .value (by simp [valueType, typed])⟩
  | okInstantiated found typed => exact ⟨.internal, _, .callOkInstantiated state _ _ _ _ _ _ found typed, .value (by simp [valueType, typed])⟩
  | errorInstantiated found typed => exact ⟨.internal, _, .callErrorInstantiated state _ _ _ _ _ _ found typed, .value (by simp [valueType, typed])⟩
  | time found => exact ⟨.request .timeNow response, _, .callTime state _ response found, .value rfl⟩
  | timeInstantiated found => exact ⟨.request .timeNow response, _, .callTimeInstantiated state _ _ _ response found, .value rfl⟩

end Xsh.Semantics.Connected
