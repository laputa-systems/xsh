import Xsh.Validation.Core.Union
import Xsh.Validation.Core.Store

namespace Xsh.Validation.Core.Narrow

open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Validation.Core
open Xsh.Validation.Core.Union
open Xsh.Proofs.Core.Env

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

def typeTest (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) : Bool :=
  @decide (Membership.value world type candidate) (Membership.decideTerm world contracts type candidate)

theorem typeTest_iff (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    typeTest world contracts type candidate = true ↔ Membership.value world type candidate := by
  letI := Membership.decideTerm world contracts type candidate
  exact decide_eq_true_iff

abbrev Facts (Declaration : Type) := CellId → Option (ValueType Declaration)

def FactsSound (world : World Declaration Identity Source) (facts : Facts Declaration) (store : Store world) : Prop :=
  ∀ cell type, facts cell = some type → ∃ stored, store cell = some stored ∧
    Membership.value world.membership type stored.contents.value

def learn (facts : Facts Declaration) (cell : CellId) (type : ValueType Declaration) : Facts Declaration :=
  fun other => if other = cell then some type else facts other

def forget (facts : Facts Declaration) (cell : CellId) : Facts Declaration :=
  fun other => if other = cell then none else facts other

omit [DecidableEq Declaration] in
theorem optional_without_null (world : MembershipWorld Declaration Identity Source)
    {element : ValueType Declaration} {candidate : Value Declaration Identity Source}
    (member : Membership.value world (.optional element) candidate)
    (notNull : ¬ Membership.primitive .null candidate) : Membership.value world element candidate := by
  simp only [Membership.value] at member
  rcases member with isNull | member
  · exact absurd isNull notNull
  · exact member

omit [DecidableEq Declaration] in
theorem union_remainder (world : MembershipWorld Declaration Identity Source)
    {union remainderType : ValueType Declaration} {members : List (ValueType Declaration)}
    (listed : unionMembers union = some members) (index : Nat) (present : index < members.length)
    (built : unionOf (members.eraseIdx index) = some remainderType)
    {candidate : Value Declaration Identity Source} (member : Membership.value world union candidate)
    (failed : ¬ Membership.value world members[index] candidate) :
    Membership.value world remainderType candidate := by
  obtain ⟨found, foundPresent, foundMember⟩ := (union_member_iff world listed candidate).mp member
  refine (unionOf_member_iff world built candidate).mpr ⟨found, ?_, foundMember⟩
  obtain ⟨position, positionLt, positionFound⟩ := List.getElem_of_mem foundPresent
  have different : position ≠ index := by
    intro same
    subst same
    exact failed (positionFound ▸ foundMember)
  rw [← positionFound]
  exact (List.mem_eraseIdx_iff_getElem.mpr ⟨position, positionLt, different, rfl⟩)

theorem learn_sound (world : World Declaration Identity Source) (contracts : ContractDecisions world.membership)
    (facts : Facts Declaration) (store : Store world) (sound : FactsSound world facts store) (cell : CellId)
    (stored : Cell world) (present : store cell = some stored) (type : ValueType Declaration)
    (passed : typeTest world.membership contracts type stored.contents.value = true) :
    FactsSound world (learn facts cell type) store := by
  intro other otherType known
  by_cases same : other = cell
  · subst same
    simp only [learn, ite_true, Option.some.injEq] at known
    subst known
    exact ⟨stored, present, (typeTest_iff _ _ _ _).mp passed⟩
  · simp only [learn, same, ite_false] at known
    exact sound other otherType known

theorem assignment_forgets_its_fact (world : World Declaration Identity Source) (decisions : Decisions world)
    (facts : Facts Declaration) (store next : Store world) (sound : FactsSound world facts store) (cell : CellId)
    (candidate : Value Declaration Identity Source)
    (committed : assign world decisions store cell candidate = .committed next) :
    FactsSound world (forget facts cell) next := by
  obtain ⟨_, _, _, others⟩ := assign_committed_effect world decisions store next cell candidate committed
  intro other type known
  by_cases same : other = cell
  · simp [forget, same] at known
  · simp only [forget, same, ite_false] at known
    rw [others other same]
    exact sound other type known

/-- Narrowing over every Core type, value and world. A type test is true
exactly for members, so the passing branch may use the tested type. A union
member test that fails narrows to the union of the remaining members in
order. A non-null value of `T?` is a `T`. A fact learned from a passing test
about a cell's current value is sound, assignment to that cell forgets the
fact and keeps every other sound fact, and a failed or uncommitted store
keeps the store, so every remaining fact stays sound. -/
theorem narrowing (world : World Declaration Identity Source) (decisions : Decisions world) :
    (∀ (type : ValueType Declaration) candidate,
      typeTest world.membership decisions.contracts type candidate = true ↔
        Membership.value world.membership type candidate) ∧
    (∀ (union remainderType : ValueType Declaration) members index (present : index < members.length)
      candidate, unionMembers union = some members → unionOf (members.eraseIdx index) = some remainderType →
      Membership.value world.membership union candidate →
      typeTest world.membership decisions.contracts members[index] candidate = false →
        Membership.value world.membership remainderType candidate) ∧
    (∀ (element : ValueType Declaration) candidate, Membership.value world.membership (.optional element) candidate →
      ¬ Membership.primitive .null candidate → Membership.value world.membership element candidate) ∧
    (∀ (facts : Facts Declaration) (store : Store world) cell (stored : Cell world) type, FactsSound world facts store →
      store cell = some stored → typeTest world.membership decisions.contracts type stored.contents.value = true →
        FactsSound world (learn facts cell type) store) ∧
    (∀ (facts : Facts Declaration) (store next : Store world) cell candidate, FactsSound world facts store →
      assign world decisions store cell candidate = .committed next → FactsSound world (forget facts cell) next) ∧
    (∀ (facts : Facts Declaration) (store : Store world) cell candidate, FactsSound world facts store →
      (∀ next, assign world decisions store cell candidate ≠ .committed next) →
        FactsSound world facts (assign world decisions store cell candidate).storeAfter) := by
  refine ⟨typeTest_iff world.membership decisions.contracts, ?_, fun _ _ member notNull =>
    optional_without_null world.membership member notNull,
    fun facts store cell stored type sound present passed =>
      learn_sound world decisions.contracts facts store sound cell stored present type passed,
    fun facts store next cell candidate sound committed =>
      assignment_forgets_its_fact world decisions facts store next sound cell candidate committed, ?_⟩
  · intro union remainderType members index present candidate listed built member failed
    refine union_remainder world.membership listed index present built member ?_
    intro passes
    rw [(typeTest_iff _ _ _ _).mpr passes] at failed
    cases failed
  · intro facts store cell candidate sound uncommitted
    rw [assign_uncommitted_keeps_store world decisions store cell candidate uncommitted]
    exact sound

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Validation.Core.Store.Witness

def witnessContracts : ContractDecisions witnessWorld.membership := witnessDecisions.contracts

theorem retained_fact_after_assignment_is_unsound :
    ∃ emptied, assign witnessWorld witnessDecisions Xsh.Validation.Core.Store.Witness.store listAt noWords =
        .committed emptied ∧
      FactsSound witnessWorld (learn (fun _ => none) listAt (.nonEmpty (.primitive .string)))
        (assign witnessWorld witnessDecisions Xsh.Validation.Core.Store.Witness.store listAt words).storeAfter ∧
      ¬ FactsSound witnessWorld (learn (fun _ => none) listAt (.nonEmpty (.primitive .string))) emptied ∧
      FactsSound witnessWorld (forget (learn (fun _ => none) listAt (.nonEmpty (.primitive .string))) listAt)
        emptied := by
  have listPresent : Xsh.Validation.Core.Store.Witness.store listAt = some listCell := by
    simp [Xsh.Validation.Core.Store.Witness.store, cells_differ.symm]
  obtain ⟨emptied, committed⟩ := (assign_committed_iff witnessWorld witnessDecisions
    Xsh.Validation.Core.Store.Witness.store listAt noWords).mpr
    ⟨listCell, listPresent, rfl, listCell.contents.wellTyped⟩
  obtain ⟨copied, copiedCommitted⟩ := (assign_committed_iff witnessWorld witnessDecisions
    Xsh.Validation.Core.Store.Witness.store listAt words).mpr
    ⟨listCell, listPresent, rfl, fun reference present => by simp [words, Value.references, referenceValues] at present,
      by simp [listCell, words, Membership.value, Membership.primitive, Value.eraseNominal]⟩
  obtain ⟨_, _, ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
    assign_committed_effect witnessWorld witnessDecisions _ emptied listAt noWords committed
  obtain ⟨_, _, ⟨copiedCell, copiedAt, _, copiedValue, _⟩, _⟩ :=
    assign_committed_effect witnessWorld witnessDecisions _ copied listAt words copiedCommitted
  refine ⟨emptied, committed, ?_, ?_, ?_⟩
  · rw [copiedCommitted]
    intro cell type known
    by_cases same : cell = listAt
    · subst same
      simp [learn] at known
      subst known
      refine ⟨copiedCell, copiedAt, ?_⟩
      rw [copiedValue]
      simp [words, Membership.value, Membership.primitive, Value.eraseNominal]
    · simp [learn, same] at known
  · intro sound
    obtain ⟨stored, present, member⟩ := sound listAt (.nonEmpty (.primitive .string)) (by simp [learn])
    rw [replacedAt, Option.some.injEq] at present
    subst present
    rw [replacedValue] at member
    simp [noWords, Membership.value, Value.eraseNominal] at member
  · intro cell type known
    by_cases same : cell = listAt
    · simp [forget, same] at known
    · simp [forget, learn, same] at known

end Witness

end Xsh.Validation.Core.Narrow
