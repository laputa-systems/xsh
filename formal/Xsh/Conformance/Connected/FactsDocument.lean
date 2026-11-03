import Xsh.Conformance.Connected.FieldFacts
import Xsh.Conformance.Transport.Json
import Xsh.Conformance.Transport.Codecs

namespace Xsh.Conformance.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Typing.Connected
open Xsh.Conformance.Transport

structure ReadBinding where
  name : String
  site : SourceSite
  moduleName : Option String
  global : Bool
  exported : Bool
  deriving DecidableEq

inductive EffectFact where
  | unknown
  | known (names : List String)
  deriving DecidableEq

inductive CallableKind where
  | pure
  | proc
  | stream
  deriving DecidableEq

inductive InstantiationRole where
  | call
  | value
  deriving DecidableEq

structure FunctionTarget where
  definition : Nat
  name : String
  moduleName : Option String
  body : SourceSite
  deriving DecidableEq

structure BodyDocument where
  site : SourceSite
  kind : CallableKind
  typeParameters : List Nat
  parameters : List (String × MonoType)
  result : MonoType
  explicitReturn : Bool
  effects : EffectFact
  inferredEffects : Bool
  operations : List FieldOperation
  expressionTypes : List (Nat × SourceSite × MonoType)
  tailTypes : List (Nat × SourceSite × MonoType)
  readBindings : List ReadBinding
  resolvedReads : List (Nat × ReadBinding)
  resolvedTails : List (Nat × ReadBinding)
  operationSites : List (Nat × Nat)
  deriving DecidableEq

structure CallDocument where
  expression : Nat
  site : SourceSite
  role : InstantiationRole
  target : FunctionTarget
  typeArguments : List (Nat × MonoType)
  operationBindings : List OperationBinding
  deriving DecidableEq

structure FactsDocument where
  functions : List FunctionTarget
  bodies : List BodyDocument
  calls : List CallDocument
  deriving DecidableEq

def textField (fields : List (String × WireValue)) (name : String) : Option String :=
  (WireValue.field? fields name).bind WireValue.text?

def wordField (fields : List (String × WireValue)) (name : String) : Option Nat := do
  pure (← (textField fields name).bind decodeHex32?).val

def listField (fields : List (String × WireValue)) (name : String)
    (decode : WireValue → Option α) : Option (List α) :=
  ((WireValue.field? fields name).bind WireValue.items?).bind fun items => items.mapM decode

def boolField (fields : List (String × WireValue)) (name : String) : Option Bool :=
  match textField fields name with
  | some "true" => some true
  | some "false" => some false
  | _ => none

def optionalText (fields : List (String × WireValue)) (name : String) : Option (Option String) := do
  let values ← listField fields name WireValue.text?
  match values with
  | [] => some none
  | [value] => some (some value)
  | _ => none

def site (value : WireValue) : Option SourceSite := do
  let fields ← value.closedObject? ["source", "start", "length"]
  pure ⟨← wordField fields "source", ← wordField fields "start", ← wordField fields "length"⟩

def siteField (fields : List (String × WireValue)) (name : String) : Option SourceSite :=
  (WireValue.field? fields name).bind site

abbrev NameMap := String → Option NameId
abbrev VariableMap := Nat → Option Variable

mutual
  def decodeType : Nat → NameMap → VariableMap → WireValue → Option MonoType
    | 0, _, _, _ => none
    | fuel + 1, names, variables, value => do
      let .object fields := value | none
      match textField fields "kind" with
      | some "parameter" =>
        let fields ← value.closedObject? ["kind", "id"]
        let typeVariable ← variables (← wordField fields "id")
        if typeVariable.kind = .value then some (.variable typeVariable) else none
      | some "int" => do
        let _ ← value.closedObject? ["kind"]
        some (.primitive .integer)
      | some "bool" => do
        let _ ← value.closedObject? ["kind"]
        some (.primitive .boolean)
      | some "str" => do
        let _ ← value.closedObject? ["kind"]
        some (.primitive .string)
      | some "error" => do
        let _ ← value.closedObject? ["kind"]
        some (.primitive .error)
      | some "unit" => do
        let _ ← value.closedObject? ["kind"]
        some (.primitive .unit)
      | some "result" => do
        let fields ← value.closedObject? ["kind", "value", "error"]
        let left ← (WireValue.field? fields "value").bind (decodeType fuel names variables)
        let right ← (WireValue.field? fields "error").bind (decodeType fuel names variables)
        some (.result left right)
      | some "record" => do
        let fields ← value.closedObject? ["kind", "fields"]
        let fields ← listField fields "fields" (decodeTypeField fuel names variables)
        if (fields.map Prod.fst).Nodup then
          some (.record ((canonicalFields fields).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
        else none
      | _ => none

  def decodeTypeField (fuel : Nat) (names : NameMap) (variables : VariableMap)
      (value : WireValue) : Option (NameId × MonoType) := do
    let fields ← value.closedObject? ["name", "type"]
    let name ← names (← textField fields "name")
    let ty ← (WireValue.field? fields "type").bind (decodeType fuel names variables)
    pure (name, ty)
end

def decodeRead (value : WireValue) : Option ReadBinding := do
  let fields ← value.closedObject? ["name", "span", "namespace", "global", "exported"]
  pure ⟨← textField fields "name", ← siteField fields "span", ← optionalText fields "namespace",
    ← boolField fields "global", ← boolField fields "exported"⟩

def decodeEffects (value : WireValue) : Option EffectFact := do
  let fields ← value.closedObject? ["kind", "effects"]
  let effects ← listField fields "effects" WireValue.text?
  match textField fields "kind" with
  | some "unknown" => if effects.isEmpty then some .unknown else none
  | some "known" => some (.known effects)
  | _ => none

def decodeOperation (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option FieldOperation := do
  let fields ← value.closedObject? ["kind", "receiver", "name", "result", "span"]
  if textField fields "kind" = some "field" then pure () else none
  pure ⟨← siteField fields "span",
    ← (WireValue.field? fields "receiver").bind (decodeType fuel names variables),
    ← names (← textField fields "name"),
    ← (WireValue.field? fields "result").bind (decodeType fuel names variables)⟩

def decodeTypedSite (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option (Nat × SourceSite × MonoType) := do
  let fields ← value.closedObject? ["id", "span", "type"]
  pure (← wordField fields "id", ← siteField fields "span",
    ← (WireValue.field? fields "type").bind (decodeType fuel names variables))

def decodeResolvedRead (value : WireValue) : Option (Nat × ReadBinding) := do
  let fields ← value.closedObject? ["expression", "binding"]
  pure (← wordField fields "expression", ← (WireValue.field? fields "binding").bind decodeRead)

def decodeResolvedTail (value : WireValue) : Option (Nat × ReadBinding) := do
  let fields ← value.closedObject? ["statement", "binding"]
  pure (← wordField fields "statement", ← (WireValue.field? fields "binding").bind decodeRead)

def decodeOperationSite (value : WireValue) : Option (Nat × Nat) := do
  let fields ← value.closedObject? ["expression", "operation"]
  pure (← wordField fields "expression", ← wordField fields "operation")

def decodeParameter (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option (String × MonoType) := do
  let fields ← value.closedObject? ["name", "type"]
  pure (← textField fields "name", ← (WireValue.field? fields "type").bind (decodeType fuel names variables))

def decodeWord (value : WireValue) : Option Nat := do
  pure (← value.text?.bind decodeHex32?).val

def decodeBody (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option BodyDocument := do
  let fields ← value.closedObject? ["span", "kind", "typeParameters", "parameters", "result", "explicitReturn",
    "effects", "inferredEffects", "operations", "expressionTypes", "tailTypes", "readBindings",
    "resolvedReads", "resolvedTails", "operationSites"]
  let kind ← match textField fields "kind" with
    | some "pure" => some CallableKind.pure
    | some "proc" => some CallableKind.proc
    | some "stream" => some CallableKind.stream
    | _ => none
  pure ⟨← siteField fields "span", kind, ← listField fields "typeParameters" decodeWord,
    ← listField fields "parameters" (decodeParameter fuel names variables),
    ← (WireValue.field? fields "result").bind (decodeType fuel names variables),
    ← boolField fields "explicitReturn", ← (WireValue.field? fields "effects").bind decodeEffects,
    ← boolField fields "inferredEffects", ← listField fields "operations" (decodeOperation fuel names variables),
    ← listField fields "expressionTypes" (decodeTypedSite fuel names variables),
    ← listField fields "tailTypes" (decodeTypedSite fuel names variables),
    ← listField fields "readBindings" decodeRead, ← listField fields "resolvedReads" decodeResolvedRead,
    ← listField fields "resolvedTails" decodeResolvedTail, ← listField fields "operationSites" decodeOperationSite⟩

def decodeTarget (value : WireValue) : Option FunctionTarget := do
  let fields ← value.closedObject? ["definition", "name", "namespace", "body"]
  pure ⟨← wordField fields "definition", ← textField fields "name", ← optionalText fields "namespace",
    ← siteField fields "body"⟩

def decodeTypeArgument (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option (Nat × MonoType) := do
  let fields ← value.closedObject? ["parameter", "type"]
  pure (← wordField fields "parameter", ← (WireValue.field? fields "type").bind (decodeType fuel names variables))

def decodeBinding (value : WireValue) : Option OperationBinding :=
  match value.text? with
  | some "field" => some .field
  | _ => none

def decodeCall (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option CallDocument := do
  let fields ← value.closedObject? ["expression", "span", "role", "target", "typeArguments", "operationBindings"]
  let role ← match textField fields "role" with
    | some "call" => some InstantiationRole.call
    | some "value" => some InstantiationRole.value
    | _ => none
  pure ⟨← wordField fields "expression", ← siteField fields "span", role,
    ← (WireValue.field? fields "target").bind decodeTarget,
    ← listField fields "typeArguments" (decodeTypeArgument fuel names variables),
    ← listField fields "operationBindings" decodeBinding⟩

abbrev NameResolver := Nat → NameMap
abbrev VariableResolver := SourceSite → VariableMap

def decodeLocatedBody (fuel : Nat) (names : NameResolver) (variables : VariableResolver)
    (value : WireValue) : Option BodyDocument := do
  let .object fields := value | none
  let body ← siteField fields "span"
  decodeBody fuel (names body.source) (variables body) value

def decodeLocatedCall (fuel : Nat) (names : NameResolver) (variables : VariableResolver)
    (value : WireValue) : Option CallDocument := do
  let .object fields := value | none
  let call ← siteField fields "span"
  let target ← (WireValue.field? fields "target").bind decodeTarget
  decodeCall fuel (names call.source) (variables target.body) value

def decodeFactsValue (fuel : Nat) (names : NameResolver) (variables : VariableResolver)
    (value : WireValue) : Option FactsDocument := do
  let fields ← value.closedObject? ["schema", "functions", "bodies", "calls"]
  if textField fields "schema" = some "xsh-connected-field-facts-v1" then pure () else none
  pure ⟨← listField fields "functions" decodeTarget,
    ← listField fields "bodies" (decodeLocatedBody fuel names variables),
    ← listField fields "calls" (decodeLocatedCall fuel names variables)⟩

def decodeFactsDocument (fuel : Nat) (names : NameResolver) (variables : VariableResolver)
    (codes : ExchangeCodes) : Option FactsDocument := do
  let value ← parseCodes codes
  decodeFactsValue fuel names variables value

private def exampleNames : NameMap := fun name =>
  if name = "amount" then some generatedProjectionField
  else if name = "enabled" then some additionalField
  else none

private def exampleVariables : VariableMap := fun id => some ⟨id, .value⟩

private def exampleRecord : WireValue := .object [
  ("kind", .string "record"),
  ("fields", .array [
    .object [("name", .string "enabled"), ("type", .object [("kind", .string "bool")])],
    .object [("name", .string "amount"), ("type", .object [("kind", .string "int")])]
  ])]

theorem decoded_record_preserves_fields_and_checks_names :
    decodeType 8 exampleNames exampleVariables exampleRecord =
      some (.record (.rowField generatedProjectionField (.primitive .integer)
        (.rowField additionalField (.primitive .boolean) .rowEmpty))) ∧
    (decodeType 8 (fun _ => none) exampleVariables exampleRecord).isNone = true := by
  simp [decodeType, decodeTypeField, exampleNames, exampleRecord,
    WireValue.closedObject?, WireValue.field?, WireValue.items?, WireValue.text?,
    listField, textField, canonicalFields, insertField, generatedProjectionField, additionalField,
    exampleName, NameId.value]
  decide +kernel

theorem unknown_and_empty_effects_remain_distinct :
    decodeEffects (.object [("kind", .string "unknown"), ("effects", .array [])]) = some .unknown ∧
    decodeEffects (.object [("kind", .string "known"), ("effects", .array [])]) = some (.known []) ∧
    (decodeEffects (.object [("kind", .string "unknown"), ("effects", .array [.string "time"])] )).isNone = true := by
  constructor <;> decide +kernel

theorem field_binding_decoder_rejects_other_operations :
    decodeBinding (.string "field") = some .field ∧ (decodeBinding (.string "index")).isNone = true := by
  constructor <;> decide +kernel

private def scopedBody (start : String) : WireValue := .object [
  ("span", .object [("source", .string "00000000"), ("start", .string start), ("length", .string "00000001")]),
  ("kind", .string "pure"), ("typeParameters", .array [.string "00000000"]),
  ("parameters", .array []), ("result", .object [("kind", .string "parameter"), ("id", .string "00000000")]),
  ("explicitReturn", .string "false"),
  ("effects", .object [("kind", .string "unknown"), ("effects", .array [])]),
  ("inferredEffects", .string "false"), ("operations", .array []), ("expressionTypes", .array []),
  ("tailTypes", .array []), ("readBindings", .array []), ("resolvedReads", .array []),
  ("resolvedTails", .array []), ("operationSites", .array [])]

theorem scheme_variables_are_scoped_by_body_site :
    (decodeLocatedBody 8 (fun _ _ => none) (fun body id => some ⟨body.start + id, .value⟩)
      (scopedBody "00000014")).map BodyDocument.result = some (.variable ⟨20, .value⟩) ∧
    (decodeLocatedBody 8 (fun _ _ => none) (fun body id => some ⟨body.start + id, .value⟩)
      (scopedBody "00000015")).map BodyDocument.result = some (.variable ⟨21, .value⟩) := by
  simp [decodeLocatedBody, decodeBody, decodeType, siteField, site, wordField, decodeWord,
    boolField, listField, textField, scopedBody, WireValue.closedObject?, WireValue.field?,
    WireValue.items?, WireValue.text?]
  constructor <;> decide +kernel

end Xsh.Conformance.Connected
