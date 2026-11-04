import Xsh.Conformance.Connected.ScopedData

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

set_option maxRecDepth 100000 in
theorem scoped_scopes_decode : factScopesValue scopedFactsWire = some scopedScopes := by
  simp [factScopesValue, decodeBodyScope, listField, siteField, site, decodeWord,
    WireValue.closedObject?, WireValue.field?, WireValue.items?, WireValue.text?,
    scopedFactsWire, scopedScopes, scopedBodySite]
  decide +kernel

set_option maxRecDepth 100000 in
theorem scoped_facts_decode : decodeScopedFactsValue 128 (sourceNameResolver scopedUnit) scopedFactsWire =
    some (scopedDocument, scopedScopes) := by
  simp [decodeScopedFactsValue, scoped_scopes_decode, decodeScopedBody, decodeScopedOperation,
    decodeScopedCall, decodeScopedBinding, scopedVariableResolver, decodeType, decodeTypeField,
    decodeRead, decodeEffects, decodeTypedSite, decodeResolvedRead, decodeResolvedTail,
    decodeOperationSite, decodeParameter, decodeWord, decodeTarget, decodeTypeArgument,
    decodeBinding, textField, wordField, listField, boolField, optionalText, siteField, site,
    WireValue.closedObject?, WireValue.field?, WireValue.items?, WireValue.text?,
    scopedFactsWire, sourceNameResolver, sourceName, scopedUnit, scopedDocument, scopedScopes,
    scopedBodyDocument, scopedBodySite, scopedTarget, scopedStore, scopedVariable,
    scopedValueRead, scopedLocalRead, ScopedOperation.field?]
  decide +kernel

theorem scoped_document_decodes :
    decodeScopedFactsDocument 128 (sourceNameResolver scopedUnit) scopedFactsCodes =
      some (scopedDocument, scopedScopes) := by
  unfold decodeScopedFactsDocument
  rw [scoped_facts_parse_to_wire]
  change decodeScopedFactsValue 128 (sourceNameResolver scopedUnit) scopedFactsWire = _
  exact scoped_facts_decode

theorem scoped_admission_maps :
    (admitSource 0 actualScopedSource).mapError (fun _ => ValidationError.unsupported) = .ok scopedUnit := by
  rw [actual_scoped_source_is_admitted]
  rfl

set_option maxRecDepth 100000 in
theorem source_scoped_validation_reduces (declaration bodyIndex : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType)) :
    validateSourceScopedFacts 128 0 actualScopedSource scopedFactsCodes declaration bodyIndex names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant =
    validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes declaration bodyIndex names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant := by
  unfold validateSourceScopedFacts
  rw [scoped_admission_maps]
  simp only [Bind.bind, Except.bind]
  rw [scoped_document_decodes]

end Xsh.Conformance.Connected
