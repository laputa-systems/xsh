import Xsh.Conformance.Connected.QualifiedNodeFacts

namespace Xsh.Conformance.Connected

open Xsh.Syntax
open Xsh.Typing.Connected
open Xsh.Conformance.Transport

inductive TransferRole where
  | blockValue
  | returned
  | assignment
  | breakValue
  | yielded
  deriving DecidableEq

structure TransferFact where
  role : TransferRole
  site : SourceSite
  transfers : Bool
  checkedOverride : Option Bool
  deriving DecidableEq

def decodeTransferRole : WireValue → Option TransferRole
  | .string "block" => some .blockValue
  | .string "return" => some .returned
  | .string "assignment" => some .assignment
  | .string "break" => some .breakValue
  | .string "yield" => some .yielded
  | _ => none

def decodeTransferFact (value : WireValue) : Option TransferFact := do
  let fields ← value.closedObject? ["role", "site", "transfers", "checkedOverride"]
  let overrides ← listField fields "checkedOverride" WireValue.text?
  let checkedOverride ← match overrides with
    | [] => some none
    | ["true"] => some (some true)
    | ["false"] => some (some false)
    | _ => none
  pure ⟨← (WireValue.field? fields "role").bind decodeTransferRole,
    ← siteField fields "site", ← boolField fields "transfers", checkedOverride⟩

def decodeTransferFacts (value : WireValue) : Option (List TransferFact) := do
  let fields ← value.closedObject? ["schema", "facts"]
  if textField fields "schema" = some "xsh-connected-resource-transfers-v1" then pure () else none
  let facts ← listField fields "facts" decodeTransferFact
  if (facts.map fun fact => (fact.role, fact.site)).Nodup then some facts else none

def checkerMayHoldResource : MonoType → Option Bool
  | .variable _ | .record _ | .primitive .error => some true
  | .primitive .integer | .primitive .boolean | .primitive .string | .primitive .unit => some false
  | .result value failure => do some ((← checkerMayHoldResource value) || (← checkerMayHoldResource failure))
  | .nominal _ base => checkerMayHoldResource base
  | _ => none


def containsTypeParameter : MonoType → Bool
  | .variable _ => true
  | .function arguments result | .argument arguments result | .result arguments result =>
      containsTypeParameter arguments || containsTypeParameter result
  | .record row | .nominal _ row => containsTypeParameter row
  | .rowField _ value tail => containsTypeParameter value || containsTypeParameter tail
  | _ => false

def expectedFunctionBlockTransfer (source : SourceFunction) (result : MonoType) : Option Bool :=
  match source.body.getLast? with
  | none => some false
  | some (.returnValue _) => some false
  | some (.expression _) | some (.tailName _) => checkerMayHoldResource result
  | _ => none


def expectedFunctionBlockOverride (source : SourceFunction) (result : MonoType) : Option (Option Bool) :=
  match source.body.getLast? with
  | none | some (.returnValue _) => some none
  | some (.expression _) | some (.tailName _) =>
    if containsTypeParameter result then some none else (checkerMayHoldResource result).map some
  | _ => none

def checkFunctionBlockTransfer (source : SourceFunction) (result : MonoType)
    (facts : List TransferFact) : Bool :=
  match expectedFunctionBlockTransfer source result, expectedFunctionBlockOverride source result,
      facts.find? (fun fact => decide (fact.role = .blockValue ∧ fact.site = source.bodySite)) with
  | some expected, some override, some actual =>
      decide (actual.transfers = expected) && decide (actual.checkedOverride = override)
  | _, _, _ => false

theorem checked_function_block_transfer (source : SourceFunction) (result : MonoType)
    (facts : List TransferFact) (accepted : checkFunctionBlockTransfer source result facts = true) :
    ∃ fact expected, fact ∈ facts ∧ fact.role = .blockValue ∧ fact.site = source.bodySite ∧
      expectedFunctionBlockTransfer source result = some expected ∧ fact.transfers = expected ∧
      expectedFunctionBlockOverride source result = some fact.checkedOverride := by
  cases expected : expectedFunctionBlockTransfer source result with
  | none => simp [checkFunctionBlockTransfer, expected] at accepted
  | some expectedValue =>
    cases override : expectedFunctionBlockOverride source result with
    | none => simp [checkFunctionBlockTransfer, expected, override] at accepted
    | some expectedOverride =>
      cases found : facts.find? (fun fact => decide (fact.role = .blockValue ∧ fact.site = source.bodySite)) with
      | none => simp only [checkFunctionBlockTransfer, expected, override, found, Bool.false_eq_true] at accepted
      | some fact =>
        have selected := List.find?_some found
        simp only [decide_eq_true_eq] at selected
        simp only [checkFunctionBlockTransfer, expected, override, found, Bool.and_eq_true, decide_eq_true_eq] at accepted
        exact ⟨fact, expectedValue, List.mem_of_find?_eq_some found, selected.1, selected.2, rfl, accepted.1,
          by rw [accepted.2]⟩

def validateSourceFactsAndBlockTransfer (fuel : Nat) (sourceId : Nat) (text : String)
    (factsCodes transferCodes : ExchangeCodes) (declaration bodyIndex : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType)) :
    Except ValidationError Substitution := do
  let principal ← validateSourceFactsBody fuel sourceId text factsCodes declaration bodyIndex names parameters globals
    result ordered steps solvedCount candidate binderTypes fields inhabitant
  let unit ← match admitSource sourceId text with
    | .ok unit => pure unit
    | .error .exhausted => throw .exhausted
    | .error .unsupported => throw .unsupported
    | .error .malformed => throw .mismatch
  let some (facts, _) := decodeSelfScopedFactsDocument fuel (sourceNameResolver unit) factsCodes | throw .mismatch
  let some body := facts.bodies[bodyIndex]? | throw .mismatch
  let some (.function source) := unit.declarations[declaration]? | throw .mismatch
  let some transfers := (parseCodes transferCodes).bind decodeTransferFacts | throw .mismatch
  if (expectedFunctionBlockTransfer source body.result).isSome then pure () else throw .unsupported
  requireCheck (checkFunctionBlockTransfer source body.result transfers)
  pure principal

theorem validated_source_facts_block_transfer (fuel : Nat) (sourceId : Nat) (text : String)
    (factsCodes transferCodes : ExchangeCodes) (declaration bodyIndex : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType))
    (principal : Substitution)
    (accepted : validateSourceFactsAndBlockTransfer fuel sourceId text factsCodes transferCodes declaration bodyIndex
      names parameters globals result ordered steps solvedCount candidate binderTypes fields inhabitant = .ok principal) :
    ∃ unit facts scopes source body scheme transfers fact expected,
      admitSource sourceId text = .ok unit ∧
      decodeSelfScopedFactsDocument fuel (sourceNameResolver unit) factsCodes = some (facts, scopes) ∧
      facts.bodies[bodyIndex]? = some body ∧ unit.declarations[declaration]? = some (.function source) ∧
      schemeFromBody (scopedVariableResolver scopes) body = some scheme ∧
      (parseCodes transferCodes).bind decodeTransferFacts = some transfers ∧
      fact ∈ transfers ∧ fact.role = .blockValue ∧ fact.site = source.bodySite ∧
      expectedFunctionBlockTransfer source body.result = some expected ∧ fact.transfers = expected ∧
      expectedFunctionBlockOverride source body.result = some fact.checkedOverride ∧
      ∀ later, WellSorted later → Solves scheme.requirements later →
        (∃ generated state,
          (block fuel names (parameters ++ globals) result source.body).run
            (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
          QualifiedScheme .generalize (globalEnvironment globals)
            (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
          BlockHasType names (compose later principal) (parameters ++ globals)
            (substitute (compose later principal) result) source.body) ∧
        NodeBlockHasType (compose later principal) (parameters ++ globals)
          (substitute (compose later principal) result) candidate := by
  cases base : validateSourceFactsBody fuel sourceId text factsCodes declaration bodyIndex names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant with
  | error error => simp [validateSourceFactsAndBlockTransfer, base, except_bind_ok] at accepted
  | ok chosen =>
    obtain ⟨unit, facts, scopes, source, body, scheme, admitted, decoded, selected, member, schemeDecoded, typed⟩ :=
      validated_source_facts_nodes_every_solution fuel sourceId text factsCodes declaration bodyIndex names parameters globals
        result ordered steps solvedCount candidate binderTypes fields inhabitant chosen base
    cases transferDecoded : (parseCodes transferCodes).bind decodeTransferFacts with
    | none => simp [validateSourceFactsAndBlockTransfer, base, admitted, decoded, selected, member, transferDecoded, except_bind_ok] at accepted
    | some transfers =>
      cases expected : expectedFunctionBlockTransfer source body.result with
      | none => simp [validateSourceFactsAndBlockTransfer, base, admitted, decoded, selected, member, transferDecoded, expected, except_bind_ok] at accepted
      | some expectedValue =>
        simp [validateSourceFactsAndBlockTransfer, base, admitted, decoded, selected, member, transferDecoded, expected,
          except_bind_ok, except_map_ok, exists_require_ok] at accepted
        obtain ⟨checked, same⟩ := accepted
        subst chosen
        obtain ⟨fact, expectedValue, factIn, role, site, classified, sameTransfer, overrideAgrees⟩ :=
          checked_function_block_transfer source body.result transfers checked
        exact ⟨unit, facts, scopes, source, body, scheme, transfers, fact, expectedValue,
          admitted, decoded, selected, member, schemeDecoded, rfl,
          factIn, role, site, classified, sameTransfer, overrideAgrees, typed⟩

end Xsh.Conformance.Connected
