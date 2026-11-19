import Xsh.Conformance.Vertical.RecordLocalLowering
import Xsh.Conformance.Vertical.Documents
import Xsh.Conformance.Vertical.SourceAdmissionSound
import Xsh.Conformance.Vertical.RecordLocalAdmission

namespace Xsh.Conformance.Vertical

set_option maxRecDepth 100000 in
theorem record_pair_source_decodes :
    decodeSourceDocument recordLocalSourceDocumentCodes = some recordLocalSourceDocument := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_pair_raw_decodes :
    decodeRawDocument recordLocalRawIrDocumentCodes = some recordLocalArtifact := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_pair_source_parses :
    parseSource recordLocalSourceDocument.text = some recordLocalFunctions := by
  have codes : (recordLocalTextCodes.map Char.ofNat).map Char.toNat = recordLocalTextCodes := by
    decide +kernel
  simp only [recordLocalSourceDocument, parseSource, String.toList_ofList, codes,
    record_local_text_parses]

theorem record_pair_lowers : lowersTo recordLocalArtifact recordLocalFunctions = true :=
  record_local_lowers_to_artifact

theorem record_pair_source_agrees : sourceAgrees recordLocalArtifact recordLocalFunctions = true :=
  record_local_source_agrees

theorem record_pair_means_source :
    functionPrograms recordLocalArtifact = sourcePrograms recordLocalFunctions :=
  lowers_to_means_the_source record_pair_lowers

theorem record_pair_admitted_meaning :
    SourceAdmission.AdmittedSource
      ((SourceAdmission.witnessBytes recordLocalSourceDocument.text).map UInt8.toNat)
      recordLocalFunctions ∧
    SourceAdmission.ScopesProjection recordLocalFunctions [[("value", 0), ("box", 2)]] ∧
    functionPrograms recordLocalArtifact = sourcePrograms recordLocalFunctions := by
  have proved := SourceAdmission.admitted_lowering_means_source
    SourceAdmission.record_bytes_admitted record_pair_lowers
  exact ⟨proved.2.1, proved.2.2.1, proved.2.2.2⟩

theorem record_pair_wrong_trees_refused :
    lowersTo recordLocalArtifact swappedLabelsRecordLocalFunctions = false ∧
      sourceAgrees recordLocalArtifact swappedLabelsRecordLocalFunctions = false ∧
      lowersTo recordLocalArtifact changedFieldRecordLocalFunctions = false ∧
      sourceAgrees recordLocalArtifact changedFieldRecordLocalFunctions = false :=
  record_wrong_trees_are_refused

theorem record_pair_changed_low_refused :
    lowersTo recordLocalArtifact changedLowRecordLocalFunctions = false ∧
      sourceAgrees recordLocalArtifact changedLowRecordLocalFunctions = false :=
  changed_low_constant_disagrees_on_recorded_calls

set_option maxRecDepth 100000 in
theorem record_pair_validates : validateArtifact recordLocalArtifact = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_pair_results :
    namedResults recordLocalArtifact = some recordLocalExpectedObservations := by
  decide +kernel

theorem record_local_pair_certified :
    certifyFromSource recordLocalSourceDocumentCodes recordLocalRawIrDocumentCodes = some recordLocalExpectedObservations := by
  rw [certify_from_source_of_decodes record_pair_source_decodes record_pair_raw_decodes]
  rw [certify_decoded_of_checks (by decide) record_pair_source_parses
    record_local_lowers_to_artifact record_pair_validates]
  exact record_pair_results

def reorderedRecordLocalFunctions : List SourceFunction :=
  [⟨"spans", ["value"], [], none,
    .recordLocal "box" [("high", .binary .div (.int 60) (.var "value")),
      ("low", .binary .sub (.var "value") (.int 3)),
      ("wide", .binary .add (.var "value") (.int 9223372036854775800))]
      (.binary .sub (.field "box" "high") (.field "box" "low"))⟩]

set_option maxRecDepth 100000 in
theorem reordered_record_entries_refused_by_lowering :
    lowersTo recordLocalArtifact reorderedRecordLocalFunctions = false ∧
      sourceAgrees recordLocalArtifact reorderedRecordLocalFunctions = true := by
  constructor <;> decide +kernel

def changedWideSourceText : String :=
  "pure spans(value: Int) -> Int {\n" ++
  "  let box = {low: value - 3, high: 60 / value, wide: value + 9223372036854775799}\n" ++
  "  return box.high - box.low\n}\n"

set_option maxRecDepth 100000 in
theorem changed_wide_source_rejected_by_pair :
    parseSource changedWideSourceText = some changedWideRecordLocalFunctions ∧
      sourceAgrees recordLocalArtifact changedWideRecordLocalFunctions = true ∧
      certifyDecoded { recordLocalSourceDocument with text := changedWideSourceText }
        recordLocalArtifact = none := by
  refine ⟨?_, changed_wide_constant_is_observationally_equal_on_recorded_calls.2, ?_⟩ <;>
    decide +kernel

def changedLowSourceText : String :=
  "pure spans(value: Int) -> Int {\n" ++
  "  let box = {low: value - 4, high: 60 / value, wide: value + 9223372036854775800}\n" ++
  "  return box.high - box.low\n}\n"

def renamedLabelSourceText : String :=
  "pure spans(value: Int) -> Int {\n" ++
  "  let box = {tiny: value - 3, high: 60 / value, wide: value + 9223372036854775800}\n" ++
  "  return box.high - box.tiny\n}\n"

set_option maxRecDepth 100000 in
theorem substituted_record_sources_are_refused :
    certifyDecoded { recordLocalSourceDocument with text := changedLowSourceText }
        recordLocalArtifact = none ∧
      certifyDecoded { recordLocalSourceDocument with text := renamedLabelSourceText }
        recordLocalArtifact = none := by
  constructor <;> decide +kernel

end Xsh.Conformance.Vertical
