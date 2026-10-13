import Xsh.Values.Scalars
import Xsh.Values.Composite

namespace Xsh.Values

/-- Executable classification of a raw IEEE-754 NaN payload. -/
def floatIsNaNB (bits : FloatBits) : Bool :=
  decide (floatExponent bits = floatExponentLimit - 1) &&
    decide (floatFraction bits ≠ 0)

theorem float_is_nanb_spec (bits : FloatBits) :
    floatIsNaNB bits = true ↔ floatIsNaN bits := by
  simp [floatIsNaNB, floatIsNaN, Bool.and_eq_true]

theorem float_is_nanb_false_spec (bits : FloatBits) :
    floatIsNaNB bits = false ↔ ¬ floatIsNaN bits := by
  constructor
  · intro isFalse isNaN
    have isTrue := (float_is_nanb_spec bits).2 isNaN
    rw [isTrue] at isFalse
    contradiction
  · intro notNaN
    cases isTrue : floatIsNaNB bits with
    | false => rfl
    | true => exact False.elim (notNaN ((float_is_nanb_spec bits).1 isTrue))

/-- Executable zero-magnitude classification, ignoring the sign bit. -/
def floatIsZeroB (bits : FloatBits) : Bool := decide (bits.val % floatSign = 0)

theorem float_is_zerob_spec (bits : FloatBits) :
    floatIsZeroB bits = true ↔ floatIsZero bits := by
  simp [floatIsZeroB, floatIsZero]

/-- Executable XSH Float equality over raw IEEE-754 bit patterns. -/
def floatValueEq (left right : FloatBits) : Bool :=
  !floatIsNaNB left && !floatIsNaNB right &&
    ((floatIsZeroB left && floatIsZeroB right) || decide (left.val = right.val))

/-- XSH `!=` is the Boolean complement of IEEE value equality. -/
def floatValueNeq (left right : FloatBits) : Bool := !(floatValueEq left right)

theorem float_value_eq_spec (left right : FloatBits) :
    floatValueEq left right = true ↔ ieeeFloatEq left right := by
  simp [floatValueEq, ieeeFloatEq, float_is_nanb_false_spec, float_is_zerob_spec,
    Bool.and_eq_true, Bool.or_eq_true, and_assoc]

theorem float_value_eq_false_iff (left right : FloatBits) :
    floatValueEq left right = false ↔ ¬ ieeeFloatEq left right := by
  constructor
  · intro isFalse equal
    have isTrue := (float_value_eq_spec left right).2 equal
    rw [isTrue] at isFalse
    contradiction
  · intro notEqual
    cases isTrue : floatValueEq left right with
    | false => rfl
    | true => exact False.elim (notEqual ((float_value_eq_spec left right).1 isTrue))

theorem float_value_neq_spec (left right : FloatBits) :
    floatValueNeq left right = true ↔ ¬ ieeeFloatEq left right := by
  simpa [floatValueNeq] using float_value_eq_false_iff left right

theorem ieee_float_eq_symmetric {left right : FloatBits}
    (equal : ieeeFloatEq left right) : ieeeFloatEq right left := by
  rcases equal with ⟨leftNotNaN, rightNotNaN, valueRelation⟩
  refine ⟨rightNotNaN, leftNotNaN, ?_⟩
  rcases valueRelation with ⟨leftZero, rightZero⟩ | sameBits
  · exact Or.inl ⟨rightZero, leftZero⟩
  · exact Or.inr sameBits.symm

theorem ieee_float_eq_transitive {first second third : FloatBits}
    (firstSecond : ieeeFloatEq first second)
    (secondThird : ieeeFloatEq second third) : ieeeFloatEq first third := by
  rcases firstSecond with ⟨firstNotNaN, secondNotNaN, firstSecond⟩
  rcases secondThird with ⟨_, thirdNotNaN, secondThird⟩
  refine ⟨firstNotNaN, thirdNotNaN, ?_⟩
  rcases firstSecond with ⟨firstZero, secondZero⟩ | firstSecondBits
  · rcases secondThird with ⟨_, thirdZero⟩ | secondThirdBits
    · exact Or.inl ⟨firstZero, thirdZero⟩
    · apply Or.inl
      refine ⟨firstZero, ?_⟩
      change third.val % floatSign = 0
      rw [← secondThirdBits]
      exact secondZero
  · rcases secondThird with ⟨secondZero, thirdZero⟩ | secondThirdBits
    · apply Or.inl
      refine ⟨?_, thirdZero⟩
      change first.val % floatSign = 0
      rw [firstSecondBits]
      exact secondZero
    · exact Or.inr (firstSecondBits.trans secondThirdBits)

theorem ieee_float_eq_reflexive_iff (bits : FloatBits) :
    ieeeFloatEq bits bits ↔ ¬ floatIsNaN bits := by
  constructor
  · intro equal
    exact equal.1
  · exact non_nan_equals_itself bits

theorem ieee_float_eq_preserves_nonzero_payload {left right : FloatBits}
    (equal : ieeeFloatEq left right) (leftNonzero : ¬ floatIsZero left) :
    left.val = right.val := by
  rcases equal with ⟨_, _, valueRelation⟩
  rcases valueRelation with ⟨leftZero, _⟩ | sameBits
  · exact False.elim (leftNonzero leftZero)
  · exact sameBits

theorem composite_float_equality_matches_boolean_decision (left right : FloatBits) :
    CompositeValue.semanticEq (.float left) (.float right) ↔
      floatValueEq left right = true := by
  simpa [CompositeValue.semanticEq] using (float_value_eq_spec left right).symm

theorem signed_zero_equality_preserves_raw_parts_and_total_order :
    ieeeFloatEq floatPositiveZero floatNegativeZero ∧
      encodeFloatParts floatPositiveZero ≠ encodeFloatParts floatNegativeZero ∧
      floatTotalKey floatNegativeZero < floatTotalKey floatPositiveZero := by
  refine ⟨ieee_signed_zeros_equal, ?_, float_total_order_negative_zero_before_positive_zero⟩
  intro sameParts
  have sameBits := congrArg decodeFloatParts sameParts
  rw [float_parts_round_trip, float_parts_round_trip] at sameBits
  exact signed_zero_raw_bits_differ (congrArg Fin.val sameBits)

theorem signed_zero_equality_preserves_hex_and_total_sort :
    CompositeValue.semanticEq positiveZeroValue negativeZeroValue ∧
      floatTransportHex floatPositiveZero ≠ floatTransportHex floatNegativeZero ∧
      floatTotalKey floatNegativeZero < floatTotalKey floatPositiveZero := by
  exact ⟨signed_zero_semantically_equal, signed_zero_transport_hex_remains_distinct,
    float_total_order_negative_zero_before_positive_zero⟩

theorem float_value_eq_signed_zeros :
    floatValueEq floatPositiveZero floatNegativeZero = true := by
  exact float_value_eq_spec _ _ |>.2 ieee_signed_zeros_equal

theorem float_value_neq_same_bit_nan :
    floatValueNeq floatQuietNaN floatQuietNaN = true := by
  exact float_value_neq_spec _ _ |>.2 same_bit_nan_is_unequal

theorem distinct_nan_payloads_are_unequal :
    ¬ ieeeFloatEq floatQuietNaN floatNegativeQuietNaN := by
  apply nan_never_equals
  simp [floatIsNaN, floatExponent, floatFraction, floatQuietNaN,
    floatExponentLimit, floatFractionWidth]

theorem float_value_neq_distinct_nan_payloads :
    floatValueNeq floatQuietNaN floatNegativeQuietNaN = true := by
  exact float_value_neq_spec _ _ |>.2 distinct_nan_payloads_are_unequal

end Xsh.Values
