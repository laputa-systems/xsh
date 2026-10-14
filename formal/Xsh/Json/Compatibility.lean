import Xsh.Json.Path
import Xsh.Collections.KeyOrder
import Xsh.Values.OrderedSet
import Xsh.Enums.WireConversion

/-!
A finite recursive projection from a bounded XSH value fragment to a JSON
tree. The source is finite by construction; record and map entry lists are
assumed unique by their caller, while this fragment checks that Map keys are
Str. Float leaves retain raw bits and require a finite exponent; enum mappings
are prepared with nonempty unique declaration names and unique wire spellings.
Sets use the shared finite ordinal key model, whose iteration is canonical key
order. A Path has no implicit projection: the `convertedPath` constructor
represents text supplied by an explicit conversion outside this fragment. This
model excludes text escaping, parsing, I/O, and runtime correspondence.
-/

namespace Xsh.Json.Compatibility

open Xsh.Values
open Xsh.Collections.KeyOrder
open Xsh.Enums.WireConversion

abbrev JsonTree := Xsh.Json.Path.Value

/-- Other primitive map-key domains remain distinct from Str keys. -/
inductive MapKey where
  | integer (value : Int)
  | boolean (value : Bool)
  | string (value : String)
  | bytes (value : Bytes)
  | path (value : Bytes)
  | duration (value : Nat)
  deriving DecidableEq, Repr

/-- Finite values accepted at the projection boundary or explicitly rejected. -/
inductive SourceValue where
  | null
  | boolean (value : Bool)
  | integer (value : IntValue)
  | string (value : String)
  | float (bits : FloatBits)
  | list (items : List SourceValue)
  | record (fields : List (String × SourceValue))
  | map (entries : List (MapKey × SourceValue))
  | set (domainSize : Nat) (keys : List (Fin domainSize))
      (keyOrdered : keys.Pairwise (· < ·))
  | strEnum (mapping : StrEnumMapping) (variantName : String)
  | path (value : NativePath)
  | convertedPath (text : String)

/-- The projection rejects a whole root when any nested value is incompatible. -/
inductive Rejection where
  | nonFiniteFloat
  | nonStringMapKey
  | unknownEnumVariant
  | pathNeedsExplicitConversion
  | depthLimit
  deriving DecidableEq, Repr

/-- Decide whether one string precedes another in the shared key order. -/
def stringKeyBefore (left right : String) : Bool :=
  decide (!(List.Lex (· < ·) right.toList left.toList))

/-- This executable comparator is the shared KeyOrder string relation. -/
private theorem stringKeyBefore_spec (left right : String) :
    stringKeyBefore left right = true ↔ keyLE .string left right := by
  simp [stringKeyBefore, Xsh.Collections.KeyOrder.string_order_is_lexical]

/-- Insert one field into a key-ordered list. -/
def insertField (field : String × JsonTree) : List (String × JsonTree) →
    List (String × JsonTree)
  | [] => [field]
  | next :: rest =>
      if stringKeyBefore field.1 next.1 then field :: next :: rest
      else next :: insertField field rest

/-- Sort JSON object fields using the shared string-key order. -/
def sortFields (fields : List (String × JsonTree)) : List (String × JsonTree) :=
  fields.foldr insertField []

/-- Find the wire spelling assigned to a declaration variant. -/
def wireSpelling? (mapping : StrEnumMapping) (variantName : String) : Option String :=
  (mapping.variants.find? fun (name, _) => name == variantName).map Prod.snd

/-- The key-ordered array projection for the finite ordinal set model. -/
def setItemsToJson {domainSize : Nat} (keys : List (Fin domainSize)) :
    List JsonTree :=
  keys.map fun key => .integer key.val

/-- Executable finite-float guard over the source's unchanged raw bits. -/
def finiteFloatB (bits : FloatBits) : Bool :=
  decide (floatExponent bits < floatExponentLimit - 1)

private theorem finiteFloatB_sound (bits : FloatBits)
    (accepted : finiteFloatB bits = true) : Xsh.Json.Path.isFiniteFloat bits := by
  have fact : floatExponent bits < floatExponentLimit - 1 := by
    simpa [finiteFloatB] using accepted
  exact fact

private theorem finiteFloatB_false (bits : FloatBits)
    (rejected : finiteFloatB bits = false) : ¬ Xsh.Json.Path.isFiniteFloat bits := by
  intro finite
  have fact : floatExponent bits < floatExponentLimit - 1 := finite
  have accepted : finiteFloatB bits = true := by
    simp [finiteFloatB, fact]
  simp [accepted] at rejected

/- The caller supplies a traversal budget, which decreases at each nested
   value. Encoding and parsing are not part of this recursive projection. -/
private def projectFuel : Nat → SourceValue → Except Rejection JsonTree
  | 0, _ => .error .depthLimit
  | Nat.succ fuel, value =>
      match value with
      | .null => .ok .null
      | .boolean value => .ok (.boolean value)
      | .integer value => .ok (.integer value.val)
      | .string value => .ok (.string value)
      | .float bits =>
          if accepted : finiteFloatB bits = true then
            .ok (.float ⟨bits, finiteFloatB_sound bits accepted⟩)
          else
            .error .nonFiniteFloat
      | .list items => do
          return .array (← items.mapM (projectFuel fuel))
      | .record fields => do
          let fields ← fields.mapM fun (name, fieldValue) => do
            let fieldValue ← projectFuel fuel fieldValue
            pure (name, fieldValue)
          return .record (sortFields fields)
      | .map entries => do
          let fields ← entries.mapM fun (key, fieldValue) => do
            match key with
            | .string name => do
                let fieldValue ← projectFuel fuel fieldValue
                pure (name, fieldValue)
            | _ => throw Rejection.nonStringMapKey
          return .map (sortFields fields)
      | .set _ keys _ => .ok (.array (setItemsToJson keys))
      | .strEnum mapping variantName =>
          match wireSpelling? mapping variantName with
          | some spelling => .ok (.string spelling)
          | none => .error .unknownEnumVariant
      | .path _ => .error .pathNeedsExplicitConversion
      | .convertedPath text => .ok (.string text)

/-- Project with an explicit traversal budget; each nested value spends one. -/
def project (fuel : Nat) (value : SourceValue) : Except Rejection JsonTree :=
  projectFuel fuel value

/-- Expose only a completed projection; failures carry no tree. -/
def publish? (fuel : Nat) (value : SourceValue) : Option JsonTree :=
  (project fuel value).toOption

/-- A valid finite native path used to distinguish it from explicit text. -/
def samplePath : NativePath :=
  ⟨[97], by
    intro byte member
    simp at member
    subst byte
    decide⟩

private theorem finiteFloatB_nan : finiteFloatB floatQuietNaN = false := by
  decide

theorem null_projects_to_json_null :
    project 1 .null = .ok .null := by
  simp [project, projectFuel]

theorem scalar_projection_preserves_value (boolean : Bool) (integer : IntValue)
    (string : String) :
    project 1 (.boolean boolean) = .ok (.boolean boolean) ∧
      project 1 (.integer integer) = .ok (.integer integer.val) ∧
      project 1 (.string string) = .ok (.string string) := by
  simp [project, projectFuel]

theorem finite_float_is_accepted (bits : FloatBits)
    (finite : Xsh.Json.Path.isFiniteFloat bits) :
    project 1 (.float bits) = .ok (.float ⟨bits, finite⟩) := by
  have accepted : finiteFloatB bits = true := by
    simpa [finiteFloatB, Xsh.Json.Path.isFiniteFloat] using finite
  simp [project, projectFuel, accepted]

theorem nonfinite_float_is_rejected (bits : FloatBits)
    (nonfinite : ¬ Xsh.Json.Path.isFiniteFloat bits) :
    project 1 (.float bits) = .error .nonFiniteFloat := by
  have rejected : finiteFloatB bits = false := by
    cases guard : finiteFloatB bits with
    | false => rfl
    | true => exact False.elim (nonfinite (finiteFloatB_sound bits guard))
  simp [project, projectFuel, rejected]

theorem list_projection_preserves_order :
    project 4 (.list [ .string "first", .integer intOne, .null ]) =
      .ok (.array [ .string "first", .integer 1, .null ]) := by
  simp [project, projectFuel, intOne] <;> rfl

theorem record_projection_sorts_fields :
    project 4 (.record [
      ("z", .integer intOne),
      ("a", .boolean false),
      ("m", .null)
    ]) =
      .ok (.record [
        ("a", .boolean false),
        ("m", .null),
        ("z", .integer 1)
      ]) := by
  simp [project, projectFuel, intOne] <;> rfl

theorem map_projection_requires_string_keys :
    project 4 (.map [
      (.string "good", .boolean true),
      (.integer 1, .null)
    ]) = .error .nonStringMapKey := by
  simp [project, projectFuel] <;> rfl

theorem map_projection_checks_values_recursively :
    project 8 (.map [
      (.string "nested", .list [.null, .path samplePath])
    ]) = .error .pathNeedsExplicitConversion := by
  simp [project, projectFuel, samplePath] <;> rfl

theorem set_projects_as_key_ordered_array {domainSize : Nat}
    (items : Xsh.Values.OrderedSet.OrderedSet domainSize) :
    project 1 (.set domainSize (Xsh.Values.OrderedSet.toList items)
      (Xsh.Values.OrderedSet.toList_key_ordered items)) =
      .ok (.array (setItemsToJson (Xsh.Values.OrderedSet.toList items))) := by
  simp [project, projectFuel, setItemsToJson]

theorem str_enum_projects_to_wire_spelling (mapping : StrEnumMapping)
    (variantName spelling : String)
    (mapped : wireSpelling? mapping variantName = some spelling) :
    project 1 (.strEnum mapping variantName) = .ok (.string spelling) := by
  simp [project, projectFuel, mapped]

theorem nested_incompatible_value_rejects_root :
    project 5 (.list [.null, .record [
      ("path", .path samplePath)
    ]]) = .error .pathNeedsExplicitConversion := by
  simp [project, projectFuel, samplePath] <;> rfl

theorem path_requires_explicit_json_conversion (path : NativePath) (text : String) :
    project 1 (.path path) = .error .pathNeedsExplicitConversion ∧
      project 1 (.convertedPath text) = .ok (.string text) := by
  simp [project, projectFuel]

theorem failure_publishes_no_partial_tree (fuel : Nat) (value : SourceValue)
    (reason : Rejection) (failed : project fuel value = .error reason) :
    publish? fuel value = none := by
  simp [publish?, failed] <;> rfl

/-- Faulty guard that accepts every raw bit pattern, including NaNs. -/
def acceptAllFloatBitsMutation (_bits : FloatBits) : Bool := true

/-- The concrete NaN witness makes the missing finite check observable. -/
theorem nonfinite_acceptance_mutation_is_detected :
    project 1 (.float floatQuietNaN) = .error .nonFiniteFloat ∧
      finiteFloatB floatQuietNaN = false ∧
      acceptAllFloatBitsMutation floatQuietNaN = true := by
  simp [project, projectFuel, finiteFloatB_nan, acceptAllFloatBitsMutation] <;> rfl

end Xsh.Json.Compatibility
