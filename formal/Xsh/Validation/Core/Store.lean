import Xsh.Validation.Core.Widen
import Xsh.Proofs.Core.Env
import Xsh.Proofs.Core.Writes
import Xsh.Validation.Core.Preserve
import Xsh.Validation.Core.WellFormed

namespace Xsh.Validation.Core.Store

open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Validation.Core
open Xsh.Validation.Core.Widen
open Xsh.Proofs.Core.Env
open Xsh.Proofs.Core.Writes
open Xsh.Semantics.CoreWrites
open Xsh.Validation.Core.WellFormed

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

omit [DecidableEq Declaration] in
theorem field_lookup_mem {α : Type} : ∀ {fields : List (String × α)} {name : String} {found : α},
    fields.lookup name = some found → (name, found) ∈ fields
  | [], _, _, present => by simp at present
  | (other, value) :: rest, name, found, present => by
    by_cases same : name = other
    · subst same
      simp [List.lookup] at present
      simp [present]
    · have differentNames : (name == other) = false := by simpa using same
      simp only [List.lookup, differentNames] at present
      exact List.mem_cons_of_mem _ (field_lookup_mem present)

omit [DecidableEq Declaration] in
theorem referenceValues_mem {reference : AnyReference Identity} :
    ∀ items : List (Value Declaration Identity Source),
      reference ∈ referenceValues items ↔ ∃ item ∈ items, reference ∈ item.references
  | [] => by simp [referenceValues]
  | item :: rest => by simp [referenceValues, referenceValues_mem rest]

omit [DecidableEq Declaration] in
theorem referenceFields_mem {reference : AnyReference Identity} :
    ∀ fields : List (String × Value Declaration Identity Source),
      reference ∈ referenceFields fields ↔ ∃ field ∈ fields, reference ∈ field.2.references
  | [] => by simp [referenceFields]
  | (name, item) :: rest => by simp [referenceFields, referenceFields_mem rest]

omit [DecidableEq Declaration] in
theorem replaceField_mem (name : String) (replacement : Value Declaration Identity Source) :
    ∀ {fields : List (String × Value Declaration Identity Source)} {field},
      field ∈ replaceField name replacement fields → field ∈ fields ∨ field = (name, replacement)
  | [], _, present => by simp [replaceField] at present
  | (other, value) :: rest, field, present => by
    by_cases same : other = name
    · subst same
      simp only [replaceField, ite_true, List.mem_cons] at present
      rcases present with here | later
      · exact Or.inr here
      · exact Or.inl (List.mem_cons_of_mem _ later)
    · simp only [replaceField, same, ite_false, List.mem_cons] at present
      rcases present with here | later
      · exact Or.inl (by simp [here])
      · rcases replaceField_mem name replacement later with kept | replaced
        · exact Or.inl (List.mem_cons_of_mem _ kept)
        · exact Or.inr replaced

omit [DecidableEq Declaration] in
theorem lookup_replaceField_other' (name other : String) (replacement : Value Declaration Identity Source)
    (different : other ≠ name) :
    ∀ fields : List (String × Value Declaration Identity Source),
      (replaceField name replacement fields).lookup other = fields.lookup other
  | [] => rfl
  | (key, value) :: rest => by
    by_cases same : key = name
    · subst same
      have differentNames : (other == key) = false := by simpa using different
      simp [replaceField, List.lookup, differentNames]
    · simp only [replaceField, same, ite_false, List.lookup]
      split <;> simp_all [lookup_replaceField_other' name other replacement different rest]

omit [DecidableEq Declaration] in
theorem row_congr (world : MembershipWorld Declaration Identity Source) :
    ∀ (row : RecordType Declaration) (first second : List (String × Value Declaration Identity Source)),
      (∀ name ∈ rowNames row, first.lookup name = second.lookup name) →
        Membership.row world row first → Membership.row world row second
  | .variable id, _, _, _, _ => nomatch id
  | .rowEmpty, _, _, _, _ => by simp [Membership.row]
  | .rowField name type rest, first, second, same, member => by
    simp only [Membership.row] at member ⊢
    rw [← same name (by simp [rowNames])]
    exact ⟨member.1, row_congr world rest first second (fun other listed => same other (by simp [rowNames, listed]))
      member.2⟩

omit [DecidableEq Declaration] in
theorem row_replace_member (world : MembershipWorld Declaration Identity Source) :
    ∀ (row : RecordType Declaration) (fields : List (String × Value Declaration Identity Source)) name type
      (replacement : Value Declaration Identity Source),
      RowWellFormed row → Membership.row world row fields → (rowEntries row).lookup name = some type →
        Membership.value world type replacement → Membership.row world row (replaceField name replacement fields)
  | .variable id, _, _, _, _, _, _, _, _ => nomatch id
  | .rowEmpty, _, _, _, _, _, _, found, _ => by simp [rowEntries] at found
  | .rowField other fieldType rest, fields, name, type, replacement, wellFormed, member, found, replacementMember => by
    simp only [RowWellFormed] at wellFormed
    simp only [Membership.row] at member ⊢
    by_cases same : other = name
    · subst same
      simp only [rowEntries, List.lookup, beq_self_eq_true, Option.some.injEq] at found
      subst found
      cases present : fields.lookup other with
      | none => simp [present] at member
      | some found =>
        rw [Xsh.Proofs.Core.Writes.lookup_replaceField_same other replacement present]
        refine ⟨replacementMember, ?_⟩
        exact row_congr world rest fields _ (fun listed inside =>
          (lookup_replaceField_other' other listed replacement (fun equal => wellFormed.1 (equal ▸ inside)) fields).symm)
          member.2
    · have differentNames : (name == other) = false := by simpa using Ne.symm same
      simp only [rowEntries, List.lookup, differentNames] at found
      rw [lookup_replaceField_other' name other replacement same fields]
      exact ⟨member.1, row_replace_member world rest fields name type replacement wellFormed.2.2 member.2 found
        replacementMember⟩

theorem validated_path_write (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source) (current : Cell world) (present : store cell = some current)
    (mutableCell : current.mutability = .mutable) :
    ((∃ next, assignPath world decisions raises operate store cell path op right = .committed next) ↔
      ∃ updated, update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
          current.contents.value path = .ok updated ∧ HasType world current.contents.typeTag updated) ∧
    (∀ updated, update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
        current.contents.value path = .ok updated →
      ¬ Membership.value world.membership current.contents.typeTag updated →
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

theorem committed_write_value (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source) (current : Cell world) (present : store cell = some current)
    (mutableCell : current.mutability = .mutable) (updated : Value Declaration Identity Source)
    (updated' : update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
      current.contents.value path = .ok updated)
    (typed : HasType world current.contents.typeTag updated) :
    ∃ next, assignPath world decisions raises operate store cell path op right = .committed next ∧
      ∃ replaced, next cell = some replaced ∧ replaced.contents.value = updated := by
  obtain ⟨next, committed⟩ := ((validated_path_write world decisions raises operate store cell path op right current
    present mutableCell).1).mpr ⟨updated, updated', typed⟩
  obtain ⟨found, written, foundPresent, written', ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
    assignPath_committed_effect world decisions raises operate store next cell path op right committed
  rw [present, Option.some.injEq] at foundPresent
  subst foundPresent
  rw [updated', Except.ok.injEq] at written'
  subst written'
  exact ⟨next, committed, replaced, replacedAt, replacedValue⟩

theorem index_write_preserves_nonEmpty (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (current : Cell world) (element : ValueType Declaration)
    (items : List (Value Declaration Identity Source)) (position : Int) (item : Value Declaration Identity Source)
    (present : store cell = some current) (mutableCell : current.mutability = .mutable)
    (validated : current.contents.typeTag = .nonEmpty element)
    (listed : listValues current.contents.value = some items) (inRange : 0 ≤ position ∧ position < items.length)
    (itemMember : Membership.value world.membership element item)
    (itemReferences : item.ReferencesValid world.references) :
    ∃ next, assignPath world decisions raises operate store cell [.index position] .set item = .committed next ∧
      ∃ replaced, next cell = some replaced ∧
        Xsh.Validation.Core.Preserve.replaceAt current.contents.value position.toNat item =
          some replaced.contents.value ∧
        Membership.value world.membership (.nonEmpty element) replaced.contents.value := by
  have currentMember : Membership.value world.membership (.nonEmpty element) current.contents.value :=
    validated ▸ current.contents.wellTyped.2
  have preservedListed : Xsh.Validation.Core.Preserve.listItems current.contents.value = some items := by
    unfold listValues at listed
    unfold Xsh.Validation.Core.Preserve.listItems
    exact listed
  have replaced : Xsh.Validation.Core.Preserve.replaceAt current.contents.value position.toNat item =
      some (.list (items.set position.toNat item)) := by
    simp [Xsh.Validation.Core.Preserve.replaceAt, preservedListed]
  have resultMember := (Xsh.Validation.Core.Preserve.preserving_operations world.membership).2.2.2.2.1 element _ position.toNat
    item _ currentMember itemMember replaced
  have updated' : update world.membership raises (leafUpdate raises operate .set item) current.contents.typeTag
      current.contents.value [.index position] = .ok (.list (items.set position.toNat item)) := by
    rw [validated]
    simp [update, elementType, listed, inRange, leafUpdate, Except.map]
  have references : (Value.list (items.set position.toNat item) : Value Declaration Identity Source).ReferencesValid
      world.references := by
    intro reference inside
    simp only [Value.references] at inside
    obtain ⟨found, foundInside, foundReference⟩ := (referenceValues_mem _).mp inside
    rcases List.mem_or_eq_of_mem_set foundInside with kept | same
    · unfold listValues at listed
      split at listed
      · rename_i values erased
        simp only [Option.some.injEq] at listed
        subst listed
        apply current.contents.wellTyped.1
        rw [← Value.references_eraseNominal, erased]
        simp only [Value.references]
        exact (referenceValues_mem _).mpr ⟨found, kept, foundReference⟩
      · rename_i paths erased
        simp only [Option.some.injEq] at listed
        subst listed
        obtain ⟨path, _, rfl⟩ := List.mem_map.mp kept
        simp [Value.references] at foundReference
      · simp at listed
    · exact itemReferences reference (same ▸ foundReference)
  obtain ⟨next, committed, written, writtenAt, writtenValue⟩ := committed_write_value world decisions raises operate
    store cell [.index position] .set item current present mutableCell _ updated'
    ⟨references, validated ▸ resultMember⟩
  exact ⟨next, committed, written, writtenAt, by rw [writtenValue]; exact replaced, writtenValue ▸ resultMember⟩

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

theorem concatenate_references {left right result : Value Declaration Identity Source}
    (policy : ReferencePolicy Identity) (leftValid : left.ReferencesValid policy) (rightValid : right.ReferencesValid policy)
    (extended : Xsh.Validation.Core.Preserve.concatenate left right = some result) : result.ReferencesValid policy := by
  have itemsValid : ∀ (candidate : Value Declaration Identity Source) items,
      candidate.ReferencesValid policy → Xsh.Validation.Core.Preserve.listItems candidate = some items →
        ∀ item ∈ items, item.ReferencesValid policy := by
    intro candidate items valid listed item inside reference present
    unfold Xsh.Validation.Core.Preserve.listItems at listed
    split at listed
    · rename_i values erased
      simp only [Option.some.injEq] at listed
      subst listed
      apply valid
      rw [← Value.references_eraseNominal, erased]
      simp only [Value.references]
      exact (referenceValues_mem _).mpr ⟨item, inside, present⟩
    · rename_i paths erased
      simp only [Option.some.injEq] at listed
      subst listed
      obtain ⟨path, _, rfl⟩ := List.mem_map.mp inside
      simp [Value.references] at present
    · simp at listed
  unfold Xsh.Validation.Core.Preserve.concatenate at extended
  split at extended
  · rename_i first second firstListed secondListed
    simp only [Option.some.injEq] at extended
    subst extended
    intro reference inside
    simp only [Value.references] at inside
    obtain ⟨item, itemInside, itemReference⟩ := (referenceValues_mem _).mp inside
    rcases List.mem_append.mp itemInside with early | late
    · exact itemsValid left first leftValid firstListed item early reference itemReference
    · exact itemsValid right second rightValid secondListed item late reference itemReference
  · simp at extended

theorem field_add_preserves_nonEmpty (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (current : Cell world) (row : RecordType Declaration) (name : String)
    (element : ValueType Declaration) (fields : List (String × Value Declaration Identity Source))
    (field other result : Value Declaration Identity Source) (present : store cell = some current)
    (mutableCell : current.mutability = .mutable) (typed : current.contents.typeTag = .record row)
    (wellFormed : RowWellFormed row) (declared : (rowEntries row).lookup name = some (.nonEmpty element))
    (record : current.contents.value.eraseNominal = .record fields) (found : fields.lookup name = some field)
    (otherMember : Membership.value world.membership (.list element) other)
    (otherReferences : other.ReferencesValid world.references)
    (extended : Xsh.Validation.Core.Preserve.concatenate field other = some result)
    (operated : operate .add field other = .ok result) :
    ∃ next, assignPath world decisions raises operate store cell [.field name] .add other = .committed next ∧
      ∃ replaced, next cell = some replaced ∧ readPath replaced.contents.value [.field name] = some result ∧
        Membership.value world.membership (.nonEmpty element) result := by
  have currentMember : Membership.value world.membership (.record row) current.contents.value :=
    typed ▸ current.contents.wellTyped.2
  have fielded : Membership.recordFields world.membership current.contents.value = some fields := by
    simp [Membership.recordFields, record]
  have rowMember : Membership.row world.membership row fields := by
    simpa [Membership.value, fielded] using currentMember
  obtain ⟨found', foundAt, fieldMember⟩ :=
    Xsh.Proofs.Core.Writes.row_lookup_member world.membership row fields name _ rowMember declared
  rw [found, Option.some.injEq] at foundAt
  subst foundAt
  have resultMember := (Xsh.Validation.Core.Preserve.preserving_operations world.membership).2.1 element field other
    result fieldMember otherMember extended
  have updated' : update world.membership raises (leafUpdate raises operate .add other) current.contents.typeTag
      current.contents.value [.field name] = .ok (.record (replaceField name result fields)) := by
    rw [typed]
    simp [update, recordRow, fielded, found, declared, leafUpdate, operated, Except.map]
  have fieldReferences : field.ReferencesValid world.references := by
    intro reference inside
    apply current.contents.wellTyped.1
    rw [← Value.references_eraseNominal, record]
    simp only [Value.references]
    exact (referenceFields_mem _).mpr ⟨(name, field), field_lookup_mem found, inside⟩
  have resultReferences := concatenate_references world.references fieldReferences otherReferences extended
  have recordReferences : (Value.record (replaceField name result fields) : Value Declaration Identity Source).ReferencesValid
      world.references := by
    intro reference inside
    simp only [Value.references] at inside
    obtain ⟨entry, entryInside, entryReference⟩ := (referenceFields_mem _).mp inside
    rcases replaceField_mem name result entryInside with kept | same
    · apply current.contents.wellTyped.1
      rw [← Value.references_eraseNominal, record]
      simp only [Value.references]
      exact (referenceFields_mem _).mpr ⟨entry, kept, entryReference⟩
    · subst same
      exact resultReferences reference entryReference
  have recordMember : Membership.value world.membership (.record row) (.record (replaceField name result fields)) := by
    simp only [Membership.value, Membership.recordFields, Value.eraseNominal]
    exact row_replace_member world.membership row fields name _ result wellFormed rowMember declared resultMember
  obtain ⟨next, committed, written, writtenAt, writtenValue⟩ := committed_write_value world decisions raises operate
    store cell [.field name] .add other current present mutableCell _ updated'
    ⟨recordReferences, typed ▸ recordMember⟩
  refine ⟨next, committed, written, writtenAt, ?_, resultMember⟩
  rw [writtenValue]
  simp [readPath, Value.eraseNominal, Xsh.Proofs.Core.Writes.lookup_replaceField_same name result found]

/-- Typed stores of validated types under every store-writing form, over
every Core world. Whole-cell assignment and widening copies are
`validated_store`. A field, key or index write or a compound assignment to a
cell of any type commits exactly when the whole updated cell value is a member
with retained references, and otherwise raises `type-error` and keeps the
store, so no write breaks a refinement or a nominal schema; a commit changes no
other cell, including a validated holder. `names[i] = v` on a `NonEmpty` cell
commits VAL-PRESERVE's `replaceAt` and keeps a `NonEmpty`. Where `+=` means
list concatenation, `+=` on a `NonEmpty` cell commits through
`add_assign_commits`, and `+=` into a `NonEmpty` field of a well-formed record
cell commits and keeps the field `NonEmpty`. -/
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
          ∃ updated, update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
              current.contents.value path = .ok updated ∧ HasType world current.contents.typeTag updated) ∧
        (∀ updated, update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
            current.contents.value path = .ok updated →
          ¬ Membership.value world.membership current.contents.typeTag updated →
          assignPath world decisions raises operate store cell path op right =
            .failed (.typeError current.contents.typeTag updated) store)) ∧
    (∀ (store next : Store world) cell path op right source (holder : Cell world),
      assignPath world decisions raises operate store cell path op right = .committed next →
        source ≠ cell → store source = some holder → next source = some holder) ∧
    (∀ (store : Store world) cell (current : Cell world) element items (position : Int) item,
      store cell = some current → current.mutability = .mutable → current.contents.typeTag = .nonEmpty element →
      listValues current.contents.value = some items → (0 ≤ position ∧ position < items.length) →
      Membership.value world.membership element item → item.ReferencesValid world.references →
        ∃ next, assignPath world decisions raises operate store cell [.index position] .set item = .committed next ∧
          ∃ replaced, next cell = some replaced ∧
            Xsh.Validation.Core.Preserve.replaceAt current.contents.value position.toNat item =
              some replaced.contents.value ∧
            Membership.value world.membership (.nonEmpty element) replaced.contents.value) ∧
    (∀ (store : Store world) cell (current : Cell world) element (other result : Value Declaration Identity Source),
      store cell = some current → current.mutability = .mutable → current.contents.typeTag = .nonEmpty element →
      Membership.value world.membership (.list element) other → other.ReferencesValid world.references →
      Xsh.Validation.Core.Preserve.concatenate current.contents.value other = some result →
      operate .add current.contents.value other = .ok result →
        ∃ next, assignPath world decisions raises operate store cell [] .add other = .committed next ∧
          ∃ replaced, next cell = some replaced ∧ replaced.contents.value = result ∧
            Membership.value world.membership (.nonEmpty element) replaced.contents.value) ∧
    (∀ (store : Store world) cell (current : Cell world) row name element fields
      (field other result : Value Declaration Identity Source),
      store cell = some current → current.mutability = .mutable → current.contents.typeTag = .record row →
      RowWellFormed row → (rowEntries row).lookup name = some (.nonEmpty element) →
      current.contents.value.eraseNominal = .record fields → fields.lookup name = some field →
      Membership.value world.membership (.list element) other → other.ReferencesValid world.references →
      Xsh.Validation.Core.Preserve.concatenate field other = some result → operate .add field other = .ok result →
        ∃ next, assignPath world decisions raises operate store cell [.field name] .add other = .committed next ∧
          ∃ replaced, next cell = some replaced ∧ readPath replaced.contents.value [.field name] = some result ∧
            Membership.value world.membership (.nonEmpty element) result) := by
  obtain ⟨whole, copies, _⟩ := validated_store world decisions
  refine ⟨whole, copies, fun store cell path op right current present mutableCell =>
      validated_path_write world decisions raises operate store cell path op right current present mutableCell,
    ?_, fun store cell current element items position item present mutableCell validated listed inRange itemMember
      itemReferences => index_write_preserves_nonEmpty world decisions raises operate store cell current element items
        position item present mutableCell validated listed inRange itemMember itemReferences,
    fun store cell current element other result present mutableCell validated otherMember otherReferences extended
      operated => compound_add_commits world decisions raises operate store cell current element other result present
        mutableCell validated otherMember otherReferences extended operated,
    fun store cell current row name element fields field other result present mutableCell typed wellFormed declared
      record found otherMember otherReferences extended operated => field_add_preserves_nonEmpty world decisions raises
        operate store cell current row name element fields field other result present mutableCell typed wellFormed
        declared record found otherMember otherReferences extended operated⟩
  intro store next cell path op right source holder committed different held
  obtain ⟨_, _, _, _, _, others⟩ :=
    assignPath_committed_effect world decisions raises operate store next cell path op right committed
  rw [others source different]
  exact held

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
  missingKey := .scalar (.string "missing-field")
  schema := .scalar (.string "schema")
  nulArgv := .scalar (.string "nul-argv")

def concatenation : Operate Unit Unit Unit := fun _ current right =>
  match Xsh.Validation.Core.Preserve.concatenate current right with
  | some result => .ok result
  | none => .error (.scalar (.string "type-error"))

theorem model_rejects_emptying_a_validated_field :
    assignPath witnessWorld witnessDecisions writeRaises concatenation rosterStore nonEmptyAt [.field "names"] .set
      noWords = .failed (.typeError nameList (.record [("names", noWords)])) rosterStore := by
  have updated : update witnessWorld.membership writeRaises (leafUpdate writeRaises concatenation .set noWords)
      nameList (.record [("names", words)]) [.field "names"] = .ok (.record [("names", noWords)]) := by
    simp [update, recordRow, nameList, rowEntries, Membership.recordFields, Value.eraseNominal, List.lookup,
      leafUpdate, Except.map, replaceField]
  have rejected : ¬ Membership.value witnessWorld.membership nameList (.record [("names", noWords)]) := by
    simp [nameList, noWords, Membership.value, Membership.row, Membership.recordFields, Value.eraseNominal,
      List.lookup]
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

theorem field_add_assign_keeps_a_nonempty_field :
    ∃ next, assignPath witnessWorld witnessDecisions writeRaises concatenation rosterStore nonEmptyAt
        [.field "names"] .add words = .committed next ∧
      ∃ replaced, next nonEmptyAt = some replaced ∧
        readPath replaced.contents.value [.field "names"] = some (.list [.scalar (.string "a"), .scalar (.string "a")]) := by
  have extended : Xsh.Validation.Core.Preserve.concatenate words words =
      some (.list [.scalar (.string "a"), .scalar (.string "a")]) := by
    simp [Xsh.Validation.Core.Preserve.concatenate, Xsh.Validation.Core.Preserve.listItems, words, Value.eraseNominal]
  obtain ⟨next, committed, replaced, replacedAt, readBack, _⟩ :=
    field_add_preserves_nonEmpty witnessWorld witnessDecisions writeRaises concatenation rosterStore nonEmptyAt roster
      (.rowField "names" (.nonEmpty (.primitive .string)) .rowEmpty) "names" (.primitive .string) [("names", words)]
      words words _ (by simp [rosterStore]) rfl rfl (by simp [RowWellFormed, rowNames, WellFormed]) (by simp [rowEntries])
      (by simp [roster, Value.eraseNominal]) (by simp)
      (by simp [words, Membership.value, Membership.primitive, Value.eraseNominal]) (fun _ _ => trivial) extended
      (by simp [concatenation, extended])
  exact ⟨next, committed, replaced, replacedAt, readBack⟩

theorem index_write_keeps_a_nonempty_cell :
    ∃ next, assignPath witnessWorld witnessDecisions writeRaises concatenation store nonEmptyAt [.index 0] .set
        (.scalar (.string "b")) = .committed next ∧
      ∃ replaced, next nonEmptyAt = some replaced ∧ replaced.contents.value = .list [.scalar (.string "b")] := by
  obtain ⟨next, committed, replaced, replacedAt, replacedAtIndex, _⟩ :=
    index_write_preserves_nonEmpty witnessWorld witnessDecisions writeRaises concatenation store nonEmptyAt nonEmptyCell
      (.primitive .string) [.scalar (.string "a")] 0 (.scalar (.string "b")) (by simp [store]) rfl rfl
      (by simp [nonEmptyCell, words, listValues, Value.eraseNominal]) (by decide)
      (by simp [Membership.value, Membership.primitive, Value.eraseNominal]) (fun _ _ => trivial)
  refine ⟨next, committed, replaced, replacedAt, ?_⟩
  simp [Xsh.Validation.Core.Preserve.replaceAt, Xsh.Validation.Core.Preserve.listItems, nonEmptyCell, words,
    Value.eraseNominal] at replacedAtIndex
  exact replacedAtIndex.symm

end Witness

end Xsh.Validation.Core.Store
