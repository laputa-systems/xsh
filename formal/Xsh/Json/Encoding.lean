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

/-- Sample controls use short escapes or lowercase Unicode escapes. -/
theorem control_characters_are_escaped :
    quoteJsonString (String.ofList [Char.ofNat 0, Char.ofNat 9, Char.ofNat 10,
      Char.ofNat 12, Char.ofNat 13, Char.ofNat 31]) =
      "\"\\u0000\\t\\n\\f\\r\\u001f\"" := by
  decide

/-- Printable non-ASCII characters survive string encoding unchanged. -/
theorem noncontrol_unicode_is_preserved :
    quoteJsonString "☃𝄞" = "\"☃𝄞\"" := by
  decide

/-- An empty array has no spaces or element separators. -/
theorem empty_array_is_compact :
    encodeFuel 1 (.array []) = some "[]" := by
  decide

/-- Array elements remain in their input order. -/
theorem array_order_is_preserved :
    encodeFuel 2 (.array [.string "z", .integer 1, .boolean false]) =
      some "[\"z\",1,false]" := by
  decide

/-- Record fields are emitted in the shared sorted string-key order. -/
theorem object_fields_are_sorted :
    encodeFuel 2 (.record [("z", .integer 1), ("a", .integer 2)]) =
      some "{\"a\":2,\"z\":1}" := by
  decide

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
      ("\"" ++ quoteAndBackslashOnlyMutation ['"'] ++ "\"") := by
  decide

end Xsh.Json.Encoding
