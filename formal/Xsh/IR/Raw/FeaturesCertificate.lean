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
    verifyCodes (mutatedFeatures "\"00000007\",\"00000001\",\"00000002\",\"00000002\",\"00000003\",\"00000003\""
      "\"00000007\",\"00000001\",\"00000002\",\"00000004\",\"00000003\",\"00000003\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- The `Use` step of the root program imports program 1; importing the root program is a cycle. -/
theorem driver_program_cycle_is_rejected :
    verifyCodes (mutatedFeatures "\"0000073b\",\"0000000a\",\"00000001\",\"00000007\""
      "\"0000073b\",\"0000000a\",\"00000002\",\"00000007\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- The text pattern's hole has kind 0 of six; kind 6 is outside the domain. -/
theorem text_hole_kind_out_of_domain_is_rejected :
    verifyCodes (mutatedFeatures "\"00000000\",\"00000001\",\"00000000\",\"00000004\""
      "\"00000000\",\"00000001\",\"00000006\",\"00000004\"") = false := by
  decide +kernel

/-- Patterns 3 (`Literal`) and 4 (`Alternation`) trade rows, and the match arm and the alternation's
list follow them, so every pattern is still read once and only the alternation now names a
pattern above its own index. -/
def swappedPatternCodes : Xsh.Conformance.Transport.ExchangeCodes :=
  mutate "\"00000003\",\"00000004\",\"00000000\"" "\"00000003\",\"00000003\",\"00000000\""
    (mutate "\"00000002\",\"00000003\",\"00000003\"" "\"00000002\",\"00000004\",\"00000003\""
      (mutate "[\"00000011\",\"00000001\"],[\"00000015\",\"00000001\"]" "[\"00000015\",\"00000001\"],[\"00000011\",\"00000001\"]"
        (mutatedFeatures "{\"name\":\"Literal\",\"ordinal\":\"0000000b\"},{\"name\":\"Alternation\",\"ordinal\":\"00000001\"}" "{\"name\":\"Alternation\",\"ordinal\":\"00000001\"},{\"name\":\"Literal\",\"ordinal\":\"0000000b\"}")))

set_option maxRecDepth 10000 in
theorem pattern_descent_is_rejected : verifyCodes swappedPatternCodes = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem reserved_driver_slot_byte_is_rejected :
    verifyCodes (mutatedFeatures "\"driverSlots\":[[\"0000000d\",\"00000003\",\"00000000\",\"00000001\",\"00000001\",\"00000000\""
      "\"driverSlots\":[[\"0000000d\",\"00000003\",\"00000000\",\"00000001\",\"00000001\",\"00000001\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Flag word 33 keeps the read bit and adds bit 5, which no flag defines. -/
theorem undefined_driver_slot_flag_is_rejected :
    verifyCodes (mutatedFeatures "\"driverSlots\":[[\"0000000d\",\"00000003\",\"00000000\",\"00000001\",\"00000001\""
      "\"driverSlots\":[[\"0000000d\",\"00000003\",\"00000000\",\"00000001\",\"00000021\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- The last step has three slots; slot 999 is past them. -/
theorem driver_slot_past_the_step_is_rejected :
    verifyCodes (mutatedFeatures "\"driverSlots\":[[\"0000000d\",\"00000003\",\"00000000\""
      "\"driverSlots\":[[\"0000000d\",\"00000003\",\"000003e7\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- The last step takes slot rows 0 to 2; taking only two leaves row 2 without an owner. -/
theorem unowned_driver_slot_is_rejected :
    verifyCodes (mutatedFeatures "\"0000007c\",\"00000001\",\"00000000\",\"00000003\""
      "\"0000007c\",\"00000001\",\"00000000\",\"00000002\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- There are two binding identities; writer identity 2 is past them. -/
theorem driver_writer_identity_out_of_range_is_rejected :
    verifyCodes (mutatedFeatures "\"driverWriters\":[[\"00000001\""
      "\"driverWriters\":[[\"00000002\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Step 2 also takes writer row 0, which the `Use` step owns. -/
theorem doubly_owned_driver_writer_is_rejected :
    verifyCodes (mutatedFeatures "\"00000045\",\"00000000\",\"00000000\",\"00000000\",\"00000001\",\"00000000\""
      "\"00000045\",\"00000000\",\"00000000\",\"00000000\",\"00000000\",\"00000001\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- The region's one shared slot is only read; a sync row that also writes is not exact. -/
theorem inexact_sync_flags_are_rejected :
    verifyCodes (mutatedFeatures "\"driverSync\":[[\"0000000d\",\"00000003\",\"00000001\""
      "\"driverSync\":[[\"0000000d\",\"00000003\",\"00000003\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem reserved_sync_byte_is_rejected :
    verifyCodes (mutatedFeatures "\"driverSync\":[[\"0000000d\",\"00000003\",\"00000001\",\"00000000\""
      "\"driverSync\":[[\"0000000d\",\"00000003\",\"00000001\",\"00000001\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Region 2 holds step 2; emptying it leaves that step outside every region. -/
theorem empty_driver_region_is_rejected :
    verifyCodes (mutatedFeatures "[\"00000002\",\"00000001\",\"00000000\",\"00000000\",\"00000000\"]"
      "[\"00000002\",\"00000000\",\"00000000\",\"00000000\",\"00000000\"]") = false := by
  decide +kernel

end Xsh.IR.Raw
