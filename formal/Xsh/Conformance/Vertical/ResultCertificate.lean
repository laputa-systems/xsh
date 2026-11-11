import Xsh.Conformance.Vertical.BooleanCertificate
import Xsh.Conformance.Vertical.Documents

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the result certificate proves: `keeps` on 7, `stops` on 5, and `adds` on 4
and on -1, with the constructor and integer payload of each outcome. -/
def resultExpectedObservations : List ObservedResult :=
  [⟨"keeps", [7], .constructed true 7⟩, ⟨"stops", [5], .constructed false 5⟩,
    ⟨"adds", [4], .constructed true 5⟩, ⟨"adds", [-1], .constructed true 0⟩]

set_option maxRecDepth 10000 in
theorem result_artifact_is_the_decoded_raw_ir_document :
    decodeRawDocument resultRawIrDocumentCodes = some resultArtifact := by
  decide +kernel

theorem result_artifact_is_the_decoded_source_document :
    decodeSourceDocument resultSourceDocumentCodes = some resultSourceDocument := by
  decide +kernel

/-- The functions the result source document parses to. -/
def resultFunctions : List SourceFunction :=
  [⟨"keeps", ["value"], [], .constructed true (.atom (.var "value"))⟩,
    ⟨"stops", ["value"], [], .constructed false (.atom (.var "value"))⟩,
    ⟨"adds", ["value"], [], .constructed true (.binary .add (.var "value") (.int 1))⟩]

set_option maxRecDepth 10000 in
theorem the_result_source_parses_to_the_result_functions :
    parseSource resultSourceDocument.text = some resultFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_result_functions_lower_to_the_result_artifact :
    lowersTo resultArtifact resultFunctions = true := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_result_artifact_validates : validateArtifact resultArtifact = true := by
  decide +kernel

theorem the_result_functions_mean_the_same_as_the_result_artifact :
    functionPrograms resultArtifact = sourcePrograms resultFunctions :=
  lowers_to_means_the_source the_result_functions_lower_to_the_result_artifact

set_option maxRecDepth 10000 in
theorem the_result_artifact_names_the_expected_outcomes :
    namedResults resultArtifact = some resultExpectedObservations := by
  decide +kernel

theorem result_pair_is_certified :
    certifyDecoded resultSourceDocument resultArtifact = some resultExpectedObservations := by
  rw [certify_decoded_of_checks (by decide) the_result_source_parses_to_the_result_functions
    the_result_functions_lower_to_the_result_artifact the_result_artifact_validates]
  exact the_result_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `resultSourceDocumentCodes` and
`resultRawIrDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions (three
functions of one integer parameter under a declared `Result[Int, Int]` return: two build a
constructor over their parameter and one over an addition); the declared `Result[Int, Int]` return
admits only an `Ok` or `Err` construction; lowering the parsed functions reproduces the raw-IR
document's rows, payload words, operation pool, blocks and function table exactly, with only the
span words taken from the document; the artifact validates; and the base machine runs of the four
recorded calls end with `Ok` of 7 for `keeps` on 7, `Err` of 5 for `stops` on 5, and `Ok` of 5 and
0 for `adds` on 4 and on -1, in the recorded order. A constructor outcome is recorded with its own
word (`ok` or `err`) and the payload's sixteen-digit pattern.
That each function means the same under the source and the IR follows from the universal
`lowering_means_the_source`, not from a check of this artifact
(`the_result_functions_mean_the_same_as_the_result_artifact`).
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, nothing covers `?` propagation, and nothing covers other inputs. -/
theorem result_artifact_certified :
    certifyFromSource resultSourceDocumentCodes resultRawIrDocumentCodes =
      some resultExpectedObservations := by
  rw [certify_from_source_of_decodes result_artifact_is_the_decoded_source_document
    result_artifact_is_the_decoded_raw_ir_document]
  exact result_pair_is_certified

def resultVerdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly resultExpectedObservations (mutate resultArtifact)

theorem the_unmodified_result_artifact_is_certified : resultVerdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- A constructor replaced by the other one is refused: `keeps`'s `Ok` row (1) becomes `Err`,
`stops`'s `Err` row (4) becomes `Ok`, and `adds`'s `Ok` row (9) becomes `Err`. -/
theorem a_wrong_constructor_is_rejected :
    resultVerdictAfter (·.mapRow 1 fun row => { row with tag := .exprErr, rhs := 2 }) = false ∧
      resultVerdictAfter (·.mapRow 4 fun row => { row with tag := .exprOk, rhs := 1 }) = false ∧
      resultVerdictAfter (·.mapRow 9 fun row => { row with tag := .exprErr, rhs := 2 }) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A recorded constructor that does not match the meaning is refused, including the payload under
the other constructor word and the integer word for a constructor outcome. -/
theorem a_wrong_recorded_constructor_is_rejected :
    resultVerdictAfter (·.setObservation 0 (.err ⟨7, by decide⟩)) = false ∧
      resultVerdictAfter (·.setObservation 1 (.ok ⟨5, by decide⟩)) = false ∧
      resultVerdictAfter (·.setObservation 0 (.value ⟨7, by decide⟩)) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A wrong recorded payload is refused. -/
theorem a_wrong_recorded_result_payload_is_rejected :
    resultVerdictAfter (·.setObservation 2 (.ok ⟨6, by decide⟩)) = false ∧
      resultVerdictAfter (·.setObservation 3 (.ok ⟨1, by decide⟩)) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Structurally malformed IR over the exchanged rows is refused: an `Ok` payload moved to the
row itself, an `Err` payload outside the table, and an `Err` cause word that is not the absent
marker. -/
theorem malformed_result_ir_is_rejected :
    resultVerdictAfter (·.setExtraWord 1 ⟨2, by decide⟩) = false ∧
      resultVerdictAfter (·.setExtraWord 6 ⟨99, by decide⟩) = false ∧
      resultVerdictAfter (·.setExtraWord 7 ⟨1, by decide⟩) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A changed source is not certified against the same IR: `Ok` changed to `Err` in the first
function, the `Result` return annotation changed to `Int`, the added literal changed, and another
document identifier. The offsets are into the source document's own bytes. -/
theorem a_substituted_result_source_is_rejected :
    certifyCodes (bridgeId! 1) (resultSourceDocumentCodes.set 142 69) resultArtifact = none ∧
      certifyCodes (bridgeId! 1) (resultSourceDocumentCodes.set 103 73) resultArtifact = none ∧
      certifyCodes (bridgeId! 1) (resultSourceDocumentCodes.set 265 50) resultArtifact = none ∧
      certifyCodes (bridgeId! 2) resultSourceDocumentCodes resultArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning check
against the same artifact: the other constructor for two functions and a changed addition. -/
theorem a_wrong_result_lowering_is_rejected :
    lowersTo resultArtifact (resultFunctions.set 0
        ⟨"keeps", ["value"], [], .constructed false (.atom (.var "value"))⟩) = false ∧
      sourceAgrees resultArtifact (resultFunctions.set 0
        ⟨"keeps", ["value"], [], .constructed false (.atom (.var "value"))⟩) = false ∧
      lowersTo resultArtifact (resultFunctions.set 2
        ⟨"adds", ["value"], [], .constructed true (.binary .sub (.var "value") (.int 1))⟩) =
        false ∧
      sourceAgrees resultArtifact (resultFunctions.set 2
        ⟨"adds", ["value"], [], .constructed true (.binary .sub (.var "value") (.int 1))⟩) =
        false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
