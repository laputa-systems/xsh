import Xsh.Conformance.Vertical.RecordLocalLowering
import Xsh.Conformance.Vertical.Documents

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

end Xsh.Conformance.Vertical
