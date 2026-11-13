import Xsh.Core.Values

namespace Xsh.Core

abbrev AnyReference (Identity : Type) := (kind : ReferenceKind) × Reference Identity kind

abbrev ReferencePolicy (Identity : Type) := AnyReference Identity → Prop

def ProcessStatus.references (status : ProcessStatus Identity) : List (AnyReference Identity) :=
  status.segments.flatMap fun segment =>
    segment.process.toList.map fun reference => ⟨.process, reference⟩

mutual
  def Value.references (candidate : Value Declaration Identity Source) : List (AnyReference Identity) :=
    match candidate with
    | .scalar _ | .set _ | .environmentPaths _ => []
    | .list items | .tagged _ _ items => referenceValues items
    | .map entries => referenceEntries entries
    | .record fields => referenceFields fields
    | .stream reference => [⟨.stream, reference⟩]
    | .filesystemEntry reference => [⟨.filesystemEntry, reference⟩]
    | .module reference exports => ⟨.module, reference⟩ :: referenceFields exports
    | .ok value | .err value | .nominal _ value => value.references
    | .status status => status.references
    | .error ⟨_, _, _, _, payload, _, _, _, cause, _⟩ =>
        referenceFields payload ++ referenceCause cause
    | .processError ⟨_, _, _, status, _, cause⟩ =>
        status.toList.flatMap ProcessStatus.references ++ referenceCause cause
    | .callable reference _ => [⟨.callable, reference⟩]
    | .command reference => [⟨.command, reference⟩]
    | .processHandle reference => [⟨.process, reference⟩]
    | .networkJob reference => [⟨.networkJob, reference⟩]
    | .filesystemRoot reference => [⟨.filesystemRoot, reference⟩]
    | .filesystemLock reference => [⟨.filesystemLock, reference⟩]
    | .sshSession reference => [⟨.sshSession, reference⟩]
  termination_by sizeOf candidate

  def referenceValues (items : List (Value Declaration Identity Source)) : List (AnyReference Identity) :=
    match items with
    | [] => []
    | item :: rest => item.references ++ referenceValues rest
  termination_by sizeOf items

  def referenceFields (fields : List (String × Value Declaration Identity Source)) : List (AnyReference Identity) :=
    match fields with
    | [] => []
    | (_, value) :: rest => value.references ++ referenceFields rest
  termination_by sizeOf fields

  def referenceEntries (entries : List (MapKey × Value Declaration Identity Source)) : List (AnyReference Identity) :=
    match entries with
    | [] => []
    | (_, value) :: rest => value.references ++ referenceEntries rest
  termination_by sizeOf entries

  def referenceCause (cause : Option (Value Declaration Identity Source)) : List (AnyReference Identity) :=
    match cause with
    | none => []
    | some value => value.references
  termination_by sizeOf cause
end

def Value.ReferencesValid (policy : ReferencePolicy Identity)
    (candidate : Value Declaration Identity Source) : Prop :=
  ∀ reference ∈ candidate.references, policy reference

def Value.decideReferencesValid (policy : ReferencePolicy Identity)
    (decisions : ∀ reference, Decidable (policy reference))
    (candidate : Value Declaration Identity Source) : Decidable (candidate.ReferencesValid policy) :=
  @List.decidableBAll _ policy decisions candidate.references

theorem Value.referencesValid_mono
    (first second : ReferencePolicy Identity)
    (retained : ∀ reference, first reference → second reference)
    (candidate : Value Declaration Identity Source) :
    candidate.ReferencesValid first → candidate.ReferencesValid second := by
  intro valid reference present
  exact retained reference (valid reference present)

theorem Value.references_eraseNominal (candidate : Value Declaration Identity Source) :
    candidate.eraseNominal.references = candidate.references := by
  cases candidate with
  | nominal _ value => simpa only [eraseNominal, references] using references_eraseNominal value
  | _ => rfl
termination_by sizeOf candidate

end Xsh.Core
