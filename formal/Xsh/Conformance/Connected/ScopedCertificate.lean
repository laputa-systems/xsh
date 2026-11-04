import Xsh.Conformance.Connected.ScopedDecode

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def actualScopedCheckDecoded (unit : SourceUnit) (facts : ScopedFactsDocument) (scopes : List (SourceSite × List Nat)) : Except ValidationError Substitution := do
  let some intName := sourceName unit.names "Int" | throw .mismatch
  let some boolName := sourceName unit.names "Bool" | throw .mismatch
  let some stringName := sourceName unit.names "Str" | throw .mismatch
  let some resultName := sourceName unit.names "Result" | throw .mismatch
  let some valueName := sourceName unit.names "value" | throw .mismatch
  let typeVar : Variable := ⟨0, .value⟩
  let completion : Variable := ⟨1, .value⟩
  let input : BinderId := ⟨⟨0, by decide⟩⟩
  let localBinder : BinderId := ⟨⟨1, by decide⟩⟩
  let parameters : List Binding := [⟨valueName, input, .immutable, ⟨[], .variable typeVar, []⟩⟩]
  let candidate : List StatementNode := [
    .binding localBinder .mutable (.variable typeVar) (.read input []),
    .evaluated (.valueBlock (.primitive .unit) [.assignment localBinder (.read input [])]),
    .evaluated (.read localBinder [])]
  let ordered : List Constraint := [.equal (.variable completion) (.primitive .unit),
    .equal (.variable typeVar) (.variable typeVar), .store (.variable typeVar) (.variable typeVar)]
  validateSourceScopedDecoded 128 unit facts scopes 0 0
    ⟨intName, boolName, stringName, resultName⟩ parameters [] (.variable typeVar) ordered
    [.bindLeft, .reflexive] 2 candidate
    (fun binder => if binder = localBinder then some (.variable typeVar) else none) (fun _ => none)
    [(typeVar, .primitive .integer)]

def actualScopedCheck : Except ValidationError Substitution := do
  let unit := scopedUnit
  let some intName := sourceName unit.names "Int" | throw .mismatch
  let some boolName := sourceName unit.names "Bool" | throw .mismatch
  let some stringName := sourceName unit.names "Str" | throw .mismatch
  let some resultName := sourceName unit.names "Result" | throw .mismatch
  let some valueName := sourceName unit.names "value" | throw .mismatch
  let typeVar : Variable := ⟨0, .value⟩
  let completion : Variable := ⟨1, .value⟩
  let input : BinderId := ⟨⟨0, by decide⟩⟩
  let localBinder : BinderId := ⟨⟨1, by decide⟩⟩
  let parameters : List Binding := [⟨valueName, input, .immutable, ⟨[], .variable typeVar, []⟩⟩]
  let candidate : List StatementNode := [
    .binding localBinder .mutable (.variable typeVar) (.read input []),
    .evaluated (.valueBlock (.primitive .unit) [.assignment localBinder (.read input [])]),
    .evaluated (.read localBinder [])]
  let ordered : List Constraint := [.equal (.variable completion) (.primitive .unit),
    .equal (.variable typeVar) (.variable typeVar), .store (.variable typeVar) (.variable typeVar)]
  validateSourceScopedFacts 128 0 actualScopedSource scopedFactsCodes 0 0
    ⟨intName, boolName, stringName, resultName⟩ parameters [] (.variable typeVar) ordered
    [.bindLeft, .reflexive] 2 candidate
    (fun binder => if binder = localBinder then some (.variable typeVar) else none) (fun _ => none)
    [(typeVar, .primitive .integer)]

set_option maxRecDepth 100000 in
theorem actual_scoped_facts_are_checked :
    (match actualScopedCheck with | .ok _ => true | .error _ => false) = true := by
  unfold actualScopedCheck
  simp only [source_scoped_validation_reduces]
  change (match actualScopedCheckDecoded scopedUnit scopedDocument scopedScopes with | .ok _ => true | .error _ => false) = true
  decide +kernel

def scopedMutatedBodies : List ScopedBodyDocument :=
  [⟨scopedBodyDocument, [.store { scopedStore with storage := .primitive .boolean }]⟩,
   ⟨scopedBodyDocument, [.store { scopedStore with replacement := .primitive .boolean }]⟩,
   ⟨scopedBodyDocument, [.store { scopedStore with site := ⟨0, 67, 4⟩ }]⟩,
   ⟨{ scopedBodyDocument with operationSites := [] }, [.store scopedStore]⟩,
   ⟨{ scopedBodyDocument with expressionTypes :=
      [(0, ⟨0, 35, 5⟩, .variable scopedVariable), (1, ⟨0, 67, 5⟩, .primitive .boolean),
       (2, ⟨0, 55, 19⟩, .primitive .unit)] }, [.store scopedStore]⟩,
   ⟨{ scopedBodyDocument with
      readBindings := [scopedValueRead, { scopedLocalRead with site := ⟨0, 21, 19⟩ }],
      resolvedTails := [(3, { scopedLocalRead with site := ⟨0, 21, 19⟩ })] }, [.store scopedStore]⟩]

def scopedMutatedCalls : List ScopedCallDocument :=
  [⟨5, ⟨0, 93, 9⟩, .call, scopedTarget, [(0, .primitive .boolean)], [.store]⟩,
   ⟨5, ⟨0, 93, 9⟩, .call, { scopedTarget with definition := 1 }, [(0, .primitive .integer)], [.store]⟩,
   ⟨5, ⟨0, 93, 8⟩, .call, scopedTarget, [(0, .primitive .integer)], [.store]⟩,
   ⟨5, ⟨0, 93, 9⟩, .call, scopedTarget, [(0, .primitive .integer)], [.field]⟩]

def rejectsScopedDocument (facts : ScopedFactsDocument) : Bool :=
  match actualScopedCheckDecoded scopedUnit facts scopedScopes with | .error _ => true | .ok _ => false

set_option maxRecDepth 100000 in
theorem actual_scoped_fact_mutations_are_rejected :
    scopedMutatedBodies.all (fun body => rejectsScopedDocument { scopedDocument with bodies := [body] }) = true ∧
    scopedMutatedCalls.all (fun call => rejectsScopedDocument { scopedDocument with calls := [call] }) = true := by
  decide +kernel

private def shadowOtherName : NameId := scopedName 12 (by decide)
private def shadowCallSite : SourceSite := ⟨0, 130, 9⟩
private def shadowOtherBody : SourceSite := ⟨0, 120, 25⟩
private def shadowOther : SourceFunction :=
  ⟨.pure, none, shadowOtherName, ⟨0, 110, 5⟩, shadowOtherBody,
    [⟨scopedFunctionName, ⟨0, 116, 6⟩, none⟩], none,
    [.expression scopedCall], [(shadowCallSite, scopedCall)], [], []⟩
private def shadowUnit : SourceUnit :=
  { scopedUnit with
    names := scopedUnit.names ++ ["other"],
    declarations := scopedUnit.declarations ++ [.function shadowOther] }
private def shadowTarget : FunctionTarget := ⟨1, "other", none, shadowOtherBody⟩
private def shadowCall : ScopedCallDocument :=
  ⟨6, shadowCallSite, .call, scopedTarget, [(0, .primitive .integer)], [.store]⟩
private def shadowFacts : ScopedFactsDocument :=
  { scopedDocument with
    functions := scopedDocument.functions ++ [shadowTarget],
    calls := scopedDocument.calls ++ [shadowCall] }
private def shadowGlobalUnit : SourceUnit :=
  { scopedUnit with declarations := scopedUnit.declarations.take 1 ++ [
    .statement (.binding .letBinding (.name scopedFunctionName) none (.literal (.integer "7"))) [] []] ++
    scopedUnit.declarations.drop 1 }

private def shadowFromSource : Bool :=
  match admitSource 0 (actualScopedSource ++ "pure other(scoped) { scoped(7) }\n") with
  | .error _ => false
  | .ok unit =>
    match unit.declarations[2]?, (scopedCallSites unit).getLast? with
    | some (SourceDeclaration.function other), some site =>
      let target : FunctionTarget := ⟨1, "other", none, other.bodySite⟩
      let call : ScopedCallDocument := ⟨6, site, .call, scopedTarget, [(0, .primitive .integer)], [.store]⟩
      let facts : ScopedFactsDocument := { scopedDocument with
        functions := scopedDocument.functions ++ [target], calls := scopedDocument.calls ++ [call] }
      match actualScopedCheckDecoded unit facts scopedScopes with
      | .error .unsupported => true
      | _ => false
    | _, _ => false

private def fieldCallFromSource : Bool :=
  match admitSource 0 (actualScopedSource ++ "pure other(value) { value.run(7) }\n") with
  | .error _ => false
  | .ok unit =>
    match unit.declarations[2]? with
    | some (SourceDeclaration.function other) =>
      let target : FunctionTarget := ⟨1, "other", none, other.bodySite⟩
      let facts : ScopedFactsDocument := { scopedDocument with functions := scopedDocument.functions ++ [target] }
      match actualScopedCheckDecoded unit facts scopedScopes with
      | .error .unsupported => true
      | _ => false
    | _ => false

set_option maxRecDepth 100000 in
theorem shadowed_scoped_calls_are_not_accepted :
    (match actualScopedCheckDecoded shadowUnit shadowFacts scopedScopes with
      | .error .unsupported => true | _ => false) = true ∧
    (match actualScopedCheckDecoded shadowGlobalUnit scopedDocument scopedScopes with
      | .error .unsupported => true | _ => false) = true ∧
    shadowFromSource = true ∧ fieldCallFromSource = true := by
  decide +kernel

private def scopedAssignmentStatementSite : Option (Option SourceSite) := do
  let input : BinderId := ⟨⟨0, by decide⟩⟩
  let localBinder : BinderId := ⟨⟨1, by decide⟩⟩
  let parameter : Binding := ⟨scopedInput, input, .immutable, ⟨[], .variable scopedVariable, []⟩⟩
  let candidate : List StatementNode := [
    .binding localBinder .mutable (.variable scopedVariable) (.read input []),
    .evaluated (.valueBlock (.primitive .unit) [.assignment localBinder (.read input [])]),
    .evaluated (.read localBinder [])]
  let occurrences ← (sourceScopedOccurrences 128 scopedFunction [parameter] [] []
    (fun binder => if binder = localBinder then some (.variable scopedVariable) else none) candidate).toOption
  let occurrence ← occurrences.find? (fun item => decide (item.site = ⟨0, 67, 5⟩))
  pure (occurrence.storeTarget.map Prod.snd)

theorem scoped_assignment_retains_statement_and_expression_sites :
    scopedAssignmentStatementSite = some (some ⟨0, 57, 15⟩) := by
  decide +kernel

end Xsh.Conformance.Connected
