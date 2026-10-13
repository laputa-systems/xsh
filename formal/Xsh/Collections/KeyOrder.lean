import Xsh.Values.Scalars

/-!
Primitive keys have separate domains and compare only with values from the
same domain. This fragment models numeric, Boolean, string, byte, native-path,
and duration order. It does not model map or set operations, key validation,
or correspondence with the runtime.
-/

namespace Xsh.Collections.KeyOrder

/-- The primitive domains allowed as map and set keys. -/
inductive KeyDomain where
  | int
  | uint
  | bool
  | string
  | bytes
  | path
  | duration
  deriving DecidableEq, Repr

/-- Values are indexed by key domain, so unlike domains cannot be compared. -/
abbrev Key : KeyDomain → Type
  | .int => Xsh.Values.IntValue
  | .uint => Xsh.Values.UIntValue
  | .bool => Bool
  | .string => String
  | .bytes => Xsh.Values.Bytes
  | .path => Xsh.Values.NativePath
  | .duration => Xsh.Values.DurationValue

/-- Compare keys using their domain's specified order. -/
def keyLE : (domain : KeyDomain) → Key domain → Key domain → Prop
  | .int, left, right => left.val ≤ right.val
  | .uint, left, right => left.val ≤ right.val
  | .bool, left, right => left = false ∨ right = true
  | .string, left, right => left ≤ right
  | .bytes, left, right => left ≤ right
  | .path, left, right => left.bytes ≤ right.bytes
  | .duration, left, right => left.val ≤ right.val

/-- Every primitive key is no greater than itself. -/
theorem key_order_reflexive (domain : KeyDomain) (key : Key domain) :
    keyLE domain key key := by
  cases domain with
  | int => exact Int.le_refl _
  | uint => exact Int.le_refl _
  | bool => cases key <;> simp [keyLE]
  | string => exact String.le_refl _
  | bytes => exact List.le_refl _
  | path => exact List.le_refl _
  | duration => exact Nat.le_refl _

/-- Mutual key ordering implies equality within the selected domain. -/
theorem key_order_antisymmetric (domain : KeyDomain) {left right : Key domain}
    (leftBeforeRight : keyLE domain left right)
    (rightBeforeLeft : keyLE domain right left) :
    left = right := by
  cases domain with
  | int =>
    apply Subtype.ext
    exact Int.le_antisymm leftBeforeRight rightBeforeLeft
  | uint =>
    apply Subtype.ext
    exact Int.le_antisymm leftBeforeRight rightBeforeLeft
  | bool => cases left <;> cases right <;> simp_all [keyLE]
  | string => exact String.le_antisymm leftBeforeRight rightBeforeLeft
  | bytes => exact List.le_antisymm leftBeforeRight rightBeforeLeft
  | path =>
    cases left with
    | mk leftBytes leftNoNul =>
      cases right with
      | mk rightBytes rightNoNul =>
        have sameBytes : leftBytes = rightBytes :=
          List.le_antisymm leftBeforeRight rightBeforeLeft
        cases sameBytes
        rfl
  | duration =>
    apply Subtype.ext
    exact Nat.le_antisymm leftBeforeRight rightBeforeLeft

/-- Key ordering is transitive within every primitive domain. -/
theorem key_order_transitive (domain : KeyDomain)
    {first second third : Key domain}
    (firstBeforeSecond : keyLE domain first second)
    (secondBeforeThird : keyLE domain second third) :
    keyLE domain first third := by
  cases domain with
  | int => exact Int.le_trans firstBeforeSecond secondBeforeThird
  | uint => exact Int.le_trans firstBeforeSecond secondBeforeThird
  | bool => cases first <;> cases second <;> cases third <;> simp_all [keyLE]
  | string => exact String.le_trans firstBeforeSecond secondBeforeThird
  | bytes => exact List.le_trans firstBeforeSecond secondBeforeThird
  | path =>
    exact List.le_trans firstBeforeSecond secondBeforeThird
  | duration => exact Nat.le_trans firstBeforeSecond secondBeforeThird

/-- Integer keys use their signed numeric values for ordering. -/
theorem int_order_is_numeric (left right : Key .int) :
    keyLE .int left right ↔ left.val ≤ right.val := Iff.rfl

/-- Unsigned integer keys use their nonnegative numeric values for ordering. -/
theorem uint_order_is_numeric (left right : Key .uint) :
    keyLE .uint left right ↔ left.val ≤ right.val := Iff.rfl

/-- Boolean keys place `false` before `true`. -/
theorem bool_false_precedes_true : keyLE .bool false true := by
  exact Or.inl rfl

/-- String order is lexicographic over Unicode scalar values. -/
theorem string_order_is_lexical (left right : Key .string) :
    keyLE .string left right ↔
      ¬ List.Lex (· < ·) right.toList left.toList := Iff.rfl

/-- Byte sequences use lexicographic unsigned-byte order. -/
theorem bytes_order_is_lexical (left right : Key .bytes) :
    keyLE .bytes left right ↔ ¬ List.Lex (· < ·) right left := Iff.rfl

/-- Native paths compare exactly their stored bytes, including non-UTF-8 bytes. -/
theorem path_order_is_exact_native_bytes (left right : Key .path) :
    keyLE .path left right ↔
      ¬ List.Lex (· < ·) right.bytes left.bytes := Iff.rfl

/-- Duration keys are ordered by their millisecond counts. -/
theorem duration_order_is_milliseconds (left right : Key .duration) :
    keyLE .duration left right ↔ left.val ≤ right.val := Iff.rfl

/-- The faulty reverse Boolean comparator used to check the mutation witness. -/
def reversedBooleanOrder (left right : Bool) : Prop :=
  keyLE .bool right left

/-- A concrete pair distinguishes false-before-true from reversed Boolean order. -/
theorem reversed_boolean_order_mutation_detected :
    keyLE .bool false true ∧ ¬ reversedBooleanOrder false true := by
  constructor
  · exact Or.inl rfl
  · simp [reversedBooleanOrder, keyLE]

end Xsh.Collections.KeyOrder
