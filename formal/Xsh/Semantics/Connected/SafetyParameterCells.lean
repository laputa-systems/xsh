import Xsh.Semantics.Connected.SafetyParameters

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Semantics.Environments

inductive ParameterCellsAgree (activation : Nat) (cells : ActiveCells) (store : Store) :
    List BinderId → List Value → RuntimeType → Prop where
  | nil : ParameterCellsAgree activation cells store [] [] .argumentsEmpty
  | cons {binder binders value values ty types} :
      cells binder = some ⟨activation, binder⟩ →
      (∃ entry, store ⟨activation, binder⟩ = some entry ∧ entry.mutability = .immutable ∧
        entry.contents.typeTag = ty ∧ entry.contents.value = value) →
      ParameterCellsAgree activation cells store binders values types →
      ParameterCellsAgree activation cells store (binder :: binders) (value :: values) (.argument ty types)

theorem parameter_cells_agree_after_binding (activation : Nat) (cells : ActiveCells) (store : Store)
    (binders : List BinderId) (values : List Value) (types : RuntimeType) (binder : BinderId) (cell : CellId)
    (agrees : ParameterCellsAgree activation cells store binders values types) (fresh : binder ∉ binders) :
    ParameterCellsAgree activation (bindActive cells binder cell) store binders values types := by
  induction agrees with
  | nil => exact .nil
  | @cons first rest value values ty types active present tail ih =>
    simp only [List.mem_cons, not_or] at fresh
    apply ParameterCellsAgree.cons
    · simpa [bindActive, Ne.symm fresh.1] using active
    · exact present
    · exact ih fresh.2

theorem bound_parameters_agree (activation : Nat) (binders : List BinderId)
    (values : List Value) (types : RuntimeType) (store next : Store) (cells : ActiveCells)
    (typed : ParameterValuesTyped binders values types) (unique : binders.Nodup)
    (bound : bindParameters activation binders values types store = .ok (cells, next)) :
    ParameterCellsAgree activation cells next binders values types := by
  induction typed generalizing store next cells with
  | nil => cases bound; exact .nil
  | @cons binder binders value values ty types valueTyped restTyped ih =>
    simp only [List.nodup_cons] at unique
    cases allocated : allocate store ⟨activation, binder⟩ .immutable ty value with
    | none => simp [bindParameters, valueTyped, allocated] at bound
    | some intermediate =>
      simp [bindParameters, valueTyped, allocated, Xsh.Typing.Connected.except_bind_ok] at bound
      obtain ⟨priorCells, finalStore, recursive, finished⟩ := bound
      split at finished
      · cases finished
      · cases finished
        have fresh : store ⟨activation, binder⟩ = none := by
          cases occupied : store ⟨activation, binder⟩ with
          | none => rfl
          | some entry => simp [allocate, allocateCell, occupied] at allocated
        obtain ⟨constructed, current, built, present, mutable, cellType, cellValue⟩ :=
          allocate_fresh_typed_cell store ⟨activation, binder⟩ .immutable value ty fresh valueTyped
        rw [allocated] at built
        cases built
        have preserved := parameters_preserve_existing activation binders values types intermediate next priorCells recursive
        have finalPresent := preserved _ _ present
        apply ParameterCellsAgree.cons
        · simp [bindActive]
        · exact ⟨current, finalPresent, mutable, cellType, cellValue⟩
        · exact parameter_cells_agree_after_binding activation priorCells next binders values types binder _
            (ih intermediate next priorCells unique.2 recursive) unique.1

end Xsh.Semantics.Connected
