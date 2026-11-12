import Xsh.Conformance.Vertical.Certificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the computed-let certificate proves: `scales` on 5 and `runs` on 5 and -9,
each with the integer the call completes with. -/
def computedLetExpectedObservations : List ObservedResult :=
  [⟨"scales", [5], .value 14⟩, ⟨"runs", [5], .value 14⟩, ⟨"runs", [-9], .value (-14)⟩]

set_option maxRecDepth 100000 in
theorem computedLet_artifact_is_the_decoded_raw_ir_document :
    decodeRawDocument computedLetRawIrDocumentCodes = some computedLetArtifact := by
  decide +kernel

theorem computedLet_artifact_is_the_decoded_source_document :
    decodeSourceDocument computedLetSourceDocumentCodes = some computedLetSourceDocument := by
  decide +kernel

/-- The functions the computed-let source document parses to: a function of one integer parameter
and one computed let whose value adds the parameter and 4, and a function that calls it on its
parameter. -/
def computedLetFunctions : List SourceFunction :=
  [⟨"scales", ["value"], [], none,
      .computedLet "step" (.binary .add (.var "value") (.int 4))
        (.binary .add (.var "value") (.var "step"))⟩,
    ⟨"runs", ["value"], [], none, .call "scales" [.var "value"]⟩]

set_option maxRecDepth 10000 in
theorem computed_let_source_parses :
    parseSource ("pure scales(value: Int) -> Int {\n  let step = value + 4\n" ++
        "  return value + step\n}\n\n") =
      some [⟨"scales", ["value"], [], none,
        .computedLet "step" (.binary .add (.var "value") (.int 4))
          (.binary .add (.var "value") (.var "step"))⟩] := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- The character codes of `computedLetSourceDocument`'s text come from the exported document
(`computedLetTextCodes`), so the parser does not decode the string in the kernel. -/
theorem computedLet_text_is_the_decoded_document_text :
    computedLetSourceDocument.text = String.ofList (computedLetTextCodes.map Char.ofNat) := rfl

set_option maxRecDepth 10000 in
theorem computedLet_text_codes_round_trip :
    (computedLetTextCodes.map Char.ofNat).map Char.toNat = computedLetTextCodes := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_computed_let_text_codes_parse :
    parseSourceCodes computedLetTextCodes = some computedLetFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_computed_let_source_parses_to_the_computed_let_functions :
    parseSource computedLetSourceDocument.text = some computedLetFunctions := by
  simp only [computedLet_text_is_the_decoded_document_text, parseSource, String.toList_ofList,
    computedLet_text_codes_round_trip, the_computed_let_text_codes_parse]

set_option maxRecDepth 100000 in
theorem computedLet_span_words : spanWords computedLetArtifact = [⟨2, by decide⟩, ⟨3, by decide⟩] := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem the_computed_let_functions_lower_to_the_computed_let_artifact :
    lowersTo computedLetArtifact computedLetFunctions = true := by
  unfold lowersTo
  rw [computedLet_span_words]
  decide +kernel

set_option maxRecDepth 100000 in
theorem the_computed_let_artifact_validates : validateArtifact computedLetArtifact = true := by
  decide +kernel

theorem the_computed_let_functions_mean_the_same_as_the_computed_let_artifact :
    functionPrograms computedLetArtifact = sourcePrograms computedLetFunctions :=
  lowers_to_means_the_source the_computed_let_functions_lower_to_the_computed_let_artifact

set_option maxRecDepth 100000 in
theorem the_computed_let_artifact_names_the_expected_outcomes :
    namedResults computedLetArtifact = some computedLetExpectedObservations := by
  decide +kernel

theorem computed_let_pair_is_certified :
    certifyDecoded computedLetSourceDocument computedLetArtifact =
      some computedLetExpectedObservations := by
  rw [certify_decoded_of_checks (by decide)
    the_computed_let_source_parses_to_the_computed_let_functions
    the_computed_let_functions_lower_to_the_computed_let_artifact the_computed_let_artifact_validates]
  exact the_computed_let_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `computedLetSourceDocumentCodes` and
`computedLetRawIRDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions (two
integer functions: `scales` has one integer parameter and one computed let binding
`value + 4` and returns the sum of the parameter and the let; `runs` calls `scales` on its
parameter); the parameter takes slot 0 and the let slot 2, where the exporter places them, and the
let's value is emitted as the integer-family `IntSlot`, `IntInt` and `IntBinary` rows the Rust
lowerer emits; lowering the parsed functions reproduces the raw-IR document's rows, payload words,
operation pool, blocks and function table exactly, with only the span words taken from the
document; the artifact validates; and the base machine runs of the three recorded calls end with 14
for `scales` on 5, 14 for `runs` on 5 and -14 for `runs` on -9. That each function means the same
under the source and the IR follows from the universal `lowering_means_the_source`, not from a
check of this artifact (`the_computed_let_functions_mean_the_same_as_the_computed_let_artifact`).
A callee's computed binding is part of its callee form: the IR call row carries the callee's body
with the let slot bound, and the call supplies the binding's value next to the parameter, with the
parameter reads inside that value replaced by the argument expressions, which is what an immutable
let of a computed value means.
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, nothing covers several lets, a callee with an assigned binding or a callee outside this shape,
and nothing covers other inputs. -/
theorem computedLet_artifact_certified :
    certifyFromSource computedLetSourceDocumentCodes computedLetRawIrDocumentCodes =
      some computedLetExpectedObservations := by
  rw [certify_from_source_of_decodes computedLet_artifact_is_the_decoded_source_document
    computedLet_artifact_is_the_decoded_raw_ir_document]
  exact computed_let_pair_is_certified

def computedLetVerdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly computedLetExpectedObservations (mutate computedLetArtifact)

theorem the_unmodified_computed_let_artifact_is_certified :
    computedLetVerdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- A call retargeted to a function the callee table does not hold is refused: `runs` names
function two, while only `scales` precedes it. The call's function word is the first payload
word. -/
theorem a_wrong_computed_let_callee_is_rejected :
    computedLetVerdictAfter (·.setExtraWord 22 ⟨2, by decide⟩) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- A callee body whose let read is replaced by its parameter is still well typed, but it returns 10
for `scales` on 5 where the record holds 14. The read is the third expression row's slot word. -/
theorem a_wrong_computed_let_binding_read_is_rejected :
    computedLetVerdictAfter (·.setExtraWord 9 ⟨0, by decide⟩) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- A binding whose value is a literal instead of the computed expression is still well typed, but
it returns 10 for `scales` on 5 where the record holds 14. The binding's value row is its second
payload word, and the literal it takes is the value row's own two words. -/
theorem a_literal_in_place_of_the_computed_value_is_rejected :
    computedLetVerdictAfter (fun artifact =>
        (artifact.setExtraWord 7 ⟨1, by decide⟩).setExtraWord 1 ⟨5, by decide⟩) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- Recorded outcomes that do not match the meaning are refused, including the payload of another
call and a wrong binding argument. -/
theorem a_wrong_recorded_computed_let_outcome_is_rejected :
    computedLetVerdictAfter (·.setObservation 0 (.value ⟨15, by decide⟩)) = false ∧
      computedLetVerdictAfter (·.setObservation 2 (.value ⟨6, by decide⟩)) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- Structurally malformed rows are refused: the binding naming itself as its value row and a
binding slot no read resolves. The binding's payload is the target slot and the value row. -/
theorem malformed_computed_let_ir_is_rejected :
    computedLetVerdictAfter (·.setExtraWord 6 ⟨3, by decide⟩) = false ∧
      computedLetVerdictAfter (·.setExtraWord 4 ⟨1, by decide⟩) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- A changed source is not certified against the same IR: the callee renamed, the let's added
constant changed, the calling function renamed, and another document identifier. The offsets are
into the source document's own bytes. -/
theorem a_substituted_computed_let_source_is_rejected :
    certifyCodes (bridgeId! 1) (computedLetSourceDocumentCodes.set 55 122)
        computedLetArtifact = none ∧
      certifyCodes (bridgeId! 1) (computedLetSourceDocumentCodes.set 103 53)
        computedLetArtifact = none ∧
      certifyCodes (bridgeId! 1) (computedLetSourceDocumentCodes.set 141 122)
        computedLetArtifact = none ∧
      certifyCodes (bridgeId! 2) computedLetSourceDocumentCodes computedLetArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning check
against the same artifact: the let's additive constant changed, the calling function's argument
replaced by a literal, and the first callee's addition reversed. -/
theorem a_wrong_computed_let_lowering_is_rejected :
    lowersTo computedLetArtifact (computedLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLet "step" (.binary .add (.var "value") (.int 5))
            (.binary .add (.var "value") (.var "step"))⟩) = false ∧
      sourceAgrees computedLetArtifact (computedLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLet "step" (.binary .add (.var "value") (.int 5))
            (.binary .add (.var "value") (.var "step"))⟩) = false ∧
      lowersTo computedLetArtifact (computedLetFunctions.set 1
        ⟨"runs", ["value"], [], none, .call "scales" [.int 5]⟩) = false ∧
      sourceAgrees computedLetArtifact (computedLetFunctions.set 1
        ⟨"runs", ["value"], [], none, .call "scales" [.int 5]⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
