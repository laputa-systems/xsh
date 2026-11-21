import XshImpl.Arith
import XshImpl.Duration

open Aeneas Aeneas.Std Result
open xsh_arith_slice

namespace XshImpl.Controls

local macro "int" value:term:max : term => `(I64.ofInt $value)
local macro "ms" value:term:max : term => `(U64.ofNat $value)

theorem int_min_val : (int (-9223372036854775808)).val = I64.min := by
  simp [I64.min_eq]

theorem int_negative_one_val : (int (-1)).val = -1 := by
  simp

theorem checked_div_of_val (x y c : Std.I64) (nonzero : y.val ≠ 0)
    (fits : ¬ (x.val = I64.min ∧ y.val = -1)) (value : c.val = x.val.tdiv y.val) :
    I64.checked_div x y = some c := by
  have spec := I64.checked_div_bv_spec x y
  cases h : core.num.checked_div_IScalar x y with
  | none =>
    simp only [h] at spec
    rcases spec with zero | minimal
    · exact absurd zero nonzero
    · exact absurd minimal fits
  | some z =>
    simp only [h] at spec
    simp only [I64.checked_div, h]
    exact congrArg some (IScalar.eq_of_val_eq (by rw [spec.2.2.1, value]))

theorem checked_rem_of_val (x y c : Std.I64) (nonzero : y.val ≠ 0)
    (fits : ¬ (x.val = I64.min ∧ y.val = -1)) (value : c.val = x.val.tmod y.val) :
    I64.checked_rem x y = some c := by
  have spec := I64.checked_rem_bv_spec x y
  cases h : core.num.checked_rem_IScalar x y with
  | none =>
    simp only [h] at spec
    rcases spec with zero | minimal
    · exact absurd zero nonzero
    · exact absurd minimal fits
  | some z =>
    simp only [h] at spec
    simp only [I64.checked_rem, h]
    exact congrArg some (IScalar.eq_of_val_eq (by rw [spec.2.2.1, value]))

theorem min_quotient_overflows :
    I64.checked_div (int (-9223372036854775808)) (int (-1)) = none := by
  have spec := I64.checked_div_bv_spec (int (-9223372036854775808)) (int (-1))
  cases h : core.num.checked_div_IScalar (int (-9223372036854775808)) (int (-1)) with
  | none => simp [I64.checked_div, h]
  | some z =>
    simp only [h] at spec
    exact absurd ⟨int_min_val, int_negative_one_val⟩ spec.2.1

theorem min_remainder_overflows :
    I64.checked_rem (int (-9223372036854775808)) (int (-1)) = none := by
  have spec := I64.checked_rem_bv_spec (int (-9223372036854775808)) (int (-1))
  cases h : core.num.checked_rem_IScalar (int (-9223372036854775808)) (int (-1)) with
  | none => simp [I64.checked_rem, h]
  | some z =>
    simp only [h] at spec
    exact absurd ⟨int_min_val, int_negative_one_val⟩ spec.2.1

theorem int_success_and_truncation :
    arith.checked_int_binary .Add (int 2) (int 3) = ok (some (.Ok (int 5))) ∧
    arith.checked_int_binary .Div (int (-7)) (int 2) = ok (some (.Ok (int (-3)))) ∧
    arith.checked_int_binary .Rem (int (-7)) (int 2) = ok (some (.Ok (int (-1)))) ∧
    arith.checked_int_binary .Div (int 7) (int (-2)) = ok (some (.Ok (int (-3)))) ∧
    arith.checked_int_binary .Rem (int 7) (int (-2)) = ok (some (.Ok (int 1))) := by
  have add : I64.checked_add (int 2) (int 3) = some (int 5) := by decide
  have q₁ : I64.checked_div (int (-7)) (int 2) = some (int (-3)) :=
    checked_div_of_val _ _ _ (by simp) (by simp [I64.min_eq]) (by simp)
  have r₁ : I64.checked_rem (int (-7)) (int 2) = some (int (-1)) :=
    checked_rem_of_val _ _ _ (by simp) (by simp [I64.min_eq]) (by simp)
  have q₂ : I64.checked_div (int 7) (int (-2)) = some (int (-3)) :=
    checked_div_of_val _ _ _ (by simp) (by simp [I64.min_eq]) (by simp)
  have r₂ : I64.checked_rem (int 7) (int (-2)) = some (int 1) :=
    checked_rem_of_val _ _ _ (by simp) (by simp [I64.min_eq]) (by simp)
  have two : ¬ int 2 = 0#i64 := by decide
  have negTwo : ¬ int (-2) = 0#i64 := by decide
  simp [arith.checked_int_binary, add, q₁, r₁, q₂, r₂, two, negTwo, lift,
    core.option.Option.ok_or]

theorem int_error_branches :
    arith.checked_int_binary .Div (int 7) (int 0) = ok (some (.Err .DivisionByZero)) ∧
    arith.checked_int_binary .Rem (int 7) (int 0) = ok (some (.Err .DivisionByZero)) ∧
    arith.checked_int_binary .Add (int 9223372036854775807) (int 1) = ok (some (.Err .Overflow)) ∧
    arith.checked_int_binary .Sub (int (-9223372036854775808)) (int 1) = ok (some (.Err .Overflow)) ∧
    arith.checked_int_binary .Mul (int 9223372036854775807) (int 2) = ok (some (.Err .Overflow)) ∧
    arith.checked_int_binary .Div (int (-9223372036854775808)) (int (-1)) = ok (some (.Err .Overflow)) ∧
    arith.checked_int_binary .Rem (int (-9223372036854775808)) (int (-1)) = ok (some (.Err .Overflow)) ∧
    arith.checked_int_binary .Lt (int 1) (int 2) = ok none := by
  have negOne : ¬ int (-1) = 0#i64 := by decide
  have add : I64.checked_add (int 9223372036854775807) (int 1) = none := by decide
  have sub : I64.checked_sub (int (-9223372036854775808)) (int 1) = none := by decide
  have mul : I64.checked_mul (int 9223372036854775807) (int 2) = none := by decide
  simp [arith.checked_int_binary, negOne, add, sub, mul, min_quotient_overflows,
    min_remainder_overflows, lift,
    core.option.Option.ok_or]

theorem int_diagnostics :
    arith.IntArithmeticError.diagnostic .DivisionByZero =
      ok (toStr "division-by-zero", toStr "division by zero") ∧
    arith.IntArithmeticError.diagnostic .Overflow =
      ok (toStr "integer-overflow", toStr "integer overflow") := by
  simp [arith.IntArithmeticError.diagnostic]

def upstreamCheckedRem (x y : Std.I64) : Option Std.I64 := Option.ofResult (IScalar.rem x y)

theorem upstream_remainder_model_differs_at_min :
    upstreamCheckedRem (int (-9223372036854775808)) (int (-1)) = some (int 0) ∧
    I64.checked_rem (int (-9223372036854775808)) (int (-1)) = none :=
  ⟨by decide, min_remainder_overflows⟩

def wrappingOverflow : XshImpl.Arith.Implementation
  | .Add, left, right => ok (some (.Ok (core.num.I64.wrapping_add left right)))
  | op, left, right => arith.checked_int_binary op left right

def upstreamRemainder : XshImpl.Arith.Implementation
  | .Rem, left, right =>
    if right = 0#i64 then ok (some (.Err .DivisionByZero))
    else ok (some ((upstreamCheckedRem left right).elim (.Err .Overflow) .Ok))
  | op, left, right => arith.checked_int_binary op left right

def flooredDivision : XshImpl.Arith.Implementation
  | .Div, left, right =>
    if right = 0#i64 then ok (some (.Err .DivisionByZero))
    else ok (some (.Ok ⟨BitVec.ofInt _ (left.val.fdiv right.val)⟩))
  | op, left, right => arith.checked_int_binary op left right

def swappedZeroError : XshImpl.Arith.Implementation
  | .Div, left, right =>
    if right = 0#i64 then ok (some (.Err .Overflow)) else arith.checked_int_binary .Div left right
  | op, left, right => arith.checked_int_binary op left right

def admitsComparison : XshImpl.Arith.Implementation
  | .Lt, _, _ => ok (some (.Ok (int 1)))
  | op, left, right => arith.checked_int_binary op left right

def alwaysOverflow : XshImpl.Arith.Implementation
  | op, _, _ => ok ((XshImpl.Arith.intOp? op).map fun _ => .Err .Overflow)

theorem wrapping_overflow_rejected : ¬ XshImpl.Arith.Refines wrappingOverflow := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Add (int 9223372036854775807) (int 1)
  have wrapped :
      core.num.I64.wrapping_add (int 9223372036854775807) (int 1) = int (-9223372036854775808) := by
    decide
  simp [wrappingOverflow, wrapped] at run
  subst run
  revert decoded
  decide

theorem upstream_remainder_rejected : ¬ XshImpl.Arith.Refines upstreamRemainder := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Rem (int (-9223372036854775808)) (int (-1))
  have negOne : ¬ int (-1) = 0#i64 := by decide
  simp [upstreamRemainder, negOne, upstream_remainder_model_differs_at_min.1] at run
  subst run
  revert decoded
  decide

theorem floored_division_rejected : ¬ XshImpl.Arith.Refines flooredDivision := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Div (int (-7)) (int 2)
  have two : ¬ int 2 = 0#i64 := by decide
  simp [flooredDivision, two] at run
  subst run
  revert decoded
  decide

theorem swapped_zero_error_rejected : ¬ XshImpl.Arith.Refines swappedZeroError := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Div (int 1) (int 0)
  simp [swappedZeroError] at run
  subst run
  revert decoded
  decide

theorem admitted_comparison_rejected : ¬ XshImpl.Arith.Refines admitsComparison := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Lt (int 1) (int 2)
  simp [admitsComparison] at run
  subst run
  revert decoded
  decide

theorem always_overflow_rejected : ¬ XshImpl.Arith.Refines alwaysOverflow := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Add (int 2) (int 3)
  simp [alwaysOverflow, XshImpl.Arith.intOp?] at run
  subst run
  revert decoded
  decide

open XshImpl.Duration (decode expected operand) in
theorem duration_output (op : syntax.node.BinaryOp) (left right : duration.DurationOperand)
    (result : core.result.Result duration.DurationResult duration.DurationArithmeticError)
    (decoded : decode result = expected op (operand left) (operand right)) :
    duration.checked_duration_binary op left right = ok result := by
  obtain ⟨out, run, agrees⟩ := XshImpl.Duration.checked_duration_binary_refines op left right
  rw [run, XshImpl.Duration.decode_injective (agrees.trans decoded.symm)]

theorem duration_success_and_truncation :
    duration.checked_duration_binary .Add (.Millis (ms 250)) (.Millis (ms 1000)) =
      ok (.Ok (.Millis (ms 1250))) ∧
    duration.checked_duration_binary .Mul (.Count (int 3)) (.Millis (ms 250)) =
      ok (.Ok (.Millis (ms 750))) ∧
    duration.checked_duration_binary .Div (.Millis (ms 5)) (.Count (int 2)) =
      ok (.Ok (.Millis (ms 2))) ∧
    duration.checked_duration_binary .Div (.Millis (ms 7000)) (.Millis (ms 2000)) =
      ok (.Ok (.Count (int 3))) :=
  ⟨duration_output _ _ _ _ (by decide), duration_output _ _ _ _ (by decide),
    duration_output _ _ _ _ (by decide), duration_output _ _ _ _ (by decide)⟩

theorem duration_error_branches :
    duration.checked_duration_binary .Sub (.Millis (ms 0)) (.Millis (ms 1)) = ok (.Err .Underflow) ∧
    duration.checked_duration_binary .Add (.Millis (ms 18446744073709551615)) (.Millis (ms 1)) =
      ok (.Err .Overflow) ∧
    duration.checked_duration_binary .Mul (.Millis (ms 18446744073709551615)) (.Count (int 2)) =
      ok (.Err .Overflow) ∧
    duration.checked_duration_binary .Mul (.Millis (ms 1)) (.Count (int (-1))) =
      ok (.Err .NegativeFactor) ∧
    duration.checked_duration_binary .Mul (.Count (int (-1))) (.Millis (ms 0)) =
      ok (.Err .NegativeFactor) ∧
    duration.checked_duration_binary .Div (.Millis (ms 1)) (.Count (int 0)) =
      ok (.Err .NonpositiveDivisor) ∧
    duration.checked_duration_binary .Div (.Millis (ms 1)) (.Count (int (-1))) =
      ok (.Err .NonpositiveDivisor) ∧
    duration.checked_duration_binary .Div (.Millis (ms 1)) (.Millis (ms 0)) =
      ok (.Err .NonpositiveDivisor) ∧
    duration.checked_duration_binary .Div (.Millis (ms 18446744073709551615)) (.Millis (ms 1)) =
      ok (.Err .CountOverflow) ∧
    duration.checked_duration_binary .Rem (.Millis (ms 7)) (.Millis (ms 2)) =
      ok (.Err .InvalidDimensions) ∧
    duration.checked_duration_binary .Add (.Millis (ms 1)) (.Count (int 1)) =
      ok (.Err .InvalidDimensions) ∧
    duration.checked_duration_binary .Div (.Count (int 1)) (.Millis (ms 1)) =
      ok (.Err .InvalidDimensions) :=
  ⟨duration_output _ _ _ _ (by decide), duration_output _ _ _ _ (by decide),
    duration_output _ _ _ _ (by decide), duration_output _ _ _ _ (by decide),
    duration_output _ _ _ _ (by decide), duration_output _ _ _ _ (by decide),
    duration_output _ _ _ _ (by decide), duration_output _ _ _ _ (by decide),
    duration_output _ _ _ _ (by decide), duration_output _ _ _ _ (by decide),
    duration_output _ _ _ _ (by decide), duration_output _ _ _ _ (by decide)⟩

theorem duration_diagnostics :
    duration.DurationArithmeticError.diagnostic .InvalidDimensions =
      ok (toStr "type-error", toStr "invalid Duration arithmetic dimensions") ∧
    duration.DurationArithmeticError.diagnostic .Overflow =
      ok (toStr "duration-overflow", toStr "Duration arithmetic exceeds representable milliseconds") ∧
    duration.DurationArithmeticError.diagnostic .Underflow =
      ok (toStr "duration-underflow", toStr "Duration subtraction would be negative") ∧
    duration.DurationArithmeticError.diagnostic .NegativeFactor =
      ok (toStr "duration-negative-factor", toStr "Duration multiplier must be nonnegative") ∧
    duration.DurationArithmeticError.diagnostic .NonpositiveDivisor =
      ok (toStr "division-by-zero", toStr "Duration divisor must be positive") ∧
    duration.DurationArithmeticError.diagnostic .CountOverflow =
      ok (toStr "integer-overflow", toStr "Duration interval count exceeds Int") := by
  simp [duration.DurationArithmeticError.diagnostic]

def wrappingSum : XshImpl.Duration.Implementation
  | .Add, .Millis a, .Millis b => ok (.Ok (.Millis (core.num.U64.wrapping_add a b)))
  | op, left, right => duration.checked_duration_binary op left right

def saturatingDifference : XshImpl.Duration.Implementation
  | .Sub, .Millis a, .Millis b => ok (.Ok (.Millis (core.num.U64.saturating_sub a b)))
  | op, left, right => duration.checked_duration_binary op left right

def wrappedFactor : XshImpl.Duration.Implementation
  | .Mul, .Millis a, .Count b =>
    ok (.Ok (.Millis (core.num.U64.wrapping_mul a (IScalar.hcast .U64 b))))
  | op, left, right => duration.checked_duration_binary op left right

def wrappingCount : XshImpl.Duration.Implementation
  | .Div, .Millis a, .Millis b =>
    if b.val = 0 then ok (.Err .NonpositiveDivisor)
    else ok (.Ok (.Count (UScalar.hcast .I64 (⟨BitVec.udiv a.bv b.bv⟩ : Std.U64))))
  | op, left, right => duration.checked_duration_binary op left right

def alwaysTypeError : XshImpl.Duration.Implementation
  | _, _, _ => ok (.Err .InvalidDimensions)

theorem wrapping_sum_rejected : ¬ XshImpl.Duration.Refines wrappingSum := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Add (.Millis (ms 18446744073709551615)) (.Millis (ms 1))
  have wrapped : core.num.U64.wrapping_add (ms 18446744073709551615) (ms 1) = ms 0 := by decide
  simp [wrappingSum, wrapped] at run
  subst run
  revert decoded
  decide

theorem saturating_difference_rejected : ¬ XshImpl.Duration.Refines saturatingDifference := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Sub (.Millis (ms 0)) (.Millis (ms 1))
  have saturated : core.num.U64.saturating_sub (ms 0) (ms 1) = ms 0 := by decide
  simp [saturatingDifference, saturated] at run
  subst run
  revert decoded
  decide

theorem wrapped_factor_rejected : ¬ XshImpl.Duration.Refines wrappedFactor := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Mul (.Millis (ms 1)) (.Count (int (-1)))
  have wrapped :
      core.num.U64.wrapping_mul (ms 1) (IScalar.hcast .U64 (int (-1))) = ms 18446744073709551615 := by
    decide
  simp [wrappedFactor, wrapped] at run
  subst run
  revert decoded
  decide

theorem wrapping_count_rejected : ¬ XshImpl.Duration.Refines wrappingCount := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Div (.Millis (ms 18446744073709551615)) (.Millis (ms 1))
  simp [wrappingCount] at run
  subst run
  revert decoded
  decide

theorem always_type_error_rejected : ¬ XshImpl.Duration.Refines alwaysTypeError := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Add (.Millis (ms 1)) (.Millis (ms 2))
  simp [alwaysTypeError] at run
  subst run
  revert decoded
  decide

end XshImpl.Controls
