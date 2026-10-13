import Xsh.IR.Raw.Words

namespace Xsh.IR.Raw

open Xsh.Foundation

/-- The raw start and length words used by instruction data and pool ranges. -/
structure RawRange where
  start : DirectIndex
  len : Word32
  deriving DecidableEq, Repr

namespace RawRange

def startValue (range : RawRange) : Nat := range.start.value

def length (range : RawRange) : Nat := range.len.val

/-- Resolve a range only when its end is within the target table. -/
def resolve? (range : RawRange) (total : Nat) : Option (Nat × Nat) :=
  let start := range.startValue
  let finish := start + range.length
  if finish ≤ total then some (start, finish) else none

theorem resolve_sound (range : RawRange) (total start finish : Nat)
    (resolved : resolve? range total = some (start, finish)) :
    start = range.startValue ∧ finish = range.startValue + range.length ∧ finish ≤ total := by
  unfold resolve? at resolved
  by_cases within : range.startValue + range.length ≤ total
  · simp [within] at resolved
    rcases resolved with ⟨startEq, finishEq⟩
    rw [← startEq, ← finishEq]
    exact ⟨rfl, rfl, within⟩
  · simp [within] at resolved

theorem resolve_complete (range : RawRange) (total : Nat)
    (within : range.startValue + range.length ≤ total) :
    resolve? range total = some (range.startValue, range.startValue + range.length) := by
  simp [resolve?, within]

theorem resolve_bounds (range : RawRange) (total start finish : Nat)
    (resolved : resolve? range total = some (start, finish)) :
    start ≤ finish ∧ finish ≤ total := by
  obtain ⟨rfl, rfl, within⟩ := resolve_sound range total start finish resolved
  exact ⟨Nat.le_add_right _ _, within⟩

end RawRange

end Xsh.IR.Raw
