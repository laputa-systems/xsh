import XshImpl.XshArithSlice
import Xsh.Core.Arith

open Aeneas Aeneas.Std Result
open xsh_arith_slice

namespace XshImpl.Arith

open Xsh.Core.Arith (IntOp IntError Representable)

def intOp? : syntax.node.BinaryOp → Option IntOp
  | .Add => some .add
  | .Sub => some .sub
  | .Mul => some .mul
  | .Div => some .div
  | .Rem => some .rem
  | _ => none

def errorOf : arith.IntArithmeticError → IntError
  | .DivisionByZero => .divisionByZero
  | .Overflow => .overflow

def decode : Option (core.result.Result Std.I64 arith.IntArithmeticError) →
    Option (Except IntError Int)
  | none => none
  | some (.Ok value) => some (.ok value.val)
  | some (.Err error) => some (.error (errorOf error))

def expected (op : syntax.node.BinaryOp) (left right : Int) : Option (Except IntError Int) :=
  (intOp? op).map fun op => Xsh.Core.Arith.checked op left right

abbrev Implementation :=
  syntax.node.BinaryOp → Std.I64 → Std.I64 →
    Result (Option (core.result.Result Std.I64 arith.IntArithmeticError))

def Refines (implementation : Implementation) : Prop :=
  ∀ op left right, ∃ out, implementation op left right = ok out ∧
    decode out = expected op left.val right.val

theorem errorOf_injective : Function.Injective errorOf := by
  intro a b h
  cases a <;> cases b <;> first | rfl | cases h

theorem decode_injective : Function.Injective decode := by
  intro a b h
  rcases a with _ | (a | a) <;> rcases b with _ | (b | b) <;> simp [decode] at h ⊢
  · exact IScalar.eq_of_val_eq h
  · exact errorOf_injective h

theorem representable_iff (value : Int) :
    Representable value ↔ I64.min ≤ value ∧ value ≤ I64.max := by
  simp [Representable, Xsh.Core.Arith.minInt, Xsh.Core.Arith.maxInt, I64.min_eq, I64.max_eq]

theorem tdiv_representable (a b : Int) (ha : I64.min ≤ a ∧ a ≤ I64.max)
    (hb : I64.min ≤ b ∧ b ≤ I64.max) (nonzero : b ≠ 0) :
    Representable (a.tdiv b) ↔ ¬ (a = I64.min ∧ b = -1) := by
  rw [representable_iff]
  simp only [I64.min_eq, I64.max_eq] at *
  by_cases negativeOne : b = -1
  · subst negativeOne
    rw [Int.tdiv_neg, Int.tdiv_one]
    omega
  · have bound : (a.tdiv b).natAbs ≤ a.natAbs / 2 ∨ b = 1 := by
      by_cases unit : b.natAbs = 1
      · right; omega
      · left
        rw [Int.natAbs_tdiv]
        exact Nat.div_le_div_left (by omega) (by omega)
    rcases bound with bound | one
    · constructor
      · intro _ minimal; exact negativeOne minimal.2
      · intro _; omega
    · subst one; rw [Int.tdiv_one]; omega

end XshImpl.Arith
