import Xsh.Validation.RecursiveRequire

/-!
A conservative wire-enum extension around the existing recursive `.require`
boundary. The shared checker validates the portion representable by its current
Target grammar; this module handles Str-enum replacement and visits every
finite Map value. The wrapper publishes a converted value only when both the
shared check and the whole-tree conversion succeed. Maps here have Str keys.

Enum mappings are supplied as prepared data with unique wire spellings. This
model assumes each mapping is nonempty and its declaration names and wire
spellings are unique; it records those assumptions as structure proofs but
does not check source declarations. Structural rejection carries the existing
pathless `ValidationFailure`; conversion rejection retains the original
candidate and a `BlamePath`. This fragment excludes implicit coercions, JSON
encoding, diagnostic spans, and Rust correspondence.
-/

namespace Xsh.Enums.WireConversion

open Xsh.Values
open Xsh.Validation.Nominal
open Xsh.Validation.RecursiveRequire

/-- A prepared Str-backed enum mapping; declaration checking is outside this model. -/
structure StrEnumMapping where
  typeName : NominalId
  variants : List (String × String)
  variantsNonempty : variants ≠ []
  variantNamesUnique : (variants.map Prod.fst).Nodup
  wireSpellingsUnique : (variants.map Prod.snd).Nodup

/-- The input and output values needed at this conversion boundary. -/
inductive WireValue where
  | atom (value : CompositeValue)
  | list (values : List WireValue)
  | record (fields : List (String × WireValue))
  | map (entries : List (String × WireValue))
  | enumValue (typeName : NominalId) (variantName : String)

/-- A schema adds enum conversion and open Str-keyed maps to recursive require. -/
inductive WireSchema where
  | base (target : Target)
  | enumeration (mapping : StrEnumMapping)
  | optional (inner : WireSchema)
  | list (element : WireSchema)
  | record (fields : List (String × WireSchema))
  | map (value : WireSchema)

/- The shared checker has no Map target. This projection treats the map's Str
   keys as open record fields; the local conversion below checks every value. -/
mutual
  def requireTarget : WireSchema → Target
    | .base target => target
    | .enumeration _ => .string
    | .optional inner => .optional (requireTarget inner)
    | .list element => .list (requireTarget element)
    | .record fields => .record (requireFields fields)
    | .map _ => .record []

  def requireFields : List (String × WireSchema) → List Target
    | [] => []
    | (name, schema) :: fields =>
        .field name (requireTarget schema) :: requireFields fields
end

/- Project the finite wire tree into the shared validation value domain. A
   tagged enum becomes a placeholder Str only for structural checking; the
   conversion pass validates its declaration and variant before publishing it. -/
mutual
  def WireValue.toRequireValue : WireValue → CompositeValue
    | .atom value => value
    | .list values => .list (wireValuesToRequire values)
    | .record fields => .record (wireFieldsToRequire fields)
    | .map entries => .record (wireFieldsToRequire entries)
    | .enumValue _ _ => .string ""

  def wireValuesToRequire : List WireValue → List CompositeValue
    | [] => []
    | value :: values => value.toRequireValue :: wireValuesToRequire values

  def wireFieldsToRequire : List (String × WireValue) → List (String × CompositeValue)
    | [] => []
    | (name, value) :: fields =>
        (name, value.toRequireValue) :: wireFieldsToRequire fields
end

/-- The candidate keeps the original value and `.require` provenance. -/
structure WireCandidate where
  value : WireValue
  provenance : RequireInput

def WireCandidate.toRequireCandidate (candidate : WireCandidate) : Candidate :=
  ⟨candidate.value.toRequireValue, candidate.provenance⟩

/-- A path segment names the container edge containing the failing enum. -/
inductive PathSegment where
  | field (name : String)
  | index (value : Nat)
  | key (value : String)
  deriving DecidableEq, Repr

abbrev BlamePath := List PathSegment

inductive ConversionIssue where
  | unknownWireString (typeName : NominalId) (spelling : String)
  | expectedValue (expected : String)
  | fuelExhausted
  deriving DecidableEq, Repr

structure ConversionFailure where
  path : BlamePath
  issue : ConversionIssue
  deriving DecidableEq, Repr

def enumVariant? (mapping : StrEnumMapping) (spelling : String) : Option String :=
  (mapping.variants.find? fun (_, wire) => wire == spelling).map Prod.fst

def recordFieldSchema? (fields : List (String × WireSchema)) (name : String) :
    Option WireSchema :=
  (fields.find? fun (candidate, _) => candidate == name).map Prod.snd

def expectedSchemaName : WireSchema → String
  | .base .integer => "Int"
  | .base .string => "Str"
  | .base (.optional _) => "Optional"
  | .base (.list _) => "List"
  | .base (.field _ _) => "Field"
  | .base (.record _) => "Record"
  | .enumeration mapping => mapping.typeName.declarationName
  | .optional inner => s!"{expectedSchemaName inner}?"
  | .list _ => "List"
  | .record _ => "Record"
  | .map _ => "Map"

/-! `fuel` decreases at every schema edge. Container folds retain source order,
and record extras pass through unchanged as `.require` requires. -/
def convertFuel : Nat → WireSchema → WireValue → BlamePath →
    Except ConversionFailure WireValue
  | 0, _, _, path => .error ⟨path, .fuelExhausted⟩
  | Nat.succ fuel, schema, value, path =>
      match schema, value with
      | .base target, .atom atom =>
          if check (Nat.succ fuel) target atom then .ok (.atom atom)
          else .error ⟨path, .expectedValue (expectedSchemaName schema)⟩
      | .enumeration mapping, .atom (.string spelling) =>
          match enumVariant? mapping spelling with
          | some variant => .ok (.enumValue mapping.typeName variant)
          | none => .error ⟨path, .unknownWireString mapping.typeName spelling⟩
      | .enumeration mapping, .enumValue typeName variantName =>
          if typeName == mapping.typeName &&
              mapping.variants.any (fun (variant, _) => variant == variantName) then
            .ok value
          else
            .error ⟨path, .expectedValue mapping.typeName.declarationName⟩
      | .optional _, .atom .null => .ok (.atom .null)
      | .optional inner, other => convertFuel fuel inner other path
      | .list element, .list values => do
          let indexed := values.zip (List.range values.length)
          let converted ← indexed.mapM fun (item, index) =>
            convertFuel fuel element item (path ++ [.index index])
          pure (.list converted)
      | .record fields, .record actual => do
          let converted ← actual.mapM fun (name, item) => do
            match recordFieldSchema? fields name with
            | none => pure (name, item)
            | some fieldSchema => do
                let value ← convertFuel fuel fieldSchema item (path ++ [.field name])
                pure (name, value)
          pure (.record converted)
      | .map itemSchema, .map entries => do
          let converted ← entries.mapM fun (key, item) => do
            let value ← convertFuel fuel itemSchema item (path ++ [.key key])
            pure (key, value)
          pure (.map converted)
      | .base target, _ => .error ⟨path, .expectedValue (expectedSchemaName (.base target))⟩
      | .enumeration mapping, _ =>
          .error ⟨path, .expectedValue mapping.typeName.declarationName⟩
      | .list _, _ => .error ⟨path, .expectedValue "List"⟩
      | .record _, _ => .error ⟨path, .expectedValue "Record"⟩
      | .map _, _ => .error ⟨path, .expectedValue "Map"⟩

structure WireCheckedValue where
  sourceCheck : CheckedValue
  converted : WireValue

inductive WireFailure where
  | recursiveRequire (candidate : WireCandidate) (failure : ValidationFailure)
  | conversion (candidate : WireCandidate) (target : NominalId) (shape : Target)
      (failure : ConversionFailure)

inductive WireRequireResult where
  | accepted (value : WireCheckedValue)
  | rejected (failure : WireFailure)

/-- `.require` first checks its shared structural fragment. The additional
    converter must then finish every enum and map value before output exists. -/
def requireWire (fuel : Nat) (target : NominalId) (schema : WireSchema)
    (candidate : WireCandidate) : WireRequireResult :=
  let requireCandidate := candidate.toRequireCandidate
  let shape := requireTarget schema
  match _root_.Xsh.Validation.RecursiveRequire.require fuel target shape requireCandidate with
  | .rejected failure => .rejected (.recursiveRequire candidate failure)
  | .accepted checked =>
      match convertFuel fuel schema candidate.value [] with
      | .ok converted => .accepted ⟨checked, converted⟩
      | .error failure =>
          .rejected (.conversion candidate target shape failure)

/-- A faulty map path that returns entries without checking their values. -/
def skipMapValueValidationMutation (fuel : Nat) (schema : WireSchema)
    (value : WireValue) (path : BlamePath) : Except ConversionFailure WireValue :=
  match schema, value with
  | .map _, .map _ => .ok value
  | _, _ => convertFuel fuel schema value path

def stateEnumId : NominalId := ⟨"wire", "State"⟩

def stateMapping : StrEnumMapping :=
  ⟨stateEnumId, [("Ready", "ready"), ("Missing", "")], by decide, by decide, by decide⟩

def stateSchema : WireSchema := .enumeration stateMapping

def requireId : NominalId := stateEnumId

def nestedSchema : WireSchema := .record [
  ("state", stateSchema),
  ("attempts", .list (.base .integer)),
  ("fallback", .optional stateSchema),
  ("labels", .map stateSchema)
]

def nestedCandidate : WireCandidate := ⟨
  .record [
    ("state", .atom (.string "ready")),
    ("attempts", .list [.atom (.integer intOne), .atom (.integer intOne)]),
    ("fallback", .atom .null),
    ("labels", .map [("primary", .atom (.string "")), ("backup", .atom (.string "ready"))]),
    ("extra", .atom (.string "preserved"))
  ],
  .nominal ⟨"json", "WireState"⟩
⟩

def unknownMapCandidate : WireCandidate := ⟨
  .map [("accepted", .atom (.string "ready")), ("rejected", .atom (.string "unknown"))],
  .nonNominal
⟩

def nestedConverted : WireValue := .record [
  ("state", .enumValue stateEnumId "Ready"),
  ("attempts", .list [.atom (.integer intOne), .atom (.integer intOne)]),
  ("fallback", .atom .null),
  ("labels", .map [
    ("primary", .enumValue stateEnumId "Missing"),
    ("backup", .enumValue stateEnumId "Ready")
  ]),
  ("extra", .atom (.string "preserved"))
]

theorem wire_mapping_retains_unique_spellings :
    stateMapping.variants.map Prod.snd = ["ready", ""] ∧
      stateMapping.variants ≠ [] ∧
      (stateMapping.variants.map Prod.fst).Nodup ∧
      (stateMapping.variants.map Prod.snd).Nodup := by
  exact ⟨rfl, stateMapping.variantsNonempty,
    stateMapping.variantNamesUnique, stateMapping.wireSpellingsUnique⟩

theorem exact_wire_string_converts_to_its_variant :
    convertFuel 2 stateSchema (.atom (.string "ready")) [] =
      .ok (.enumValue stateEnumId "Ready") := by
  rfl

theorem unknown_wire_string_reports_root_path :
    convertFuel 2 stateSchema (.atom (.string "unknown")) [] =
      .error ⟨[], .unknownWireString stateEnumId "unknown"⟩ := by
  rfl

theorem optional_null_is_preserved :
    convertFuel 2 (.optional stateSchema) (.atom .null) [] =
      .ok (.atom .null) := by
  rfl

theorem optional_wire_string_converts :
    convertFuel 2 (.optional stateSchema) (.atom (.string "")) [] =
      .ok (.enumValue stateEnumId "Missing") := by
  rfl

theorem matching_enum_value_is_preserved :
    convertFuel 2 stateSchema (.enumValue stateEnumId "Ready") [] =
      .ok (.enumValue stateEnumId "Ready") := by
  rfl

theorem same_display_name_from_another_module_is_rejected :
    (⟨"other", "State"⟩ : NominalId) ≠ stateEnumId ∧
      convertFuel 2 stateSchema (.enumValue ⟨"other", "State"⟩ "Ready") [] =
        .error ⟨[], .expectedValue "State"⟩ := by
  exact ⟨by decide, by rfl⟩

theorem wire_string_matching_is_case_sensitive :
    convertFuel 2 stateSchema (.atom (.string "Ready")) [] =
      .error ⟨[], .unknownWireString stateEnumId "Ready"⟩ := by
  rfl

theorem list_values_convert_in_order :
    convertFuel 3 (.list stateSchema)
      (.list [.atom (.string "ready"), .atom (.string "")]) [] =
      .ok (.list [.enumValue stateEnumId "Ready", .enumValue stateEnumId "Missing"]) := by
  rfl

theorem record_conversion_preserves_extras_and_field_order :
    convertFuel 4 (.record [("state", stateSchema)])
      (.record [("extra", .atom (.string "keep")), ("state", .atom (.string "ready"))]) [] =
      .ok (.record [("extra", .atom (.string "keep")), ("state", .enumValue stateEnumId "Ready")]) := by
  rfl

theorem map_values_convert_and_keys_remain_ordered :
    convertFuel 3 (.map stateSchema)
      (.map [("primary", .atom (.string "")), ("backup", .atom (.string "ready"))]) [] =
      .ok (.map [("primary", .enumValue stateEnumId "Missing"),
        ("backup", .enumValue stateEnumId "Ready")]) := by
  rfl

theorem nested_failure_blames_field_then_index :
    convertFuel 4 (.record [("states", .list stateSchema)])
      (.record [("states", .list [.atom (.string "ready"), .atom (.string "bad")])]) [] =
      .error ⟨[.field "states", .index 1], .unknownWireString stateEnumId "bad"⟩ := by
  rfl

theorem unknown_map_value_blames_its_key :
    convertFuel 3 (.map stateSchema)
      (.map [("bad-key", .atom (.string "bad"))]) [] =
      .error ⟨[.key "bad-key"], .unknownWireString stateEnumId "bad"⟩ := by
  rfl

theorem require_wire_publishes_only_converted_nested_value :
    ∃ checked,
      requireWire 8 requireId nestedSchema nestedCandidate = .accepted checked ∧
      checked.converted = .record [
        ("state", .enumValue stateEnumId "Ready"),
        ("attempts", .list [.atom (.integer intOne), .atom (.integer intOne)]),
        ("fallback", .atom .null),
        ("labels", .map [("primary", .enumValue stateEnumId "Missing"),
          ("backup", .enumValue stateEnumId "Ready")]),
        ("extra", .atom (.string "preserved"))
      ] ∧ checked.sourceCheck.nominal.provenance = .require nestedCandidate.provenance := by
  let candidate := nestedCandidate.toRequireCandidate
  let shape := requireTarget nestedSchema
  let sourceChecked : CheckedValue := ⟨8, shape,
    introduceByRequire requireId candidate.provenance candidate.value, by decide⟩
  have sourcePasses : check 8 shape candidate.value = true := by decide
  have sourceAccepted :
      _root_.Xsh.Validation.RecursiveRequire.require 8 requireId shape candidate =
        .accepted sourceChecked := by
    unfold _root_.Xsh.Validation.RecursiveRequire.require
    exact dite_eq_left sourcePasses
  refine ⟨⟨sourceChecked, nestedConverted⟩, ?_, ?_, rfl⟩
  · unfold requireWire
    change (match _root_.Xsh.Validation.RecursiveRequire.require
        8 requireId shape candidate with
      | _root_.Xsh.Validation.RecursiveRequire.RequireResult.rejected failure =>
          WireRequireResult.rejected (.recursiveRequire nestedCandidate failure)
      | _root_.Xsh.Validation.RecursiveRequire.RequireResult.accepted checked =>
          WireRequireResult.accepted ⟨checked, nestedConverted⟩) =
        WireRequireResult.accepted ⟨sourceChecked, nestedConverted⟩
    simp only [sourceAccepted]
  · rfl

theorem skipping_map_value_validation_is_detected :
    convertFuel 3 (.map stateSchema) unknownMapCandidate.value [] =
        .error ⟨[.key "rejected"], .unknownWireString stateEnumId "unknown"⟩ ∧
      skipMapValueValidationMutation 3 (.map stateSchema) unknownMapCandidate.value [] =
        .ok unknownMapCandidate.value := by
  exact ⟨by rfl, by rfl⟩

theorem rejected_wire_conversion_retains_candidate_and_map_blame :
    requireWire 3 requireId (.map stateSchema) unknownMapCandidate =
      .rejected (.conversion unknownMapCandidate requireId
        (requireTarget (.map stateSchema))
        ⟨[.key "rejected"], .unknownWireString stateEnumId "unknown"⟩) := by
  let candidate := unknownMapCandidate.toRequireCandidate
  let shape := requireTarget (.map stateSchema)
  let sourceChecked : CheckedValue := ⟨3, shape,
    introduceByRequire requireId candidate.provenance candidate.value, by decide⟩
  have sourcePasses : check 3 shape candidate.value = true := by decide
  have sourceAccepted :
      _root_.Xsh.Validation.RecursiveRequire.require 3 requireId shape candidate =
        .accepted sourceChecked := by
    unfold _root_.Xsh.Validation.RecursiveRequire.require
    exact dite_eq_left sourcePasses
  unfold requireWire
  change (match _root_.Xsh.Validation.RecursiveRequire.require
      3 requireId shape candidate with
    | _root_.Xsh.Validation.RecursiveRequire.RequireResult.rejected failure =>
        WireRequireResult.rejected (.recursiveRequire unknownMapCandidate failure)
    | _root_.Xsh.Validation.RecursiveRequire.RequireResult.accepted checked =>
        WireRequireResult.rejected (.conversion unknownMapCandidate requireId shape
          ⟨[.key "rejected"], .unknownWireString stateEnumId "unknown"⟩)) =
      WireRequireResult.rejected (.conversion unknownMapCandidate requireId shape
        ⟨[.key "rejected"], .unknownWireString stateEnumId "unknown"⟩)
  simp only [sourceAccepted]

theorem rejected_conversion_exposes_no_converted_value :
    (match requireWire 3 requireId (.map stateSchema) unknownMapCandidate with
    | .accepted checked => some checked.converted
    | .rejected _ => none) = none := by
  rw [rejected_wire_conversion_retains_candidate_and_map_blame]

end Xsh.Enums.WireConversion
