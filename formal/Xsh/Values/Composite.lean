import Xsh.Values.Scalars
import Xsh.Conformance.Transport.Codecs

/-!
A focused value-domain fragment for scalar, list, record, and tagged values.
Structural identity preserves raw Float bits and record field order. Semantic
comparison applies IEEE Float equality recursively, treats records as named
fields, and requires unique record keys. This is an independent Lean model;
it does not establish Rust runtime or transport conformance.

The fragment omits maps, sets, streams, resources, modules, callables, errors,
nominal constructor provenance, and runtime identity. It does not define
coercions, mutation, or general source evaluation. Tagged values compare exact
type and variant names with positional payloads. Record lists with duplicate
keys are representable for structural comparison but are outside the semantic
record domain and do not compare semantically.
-/

namespace Xsh.Values

open Xsh.Conformance.Transport

/-- Composite runtime-value fragment; Float stores its exact 64-bit pattern. -/
inductive CompositeValue where
  | null
  | boolean (value : Bool)
  | integer (value : IntValue)
  | float (bits : FloatBits)
  | duration (value : DurationValue)
  | string (value : String)
  | bytes (value : Bytes)
  | list (values : List CompositeValue)
  | record (fields : List (String × CompositeValue))
  | tagged (typeName variantName : String) (fields : List CompositeValue)

/-- Exact structural identity, including Float payload bits and record order. -/
def representationIdentity (left right : CompositeValue) : Prop := left = right

def findRecordField? (key : String) : List (String × CompositeValue) → Option CompositeValue
  | [] => none
  | (candidate, value) :: rest =>
      if candidate == key then some value else findRecordField? key rest

mutual
  /-- Semantic equality for this fragment, with recursive IEEE Float comparison. -/
  def CompositeValue.semanticEq : CompositeValue → CompositeValue → Prop
    | .null, .null => True
    | .boolean left, .boolean right => left = right
    | .integer left, .integer right => left = right
    | .float left, .float right => ieeeFloatEq left right
    | .duration left, .duration right => left = right
    | .string left, .string right => left = right
    | .bytes left, .bytes right => left = right
    | .list left, .list right => listSemanticEq left right
    | .record left, .record right => recordSemanticEq left right
    | .tagged leftType leftVariant leftFields, .tagged rightType rightVariant rightFields =>
        leftType = rightType ∧ leftVariant = rightVariant ∧
          listSemanticEq leftFields rightFields
    | _, _ => False

  def listSemanticEq : List CompositeValue → List CompositeValue → Prop
    | [], [] => True
    | left :: leftRest, right :: rightRest =>
        left.semanticEq right ∧ listSemanticEq leftRest rightRest
    | _, _ => False

  def recordFieldsSemanticEq : List (String × CompositeValue) →
      List (String × CompositeValue) → Prop
    | [], _ => True
    | (key, value) :: rest, right =>
        match findRecordField? key right with
        | none => False
        | some otherValue => value.semanticEq otherValue ∧ recordFieldsSemanticEq rest right

  def recordSemanticEq (left right : List (String × CompositeValue)) : Prop :=
    (left.map Prod.fst).Nodup ∧
      (right.map Prod.fst).Nodup ∧
      left.length = right.length ∧ recordFieldsSemanticEq left right
end

/-- The bridge's u64 identity for a Float is its unchanged raw payload word. -/
def floatTransportIdentity (bits : FloatBits) : Word64 := ⟨bits.val, bits.isLt⟩

def floatTransportHex (bits : FloatBits) : String :=
  encodeHex64 (floatTransportIdentity bits)

def positiveZeroValue : CompositeValue := .float floatPositiveZero
def negativeZeroValue : CompositeValue := .float floatNegativeZero
def quietNaNValue : CompositeValue := .float floatQuietNaN

theorem signed_zero_representation_differs :
    ¬ representationIdentity positiveZeroValue negativeZeroValue := by
  intro same
  have bitsEqual := congrArg (fun value => match value with
    | .float bits => bits.val
    | _ => 0) same
  simp [positiveZeroValue, negativeZeroValue, floatPositiveZero, floatNegativeZero,
    floatSign] at bitsEqual

theorem signed_zero_semantically_equal :
    positiveZeroValue.semanticEq negativeZeroValue := by
  simpa [positiveZeroValue, negativeZeroValue, CompositeValue.semanticEq] using
    ieee_signed_zeros_equal

theorem same_bit_nan_representation_is_identical :
    representationIdentity quietNaNValue quietNaNValue := by
  rfl

theorem same_bit_nan_is_semantically_unequal :
    ¬ quietNaNValue.semanticEq quietNaNValue := by
  simpa [quietNaNValue, CompositeValue.semanticEq] using same_bit_nan_is_unequal

def signedZeroListLeft : CompositeValue := .list [
  .integer intOne,
  .float floatPositiveZero
]

def signedZeroListRight : CompositeValue := .list [
  .integer intOne,
  .float floatNegativeZero
]

theorem nested_list_uses_ieee_float_equality :
    signedZeroListLeft.semanticEq signedZeroListRight := by
  simp [signedZeroListLeft, signedZeroListRight, CompositeValue.semanticEq,
    listSemanticEq, ieee_signed_zeros_equal]

def signedZeroRecordLeft : CompositeValue := .record [
  ("count", .integer intOne),
  ("ratio", .float floatPositiveZero)
]

def signedZeroRecordRight : CompositeValue := .record [
  ("ratio", .float floatNegativeZero),
  ("count", .integer intOne)
]

theorem named_record_order_does_not_change_semantic_equality :
    signedZeroRecordLeft.semanticEq signedZeroRecordRight := by
  simp [signedZeroRecordLeft, signedZeroRecordRight, CompositeValue.semanticEq,
    recordSemanticEq, recordFieldsSemanticEq, findRecordField?, ieee_signed_zeros_equal]

theorem named_record_order_remains_structurally_distinct :
    ¬ representationIdentity signedZeroRecordLeft signedZeroRecordRight := by
  intro same
  have fieldsEqual := congrArg (fun value => match value with
    | .record fields => fields
    | _ => []) same
  simp [signedZeroRecordLeft, signedZeroRecordRight] at fieldsEqual

def taggedPositiveZero : CompositeValue :=
  .tagged "Measure" "Exact" [.float floatPositiveZero]

def taggedNegativeZero : CompositeValue :=
  .tagged "Measure" "Exact" [.float floatNegativeZero]

theorem tagged_payload_uses_ieee_float_equality :
    taggedPositiveZero.semanticEq taggedNegativeZero := by
  simp [taggedPositiveZero, taggedNegativeZero, CompositeValue.semanticEq,
    listSemanticEq, ieee_signed_zeros_equal]

theorem tagged_variant_identity_is_preserved :
    ¬ (CompositeValue.tagged "Measure" "Exact" [.integer intOne]).semanticEq
      (CompositeValue.tagged "Measure" "Approximate" [.integer intOne]) := by
  simp [CompositeValue.semanticEq]

theorem duplicate_key_records_are_outside_semantic_domain :
    ¬ (CompositeValue.record [("x", .integer intOne), ("x", .integer intOne)]).semanticEq
      (CompositeValue.record [("x", .integer intOne), ("x", .integer intOne)]) := by
  simp [CompositeValue.semanticEq, recordSemanticEq]

def sameBitNaNList : CompositeValue := .list [.float floatQuietNaN]

theorem nested_same_bit_nan_is_semantically_unequal :
    ¬ sameBitNaNList.semanticEq sameBitNaNList := by
  simp [sameBitNaNList, CompositeValue.semanticEq, listSemanticEq,
    same_bit_nan_is_unequal]

theorem float_transport_identity_is_injective {left right : FloatBits}
    (same : floatTransportIdentity left = floatTransportIdentity right) : left = right := by
  apply Fin.ext
  exact congrArg Fin.val same

theorem signed_zero_transport_bits_remain_distinct :
    floatTransportIdentity floatPositiveZero ≠ floatTransportIdentity floatNegativeZero := by
  decide

theorem signed_zero_transport_hex_remains_distinct :
    floatTransportHex floatPositiveZero ≠ floatTransportHex floatNegativeZero := by
  decide

theorem positive_zero_transport_hex_is_exact :
    floatTransportHex floatPositiveZero = "0000000000000000" := by
  decide

theorem negative_zero_transport_hex_is_exact :
    floatTransportHex floatNegativeZero = "8000000000000000" := by
  decide

end Xsh.Values
