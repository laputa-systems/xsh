import Xsh.Conformance.Connected.RawExpression
import Xsh.Conformance.Connected.RawRecordDocuments

namespace Xsh.Conformance.Connected
open Xsh.Conformance.Transport

private def recordRows : RawInstructionRows :=
  ⟨[⟨"ExprInt", 20⟩, ⟨"ExprRecord", 44⟩], [false, false], [false, false],
    [⟨0, 2⟩, ⟨2, 1⟩], [7, 0, 1, 1, 0, 1, 0], [], [], [], [],
    [⟨0, 5⟩], [119, 114, 111, 110, 103], [(1, [108, 111, 119])]⟩

private def recordBlocks : RawFunctionRows :=
  ⟨[⟨⟨3, 4⟩, 4294967295, 1, 0, (0, 0, 0)⟩], []⟩

theorem record_label_comes_from_labels :
    readRawValueTree recordRows recordBlocks 1 [] 10 1 =
      some (.record (.cons [108, 111, 119] (.integer 7) .nil)) := by
  decide +kernel

theorem assignment_field_comes_from_labels :
    decodeRawAssignSteps recordRows 1 [0, 1] = some ([.field [108, 111, 119]], []) ∧
    decodeRawAssignSteps recordRows 1 [0, 2] = none := by
  decide +kernel

def decodedRecordDocument : Option RawValueTree := do
  let rows ← (parseCodes recordRawInstructionCodes).bind decodeRawInstructionRows
  let functions ← (parseCodes recordRawFunctionCodes).bind decodeRawFunctionRows
  readRawValueTree rows functions 1 [] 16 4

set_option maxRecDepth 100000 in
theorem rust_record_document_decodes_label_spellings :
    decodedRecordDocument = some (.record (.cons (rawBytes "name") (.slot 0)
      (.cons (rawBytes "size") (.binary .add (.slot 0) (.integer 1)) .nil))) := by
  decide +kernel

end Xsh.Conformance.Connected
