import Xsh.Conformance.Connected.ScopedTransferDocuments
import Xsh.Conformance.Connected.RawStoreFixture

namespace Xsh.Conformance.Connected
open Xsh.Typing.Connected Xsh.Semantics.Connected

def scopedAssignmentTransferCheck (rows : RawInstructionRows) (fact : TransferFact) : Bool :=
  match actualScopedCheckDecoded scopedUnit scopedDocument scopedScopes,
      sourceScopedOccurrences 128 scopedFunction scopedFixtureParameters [] []
        scopedFixtureBinders scopedFixtureCandidate with
  | .ok principal, .ok occurrences =>
    match occurrences.find? (fun occurrence => decide (occurrence.site = ⟨0, 67, 5⟩)) with
    | none => false
    | some occurrence =>
      decide (occurrence.storeTarget = some (scopedFixtureLocal, ⟨0, 57, 15⟩)) &&
      !occurrence.storeDeclaredHere &&
      decide (fact.role = .assignment ∧ fact.site = ⟨0, 57, 15⟩) &&
      decide (fact.checkedOverride = none) &&
      decide (checkerMayHoldResource (.variable scopedFixtureTypeVar) = some fact.transfers) &&
      decide (rows.resourceTransfers[9]? = some fact.transfers) &&
      checkRawStoreFact 128 scopedFixtureNames (substitute principal (.variable scopedFixtureTypeVar))
        scopedFixtureBinders principal scopedFixtureScheme.requirements (fun _ => none)
        rows projectedScopedBlocks 1 9 [(scopedFixtureLocal, 2)] occurrences
        ⟨scopedBodyDocument, [.store scopedStore]⟩ (1, 0)
  | _, _ => false

def scopedTransferRows : RawInstructionRows :=
  projectedScopedRows 0 2 2 "StmtAssignPath" .set

def scopedTransferRowsWrongMark : RawInstructionRows :=
  { scopedTransferRows with resourceTransfers := List.replicate 10 false }

set_option maxRecDepth 100000 in
theorem scoped_assignment_transfer_matches_source_and_rejects_mutations :
    scopedTransferFacts[2]? = some ⟨.assignment, ⟨0, 57, 15⟩, true, none⟩ ∧
    scopedAssignmentTransferCheck scopedTransferRows ⟨.assignment, ⟨0, 57, 15⟩, true, none⟩ = true ∧
    scopedAssignmentTransferCheck scopedTransferRowsWrongMark ⟨.assignment, ⟨0, 57, 15⟩, true, none⟩ = false ∧
    scopedAssignmentTransferCheck scopedTransferRows ⟨.assignment, ⟨0, 57, 15⟩, false, none⟩ = false ∧
    scopedAssignmentTransferCheck scopedTransferRows ⟨.assignment, ⟨0, 57, 15⟩, true, some false⟩ = false ∧
    scopedAssignmentTransferCheck scopedTransferRows ⟨.assignment, ⟨0, 57, 15⟩, true, some true⟩ = false ∧
    scopedAssignmentTransferCheck scopedTransferRows ⟨.returned, ⟨0, 57, 15⟩, true, none⟩ = false ∧
    scopedAssignmentTransferCheck scopedTransferRows ⟨.assignment, ⟨0, 67, 5⟩, true, none⟩ = false := by
  decide +kernel

end Xsh.Conformance.Connected
