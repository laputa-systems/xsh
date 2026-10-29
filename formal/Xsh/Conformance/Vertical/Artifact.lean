import Xsh.Conformance.Transport.Json
import Xsh.Conformance.Transport.Codecs

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

def sourceSchema : String := "xsh-source-v1"

def rawIrSchema : String := "xsh-raw-ir-v1"

def arithmeticScope : String := "integer-arithmetic-local-fixture-v1"

/-- The raw instruction tags of the integer-arithmetic local fixture. Any other tag name is
an unknown transport tag and fails the decoder. -/
inductive FragmentTag where
  | intInt
  | stmtLetInt
  | exprInt
  | exprParam
  | exprBinary
  | stmtReturn
  deriving DecidableEq, Repr

namespace FragmentTag

def all : List FragmentTag :=
  [intInt, stmtLetInt, exprInt, exprParam, exprBinary, stmtReturn]

def name : FragmentTag → String
  | intInt => "IntInt"
  | stmtLetInt => "StmtLetInt"
  | exprInt => "ExprInt"
  | exprParam => "ExprParam"
  | exprBinary => "ExprBinary"
  | stmtReturn => "StmtReturn"

def word : FragmentTag → Word32
  | intInt => ⟨0, by decide⟩
  | stmtLetInt => ⟨118, by decide⟩
  | exprInt => ⟨20, by decide⟩
  | exprParam => ⟨31, by decide⟩
  | exprBinary => ⟨33, by decide⟩
  | stmtReturn => ⟨150, by decide⟩

def payloadWords : FragmentTag → Nat
  | intInt | stmtLetInt | exprInt => 2
  | exprParam | stmtReturn => 1
  | exprBinary => 4

def ofName? (text : String) : Option FragmentTag := all.find? fun tag => tag.name == text

def ofWord? (value : Word32) : Option FragmentTag := all.find? fun tag => tag.word == value

end FragmentTag

/-- The arithmetic operations of the integer fragment, named and numbered as the Rust
`BinaryOp` enumeration. -/
inductive FragmentOp where
  | add
  | sub
  | mul
  | div
  | rem
  deriving DecidableEq, Repr

namespace FragmentOp

def all : List FragmentOp := [add, sub, mul, div, rem]

def name : FragmentOp → String
  | add => "Add"
  | sub => "Sub"
  | mul => "Mul"
  | div => "Div"
  | rem => "Rem"

def word : FragmentOp → Word32
  | add => ⟨11, by decide⟩
  | sub => ⟨12, by decide⟩
  | mul => ⟨13, by decide⟩
  | div => ⟨14, by decide⟩
  | rem => ⟨15, by decide⟩

def ofName? (text : String) : Option FragmentOp := all.find? fun op => op.name == text

end FragmentOp

structure SourceDocument where
  id : BridgeId
  text : String
  deriving DecidableEq

structure Row where
  id : BridgeId
  tag : FragmentTag
  lhs : Word32
  rhs : Word32
  deriving DecidableEq

structure Block where
  id : BridgeId
  start : Word32
  len : Word32
  result : OptionalBridgeId
  owner : OptionalBridgeId
  flags : WireByte
  deriving DecidableEq

structure FunctionRow where
  id : BridgeId
  name : String
  paramsStart : Word32
  paramsLen : Word32
  capturesStart : Word32
  capturesLen : Word32
  body : BridgeId
  slotCount : Word32
  deriving DecidableEq

structure Observation where
  function : BridgeId
  result : Word64
  deriving DecidableEq

structure RawArtifact where
  sourceId : BridgeId
  rows : List Row
  extra : List Word32
  ops : List FragmentOp
  blocks : List Block
  functions : List FunctionRow
  observations : List Observation
  deriving DecidableEq

section Decoding

private def textField (fields : List (String × WireValue)) (name : String) : Option String :=
  (WireValue.field? fields name).bind WireValue.text?

private def wordField (fields : List (String × WireValue)) (name : String) :
    Option Word32 :=
  (textField fields name).bind decodeHex32?

private def wideField (fields : List (String × WireValue)) (name : String) : Option Word64 :=
  (textField fields name).bind decodeHex64?

private def idField (fields : List (String × WireValue)) (name : String) : Option BridgeId :=
  (textField fields name).bind BridgeId.fromHex?

private def optionalIdField (fields : List (String × WireValue)) (name : String) :
    Option OptionalBridgeId :=
  (textField fields name).bind OptionalBridgeId.fromHex?

private def byteField (fields : List (String × WireValue)) (name : String) : Option WireByte :=
  match (textField fields name).bind decodeHexBytes? with
  | some [byte] => some byte
  | _ => none

def decodeSourceDocument (text : ExchangeCodes) : Option SourceDocument := do
  let value ← parseCodes text
  let fields ← value.closedObject? ["schema", "id", "text"]
  if textField fields "schema" == some sourceSchema then pure () else none
  let id ← idField fields "id"
  let body ← textField fields "text"
  pure ⟨id, body⟩

/-- A row is accepted only for a known tag name carrying that tag's own ordinal. -/
def decodeRow (value : WireValue) : Option Row := do
  let fields ← value.closedObject? ["id", "tag", "ordinal", "lhs", "rhs"]
  let id ← idField fields "id"
  let tag ← (textField fields "tag").bind FragmentTag.ofName?
  let ordinal ← wordField fields "ordinal"
  if ordinal == tag.word then pure () else none
  let lhs ← wordField fields "lhs"
  let rhs ← wordField fields "rhs"
  pure ⟨id, tag, lhs, rhs⟩

def decodeOp (value : WireValue) : Option FragmentOp := do
  let fields ← value.closedObject? ["name", "ordinal"]
  let op ← (textField fields "name").bind FragmentOp.ofName?
  let ordinal ← wordField fields "ordinal"
  if ordinal == op.word then pure op else none

def decodeBlock (value : WireValue) : Option Block := do
  let fields ← value.closedObject? ["id", "start", "len", "result", "owner", "flags"]
  pure ⟨← idField fields "id", ← wordField fields "start", ← wordField fields "len",
    ← optionalIdField fields "result", ← optionalIdField fields "owner",
    ← byteField fields "flags"⟩

def decodeFunction (value : WireValue) : Option FunctionRow := do
  let fields ← value.closedObject?
    ["id", "name", "paramsStart", "paramsLen", "capturesStart", "capturesLen", "body",
      "slotCount"]
  pure ⟨← idField fields "id", ← textField fields "name", ← wordField fields "paramsStart",
    ← wordField fields "paramsLen", ← wordField fields "capturesStart",
    ← wordField fields "capturesLen", ← idField fields "body", ← wordField fields "slotCount"⟩

def decodeObservation (value : WireValue) : Option Observation := do
  let fields ← value.closedObject? ["function", "result"]
  pure ⟨← idField fields "function", ← wideField fields "result"⟩

private def listField (fields : List (String × WireValue)) (name : String)
    (decode : WireValue → Option α) : Option (List α) :=
  ((WireValue.field? fields name).bind WireValue.items?).bind fun items => items.mapM decode

def decodeWordList (value : WireValue) : Option Word32 :=
  value.text?.bind decodeHex32?

def decodeRawDocument (text : ExchangeCodes) : Option RawArtifact := do
  let value ← parseCodes text
  let fields ← value.closedObject?
    ["schema", "sourceId", "scope", "instructions", "extra", "binaryOps", "blocks",
      "functions", "observations"]
  if textField fields "schema" == some rawIrSchema then pure () else none
  if textField fields "scope" == some arithmeticScope then pure () else none
  pure ⟨← idField fields "sourceId", ← listField fields "instructions" decodeRow,
    ← listField fields "extra" decodeWordList, ← listField fields "binaryOps" decodeOp,
    ← listField fields "blocks" decodeBlock, ← listField fields "functions" decodeFunction,
    ← listField fields "observations" decodeObservation⟩

end Decoding

theorem fragment_tag_words_are_distinct : (FragmentTag.all.map FragmentTag.word).Nodup := by
  decide

theorem fragment_tag_words_decode_to_their_tags (tag : FragmentTag) :
    FragmentTag.ofWord? tag.word = some tag := by
  cases tag <;> decide

theorem fragment_tag_names_decode_to_their_tags (tag : FragmentTag) :
    FragmentTag.ofName? tag.name = some tag := by
  cases tag <;> decide

theorem fragment_tags_exclude_the_unlisted_ordinals :
    FragmentTag.ofWord? ⟨429, by decide⟩ = none ∧ FragmentTag.ofName? "ExprUnknown" = none := by
  constructor <;> decide

def sampleRow (tag ordinal : String) : WireValue :=
  .object [("id", .string "00000001"), ("tag", .string tag), ("ordinal", .string ordinal),
    ("lhs", .string "00000000"), ("rhs", .string "00000001")]

theorem known_transport_tag_is_accepted :
    (decodeRow (sampleRow "ExprParam" "0000001f")).isSome = true := by
  decide

theorem unknown_transport_tag_is_rejected :
    (decodeRow (sampleRow "ExprUnknown" "0000001f")).isNone = true := by
  decide

theorem tag_carrying_another_tags_ordinal_is_rejected :
    (decodeRow (sampleRow "ExprParam" "00000021")).isNone = true := by
  decide

theorem row_with_an_unknown_field_is_rejected :
    (decodeRow (.object [("id", .string "00000001"), ("tag", .string "ExprParam"),
      ("ordinal", .string "0000001f"), ("lhs", .string "00000000"),
      ("rhs", .string "00000001"), ("extra", .string "")])).isNone = true := by
  decide

theorem operation_carrying_another_operations_ordinal_is_rejected :
    (decodeOp (.object [("name", .string "Add"), ("ordinal", .string "0000000b")])).isSome =
        true ∧
      (decodeOp (.object [("name", .string "Add"), ("ordinal", .string "0000000d")])).isNone =
        true ∧
      (decodeOp (.object [("name", .string "Pow"), ("ordinal", .string "0000000b")])).isNone =
        true := by
  refine ⟨?_, ?_, ?_⟩ <;> decide

theorem source_document_round_trips_exact_text :
    decodeSourceDocument (codesOf "{\"schema\":\"xsh-source-v1\",\"id\":\"00000001\",\"text\":\"pure f() {\\n}\"}") =
      some ⟨⟨⟨1, by decide⟩, by decide, by decide⟩, "pure f() {\n}"⟩ := by
  decide

theorem source_document_rejects_a_reserved_identifier_and_an_unknown_schema :
    (decodeSourceDocument (codesOf "{\"schema\":\"xsh-source-v1\",\"id\":\"00000000\",\"text\":\"\"}")).isNone =
        true ∧
      (decodeSourceDocument (codesOf "{\"schema\":\"xsh-source-v2\",\"id\":\"00000001\",\"text\":\"\"}")).isNone =
        true := by
  constructor <;> decide

end Xsh.Conformance.Vertical
