import Xsh.Conformance.Vertical.Certificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the let-call certificate proves: `bumps` on 5 and `runs` on 5 and -9, each
with the integer the call completes with. -/
def letCallExpectedObservations : List ObservedResult :=
  [⟨"bumps", [5], .value 7⟩, ⟨"runs", [5], .value 7⟩, ⟨"runs", [-9], .value (-7)⟩]

set_option maxRecDepth 100000 in
theorem letCall_artifact_is_the_decoded_raw_ir_document :
    decodeRawDocument letCallRawIrDocumentCodes = some letCallArtifact := by
  decide +kernel

theorem letCall_artifact_is_the_decoded_source_document :
    decodeSourceDocument letCallSourceDocumentCodes = some letCallSourceDocument := by
  decide +kernel

/-- The functions the let-call source document parses to: a function of one integer parameter and
one integer let whose returned value adds the parameter and the let, and a function that calls it
on its parameter. -/
def letCallFunctions : List SourceFunction :=
  [⟨"bumps", ["value"], [("step", 2)], none,
      .value (.binary .add (.var "value") (.var "step"))⟩,
    ⟨"runs", ["value"], [], none, .call "bumps" [.var "value"]⟩]

/-- The character codes of `letCallSourceDocument`'s text, which the admitted parser reads. The
document's literal is tied to it through `String.ofList`, so the parser does not decode the
string in the kernel. -/
def letCallTextCodes : List Nat :=
  [112, 117, 114, 101, 32, 98, 117, 109, 112, 115, 40, 118, 97, 108, 117, 101, 58, 32, 73, 110,
   116, 41, 32, 45, 62, 32, 73, 110, 116, 32, 123, 10, 32, 32, 108, 101, 116, 32, 115, 116,
   101, 112, 32, 61, 32, 50, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 118, 97, 108, 117,
   101, 32, 43, 32, 115, 116, 101, 112, 10, 125, 10, 10, 112, 117, 114, 101, 32, 114, 117, 110,
   115, 40, 118, 97, 108, 117, 101, 58, 32, 73, 110, 116, 41, 32, 45, 62, 32, 73, 110, 116,
   32, 123, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 98, 117, 109, 112, 115, 40, 118, 97,
   108, 117, 101, 41, 10, 125, 10]

set_option maxRecDepth 100000 in
theorem letCall_text_is_the_decoded_document_text :
    letCallSourceDocument.text = String.ofList (letCallTextCodes.map Char.ofNat) := rfl

set_option maxRecDepth 10000 in
theorem letCall_text_codes_round_trip :
    (letCallTextCodes.map Char.ofNat).map Char.toNat = letCallTextCodes := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_let_call_text_codes_parse :
    parseSourceCodes letCallTextCodes = some letCallFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_let_call_source_parses_to_the_let_call_functions :
    parseSource letCallSourceDocument.text = some letCallFunctions := by
  simp only [letCall_text_is_the_decoded_document_text, parseSource, String.toList_ofList,
    letCall_text_codes_round_trip, the_let_call_text_codes_parse]

set_option maxRecDepth 100000 in
theorem letCall_span_words : spanWords letCallArtifact = [⟨2, by decide⟩, ⟨3, by decide⟩] := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem the_let_call_functions_lower_to_the_let_call_artifact :
    lowersTo letCallArtifact letCallFunctions = true := by
  unfold lowersTo
  rw [letCall_span_words]
  decide +kernel

set_option maxRecDepth 100000 in
theorem the_let_call_artifact_validates : validateArtifact letCallArtifact = true := by
  decide +kernel

theorem the_let_call_functions_mean_the_same_as_the_let_call_artifact :
    functionPrograms letCallArtifact = sourcePrograms letCallFunctions :=
  lowers_to_means_the_source the_let_call_functions_lower_to_the_let_call_artifact

set_option maxRecDepth 100000 in
theorem the_let_call_artifact_names_the_expected_outcomes :
    namedResults letCallArtifact = some letCallExpectedObservations := by
  decide +kernel

theorem let_call_pair_is_certified :
    certifyDecoded letCallSourceDocument letCallArtifact = some letCallExpectedObservations := by
  rw [certify_decoded_of_checks (by decide)
    the_let_call_source_parses_to_the_let_call_functions
    the_let_call_functions_lower_to_the_let_call_artifact the_let_call_artifact_validates]
  exact the_let_call_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `letCallSourceDocumentCodes` and
`letCallRawIRDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions (two
integer functions: `bumps` has one integer parameter and one integer let and returns the sum of
the parameter and the let; `runs` calls `bumps` on its parameter); the parameter takes slot 0 and
the let slot 2, where the exporter places them; lowering the parsed functions reproduces the
raw-IR document's rows, payload words, operation pool, blocks and function table exactly, with
only the span words taken from the document; the artifact validates; and the base machine runs of
the three recorded calls end with 7 for `bumps` on 5, 7 for `runs` on 5 and -7 for `runs` on -9.
That each function means the same under the source and the IR follows from the universal
`lowering_means_the_source`, not from a check of this artifact
(`the_let_call_functions_mean_the_same_as_the_let_call_artifact`). A callee's literal binding is
part of its callee form: the IR call row carries the callee's body with each let slot replaced by
its integer literal, which is what an immutable let of a literal means.
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, nothing covers a callee with an assigned binding, a computed let value, several lets or a
callee outside this shape, and nothing covers other inputs. -/
theorem letCall_artifact_certified :
    certifyFromSource letCallSourceDocumentCodes letCallRawIrDocumentCodes =
      some letCallExpectedObservations := by
  rw [certify_from_source_of_decodes letCall_artifact_is_the_decoded_source_document
    letCall_artifact_is_the_decoded_raw_ir_document]
  exact let_call_pair_is_certified

def letCallVerdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly letCallExpectedObservations (mutate letCallArtifact)

theorem the_unmodified_let_call_artifact_is_certified :
    letCallVerdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- A call retargeted to a function the callee table does not hold is refused: `runs` names
function two, while only `bumps` precedes it. The call's function word is the first payload
word. -/
theorem a_wrong_let_call_callee_is_rejected :
    letCallVerdictAfter (·.setExtraWord 18 ⟨2, by decide⟩) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- A callee body whose let read is replaced by its parameter is still well typed, but it returns
10 for `bumps` on 5 where the record holds 7. The read is the second expression row's slot word. -/
theorem a_wrong_let_call_binding_read_is_rejected :
    letCallVerdictAfter (·.setExtraWord 5 ⟨0, by decide⟩) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- Recorded outcomes that do not match the meaning are refused, including the payload of
another call and a wrong binding argument. -/
theorem a_wrong_recorded_let_call_outcome_is_rejected :
    letCallVerdictAfter (·.setObservation 0 (.value ⟨8, by decide⟩)) = false ∧
      letCallVerdictAfter (·.setObservation 2 (.value ⟨6, by decide⟩)) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- Structurally malformed rows are refused: the let binding naming itself as its value row and a
binding slot no read resolves. The binding's payload is the target slot and the value row. -/
theorem malformed_let_call_ir_is_rejected :
    letCallVerdictAfter (·.setExtraWord 3 ⟨1, by decide⟩) = false ∧
      letCallVerdictAfter (·.setExtraWord 2 ⟨1, by decide⟩) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- A changed source is not certified against the same IR: the callee renamed, the let's literal
changed, the calling function renamed, and another document identifier. The offsets are into the
source document's own bytes. -/
theorem a_substituted_let_call_source_is_rejected :
    certifyCodes (bridgeId! 1) (letCallSourceDocumentCodes.set 55 122) letCallArtifact = none ∧
      certifyCodes (bridgeId! 1) (letCallSourceDocumentCodes.set 96 51) letCallArtifact = none ∧
      certifyCodes (bridgeId! 1) (letCallSourceDocumentCodes.set 132 122) letCallArtifact = none ∧
      certifyCodes (bridgeId! 2) letCallSourceDocumentCodes letCallArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning check
against the same artifact: the let's literal changed, the calling function's argument replaced by
a literal, and the first callee's addition reversed. -/
theorem a_wrong_let_call_lowering_is_rejected :
    lowersTo letCallArtifact (letCallFunctions.set 0
        ⟨"bumps", ["value"], [("step", 3)], none,
          .value (.binary .add (.var "value") (.var "step"))⟩) = false ∧
      sourceAgrees letCallArtifact (letCallFunctions.set 0
        ⟨"bumps", ["value"], [("step", 3)], none,
          .value (.binary .add (.var "value") (.var "step"))⟩) = false ∧
      lowersTo letCallArtifact (letCallFunctions.set 1
        ⟨"runs", ["value"], [], none, .call "bumps" [.int 5]⟩) = false ∧
      sourceAgrees letCallArtifact (letCallFunctions.set 1
        ⟨"runs", ["value"], [], none, .call "bumps" [.int 5]⟩) = false ∧
      lowersTo letCallArtifact (letCallFunctions.set 0
        ⟨"bumps", ["value"], [("step", 2)], none,
          .value (.binary .sub (.var "value") (.var "step"))⟩) = false ∧
      sourceAgrees letCallArtifact (letCallFunctions.set 0
        ⟨"bumps", ["value"], [("step", 2)], none,
          .value (.binary .sub (.var "value") (.var "step"))⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
