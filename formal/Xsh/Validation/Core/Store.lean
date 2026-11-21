import Xsh.Validation.Core.Widen
import Xsh.Proofs.Core.Env

namespace Xsh.Validation.Core.Store

open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Validation.Core
open Xsh.Validation.Core.Widen
open Xsh.Proofs.Core.Env

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

theorem widening_copy_commits (world : World Declaration Identity Source) (decisions : Decisions world)
    (closed : ViewClosed world.membership) (store : Store world) (source target : CellId)
    (holder receiver : Cell world) (sourcePresent : store source = some holder)
    (targetPresent : store target = some receiver) (receiverMutable : receiver.mutability = .mutable)
    (view : BaseView holder.contents.typeTag receiver.contents.typeTag) :
    ∃ next, assign world decisions store target holder.contents.value = .committed next ∧
      (∃ copied, next target = some copied ∧ copied.contents.value = holder.contents.value ∧
        copied.contents.typeTag = receiver.contents.typeTag ∧
        copied.contents.value.references = holder.contents.value.references) ∧
      (source ≠ target → next source = store source) := by
  have typed : HasType world receiver.contents.typeTag holder.contents.value :=
    ⟨holder.contents.wellTyped.1, view_member world.membership closed view _ holder.contents.wellTyped.2⟩
  obtain ⟨next, committed⟩ := (assign_committed_iff world decisions store target holder.contents.value).mpr
    ⟨receiver, targetPresent, receiverMutable, typed⟩
  obtain ⟨current, present, ⟨replaced, replacedAt, replacedType, replacedValue, _⟩, others⟩ :=
    assign_committed_effect world decisions store next target holder.contents.value committed
  rw [targetPresent, Option.some.injEq] at present
  subst present
  exact ⟨next, committed, ⟨replaced, replacedAt, replacedValue, replacedType, by rw [replacedValue]⟩,
    fun different => others source different⟩

/-- Typed stores of validated types over every Core world. A write to a
cell whose declared type is validated commits exactly for a member with
retained references; any other candidate raises `type-error` and keeps the
store. A validated value copied into a cell of a base view commits, keeps its
value and therefore its resource references, and leaves the holder's cell
unchanged. A later committed write to the wider cell changes no other cell,
so the validated holder keeps a member of its validated type. -/
theorem validated_store (world : World Declaration Identity Source) (decisions : Decisions world)
    (closed : ViewClosed world.membership) :
    (∀ (store : Store world) cell candidate (current : Cell world), store cell = some current →
      current.mutability = .mutable → validation current.contents.typeTag ≠ none →
        ((∃ next, assign world decisions store cell candidate = .committed next) ↔
          candidate.ReferencesValid world.references ∧
            Membership.value world.membership current.contents.typeTag candidate) ∧
        (¬ Membership.value world.membership current.contents.typeTag candidate →
          assign world decisions store cell candidate =
            .failed (.typeError current.contents.typeTag candidate) store)) ∧
    (∀ (store : Store world) source target (holder receiver : Cell world), store source = some holder →
      store target = some receiver → receiver.mutability = .mutable →
      BaseView holder.contents.typeTag receiver.contents.typeTag →
        ∃ next, assign world decisions store target holder.contents.value = .committed next ∧
          (∃ copied, next target = some copied ∧ copied.contents.value = holder.contents.value ∧
            copied.contents.typeTag = receiver.contents.typeTag ∧
            copied.contents.value.references = holder.contents.value.references) ∧
          (source ≠ target → next source = store source)) ∧
    (∀ (store next : Store world) source target candidate (holder : Cell world), source ≠ target →
      store source = some holder → assign world decisions store target candidate = .committed next →
        next source = some holder ∧
          Membership.value world.membership holder.contents.typeTag holder.contents.value) := by
  refine ⟨?_, fun store source target holder receiver => widening_copy_commits world decisions closed store
    source target holder receiver, ?_⟩
  · intro store cell candidate current present mutableCell _
    refine ⟨?_, fun rejected => assign_rejects_nonmember world decisions store cell candidate current present
      mutableCell (fun typed => rejected typed.2)⟩
    rw [assign_committed_iff]
    constructor
    · rintro ⟨found, foundPresent, _, typed⟩
      rw [present, Option.some.injEq] at foundPresent
      subst foundPresent
      exact typed
    · intro typed
      exact ⟨current, present, mutableCell, typed⟩
  · intro store next source target candidate holder different present committed
    obtain ⟨_, _, _, others⟩ := assign_committed_effect world decisions store next target candidate committed
    rw [others source different]
    exact ⟨present, holder.contents.wellTyped.2⟩

namespace Witness

open Xsh.Proofs.Core.Env.Witness

def words : WitnessValue := .list [.scalar (.string "a")]
def noWords : WitnessValue := .list []

def nonEmptyCell : Cell witnessWorld :=
  ⟨⟨.nonEmpty (.primitive .string), words, ⟨fun reference present => by simp [words, Value.references,
    referenceValues] at present, by simp [words, Membership.value, Membership.primitive, Value.eraseNominal]⟩⟩,
    .mutable⟩

def listCell : Cell witnessWorld :=
  ⟨⟨.list (.primitive .string), noWords, ⟨fun reference present => by simp [noWords, Value.references,
    referenceValues] at present, by simp [noWords, Membership.value, Value.eraseNominal]⟩⟩, .mutable⟩

def nonEmptyAt : CellId := counterCell
def listAt : CellId := otherCell

def store : Store witnessWorld := fun cell =>
  if cell = nonEmptyAt then some nonEmptyCell else if cell = listAt then some listCell else none

theorem cells_differ : nonEmptyAt ≠ listAt := by decide

theorem widening_then_emptying_keeps_the_validated_holder :
    ∃ copied emptied, assign witnessWorld witnessDecisions store listAt words = .committed copied ∧
      assign witnessWorld witnessDecisions copied listAt noWords = .committed emptied ∧
      emptied nonEmptyAt = some nonEmptyCell ∧
      assign witnessWorld witnessDecisions store nonEmptyAt noWords =
        .failed (.typeError (.nonEmpty (.primitive .string)) noWords) store := by
  have openClosed : ViewClosed witnessWorld.membership :=
    ⟨fun _ _ _ _ found => found.elim, fun _ _ _ _ found => found.elim⟩
  obtain ⟨copied, copiedCommitted, ⟨copiedCell, copiedAt, _, copiedType, _⟩, keepsHolder⟩ :=
    widening_copy_commits witnessWorld witnessDecisions openClosed store nonEmptyAt listAt nonEmptyCell listCell
      (by simp [store]) (by simp [store, cells_differ.symm]) rfl (.validated rfl (.refl _))
  have copiedMutable : copiedCell.mutability = .mutable := by
    obtain ⟨_, present, ⟨replaced, replacedAt, _, _, sameMutability⟩, _⟩ :=
      assign_committed_effect witnessWorld witnessDecisions store copied listAt words copiedCommitted
    rw [copiedAt, Option.some.injEq] at replacedAt
    subst replacedAt
    simp only [store, cells_differ.symm, if_false, if_true, Option.some.injEq] at present
    subst present
    exact sameMutability
  obtain ⟨emptied, emptiedCommitted⟩ := (assign_committed_iff witnessWorld witnessDecisions copied listAt noWords).mpr
    ⟨copiedCell, copiedAt, copiedMutable, fun reference present => by simp [noWords, Value.references,
      referenceValues] at present, by rw [copiedType]; simp [listCell, noWords, Membership.value,
      Value.eraseNominal]⟩
  obtain ⟨_, _, _, others⟩ := assign_committed_effect witnessWorld witnessDecisions copied emptied listAt noWords
    emptiedCommitted
  refine ⟨copied, emptied, copiedCommitted, emptiedCommitted, ?_, ?_⟩
  · rw [others nonEmptyAt cells_differ, keepsHolder cells_differ]
    simp [store]
  · exact assign_rejects_nonmember witnessWorld witnessDecisions store nonEmptyAt noWords nonEmptyCell
      (by simp [store]) rfl (fun typed => by
        have member := typed.2
        simp [nonEmptyCell, noWords, Membership.value, Value.eraseNominal] at member)

end Witness

end Xsh.Validation.Core.Store
