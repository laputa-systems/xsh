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
  [⟨"shifted", [], .value 5⟩, ⟨"scaled", [], .value 42⟩,
    ⟨"divided", [], .failure .divisionByZero⟩, ⟨"overflowed", [], .failure .integerOverflow⟩,
    ⟨"excess", [12, 9], .value 3⟩, ⟨"excess", [3, 9], .value 0⟩, ⟨"surplus", [], .value 3⟩]

set_option maxRecDepth 10000 in
theorem fixture_is_the_decoded_raw_ir_document :
    decodeRawDocument rawIrDocumentCodes = some fixtureArtifact := by
  decide +kernel

theorem fixture_is_the_decoded_source_document :
    decodeSourceDocument sourceDocumentCodes = some fixtureSourceDocument := by
  decide +kernel

/-- The functions the source document parses to. -/
def fixtureFunctions : List SourceFunction :=
  [⟨"shifted", [], [("x", 2)], .value (.binary .add (.var "x") (.int 3))⟩,
    ⟨"scaled", [], [("y", 7)], .value (.binary .mul (.var "y") (.int 6))⟩,
    ⟨"divided", [], [("zero", 0)], .value (.binary .div (.int 1) (.var "zero"))⟩,
    ⟨"overflowed", [], [("top", 9223372036854775807)],
      .value (.binary .add (.var "top") (.int 1))⟩,
    ⟨"excess", ["value", "limit"], [],
      .conditional .gt (.var "value") (.var "limit") (.binary .sub (.var "value") (.var "limit"))
        (.atom (.int 0))⟩,
    ⟨"surplus", [], [("x", 12)], .call "excess" [.var "x", .int 9]⟩]

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
(functions with integer parameters or one integer let, returning an integer value, a call of an
earlier function on atoms, or a conditional on a comparison of atoms); lowering the parsed
functions reproduces the raw-IR document's rows, payload words, operation pool, blocks and
function table exactly, with only the span words taken from the document; the artifact
validates; and the base machine runs of the seven recorded calls end with 5, 42, a
division-by-zero failure, an integer-overflow failure, 3 and 0 (`excess` on 12, 9 and on 3, 9)
and 3 (`surplus`, which calls `excess`) in the recorded order.
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
    verdictAfter (·.setPayloadWord 2 0 ⟨0, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 8 0 ⟨0, by decide⟩) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_wrong_arithmetic_operation_is_rejected :
    verdictAfter (·.setOp 0 .mul) = false ∧ verdictAfter (·.setOp 1 .add) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_mutated_ir_operand_is_rejected :
    verdictAfter (·.setPayloadWord 4 1 ⟨3, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 4 1 ⟨99, by decide⟩) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

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
      verdictAfter (·.setObservation 1 (.value ⟨43, by decide⟩)) = false ∧
      verdictAfter (·.setObservation 6 (.value ⟨9, by decide⟩)) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_wrong_failure_outcome_is_rejected :
    verdictAfter (·.setObservation 2 (.failure .integerOverflow)) = false ∧
      verdictAfter (·.setObservation 3 (.failure .divisionByZero)) = false ∧
      verdictAfter (·.setObservation 2 (.value ⟨0, by decide⟩)) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem the_certificate_fails_for_other_claims :
    certifiesExactly (expectedObservations.reverse) fixtureArtifact = false ∧
      certifiesExactly (⟨"shifted", [], .value 5⟩ :: ⟨"scaled", [], .value 41⟩ ::
        expectedObservations.drop 2) fixtureArtifact = false ∧
      certifiesExactly (expectedObservations.take 2) fixtureArtifact = false ∧
      certifiesExactly (expectedObservations.take 4 ++ [⟨"excess", [9, 12], .value 3⟩] ++
        expectedObservations.drop 5) fixtureArtifact = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

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

def RawArtifact.swapExtraWords (artifact : RawArtifact) (first second : Nat) : RawArtifact :=
  match artifact.extra[first]?, artifact.extra[second]? with
  | some one, some other =>
      { artifact with extra := (artifact.extra.set first other).set second one }
  | _, _ => artifact

set_option maxRecDepth 10000 in
/-- A conditional that takes the other branch is refused: the condition's operands exchanged
(`limit > value`), the then value replaced by the else value, the else value replaced by the
then value, and the comparison replaced by arithmetic, which is not a boolean. -/
theorem a_wrong_branch_is_rejected :
    verdictAfter (·.setPayloadWord 26 1 ⟨25, by decide⟩) = false ∧
      verdictAfter (·.setExtraWord 79 ⟨34, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 35 1 ⟨31, by decide⟩) = false ∧
      verdictAfter (·.setOp 3 .sub) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A call that binds its arguments to the wrong parameters is refused: the two argument rows
exchanged in the call's argument table, either row of that table repeated, the two parameter
reads exchanged in the callee, the recorded arguments exchanged, a callee with another parameter
count, and a call to the function itself instead of an earlier one. -/
theorem a_wrong_argument_binding_is_rejected :
    verdictAfter (·.swapExtraWords 102 104) = false ∧
      verdictAfter (·.setExtraWord 102 ⟨40, by decide⟩) = false ∧
      verdictAfter (·.setExtraWord 104 ⟨39, by decide⟩) = false ∧
      verdictAfter (fun artifact =>
        (artifact.setPayloadWord 24 0 ⟨1, by decide⟩).setPayloadWord 25 0 ⟨0, by decide⟩) =
        false ∧
      verdictAfter (·.setArguments 4 [⟨9, by decide⟩, ⟨12, by decide⟩]) = false ∧
      verdictAfter (·.mapFunction 4 fun function => { function with paramsLen := 1 }) = false ∧
      verdictAfter (·.setPayloadWord 41 0 ⟨6, by decide⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

def outcomesOf (artifact : RawArtifact) : Option (List FunctionOutcome) :=
  (namedResults artifact).map (·.map (·.outcome))

set_option maxRecDepth 10000 in
/-- The branch and binding mutations above are well formed and change what the IR means, so the
rejection is by the meaning and not by a structural check: with the then value replaced by the
else value every recorded `excess` call and `surplus` end in 0 or 3 unchanged, and with the two
arguments exchanged `surplus` ends in 0. -/
theorem the_rejected_branch_and_binding_mutations_mean_something_else :
    (validateArtifact (fixtureArtifact.setExtraWord 79 ⟨34, by decide⟩) = false ∧
      outcomesOf (fixtureArtifact.setExtraWord 79 ⟨34, by decide⟩) =
        some [.value 5, .value 42, .failure .divisionByZero, .failure .integerOverflow,
          .value 0, .value 0, .value 0]) ∧
    (validateArtifact (fixtureArtifact.swapExtraWords 102 104) = false ∧
      outcomesOf (fixtureArtifact.swapExtraWords 102 104) =
        some [.value 5, .value 42, .failure .divisionByZero, .failure .integerOverflow,
          .value 3, .value 0, .value 0]) := by
  refine ⟨⟨?_, ?_⟩, ⟨?_, ?_⟩⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Malformed blocks and tables are refused: a block named by two users, a block named by none,
a value block with two statements, and a recorded call with a missing argument. -/
theorem malformed_blocks_and_calls_are_rejected :
    verdictAfter (·.setPayloadWord 35 0 ⟨5, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 34 0 ⟨5, by decide⟩) = false ∧
      verdictAfter (·.mapBlock 4 fun block => { block with len := 3 }) = false ∧
      verdictAfter (·.setArguments 4 [⟨12, by decide⟩]) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- A changed source is not certified against the same IR: another literal, another operator, an
unbound name, a literal past the signed 64-bit range, another document identifier, the
comparison in the conditional dropped, and the call's arguments exchanged. -/
theorem a_substituted_source_is_rejected :
    certifyCodes (bridgeId! 1) (fixtureSourceTextCodes.set 49 52) fixtureArtifact = none ∧
      certifyCodes (bridgeId! 1) (fixtureSourceTextCodes.set 47 45) fixtureArtifact = none ∧
      certifyCodes (bridgeId! 1) (fixtureSourceTextCodes.set 45 113) fixtureArtifact = none ∧
      certifyCodes (bridgeId! 1) (fixtureSourceTextCodes.set 224 56) fixtureArtifact = none ∧
      certifyCodes (bridgeId! 2) fixtureSourceTextCodes fixtureArtifact = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- Source trees that differ from the parsed source fail the lowering check or the meaning
check against the same artifact: swapped operands, another operator, another function name,
a dropped binding, a changed literal, a call with exchanged arguments, and a conditional whose
branches are exchanged. -/
theorem a_wrong_lowering_is_rejected :
    lowersTo fixtureArtifact (fixtureFunctions.set 2
        ⟨"divided", [], [("zero", 0)], .value (.binary .div (.var "zero") (.int 1))⟩) = false ∧
      sourceAgrees fixtureArtifact (fixtureFunctions.set 2
        ⟨"divided", [], [("zero", 0)], .value (.binary .div (.var "zero") (.int 1))⟩) = false ∧
      lowersTo fixtureArtifact (fixtureFunctions.set 0
        ⟨"shifted", [], [("x", 2)], .value (.binary .sub (.var "x") (.int 3))⟩) = false ∧
      sourceAgrees fixtureArtifact (fixtureFunctions.set 0
        ⟨"shifted", [], [("x", 2)], .value (.binary .sub (.var "x") (.int 3))⟩) = false ∧
      lowersTo fixtureArtifact (fixtureFunctions.set 1
        ⟨"scaled", [], [], .value (.binary .mul (.int 7) (.int 6))⟩) = false ∧
      sourceAgrees fixtureArtifact (fixtureFunctions.set 1
        ⟨"scaled", [], [("y", 8)], .value (.binary .mul (.var "y") (.int 6))⟩) = false ∧
      sourceAgrees fixtureArtifact (fixtureFunctions.set 5
        ⟨"surplus", [], [("x", 12)], .call "excess" [.int 9, .var "x"]⟩) = false ∧
      lowersTo fixtureArtifact (fixtureFunctions.set 5
        ⟨"surplus", [], [("x", 12)], .call "excess" [.int 9, .var "x"]⟩) = false ∧
      sourceAgrees fixtureArtifact (fixtureFunctions.set 4
        ⟨"excess", ["value", "limit"], [], .conditional .gt (.var "value") (.var "limit")
          (.atom (.int 0)) (.binary .sub (.var "value") (.var "limit"))⟩) = false ∧
      lowersTo fixtureArtifact (fixtureFunctions.set 4
        ⟨"excess", ["limit", "value"], [], .conditional .gt (.var "value") (.var "limit")
          (.binary .sub (.var "value") (.var "limit")) (.atom (.int 0))⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
