import Xsh.Conformance.Vertical.Lowering
import Xsh.Conformance.Vertical.Fixture

namespace Xsh.Conformance.Vertical

def recordLocalFunctions : List SourceFunction :=
  [⟨"spans", ["value"], [], none,
    .recordLocal "box" [("low", .binary .sub (.var "value") (.int 3)),
      ("high", .binary .div (.int 60) (.var "value")),
      ("wide", .binary .add (.var "value") (.int 9223372036854775800))]
      (.binary .sub (.field "box" "high") (.field "box" "low"))⟩]

set_option maxRecDepth 100000 in
theorem record_local_text_parses :
    parseSourceCodes recordLocalTextCodes = some recordLocalFunctions := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_local_lowers_to_artifact :
    lowersTo recordLocalArtifact recordLocalFunctions = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem record_local_source_agrees :
    sourceAgrees recordLocalArtifact recordLocalFunctions = true := by
  decide +kernel

def changedWideRecordLocalFunctions : List SourceFunction :=
  [⟨"spans", ["value"], [], none,
    .recordLocal "box" [("low", .binary .sub (.var "value") (.int 3)),
      ("high", .binary .div (.int 60) (.var "value")),
      ("wide", .binary .add (.var "value") (.int 9223372036854775799))]
      (.binary .sub (.field "box" "high") (.field "box" "low"))⟩]

set_option maxRecDepth 100000 in
theorem changed_wide_constant_is_observationally_equal_on_recorded_calls :
    lowersTo recordLocalArtifact changedWideRecordLocalFunctions = false ∧
      sourceAgrees recordLocalArtifact changedWideRecordLocalFunctions = true := by
  constructor <;> decide +kernel

end Xsh.Conformance.Vertical
