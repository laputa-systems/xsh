import Xsh.Conformance.Connected.RawResultFixture

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def validationAccepts (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep) : Bool :=
  match admitSource 0 resultControlSource, signatures[index]? with
  | .ok unit, some signature =>
      match validateResultFunction unit signature index candidate steps with | .ok _ => true | .error _ => false
  | _, _ => false

def rowsAccept (rows : RawInstructionRows) (functions : RawFunctionRows) (index : Nat)
    (candidate : List StatementNode) : Bool :=
  match resultSignatures[index]? with
  | some signature => rawResultReturnCheckFor resultSourceNames rows functions signature index candidate
  | none => false

set_option maxRecDepth 100000 in
theorem source_validation_accepts_each_candidate :
    validationAccepts resultSignatures 0 throughCandidate throughSteps = true ∧
    validationAccepts resultSignatures 1 refuseCandidate refuseSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem exported_rows_accept_each_candidate :
    rowsAccept resultRows resultFunctions 0 throughCandidate = true ∧
    rowsAccept resultRows resultFunctions 1 refuseCandidate = true := by
  decide +kernel

def editSignature (index : Nat) (change : ResultSignature → ResultSignature) : List ResultSignature :=
  resultSignatures.mapIdx fun position signature => if position = index then change signature else signature

def rejectsSignature (index : Nat) (change : ResultSignature → ResultSignature)
    (candidate : List StatementNode) (steps : List DerivationStep) : Bool :=
  !validationAccepts (editSignature index change) index candidate steps

def integerErrorResult : MonoType := .result (.primitive .integer) (.primitive .integer)

set_option maxRecDepth 100000 in
theorem a_forged_result_type_is_rejected :
    rejectsSignature 0 (fun s => { s with result := integerErrorResult }) throughCandidate throughSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_forged_parameter_type_is_rejected :
    rejectsSignature 0 (fun s => { s with parameters := [("r", .primitive .integer)] })
      throughCandidate throughSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_forged_error_type_of_the_failure_is_rejected :
    rejectsSignature 1 (fun s => { s with result := .result (.primitive .string) (.primitive .integer) })
      refuseCandidate refuseSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_forged_body_site_is_rejected :
    rejectsSignature 0 (fun s => { s with body := ⟨0, 0, 1⟩ }) throughCandidate throughSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_swapped_function_name_is_rejected :
    rejectsSignature 0 (fun s => { s with name := "refuse" }) throughCandidate throughSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem err_where_the_source_builds_ok_is_rejected :
    validationAccepts resultSignatures 0
      [.returned (errorNode (.propagate (.read resultFirstBinder []) (.primitive .string)))] throughSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem ok_where_the_source_builds_err_is_rejected :
    validationAccepts resultSignatures 1 [.returned (okNode (.string "no"))] refuseSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_dropped_propagation_is_rejected :
    validationAccepts resultSignatures 0 [.returned (okNode (.read resultFirstBinder []))] throughSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_propagation_with_the_wrong_error_type_is_rejected :
    validationAccepts resultSignatures 0
      [.returned (okNode (.propagate (.read resultFirstBinder []) (.primitive .integer)))] throughSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_propagation_outside_the_constructor_is_rejected :
    validationAccepts resultSignatures 0
      [.returned (.propagate (okNode (.read resultFirstBinder [])) (.primitive .string))] throughSteps = false := by
  decide +kernel

def typesAccept (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep) : Bool :=
  match admitSource 0 resultControlSource, signatures[index]? with
  | .ok unit, some signature => checkResultExpressionTypes unit signature index candidate steps
  | _, _ => false

set_option maxRecDepth 100000 in
theorem published_expression_types_match_each_candidate :
    typesAccept resultSignatures 0 throughCandidate throughSteps = true ∧
    typesAccept resultSignatures 1 refuseCandidate refuseSteps = true := by
  decide +kernel

def withExpressions (index : Nat) (expressions : List (SourceSite × MonoType)) : List ResultSignature :=
  editSignature index fun s => { s with expressions := expressions }

def stringResult : MonoType := .result (.primitive .integer) (.primitive .string)

set_option maxRecDepth 100000 in
theorem a_forged_propagated_value_type_is_rejected :
    typesAccept (withExpressions 0 [(⟨0, 63, 6⟩, stringResult), (⟨0, 66, 2⟩, .primitive .string),
      (⟨0, 66, 1⟩, stringResult)]) 0 throughCandidate throughSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_forged_constructor_type_is_rejected :
    typesAccept (withExpressions 1 [(⟨0, 115, 9⟩, integerErrorResult), (⟨0, 119, 4⟩, .primitive .string)])
      1 refuseCandidate refuseSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_shifted_expression_site_is_rejected :
    typesAccept (withExpressions 0 [(⟨0, 63, 6⟩, stringResult), (⟨0, 67, 2⟩, .primitive .integer),
      (⟨0, 66, 1⟩, stringResult)]) 0 throughCandidate throughSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_dropped_expression_type_is_rejected :
    typesAccept (withExpressions 0 [(⟨0, 63, 6⟩, stringResult), (⟨0, 66, 1⟩, stringResult)])
      0 throughCandidate throughSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem an_extra_expression_type_is_rejected :
    typesAccept (withExpressions 1 [(⟨0, 115, 9⟩, stringResult), (⟨0, 119, 4⟩, .primitive .string),
      (⟨0, 119, 4⟩, .primitive .string)]) 1 refuseCandidate refuseSteps = false := by
  decide +kernel

def withTag (rows : RawInstructionRows) (row : Nat) (tag : RawTag) : RawInstructionRows :=
  { rows with tags := rows.tags.set row tag }

def withWord (rows : RawInstructionRows) (position value : Nat) : RawInstructionRows :=
  { rows with extra := rows.extra.set position value }

def withRetaggedPayload (rows : RawInstructionRows) (row : Nat) (tag : RawTag) (payload : List Nat) :
    RawInstructionRows :=
  { rows with
      tags := rows.tags.set row tag
      data := rows.data.set row ⟨rows.extra.length, payload.length⟩
      extra := rows.extra ++ payload }

set_option maxRecDepth 100000 in
theorem an_err_row_where_the_candidate_builds_ok_is_rejected :
    rowsAccept (withRetaggedPayload resultRows 2 ⟨"ExprErr", 105⟩ [1, 0]) resultFunctions 0 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem an_ok_row_where_the_candidate_builds_err_is_rejected :
    rowsAccept (withRetaggedPayload resultRows 5 ⟨"ExprOk", 104⟩ [4]) resultFunctions 1 refuseCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_changed_parameter_slot_is_rejected :
    rowsAccept (withWord resultRows 0 1) resultFunctions 0 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_forward_reference_is_rejected :
    rowsAccept (withWord resultRows 3 5) resultFunctions 0 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_constructor_that_skips_the_propagation_row_is_rejected :
    rowsAccept (withWord resultRows 3 0) resultFunctions 0 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_propagation_that_reads_another_row_is_rejected :
    rowsAccept (withWord resultRows 1 6) resultFunctions 0 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_propagation_tag_with_a_foreign_ordinal_is_rejected :
    rowsAccept (withTag resultRows 1 ⟨"ExprTry", 106⟩) resultFunctions 0 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem an_ok_name_with_the_err_ordinal_is_rejected :
    rowsAccept (withTag resultRows 2 ⟨"ExprOk", 105⟩) resultFunctions 0 throughCandidate = false := by
  decide +kernel

def withFunctions (change : RawFunction → RawFunction) (index : Nat) : RawFunctionRows :=
  { resultFunctions with functions := resultFunctions.functions.mapIdx fun position function =>
      if position = index then change function else function }

set_option maxRecDepth 100000 in
theorem a_function_entry_with_no_parameters_is_rejected :
    rowsAccept resultRows (withFunctions (fun f => { f with parameters := ⟨f.parameters.start, 0⟩ }) 0) 0
      throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_function_entry_with_another_body_is_rejected :
    rowsAccept resultRows (withFunctions (fun f => { f with body := 2 }) 0) 0 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_body_block_owned_by_another_function_is_rejected :
    rowsAccept resultRows { resultFunctions with blocks := resultFunctions.blocks.mapIdx fun position block =>
      if position = 0 then { block with owner := 2 } else block } 0 throughCandidate = false := by
  decide +kernel

end Xsh.Conformance.Connected
