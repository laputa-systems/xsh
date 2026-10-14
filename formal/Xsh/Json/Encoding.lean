import Xsh.Json.Compatibility

/-!
A compact encoder for a predecessor-bounded fragment of the finite JSON tree.
Exhausted fuel and Float leaves return no text, so this fragment makes no claim
about Float formatting. Object keys use the shared canonical string order.
-/

namespace Xsh.Json.Encoding

open Xsh.Json.Compatibility

/-- Turn one hexadecimal digit into its lowercase ASCII spelling. -/
private def hexDigit (value : Nat) : Char :=
  Char.ofNat (if value < 10 then 48 + value else 87 + value)

/-- Render one control byte as two lowercase hexadecimal digits. -/
private def hexByte (value : Nat) : String :=
  String.ofList [hexDigit (value / 16), hexDigit (value % 16)]

/-- Escape one JSON string character while preserving printable Unicode. -/
private def escapeChar (value : Char) : String :=
  if value == '"' then "\\\""
  else if value == '\\' then "\\\\"
  else if value.toNat == 8 then "\\b"
  else if value.toNat == 9 then "\\t"
  else if value.toNat == 10 then "\\n"
  else if value.toNat == 12 then "\\f"
  else if value.toNat == 13 then "\\r"
  else if value.toNat < 32 then "\\u00" ++ hexByte value.toNat
  else String.ofList [value]

/-- Escape each character without interpreting or normalizing the input text. -/
private def escapeText : List Char → String
  | [] => ""
  | value :: rest => escapeChar value ++ escapeText rest

/-- Join already-encoded elements with compact JSON separators. -/
private def joinComma (parts : List String) : String :=
  match parts with
  | [] => ""
  | first :: rest => rest.foldl (fun output part => output ++ "," ++ part) first

/-- Quote and escape one string, including object keys. -/
private def quoteJsonString (value : String) : String :=
  "\"" ++ escapeText value.toList ++ "\""

private def fieldKeyLE (left right : String × Xsh.Json.Path.Value) : Prop :=
  left.1 ≤ right.1

private theorem stringKeyBefore_iff (left right : String) :
    stringKeyBefore left right = true ↔ left ≤ right := by
  change stringKeyBefore left right = true ↔
    Xsh.Collections.KeyOrder.keyLE .string left right
  simp [stringKeyBefore, Xsh.Collections.KeyOrder.string_order_is_lexical]

private theorem insertField_mem (field item : String × Xsh.Json.Path.Value) :
    ∀ fields, item ∈ insertField field fields ↔ item = field ∨ item ∈ fields := by
  intro fields
  induction fields with
  | nil => simp [insertField]
  | cons next rest ih =>
      by_cases before : stringKeyBefore field.1 next.1 = true
      · simp [insertField, before]
      · simp [insertField, before, ih, or_left_comm]

private theorem insertField_pairwise (field : String × Xsh.Json.Path.Value) :
    ∀ {fields}, fields.Pairwise fieldKeyLE →
      (insertField field fields).Pairwise fieldKeyLE := by
  intro fields sorted
  induction fields with
  | nil => simp [insertField]
  | cons next rest ih =>
      have tailSorted := sorted.of_cons
      have headBeforeRest : ∀ item ∈ rest, next.1 ≤ item.1 := by
        intro item member
        simpa [fieldKeyLE] using List.rel_of_pairwise_cons sorted member
      by_cases before : stringKeyBefore field.1 next.1 = true
      · have fieldBeforeNext : field.1 ≤ next.1 :=
          (stringKeyBefore_iff field.1 next.1).mp before
        have fieldBeforeRest : ∀ item ∈ rest, field.1 ≤ item.1 := by
          intro item member
          exact String.le_trans fieldBeforeNext (headBeforeRest item member)
        rw [insertField, if_pos before]
        constructor
        · intro item member
          simp only [List.mem_cons] at member
          rcases member with rfl | member
          · exact fieldBeforeNext
          · exact fieldBeforeRest item member
        · exact sorted
      · have notFieldBeforeNext : ¬ field.1 ≤ next.1 := by
          intro keyOrder
          have isBefore : stringKeyBefore field.1 next.1 = true :=
            (stringKeyBefore_iff field.1 next.1).mpr keyOrder
          contradiction
        have nextBeforeField : next.1 ≤ field.1 :=
          (String.le_total next.1 field.1).resolve_right notFieldBeforeNext
        rw [insertField, if_neg before, List.pairwise_cons]
        constructor
        · intro item member
          rcases (insertField_mem field item rest).mp member with same | inRest
          · cases same
            exact nextBeforeField
          · exact headBeforeRest item inRest
        · exact ih tailSorted

private theorem insertField_perm (field : String × Xsh.Json.Path.Value) :
    ∀ fields, List.Perm (field :: fields) (insertField field fields) := by
  intro fields
  induction fields with
  | nil => simp [insertField]
  | cons next rest ih =>
      by_cases before : stringKeyBefore field.1 next.1 = true
      · simp [insertField, before]
      · simp only [insertField, if_neg before]
        exact (List.Perm.swap next field rest).trans (List.Perm.cons next ih)

private theorem sortFields_pairwise (fields : List (String × Xsh.Json.Path.Value)) :
    (sortFields fields).Pairwise fieldKeyLE := by
  induction fields with
  | nil => simp [sortFields]
  | cons field rest ih =>
      simp only [sortFields, List.foldr_cons]
      exact insertField_pairwise field ih

private theorem sortFields_perm (fields : List (String × Xsh.Json.Path.Value)) :
    List.Perm fields (sortFields fields) := by
  induction fields with
  | nil => simp [sortFields]
  | cons field rest ih =>
      simp only [sortFields, List.foldr_cons]
      exact (List.Perm.cons field ih).trans (insertField_perm field (sortFields rest))

/-- A tie-breaking mutation that uses strict order and reverses duplicate keys. -/
private def strictTieInsert (field : String × Xsh.Json.Path.Value) :
    List (String × Xsh.Json.Path.Value) → List (String × Xsh.Json.Path.Value)
  | [] => [field]
  | next :: rest =>
      if field.1 < next.1 then field :: next :: rest
      else next :: strictTieInsert field rest

private def strictTieSort (fields : List (String × Xsh.Json.Path.Value)) :=
  fields.foldr strictTieInsert []

/-- An off-by-one lower control bound exposes U+001F as a raw character. -/
private def lowerControlBoundMutation (value : Char) : String :=
  if value.toNat < 31 then escapeChar value else String.ofList [value]

/-- An off-by-one upper control bound escapes U+0020 as though it were a control. -/
private def upperControlBoundMutation (value : Char) : String :=
  if value.toNat ≤ 32 then "\\u00" ++ hexByte value.toNat else escapeChar value

/-- An ASCII-only mutation truncates every escaped Unicode scalar to one byte. -/
private def asciiOnlyUnicodeMutation (value : Char) : String :=
  if value.toNat > 126 then "\\u00" ++ hexByte (value.toNat % 256) else escapeChar value

/-- Encode a JSON value using fuel that decreases once per nested container. -/
def encodeFuel : Nat → Xsh.Json.Path.Value → Option String
  | 0, _ => none
  | Nat.succ fuel, value =>
      match value with
      | .null => some "null"
      | .boolean value => some (if value then "true" else "false")
      | .integer value => some (toString value)
      | .string value => some (quoteJsonString value)
      | .float _ => none
      | .array items => do
          let items ← items.mapM (encodeFuel fuel)
          pure ("[" ++ joinComma items ++ "]")
      | .record fields => do
          let fields := Xsh.Json.Compatibility.sortFields fields
          let fields ← fields.mapM fun (key, item) => do
            let item ← encodeFuel fuel item
            pure (quoteJsonString key ++ ":" ++ item)
          pure ("{" ++ joinComma fields ++ "}")
      | .map fields => do
          let fields := Xsh.Json.Compatibility.sortFields fields
          let fields ← fields.mapM fun (key, item) => do
            let item ← encodeFuel fuel item
            pure (quoteJsonString key ++ ":" ++ item)
          pure ("{" ++ joinComma fields ++ "}")

private def encodeFieldsWithStrictTieMutation
    (fields : List (String × Xsh.Json.Path.Value)) : Option String := do
  let encoded ← (strictTieSort fields).mapM fun (key, item) => do
    let item ← encodeFuel 1 item
    pure (quoteJsonString key ++ ":" ++ item)
  pure ("{" ++ joinComma encoded ++ "}")

/-- Null is the compact JSON null token. -/
theorem null_token :
    encodeFuel 1 .null = some "null" := by
  decide

/-- Both Boolean values use their lowercase JSON tokens. -/
theorem boolean_tokens :
    encodeFuel 1 (.boolean true) = some "true" ∧
      encodeFuel 1 (.boolean false) = some "false" := by
  decide

/-- Integer tokens use a minus sign and decimal digits without extra spacing. -/
theorem integer_token :
    encodeFuel 1 (.integer (-37)) = some "-37" := by
  decide

/-- A quote receives its JSON escape. -/
theorem quote_is_escaped :
    quoteJsonString "\"" = String.ofList ['"', '\\', '"', '"'] := by
  decide

/-- A backslash receives its JSON escape. -/
theorem backslash_is_escaped :
    quoteJsonString "\\" = String.ofList ['"', '\\', '\\', '"'] := by
  decide

/-- Every JSON control scalar expands to a short escape or six-character Unicode escape. -/
theorem control_characters_are_escaped (value : Char) (control : value.toNat < 32) :
    (escapeChar value).length =
      if value.toNat == 8 then 2
      else if value.toNat == 9 then 2
      else if value.toNat == 10 then 2
      else if value.toNat == 12 then 2
      else if value.toNat == 13 then 2
      else 6 := by
  have notQuote : value ≠ '"' := by
    intro equal
    subst value
    simp at control
  have notBackslash : value ≠ '\\' := by
    intro equal
    subst value
    simp at control
  by_cases isEight : value.toNat = 8
  · simp [escapeChar, notQuote, notBackslash, isEight] <;> decide
  · by_cases isNine : value.toNat = 9
    · simp [escapeChar, notQuote, notBackslash, isEight, isNine] <;> decide
    · by_cases isTen : value.toNat = 10
      · simp [escapeChar, notQuote, notBackslash, isEight, isNine, isTen] <;> decide
      · by_cases isTwelve : value.toNat = 12
        · simp [escapeChar, notQuote, notBackslash, isEight, isNine, isTen, isTwelve] <;> decide
        · by_cases isThirteen : value.toNat = 13
          · simp [escapeChar, notQuote, notBackslash, isEight, isNine, isTen,
              isTwelve, isThirteen] <;> decide
          · simp [escapeChar, notQuote, notBackslash, isEight, isNine, isTen,
              isTwelve, isThirteen, control, hexByte, hexDigit, String.length_append] <;>
              decide

/-- Non-control Unicode scalars survive unchanged unless JSON requires escaping. -/
theorem noncontrol_unicode_is_preserved (value : Char) (noncontrol : 32 ≤ value.toNat)
    (notQuote : value ≠ '"') (notBackslash : value ≠ '\\') :
    escapeChar value = String.ofList [value] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0x7f) ≠ String.ofList [Char.ofNat 0x7f] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0x80) ≠ String.ofList [Char.ofNat 0x80] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0x7ff) ≠ String.ofList [Char.ofNat 0x7ff] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0x800) ≠ String.ofList [Char.ofNat 0x800] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0xd7ff) ≠ String.ofList [Char.ofNat 0xd7ff] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0xe000) ≠ String.ofList [Char.ofNat 0xe000] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0xffff) ≠ String.ofList [Char.ofNat 0xffff] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0x10000) ≠ String.ofList [Char.ofNat 0x10000] ∧
      asciiOnlyUnicodeMutation (Char.ofNat 0x10ffff) ≠ String.ofList [Char.ofNat 0x10ffff] := by
  have notEight : value.toNat ≠ 8 := by omega
  have notNine : value.toNat ≠ 9 := by omega
  have notTen : value.toNat ≠ 10 := by omega
  have notTwelve : value.toNat ≠ 12 := by omega
  have notThirteen : value.toNat ≠ 13 := by omega
  constructor
  · simp [escapeChar, notQuote, notBackslash, notEight, notNine, notTen,
      notTwelve, notThirteen, Nat.not_lt_of_ge noncontrol]
  · decide

/-- An empty array has no spaces or element separators. -/
theorem empty_array_is_compact :
    encodeFuel 1 (.array []) = some "[]" := by
  decide

/-- Array elements remain in their input order. -/
theorem array_order_is_preserved :
    encodeFuel 2 (.array [.string "z", .integer 1, .boolean false]) =
      some "[\"z\",1,false]" := by
  decide

/-- Arbitrary object fields are sorted, preserved, and expose strict-tie mutations. -/
theorem object_fields_are_sorted (fields : List (String × Xsh.Json.Path.Value)) :
    (sortFields fields).Pairwise (fun left right => left.1 ≤ right.1) ∧
      List.Perm fields (sortFields fields) ∧
      encodeFuel 2 (.record [("a", .integer 1), ("a", .integer 2)]) =
        some "{\"a\":1,\"a\":2}" ∧
      encodeFieldsWithStrictTieMutation [("a", .integer 1), ("a", .integer 2)] =
        some "{\"a\":2,\"a\":1}" := by
  exact ⟨sortFields_pairwise fields, sortFields_perm fields, by decide, by decide⟩

/-- Nested arrays and records retain their shape while each object is sorted. -/
theorem nested_shape_and_order_are_preserved :
    encodeFuel 5 (.record [
      ("z", .array [.integer 1, .record [
        ("z", .boolean false), ("a", .string "☃")
      ]]),
      ("a", .null)
    ]) = some "{\"a\":null,\"z\":[1,{\"a\":\"☃\",\"z\":false}]}" := by
  decide

/-- This mutation escapes quotes and slashes but leaves other controls raw. -/
private def quoteAndBackslashOnlyMutation : List Char → String
  | [] => ""
  | '"' :: rest => "\\\"" ++ quoteAndBackslashOnlyMutation rest
  | '\\' :: rest => "\\\\" ++ quoteAndBackslashOnlyMutation rest
  | value :: rest => String.ofList [value] ++ quoteAndBackslashOnlyMutation rest

/-- The mutation keeps quote escaping but exposes a raw line feed. -/
theorem raw_control_and_quote_mutation_are_distinguished :
    quoteJsonString (String.ofList [Char.ofNat 10]) =
      String.ofList ['"', '\\', 'n', '"'] ∧
    ("\"" ++ quoteAndBackslashOnlyMutation [Char.ofNat 10] ++ "\"") =
      String.ofList ['"', Char.ofNat 10, '"'] ∧
    quoteJsonString "\"" =
      ("\"" ++ quoteAndBackslashOnlyMutation ['"'] ++ "\"") ∧
    escapeChar (Char.ofNat 31) = "\\u001f" ∧
    lowerControlBoundMutation (Char.ofNat 31) = String.ofList [Char.ofNat 31] ∧
    upperControlBoundMutation (Char.ofNat 32) = "\\u0020" ∧
    escapeChar (Char.ofNat 32) = String.ofList [Char.ofNat 32] ∧
    escapeChar (Char.ofNat 127) = String.ofList [Char.ofNat 127] := by
  decide

end Xsh.Json.Encoding
