import Xsh.Values.Scalars

/-!
Checked arithmetic for unsigned millisecond durations, plus the saturating
integer-to-duration conversions used by the time module. This is an
independent value-operation fragment; it does not model parsing, evaluation,
source spans, compound assignment, or Rust conformance.
-/

namespace Xsh.Values.DurationOps

open Xsh.Values

inductive DurationFault where
  | overflow
  | underflow
  | negativeFactor
  | nonpositiveDivisor
  | intervalOverflow
  deriving DecidableEq, Repr

inductive DurationResult where
  | millis (value : DurationValue)
  | count (value : IntValue)
  | fault (fault : DurationFault)
  deriving DecidableEq, Repr

/-- Preserve a millisecond count exactly when it fits the unsigned duration
domain; addition and multiplication use this single checked boundary. -/
def checkedMillis (millis : Nat) : DurationResult :=
  match validateDuration? millis with
  | some value => .millis value
  | none => .fault .overflow

/-- Addition is checked against the full unsigned 64-bit duration range. -/
def add (left right : DurationValue) : DurationResult :=
  checkedMillis (left.val + right.val)

/-- Subtraction rejects a negative mathematical result before constructing a
duration. -/
def subtract (left right : DurationValue) : DurationResult :=
  if left.val < right.val then
    .fault .underflow
  else
    checkedMillis (left.val - right.val)

/-- Scaling accepts signed Int factors, rejecting negative values before
converting the nonnegative factor to a natural number. -/
def multiply (duration : DurationValue) (factor : IntValue) : DurationResult :=
  if factor.val < 0 then
    .fault .negativeFactor
  else
    checkedMillis (duration.val * factor.val.toNat)

/-- Integer division truncates the millisecond remainder and requires a
strictly positive signed divisor. -/
def divideByInt (duration : DurationValue) (divisor : IntValue) : DurationResult :=
  if divisor.val ≤ 0 then
    .fault .nonpositiveDivisor
  else
    checkedMillis (duration.val / divisor.val.toNat)

/-- Duration division produces a checked signed interval count. -/
def ratio (numerator denominator : DurationValue) : DurationResult :=
  if denominator.val = 0 then
    .fault .nonpositiveDivisor
  else
    match validateInt? (Int.ofNat (numerator.val / denominator.val)) with
    | some count => .count count
    | none => .fault .intervalOverflow

/-- Saturation is applied after negative values have been clamped by callers. -/
def saturatingDuration (millis : Nat) : DurationValue :=
  if withinRange : millis < signed64Width then
    ⟨millis, withinRange⟩
  else
    durationMax

/-- Convert a mathematical integer count to milliseconds, clamping negatives
to zero and saturating at the duration maximum. XSH callers supply signed-64
`IntValue`s; the wider domain here makes the saturation boundary explicit. -/
def timeMillis (count : Int) : DurationValue :=
  if count < 0 then durationZero else saturatingDuration count.toNat

/-- Convert seconds with saturating millisecond scaling, after clamping a
negative count to zero. -/
def timeSeconds (seconds : Int) : DurationValue :=
  if seconds < 0 then durationZero
  else saturatingDuration (seconds.toNat * 1000)

/-- A wrapping implementation reduces the mathematical sum modulo 2^64. This
is kept only as a mutation model for distinguishing checked addition. -/
def wrappingAdd (left right : DurationValue) : DurationValue :=
  ⟨(left.val + right.val) % signed64Width,
    Nat.mod_lt _ (by unfold signed64Width; omega)⟩

private def fiveMillis : DurationValue := ⟨5, by decide⟩
private def threeMillis : DurationValue := ⟨3, by decide⟩
private def eightMillis : DurationValue := ⟨8, by decide⟩
private def twoMillis : DurationValue := ⟨2, by decide⟩
private def sevenMillis : DurationValue := ⟨7, by decide⟩

def thousandMillis : DurationValue := ⟨1000, by decide⟩

/-- Addition preserves the exact sum whenever it fits the unsigned duration
domain. -/
theorem addition_returns_in_range_sum (left right : DurationValue)
    (inRange : left.val + right.val < signed64Width) :
    add left right = .millis ⟨left.val + right.val, inRange⟩ := by
  simp [add, checkedMillis, validateDuration?, inRange]

/-- Addition rejects every mathematical sum at or above the unsigned 64-bit
duration boundary. -/
theorem addition_rejects_out_of_range_sum (left right : DurationValue)
    (tooLarge : signed64Width ≤ left.val + right.val) :
    add left right = .fault .overflow := by
  have outOfRange : ¬ left.val + right.val < signed64Width := Nat.not_lt_of_ge tooLarge
  simp [add, checkedMillis, validateDuration?, outOfRange]

/-- Subtraction returns the exact difference when the right operand does not
exceed the left. -/
theorem subtraction_returns_ordered_difference (left right : DurationValue)
    (ordered : right.val ≤ left.val) :
    subtract left right = .millis ⟨left.val - right.val, by
      omega⟩ := by
  have resultInRange : left.val - right.val < signed64Width := by
    have leftInRange := left.property
    omega
  simp [subtract, Nat.not_lt_of_ge ordered, checkedMillis, validateDuration?,
    resultInRange]

/-- Subtraction reports underflow whenever the right operand exceeds the left. -/
theorem subtraction_rejects_larger_right_operand (left right : DurationValue)
    (tooSmall : left.val < right.val) :
    subtract left right = .fault .underflow := by
  simp [subtract, tooSmall]

/-- Nonnegative scaling preserves the exact product when it fits the duration
domain. -/
theorem multiplication_returns_in_range_product (duration : DurationValue)
    (factor : IntValue) (_nonnegative : 0 ≤ factor.val)
    (inRange : duration.val * factor.val.toNat < signed64Width) :
    multiply duration factor =
      .millis ⟨duration.val * factor.val.toNat, inRange⟩ := by
  have notNegative : ¬ factor.val < 0 := by omega
  simp [multiply, notNegative, checkedMillis, validateDuration?, inRange]

/-- Nonnegative scaling reports overflow whenever the mathematical product
reaches the unsigned duration boundary. -/
theorem multiplication_rejects_out_of_range_product (duration : DurationValue)
    (factor : IntValue) (_nonnegative : 0 ≤ factor.val)
    (tooLarge : signed64Width ≤ duration.val * factor.val.toNat) :
    multiply duration factor = .fault .overflow := by
  have notNegative : ¬ factor.val < 0 := by omega
  have outOfRange : ¬ duration.val * factor.val.toNat < signed64Width :=
    Nat.not_lt_of_ge tooLarge
  simp [multiply, notNegative, checkedMillis, validateDuration?, outOfRange]

/-- Negative signed factors have their explicit duration fault. -/
theorem multiplication_rejects_negative_factor (duration : DurationValue)
    (factor : IntValue) (negative : factor.val < 0) :
    multiply duration factor = .fault .negativeFactor := by
  simp [multiply, negative]

/-- A positive signed divisor returns the natural quotient, truncating the
millisecond remainder. -/
theorem division_returns_quotient_for_positive_divisor (duration : DurationValue)
    (divisor : IntValue) (positive : 0 < divisor.val) :
    divideByInt duration divisor =
      .millis ⟨duration.val / divisor.val.toNat, by
        have durationInRange := duration.property
        exact Nat.lt_of_le_of_lt (Nat.div_le_self _ _) durationInRange⟩ := by
  have notNonpositive : ¬ divisor.val ≤ 0 := by omega
  have quotientInRange : duration.val / divisor.val.toNat < signed64Width :=
    Nat.lt_of_le_of_lt (Nat.div_le_self _ _) duration.property
  simp [divideByInt, notNonpositive, checkedMillis, validateDuration?, quotientInRange]

/-- Zero and negative signed divisors report the specified nonpositive fault. -/
theorem division_rejects_nonpositive_divisor (duration : DurationValue)
    (divisor : IntValue) (nonpositive : divisor.val ≤ 0) :
    divideByInt duration divisor = .fault .nonpositiveDivisor := by
  simp [divideByInt, nonpositive]

/-- The quotient returns precisely the payload produced by signed validation. -/
theorem ratio_returns_validated_count (numerator denominator : DurationValue)
    (count : IntValue) :
    ratio numerator denominator = .count count ↔
      denominator.val ≠ 0 ∧
        validateInt? (Int.ofNat (numerator.val / denominator.val)) = some count := by
  by_cases zero : denominator.val = 0
  · simp [ratio, zero]
  · simp only [ratio, zero]
    cases checked : validateInt? (Int.ofNat (numerator.val / denominator.val)) <;>
      simp [zero]

/-- A zero duration divisor has the designated nonpositive-divisor fault. -/
theorem ratio_rejects_zero_denominator (numerator denominator : DurationValue)
    (zero : denominator.val = 0) :
    ratio numerator denominator = .fault .nonpositiveDivisor := by
  simp [ratio, zero]

/-- A nonzero duration divisor reports interval overflow exactly when the
shared signed-Int validator rejects the quotient. -/
theorem ratio_overflow_iff_validator_rejects (numerator denominator : DurationValue) :
    ratio numerator denominator = .fault .intervalOverflow ↔
      denominator.val ≠ 0 ∧
        validateInt? (Int.ofNat (numerator.val / denominator.val)) = none := by
  by_cases zero : denominator.val = 0
  · simp [ratio, zero]
  · simp only [ratio, zero]
    cases checked : validateInt? (Int.ofNat (numerator.val / denominator.val)) <;>
      simp [zero]

/-- Any nonzero duration divisor whose interval quotient exceeds signed Int is
rejected as an integer overflow. -/
theorem ratio_rejects_interval_count_overflow :
    ∀ numerator denominator,
      denominator.val ≠ 0 →
      signed64Limit.toNat ≤ numerator.val / denominator.val →
      ratio numerator denominator = .fault .intervalOverflow := by
  intro numerator denominator nonzero tooLarge
  have quotientOutOfIntRange :
      ¬ (signed64Min ≤ Int.ofNat (numerator.val / denominator.val) ∧
        Int.ofNat (numerator.val / denominator.val) < signed64Limit) := by
    intro inRange
    have tooLargeInt : signed64Limit ≤ Int.ofNat (numerator.val / denominator.val) := by
      simpa [signed64Limit] using
        (Int.ofNat_le.mpr tooLarge :
          Int.ofNat signed64Limit.toNat ≤ Int.ofNat (numerator.val / denominator.val))
    exact Int.not_lt_of_ge tooLargeInt inRange.2
  apply (ratio_overflow_iff_validator_rejects numerator denominator).2
  exact ⟨nonzero, by
    unfold validateInt?
    split
    · rename_i inRange
      exact False.elim (quotientOutOfIntRange inRange)
    · rfl⟩

/-- The unsigned duration maximum divided by one millisecond is an actual
interval-count overflow witness. -/
theorem duration_max_ratio_overflows_int :
    ratio durationMax durationOne = .fault .intervalOverflow := by
  decide

/-- The mathematical-Int conversion clamps negatives, preserves every
nonnegative in-range count, and saturates counts beyond the duration range.
Its saturation case is generalized: XSH `IntValue` inputs are signed 64-bit
and therefore cannot reach the millisecond upper boundary. -/
theorem time_millis_clamp_exact_saturation_laws :
    (∀ count, count < 0 → timeMillis count = durationZero) ∧
    (∀ count (_nonnegative : 0 ≤ count) (inRange : count.toNat < signed64Width),
      timeMillis count = ⟨count.toNat, inRange⟩) ∧
    (∀ count, 0 ≤ count → signed64Width ≤ count.toNat →
      timeMillis count = durationMax) ∧
    timeMillis (Int.ofNat signed64Width) = durationMax := by
  refine ⟨?_, ?_, ?_, by decide⟩
  · intro count negative
    simp [timeMillis, negative]
  · intro count nonnegative inRange
    have notNegative : ¬ count < 0 := by omega
    simp [timeMillis, notNegative, saturatingDuration, inRange]
  · intro count nonnegative tooLarge
    have notNegative : ¬ count < 0 := by omega
    have outOfRange : ¬ count.toNat < signed64Width := Nat.not_lt_of_ge tooLarge
    simp [timeMillis, notNegative, saturatingDuration, outOfRange]

/-- One positive second is exactly 1000 milliseconds. -/
theorem time_seconds_one_is_one_thousand_milliseconds :
    timeSeconds 1 = thousandMillis := by
  decide

/-- Seconds conversion clamps negative inputs, preserves in-range millisecond
products, and saturates products at or above the duration boundary. -/
theorem time_seconds_clamp_exact_saturation_laws :
    (∀ seconds, seconds < 0 → timeSeconds seconds = durationZero) ∧
    (∀ seconds (_nonnegative : 0 ≤ seconds)
        (_inRange : seconds.toNat * 1000 < signed64Width),
      (timeSeconds seconds).val = seconds.toNat * 1000) ∧
    (∀ seconds, 0 ≤ seconds → signed64Width ≤ seconds.toNat * 1000 →
      timeSeconds seconds = durationMax) ∧
    timeSeconds intMax.val = durationMax := by
  refine ⟨?_, ?_, ?_, by decide⟩
  · intro seconds negative
    simp [timeSeconds, negative]
  · intro seconds _nonnegative inRange
    have notNegative : ¬ seconds < 0 := by omega
    simp [timeSeconds, notNegative, saturatingDuration, inRange]
  · intro seconds nonnegative tooLarge
    have notNegative : ¬ seconds < 0 := by omega
    have outOfRange : ¬ seconds.toNat * 1000 < signed64Width :=
      Nat.not_lt_of_ge tooLarge
    simp [timeSeconds, notNegative, saturatingDuration, outOfRange]

/-- Wrapping at the u64 boundary changes max-plus-one to zero, while checked
addition preserves the overflow failure. -/
theorem checked_add_rejects_wrapping_mutation :
    add durationMax durationOne = .fault .overflow ∧
      wrappingAdd durationMax durationOne = durationZero := by
  decide

private theorem checked_add_returns_in_range_sum :
    add fiveMillis threeMillis = .millis eightMillis := by
  decide

private theorem checked_add_rejects_u64_overflow :
    add durationMax durationOne = .fault .overflow := by
  decide

private theorem checked_subtraction_rejects_negative_result :
    subtract durationZero durationOne = .fault .underflow := by
  decide

private theorem checked_multiply_rejects_negative_factor :
    multiply durationOne intNegativeOne = .fault .negativeFactor := by
  decide

private theorem checked_multiply_zero_factor_is_zero :
    multiply durationMax intZero = .millis durationZero := by
  decide

private theorem positive_division_truncates_milliseconds :
    divideByInt fiveMillis (⟨2, by decide⟩ : IntValue) = .millis twoMillis := by
  decide

private theorem nonpositive_divisors_are_rejected :
    divideByInt fiveMillis intZero = .fault .nonpositiveDivisor ∧
      divideByInt fiveMillis intNegativeOne = .fault .nonpositiveDivisor := by
  decide

private theorem duration_ratio_returns_interval_count :
    ratio sevenMillis twoMillis = .count (⟨3, by decide⟩ : IntValue) := by
  decide

private theorem time_millis_clamps_and_saturates :
    timeMillis (-1) = durationZero ∧
      timeMillis (Int.ofNat signed64Width) = durationMax := by
  decide

private theorem time_seconds_clamps_and_saturates :
    timeSeconds intMin.val = durationZero ∧
      timeSeconds intMax.val = durationMax := by
  decide

end Xsh.Values.DurationOps
