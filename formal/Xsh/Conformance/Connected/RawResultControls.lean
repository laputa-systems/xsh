import Xsh.Conformance.Connected.RawResultFixture

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def functionCertifies (signatures : List ResultSignature) (rows : RawInstructionRows)
    (functions : RawFunctionRows) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep) : Bool :=
  match admitSource 0 resultControlSource, signatures[index]? with
  | .ok unit, some signature => certifyResultFunction unit rows functions signature index candidate steps
  | _, _ => false

def baselineCertifies (index : Nat) (candidate : List StatementNode) (steps : List DerivationStep) : Bool :=
  functionCertifies resultSignatures resultRows resultFunctions index candidate steps

set_option maxRecDepth 100000 in
theorem each_result_function_is_certified_alone :
    baselineCertifies 0 keepCandidate keepSteps = true ∧
    baselineCertifies 1 refuseCandidate refuseSteps = true ∧
    baselineCertifies 2 throughCandidate throughSteps = true ∧
    baselineCertifies 3 advanceCandidate advanceSteps = true := by
  decide +kernel

def validationAccepts (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep) : Bool :=
  match admitSource 0 resultControlSource, signatures[index]? with
  | .ok unit, some signature =>
      match validateResultFunction unit signature index candidate steps with | .ok _ => true | .error _ => false
  | _, _ => false

def rowsAccept (rows : RawInstructionRows) (functions : RawFunctionRows) (index : Nat)
    (candidate : List StatementNode) : Bool :=
  match admitSource 0 resultControlSource, resultSignatures[index]? with
  | .ok unit, some signature => rawResultReturnCheck unit rows functions signature index candidate
  | _, _ => false

set_option maxRecDepth 100000 in
theorem source_validation_and_exported_rows_each_accept_the_candidates :
    validationAccepts resultSignatures 0 keepCandidate keepSteps = true ∧
    validationAccepts resultSignatures 1 refuseCandidate refuseSteps = true ∧
    validationAccepts resultSignatures 2 throughCandidate throughSteps = true ∧
    validationAccepts resultSignatures 3 advanceCandidate advanceSteps = true ∧
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

set_option maxRecDepth 100000 in
theorem forged_published_facts_are_rejected :
    rejectsSignature 0 (fun s => { s with result := .result (.primitive .integer) (.primitive .integer) })
      keepCandidate keepSteps = true ∧
    rejectsSignature 0 (fun s => { s with result := .primitive .integer }) keepCandidate keepSteps = true ∧
    rejectsSignature 0 (fun s => { s with parameters := [("a", .primitive .string)] })
      keepCandidate keepSteps = true ∧
    rejectsSignature 0 (fun s => { s with parameters := [] }) keepCandidate keepSteps = true ∧
    rejectsSignature 0 (fun s => { s with body := ⟨0, 0, 1⟩ }) keepCandidate keepSteps = true ∧
    rejectsSignature 0 (fun s => { s with name := "refuse" }) keepCandidate keepSteps = true ∧
    rejectsSignature 1 (fun s => { s with result := .result (.primitive .string) (.primitive .integer) })
      refuseCandidate refuseSteps = true ∧
    rejectsSignature 2 (fun s => { s with parameters := [("r", .primitive .integer)] })
      throughCandidate throughSteps = true ∧
    rejectsSignature 2 (fun s => { s with parameters := [("r", .result (.primitive .integer) (.primitive .integer))] })
      throughCandidate throughSteps = true ∧
    rejectsSignature 3 (fun s => { s with result := .result (.primitive .string) (.primitive .string) })
      advanceCandidate advanceSteps = true := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem swapped_ok_and_error_candidates_are_rejected :
    validationAccepts resultSignatures 0 [.returned (errorNode (.read resultFirstBinder []))] keepSteps = false ∧
    validationAccepts resultSignatures 1 [.returned (okNode (.string "no"))] refuseSteps = false ∧
    validationAccepts resultSignatures 2
      [.returned (errorNode (.propagate (.read resultFirstBinder []) (.primitive .string)))] throughSteps = false ∧
    validationAccepts resultSignatures 3
      [.returned (errorNode (.binary .add (.propagate (.read resultFirstBinder []) (.primitive .string))
        (.integer "1")))] advanceSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem wrong_propagation_candidates_are_rejected :
    validationAccepts resultSignatures 2 [.returned (okNode (.read resultFirstBinder []))] throughSteps = false ∧
    validationAccepts resultSignatures 0
      [.returned (okNode (.propagate (.read resultFirstBinder []) (.primitive .string)))] keepSteps = false ∧
    validationAccepts resultSignatures 2
      [.returned (okNode (.propagate (.read resultFirstBinder []) (.primitive .integer)))] throughSteps = false ∧
    validationAccepts resultSignatures 3
      [.returned (okNode (.binary .add (.read resultFirstBinder []) (.integer "1")))] advanceSteps = false ∧
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
theorem forged_rows_are_rejected :
    rowsAccept (withRetaggedPayload resultRows 1 ⟨"ExprErr", 105⟩ [0, 0]) resultFunctions 0 keepCandidate = false ∧
    rowsAccept (withRetaggedPayload resultRows 4 ⟨"ExprOk", 104⟩ [3]) resultFunctions 1 refuseCandidate = false ∧
    rowsAccept (withRetaggedPayload resultRows 8 ⟨"ExprErr", 105⟩ [7, 0]) resultFunctions 2 throughCandidate = false ∧
    rowsAccept (withWord resultRows 0 1) resultFunctions 0 keepCandidate = false ∧
    rowsAccept (withWord resultRows 1 5) resultFunctions 0 keepCandidate = false ∧
    rowsAccept (withWord resultRows 14 6) resultFunctions 2 throughCandidate = false ∧
    rowsAccept (withWord resultRows 24 10) resultFunctions 3 advanceCandidate = false ∧
    rowsAccept (withTag resultRows 7 ⟨"ExprTry", 106⟩) resultFunctions 2 throughCandidate = false ∧
    rowsAccept (withTag resultRows 1 ⟨"ExprOk", 105⟩) resultFunctions 0 keepCandidate = false ∧
    rowsAccept (withWord resultRows 12 10) resultFunctions 2 throughCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem forged_function_rows_are_rejected :
    rowsAccept resultRows { resultFunctions with functions := resultFunctions.functions.mapIdx fun position function =>
      if position = 0 then { function with parameters := ⟨function.parameters.start, 0⟩ } else function }
      0 keepCandidate = false ∧
    rowsAccept resultRows { resultFunctions with functions := resultFunctions.functions.mapIdx fun position function =>
      if position = 0 then { function with body := 2 } else function } 0 keepCandidate = false ∧
    rowsAccept resultRows { resultFunctions with blocks := resultFunctions.blocks.mapIdx fun position block =>
      if position = 0 then { block with owner := 2 } else block } 0 keepCandidate = false := by
  decide +kernel

end Xsh.Conformance.Connected
