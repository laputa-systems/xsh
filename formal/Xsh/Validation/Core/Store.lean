import Xsh.Validation.Core.Widen
import Xsh.Proofs.Core.Env
import Xsh.Proofs.Core.Writes
import Xsh.Validation.Core.Preserve

namespace Xsh.Validation.Core.Store

open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Validation.Core
open Xsh.Validation.Core.Widen
open Xsh.Proofs.Core.Env
open Xsh.Proofs.Core.Writes
open Xsh.Semantics.CoreWrites

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

theorem widening_copy_commits (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (source target : CellId)
    (holder receiver : Cell world) (sourcePresent : store source = some holder)
    (targetPresent : store target = some receiver) (receiverMutable : receiver.mutability = .mutable)
    (view : BaseView holder.contents.typeTag receiver.contents.typeTag) :
    ∃ next, assign world decisions store target holder.contents.value = .committed next ∧
      (∃ copied, next target = some copied ∧ copied.contents.value = holder.contents.value ∧
        copied.contents.typeTag = receiver.contents.typeTag ∧
        copied.contents.value.references = holder.contents.value.references) ∧
      (source ≠ target → next source = store source) := by
  have typed : HasType world receiver.contents.typeTag holder.contents.value :=
    ⟨holder.contents.wellTyped.1, view_member world.membership view _ holder.contents.wellTyped.2⟩
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
theorem validated_store (world : World Declaration Identity Source) (decisions : Decisions world) :
    (∀ (store : Store world) cell candidate (current : Cell world), store cell = some current →
      current.mutability = .mutable → validationTag current.contents.typeTag ≠ none →
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
  refine ⟨?_, fun store source target holder receiver => widening_copy_commits world decisions store
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

theorem validated_path_write (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source) (current : Cell world) (present : store cell = some current)
    (mutableCell : current.mutability = .mutable) :
    ((∃ next, assignPath world decisions raises operate store cell path op right = .committed next) ↔
      ∃ updated, update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value
          path = .ok updated ∧ HasType world current.contents.typeTag updated) ∧
    (∀ updated, update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value
        path = .ok updated → ¬ Membership.value world.membership current.contents.typeTag updated →
      assignPath world decisions raises operate store cell path op right =
        .failed (.typeError current.contents.typeTag updated) store) := by
  refine ⟨?_, ?_⟩
  · rw [assignPath_committed_iff]
    constructor
    · rintro ⟨found, updated, foundPresent, _, updated', typed⟩
      rw [present, Option.some.injEq] at foundPresent
      subst foundPresent
      exact ⟨updated, updated', typed⟩
    · rintro ⟨updated, updated', typed⟩
      exact ⟨current, updated, present, mutableCell, updated', typed⟩
  · intro updated updated' rejected
    unfold assignPath
    simp only [present, mutableCell, updated']
    exact assign_rejects_nonmember world decisions store cell updated current present mutableCell
      (fun typed => rejected typed.2)

theorem compound_add_commits (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (current : Cell world) (element : ValueType Declaration)
    (other result : Value Declaration Identity Source) (present : store cell = some current)
    (mutableCell : current.mutability = .mutable) (validated : current.contents.typeTag = .nonEmpty element)
    (otherMember : Membership.value world.membership (.list element) other)
    (otherReferences : other.ReferencesValid world.references)
    (extended : Xsh.Validation.Core.Preserve.concatenate current.contents.value other = some result)
    (operated : operate .add current.contents.value other = .ok result) :
    ∃ next, assignPath world decisions raises operate store cell [] .add other = .committed next ∧
      ∃ replaced, next cell = some replaced ∧ replaced.contents.value = result ∧
        Membership.value world.membership (.nonEmpty element) replaced.contents.value := by
  rw [assignPath_compound_reads_at_commit world decisions raises operate store cell .add (by decide) other current
    present mutableCell, operated]
  exact Xsh.Validation.Core.Preserve.add_assign_commits world decisions store cell current element other result
    present mutableCell validated otherMember otherReferences extended

/-- Typed stores of validated types under every store-writing form, over
every Core world. `validated_store` covers whole-cell assignment and widening
copies. A field, key or index write or a compound assignment to a cell of any
type, validated or not, commits exactly when the whole updated cell value is a
member with retained references, and otherwise raises `type-error` and keeps
the store, so no write can break a refinement or a nominal schema. A
committed write leaves a member in its cell and every other cell, including
a validated holder that was copied from, unchanged. `+=` on a `NonEmpty` cell
with a list of its elements commits the concatenation that `add_assign_commits`
proves a member. -/
theorem validated_store_writes (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source) :
    (∀ (store : Store world) cell candidate (current : Cell world), store cell = some current →
      current.mutability = .mutable → validationTag current.contents.typeTag ≠ none →
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
    (∀ (store : Store world) cell path op right (current : Cell world), store cell = some current →
      current.mutability = .mutable →
        ((∃ next, assignPath world decisions raises operate store cell path op right = .committed next) ↔
          ∃ updated, update raises (leafUpdate raises operate op right) current.contents.typeTag
              current.contents.value path = .ok updated ∧ HasType world current.contents.typeTag updated) ∧
        (∀ updated, update raises (leafUpdate raises operate op right) current.contents.typeTag
            current.contents.value path = .ok updated →
          ¬ Membership.value world.membership current.contents.typeTag updated →
          assignPath world decisions raises operate store cell path op right =
            .failed (.typeError current.contents.typeTag updated) store)) ∧
    (∀ (store next : Store world) cell path op right source (holder : Cell world),
      assignPath world decisions raises operate store cell path op right = .committed next →
        (∃ replaced, next cell = some replaced ∧
          Membership.value world.membership replaced.contents.typeTag replaced.contents.value) ∧
        (source ≠ cell → store source = some holder → next source = some holder ∧
          Membership.value world.membership holder.contents.typeTag holder.contents.value)) ∧
    (∀ (store : Store world) cell (current : Cell world) element (other result : Value Declaration Identity Source),
      store cell = some current → current.mutability = .mutable → current.contents.typeTag = .nonEmpty element →
      Membership.value world.membership (.list element) other → other.ReferencesValid world.references →
      Xsh.Validation.Core.Preserve.concatenate current.contents.value other = some result →
      operate .add current.contents.value other = .ok result →
        ∃ next, assignPath world decisions raises operate store cell [] .add other = .committed next ∧
          ∃ replaced, next cell = some replaced ∧ replaced.contents.value = result ∧
            Membership.value world.membership (.nonEmpty element) replaced.contents.value) := by
  obtain ⟨whole, copies, _⟩ := validated_store world decisions
  refine ⟨whole, copies, fun store cell path op right current present mutableCell =>
      validated_path_write world decisions raises operate store cell path op right current present mutableCell,
    ?_, fun store cell current element other result present mutableCell validated otherMember otherReferences extended
      operated => compound_add_commits world decisions raises operate store cell current element other result present
        mutableCell validated otherMember otherReferences extended operated⟩
  intro store next cell path op right source holder committed
  obtain ⟨_, _, _, _, ⟨replaced, replacedAt, _, _, _⟩, others⟩ :=
    assignPath_committed_effect world decisions raises operate store next cell path op right committed
  refine ⟨⟨replaced, replacedAt, replaced.contents.wellTyped.2⟩, fun different held => ?_⟩
  rw [others source different]
  exact ⟨held, holder.contents.wellTyped.2⟩

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
  obtain ⟨copied, copiedCommitted, ⟨copiedCell, copiedAt, _, copiedType, _⟩, keepsHolder⟩ :=
    widening_copy_commits witnessWorld witnessDecisions store nonEmptyAt listAt nonEmptyCell listCell
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

def nameList : ValueType Unit := .record (.rowField "names" (.nonEmpty (.primitive .string)) .rowEmpty)

def roster : Cell witnessWorld :=
  ⟨⟨nameList, .record [("names", words)], ⟨fun _ _ => trivial, by
    simp [nameList, words, Membership.value, Membership.row, Membership.recordFields, Membership.primitive,
      Value.eraseNominal, List.lookup]⟩⟩, .mutable⟩

def rosterStore : Store witnessWorld := fun cell => if cell = nonEmptyAt then some roster else none

def writeRaises : Raises Unit Unit Unit where
  indexOutOfRange := .scalar (.string "index-out-of-range")
  schema := .scalar (.string "schema")
  mapMissing := .scalar (.string "map-missing")
  envValue := .scalar (.string "env-value")

def concatenation : Operate Unit Unit Unit := fun _ current right =>
  match Xsh.Validation.Core.Preserve.concatenate current right with
  | some result => .ok result
  | none => .error (.scalar (.string "type-error"))

theorem emptying_a_validated_field_is_a_type_error :
    update writeRaises (leafUpdate writeRaises concatenation .set noWords) nameList (.record [("names", words)])
        [.field "names"] = .ok (.record [("names", noWords)]) ∧
      ¬ Membership.value witnessWorld.membership nameList (.record [("names", noWords)]) ∧
      assignPath witnessWorld witnessDecisions writeRaises concatenation rosterStore nonEmptyAt [.field "names"] .set
        noWords = .failed (.typeError nameList (.record [("names", noWords)])) rosterStore := by
  have updated : update writeRaises (leafUpdate writeRaises concatenation .set noWords) nameList
      (.record [("names", words)]) [.field "names"] = .ok (.record [("names", noWords)]) := by
    simp [update, recordRow, nameList, rowEntries, Value.eraseNominal, List.lookup, leafUpdate, Except.map,
      rebuild, replaceField]
  have rejected : ¬ Membership.value witnessWorld.membership nameList (.record [("names", noWords)]) := by
    simp [nameList, noWords, Membership.value, Membership.row, Membership.recordFields, Value.eraseNominal,
      List.lookup]
  refine ⟨updated, rejected, ?_⟩
  have present : rosterStore nonEmptyAt = some roster := by simp [rosterStore]
  exact (validated_path_write witnessWorld witnessDecisions writeRaises concatenation rosterStore nonEmptyAt
    [.field "names"] .set noWords roster present rfl).2 _ updated rejected

theorem add_assign_extends_a_nonempty_cell :
    ∃ next, assignPath witnessWorld witnessDecisions writeRaises concatenation store nonEmptyAt [] .add words =
        .committed next ∧
      ∃ replaced, next nonEmptyAt = some replaced ∧
        replaced.contents.value = .list [.scalar (.string "a"), .scalar (.string "a")] := by
  have extended : Xsh.Validation.Core.Preserve.concatenate words words =
      some (.list [.scalar (.string "a"), .scalar (.string "a")]) := by
    simp [Xsh.Validation.Core.Preserve.concatenate, Xsh.Validation.Core.Preserve.listItems, words, Value.eraseNominal]
  obtain ⟨next, committed, replaced, replacedAt, replacedValue, _⟩ :=
    compound_add_commits witnessWorld witnessDecisions writeRaises concatenation store nonEmptyAt nonEmptyCell
      (.primitive .string) words _ (by simp [store]) rfl rfl
      (by simp [words, Membership.value, Membership.primitive, Value.eraseNominal]) (fun _ _ => trivial) extended
      (by simp [concatenation, nonEmptyCell, extended])
  exact ⟨next, committed, replaced, replacedAt, replacedValue⟩

end Witness

end Xsh.Validation.Core.Store
