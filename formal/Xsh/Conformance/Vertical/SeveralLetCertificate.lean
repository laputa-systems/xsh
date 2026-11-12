import Xsh.Conformance.Vertical.Certificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the several-let certificate proves: `scales` on 5 and `runs` on 5 and -9,
each with the integer the call completes with. -/
def severalLetExpectedObservations : List ObservedResult :=
  [⟨"scales", [5], .value 13⟩, ⟨"runs", [5], .value 13⟩, ⟨"runs", [-9], .value (-1)⟩]

set_option maxRecDepth 100000 in
theorem severalLet_artifact_is_the_decoded_raw_ir_document :
    decodeRawDocument severalLetRawIrDocumentCodes = some severalLetArtifact := by
  decide +kernel

theorem severalLet_artifact_is_the_decoded_source_document :
    decodeSourceDocument severalLetSourceDocumentCodes = some severalLetSourceDocument := by
  decide +kernel

/-- The functions the several-let source document parses to: a function of one integer parameter
whose first binding adds the parameter and 4, whose second binding doubles the first, and which
returns the difference of the second binding and the parameter; and a function that calls it on
its parameter. -/
def severalLetFunctions : List SourceFunction :=
  [⟨"scales", ["value"], [], none,
      .computedLets [("a", .binary .add (.var "value") (.int 4)),
        ("b", .binary .mul (.var "a") (.int 2))]
        (.binary .sub (.var "b") (.var "value"))⟩,
    ⟨"runs", ["value"], [], none, .call "scales" [.var "value"]⟩]

set_option maxRecDepth 100000 in
/-- The character codes of `severalLetSourceDocument`'s text come from the exported document
(`severalLetTextCodes`), so the parser does not decode the string in the kernel. -/
theorem severalLet_text_is_the_decoded_document_text :
    severalLetSourceDocument.text = String.ofList (severalLetTextCodes.map Char.ofNat) := rfl

set_option maxRecDepth 10000 in
theorem severalLet_text_codes_round_trip :
    (severalLetTextCodes.map Char.ofNat).map Char.toNat = severalLetTextCodes := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_several_let_text_codes_parse :
    parseSourceCodes severalLetTextCodes = some severalLetFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_several_let_source_parses_to_the_several_let_functions :
    parseSource severalLetSourceDocument.text = some severalLetFunctions := by
  simp only [severalLet_text_is_the_decoded_document_text, parseSource, String.toList_ofList,
    severalLet_text_codes_round_trip, the_several_let_text_codes_parse]

set_option maxRecDepth 100000 in
theorem severalLet_span_words : spanWords severalLetArtifact = [⟨2, by decide⟩, ⟨3, by decide⟩] := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem the_several_let_functions_lower_to_the_several_let_artifact :
    lowersTo severalLetArtifact severalLetFunctions = true := by
  unfold lowersTo
  rw [severalLet_span_words]
  decide +kernel

set_option maxRecDepth 100000 in
theorem the_several_let_artifact_validates : validateArtifact severalLetArtifact = true := by
  decide +kernel

theorem the_several_let_functions_mean_the_same_as_the_several_let_artifact :
    functionPrograms severalLetArtifact = sourcePrograms severalLetFunctions :=
  lowers_to_means_the_source the_several_let_functions_lower_to_the_several_let_artifact

set_option maxRecDepth 100000 in
theorem the_several_let_artifact_names_the_expected_outcomes :
    namedResults severalLetArtifact = some severalLetExpectedObservations := by
  decide +kernel

theorem several_let_pair_is_certified :
    certifyDecoded severalLetSourceDocument severalLetArtifact =
      some severalLetExpectedObservations := by
  rw [certify_decoded_of_checks (by decide)
    the_several_let_source_parses_to_the_several_let_functions
    the_several_let_functions_lower_to_the_several_let_artifact the_several_let_artifact_validates]
  exact the_several_let_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `severalLetSourceDocumentCodes` and
`severalLetRawIrDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions (two
integer functions: `scales` has one integer parameter and two computed lets, the first adding the
parameter and 4 and the second doubling the first, and returns the difference of the second and the
parameter; `runs` calls `scales` on its parameter); the parameter takes slot 0 and the bindings take
slots 2 and 3, where the exporter places them; lowering the parsed functions emits each value in
the integer family in source order followed by its `StmtLetInt` binding, so the second value reads
the first binding at its slot, and reproduces the raw-IR document's rows, payload words, operation
pool (`Add`, `Mul`, `Sub`), blocks and function table exactly, with only the span words taken from
the document; the artifact validates; and the base machine runs of the three recorded calls end with
13 for `scales` on 5, 13 for `runs` on 5 and -1 for `runs` on -9. That each function means the same
under the source and the IR follows from the universal `lowering_means_the_source`, not from a
check of this artifact (`the_several_let_functions_mean_the_same_as_the_several_let_artifact`).
A callee's computed bindings are part of its callee form in source order; the call applies the
parameters and each binding value, and every earlier binding slot inside a later value is replaced
by that earlier binding's applied value, because the base machine evaluates call arguments in the
caller before binding them. The earlier binding still evaluates as its own argument, and the
substitution makes the machine evaluate its applied expression again inside the later value; the
admitted values are pure integer expressions, so that gives the value and the first failure of
evaluating each binding once in order.
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, nothing covers a callee outside this shape, and nothing covers other inputs. -/
theorem severalLet_artifact_certified :
    certifyFromSource severalLetSourceDocumentCodes severalLetRawIrDocumentCodes =
      some severalLetExpectedObservations := by
  rw [certify_from_source_of_decodes severalLet_artifact_is_the_decoded_source_document
    severalLet_artifact_is_the_decoded_raw_ir_document]
  exact several_let_pair_is_certified

def severalLetVerdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly severalLetExpectedObservations (mutate severalLetArtifact)

theorem the_unmodified_several_let_artifact_is_certified :
    severalLetVerdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- A call retargeted to a function the callee table does not hold is refused: `runs` names
function two, while only `scales` precedes it. The call's function word is the first payload
word. -/
theorem a_wrong_several_let_callee_is_rejected :
    severalLetVerdictAfter (·.setExtraWord 31 ⟨2, by decide⟩) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- A second binding whose read is replaced by the parameter is still well typed, but it returns 5
for `scales` on 5 where the record holds 13. The read is the first word of the second value's
`IntSlot` row. -/
theorem a_wrong_several_let_binding_read_is_rejected :
    severalLetVerdictAfter (·.setExtraWord 8 ⟨0, by decide⟩) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- Reordering the two binding statements is refused: the binding that now runs first reads slot 2
before any statement binds it, so no recorded call has an outcome. The body block holds the
statement count, both binding rows and the return. -/
theorem a_reordered_several_let_binding_is_rejected :
    severalLetVerdictAfter (fun artifact =>
        (artifact.setExtraWord 24 ⟨7, by decide⟩).setExtraWord 25 ⟨3, by decide⟩) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- Swapping the first binding's addition for a multiplication keeps every row well formed and the
recorded calls defined, but the program now returns 35 for `scales` on 5 where 13 is recorded. -/
theorem a_swapped_several_let_operator_is_rejected :
    severalLetVerdictAfter (·.setOp 0 .mul) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- The swapped-operator mutant is a well-formed artifact: with its own outcomes recorded, 35, 35
and -63, it validates, so the refusal comes only from the recorded calls of the source. -/
theorem the_swapped_several_let_operator_is_well_formed :
    validateArtifact ((((severalLetArtifact.setOp 0 .mul).setObservation 0
        (.value ⟨35, by decide⟩)).setObservation 1 (.value ⟨35, by decide⟩)).setObservation 2
        (.value ⟨18446744073709551553, by decide⟩)) = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem the_swapped_several_let_operator_means_something_else :
    namedResults (severalLetArtifact.setOp 0 .mul) =
      some [⟨"scales", [5], .value 35⟩, ⟨"runs", [5], .value 35⟩,
        ⟨"runs", [-9], .value (-63)⟩] := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- Recorded outcomes that do not match the meaning are refused, including the first call's value
and the last call's payload. -/
theorem a_wrong_recorded_several_let_outcome_is_rejected :
    severalLetVerdictAfter (·.setObservation 0 (.value ⟨14, by decide⟩)) = false ∧
      severalLetVerdictAfter (·.setObservation 2 (.value ⟨0, by decide⟩)) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- Structurally malformed rows are refused: the first binding naming itself as its value, the
second binding naming the first binding statement as its value, and two bindings on the same slot.
The bindings' payloads are the target slot and the value row. -/
theorem malformed_several_let_ir_is_rejected :
    severalLetVerdictAfter (·.setExtraWord 7 ⟨3, by decide⟩) = false ∧
      severalLetVerdictAfter (·.setExtraWord 15 ⟨3, by decide⟩) = false ∧
      severalLetVerdictAfter (·.setExtraWord 14 ⟨2, by decide⟩) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- A changed source is not certified against the same IR: the callee renamed, the first added
constant changed, the second operation changed, the returned name changed, and another document
identifier. The offsets are into the source text's own bytes. -/
theorem a_substituted_several_let_source_is_rejected :
    certifyCodes (bridgeId! 1) (severalLetTextCodes.set 5 122) severalLetArtifact = none ∧
      certifyCodes (bridgeId! 1) (severalLetTextCodes.set 51 53) severalLetArtifact = none ∧
      certifyCodes (bridgeId! 1) (severalLetTextCodes.set 65 43) severalLetArtifact = none ∧
      certifyCodes (bridgeId! 1) (severalLetTextCodes.set 78 97) severalLetArtifact = none ∧
      certifyCodes (bridgeId! 2) severalLetTextCodes severalLetArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning check
against the same artifact: the first binding's constant changed, the second binding's operation
changed, the returned difference reversed, and the calling function's argument replaced by a
literal. -/
theorem a_wrong_several_let_lowering_is_rejected :
    lowersTo severalLetArtifact (severalLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLets [("a", .binary .add (.var "value") (.int 5)),
            ("b", .binary .mul (.var "a") (.int 2))]
            (.binary .sub (.var "b") (.var "value"))⟩) = false ∧
      sourceAgrees severalLetArtifact (severalLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLets [("a", .binary .add (.var "value") (.int 5)),
            ("b", .binary .mul (.var "a") (.int 2))]
            (.binary .sub (.var "b") (.var "value"))⟩) = false ∧
      lowersTo severalLetArtifact (severalLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLets [("a", .binary .add (.var "value") (.int 4)),
            ("b", .binary .add (.var "a") (.int 2))]
            (.binary .sub (.var "b") (.var "value"))⟩) = false ∧
      sourceAgrees severalLetArtifact (severalLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLets [("a", .binary .add (.var "value") (.int 4)),
            ("b", .binary .add (.var "a") (.int 2))]
            (.binary .sub (.var "b") (.var "value"))⟩) = false ∧
      lowersTo severalLetArtifact (severalLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLets [("a", .binary .add (.var "value") (.int 4)),
            ("b", .binary .mul (.var "a") (.int 2))]
            (.binary .add (.var "b") (.var "value"))⟩) = false ∧
      sourceAgrees severalLetArtifact (severalLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLets [("a", .binary .add (.var "value") (.int 4)),
            ("b", .binary .mul (.var "a") (.int 2))]
            (.binary .add (.var "b") (.var "value"))⟩) = false ∧
      lowersTo severalLetArtifact (severalLetFunctions.set 1
        ⟨"runs", ["value"], [], none, .call "scales" [.int 5]⟩) = false ∧
      sourceAgrees severalLetArtifact (severalLetFunctions.set 1
        ⟨"runs", ["value"], [], none, .call "scales" [.int 5]⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
/-- A source tree with the two bindings in the other order is refused against the same artifact:
the first binding would then read a name bound only after it, so the tree neither lowers to the
artifact nor has a meaning that agrees with the recorded calls. -/
theorem a_reordered_several_let_source_tree_is_rejected :
    lowersTo severalLetArtifact (severalLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLets [("b", .binary .mul (.var "a") (.int 2)),
            ("a", .binary .add (.var "value") (.int 4))]
            (.binary .sub (.var "b") (.var "value"))⟩) = false ∧
      sourceAgrees severalLetArtifact (severalLetFunctions.set 0
        ⟨"scales", ["value"], [], none,
          .computedLets [("b", .binary .mul (.var "a") (.int 2)),
            ("a", .binary .add (.var "value") (.int 4))]
            (.binary .sub (.var "b") (.var "value"))⟩) = false := by
  constructor <;> decide +kernel

end Xsh.Conformance.Vertical
