import Xsh.Conformance.Connected.SparseFacts
import Xsh.Conformance.Connected.Documents
import Xsh.Conformance.Connected.QualifiedFacts

namespace Xsh.Conformance.Connected

open Xsh.Typing.Connected

def projectionVariables : VariableResolver := fun body id =>
  if body = ⟨0, 20, 16⟩ && (id == 0 || id == 1) then some ⟨id, .value⟩ else none

def decodedProjectionFacts : Option FactsDocument :=
  match admitSource 0 projectionSource with
  | .ok source => decodeAdmittedFacts 32 source projectionVariables connectedFactsCodes
  | .error _ => none

def decodedProjectionWire : Option FactsDocument :=
  match admitSource 0 projectionSource with
  | .ok source => decodeFactsValue 32 (sourceNameResolver source) projectionVariables connectedFactsWire
  | .error _ => none

set_option maxHeartbeats 800000 in
theorem decoded_projection_uses_shared_wire : decodedProjectionFacts = decodedProjectionWire := by
  unfold decodedProjectionFacts decodedProjectionWire
  cases admitted : admitSource 0 projectionSource with
  | error error => rfl
  | ok source =>
    simp only [decodeAdmittedFacts, decodeFactsDocument, connected_facts_parse_to_wire]
    rfl


def projectionNames : List String :=
  ["Int", "Bool", "Str", "Result", "pure", "project", "value", "amount", "use_project", "enabled", "true", "assert"]

def projectionNameResolver : NameResolver := fun source name =>
  if source = 0 then sourceName projectionNames name else none

theorem admitted_projection_has_expected_names :
    (match admitSource 0 projectionSource with | .ok source => source.names | .error _ => []) = projectionNames := by
  decide +kernel

def decodedProjectionWithNames : Option FactsDocument :=
  decodeFactsValue 32 projectionNameResolver projectionVariables connectedFactsWire

theorem admitted_projection_uses_names : decodedProjectionWire = decodedProjectionWithNames := by
  unfold decodedProjectionWire decodedProjectionWithNames
  cases admitted : admitSource 0 projectionSource with
  | error error =>
    have same := admitted_projection_has_expected_names
    rw [admitted] at same
    have impossible : False := by simpa [projectionNames] using same
    exact False.elim impossible
  | ok source =>
    have names := admitted_projection_has_expected_names
    rw [admitted] at names
    simp only at names
    simp only [admitted]
    have sourceIdentity := (admitted_source_has_tokenization 0 projectionSource source admitted).choose_spec.2.1
    have equal : sourceNameResolver source = projectionNameResolver := by
      funext origin name
      simp [sourceNameResolver, projectionNameResolver, sourceIdentity, names]
    rw [equal]

set_option maxRecDepth 100000 in
theorem projection_wire_decodes : decodedProjectionWithNames.isSome = true := by
  simp [decodedProjectionWithNames, decodeFactsValue, decodeLocatedBody, decodeLocatedCall, decodeBody, decodeCall,
    decodeType, decodeTypeField, decodeRead, decodeEffects, decodeOperation, decodeTypedSite,
    decodeResolvedRead, decodeResolvedTail, decodeOperationSite, decodeParameter, decodeWord, decodeTarget,
    decodeTypeArgument, decodeBinding, textField, wordField, listField, boolField, optionalText,
    siteField, site, Xsh.Conformance.Transport.WireValue.closedObject?, Xsh.Conformance.Transport.WireValue.field?,
    Xsh.Conformance.Transport.WireValue.items?, Xsh.Conformance.Transport.WireValue.text?,
    connectedFactsWire, projectionNameResolver, projectionNames, sourceName, projectionVariables,
    canonicalFields, insertField]
  decide +kernel

theorem rust_field_facts_decode_against_admitted_source : decodedProjectionFacts.isSome = true := by
  rw [decoded_projection_uses_shared_wire, admitted_projection_uses_names]
  exact projection_wire_decodes

def projectionAmount : Xsh.Foundation.NameId := ⟨⟨7, by decide⟩⟩
def projectionEnabled : Xsh.Foundation.NameId := ⟨⟨9, by decide⟩⟩
def projectionRead : ReadBinding := ⟨"value", ⟨0, 13, 5⟩, none, false, false⟩
def projectionField : FieldOperation :=
  ⟨⟨0, 22, 12⟩, .variable ⟨0, .value⟩, projectionAmount, .variable ⟨1, .value⟩⟩
def projectionBodyFacts : BodyDocument :=
  ⟨⟨0, 20, 16⟩, .pure, [0, 1], [("value", .variable ⟨0, .value⟩)], .variable ⟨1, .value⟩,
    false, .unknown, false, [projectionField],
    [(0, ⟨0, 22, 5⟩, .variable ⟨0, .value⟩), (1, ⟨0, 22, 12⟩, .variable ⟨1, .value⟩)],
    [], [projectionRead], [(0, projectionRead)], [], [(1, 0)]⟩
def projectionCallFacts : CallDocument :=
  ⟨8, ⟨0, 65, 35⟩, .call, ⟨0, "project", none, ⟨0, 20, 16⟩⟩,
    [(0, .record (.rowField projectionAmount (.primitive .integer)
      (.rowField projectionEnabled (.primitive .boolean) .rowEmpty))),
     (1, .primitive .integer)], [.field]⟩
def projectionFunctions : List FunctionTarget :=
  [⟨0, "project", none, ⟨0, 20, 16⟩⟩, ⟨1, "use_project", none, ⟨0, 63, 39⟩⟩]
def projectionFacts : FactsDocument := ⟨projectionFunctions, [projectionBodyFacts], [projectionCallFacts]⟩

set_option maxRecDepth 100000 in
theorem projection_wire_decodes_exactly : decodedProjectionWithNames = some projectionFacts := by
  simp [decodedProjectionWithNames, decodeFactsValue, decodeLocatedBody, decodeLocatedCall, decodeBody, decodeCall,
    decodeType, decodeTypeField, decodeRead, decodeEffects, decodeOperation, decodeTypedSite,
    decodeResolvedRead, decodeResolvedTail, decodeOperationSite, decodeParameter, decodeWord, decodeTarget,
    decodeTypeArgument, decodeBinding, textField, wordField, listField, boolField, optionalText,
    siteField, site, Xsh.Conformance.Transport.WireValue.closedObject?, Xsh.Conformance.Transport.WireValue.field?,
    Xsh.Conformance.Transport.WireValue.items?, Xsh.Conformance.Transport.WireValue.text?,
    connectedFactsWire, projectionNameResolver, projectionNames, sourceName, projectionVariables,
    canonicalFields, insertField, projectionFacts, projectionFunctions, projectionBodyFacts, projectionCallFacts,
    projectionRead, projectionField, projectionAmount, projectionEnabled, Xsh.Foundation.NameId.value]
  decide +kernel

theorem rust_field_facts_decode_exactly : decodedProjectionFacts = some projectionFacts := by
  rw [decoded_projection_uses_shared_wire, admitted_projection_uses_names]
  exact projection_wire_decodes_exactly

def sourceFieldSites (nodes : List (SourceSite × Xsh.Syntax.Expr)) :
    List (SourceSite × Xsh.Foundation.NameId) :=
  nodes.filterMap fun (site, expression) => match expression with
    | .field _ name => some (site, name)
    | _ => none

def sourceCallsFunction (name : Xsh.Foundation.NameId) (site : SourceSite)
    (nodes : List (SourceSite × Xsh.Syntax.Expr)) : Bool :=
  nodes.any fun (candidateSite, expression) =>
    match expression with
    | .call (.ident callee) _ => decide (candidateSite = site ∧ callee = name)
    | _ => false

def sourceFunctionIdentities (unit : SourceUnit) : List (Xsh.Foundation.NameId × SourceSite) :=
  unit.declarations.filterMap fun declaration => match declaration with
    | .function function => some (function.name, function.bodySite)
    | _ => none

def targetSourceIdentity (unit : SourceUnit) (target : FunctionTarget) : Option (Xsh.Foundation.NameId × SourceSite) := do
  if target.moduleName = none ∧ target.body.source = unit.source then pure () else none
  let name ← sourceNameResolver unit target.body.source target.name
  pure (name, target.body)

def checksSourceFunctions (unit : SourceUnit) (functions : List FunctionTarget) : Bool :=
  match functions.mapM (targetSourceIdentity unit) with
  | none => false
  | some identities =>
    decide ((functions.map FunctionTarget.definition).Nodup) && decide identities.Nodup &&
      decide (identities.Perm (sourceFunctionIdentities unit))

def projectionSchemeValidation (sparseFuel : Nat) (functions : List FunctionTarget) (body : BodyDocument) (call : CallDocument) :
    Except ValidationError Unit := do
  let unit ← match admitSource 0 projectionSource with
    | .ok source => pure source
    | .error .exhausted => throw .exhausted
    | .error .unsupported => throw .unsupported
    | .error .malformed => throw .mismatch
  let [.function source, .function caller, .statement _ _ _] := unit.declarations | throw .unsupported
  let some intName := sourceName unit.names "Int" | throw .unsupported
  let some boolName := sourceName unit.names "Bool" | throw .unsupported
  let some stringName := sourceName unit.names "Str" | throw .unsupported
  let some resultName := sourceName unit.names "Result" | throw .unsupported
  let [parameter] := source.parameters | throw .unsupported
  let [.expression (.field (.ident readName) fieldName)] := source.body | throw .unsupported
  if parameter.annotation.isSome || source.result.isSome then throw .unsupported else pure ()
  let binder : Xsh.Foundation.BinderId := ⟨⟨0, by decide⟩⟩
  let rawParameter : Binding := ⟨parameter.name, binder, .immutable, ⟨[], .variable ⟨0, .value⟩, []⟩⟩
  let rawResult : MonoType := .variable ⟨1, .value⟩
  let names : TypeNames := ⟨intName, boolName, stringName, resultName⟩
  let candidate : List StatementNode := [.evaluated (.field (.read binder []) fieldName)]
  let fieldSites := body.operations.map fun operation => (operation.site, operation.name)
  requireCheck (checksSourceFunctions unit functions && decide (call.target ∈ functions) &&
    decide (parameter.name = readName) && decide (body.site = source.bodySite) &&
    decide (call.target.moduleName = none) &&
    decide (sourceNameResolver unit call.site.source call.target.name = some source.name) &&
    sourceCallsFunction source.name call.site caller.expressions &&
    decide (fieldSites.Perm (sourceFieldSites source.expressions)))
  let (_, generated) ← ((block 64 names [rawParameter] rawResult source.body).run
    (initialBlockState [rawParameter] rawResult)).mapError generationFailure
  let principal ← validateDecodedFieldInstance 64 (sourceNameResolver unit) projectionVariables names
    [rawParameter] [] rawResult source.body generated.constraints.reverse [.bindLeft] 1
    candidate (fun _ => none) call.target body.operations body call
  let some scheme := schemeFromBody projectionVariables body | throw .mismatch
  let some arguments := argumentsFromCall projectionVariables body call | throw .mismatch
  let qualifiedPrincipal ← validateQualifiedFunction 64 names [rawParameter] [] rawResult source.body
    generated.constraints.reverse [.bindLeft] 1 scheme candidate (fun _ => none) (fun _ => some body.result) arguments
  requireCheck (checksQualifiedExpressionFacts 64 [rawParameter] qualifiedPrincipal scheme.requirements
    (fun _ => some body.result) source candidate body)
  let instantiation := witnessSubstitution arguments
  validateSparseFacts sparseFuel unit.names [rawParameter] (compose instantiation principal)
    instantiation source candidate body

def projectionSchemeChecksFor (functions : List FunctionTarget) (body : BodyDocument) (call : CallDocument) : Bool :=
  match projectionSchemeValidation 64 functions body call with | .ok _ => true | .error _ => false

set_option maxRecDepth 100000 in
theorem admitted_projection_scheme_matches_rust_facts :
    projectionSchemeChecksFor projectionFunctions projectionBodyFacts projectionCallFacts = true := by
  decide +kernel

def validateDecodedProjection : Except ValidationError Unit :=
  match decodedProjectionFacts with
  | some ⟨functions, [body], [call]⟩ => projectionSchemeValidation 64 functions body call
  | _ => .error .mismatch

def checkDecodedProjection : Bool :=
  match decodedProjectionFacts with
  | some ⟨functions, [body], [call]⟩ => projectionSchemeChecksFor functions body call
  | _ => false

theorem rust_field_scheme_checks_against_admitted_source : checkDecodedProjection = true := by
  unfold checkDecodedProjection
  rw [rust_field_facts_decode_exactly]
  exact admitted_projection_scheme_matches_rust_facts

def wrongFieldTypeFacts : BodyDocument :=
  { projectionBodyFacts with expressionTypes :=
      [(0, ⟨0, 22, 5⟩, .variable ⟨0, .value⟩), (1, ⟨0, 22, 12⟩, .variable ⟨0, .value⟩)] }
def concretizedFieldTypeFacts : BodyDocument :=
  { projectionBodyFacts with expressionTypes :=
      [(0, ⟨0, 22, 5⟩, .variable ⟨0, .value⟩), (1, ⟨0, 22, 12⟩, .primitive .integer)] }

def wrongRead : ReadBinding := { projectionRead with name := "amount" }
def wrongReadFacts : BodyDocument :=
  { projectionBodyFacts with readBindings := [wrongRead], resolvedReads := [(0, wrongRead)] }

set_option maxRecDepth 100000 in
theorem projection_sparse_fact_mutations_are_refused :
    projectionSchemeChecksFor projectionFunctions wrongFieldTypeFacts projectionCallFacts = false ∧
    projectionSchemeChecksFor projectionFunctions concretizedFieldTypeFacts projectionCallFacts = false ∧
    projectionSchemeChecksFor projectionFunctions { projectionBodyFacts with operationSites := [] } projectionCallFacts = false ∧
    projectionSchemeChecksFor projectionFunctions { projectionBodyFacts with resolvedReads := [] } projectionCallFacts = false ∧
    projectionSchemeChecksFor projectionFunctions wrongReadFacts projectionCallFacts = false ∧
    projectionSchemeChecksFor projectionFunctions projectionBodyFacts { projectionCallFacts with role := .value } = false ∧
    projectionSchemeChecksFor projectionFunctions projectionBodyFacts
      { projectionCallFacts with target := { projectionCallFacts.target with name := "use_project" } } = false := by
  constructor <;> decide +kernel

set_option maxRecDepth 100000 in
theorem projection_effects_remain_unchecked :
    projectionSchemeChecksFor projectionFunctions { projectionBodyFacts with effects := .known [] } projectionCallFacts = true := by
  decide +kernel

def validationFailure (result : Except ValidationError Unit) : Option ValidationError :=
  match result with | .ok _ => none | .error error => some error

set_option maxRecDepth 100000 in
theorem projection_validation_distinguishes_exhaustion :
    validationFailure (projectionSchemeValidation 0 projectionFunctions projectionBodyFacts projectionCallFacts) = some .exhausted ∧
    validationFailure (projectionSchemeValidation 64 projectionFunctions wrongFieldTypeFacts projectionCallFacts) = some .mismatch := by
  constructor <;> decide +kernel

def renamedProjectionFunctions : List FunctionTarget :=
  [⟨1, "use_project", none, ⟨0, 63, 39⟩⟩, ⟨99, "project", none, ⟨0, 20, 16⟩⟩]
def renamedProjectionCall : CallDocument :=
  { projectionCallFacts with target := { projectionCallFacts.target with definition := 99 } }
def duplicateProjectionFunctions : List FunctionTarget :=
  [⟨0, "project", none, ⟨0, 20, 16⟩⟩, ⟨0, "use_project", none, ⟨0, 63, 39⟩⟩]

set_option maxRecDepth 100000 in
theorem selected_definition_identity_is_checked_modulo_renaming :
    projectionSchemeChecksFor projectionFunctions projectionBodyFacts renamedProjectionCall = false ∧
    projectionSchemeChecksFor renamedProjectionFunctions projectionBodyFacts renamedProjectionCall = true ∧
    projectionSchemeChecksFor duplicateProjectionFunctions projectionBodyFacts projectionCallFacts = false := by
  constructor <;> decide +kernel

end Xsh.Conformance.Connected
