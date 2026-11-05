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
    validationAccepts resultSignatures 0 keepCandidate keepSteps = true ∧
    validationAccepts resultSignatures 1 refuseCandidate refuseSteps = true ∧
    validationAccepts resultSignatures 2 throughCandidate throughSteps = true ∧
    validationAccepts resultSignatures 3 advanceCandidate advanceSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem exported_rows_accept_each_candidate :
    rowsAccept resultRows resultFunctions 0 keepCandidate = true ∧
    rowsAccept resultRows resultFunctions 1 refuseCandidate = true ∧
    rowsAccept resultRows resultFunctions 2 throughCandidate = true ∧
    rowsAccept resultRows resultFunctions 3 advanceCandidate = true := by
  decide +kernel

def editSignature (index : Nat) (change : ResultSignature → ResultSignature) : List ResultSignature :=
  resultSignatures.mapIdx fun position signature => if position = index then change signature else signature

def rejectsSignature (index : Nat) (change : ResultSignature → ResultSignature)
    (candidate : List StatementNode) (steps : List DerivationStep) : Bool :=
  !validationAccepts (editSignature index change) index candidate steps

def integerErrorResult : MonoType := .result (.primitive .integer) (.primitive .integer)

set_option maxRecDepth 100000 in
theorem forged_result_types_are_rejected :
    rejectsSignature 0 (fun s => { s with result := integerErrorResult }) keepCandidate keepSteps = true ∧
    rejectsSignature 0 (fun s => { s with result := .primitive .integer }) keepCandidate keepSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem forged_parameter_types_are_rejected :
    rejectsSignature 0 (fun s => { s with parameters := [("a", .primitive .string)] })
      keepCandidate keepSteps = true ∧
    rejectsSignature 2 (fun s => { s with parameters := [("r", .primitive .integer)] })
      throughCandidate throughSteps = true ∧
    rejectsSignature 2 (fun s => { s with parameters := [("r", integerErrorResult)] })
      throughCandidate throughSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem forged_function_identity_facts_are_rejected :
    rejectsSignature 0 (fun s => { s with parameters := [] }) keepCandidate keepSteps = true ∧
    rejectsSignature 0 (fun s => { s with body := ⟨0, 0, 1⟩ }) keepCandidate keepSteps = true ∧
    rejectsSignature 0 (fun s => { s with name := "refuse" }) keepCandidate keepSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem forged_error_and_sum_result_types_are_rejected :
    rejectsSignature 1 (fun s => { s with result := .result (.primitive .string) (.primitive .integer) })
      refuseCandidate refuseSteps = true ∧
    rejectsSignature 3 (fun s => { s with result := .result (.primitive .string) (.primitive .string) })
      advanceCandidate advanceSteps = true := by
  decide +kernel

def swappedKeep : List StatementNode := [.returned (errorNode (.read resultFirstBinder []))]
def swappedRefuse : List StatementNode := [.returned (okNode (.string "no"))]
def swappedThrough : List StatementNode :=
  [.returned (errorNode (.propagate (.read resultFirstBinder []) (.primitive .string)))]
def swappedAdvance : List StatementNode :=
  [.returned (errorNode (.binary .add (.propagate (.read resultFirstBinder []) (.primitive .string))
    (.integer "1")))]

set_option maxRecDepth 100000 in
theorem swapped_ok_and_error_candidates_are_rejected :
    validationAccepts resultSignatures 0 swappedKeep keepSteps = false ∧
    validationAccepts resultSignatures 1 swappedRefuse refuseSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem swapped_constructors_around_propagation_are_rejected :
    validationAccepts resultSignatures 2 swappedThrough throughSteps = false ∧
    validationAccepts resultSignatures 3 swappedAdvance advanceSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem dropped_or_mistyped_propagation_is_rejected :
    validationAccepts resultSignatures 2 [.returned (okNode (.read resultFirstBinder []))] throughSteps = false ∧
    validationAccepts resultSignatures 2
      [.returned (okNode (.propagate (.read resultFirstBinder []) (.primitive .integer)))] throughSteps = false ∧
    validationAccepts resultSignatures 3
      [.returned (okNode (.binary .add (.read resultFirstBinder []) (.integer "1")))] advanceSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem misplaced_or_unwarranted_propagation_is_rejected :
    validationAccepts resultSignatures 0
      [.returned (okNode (.propagate (.read resultFirstBinder []) (.primitive .string)))] keepSteps = false ∧
    validationAccepts resultSignatures 2
      [.returned (.propagate (okNode (.read resultFirstBinder [])) (.primitive .string))] throughSteps = false := by
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
    rowsAccept (withRetaggedPayload resultRows 1 ⟨"ExprErr", 105⟩ [0, 0]) resultFunctions 0 keepCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem an_ok_row_where_the_candidate_builds_err_is_rejected :
    rowsAccept (withRetaggedPayload resultRows 4 ⟨"ExprOk", 104⟩ [3]) resultFunctions 1 refuseCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem an_err_row_around_a_propagation_is_rejected :
    rowsAccept (withRetaggedPayload resultRows 8 ⟨"ExprErr", 105⟩ [7, 0]) resultFunctions 2 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_changed_parameter_slot_is_rejected :
    rowsAccept (withWord resultRows 0 1) resultFunctions 0 keepCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_forward_reference_is_rejected :
    rowsAccept (withWord resultRows 1 5) resultFunctions 0 keepCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_constructor_that_skips_its_propagation_row_is_rejected :
    rowsAccept (withWord resultRows 14 6) resultFunctions 2 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem an_addition_that_skips_its_propagation_row_is_rejected :
    rowsAccept (withWord resultRows 24 10) resultFunctions 3 advanceCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_propagation_that_reads_another_row_is_rejected :
    rowsAccept (withWord resultRows 12 10) resultFunctions 2 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_propagation_tag_with_a_foreign_ordinal_is_rejected :
    rowsAccept (withTag resultRows 7 ⟨"ExprTry", 106⟩) resultFunctions 2 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem an_ok_name_with_the_err_ordinal_is_rejected :
    rowsAccept (withTag resultRows 1 ⟨"ExprOk", 105⟩) resultFunctions 0 keepCandidate = false := by
  decide +kernel

def withFunctions (change : RawFunction → RawFunction) (index : Nat) : RawFunctionRows :=
  { resultFunctions with functions := resultFunctions.functions.mapIdx fun position function =>
      if position = index then change function else function }

set_option maxRecDepth 100000 in
theorem a_function_entry_with_no_parameters_is_rejected :
    rowsAccept resultRows (withFunctions (fun f => { f with parameters := ⟨f.parameters.start, 0⟩ }) 0) 0
      keepCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_function_entry_with_another_body_is_rejected :
    rowsAccept resultRows (withFunctions (fun f => { f with body := 2 }) 0) 0 keepCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_body_block_owned_by_another_function_is_rejected :
    rowsAccept resultRows { resultFunctions with blocks := resultFunctions.blocks.mapIdx fun position block =>
      if position = 0 then { block with owner := 2 } else block } 0 keepCandidate = false := by
  decide +kernel

end Xsh.Conformance.Connected
