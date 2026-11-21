import XshImpl.ArithSpec

open Aeneas Aeneas.Std Result
open xsh_arith_slice

namespace XshImpl.Arith

open Xsh.Core.Arith (Representable)

theorem checked_int_binary_refines (op : syntax.node.BinaryOp) (left right : Std.I64) :
    ∃ out, arith.checked_int_binary op left right = ok out ∧
      decode out = expected op left.val right.val := by
  have bounds : ∀ x : Std.I64, I64.min ≤ x.val ∧ x.val ≤ I64.max :=
    fun x => ⟨by scalar_tac, by scalar_tac⟩
  cases op
  case Add =>
    have spec := I64.checked_add_bv_spec left right
    simp only [arith.checked_int_binary, I64.checked_add]
    cases h : core.num.checked_add_IScalar left right with
    | none =>
      simp only [h] at spec
      refine ⟨some (.Err .Overflow), by simp [lift, core.option.Option.ok_or], ?_⟩
      simp [decode, errorOf, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
        Xsh.Core.Arith.exact, representable_iff]
      all_goals omega
    | some z =>
      simp only [h] at spec
      obtain ⟨lower, upper, value, _⟩ := spec
      refine ⟨some (.Ok z), by simp [lift, core.option.Option.ok_or], ?_⟩
      simp [decode, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
        Xsh.Core.Arith.exact, representable_iff, lower, upper, value]
  case Sub =>
    have spec := I64.checked_sub_bv_spec left right
    simp only [arith.checked_int_binary, I64.checked_sub]
    cases h : core.num.checked_sub_IScalar left right with
    | none =>
      simp only [h] at spec
      refine ⟨some (.Err .Overflow), by simp [lift, core.option.Option.ok_or], ?_⟩
      simp [decode, errorOf, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
        Xsh.Core.Arith.exact, representable_iff]
      all_goals omega
    | some z =>
      simp only [h] at spec
      obtain ⟨lower, upper, value, _⟩ := spec
      refine ⟨some (.Ok z), by simp [lift, core.option.Option.ok_or], ?_⟩
      simp [decode, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
        Xsh.Core.Arith.exact, representable_iff, lower, upper, value]
  case Mul =>
    have spec := I64.checked_mul_bv_spec left right
    simp only [arith.checked_int_binary, I64.checked_mul]
    cases h : core.num.checked_mul_IScalar left right with
    | none =>
      simp only [h] at spec
      refine ⟨some (.Err .Overflow), by simp [lift, core.option.Option.ok_or], ?_⟩
      simp [decode, errorOf, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
        Xsh.Core.Arith.exact, representable_iff]
      all_goals omega
    | some z =>
      simp only [h] at spec
      obtain ⟨lower, upper, value, _⟩ := spec
      refine ⟨some (.Ok z), by simp [lift, core.option.Option.ok_or], ?_⟩
      simp [decode, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
        Xsh.Core.Arith.exact, representable_iff, lower, upper, value]
  case Div =>
    simp only [arith.checked_int_binary, I64.checked_div]
    by_cases zero : right = 0#i64
    · have value : right.val = 0 := by simp [zero]
      refine ⟨some (.Err .DivisionByZero), by simp [zero], ?_⟩
      simp [decode, errorOf, expected, intOp?, Xsh.Core.Arith.checked, value]
    · have nonzero : right.val ≠ 0 := fun h => zero (by scalar_tac)
      have spec := I64.checked_div_bv_spec left right
      cases h : core.num.checked_div_IScalar left right with
      | none =>
        simp only [h] at spec
        have minimal := spec.resolve_left nonzero
        have unrepresentable : ¬ Representable (left.val.tdiv right.val) := fun fits =>
          (tdiv_representable _ _ (bounds left) (bounds right) nonzero).mp fits minimal
        refine ⟨some (.Err .Overflow), by simp [zero, lift, core.option.Option.ok_or], ?_⟩
        simp [decode, errorOf, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
          Xsh.Core.Arith.exact, nonzero, unrepresentable]
      | some z =>
        simp only [h] at spec
        obtain ⟨_, fits, value, _⟩ := spec
        have representable :=
          (tdiv_representable _ _ (bounds left) (bounds right) nonzero).mpr fits
        refine ⟨some (.Ok z), by simp [zero, lift, core.option.Option.ok_or], ?_⟩
        simp [decode, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
          Xsh.Core.Arith.exact, nonzero, representable, value]
  case Rem =>
    simp only [arith.checked_int_binary, I64.checked_rem]
    by_cases zero : right = 0#i64
    · have value : right.val = 0 := by simp [zero]
      refine ⟨some (.Err .DivisionByZero), by simp [zero], ?_⟩
      simp [decode, errorOf, expected, intOp?, Xsh.Core.Arith.checked, value]
    · have nonzero : right.val ≠ 0 := fun h => zero (by scalar_tac)
      have spec := I64.checked_rem_bv_spec left right
      cases h : core.num.checked_rem_IScalar left right with
      | none =>
        simp only [h] at spec
        have minimal := spec.resolve_left nonzero
        have unrepresentable : ¬ Representable (left.val.tdiv right.val) := fun fits =>
          (tdiv_representable _ _ (bounds left) (bounds right) nonzero).mp fits minimal
        refine ⟨some (.Err .Overflow), by simp [zero, lift, core.option.Option.ok_or], ?_⟩
        simp [decode, errorOf, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
          nonzero, unrepresentable]
      | some z =>
        simp only [h] at spec
        obtain ⟨_, fits, value, _⟩ := spec
        have representable :=
          (tdiv_representable _ _ (bounds left) (bounds right) nonzero).mpr fits
        refine ⟨some (.Ok z), by simp [zero, lift, core.option.Option.ok_or], ?_⟩
        simp [decode, expected, intOp?, Xsh.Core.Arith.checked, Xsh.Core.Arith.witness,
          Xsh.Core.Arith.exact, nonzero, representable, value]
  all_goals exact ⟨none, by simp [arith.checked_int_binary], rfl⟩

theorem production_refines : Refines arith.checked_int_binary :=
  checked_int_binary_refines

end XshImpl.Arith
