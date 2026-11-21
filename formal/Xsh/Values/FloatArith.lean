import Xsh.Values.Scalars

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

def floorMagnitude (numerator denominator : Nat) : Nat := numerator / denominator

def ceilMagnitude (numerator denominator : Nat) : Nat :=
  (numerator + denominator - 1) / denominator

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

theorem round_even_ge_div (n d : Nat) : n / d ≤ roundEven n d := by
  unfold roundEven
  dsimp only
  split <;> omega

theorem round_even_le_div_succ (n d : Nat) : roundEven n d ≤ n / d + 1 := by
  unfold roundEven
  dsimp only
  split <;> omega

theorem magnitude_units_subnormal (raw : Nat) (small : raw < floatFractionWidth) :
    magnitudeUnits raw = raw := by
  simp [magnitudeUnits, Nat.div_eq_of_lt small, Nat.mod_eq_of_lt small]

theorem magnitude_units_upper (raw : Nat) :
    magnitudeUnits raw < floatFractionWidth * 2 ^ (raw / floatFractionWidth) := by
  have fractionBound := Nat.mod_lt raw (by decide : 0 < floatFractionWidth)
  unfold magnitudeUnits
  dsimp only
  generalize raw / floatFractionWidth = exponent
  generalize raw % floatFractionWidth = fraction at *
  cases exponent with
  | zero => simpa using fractionBound
  | succ exponent =>
    simp only [Nat.add_one_ne_zero, ite_false, Nat.add_sub_cancel]
    have scale := Nat.two_pow_pos exponent
    have strict := Nat.mul_lt_mul_of_pos_right
      (show floatFractionWidth + fraction < 2 * floatFractionWidth by omega) scale
    rw [Nat.pow_succ]
    have regroup : 2 * floatFractionWidth * 2 ^ exponent =
        floatFractionWidth * (2 ^ exponent * 2) := by
      rw [Nat.mul_comm 2 floatFractionWidth, Nat.mul_assoc, Nat.mul_comm 2]
    omega

theorem magnitude_units_lower (raw : Nat) (normal : 0 < raw / floatFractionWidth) :
    floatFractionWidth * 2 ^ (raw / floatFractionWidth - 1) ≤ magnitudeUnits raw := by
  unfold magnitudeUnits
  dsimp only
  rw [ite_eq_right (by omega)]
  exact Nat.mul_le_mul_right _ (Nat.le_add_right _ _)

theorem magnitude_units_low_or_aligned (raw exponent : Nat) :
    magnitudeUnits raw < floatFractionWidth * 2 ^ exponent ∨ 2 ^ exponent ∣ magnitudeUnits raw := by
  by_cases low : raw / floatFractionWidth ≤ exponent
  · exact Or.inl (Nat.lt_of_lt_of_le (magnitude_units_upper raw)
      (Nat.mul_le_mul_left _ (Nat.pow_le_pow_right (by decide) low)))
  · refine Or.inr ?_
    unfold magnitudeUnits
    dsimp only
    rw [ite_eq_right (by omega)]
    exact Nat.dvd_trans (Nat.pow_dvd_pow 2 (by omega)) (Nat.dvd_mul_left _ _)

theorem magnitude_units_strict (left right : Nat)
    (lower : left / floatFractionWidth < right / floatFractionWidth) :
    magnitudeUnits left < magnitudeUnits right := by
  have upper := magnitude_units_upper left
  have floor := magnitude_units_lower right (Nat.lt_of_le_of_lt (Nat.zero_le _) lower)
  have scale := Nat.mul_le_mul_left floatFractionWidth
    (Nat.pow_le_pow_right (n := 2) (by decide)
      (show left / floatFractionWidth ≤ right / floatFractionWidth - 1 by omega))
  omega

theorem magnitude_units_injective {left right : Nat}
    (same : magnitudeUnits left = magnitudeUnits right) : left = right := by
  rcases Nat.lt_trichotomy (left / floatFractionWidth) (right / floatFractionWidth) with
    lower | equal | higher
  · exact absurd same (Nat.ne_of_lt (magnitude_units_strict _ _ lower))
  · have leftParts := Nat.div_add_mod left floatFractionWidth
    have rightParts := Nat.div_add_mod right floatFractionWidth
    have fractions : left % floatFractionWidth = right % floatFractionWidth := by
      unfold magnitudeUnits at same
      dsimp only at same
      rw [equal] at same
      split at same
      · exact same
      · have cancelled := Nat.mul_right_cancel (Nat.two_pow_pos _) same
        omega
    rw [equal, fractions] at leftParts
    omega
  · exact absurd same (Nat.ne_of_gt (magnitude_units_strict _ _ higher))

theorem binary_nearest_of_grid (n d exponent mantissa raw : Nat) (positive : 0 < d)
    (grid : NearestEven (n * 2 ^ 1074) (d * 2 ^ exponent) mantissa)
    (decodes : magnitudeUnits raw = mantissa * 2 ^ exponent)
    (finite : raw < (floatExponentLimit - 1) * floatFractionWidth)
    (parity : mantissa % 2 = 0 → raw % 2 = 0)
    (binade : 0 < exponent → d * (floatFractionWidth * 2 ^ exponent) ≤ n * 2 ^ 1074) :
    BinaryNearest n d raw := by
  have onGrid : ∀ multiple, d * (multiple * 2 ^ exponent) = multiple * (d * 2 ^ exponent) :=
    fun multiple => Nat.mul_left_comm d multiple _
  have belowBinade : ∀ candidate, ¬ 2 ^ exponent ∣ magnitudeUnits candidate →
      distance (n * 2 ^ 1074) (floatFractionWidth * (d * 2 ^ exponent)) <
        distance (n * 2 ^ 1074) (d * magnitudeUnits candidate) := by
    intro candidate unaligned
    have low := (magnitude_units_low_or_aligned candidate exponent).resolve_right unaligned
    have coarse : 0 < exponent := by
      apply Nat.pos_of_ne_zero
      intro zero
      subst zero
      exact unaligned (by simp)
    have floor := binade coarse
    have scaled := Nat.mul_lt_mul_of_pos_left low positive
    rw [onGrid] at floor scaled
    unfold distance
    omega
  refine ⟨finite, ?_, ?_⟩
  · intro candidate _
    rw [decodes, onGrid]
    by_cases aligned : 2 ^ exponent ∣ magnitudeUnits candidate
    · obtain ⟨multiple, aligned⟩ := aligned
      rw [aligned, Nat.mul_comm (2 ^ exponent), onGrid]
      exact grid.1 multiple
    · exact Nat.le_of_lt (Nat.lt_of_le_of_lt (grid.1 _) (belowBinade candidate aligned))
  · intro candidate _ tie
    rw [decodes, onGrid] at tie
    by_cases aligned : 2 ^ exponent ∣ magnitudeUnits candidate
    · obtain ⟨multiple, alignedEq⟩ := aligned
      rw [alignedEq, Nat.mul_comm (2 ^ exponent), onGrid] at tie
      rcases grid.2 multiple tie with even | same
      · exact Or.inl (parity even)
      · refine Or.inr (magnitude_units_injective ?_)
        rw [alignedEq, decodes, same, Nat.mul_comm]
    · have := Nat.lt_of_le_of_lt (grid.1 _) (belowBinade candidate aligned)
      omega

def overflowMidpointUnits : Nat := (2 ^ 54 - 1) * 2 ^ 2044

theorem round_even_stays_below_carry (scaled half : Nat)
    (below : scaled < half * (2 ^ 54 - 1)) : roundEven scaled (2 * half) < 2 ^ 53 := by
  have positive : 0 < 2 * half := by omega
  have remainder := Nat.mod_lt scaled positive
  have parts := Nat.div_add_mod scaled (2 * half)
  unfold roundEven
  dsimp only
  generalize scaled / (2 * half) = quotient at *
  generalize scaled % (2 * half) = rest at *
  rw [Nat.mul_assoc] at parts
  by_cases high : 2 ^ 53 ≤ quotient
  · have := Nat.mul_le_mul_left half high
    omega
  · by_cases top : quotient = 2 ^ 53 - 1
    · subst top
      split <;> omega
    · split <;> omega

theorem round_even_reaches_carry (scaled half : Nat) (positive : 0 < half)
    (above : half * (2 ^ 54 - 1) ≤ scaled) : 2 ^ 53 ≤ roundEven scaled (2 * half) := by
  have remainder := Nat.mod_lt scaled (show 0 < 2 * half by omega)
  have parts := Nat.div_add_mod scaled (2 * half)
  unfold roundEven
  dsimp only
  generalize scaled / (2 * half) = quotient at *
  generalize scaled % (2 * half) = rest at *
  rw [Nat.mul_assoc] at parts
  by_cases low : quotient + 2 ≤ 2 ^ 53
  · have := Nat.mul_le_mul_left half low
    rw [Nat.mul_add] at this
    omega
  · by_cases top : quotient = 2 ^ 53 - 1
    · subst top
      split <;> omega
    · split <;> omega

theorem top_binade_half (d : Nat) : d * 2 ^ 2045 = 2 * (d * 2 ^ 2044) := by
  rw [show (2 : Nat) ^ 2045 = 2 ^ 2044 * 2 from Nat.pow_succ 2 2044]
  rw [Nat.mul_comm (2 ^ 2044) 2, Nat.mul_left_comm]

theorem midpoint_half (d : Nat) :
    d * overflowMidpointUnits = d * 2 ^ 2044 * (2 ^ 54 - 1) := by
  unfold overflowMidpointUnits
  rw [Nat.mul_comm (2 ^ 54 - 1), Nat.mul_assoc]

theorem round_magnitude_bits_nearest (n d : Nat) (positive : 0 < d)
    (below : n * 2 ^ 1074 < d * overflowMidpointUnits) :
    BinaryNearest n d (roundMagnitudeBits n d) := by
  unfold roundMagnitudeBits
  dsimp only
  generalize exponentDef : min 2045 (Nat.log2 (n * 2 ^ 1074 / d) - 52) = exponent
  have grid := roundEven_nearest (n * 2 ^ 1074) (d * 2 ^ exponent)
    (Nat.mul_pos positive (Nat.two_pow_pos exponent))
  have mantissaFloor := round_even_ge_div (n * 2 ^ 1074) (d * 2 ^ exponent)
  have mantissaCeiling := round_even_le_div_succ (n * 2 ^ 1074) (d * 2 ^ exponent)
  have toppedCarry : exponent = 2045 → roundEven (n * 2 ^ 1074) (d * 2 ^ exponent) < 2 ^ 53 := by
    intro top
    subst top
    rw [top_binade_half]
    apply round_even_stays_below_carry
    rw [← midpoint_half]
    exact below
  generalize roundEven (n * 2 ^ 1074) (d * 2 ^ exponent) = mantissa at *
  generalize valueDef : n * 2 ^ 1074 / d = value at exponentDef
  have capped : exponent ≤ 2045 := by omega
  have binade : 0 < exponent → d * (floatFractionWidth * 2 ^ exponent) ≤ n * 2 ^ 1074 := by
    intro coarse
    have nonzero : value ≠ 0 := by
      intro zero
      subst zero
      simp [Nat.log2_zero] at exponentDef
      omega
    have floor := (Nat.le_log2 nonzero).1 (show 52 + exponent ≤ value.log2 by omega)
    have scaled := Nat.div_mul_le_self (n * 2 ^ 1074) d
    rw [valueDef] at scaled
    have := Nat.mul_le_mul_right d floor
    rw [Nat.pow_add] at this
    unfold floatFractionWidth
    rw [Nat.mul_comm d]
    omega
  have normalMantissa : 0 < exponent → floatFractionWidth ≤ mantissa := by
    intro coarse
    have floor := binade coarse
    rw [← Nat.mul_left_comm floatFractionWidth] at floor
    have := (Nat.le_div_iff_mul_le (Nat.mul_pos positive (Nat.two_pow_pos exponent))).2 floor
    omega
  have uncappedCarry : exponent < 2045 → mantissa ≤ 2 ^ 53 := by
    intro uncapped
    have nonzeroCase : value = 0 ∨ value < 2 ^ (53 + exponent) := by
      by_cases zero : value = 0
      · exact Or.inl zero
      · refine Or.inr ((Nat.log2_lt zero).1 ?_)
        omega
    have ceiling : value < 2 ^ 53 * 2 ^ exponent := by
      rw [← Nat.pow_add, Nat.add_comm 53]
      rcases nonzeroCase with zero | bound
      · rw [zero]; exact Nat.two_pow_pos _
      · rwa [Nat.add_comm]
    rw [← valueDef, ← Nat.div_lt_iff_lt_mul (Nat.two_pow_pos exponent),
      Nat.div_div_eq_div_mul] at ceiling
    omega
  have evenWidth : 2 ∣ floatFractionWidth := by decide
  by_cases subnormal : exponent = 0 ∧ mantissa < floatFractionWidth
  · rw [ite_eq_left subnormal]
    obtain ⟨zero, small⟩ := subnormal
    subst zero
    refine binary_nearest_of_grid n d 0 mantissa mantissa positive grid ?_ ?_ id (by omega)
    · rw [magnitude_units_subnormal mantissa small, Nat.pow_zero, Nat.mul_one]
    · unfold floatExponentLimit floatFractionWidth at *
      omega
  · rw [ite_eq_right subnormal]
    by_cases carry : 2 ^ 53 ≤ mantissa
    · rw [ite_eq_left carry]
      have uncapped : exponent ≠ 2045 := fun top => by
        have := toppedCarry top
        omega
      rw [ite_eq_right uncapped]
      have exact : mantissa = 2 ^ 53 := by
        have := uncappedCarry (by omega)
        omega
      subst exact
      refine binary_nearest_of_grid n d exponent (2 ^ 53) _ positive grid ?_ ?_ ?_ binade
      · have decoded := magnitude_units_normal (exponent + 1) floatFractionWidth
          (Nat.le_refl _) (by decide)
        rw [Nat.sub_self, Nat.add_zero] at decoded
        rw [decoded, ← Nat.pow_add]
        unfold floatFractionWidth
        rw [← Nat.pow_add]
        congr 1
        omega
      · have := Nat.mul_le_mul_right floatFractionWidth (show exponent + 2 ≤ 2046 by omega)
        have : 0 < floatFractionWidth := by decide
        unfold floatExponentLimit
        omega
      · intro _
        exact Nat.mod_eq_zero_of_dvd (Nat.dvd_trans evenWidth (Nat.dvd_mul_left _ _))
    · rw [ite_eq_right carry]
      have normal : floatFractionWidth ≤ mantissa := by
        by_cases zero : exponent = 0
        · unfold floatFractionWidth at *
          omega
        · exact normalMantissa (by omega)
      have narrow : mantissa < 2 * floatFractionWidth := by
        unfold floatFractionWidth at *
        omega
      refine binary_nearest_of_grid n d exponent mantissa _ positive grid
        (magnitude_units_normal exponent mantissa normal narrow) ?_ ?_ binade
      · have := Nat.mul_le_mul_right floatFractionWidth (show exponent + 1 ≤ 2046 by omega)
        have : 0 < floatFractionWidth := by decide
        unfold floatExponentLimit
        omega
      · intro even
        have := Nat.dvd_trans evenWidth (Nat.dvd_mul_left floatFractionWidth (exponent + 1))
        omega

theorem round_magnitude_bits_overflow (n d : Nat) (positive : 0 < d)
    (above : d * overflowMidpointUnits ≤ n * 2 ^ 1074) :
    roundMagnitudeBits n d = (floatExponentLimit - 1) * floatFractionWidth := by
  have large : 2 ^ 2097 ≤ n * 2 ^ 1074 / d := by
    apply (Nat.le_div_iff_mul_le positive).2
    have midpoint : 2 ^ 2097 ≤ overflowMidpointUnits := by
      unfold overflowMidpointUnits
      rw [show (2097 : Nat) = 53 + 2044 from rfl, Nat.pow_add]
      exact Nat.mul_le_mul_right _ (by decide)
    have scaled := Nat.mul_le_mul_right d midpoint
    rw [Nat.mul_comm overflowMidpointUnits] at scaled
    exact Nat.le_trans scaled above
  have nonzero : n * 2 ^ 1074 / d ≠ 0 := by
    have := Nat.two_pow_pos 2097
    omega
  have logarithm := (Nat.le_log2 nonzero).2 large
  have top : min 2045 (Nat.log2 (n * 2 ^ 1074 / d) - 52) = 2045 := by omega
  have carry := round_even_reaches_carry (n * 2 ^ 1074) (d * 2 ^ 2044)
    (Nat.mul_pos positive (Nat.two_pow_pos _)) (by rw [← midpoint_half]; exact above)
  rw [← top_binade_half] at carry
  unfold roundMagnitudeBits
  dsimp only
  rw [top, ite_eq_right (fun subnormal => absurd subnormal.1 (by decide)), ite_eq_left carry,
    ite_eq_left rfl]

theorem magnitude_units_strict_mono {left right : Nat} (lower : left < right) :
    magnitudeUnits left < magnitudeUnits right := by
  rcases Nat.lt_trichotomy (left / floatFractionWidth) (right / floatFractionWidth) with
    smaller | equal | larger
  · exact magnitude_units_strict _ _ smaller
  · have leftParts := Nat.div_add_mod left floatFractionWidth
    have rightParts := Nat.div_add_mod right floatFractionWidth
    rw [equal] at leftParts
    have fractions : left % floatFractionWidth < right % floatFractionWidth := by omega
    unfold magnitudeUnits
    dsimp only
    rw [equal]
    split
    · exact fractions
    · exact Nat.mul_lt_mul_of_pos_right (by omega) (Nat.two_pow_pos _)
  · have := magnitude_units_strict _ _ larger
    have := Nat.div_le_div_right (c := floatFractionWidth) (Nat.le_of_lt lower)
    omega

theorem binary_nearest_unique (n d : Nat) (positive : 0 < d) {first second : Nat}
    (firstNearest : BinaryNearest n d first) (secondNearest : BinaryNearest n d second) :
    first = second := by
  have ordered : ∀ low high, low < high → BinaryNearest n d low → BinaryNearest n d high → False := by
    intro low high lower lowNearest highNearest
    have lowBound := lowNearest.2.1 high highNearest.1
    have highBound := highNearest.2.1 low lowNearest.1
    have tie : distance (n * 2 ^ 1074) (d * magnitudeUnits low) =
        distance (n * 2 ^ 1074) (d * magnitudeUnits high) := Nat.le_antisymm lowBound highBound
    have lowEven := (lowNearest.2.2 high highNearest.1 tie).resolve_right (by omega)
    have highEven := (highNearest.2.2 low lowNearest.1 tie.symm).resolve_right (by omega)
    have middleFinite : low + 1 < (floatExponentLimit - 1) * floatFractionWidth := by
      have := highNearest.1
      omega
    have middleBound := lowNearest.2.1 (low + 1) middleFinite
    have belowMiddle := Nat.mul_lt_mul_of_pos_left
      (magnitude_units_strict_mono (show low < low + 1 by omega)) positive
    have aboveMiddle := Nat.mul_lt_mul_of_pos_left
      (magnitude_units_strict_mono (show low + 1 < high by omega)) positive
    unfold distance at tie middleBound
    omega
  rcases Nat.lt_trichotomy first second with lower | same | higher
  · exact (ordered _ _ lower firstNearest secondNearest).elim
  · exact same
  · exact (ordered _ _ higher secondNearest firstNearest).elim

end Xsh.Values.FloatArith
