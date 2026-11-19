import Xsh.Conformance.Vertical.Lowering
import Xsh.Conformance.Vertical.Fixture

namespace Xsh.Conformance.Vertical

def recordLocalExpectedObservations : List ObservedResult :=
  [⟨"spans", [5], .value 10⟩, ⟨"spans", [-4], .value (-8)⟩,
    ⟨"spans", [0], .failure .divisionByZero⟩, ⟨"spans", [9], .failure .integerOverflow⟩]

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

def changedLowRecordLocalFunctions : List SourceFunction :=
  [⟨"spans", ["value"], [], none,
    .recordLocal "box" [("low", .binary .sub (.var "value") (.int 4)),
      ("high", .binary .div (.int 60) (.var "value")),
      ("wide", .binary .add (.var "value") (.int 9223372036854775800))]
      (.binary .sub (.field "box" "high") (.field "box" "low"))⟩]

set_option maxRecDepth 100000 in
theorem changed_low_constant_disagrees_on_recorded_calls :
    lowersTo recordLocalArtifact changedLowRecordLocalFunctions = false ∧
      sourceAgrees recordLocalArtifact changedLowRecordLocalFunctions = false := by
  constructor <;> decide +kernel

def swappedLabelsRecordLocalFunctions : List SourceFunction :=
  [⟨"spans", ["value"], [], none,
    .recordLocal "box" [("high", .binary .sub (.var "value") (.int 3)),
      ("low", .binary .div (.int 60) (.var "value")),
      ("wide", .binary .add (.var "value") (.int 9223372036854775800))]
      (.binary .sub (.field "box" "high") (.field "box" "low"))⟩]

def changedFieldRecordLocalFunctions : List SourceFunction :=
  [⟨"spans", ["value"], [], none,
    .recordLocal "box" [("low", .binary .sub (.var "value") (.int 3)),
      ("high", .binary .div (.int 60) (.var "value")),
      ("wide", .binary .add (.var "value") (.int 9223372036854775800))]
      (.binary .sub (.field "box" "low") (.field "box" "low"))⟩]

set_option maxRecDepth 100000 in
theorem record_wrong_trees_are_refused :
    lowersTo recordLocalArtifact swappedLabelsRecordLocalFunctions = false ∧
      sourceAgrees recordLocalArtifact swappedLabelsRecordLocalFunctions = false ∧
      lowersTo recordLocalArtifact changedFieldRecordLocalFunctions = false ∧
      sourceAgrees recordLocalArtifact changedFieldRecordLocalFunctions = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
