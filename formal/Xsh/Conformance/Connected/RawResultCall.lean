import Xsh.Conformance.Connected.RawResultFixture

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def resultCallSource : String :=
  "pure wrap(n: Int) -> Result[Int, Str] { return Ok(n) }\npure lift() -> Result[Int, Str] { return Ok(wrap(1)?) }\n"

def resultCallSourceNames : List String :=
  ["Int", "Bool", "Str", "Result", "pure", "wrap", "n", "return", "Ok", "lift"]

def resultCallRows : RawInstructionRows :=
  ⟨[⟨"ExprParam", 31⟩, ⟨"ExprOk", 104⟩, ⟨"StmtReturn", 150⟩, ⟨"ExprInt", 20⟩, ⟨"ExprDirectPureCall", 109⟩, ⟨"ExprTry", 107⟩, ⟨"ExprOk", 104⟩, ⟨"StmtReturn", 150⟩],
    [false, false, false, false, false, false, false, false],
    [false, false, false, false, true, false, false, false],
    [⟨0, 1⟩, ⟨1, 1⟩, ⟨2, 1⟩, ⟨5, 2⟩, ⟨10, 3⟩, ⟨13, 2⟩, ⟨15, 1⟩, ⟨16, 1⟩],
    [0, 0, 1, 1, 2, 1, 0, 1, 0, 3, 1, 2, 2, 4, 3, 5, 6, 1, 7],
    [], [],
    [⟨0, 0⟩, ⟨99, 7⟩, ⟨99, 8⟩, ⟨0, 54⟩, ⟨55, 55⟩], [0, 0, 0, 0, 0],
    [⟨0, 4⟩, ⟨4, 1⟩, ⟨5, 4⟩, ⟨9, 4⟩], [119, 114, 97, 112, 110, 97, 114, 103, 115, 108, 105, 102, 116]⟩

def resultCallFunctions : RawFunctionRows :=
  ⟨[⟨⟨3, 2⟩, 4294967295, 1, 3, (0, 0, 0)⟩, ⟨⟨7, 3⟩, 4294967295, 2, 0, (0, 0, 0)⟩, ⟨⟨17, 2⟩, 4294967295, 2, 3, (0, 0, 0)⟩],
    [⟨1, .ground 1, ⟨0, 1⟩, ⟨0, 1⟩, 1, 2, ⟨0, 2⟩, 4⟩, ⟨4, .ground 2, ⟨1, 0⟩, ⟨1, 1⟩, 3, 1, ⟨2, 1⟩, 4⟩]⟩

def resultCallSignatures : List ResultSignature :=
  [⟨"wrap", ⟨0, 38, 16⟩, [("n", .primitive .integer)], .result (.primitive .integer) (.primitive .string),
    [(⟨0, 47, 5⟩, .result (.primitive .integer) (.primitive .string)), (⟨0, 50, 1⟩, .primitive .integer)],
    []⟩,
   ⟨"lift", ⟨0, 87, 23⟩, [], .result (.primitive .integer) (.primitive .string),
    [(⟨0, 96, 12⟩, .result (.primitive .integer) (.primitive .string)), (⟨0, 99, 8⟩, .primitive .integer), (⟨0, 99, 7⟩, .result (.primitive .integer) (.primitive .string)), (⟨0, 104, 1⟩, .primitive .integer)],
    [(⟨0, 99, 4⟩, "wrap")]⟩]

def wrapBinder : BinderId := ⟨⟨20, by decide⟩⟩

def wrapCandidate : List StatementNode := [.returned (okNode (.read resultFirstBinder []))]
def liftCandidate : List StatementNode :=
  [.returned (okNode (.propagate (.call (.read wrapBinder []) [.integer "1"]) (.primitive .string)))]

def wrapSteps : List DerivationStep :=
  [.function, .argument, .bindLeft, .reflexive, .bindRight, .result, .reflexive, .bindLeft]
def liftSteps : List DerivationStep :=
  [.function, .reflexive, .bindRight, .result, .bindRight, .bindRight, .function, .argument, .bindLeft, .reflexive,
    .bindRight, .result, .bindRight, .reflexive, .result, .reflexive, .bindLeft]

def resultCallCandidates : List (List StatementNode × List DerivationStep) :=
  [(wrapCandidate, wrapSteps), (liftCandidate, liftSteps)]

def resultCallCertified : Bool :=
  match admitSource 0 resultCallSource with
  | .ok unit => certifyResultControl unit resultCallSignatures resultCallRows resultCallFunctions resultCallCandidates
  | .error _ => false

set_option maxRecDepth 100000 in
theorem result_call_is_certified : resultCallCertified = true := by
  decide +kernel

end Xsh.Conformance.Connected
