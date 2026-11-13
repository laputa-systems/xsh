import Xsh.Semantics.Connected.TypedScope

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Semantics.Environments

inductive ParameterValuesTyped : List BinderId → List Value → RuntimeType → Prop where
  | nil : ParameterValuesTyped [] [] .argumentsEmpty
  | cons {binder binders value values ty types} :
      HasType ty value → ParameterValuesTyped binders values types →
      ParameterValuesTyped (binder :: binders) (value :: values) (.argument ty types)

theorem allocate_other_cell (store next : Store) (cell other : CellId) (kind : Mutability)
    (ty : RuntimeType) (value : Value) (different : other ≠ cell) (allocated : allocate store cell kind ty value = some next) :
    next other = store other := by
  exact allocate_preserves_unrelated_cells (fun _ _ => inferInstance)
    store next cell other kind ty value different allocated

theorem bound_parameter_cell_is_listed (activation : Nat) (binders : List BinderId)
    (values : List Value) (types : RuntimeType) (store next : Store) (cells : ActiveCells)
    (bound : bindParameters activation binders values types store = .ok (cells, next))
    (binder : BinderId) (cell : CellId) (active : cells binder = some cell) : binder ∈ binders := by
  induction binders generalizing values types store next cells with
  | nil =>
    cases values with
    | nil =>
      cases types with
      | argumentsEmpty => cases bound; simp [emptyCells] at active
      | _ => cases bound
    | cons _ _ => cases types <;> cases bound
  | cons first rest ih =>
    cases values with
    | nil => cases types <;> cases bound
    | cons value values =>
      cases types with
      | argument ty types =>
        by_cases typed : HasType ty value
        · cases allocated : allocate store ⟨activation, first⟩ .immutable ty value with
          | none => simp [bindParameters, typed, allocated] at bound
          | some intermediate =>
            simp [bindParameters, typed, allocated, Xsh.Typing.Connected.except_bind_ok] at bound
            obtain ⟨priorCells, finalStore, recursive, finished⟩ := bound
            split at finished
            · cases finished
            · cases finished
              by_cases same : binder = first
              · simp [same]
              · have prior : priorCells binder = some cell := by simpa [bindActive, same] using active
                exact List.mem_cons_of_mem _ (ih values types intermediate _ priorCells recursive prior)
        · simp only [bindParameters, typed, ↓reduceIte] at bound
          cases bound
      | _ => cases bound

theorem fresh_typed_parameters_bind (activation : Nat) (binders : List BinderId)
    (values : List Value) (types : RuntimeType) (store : Store)
    (typed : ParameterValuesTyped binders values types) (unique : binders.Nodup)
    (fresh : ∀ binder ∈ binders, store ⟨activation, binder⟩ = none) :
    ∃ cells next, bindParameters activation binders values types store = .ok (cells, next) := by
  induction typed generalizing store with
  | nil => exact ⟨emptyCells, store, rfl⟩
  | @cons binder binders value values ty types valueTyped restTyped ih =>
    simp only [List.nodup_cons] at unique
    have firstFresh : store ⟨activation, binder⟩ = none := fresh binder (by simp)
    obtain ⟨intermediate, entry, allocated, _, _, _, _⟩ :=
      allocate_fresh_typed_cell store ⟨activation, binder⟩ .immutable value ty firstFresh valueTyped
    have restFresh : ∀ other ∈ binders, intermediate ⟨activation, other⟩ = none := by
      intro other member
      have different : (⟨activation, other⟩ : CellId) ≠ ⟨activation, binder⟩ := by
        intro equal
        cases equal
        exact unique.1 member
      rw [allocate_other_cell store intermediate _ _ .immutable ty value different allocated]
      exact fresh other (List.mem_cons_of_mem _ member)
    obtain ⟨cells, next, bound⟩ := ih intermediate unique.2 restFresh
    have absent : cells binder = none := by
      cases found : cells binder with
      | none => rfl
      | some cell =>
        have member := bound_parameter_cell_is_listed activation binders values types intermediate next cells bound binder cell found
        exact False.elim (unique.1 member)
    refine ⟨bindActive cells binder ⟨activation, binder⟩, next, ?_⟩
    simp [bindParameters, valueTyped, allocated, bound]
    change (if (cells binder).isSome then Except.error Fault.wrongArguments else
      Except.ok (bindActive cells binder ⟨activation, binder⟩, next)) = _
    simp [absent]

end Xsh.Semantics.Connected
