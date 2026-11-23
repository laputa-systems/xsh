import Arith

open Xsh.Core.Arith

namespace MillipedeArith

inductive ResultView where
  | ok (value : UInt64)
  | overflow
  | divisionByZero


def view (answer : Answer) : ResultView :=
  if answer.tag = 0 then .ok answer.value
  else if answer.tag = 1 then .overflow
  else .divisionByZero

def decode : ResultView → Except IntError Int
  | .ok value => .ok value.toBitVec.toInt
  | .overflow => .error .overflow
  | .divisionByZero => .error .divisionByZero

theorem decode_injective : Function.Injective decode := by
  intro a b h
  cases a with
  | ok a =>
    cases b with
    | ok b =>
      simp only [decode, Except.ok.injEq] at h
      have hb := BitVec.toInt_inj.mp h
      cases a; cases b; cases hb; rfl
    | overflow => cases h
    | divisionByZero => cases h
  | overflow => cases b <;> cases h <;> rfl
  | divisionByZero => cases b <;> cases h <;> rfl

def encode (a : Int) : UInt64 := ⟨BitVec.ofInt 64 a⟩

theorem decode_encode (a : Int) (h : Representable a) : (encode a).toBitVec.toInt = a := by
  apply BitVec.toInt_ofInt_eq_self (by decide)
  all_goals simp only [Representable, minInt, maxInt] at h; omega

theorem add_refines_words (a b : UInt64) :
    decode (view (checkedAdd a b).run) =
      checked .add a.toBitVec.toInt b.toBitVec.toInt := by
  have ha := a.toNat_lt
  have hb := b.toNat_lt
  by_cases hs : a.toNat + b.toNat < 18446744073709551616
  · have hm : (a.toNat + b.toNat) % 18446744073709551616 = a.toNat + b.toNat := Nat.mod_eq_of_lt hs
    unfold checkedAdd
    split <;> simp_all [view, decode, checked, exact, witness, Representable, minInt, maxInt,
      UInt64.lt_iff_toNat_lt, UInt64.le_iff_toNat_le, UInt64.toNat_add, BitVec.toInt]
    all_goals try split
    all_goals try split
    all_goals try split
    all_goals simp_all [Nat.mod_eq_of_lt hs]
    all_goals try split
    all_goals try simp_all
    all_goals omega
  · have hm : (a.toNat + b.toNat) % 18446744073709551616 = a.toNat + b.toNat - 18446744073709551616 := by omega
    have hcast : ((a.toNat + b.toNat - 18446744073709551616 : Nat) : Int) =
        (a.toNat : Int) + (b.toNat : Int) - 18446744073709551616 := by omega
    unfold checkedAdd
    split <;> simp_all [view, decode, checked, exact, witness, Representable, minInt, maxInt,
      UInt64.lt_iff_toNat_lt, UInt64.le_iff_toNat_le, UInt64.toNat_add, BitVec.toInt]
    all_goals try split
    all_goals try split
    all_goals try split
    all_goals try simp_all [hm, hcast]
    all_goals try split
    all_goals try simp_all
    all_goals omega

theorem sub_refines_words (a b : UInt64) :
    decode (view (checkedSub a b).run) =
      checked .sub a.toBitVec.toInt b.toBitVec.toInt := by
  have ha := a.toNat_lt
  have hb := b.toNat_lt
  by_cases hs : b.toNat ≤ a.toNat
  · have hm : (a - b).toNat = a.toNat - b.toNat := by
      exact UInt64.toNat_sub_of_le a b (UInt64.le_iff_toNat_le.mpr hs)
    have hc : (((a - b).toNat : Nat) : Int) = (a.toNat : Int) - b.toNat := by omega
    unfold checkedSub
    split <;> simp_all [view, decode, checked, exact, witness, Representable, minInt, maxInt,
      UInt64.lt_iff_toNat_lt, UInt64.le_iff_toNat_le, BitVec.toInt]
    all_goals try split
    all_goals try split
    all_goals try split
    all_goals try simp_all [hm, hc]
    all_goals try split
    all_goals try simp_all
    all_goals omega
  · have hm : (a - b).toNat = 18446744073709551616 + a.toNat - b.toNat := by
      rw [UInt64.toNat_sub]
      have : 18446744073709551616 - b.toNat + a.toNat < 18446744073709551616 := by omega
      rw [Nat.mod_eq_of_lt this]
      omega
    have hc : (((a - b).toNat : Nat) : Int) = 18446744073709551616 + (a.toNat : Int) - b.toNat := by omega
    unfold checkedSub
    split <;> simp_all [view, decode, checked, exact, witness, Representable, minInt, maxInt,
      UInt64.lt_iff_toNat_lt, UInt64.le_iff_toNat_le, BitVec.toInt]
    all_goals try split
    all_goals try split
    all_goals try split
    all_goals try simp_all [hm, hc]
    all_goals try split
    all_goals try simp_all
    all_goals omega

theorem signed_msb (a : UInt64) :
    a.toBitVec.msb = decide (¬ a < 9223372036854775808) := by
  simp [BitVec.msb_eq_decide, UInt64.lt_iff_toNat_lt]

theorem div_value (a b : UInt64) (hz : b ≠ 0)
    (ho : ¬ (a = 9223372036854775808 ∧ b = 18446744073709551615)) :
    ((checkedDiv a b).run).value.toBitVec = a.toBitVec.sdiv b.toBitVec := by
  by_cases ha : a < 9223372036854775808 <;>
  by_cases hb : b < 9223372036854775808 <;>
    simp [checkedDiv, magnitude, hz, ho, ha, hb, BitVec.sdiv, signed_msb,
      UInt64.toBitVec_sub, UInt64.toBitVec_div, BitVec.zero_sub]

theorem rem_value (a b : UInt64) (hz : b ≠ 0)
    (ho : ¬ (a = 9223372036854775808 ∧ b = 18446744073709551615)) :
    ((checkedRem a b).run).value.toBitVec = a.toBitVec.srem b.toBitVec := by
  by_cases ha : a < 9223372036854775808 <;>
  by_cases hb : b < 9223372036854775808 <;>
    simp [checkedRem, magnitude, hz, ho, ha, hb, BitVec.srem, signed_msb,
      UInt64.toBitVec_sub, UInt64.toBitVec_mod, BitVec.zero_sub]
  all_goals split <;> simp_all [UInt64.lt_iff_toNat_lt, UInt64.le_iff_toNat_le] <;> omega

theorem word_representable (x : BitVec 64) : Representable x.toInt := by
  have h1 := BitVec.le_toInt x
  have h2 := BitVec.toInt_lt (x := x)
  simp only [Representable, minInt, maxInt] at *
  omega

theorem word_nonzero (x : UInt64) (h : x ≠ 0) : x.toBitVec.toInt ≠ 0 := by
  intro he
  apply h
  apply UInt64.toBitVec_inj.mp
  apply BitVec.toInt_inj.mp
  simpa using he

theorem div_no_overflow (a b : UInt64)
    (h : ¬ (a = 9223372036854775808 ∧ b = 18446744073709551615)) :
    a.toBitVec ≠ BitVec.intMin 64 ∨ b.toBitVec ≠ -1#64 := by
  by_cases ha : a.toBitVec = BitVec.intMin 64
  · right
    intro hb
    apply h
    constructor <;> apply UInt64.toBitVec_inj.mp
    · exact ha.trans (by decide)
    · simpa using hb
  · exact Or.inl ha

theorem quotient_representable (a b : UInt64)
    (h : ¬ (a = 9223372036854775808 ∧ b = 18446744073709551615)) :
    Representable (a.toBitVec.toInt.tdiv b.toBitVec.toInt) := by
  rw [← BitVec.toInt_sdiv_of_ne_or_ne _ _ (div_no_overflow a b h)]
  exact word_representable _

theorem div_refines_words (a b : UInt64) :
    decode (view (checkedDiv a b).run) =
      checked .div a.toBitVec.toInt b.toBitVec.toInt := by
  by_cases hz : b = 0
  · subst b
    simp [checkedDiv, view, decode, checked]
  by_cases ho : a = 9223372036854775808 ∧ b = 18446744073709551615
  · rcases ho with ⟨rfl, rfl⟩
    simp [checkedDiv, checkedRem, view, decode, checked, witness, exact, Representable, minInt, maxInt, BitVec.toInt]
  · have hn := word_nonzero b hz
    have hf := quotient_representable a b ho
    have hv := congrArg BitVec.toInt (div_value a b hz ho)
    rw [BitVec.toInt_sdiv_of_ne_or_ne _ _ (div_no_overflow a b ho)] at hv
    simp [checked, exact, witness, hn, hf, view, decode, checkedDiv, hz, ho] at *
    exact hv

theorem rem_refines_words (a b : UInt64) :
    decode (view (checkedRem a b).run) =
      checked .rem a.toBitVec.toInt b.toBitVec.toInt := by
  by_cases hz : b = 0
  · subst b
    simp [checkedRem, view, decode, checked]
  by_cases ho : a = 9223372036854775808 ∧ b = 18446744073709551615
  · rcases ho with ⟨rfl, rfl⟩
    simp [checkedDiv, checkedRem, view, decode, checked, witness, exact, Representable, minInt, maxInt, BitVec.toInt]
  · have hn := word_nonzero b hz
    have hf := quotient_representable a b ho
    have hv := congrArg BitVec.toInt (rem_value a b hz ho)
    rw [BitVec.toInt_srem] at hv
    simp [checked, exact, witness, hn, hf, view, decode, checkedRem, hz, ho] at *
    exact hv

theorem magnitude_signed (a : UInt64) :
    a.toBitVec.toInt = if a < 9223372036854775808 then
      ((magnitude a).run.toNat : Int) else -((magnitude a).run.toNat : Int) := by
  by_cases ha : a < 9223372036854775808
  · have hm : a.toBitVec.msb = false := by simp [signed_msb, ha]
    rw [BitVec.toInt_eq_toNat_of_msb hm]
    simp [magnitude, ha]
  · have hm : a.toBitVec.msb = true := by simp [signed_msb, ha]
    rw [BitVec.toInt_eq_neg_toNat_neg_of_msb_true hm]
    simp only [ha, if_false, magnitude, Id.run]
    rfl

theorem mul_bound (limit x y : UInt64) :
    (y ≠ 0 ∧ limit / y < x) ↔ limit.toNat < x.toNat * y.toNat := by
  by_cases hy : y = 0
  · subst y
    simp
  · have hp : 0 < y.toNat := by
      have hn : y.toNat ≠ 0 := fun h => hy (UInt64.toNat_inj.mp (by simpa using h))
      omega
    rw [UInt64.lt_iff_toNat_lt, UInt64.toNat_div, Nat.div_lt_iff_lt_mul hp]
    exact ⟨fun h => h.2, fun h => ⟨hy, h⟩⟩

def mulBad (a b : UInt64) : Prop :=
  let x := (magnitude a).run
  let y := (magnitude b).run
  let limit : UInt64 := if (a < 9223372036854775808) != (b < 9223372036854775808)
    then 9223372036854775808 else 9223372036854775807
  y ≠ 0 ∧ limit / y < x

theorem mul_bad_iff (a b : UInt64) :
    mulBad a b ↔ ¬ Representable (a.toBitVec.toInt * b.toBitVec.toInt) := by
  have ah := magnitude_signed a
  have bh := magnitude_signed b
  by_cases ha : a < 9223372036854775808 <;>
  by_cases hb : b < 9223372036854775808 <;>
    simp only [ha, hb, ↓reduceIte] at ah bh <;>
    rw [ah, bh] <;>
    simp [mulBad, ha, hb, mul_bound, Representable, minInt, maxInt,
      Int.neg_mul, Int.mul_neg, ← Int.natCast_mul] <;>
    omega

open Classical in
theorem mul_shape (a b : UInt64) :
    (checkedMul a b).run = if mulBad a b then Answer.mk 1 0 else
      Answer.mk 0 (if (a < 9223372036854775808) != (b < 9223372036854775808)
        then 0 - ((magnitude a).run * (magnitude b).run)
        else (magnitude a).run * (magnitude b).run) := by
  by_cases ha : a < 9223372036854775808 <;>
  by_cases hb : b < 9223372036854775808 <;>
    simp [checkedMul, mulBad, ha, hb] <;> split <;> rfl

theorem mul_value (a b : UInt64) (h : ¬ mulBad a b) :
    ((checkedMul a b).run).value.toBitVec = a.toBitVec * b.toBitVec := by
  rw [mul_shape, if_neg h]
  by_cases ha : a < 9223372036854775808 <;>
  by_cases hb : b < 9223372036854775808 <;>
    simp [magnitude, ha, hb, UInt64.toBitVec_mul, BitVec.neg_mul, BitVec.mul_neg]

theorem mul_refines_words (a b : UInt64) :
    decode (view (checkedMul a b).run) =
      checked .mul a.toBitVec.toInt b.toBitVec.toInt := by
  by_cases h : mulBad a b
  · have hf : ¬ Representable (a.toBitVec.toInt * b.toBitVec.toInt) := (mul_bad_iff a b).mp h
    simp [mul_shape, h, view, decode, checked, exact, witness, hf]
  · have hf : Representable (a.toBitVec.toInt * b.toBitVec.toInt) := by
      by_cases hh : Representable (a.toBitVec.toInt * b.toBitVec.toInt)
      · exact hh
      · exact False.elim (h ((mul_bad_iff a b).mpr hh))
    have hm : ¬ a.toBitVec.smulOverflow b.toBitVec = true := by
      simp [BitVec.smulOverflow, Representable, minInt, maxInt] at hf ⊢
      omega
    have hv := congrArg BitVec.toInt (mul_value a b h)
    rw [BitVec.toInt_mul_of_not_smulOverflow hm] at hv
    simpa [mul_shape, h, view, decode, checked, exact, witness, hf] using hv

def opCode : IntOp → UInt64
  | .add => 0
  | .sub => 1
  | .mul => 2
  | .div => 3
  | .rem => 4

def expected (op : IntOp) (a b : Int) : Except IntError Int := checked op a b

theorem checked_refines (op : IntOp) (a b : Int) (ha : Representable a) (hb : Representable b) :
    decode (view (checkedOp (opCode op) (encode a) (encode b)).run) = expected op a b := by
  cases op with
  | add => simpa [checkedOp, opCode, expected, decode_encode a ha, decode_encode b hb] using add_refines_words (encode a) (encode b)
  | sub => simpa [checkedOp, opCode, expected, decode_encode a ha, decode_encode b hb] using sub_refines_words (encode a) (encode b)
  | mul => simpa [checkedOp, opCode, expected, decode_encode a ha, decode_encode b hb] using mul_refines_words (encode a) (encode b)
  | div => simpa [checkedOp, opCode, expected, decode_encode a ha, decode_encode b hb] using div_refines_words (encode a) (encode b)
  | rem => simpa [checkedOp, opCode, expected, decode_encode a ha, decode_encode b hb] using rem_refines_words (encode a) (encode b)

theorem core_refines (op : IntOp) (a b : Int) (ha : Representable a) (hb : Representable b)
    (ω : Millipede.Oracle) (w : Millipede.World) :
    decode (view ((Millipede.Exp.denote ω checkedOp.exp
      (Millipede.Env.cons (encode b) (Millipede.Env.cons (encode a)
        (Millipede.Env.cons (opCode op) Millipede.Env.nil))) w).1)) = expected op a b := by
  rw [checkedOp.exp_denote]
  exact checked_refines op a b ha hb

def Canonical (r : Answer) : Prop :=
  r.tag = 0 ∨ (r.tag = 1 ∧ r.value = 0) ∨ (r.tag = 2 ∧ r.value = 0)

def decodeAnswer (r : { r : Answer // Canonical r }) : Except IntError Int := decode (view r.val)

theorem decodeAnswer_injective : Function.Injective decodeAnswer := by
  intro ⟨⟨ta, va⟩, ha⟩ ⟨⟨tb, vb⟩, hb⟩ h
  have hv := decode_injective h
  simp only [Canonical] at ha hb
  rcases ha with ha | ⟨ha, hza⟩ | ⟨ha, hza⟩ <;>
  rcases hb with hb | ⟨hb, hzb⟩ | ⟨hb, hzb⟩ <;>
    simp_all [view, ResultView.ok.injEq]

theorem checkedOp_canonical (op a b : UInt64) : Canonical (checkedOp op a b).run := by
  unfold checkedOp
  repeat' split
  all_goals dsimp [checkedAdd, checkedSub, checkedMul, checkedDiv, checkedRem, Id.run, pure, bind, Functor.map]
  all_goals repeat' split
  all_goals simp [Canonical]

theorem checked_answer_refines (op : IntOp) (a b : Int) (ha : Representable a) (hb : Representable b) :
    decodeAnswer ⟨(checkedOp (opCode op) (encode a) (encode b)).run,
      checkedOp_canonical (opCode op) (encode a) (encode b)⟩ = expected op a b :=
  checked_refines op a b ha hb

#print axioms checked_answer_refines
#print axioms decodeAnswer_injective
#print axioms checked_refines
#print axioms core_refines

end MillipedeArith
