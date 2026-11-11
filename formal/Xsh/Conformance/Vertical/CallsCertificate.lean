import Xsh.Conformance.Vertical.Certificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the second certificate proves: a conditional over two parameters on both
branches, and a function that calls it. -/
def callsExpectedObservations : List ObservedResult :=
  [⟨"excess", [12, 9], .value 3⟩, ⟨"excess", [3, 9], .value 0⟩, ⟨"surplus", [], .value 3⟩]

set_option maxRecDepth 10000 in
theorem calls_artifact_is_the_decoded_raw_ir_document :
    decodeRawDocument callsRawIrDocumentCodes = some callsArtifact := by
  decide +kernel

theorem calls_artifact_is_the_decoded_source_document :
    decodeSourceDocument callsSourceDocumentCodes = some callsSourceDocument := by
  decide +kernel

/-- The functions the calls source document parses to. -/
def callsFunctions : List SourceFunction :=
  [⟨"excess", ["value", "limit"], [], none,
      .conditional .gt (.var "value") (.var "limit") (.binary .sub (.var "value") (.var "limit"))
        (.atom (.int 0))⟩,
    ⟨"surplus", [], [("x", 12)], none, .call "excess" [.var "x", .int 9]⟩]

set_option maxRecDepth 10000 in
theorem the_calls_source_parses_to_the_calls_functions :
    parseSource callsSourceDocument.text = some callsFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_calls_functions_lower_to_the_calls_artifact :
    lowersTo callsArtifact callsFunctions = true := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_calls_artifact_validates : validateArtifact callsArtifact = true := by
  decide +kernel

theorem the_calls_functions_mean_the_same_as_the_calls_artifact :
    functionPrograms callsArtifact = sourcePrograms callsFunctions :=
  lowers_to_means_the_source the_calls_functions_lower_to_the_calls_artifact

set_option maxRecDepth 10000 in
theorem the_calls_artifact_names_the_expected_outcomes :
    namedResults callsArtifact = some callsExpectedObservations := by
  decide +kernel

theorem calls_pair_is_certified :
    certifyDecoded callsSourceDocument callsArtifact = some callsExpectedObservations := by
  rw [certify_decoded_of_checks (by decide) the_calls_source_parses_to_the_calls_functions
    the_calls_functions_lower_to_the_calls_artifact the_calls_artifact_validates]
  exact the_calls_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `callsSourceDocumentCodes` and
`callsRawIrDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions (a
function of two integer parameters returning a conditional on a comparison of its parameters,
and a function with one integer let returning a call of the first on atoms); lowering the
parsed functions reproduces the raw-IR document's rows, payload words, operation pool, blocks
and function table exactly, with only the span words taken from the document; the artifact
validates; and the base machine runs of the three recorded calls end with 3 and 0 (`excess` on
12, 9 and on 3, 9) and 3 (`surplus`, which calls `excess`) in the recorded order.
That each function means the same under the source and the IR follows from the universal
`lowering_means_the_source`, not from a check of this artifact
(`the_calls_functions_mean_the_same_as_the_calls_artifact`).
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, and nothing covers other inputs. -/
theorem calls_artifact_certified :
    certifyFromSource callsSourceDocumentCodes callsRawIrDocumentCodes =
      some callsExpectedObservations := by
  rw [certify_from_source_of_decodes calls_artifact_is_the_decoded_source_document
    calls_artifact_is_the_decoded_raw_ir_document]
  exact calls_pair_is_certified

def callsVerdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly callsExpectedObservations (mutate callsArtifact)

def RawArtifact.swapExtraWords (artifact : RawArtifact) (first second : Nat) : RawArtifact :=
  match artifact.extra[first]?, artifact.extra[second]? with
  | some one, some other =>
      { artifact with extra := (artifact.extra.set first other).set second one }
  | _, _ => artifact

def outcomesOf (artifact : RawArtifact) : Option (List FunctionOutcome) :=
  (namedResults artifact).map (·.map (·.outcome))

theorem the_unmodified_calls_artifact_is_certified : callsVerdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- A conditional that takes the other branch is refused: the condition's operands exchanged
(`limit > value`), the then value replaced by the else value, the else value replaced by the
then value, and the comparison replaced by arithmetic, which is not a boolean. -/
theorem a_wrong_branch_is_rejected :
    callsVerdictAfter (·.swapExtraWords 3 4) = false ∧
      callsVerdictAfter (·.setExtraWord 19 ⟨10, by decide⟩) = false ∧
      callsVerdictAfter (·.setPayloadWord 11 1 ⟨7, by decide⟩) = false ∧
      callsVerdictAfter (·.setOp 0 .sub) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A call that binds its arguments to the wrong parameters is refused: the two argument rows
exchanged in the call's argument table, either row of that table repeated, the two parameter
reads exchanged in the callee, the recorded arguments exchanged, a callee with another parameter
count, and a call to the function itself instead of an earlier one. -/
theorem a_wrong_argument_binding_is_rejected :
    callsVerdictAfter (·.swapExtraWords 42 44) = false ∧
      callsVerdictAfter (·.setExtraWord 42 ⟨16, by decide⟩) = false ∧
      callsVerdictAfter (·.setExtraWord 44 ⟨15, by decide⟩) = false ∧
      callsVerdictAfter (fun artifact =>
        (artifact.setPayloadWord 0 0 ⟨1, by decide⟩).setPayloadWord 1 0 ⟨0, by decide⟩) = false ∧
      callsVerdictAfter (·.setArguments 0 [⟨9, by decide⟩, ⟨12, by decide⟩]) = false ∧
      callsVerdictAfter (·.mapFunction 0 fun function => { function with paramsLen := 1 }) =
        false ∧
      callsVerdictAfter (·.setPayloadWord 17 0 ⟨2, by decide⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- The branch and binding mutations above are well formed and change what the IR means, so the
rejection is by the meaning and not by a structural check: with the then value replaced by the
else value every recorded `excess` call and `surplus` end in 0 unchanged, and with the two
arguments exchanged `surplus` ends in 0. -/
theorem the_rejected_branch_and_binding_mutations_mean_something_else :
    (validateArtifact (callsArtifact.setExtraWord 19 ⟨10, by decide⟩) = false ∧
      outcomesOf (callsArtifact.setExtraWord 19 ⟨10, by decide⟩) =
        some [.value 0, .value 0, .value 0]) ∧
    (validateArtifact (callsArtifact.swapExtraWords 42 44) = false ∧
      outcomesOf (callsArtifact.swapExtraWords 42 44) =
        some [.value 3, .value 0, .value 0]) := by
  refine ⟨⟨?_, ?_⟩, ⟨?_, ?_⟩⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Malformed blocks and tables are refused: a block named by two users, a block named by none,
a value block whose range holds more than its statement, and a recorded call with a missing
argument. -/
theorem malformed_blocks_and_calls_are_rejected :
    callsVerdictAfter (·.setPayloadWord 10 0 ⟨1, by decide⟩) = false ∧
      callsVerdictAfter (·.setPayloadWord 7 0 ⟨3, by decide⟩) = false ∧
      callsVerdictAfter (·.mapBlock 2 fun block => { block with len := 3 }) = false ∧
      callsVerdictAfter (·.setArguments 0 [⟨12, by decide⟩]) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A changed source is not certified against the same IR: another operator, another literal in
the call, an unbound name, the comparison dropped, a changed branch value, and another document
identifier. -/
theorem a_substituted_calls_source_is_rejected :
    certifyCodes (bridgeId! 1) (callsSourceTextCodes.set 79 43) callsArtifact = none ∧
      certifyCodes (bridgeId! 1) (callsSourceTextCodes.set 159 56) callsArtifact = none ∧
      certifyCodes (bridgeId! 1) (callsSourceTextCodes.set 81 113) callsArtifact = none ∧
      certifyCodes (bridgeId! 1) (callsSourceTextCodes.set 63 43) callsArtifact = none ∧
      certifyCodes (bridgeId! 1) (callsSourceTextCodes.set 96 49) callsArtifact = none ∧
      certifyCodes (bridgeId! 2) callsSourceTextCodes callsArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning
check against the same artifact: a call with exchanged arguments and a conditional whose
branches are exchanged. -/
theorem a_wrong_calls_lowering_is_rejected :
    sourceAgrees callsArtifact (callsFunctions.set 1
        ⟨"surplus", [], [("x", 12)], none, .call "excess" [.int 9, .var "x"]⟩) = false ∧
      lowersTo callsArtifact (callsFunctions.set 1
        ⟨"surplus", [], [("x", 12)], none, .call "excess" [.int 9, .var "x"]⟩) = false ∧
      sourceAgrees callsArtifact (callsFunctions.set 0
        ⟨"excess", ["value", "limit"], [], none, .conditional .gt (.var "value") (.var "limit")
          (.atom (.int 0)) (.binary .sub (.var "value") (.var "limit"))⟩) = false ∧
      lowersTo callsArtifact (callsFunctions.set 0
        ⟨"excess", ["limit", "value"], [], none, .conditional .gt (.var "value") (.var "limit")
          (.binary .sub (.var "value") (.var "limit")) (.atom (.int 0))⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
