import Xsh.Semantics.Connected.SafetyParameterCells

namespace Xsh.Semantics.Connected

open Xsh.Syntax.Foundation
open Xsh.Semantics.Environments
open Xsh.Typing.Connected
open Xsh.Values

def StoreBelowActivation (store : Store) (limit : Nat) : Prop :=
  ∀ cell entry, store cell = some entry → cell.activation < limit

theorem allocate_preserves_activation_bound (store next : Store) (cell : CellId)
    (kind : Mutability) (ty : RuntimeType) (value : Value) (limit : Nat) (below : StoreBelowActivation store limit)
    (cellBelow : cell.activation < limit) (allocated : allocate store cell kind ty value = some next) :
    StoreBelowActivation next limit := by
  intro other entry present
  by_cases same : other = cell
  · simpa [same] using cellBelow
  · rw [allocate_other_cell store next cell other kind ty value same allocated] at present
    exact below other entry present

theorem parameters_preserve_activation_bound (activation : Nat) (binders : List Xsh.Foundation.BinderId)
    (values : List Value) (types : RuntimeType) (store next : Store) (cells : ActiveCells) (limit : Nat)
    (below : StoreBelowActivation store limit) (activationBelow : activation < limit)
    (bound : bindParameters activation binders values types store = .ok (cells, next)) :
    StoreBelowActivation next limit := by
  induction binders generalizing values types store next cells with
  | nil =>
    cases values with
    | nil =>
      cases types with
      | argumentsEmpty => cases bound; exact below
      | _ => cases bound
    | cons _ _ => cases types <;> cases bound
  | cons binder rest ih =>
    cases values with
    | nil => cases types <;> cases bound
    | cons value values =>
      cases types with
      | argument ty types =>
        by_cases typed : HasType ty value
        · cases allocated : allocate store ⟨activation, binder⟩ .immutable ty value with
          | none => simp [bindParameters, typed, allocated] at bound
          | some intermediate =>
            simp [bindParameters, typed, allocated, except_bind_ok] at bound
            obtain ⟨priorCells, finalStore, recursive, finished⟩ := bound
            split at finished
            · cases finished
            · cases finished
              exact ih values types intermediate next priorCells
                (allocate_preserves_activation_bound store intermediate _ .immutable ty value limit below activationBelow allocated)
                recursive
        · simp only [bindParameters, typed, ↓reduceIte] at bound
          cases bound
      | _ => cases bound

inductive InvocationReady (state : Machine) : Value → List Value → RuntimeType → Prop where
  | function {declaration arguments parameters result captured typeArguments normalized function} :
      state.functions declaration = some (.function function) →
      instantiateFunction captured function.scheme typeArguments = some (parameters, result, normalized) →
      ParameterValuesTyped function.parameters arguments parameters → function.parameters.Nodup →
      StoreBelowActivation state.store state.nextActivation →
      InvocationReady state (.instantiatedFunction declaration parameters result captured typeArguments) arguments result
  | ok {declaration value success failure} :
      state.functions declaration = some .resultOk → HasType success value →
      InvocationReady state (.function declaration (.argument success .argumentsEmpty) (.result success failure))
        [value] (.result success failure)
  | error {declaration value success failure} :
      state.functions declaration = some .resultError → HasType failure value →
      InvocationReady state (.function declaration (.argument failure .argumentsEmpty) (.result success failure))
        [value] (.result success failure)
  | okInstantiated {declaration value success failure captured typeArguments} :
      state.functions declaration = some .resultOk → HasType success value →
      InvocationReady state (.instantiatedFunction declaration (.argument success .argumentsEmpty)
        (.result success failure) captured typeArguments) [value] (.result success failure)
  | errorInstantiated {declaration value success failure captured typeArguments} :
      state.functions declaration = some .resultError → HasType failure value →
      InvocationReady state (.instantiatedFunction declaration (.argument failure .argumentsEmpty)
        (.result success failure) captured typeArguments) [value] (.result success failure)
  | time {declaration} : state.functions declaration = some .timeNow →
      InvocationReady state (.function declaration .argumentsEmpty .integer) [] .integer
  | timeInstantiated {declaration captured typeArguments} : state.functions declaration = some .timeNow →
      InvocationReady state (.instantiatedFunction declaration .argumentsEmpty .integer captured typeArguments) [] .integer

inductive InvocationEntered (result : RuntimeType) : Machine → Prop where
  | value {state : Machine} {value : Value} : HasType result value → InvocationEntered result { state with control := .value value }
  | function {state : Machine} {function : Function} {arguments : List Value} {parameters resultType : RuntimeType} {captured : Substitution} {typeArguments : List (Variable × MonoType)} {cells : ActiveCells} {store : Store} :
      result = resultType →
      ParameterCellsAgree state.nextActivation cells store function.parameters arguments parameters →
      StoreBelowActivation store (state.nextActivation + 1) →
      InvocationEntered result { state with control := .statements function.body, cells := withGlobals cells state.globals, store := store, activation := state.nextActivation, nextActivation := state.nextActivation + 1, solution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration), stack := .caller state.cells state.activation state.solution resultType :: state.stack }

theorem compatible_ok_value (value : Value) (success failure : RuntimeType) (typed : HasType success value) :
    HasType (.result success failure) (.ok value failure) := by
  obtain ⟨actual, found, fits⟩ := (has_type_iff_compatible success value).mp typed
  apply (has_type_iff_compatible _ _).mpr
  refine ⟨.result actual failure, by simp [valueType, found], ?_⟩
  have same : typeFits failure failure = true := by cases failure <;> simp [typeFits]
  simp [typeFits, fits, same]

theorem compatible_error_value (value : Value) (success failure : RuntimeType) (typed : HasType failure value) :
    HasType (.result success failure) (.error value success) := by
  obtain ⟨actual, found, fits⟩ := (has_type_iff_compatible failure value).mp typed
  apply (has_type_iff_compatible _ _).mpr
  refine ⟨.result success actual, by simp [valueType, found], ?_⟩
  have same : typeFits success success = true := by cases success <;> simp [typeFits]
  simp [typeFits, fits, same]

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
    apply InvocationEntered.function rfl
    · exact bound_parameters_agree state.nextActivation function.parameters arguments parameters state.store store cells typed unique bound
    · apply parameters_preserve_activation_bound state.nextActivation function.parameters arguments parameters state.store store cells
      · intro cell entry present
        exact Nat.lt_succ_of_lt (below cell entry present)
      · exact Nat.lt_succ_self _
      · exact bound
  | ok found typed => exact ⟨.internal, _, .callOk state _ _ _ _ found typed, .value (compatible_ok_value _ _ _ typed)⟩
  | error found typed => exact ⟨.internal, _, .callError state _ _ _ _ found typed, .value (compatible_error_value _ _ _ typed)⟩
  | okInstantiated found typed => exact ⟨.internal, _, .callOkInstantiated state _ _ _ _ _ _ found typed, .value (compatible_ok_value _ _ _ typed)⟩
  | errorInstantiated found typed => exact ⟨.internal, _, .callErrorInstantiated state _ _ _ _ _ _ found typed, .value (compatible_error_value _ _ _ typed)⟩
  | time found => exact ⟨.request .timeNow response, _, .callTime state _ response found, .value (has_type_of_exact _ _ rfl)⟩
  | timeInstantiated found => exact ⟨.request .timeNow response, _, .callTimeInstantiated state _ _ _ response found, .value (has_type_of_exact _ _ rfl)⟩

end Xsh.Semantics.Connected
