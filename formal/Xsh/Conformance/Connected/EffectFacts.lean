import Xsh.Conformance.Connected.SourceAdmission
import Xsh.Conformance.Connected.Documents

namespace Xsh.Conformance.Connected

open Xsh.Syntax
open Xsh.Conformance.Transport

structure EffectIdentity where
  moduleName : Option String
  body : SourceSite
  deriving DecidableEq

inductive CheckedEffectContract where
  | unknown
  | known (direct : List Effect) (variables : List (Nat × List Effect))
  deriving DecidableEq

structure DeclarationEffectFact where
  identity : EffectIdentity
  effective : CheckedEffectContract
  required : CheckedEffectContract
  owner : Option EffectIdentity
  parameters : List Nat
  inferred : Bool
  inferenceAllowed : Bool
  redundantClause : Bool
  unknownChain : List String
  deriving DecidableEq

def decodeEffect : WireValue → Option Effect
  | .string "fs" => some .fs
  | .string "net" => some .net
  | .string "process" => some .process
  | .string "env" => some .env
  | .string "time" => some .time
  | .string "error" => some .error
  | .string "io" => some .io
  | _ => none

def decodeEffectVariable (value : WireValue) : Option (Nat × List Effect) := do
  let fields ← value.closedObject? ["id", "mask"]
  let id ← wordField fields "id"
  let mask ← listField fields "mask" decodeEffect
  if mask.Nodup then some (id, mask) else none

def decodeEffectContract (value : WireValue) : Option CheckedEffectContract := do
  let fields ← value.closedObject? ["kind", "direct", "variables"]
  let direct ← listField fields "direct" decodeEffect
  let variables ← listField fields "variables" decodeEffectVariable
  if direct.Nodup ∧ (variables.map Prod.fst).Nodup then pure () else none
  match textField fields "kind" with
  | some "unknown" => if direct.isEmpty && variables.isEmpty then some .unknown else none
  | some "known" => some (.known direct variables)
  | _ => none

def decodeEffectIdentity (value : WireValue) : Option EffectIdentity := do
  let fields ← value.closedObject? ["namespace", "body"]
  pure ⟨← optionalText fields "namespace", ← siteField fields "body"⟩

def decodeEffectOwner (value : WireValue) : Option (Option EffectIdentity) := do
  let values ← value.items?
  match values with
  | [] => some none
  | [owner] => some (some (← decodeEffectIdentity owner))
  | _ => none

def decodeDeclarationEffect (value : WireValue) : Option DeclarationEffectFact := do
  let fields ← value.closedObject? ["identity", "effective", "required", "owner", "parameters",
    "inferred", "inferenceAllowed", "redundantClause", "unknownChain"]
  let parameters ← listField fields "parameters" decodeWord
  if parameters.Nodup then pure () else none
  pure ⟨← (WireValue.field? fields "identity").bind decodeEffectIdentity,
    ← (WireValue.field? fields "effective").bind decodeEffectContract,
    ← (WireValue.field? fields "required").bind decodeEffectContract,
    ← (WireValue.field? fields "owner").bind decodeEffectOwner,
    parameters, ← boolField fields "inferred", ← boolField fields "inferenceAllowed",
    ← boolField fields "redundantClause", ← listField fields "unknownChain" WireValue.text?⟩

def decodeDeclarationEffects (value : WireValue) : Option (List DeclarationEffectFact) := do
  let fields ← value.closedObject? ["schema", "declarations"]
  if textField fields "schema" = some "xsh-connected-declaration-effects-v1" then pure () else none
  let facts ← listField fields "declarations" decodeDeclarationEffect
  if (facts.map DeclarationEffectFact.identity).Nodup then some facts else none

def clockEffectIdentity : EffectIdentity := ⟨none, ⟨0, 27, 14⟩⟩
def clockEffectFact : DeclarationEffectFact :=
  ⟨clockEffectIdentity, .known [.time] [], .known [.time] [], some clockEffectIdentity,
    [], false, true, true, []⟩

set_option maxRecDepth 100000 in
theorem emitted_clock_effects_decode : decodeDeclarationEffects effectFactsWire = some [clockEffectFact] := by
  simp [decodeDeclarationEffects, decodeDeclarationEffect, decodeEffectIdentity, decodeEffectOwner,
    decodeEffectContract, decodeEffectVariable, decodeEffect, textField, wordField, listField, boolField,
    optionalText, siteField, site, decodeWord, WireValue.closedObject?, WireValue.field?, WireValue.items?,
    WireValue.text?, effectFactsWire, clockEffectFact, clockEffectIdentity]
  decide +kernel

theorem emitted_clock_effect_bytes_decode :
    (parseCodes effectFactsCodes).bind decodeDeclarationEffects = some [clockEffectFact] := by
  rw [effect_facts_parse_to_wire]
  exact emitted_clock_effects_decode

end Xsh.Conformance.Connected
