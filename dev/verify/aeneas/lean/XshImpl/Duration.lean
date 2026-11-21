import XshImpl.XshArithSlice
import Xsh.Values.DurationOps

open Aeneas Aeneas.Std Result
open xsh_arith_slice

namespace XshImpl.Duration

open Xsh.Values (IntValue DurationValue signed64Width signed64Min signed64Limit)
open Xsh.Values.DurationOps (DurationFault)

abbrev Outcome := Xsh.Values.DurationOps.DurationResult

inductive Operand where
  | millis (value : DurationValue)
  | count (value : IntValue)

def millisOf (value : Std.U64) : DurationValue :=
  ⟨value.val, by have := value.hmax; simpa [signed64Width] using this⟩

def countOf (value : Std.I64) : IntValue :=
  ⟨value.val, by
    have lower := value.hmin
    have upper := value.hmax
    simp [signed64Min, signed64Limit] at *
    omega⟩

def operand : duration.DurationOperand → Operand
  | .Millis value => .millis (millisOf value)
  | .Count value => .count (countOf value)

def expected : syntax.node.BinaryOp → Operand → Operand → Option Outcome
  | .Add, .millis a, .millis b => some (Xsh.Values.DurationOps.add a b)
  | .Sub, .millis a, .millis b => some (Xsh.Values.DurationOps.subtract a b)
  | .Mul, .millis a, .count b => some (Xsh.Values.DurationOps.multiply a b)
  | .Mul, .count b, .millis a => some (Xsh.Values.DurationOps.multiply a b)
  | .Div, .millis a, .count b => some (Xsh.Values.DurationOps.divideByInt a b)
  | .Div, .millis a, .millis b => some (Xsh.Values.DurationOps.ratio a b)
  | _, _, _ => none

def faultOf : duration.DurationArithmeticError → Option DurationFault
  | .InvalidDimensions => none
  | .Overflow => some .overflow
  | .Underflow => some .underflow
  | .NegativeFactor => some .negativeFactor
  | .NonpositiveDivisor => some .nonpositiveDivisor
  | .CountOverflow => some .intervalOverflow

def decode :
    core.result.Result duration.DurationResult duration.DurationArithmeticError → Option Outcome
  | .Ok (.Millis value) => some (.millis (millisOf value))
  | .Ok (.Count value) => some (.count (countOf value))
  | .Err error => (faultOf error).map .fault

abbrev Implementation :=
  syntax.node.BinaryOp → duration.DurationOperand → duration.DurationOperand →
    Result (core.result.Result duration.DurationResult duration.DurationArithmeticError)

def Refines (implementation : Implementation) : Prop :=
  ∀ op left right, ∃ out, implementation op left right = ok out ∧
    decode out = expected op (operand left) (operand right)

theorem millisOf_injective : Function.Injective millisOf := fun _ _ h =>
  UScalar.eq_of_val_eq (congrArg Subtype.val h)

theorem countOf_injective : Function.Injective countOf := fun _ _ h =>
  IScalar.eq_of_val_eq (congrArg Subtype.val h)

theorem decode_injective : Function.Injective decode := by
  intro a b h
  rcases a with (a | a) | a <;> rcases b with (b | b) | b
  · simp only [decode, Option.some.injEq, Xsh.Values.DurationOps.DurationResult.millis.injEq] at h
    rw [millisOf_injective h]
  · simp [decode] at h
  · cases b <;> simp [decode, faultOf] at h
  · simp [decode] at h
  · simp only [decode, Option.some.injEq, Xsh.Values.DurationOps.DurationResult.count.injEq] at h
    rw [countOf_injective h]
  · cases b <;> simp [decode, faultOf] at h
  · cases a <;> simp [decode, faultOf] at h
  · cases a <;> simp [decode, faultOf] at h
  · cases a <;> cases b <;> simp [decode, faultOf] at h ⊢

theorem factor_val (factor : Std.I64) (nonneg : 0 ≤ factor.val) :
    (IScalar.hcast .U64 factor).val = factor.val.toNat := by
  rw [IScalar.hcast_val_eq]
  have upper := factor.hmax
  simp at upper ⊢
  rw [Int.emod_eq_of_lt nonneg (by omega)]

theorem count_val (count : Std.U64) (fits : count.val ≤ 9223372036854775807) :
    (UScalar.hcast .I64 count).val = count.val := by
  have := UScalar.hcast_inBounds_spec .I64 count (by simp [I64.max_eq]; omega)
  simpa [lift, WP.spec_ok] using this

section Plumbing

attribute [local simp] lift core.option.Option.ok_or core.result.Result.Insts.CoreOpsTry.branch
  core.result.Result.Insts.CoreOpsTry_traitFromResidualResult.from_residual
  core.convert.FromSame core.convert.FromSame.from core.result.Result.map_err
  duration.checked_duration_binary.closure.Insts.CoreOpsFunctionFnOnceTupleTryFromIntErrorDurationArithmeticError.call_once
  duration.checked_duration_binary.closure_1.Insts.CoreOpsFunctionFnOnceTupleTryFromIntErrorDurationArithmeticError.call_once
  I64.Insts.CoreConvertTryFromU64TryFromIntError.try_from
  U64.Insts.CoreConvertTryFromI64TryFromIntError.try_from
  decode faultOf expected operand millisOf countOf
  Xsh.Values.DurationOps.add Xsh.Values.DurationOps.subtract Xsh.Values.DurationOps.multiply
  Xsh.Values.DurationOps.divideByInt Xsh.Values.DurationOps.ratio
  Xsh.Values.DurationOps.checkedMillis Xsh.Values.validateDuration? Xsh.Values.validateInt?
  signed64Width signed64Min signed64Limit U64.max_eq I64.max_eq I64.min_eq

theorem limit_toNat : Xsh.Values.signed64Limit.toNat = 9223372036854775808 := by decide

theorem ratio_case (a b : Std.U64) :
    ∃ out, duration.checked_duration_binary .Div (.Millis a) (.Millis b) = ok out ∧
      decode out = expected .Div (operand (.Millis a)) (operand (.Millis b)) := by
  by_cases zero : b.val = 0
  · refine ⟨.Err .NonpositiveDivisor, by simp [duration.checked_duration_binary, zero], ?_⟩
    show (some (.fault .nonpositiveDivisor) : Option Outcome) =
      some (Xsh.Values.DurationOps.ratio (millisOf a) (millisOf b))
    rw [Xsh.Values.DurationOps.ratio_rejects_zero_denominator _ _ zero]
  · obtain ⟨q, divided, quotient⟩ := UScalar.div_spec a zero
    by_cases fits : q.val ≤ 9223372036854775807
    · refine ⟨.Ok (.Count (UScalar.hcast .I64 q)), by
        simp [duration.checked_duration_binary, zero, divided, Nat.not_lt.mpr fits], ?_⟩
      show (some (.count (countOf (UScalar.hcast .I64 q))) : Option Outcome) =
        some (Xsh.Values.DurationOps.ratio (millisOf a) (millisOf b))
      have validated : Xsh.Values.validateInt? (Int.ofNat ((millisOf a).val / (millisOf b).val)) =
          some (countOf (UScalar.hcast .I64 q)) := by
        unfold Xsh.Values.validateInt?
        split
        · exact congrArg some (Subtype.ext (by
            simp only [countOf, millisOf, Int.ofNat_eq_natCast]; rw [count_val q fits]; omega))
        · rename_i outside
          exact absurd (by simp only [millisOf, signed64Min, signed64Limit, Int.ofNat_eq_natCast]; omega)
            outside
      rw [(Xsh.Values.DurationOps.ratio_returns_validated_count _ _ _).mpr ⟨zero, validated⟩]
    · refine ⟨.Err .CountOverflow, by
        simp [duration.checked_duration_binary, zero, divided, Nat.lt_of_not_ge fits], ?_⟩
      show (some (.fault .intervalOverflow) : Option Outcome) =
        some (Xsh.Values.DurationOps.ratio (millisOf a) (millisOf b))
      have large : Xsh.Values.signed64Limit.toNat ≤ (millisOf a).val / (millisOf b).val := by
        rw [limit_toNat]; simp only [millisOf]; omega
      rw [Xsh.Values.DurationOps.ratio_rejects_interval_count_overflow _ _ zero large]

theorem quotient_case (a : Std.U64) (b : Std.I64) :
    ∃ out, duration.checked_duration_binary .Div (.Millis a) (.Count b) = ok out ∧
      decode out = expected .Div (operand (.Millis a)) (operand (.Count b)) := by
  by_cases positive : 0 < b.val
  · have nonzero : (IScalar.hcast .U64 b).val ≠ 0 := by
      rw [factor_val b (Int.le_of_lt positive)]; omega
    obtain ⟨q, divided, quotient⟩ := UScalar.div_spec a nonzero
    rw [factor_val b (Int.le_of_lt positive)] at quotient
    refine ⟨.Ok (.Millis q), by
      simp [duration.checked_duration_binary, Int.not_le.mpr positive, divided], ?_⟩
    have fits : a.val / b.val.toNat < 18446744073709551616 := by
      have := a.hmax; simp at this
      exact Nat.lt_of_le_of_lt (Nat.div_le_self _ _) this
    simp [Int.not_le.mpr positive, fits, quotient]
  · refine ⟨.Err .NonpositiveDivisor, by
      simp [duration.checked_duration_binary, Int.le_of_not_gt positive], ?_⟩
    simp [Int.le_of_not_gt positive]

theorem checked_duration_binary_refines (op : syntax.node.BinaryOp)
    (left right : duration.DurationOperand) :
    ∃ out, duration.checked_duration_binary op left right = ok out ∧
      decode out = expected op (operand left) (operand right) := by
  cases op <;> rcases left with a | a <;> rcases right with b | b
  all_goals try
    (refine ⟨.Err .InvalidDimensions, ?_, ?_⟩
     · simp [duration.checked_duration_binary]
     · simp)
  · have spec := U64.checked_add_bv_spec a b
    cases h : U64.checked_add a b with
    | none =>
      simp only [h] at spec
      refine ⟨.Err .Overflow, by simp [duration.checked_duration_binary, h], ?_⟩
      have overflows : ¬ (a.val + b.val < 18446744073709551616) := by simp at spec; omega
      simp [overflows]
    | some z =>
      simp only [h] at spec
      obtain ⟨upper, value, _⟩ := spec
      refine ⟨.Ok (.Millis z), by simp [duration.checked_duration_binary, h], ?_⟩
      have fits : a.val + b.val < 18446744073709551616 := by simp at upper; omega
      simp [fits, value]
  · have spec := U64.checked_sub_bv_spec a b
    cases h : U64.checked_sub a b with
    | none =>
      simp only [h] at spec
      refine ⟨.Err .Underflow, by simp [duration.checked_duration_binary, h], ?_⟩
      simp [spec]
    | some z =>
      simp only [h] at spec
      obtain ⟨ordered, value, _⟩ := spec
      refine ⟨.Ok (.Millis z), by simp [duration.checked_duration_binary, h], ?_⟩
      have notBelow : ¬ a.val < b.val := by omega
      have fits : a.val - b.val < 18446744073709551616 := by have := a.hmax; simp at this; omega
      simp [notBelow, fits, value]
  · by_cases nonneg : 0 ≤ b.val
    · have scaled := factor_val b nonneg
      have spec := U64.checked_mul_bv_spec a (IScalar.hcast .U64 b)
      cases h : U64.checked_mul a (IScalar.hcast .U64 b) with
      | none =>
        simp only [h, scaled] at spec
        refine ⟨.Err .Overflow, by simp [duration.checked_duration_binary, nonneg, h], ?_⟩
        have overflows : ¬ (a.val * b.val.toNat < 18446744073709551616) := by simp at spec; omega
        simp [overflows, Int.not_lt.mpr nonneg]
      | some z =>
        simp only [h, scaled] at spec
        obtain ⟨upper, value, _⟩ := spec
        refine ⟨.Ok (.Millis z), by simp [duration.checked_duration_binary, nonneg, h], ?_⟩
        have fits : a.val * b.val.toNat < 18446744073709551616 := by simp at upper; omega
        simp [fits, value, Int.not_lt.mpr nonneg]
    · refine ⟨.Err .NegativeFactor, by simp [duration.checked_duration_binary, nonneg], ?_⟩
      simp [Int.lt_of_not_ge nonneg]
  · by_cases nonneg : 0 ≤ a.val
    · have scaled := factor_val a nonneg
      have spec := U64.checked_mul_bv_spec b (IScalar.hcast .U64 a)
      cases h : U64.checked_mul b (IScalar.hcast .U64 a) with
      | none =>
        simp only [h, scaled] at spec
        refine ⟨.Err .Overflow, by simp [duration.checked_duration_binary, nonneg, h], ?_⟩
        have overflows : ¬ (b.val * a.val.toNat < 18446744073709551616) := by simp at spec; omega
        simp [overflows, Int.not_lt.mpr nonneg]
      | some z =>
        simp only [h, scaled] at spec
        obtain ⟨upper, value, _⟩ := spec
        refine ⟨.Ok (.Millis z), by simp [duration.checked_duration_binary, nonneg, h], ?_⟩
        have fits : b.val * a.val.toNat < 18446744073709551616 := by simp at upper; omega
        simp [fits, value, Int.not_lt.mpr nonneg]
    · refine ⟨.Err .NegativeFactor, by simp [duration.checked_duration_binary, nonneg], ?_⟩
      simp [Int.lt_of_not_ge nonneg]
  · exact ratio_case a b
  · exact quotient_case a b

end Plumbing

theorem production_refines : Refines duration.checked_duration_binary :=
  checked_duration_binary_refines

end XshImpl.Duration
