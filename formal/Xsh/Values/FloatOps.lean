import Xsh.Values.FloatArith

namespace Xsh.Values.FloatOps

open Xsh.Values
open Xsh.Values.FloatArith

def infinityRaw : Nat := (floatExponentLimit - 1) * floatFractionWidth

def floatMagnitudeRaw (bits : FloatBits) : Nat := bits.val % floatSign

theorem round_magnitude_bits_le_infinity (n d : Nat) : roundMagnitudeBits n d ≤ infinityRaw := by
  unfold roundMagnitudeBits infinityRaw
  dsimp only
  generalize exponentDef : min 2045 (Nat.log2 (n * 2 ^ 1074 / d) - 52) = exponent
  generalize roundEven (n * 2 ^ 1074) (d * 2 ^ exponent) = mantissa
  have capped : exponent ≤ 2045 := by omega
  unfold floatExponentLimit floatFractionWidth
  split
  · omega
  · split
    · split <;> omega
    · omega

def signedFloat (negative : Bool) (magnitude : Nat) (bounded : magnitude ≤ infinityRaw) :
    FloatBits :=
  ⟨(if negative then floatSign else 0) + magnitude, by
    unfold infinityRaw floatExponentLimit floatFractionWidth at bounded
    unfold floatSign signed64Width
    split <;> omega⟩

theorem signed_float_negative (negative : Bool) (magnitude : Nat)
    (bounded : magnitude ≤ infinityRaw) :
    floatNegative (signedFloat negative magnitude bounded) = negative := by
  unfold infinityRaw floatExponentLimit floatFractionWidth at bounded
  cases negative <;> simp [floatNegative, signedFloat, floatSign] <;> omega

theorem signed_float_magnitude (negative : Bool) (magnitude : Nat)
    (bounded : magnitude ≤ infinityRaw) :
    floatMagnitudeRaw (signedFloat negative magnitude bounded) = magnitude := by
  unfold infinityRaw floatExponentLimit floatFractionWidth at bounded
  cases negative <;> simp [floatMagnitudeRaw, signedFloat, floatSign] <;> omega

theorem float_bits_ext {left right : FloatBits}
    (sign : floatNegative left = floatNegative right)
    (magnitude : floatMagnitudeRaw left = floatMagnitudeRaw right) : left = right := by
  apply Fin.ext
  have leftBound := left.isLt
  have rightBound := right.isLt
  unfold floatNegative at sign
  unfold floatMagnitudeRaw at magnitude
  unfold signed64Width at leftBound rightBound
  unfold floatSign at sign magnitude
  by_cases leftSign : 2 ^ 63 ≤ left.val <;> by_cases rightSign : 2 ^ 63 ≤ right.val <;>
    simp [leftSign, rightSign] at sign <;> omega

def floatInfinity (negative : Bool) : FloatBits :=
  signedFloat negative infinityRaw (Nat.le_refl _)

def floatZero (negative : Bool) : FloatBits := signedFloat negative 0 (Nat.zero_le _)

def roundToFloat (negative : Bool) (numerator denominator : Nat) : FloatBits :=
  signedFloat negative (roundMagnitudeBits numerator denominator)
    (round_magnitude_bits_le_infinity numerator denominator)

inductive FloatClass where
  | nan
  | infinite (negative : Bool)
  | finite (negative : Bool) (units : Nat)
  deriving DecidableEq

def floatClass (bits : FloatBits) : FloatClass :=
  if infinityRaw < floatMagnitudeRaw bits then .nan
  else if floatMagnitudeRaw bits = infinityRaw then .infinite (floatNegative bits)
  else .finite (floatNegative bits) (magnitudeUnits (floatMagnitudeRaw bits))

def FloatClass.negate : FloatClass → FloatClass
  | .nan => .nan
  | .infinite negative => .infinite (!negative)
  | .finite negative units => .finite (!negative) units

theorem float_class_nan_iff (bits : FloatBits) : floatClass bits = .nan ↔ floatIsNaN bits := by
  have bound := bits.isLt
  unfold signed64Width at bound
  unfold floatClass floatIsNaN floatExponent floatFraction floatMagnitudeRaw infinityRaw
    floatExponentLimit floatFractionWidth floatSign
  by_cases above : (2 ^ 11 - 1) * 2 ^ 52 < bits.val % 2 ^ 63
  · simp only [above, ite_true, true_iff]
    omega
  · simp only [above, ite_false]
    constructor
    · intro classified
      split at classified <;> cases classified
    · intro isNaN
      omega

theorem float_class_infinite_iff (bits : FloatBits) (negative : Bool) :
    floatClass bits = .infinite negative ↔
      floatMagnitudeRaw bits = infinityRaw ∧ floatNegative bits = negative := by
  unfold floatClass
  by_cases above : infinityRaw < floatMagnitudeRaw bits
  · simp only [above, ite_true]
    constructor
    · intro impossible
      cases impossible
    · intro ⟨exact, _⟩
      omega
  · by_cases exact : floatMagnitudeRaw bits = infinityRaw
    · simp [exact]
    · simp [above, exact]

theorem float_class_finite_iff (bits : FloatBits) (negative : Bool) (units : Nat) :
    floatClass bits = .finite negative units ↔
      floatMagnitudeRaw bits < infinityRaw ∧ floatNegative bits = negative ∧
        magnitudeUnits (floatMagnitudeRaw bits) = units := by
  unfold floatClass
  by_cases above : infinityRaw < floatMagnitudeRaw bits
  · simp only [above, ite_true]
    constructor
    · intro impossible
      cases impossible
    · intro ⟨below, _⟩
      omega
  · by_cases exact : floatMagnitudeRaw bits = infinityRaw
    · simp [exact]
    · have below : floatMagnitudeRaw bits < infinityRaw := by omega
      simp [above, exact, below]

def signedUnits (negative : Bool) (units : Nat) : Int :=
  if negative then -(units : Int) else units

inductive FloatOutcome where
  | nan
  | infinite (negative : Bool)
  | zero (negative : Bool)
  | rounded (negative : Bool) (numerator denominator : Nat)

def FloatOutcome.Holds : FloatOutcome → FloatBits → Prop
  | .nan, result => floatIsNaN result
  | .infinite negative, result =>
    floatNegative result = negative ∧ floatMagnitudeRaw result = infinityRaw
  | .zero negative, result => floatNegative result = negative ∧ floatMagnitudeRaw result = 0
  | .rounded negative numerator denominator, result =>
    floatNegative result = negative ∧
      (numerator * 2 ^ 1074 < denominator * overflowMidpointUnits →
        BinaryNearest numerator denominator (floatMagnitudeRaw result)) ∧
      (denominator * overflowMidpointUnits ≤ numerator * 2 ^ 1074 →
        floatMagnitudeRaw result = infinityRaw)

def FloatOutcome.WellFormed : FloatOutcome → Prop
  | .rounded _ _ denominator => 0 < denominator
  | _ => True

def sumOutcome : FloatClass → FloatClass → FloatOutcome
  | .nan, _ => .nan
  | _, .nan => .nan
  | .infinite left, .infinite right => if left = right then .infinite left else .nan
  | .infinite left, .finite _ _ => .infinite left
  | .finite _ _, .infinite right => .infinite right
  | .finite leftNegative left, .finite rightNegative right =>
    let exact := signedUnits leftNegative left + signedUnits rightNegative right
    if exact = 0 then .zero (leftNegative && rightNegative)
    else .rounded (decide (exact < 0)) exact.natAbs (2 ^ 1074)

def productOutcome : FloatClass → FloatClass → FloatOutcome
  | .nan, _ => .nan
  | _, .nan => .nan
  | .infinite left, .infinite right => .infinite (Bool.xor left right)
  | .infinite left, .finite right units =>
    if units = 0 then .nan else .infinite (Bool.xor left right)
  | .finite left units, .infinite right =>
    if units = 0 then .nan else .infinite (Bool.xor left right)
  | .finite leftNegative left, .finite rightNegative right =>
    .rounded (Bool.xor leftNegative rightNegative) (left * right) (2 ^ 2148)

def quotientOutcome : FloatClass → FloatClass → FloatOutcome
  | .nan, _ => .nan
  | _, .nan => .nan
  | .infinite _, .infinite _ => .nan
  | .infinite left, .finite right _ => .infinite (Bool.xor left right)
  | .finite left _, .infinite right => .zero (Bool.xor left right)
  | .finite leftNegative left, .finite rightNegative right =>
    if right = 0 then
      if left = 0 then .nan else .infinite (Bool.xor leftNegative rightNegative)
    else .rounded (Bool.xor leftNegative rightNegative) left right

inductive FloatBinaryOp where
  | add
  | subtract
  | multiply
  | divide
  deriving DecidableEq

def floatOutcome : FloatBinaryOp → FloatClass → FloatClass → FloatOutcome
  | .add, left, right => sumOutcome left right
  | .subtract, left, right => sumOutcome left right.negate
  | .multiply, left, right => productOutcome left right
  | .divide, left, right => quotientOutcome left right

def FloatMeaning (operator : FloatBinaryOp) (left right result : FloatBits) : Prop :=
  (floatOutcome operator (floatClass left) (floatClass right)).Holds result

structure NaNPolicy where
  choose : FloatBinaryOp → FloatBits → FloatBits → FloatBits
  chooses_nan : ∀ operator left right, floatIsNaN (choose operator left right)

def FloatOutcome.realize (nan : FloatBits) : FloatOutcome → FloatBits
  | .nan => nan
  | .infinite negative => floatInfinity negative
  | .zero negative => floatZero negative
  | .rounded negative numerator denominator => roundToFloat negative numerator denominator

def floatApplyWith (nan : FloatBits) (operator : FloatBinaryOp) (left right : FloatBits) :
    FloatBits :=
  (floatOutcome operator (floatClass left) (floatClass right)).realize nan

def floatApply (policy : NaNPolicy) (operator : FloatBinaryOp) (left right : FloatBits) :
    FloatBits :=
  floatApplyWith (policy.choose operator left right) operator left right

theorem outcome_well_formed (operator : FloatBinaryOp) (left right : FloatClass) :
    (floatOutcome operator left right).WellFormed := by
  cases operator <;> cases left <;> cases right <;>
    simp only [floatOutcome, sumOutcome, productOutcome, quotientOutcome, FloatClass.negate] <;>
    (try split) <;> (try split) <;>
    simp [FloatOutcome.WellFormed, Nat.two_pow_pos] <;> omega

theorem realized_outcome_holds (nan : FloatBits) (isNaN : floatIsNaN nan)
    (outcome : FloatOutcome) (wellFormed : outcome.WellFormed) :
    outcome.Holds (outcome.realize nan) := by
  cases outcome with
  | nan => exact isNaN
  | infinite negative =>
    exact ⟨signed_float_negative negative infinityRaw (Nat.le_refl _),
      signed_float_magnitude negative infinityRaw (Nat.le_refl _)⟩
  | zero negative =>
    exact ⟨signed_float_negative negative 0 (Nat.zero_le _),
      signed_float_magnitude negative 0 (Nat.zero_le _)⟩
  | rounded negative numerator denominator =>
    refine ⟨signed_float_negative _ _ (round_magnitude_bits_le_infinity _ _), ?_⟩
    simp only [FloatOutcome.realize, roundToFloat, signed_float_magnitude]
    exact ⟨round_magnitude_bits_nearest numerator denominator wellFormed,
      round_magnitude_bits_overflow numerator denominator wellFormed⟩

theorem outcome_holds_unique (outcome : FloatOutcome) (wellFormed : outcome.WellFormed)
    {first second : FloatBits} (firstHolds : outcome.Holds first)
    (secondHolds : outcome.Holds second) :
    (floatIsNaN first ∧ floatIsNaN second) ∨ first = second := by
  cases outcome with
  | nan => exact Or.inl ⟨firstHolds, secondHolds⟩
  | infinite negative =>
    exact Or.inr (float_bits_ext (firstHolds.1.trans secondHolds.1.symm)
      (firstHolds.2.trans secondHolds.2.symm))
  | zero negative =>
    exact Or.inr (float_bits_ext (firstHolds.1.trans secondHolds.1.symm)
      (firstHolds.2.trans secondHolds.2.symm))
  | rounded negative numerator denominator =>
    obtain ⟨firstSign, firstNearest, firstOverflow⟩ := firstHolds
    obtain ⟨secondSign, secondNearest, secondOverflow⟩ := secondHolds
    refine Or.inr (float_bits_ext (firstSign.trans secondSign.symm) ?_)
    by_cases below : numerator * 2 ^ 1074 < denominator * overflowMidpointUnits
    · exact binary_nearest_unique numerator denominator wellFormed (firstNearest below)
        (secondNearest below)
    · have above := Nat.le_of_not_lt below
      exact (firstOverflow above).trans (secondOverflow above).symm

theorem float_apply_meaning (policy : NaNPolicy) (operator : FloatBinaryOp)
    (left right : FloatBits) :
    FloatMeaning operator left right (floatApply policy operator left right) :=
  realized_outcome_holds _ (policy.chooses_nan operator left right) _
    (outcome_well_formed operator _ _)

theorem float_meaning_determines_result (operator : FloatBinaryOp) (left right : FloatBits)
    {first second : FloatBits} (firstMeaning : FloatMeaning operator left right first)
    (secondMeaning : FloatMeaning operator left right second) :
    (floatIsNaN first ∧ floatIsNaN second) ∨ first = second :=
  outcome_holds_unique _ (outcome_well_formed operator _ _) firstMeaning secondMeaning

def unitScale : Int := ((2 ^ 1074 : Nat) : Int)

def FloorBound (scaled rounded : Int) : Prop :=
  rounded * unitScale ≤ scaled ∧ scaled < rounded * unitScale + unitScale

def CeilBound (scaled rounded : Int) : Prop :=
  rounded * unitScale - unitScale < scaled ∧ scaled ≤ rounded * unitScale

def RoundAwayBound (scaled rounded : Int) : Prop :=
  2 * scaled - unitScale ≤ 2 * (rounded * unitScale) ∧
    2 * (rounded * unitScale) ≤ 2 * scaled + unitScale ∧
    (2 * (rounded * unitScale) = 2 * scaled + unitScale → 0 < scaled) ∧
    (2 * (rounded * unitScale) = 2 * scaled - unitScale → scaled < 0)

def ToIntMeaning (bound : Int → Int → Prop) (bits : FloatBits) (result : Option IntValue) :
    Prop :=
  match floatClass bits with
  | .finite negative units =>
    ∃ rounded, bound (signedUnits negative units) rounded ∧ result = validateInt? rounded
  | _ => result = none

def floatToInt (mode : Bool → Nat → Nat → Nat) (bits : FloatBits) : Option IntValue :=
  match floatClass bits with
  | .finite negative units => validateInt? (signedUnits negative (mode negative units (2 ^ 1074)))
  | _ => none

def floatFloor : FloatBits → Option IntValue :=
  floatToInt fun negative => if negative then ceilMagnitude else floorMagnitude

def floatCeil : FloatBits → Option IntValue :=
  floatToInt fun negative => if negative then floorMagnitude else ceilMagnitude

def floatRound : FloatBits → Option IntValue := floatToInt fun _ => roundAway

theorem float_to_int_meaning (bound : Int → Int → Prop) (mode : Bool → Nat → Nat → Nat)
    (correct : ∀ negative units,
      bound (signedUnits negative units) (signedUnits negative (mode negative units (2 ^ 1074))))
    (bits : FloatBits) : ToIntMeaning bound bits (floatToInt mode bits) := by
  unfold ToIntMeaning floatToInt
  cases floatClass bits with
  | nan => rfl
  | infinite _ => rfl
  | finite negative units => exact ⟨_, correct negative units, rfl⟩

theorem to_int_meaning_unique (bound : Int → Int → Prop)
    (functional : ∀ scaled first second, bound scaled first → bound scaled second → first = second)
    (bits : FloatBits) {first second : Option IntValue}
    (firstMeaning : ToIntMeaning bound bits first) (secondMeaning : ToIntMeaning bound bits second) :
    first = second := by
  unfold ToIntMeaning at firstMeaning secondMeaning
  generalize floatClass bits = classified at firstMeaning secondMeaning
  cases classified with
  | finite negative units =>
    obtain ⟨firstRounded, firstBound, firstResult⟩ := firstMeaning
    obtain ⟨secondRounded, secondBound, secondResult⟩ := secondMeaning
    rw [firstResult, secondResult, functional _ _ _ firstBound secondBound]
  | nan => exact firstMeaning.trans secondMeaning.symm
  | infinite _ => exact firstMeaning.trans secondMeaning.symm

theorem floor_bound_functional (scaled first second : Int)
    (firstBound : FloorBound scaled first) (secondBound : FloorBound scaled second) :
    first = second := by
  have positive : (0 : Int) < unitScale := by unfold unitScale; exact_mod_cast Nat.two_pow_pos 1074
  have ordered : ∀ low high : Int, low < high → FloorBound scaled low → FloorBound scaled high →
      False := by
    intro low high lower lowBound highBound
    have step := Int.mul_le_mul_of_nonneg_right (show low + 1 ≤ high by omega)
      (Int.le_of_lt positive)
    rw [Int.add_mul, Int.one_mul] at step
    unfold FloorBound at lowBound highBound
    omega
  rcases Int.lt_trichotomy first second with lower | same | higher
  · exact (ordered _ _ lower firstBound secondBound).elim
  · exact same
  · exact (ordered _ _ higher secondBound firstBound).elim

theorem ceil_bound_functional (scaled first second : Int)
    (firstBound : CeilBound scaled first) (secondBound : CeilBound scaled second) :
    first = second := by
  have positive : (0 : Int) < unitScale := by unfold unitScale; exact_mod_cast Nat.two_pow_pos 1074
  have ordered : ∀ low high : Int, low < high → CeilBound scaled low → CeilBound scaled high →
      False := by
    intro low high lower lowBound highBound
    have step := Int.mul_le_mul_of_nonneg_right (show low + 1 ≤ high by omega)
      (Int.le_of_lt positive)
    rw [Int.add_mul, Int.one_mul] at step
    unfold CeilBound at lowBound highBound
    omega
  rcases Int.lt_trichotomy first second with lower | same | higher
  · exact (ordered _ _ lower firstBound secondBound).elim
  · exact same
  · exact (ordered _ _ higher secondBound firstBound).elim

theorem round_away_bound_functional (scaled first second : Int)
    (firstBound : RoundAwayBound scaled first) (secondBound : RoundAwayBound scaled second) :
    first = second := by
  have positive : (0 : Int) < unitScale := by unfold unitScale; exact_mod_cast Nat.two_pow_pos 1074
  have ordered : ∀ low high : Int, low < high → RoundAwayBound scaled low →
      RoundAwayBound scaled high → False := by
    intro low high lower lowBound highBound
    unfold RoundAwayBound at lowBound highBound
    by_cases adjacent : high = low + 1
    · subst adjacent
      rw [Int.add_mul, Int.one_mul] at highBound
      omega
    · have step := Int.mul_le_mul_of_nonneg_right (show low + 2 ≤ high by omega)
        (Int.le_of_lt positive)
      rw [Int.add_mul] at step
      omega
  rcases Int.lt_trichotomy first second with lower | same | higher
  · exact (ordered _ _ lower firstBound secondBound).elim
  · exact same
  · exact (ordered _ _ higher secondBound firstBound).elim

theorem floor_magnitude_bounds (units scale : Nat) (positive : 0 < scale) :
    floorMagnitude units scale * scale ≤ units ∧
      units < floorMagnitude units scale * scale + scale := by
  have parts := Nat.div_add_mod units scale
  have remainder := Nat.mod_lt units positive
  unfold floorMagnitude
  rw [Nat.mul_comm] at parts
  omega

theorem ceil_magnitude_bounds (units scale : Nat) (positive : 0 < scale) :
    units ≤ ceilMagnitude units scale * scale ∧
      ceilMagnitude units scale * scale < units + scale := by
  have parts := Nat.div_add_mod (units + scale - 1) scale
  have remainder := Nat.mod_lt (units + scale - 1) positive
  unfold ceilMagnitude
  rw [Nat.mul_comm] at parts
  omega

theorem round_away_bounds (units scale : Nat) (positive : 0 < scale) :
    2 * units ≤ 2 * (roundAway units scale * scale) + scale ∧
      2 * (roundAway units scale * scale) ≤ 2 * units + scale ∧
      (2 * (roundAway units scale * scale) = 2 * units + scale → 0 < units) ∧
      2 * (roundAway units scale * scale) + scale ≠ 2 * units := by
  have parts := Nat.div_add_mod units scale
  have remainder := Nat.mod_lt units positive
  unfold roundAway
  dsimp only
  rw [Nat.mul_comm] at parts
  split
  · rw [Nat.add_mul, Nat.one_mul]
    omega
  · omega

theorem scale_cast : ((2 ^ 1074 : Nat) : Int) = unitScale := rfl

theorem floor_is_correct (negative : Bool) (units : Nat) :
    FloorBound (signedUnits negative units)
      (signedUnits negative ((if negative then ceilMagnitude else floorMagnitude) units (2 ^ 1074))) := by
  unfold FloorBound signedUnits
  cases negative with
  | false =>
    have bounds := floor_magnitude_bounds units (2 ^ 1074) (Nat.two_pow_pos _)
    simp only [Bool.false_eq_true, ite_false]
    rw [← scale_cast]
    generalize floorMagnitude units (2 ^ 1074) = rounded at *
    generalize 2 ^ 1074 = scale at *
    constructor
    · exact_mod_cast bounds.1
    · exact_mod_cast bounds.2
  | true =>
    have bounds := ceil_magnitude_bounds units (2 ^ 1074) (Nat.two_pow_pos _)
    simp only [ite_true]
    rw [← scale_cast]
    generalize ceilMagnitude units (2 ^ 1074) = rounded at *
    generalize 2 ^ 1074 = scale at *
    have lower : ((units : Nat) : Int) ≤ (rounded : Int) * scale := by exact_mod_cast bounds.1
    have upper : (rounded : Int) * scale < units + scale := by exact_mod_cast bounds.2
    rw [Int.neg_mul]
    omega

theorem ceil_is_correct (negative : Bool) (units : Nat) :
    CeilBound (signedUnits negative units)
      (signedUnits negative ((if negative then floorMagnitude else ceilMagnitude) units (2 ^ 1074))) := by
  unfold CeilBound signedUnits
  cases negative with
  | false =>
    have bounds := ceil_magnitude_bounds units (2 ^ 1074) (Nat.two_pow_pos _)
    simp only [Bool.false_eq_true, ite_false]
    rw [← scale_cast]
    generalize ceilMagnitude units (2 ^ 1074) = rounded at *
    generalize 2 ^ 1074 = scale at *
    have lower : ((units : Nat) : Int) ≤ (rounded : Int) * scale := by exact_mod_cast bounds.1
    have upper : (rounded : Int) * scale < units + scale := by exact_mod_cast bounds.2
    omega
  | true =>
    have bounds := floor_magnitude_bounds units (2 ^ 1074) (Nat.two_pow_pos _)
    simp only [ite_true]
    rw [← scale_cast]
    generalize floorMagnitude units (2 ^ 1074) = rounded at *
    generalize 2 ^ 1074 = scale at *
    have lower : (rounded : Int) * scale ≤ units := by exact_mod_cast bounds.1
    have upper : (units : Int) < rounded * scale + scale := by exact_mod_cast bounds.2
    rw [Int.neg_mul]
    omega

theorem round_is_correct (negative : Bool) (units : Nat) :
    RoundAwayBound (signedUnits negative units)
      (signedUnits negative (roundAway units (2 ^ 1074))) := by
  have bounds := round_away_bounds units (2 ^ 1074) (Nat.two_pow_pos _)
  unfold RoundAwayBound signedUnits
  rw [← scale_cast]
  generalize roundAway units (2 ^ 1074) = rounded at *
  generalize 2 ^ 1074 = scale at *
  obtain ⟨lower, upper, tieUp, noTieDown⟩ := bounds
  have lower' : 2 * (units : Int) ≤ 2 * ((rounded : Int) * scale) + scale := by exact_mod_cast lower
  have upper' : 2 * ((rounded : Int) * scale) ≤ 2 * units + scale := by exact_mod_cast upper
  have tieUp' : 2 * ((rounded : Int) * scale) = 2 * units + scale → (0 : Int) < units := by
    intro tie
    have := tieUp (by exact_mod_cast tie)
    exact_mod_cast this
  have noTieDown' : 2 * ((rounded : Int) * scale) + scale ≠ 2 * units := by
    intro tie
    exact noTieDown (by exact_mod_cast tie)
  cases negative with
  | false =>
    simp only [Bool.false_eq_true, ite_false]
    omega
  | true =>
    simp only [ite_true]
    rw [Int.neg_mul]
    omega

def intToFloat (value : IntValue) : FloatBits :=
  roundToFloat (decide (value.val < 0)) value.val.natAbs 1

def IntToFloatMeaning (value : IntValue) (result : FloatBits) : Prop :=
  (FloatOutcome.rounded (decide (value.val < 0)) value.val.natAbs 1).Holds result

theorem quiet_nan_is_nan : floatIsNaN floatQuietNaN := by
  unfold floatIsNaN
  decide

theorem negative_quiet_nan_is_nan : floatIsNaN floatNegativeQuietNaN := by
  unfold floatIsNaN
  decide

theorem int_to_float_meaning (value : IntValue) : IntToFloatMeaning value (intToFloat value) :=
  realized_outcome_holds floatQuietNaN quiet_nan_is_nan
    (.rounded (decide (value.val < 0)) value.val.natAbs 1) Nat.one_pos

theorem int_to_float_unique (value : IntValue) {first second : FloatBits}
    (firstMeaning : IntToFloatMeaning value first) (secondMeaning : IntToFloatMeaning value second) :
    first = second := by
  have finite : ∀ result, IntToFloatMeaning value result → ¬ floatIsNaN result := by
    intro result meaning isNaN
    have bound := value.property
    unfold signed64Min signed64Limit at bound
    have magnitude : value.val.natAbs ≤ 2 ^ 63 := by omega
    have scaled := Nat.mul_le_mul_right (2 ^ 1074) magnitude
    rw [← Nat.pow_add] at scaled
    have smaller : 2 ^ (63 + 1074) < 2 ^ 2044 := Nat.pow_lt_pow_right (by decide) (by decide)
    have midpoint : 2 ^ 2044 ≤ overflowMidpointUnits := by
      unfold overflowMidpointUnits
      exact Nat.le_mul_of_pos_left _ (by decide)
    have nearest := meaning.2.1 (by omega)
    have nonNaN := (float_class_nan_iff result).2 isNaN
    unfold floatClass at nonNaN
    have := nearest.1
    unfold infinityRaw at nonNaN
    split at nonNaN
    · omega
    · split at nonNaN <;> cases nonNaN
  rcases outcome_holds_unique (.rounded (decide (value.val < 0)) value.val.natAbs 1) Nat.one_pos
    firstMeaning secondMeaning with ⟨isNaN, _⟩ | same
  · exact (finite first firstMeaning isNaN).elim
  · exact same

def quietNaNPolicy : NaNPolicy := ⟨fun _ _ _ => floatQuietNaN, fun _ _ _ => quiet_nan_is_nan⟩

def negativeNaNPolicy : NaNPolicy :=
  ⟨fun _ _ _ => floatNegativeQuietNaN, fun _ _ _ => negative_quiet_nan_is_nan⟩

def floatOfBits (raw : Nat) (fits : raw < signed64Width := by decide) : FloatBits := ⟨raw, fits⟩

def floatOne : FloatBits := floatOfBits 0x3FF0000000000000
def floatTwo : FloatBits := floatOfBits 0x4000000000000000
def floatThree : FloatBits := floatOfBits 0x4008000000000000
def floatTenth : FloatBits := floatOfBits 0x3FB999999999999A
def floatFifth : FloatBits := floatOfBits 0x3FC999999999999A
def floatTenthPlusFifth : FloatBits := floatOfBits 0x3FD3333333333334
def floatHalfUlpOfOne : FloatBits := floatOfBits 0x3CA0000000000000
def floatOneAndUlp : FloatBits := floatOfBits 0x3FF0000000000001
def floatOneAndTwoUlps : FloatBits := floatOfBits 0x3FF0000000000002
def floatLargest : FloatBits := floatOfBits 0x7FEFFFFFFFFFFFFF
def floatLeastSubnormal : FloatBits := floatOfBits 1
def floatTwoLeastSubnormals : FloatBits := floatOfBits 2
def floatThreeLeastSubnormals : FloatBits := floatOfBits 3
def floatNegativeOne : FloatBits := floatOfBits 0xBFF0000000000000
def floatHalf : FloatBits := floatOfBits 0x3FE0000000000000
def floatTwoAndHalf : FloatBits := floatOfBits 0x4004000000000000
def floatNegativeTwoAndHalf : FloatBits := floatOfBits 0xC004000000000000
def floatTwoPow53 : FloatBits := floatOfBits 0x4340000000000000
def floatTwoPow63 : FloatBits := floatOfBits 0x43E0000000000000
def floatNegativeTwoPow63 : FloatBits := floatOfBits 0xC3E0000000000000

def intValue (value : Int) (inRange : signed64Min ≤ value ∧ value < signed64Limit := by
    unfold signed64Min signed64Limit; omega) : IntValue := ⟨value, inRange⟩

theorem float_meaning_rejects (policy : NaNPolicy) (operator : FloatBinaryOp)
    (left right candidate : FloatBits) (notNaN : ¬ floatIsNaN candidate)
    (different : candidate ≠ floatApply policy operator left right) :
    ¬ FloatMeaning operator left right candidate := by
  intro meaning
  rcases float_meaning_determines_result operator left right meaning
      (float_apply_meaning policy operator left right) with ⟨isNaN, _⟩ | same
  · exact notNaN isNaN
  · exact different same

set_option maxRecDepth 10000 in
theorem float_arithmetic_witnesses :
    floatApply quietNaNPolicy .add floatTenth floatFifth = floatTenthPlusFifth ∧
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
    floatApply quietNaNPolicy .subtract floatNegativeZero floatPositiveZero = floatNegativeZero ∧
    floatApply quietNaNPolicy .multiply floatNegativeOne floatPositiveZero = floatNegativeZero ∧
    floatApply quietNaNPolicy .divide floatOne floatPositiveZero = floatPositiveInfinity ∧
    floatApply quietNaNPolicy .divide floatNegativeOne floatPositiveZero = floatNegativeInfinity ∧
    floatApply quietNaNPolicy .divide floatOne floatNegativeZero = floatNegativeInfinity ∧
    floatApply quietNaNPolicy .divide floatOne floatPositiveInfinity = floatPositiveZero ∧
    floatApply quietNaNPolicy .add floatPositiveInfinity floatOne = floatPositiveInfinity ∧
    floatApply quietNaNPolicy .subtract floatOne floatPositiveInfinity = floatNegativeInfinity := by
  decide

theorem float_nan_witnesses (policy : NaNPolicy) :
    floatIsNaN (floatApply policy .divide floatPositiveZero floatPositiveZero) ∧
    floatIsNaN (floatApply policy .subtract floatPositiveInfinity floatPositiveInfinity) ∧
    floatIsNaN (floatApply policy .multiply floatPositiveInfinity floatPositiveZero) ∧
    floatIsNaN (floatApply policy .divide floatPositiveInfinity floatNegativeInfinity) ∧
    floatIsNaN (floatApply policy .add floatQuietNaN floatOne) := by
  refine ⟨policy.chooses_nan _ _ _, policy.chooses_nan _ _ _, policy.chooses_nan _ _ _,
    policy.chooses_nan _ _ _, policy.chooses_nan _ _ _⟩

theorem nan_payload_is_unconstrained :
    floatApply quietNaNPolicy .divide floatPositiveZero floatPositiveZero ≠
      floatApply negativeNaNPolicy .divide floatPositiveZero floatPositiveZero ∧
    FloatMeaning .divide floatPositiveZero floatPositiveZero
      (floatApply quietNaNPolicy .divide floatPositiveZero floatPositiveZero) ∧
    FloatMeaning .divide floatPositiveZero floatPositiveZero
      (floatApply negativeNaNPolicy .divide floatPositiveZero floatPositiveZero) :=
  ⟨by decide, float_apply_meaning _ _ _ _, float_apply_meaning _ _ _ _⟩

set_option maxRecDepth 10000 in
theorem float_conversion_witnesses :
    floatRound floatTwoAndHalf = some (intValue 3) ∧
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
    intToFloat intZero = floatPositiveZero := by
  decide

def roundMagnitudeBitsTiesAway (numerator denominator : Nat) : Nat :=
  let scaled := numerator * 2 ^ 1074
  let exponent := min 2045 (Nat.log2 (scaled / denominator) - 52)
  let mantissa := roundAway scaled (denominator * 2 ^ exponent)
  if exponent = 0 ∧ mantissa < floatFractionWidth then mantissa
  else if 2 ^ 53 ≤ mantissa then
    if exponent = 2045 then (floatExponentLimit - 1) * floatFractionWidth
    else (exponent + 2) * floatFractionWidth
  else (exponent + 1) * floatFractionWidth + (mantissa - floatFractionWidth)

def floatOfNatWrapping (raw : Nat) : FloatBits := ⟨raw % signed64Width, Nat.mod_lt _ (by decide)⟩

def tiesAwayApply (operator : FloatBinaryOp) (left right : FloatBits) : FloatBits :=
  match floatOutcome operator (floatClass left) (floatClass right) with
  | .rounded negative numerator denominator =>
    floatOfNatWrapping ((if negative then floatSign else 0) +
      roundMagnitudeBitsTiesAway numerator denominator)
  | outcome => outcome.realize floatQuietNaN

def offByOneClass (bits : FloatBits) : FloatClass :=
  if infinityRaw < floatMagnitudeRaw bits then .nan
  else if floatMagnitudeRaw bits = infinityRaw then .infinite (floatNegative bits)
  else .finite (floatNegative bits) (exponentOffByOne (floatMagnitudeRaw bits))

def offByOneExponentApply (operator : FloatBinaryOp) (left right : FloatBits) : FloatBits :=
  (floatOutcome operator (offByOneClass left) (offByOneClass right)).realize floatQuietNaN

def unsignedZeroApply (operator : FloatBinaryOp) (left right : FloatBits) : FloatBits :=
  match floatOutcome operator (floatClass left) (floatClass right) with
  | .zero _ => floatPositiveZero
  | outcome => outcome.realize floatQuietNaN

def floatRoundTiesEven : FloatBits → Option IntValue := floatToInt fun _ => roundEven

set_option maxRecDepth 10000 in
theorem ties_away_arithmetic_mutant_rejected :
    ¬ ∀ operator left right, FloatMeaning operator left right (tiesAwayApply operator left right) :=
  fun accepted => float_meaning_rejects quietNaNPolicy .add floatOne floatHalfUlpOfOne _
    (by unfold floatIsNaN; decide) (by decide) (accepted .add floatOne floatHalfUlpOfOne)

set_option maxRecDepth 10000 in
theorem off_by_one_exponent_mutant_rejected :
    ¬ ∀ operator left right,
      FloatMeaning operator left right (offByOneExponentApply operator left right) :=
  fun accepted => float_meaning_rejects quietNaNPolicy .add floatOne floatOne _
    (by unfold floatIsNaN; decide) (by decide) (accepted .add floatOne floatOne)

theorem unsigned_zero_mutant_rejected :
    ¬ ∀ operator left right,
      FloatMeaning operator left right (unsignedZeroApply operator left right) :=
  fun accepted => float_meaning_rejects quietNaNPolicy .add floatNegativeZero floatNegativeZero _
    (by unfold floatIsNaN; decide) (by decide)
    (accepted .add floatNegativeZero floatNegativeZero)

theorem non_nan_policy_mutant_rejected :
    ¬ ∀ operator left right,
      FloatMeaning operator left right (floatApplyWith floatPositiveZero operator left right) :=
  fun accepted => float_meaning_rejects quietNaNPolicy .divide floatPositiveZero floatPositiveZero _
    (by unfold floatIsNaN; decide) (by decide)
    (accepted .divide floatPositiveZero floatPositiveZero)

set_option maxRecDepth 10000 in
theorem ties_even_round_mutant_rejected :
    ¬ ∀ bits, ToIntMeaning RoundAwayBound bits (floatRoundTiesEven bits) := by
  intro accepted
  have unique := to_int_meaning_unique RoundAwayBound round_away_bound_functional floatTwoAndHalf
    (accepted floatTwoAndHalf) (float_to_int_meaning _ _ (fun _ _ => round_is_correct _ _) _)
  revert unique
  decide

theorem nonfinite_conversions_rejected (bits : FloatBits)
    (nonfinite : floatIsNaN bits ∨ floatMagnitudeRaw bits = infinityRaw) :
    floatFloor bits = none ∧ floatCeil bits = none ∧ floatRound bits = none := by
  have notFinite : ∀ negative units, floatClass bits ≠ .finite negative units := by
    intro negative units finite
    have below := ((float_class_finite_iff bits negative units).1 finite).1
    rcases nonfinite with isNaN | infinite
    · rw [(float_class_nan_iff bits).2 isNaN] at finite
      cases finite
    · omega
  unfold floatFloor floatCeil floatRound floatToInt
  cases classified : floatClass bits with
  | finite negative units => exact (notFinite negative units classified).elim
  | nan => exact ⟨rfl, rfl, rfl⟩
  | infinite _ => exact ⟨rfl, rfl, rfl⟩

set_option maxRecDepth 10000 in
theorem cancellation_is_ieee_equal_to_negative_zero :
    ieeeFloatEq (floatApply quietNaNPolicy .subtract floatOne floatOne) floatNegativeZero := by
  have cancelled : floatApply quietNaNPolicy .subtract floatOne floatOne = floatPositiveZero := by
    decide
  rw [cancelled]
  exact ieee_signed_zeros_equal

end Xsh.Values.FloatOps
