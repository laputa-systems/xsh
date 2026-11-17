import Xsh.Conformance.Connected.RawResultCall
import Xsh.Conformance.Connected.RawResultControls

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def callValidationAccepts (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep) : Bool :=
  match admitSource 0 resultCallSource with
  | .ok unit => match validateResultFunction unit signatures index candidate steps with | .ok _ => true | .error _ => false
  | .error _ => false

def callRowsAcceptWith (table : List RawParameter) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (index : Nat) (candidate : List StatementNode) : Bool :=
  rawResultReturnCheckFor resultCallSourceNames rows functions table resultCallSignatures index candidate

def callRowsAccept (rows : RawInstructionRows) (functions : RawFunctionRows) (index : Nat)
    (candidate : List StatementNode) : Bool :=
  callRowsAcceptWith resultCallParameters rows functions index candidate

def callTypesAccept (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep) : Bool :=
  match admitSource 0 resultCallSource with
  | .ok unit => checkResultExpressionTypes unit signatures index candidate steps
  | .error _ => false

def callTargetsAccept (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode) : Bool :=
  match admitSource 0 resultCallSource with
  | .ok unit => checkResultCallTargets unit signatures index candidate
  | .error _ => false

set_option maxRecDepth 100000 in
theorem each_call_fixture_component_accepts_the_caller :
    callValidationAccepts resultCallSignatures 1 liftCandidate liftSteps = true ∧
    callRowsAccept resultCallRows resultCallFunctions 1 liftCandidate = true ∧
    callTypesAccept resultCallSignatures 1 liftCandidate liftSteps = true ∧
    callTargetsAccept resultCallSignatures 1 liftCandidate = true := by
  decide +kernel

def editCallSignature (index : Nat) (change : ResultSignature → ResultSignature) : List ResultSignature :=
  resultCallSignatures.mapIdx fun position signature => if position = index then change signature else signature

set_option maxRecDepth 100000 in
theorem a_forged_callee_result_type_is_rejected_at_the_caller :
    callValidationAccepts (editCallSignature 0 fun s => { s with result := integerErrorResult }) 1
      liftCandidate liftSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_forged_callee_parameter_type_is_rejected_at_the_caller :
    callValidationAccepts (editCallSignature 0 fun s => { s with parameters := [("n", .primitive .string)] }) 1
      liftCandidate liftSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_callee_published_under_another_name_is_rejected_at_the_caller :
    callValidationAccepts (editCallSignature 0 fun s => { s with name := "n" }) 1 liftCandidate liftSteps = false := by
  decide +kernel

def unknownFunctionBinder : BinderId := ⟨⟨22, by decide⟩⟩

def unknownCallCandidate : List StatementNode :=
  [.returned (okNode (.propagate (.call (.read unknownFunctionBinder []) [.integer "1"]) (.primitive .string)))]

set_option maxRecDepth 100000 in
theorem a_call_of_an_unknown_function_is_rejected :
    callValidationAccepts resultCallSignatures 1 unknownCallCandidate liftSteps = false ∧
    callRowsAccept resultCallRows resultCallFunctions 1 unknownCallCandidate = false ∧
    callTargetsAccept resultCallSignatures 1 unknownCallCandidate = false := by
  decide +kernel

def withCallExpressions (index : Nat) (expressions : List (SourceSite × MonoType)) : List ResultSignature :=
  editCallSignature index fun s => { s with expressions := expressions }

set_option maxRecDepth 100000 in
theorem a_forged_call_site_type_is_rejected :
    callTypesAccept (withCallExpressions 1 [(⟨0, 96, 12⟩, stringResult), (⟨0, 99, 8⟩, .primitive .integer),
      (⟨0, 99, 7⟩, integerErrorResult), (⟨0, 104, 1⟩, .primitive .integer)]) 1 liftCandidate liftSteps = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_forged_call_argument_type_is_rejected :
    callTypesAccept (withCallExpressions 1 [(⟨0, 96, 12⟩, stringResult), (⟨0, 99, 8⟩, .primitive .integer),
      (⟨0, 99, 7⟩, stringResult), (⟨0, 104, 1⟩, .primitive .string)]) 1 liftCandidate liftSteps = false := by
  decide +kernel

def withCalls (calls : List (SourceSite × String)) : List ResultSignature :=
  editCallSignature 1 fun s => { s with calls := calls }

set_option maxRecDepth 100000 in
theorem a_forged_call_target_is_rejected :
    callTargetsAccept (withCalls [(⟨0, 99, 4⟩, "lift")]) 1 liftCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_dropped_call_target_is_rejected :
    callTargetsAccept (withCalls []) 1 liftCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_shifted_call_name_site_is_rejected :
    callTargetsAccept (withCalls [(⟨0, 99, 7⟩, "wrap")]) 1 liftCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_call_row_with_another_target_is_rejected :
    callRowsAccept (withWord resultCallRows 10 2) resultCallFunctions 1 liftCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem an_indirect_call_row_is_rejected :
    callRowsAccept (withTag resultCallRows 4 ⟨"ExprCall", 108⟩) resultCallFunctions 1 liftCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_call_row_with_another_argument_is_rejected :
    callRowsAccept (withWord resultCallRows 9 0) resultCallFunctions 1 liftCandidate = false := by
  decide +kernel

def withCallFunctions (change : RawFunction → RawFunction) (index : Nat) : RawFunctionRows :=
  { resultCallFunctions with functions := resultCallFunctions.functions.mapIdx fun position function =>
      if position = index then change function else function }

set_option maxRecDepth 100000 in
theorem a_callee_function_entry_with_another_name_is_rejected :
    callRowsAccept resultCallRows (withCallFunctions (fun f => { f with name := 4 }) 0) 0 wrapCandidate = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem a_callee_parameter_row_with_another_name_is_rejected :
    callRowsAcceptWith (resultCallParameters.map fun parameter => { parameter with name := 4 }) resultCallRows
      resultCallFunctions 0 wrapCandidate = false ∧
    callRowsAcceptWith (resultCallParameters.map fun parameter => { parameter with flags := 2 }) resultCallRows
      resultCallFunctions 0 wrapCandidate = false := by
  decide +kernel

end Xsh.Conformance.Connected
