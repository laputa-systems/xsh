import Xsh.Conformance.Vertical.ResultCertificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the propagation certificate proves: `keeps` on 7, `stops` on 5, `passes`
on 7 and -3, and `blocks` on 5, with the constructor and integer payload of each outcome. -/
def propagationExpectedObservations : List ObservedResult :=
  [⟨"keeps", [7], .constructed true 8⟩, ⟨"stops", [5], .constructed false 5⟩,
    ⟨"passes", [7], .constructed true 8⟩, ⟨"blocks", [5], .constructed false 5⟩,
    ⟨"passes", [-3], .constructed true (-2)⟩]

set_option maxRecDepth 100000 in
theorem propagation_artifact_is_the_decoded_raw_ir_document :
    decodeRawDocument propagationRawIrDocumentCodes = some propagationArtifact := by
  decide +kernel

theorem propagation_artifact_is_the_decoded_source_document :
    decodeSourceDocument propagationSourceDocumentCodes = some propagationSourceDocument := by
  decide +kernel

/-- The functions the propagation source document parses to. -/
def propagationFunctions : List SourceFunction :=
  [⟨"keeps", ["value"], [], none, .constructed true (.binary .add (.var "value") (.int 1))⟩,
    ⟨"stops", ["value"], [], none, .constructed false (.atom (.var "value"))⟩,
    ⟨"passes", ["value"], [], some ⟨"kept", "keeps", [.var "value"]⟩,
      .constructed true (.atom (.var "kept"))⟩,
    ⟨"blocks", ["value"], [], some ⟨"kept", "stops", [.var "value"]⟩,
      .constructed true (.atom (.var "kept"))⟩]

/-- The character codes of `propagationSourceDocument`'s text, which the admitted parser reads.
The document's literal is tied to it through `String.ofList`, so the parser does not decode the
string in the kernel. -/
def propagationTextCodes : List Nat :=
  [112, 117, 114, 101, 32, 107, 101, 101, 112, 115, 40, 118,
    97, 108, 117, 101, 58, 32, 73, 110, 116, 41, 32, 45,
    62, 32, 82, 101, 115, 117, 108, 116, 91, 73, 110, 116,
    44, 32, 73, 110, 116, 93, 32, 123, 10, 32, 32, 114,
    101, 116, 117, 114, 110, 32, 79, 107, 40, 118, 97, 108,
    117, 101, 32, 43, 32, 49, 41, 10, 125, 10, 10, 112,
    117, 114, 101, 32, 115, 116, 111, 112, 115, 40, 118, 97,
    108, 117, 101, 58, 32, 73, 110, 116, 41, 32, 45, 62,
    32, 82, 101, 115, 117, 108, 116, 91, 73, 110, 116, 44,
    32, 73, 110, 116, 93, 32, 123, 10, 32, 32, 114, 101,
    116, 117, 114, 110, 32, 69, 114, 114, 40, 118, 97, 108,
    117, 101, 41, 10, 125, 10, 10, 112, 117, 114, 101, 32,
    112, 97, 115, 115, 101, 115, 40, 118, 97, 108, 117, 101,
    58, 32, 73, 110, 116, 41, 32, 45, 62, 32, 82, 101,
    115, 117, 108, 116, 91, 73, 110, 116, 44, 32, 73, 110,
    116, 93, 32, 123, 10, 32, 32, 108, 101, 116, 32, 107,
    101, 112, 116, 32, 61, 32, 107, 101, 101, 112, 115, 40,
    118, 97, 108, 117, 101, 41, 63, 10, 32, 32, 114, 101,
    116, 117, 114, 110, 32, 79, 107, 40, 107, 101, 112, 116,
    41, 10, 125, 10, 10, 112, 117, 114, 101, 32, 98, 108,
    111, 99, 107, 115, 40, 118, 97, 108, 117, 101, 58, 32,
    73, 110, 116, 41, 32, 45, 62, 32, 82, 101, 115, 117,
    108, 116, 91, 73, 110, 116, 44, 32, 73, 110, 116, 93,
    32, 123, 10, 32, 32, 108, 101, 116, 32, 107, 101, 112,
    116, 32, 61, 32, 115, 116, 111, 112, 115, 40, 118, 97,
    108, 117, 101, 41, 63, 10, 32, 32, 114, 101, 116, 117,
    114, 110, 32, 79, 107, 40, 107, 101, 112, 116, 41, 10,
    125, 10]

set_option maxRecDepth 100000 in
theorem propagation_text_is_the_decoded_document_text :
    propagationSourceDocument.text = String.ofList (propagationTextCodes.map Char.ofNat) := rfl

set_option maxRecDepth 10000 in
theorem propagation_text_codes_round_trip :
    (propagationTextCodes.map Char.ofNat).map Char.toNat = propagationTextCodes := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_propagation_text_codes_parse :
    parseSourceCodes propagationTextCodes = some propagationFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_propagation_source_parses_to_the_propagation_functions :
    parseSource propagationSourceDocument.text = some propagationFunctions := by
  simp only [propagation_text_is_the_decoded_document_text, parseSource, String.toList_ofList,
    propagation_text_codes_round_trip, the_propagation_text_codes_parse]

set_option maxRecDepth 100000 in
theorem the_propagation_functions_lower_to_the_propagation_artifact :
    lowersTo propagationArtifact propagationFunctions = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem the_propagation_artifact_validates : validateArtifact propagationArtifact = true := by
  decide +kernel

theorem the_propagation_functions_mean_the_same_as_the_propagation_artifact :
    functionPrograms propagationArtifact = sourcePrograms propagationFunctions :=
  lowers_to_means_the_source the_propagation_functions_lower_to_the_propagation_artifact

set_option maxRecDepth 100000 in
theorem the_propagation_artifact_names_the_expected_outcomes :
    namedResults propagationArtifact = some propagationExpectedObservations := by
  decide +kernel

theorem propagation_pair_is_certified :
    certifyDecoded propagationSourceDocument propagationArtifact =
      some propagationExpectedObservations := by
  rw [certify_decoded_of_checks (by decide)
    the_propagation_source_parses_to_the_propagation_functions
    the_propagation_functions_lower_to_the_propagation_artifact the_propagation_artifact_validates]
  exact the_propagation_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `propagationSourceDocumentCodes` and
`propagationRawIrDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions (four
functions of one integer parameter under a declared `Result[Int, Int]` return; two build a
constructor, one adds one before `Ok`, and two each bind a call of an earlier function under `?`
and return `Ok` of the bound payload); each propagation names an earlier callable function with a
constructor result and one argument; lowering the parsed functions reproduces the raw-IR
document's rows, payload words, operation pool, blocks and function table exactly, with only the
span words taken from the document; the artifact validates; and the base machine runs of the five
recorded calls end with `Ok` of 8 for `keeps` on 7, `Err` of 5 for `stops` on 5, `Ok` of 8 for
`passes` on 7 and `Ok` of -2 for `passes` on -3 (the callee's payload, not the argument), and
`Err` of 5 for `blocks` on 5, in the recorded order. A constructor outcome is recorded with its
own word (`ok` or `err`) and the payload's sixteen-digit pattern.
That each function means the same under the source and the IR follows from the universal
`lowering_means_the_source`, not from a check of this artifact
(`the_propagation_functions_mean_the_same_as_the_propagation_artifact`).
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, nothing covers `?` outside a single call binding of a one-parameter function, and nothing
covers other inputs. -/
theorem propagation_artifact_certified :
    certifyFromSource propagationSourceDocumentCodes propagationRawIrDocumentCodes =
      some propagationExpectedObservations := by
  rw [certify_from_source_of_decodes propagation_artifact_is_the_decoded_source_document
    propagation_artifact_is_the_decoded_raw_ir_document]
  exact propagation_pair_is_certified

def propagationVerdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly propagationExpectedObservations (mutate propagationArtifact)

theorem the_unmodified_propagation_artifact_is_certified :
    propagationVerdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- A propagation call replaced by the other callee is refused: `passes` calls `stops` instead of
`keeps`, so its run propagates `Err` 7 where the record holds `Ok` 8, and `blocks` calls `keeps`
instead of `stops`. The call's function word is the first payload word. -/
theorem a_wrong_propagation_call_is_rejected :
    propagationVerdictAfter (·.setExtraWord 21 ⟨2, by decide⟩) = false ∧
      propagationVerdictAfter (·.setExtraWord 39 ⟨1, by decide⟩) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- A recorded propagation outcome that does not match the meaning is refused, including the
payload under the other constructor and a payload that differs from the callee's. -/
theorem a_wrong_recorded_propagation_outcome_is_rejected :
    propagationVerdictAfter (·.setObservation 2 (.err ⟨8, by decide⟩)) = false ∧
      propagationVerdictAfter (·.setObservation 3 (.ok ⟨5, by decide⟩)) = false ∧
      propagationVerdictAfter (·.setObservation 4 (.ok ⟨18446744073709551615, by decide⟩)) =
        false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- Structurally malformed propagation rows are refused: the `?` row naming itself, the binding
value not being a `?` row, a type annotation on the binding, and a call naming the absent
function. The binding's payload is the target slot, the `?` row and the absent annotation word. -/
theorem malformed_propagation_ir_is_rejected :
    propagationVerdictAfter (·.setExtraWord 24 ⟨10, by decide⟩) = false ∧
      propagationVerdictAfter (·.setExtraWord 27 ⟨9, by decide⟩) = false ∧
      propagationVerdictAfter (·.setExtraWord 28 ⟨1, by decide⟩) = false ∧
      propagationVerdictAfter (·.setExtraWord 21 ⟨0, by decide⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- A changed source is not certified against the same IR: the first callee renamed, the `?` of
the first propagation replaced, the propagated call retargeted to the other callee, and another
document identifier. The offsets are into the source document's own bytes. -/
theorem a_substituted_propagation_source_is_rejected :
    certifyCodes (bridgeId! 1) (propagationSourceDocumentCodes.set 55 75) propagationArtifact =
        none ∧
      certifyCodes (bridgeId! 1) (propagationSourceDocumentCodes.set 269 33) propagationArtifact =
        none ∧
      certifyCodes (bridgeId! 1)
        (((propagationSourceDocumentCodes.set 257 115).set 258 116).set 259 111)
        propagationArtifact = none ∧
      certifyCodes (bridgeId! 2) propagationSourceDocumentCodes propagationArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning check
against the same artifact: the propagated call retargeted, the other binding's call retargeted,
and a binding whose returned expression reads the parameter instead of the payload. -/
theorem a_wrong_propagation_lowering_is_rejected :
    lowersTo propagationArtifact (propagationFunctions.set 2
        ⟨"passes", ["value"], [], some ⟨"kept", "stops", [.var "value"]⟩,
          .constructed true (.atom (.var "kept"))⟩) = false ∧
      sourceAgrees propagationArtifact (propagationFunctions.set 2
        ⟨"passes", ["value"], [], some ⟨"kept", "stops", [.var "value"]⟩,
          .constructed true (.atom (.var "kept"))⟩) = false ∧
      lowersTo propagationArtifact (propagationFunctions.set 3
        ⟨"blocks", ["value"], [], some ⟨"kept", "keeps", [.var "value"]⟩,
          .constructed true (.atom (.var "kept"))⟩) = false ∧
      sourceAgrees propagationArtifact (propagationFunctions.set 3
        ⟨"blocks", ["value"], [], some ⟨"kept", "keeps", [.var "value"]⟩,
          .constructed true (.atom (.var "kept"))⟩) = false ∧
      lowersTo propagationArtifact (propagationFunctions.set 2
        ⟨"passes", ["value"], [], some ⟨"kept", "keeps", [.var "value"]⟩,
          .constructed true (.atom (.var "value"))⟩) = false ∧
      sourceAgrees propagationArtifact (propagationFunctions.set 2
        ⟨"passes", ["value"], [], some ⟨"kept", "keeps", [.var "value"]⟩,
          .constructed true (.atom (.var "value"))⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
