namespace Xsh.Core.Arith

def minInt : Int := -(2 ^ 63)
def maxInt : Int := 2 ^ 63 - 1

def Representable (value : Int) : Prop := minInt ≤ value ∧ value ≤ maxInt

instance (value : Int) : Decidable (Representable value) := by
  unfold Representable; infer_instance

inductive IntOp where
  | add | sub | mul | div | rem
  deriving DecidableEq, Repr

inductive IntError where
  | divisionByZero | overflow
  deriving DecidableEq, Repr

def exact : IntOp → Int → Int → Int
  | .add, a, b => a + b
  | .sub, a, b => a - b
  | .mul, a, b => a * b
  | .div, a, b => a.tdiv b
  | .rem, a, b => a.tmod b

def witness : IntOp → Int → Int → Int
  | .rem, a, b => a.tdiv b
  | op, a, b => exact op a b

def checked (op : IntOp) (a b : Int) : Except IntError Int :=
  if (op = .div ∨ op = .rem) ∧ b = 0 then .error .divisionByZero
  else if Representable (witness op a b) then .ok (exact op a b)
  else .error .overflow

theorem checked_zero_divisor (op : IntOp) (a : Int) (divides : op = .div ∨ op = .rem) :
    checked op a 0 = .error .divisionByZero := by
  simp [checked, divides]

theorem checked_ok (op : IntOp) (a b value : Int) (accepted : checked op a b = .ok value) :
    value = exact op a b ∧ Representable (witness op a b) ∧ ((op = .div ∨ op = .rem) → b ≠ 0) := by
  unfold checked at accepted
  by_cases zero : (op = .div ∨ op = .rem) ∧ b = 0
  · simp [zero] at accepted
  · by_cases fits : Representable (witness op a b)
    · simp only [zero, fits, ite_false, ite_true] at accepted
      refine ⟨(Except.ok.inj accepted).symm, fits, ?_⟩
      intro divides
      exact fun hb => zero ⟨divides, hb⟩
    · simp [zero, fits] at accepted

theorem checked_overflow (op : IntOp) (a b : Int) :
    checked op a b = .error .overflow ↔
      ¬ ((op = .div ∨ op = .rem) ∧ b = 0) ∧ ¬ Representable (witness op a b) := by
  unfold checked
  by_cases zero : (op = .div ∨ op = .rem) ∧ b = 0
  · simp [zero]
  · by_cases fits : Representable (witness op a b) <;> simp [zero, fits]

end Xsh.Core.Arith
