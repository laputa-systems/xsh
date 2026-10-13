import Xsh.Values.Scalars
import Lean.Elab.Tactic.Omega

/-!
An independent model of the `NonEmpty`, `UInt`, and bounded-integer validation
boundaries. Validated values retain their base value; the proof is carried in
Lean's subtype only. `RequireOutcome.rejected` records a recoverable validation
miss, without modeling XSH error payloads or runtime dispatch from `Any`.

This fragment proves the boundary predicates and representative
introduction/require invariants. It does not model persistent stores, the
remaining validated types, source checking, Rust execution, or conformance.
-/

namespace Xsh.Validation

universe u v

inductive RequireOutcome (α : Type u) : Type u where
  | accepted (value : α)
  | rejected

abbrev NonEmptyList (α : Type u) := { values : List α // values ≠ [] }

namespace NonEmptyList

theorem append_ne_nil_right {α : Type u} (before after : List α) (after_nonempty : after ≠ []) :
    before ++ after ≠ [] := by
  cases before with
  | nil => simpa using after_nonempty
  | cons head tail => simp

theorem append_ne_nil_left {α : Type u} (before after : List α) (before_nonempty : before ≠ []) :
    before ++ after ≠ [] := by
  cases before with
  | nil => simp at before_nonempty
  | cons head tail => simp

def erase {α : Type u} (value : NonEmptyList α) : List α := value.val

def fromHead {α : Type u} (head : α) (tail : List α) : NonEmptyList α :=
  ⟨head :: tail, by simp⟩

def fromExplicit {α : Type u} (before : List α) (head : α) (after : List α) :
    NonEmptyList α :=
  ⟨before ++ (head :: after), append_ne_nil_right before (head :: after) (by simp)⟩

def fromSplice {α : Type u} (before : List α) (spread : NonEmptyList α)
    (after : List α) : NonEmptyList α :=
  let throughSpread := before ++ spread.val
  ⟨throughSpread ++ after,
    append_ne_nil_left throughSpread after
      (append_ne_nil_right before spread.val spread.property)⟩

def require {α : Type u} : List α → RequireOutcome (NonEmptyList α)
  | [] => .rejected
  | head :: tail => .accepted (fromHead head tail)

def map {α : Type u} {β : Type v} (f : α → β) (value : NonEmptyList α) :
    NonEmptyList β :=
  ⟨value.val.map f, by
    cases hvalues : value.val with
    | nil => exact value.property hvalues
    | cons head tail => simp [hvalues]⟩

def append {α : Type u} (value : NonEmptyList α) (tail : List α) : NonEmptyList α :=
  ⟨value.val ++ tail, by
    cases hvalues : value.val with
    | nil => exact value.property hvalues
    | cons head rest => simp [hvalues]⟩

def prepend {α : Type u} (before : List α) (value : NonEmptyList α) : NonEmptyList α :=
  ⟨before ++ value.val, append_ne_nil_right before value.val value.property⟩

def eraseFiltered {α : Type u} (value : NonEmptyList α) : List α :=
  value.val.filter (fun _ => False)

theorem fromHead_erases_to_literal {α : Type u} (head : α) (tail : List α) :
    erase (fromHead head tail) = head :: tail := rfl

theorem fromExplicit_erases_to_literal {α : Type u} (before : List α) (head : α)
    (after : List α) :
    erase (fromExplicit before head after) = before ++ (head :: after) := rfl

theorem fromSplice_erases_to_literal {α : Type u} (before : List α)
    (spread : NonEmptyList α) (after : List α) :
    erase (fromSplice before spread after) = (before ++ spread.val) ++ after := rfl

theorem require_accepts_nonempty {α : Type u} {values : List α} (nonempty : values ≠ []) :
    ∃ checked, require values = .accepted checked ∧ erase checked = values := by
  cases values with
  | nil => contradiction
  | cons head tail => exact ⟨fromHead head tail, rfl, rfl⟩

theorem require_rejects_empty {α : Type u} : require ([] : List α) = .rejected := rfl

theorem require_success_preserves_input {α : Type u} {values : List α}
    {checked : NonEmptyList α} (success : require values = .accepted checked) :
    erase checked = values := by
  cases values with
  | nil => cases success
  | cons head tail => cases success; rfl

theorem require_success_proves_nonempty {α : Type u} {values : List α}
    {checked : NonEmptyList α} (success : require values = .accepted checked) :
    values ≠ [] := by
  rw [← require_success_preserves_input success]
  exact checked.property

theorem require_rejected_iff_empty {α : Type u} (values : List α) :
    require values = .rejected ↔ values = [] := by
  cases values <;> simp [require]

theorem map_preserves_length {α : Type u} {β : Type v} (f : α → β)
    (value : NonEmptyList α) : (map f value).val.length = value.val.length := by
  simp [map]

theorem append_preserves_nonempty {α : Type u} (value : NonEmptyList α)
    (tail : List α) : (append value tail).val ≠ [] := (append value tail).property

theorem prepend_preserves_nonempty {α : Type u} (before : List α)
    (value : NonEmptyList α) : (prepend before value).val ≠ [] :=
  (prepend before value).property

theorem filtering_can_remove_every_element {α : Type u} (value : NonEmptyList α) :
    eraseFiltered value = [] := by
  simp [eraseFiltered]

end NonEmptyList

inductive IntegerBase where
  | int
  | uint
  deriving DecidableEq

def IntegerBase.accepts : IntegerBase → Int → Prop
  | .int, value => Xsh.Values.signed64Min ≤ value ∧ value < Xsh.Values.signed64Limit
  | .uint, value => 0 ≤ value ∧ value < Xsh.Values.signed64Limit

structure BoundedSpec where
  base : IntegerBase
  low : Int
  high : Int
  nonempty : low ≤ high
  low_in_base : base.accepts low
  high_in_base : base.accepts high

def BoundedSpec.accepts (spec : BoundedSpec) (value : Int) : Prop :=
  spec.low ≤ value ∧ value ≤ spec.high ∧ spec.base.accepts value

instance (spec : BoundedSpec) (value : Int) : Decidable (spec.accepts value) := by
  cases spec with
  | mk base low high nonempty low_in_base high_in_base =>
      cases base <;> dsimp [BoundedSpec.accepts, IntegerBase.accepts] <;> infer_instance

abbrev BoundedValue (spec : BoundedSpec) :=
  { value : Xsh.Values.IntValue // spec.accepts value.val }

def introduceBounded (spec : BoundedSpec) (value : Xsh.Values.IntValue)
    (in_range : spec.accepts value.val) : BoundedValue spec :=
  ⟨value, in_range⟩

def eraseBounded {spec : BoundedSpec} (value : BoundedValue spec) : Xsh.Values.IntValue :=
  value.val

def requireBounded (spec : BoundedSpec) (value : Xsh.Values.IntValue) :
    RequireOutcome (BoundedValue spec) :=
  if in_range : spec.accepts value.val then
    .accepted (introduceBounded spec value in_range)
  else
    .rejected

theorem bounded_low_is_accepted (spec : BoundedSpec) : spec.accepts spec.low := by
  exact ⟨by omega, spec.nonempty, spec.low_in_base⟩

theorem bounded_high_is_accepted (spec : BoundedSpec) : spec.accepts spec.high := by
  exact ⟨spec.nonempty, by omega, spec.high_in_base⟩

theorem introduce_bounded_erases_exactly (spec : BoundedSpec)
    (value : Xsh.Values.IntValue) (in_range : spec.accepts value.val) :
    (eraseBounded (introduceBounded spec value in_range)).val = value.val := rfl

theorem require_bounded_accepts (spec : BoundedSpec) (value : Xsh.Values.IntValue)
    (in_range : spec.accepts value.val) :
    requireBounded spec value = .accepted (introduceBounded spec value in_range) := by
  unfold requireBounded
  exact dif_pos in_range

theorem require_bounded_rejects (spec : BoundedSpec) (value : Xsh.Values.IntValue)
    (outside : ¬ spec.accepts value.val) : requireBounded spec value = .rejected := by
  unfold requireBounded
  exact dif_neg outside

theorem require_bounded_success_preserves_input (spec : BoundedSpec)
    (value : Xsh.Values.IntValue) (checked : BoundedValue spec)
    (success : requireBounded spec value = .accepted checked) :
    eraseBounded checked = value := by
  by_cases in_range : spec.accepts value.val
  · rw [require_bounded_accepts spec value in_range] at success
    injection success with same
    simpa [eraseBounded, introduceBounded] using (congrArg eraseBounded same).symm
  · rw [require_bounded_rejects spec value in_range] at success
    cases success

theorem require_bounded_success_is_in_range (spec : BoundedSpec)
    (value : Xsh.Values.IntValue) (checked : BoundedValue spec)
    (success : requireBounded spec value = .accepted checked) :
    spec.accepts value.val := by
  have same_value := require_bounded_success_preserves_input spec value checked success
  rw [← same_value]
  exact checked.property

theorem bounded_uint_values_remain_uint (spec : BoundedSpec) (value : BoundedValue spec)
    (is_uint : spec.base = .uint) :
    ∃ unsigned : Xsh.Values.UIntValue, unsigned.val = value.val.val := by
  rcases value.property with ⟨_, _, base_accepts⟩
  have nonnegative : 0 ≤ value.val.val := by
    rw [is_uint] at base_accepts
    exact base_accepts.1
  refine ⟨⟨value.val.val, ⟨nonnegative, value.val.property.2⟩⟩, rfl⟩

def intRange (low high : Int) (low_le_high : low ≤ high)
    (low_in_domain : Xsh.Values.signed64Min ≤ low ∧ low < Xsh.Values.signed64Limit)
    (high_in_domain : Xsh.Values.signed64Min ≤ high ∧ high < Xsh.Values.signed64Limit) :
    BoundedSpec :=
  ⟨.int, low, high, low_le_high, low_in_domain, high_in_domain⟩

def uintRange (low high : Int) (low_le_high : low ≤ high)
    (low_in_domain : 0 ≤ low ∧ low < Xsh.Values.signed64Limit)
    (high_in_domain : 0 ≤ high ∧ high < Xsh.Values.signed64Limit) : BoundedSpec :=
  ⟨.uint, low, high, low_le_high, low_in_domain, high_in_domain⟩

def portRange : BoundedSpec :=
  intRange 1 65535 (by decide)
    (by unfold Xsh.Values.signed64Min Xsh.Values.signed64Limit; omega)
    (by unfold Xsh.Values.signed64Min Xsh.Values.signed64Limit; omega)

def byteRange : BoundedSpec :=
  uintRange 0 255 (by decide)
    (by unfold Xsh.Values.signed64Limit; omega)
    (by unfold Xsh.Values.signed64Limit; omega)

theorem port_range_accepts_low : portRange.accepts 1 := by
  change portRange.accepts portRange.low
  exact bounded_low_is_accepted portRange

theorem port_range_accepts_high : portRange.accepts 65535 := by
  change portRange.accepts portRange.high
  exact bounded_high_is_accepted portRange

theorem port_range_rejects_below : ¬ portRange.accepts 0 := by
  intro accepted
  have lower : (1 : Int) ≤ 0 := by
    simpa [BoundedSpec.accepts, portRange, intRange] using accepted.1
  omega

theorem port_range_rejects_above : ¬ portRange.accepts 65536 := by
  intro accepted
  have upper : (65536 : Int) ≤ 65535 := by
    simpa [BoundedSpec.accepts, portRange, intRange] using accepted.2.1
  omega

theorem byte_range_accepts_upper : byteRange.accepts 255 := by
  change byteRange.accepts byteRange.high
  exact bounded_high_is_accepted byteRange

theorem byte_range_rejects_half_open_upper : ¬ byteRange.accepts 256 := by
  intro accepted
  have upper : (256 : Int) ≤ 255 := by
    simpa [BoundedSpec.accepts, byteRange, uintRange] using accepted.2.1
  omega

theorem byte_range_rejects_negative : ¬ byteRange.accepts (-1) := by
  intro accepted
  have lower : (0 : Int) ≤ -1 := by
    simpa [BoundedSpec.accepts, byteRange, uintRange] using accepted.1
  omega

def requireUInt (value : Xsh.Values.IntValue) : RequireOutcome Xsh.Values.UIntValue :=
  match Xsh.Values.validateUInt? value.val with
  | none => .rejected
  | some checked => .accepted checked

theorem require_uint_accepts_nonnegative (value : Xsh.Values.IntValue)
    (nonnegative : 0 ≤ value.val) :
    requireUInt value =
      .accepted ⟨value.val, ⟨nonnegative, value.property.2⟩⟩ := by
  simp [requireUInt, Xsh.Values.validateUInt?, nonnegative, value.property.2]

theorem require_uint_rejects_negative (value : Xsh.Values.IntValue)
    (negative : value.val < 0) : requireUInt value = .rejected := by
  have not_nonnegative : ¬ 0 ≤ value.val := by omega
  simp [requireUInt, Xsh.Values.validateUInt?, not_nonnegative]

theorem require_uint_success_preserves_input (value : Xsh.Values.IntValue)
    (checked : Xsh.Values.UIntValue) (success : requireUInt value = .accepted checked) :
    checked.val = value.val := by
  by_cases nonnegative : 0 ≤ value.val
  · rw [require_uint_accepts_nonnegative value nonnegative] at success
    injection success with same
    simpa using (congrArg Subtype.val same).symm
  · have negative : value.val < 0 := by omega
    rw [require_uint_rejects_negative value negative] at success
    cases success

theorem require_uint_rejected_iff_negative (value : Xsh.Values.IntValue) :
    requireUInt value = .rejected ↔ value.val < 0 := by
  by_cases nonnegative : 0 ≤ value.val
  · rw [require_uint_accepts_nonnegative value nonnegative]
    constructor
    · intro impossible
      cases impossible
    · intro negative
      omega
  · have negative : value.val < 0 := by omega
    rw [require_uint_rejects_negative value negative]
    simp [negative]

theorem require_uint_accepts_zero :
    requireUInt Xsh.Values.intZero = .accepted ⟨0, by decide⟩ := by
  apply require_uint_accepts_nonnegative
  decide

theorem require_uint_rejects_negative_one :
    requireUInt Xsh.Values.intNegativeOne = .rejected := by
  apply require_uint_rejects_negative
  change (-1 : Int) < 0
  omega

end Xsh.Validation
