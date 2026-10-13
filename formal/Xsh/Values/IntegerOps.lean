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

end Xsh.Values.IntegerOps
