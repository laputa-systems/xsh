import Xsh.IR.Raw.Agreement
import Xsh.IR.Raw.Exported

/-!
The exported points document is accepted, and documents that differ from it in one recorded
fact are rejected. Each theorem evaluates the whole document in the kernel.
-/

namespace Xsh.IR.Raw

open Xsh.Conformance.Transport

/-- Replace the first occurrence of `needle` in a code list. -/
def replaceCodes (needle replacement : List Nat) : List Nat → List Nat
  | [] => []
  | code :: rest =>
      if needle.isPrefixOf (code :: rest) then replacement ++ (code :: rest).drop needle.length
      else code :: replaceCodes needle replacement rest

def mutate (needle replacement : String) (codes : ExchangeCodes) : ExchangeCodes :=
  replaceCodes (codesOf needle) (codesOf replacement) codes

def mutatedExport (needle replacement : String) : ExchangeCodes := mutate needle replacement exportedDocumentCodes

set_option maxRecDepth 10000 in
theorem exported_document_is_accepted : verifyCodes exportedDocumentCodes = true := by
  decide +kernel

theorem exported_document_is_well_formed : AcceptedDocument exportedDocumentCodes :=
  (verifyCodes_iff exportedDocumentCodes).mp exported_document_is_accepted

set_option maxRecDepth 10000 in
theorem unknown_instruction_ordinal_is_rejected :
    verifyCodes (mutatedExport "\"name\":\"ExprParam\",\"ordinal\":\"0000001f\""
      "\"name\":\"ExprParam\",\"ordinal\":\"000000ff\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem reserved_instruction_ordinal_is_rejected :
    verifyCodes (mutatedExport "\"name\":\"ExprParam\",\"ordinal\":\"0000001f\""
      "\"name\":\"ExprParam\",\"ordinal\":\"00000020\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem misnamed_instruction_tag_is_rejected :
    verifyCodes (mutatedExport "\"name\":\"ExprParam\",\"ordinal\":\"0000001f\""
      "\"name\":\"ExprField\",\"ordinal\":\"0000001f\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem duplicate_field_is_rejected :
    verifyCodes (mutatedExport "\"driverRoot\":" "\"driverRoot\":\"00000001\",\"driverRoot\":") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem short_hex_word_is_rejected :
    verifyCodes (mutatedExport "\"sourceId\":\"00000000\"" "\"sourceId\":\"0000000\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem short_row_is_rejected :
    verifyCodes (mutatedExport "\"data\":[[\"00000000\",\"00000001\"]" "\"data\":[[\"00000000\"]") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem payload_past_the_word_table_is_rejected :
    verifyCodes (mutatedExport "\"data\":[[\"00000000\",\"00000001\"]"
      "\"data\":[[\"00000000\",\"000000ff\"]") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem misaligned_column_is_rejected :
    verifyCodes (mutatedExport ",\"00000000\"],\"checkedCallTargetRequired\"" "],\"checkedCallTargetRequired\"") =
      false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- A start of `u32::MAX` and a length of two: in 32-bit arithmetic the end would wrap to one. -/
theorem overflowing_payload_range_is_rejected :
    verifyCodes (mutatedExport "\"data\":[[\"00000000\",\"00000001\"]"
      "\"data\":[[\"ffffffff\",\"00000002\"]") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem out_of_range_driver_root_is_rejected :
    verifyCodes (mutatedExport "\"driverRoot\":\"00000001\"" "\"driverRoot\":\"00000009\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Instruction 1 (`ExprField`) reads string 5; string zero names nothing. -/
theorem zero_string_id_is_rejected :
    verifyCodes (mutatedExport "\"extra\":[\"00000000\",\"00000000\",\"00000005\""
      "\"extra\":[\"00000000\",\"00000000\",\"00000000\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem absent_string_id_is_rejected :
    verifyCodes (mutatedExport "\"extra\":[\"00000000\",\"00000000\",\"00000005\""
      "\"extra\":[\"00000000\",\"00000000\",\"ffffffff\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Instruction 0 (`ExprParam`) reads slot 0 of a function with three slots. -/
theorem slot_past_the_owner_is_rejected :
    verifyCodes (mutatedExport "\"extra\":[\"00000000\"" "\"extra\":[\"00000003\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem invalid_utf8_string_is_rejected :
    verifyCodes (mutatedExport "\"stringBytes\":\"74" "\"stringBytes\":\"ff") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- There are eighteen binary operators; operator word 18 is outside the pool's domain. -/
theorem out_of_domain_operator_is_rejected :
    verifyCodes (mutatedExport "\"binaryOps\":[\"00000005\"" "\"binaryOps\":[\"00000012\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Instruction 3 (`ExprBinary`) reads instructions 1 and 2; reading instruction 1 twice takes it twice. -/
theorem doubly_claimed_operand_is_rejected :
    verifyCodes (mutatedExport
      "\"extra\":[\"00000000\",\"00000000\",\"00000005\",\"00000002\",\"00000000\",\"00000000\",\"00000000\",\"00000001\",\"00000002\""
      "\"extra\":[\"00000000\",\"00000000\",\"00000005\",\"00000002\",\"00000000\",\"00000000\",\"00000000\",\"00000001\",\"00000001\"") =
      false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Block 1 holds statements for an `ExprValueBlock`; marking it a list block is rejected. -/
theorem block_kind_mismatch_is_rejected :
    verifyCodes (mutatedExport "\"blocks\":[[\"00000015\",\"00000002\",\"ffffffff\",\"00000001\",\"00000001\""
      "\"blocks\":[[\"00000015\",\"00000002\",\"ffffffff\",\"00000001\",\"00000000\"") = false := by
  decide +kernel

/-- Instructions 0 (`ExprParam`) and 1 (`ExprField`) trade rows, and the two operand words that
named them follow, so only the order of operand before user changes. -/
def swappedOperandCodes : ExchangeCodes :=
  mutate "\"extra\":[\"00000000\",\"00000000\",\"00000005\",\"00000002\",\"00000000\",\"00000000\",\"00000000\",\"00000001\""
    "\"extra\":[\"00000000\",\"00000001\",\"00000005\",\"00000002\",\"00000000\",\"00000000\",\"00000000\",\"00000000\""
    (mutate "\"data\":[[\"00000000\",\"00000001\"],[\"00000001\",\"00000003\"]"
      "\"data\":[[\"00000001\",\"00000003\"],[\"00000000\",\"00000001\"]"
      (mutatedExport
        "\"tags\":[{\"name\":\"ExprParam\",\"ordinal\":\"0000001f\"},{\"name\":\"ExprField\",\"ordinal\":\"00000038\"}"
        "\"tags\":[{\"name\":\"ExprField\",\"ordinal\":\"00000038\"},{\"name\":\"ExprParam\",\"ordinal\":\"0000001f\"}"))

set_option maxRecDepth 10000 in
theorem forward_operand_is_rejected : verifyCodes swappedOperandCodes = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem reserved_function_metadata_is_rejected :
    verifyCodes (mutatedExport "\"functionMetadata\":[[\"ffffffff\",\"00000000\",\"00000000\",\"00000000\""
      "\"functionMetadata\":[[\"ffffffff\",\"00000000\",\"00000000\",\"00000001\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- The function's capture names slot 1 of three; slot 99 is past them. -/
theorem capture_slot_past_the_function_is_rejected :
    verifyCodes (mutatedExport "\"captures\":[[\"00000004\",\"00000004\",\"00000001\""
      "\"captures\":[[\"00000004\",\"00000004\",\"00000063\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem undefined_parameter_flag_is_rejected :
    verifyCodes (mutatedExport "\"params\":[[\"00000003\",\"00000000\"" "\"params\":[[\"00000003\",\"00000008\"") =
      false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem absent_driver_location_is_rejected :
    verifyCodes (mutatedExport
      "\"driverSteps\":[[\"00000038\",\"00000000\",\"00000000\",\"00000000\",\"00000000\",\"00000000\",\"00000015\",\"00000000\",\"0000000c\""
      "\"driverSteps\":[[\"00000038\",\"00000000\",\"00000000\",\"00000000\",\"00000000\",\"00000000\",\"00000015\",\"00000000\",\"ffffffff\"") =
      false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Instruction 0 is an `ExprParam`, which transfers no resource. -/
theorem resource_transfer_on_ordinary_instruction_is_rejected :
    verifyCodes (mutatedExport "\"resourceTransfers\":[\"00000000\"" "\"resourceTransfers\":[\"00000001\"") = false := by
  decide +kernel

/-- An `ExprNull` appended to the last driver step's instructions fits, but nothing reads it. -/
def orphanCodes : ExchangeCodes :=
  mutate "\"00000000\"],\"extra\"" "\"00000000\",\"00000000\"],\"extra\""
    (mutate "\"00000000\"],\"checkedCallTargetRequired\"" "\"00000000\",\"00000000\"],\"checkedCallTargetRequired\""
      (mutate "[\"00000058\",\"00000005\"]]" "[\"00000058\",\"00000005\"],[\"00000000\",\"00000000\"]]"
        (mutatedExport "{\"name\":\"StmtPrint\",\"ordinal\":\"00000090\"}]"
          "{\"name\":\"StmtPrint\",\"ordinal\":\"00000090\"},{\"name\":\"ExprNull\",\"ordinal\":\"00000012\"}]")))

set_option maxRecDepth 10000 in
theorem orphan_instruction_is_rejected : verifyCodes orphanCodes = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- Block 1 is read by the function; the owner word of driver step 3 claims it for another owner. -/
theorem block_of_another_owner_is_rejected :
    verifyCodes (mutatedExport "\"blocks\":[[\"00000015\",\"00000002\",\"ffffffff\",\"00000001\""
      "\"blocks\":[[\"00000015\",\"00000002\",\"ffffffff\",\"80000003\"") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem missing_scope_metadata_is_rejected :
    verifyCodes (mutatedExport "\"scope\":\"indexed-store-structure-v1\"," "") = false := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem other_schema_version_is_rejected :
    verifyCodes (mutatedExport "xsh-raw-ir-v1" "xsh-raw-ir-v2") = false := by
  decide +kernel

end Xsh.IR.Raw
