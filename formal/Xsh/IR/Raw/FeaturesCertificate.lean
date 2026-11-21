import Xsh.IR.Raw.Certificate
import Xsh.IR.Raw.ExportedFeatures

/-!
The exported features document, which imports a module and has patterns with a text pattern,
a pipeline stage, prepared values and a `Use` driver step, is accepted, and documents that
differ from it in one recorded fact are rejected.
-/

namespace Xsh.IR.Raw

def mutatedFeatures (needle replacement : String) : Xsh.Conformance.Transport.ExchangeCodes :=
  mutate needle replacement featuresDocumentCodes

set_option maxRecDepth 10000 in
theorem features_document_is_accepted : verifyCodes featuresDocumentCodes = true := by
  decide +kernel

theorem features_document_is_well_formed : AcceptedDocument featuresDocumentCodes :=
  (verifyCodes_iff featuresDocumentCodes).mp features_document_is_accepted

set_option maxRecDepth 10000 in
/-- Pattern 4 (`Alternation`) lists patterns 2 and 3; listing itself first is a cycle. -/
theorem pattern_cycle_is_rejected :
    verifyCodes (mutatedFeatures "\"00000008\",\"00000001\",\"00000002\",\"00000002\",\"00000003\",\"00000003\""
      "\"00000008\",\"00000001\",\"00000002\",\"00000004\",\"00000003\",\"00000003\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- The `Use` step of the root program imports program 1; importing the root program is a cycle. -/
theorem driver_program_cycle_is_rejected :
    verifyCodes (mutatedFeatures "\"0000073b\",\"0000000a\",\"00000001\",\"00000002\""
      "\"0000073b\",\"0000000a\",\"00000002\",\"00000002\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- The text pattern's hole has kind 0 of six; kind 6 is outside the domain. -/
theorem text_hole_kind_out_of_domain_is_rejected :
    verifyCodes (mutatedFeatures "\"00000000\",\"00000001\",\"00000000\",\"00000005\""
      "\"00000000\",\"00000001\",\"00000006\",\"00000005\"") = false := by
  decide +kernel

end Xsh.IR.Raw
