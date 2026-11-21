import Xsh.Proofs.Core.Env
import Xsh.Semantics.CoreWrites

namespace Xsh.Proofs.Core.Writes

open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Proofs.Core.Env

variable {Declaration Identity Source : Type}

section Values

theorem lookup_replaceField_same (name : String) (replacement : Value Declaration Identity Source) :
    ∀ {fields : List (String × Value Declaration Identity Source)} {found : Value Declaration Identity Source},
      fields.lookup name = some found → (replaceField name replacement fields).lookup name = some replacement
  | [], _, present => by simp at present
  | (other, value) :: rest, found, present => by
    by_cases same : other = name
    · subst same
      simp [replaceField]
    · have differentNames : (name == other) = false := by simpa using Ne.symm same
      simp only [List.lookup, differentNames] at present
      simp only [replaceField, same, ite_false, List.lookup, differentNames]
      exact lookup_replaceField_same name replacement present

theorem lookup_replaceEntry_same (key : MapKey) (replacement : Value Declaration Identity Source) :
    ∀ {entries : List (MapKey × Value Declaration Identity Source)} {found : Value Declaration Identity Source},
      entries.lookup key = some found → (replaceEntry key replacement entries).lookup key = some replacement
  | [], _, present => by simp at present
  | (other, value) :: rest, found, present => by
    by_cases same : other = key
    · subst same
      simp [replaceEntry]
    · have differentKeys : (key == other) = false := by simpa using Ne.symm same
      simp only [List.lookup, differentKeys] at present
      simp only [replaceEntry, same, ite_false, List.lookup, differentKeys]
      exact lookup_replaceEntry_same key replacement present

theorem lookup_insertOrdered_absent (key : MapKey) (inserted : Value Declaration Identity Source) :
    ∀ {entries : List (MapKey × Value Declaration Identity Source)}, entries.lookup key = none →
      (insertOrdered key inserted entries).lookup key = some inserted
  | [], _ => by simp [insertOrdered, List.lookup]
  | (other, value) :: rest, absent => by
    by_cases same : key = other
    · subst same
      simp [List.lookup] at absent
    · have differentKeys : (key == other) = false := by simpa using same
      simp only [List.lookup, differentKeys] at absent
      by_cases before : keyBefore key other = true
      · simp [insertOrdered, before, List.lookup]
      · simp only [insertOrdered, before, Bool.false_eq_true, ite_false, List.lookup, differentKeys]
        exact lookup_insertOrdered_absent key inserted absent

theorem insertOrdered_position (key : MapKey) (inserted : Value Declaration Identity Source) :
    ∀ entries : List (MapKey × Value Declaration Identity Source), ∃ earlier later,
      insertOrdered key inserted entries = earlier ++ (key, inserted) :: later ∧ entries = earlier ++ later ∧
        (∀ entry ∈ earlier, keyBefore key entry.1 = false) ∧ (∀ entry ∈ later.head?, keyBefore key entry.1 = true)
  | [] => ⟨[], [], by simp [insertOrdered]⟩
  | (other, value) :: rest => by
    by_cases before : keyBefore key other = true
    · exact ⟨[], (other, value) :: rest, by simp [insertOrdered, before]⟩
    · obtain ⟨earlier, later, inserted', split, earlierAfter, laterBefore⟩ := insertOrdered_position key inserted rest
      refine ⟨(other, value) :: earlier, later, ?_, by simp [split], ?_, laterBefore⟩
      · simp only [insertOrdered, before, Bool.false_eq_true, ite_false, inserted', List.cons_append]
      · intro entry present
        simp only [List.mem_cons] at present
        rcases present with same | inside
        · subst same
          simpa using before
        · exact earlierAfter entry inside

theorem map_ok {α β ε : Type} {transform : α → β} {outcome : Except ε α} {result : β}
    (mapped : outcome.map transform = .ok result) : ∃ value, outcome = .ok value ∧ result = transform value := by
  cases outcome with
  | error failure => simp [Except.map] at mapped
  | ok value =>
    simp only [Except.map, Except.ok.injEq] at mapped
    exact ⟨value, rfl, mapped.symm⟩

theorem map_error {α β ε : Type} {transform : α → β} {outcome : Except ε α} {failure : ε}
    (mapped : outcome.map transform = .error failure) : outcome = .error failure := by
  cases outcome <;> simp_all [Except.map]

end Values

section Paths

theorem row_lookup_member (world : MembershipWorld Declaration Identity Source) :
    ∀ (row : RecordType Declaration) (fields : List (String × Value Declaration Identity Source)) name type,
      Membership.row world row fields → (rowEntries row).lookup name = some type →
        ∃ found, fields.lookup name = some found ∧ Membership.value world type found
  | .variable id, _, _, _, _, _ => nomatch id
  | .rowEmpty, _, _, _, _, found => by simp [rowEntries] at found
  | .rowField other type rest, fields, name, wanted, member, found => by
    simp only [Membership.row] at member
    by_cases same : name = other
    · subst same
      simp only [rowEntries, List.lookup, beq_self_eq_true, Option.some.injEq] at found
      subst found
      cases present : fields.lookup name with
      | none => simp [present] at member
      | some value =>
        simp only [present] at member
        exact ⟨value, rfl, member.1⟩
    · have differentNames : (name == other) = false := by simpa using same
      simp only [rowEntries, List.lookup, differentNames] at found
      exact row_lookup_member world rest fields name wanted member.2 found

theorem update_set_reads (world : MembershipWorld Declaration Identity Source) (raises : Raises Declaration Identity Source)
    (operate : Operate Declaration Identity Source) (right : Value Declaration Identity Source) :
    ∀ (path : List Selector) (type : ValueType Declaration) (value updated : Value Declaration Identity Source),
      update world raises (leafUpdate raises operate .set right) type value path = .ok updated →
        readPath updated path = some right
  | [], _, value, updated, updated' => by
    simp only [update, leafUpdate, Except.ok.injEq] at updated'
    subst updated'
    rfl
  | .field name :: rest, type, value, updated, updated' => by
    simp only [update] at updated'
    split at updated'
    · rename_i row fields _ _
      split at updated'
      · rename_i child childType present _
        obtain ⟨replaced, inner, rfl⟩ := map_ok updated'
        simp only [readPath, Value.eraseNominal, lookup_replaceField_same name replaced present, Option.bind_some]
        exact update_set_reads world raises operate right rest childType child replaced inner
      · simp at updated'
    · simp at updated'
  | .index position :: rest, type, value, updated, updated' => by
    simp only [update] at updated'
    split at updated'
    · rename_i element items _ _
      split at updated'
      · rename_i inRange
        obtain ⟨replaced, inner, rfl⟩ := map_ok updated'
        have nonnegative : ¬ position < 0 := by omega
        have bound : position.toNat < items.length := by omega
        simp only [readPath, listValues, Value.eraseNominal, nonnegative, ite_false, List.getElem?_set_self bound,
          Option.bind_some]
        exact update_set_reads world raises operate right rest element _ replaced inner
      · simp at updated'
    · simp at updated'
  | .key key :: rest, type, value, updated, updated' => by
    simp only [update] at updated'
    split at updated'
    · rename_i keyType element _
      split at updated'
      · rename_i entries _
        split at updated'
        · obtain ⟨replaced, inner, rfl⟩ := map_ok updated'
          rename_i present
          simp only [readPath, Value.eraseNominal, lookup_replaceEntry_same key replaced present, Option.bind_some]
          exact update_set_reads world raises operate right _ element _ replaced inner
        · rename_i absent
          obtain ⟨inserted, inner, rfl⟩ := map_ok updated'
          simp only [leafUpdate, Except.ok.injEq] at inner
          subst inner
          simp [readPath, Value.eraseNominal, lookup_insertOrdered_absent key right absent]
        · simp at updated'
      · simp at updated'
    · simp at updated'

theorem leafUpdate_runtime (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (op : Xsh.Syntax.AssignOp) (right : Value Declaration Identity Source) :
    ∀ current error, leafUpdate raises operate op right current = .error (.runtime error) →
      error = raises.missingKey ∨ ∃ current, operate op current right = .error error := by
  intro current error raised
  cases op with
  | set => simp [leafUpdate] at raised
  | _ =>
    cases current with
    | none =>
      simp only [leafUpdate, Except.error.injEq, PathError.runtime.injEq] at raised
      exact Or.inl raised.symm
    | some value =>
      simp only [leafUpdate] at raised
      split at raised
      · simp at raised
      · rename_i failure failed
        simp only [Except.error.injEq, PathError.runtime.injEq] at raised
        subst raised
        exact Or.inr ⟨value, failed⟩

theorem leafUpdate_never_malformed (raises : Raises Declaration Identity Source)
    (operate : Operate Declaration Identity Source) (op : Xsh.Syntax.AssignOp) (right : Value Declaration Identity Source)
    (current : Option (Value Declaration Identity Source)) :
    leafUpdate raises operate op right current ≠ .error .malformed := by
  intro malformed
  cases op with
  | set => simp [leafUpdate] at malformed
  | _ =>
    cases current with
    | none => simp [leafUpdate] at malformed
    | some value =>
      simp only [leafUpdate] at malformed
      split at malformed <;> simp at malformed

theorem update_runtime (world : MembershipWorld Declaration Identity Source) (raises : Raises Declaration Identity Source)
    (operate : Operate Declaration Identity Source) (op : Xsh.Syntax.AssignOp) (right : Value Declaration Identity Source) :
    ∀ (path : List Selector) (type : ValueType Declaration) (value : Value Declaration Identity Source) error,
      update world raises (leafUpdate raises operate op right) type value path = .error (.runtime error) →
        error = raises.indexOutOfRange ∨ error = raises.missingKey ∨
          ∃ current, operate op current right = .error error := by
  intro path
  induction path with
  | nil => exact fun _ value error raised => Or.inr (leafUpdate_runtime raises operate op right (some value) error raised)
  | cons step rest ih =>
    intro type value error raised
    cases step with
    | field name =>
      simp only [update] at raised
      split at raised
      · split at raised
        · exact ih _ _ error (map_error raised)
        · simp at raised
      · simp at raised
    | index position =>
      simp only [update] at raised
      split at raised
      · split at raised
        · exact ih _ _ error (map_error raised)
        · simp only [Except.error.injEq, PathError.runtime.injEq] at raised
          exact Or.inl raised.symm
      · simp at raised
    | key key =>
      simp only [update] at raised
      split at raised
      · split at raised
        · split at raised
          · exact ih _ _ error (map_error raised)
          · exact Or.inr (leafUpdate_runtime raises operate op right none error (map_error raised))
          · simp only [Except.error.injEq, PathError.runtime.injEq] at raised
            exact Or.inr (Or.inl raised.symm)
        · simp at raised
      · simp at raised

theorem update_index_out_of_range (world : MembershipWorld Declaration Identity Source)
    (raises : Raises Declaration Identity Source)
    (leaf : Option (Value Declaration Identity Source) →
      Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
    {type element : ValueType Declaration} {value : Value Declaration Identity Source}
    {items : List (Value Declaration Identity Source)} (position : Int) (rest : List Selector)
    (typed : elementType type = some element) (listed : listValues value = some items)
    (outside : ¬ (0 ≤ position ∧ position < items.length)) :
    update world raises leaf type value (.index position :: rest) = .error (.runtime raises.indexOutOfRange) := by
  simp only [update, typed, listed, outside, dite_false]

theorem update_missing_key_deeper (world : MembershipWorld Declaration Identity Source)
    (raises : Raises Declaration Identity Source)
    (leaf : Option (Value Declaration Identity Source) →
      Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
    {type keyType element : ValueType Declaration} {value : Value Declaration Identity Source}
    {entries : List (MapKey × Value Declaration Identity Source)} (key : MapKey) (next : Selector) (rest : List Selector)
    (typed : mapTypes type = some (keyType, element)) (listed : mapEntries keyType value = some entries)
    (absent : entries.lookup key = none) :
    update world raises leaf type value (.key key :: next :: rest) = .error (.runtime raises.missingKey) := by
  simp only [update, typed, listed, absent]

theorem update_missing_key_compound (world : MembershipWorld Declaration Identity Source)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (op : Xsh.Syntax.AssignOp) (compound : op ≠ .set) (right : Value Declaration Identity Source)
    {type keyType element : ValueType Declaration} {value : Value Declaration Identity Source}
    {entries : List (MapKey × Value Declaration Identity Source)} (key : MapKey)
    (typed : mapTypes type = some (keyType, element)) (listed : mapEntries keyType value = some entries)
    (absent : entries.lookup key = none) :
    update world raises (leafUpdate raises operate op right) type value [.key key] =
      .error (.runtime raises.missingKey) := by
  simp only [update, typed, listed, absent]
  cases op with
  | set => exact absurd rfl compound
  | _ => rfl

theorem update_insert_ordered (world : MembershipWorld Declaration Identity Source)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (right : Value Declaration Identity Source)
    {type keyType element : ValueType Declaration} {value : Value Declaration Identity Source}
    {entries : List (MapKey × Value Declaration Identity Source)} (key : MapKey)
    (typed : mapTypes type = some (keyType, element)) (listed : mapEntries keyType value = some entries)
    (absent : entries.lookup key = none) :
    update world raises (leafUpdate raises operate .set right) type value [.key key] =
      .ok (.map (insertOrdered key right entries)) := by
  simp [update, typed, listed, absent, leafUpdate, Except.map]

theorem update_nominal_field_malformed (world : MembershipWorld Declaration Identity Source)
    (raises : Raises Declaration Identity Source)
    (leaf : Option (Value Declaration Identity Source) →
      Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
    (declaration : Declaration) (row : RecordType Declaration) (value : Value Declaration Identity Source)
    (name : String) (rest : List Selector) :
    update world raises leaf (.nominal declaration row) value (.field name :: rest) = .error .malformed := by
  simp [update, recordRow]

def PathTyped : ValueType Declaration → List Selector → Prop
  | _, [] => True
  | type, .field name :: rest =>
    ∃ row child, recordRow type = some row ∧ (rowEntries row).lookup name = some child ∧ PathTyped child rest
  | type, .index _ :: rest => ∃ element, elementType type = some element ∧ PathTyped element rest
  | type, .key _ :: rest => ∃ keyType element, mapTypes type = some (keyType, element) ∧ PathTyped element rest

theorem lookup_entry_mem {α : Type} : ∀ {entries : List (MapKey × α)} {key : MapKey} {found : α},
    entries.lookup key = some found → (key, found) ∈ entries
  | [], _, _, present => by simp at present
  | (other, value) :: rest, key, found, present => by
    by_cases same : key = other
    · subst same
      simp [List.lookup] at present
      simp [present]
    · have differentKeys : (key == other) = false := by simpa using same
      simp only [List.lookup, differentKeys] at present
      exact List.mem_cons_of_mem _ (lookup_entry_mem present)

theorem mapTypes_some {type keyType element : ValueType Declaration}
    (found : mapTypes type = some (keyType, element)) : type = .map keyType element := by
  cases type <;> simp_all [mapTypes]

theorem update_never_malformed (world : MembershipWorld Declaration Identity Source)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (op : Xsh.Syntax.AssignOp) (right : Value Declaration Identity Source) :
    ∀ (path : List Selector) (type : ValueType Declaration) (value : Value Declaration Identity Source),
      Membership.value world type value → PathTyped type path →
        update world raises (leafUpdate raises operate op right) type value path ≠ .error .malformed
  | [], _, value, _, _ => by
    simpa [update] using leafUpdate_never_malformed raises operate op right (some value)
  | .field name :: rest, type, value, member, typed => by
    obtain ⟨row, child, rowFound, childFound, restTyped⟩ := typed
    cases type <;> simp only [recordRow, reduceCtorEq, Option.some.injEq] at rowFound
    subst rowFound
    simp only [Membership.value] at member
    cases fielded : Membership.recordFields world value with
    | none => simp [fielded] at member
    | some fields =>
      simp only [fielded] at member
      obtain ⟨found, present, foundMember⟩ := row_lookup_member world _ fields name child member childFound
      simp only [update, recordRow, fielded, present, childFound]
      intro malformed
      exact update_never_malformed world raises operate op right rest child found foundMember restTyped
        (map_error malformed)
  | .index position :: rest, type, value, member, typed => by
    obtain ⟨element, elementFound, restTyped⟩ := typed
    have items : ∃ items, listValues value = some items ∧ ∀ item ∈ items, Membership.value world element item := by
      cases type <;> simp only [elementType, reduceCtorEq, Option.some.injEq] at elementFound <;> subst elementFound <;>
        simp only [Membership.value] at member <;> unfold listValues <;> split at member <;> simp_all
    obtain ⟨items, listed, members⟩ := items
    simp only [update, elementFound, listed]
    split
    · rename_i inRange
      intro malformed
      exact update_never_malformed world raises operate op right rest element _
        (members _ (List.getElem_mem _)) restTyped (map_error malformed)
    · simp
  | .key key :: rest, type, value, member, typed => by
    obtain ⟨keyType, element, mapFound, restTyped⟩ := typed
    obtain rfl := mapTypes_some mapFound
    have entries : ∃ entries, mapEntries keyType value = some entries ∧
        ∀ entry ∈ entries, Membership.value world element entry.2 := by
      simp only [Membership.value] at member
      unfold mapEntries
      split at member
      · rename_i present erased
        exact ⟨present, by simp [erased], fun entry inside => (member entry inside).2⟩
      · rename_i fields erased
        obtain ⟨stringKey, members⟩ := member
        subst stringKey
        refine ⟨fields.map fun field => (MapKey.string field.1, field.2), by simp [erased], ?_⟩
        intro entry inside
        obtain ⟨field, fieldInside, rfl⟩ := List.mem_map.mp inside
        exact members field fieldInside
      · exact absurd member id
    obtain ⟨present, listed, members⟩ := entries
    simp only [update, mapTypes]
    rw [listed]
    cases found : present.lookup key with
    | some child =>
      simp only [found]
      intro malformed
      exact update_never_malformed world raises operate op right rest element child
        (members _ (lookup_entry_mem found)) restTyped (map_error malformed)
    | none =>
      cases rest with
      | nil =>
        simp only [found]
        intro malformed
        exact leafUpdate_never_malformed raises operate op right none (map_error malformed)
      | cons next more => simp [found]

end Paths

section Selectors

theorem keyFailure_negative (raises : Raises Declaration Identity Source) {type element : ValueType Declaration}
    (key : Xsh.Values.IntValue) (rest : List Selector)
    (typed : mapTypes type = some (.primitive .unsignedInteger, element)) (negative : key.val < 0) :
    keyFailure raises type (.key (.integer key) :: rest) = some raises.schema := by
  simp [keyFailure, typed, negativeUnsignedKey, negative]

end Selectors

section Assignment

variable [DecidableEq Declaration]

theorem assignPath_whole_cell (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (right : Value Declaration Identity Source) :
    assignPath world decisions raises operate store cell [] .set right = assign world decisions store cell right := by
  unfold assignPath
  cases present : store cell with
  | none => simp [assign, present]
  | some current =>
    cases mutability : current.mutability with
    | immutable => simp [assign, present, mutability]
    | mutable => simp [update, leafUpdate, mutability]

theorem assignPath_compound_reads_at_commit (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (op : Xsh.Syntax.AssignOp) (compound : op ≠ .set)
    (right : Value Declaration Identity Source) (current : Cell world) (present : store cell = some current)
    (mutableCell : current.mutability = .mutable) :
    assignPath world decisions raises operate store cell [] op right =
      match operate op current.contents.value right with
      | .ok result => assign world decisions store cell result
      | .error error => .failed (.runtime error) store := by
  unfold assignPath
  simp only [present, mutableCell, update]
  cases op with
  | set => exact absurd rfl compound
  | _ => simp only [leafUpdate]; cases operate _ current.contents.value right <;> rfl

theorem assignPath_committed_iff (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source) :
    (∃ next, assignPath world decisions raises operate store cell path op right = .committed next) ↔
      ∃ current updated, store cell = some current ∧ current.mutability = .mutable ∧
        update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
          .ok updated ∧ HasType world current.contents.typeTag updated := by
  constructor
  · rintro ⟨next, committed⟩
    unfold assignPath at committed
    cases present : store cell with
    | none => simp [present] at committed
    | some current =>
      rw [present] at committed
      cases mutability : current.mutability with
      | immutable => simp [mutability] at committed
      | mutable =>
        simp only [mutability] at committed
        split at committed
        · simp at committed
        · simp at committed
        · rename_i updated updated'
          obtain ⟨found, foundPresent, _, typed⟩ :=
            (assign_committed_iff world decisions store cell updated).mp ⟨next, committed⟩
          rw [present, Option.some.injEq] at foundPresent
          subst foundPresent
          exact ⟨current, updated, rfl, mutability, updated', typed⟩
  · rintro ⟨current, updated, present, mutableCell, updated', typed⟩
    unfold assignPath
    simp only [present, mutableCell, updated']
    exact (assign_committed_iff world decisions store cell updated).mpr ⟨current, present, mutableCell, typed⟩

theorem assignPath_committed_effect (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store next : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source)
    (committed : assignPath world decisions raises operate store cell path op right = .committed next) :
    ∃ current updated, store cell = some current ∧
      update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
        .ok updated ∧
      (∃ replaced, next cell = some replaced ∧ replaced.contents.typeTag = current.contents.typeTag ∧
        replaced.contents.value = updated ∧ replaced.mutability = current.mutability) ∧
      ∀ other, other ≠ cell → next other = store other := by
  unfold assignPath at committed
  cases present : store cell with
  | none => simp [present] at committed
  | some current =>
    rw [present] at committed
    cases mutability : current.mutability with
    | immutable => simp [mutability] at committed
    | mutable =>
      simp only [mutability] at committed
      split at committed
      · simp at committed
      · simp at committed
      · rename_i updated updated'
        obtain ⟨found, foundPresent, replaced, others⟩ :=
          assign_committed_effect world decisions store next cell updated committed
        rw [present, Option.some.injEq] at foundPresent
        subst foundPresent
        exact ⟨current, updated, rfl, updated', replaced, others⟩

theorem assignPath_keeps_store (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source)
    (uncommitted : ∀ next, assignPath world decisions raises operate store cell path op right ≠ .committed next) :
    (assignPath world decisions raises operate store cell path op right).storeAfter = store := by
  unfold assignPath at uncommitted ⊢
  cases present : store cell with
  | none => rfl
  | some current =>
    simp only [present] at uncommitted ⊢
    cases mutability : current.mutability with
    | immutable => rfl
    | mutable =>
      simp only [mutability] at uncommitted ⊢
      split
      · rfl
      · rfl
      · rename_i updated _
        rename_i updated'
        simp only [updated'] at uncommitted
        exact assign_uncommitted_keeps_store world decisions store cell updated uncommitted

theorem assignPath_failed (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store after : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source) (failure : Failure Declaration Identity Source)
    (failed : assignPath world decisions raises operate store cell path op right = .failed failure after) :
    after = store ∧ ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
      ((∃ error, failure = .runtime error ∧
          update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
            .error (.runtime error)) ∨
        ∃ updated, update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
            current.contents.value path = .ok updated ∧
          ¬ HasType world current.contents.typeTag updated ∧ failure = .typeError current.contents.typeTag updated) := by
  unfold assignPath at failed
  cases present : store cell with
  | none => simp [present] at failed
  | some current =>
    rw [present] at failed
    cases mutability : current.mutability with
    | immutable => simp [mutability] at failed
    | mutable =>
      simp only [mutability] at failed
      split at failed
      · rename_i error raised
        simp only [StoreStep.failed.injEq] at failed
        exact ⟨failed.2.symm, current, rfl, mutability, Or.inl ⟨error, failed.1.symm, raised⟩⟩
      · simp at failed
      · rename_i updated updated'
        obtain ⟨sameStore, found, foundPresent, _, rejected, sameFailure⟩ :=
          assign_failure_is_type_error world decisions store cell updated failure after failed
        rw [present, Option.some.injEq] at foundPresent
        subst foundPresent
        exact ⟨sameStore, current, rfl, mutability, Or.inr ⟨updated, updated', rejected, sameFailure⟩⟩

end Assignment

section Declarations

variable [DecidableEq Declaration]

theorem declareAll_committed (world : World Declaration Identity Source) (decisions : Decisions world)
    (original : Store world) (mutability : Mutability) :
    ∀ (bindings : List (Binding Declaration Identity Source)) (current next : Store world),
      declareAll world decisions original mutability current bindings = .committed next →
        (∀ binding ∈ bindings, current binding.1 = none ∧ HasType world binding.2.1 binding.2.2) ∧
        (bindings.map Prod.fst).Nodup ∧
        (∀ binding ∈ bindings, ∃ declared, next binding.1 = some declared ∧ declared.mutability = mutability ∧
          declared.contents.typeTag = binding.2.1 ∧ declared.contents.value = binding.2.2) ∧
        ∀ other, other ∉ bindings.map Prod.fst → next other = current other
  | [], current, next, committed => by
    simp only [declareAll, StoreStep.committed.injEq] at committed
    subst committed
    simp
  | (cell, type, value) :: rest, current, next, committed => by
    simp only [declareAll] at committed
    split at committed
    · rename_i first declared
      obtain ⟨fresh, typed⟩ := (declare_committed_iff world decisions current cell mutability type value).mp
        ⟨first, declared⟩
      obtain ⟨⟨placed, placedAt, placedMutability, placedType, placedValue⟩, firstOthers⟩ :=
        declare_committed_effect world decisions current first cell mutability type value declared
      obtain ⟨restFresh, restDistinct, restPlaced, restOthers⟩ :=
        declareAll_committed world decisions original mutability rest first next committed
      have notLater : cell ∉ rest.map Prod.fst := by
        intro later
        obtain ⟨binding, inside, same⟩ := List.mem_map.mp later
        have := (restFresh binding inside).1
        rw [same, placedAt] at this
        cases this
      refine ⟨?_, ?_, ?_, ?_⟩
      · intro binding inside
        simp only [List.mem_cons] at inside
        rcases inside with rfl | later
        · exact ⟨fresh, typed⟩
        · have different : binding.1 ≠ cell := fun same => notLater (same ▸ List.mem_map_of_mem later)
          exact ⟨(firstOthers binding.1 different) ▸ (restFresh binding later).1, (restFresh binding later).2⟩
      · simp only [List.map_cons, List.nodup_cons]
        exact ⟨notLater, restDistinct⟩
      · intro binding inside
        simp only [List.mem_cons] at inside
        rcases inside with rfl | later
        · exact ⟨placed, by rw [restOthers cell notLater, placedAt], placedMutability, placedType, placedValue⟩
        · exact restPlaced binding later
      · intro other absent
        simp only [List.map_cons, List.mem_cons, not_or] at absent
        rw [restOthers other absent.2, firstOthers other absent.1]
    · simp at committed
    · simp at committed

theorem declareAll_keeps_original (world : World Declaration Identity Source) (decisions : Decisions world)
    (original : Store world) (mutability : Mutability) :
    ∀ (bindings : List (Binding Declaration Identity Source)) (current : Store world),
      (∀ next, declareAll world decisions original mutability current bindings ≠ .committed next) →
        (declareAll world decisions original mutability current bindings).storeAfter = original
  | [], current, uncommitted => by simp [declareAll] at uncommitted
  | (cell, type, value) :: rest, current, uncommitted => by
    simp only [declareAll] at uncommitted ⊢
    split
    · rename_i first declared
      simp only [declared] at uncommitted
      exact declareAll_keeps_original world decisions original mutability rest first uncommitted
    · rfl
    · rfl

theorem declareTarget_committed_iff (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (mutability : Mutability) (target : Target Declaration)
    (source : Value Declaration Identity Source) :
    (∃ next, declareTarget world decisions store mutability target source = .committed next) ↔
      ∃ bindings, select world.membership target source = some bindings ∧
        (∀ binding ∈ bindings, store binding.1 = none ∧ HasType world binding.2.1 binding.2.2) ∧
        (bindings.map Prod.fst).Nodup := by
  unfold declareTarget
  constructor
  · rintro ⟨next, committed⟩
    split at committed
    · simp at committed
    · rename_i bindings selected
      obtain ⟨fresh, distinct, _, _⟩ := declareAll_committed world decisions store mutability bindings store next committed
      exact ⟨bindings, selected, fresh, distinct⟩
  · rintro ⟨bindings, selected, fresh, distinct⟩
    simp only [selected]
    exact declareAll_succeeds world decisions store mutability bindings store fresh distinct
where
  declareAll_succeeds (world : World Declaration Identity Source) (decisions : Decisions world)
      (original : Store world) (mutability : Mutability) :
      ∀ (bindings : List (Binding Declaration Identity Source)) (current : Store world),
        (∀ binding ∈ bindings, current binding.1 = none ∧ HasType world binding.2.1 binding.2.2) →
        (bindings.map Prod.fst).Nodup →
          ∃ next, declareAll world decisions original mutability current bindings = .committed next
    | [], current, _, _ => ⟨current, rfl⟩
    | (cell, type, value) :: rest, current, fresh, distinct => by
      obtain ⟨headFresh, headTyped⟩ := fresh (cell, type, value) (by simp)
      obtain ⟨first, declared⟩ := (declare_committed_iff world decisions current cell mutability type value).mpr
        ⟨headFresh, headTyped⟩
      obtain ⟨_, firstOthers⟩ := declare_committed_effect world decisions current first cell mutability type value declared
      simp only [List.map_cons, List.nodup_cons] at distinct
      have restFresh : ∀ binding ∈ rest, first binding.1 = none ∧ HasType world binding.2.1 binding.2.2 := by
        intro binding inside
        have different : binding.1 ≠ cell := fun same => distinct.1 (same ▸ List.mem_map_of_mem inside)
        obtain ⟨wasFresh, typed⟩ := fresh binding (List.mem_cons_of_mem _ inside)
        exact ⟨(firstOthers binding.1 different).trans wasFresh, typed⟩
      obtain ⟨next, rest'⟩ := declareAll_succeeds world decisions original mutability rest first restFresh distinct.2
      exact ⟨next, by simp only [declareAll, declared]; exact rest'⟩

theorem declareTarget_keeps_store (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (mutability : Mutability) (target : Target Declaration)
    (source : Value Declaration Identity Source)
    (uncommitted : ∀ next, declareTarget world decisions store mutability target source ≠ .committed next) :
    (declareTarget world decisions store mutability target source).storeAfter = store := by
  unfold declareTarget at uncommitted ⊢
  split
  · rfl
  · rename_i bindings selected
    simp only [selected] at uncommitted
    exact declareAll_keeps_original world decisions store mutability bindings store uncommitted

theorem declareTarget_name (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (mutability : Mutability) (cell : CellId) (type : ValueType Declaration)
    (source : Value Declaration Identity Source) :
    declareTarget world decisions store mutability (.name cell type) source =
      declare world decisions store cell mutability type source := by
  simp only [declareTarget, select, declareAll]
  unfold declare
  cases store cell with
  | some _ => rfl
  | none => cases Xsh.Core.Store.allocate world decisions store cell mutability type source <;> rfl

theorem declareTarget_discard (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (mutability : Mutability) (source : Value Declaration Identity Source) :
    declareTarget world decisions store mutability .discard source = .committed store := rfl

end Declarations

section Environment

theorem setEnvironment_ok_iff (raises : Raises Declaration Identity Source) (overlay : Overlay) (name value : List UInt8) :
    (∃ next, setEnvironment raises overlay name value = .ok next) ↔ 0 ∉ value := by
  unfold setEnvironment
  by_cases nul : 0 ∈ value <;> simp [nul]

theorem setEnvironment_effect (raises : Raises Declaration Identity Source) (overlay next : Overlay)
    (name value : List UInt8) (set : setEnvironment raises overlay name value = .ok next) :
    readEnvironment next name = some value ∧ ∀ other, other ≠ name → readEnvironment next other = readEnvironment overlay other := by
  unfold setEnvironment at set
  by_cases nul : 0 ∈ value
  · simp [nul] at set
  · simp only [nul, ite_false, Except.ok.injEq] at set
    subst set
    refine ⟨by simp [readEnvironment, List.lookup], ?_⟩
    intro other different
    have differentNames : (other == name) = false := by simpa using different
    simp only [readEnvironment, List.lookup, differentNames]
    simpa using lookup_filter_other overlay other name different
where
  lookup_filter_other : ∀ (overlay : Overlay) (other name : List UInt8), other ≠ name →
      (overlay.filter fun entry => !decide (entry.1 = name)).lookup other = overlay.lookup other
    | [], _, _, _ => rfl
    | (key, item) :: rest, other, name, different => by
      have tail := lookup_filter_other rest other name different
      by_cases removed : key = name
      · subst removed
        have differentKeys : (other == key) = false := by simpa using different
        simp [List.filter, List.lookup, differentKeys, tail]
      · by_cases same : other = key
        · subst same
          simp [List.filter, removed, List.lookup]
        · have differentKeys : (other == key) = false := by simpa using same
          simp [List.filter, removed, List.lookup, differentKeys, tail]

theorem setEnvironment_rejects_nul (raises : Raises Declaration Identity Source) (overlay : Overlay)
    (name value : List UInt8) (nul : 0 ∈ value) :
    setEnvironment raises overlay name value = .error (.runtime raises.nulArgv) := by
  simp [setEnvironment, nul]

end Environment

section Statements

variable [DecidableEq Declaration]

theorem assignStatement_key_failure (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (current : Cell world) (present : store cell = some current) (error : Value Declaration Identity Source)
    (failed : keyFailure raises current.contents.typeTag path = some error) (right : RightSide world) :
    assignStatement world decisions raises operate store cell path op right = .failed (.raised error) store := by
  simp [assignStatement, present, failed]

theorem assignStatement_commits_after_right (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store after : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (current : Cell world) (present : store cell = some current)
    (checked : keyFailure raises current.contents.typeTag path = none) (right : RightSide world)
    (value : Value Declaration Identity Source) (evaluated : right store = .ok (value, after)) :
    assignStatement world decisions raises operate store cell path op right =
      assignPath world decisions raises operate after cell path op value := by
  simp [assignStatement, present, checked, evaluated]

theorem assignStatement_right_fails (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store after : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (current : Cell world) (present : store cell = some current)
    (checked : keyFailure raises current.contents.typeTag path = none) (right : RightSide world)
    (failure : Failure Declaration Identity Source) (failed : right store = .error (failure, after)) :
    assignStatement world decisions raises operate store cell path op right = .failed failure after := by
  simp [assignStatement, present, checked, failed]

end Statements

section Unwinding

theorem runCleanups_failed_primary {world : World Declaration Identity Source} :
    ∀ (cleanups : List (Cleanup world)) (failure : Failure Declaration Identity Source) (store : Store world),
      (∃ after, runCleanups cleanups (.failed failure store) = .failed failure after) ∨
        ∃ fault after, runCleanups cleanups (.failed failure store) = .internal fault after
  | [], failure, store => Or.inl ⟨store, rfl⟩
  | cleanup :: rest, failure, store => by
    simp only [runCleanups]
    cases ran : cleanup (StoreStep.storeAfter (.failed failure store)) with
    | internal fault after =>
      simp only [combine]
      exact Or.inr (internal_stays rest fault after)
    | committed after => exact runCleanups_failed_primary rest failure after
    | failed other after => exact runCleanups_failed_primary rest failure after
where
  internal_stays : ∀ (cleanups : List (Cleanup world)) fault (store : Store world),
      ∃ fault' after, runCleanups cleanups (.internal fault store) = .internal fault' after
    | [], fault, store => ⟨fault, store, rfl⟩
    | _ :: rest, fault, store => by simpa [runCleanups] using internal_stays rest fault store

theorem unwind_failed_primary {world : World Declaration Identity Source} (registered : List (Cleanup world))
    (failure : Failure Declaration Identity Source) (store : Store world) :
    (∃ after, unwind registered (.failed failure store) = .failed failure after) ∨
      ∃ fault after, unwind registered (.failed failure store) = .internal fault after :=
  runCleanups_failed_primary registered.reverse failure store

theorem unwind_last_registered_runs_first {world : World Declaration Identity Source}
    (registered : List (Cleanup world)) (cleanup : Cleanup world) (failure : Failure Declaration Identity Source)
    (store : Store world) :
    unwind (registered ++ [cleanup]) (.failed failure store) =
      runCleanups registered.reverse (combine (.failed failure store) (cleanup store)) := by
  simp [unwind, runCleanups, StoreStep.storeAfter]

theorem unwind_committed_first_failure {world : World Declaration Identity Source}
    (registered : List (Cleanup world)) (cleanup : Cleanup world) (store : Store world) :
    unwind (registered ++ [cleanup]) (.committed store) = runCleanups registered.reverse (cleanup store) := by
  simp only [unwind, List.reverse_append, List.reverse_cons, List.reverse_nil, List.nil_append, List.cons_append,
    runCleanups, StoreStep.storeAfter]
  cases cleanup store <;> rfl

theorem tryBlock_uncaught {world : World Declaration Identity Source} (registered : List (Cleanup world))
    (failure : Failure Declaration Identity Source) (uncaught : failure.catchable = false) (store : Store world) :
    (∃ after, tryBlock registered (.failed failure store) = .propagated failure after) ∨
      ∃ fault after, tryBlock registered (.failed failure store) = .internal fault after := by
  unfold tryBlock
  rcases unwind_failed_primary registered failure store with ⟨after, failed⟩ | ⟨fault, after, internal⟩
  · refine Or.inl ⟨after, ?_⟩
    rw [failed]
    cases failure <;> simp_all [recover, Failure.catchable]
  · exact Or.inr ⟨fault, after, by rw [internal]; rfl⟩

theorem tryBlock_raised {world : World Declaration Identity Source} (registered : List (Cleanup world))
    (error : Value Declaration Identity Source) (store : Store world) :
    (∃ after, tryBlock registered (.failed (.raised error) store) = .caught error after) ∨
      ∃ fault after, tryBlock registered (.failed (.raised error) store) = .internal fault after := by
  unfold tryBlock
  rcases unwind_failed_primary registered (.raised error) store with ⟨after, failed⟩ | ⟨fault, after, internal⟩
  · exact Or.inl ⟨after, by rw [failed]; rfl⟩
  · exact Or.inr ⟨fault, after, by rw [internal]; rfl⟩

end Unwinding

section Root

variable [DecidableEq Declaration]

/-- The store-writing forms on the Core store API over every Core world, type
and value, with selectors and the right side already evaluated. A path
assignment commits exactly when its updated cell value is a member; the commit
replaces only that cell and a `=` update reads back. A well-typed path never
reaches a malformed state, and a field write through a nominal type is not a
write. An index outside the list, a missing key under a deeper selector or a
compound operator, and an operator failure are runtime failures; a missing key
under `=` is inserted in key order. A negative key into `Map[UInt, V]` fails
before the right side runs and the update commits on the store the right side
left. A destructuring declaration commits all its cells or none. Every failure
keeps the store; unwinding runs the last registered cleanup first, keeps an
earlier failure primary, makes the first cleanup failure primary after a
committed body, and `try` catches only raised errors. -/
theorem store_writing_forms (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source) :
    (∀ (store : Store world) cell right,
      assignPath world decisions raises operate store cell [] .set right = assign world decisions store cell right) ∧
    (∀ (store : Store world) cell path op right,
      (∃ next, assignPath world decisions raises operate store cell path op right = .committed next) ↔
        ∃ current updated, store cell = some current ∧ current.mutability = .mutable ∧
          update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
            current.contents.value path = .ok updated ∧ HasType world current.contents.typeTag updated) ∧
    (∀ (store next : Store world) cell path op right,
      assignPath world decisions raises operate store cell path op right = .committed next →
        ∃ current updated, store cell = some current ∧
          update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
            current.contents.value path = .ok updated ∧
          (∃ replaced, next cell = some replaced ∧ replaced.contents.typeTag = current.contents.typeTag ∧
            replaced.contents.value = updated ∧ replaced.mutability = current.mutability) ∧
          ∀ other, other ≠ cell → next other = store other) ∧
    (∀ (store : Store world) cell path op right,
      (∀ next, assignPath world decisions raises operate store cell path op right ≠ .committed next) →
        (assignPath world decisions raises operate store cell path op right).storeAfter = store) ∧
    (∀ (store after : Store world) cell path op right failure,
      assignPath world decisions raises operate store cell path op right = .failed failure after →
        after = store ∧ ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
          ((∃ error, failure = .runtime error ∧
              update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
                current.contents.value path = .error (.runtime error)) ∨
            ∃ updated, update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
                current.contents.value path = .ok updated ∧
              ¬ HasType world current.contents.typeTag updated ∧
              failure = .typeError current.contents.typeTag updated)) ∧
    (∀ path type (value updated right : Value Declaration Identity Source),
      update world.membership raises (leafUpdate raises operate .set right) type value path = .ok updated →
        readPath updated path = some right) ∧
    (∀ path type (value : Value Declaration Identity Source) op right error,
      update world.membership raises (leafUpdate raises operate op right) type value path = .error (.runtime error) →
        error = raises.indexOutOfRange ∨ error = raises.missingKey ∨
          ∃ current, operate op current right = .error error) ∧
    (∀ path type (value : Value Declaration Identity Source) op right,
      Membership.value world.membership type value → PathTyped type path →
        update world.membership raises (leafUpdate raises operate op right) type value path ≠ .error .malformed) ∧
    (∀ (leaf : Option (Value Declaration Identity Source) →
        Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
      declaration row (value : Value Declaration Identity Source) name rest,
      update world.membership raises leaf (.nominal declaration row) value (.field name :: rest) = .error .malformed) ∧
    (∀ (leaf : Option (Value Declaration Identity Source) →
        Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
      type element (value : Value Declaration Identity Source) items (position : Int) rest,
      elementType type = some element → listValues value = some items →
      ¬ (0 ≤ position ∧ position < items.length) →
        update world.membership raises leaf type value (.index position :: rest) =
          .error (.runtime raises.indexOutOfRange)) ∧
    (∀ (leaf : Option (Value Declaration Identity Source) →
        Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
      type keyType element (value : Value Declaration Identity Source) entries key next rest,
      mapTypes type = some (keyType, element) → mapEntries keyType value = some entries → entries.lookup key = none →
        update world.membership raises leaf type value (.key key :: next :: rest) =
          .error (.runtime raises.missingKey)) ∧
    (∀ (op : Xsh.Syntax.AssignOp) right type keyType element (value : Value Declaration Identity Source) entries key,
      op ≠ .set → mapTypes type = some (keyType, element) → mapEntries keyType value = some entries →
      entries.lookup key = none →
        update world.membership raises (leafUpdate raises operate op right) type value [.key key] =
          .error (.runtime raises.missingKey)) ∧
    (∀ right type keyType element (value : Value Declaration Identity Source) entries key,
      mapTypes type = some (keyType, element) → mapEntries keyType value = some entries →
      entries.lookup key = none →
        update world.membership raises (leafUpdate raises operate .set right) type value [.key key] =
          .ok (.map (insertOrdered key right entries))) ∧
    (∀ key (inserted : Value Declaration Identity Source) entries, ∃ earlier later,
      insertOrdered key inserted entries = earlier ++ (key, inserted) :: later ∧ entries = earlier ++ later ∧
        (∀ entry ∈ earlier, keyBefore key entry.1 = false) ∧ (∀ entry ∈ later.head?, keyBefore key entry.1 = true)) ∧
    (∀ (store : Store world) cell op right (current : Cell world), op ≠ .set → store cell = some current →
      current.mutability = .mutable →
        assignPath world decisions raises operate store cell [] op right =
          match operate op current.contents.value right with
          | .ok result => assign world decisions store cell result
          | .error error => .failed (.runtime error) store) ∧
    (∀ type element (key : Xsh.Values.IntValue) rest, mapTypes type = some (.primitive .unsignedInteger, element) →
      key.val < 0 → keyFailure raises type (.key (.integer key) :: rest) = some raises.schema) ∧
    (∀ (store : Store world) cell path op (current : Cell world) error right, store cell = some current →
      keyFailure raises current.contents.typeTag path = some error →
        assignStatement world decisions raises operate store cell path op right = .failed (.raised error) store) ∧
    (∀ (store after : Store world) cell path op (current : Cell world) right value, store cell = some current →
      keyFailure raises current.contents.typeTag path = none → right store = .ok (value, after) →
        assignStatement world decisions raises operate store cell path op right =
          assignPath world decisions raises operate after cell path op value) ∧
    (∀ (store after : Store world) cell path op (current : Cell world) right failure, store cell = some current →
      keyFailure raises current.contents.typeTag path = none → right store = .error (failure, after) →
        assignStatement world decisions raises operate store cell path op right = .failed failure after) ∧
    (∀ (store : Store world) mutability target source,
      (∃ next, declareTarget world decisions store mutability target source = .committed next) ↔
        ∃ bindings, select world.membership target source = some bindings ∧
          (∀ binding ∈ bindings, store binding.1 = none ∧ HasType world binding.2.1 binding.2.2) ∧
          (bindings.map Prod.fst).Nodup) ∧
    (∀ (store next : Store world) mutability target source bindings,
      select world.membership target source = some bindings →
      declareTarget world decisions store mutability target source = .committed next →
        (∀ binding ∈ bindings, ∃ declared, next binding.1 = some declared ∧ declared.mutability = mutability ∧
          declared.contents.typeTag = binding.2.1 ∧ declared.contents.value = binding.2.2) ∧
        ∀ other, other ∉ bindings.map Prod.fst → next other = store other) ∧
    (∀ (store : Store world) mutability target source,
      (∀ next, declareTarget world decisions store mutability target source ≠ .committed next) →
        (declareTarget world decisions store mutability target source).storeAfter = store) ∧
    (∀ (store : Store world) mutability cell type source,
      declareTarget world decisions store mutability (.name cell type) source =
        declare world decisions store cell mutability type source) ∧
    (∀ (store : Store world) mutability source,
      declareTarget world decisions store mutability .discard source = .committed store) ∧
    (∀ overlay name value, (∃ next, setEnvironment raises overlay name value = .ok next) ↔ 0 ∉ value) ∧
    (∀ overlay next name value, setEnvironment raises overlay name value = .ok next →
      readEnvironment next name = some value ∧
        ∀ other, other ≠ name → readEnvironment next other = readEnvironment overlay other) ∧
    (∀ overlay name value, 0 ∈ value →
      setEnvironment raises overlay name value = .error (.runtime raises.nulArgv)) ∧
    (∀ (registered : List (Cleanup world)) failure (store : Store world),
      (∃ after, unwind registered (.failed failure store) = .failed failure after) ∨
        ∃ fault after, unwind registered (.failed failure store) = .internal fault after) ∧
    (∀ (registered : List (Cleanup world)) cleanup failure (store : Store world),
      unwind (registered ++ [cleanup]) (.failed failure store) =
        runCleanups registered.reverse (combine (.failed failure store) (cleanup store))) ∧
    (∀ (registered : List (Cleanup world)) cleanup (store : Store world),
      unwind (registered ++ [cleanup]) (.committed store) = runCleanups registered.reverse (cleanup store)) ∧
    (∀ (registered : List (Cleanup world)) (failure : Failure Declaration Identity Source) (store : Store world),
      failure.catchable = false →
        (∃ after, tryBlock registered (.failed failure store) = .propagated failure after) ∨
          ∃ fault after, tryBlock registered (.failed failure store) = .internal fault after) ∧
    (∀ (registered : List (Cleanup world)) error (store : Store world),
      (∃ after, tryBlock registered (.failed (.raised error) store) = .caught error after) ∨
        ∃ fault after, tryBlock registered (.failed (.raised error) store) = .internal fault after) := by
  refine ⟨assignPath_whole_cell world decisions raises operate,
    assignPath_committed_iff world decisions raises operate,
    fun store next cell path op right committed =>
      assignPath_committed_effect world decisions raises operate store next cell path op right committed,
    assignPath_keeps_store world decisions raises operate,
    fun store after cell path op right failure failed =>
      assignPath_failed world decisions raises operate store after cell path op right failure failed,
    fun path type value updated right => update_set_reads world.membership raises operate right path type value updated,
    fun path type value op right error => update_runtime world.membership raises operate op right path type value error,
    fun path type value op right member typed =>
      update_never_malformed world.membership raises operate op right path type value member typed,
    fun leaf declaration row value name rest =>
      update_nominal_field_malformed world.membership raises leaf declaration row value name rest,
    fun leaf _ _ _ _ position rest typed listed outside =>
      update_index_out_of_range world.membership raises leaf position rest typed listed outside,
    fun leaf _ _ _ _ _ key next rest typed listed absent =>
      update_missing_key_deeper world.membership raises leaf key next rest typed listed absent,
    fun op right _ _ _ _ _ key compound typed listed absent =>
      update_missing_key_compound world.membership raises operate op compound right key typed listed absent,
    fun right _ _ _ _ _ key typed listed absent =>
      update_insert_ordered world.membership raises operate right key typed listed absent,
    insertOrdered_position,
    fun store cell op right current compound present mutableCell =>
      assignPath_compound_reads_at_commit world decisions raises operate store cell op compound right current present
        mutableCell,
    fun _ _ key rest typed negative => keyFailure_negative raises key rest typed negative,
    fun store cell path op current error right present failed =>
      assignStatement_key_failure world decisions raises operate store cell path op current present error failed right,
    fun store after cell path op current right value present checked evaluated =>
      assignStatement_commits_after_right world decisions raises operate store after cell path op current present
        checked right value evaluated,
    fun store after cell path op current right failure present checked failed =>
      assignStatement_right_fails world decisions raises operate store after cell path op current present checked
        right failure failed,
    declareTarget_committed_iff world decisions, ?_, declareTarget_keeps_store world decisions,
    declareTarget_name world decisions, declareTarget_discard world decisions,
    setEnvironment_ok_iff raises, fun overlay next name value set => setEnvironment_effect raises overlay next name value set,
    fun overlay name value nul => setEnvironment_rejects_nul raises overlay name value nul,
    unwind_failed_primary, unwind_last_registered_runs_first, unwind_committed_first_failure,
    fun registered failure store uncaught => tryBlock_uncaught registered failure uncaught store, tryBlock_raised⟩
  intro store next mutability target source bindings selected committed
  unfold declareTarget at committed
  simp only [selected] at committed
  obtain ⟨_, _, placed, others⟩ := declareAll_committed world decisions store mutability bindings store next committed
  exact ⟨placed, others⟩

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness

def raises : Raises Unit Unit Unit where
  indexOutOfRange := .scalar (.string "index-out-of-range")
  missingKey := .scalar (.string "missing-field")
  schema := .scalar (.string "schema")
  nulArgv := .scalar (.string "nul-argv")

def pair : Operate Unit Unit Unit := fun op current right =>
  match op with
  | .div => .error (.scalar (.string "division-by-zero"))
  | _ => .ok (.list [current, right])

def jobsType : ValueType Unit := .record (.rowField "jobs" (.primitive .unsignedInteger) .rowEmpty)

theorem jobs_member (value : Int) (inRange : Xsh.Values.signed64Min ≤ value ∧ value < Xsh.Values.signed64Limit)
    (nonnegative : 0 ≤ value) : HasType witnessWorld jobsType (.record [("jobs", integer value inRange)]) := by
  refine ⟨fun _ _ => trivial, ?_⟩
  simpa [jobsType, integer, Membership.value, Membership.row, Membership.recordFields, Membership.primitive,
    Value.eraseNominal, List.lookup] using nonnegative

def config : Cell witnessWorld := ⟨⟨jobsType, .record [("jobs", zero)], jobs_member 0 (by decide) (by decide)⟩, .mutable⟩

def configStore : Store witnessWorld := fun cell => if cell = counterCell then some config else none

theorem configStore_present : configStore counterCell = some config := by simp [configStore]

theorem field_write_commits_and_reads_back :
    ∃ next, assignPath witnessWorld witnessDecisions raises pair configStore counterCell [.field "jobs"] .set five =
        .committed next ∧
      ∃ replaced, next counterCell = some replaced ∧ readPath replaced.contents.value [.field "jobs"] = some five := by
  have updated : update witnessWorld.membership raises (leafUpdate raises pair .set five) jobsType
      (.record [("jobs", zero)]) [.field "jobs"] = .ok (.record [("jobs", five)]) := by
    simp [update, recordRow, jobsType, rowEntries, Membership.recordFields, Value.eraseNominal, List.lookup,
      leafUpdate, Except.map, replaceField]
  obtain ⟨next, committed⟩ := (assignPath_committed_iff witnessWorld witnessDecisions raises pair configStore
    counterCell [.field "jobs"] .set five).mpr
    ⟨config, _, configStore_present, rfl, updated, jobs_member 5 (by decide) (by decide)⟩
  obtain ⟨current, written, present, updated', ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
    assignPath_committed_effect witnessWorld witnessDecisions raises pair configStore next counterCell
      [.field "jobs"] .set five committed
  refine ⟨next, committed, replaced, replacedAt, ?_⟩
  rw [replacedValue]
  exact update_set_reads witnessWorld.membership raises pair five [.field "jobs"] _ _ _ updated'

theorem negative_field_write_is_a_type_error :
    assignPath witnessWorld witnessDecisions raises pair configStore counterCell [.field "jobs"] .set negativeOne =
      .failed (.typeError jobsType (.record [("jobs", negativeOne)])) configStore := by
  have updated : update witnessWorld.membership raises (leafUpdate raises pair .set negativeOne) jobsType
      (.record [("jobs", zero)]) [.field "jobs"] = .ok (.record [("jobs", negativeOne)]) := by
    simp [update, recordRow, jobsType, rowEntries, Membership.recordFields, Value.eraseNominal, List.lookup,
      leafUpdate, Except.map, replaceField]
  have rejected : ¬ HasType witnessWorld jobsType (.record [("jobs", negativeOne)]) := by
    intro typed
    have member := typed.2
    simp [jobsType, negativeOne, integer, Membership.value, Membership.row, Membership.recordFields,
      Membership.primitive, Value.eraseNominal, List.lookup] at member
  unfold assignPath
  simp only [configStore_present, config, updated]
  exact assign_rejects_nonmember witnessWorld witnessDecisions configStore counterCell _ config configStore_present rfl
    rejected

def listCell : Cell witnessWorld :=
  ⟨⟨.list (.primitive .integer), .list [zero], ⟨fun _ _ => trivial,
    by simp [zero, integer, Membership.value, Membership.primitive, Value.eraseNominal]⟩⟩, .mutable⟩

def listStore : Store witnessWorld := fun cell => if cell = counterCell then some listCell else none

theorem out_of_range_index_is_not_caught :
    assignPath witnessWorld witnessDecisions raises pair listStore counterCell [.index 1] .set five =
        .failed (.runtime raises.indexOutOfRange) listStore ∧
      assignPath witnessWorld witnessDecisions raises pair listStore counterCell [.index (-1)] .set five =
        .failed (.runtime raises.indexOutOfRange) listStore ∧
      tryBlock [] (assignPath witnessWorld witnessDecisions raises pair listStore counterCell [.index 1] .set five) =
        .propagated (.runtime raises.indexOutOfRange) listStore := by
  have past : assignPath witnessWorld witnessDecisions raises pair listStore counterCell [.index 1] .set five =
      .failed (.runtime raises.indexOutOfRange) listStore := by
    simp [assignPath, listStore, listCell, update, elementType, listValues, Value.eraseNominal]
  refine ⟨past, by simp [assignPath, listStore, listCell, update, elementType, listValues, Value.eraseNominal], ?_⟩
  rw [past]
  rfl

def anyCell (value : Value Unit Unit Unit) : Cell witnessWorld :=
  ⟨⟨.primitive .any, value, ⟨fun _ _ => trivial, by simp [Membership.value, Membership.primitive]⟩⟩, .mutable⟩

def one : Value Unit Unit Unit := integer 1 (by decide)
def ten : Value Unit Unit Unit := integer 10 (by decide)

def oneStore : Store witnessWorld := fun cell => if cell = counterCell then some (anyCell one) else none

theorem compound_division_failure_is_not_caught :
    tryBlock [] (assignPath witnessWorld witnessDecisions raises pair oneStore counterCell [] .div zero) =
      .propagated (.runtime (.scalar (.string "division-by-zero"))) oneStore := by
  have present : oneStore counterCell = some (anyCell one) := by simp [oneStore]
  rw [assignPath_compound_reads_at_commit witnessWorld witnessDecisions raises pair oneStore counterCell .div
    (by decide) zero (anyCell one) present rfl]
  rfl

def stringMap : ValueType Unit := .map (.primitive .string) (.primitive .integer)

def mapCell (entries : List (MapKey × Value Unit Unit Unit)) (typed : HasType witnessWorld stringMap (.map entries)) :
    Cell witnessWorld := ⟨⟨stringMap, .map entries, typed⟩, .mutable⟩

theorem b_map_typed : HasType witnessWorld stringMap (.map [(.string "b", zero)]) :=
  ⟨fun _ _ => trivial, by simp [stringMap, zero, integer, Membership.value, Membership.key, Membership.primitive,
    MapKey.scalar, Value.eraseNominal]⟩

def bStore : Store witnessWorld := fun cell => if cell = counterCell then some (mapCell _ b_map_typed) else none

theorem missing_key_compound_is_not_caught :
    tryBlock [] (assignPath witnessWorld witnessDecisions raises pair bStore counterCell [.key (.string "a")] .add one) =
      .propagated (.runtime raises.missingKey) bStore := by
  have different : (MapKey.string "a" == MapKey.string "b") = false := by decide
  simp [tryBlock, unwind, runCleanups, recover, assignPath, bStore, mapCell, update, mapTypes, mapEntries,
    stringMap, Value.eraseNominal, List.lookup, different, leafUpdate, Except.map]

theorem missing_key_set_inserts_in_key_order :
    update witnessWorld.membership raises (leafUpdate raises pair .set five) stringMap (.map [(.string "b", zero)])
      [.key (.string "a")] = .ok (.map [(.string "a", five), (.string "b", zero)]) := by
  have different : (MapKey.string "a" == MapKey.string "b") = false := by decide
  have before : keyBefore (.string "a") (.string "b") = true := by decide
  simp [update, mapTypes, mapEntries, stringMap, Value.eraseNominal, List.lookup, different, leafUpdate, Except.map,
    insertOrdered, before]

def unsignedMap : ValueType Unit := .map (.primitive .unsignedInteger) (.primitive .integer)

def keyedCell : Cell witnessWorld :=
  ⟨⟨unsignedMap, .map [], ⟨fun _ _ => trivial, by simp [unsignedMap, Membership.value, Value.eraseNominal]⟩⟩, .mutable⟩

def keyedStore : Store witnessWorld := fun cell => if cell = counterCell then some keyedCell else none

def assignTen : RightSide witnessWorld := fun store =>
  match assign witnessWorld witnessDecisions store otherCell ten with
  | .committed next => .ok (one, next)
  | _ => .ok (one, store)

theorem negative_unsigned_key_fails_before_the_right_side :
    assignStatement witnessWorld witnessDecisions raises pair keyedStore counterCell
        [.key (.integer ⟨-1, by decide⟩)] .set assignTen = .failed (.raised raises.schema) keyedStore ∧
      tryBlock [] (assignStatement witnessWorld witnessDecisions raises pair keyedStore counterCell
        [.key (.integer ⟨-1, by decide⟩)] .set assignTen) = .caught raises.schema keyedStore := by
  have failed := assignStatement_key_failure witnessWorld witnessDecisions raises pair keyedStore counterCell
    [.key (.integer ⟨-1, by decide⟩)] .set keyedCell (by simp [keyedStore]) raises.schema
    (by simp [keyFailure, keyedCell, unsignedMap, mapTypes, negativeUnsignedKey]) assignTen
  exact ⟨failed, by rw [failed]; rfl⟩

def increments : RightSide witnessWorld := fun store =>
  match assign witnessWorld witnessDecisions store counterCell ten with
  | .committed next => .ok (one, next)
  | .failed failure after => .error (failure, after)
  | .internal _ after => .ok (one, after)

def readThenIncrement : RightSide witnessWorld := fun store =>
  match store counterCell with
  | some current =>
    match increments store with
    | .ok (right, after) => .ok (.list [current.contents.value, right], after)
    | .error failure => .error failure
  | none => .ok (one, store)

theorem compound_reads_the_target_after_the_right_side :
    (∃ next, assignStatement witnessWorld witnessDecisions raises pair oneStore counterCell [] .add increments =
        .committed next ∧ (next counterCell).map (fun cell => cell.contents.value) = some (.list [ten, one])) ∧
    (∃ next, assignStatement witnessWorld witnessDecisions raises pair oneStore counterCell [] .set readThenIncrement =
        .committed next ∧ (next counterCell).map (fun cell => cell.contents.value) = some (.list [one, one])) := by
  have anyTyped : ∀ value, HasType witnessWorld (.primitive .any) value :=
    fun _ => ⟨fun _ _ => trivial, by simp [Membership.value, Membership.primitive]⟩
  have present : oneStore counterCell = some (anyCell one) := by simp [oneStore]
  obtain ⟨tenStore, tenCommitted⟩ := (assign_committed_iff witnessWorld witnessDecisions oneStore counterCell ten).mpr
    ⟨anyCell one, present, rfl, anyTyped _⟩
  obtain ⟨found, foundPresent, ⟨tenCell, tenAt, tenType', tenValue, tenMutability⟩, _⟩ :=
    assign_committed_effect witnessWorld witnessDecisions oneStore tenStore counterCell ten tenCommitted
  rw [present, Option.some.injEq] at foundPresent
  subst foundPresent
  have tenMutable : tenCell.mutability = .mutable := tenMutability
  have tenType : tenCell.contents.typeTag = .primitive .any := tenType'
  have rightRan : increments oneStore = .ok (one, tenStore) := by simp [increments, tenCommitted]
  have noKey : keyFailure raises (anyCell one).contents.typeTag [] = none := rfl
  constructor
  · rw [assignStatement_commits_after_right witnessWorld witnessDecisions raises pair oneStore tenStore counterCell []
      .add (anyCell one) present noKey increments one rightRan]
    have tenPresent : tenStore counterCell = some tenCell := tenAt
    rw [assignPath_compound_reads_at_commit witnessWorld witnessDecisions raises pair tenStore counterCell .add
      (by decide) one tenCell tenPresent tenMutable, tenValue]
    simp only [pair]
    obtain ⟨next, committed⟩ := (assign_committed_iff witnessWorld witnessDecisions tenStore counterCell
      (.list [ten, one])).mpr ⟨tenCell, tenPresent, tenMutable, by rw [tenType]; exact anyTyped _⟩
    obtain ⟨_, _, ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
      assign_committed_effect witnessWorld witnessDecisions tenStore next counterCell _ committed
    exact ⟨next, committed, by simp [replacedAt, replacedValue]⟩
  · have desugared : readThenIncrement oneStore = .ok (.list [one, one], tenStore) := by
      simp [readThenIncrement, present, anyCell, rightRan]
    rw [assignStatement_commits_after_right witnessWorld witnessDecisions raises pair oneStore tenStore counterCell []
      .set (anyCell one) present noKey readThenIncrement _ desugared,
      assignPath_whole_cell witnessWorld witnessDecisions raises pair tenStore counterCell]
    obtain ⟨next, committed⟩ := (assign_committed_iff witnessWorld witnessDecisions tenStore counterCell
      (.list [one, one])).mpr ⟨tenCell, tenAt, tenMutable, by rw [tenType]; exact anyTyped _⟩
    obtain ⟨_, _, ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
      assign_committed_effect witnessWorld witnessDecisions tenStore next counterCell _ committed
    exact ⟨next, committed, by simp [replacedAt, replacedValue]⟩

def entryWorld : World Unit Unit Unit where
  membership := {
    entryFields := fun _ => [("size", zero)]
    stream := fun _ _ => False
    callable := fun _ _ => False
    scheme := fun _ _ => False
    generic := fun _ _ => False
  }
  references := fun _ => True

def sizeType : ValueType Unit := .record (.rowField "size" (.primitive .integer) .rowEmpty)

theorem entry_field_write_publishes_the_entry_record :
    update entryWorld.membership raises (leafUpdate raises pair .set five) sizeType (.filesystemEntry ⟨()⟩)
      [.field "size"] = .ok (.record [("size", five)]) := by
  simp [update, recordRow, sizeType, rowEntries, Membership.recordFields, entryWorld, Value.eraseNominal, List.lookup,
    leafUpdate, Except.map, replaceField]

theorem record_held_in_a_string_map_takes_a_key_write :
    update witnessWorld.membership raises (leafUpdate raises pair .set five) stringMap (.record [("a", zero)])
      [.key (.string "b")] = .ok (.map [(.string "a", zero), (.string "b", five)]) := by
  have different : (MapKey.string "b" == MapKey.string "a") = false := by decide
  have notBefore : keyBefore (.string "b") (.string "a") = false := by decide
  simp [update, mapTypes, mapEntries, stringMap, Value.eraseNominal, List.lookup, different, leafUpdate, Except.map,
    insertOrdered, notBefore]

theorem nominal_field_write_is_not_a_typed_path :
    ¬ PathTyped (.nominal () (.rowField "jobs" (.primitive .unsignedInteger) .rowEmpty) : ValueType Unit)
      [.field "jobs"] := by
  simp [PathTyped, recordRow]

def mark (value : Value Unit Unit Unit) : Cleanup witnessWorld := fun store => .failed (.raised value) store

theorem last_registered_cleanup_runs_first :
    tryBlock [mark zero, mark five] (.committed store) = .caught five store := rfl

theorem environment_nul_is_not_caught :
    setEnvironment raises [] [67, 67] [99, 0] = .error (.runtime raises.nulArgv) ∧
      (Failure.runtime raises.nulArgv : Failure Unit Unit Unit).catchable = false ∧
      setEnvironment raises [([67, 67], [103])] [67, 67] [99] = .ok [([67, 67], [99])] := by
  refine ⟨by simp [setEnvironment], rfl, by simp [setEnvironment]⟩

def emptyStore : Store witnessWorld := fun _ => none

def firstStore : Store witnessWorld := fun cell => if cell = counterCell then some ⟨⟨.primitive .unsignedInteger, zero,
  member_of_nonnegative 0 (by decide) (by decide)⟩, .immutable⟩ else none

def pairTarget : Target Unit :=
  .record [("a", .name counterCell (.primitive .unsignedInteger)), ("b", .name otherCell (.primitive .unsignedInteger))]
    true

def partialDeclare (store : Store witnessWorld) : List (Binding Unit Unit Unit) → StoreStep witnessWorld
  | [] => .committed store
  | (cell, type, value) :: rest =>
    match declare witnessWorld witnessDecisions store cell .immutable type value with
    | .committed next => partialDeclare next rest
    | other => other

theorem first_declared : declare witnessWorld witnessDecisions emptyStore counterCell .immutable
    (.primitive .unsignedInteger) zero = .committed firstStore := by
  obtain ⟨next, committed⟩ := (declare_committed_iff witnessWorld witnessDecisions emptyStore counterCell .immutable
    (.primitive .unsignedInteger) zero).mpr ⟨rfl, member_of_nonnegative 0 (by decide) (by decide)⟩
  obtain ⟨⟨placed, placedAt, placedMutability, placedType, placedValue⟩, others⟩ :=
    declare_committed_effect witnessWorld witnessDecisions emptyStore next counterCell .immutable _ _ committed
  rw [committed]
  congr 1
  funext cell
  by_cases same : cell = counterCell
  · subst same
    rw [placedAt]
    simp only [firstStore, ite_true, Option.some.injEq]
    cases placed with
    | mk contents mutability =>
      cases contents
      simp_all
  · rw [others cell same]
    simp [same, emptyStore, firstStore]

theorem second_rejected : declare witnessWorld witnessDecisions firstStore otherCell .immutable
    (.primitive .unsignedInteger) negativeOne =
      .failed (.typeError (.primitive .unsignedInteger) negativeOne) firstStore :=
  declare_rejects_nonmember witnessWorld witnessDecisions firstStore otherCell .immutable
    (.primitive .unsignedInteger) negativeOne (by simp [firstStore, otherCell, counterCell, binder])
    (nonmember_of_negative (-1) (by decide) (by decide))

theorem destructuring_commits_all_or_nothing :
    declareTarget witnessWorld witnessDecisions emptyStore .immutable pairTarget
        (.record [("a", zero), ("b", negativeOne), ("x", five)]) =
      .failed (.typeError (.primitive .unsignedInteger) negativeOne) emptyStore := by
  simp [declareTarget, pairTarget, select, selectFields, Membership.recordFields, Value.eraseNominal, List.lookup]
  unfold declareAll
  rw [first_declared]
  simp only []
  unfold declareAll
  rw [second_rejected]

theorem non_atomic_destructuring_falsifies_store_keeping :
    (partialDeclare emptyStore [(counterCell, .primitive .unsignedInteger, zero),
        (otherCell, .primitive .unsignedInteger, negativeOne)]).storeAfter ≠ emptyStore := by
  unfold partialDeclare
  rw [first_declared]
  simp only []
  unfold partialDeclare
  rw [second_rejected]
  intro same
  have := congrFun same counterCell
  simp [StoreStep.storeAfter, firstStore, emptyStore] at this

def countFromEnd (items : List (Value Unit Unit Unit)) (position : Int) (right : Value Unit Unit Unit) :
    Except (PathError Unit Unit Unit) (Value Unit Unit Unit) :=
  let index := if position < 0 then items.length + position else position
  if 0 ≤ index ∧ index < items.length then .ok (.list (items.set index.toNat right))
  else .error (.runtime raises.indexOutOfRange)

theorem counting_from_the_end_falsifies_the_range_rule :
    elementType (.list (.primitive .integer) : ValueType Unit) = some (.primitive .integer) ∧
      listValues (.list [zero] : Value Unit Unit Unit) = some [zero] ∧ ¬ (0 ≤ (-1 : Int) ∧ (-1 : Int) < 1) ∧
      countFromEnd [zero] (-1) five ≠ .error (.runtime raises.indexOutOfRange) := by
  refine ⟨rfl, rfl, by decide, ?_⟩
  simp [countFromEnd]

def defaultingLeaf (right : Value Unit Unit Unit) :
    Option (Value Unit Unit Unit) → Except (PathError Unit Unit Unit) (Value Unit Unit Unit)
  | some current => pair .add current right |>.mapError .runtime
  | none => .ok right

theorem defaulting_missing_key_falsifies_the_missing_key_rule :
    (.map [(.string "b", zero)] : Value Unit Unit Unit).eraseNominal = .map [(.string "b", zero)] ∧
      update witnessWorld.membership raises (defaultingLeaf one) stringMap (.map [(.string "b", zero)])
        [.key (.string "a")] ≠ .error (.runtime raises.missingKey) := by
  have different : (MapKey.string "a" == MapKey.string "b") = false := by decide
  refine ⟨rfl, ?_⟩
  simp [update, mapTypes, mapEntries, stringMap, Value.eraseNominal, List.lookup, different, defaultingLeaf, Except.map]

def appendInsert (key : MapKey) (inserted : Value Unit Unit Unit) (entries : List (MapKey × Value Unit Unit Unit)) :
    List (MapKey × Value Unit Unit Unit) := entries ++ [(key, inserted)]

theorem appending_falsifies_key_order :
    ¬ ∃ earlier later, appendInsert (.string "a") five [(.string "b", zero)] = earlier ++ (.string "a", five) :: later ∧
      (∀ entry ∈ earlier, keyBefore (.string "a") entry.1 = false) := by
  rintro ⟨earlier, later, split, after⟩
  have before : keyBefore (.string "a") (.string "b") = true := by decide
  cases earlier with
  | nil => simp [appendInsert] at split
  | cons first rest =>
    simp only [appendInsert, List.cons_append, List.cons.injEq] at split
    have := after first (by simp)
    rw [← split.1] at this
    simp [before] at this

def keepLaterFailure {world : World Declaration Identity Source} : StoreStep world → StoreStep world → StoreStep world
  | _, cleanup@(.failed _ _) => cleanup
  | primary, cleanup => combine primary cleanup

theorem cleanup_failure_replacement_falsifies_the_primary_rule :
    keepLaterFailure (world := witnessWorld) (.failed (.typeError (.primitive .unsignedInteger) negativeOne) store)
        (mark zero store) = .failed (.raised zero) store ∧
      tryBlock [mark zero] (.failed (.typeError (.primitive .unsignedInteger) negativeOne) store) =
        .propagated (.typeError (.primitive .unsignedInteger) negativeOne) store :=
  ⟨rfl, rfl⟩

end Witness

end Xsh.Proofs.Core.Writes
