import Xsh.Conformance.Vertical.CallsCertificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the third certificate proves: a function that assigns a var and returns an
expression over it, and a function whose assignment overflows. -/
def assignmentsExpectedObservations : List ObservedResult :=
  [⟨"scaled", [], .value 31⟩, ⟨"wrapped", [], .failure .integerOverflow⟩]

set_option maxRecDepth 10000 in
theorem assignments_artifact_is_the_decoded_raw_ir_document :
    decodeRawDocument assignmentsRawIrDocumentCodes = some assignmentsArtifact := by
  decide +kernel

theorem assignments_artifact_is_the_decoded_source_document :
    decodeSourceDocument assignmentsSourceDocumentCodes = some assignmentsSourceDocument := by
  decide +kernel

/-- The functions the assignments source document parses to. -/
def assignmentsFunctions : List SourceFunction :=
  [⟨"scaled", [], [("total", 5)],
      .assigned "total" (.binary .mul (.var "total") (.int 6))
        (.binary .add (.var "total") (.int 1))⟩,
    ⟨"wrapped", [], [("top", 9223372036854775807)],
      .assigned "top" (.binary .add (.var "top") (.int 1)) (.atom (.var "top"))⟩]

set_option maxRecDepth 10000 in
theorem the_assignments_source_parses_to_the_assignments_functions :
    parseSource assignmentsSourceDocument.text = some assignmentsFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_assignments_functions_lower_to_the_assignments_artifact :
    lowersTo assignmentsArtifact assignmentsFunctions = true := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_assignments_artifact_validates : validateArtifact assignmentsArtifact = true := by
  decide +kernel

theorem the_assignments_functions_mean_the_same_as_the_assignments_artifact :
    functionPrograms assignmentsArtifact = sourcePrograms assignmentsFunctions :=
  lowers_to_means_the_source the_assignments_functions_lower_to_the_assignments_artifact

set_option maxRecDepth 10000 in
theorem the_assignments_artifact_names_the_expected_outcomes :
    namedResults assignmentsArtifact = some assignmentsExpectedObservations := by
  decide +kernel

theorem assignments_pair_is_certified :
    certifyDecoded assignmentsSourceDocument assignmentsArtifact =
      some assignmentsExpectedObservations := by
  rw [certify_decoded_of_checks (by decide)
    the_assignments_source_parses_to_the_assignments_functions
    the_assignments_functions_lower_to_the_assignments_artifact
    the_assignments_artifact_validates]
  exact the_assignments_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `assignmentsSourceDocumentCodes` and
`assignmentsRawIrDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions (a
function with one integer var, an assignment of an integer value to it, and a returned integer
value, here with an arithmetic fault in the returned value's operand and in the assignment);
lowering the parsed functions reproduces the raw-IR document's rows, payload words, operation
pool, blocks and function table exactly, with only the span words taken from the document; the
artifact validates; and the base machine runs of the two recorded calls end with 31 (`scaled`:
the var holds 5, the assignment makes it 30, the return adds 1) and an integer-overflow failure
raised by the assignment (`wrapped`) in the recorded order.
That each function means the same under the source and the IR follows from the universal
`lowering_means_the_source`, not from a check of this artifact
(`the_assignments_functions_mean_the_same_as_the_assignments_artifact`).
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, and nothing covers other inputs. -/
theorem assignments_artifact_certified :
    certifyFromSource assignmentsSourceDocumentCodes assignmentsRawIrDocumentCodes =
      some assignmentsExpectedObservations := by
  rw [certify_from_source_of_decodes assignments_artifact_is_the_decoded_source_document
    assignments_artifact_is_the_decoded_raw_ir_document]
  exact assignments_pair_is_certified

def assignmentsVerdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly assignmentsExpectedObservations (mutate assignmentsArtifact)

theorem the_unmodified_assignments_artifact_is_certified :
    assignmentsVerdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- A wrong assigned operation is refused: the assignment's product replaced by a difference,
and the return's sum replaced by a difference. -/
theorem a_wrong_assigned_operation_is_rejected :
    assignmentsVerdictAfter (·.setOp 0 .sub) = false ∧
      assignmentsVerdictAfter (·.setOp 1 .sub) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A wrong assigned value or target is refused: the assignment given the literal 6 instead of
the product, given the initial literal, an assignment to a slot no let holds, a compound
assignment word, an integer slot read of a slot no let holds, an integer operand that is a
statement row, and a body whose assignment statement was dropped. -/
theorem a_wrong_assignment_is_rejected :
    assignmentsVerdictAfter (·.setExtraWord 12 ⟨3, by decide⟩) = false ∧
      assignmentsVerdictAfter (·.setExtraWord 12 ⟨0, by decide⟩) = false ∧
      assignmentsVerdictAfter (·.setExtraWord 10 ⟨0, by decide⟩) = false ∧
      assignmentsVerdictAfter (·.setExtraWord 11 ⟨1, by decide⟩) = false ∧
      assignmentsVerdictAfter (·.setExtraWord 4 ⟨0, by decide⟩) = false ∧
      assignmentsVerdictAfter (·.setExtraWord 8 ⟨1, by decide⟩) = false ∧
      assignmentsVerdictAfter (fun artifact =>
        ((artifact.setExtraWord 22 ⟨2, by decide⟩).setExtraWord 24 ⟨9, by decide⟩).mapBlock 0
          fun block => { block with len := 3 }) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

def assignmentOutcomesOf (artifact : RawArtifact) : Option (List FunctionOutcome) :=
  (namedResults artifact).map (·.map (·.outcome))

set_option maxRecDepth 10000 in
/-- The value and operation mutations above are well formed apart from the recorded outcomes and
change what the IR means, so the rejection is by the meaning and not by a structural check: with
the assignment given the literal 6 the functions end in 7 and the overflow, and with the
product a difference they end in 0 and the overflow. -/
theorem the_rejected_assignment_mutations_mean_something_else :
    (assignmentOutcomesOf (assignmentsArtifact.setExtraWord 12 ⟨3, by decide⟩) =
      some [.value 7, .failure .integerOverflow]) ∧
    (assignmentOutcomesOf (assignmentsArtifact.setOp 0 .sub) =
      some [.value 0, .failure .integerOverflow]) := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A wrong recorded result for either function is refused. -/
theorem a_wrong_recorded_assignment_result_is_rejected :
    assignmentsVerdictAfter (·.setObservation 0 (.value ⟨30, by decide⟩)) = false ∧
      assignmentsVerdictAfter (·.setObservation 1 (.value ⟨0, by decide⟩)) = false ∧
      assignmentsVerdictAfter (·.setObservation 0 (.failure .integerOverflow)) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A changed source is not certified against the same IR: another initial literal, another
assigned operation, an unbound name in the return, a changed operation in the second function, a
var that is a let, an assignment to an undeclared name, and another document identifier. -/
theorem a_substituted_assignments_source_is_rejected :
    certifyCodes (bridgeId! 1) (assignmentsSourceTextCodes.set 37 52) assignmentsArtifact = none ∧
      certifyCodes (bridgeId! 1) (assignmentsSourceTextCodes.set 55 43) assignmentsArtifact =
        none ∧
      certifyCodes (bridgeId! 1) (assignmentsSourceTextCodes.set 68 113) assignmentsArtifact =
        none ∧
      certifyCodes (bridgeId! 1) (assignmentsSourceTextCodes.set 149 45) assignmentsArtifact =
        none ∧
      certifyCodes (bridgeId! 1)
        (((assignmentsSourceTextCodes.set 25 108).set 26 101).set 27 116) assignmentsArtifact =
        none ∧
      certifyCodes (bridgeId! 1) (assignmentsSourceTextCodes.set 41 113) assignmentsArtifact =
        none ∧
      certifyCodes (bridgeId! 2) assignmentsSourceTextCodes assignmentsArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning check
against the same artifact: the product's operands exchanged (the same value, other rows), and
the assignment dropped (another value). -/
theorem a_wrong_assignments_lowering_is_rejected :
    lowersTo assignmentsArtifact (assignmentsFunctions.set 0
        ⟨"scaled", [], [("total", 5)],
          .assigned "total" (.binary .mul (.int 6) (.var "total"))
            (.binary .add (.var "total") (.int 1))⟩) = false ∧
      lowersTo assignmentsArtifact (assignmentsFunctions.set 0
        ⟨"scaled", [], [("total", 5)], .value (.binary .add (.var "total") (.int 1))⟩) = false ∧
      sourceAgrees assignmentsArtifact (assignmentsFunctions.set 0
        ⟨"scaled", [], [("total", 5)], .value (.binary .add (.var "total") (.int 1))⟩) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
