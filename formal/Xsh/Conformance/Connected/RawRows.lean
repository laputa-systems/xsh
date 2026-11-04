import Xsh.Conformance.Connected.FactsDocument

namespace Xsh.Conformance.Connected

open Xsh.Conformance.Transport
open Xsh.Syntax

structure RawTag where
  name : String
  ordinal : Nat
  deriving DecidableEq

structure RawPair where
  lhs : Nat
  rhs : Nat
  deriving DecidableEq

structure RawInstructionRows where
  tags : List RawTag
  resourceTransfers : List Bool
  checkedCallTargetRequired : List Bool
  data : List RawPair
  extra : List Nat
  binaryOps : List BinaryOp
  locations : List RawPair
  locationSources : List Nat
  strings : List RawPair
  stringBytes : List Nat
  deriving DecidableEq

structure RawRange where
  start : Nat
  length : Nat
  deriving DecidableEq

structure RawBlock where
  instructions : RawRange
  result : Nat
  owner : Nat
  flags : Nat
  reserved : Nat × Nat × Nat
  deriving DecidableEq

inductive RawContract where
  | ground (id : Nat)
  | scheme (id : Nat)
  deriving DecidableEq

structure RawFunction where
  name : Nat
  contract : RawContract
  parameters : RawRange
  captures : RawRange
  body : Nat
  slotCount : Nat
  slotTypes : RawRange
  checkedReturn : Nat
  deriving DecidableEq

structure RawFunctionRows where
  blocks : List RawBlock
  functions : List RawFunction
  deriving DecidableEq

structure RawInstruction where
  tag : RawTag
  resourceTransfer : Bool
  checkedCallTargetRequired : Bool
  payload : List Nat
  deriving DecidableEq

def rawInstructionColumnsComplete (rows : RawInstructionRows) : Bool :=
  rows.tags.length == rows.resourceTransfers.length &&
    rows.tags.length == rows.checkedCallTargetRequired.length &&
    rows.tags.length == rows.data.length &&
    rows.locations.length == rows.locationSources.length

def readRawRange (words : List Nat) (range : RawRange) : Option (List Nat) :=
  if range.start + range.length ≤ words.length then
    some ((words.drop range.start).take range.length)
  else none

def readRawInstruction (rows : RawInstructionRows) (index : Nat) : Option RawInstruction := do
  let tag ← rows.tags[index]?
  let transfer ← rows.resourceTransfers[index]?
  let checked ← rows.checkedCallTargetRequired[index]?
  let data ← rows.data[index]?
  let payload ← readRawRange rows.extra ⟨data.lhs, data.rhs⟩
  pure ⟨tag, transfer, checked, payload⟩

def readRawLocation (rows : RawInstructionRows) (id : Nat) : Option SourceSite := do
  if id = 0 || id = 4294967295 then none else
    let location ← rows.locations[id - 1]?
    let source ← rows.locationSources[id - 1]?
    pure ⟨source, location.lhs, location.rhs⟩

def readRawStringBytes (rows : RawInstructionRows) (id : Nat) : Option (List Nat) := do
  if id = 0 || id = 4294967295 then none else
    let range ← rows.strings[id - 1]?
    readRawRange rows.stringBytes ⟨range.lhs, range.rhs⟩

def readRawBlock (instructions : RawInstructionRows) (functions : RawFunctionRows)
    (id : Nat) : Option (RawBlock × List Nat) := do
  if id = 0 || id = 4294967295 then none else
    let block ← functions.blocks[id - 1]?
    let words ← readRawRange instructions.extra block.instructions
    pure (block, words)

def readOwnedRawBlock (instructions : RawInstructionRows) (functions : RawFunctionRows)
    (owner id kind : Nat) : Option (List Nat) := do
  let (block, words) ← readRawBlock instructions functions id
  if (block.owner = 4294967295 || block.owner = owner) &&
      block.flags % 2 = kind && block.result = 4294967295 then
    some words
  else none

def readRawStatementBlock (instructions : RawInstructionRows) (functions : RawFunctionRows)
    (owner id : Nat) : Option (List Nat) := do
  let words ← readOwnedRawBlock instructions functions owner id 1
  match words with
  | count :: statements => if statements.length = count then some statements else none
  | [] => none

inductive RawExpressionShape where
  | integer (low high : Nat)
  | boolean (value : Bool)
  | string (bytes : List Nat)
  | slot (slot : Nat)
  | binary (operator : BinaryOp) (left right : Nat) (site : Option SourceSite)
  | field (base : Nat) (name : List Nat) (site : SourceSite)
  | record (entries : Nat)
  | conditional (branches otherwise : Nat) (site : SourceSite)
  | valueBlock (body : Nat) (site : SourceSite)
  | resultOk (value : Nat)
  | resultError (value : Nat) (cause : Option Nat)
  | propagate (value : Nat) (site : SourceSite)
  | call (direct : Bool) (function arguments : Nat) (site : SourceSite)
  deriving DecidableEq

def rawExpressionShape (rows : RawInstructionRows) (instruction : RawInstruction) : Option RawExpressionShape := do
  match instruction.tag.ordinal, instruction.payload with
  | 0, [low, high] | 20, [low, high] => some (.integer low high)
  | 6, [0] | 23, [0] => some (.boolean false)
  | 6, [1] | 23, [1] => some (.boolean true)
  | 24, [id] => some (.string (← readRawStringBytes rows id))
  | 1, [slot] | 7, [slot] | 31, [slot] => some (.slot slot)
  | 2, [operator, left, right] => some (.binary (← rows.binaryOps[operator]?) left right none)
  | 9, [left, right] => some (.binary .and left right none)
  | 10, [left, right] => some (.binary .or left right none)
  | 11, [operator, left, right] => some (.binary (← rows.binaryOps[operator]?) left right none)
  | 33, [operator, left, right, location] =>
      some (.binary (← rows.binaryOps[operator]?) left right (some (← readRawLocation rows location)))
  | 34, [branches, otherwise, location] =>
      some (.conditional branches otherwise (← readRawLocation rows location))
  | 44, [entries] => some (.record entries)
  | 56, [base, name, location] =>
      some (.field base (← readRawStringBytes rows name) (← readRawLocation rows location))
  | 73, [body, location] => some (.valueBlock body (← readRawLocation rows location))
  | 104, [value] => some (.resultOk value)
  | 105, [value, 0] => some (.resultError value none)
  | 105, [value, 1, cause] => some (.resultError value (some cause))
  | 107, [value, location] => some (.propagate value (← readRawLocation rows location))
  | 108, [function, arguments, location] =>
      some (.call false function arguments (← readRawLocation rows location))
  | 109, [function, arguments, location] =>
      some (.call true function arguments (← readRawLocation rows location))
  | _, _ => none

def connectedTagOrdinal : String → Option Nat
  | "IntInt" => some 0
  | "IntSlot" => some 1
  | "IntBinary" => some 2
  | "BoolBool" => some 6
  | "BoolSlot" => some 7
  | "BoolNot" => some 8
  | "BoolAnd" => some 9
  | "BoolOr" => some 10
  | "BoolIntCompare" => some 11
  | "ExprInt" => some 20
  | "ExprBool" => some 23
  | "ExprStr" => some 24
  | "ExprFunctionRef" => some 29
  | "ExprParam" => some 31
  | "ExprBinary" => some 33
  | "ExprIf" => some 34
  | "ExprRecord" => some 44
  | "ExprField" => some 56
  | "ExprRequire" => some 65
  | "ExprCheckedValue" => some 66
  | "ExprValueBlock" => some 73
  | "ExprModuleCall" => some 99
  | "ExprOk" => some 104
  | "ExprErr" => some 105
  | "ExprTry" => some 107
  | "ExprCall" => some 108
  | "ExprDirectPureCall" => some 109
  | "StmtLet" => some 115
  | "StmtLetInt" => some 118
  | "StmtLetBool" => some 119
  | "StmtAssign" => some 120
  | "StmtAssignInt" => some 124
  | "StmtAssignBool" => some 125
  | "StmtValue" => some 126
  | "StmtAssert" => some 127
  | "StmtExpr" => some 128
  | "StmtIf" => some 129
  | "StmtIfBool" => some 130
  | "StmtReturn" => some 150
  | _ => none

def rawFlag : WireValue → Option Bool
  | .string "0" => some false
  | .string "1" => some true
  | _ => none

def decodeRawTag (value : WireValue) : Option RawTag := do
  let fields ← value.closedObject? ["name", "ordinal"]
  let name ← textField fields "name"
  let ordinal ← wordField fields "ordinal"
  if connectedTagOrdinal name = some ordinal then some ⟨name, ordinal⟩ else none

theorem decoded_raw_tag_has_canonical_ordinal (value : WireValue) (tag : RawTag)
    (accepted : decodeRawTag value = some tag) : connectedTagOrdinal tag.name = some tag.ordinal := by
  unfold decodeRawTag at accepted
  cases hFields : value.closedObject? ["name", "ordinal"] with
  | none => simp [hFields] at accepted
  | some fields =>
    cases hName : textField fields "name" with
    | none => simp [hFields, hName] at accepted
    | some name =>
      cases hOrdinal : wordField fields "ordinal" with
      | none => simp [hFields, hName, hOrdinal] at accepted
      | some ordinal =>
        by_cases consistent : connectedTagOrdinal name = some ordinal
        · simp [hFields, hName, hOrdinal, consistent] at accepted
          subst tag
          exact consistent
        · simp [hFields, hName, hOrdinal, consistent] at accepted

theorem raw_tag_rejects_wrong_ordinal :
    decodeRawTag (.object [("name", .string "StmtReturn"), ("ordinal", .string "00000000")]) = none := by
  decide +kernel

def decodeRawPair (value : WireValue) : Option RawPair := do
  let fields ← value.closedObject? ["lhs", "rhs"]
  pure ⟨← wordField fields "lhs", ← wordField fields "rhs"⟩

def decodeRawLocation (value : WireValue) : Option RawPair := do
  let fields ← value.closedObject? ["start", "length"]
  pure ⟨← wordField fields "start", ← wordField fields "length"⟩

def decodeRawByte (value : WireValue) : Option Nat := do
  let byte ← decodeWord value
  if byte < 256 then some byte else none

def decodeRawBinaryOp : WireValue → Option BinaryOp
  | .string "resultFallback" => some .resultFallback
  | .string "or" => some .or
  | .string "and" => some .and
  | .string "eq" => some .eq
  | .string "ne" => some .ne
  | .string "lt" => some .lt
  | .string "le" => some .le
  | .string "gt" => some .gt
  | .string "ge" => some .ge
  | .string "in" => some .member
  | .string "notIn" => some .notMember
  | .string "add" => some .add
  | .string "sub" => some .sub
  | .string "mul" => some .mul
  | .string "div" => some .div
  | .string "rem" => some .rem
  | .string "union" => some .union
  | .string "intersect" => some .intersect
  | _ => none

def decodeRawInstructionRows (value : WireValue) : Option RawInstructionRows := do
  let fields ← value.closedObject? ["tags", "resourceTransfers", "checkedCallTargetRequired", "data", "extra", "binaryOps", "locations", "locationSources", "strings", "stringBytes"]
  pure ⟨← listField fields "tags" decodeRawTag, ← listField fields "resourceTransfers" rawFlag,
    ← listField fields "checkedCallTargetRequired" rawFlag, ← listField fields "data" decodeRawPair,
    ← listField fields "extra" decodeWord, ← listField fields "binaryOps" decodeRawBinaryOp,
    ← listField fields "locations" decodeRawLocation, ← listField fields "locationSources" decodeWord,
    ← listField fields "strings" decodeRawLocation, ← listField fields "stringBytes" decodeRawByte⟩

def decodeRawRange (value : WireValue) : Option RawRange := do
  let fields ← value.closedObject? ["start", "length"]
  pure ⟨← wordField fields "start", ← wordField fields "length"⟩

def rawRangeField (fields : List (String × WireValue)) (name : String) : Option RawRange :=
  (WireValue.field? fields name).bind decodeRawRange

def rawByteField (fields : List (String × WireValue)) (name : String) : Option Nat := do
  let value ← wordField fields name
  if value < 256 then some value else none

def decodeRawBlock (value : WireValue) : Option RawBlock := do
  let fields ← value.closedObject? ["instructions", "result", "owner", "flags", "reserved0", "reserved1", "reserved2"]
  pure ⟨← rawRangeField fields "instructions", ← wordField fields "result", ← wordField fields "owner",
    ← rawByteField fields "flags", (← rawByteField fields "reserved0", ← rawByteField fields "reserved1", ← rawByteField fields "reserved2")⟩

def decodeRawFunction (value : WireValue) : Option RawFunction := do
  let fields ← value.closedObject? ["name", "contractKind", "contract", "params", "captures", "body", "slotCount", "slotTypeFacts", "checkedReturnType"]
  let kind ← textField fields "contractKind"
  let id ← wordField fields "contract"
  let contract ← match kind with
    | "ground" => if id != 0 && id != 4294967295 then some (.ground id) else none
    | "scheme" => if id != 4294967295 then some (.scheme id) else none
    | _ => none
  pure ⟨← wordField fields "name", contract, ← rawRangeField fields "params", ← rawRangeField fields "captures",
    ← wordField fields "body", ← wordField fields "slotCount", ← rawRangeField fields "slotTypeFacts",
    ← wordField fields "checkedReturnType"⟩

def decodeRawFunctionRows (value : WireValue) : Option RawFunctionRows := do
  let fields ← value.closedObject? ["blocks", "functions"]
  pure ⟨← listField fields "blocks" decodeRawBlock, ← listField fields "functions" decodeRawFunction⟩

end Xsh.Conformance.Connected
