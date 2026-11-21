import Xsh.Values.IntegerOps
import Xsh.Values.DurationOps
import Xsh.Values.FloatEquality
import Xsh.Values.FloatOps

namespace Xsh.Proofs.Base.Num

open Xsh.Values
open Xsh.Values.IntegerOps

/-- A result is the exact mathematical value when that value is an `Int`, and the overflow
fault precisely when it is not. -/
def IntegerExact (exact : Int) : IntegerResult → Prop
  | .value result => result.val = exact
  | .fault fault => fault = .overflow ∧ (exact < signed64Min ∨ signed64Limit ≤ exact)

/-- The specified result of each checked integer operation on exact integers. Division and
remainder truncate toward zero. A zero divisor has its own fault, and the minimum value divided by
negative one overflows for both operations. -/
def ArithmeticMeaning : IntegerBinaryOp → IntValue → IntValue → IntegerResult → Prop
  | .add, left, right, result => IntegerExact (left.val + right.val) result
  | .subtract, left, right, result => IntegerExact (left.val - right.val) result
  | .multiply, left, right, result => IntegerExact (left.val * right.val) result
  | .divide, left, right, result =>
    if right.val = 0 then result = .fault .divisionByZero
    else IntegerExact (left.val.tdiv right.val) result
  | .remainder, left, right, result =>
    if right.val = 0 then result = .fault .divisionByZero
    else if left.val = signed64Min ∧ right.val = -1 then result = .fault .overflow
    else IntegerExact (left.val.tmod right.val) result

instance (exact : Int) (result : IntegerResult) : Decidable (IntegerExact exact result) := by
  cases result <;> unfold IntegerExact <;> infer_instance

instance (operator : IntegerBinaryOp) (left right : IntValue) (result : IntegerResult) :
    Decidable (ArithmeticMeaning operator left right result) := by
  cases operator <;> unfold ArithmeticMeaning <;> infer_instance

theorem integer_exact_determines_the_result {exact : Int} {first second : IntegerResult}
    (firstExact : IntegerExact exact first) (secondExact : IntegerExact exact second) :
    first = second := by
  cases first with
  | value firstValue =>
    cases second with
    | value secondValue =>
      have sameValue : firstValue = secondValue := Subtype.ext (firstExact.trans secondExact.symm)
      rw [sameValue]
    | fault secondFault =>
      have inRange := firstValue.property
      have outOfRange := secondExact.2
      simp only [IntegerExact] at firstExact
      rw [← firstExact] at outOfRange
      unfold signed64Min signed64Limit at inRange outOfRange
      omega
  | fault firstFault =>
    cases second with
    | value secondValue =>
      have inRange := secondValue.property
      have outOfRange := firstExact.2
      simp only [IntegerExact] at secondExact
      rw [← secondExact] at outOfRange
      unfold signed64Min signed64Limit at inRange outOfRange
      omega
    | fault secondFault =>
      rw [firstExact.1, secondExact.1]

theorem checked_conversion_is_exact (exact : Int) : IntegerExact exact (checkedInt exact) := by
  rcases checkedInt_total exact with ⟨bounded, converted, same⟩ | ⟨faulted, outside⟩
  · rw [converted]
    exact same
  · rw [faulted]
    exact ⟨rfl, outside⟩

theorem applied_operation_has_its_meaning (operator : IntegerBinaryOp) (left right : IntValue) :
    ArithmeticMeaning operator left right (applyBinary operator left right) := by
  cases operator with
  | add => exact checked_conversion_is_exact _
  | subtract => exact checked_conversion_is_exact _
  | multiply => exact checked_conversion_is_exact _
  | divide =>
    by_cases zero : right.val = 0
    · simp [ArithmeticMeaning, applyBinary, zero]
    · simp only [ArithmeticMeaning, applyBinary, zero, ite_false]
      exact checked_conversion_is_exact _
  | remainder =>
    by_cases zero : right.val = 0
    · simp [ArithmeticMeaning, applyBinary, zero]
    · by_cases overflowing : left.val = signed64Min ∧ right.val = -1
      · simp [ArithmeticMeaning, applyBinary, zero, overflowing]
      · simp only [ArithmeticMeaning, applyBinary, zero, overflowing, ite_false]
        exact checked_conversion_is_exact _

theorem the_meaning_determines_the_operation (operator : IntegerBinaryOp) (left right : IntValue)
    (result : IntegerResult) (meaning : ArithmeticMeaning operator left right result) :
    result = applyBinary operator left right := by
  have applied := applied_operation_has_its_meaning operator left right
  cases operator with
  | add => exact integer_exact_determines_the_result meaning applied
  | subtract => exact integer_exact_determines_the_result meaning applied
  | multiply => exact integer_exact_determines_the_result meaning applied
  | divide =>
    by_cases zero : right.val = 0
    · simp only [ArithmeticMeaning, zero, if_true] at meaning applied
      rw [meaning, applied]
    · simp only [ArithmeticMeaning, zero, ite_false] at meaning applied
      exact integer_exact_determines_the_result meaning applied
  | remainder =>
    by_cases zero : right.val = 0
    · simp only [ArithmeticMeaning, zero, if_true] at meaning applied
      rw [meaning, applied]
    · by_cases overflowing : left.val = signed64Min ∧ right.val = -1
      · simp only [ArithmeticMeaning, zero, overflowing, if_true, ite_false] at meaning applied
        rw [meaning, applied]
      · simp only [ArithmeticMeaning, zero, overflowing, ite_false] at meaning applied
        exact integer_exact_determines_the_result meaning applied

theorem negation_is_exact (value : IntValue) : IntegerExact (-value.val) (negate value) :=
  checked_conversion_is_exact _

private theorem bmod_two_pow_64_is_identity_exactly_on_int (exact : Int) :
    exact.bmod (2 ^ 64) = exact ↔ signed64Min ≤ exact ∧ exact < signed64Limit := by
  unfold signed64Min signed64Limit Int.bmod
  have width : ((2 ^ 64 : Nat) : Int) = 18446744073709551616 := by decide
  simp only [width]
  omega

private theorem encoded_value_reads_back (value : IntValue) : (encodeInt value).toInt = value.val :=
  congrArg Subtype.val (decode_encode_int value)

private theorem exact_value_in_range_of_value_result {exact : Int} {result : IntValue}
    (exactResult : IntegerExact exact (.value result)) :
    signed64Min ≤ exact ∧ exact < signed64Limit := by
  have inRange := result.property
  simp only [IntegerExact] at exactResult
  rw [← exactResult]
  exact inRange

private theorem exact_fault_is_out_of_range {exact : Int} {fault : IntegerFault}
    (exactResult : IntegerExact exact (.fault fault)) :
    ¬ (signed64Min ≤ exact ∧ exact < signed64Limit) := by
  have outside := exactResult.2
  omega

private theorem exact_result_matches_wrapped (exact : Int) (result : IntegerResult)
    (meaning : IntegerExact exact result) :
    (∀ value, result = .value value → encodeInt value = BitVec.ofInt 64 exact) ∧
    (result = .fault .overflow ↔ (BitVec.ofInt 64 exact).toInt ≠ exact) := by
  have wrapped : (BitVec.ofInt 64 exact).toInt = exact.bmod (2 ^ 64) := BitVec.toInt_ofInt exact
  have equivalence : (BitVec.ofInt 64 exact).toInt = exact ↔
      signed64Min ≤ exact ∧ exact < signed64Limit := by
    rw [wrapped]
    exact bmod_two_pow_64_is_identity_exactly_on_int exact
  cases result with
  | value value =>
    refine ⟨?_, ?_⟩
    · intro found same
      cases same
      simp only [IntegerExact] at meaning
      unfold encodeInt
      rw [meaning]
    · have inRange := exact_value_in_range_of_value_result meaning
      constructor
      · intro impossible
        cases impossible
      · intro different
        exact absurd (equivalence.2 inRange) different
  | fault fault =>
    have outside := exact_fault_is_out_of_range meaning
    have faultIsOverflow := meaning.1
    subst faultIsOverflow
    refine ⟨?_, ?_⟩
    · intro value impossible
      cases impossible
    · constructor
      · intro _ same
        exact outside (equivalence.1 same)
      · intro _
        rfl

theorem exact_addition_matches_hardware (left right : IntValue) :
    (∀ result, applyBinary .add left right = .value result →
      encodeInt result = encodeInt left + encodeInt right) ∧
    (applyBinary .add left right = .fault .overflow ↔
      (encodeInt left + encodeInt right).toInt ≠ left.val + right.val) := by
  have agreement := exact_result_matches_wrapped _ _ (applied_operation_has_its_meaning .add left right)
  have wrapped : BitVec.ofInt 64 (left.val + right.val) = encodeInt left + encodeInt right :=
    BitVec.ofInt_add _ _
  rw [wrapped] at agreement
  exact agreement

theorem exact_subtraction_matches_hardware (left right : IntValue) :
    (∀ result, applyBinary .subtract left right = .value result →
      encodeInt result = encodeInt left - encodeInt right) ∧
    (applyBinary .subtract left right = .fault .overflow ↔
      (encodeInt left - encodeInt right).toInt ≠ left.val - right.val) := by
  have agreement := exact_result_matches_wrapped _ _
    (applied_operation_has_its_meaning .subtract left right)
  have wrapped : BitVec.ofInt 64 (left.val - right.val) = encodeInt left - encodeInt right := by
    rw [Int.sub_eq_add_neg, BitVec.ofInt_add, BitVec.ofInt_neg, BitVec.sub_eq_add_neg]
    rfl
  rw [wrapped] at agreement
  exact agreement

theorem exact_multiplication_matches_hardware (left right : IntValue) :
    (∀ result, applyBinary .multiply left right = .value result →
      encodeInt result = encodeInt left * encodeInt right) ∧
    (applyBinary .multiply left right = .fault .overflow ↔
      (encodeInt left * encodeInt right).toInt ≠ left.val * right.val) := by
  have agreement := exact_result_matches_wrapped _ _
    (applied_operation_has_its_meaning .multiply left right)
  have wrapped : BitVec.ofInt 64 (left.val * right.val) = encodeInt left * encodeInt right :=
    BitVec.ofInt_mul _ _
  rw [wrapped] at agreement
  exact agreement

theorem exact_negation_matches_hardware (value : IntValue) :
    (∀ result, negate value = .value result → encodeInt result = -encodeInt value) ∧
    (negate value = .fault .overflow ↔ (-encodeInt value).toInt ≠ -value.val) := by
  have agreement := exact_result_matches_wrapped _ _ (negation_is_exact value)
  have wrapped : BitVec.ofInt 64 (-value.val) = -encodeInt value := BitVec.ofInt_neg
  rw [wrapped] at agreement
  exact agreement

theorem exact_division_matches_hardware (left right : IntValue)
    (result : IntValue) (divided : applyBinary .divide left right = .value result) :
    encodeInt result = (encodeInt left).sdiv (encodeInt right) := by
  have meaning := applied_operation_has_its_meaning .divide left right
  rw [divided] at meaning
  have nonzero : ¬ right.val = 0 := by
    intro zero
    simp [ArithmeticMeaning, zero] at meaning
  simp only [ArithmeticMeaning, nonzero, ite_false, IntegerExact] at meaning
  apply BitVec.eq_of_toInt_eq
  rw [BitVec.toInt_sdiv]
  simp only [encoded_value_reads_back]
  rw [← meaning]
  exact ((bmod_two_pow_64_is_identity_exactly_on_int result.val).2 result.property).symm

theorem exact_remainder_matches_hardware (left right : IntValue)
    (result : IntValue) (remainder : applyBinary .remainder left right = .value result) :
    encodeInt result = (encodeInt left).srem (encodeInt right) := by
  have meaning := applied_operation_has_its_meaning .remainder left right
  rw [remainder] at meaning
  have nonzero : ¬ right.val = 0 := by
    intro zero
    simp [ArithmeticMeaning, zero] at meaning
  have notOverflowing : ¬ (left.val = signed64Min ∧ right.val = -1) := by
    intro overflowing
    simp [ArithmeticMeaning, nonzero, overflowing] at meaning
  simp only [ArithmeticMeaning, nonzero, notOverflowing, ite_false, IntegerExact] at meaning
  apply BitVec.eq_of_toInt_eq
  rw [BitVec.toInt_srem]
  simp only [encoded_value_reads_back]
  rw [← meaning]

theorem unsigned_boundary_accepts_exactly_the_nonnegative_half (value : Int) :
    (validateUInt? value).isSome = true ↔ 0 ≤ value ∧ value < signed64Limit := by
  by_cases inRange : 0 ≤ value ∧ value < signed64Limit
  · simp [validateUInt?, inRange]
  · simp [validateUInt?, inRange]

theorem signed_value_is_unsigned_exactly_when_nonnegative (value : IntValue) :
    value.toUInt?.isSome = true ↔ 0 ≤ value.val := by
  by_cases nonnegative : 0 ≤ value.val
  · simp [IntValue.toUInt?, nonnegative]
  · simp [IntValue.toUInt?, nonnegative]

theorem duration_addition_is_exact_or_overflow (left right : DurationValue) :
    (∃ sum, DurationOps.add left right = .millis sum ∧ sum.val = left.val + right.val) ∨
      (DurationOps.add left right = .fault .overflow ∧
        signed64Width ≤ left.val + right.val) := by
  by_cases inRange : left.val + right.val < signed64Width
  · exact Or.inl ⟨⟨_, inRange⟩, DurationOps.addition_returns_in_range_sum left right inRange, rfl⟩
  · have tooLarge : signed64Width ≤ left.val + right.val := Nat.le_of_not_lt inRange
    exact Or.inr ⟨DurationOps.addition_rejects_out_of_range_sum left right tooLarge, tooLarge⟩

theorem duration_subtraction_is_exact_or_underflow (left right : DurationValue) :
    (right.val ≤ left.val ∧
      ∃ difference, DurationOps.subtract left right = .millis difference ∧
        difference.val = left.val - right.val) ∨
      (left.val < right.val ∧ DurationOps.subtract left right = .fault .underflow) := by
  by_cases ordered : right.val ≤ left.val
  · exact Or.inl ⟨ordered, _, DurationOps.subtraction_returns_ordered_difference left right ordered,
      rfl⟩
  · have smaller : left.val < right.val := Nat.lt_of_not_le ordered
    exact Or.inr ⟨smaller, DurationOps.subtraction_rejects_larger_right_operand left right smaller⟩

theorem duration_scaling_is_exact_or_faults (duration : DurationValue) (factor : IntValue) :
    (factor.val < 0 ∧ DurationOps.multiply duration factor = .fault .negativeFactor) ∨
      (0 ≤ factor.val ∧
        ((∃ product, DurationOps.multiply duration factor = .millis product ∧
            product.val = duration.val * factor.val.toNat) ∨
          (DurationOps.multiply duration factor = .fault .overflow ∧
            signed64Width ≤ duration.val * factor.val.toNat))) := by
  by_cases negative : factor.val < 0
  · exact Or.inl ⟨negative, DurationOps.multiplication_rejects_negative_factor duration factor
      negative⟩
  · have nonnegative : 0 ≤ factor.val := Int.not_lt.1 negative
    refine Or.inr ⟨nonnegative, ?_⟩
    by_cases inRange : duration.val * factor.val.toNat < signed64Width
    · exact Or.inl ⟨⟨_, inRange⟩, DurationOps.multiplication_returns_in_range_product duration
        factor nonnegative inRange, rfl⟩
    · have tooLarge : signed64Width ≤ duration.val * factor.val.toNat := Nat.le_of_not_lt inRange
      exact Or.inr ⟨DurationOps.multiplication_rejects_out_of_range_product duration factor
        nonnegative tooLarge, tooLarge⟩

theorem duration_division_is_exact_or_nonpositive (duration : DurationValue) (divisor : IntValue) :
    (divisor.val ≤ 0 ∧ DurationOps.divideByInt duration divisor = .fault .nonpositiveDivisor) ∨
      (0 < divisor.val ∧ ∃ quotient, DurationOps.divideByInt duration divisor = .millis quotient ∧
        quotient.val = duration.val / divisor.val.toNat) := by
  by_cases nonpositive : divisor.val ≤ 0
  · exact Or.inl ⟨nonpositive, DurationOps.division_rejects_nonpositive_divisor duration divisor
      nonpositive⟩
  · have positive : 0 < divisor.val := Int.not_le.1 nonpositive
    exact Or.inr ⟨positive, _, DurationOps.division_returns_quotient_for_positive_divisor duration
      divisor positive, rfl⟩

theorem float_total_order_is_antisymmetric {left right : FloatBits}
    (leftBefore : floatTotalLE left right) (rightBefore : floatTotalLE right left) :
    left = right :=
  float_total_key_injective (Nat.le_antisymm leftBefore rightBefore)

theorem float_bits_outside_the_word_are_rejected (raw : Nat) (wide : signed64Width ≤ raw) :
    decodeFloatBits? raw = none := by
  simp [decodeFloatBits?, Nat.not_lt.2 wide]

theorem nan_is_unequal_to_every_float (nan : FloatBits) (isNaN : floatIsNaN nan)
    (other : FloatBits) : ¬ ieeeFloatEq nan other ∧ ¬ ieeeFloatEq other nan :=
  ⟨nan_never_equals nan other isNaN, nan_never_equals_on_right other nan isNaN⟩

/-- The integer, Duration and Float equality facts of the base machine. Checked integer arithmetic returns the exact
mathematical result when it is an `Int` and otherwise the specified fault, and the specification
determines the result. Whenever a result is a value it equals the two's-complement hardware
result, and the overflow fault is reported exactly when the wrapped hardware result differs from
the exact one. The signed 64-bit encoding is a bijection. `UInt` is exactly the nonnegative half
of `Int`. Duration arithmetic is exact or reports its specified fault. Float bit patterns
round-trip, value equality is IEEE equality with signed zeros equal and every NaN unequal, and
the total order is total and antisymmetric on the raw bit patterns. Float arithmetic is outside
this model. -/
def IntegerDurationAndEqualityFacts : Prop :=
    (∀ operator left right,
      ArithmeticMeaning operator left right (applyBinary operator left right)) ∧
    (∀ operator left right result, ArithmeticMeaning operator left right result →
      result = applyBinary operator left right) ∧
    (∀ value : IntValue, IntegerExact (-value.val) (negate value)) ∧
    (∀ left right : IntValue,
      (∀ result, applyBinary .add left right = .value result →
        encodeInt result = encodeInt left + encodeInt right) ∧
      (applyBinary .add left right = .fault .overflow ↔
        (encodeInt left + encodeInt right).toInt ≠ left.val + right.val)) ∧
    (∀ left right : IntValue,
      (∀ result, applyBinary .subtract left right = .value result →
        encodeInt result = encodeInt left - encodeInt right) ∧
      (applyBinary .subtract left right = .fault .overflow ↔
        (encodeInt left - encodeInt right).toInt ≠ left.val - right.val)) ∧
    (∀ left right : IntValue,
      (∀ result, applyBinary .multiply left right = .value result →
        encodeInt result = encodeInt left * encodeInt right) ∧
      (applyBinary .multiply left right = .fault .overflow ↔
        (encodeInt left * encodeInt right).toInt ≠ left.val * right.val)) ∧
    (∀ value : IntValue,
      (∀ result, negate value = .value result → encodeInt result = -encodeInt value) ∧
      (negate value = .fault .overflow ↔ (-encodeInt value).toInt ≠ -value.val)) ∧
    (∀ left right result : IntValue, applyBinary .divide left right = .value result →
      encodeInt result = (encodeInt left).sdiv (encodeInt right)) ∧
    (∀ left right result : IntValue, applyBinary .remainder left right = .value result →
      encodeInt result = (encodeInt left).srem (encodeInt right)) ∧
    (∀ value : IntValue, decodeInt (encodeInt value) = value) ∧
    (∀ word : IntWord, encodeInt (decodeInt word) = word) ∧
    (∀ value : Int, (validateUInt? value).isSome = true ↔ 0 ≤ value ∧ value < signed64Limit) ∧
    (∀ value : UIntValue, value.toInt.toUInt? = some value) ∧
    (∀ value : IntValue, value.toUInt?.isSome = true ↔ 0 ≤ value.val) ∧
    (∀ left right : DurationValue,
      (∃ sum, DurationOps.add left right = .millis sum ∧ sum.val = left.val + right.val) ∨
        (DurationOps.add left right = .fault .overflow ∧
          signed64Width ≤ left.val + right.val)) ∧
    (∀ left right : DurationValue,
      (right.val ≤ left.val ∧
        ∃ difference, DurationOps.subtract left right = .millis difference ∧
          difference.val = left.val - right.val) ∨
        (left.val < right.val ∧ DurationOps.subtract left right = .fault .underflow)) ∧
    (∀ (duration : DurationValue) (factor : IntValue),
      (factor.val < 0 ∧ DurationOps.multiply duration factor = .fault .negativeFactor) ∨
        (0 ≤ factor.val ∧
          ((∃ product, DurationOps.multiply duration factor = .millis product ∧
              product.val = duration.val * factor.val.toNat) ∨
            (DurationOps.multiply duration factor = .fault .overflow ∧
              signed64Width ≤ duration.val * factor.val.toNat)))) ∧
    (∀ (duration : DurationValue) (divisor : IntValue),
      (divisor.val ≤ 0 ∧ DurationOps.divideByInt duration divisor = .fault .nonpositiveDivisor) ∨
        (0 < divisor.val ∧ ∃ quotient, DurationOps.divideByInt duration divisor = .millis quotient ∧
          quotient.val = duration.val / divisor.val.toNat)) ∧
    (∀ bits : FloatBits, decodeFloatBits? bits.val = some bits) ∧
    (∀ raw, signed64Width ≤ raw → decodeFloatBits? raw = none) ∧
    (∀ left right, floatValueEq left right = true ↔ ieeeFloatEq left right) ∧
    (∀ left right, floatValueNeq left right = true ↔ ¬ ieeeFloatEq left right) ∧
    (∀ nan, floatIsNaN nan → ∀ other, ¬ ieeeFloatEq nan other ∧ ¬ ieeeFloatEq other nan) ∧
    (ieeeFloatEq floatPositiveZero floatNegativeZero ∧
      floatPositiveZero.val ≠ floatNegativeZero.val) ∧
    (∀ bits, floatTotalLE bits bits) ∧
    (∀ first second third, floatTotalLE first second → floatTotalLE second third →
      floatTotalLE first third) ∧
    (∀ left right, floatTotalLE left right ∨ floatTotalLE right left) ∧
    (∀ left right, floatTotalLE left right → floatTotalLE right left → left = right)

theorem base_num : IntegerDurationAndEqualityFacts :=
  ⟨applied_operation_has_its_meaning, the_meaning_determines_the_operation, negation_is_exact,
    exact_addition_matches_hardware, exact_subtraction_matches_hardware,
    exact_multiplication_matches_hardware, exact_negation_matches_hardware,
    exact_division_matches_hardware, exact_remainder_matches_hardware,
    decode_encode_int, encode_decode_int,
    unsigned_boundary_accepts_exactly_the_nonnegative_half, uint_to_int_round_trip,
    signed_value_is_unsigned_exactly_when_nonnegative,
    duration_addition_is_exact_or_overflow, duration_subtraction_is_exact_or_underflow,
    duration_scaling_is_exact_or_faults, duration_division_is_exact_or_nonpositive,
    decode_float_bits_round_trip, float_bits_outside_the_word_are_rejected,
    float_value_eq_spec, float_value_neq_spec, nan_is_unequal_to_every_float,
    ⟨ieee_signed_zeros_equal, by decide⟩, float_total_order_reflexive,
    fun _ _ _ => float_total_order_transitive, float_total_order_total,
    fun _ _ => float_total_order_is_antisymmetric⟩

private def intNegativeSeven : IntValue := ⟨-7, by unfold signed64Min signed64Limit; omega⟩
private def intThree : IntValue := ⟨3, by unfold signed64Min signed64Limit; omega⟩
private def intTwo : IntValue := ⟨2, by unfold signed64Min signed64Limit; omega⟩

theorem base_num_witness :
    ArithmeticMeaning .divide intMin intNegativeOne (.fault .overflow) ∧
    ArithmeticMeaning .remainder intMin intNegativeOne (.fault .overflow) ∧
    ArithmeticMeaning .divide intOne intZero (.fault .divisionByZero) ∧
    ArithmeticMeaning .divide intNegativeSeven intThree
      (.value ⟨-2, by unfold signed64Min signed64Limit; omega⟩) ∧
    ArithmeticMeaning .add intMax intOne (.fault .overflow) ∧
    (encodeInt intMax + encodeInt intOne).toInt = signed64Min ∧
    applyBinary .add intTwo intThree = .value ⟨5, by unfold signed64Min signed64Limit; omega⟩ ∧
    validateUInt? (-1) = none ∧ validateUInt? 0 ≠ none ∧
    DurationOps.add durationMax durationOne = .fault .overflow ∧
    ieeeFloatEq floatPositiveZero floatNegativeZero ∧ ¬ ieeeFloatEq floatQuietNaN floatQuietNaN := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ieee_signed_zeros_equal, same_bit_nan_is_unequal⟩
  all_goals decide

def wrappingAddition (left right : IntValue) : IntegerResult :=
  .value (decodeInt (encodeInt left + encodeInt right))

theorem wrapping_addition_is_rejected :
    ¬ ∀ left right, ArithmeticMeaning .add left right (wrappingAddition left right) := by
  intro meaning
  have atTheBoundary := meaning intMax intOne
  have wrapped : ¬ ((decodeInt (encodeInt intMax + encodeInt intOne)).val =
      intMax.val + intOne.val) := by decide
  exact wrapped atTheBoundary

def flooredDivision (left right : IntValue) : IntegerResult :=
  if right.val = 0 then .fault .divisionByZero else checkedInt (left.val / right.val)

theorem floored_division_is_rejected :
    ¬ ∀ left right, ArithmeticMeaning .divide left right (flooredDivision left right) := by
  intro meaning
  have atNegativeNumerator := meaning intNegativeSeven intThree
  revert atNegativeNumerator
  decide

def remainderWithoutDivisionOverflow (left right : IntValue) : IntegerResult :=
  if right.val = 0 then .fault .divisionByZero else checkedInt (left.val.tmod right.val)

theorem remainder_without_division_overflow_is_rejected :
    ¬ ∀ left right,
      ArithmeticMeaning .remainder left right (remainderWithoutDivisionOverflow left right) := by
  intro meaning
  have atTheMinimum := meaning intMin intNegativeOne
  revert atTheMinimum
  decide

def bitwiseFloatEquality (left right : FloatBits) : Bool := decide (left.val = right.val)

theorem bitwise_float_equality_is_rejected :
    ¬ ∀ left right, bitwiseFloatEquality left right = true ↔ ieeeFloatEq left right := by
  intro agrees
  exact same_bit_nan_is_unequal ((agrees floatQuietNaN floatQuietNaN).1 (by decide))

def durationAdditionThatWraps (left right : DurationValue) : DurationOps.DurationResult :=
  .millis (DurationOps.wrappingAdd left right)

theorem wrapping_duration_addition_is_rejected :
    ¬ ∀ left right : DurationValue,
      (∃ sum, durationAdditionThatWraps left right = .millis sum ∧
        sum.val = left.val + right.val) ∨
        (durationAdditionThatWraps left right = .fault .overflow ∧
          signed64Width ≤ left.val + right.val) := by
  intro meaning
  rcases meaning durationMax durationOne with ⟨sum, isSum, exact⟩ | ⟨impossible, _⟩
  · have wrapped : DurationOps.wrappingAdd durationMax durationOne = sum := by
      simpa [durationAdditionThatWraps] using isSum
    rw [← wrapped] at exact
    revert exact
    decide
  · cases impossible

open Xsh.Values.FloatArith
open Xsh.Values.FloatOps

/-- The exact numeric values of the core. The integer, Duration and Float equality facts hold.
Float `+`, `-`, `*` and `/` round the exact rational result of their finite operands to the
nearest finite binary64 value with ties to even, overflow to a signed infinity at or above the
overflow midpoint, keep the IEEE signed-zero and infinity rules, and return a NaN under every
NaN-payload policy; the meaning determines every non-NaN result. `.float()` rounds an `Int` the
same way. `.floor()`, `.ceil()` and `.round()` return the exact floor, ceiling and nearest integer
with ties away from zero, and reject NaN, infinities and results outside `Int`. -/
theorem numeric_semantics :
    IntegerDurationAndEqualityFacts ∧
    (∀ (policy : NaNPolicy) operator left right,
      FloatMeaning operator left right (floatApply policy operator left right)) ∧
    (∀ operator left right first second,
      FloatMeaning operator left right first → FloatMeaning operator left right second →
        (floatIsNaN first ∧ floatIsNaN second) ∨ first = second) ∧
    (∀ numerator denominator, 0 < denominator →
      numerator * 2 ^ 1074 < denominator * overflowMidpointUnits →
        BinaryNearest numerator denominator (roundMagnitudeBits numerator denominator)) ∧
    (∀ numerator denominator, 0 < denominator →
      denominator * overflowMidpointUnits ≤ numerator * 2 ^ 1074 →
        roundMagnitudeBits numerator denominator = infinityRaw) ∧
    (∀ numerator denominator, 0 < denominator → ∀ first second,
      BinaryNearest numerator denominator first → BinaryNearest numerator denominator second →
        first = second) ∧
    (∀ bits, floatClass bits = .nan ↔ floatIsNaN bits) ∧
    (∀ value, IntToFloatMeaning value (intToFloat value)) ∧
    (∀ value first second,
      IntToFloatMeaning value first → IntToFloatMeaning value second → first = second) ∧
    (∀ bits, ToIntMeaning FloorBound bits (floatFloor bits)) ∧
    (∀ bits, ToIntMeaning CeilBound bits (floatCeil bits)) ∧
    (∀ bits, ToIntMeaning RoundAwayBound bits (floatRound bits)) ∧
    (∀ bits first second, ToIntMeaning FloorBound bits first →
      ToIntMeaning FloorBound bits second → first = second) ∧
    (∀ bits first second, ToIntMeaning CeilBound bits first →
      ToIntMeaning CeilBound bits second → first = second) ∧
    (∀ bits first second, ToIntMeaning RoundAwayBound bits first →
      ToIntMeaning RoundAwayBound bits second → first = second) ∧
    (∀ bits, floatIsNaN bits ∨ floatMagnitudeRaw bits = infinityRaw →
      floatFloor bits = none ∧ floatCeil bits = none ∧ floatRound bits = none) ∧
    (floatApply quietNaNPolicy .add floatTenth floatFifth = floatTenthPlusFifth ∧
      floatApply quietNaNPolicy .add floatOne floatTwo = floatThree ∧
      floatApply quietNaNPolicy .add floatOne floatHalfUlpOfOne = floatOne ∧
      floatApply quietNaNPolicy .add floatOneAndUlp floatHalfUlpOfOne = floatOneAndTwoUlps ∧
      floatApply quietNaNPolicy .multiply floatLargest floatTwo = floatPositiveInfinity ∧
      floatApply quietNaNPolicy .divide floatLeastSubnormal floatTwo = floatPositiveZero ∧
      floatApply quietNaNPolicy .divide floatThreeLeastSubnormals floatTwo =
        floatTwoLeastSubnormals ∧
      floatApply quietNaNPolicy .add floatNegativeZero floatNegativeZero = floatNegativeZero ∧
      floatApply quietNaNPolicy .add floatPositiveZero floatNegativeZero = floatPositiveZero ∧
      floatApply quietNaNPolicy .subtract floatOne floatOne = floatPositiveZero ∧
      floatApply quietNaNPolicy .subtract floatNegativeZero floatPositiveZero =
        floatNegativeZero ∧
      floatApply quietNaNPolicy .multiply floatNegativeOne floatPositiveZero = floatNegativeZero ∧
      floatApply quietNaNPolicy .divide floatOne floatPositiveZero = floatPositiveInfinity ∧
      floatApply quietNaNPolicy .divide floatNegativeOne floatPositiveZero =
        floatNegativeInfinity ∧
      floatApply quietNaNPolicy .divide floatOne floatNegativeZero = floatNegativeInfinity ∧
      floatApply quietNaNPolicy .divide floatOne floatPositiveInfinity = floatPositiveZero ∧
      floatApply quietNaNPolicy .add floatPositiveInfinity floatOne = floatPositiveInfinity ∧
      floatApply quietNaNPolicy .subtract floatOne floatPositiveInfinity =
        floatNegativeInfinity) ∧
    (∀ policy : NaNPolicy,
      floatIsNaN (floatApply policy .divide floatPositiveZero floatPositiveZero) ∧
      floatIsNaN (floatApply policy .subtract floatPositiveInfinity floatPositiveInfinity) ∧
      floatIsNaN (floatApply policy .multiply floatPositiveInfinity floatPositiveZero) ∧
      floatIsNaN (floatApply policy .divide floatPositiveInfinity floatNegativeInfinity) ∧
      floatIsNaN (floatApply policy .add floatQuietNaN floatOne)) ∧
    (floatApply quietNaNPolicy .divide floatPositiveZero floatPositiveZero ≠
        floatApply negativeNaNPolicy .divide floatPositiveZero floatPositiveZero ∧
      FloatMeaning .divide floatPositiveZero floatPositiveZero
        (floatApply quietNaNPolicy .divide floatPositiveZero floatPositiveZero) ∧
      FloatMeaning .divide floatPositiveZero floatPositiveZero
        (floatApply negativeNaNPolicy .divide floatPositiveZero floatPositiveZero)) ∧
    ieeeFloatEq (floatApply quietNaNPolicy .subtract floatOne floatOne) floatNegativeZero ∧
    (floatRound floatTwoAndHalf = some (intValue 3) ∧
      floatRound floatNegativeTwoAndHalf = some (intValue (-3)) ∧
      floatRound floatHalf = some (intValue 1) ∧
      floatFloor floatNegativeTwoAndHalf = some (intValue (-3)) ∧
      floatCeil floatNegativeTwoAndHalf = some (intValue (-2)) ∧
      floatFloor floatTwoAndHalf = some (intValue 2) ∧
      floatCeil floatTwoAndHalf = some (intValue 3) ∧
      floatRound floatNegativeZero = some (intValue 0) ∧
      floatFloor floatNegativeTwoPow63 = some intMin ∧
      floatFloor floatTwoPow63 = none ∧
      floatCeil floatTwoPow63 = none ∧
      floatRound floatQuietNaN = none ∧
      floatFloor floatPositiveInfinity = none ∧
      floatCeil floatNegativeInfinity = none ∧
      intToFloat (intValue (2 ^ 53 + 1)) = floatTwoPow53 ∧
      intToFloat intMax = floatTwoPow63 ∧
      intToFloat intMin = floatNegativeTwoPow63 ∧
      intToFloat intNegativeOne = floatNegativeOne ∧
      intToFloat intZero = floatPositiveZero) ∧
    (¬ ∀ operator left right, FloatMeaning operator left right (tiesAwayApply operator left right)) ∧
    (¬ ∀ operator left right,
      FloatMeaning operator left right (offByOneExponentApply operator left right)) ∧
    (¬ ∀ operator left right,
      FloatMeaning operator left right (unsignedZeroApply operator left right)) ∧
    (¬ ∀ operator left right,
      FloatMeaning operator left right (floatApplyWith floatPositiveZero operator left right)) ∧
    (¬ ∀ bits, ToIntMeaning RoundAwayBound bits (floatRoundTiesEven bits)) :=
  ⟨base_num, float_apply_meaning,
    fun operator left right _ _ => float_meaning_determines_result operator left right,
    fun numerator denominator positive =>
      round_magnitude_bits_nearest numerator denominator positive,
    fun numerator denominator positive =>
      round_magnitude_bits_overflow numerator denominator positive,
    fun numerator denominator positive _ _ =>
      binary_nearest_unique numerator denominator positive,
    float_class_nan_iff, int_to_float_meaning,
    fun value _ _ => int_to_float_unique value,
    float_to_int_meaning _ _ floor_is_correct,
    float_to_int_meaning _ _ ceil_is_correct,
    float_to_int_meaning _ _ round_is_correct,
    fun bits _ _ => to_int_meaning_unique _ floor_bound_functional bits,
    fun bits _ _ => to_int_meaning_unique _ ceil_bound_functional bits,
    fun bits _ _ => to_int_meaning_unique _ round_away_bound_functional bits,
    nonfinite_conversions_rejected, float_arithmetic_witnesses, float_nan_witnesses,
    nan_payload_is_unconstrained, cancellation_is_ieee_equal_to_negative_zero,
    float_conversion_witnesses, ties_away_arithmetic_mutant_rejected,
    off_by_one_exponent_mutant_rejected, unsigned_zero_mutant_rejected,
    non_nan_policy_mutant_rejected, ties_even_round_mutant_rejected⟩

end Xsh.Proofs.Base.Num
