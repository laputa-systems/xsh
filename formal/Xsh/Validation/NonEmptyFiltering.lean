import Xsh.Validation.Boundary

/-!
Filtering can remove every element, so a filtered list must pass through the
non-empty boundary before it is exposed as a checked value. Successful
validation preserves the filtered sequence exactly. These are independent
Lean fragments and do not close the full validated-type obligations or
establish checker, runtime, or Rust correspondence.
-/

namespace Xsh.Validation.NonEmptyFiltering

/-- Check a filtered list through the shared non-empty validation boundary. -/
def filterThenRequire {α : Type} (values : List α) (keep : α → Bool) :
    RequireOutcome (NonEmptyList α) :=
  NonEmptyList.require (values.filter keep)

theorem filter_is_nonempty_iff_survivor {α : Type} (values : List α)
    (keep : α → Bool) :
    values.filter keep ≠ [] ↔ ∃ value, value ∈ values ∧ keep value = true := by
  induction values with
  | nil => simp
  | cons head tail inductionHypothesis =>
      cases kept : keep head <;> simp [kept, inductionHypothesis]

theorem filterThenRequire_accepts_iff_survivor {α : Type} (values : List α)
    (keep : α → Bool) :
    (∃ checked : NonEmptyList α, filterThenRequire values keep = .accepted checked) ↔
      ∃ value, value ∈ values ∧ keep value = true := by
  constructor
  · rintro ⟨checked, success⟩
    have filteredNonempty : values.filter keep ≠ [] :=
      NonEmptyList.require_success_proves_nonempty success
    exact filter_is_nonempty_iff_survivor values keep |>.mp filteredNonempty
  · intro survivor
    have filteredNonempty : values.filter keep ≠ [] :=
      filter_is_nonempty_iff_survivor values keep |>.mpr survivor
    obtain ⟨checked, success, _⟩ :=
      NonEmptyList.require_accepts_nonempty filteredNonempty
    exact ⟨checked, success⟩

theorem filterThenRequire_success_erases_exactly {α : Type} (values : List α)
    (keep : α → Bool) (checked : NonEmptyList α)
    (success : filterThenRequire values keep = .accepted checked) :
    NonEmptyList.erase checked = values.filter keep :=
  NonEmptyList.require_success_preserves_input success

theorem all_false_filter_rejects {α : Type} (values : List α) :
    filterThenRequire values (fun _ => false) = .rejected := by
  have noElements : values.filter (fun _ => false) = [] := by
    induction values with
    | nil => rfl
    | cons head tail inductionHypothesis => simp [inductionHypothesis]
  unfold filterThenRequire
  rw [noElements]
  rfl

/-- A guard-removal mutation that accepts the filtered list without validation. -/
def acceptFilteredWithoutNonemptyGuard {α : Type} (values : List α)
    (keep : α → Bool) : RequireOutcome (List α) :=
  .accepted (values.filter keep)

theorem removing_nonempty_guard_is_detected {α : Type} (values : List α) :
    filterThenRequire values (fun _ => false) = .rejected ∧
      acceptFilteredWithoutNonemptyGuard values (fun _ => false) =
        .accepted ([] : List α) := by
  exact ⟨all_false_filter_rejects values, by simp [acceptFilteredWithoutNonemptyGuard]⟩

end Xsh.Validation.NonEmptyFiltering
