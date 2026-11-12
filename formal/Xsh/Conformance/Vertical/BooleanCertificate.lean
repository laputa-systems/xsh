import Xsh.Conformance.Vertical.CallsCertificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the boolean certificate proves: `exceeds` on two argument pairs and `below`
on the same pairs, with the truth of each comparison. -/
def booleanExpectedObservations : List ObservedResult :=
  [⟨"exceeds", [12, 9], .truth true⟩, ⟨"exceeds", [3, 9], .truth false⟩,
    ⟨"below", [3, 9], .truth true⟩, ⟨"below", [12, 9], .truth false⟩]

set_option maxRecDepth 10000 in
theorem boolean_artifact_is_the_decoded_raw_ir_document :
    decodeRawDocument booleanRawIrDocumentCodes = some booleanArtifact := by
  decide +kernel

theorem boolean_artifact_is_the_decoded_source_document :
    decodeSourceDocument booleanSourceDocumentCodes = some booleanSourceDocument := by
  decide +kernel

/-- The functions the boolean source document parses to. -/
def booleanFunctions : List SourceFunction :=
  [⟨"exceeds", ["value", "limit"], [], none,
      .comparison .gt (.var "value") (.var "limit")⟩,
    ⟨"below", ["value", "limit"], [], none,
      .comparison .lt (.var "value") (.var "limit")⟩]

set_option maxRecDepth 10000 in
theorem the_boolean_source_parses_to_the_boolean_functions :
    parseSource booleanSourceDocument.text = some booleanFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_boolean_functions_lower_to_the_boolean_artifact :
    lowersTo booleanArtifact booleanFunctions = true := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_boolean_artifact_validates : validateArtifact booleanArtifact = true := by
  decide +kernel

theorem the_boolean_functions_mean_the_same_as_the_boolean_artifact :
    functionPrograms booleanArtifact = sourcePrograms booleanFunctions :=
  lowers_to_means_the_source the_boolean_functions_lower_to_the_boolean_artifact

set_option maxRecDepth 10000 in
theorem the_boolean_artifact_names_the_expected_outcomes :
    namedResults booleanArtifact = some booleanExpectedObservations := by
  decide +kernel

theorem boolean_pair_is_certified :
    certifyDecoded booleanSourceDocument booleanArtifact = some booleanExpectedObservations := by
  rw [certify_decoded_of_checks (by decide) the_boolean_source_parses_to_the_boolean_functions
    the_boolean_functions_lower_to_the_boolean_artifact the_boolean_artifact_validates]
  exact the_boolean_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `booleanSourceDocumentCodes` and
`booleanRawIrDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions (two
functions of two integer parameters, each returning a comparison of its parameters under `-> Bool`);
the declared `Bool` return admits only the comparison result; lowering the parsed functions
reproduces the raw-IR document's rows, payload words, operation pool (`Gt` then `Lt`), blocks and
function table exactly, with only the span words taken from the document; the artifact validates;
and the base machine runs of the four recorded calls end with true and false for `exceeds` on
12, 9 and 3, 9 and true and false for `below` on 3, 9 and 12, 9, in the recorded order. A boolean
outcome is recorded as one value bit (`1` for true).
That each function means the same under the source and the IR follows from the universal
`lowering_means_the_source`, not from a check of this artifact
(`the_boolean_functions_mean_the_same_as_the_boolean_artifact`).
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, and nothing covers other inputs. -/
theorem boolean_artifact_certified :
    certifyFromSource booleanSourceDocumentCodes booleanRawIrDocumentCodes =
      some booleanExpectedObservations := by
  rw [certify_from_source_of_decodes boolean_artifact_is_the_decoded_source_document
    boolean_artifact_is_the_decoded_raw_ir_document]
  exact boolean_pair_is_certified

def booleanVerdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly booleanExpectedObservations (mutate booleanArtifact)

theorem the_unmodified_boolean_artifact_is_certified : booleanVerdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- A wrong comparison is refused: `exceeds` compared by subtraction or a difference, and `below`
compared by the other direction. -/
theorem a_wrong_boolean_operation_is_rejected :
    booleanVerdictAfter (·.setOp 0 .sub) = false ∧
      booleanVerdictAfter (·.setOp 0 .add) = false ∧
      booleanVerdictAfter (·.setOp 1 .gt) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Exchanged operands are refused: both comparison rows have their two operand words swapped,
which reverses each comparison on the recorded arguments. -/
theorem a_wrong_boolean_operand_order_is_rejected :
    booleanVerdictAfter (·.swapExtraWords 3 4) = false ∧
      booleanVerdictAfter (·.swapExtraWords 12 13) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A recorded truth that does not match the meaning is refused, including the bit for the other
boolean and an integer outcome where a boolean is due. -/
theorem a_wrong_recorded_truth_is_rejected :
    booleanVerdictAfter (·.setObservation 0 (.value ⟨0, by decide⟩)) = false ∧
      booleanVerdictAfter (·.setObservation 1 (.value ⟨1, by decide⟩)) = false ∧
      booleanVerdictAfter (·.setObservation 2 (.value ⟨0, by decide⟩)) = false ∧
      booleanVerdictAfter (·.setObservation 3 (.value ⟨1, by decide⟩)) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A wrong recorded argument list is refused. -/
theorem a_wrong_recorded_boolean_argument_is_rejected :
    booleanVerdictAfter (·.setArguments 0 [⟨9, by decide⟩, ⟨12, by decide⟩]) = false ∧
      booleanVerdictAfter (·.setArguments 0 [⟨12, by decide⟩]) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Structurally malformed IR over the exchanged rows is refused: an identifier out of table
order, a payload of the wrong width, an operand that does not precede its user, a body block whose
count word disagrees with its range, and a function with the wrong parameter count. -/
theorem malformed_boolean_ir_is_rejected :
    booleanVerdictAfter (·.mapRow 3 fun row => { row with id := bridgeId! 99 }) = false ∧
      booleanVerdictAfter (·.mapRow 2 fun row => { row with rhs := 3 }) = false ∧
      booleanVerdictAfter (·.setPayloadWord 2 1 ⟨2, by decide⟩) = false ∧
      booleanVerdictAfter (·.setExtraWord 7 ⟨3, by decide⟩) = false ∧
      booleanVerdictAfter (·.mapFunction 1 fun function => { function with paramsLen := 1 }) =
        false := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A changed source is not certified against the same IR: `>` changed to `<`, the `Bool`
annotation changed to `Int`, the second literal changed, and another document identifier. -/
theorem a_substituted_boolean_source_is_rejected :
    certifyCodes (bridgeId! 1) (booleanSourceTextCodes.set 62 60) booleanArtifact = none ∧
      certifyCodes (bridgeId! 1) (booleanSourceTextCodes.set 40 73) booleanArtifact = none ∧
      certifyCodes (bridgeId! 1) (booleanSourceTextCodes.set 62 43) booleanArtifact = none ∧
      certifyCodes (bridgeId! 2) booleanSourceTextCodes booleanArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning check
against the same artifact: exchanged operands in each comparison and the other comparison. -/
theorem a_wrong_boolean_lowering_is_rejected :
    lowersTo booleanArtifact (booleanFunctions.set 0
        ⟨"exceeds", ["value", "limit"], [], none,
          .comparison .gt (.var "limit") (.var "value")⟩) = false ∧
      sourceAgrees booleanArtifact (booleanFunctions.set 0
        ⟨"exceeds", ["value", "limit"], [], none,
          .comparison .gt (.var "limit") (.var "value")⟩) = false ∧
      lowersTo booleanArtifact (booleanFunctions.set 1
        ⟨"below", ["value", "limit"], [], none,
          .comparison .lt (.var "limit") (.var "value")⟩) = false ∧
      sourceAgrees booleanArtifact (booleanFunctions.set 1
        ⟨"below", ["value", "limit"], [], none,
          .comparison .lt (.var "limit") (.var "value")⟩) = false ∧
      lowersTo booleanArtifact (booleanFunctions.set 1
        ⟨"below", ["value", "limit"], [], none,
          .comparison .gt (.var "value") (.var "limit")⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
