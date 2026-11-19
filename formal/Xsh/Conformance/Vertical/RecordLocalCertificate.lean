import Xsh.Conformance.Vertical.Certificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.Environments

def recordLocalExpectedObservations : List ObservedResult :=
  [⟨"spans", [5], .value 10⟩, ⟨"spans", [-4], .value (-8)⟩,
    ⟨"spans", [0], .failure .divisionByZero⟩, ⟨"spans", [9], .failure .integerOverflow⟩]

set_option maxRecDepth 100000 in
theorem record_local_source_decodes :
    decodeSourceDocument recordLocalSourceDocumentCodes = some recordLocalSourceDocument := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_local_artifact_decodes :
    decodeRawDocument recordLocalRawIrDocumentCodes = some recordLocalArtifact := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_local_artifact_validates : validateArtifact recordLocalArtifact = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_local_artifact_results :
    namedResults recordLocalArtifact = some recordLocalExpectedObservations := by
  decide +kernel

theorem recordLocal_artifact_certified :
    certify recordLocalSourceDocumentCodes recordLocalRawIrDocumentCodes =
      some recordLocalExpectedObservations := by
  unfold certify
  rw [record_local_source_decodes, record_local_artifact_decodes]
  simp only [Option.bind_some]
  have same : recordLocalArtifact.sourceId = recordLocalSourceDocument.id := rfl
  simp [same, record_local_artifact_validates, record_local_artifact_results]

def recordLocalVerdict (mutate : RawArtifact → RawArtifact) : Bool :=
  certifiesExactly recordLocalExpectedObservations (mutate recordLocalArtifact)

set_option maxRecDepth 100000 in
theorem record_swapped_names_rejected :
    recordLocalVerdict (fun a => { a with names := ["high", "low", "wide"] }) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_wrong_field_rejected :
    recordLocalVerdict (·.setExtraWord 37 5) = false := by
  decide +kernel

def dropWide (a : RawArtifact) : RawArtifact :=
  { (a.setExtraWord 21 2) with blocks := a.blocks.map fun b =>
      if b.id.raw.val == 1 then { b with len := 7 } else b }

set_option maxRecDepth 100000 in
theorem record_dropped_entry_rejected : recordLocalVerdict dropWide = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_parameter_base_rejected :
    recordLocalVerdict (·.setExtraWord 35 0) = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_wrong_binding_rejected :
    recordLocalVerdict (·.setExtraWord 32 3) = false := by
  decide +kernel

theorem record_parameter_base_faults :
    runFuel 1 (recordMachine 2 (typedRecord []) (.fieldRead (NameId.mk 0) "high")
      [⟨0, .integerLiteral 5⟩]) = .completed (.fault .expectedRecord) := by
  rfl

theorem record_wrong_binding_faults :
    runFuel 1 (recordMachine 3 (typedRecord []) (.fieldRead (NameId.mk 2) "high")
      [⟨0, .integerLiteral 5⟩]) = .completed (.fault (.unboundName (NameId.mk 2))) := by
  rfl

set_option maxRecDepth 100000 in
theorem record_malformed_entries_rejected :
    recordLocalVerdict (·.setExtraWord 22 1) = false ∧
    recordLocalVerdict (·.setExtraWord 21 4) = false ∧
    recordLocalVerdict (·.setExtraWord 23 0) = false ∧
    recordLocalVerdict (·.setExtraWord 23 4) = false ∧
    recordLocalVerdict (fun a => { a with names := ["low", "low", "wide"] }) = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
theorem record_wrong_outcomes_rejected :
    recordLocalVerdict (·.setObservation 0 (.value 11)) = false ∧
    recordLocalVerdict (·.setObservation 2 (.failure .integerOverflow)) = false ∧
    recordLocalVerdict (·.setObservation 3 (.failure .divisionByZero)) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 100000 in
theorem record_wrong_field_has_different_meaning :
    validateArtifact (((recordLocalArtifact.setExtraWord 37 5).setObservation 0 (.value 0)).setObservation 1 (.value 0)) = true ∧
    namedResults (recordLocalArtifact.setExtraWord 37 5) = some
      [⟨"spans", [5], .value 0⟩, ⟨"spans", [-4], .value 0⟩,
       ⟨"spans", [0], .failure .divisionByZero⟩, ⟨"spans", [9], .failure .integerOverflow⟩] := by
  constructor <;> decide +kernel

set_option maxRecDepth 100000 in
theorem record_dropped_entry_has_different_meaning :
    validateArtifact ((dropWide recordLocalArtifact).setObservation 3 (.value 0)) = true ∧
    namedResults (dropWide recordLocalArtifact) = some
      [⟨"spans", [5], .value 10⟩, ⟨"spans", [-4], .value (-8)⟩,
       ⟨"spans", [0], .failure .divisionByZero⟩, ⟨"spans", [9], .value 0⟩] := by
  constructor <;> decide +kernel

set_option maxRecDepth 100000 in
theorem record_swapped_names_has_different_meaning :
    (namedResults { recordLocalArtifact with names := ["high", "low", "wide"] }).map List.head? =
      some (some ⟨"spans", [5], .value (-10)⟩) := by
  decide +kernel

end Xsh.Conformance.Vertical
