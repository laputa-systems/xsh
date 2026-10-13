import Lean.Elab.Tactic.Omega
import Init.Data.BitVec.Lemmas
import Xsh.Foundation

namespace Xsh.Values

def signed64Min : Int := -(2 ^ 63)
def signed64Limit : Int := 2 ^ 63
def signed64Width : Nat := 2 ^ 64

/-- XSH `Int` values are exactly the two's-complement signed 64-bit domain. -/
abbrev IntValue := { value : Int // signed64Min ≤ value ∧ value < signed64Limit }

/-- `UInt` uses the signed integer representation and admits its nonnegative half. -/
abbrev UIntValue := { value : Int // 0 ≤ value ∧ value < signed64Limit }

/-- Durations store unsigned 64-bit millisecond counts. -/
abbrev DurationValue := { millis : Nat // millis < signed64Width }

abbrev IntWord := BitVec 64

def word32Width : Nat := 4294967296

/-- The two little-endian transport limbs of a raw 64-bit scalar payload.
These words encode bits only; source binders and name-table IDs keep their
separate Foundation types. -/
structure Word64Parts where
  low : Xsh.Foundation.Word32
  high : Xsh.Foundation.Word32
  deriving DecidableEq

def Word64Parts.value (parts : Word64Parts) : Nat :=
  parts.low.val + word32Width * parts.high.val

theorem Word64Parts.value_lt_width (parts : Word64Parts) :
    parts.value < signed64Width := by
  have lowBound := parts.low.isLt
  have highBound := parts.high.isLt
  have widthProduct : word32Width * word32Width = signed64Width := by decide
  unfold Word64Parts.value
  rw [← widthProduct]
  unfold word32Width
  omega

def Word64Parts.toWord (parts : Word64Parts) : IntWord :=
  BitVec.ofFin ⟨parts.value, parts.value_lt_width⟩

def Word64Parts.fromWord (word : IntWord) : Word64Parts :=
  ⟨⟨word.toFin.val % word32Width, Nat.mod_lt _ (by unfold word32Width; omega)⟩,
   ⟨word.toFin.val / word32Width, by
     have wordBound : word.toFin.val < word32Width * word32Width := by
       have widthProduct : word32Width * word32Width = signed64Width := by decide
       rw [widthProduct]
       exact word.toFin.isLt
     have quotientBound : word.toFin.val / word32Width < word32Width := by
       exact (Nat.div_lt_iff_lt_mul (by unfold word32Width; omega)).2 wordBound
     exact quotientBound⟩⟩

theorem word64_parts_round_trip (word : IntWord) :
    Word64Parts.toWord (Word64Parts.fromWord word) = word := by
  apply BitVec.toFin_inj.mp
  apply Fin.ext
  simp [Word64Parts.toWord, Word64Parts.fromWord, Word64Parts.value]
  exact Nat.mod_add_div word.toFin.val word32Width

/-- Encode a signed value as its exact two's-complement 64-bit word. -/
def encodeInt (value : IntValue) : IntWord :=
  BitVec.ofInt 64 value.val

/-- Decode every 64-bit word to the signed value with the same bit pattern. -/
def decodeInt (word : IntWord) : IntValue :=
  ⟨word.toInt, by
    constructor
    · simpa [signed64Min] using BitVec.le_toInt word
    · simpa [signed64Limit] using (BitVec.toInt_lt (x := word))⟩

theorem decode_encode_int (value : IntValue) : decodeInt (encodeInt value) = value := by
  apply Subtype.ext
  exact BitVec.toInt_ofInt_eq_self (by omega)
    (by simpa [signed64Min] using value.property.1)
    (by simpa [signed64Limit] using value.property.2)

theorem encode_decode_int (word : IntWord) : encodeInt (decodeInt word) = word := by
  simp [encodeInt, decodeInt]

def intMin : IntValue := ⟨signed64Min, by unfold signed64Min signed64Limit; omega⟩
def intMax : IntValue := ⟨signed64Limit - 1, by unfold signed64Min signed64Limit; omega⟩
def intOne : IntValue := ⟨1, by unfold signed64Min signed64Limit; omega⟩
def intZero : IntValue := ⟨0, by unfold signed64Min signed64Limit; omega⟩
def intNegativeOne : IntValue := ⟨-1, by unfold signed64Min signed64Limit; omega⟩

theorem encode_min_int_sets_sign_bit : (encodeInt intMin).toNat = 2 ^ 63 := by
  decide

theorem encode_max_int_keeps_sign_clear : (encodeInt intMax).toNat = 2 ^ 63 - 1 := by
  decide

theorem encode_negative_one_sets_all_bits :
    (encodeInt intNegativeOne).toNat = signed64Width - 1 := by
  decide

theorem encode_zero_int_keeps_zero_bits : (encodeInt intZero).toNat = 0 := by
  decide

def validateInt? (value : Int) : Option IntValue :=
  if h : signed64Min ≤ value ∧ value < signed64Limit then some ⟨value, h⟩ else none

theorem validate_int_accepts_min : validateInt? signed64Min = some intMin := by
  decide

theorem validate_int_accepts_max : validateInt? (signed64Limit - 1) = some intMax := by
  decide

theorem validate_int_rejects_below_min (value : Int) (below : value < signed64Min) :
    validateInt? value = none := by
  have outside : ¬ signed64Min ≤ value := by omega
  simp [validateInt?, outside]

theorem validate_int_rejects_at_upper_limit : validateInt? signed64Limit = none := by
  simp [validateInt?]

/-- Checked integer addition returns no value when its mathematical sum leaves Int. -/
def addInt? (left right : IntValue) : Option IntValue :=
  let sum := left.val + right.val
  if h : signed64Min ≤ sum ∧ sum < signed64Limit then
    some ⟨sum, h⟩
  else
    none

def subtractInt? (left right : IntValue) : Option IntValue :=
  let difference := left.val - right.val
  if h : signed64Min ≤ difference ∧ difference < signed64Limit then
    some ⟨difference, h⟩
  else
    none

theorem max_int_plus_one_overflows : addInt? intMax intOne = none := by
  decide

theorem min_int_minus_one_underflows : subtractInt? intMin intOne = none := by
  decide

def validateUInt? (value : Int) : Option UIntValue :=
  if h : 0 ≤ value ∧ value < signed64Limit then some ⟨value, h⟩ else none

def UIntValue.toInt (value : UIntValue) : IntValue :=
  ⟨value.val, by
    constructor
    · unfold signed64Min
      omega
    · exact value.property.2⟩

def IntValue.toUInt? (value : IntValue) : Option UIntValue :=
  if nonnegative : 0 ≤ value.val then
    some ⟨value.val, ⟨nonnegative, value.property.2⟩⟩
  else
    none

theorem uint_to_int_round_trip (value : UIntValue) : value.toInt.toUInt? = some value := by
  simp [UIntValue.toInt, IntValue.toUInt?, value.property.1]

theorem validate_uint_rejects_negative (value : Int) (negative : value < 0) :
    validateUInt? value = none := by
  have notNonnegative : ¬ 0 ≤ value := by omega
  simp [validateUInt?, notNonnegative]

theorem validate_uint_accepts_in_range (value : Int) (nonnegative : 0 ≤ value)
    (belowLimit : value < signed64Limit) :
    validateUInt? value = some ⟨value, ⟨nonnegative, belowLimit⟩⟩ := by
  simp [validateUInt?, nonnegative, belowLimit]

theorem validate_uint_rejects_at_upper_limit : validateUInt? signed64Limit = none := by
  simp [validateUInt?]

def uintMax : UIntValue := ⟨signed64Limit - 1, by
  unfold signed64Limit
  omega⟩

theorem uint_max_round_trip : uintMax.toInt.toUInt? = some uintMax :=
  uint_to_int_round_trip uintMax

theorem validate_uint_accepts_zero : validateUInt? 0 = some ⟨0, by decide⟩ := by
  decide

def durationZero : DurationValue := ⟨0, by decide⟩
def durationMax : DurationValue := ⟨signed64Width - 1, by unfold signed64Width; omega⟩

def validateDuration? (millis : Nat) : Option DurationValue :=
  if h : millis < signed64Width then some ⟨millis, h⟩ else none

def addDuration? (left right : DurationValue) : Option DurationValue :=
  let sum := left.val + right.val
  if h : sum < signed64Width then some ⟨sum, h⟩ else none

def durationOne : DurationValue := ⟨1, by decide⟩

theorem max_duration_plus_one_overflows : addDuration? durationMax durationOne = none := by
  decide

theorem validate_duration_accepts_zero : validateDuration? 0 = some durationZero := by
  decide

theorem validate_duration_accepts_max :
    validateDuration? (signed64Width - 1) = some durationMax := by
  decide

theorem validate_duration_rejects_at_width : validateDuration? signed64Width = none := by
  decide

abbrev FloatBits := Fin signed64Width

def encodeFloatParts (bits : FloatBits) : Word64Parts :=
  Word64Parts.fromWord (BitVec.ofFin bits)

def decodeFloatParts (parts : Word64Parts) : FloatBits :=
  (Word64Parts.toWord parts).toFin

theorem float_parts_round_trip (bits : FloatBits) :
    decodeFloatParts (encodeFloatParts bits) = bits := by
  simp only [decodeFloatParts, encodeFloatParts]
  rw [word64_parts_round_trip]

def floatSign : Nat := 2 ^ 63
def floatFractionWidth : Nat := 2 ^ 52
def floatExponentLimit : Nat := 2 ^ 11

def floatExponent (bits : FloatBits) : Nat := bits.val / floatFractionWidth % floatExponentLimit
def floatFraction (bits : FloatBits) : Nat := bits.val % floatFractionWidth
def floatIsNaN (bits : FloatBits) : Prop :=
  floatExponent bits = floatExponentLimit - 1 ∧ floatFraction bits ≠ 0
def floatIsZero (bits : FloatBits) : Prop := bits.val % floatSign = 0

/-- IEEE value equality keeps every raw bit pattern but identifies signed zero
and makes every comparison involving a NaN false. -/
def ieeeFloatEq (left right : FloatBits) : Prop :=
  ¬ floatIsNaN left ∧ ¬ floatIsNaN right ∧
    (floatIsZero left ∧ floatIsZero right ∨ left.val = right.val)

def floatPositiveZero : FloatBits := ⟨0, by decide⟩
def floatNegativeZero : FloatBits := ⟨floatSign, by unfold floatSign signed64Width; omega⟩
def floatQuietNaN : FloatBits := ⟨2 ^ 63 - 2 ^ 52 + 2 ^ 51, by unfold signed64Width; omega⟩
def floatNegativeQuietNaN : FloatBits :=
  ⟨floatSign + (floatExponentLimit - 1) * floatFractionWidth + 2 ^ 51, by
    unfold floatSign floatExponentLimit floatFractionWidth signed64Width
    omega⟩
def floatNegativeInfinity : FloatBits :=
  ⟨floatSign + (floatExponentLimit - 1) * floatFractionWidth, by
    unfold floatSign floatExponentLimit floatFractionWidth signed64Width
    omega⟩
def floatPositiveInfinity : FloatBits :=
  ⟨(floatExponentLimit - 1) * floatFractionWidth, by
    unfold floatExponentLimit floatFractionWidth signed64Width
    omega⟩

def decodeFloatBits? (raw : Nat) : Option FloatBits :=
  if fits : raw < signed64Width then some ⟨raw, fits⟩ else none

theorem decode_float_bits_round_trip (bits : FloatBits) : decodeFloatBits? bits.val = some bits := by
  simp [decodeFloatBits?, bits.isLt]

theorem decode_float_bits_rejects_wide_value : decodeFloatBits? signed64Width = none := by
  simp [decodeFloatBits?]

theorem ieee_signed_zeros_equal : ieeeFloatEq floatPositiveZero floatNegativeZero := by
  have positiveNotNaN : ¬ floatIsNaN floatPositiveZero := by
    simp [floatIsNaN, floatExponent, floatFraction, floatPositiveZero,
      floatExponentLimit, floatFractionWidth]
  have negativeNotNaN : ¬ floatIsNaN floatNegativeZero := by
    simp [floatIsNaN, floatExponent, floatFraction, floatNegativeZero,
      floatExponentLimit, floatFractionWidth, floatSign]
  have positiveIsZero : floatIsZero floatPositiveZero := by
    simp [floatIsZero, floatPositiveZero, floatSign]
  have negativeIsZero : floatIsZero floatNegativeZero := by
    simp [floatIsZero, floatNegativeZero, floatSign]
  exact ⟨positiveNotNaN, negativeNotNaN, Or.inl ⟨positiveIsZero, negativeIsZero⟩⟩

theorem signed_zero_raw_bits_differ :
    floatPositiveZero.val ≠ floatNegativeZero.val := by
  decide

theorem nan_never_equals (left right : FloatBits) (isNan : floatIsNaN left) :
    ¬ ieeeFloatEq left right := by
  simp [ieeeFloatEq, isNan]

theorem nan_never_equals_on_right (left right : FloatBits) (isNan : floatIsNaN right) :
    ¬ ieeeFloatEq left right := by
  simp [ieeeFloatEq, isNan]

theorem same_bit_nan_is_unequal : ¬ ieeeFloatEq floatQuietNaN floatQuietNaN := by
  apply nan_never_equals
  simp [floatIsNaN, floatExponent, floatFraction, floatQuietNaN,
    floatExponentLimit, floatFractionWidth]

theorem non_nan_equals_itself (bits : FloatBits) (notNan : ¬ floatIsNaN bits) :
    ieeeFloatEq bits bits := by
  simp [ieeeFloatEq, notNan]

/-- A monotone unsigned key for IEEE 754 total order: negative encodings are
reversed and nonnegative encodings follow them, with the raw sign bit removed. -/
def floatTotalKey (bits : FloatBits) : Nat :=
  if floatSign ≤ bits.val then signed64Width - 1 - bits.val else bits.val + floatSign

def floatTotalLE (left right : FloatBits) : Prop := floatTotalKey left ≤ floatTotalKey right

theorem float_total_key_injective {left right : FloatBits}
    (sameKey : floatTotalKey left = floatTotalKey right) : left = right := by
  apply Fin.ext
  unfold floatTotalKey at sameKey
  simp only [floatSign, signed64Width] at sameKey
  have leftBound : left.val < 2 ^ 64 := left.isLt
  have rightBound : right.val < 2 ^ 64 := right.isLt
  by_cases leftNegative : 2 ^ 63 ≤ left.val <;>
    by_cases rightNegative : 2 ^ 63 ≤ right.val <;>
    simp [leftNegative, rightNegative] at sameKey <;> omega

theorem float_total_order_reflexive (bits : FloatBits) : floatTotalLE bits bits := by
  exact Nat.le_refl _

theorem float_total_order_transitive {first second third : FloatBits}
    (firstBeforeSecond : floatTotalLE first second)
    (secondBeforeThird : floatTotalLE second third) :
    floatTotalLE first third :=
  Nat.le_trans firstBeforeSecond secondBeforeThird

theorem float_total_order_total (left right : FloatBits) :
    floatTotalLE left right ∨ floatTotalLE right left :=
  Nat.le_total _ _

theorem float_total_order_negative_infinity_before_negative_zero :
    floatTotalKey floatNegativeInfinity < floatTotalKey floatNegativeZero := by
  decide

theorem float_total_order_negative_nan_before_negative_infinity :
    floatTotalKey floatNegativeQuietNaN < floatTotalKey floatNegativeInfinity := by
  decide

theorem float_total_order_negative_zero_before_positive_zero :
    floatTotalKey floatNegativeZero < floatTotalKey floatPositiveZero := by
  decide

theorem float_total_order_positive_zero_before_positive_infinity :
    floatTotalKey floatPositiveZero < floatTotalKey floatPositiveInfinity := by
  decide

theorem float_total_order_positive_infinity_before_nan :
    floatTotalKey floatPositiveInfinity < floatTotalKey floatQuietNaN := by
  decide

abbrev Bytes := List UInt8

/-- Native paths are byte sequences with no NUL byte; UTF-8 is not required. -/
structure NativePath where
  bytes : Bytes
  noNul : ∀ byte, byte ∈ bytes → byte ≠ 0
  deriving Repr

def NativePath.fromBytes? (bytes : Bytes) : Option NativePath :=
  if h : ∀ byte, byte ∈ bytes → byte ≠ 0 then some ⟨bytes, h⟩ else none

theorem native_path_bytes_round_trip (path : NativePath) :
    NativePath.fromBytes? path.bytes = some path := by
  unfold NativePath.fromBytes?
  rw [dite_eq_left path.noNul]

theorem native_path_rejects_nul : NativePath.fromBytes? [0] = none := by
  decide

theorem native_path_accepts_non_utf8_byte :
    ∃ path : NativePath, path.bytes = [255] := by
  exact ⟨⟨[255], by intro byte member; simp at member; subst byte; decide⟩, rfl⟩

end Xsh.Values
