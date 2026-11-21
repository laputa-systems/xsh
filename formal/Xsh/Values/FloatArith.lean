import Xsh.Values.Scalars
import Init.Data.Rat

namespace Xsh.Values.FloatArith

def distance (a b : Nat) : Nat := (a - b) + (b - a)

def roundEven (numerator denominator : Nat) : Nat :=
  let q := numerator / denominator
  let r := numerator % denominator
  if denominator < 2 * r ∨ (denominator = 2 * r ∧ q % 2 = 1) then q + 1 else q

def NearestEven (numerator denominator rounded : Nat) : Prop :=
  (∀ candidate, distance numerator (rounded * denominator) ≤
    distance numerator (candidate * denominator)) ∧
  (∀ candidate, distance numerator (rounded * denominator) =
    distance numerator (candidate * denominator) → rounded % 2 = 0 ∨ candidate = rounded)

theorem roundEven_nearest (n d : Nat) (positive : 0 < d) :
    NearestEven n d (roundEven n d) := by
  have decomp := Nat.div_add_mod' n d
  have remBound := Nat.mod_lt n positive
  unfold roundEven NearestEven
  dsimp only
  split <;> rename_i h
  all_goals
    constructor
    · intro k
      unfold distance
      by_cases side : k ≤ n / d
      · have mul := Nat.mul_le_mul_right d side
        try simp only [Nat.add_mul, Nat.one_mul] at *
        omega
      · have side' : n / d + 1 ≤ k := by omega
        have mul := Nat.mul_le_mul_right d side'
        simp only [Nat.add_mul, Nat.one_mul] at mul ⊢
        omega
    · intro k eq
      unfold distance at eq
      have parity := Nat.mod_lt (n / d) (by decide : 0 < 2)
      by_cases atLower : k = n / d
      · subst k
        simp only [Nat.add_mul, Nat.one_mul] at *
        first | exact Or.inr trivial | omega
      · by_cases atUpper : k = n / d + 1
        · subst k
          simp only [Nat.add_mul, Nat.one_mul] at *
          first | exact Or.inr trivial | omega
        · by_cases side : k < n / d
          · have bound : k + 1 ≤ n / d := by omega
            have mul := Nat.mul_le_mul_right d bound
            simp only [Nat.add_mul, Nat.one_mul] at *
            omega
          · have bound : n / d + 2 ≤ k := by omega
            have mul := Nat.mul_le_mul_right d bound
            simp only [Nat.add_mul, Nat.one_mul] at *
            omega

def floatNegative (bits : FloatBits) : Bool := decide (floatSign ≤ bits.val)

def finiteMantissa (bits : FloatBits) : Nat :=
  if floatExponent bits = 0 then floatFraction bits
  else floatFractionWidth + floatFraction bits

def finiteMagnitude (bits : FloatBits) : Rat :=
  let exponent := floatExponent bits
  let scaled := finiteMantissa bits * 2 ^ (exponent - 1)
  mkRat (Int.ofNat scaled) (2 ^ 1074)

def finiteValue (bits : FloatBits) : Rat :=
  if floatNegative bits then -finiteMagnitude bits else finiteMagnitude bits

def magnitudeUnits (raw : Nat) : Nat :=
  let exponent := raw / floatFractionWidth
  let fraction := raw % floatFractionWidth
  if exponent = 0 then fraction
  else (floatFractionWidth + fraction) * 2 ^ (exponent - 1)

theorem magnitude_units_normal (exponent mantissa : Nat)
    (lower : floatFractionWidth ≤ mantissa)
    (upper : mantissa < 2 * floatFractionWidth) :
    magnitudeUnits ((exponent + 1) * floatFractionWidth + (mantissa - floatFractionWidth)) =
      mantissa * 2 ^ exponent := by
  have remainderBound : mantissa - floatFractionWidth < floatFractionWidth := by
    unfold floatFractionWidth at *
    omega
  have positive : 0 < floatFractionWidth := by decide
  simp [magnitudeUnits, Nat.add_comm, Nat.add_mul_div_right _ _ positive,
    Nat.div_eq_of_lt remainderBound, Nat.mod_eq_of_lt remainderBound]
  have restored : floatFractionWidth + (mantissa - floatFractionWidth) = mantissa := by omega
  rw [restored]

def roundAway (numerator denominator : Nat) : Nat :=
  let q := numerator / denominator
  let r := numerator % denominator
  if denominator ≤ 2 * r then q + 1 else q

def roundMagnitudeBits (numerator denominator : Nat) : Nat :=
  let scaled := numerator * 2 ^ 1074
  let exponent := min 2045 (Nat.log2 (scaled / denominator) - 52)
  let mantissa := roundEven scaled (denominator * 2 ^ exponent)
  if exponent = 0 ∧ mantissa < floatFractionWidth then mantissa
  else if 2 ^ 53 ≤ mantissa then
    if exponent = 2045 then (floatExponentLimit - 1) * floatFractionWidth
    else (exponent + 2) * floatFractionWidth
  else (exponent + 1) * floatFractionWidth + (mantissa - floatFractionWidth)

def finiteToInt (bits : FloatBits) (mode : Nat → Nat → Nat) : Option IntValue :=
  if floatExponent bits = floatExponentLimit - 1 then none
  else
    let value := finiteMagnitude bits
    let rounded := mode value.num.natAbs value.den
    validateInt? (if floatNegative bits then -Int.ofNat rounded else Int.ofNat rounded)

def floorMagnitude (numerator denominator : Nat) : Nat := numerator / denominator

def ceilMagnitude (numerator denominator : Nat) : Nat :=
  (numerator + denominator - 1) / denominator

def floatFloor (bits : FloatBits) : Option IntValue :=
  finiteToInt bits (if floatNegative bits then ceilMagnitude else floorMagnitude)

def floatCeil (bits : FloatBits) : Option IntValue :=
  finiteToInt bits (if floatNegative bits then floorMagnitude else ceilMagnitude)

def floatRound (bits : FloatBits) : Option IntValue := finiteToInt bits roundAway

def intToFloatMagnitude (value : IntValue) : Nat := roundMagnitudeBits value.val.natAbs 1

theorem round_even_tie_to_even : roundEven 5 2 = 2 ∧ roundEven 7 2 = 4 := by
  decide

theorem round_away_tie_witness : roundAway 5 2 = 3 := by
  decide

theorem ties_away_mutant_rejected : ¬ NearestEven 5 2 (roundAway 5 2) := by
  intro correct
  have tie := correct.2 2
  revert tie
  decide

theorem ties_even_conversion_mutant_rejected : roundEven 5 2 ≠ roundAway 5 2 := by
  decide

set_option exponentiation.threshold 1100 in
set_option maxRecDepth 10000 in
theorem zero_rounds_to_positive_zero : roundMagnitudeBits 0 1 = 0 := by
  decide

set_option exponentiation.threshold 1100 in
set_option maxRecDepth 10000 in
theorem one_rounds_to_one : roundMagnitudeBits 1 1 = 4607182418800017408 := by
  decide

set_option exponentiation.threshold 1100 in
set_option maxRecDepth 10000 in
theorem two_rounds_to_two : roundMagnitudeBits 2 1 = 4611686018427387904 := by
  decide

set_option exponentiation.threshold 1100 in
set_option maxRecDepth 10000 in
theorem tiny_subnormal_rounds_to_zero : roundMagnitudeBits 1 (2 ^ 1075) = 0 := by
  decide

set_option exponentiation.threshold 1100 in
set_option maxRecDepth 10000 in
theorem least_subnormal_rounds_exactly : roundMagnitudeBits 1 (2 ^ 1074) = 1 := by
  decide

theorem conversion_rejects_positive_infinity : floatRound floatPositiveInfinity = none := by
  decide

theorem conversion_rejects_nan : floatRound floatQuietNaN = none := by
  decide

end Xsh.Values.FloatArith

namespace Xsh.Values.FloatArith

def BinaryNearest (numerator denominator raw : Nat) : Prop :=
  raw < (floatExponentLimit - 1) * floatFractionWidth ∧
  (∀ candidate, candidate < (floatExponentLimit - 1) * floatFractionWidth →
    distance (numerator * 2 ^ 1074) (denominator * magnitudeUnits raw) ≤
      distance (numerator * 2 ^ 1074) (denominator * magnitudeUnits candidate)) ∧
  (∀ candidate, candidate < (floatExponentLimit - 1) * floatFractionWidth →
    distance (numerator * 2 ^ 1074) (denominator * magnitudeUnits raw) =
      distance (numerator * 2 ^ 1074) (denominator * magnitudeUnits candidate) →
        raw % 2 = 0 ∨ candidate = raw)

def exponentOffByOne (raw : Nat) : Nat :=
  let exponent := raw / floatFractionWidth
  let fraction := raw % floatFractionWidth
  if exponent = 0 then fraction
  else (floatFractionWidth + fraction) * 2 ^ exponent

set_option exponentiation.threshold 1100 in
set_option maxRecDepth 10000 in
theorem exponent_off_by_one_rejected :
    exponentOffByOne 4607182418800017408 ≠ magnitudeUnits 4607182418800017408 := by
  decide

theorem binary_subnormal_nearest (n d : Nat) (positive : 0 < d)
    (small : Nat.log2 (n * 2 ^ 1074 / d) ≤ 52)
    (fits : roundEven (n * 2 ^ 1074) d < floatFractionWidth)
    (candidate : Nat) :
    distance (n * 2 ^ 1074) (d * magnitudeUnits (roundMagnitudeBits n d)) ≤
      distance (n * 2 ^ 1074) (d * magnitudeUnits candidate) := by
  have nearest := (roundEven_nearest (n * 2 ^ 1074) d positive).1 (magnitudeUnits candidate)
  simpa [roundMagnitudeBits, small, fits, magnitudeUnits, Nat.mod_eq_of_lt fits, Nat.mul_comm] using nearest


end Xsh.Values.FloatArith
