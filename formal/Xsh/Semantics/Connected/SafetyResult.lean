import Xsh.Semantics.Connected.TypedBinary

namespace Xsh.Semantics.Connected

theorem result_value_shape (value : Value) (success failure : RuntimeType)
    (typed : valueType value = some (.result success failure)) :
    (∃ inner, value = .ok inner failure ∧ valueType inner = some success) ∨
      (∃ inner, value = .error inner success ∧ valueType inner = some failure) := by
  cases value with
  | ok inner failureType =>
    cases innerType : valueType inner with
    | none => simp [valueType, innerType] at typed
    | some actual =>
      simp [valueType, innerType] at typed
      obtain ⟨rfl, rfl⟩ := typed
      exact Or.inl ⟨inner, rfl, innerType⟩
  | error inner result =>
    cases innerType : valueType inner with
    | none => simp [valueType, innerType] at typed
    | some actual =>
      simp [valueType, innerType] at typed
      obtain ⟨rfl, rfl⟩ := typed
      exact Or.inr ⟨inner, rfl, innerType⟩
  | scalar value => cases value <;> simp [valueType] at typed
  | record fields => simp [valueType] at typed
  | nominal identity base inner =>
    cases innerType : valueType inner <;> simp [valueType, innerType] at typed
  | _ => simp [valueType] at typed

theorem typed_propagate_frame_progress (state : Machine) (value : Value) (success failure : RuntimeType)
    (typed : valueType value = some (.result success failure)) :
    (∃ inner, valueType inner = some success ∧
      Step { state with control := .value value, stack := .propagate :: state.stack } .internal
        { state with control := .value inner }) ∨
    (∃ inner, valueType inner = some failure ∧
      Step { state with control := .value value, stack := .propagate :: state.stack } .internal
        { state with control := .propagated (.error inner success) }) := by
  rcases result_value_shape value success failure typed with ⟨inner, rfl, innerTyped⟩ | ⟨inner, rfl, innerTyped⟩
  · exact Or.inl ⟨inner, innerTyped, .propagateOk state inner failure⟩
  · exact Or.inr ⟨inner, innerTyped, .propagateError state inner success⟩

end Xsh.Semantics.Connected
