/-!
This fragment models finite `Bytes` values as byte lists. Lookup and slicing
use signed integer offsets, reject negative or past-end positions, and return
raw bytes without UTF-8 decoding or boundary restrictions. It excludes `Str`
slicing, bracket slicing, source spans, checker diagnostics, runtime behavior,
and Rust correspondence.
-/

namespace Xsh.Bytes.Access

/-- Read one unsigned byte at a signed offset in a finite byte sequence. -/
def byteAt? (bytes : List UInt8) (offset : Int) : Option UInt8 :=
  match offset with
  | .ofNat index => bytes[index]?
  | .negSucc _ => none

/-- Select raw bytes from a valid offset; an omitted length selects the suffix. -/
def slice? (bytes : List UInt8) (offset : Int) (length : Option Int) : Option (List UInt8) :=
  match offset with
  | .negSucc _ => none
  | .ofNat start =>
    if start ≤ bytes.length then
      match length with
      | none => some (bytes.drop start)
      | some (.ofNat requested) => some ((bytes.drop start).take requested)
      | some (.negSucc _) => none
    else none

/-- Nonnegative lookup returns the list's optional unsigned byte at that index. -/
theorem unsigned_lookup_matches_list (bytes : List UInt8) (index : Nat) :
    byteAt? bytes (.ofNat index) = bytes[index]? := rfl

/-- Every negative signed lookup has no byte value. -/
theorem negative_lookup_is_absent (bytes : List UInt8) (index : Int)
    (negative : index < 0) :
    byteAt? bytes index = none := by
  cases index with
  | ofNat value => exact False.elim ((Int.not_lt.mpr (Int.natCast_nonneg value)) negative)
  | negSucc value => rfl

/-- Lookup at or beyond the sequence end is absent. -/
theorem past_end_lookup_is_absent (bytes : List UInt8) (index : Nat)
    (pastEnd : bytes.length ≤ index) :
    byteAt? bytes (.ofNat index) = none := by
  change bytes[index]? = none
  apply List.getElem?_eq_none_iff.mpr
  exact pastEnd

/-- Offset zero reads the first unsigned byte. -/
theorem zero_offset_reads_head (head : UInt8) (tail : List UInt8) :
    byteAt? (head :: tail) 0 = some head := rfl

/-- The final position is readable and the following end position is absent. -/
theorem last_and_end_offsets_are_distinct :
    byteAt? [UInt8.ofNat 4, UInt8.ofNat 255] 1 = some (UInt8.ofNat 255) ∧
      byteAt? [UInt8.ofNat 4, UInt8.ofNat 255] 2 = none := by
  decide

/-- An omitted length returns the complete suffix beginning at a valid offset. -/
theorem omitted_length_returns_suffix (bytes : List UInt8) (offset : Nat)
    (within : offset ≤ bytes.length) :
    slice? bytes (.ofNat offset) none = some (bytes.drop offset) := by
  simp [slice?, within]

/-- A concrete in-bounds range returns exactly its requested byte. -/
theorem exact_length_slice_selects_requested_bytes :
    slice? [UInt8.ofNat 16, UInt8.ofNat 255, UInt8.ofNat 32] 1 (some 1) =
      some [UInt8.ofNat 255] := by
  decide

/-- A zero-length range at any valid offset is the empty byte list. -/
theorem zero_length_slice_is_empty (bytes : List UInt8) (offset : Nat)
    (within : offset ≤ bytes.length) :
    slice? bytes (.ofNat offset) (some 0) = some [] := by
  simp [slice?, within]

/-- Explicit slice lengths clamp to the bytes remaining after the offset. -/
theorem slice_length_is_clamped (bytes : List UInt8) (offset requested : Nat)
    (within : offset ≤ bytes.length) :
    (slice? bytes (.ofNat offset) (some (.ofNat requested))).map List.length =
      some (min requested (bytes.length - offset)) := by
  simp [slice?, within, List.length_take, List.length_drop]

/-- Negative and strictly past-end slice offsets are rejected. -/
theorem invalid_slice_offsets_are_rejected
    (bytes : List UInt8) (negativeOffset : Int) (pastEndOffset : Nat)
    (negative : negativeOffset < 0) (pastEnd : bytes.length < pastEndOffset) :
    slice? bytes negativeOffset none = none ∧
      slice? bytes (.ofNat pastEndOffset) none = none := by
  constructor
  · cases negativeOffset with
    | ofNat value => exact False.elim ((Int.not_lt.mpr (Int.natCast_nonneg value)) negative)
    | negSucc value => rfl
  · simp [slice?, Nat.not_le.mpr pastEnd]

/-- A negative explicit length never selects a slice. -/
theorem negative_slice_length_is_rejected
    (bytes : List UInt8) (offset : Nat) (length : Int) (negative : length < 0) :
    slice? bytes (.ofNat offset) (some length) = none := by
  cases length with
  | ofNat value => exact False.elim ((Int.not_lt.mpr (Int.natCast_nonneg value)) negative)
  | negSucc value => simp [slice?]

/-- Raw `0xff` is a valid byte even though it cannot decode as UTF-8 text. -/
theorem raw_ff_byte_does_not_require_utf8_decoding :
    byteAt? [UInt8.ofNat 255] 0 = some (UInt8.ofNat 255) ∧
      slice? [UInt8.ofNat 255] 0 (some 1) = some [UInt8.ofNat 255] ∧
      String.fromUTF8? ⟨#[UInt8.ofNat 255]⟩ = none := by
  decide

end Xsh.Bytes.Access
