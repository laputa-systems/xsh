import Xsh.Validation.Boundary
import Lean.Elab.Tactic.Omega

/-!
Same-base interval containment is an independent mathematical relation here:
it transports a bounded value to a containing interval while preserving its
payload. It does not model XSH assignability, where a validated value may flow
to its base or eligible structural views, but distinct validations remain
distinct types even when their bounded intervals are nested. Narrowing checks
membership in the smaller interval and can reject a value. These laws do not
establish checker, runtime, or Rust correspondence.
-/

namespace Xsh.Validation.BoundedRefinements

/-- The target contains the source interval and keeps its integer base. -/
def widensTo (smaller larger : BoundedSpec) : Prop :=
  smaller.base = larger.base ∧ larger.low ≤ smaller.low ∧
    smaller.high ≤ larger.high

/-- Widening changes only the accepted interval, never the integer payload. -/
def widen {smaller larger : BoundedSpec} (nested : widensTo smaller larger)
    (value : BoundedValue smaller) : BoundedValue larger :=
  ⟨value.val, by
    rcases nested with ⟨sameBase, lowerBound, upperBound⟩
    rcases value.property with ⟨valueAboveLow, valueBelowHigh, baseAccepted⟩
    refine ⟨?_, ?_, ?_⟩
    · omega
    · omega
    · rw [← sameBase]
      exact baseAccepted⟩

theorem widen_preserves_exact_payload {smaller larger : BoundedSpec}
    (nested : widensTo smaller larger) (value : BoundedValue smaller) :
    eraseBounded (widen nested value) = eraseBounded value := rfl

theorem widening_composes {smaller middle larger : BoundedSpec}
    (first : widensTo smaller middle) (second : widensTo middle larger) :
    widensTo smaller larger := by
  rcases first with ⟨sameBase₁, lower₁, upper₁⟩
  rcases second with ⟨sameBase₂, lower₂, upper₂⟩
  exact ⟨sameBase₁.trans sameBase₂, by omega, by omega⟩

theorem widen_composition {smaller middle larger : BoundedSpec}
    (first : widensTo smaller middle) (second : widensTo middle larger)
    (value : BoundedValue smaller) :
    widen second (widen first value) = widen (widening_composes first second) value := by
  apply Subtype.ext
  rfl

/-- Narrowing rechecks a value against a nested smaller interval. -/
def narrow {smaller larger : BoundedSpec} (_nested : widensTo smaller larger)
    (value : BoundedValue larger) : RequireOutcome (BoundedValue smaller) :=
  requireBounded smaller value.val

theorem narrow_rejects_outside_smaller {smaller larger : BoundedSpec}
    (nested : widensTo smaller larger) (value : BoundedValue larger)
    (outside : ¬ smaller.accepts value.val.val) :
    narrow nested value = .rejected := by
  exact require_bounded_rejects smaller value.val outside

def zeroToTenRange : BoundedSpec :=
  uintRange 0 10 (by decide)
    (by unfold Xsh.Values.signed64Limit; omega)
    (by unfold Xsh.Values.signed64Limit; omega)

def zeroTo256Range : BoundedSpec :=
  uintRange 0 256 (by decide)
    (by unfold Xsh.Values.signed64Limit; omega)
    (by unfold Xsh.Values.signed64Limit; omega)

theorem zero_to_ten_widens_to_byte : widensTo zeroToTenRange byteRange := by
  exact ⟨rfl, by decide, by decide⟩

def tenIntValue : Xsh.Values.IntValue :=
  ⟨10, by unfold Xsh.Values.signed64Min Xsh.Values.signed64Limit; omega⟩

def tenInZeroToTen : BoundedValue zeroToTenRange :=
  introduceBounded zeroToTenRange tenIntValue (by
    change zeroToTenRange.accepts zeroToTenRange.high
    exact bounded_high_is_accepted zeroToTenRange)

theorem zero_to_ten_widening_witness_preserves_ten :
    eraseBounded (widen zero_to_ten_widens_to_byte tenInZeroToTen) = tenIntValue := rfl

theorem byte_is_nested_in_zero_to_256 : widensTo byteRange zeroTo256Range := by
  exact ⟨rfl, by decide, by decide⟩

def int256Value : Xsh.Values.IntValue :=
  ⟨256, by unfold Xsh.Values.signed64Min Xsh.Values.signed64Limit; omega⟩

def int256InZeroTo256 : BoundedValue zeroTo256Range :=
  introduceBounded zeroTo256Range int256Value (by
    change zeroTo256Range.accepts zeroTo256Range.high
    exact bounded_high_is_accepted zeroTo256Range)

theorem narrowing_256_to_byte_rejects :
    narrow byte_is_nested_in_zero_to_256 int256InZeroTo256 = .rejected := by
  apply narrow_rejects_outside_smaller
  exact byte_range_rejects_half_open_upper

/-- A faulty widening check that forgets to compare the upper bounds. -/
def dropsHighBoundMutation (smaller larger : BoundedSpec) : Prop :=
  smaller.base = larger.base ∧ larger.low ≤ smaller.low

theorem dropping_high_bound_mutation_is_detected :
    dropsHighBoundMutation zeroTo256Range byteRange ∧
      ¬ widensTo zeroTo256Range byteRange ∧
      zeroTo256Range.accepts 256 ∧ ¬ byteRange.accepts 256 := by
  refine ⟨⟨rfl, by decide⟩, ?_, ?_, byte_range_rejects_half_open_upper⟩
  · intro validWidening
    have upper := validWidening.2.2
    change (256 : Int) ≤ 255 at upper
    omega
  · change zeroTo256Range.accepts zeroTo256Range.high
    exact bounded_high_is_accepted zeroTo256Range

end Xsh.Validation.BoundedRefinements
