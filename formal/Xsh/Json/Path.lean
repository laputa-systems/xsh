import Xsh.Values.Scalars

/-!
A finite JSON-tree path model for key and nonnegative-index traversal.
Records and Maps remain distinct tree constructors. This fragment omits
parsing, encoding, diagnostics, validation, and runtime correspondence.
Object fields use first-match association-list lookup; key uniqueness and the
source integer-width bound are outside this model. Float leaves retain raw
bits with an all-ones exponent excluded, so neither infinities nor NaNs enter
the JSON-compatible value domain. Non-Str Map-key validation is outside this
model.
-/

namespace Xsh.Json.Path

open Xsh.Values

/-- A finite JSON Float is a raw IEEE-754 bit pattern with a finite exponent. -/
def isFiniteFloat (bits : FloatBits) : Prop :=
  floatExponent bits < floatExponentLimit - 1

/-- JSON-compatible Float payloads exclude every all-ones exponent. -/
abbrev FiniteFloat := { bits : FloatBits // isFiniteFloat bits }

/-- The exact IEEE-754 encoding for the finite value `1.0`. -/
def finiteFloatOne : FiniteFloat :=
  ⟨⟨1023 * floatFractionWidth, by decide⟩, by
    unfold isFiniteFloat
    decide
  ⟩

/-- A JSON-compatible Float cannot have an all-ones exponent. -/
theorem finite_float_excludes_all_ones_exponent (value : FiniteFloat) :
    floatExponent value.val ≠ floatExponentLimit - 1 := by
  have finite := value.property
  change floatExponent value.val < floatExponentLimit - 1 at finite
  omega

/-- A JSON tree keeps object shape and distinguishes a present null value. -/
inductive Value where
  | null
  | boolean (value : Bool)
  | integer (value : Int)
  | string (value : String)
  | float (bits : FiniteFloat)
  | array (items : List Value)
  | record (fields : List (String × Value))
  | map (fields : List (String × Value))

/-- Paths select object keys or already-normalized nonnegative indices. -/
inductive Segment where
  | key (value : String)
  | index (value : Nat)
  deriving DecidableEq, Repr

abbrev Path := List Segment

/-- Negative integer indices do not enter the path domain. -/
def indexSegment? (value : Int) : Option Segment :=
  if value < 0 then none else some (.index value.toNat)

/-- Read the first matching field in the finite object representation. -/
def lookupField? : List (String × Value) → String → Option Value
  | [], _ => none
  | (candidate, value) :: rest, key =>
      if candidate == key then some value else lookupField? rest key

/-- Replace the first matching field or append a new field when it is absent. -/
def setField : List (String × Value) → String → Value → List (String × Value)
  | [], key, value => [(key, value)]
  | (candidate, oldValue) :: rest, key, value =>
      if candidate == key then (candidate, value) :: rest
      else (candidate, oldValue) :: setField rest key value

/-- Remove the first matching field, reporting absence separately. -/
def removeField? : List (String × Value) → String → Option (List (String × Value))
  | [], _ => none
  | (candidate, value) :: rest, key =>
      if candidate == key then some rest
      else (removeField? rest key).map (fun fields => (candidate, value) :: fields)

/-- Read an existing list position. -/
def lookupIndex? : List Value → Nat → Option Value
  | [], _ => none
  | value :: _, 0 => some value
  | _ :: rest, Nat.succ index => lookupIndex? rest index

/-- Replace an existing list position without extending the list. -/
def replaceIndex? : List Value → Nat → Value → Option (List Value)
  | [], _, _ => none
  | _ :: rest, 0, value => some (value :: rest)
  | first :: rest, Nat.succ index, value =>
      (replaceIndex? rest index value).map (fun updated => first :: updated)

/-- Remove an existing list position and close the gap to its right. -/
def removeIndex? : List Value → Nat → Option (List Value)
  | [], _ => none
  | _ :: rest, 0 => some rest
  | first :: rest, Nat.succ index =>
      (removeIndex? rest index).map (fun updated => first :: updated)

private theorem replaceIndex_preserves_length (items : List Value) (index : Nat)
    (replacement : Value) (updated : List Value)
    (replaced : replaceIndex? items index replacement = some updated) :
    updated.length = items.length := by
  induction items generalizing index updated with
  | nil => simp [replaceIndex?] at replaced
  | cons first rest inductionHypothesis =>
      cases index with
      | zero =>
          simp [replaceIndex?] at replaced
          cases replaced
          rfl
      | succ index =>
          simp only [replaceIndex?] at replaced
          cases inner : replaceIndex? rest index replacement with
          | none => simp [inner] at replaced
          | some tail =>
              simp [inner] at replaced
              cases replaced
              simpa using inductionHypothesis index tail inner

/-- Read one child of a JSON tree. -/
def child? (value : Value) (segment : Segment) : Option Value :=
  match value, segment with
  | .record fields, .key key => lookupField? fields key
  | .map fields, .key key => lookupField? fields key
  | .array items, .index index => lookupIndex? items index
  | _, _ => none

/-- Return the selected subtree, with `none` denoting a missing step. -/
def getAtPath (value : Value) : Path → Option Value
  | [] => some value
  | segment :: rest => do
      let child ← child? value segment
      getAtPath child rest

/-- Rebuild one path, inserting or replacing only an object leaf. -/
def setAtPath (value : Value) : Path → Value → Option Value
  | [], replacement => some replacement
  | .key key :: rest, replacement =>
      match value with
      | .record fields =>
          match rest with
          | [] => some (.record (setField fields key replacement))
          | _ :: _ => do
              let child ← lookupField? fields key
              let updated ← setAtPath child rest replacement
              pure (.record (setField fields key updated))
      | .map fields =>
          match rest with
          | [] => some (.map (setField fields key replacement))
          | _ :: _ => do
              let child ← lookupField? fields key
              let updated ← setAtPath child rest replacement
              pure (.map (setField fields key updated))
      | _ => none
  | .index index :: rest, replacement =>
      match value with
      | .array items =>
          match rest with
          | [] => (replaceIndex? items index replacement).map Value.array
          | _ :: _ => do
              let child ← lookupIndex? items index
              let updated ← setAtPath child rest replacement
              let items ← replaceIndex? items index updated
              pure (.array items)
      | _ => none

/-- Remove a selected subtree; removing the root yields JSON null. -/
def removeAtPath (value : Value) : Path → Option Value
  | [] => some .null
  | .key key :: rest =>
      match value with
      | .record fields =>
          match rest with
          | [] => (removeField? fields key).map Value.record
          | _ :: _ => do
              let child ← lookupField? fields key
              let updated ← removeAtPath child rest
              pure (.record (setField fields key updated))
      | .map fields =>
          match rest with
          | [] => (removeField? fields key).map Value.map
          | _ :: _ => do
              let child ← lookupField? fields key
              let updated ← removeAtPath child rest
              pure (.map (setField fields key updated))
      | _ => none
  | .index index :: rest =>
      match value with
      | .array items =>
          match rest with
          | [] => (removeIndex? items index).map Value.array
          | _ :: _ => do
              let child ← lookupIndex? items index
              let updated ← removeAtPath child rest
              let items ← replaceIndex? items index updated
              pure (.array items)
      | _ => none

/-- Every nonnegative integer has a natural-index path representation. -/
theorem nonnegative_integer_becomes_index (value : Int) (nonnegative : 0 ≤ value) :
    indexSegment? value = some (.index value.toNat) := by
  have notNegative : ¬ value < 0 := by omega
  simp [indexSegment?, notNegative]

/-- A negative integer is rejected before path traversal. -/
theorem negative_integer_is_not_an_index (value : Int) (negative : value < 0) :
    indexSegment? value = none := by
  simp [indexSegment?, negative]

/-- An empty path reads the original root unchanged. -/
theorem get_empty_path_returns_root (value : Value) :
    getAtPath value [] = some value := by
  rfl

/-- An empty path replaces the entire root value. -/
theorem set_empty_path_replaces_root (value replacement : Value) :
    setAtPath value [] replacement = some replacement := by
  rfl

/-- Removing the root produces the model's null result. -/
theorem remove_empty_path_returns_null (value : Value) :
    removeAtPath value [] = some .null := by
  rfl

/-- A present null child is distinct from a missing object key. -/
theorem present_null_differs_from_absence :
    getAtPath (.record [("nil", .null)]) [.key "nil"] = some .null ∧
      getAtPath (.record [("nil", .null)]) [.key "missing"] = none := by
  simp [getAtPath, child?, lookupField?]

/-- A mixed key/index path descends through records, lists, and maps. -/
theorem mixed_path_reads_nested_value :
    getAtPath
      (.record [("rows", .array [
        .map [("cells", .array [.integer 4, .integer 9])]
      ])])
      [.key "rows", .index 0, .key "cells", .index 1] = some (.integer 9) := by
  simp [getAtPath, child?, lookupField?, lookupIndex?]

/-- Setting an object leaf replaces an old value or inserts an absent key. -/
theorem set_object_leaf_replaces_or_inserts :
    setAtPath (.record [("name", .string "old")]) [.key "name"] (.string "new") =
        some (.record [("name", .string "new")]) ∧
      setAtPath (.map []) [.key "name"] (.string "new") =
        some (.map [("name", .string "new")]) := by
  simp [setAtPath, setField]

/-- Setting and reading back a finite Float leaf preserves its raw payload. -/
theorem set_get_preserves_finite_float_leaf :
    (setAtPath (.record []) [.key "ratio"] (.float finiteFloatOne)).bind
        (fun updated => getAtPath updated [.key "ratio"]) =
      some (.float finiteFloatOne) := by
  simp [setAtPath, setField, getAtPath, child?, lookupField?]

/-- Recursive set needs every object key before the final leaf. -/
theorem set_requires_existing_intermediate_key (fields : List (String × Value))
    (key next : String) (replacement : Value)
    (absent : lookupField? fields key = none) :
    setAtPath (.record fields) (.key key :: .key next :: []) replacement = none := by
  simp [setAtPath, absent]

/-- Updating below a Record retains its outer Record constructor. -/
theorem nested_set_preserves_record_shape :
    setAtPath (.record [("child", .record [("value", .integer 1)])])
      [.key "child", .key "value"] (.integer 2) =
        some (.record [("child", .record [("value", .integer 2)])]) := by
  simp [setAtPath, setField, lookupField?]

/-- Updating below a Map retains its outer Map constructor. -/
theorem nested_set_preserves_map_shape :
    setAtPath (.map [("child", .map [("value", .integer 1)])])
      [.key "child", .key "value"] (.integer 2) =
        some (.map [("child", .map [("value", .integer 2)])]) := by
  simp [setAtPath, setField, lookupField?]

/-- Replacing an existing list element preserves the list's length. -/
theorem set_existing_list_index_preserves_length (items : List Value) (index : Nat)
    (replacement : Value) (updated : List Value)
    (replaced : replaceIndex? items index replacement = some updated) :
    updated.length = items.length := by
  exact replaceIndex_preserves_length items index replacement updated replaced

/-- Mutant that appends when a list index is exactly its current length. -/
def appendAtLengthMutation (items : List Value) (index : Nat) (value : Value) :
    Option (List Value) :=
  if index = items.length then some (items ++ [value])
  else replaceIndex? items index value

/-- The append-at-length mutation differs on a concrete nonempty list. -/
theorem append_at_length_mutation_detected :
    setAtPath (.array [.integer 1, .integer 2]) [.index 2] (.integer 3) = none ∧
      appendAtLengthMutation [.integer 1, .integer 2] 2 (.integer 3) =
        some [.integer 1, .integer 2, .integer 3] := by
  simp [setAtPath, replaceIndex?, appendAtLengthMutation]

/-- Removing a missing object leaf fails instead of changing the object. -/
theorem remove_missing_object_key_fails :
    removeAtPath (.record [("present", .integer 1)]) [.key "missing"] = none := by
  simp [removeAtPath, removeField?]

/-- Removing a list element shifts each concrete suffix element left. -/
theorem remove_list_element_shifts_suffix :
    removeAtPath (.array [.integer 10, .integer 20, .integer 30]) [.index 1] =
      some (.array [.integer 10, .integer 30]) := by
  simp [removeAtPath, removeIndex?]

/-- Removing a nested leaf preserves both enclosing container shapes. -/
theorem nested_remove_preserves_record_and_map_shapes :
    removeAtPath
      (.record [("child", .map [("gone", .null), ("kept", .integer 7)])])
      [.key "child", .key "gone"] =
        some (.record [("child", .map [("kept", .integer 7)])]) := by
  simp [removeAtPath, removeField?, setField, lookupField?]

end Xsh.Json.Path
