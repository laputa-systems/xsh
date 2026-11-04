import Xsh.Conformance.Connected.RawStore
import Xsh.Semantics.Connected.SafetyScopedFixture

namespace Xsh.Conformance.Connected
open Xsh.Typing.Connected Xsh.Semantics.Connected

def projectedScopedRows (ordinal slot location : Nat) (tagName : String) (assignment : Xsh.Syntax.AssignOp) : RawInstructionRows :=
  ⟨List.replicate 9 ⟨"ExprParam", 31⟩ ++ [⟨tagName, 123⟩],
    List.replicate 9 false ++ [true], List.replicate 10 false,
    List.replicate 9 ⟨0, 0⟩ ++ [⟨22, 6⟩],
    List.replicate 8 0 ++ [1, 4, ordinal] ++ List.replicate 11 0 ++ [slot, 1, 0, 8, 0, location],
    [assignment], [], [⟨67, 5⟩, ⟨57, 15⟩], [0, 0], [], []⟩

def projectedScopedBlocks : RawFunctionRows :=
  ⟨[⟨⟨8, 3⟩, 4294967295, 1, 0, (0, 0, 0)⟩], []⟩

def projectedScopedStoreCheck (ordinal slot location : Nat) (tagName : String) (assignment : Xsh.Syntax.AssignOp) : Bool :=
  match actualScopedCheckDecoded scopedUnit scopedDocument scopedScopes,
      sourceScopedOccurrences 128 scopedFunction scopedFixtureParameters [] [] scopedFixtureBinders scopedFixtureCandidate with
  | .ok principal, .ok occurrences =>
    checkRawStoreFact 128 scopedFixtureNames (substitute principal (.variable scopedFixtureTypeVar))
      scopedFixtureBinders principal scopedFixtureScheme.requirements (fun _ => none)
      (projectedScopedRows ordinal slot location tagName assignment) projectedScopedBlocks 1 9 [(scopedFixtureLocal, 2)]
      occurrences ⟨scopedBodyDocument, [.store scopedStore]⟩ (1, 0)
  | _, _ => false

set_option maxRecDepth 100000 in
theorem projected_scoped_store_checks_and_rejects_mutations :
    projectedScopedStoreCheck 0 2 2 "StmtAssignPath" .set = true ∧
    projectedScopedStoreCheck 1 2 2 "StmtAssignPath" .set = false ∧
    projectedScopedStoreCheck 0 3 2 "StmtAssignPath" .set = false ∧
    projectedScopedStoreCheck 0 2 1 "StmtAssignPath" .set = false ∧
    projectedScopedStoreCheck 0 2 2 "StmtAssign" .set = false ∧
    projectedScopedStoreCheck 0 2 2 "StmtAssignPath" .add = false := by
  decide +kernel
end Xsh.Conformance.Connected
