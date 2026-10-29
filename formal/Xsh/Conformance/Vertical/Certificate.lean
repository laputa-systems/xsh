import Xsh.Conformance.Vertical.Meaning
import Xsh.Conformance.Vertical.Documents
import Xsh.Conformance.Vertical.Fixture

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

/-- The ordered observations the certificate proves: each function the raw-IR document
records, with the integer its IR meaning completes with. -/
def expectedObservations : List (String × Int) := [("shifted", 5), ("scaled", 42)]

set_option maxRecDepth 10000 in
/-- ARTIFACT_CERTIFIED. Artifact kind: source document and raw-IR document pair, both in the
closed v1 exchange, exactly the bytes of `sourceDocumentCodes` and `rawIrDocumentCodes`.
Proposition: the source document decodes, the raw-IR document decodes under the
integer-arithmetic fixture scope and names that source document, the artifact validates, and
the base machine runs of its two function bodies complete with 5 and 42 in the recorded order.
This starts at the supplied IR rows: the source bytes are associated by identifier only and are
not parsed, and nothing here shows that Rust lowers or executes this IR. -/
theorem base_vertical_artifact_certified :
    certify sourceDocumentCodes rawIrDocumentCodes = some expectedObservations := by
  decide +kernel

def certifiesExactly (expected : List (String × Int)) (artifact : RawArtifact) : Bool :=
  validateArtifact artifact && namedResults artifact == some expected

set_option maxRecDepth 10000 in
theorem fixture_is_the_decoded_raw_ir_document :
    decodeRawDocument rawIrDocumentCodes = some fixtureArtifact := by
  decide +kernel

theorem fixture_is_the_decoded_source_document :
    decodeSourceDocument sourceDocumentCodes = some fixtureSourceDocument := by
  decide +kernel

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
  decide

set_option maxRecDepth 10000 in
theorem a_wrong_binding_id_is_rejected :
    verdictAfter (·.setPayloadWord 2 0 ⟨0, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 2 0 ⟨9, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 8 0 ⟨0, by decide⟩) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide

set_option maxRecDepth 10000 in
theorem a_wrong_arithmetic_operation_is_rejected :
    verdictAfter (·.setOp 0 .mul) = false ∧ verdictAfter (·.setOp 0 .sub) = false ∧
      verdictAfter (·.setOp 1 .add) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide

set_option maxRecDepth 10000 in
theorem a_mutated_ir_operand_is_rejected :
    verdictAfter (·.setPayloadWord 4 1 ⟨3, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 4 2 ⟨2, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 4 1 ⟨99, by decide⟩) = false ∧
      verdictAfter (·.setPayloadWord 4 1 ⟨4, by decide⟩) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide

set_option maxRecDepth 10000 in
theorem a_missing_or_reordered_observation_is_rejected :
    verdictAfter (fun artifact => { artifact with observations := artifact.observations.drop 1 }) =
        some false ∧
      verdictAfter (fun artifact =>
        { artifact with observations := artifact.observations.take 1 }) = false ∧
      verdictAfter (fun artifact =>
        { artifact with observations := artifact.observations.reverse }) = false ∧
      verdictAfter (fun artifact =>
        { artifact with observations := artifact.observations ++ artifact.observations }) =
        some false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide

def RawArtifact.recordingEveryResultAs (artifact : RawArtifact) (bits : Word64) :
    RawArtifact :=
  { artifact with observations := artifact.observations.map fun observation =>
    { observation with result := bits } }

set_option maxRecDepth 10000 in
theorem a_wrong_recorded_result_is_rejected :
    verdictAfter (·.recordingEveryResultAs ⟨6, by decide⟩) = false := by
  decide

set_option maxRecDepth 10000 in
theorem the_certificate_fails_for_other_claims :
    certifiesExactly [("scaled", 42), ("shifted", 5)] fixtureArtifact = false ∧
      certifiesExactly [("shifted", 5), ("scaled", 41)] fixtureArtifact = false ∧
      certifiesExactly [("shifted", 5)] fixtureArtifact = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide

end Xsh.Conformance.Vertical
