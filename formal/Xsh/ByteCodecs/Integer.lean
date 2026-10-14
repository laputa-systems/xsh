import Lean.Elab.Tactic.Omega
import Xsh.Values.Scalars

/-!
Finite semantics for the integer forms of `bytes.pack_le`, `pack_be`,
`unpack_le`, and `unpack_be`. The fragment does not establish Rust
correspondence.
-/

namespace Xsh.ByteCodecs.Integer

open Xsh.Values

/-- One unsigned byte, with values from zero through 255. -/
abbrev Byte := Fin 256

private inductive Endian where
  | little
  | big

private def supportedWidth (width : IntValue) : Bool :=
  width.val == 1 || width.val == 2 || width.val == 4 || width.val == 8

/-- The widths accepted by the integer Bytes operations are 1, 2, 4, and 8. -/
def validWidth (width : IntValue) : Bool := supportedWidth width

private def maximumPackNat (width : Nat) : Nat :=
  if width = 8 then 2 ^ 63 - 1 else 256 ^ width - 1

private def packLittleBytes : Nat → Nat → List Byte
  | 0, _ => []
  | width + 1, value =>
      ⟨value % 256, Nat.mod_lt _ (by decide)⟩ ::
        packLittleBytes width (value / 256)

private def packBytes (width : Nat) (endian : Endian) (value : Nat) : List Byte :=
  let little := packLittleBytes width value
  match endian with
  | .little => little
  | .big => little.reverse

private def packWithEndian? (value width : IntValue) (endian : Endian) :
    Option (List Byte) :=
  if validWidth width then
    if 0 ≤ value.val ∧ value.val.toNat ≤ maximumPackNat width.val.toNat then
      some (packBytes width.val.toNat endian value.val.toNat)
    else
      none
  else
    none

/-- Pack a signed 64-bit integer into one through eight bytes, using the
runtime's accepted range for the selected width. -/
def packLe? (value width : IntValue) : Option (List Byte) :=
  packWithEndian? value width .little

/-- Pack a signed 64-bit integer into one through eight bytes, using the
runtime's accepted range for the selected width. -/
def packBe? (value width : IntValue) : Option (List Byte) :=
  packWithEndian? value width .big

private def decodeLittle : List Byte → Nat
  | [] => 0
  | byte :: rest => byte.val + 256 * decodeLittle rest

private def decodeBig (bytes : List Byte) : Nat := decodeLittle bytes.reverse

private def decodeUnsigned (endian : Endian) (bytes : List Byte) : Nat :=
  match endian with
  | .little => decodeLittle bytes
  | .big => decodeBig bytes

private def decodeInteger (width value : Nat) : Int :=
  if width = 8 then
    if 2 ^ 63 ≤ value then
      Int.ofNat value - Int.ofNat (256 ^ 8)
    else
      Int.ofNat value
  else
    Int.ofNat value

private def unpackAt? (input : List Byte) (width offset : Nat) (endian : Endian) :
    Option Int :=
  if offset ≤ input.length then
    if width ≤ input.length - offset then
      some (decodeInteger width (decodeUnsigned endian ((input.drop offset).take width)))
    else
      none
  else
    none

private def unpackWithEndian? (input : List Byte) (width : IntValue)
    (offset : UIntValue) (endian : Endian) : Option Int :=
  if validWidth width then
    unpackAt? input width.val.toNat offset.val.toNat endian
  else
    none

/-- Decode bytes using little-endian order and a nonnegative byte offset. -/
def unpackLe? (data : List Byte) (width : IntValue) (offset : UIntValue) : Option Int :=
  unpackWithEndian? data width offset .little

/-- Decode bytes using big-endian order and a nonnegative byte offset. -/
def unpackBe? (data : List Byte) (width : IntValue) (offset : UIntValue) : Option Int :=
  unpackWithEndian? data width offset .big

private def zeroOffset : UIntValue := ⟨0, by unfold signed64Limit; omega⟩

private theorem packLittleBytes_length (width value : Nat) :
    (packLittleBytes width value).length = width := by
  induction width generalizing value with
  | zero => rfl
  | succ width ih => simp [packLittleBytes, ih]

private theorem decodeLittle_packLittle (width value : Nat)
    (fits : value < 256 ^ width) :
    decodeLittle (packLittleBytes width value) = value := by
  induction width generalizing value with
  | zero =>
      have valueZero : value = 0 := by simpa using fits
      simp [packLittleBytes, valueZero, decodeLittle]
  | succ width inductionHypothesis =>
      have quotientFits : value / 256 < 256 ^ width := by
        rw [Nat.pow_succ] at fits
        apply (Nat.div_lt_iff_lt_mul (by decide : 0 < 256)).2
        simpa [Nat.mul_comm] using fits
      rw [packLittleBytes, decodeLittle, inductionHypothesis _ quotientFits]
      exact Nat.mod_add_div value 256

private theorem packBytes_decode (width : Nat) (endian : Endian) (value : Nat)
    (fits : value < 256 ^ width) :
    decodeUnsigned endian (packBytes width endian value) = value := by
  cases endian with
  | little => exact decodeLittle_packLittle width value fits
  | big =>
      change decodeBig (packLittleBytes width value).reverse = value
      simp [decodeBig]
      exact decodeLittle_packLittle width value fits

private theorem maximumPack_lt_byteCapacity (width : Nat)
    (supported : width = 1 ∨ width = 2 ∨ width = 4 ∨ width = 8) :
    maximumPackNat width < 256 ^ width := by
  rcases supported with h | h | h | h <;> subst width <;> decide

private theorem maximumPack_lt_signedLimit (width : Nat)
    (supported : width = 1 ∨ width = 2 ∨ width = 4 ∨ width = 8) :
    maximumPackNat width < 2 ^ 63 := by
  rcases supported with h | h | h | h <;> subst width <;> decide

private theorem decodeInteger_small (width value : Nat)
    (small : value < 2 ^ 63) : decodeInteger width value = Int.ofNat value := by
  unfold decodeInteger
  by_cases widthIsEight : width = 8
  · simp [widthIsEight, Nat.not_le_of_gt small]
  · simp [widthIsEight]

/-- The valid width set is exactly 1, 2, 4, and 8. -/
theorem valid_widths (width : IntValue) :
    validWidth width = true ↔
      width.val = 1 ∨ width.val = 2 ∨ width.val = 4 ∨ width.val = 8 := by
  cases width with
  | mk value bounds =>
      cases value with
      | negSucc magnitude => simp [validWidth, supportedWidth]
      | ofNat natural => simp [validWidth, supportedWidth] <;> omega

/-- Unsupported widths, including the formerly accepted intermediate widths,
are rejected by pack and unpack. -/
theorem invalid_width_rejection :
    packLe? intZero (⟨3, by unfold signed64Min signed64Limit; omega⟩ : IntValue) = none ∧
      packBe? intZero (⟨9, by unfold signed64Min signed64Limit; omega⟩ : IntValue) = none ∧
      unpackLe? [] (⟨3, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) = none := by
  decide

/-- Negative values are rejected for every width and both pack byte orders. -/
theorem negative_pack_rejection (value width : IntValue) (negative : value.val < 0) :
    packLe? value width = none ∧ packBe? value width = none := by
  constructor
  · unfold packLe? packWithEndian?
    split
    · split
      · omega
      · rfl
    · rfl
  · unfold packBe? packWithEndian?
    split
    · split
      · omega
      · rfl
    · rfl

/-- Zero and each supported width's maximum fit; the next one-byte value does
not fit, while the width-eight signed maximum is accepted. -/
theorem pack_boundary_acceptance_and_overflow_rejection :
    (packLe? intZero (⟨1, by unfold signed64Min signed64Limit; omega⟩ : IntValue)).isSome = true ∧
      (packLe? (⟨255, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨1, by unfold signed64Min signed64Limit; omega⟩ : IntValue)).isSome = true ∧
      packLe? (⟨256, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨1, by unfold signed64Min signed64Limit; omega⟩ : IntValue) = none ∧
      (packLe? (⟨65535, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue)).isSome = true ∧
      (packLe? (⟨4294967295, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨4, by unfold signed64Min signed64Limit; omega⟩ : IntValue)).isSome = true ∧
      (packLe? intMax (⟨8, by unfold signed64Min signed64Limit; omega⟩ : IntValue)).isSome = true := by
  decide

/-- Packing emits the least significant byte first in little endian and the
most significant byte first in big endian. -/
theorem little_and_big_endian_byte_order :
    packLe? (⟨4660, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
      (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue) = some [52, 18] ∧
      packBe? (⟨4660, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue) = some [18, 52] := by
  decide

private theorem pack_acceptance_with_endian (value width : IntValue) (endian : Endian) :
    (packWithEndian? value width endian).isSome = true ↔
      validWidth width = true ∧ 0 ≤ value.val ∧
        value.val.toNat ≤ maximumPackNat width.val.toNat := by
  by_cases valid : validWidth width = true
  · by_cases fits : 0 ≤ value.val ∧
      value.val.toNat ≤ maximumPackNat width.val.toNat
    · simp [packWithEndian?, valid]
    · simp [packWithEndian?, valid]
  · simp [packWithEndian?, valid]

/-- For every integer and width, pack succeeds exactly in the runtime's
nonnegative fit range. Widths below eight use their full unsigned range;
width eight stops at the signed 64-bit maximum. -/
theorem pack_acceptance_matches_width_range (value width : IntValue) :
    ((packLe? value width).isSome = true ↔
      validWidth width = true ∧ 0 ≤ value.val ∧
        value.val.toNat ≤ (if width.val.toNat = 8 then 2 ^ 63 - 1
          else 256 ^ width.val.toNat - 1)) ∧
      ((packBe? value width).isSome = true ↔
        validWidth width = true ∧ 0 ≤ value.val ∧
          value.val.toNat ≤ (if width.val.toNat = 8 then 2 ^ 63 - 1
            else 256 ^ width.val.toNat - 1)) := by
  exact ⟨pack_acceptance_with_endian value width .little,
    pack_acceptance_with_endian value width .big⟩

private theorem unpackAt_success (input : List Byte) (width offset : Nat)
    (endian : Endian) (value : Int)
    (accepted : unpackAt? input width offset endian = some value) :
    value = decodeInteger width (decodeUnsigned endian ((input.drop offset).take width)) := by
  unfold unpackAt? at accepted
  split at accepted
  · split at accepted
    · injection accepted with equality
      exact equality.symm
    · simp at accepted
  · simp at accepted

private theorem pack_unpack_round_trip (value width : IntValue) (endian : Endian) :
    ∀ bytes, packWithEndian? value width endian = some bytes →
      unpackWithEndian? bytes width zeroOffset endian = some value.val := by
  intro bytes accepted
  by_cases valid : validWidth width = true
  · cases value with
    | mk valueInt valueBounds =>
      cases valueInt with
      | negSucc magnitude => simp [packWithEndian?, valid] at accepted
      | ofNat natural =>
        by_cases fits : natural ≤ maximumPackNat width.val.toNat
        · simp [packWithEndian?, valid, fits] at accepted
          have bytesEq : packBytes width.val.toNat endian natural = bytes := accepted
          subst bytes
          have fitsNat : natural ≤ maximumPackNat width.val.toNat := by
            exact fits
          have supported :
              width.val.toNat = 1 ∨ width.val.toNat = 2 ∨
                width.val.toNat = 4 ∨ width.val.toNat = 8 := by
            rcases (valid_widths width).mp valid with h | h | h | h
            · left; simp [h]
            · right; left; simp [h]
            · right; right; left; simp [h]
            · right; right; right; simp [h]
          have fitsBytes : natural < 256 ^ width.val.toNat :=
            Nat.lt_of_le_of_lt fitsNat
              (maximumPack_lt_byteCapacity width.val.toNat supported)
          have fitsSigned : natural < 2 ^ 63 :=
            Nat.lt_of_le_of_lt fitsNat
              (maximumPack_lt_signedLimit width.val.toNat supported)
          have decoded := packBytes_decode width.val.toNat endian natural fitsBytes
          have packedLength : (packBytes width.val.toNat endian natural).length = width.val.toNat := by
            cases endian <;> simp [packBytes, packLittleBytes_length]
          have selectedBytes :
              List.take width.val.toNat (packBytes width.val.toNat endian natural) =
                packBytes width.val.toNat endian natural := by
            apply List.take_of_length_le
            rw [packedLength]
            exact Nat.le_refl _
          have decodeValue := decodeInteger_small width.val.toNat natural fitsSigned
          simp [unpackWithEndian?, valid, unpackAt?, zeroOffset, packedLength,
            selectedBytes, decoded, decodeValue]
        · simp [packWithEndian?, valid] at accepted
          omega
  · simp [packWithEndian?, valid] at accepted

/-- Every accepted pack round-trips at offset zero in both byte orders. -/
theorem fit_range_pack_unpack_round_trip (value width : IntValue) :
    (∀ bytes, packLe? value width = some bytes →
      unpackLe? bytes width (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) =
        some value.val) ∧
      (∀ bytes, packBe? value width = some bytes →
        unpackBe? bytes width (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) =
          some value.val) := by
  exact ⟨pack_unpack_round_trip value width .little,
    pack_unpack_round_trip value width .big⟩

private theorem unpack_success_is_decoded (input : List Byte) (width : IntValue)
    (offset : UIntValue) (endian : Endian) (value : Int)
    (accepted : unpackWithEndian? input width offset endian = some value) :
    value = decodeInteger width.val.toNat
      (decodeUnsigned endian ((input.drop offset.val.toNat).take width.val.toNat)) := by
  by_cases valid : validWidth width = true
  · simp [unpackWithEndian?, valid] at accepted
    have decoded := unpackAt_success input width.val.toNat offset.val.toNat endian
      value accepted
    exact decoded
  · simp [unpackWithEndian?, valid] at accepted

private theorem decodeInteger_lower_width (width value : Nat)
    (lower : width = 1 ∨ width = 2 ∨ width = 4) :
    decodeInteger width value = Int.ofNat value := by
  have notEight : width ≠ 8 := by
    rcases lower with h | h | h <;> omega
  unfold decodeInteger
  simp [notEight]

private theorem take_length_le (bytes : List Byte) (width : Nat) :
    (bytes.take width).length ≤ width := by
  induction width generalizing bytes with
  | zero => simp
  | succ width inductionHypothesis =>
      cases bytes with
      | nil => simp
      | cons byte rest =>
          simp only [List.take_succ_cons, List.length_cons]
          exact Nat.succ_le_succ (inductionHypothesis rest)

private theorem decodeLittle_lt_capacity (bytes : List Byte) :
    decodeLittle bytes < 256 ^ bytes.length := by
  induction bytes with
  | nil => simp [decodeLittle]
  | cons byte rest inductionHypothesis =>
      have byteBound : byte.val < 256 := byte.isLt
      simp only [decodeLittle, List.length_cons, Nat.pow_succ]
      omega

private theorem decodeUnsigned_lt_capacity (endian : Endian) (bytes : List Byte) :
    decodeUnsigned endian bytes < 256 ^ bytes.length := by
  cases endian with
  | little => exact decodeLittle_lt_capacity bytes
  | big =>
      simpa [decodeUnsigned, decodeBig] using
        decodeLittle_lt_capacity bytes.reverse

private theorem lower_width_unpack_success_bound (input : List Byte)
    (width : IntValue) (offset : UIntValue) (endian : Endian)
    (lower : width.val.toNat = 1 ∨ width.val.toNat = 2 ∨ width.val.toNat = 4)
    (value : Int)
    (accepted : unpackWithEndian? input width offset endian = some value) :
    0 ≤ value ∧ value < 256 ^ width.val.toNat := by
  let selected := (input.drop offset.val.toNat).take width.val.toNat
  let raw := decodeUnsigned endian selected
  have equality := unpack_success_is_decoded input width offset endian value accepted
  have valueIsRaw : value = decodeInteger width.val.toNat raw := by
    simpa [raw, selected] using equality
  have selectedLength : selected.length ≤ width.val.toNat :=
    take_length_le (input.drop offset.val.toNat) width.val.toNat
  have rawBound : raw < 256 ^ width.val.toNat :=
    Nat.lt_of_lt_of_le (decodeUnsigned_lt_capacity endian selected)
      (Nat.pow_le_pow_right (by decide) selectedLength)
  rw [valueIsRaw, decodeInteger_lower_width _ _ lower]
  constructor
  · exact Int.natCast_nonneg raw
  · exact Int.ofNat_lt.mpr rawBound

/-- Widths one, two, and four decode as unsigned values with their high bytes
zero-extended, for both endian orders. -/
theorem lower_width_unpack_zero_extends (input : List Byte) (width : IntValue)
    (offset : UIntValue)
    (lower : width.val.toNat = 1 ∨ width.val.toNat = 2 ∨ width.val.toNat = 4) :
    (∀ value, unpackLe? input width offset = some value →
      0 ≤ value ∧ value < 256 ^ width.val.toNat) ∧
      (∀ value, unpackBe? input width offset = some value →
        0 ≤ value ∧ value < 256 ^ width.val.toNat) := by
  constructor <;> intro value accepted
  · exact lower_width_unpack_success_bound input width offset .little lower value accepted
  · exact lower_width_unpack_success_bound input width offset .big lower value accepted

/-- Eight-byte decoding preserves the signed 64-bit boundary: the sign bit
maps all-one bytes to negative one and the high-bit-only word to Int minimum. -/
theorem width_eight_signed_decode :
    unpackLe? [255, 255, 255, 255, 255, 255, 255, 255]
      (⟨8, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
      (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) = some (-1) ∧
      unpackBe? [128, 0, 0, 0, 0, 0, 0, 0]
        (⟨8, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) =
          some signed64Min := by
  decide

/-- An unsigned offset beyond the byte input is rejected. -/
theorem invalid_offset_rejection :
    unpackLe? [] (⟨1, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
      (uintMax : UIntValue) = none := by
  decide

/-- A valid offset with fewer than the requested bytes is rejected. -/
theorem short_input_rejection :
    unpackBe? [42] (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
      (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) = none := by
  decide

/-- Reading a little-endian packed word as big endian changes 4660 to 13330. -/
theorem endian_swap_mutation_changes_value :
    packLe? (⟨4660, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
      (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue) = some [52, 18] ∧
      unpackLe? [52, 18] (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) = some 4660 ∧
      unpackBe? [52, 18] (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) = some 13330 ∧
      unpackLe? [52, 18] (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) ≠
      unpackBe? [52, 18] (⟨2, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
        (⟨0, by unfold signed64Limit; omega⟩ : UIntValue) := by
  decide

end Xsh.ByteCodecs.Integer
