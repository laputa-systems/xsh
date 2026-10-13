/-!
This fragment models List reads, assignment indices, and slice bounds over
mathematical integers and natural-number lengths. `Index.literal` retains the
source distinction needed for negative-literal indexing; `Index.computed`
represents a run-time integer. The model does not cover parsing, signed-64-bit
overflow, String or Bytes slicing, or correspondence with the Rust evaluator.
-/

namespace Xsh.Semantics.ListAccess

/-- An index records whether its integer came directly from a literal. -/
inductive Index where
  | literal (value : Int)
  | computed (value : Int)
  deriving DecidableEq, Repr

/-- Resolve a List read index; only a negative literal counts from the end. -/
def normalizeReadIndex (length : Nat) : Index → Option Nat
  | .literal value =>
      if value < 0 then
        let magnitude := (-value).toNat
        if magnitude ≤ length then some (length - magnitude) else none
      else
        let position := value.toNat
        if position < length then some position else none
  | .computed value =>
      if value < 0 then none
      else
        let position := value.toNat
        if position < length then some position else none

/-- Read an element after applying the List-specific index rule. -/
def readList? {α : Type} (values : List α) (index : Index) : Option α :=
  match normalizeReadIndex values.length index with
  | some position => values[position]?
  | none => none

/-- Assignment uses only ordinary nonnegative positions, even for literals. -/
def normalizeAssignmentIndex (length : Nat) (index : Index) : Option Nat :=
  match index with
  | .literal value =>
      if value < 0 then none
      else
        let position := value.toNat
        if position < length then some position else none
  | .computed value =>
      if value < 0 then none
      else
        let position := value.toNat
        if position < length then some position else none

/-- Translate a negative slice bound from the end, then clamp it to the list. -/
def clampSliceBound (length : Nat) (bound : Int) : Nat :=
  if bound < 0 then
    let magnitude := (-bound).toNat
    if length ≤ magnitude then 0 else length - magnitude
  else
    min bound.toNat length

/-- Omitted slice bounds default to the beginning and end respectively. -/
def normalizeSliceBounds (length : Nat) (start stop : Option Int) : Nat × Nat :=
  let lower := match start with
    | none => 0
    | some bound => clampSliceBound length bound
  let upper := match stop with
    | none => length
    | some bound => clampSliceBound length bound
  (lower, upper)

/-- Slice a list using clamped, end-relative bounds; reversed bounds are empty. -/
def sliceList {α : Type} (values : List α) (start stop : Option Int) : List α :=
  let (lower, upper) := normalizeSliceBounds values.length start stop
  (values.drop lower).take (upper - lower)

/-- Mutant that applies literal indexing rules to every computed integer. -/
def computedNegativeFromEndMutation (length : Nat) (value : Int) : Option Nat :=
  normalizeReadIndex length (.literal value)

theorem nonnegative_literal_reads_its_position :
    readList? [10, 20, 30] (.literal 1) = some 20 := by
  decide

theorem negative_literal_reads_from_the_end :
    readList? [10, 20, 30] (.literal (-1)) = some 30 := by
  decide

theorem negative_zero_literal_is_zero :
    readList? [10, 20, 30] (.literal (-(0 : Int))) = some 10 := by
  decide

theorem too_negative_literal_is_rejected :
    readList? [10, 20, 30] (.literal (-4)) = none := by
  decide

theorem computed_negative_index_is_rejected :
    readList? [10, 20, 30] (.computed (-1)) = none := by
  decide

theorem negative_assignment_literal_does_not_count_from_end :
    normalizeAssignmentIndex 3 (.literal (-1)) = none := by
  decide

theorem normalized_slice_bounds_are_clamped (length : Nat) (start stop : Option Int) :
    let bounds := normalizeSliceBounds length start stop
    bounds.1 ≤ length ∧ bounds.2 ≤ length := by
  have clamp_le (bound : Int) : clampSliceBound length bound ≤ length := by
    unfold clampSliceBound
    by_cases negative : bound < 0
    · simp [negative]
      split <;> omega
    · simp [negative]
      exact Nat.min_le_right _ _
  cases start <;> cases stop <;>
    simp [normalizeSliceBounds, clamp_le] <;> omega

theorem out_of_range_slice_bounds_clamp_to_the_list :
    sliceList [10, 20, 30] (some (-8)) (some 12) = [10, 20, 30] := by
  decide

theorem negative_slice_bounds_count_from_the_end :
    sliceList [10, 20, 30, 40] (some (-3)) (some (-1)) = [20, 30] := by
  decide

theorem reversed_slice_bounds_produce_an_empty_list :
    sliceList [10, 20, 30, 40] (some 3) (some 1) = [] := by
  decide

theorem computed_negative_from_end_mutation_is_detectable :
    normalizeReadIndex 3 (.computed (-1)) = none ∧
      computedNegativeFromEndMutation 3 (-1) = some 2 := by
  decide

end Xsh.Semantics.ListAccess
