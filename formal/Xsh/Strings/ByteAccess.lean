import Xsh.Strings.Search
import Xsh.Strings.Slice

/-!
This fragment models `Str` byte lookup and byte slicing over its UTF-8
encoding. Lookup uses unsigned bytes and rejects negative and past-end indices.
Slicing takes a byte offset and byte length, clamps the length to the remaining
encoding, and accepts only UTF-8 boundaries. This fragment excludes the `Bytes`
API, bracket slicing, parsing, runtime evaluation, and Rust correspondence.
-/

namespace Xsh.Strings.ByteAccess

/-- The finite byte sequence used to model XSH `Str` storage operations. -/
def utf8Bytes (text : String) : List UInt8 :=
  Xsh.Strings.Search.utf8Bytes text

/-- The byte length of a string's UTF-8 storage. -/
def byteLength (text : String) : Nat :=
  (utf8Bytes text).length

/-- Decode a finite byte sequence only when it contains complete UTF-8 text. -/
private def decodeUtf8 (bytes : List UInt8) : Option String :=
  String.fromUTF8? ⟨bytes.toArray⟩

/-- A byte offset is a boundary when both sides independently decode as UTF-8. -/
def isUtf8Boundary (text : String) (offset : Nat) : Bool :=
  let bytes := utf8Bytes text
  offset ≤ bytes.length &&
    (decodeUtf8 (bytes.take offset)).isSome &&
    (decodeUtf8 (bytes.drop offset)).isSome

/-- Look up one UTF-8 storage byte by its signed byte offset. -/
def byteAt? (text : String) (offset : Int) : Option UInt8 :=
  match offset with
  | .ofNat index => (utf8Bytes text)[index]?
  | .negSucc _ => none

/-- Validate and clamp a byte slice, returning its half-open byte range. -/
private def byteSliceRange? (text : String) (offset length : Int) : Option (Nat × Nat) :=
  match offset with
  | .negSucc _ => none
  | .ofNat start =>
    match length with
    | .negSucc _ => none
    | .ofNat requested =>
      let available := byteLength text - start
      if start ≤ byteLength text then
        let stop := start + min requested available
        if isUtf8Boundary text start && isUtf8Boundary text stop then
          some (start, stop)
        else none
      else none

/-- Select the exact UTF-8 bytes in a valid, length-clamped byte slice. -/
def byteSliceBytes? (text : String) (offset length : Int) : Option (List UInt8) :=
  (byteSliceRange? text offset length).map fun (start, stop) =>
    (utf8Bytes text).drop start |>.take (stop - start)

/-- Slice text by UTF-8 byte offset and byte length, rejecting invalid ranges. -/
def byteSlice? (text : String) (offset length : Int) : Option String :=
  (byteSliceBytes? text offset length).bind decodeUtf8

/-- The byte length is exactly the length of the shared UTF-8 encoding. -/
theorem byte_length_matches_search_encoding (text : String) :
    byteLength text = (Xsh.Strings.Search.utf8Bytes text).length := rfl

/-- Byte lookup returns the unsigned value stored at a UTF-8 position. -/
theorem byte_at_returns_unsigned_utf8_storage (text : String) (index : Nat) :
    byteAt? text (.ofNat index) = (utf8Bytes text)[index]? := rfl

/-- Negative signed byte indices are absent. -/
theorem negative_byte_index_is_absent (text : String) (index : Int)
    (negative : index < 0) :
    byteAt? text index = none := by
  cases index with
  | ofNat value => exact False.elim ((Int.not_lt.mpr (Int.natCast_nonneg value)) negative)
  | negSucc value => rfl

/-- Byte lookup past the UTF-8 storage end is absent. -/
theorem past_end_byte_index_is_absent (text : String) (index : Nat)
    (pastEnd : byteLength text ≤ index) :
    byteAt? text (.ofNat index) = none := by
  change (utf8Bytes text)[index]? = none
  apply List.getElem?_eq_none_iff.mpr
  exact pastEnd

/-- Negative slice offsets are rejected. -/
theorem negative_byte_slice_offset_is_rejected
    (text : String) (offset length : Int) (negative : offset < 0) :
    byteSlice? text offset length = none := by
  cases offset with
  | ofNat value => exact False.elim ((Int.not_lt.mpr (Int.natCast_nonneg value)) negative)
  | negSucc value => rfl

/-- Slice offsets strictly beyond the UTF-8 storage end are rejected. -/
theorem past_end_byte_slice_offset_is_rejected
    (text : String) (offset length : Nat) (pastEnd : byteLength text < offset) :
    byteSlice? text (.ofNat offset) (.ofNat length) = none := by
  have notWithin : ¬ offset ≤ byteLength text := by omega
  simp [byteSlice?, byteSliceBytes?, byteSliceRange?, notWithin]

/-- Negative slice lengths are rejected. -/
theorem negative_byte_slice_length_is_rejected
    (text : String) (offset length : Int) (negative : length < 0) :
    byteSlice? text offset length = none := by
  cases offset with
  | negSucc value => rfl
  | ofNat offsetValue =>
    cases length with
    | ofNat value => exact False.elim ((Int.not_lt.mpr (Int.natCast_nonneg value)) negative)
    | negSucc value => rfl

/-- A slice whose start splits a UTF-8 scalar is rejected. -/
theorem nonboundary_byte_slice_start_is_rejected :
    isUtf8Boundary "é" 1 = false ∧ byteSlice? "é" 1 1 = none := by
  decide

/-- A slice whose end splits a UTF-8 scalar is rejected. -/
theorem nonboundary_byte_slice_end_is_rejected :
    isUtf8Boundary "é" 1 = false ∧ byteSlice? "é" 0 1 = none := by
  decide

/-- A valid byte slice contains exactly the requested length, clamped to the
remaining bytes after its offset. -/
theorem byte_slice_length_is_clamped
    (text : String) (offset requested : Nat)
    (offsetWithin : offset ≤ byteLength text)
    (startBoundary : isUtf8Boundary text offset = true)
    (endBoundary : isUtf8Boundary text
      (offset + min requested (byteLength text - offset)) = true) :
    (byteSliceBytes? text (.ofNat offset) (.ofNat requested)).map List.length =
      some (min requested (byteLength text - offset)) := by
  have selectedRange : byteSliceRange? text (.ofNat offset) (.ofNat requested) =
      some (offset, offset + min requested (byteLength text - offset)) := by
    simp [byteSliceRange?, offsetWithin, startBoundary, endBoundary]
  unfold byteSliceBytes?
  rw [selectedRange]
  simp [byteLength, utf8Bytes, List.length_take, List.length_drop]

/-- Offset 1 and length 6 select exactly the UTF-8 bytes for `é🙂`. -/
theorem byte_slice_selects_exact_multibyte_range :
    byteSlice? "aé🙂z" 1 6 = some "é🙂" := by
  decide

/-- Byte slicing and scalar slicing select different ranges on multibyte text. -/
theorem byte_slice_differs_from_scalar_slice :
    byteSlice? "aé🙂z" 1 6 = some "é🙂" ∧
      String.ofList
        (Xsh.Strings.Slice.sliceText "aé🙂z".toList (some 1) (some 7)) = "é🙂z" ∧
      byteSlice? "aé🙂z" 1 6 ≠
        some (String.ofList
          (Xsh.Strings.Slice.sliceText "aé🙂z".toList (some 1) (some 7))) := by
  decide

end Xsh.Strings.ByteAccess
