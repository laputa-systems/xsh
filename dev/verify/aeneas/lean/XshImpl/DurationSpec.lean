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

end XshImpl.Duration
