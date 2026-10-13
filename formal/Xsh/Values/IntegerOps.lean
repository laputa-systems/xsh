import Xsh.Values.Scalars

namespace Xsh.Values.IntegerOps

open Xsh.Values

inductive IntegerFault where
  | overflow
  | divisionByZero
  deriving DecidableEq, Repr

inductive IntegerBinaryOp where
  | add
  | subtract
  | multiply
  | divide
  | remainder
  deriving DecidableEq, Repr

inductive IntegerResult where
  | value (value : IntValue)
  | fault (fault : IntegerFault)
  deriving DecidableEq, Repr

def checkedInt (value : Int) : IntegerResult :=
  match validateInt? value with
  | some bounded => .value bounded
  | none => .fault .overflow

/-- A checked conversion returns the exact input when it is representable and
reports overflow precisely when it lies outside the signed 64-bit interval. -/
theorem checkedInt_total (value : Int) :
    (∃ bounded, checkedInt value = .value bounded ∧ bounded.val = value) ∨
      (checkedInt value = .fault .overflow ∧
        (value < signed64Min ∨ signed64Limit ≤ value)) := by
  by_cases inRange : signed64Min ≤ value ∧ value < signed64Limit
  · left
    refine ⟨⟨value, inRange⟩, ?_, rfl⟩
    simp [checkedInt, validateInt?, inRange]
  · right
    constructor
    · simp [checkedInt, validateInt?, inRange]
    · omega

def negate (value : IntValue) : IntegerResult :=
  checkedInt (-value.val)

/-- Both division and remainder report overflow for the signed minimum divided
by negative one, matching the checked operation's representable quotient. -/
def applyBinary (operator : IntegerBinaryOp) (left right : IntValue) :
    IntegerResult :=
  match operator with
  | .add => checkedInt (left.val + right.val)
  | .subtract => checkedInt (left.val - right.val)
  | .multiply => checkedInt (left.val * right.val)
  | .divide =>
    if right.val = 0 then .fault .divisionByZero
    else checkedInt (left.val.tdiv right.val)
  | .remainder =>
    if right.val = 0 then .fault .divisionByZero
    else if left.val = signed64Min ∧ right.val = -1 then .fault .overflow
    else checkedInt (left.val.tmod right.val)

/-- Addition returns its exact mathematical sum when it is representable and
otherwise reports overflow. -/
theorem addition_total (left right : IntValue) :
    (∃ result, applyBinary .add left right = .value result ∧
      result.val = left.val + right.val) ∨
      (applyBinary .add left right = .fault .overflow ∧
        (left.val + right.val < signed64Min ∨
          signed64Limit ≤ left.val + right.val)) := by
  simpa [applyBinary] using checkedInt_total (left.val + right.val)

/-- Subtraction returns its exact mathematical difference when it is
representable and otherwise reports overflow. -/
theorem subtraction_total (left right : IntValue) :
    (∃ result, applyBinary .subtract left right = .value result ∧
      result.val = left.val - right.val) ∨
      (applyBinary .subtract left right = .fault .overflow ∧
        (left.val - right.val < signed64Min ∨
          signed64Limit ≤ left.val - right.val)) := by
  simpa [applyBinary] using checkedInt_total (left.val - right.val)

/-- Multiplication returns its exact mathematical product when it is
representable and otherwise reports overflow. -/
theorem multiplication_total (left right : IntValue) :
    (∃ result, applyBinary .multiply left right = .value result ∧
      result.val = left.val * right.val) ∨
      (applyBinary .multiply left right = .fault .overflow ∧
        (left.val * right.val < signed64Min ∨
          signed64Limit ≤ left.val * right.val)) := by
  simpa [applyBinary] using checkedInt_total (left.val * right.val)

/-- Division distinguishes division by zero from a checked truncating
quotient. A nonzero divisor returns that exact quotient when representable
and reports overflow precisely when the quotient is outside the signed range. -/
theorem division_total (left right : IntValue) :
    (right.val = 0 ∧
      applyBinary .divide left right = .fault .divisionByZero) ∨
      (right.val ≠ 0 ∧
        ((∃ result, applyBinary .divide left right = .value result ∧
            result.val = left.val.tdiv right.val) ∨
          (applyBinary .divide left right = .fault .overflow ∧
            (left.val.tdiv right.val < signed64Min ∨
              signed64Limit ≤ left.val.tdiv right.val)))) := by
  by_cases isZero : right.val = 0
  · left
    exact ⟨isZero, by simp [applyBinary, isZero]⟩
  · right
    constructor
    · exact isZero
    · simpa [applyBinary, isZero] using
        checkedInt_total (left.val.tdiv right.val)

/-- Remainder reports the quotient overflow at signed minimum divided by -1,
then otherwise returns the truncating-division remainder when representable.
This keeps remainder aligned with the checked signed division instruction. -/
theorem remainder_total (left right : IntValue) :
    (right.val = 0 ∧
      applyBinary .remainder left right = .fault .divisionByZero) ∨
      (left.val = signed64Min ∧ right.val = -1 ∧
        applyBinary .remainder left right = .fault .overflow) ∨
      (right.val ≠ 0 ∧ ¬ (left.val = signed64Min ∧ right.val = -1) ∧
        ((∃ result, applyBinary .remainder left right = .value result ∧
            result.val = left.val.tmod right.val) ∨
          (applyBinary .remainder left right = .fault .overflow ∧
            (left.val.tmod right.val < signed64Min ∨
              signed64Limit ≤ left.val.tmod right.val)))) := by
  by_cases isZero : right.val = 0
  · left
    exact ⟨isZero, by simp [applyBinary, isZero]⟩
  · by_cases isSignedDivisionOverflow :
      left.val = signed64Min ∧ right.val = -1
    · right
      left
      exact ⟨isSignedDivisionOverflow.1, isSignedDivisionOverflow.2,
        by simp [applyBinary, isZero, isSignedDivisionOverflow]⟩
    · right
      right
      refine ⟨isZero, isSignedDivisionOverflow, ?_⟩
      simpa [applyBinary, isZero, isSignedDivisionOverflow] using
        checkedInt_total (left.val.tmod right.val)

theorem truncating_division_negative_numerator :
    (⟨-7, by unfold signed64Min signed64Limit; omega⟩ : IntValue).val.tdiv
      (⟨3, by unfold signed64Min signed64Limit; omega⟩ : IntValue).val = -2 := by
  decide

theorem truncating_division_negative_denominator :
    (⟨7, by unfold signed64Min signed64Limit; omega⟩ : IntValue).val.tdiv
      (⟨-3, by unfold signed64Min signed64Limit; omega⟩ : IntValue).val = -2 := by
  decide

theorem truncating_remainder_keeps_numerator_sign :
    (⟨-7, by unfold signed64Min signed64Limit; omega⟩ : IntValue).val.tmod
      (⟨3, by unfold signed64Min signed64Limit; omega⟩ : IntValue).val = -1 := by
  decide

theorem multiply_overflow_is_checked :
    applyBinary .multiply intMax intMax = .fault .overflow := by
  decide

theorem divide_by_zero_is_explicit :
    applyBinary .divide intOne intZero = .fault .divisionByZero := by
  decide

theorem remainder_by_zero_is_explicit :
    applyBinary .remainder intOne intZero = .fault .divisionByZero := by
  decide

theorem min_div_negative_one_overflows :
    applyBinary .divide intMin intNegativeOne = .fault .overflow := by
  decide

theorem min_remainder_negative_one_overflows :
    applyBinary .remainder intMin intNegativeOne = .fault .overflow := by
  decide

theorem negative_division_truncates_toward_zero :
    applyBinary .divide
      (⟨-7, by unfold signed64Min signed64Limit; omega⟩ : IntValue)
      (⟨3, by unfold signed64Min signed64Limit; omega⟩ : IntValue) =
      .value (⟨-2, by unfold signed64Min signed64Limit; omega⟩ : IntValue) := by
  decide

def wrappingInt (value : Int) : Int := (BitVec.ofInt 64 value).toInt

/-- A wrapping-add mutation maps max-plus-one to min, while checked addition
returns the explicit overflow fault. -/
theorem checked_add_rejects_wraparound :
    applyBinary .add intMax intOne = .fault .overflow ∧
      wrappingInt (intMax.val + intOne.val) = signed64Min := by
  decide

/-- Replacing truncating remainder with Euclidean remainder changes the sign
for a negative numerator. -/
theorem euclidean_remainder_mutation_changes_sign :
    (-7 : Int) % 3 = 2 ∧ (-7 : Int).tmod 3 = -1 := by
  decide

end Xsh.Values.IntegerOps
