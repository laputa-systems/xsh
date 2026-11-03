import Xsh.Conformance.Vertical.Meaning
import Xsh.Conformance.Vertical.LoweringSound
import Xsh.Conformance.Vertical.Documents
import Xsh.Conformance.Vertical.Fixture

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered calls the certificate proves: each call the raw-IR document records, with its
arguments and the integer its IR meaning completes with or the specified failure it raises. -/
def expectedObservations : List ObservedResult :=
  [⟨"divided", [], .failure .divisionByZero⟩, ⟨"overflowed", [], .failure .integerOverflow⟩]

set_option maxRecDepth 10000 in
theorem fixture_is_the_decoded_raw_ir_document :
    decodeRawDocument rawIrDocumentCodes = some fixtureArtifact := by
  decide +kernel

theorem fixture_is_the_decoded_source_document :
    decodeSourceDocument sourceDocumentCodes = some fixtureSourceDocument := by
  decide +kernel

/-- The functions the source document parses to. -/
def fixtureFunctions : List SourceFunction :=
  [⟨"divided", [], [("zero", 0)], .value (.binary .div (.int 1) (.var "zero"))⟩,
    ⟨"overflowed", [], [("top", 9223372036854775807)],
      .value (.binary .add (.var "top") (.int 1))⟩]

set_option maxRecDepth 10000 in
theorem the_fixture_source_parses_to_the_fixture_functions :
    parseSource fixtureSourceDocument.text = some fixtureFunctions := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_fixture_functions_lower_to_the_artifact :
    lowersTo fixtureArtifact fixtureFunctions = true := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_fixture_artifact_validates : validateArtifact fixtureArtifact = true := by
  decide +kernel

theorem the_fixture_functions_mean_the_same_as_the_artifact :
    functionPrograms fixtureArtifact = sourcePrograms fixtureFunctions :=
  lowers_to_means_the_source the_fixture_functions_lower_to_the_artifact

set_option maxRecDepth 10000 in
theorem the_fixture_artifact_names_the_expected_outcomes :
    namedResults fixtureArtifact = some expectedObservations := by
  decide +kernel

theorem fixture_pair_is_certified :
    certifyDecoded fixtureSourceDocument fixtureArtifact = some expectedObservations := by
  rw [certify_decoded_of_checks (by decide) the_fixture_source_parses_to_the_fixture_functions
    the_fixture_functions_lower_to_the_artifact the_fixture_artifact_validates]
  exact the_fixture_artifact_names_the_expected_outcomes

/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `sourceDocumentCodes` and `rawIrDocumentCodes`.
Proposition: the source document decodes and its text parses in the admitted productions
(functions with one integer let returning an integer value); lowering the parsed
functions reproduces the raw-IR document's rows, payload words, operation pool, blocks and
function table exactly, with only the span words taken from the document; the artifact
validates; and the base machine runs of the two recorded calls end with a division-by-zero
failure and an integer-overflow failure in the recorded order.
That the functions mean the same under the source and the IR follows from the universal
`lowering_means_the_source`, not from a check of this artifact
(`the_fixture_functions_mean_the_same_as_the_artifact`).
The parser and lowerer are independent Lean definitions that agree with Rust on this artifact;
nothing here shows that Rust's parser reads the source as this tree or that Rust executes this
IR, and nothing covers other inputs. -/
theorem base_vertical_artifact_certified :
    certifyFromSource sourceDocumentCodes rawIrDocumentCodes = some expectedObservations := by
  rw [certify_from_source_of_decodes fixture_is_the_decoded_source_document
    fixture_is_the_decoded_raw_ir_document]
  exact fixture_pair_is_certified

def certifiesExactly (expected : List ObservedResult) (artifact : RawArtifact) : Bool :=
  validateArtifact artifact && namedResults artifact == some expected

/-- The verdict on the decoded fixture after a corruption. The fixture equals the decoding of
the exchanged bytes by `fixture_is_the_decoded_raw_ir_document`. -/
def verdictAfter (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly expectedObservations (mutate fixtureArtifact)

def RawArtifact.setPayloadWord (artifact : RawArtifact) (rowIndex offset : Nat)
    (word : Word32) : RawArtifact :=
  match artifact.rows[rowIndex]? with
  | some row => { artifact with extra := artifact.extra.set (row.lhs.val + offset) word }
  | none => artifact

def RawArtifact.setOp (artifact : RawArtifact) (index : Nat) (op : FragmentOp) : RawArtifact :=
  { artifact with ops := artifact.ops.set index op }

theorem unmodified_artifact_is_certified : verdictAfter id = true := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem a_wrong_binding_id_is_rejected :
    verdictAfter (·.setPayloadWord 3 0 ⟨0, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 8 0 ⟨0, by decide⟩) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_wrong_arithmetic_operation_is_rejected :
    verdictAfter (·.setOp 0 .mul) = false ∧ verdictAfter (·.setOp 1 .sub) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_mutated_ir_operand_is_rejected :
    verdictAfter (·.setPayloadWord 10 1 ⟨9, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 4 1 ⟨4, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 4 1 ⟨99, by decide⟩) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_missing_or_reordered_observation_is_rejected :
    verdictAfter (fun artifact => { artifact with observations := artifact.observations.drop 1 }) =
        false ∧
      verdictAfter (fun artifact =>
        { artifact with observations := artifact.observations.reverse }) = false ∧
      verdictAfter (fun artifact =>
        { artifact with observations := artifact.observations ++ artifact.observations }) =
        false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

def RawArtifact.setObservation (artifact : RawArtifact) (index : Nat)
    (outcome : ObservedOutcome) : RawArtifact :=
  match artifact.observations[index]? with
  | some observation =>
      { artifact with observations := artifact.observations.set index { observation with outcome } }
  | none => artifact

def RawArtifact.setArguments (artifact : RawArtifact) (index : Nat) (arguments : List Word64) :
    RawArtifact :=
  match artifact.observations[index]? with
  | some observation =>
      { artifact with
          observations := artifact.observations.set index { observation with arguments } }
  | none => artifact

set_option maxRecDepth 10000 in
theorem a_wrong_recorded_result_is_rejected :
    verdictAfter (·.setObservation 0 (.value ⟨4, by decide⟩)) = false ∧
      verdictAfter (·.setObservation 1 (.value ⟨43, by decide⟩)) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_wrong_failure_outcome_is_rejected :
    verdictAfter (·.setObservation 0 (.failure .integerOverflow)) = false ∧
      verdictAfter (·.setObservation 1 (.failure .divisionByZero)) = false ∧
      verdictAfter (·.setObservation 0 (.value ⟨0, by decide⟩)) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem the_certificate_fails_for_other_claims :
    certifiesExactly (expectedObservations.reverse) fixtureArtifact = false ∧
      certifiesExactly [⟨"divided", [], .failure .divisionByZero⟩,
        ⟨"overflowed", [], .value 0⟩] fixtureArtifact = false ∧
      certifiesExactly (expectedObservations.take 1) fixtureArtifact = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

def RawArtifact.mapRow (artifact : RawArtifact) (index : Nat) (change : Row → Row) :
    RawArtifact :=
  match artifact.rows[index]? with
  | some row => { artifact with rows := artifact.rows.set index (change row) }
  | none => artifact

def RawArtifact.mapBlock (artifact : RawArtifact) (index : Nat) (change : Block → Block) :
    RawArtifact :=
  match artifact.blocks[index]? with
  | some block => { artifact with blocks := artifact.blocks.set index (change block) }
  | none => artifact

def RawArtifact.mapFunction (artifact : RawArtifact) (index : Nat)
    (change : FunctionRow → FunctionRow) : RawArtifact :=
  match artifact.functions[index]? with
  | some function => { artifact with functions := artifact.functions.set index (change function) }
  | none => artifact

def RawArtifact.setExtraWord (artifact : RawArtifact) (index : Nat) (word : Word32) :
    RawArtifact :=
  { artifact with extra := artifact.extra.set index word }

set_option maxRecDepth 10000 in
/-- Structurally malformed IR over the exchanged rows, each of which the validator refuses:
an identifier out of table order, a payload range outside the extra table, a payload of the
wrong width, an operand that does not precede its user, an operation outside the pool, a block
whose count word disagrees with its range, and a block owned by another function. -/
theorem malformed_ir_is_rejected :
    verdictAfter (·.mapRow 4 fun row => { row with id := bridgeId! 99 }) = false ∧
      verdictAfter (·.mapRow 4 fun row => { row with rhs := 99 }) = false ∧
      verdictAfter (·.mapRow 4 fun row => { row with rhs := 3 }) = false ∧
      verdictAfter (·.setPayloadWord 4 1 ⟨5, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 4 0 ⟨7, by decide⟩) = false ∧
      verdictAfter (·.setExtraWord 12 ⟨3, by decide⟩) = false ∧
      verdictAfter (·.mapBlock 0 fun block => { block with owner := ⟨some (bridgeId! 2)⟩ }) =
        false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A changed source is not certified against the same IR: another literal, another operator, an
unbound name, a literal past the signed 64-bit range, and another document identifier. -/
theorem a_substituted_source_is_rejected :
    certifyCodes (bridgeId! 1) (fixtureSourceTextCodes.set 37 52) fixtureArtifact = none ∧
      certifyCodes (bridgeId! 1) (fixtureSourceTextCodes.set 50 45) fixtureArtifact = none ∧
      certifyCodes (bridgeId! 1) (fixtureSourceTextCodes.set 52 113) fixtureArtifact = none ∧
      certifyCodes (bridgeId! 1) (fixtureSourceTextCodes.set 117 56) fixtureArtifact = none ∧
      certifyCodes (bridgeId! 2) fixtureSourceTextCodes fixtureArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning
check against the same artifact: swapped operands, another operator, a dropped binding and a
changed literal. -/
theorem a_wrong_lowering_is_rejected :
    lowersTo fixtureArtifact (fixtureFunctions.set 0
        ⟨"divided", [], [("zero", 0)], .value (.binary .div (.var "zero") (.int 1))⟩) = false ∧
      sourceAgrees fixtureArtifact (fixtureFunctions.set 0
        ⟨"divided", [], [("zero", 0)], .value (.binary .div (.var "zero") (.int 1))⟩) = false ∧
      lowersTo fixtureArtifact (fixtureFunctions.set 1
        ⟨"overflowed", [], [("top", 9223372036854775807)],
          .value (.binary .sub (.var "top") (.int 1))⟩) = false ∧
      sourceAgrees fixtureArtifact (fixtureFunctions.set 1
        ⟨"overflowed", [], [("top", 9223372036854775807)],
          .value (.binary .sub (.var "top") (.int 1))⟩) = false ∧
      lowersTo fixtureArtifact (fixtureFunctions.set 0
        ⟨"divided", [], [], .value (.binary .div (.int 1) (.int 0))⟩) = false ∧
      sourceAgrees fixtureArtifact (fixtureFunctions.set 0
        ⟨"divided", [], [("zero", 1)], .value (.binary .div (.int 1) (.var "zero"))⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
