import Xsh.Core.MembershipDecision
import Xsh.Core.ReferenceValidity
import Xsh.Semantics.Environments

namespace Xsh.Core.Store

open Xsh.Semantics.Environments

structure World (Declaration Identity Source : Type) where
  membership : MembershipWorld Declaration Identity Source
  references : ReferencePolicy Identity

structure Decisions (world : World Declaration Identity Source) where
  contracts : ContractDecisions world.membership
  references : ∀ reference, Decidable (world.references reference)

def HasType (world : World Declaration Identity Source) (type : ValueType Declaration)
    (candidate : Value Declaration Identity Source) : Prop :=
  candidate.ReferencesValid world.references ∧ Membership.value world.membership type candidate

abbrev State (world : World Declaration Identity Source) :=
  TypedStore (ValueType Declaration) (Value Declaration Identity Source) (HasType world)

variable [DecidableEq Declaration]

def decideHasType (world : World Declaration Identity Source) (decisions : Decisions world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    Decidable (HasType world type candidate) := by
  letI := candidate.decideReferencesValid world.references decisions.references
  letI := Membership.decideTerm world.membership decisions.contracts type candidate
  unfold HasType
  infer_instance

def check (world : World Declaration Identity Source) (decisions : Decisions world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) : Bool :=
  @decide (HasType world type candidate) (decideHasType world decisions type candidate)

theorem check_iff (world : World Declaration Identity Source) (decisions : Decisions world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    check world decisions type candidate = true ↔ HasType world type candidate := by
  letI := decideHasType world decisions type candidate
  exact decide_eq_true_iff

def allocate (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : State world) (cell : CellId) (mutability : Mutability)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) : Option (State world) :=
  allocateCell (decideHasType world decisions) store cell mutability type candidate

theorem allocate_accepted_iff (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : State world) (cell : CellId) (mutability : Mutability)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    (∃ next, allocate world decisions store cell mutability type candidate = some next) ↔
      store cell = none ∧ candidate.ReferencesValid world.references ∧
        Membership.value world.membership type candidate := by
  simpa only [allocate, HasType, and_assoc] using
    Xsh.Semantics.Environments.allocate_accepted_iff (decideHasType world decisions)
      store cell mutability type candidate

theorem allocate_rejects_unretained_reference
    (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : State world) (cell : CellId) (mutability : Mutability)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source)
    (reference : AnyReference Identity) (presentReference : reference ∈ candidate.references)
    (unretained : ¬ world.references reference) :
    allocate world decisions store cell mutability type candidate = none := by
  apply Xsh.Semantics.Environments.allocate_rejects_nonmember
  intro valid
  exact unretained (valid.1 reference presentReference)

def write (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : State world) (cell : CellId) (candidate : Value Declaration Identity Source) :
    StoreWriteResult (State world) :=
  writeCell (decideHasType world decisions) store cell candidate

theorem write_accepted_iff (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : State world) (cell : CellId) (candidate : Value Declaration Identity Source) :
    (∃ updated, write world decisions store cell candidate = .accepted updated) ↔
      ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
        candidate.ReferencesValid world.references ∧
        Membership.value world.membership current.contents.typeTag candidate := by
  simpa only [write, HasType, and_assoc] using
    Xsh.Semantics.Environments.write_accepted_iff (decideHasType world decisions) store cell candidate

theorem write_rejects_unretained_reference
    (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : State world) (cell : CellId) (candidate : Value Declaration Identity Source)
    (reference : AnyReference Identity) (presentReference : reference ∈ candidate.references)
    (unretained : ¬ world.references reference) :
    (write world decisions store cell candidate).storeAfter = store := by
  have invalid : ∀ type, ¬ HasType world type candidate := by
    intro type valid
    exact unretained (valid.1 reference presentReference)
  cases present : store cell with
  | none =>
    exact Xsh.Semantics.Environments.write_rejects_missing_cell_atomically
      (decideHasType world decisions) store cell candidate present
  | some current =>
    cases mutableCell : current.mutability with
    | immutable =>
      exact Xsh.Semantics.Environments.write_rejects_immutable_cell_atomically
        (decideHasType world decisions) store cell candidate current present mutableCell
    | mutable =>
      exact Xsh.Semantics.Environments.write_rejects_nonmember_atomically
        (decideHasType world decisions) store cell candidate current present mutableCell
        (invalid current.contents.typeTag)

theorem write_preserves_unrelated_cells
    (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : State world) (cell other : CellId) (candidate : Value Declaration Identity Source)
    (different : other ≠ cell) :
    (write world decisions store cell candidate).storeAfter other = store other :=
  Xsh.Semantics.Environments.write_preserves_unrelated_cells (decideHasType world decisions)
    store cell other candidate different

end Xsh.Core.Store
