import Xsh.Semantics.ListAccess

/-!
This fragment models XSH text as a finite sequence of Lean `Char` scalar
values. Slice bounds reuse the List normalization rule. The byte-offset
mutation deliberately applies UTF-8 byte offsets as scalar positions, so its
witness distinguishes byte counts from scalar counts. This fragment does not
model UTF-8 storage, Bytes APIs, text search, parsing, runtime evaluation, full
G1/G2 obligations, or Rust correspondence.
-/

namespace Xsh.Strings.Slice

/-- Text is represented by its Unicode scalar values. -/
abbrev Text := List Char

/-- Slice text with the same clamped, end-relative bounds as a List. -/
def sliceText (text : Text) (start stop : Option Int) : Text :=
  Xsh.Semantics.ListAccess.sliceList text start stop

/-- Text slicing is exactly the scalar-list slice operation. -/
theorem sliceText_is_scalar_list_slice (text : Text) (start stop : Option Int) :
    sliceText text start stop = Xsh.Semantics.ListAccess.sliceList text start stop := rfl

/-- Encoding a scalar slice as a Lean string preserves exactly those scalars. -/
theorem scalar_slice_round_trips_through_string
    (text : Text) (start stop : Option Int) :
    (String.ofList (sliceText text start stop)).toList =
      Xsh.Semantics.ListAccess.sliceList text start stop := by
  simp [sliceText]

/-- Omitting both bounds returns the complete scalar sequence. -/
theorem full_scalar_slice_is_identity (text : Text) :
    sliceText text none none = text := by
  simp [sliceText, Xsh.Semantics.ListAccess.sliceList, Xsh.Semantics.ListAccess.normalizeSliceBounds]

/-- Both normalized text bounds stay within the scalar count. -/
theorem normalized_text_bounds_are_clamped
    (text : Text) (start stop : Option Int) :
    let bounds := Xsh.Semantics.ListAccess.normalizeSliceBounds text.length start stop
    bounds.1 ≤ text.length ∧ bounds.2 ≤ text.length :=
  Xsh.Semantics.ListAccess.normalized_slice_bounds_are_clamped text.length start stop

/-- A slice contains at most the scalar span between its normalized bounds. -/
theorem scalar_slice_length_is_bounded_by_its_span
    (text : Text) (start stop : Option Int) :
    (sliceText text start stop).length ≤
      (Xsh.Semantics.ListAccess.normalizeSliceBounds text.length start stop).2 -
        (Xsh.Semantics.ListAccess.normalizeSliceBounds text.length start stop).1 := by
  exact List.length_take_le _ _

/-- A negative bound within the text counts backward from its scalar length. -/
theorem negative_text_bound_counts_from_end (text : Text) (distance : Nat)
    (distancePositive : 0 < distance)
    (distanceWithinText : distance ≤ text.length) :
    Xsh.Semantics.ListAccess.clampSliceBound text.length (-(Int.ofNat distance)) =
      text.length - distance := by
  cases distance with
  | zero => omega
  | succ distance =>
      by_cases lengthLe : text.length ≤ Nat.succ distance
      · have lengthEq : text.length = Nat.succ distance := by omega
        simp [Xsh.Semantics.ListAccess.clampSliceBound, lengthEq]
      · simp [Xsh.Semantics.ListAccess.clampSliceBound, lengthLe]

/-- A negative scalar range selects the corresponding suffix scalars. -/
theorem negative_scalar_bounds_count_from_end :
    sliceText ['a', 'é', '🦀'] (some (-2)) (some (-1)) = ['é'] := by
  decide

/-- If normalized bounds are reversed, the text slice is empty. -/
theorem reversed_normalized_text_bounds_are_empty
    (text : Text) (start stop : Option Int)
    (reversed :
      (Xsh.Semantics.ListAccess.normalizeSliceBounds text.length start stop).2 ≤
        (Xsh.Semantics.ListAccess.normalizeSliceBounds text.length start stop).1) :
    sliceText text start stop = [] := by
  have emptySpan := Nat.sub_eq_zero_of_le reversed
  simp [sliceText, Xsh.Semantics.ListAccess.sliceList, emptySpan]

/-- A scalar slice is an ordered sublist of its source text. -/
theorem scalar_slice_preserves_source_order
    (text : Text) (start stop : Option Int) :
    List.Sublist (sliceText text start stop) text := by
  change List.Sublist ((text.drop _).take _) text
  exact (List.take_sublist _ _).trans (List.drop_sublist _ _)

/-- This example has three Unicode scalars and seven UTF-8 bytes. -/
theorem multibyte_text_counts_scalars_and_utf8_bytes :
    let text : Text := "aé🦀".toList
    text.length = 3 ∧ (String.ofList text).utf8ByteSize = 7 := by
  decide

/-- Mutation that normalizes in bytes, then incorrectly indexes scalar values. -/
def byteOffsetSliceMutation (text : Text) (start stop : Option Int) : Text :=
  let byteLength := (String.ofList text).utf8ByteSize
  let bounds := Xsh.Semantics.ListAccess.normalizeSliceBounds byteLength start stop
  (text.drop bounds.1).take (bounds.2 - bounds.1)

/-- Negative byte offsets differ from negative scalar offsets on multibyte text. -/
theorem utf8_byte_offset_mutation_changes_scalar_slice :
    let text : Text := "aé🦀".toList
    sliceText text (some (-1)) none = ['🦀'] ∧
      byteOffsetSliceMutation text (some (-1)) none = [] ∧
      sliceText text (some (-1)) none ≠ byteOffsetSliceMutation text (some (-1)) none := by
  decide

end Xsh.Strings.Slice
