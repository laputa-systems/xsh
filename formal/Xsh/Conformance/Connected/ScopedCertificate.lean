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

end Xsh.Conformance.Connected
