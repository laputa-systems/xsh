import Xsh.Conformance.Connected.RawResultLiterals

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def resultInstance : List (Variable × MonoType) :=
  [(identityVariable, .primitive .integer), (resultValueVariable, .primitive .string)]

def okNode (argument : Node) : Node := .call (.read resultOkBinder resultInstance) [argument]
def errorNode (argument : Node) : Node := .call (.read resultErrorBinder resultInstance) [argument]

def resultFirstBinder : BinderId := ⟨⟨0, by decide⟩⟩

def keepCandidate : List StatementNode := [.returned (okNode (.read resultFirstBinder []))]
def refuseCandidate : List StatementNode := [.returned (errorNode (.string "no"))]
def throughCandidate : List StatementNode :=
  [.returned (okNode (.propagate (.read resultFirstBinder []) (.primitive .string)))]
def advanceCandidate : List StatementNode :=
  [.returned (okNode (.binary .add (.propagate (.read resultFirstBinder []) (.primitive .string)) (.integer "1")))]

def keepSteps : List DerivationStep :=
  [.function, .argument, .bindLeft, .reflexive, .bindRight, .result, .reflexive, .bindLeft]
def refuseSteps : List DerivationStep :=
  [.function, .argument, .bindLeft, .reflexive, .bindRight, .result, .bindLeft, .reflexive]
def throughSteps : List DerivationStep :=
  [.result, .bindRight, .bindRight, .function, .argument, .bindLeft, .reflexive, .bindRight, .result, .bindRight,
    .reflexive, .result, .reflexive, .bindLeft]
def advanceSteps : List DerivationStep :=
  [.result, .bindRight, .bindRight, .reflexive, .reflexive, .function, .argument, .bindLeft, .reflexive, .bindRight,
    .result, .bindRight, .reflexive, .result, .reflexive, .bindLeft]

def resultCandidates : List (List StatementNode × List DerivationStep) :=
  [(keepCandidate, keepSteps), (refuseCandidate, refuseSteps), (throughCandidate, throughSteps),
    (advanceCandidate, advanceSteps)]

def certifyResultControl (unit : SourceUnit) (signatures : List ResultSignature) (rows : RawInstructionRows)
    (functions : RawFunctionRows) (cases : List (List StatementNode × List DerivationStep)) : Bool :=
  decide (unit.declarations.length = signatures.length) &&
    decide (functions.functions.length = signatures.length) &&
    decide (cases.length = signatures.length) &&
    ((signatures.zip cases).zipIdx.all fun ((signature, candidate, steps), index) =>
      certifyResultFunction unit rows functions signature index candidate steps)

def resultControlCertified : Bool :=
  match admitSource 0 resultControlSource with
  | .ok unit => certifyResultControl unit resultSignatures resultRows resultFunctions resultCandidates
  | .error _ => false

set_option maxRecDepth 100000 in
theorem result_control_is_certified : resultControlCertified = true := by
  decide +kernel

end Xsh.Conformance.Connected
