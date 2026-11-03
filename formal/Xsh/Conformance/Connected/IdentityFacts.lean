import Xsh.Conformance.Connected.ActualFacts

namespace Xsh.Conformance.Connected

open Xsh.Typing.Connected

def identitySource : String :=
  "pure identity(value) { value }\npure one() -> Int { identity(1) }\npure truth() -> Bool { identity(true) }\nassert one() == 1\nassert truth()\n"

def identityVariables : VariableResolver := fun body id =>
  if body = ⟨0, 21, 9⟩ && id == 0 then some ⟨0, .value⟩ else none

def decodedIdentityFacts : Option FactsDocument :=
  match admitSource 0 identitySource with
  | .ok source => decodeAdmittedFacts 32 source identityVariables identityFactsCodes
  | .error _ => none

def decodedIdentityWire : Option FactsDocument :=
  match admitSource 0 identitySource with
  | .ok source => decodeFactsValue 32 (sourceNameResolver source) identityVariables identityFactsWire
  | .error _ => none

set_option maxHeartbeats 800000 in
theorem decoded_identity_uses_shared_wire : decodedIdentityFacts = decodedIdentityWire := by
  unfold decodedIdentityFacts decodedIdentityWire
  cases admitted : admitSource 0 identitySource with
  | error error => rfl
  | ok source =>
    simp only [decodeAdmittedFacts, decodeFactsDocument, identity_facts_parse_to_wire]
    rfl

def identityNames : List String :=
  ["Int", "Bool", "Str", "Result", "pure", "identity", "value", "one", "truth", "true", "assert"]

def identityNameResolver : NameResolver := fun source name =>
  if source = 0 then sourceName identityNames name else none

theorem admitted_identity_has_expected_names :
    (match admitSource 0 identitySource with | .ok source => source.names | .error _ => []) = identityNames := by
  decide +kernel

def decodedIdentityWithNames : Option FactsDocument :=
  decodeFactsValue 32 identityNameResolver identityVariables identityFactsWire

theorem admitted_identity_uses_names : decodedIdentityWire = decodedIdentityWithNames := by
  unfold decodedIdentityWire decodedIdentityWithNames
  cases admitted : admitSource 0 identitySource with
  | error error =>
    have same := admitted_identity_has_expected_names
    rw [admitted] at same
    have impossible : False := by simpa [identityNames] using same
    exact False.elim impossible
  | ok source =>
    have names := admitted_identity_has_expected_names
    rw [admitted] at names
    simp only at names
    simp only [admitted]
    have sourceIdentity := (admitted_source_has_tokenization 0 identitySource source admitted).choose_spec.2.1
    have equal : sourceNameResolver source = identityNameResolver := by
      funext origin name
      simp [sourceNameResolver, identityNameResolver, sourceIdentity, names]
    rw [equal]

def identityRead : ReadBinding := ⟨"value", ⟨0, 14, 5⟩, none, false, false⟩
def identityBodyFacts : BodyDocument :=
  ⟨⟨0, 21, 9⟩, .pure, [0], [("value", .variable ⟨0, .value⟩)], .variable ⟨0, .value⟩,
    false, .unknown, false, [], [], [(0, ⟨0, 23, 5⟩, .variable ⟨0, .value⟩)],
    [identityRead], [], [(0, identityRead)], []⟩
def identityTarget : FunctionTarget := ⟨0, "identity", none, ⟨0, 21, 9⟩⟩
def identityFunctions : List FunctionTarget :=
  [identityTarget, ⟨1, "one", none, ⟨0, 49, 15⟩⟩, ⟨2, "truth", none, ⟨0, 86, 18⟩⟩]
def identityFirstCall : CallDocument :=
  ⟨2, ⟨0, 51, 11⟩, .call, identityTarget, [(0, .primitive .integer)], []⟩
def identitySecondCall : CallDocument :=
  ⟨5, ⟨0, 88, 14⟩, .call, identityTarget, [(0, .primitive .boolean)], []⟩
def identityCalls : List CallDocument := [identityFirstCall, identitySecondCall]
def identityFacts : FactsDocument := ⟨identityFunctions, [identityBodyFacts], identityCalls⟩

set_option maxRecDepth 100000 in
theorem identity_wire_decodes_exactly : decodedIdentityWithNames = some identityFacts := by
  simp [decodedIdentityWithNames, decodeFactsValue, decodeLocatedBody, decodeLocatedCall, decodeBody, decodeCall,
    decodeType, decodeTypeField, decodeRead, decodeEffects, decodeOperation, decodeTypedSite,
    decodeResolvedRead, decodeResolvedTail, decodeOperationSite, decodeParameter, decodeWord, decodeTarget,
    decodeTypeArgument, decodeBinding, textField, wordField, listField, boolField, optionalText,
    siteField, site, Xsh.Conformance.Transport.WireValue.closedObject?, Xsh.Conformance.Transport.WireValue.field?,
    Xsh.Conformance.Transport.WireValue.items?, Xsh.Conformance.Transport.WireValue.text?,
    identityFactsWire, identityNameResolver, identityNames, sourceName, identityVariables,
    canonicalFields, insertField, identityFacts, identityFunctions, identityBodyFacts, identityCalls,
    identityTarget, identityFirstCall, identitySecondCall, identityRead, Xsh.Foundation.NameId.value]
  decide +kernel

theorem rust_identity_facts_decode_exactly : decodedIdentityFacts = some identityFacts := by
  rw [decoded_identity_uses_shared_wire, admitted_identity_uses_names]
  exact identity_wire_decodes_exactly

def identityValidation (functions : List FunctionTarget) (body : BodyDocument)
    (first second : CallDocument) : Except ValidationError Unit := do
  let unit ← match admitSource 0 identitySource with
    | .ok source => pure source
    | .error .exhausted => throw .exhausted
    | .error .unsupported => throw .unsupported
    | .error .malformed => throw .mismatch
  let [.function source, .function firstCaller, .function secondCaller, .statement _ _ _, .statement _ _ _] := unit.declarations
    | throw .unsupported
  let some intName := sourceName unit.names "Int" | throw .unsupported
  let some boolName := sourceName unit.names "Bool" | throw .unsupported
  let some stringName := sourceName unit.names "Str" | throw .unsupported
  let some resultName := sourceName unit.names "Result" | throw .unsupported
  let [parameter] := source.parameters | throw .unsupported
  let [.tailName readName] := source.body | throw .unsupported
  let binder : Xsh.Foundation.BinderId := ⟨⟨0, by decide⟩⟩
  let typeVariable : Variable := ⟨0, .value⟩
  let parameterBinding : Binding := ⟨parameter.name, binder, .immutable, ⟨[], .variable typeVariable, []⟩⟩
  let names : TypeNames := ⟨intName, boolName, stringName, resultName⟩
  let candidate : List StatementNode := [.evaluated (.read binder [])]
  requireCheck (checksSourceFunctions unit functions && decide (first.target ∈ functions) &&
    decide (second.target ∈ functions) && decide (first.target = second.target) &&
    decide (parameter.name = readName) && decide (first.operationBindings = []) &&
    decide (second.operationBindings = []))
  let (_, generated) ← ((block 64 names [parameterBinding] (.variable typeVariable) source.body).run
    (initialBlockState [parameterBinding] (.variable typeVariable))).mapError generationFailure
  let some scheme := schemeFromBody identityVariables body | throw .mismatch
  let target : Binding := ⟨source.name, binder, .immutable, scheme⟩
  let firstResult ← validatePublishedCallFromSource 64 0 identitySource 1 functions [target] identity source.name binder
    scheme identityVariables body first
  let secondResult ← validatePublishedCallFromSource 64 0 identitySource 2 functions [target] identity source.name binder
    scheme identityVariables body second
  requireCheck (decide (firstResult = .primitive .integer) && decide (secondResult = .primitive .boolean))
  let some firstArguments := argumentsFromCall identityVariables body first | throw .mismatch
  let some secondArguments := argumentsFromCall identityVariables body second | throw .mismatch
  let _ ← validateSourceQualifiedBodyFacts 64 0 identitySource 0 identityVariables names [parameterBinding] [] (.variable typeVariable)
    generated.constraints.reverse [.reflexive] 1 candidate (fun _ => none) (fun _ => none) firstArguments body
  let _ ← validateSourceQualifiedBodyFacts 64 0 identitySource 0 identityVariables names [parameterBinding] [] (.variable typeVariable)
    generated.constraints.reverse [.reflexive] 1 candidate (fun _ => none) (fun _ => none) secondArguments body
  pure ()

def identityChecksFor (functions : List FunctionTarget) (body : BodyDocument)
    (first second : CallDocument) : Bool :=
  match identityValidation functions body first second with | .ok _ => true | .error _ => false

set_option maxRecDepth 100000 in
theorem admitted_identity_scheme_matches_rust_facts :
    identityChecksFor identityFunctions identityBodyFacts identityFirstCall identitySecondCall = true := by
  decide +kernel

def checkDecodedIdentity : Bool :=
  match decodedIdentityFacts with
  | some ⟨functions, [body], [first, second]⟩ => identityChecksFor functions body first second
  | _ => false

theorem rust_identity_scheme_checks_against_admitted_source : checkDecodedIdentity = true := by
  unfold checkDecodedIdentity
  rw [rust_identity_facts_decode_exactly]
  exact admitted_identity_scheme_matches_rust_facts

def wrongIdentityTail : BodyDocument :=
  { identityBodyFacts with tailTypes := [(0, ⟨0, 23, 5⟩, .primitive .integer)] }
def wrongIdentityCall : CallDocument :=
  { identityFirstCall with typeArguments := [(0, .primitive .boolean)] }

set_option maxRecDepth 100000 in
theorem identity_fact_mutations_are_refused :
    identityChecksFor identityFunctions wrongIdentityTail identityFirstCall identitySecondCall = false ∧
    identityChecksFor identityFunctions identityBodyFacts wrongIdentityCall identitySecondCall = false := by
  constructor <;> decide +kernel

end Xsh.Conformance.Connected
