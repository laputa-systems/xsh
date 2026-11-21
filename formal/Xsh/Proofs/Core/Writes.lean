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

theorem rebuild_eraseNominal (value inner : Value Declaration Identity Source) :
    (rebuild value inner).eraseNominal = inner.eraseNominal := by
  cases value with
  | nominal declaration contents =>
    simp only [rebuild, Value.eraseNominal]
    exact rebuild_eraseNominal contents inner
  | _ => rfl
termination_by value

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

theorem lookup_append_absent {α : Type} (key : MapKey) (inserted : α) :
    ∀ {entries : List (MapKey × α)}, entries.lookup key = none → (entries ++ [(key, inserted)]).lookup key = some inserted
  | [], _ => by simp [List.lookup]
  | (other, value) :: rest, absent => by
    by_cases same : key = other
    · subst same
      simp [List.lookup] at absent
    · have differentKeys : (key == other) = false := by simpa using same
      simp only [List.lookup, differentKeys] at absent
      simp only [List.cons_append, List.lookup, differentKeys]
      exact lookup_append_absent key inserted absent

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

theorem update_set_reads (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (right : Value Declaration Identity Source) :
    ∀ (path : List Selector) (type : ValueType Declaration) (value updated : Value Declaration Identity Source),
      update raises (leafUpdate raises operate .set right) type value path = .ok updated →
        readPath updated path = some right
  | [], _, value, updated, updated' => by
    simp only [update, leafUpdate, Except.ok.injEq] at updated'
    subst updated'
    rfl
  | .field name :: rest, type, value, updated, updated' => by
    simp only [update] at updated'
    split at updated'
    · rename_i row fields _ erased
      split at updated'
      · rename_i child childType present _
        obtain ⟨replaced, inner, rfl⟩ := map_ok updated'
        simp only [readPath, rebuild_eraseNominal, Value.eraseNominal,
          lookup_replaceField_same name replaced present, Option.bind_some]
        exact update_set_reads raises operate right rest childType child replaced inner
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
        exact update_set_reads raises operate right rest element _ replaced inner
      · simp at updated'
    · simp at updated'
  | .key key :: rest, type, value, updated, updated' => by
    simp only [update] at updated'
    split at updated'
    · rename_i keyType element entries _ erased
      split at updated'
      · simp at updated'
      · split at updated'
        · obtain ⟨replaced, inner, rfl⟩ := map_ok updated'
          rename_i present
          simp only [readPath, Value.eraseNominal, lookup_replaceEntry_same key replaced present, Option.bind_some]
          exact update_set_reads raises operate right _ element _ replaced inner
        · rename_i absent
          obtain ⟨inserted, inner, rfl⟩ := map_ok updated'
          simp only [leafUpdate, Except.ok.injEq] at inner
          subst inner
          simp [readPath, Value.eraseNominal, lookup_append_absent key right absent]
        · simp at updated'
    · simp at updated'

theorem update_raised (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (op : Xsh.Syntax.AssignOp) (right : Value Declaration Identity Source) :
    ∀ (path : List Selector) (type : ValueType Declaration) (value : Value Declaration Identity Source) error,
      update raises (leafUpdate raises operate op right) type value path = .error (.raised error) →
        error = raises.indexOutOfRange ∨ error = raises.schema ∨ error = raises.mapMissing ∨
          ∃ current, operate op current right = .error error := by
  have leafRaised : ∀ current error, leafUpdate raises operate op right current = .error (.raised error) →
      error = raises.indexOutOfRange ∨ error = raises.schema ∨ error = raises.mapMissing ∨
        ∃ current, operate op current right = .error error := by
    intro current error raised
    cases op with
    | set => simp [leafUpdate] at raised
    | _ =>
      cases current with
      | none =>
        simp only [leafUpdate, Except.error.injEq, PathError.raised.injEq] at raised
        exact Or.inr (Or.inr (Or.inl raised.symm))
      | some value =>
        simp only [leafUpdate] at raised
        split at raised
        · simp at raised
        · rename_i failure failed
          simp only [Except.error.injEq, PathError.raised.injEq] at raised
          subst raised
          exact Or.inr (Or.inr (Or.inr ⟨value, failed⟩))
  intro path
  induction path with
  | nil => exact fun _ value error raised => leafRaised (some value) error raised
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
        · simp only [Except.error.injEq, PathError.raised.injEq] at raised
          exact Or.inl raised.symm
      · simp at raised
    | key key =>
      simp only [update] at raised
      split at raised
      · split at raised
        · simp only [Except.error.injEq, PathError.raised.injEq] at raised
          exact Or.inr (Or.inl raised.symm)
        · split at raised
          · exact ih _ _ error (map_error raised)
          · exact leafRaised none error (map_error raised)
          · simp only [Except.error.injEq, PathError.raised.injEq] at raised
            exact Or.inr (Or.inr (Or.inl raised.symm))
      · simp at raised

theorem update_index_out_of_range (raises : Raises Declaration Identity Source)
    (leaf : Option (Value Declaration Identity Source) →
      Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
    {type element : ValueType Declaration} {value : Value Declaration Identity Source}
    {items : List (Value Declaration Identity Source)} (position : Int) (rest : List Selector)
    (typed : elementType type = some element) (listed : listValues value = some items)
    (outside : ¬ (0 ≤ position ∧ position < items.length)) :
    update raises leaf type value (.index position :: rest) = .error (.raised raises.indexOutOfRange) := by
  simp only [update, typed, listed, outside, dite_false]

theorem update_negative_unsigned_key (raises : Raises Declaration Identity Source)
    (leaf : Option (Value Declaration Identity Source) →
      Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
    {type element : ValueType Declaration} {value : Value Declaration Identity Source}
    {entries : List (MapKey × Value Declaration Identity Source)} (key : Xsh.Values.IntValue) (rest : List Selector)
    (typed : mapTypes type = some (.primitive .unsignedInteger, element)) (erased : value.eraseNominal = .map entries)
    (negative : key.val < 0) :
    update raises leaf type value (.key (.integer key) :: rest) = .error (.raised raises.schema) := by
  simp [update, typed, erased, negativeUnsignedKey, negative]

end Paths

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
      | .error error => .failed (.raised error) store := by
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
        update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
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
      update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
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
      ((∃ error, failure = .raised error ∧
          update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
            .error (.raised error)) ∨
        ∃ updated, update raises (leafUpdate raises operate op right) current.contents.typeTag
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
    setEnvironment raises overlay name value = .error (.raised raises.envValue) := by
  simp [setEnvironment, nul]

end Environment

section Unwinding

theorem unwind_failed_primary {world : World Declaration Identity Source} :
    ∀ (cleanups : List (Cleanup world)) (failure : Failure Declaration Identity Source) (store : Store world),
      (∃ after, unwind cleanups (.failed failure store) = .failed failure after) ∨
        ∃ fault after, unwind cleanups (.failed failure store) = .internal fault after
  | [], failure, store => Or.inl ⟨store, rfl⟩
  | cleanup :: rest, failure, store => by
    simp only [unwind]
    cases ran : cleanup (StoreStep.storeAfter (.failed failure store)) with
    | internal fault after =>
      simp only [combine]
      exact Or.inr (internal_stays rest fault after)
    | committed after => exact unwind_failed_primary rest failure after
    | failed other after => exact unwind_failed_primary rest failure after
where
  internal_stays : ∀ (cleanups : List (Cleanup world)) fault (store : Store world),
      ∃ fault' after, unwind cleanups (.internal fault store) = .internal fault' after
    | [], fault, store => ⟨fault, store, rfl⟩
    | _ :: rest, fault, store => by simpa [unwind] using internal_stays rest fault store

theorem unwind_cleanup_sees_left_store {world : World Declaration Identity Source} (cleanup : Cleanup world)
    (rest : List (Cleanup world)) (failure : Failure Declaration Identity Source) (store : Store world) :
    unwind (cleanup :: rest) (.failed failure store) = unwind rest (combine (.failed failure store) (cleanup store)) :=
  rfl

theorem unwind_committed_first_failure {world : World Declaration Identity Source} (cleanup : Cleanup world)
    (rest : List (Cleanup world)) (store : Store world) :
    unwind (cleanup :: rest) (.committed store) = unwind rest (combine (.committed store) (cleanup store)) :=
  rfl

theorem tryBlock_type_error {world : World Declaration Identity Source} (cleanups : List (Cleanup world))
    (expected : ValueType Declaration) (candidate : Value Declaration Identity Source) (store : Store world) :
    (∃ after, tryBlock cleanups (.failed (.typeError expected candidate) store) =
        .propagated (.typeError expected candidate) after) ∨
      ∃ fault after, tryBlock cleanups (.failed (.typeError expected candidate) store) = .internal fault after := by
  unfold tryBlock
  rcases unwind_failed_primary cleanups (.typeError expected candidate) store with ⟨after, failed⟩ | ⟨fault, after, internal⟩
  · exact Or.inl ⟨after, by rw [failed]; rfl⟩
  · exact Or.inr ⟨fault, after, by rw [internal]; rfl⟩

theorem tryBlock_raised {world : World Declaration Identity Source} (cleanups : List (Cleanup world))
    (error : Value Declaration Identity Source) (store : Store world) :
    (∃ after, tryBlock cleanups (.failed (.raised error) store) = .caught error after) ∨
      ∃ fault after, tryBlock cleanups (.failed (.raised error) store) = .internal fault after := by
  unfold tryBlock
  rcases unwind_failed_primary cleanups (.raised error) store with ⟨after, failed⟩ | ⟨fault, after, internal⟩
  · exact Or.inl ⟨after, by rw [failed]; rfl⟩
  · exact Or.inr ⟨fault, after, by rw [internal]; rfl⟩

end Unwinding

section Root

variable [DecidableEq Declaration]

/-- Environments and every store-writing form of the frozen core over every
Core world, type and value. Name resolution, lookup, whole-cell assignment and
declaration are `environments_and_typed_stores`. A path assignment
`target.selectors op= right` commits exactly when its updated cell value is a
member of the cell's type; the commit replaces only that cell, a `=` update
reads back the assigned value at its path, and a compound operator reads the
target's value at commit. A list index outside the list raises
`index-out-of-range`, a negative key into `Map[UInt, V]` raises the schema
error, a missing key under a deeper selector or a compound update raises the
missing-key error, and any other raised error is the operator's. A
destructuring declaration selects every field before it allocates, commits
all its cells or none, and `_` binds nothing. An environment assignment
rejects a NUL value and otherwise overrides only its name. Every failure keeps
the store; unwinding runs cleanups innermost first on the store the failure
left, keeps an earlier failure primary, and `try` catches raised errors while
`type-error` propagates. -/
theorem environments_and_store_writes (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source) :
    ((∀ (environment : LexicalEnvironment) name binder,
      resolve (bind environment name binder) name = some binder) ∧
    (∀ (environment : LexicalEnvironment) name other binder, other ≠ name →
      resolve (bind environment name binder) other = resolve environment other) ∧
    (∀ (environment : LexicalEnvironment) name binder other,
      resolve (exitScope (bind (enterScope environment) name binder)) other = resolve environment other) ∧
    (∀ (environment : ActivationEnvironment) binder cell,
      resolveCell (bindCell environment binder cell) binder = some cell) ∧
    (∀ (environment : ActivationEnvironment) binder other cell, other ≠ binder →
      resolveCell (bindCell environment binder cell) other = resolveCell environment other) ∧
    (∀ (environment : ActivationEnvironment) binders binder,
      capture environment binders binder =
        if binder ∈ binders then resolveCell environment binder else none) ∧
    (∀ environment (store : Store world) name slot,
      lookup world environment store name = .ok slot ↔
        ∃ binder cell stored, resolve environment.names name = some binder ∧
          resolveCell environment.cells binder = some cell ∧ store cell = some stored ∧
          stored.contents = slot) ∧
    (∀ (store : Store world) cell stored, store cell = some stored →
      stored.contents.value.ReferencesValid world.references ∧
        Membership.value world.membership stored.contents.typeTag stored.contents.value) ∧
    (∀ (store : Store world) cell candidate,
      (∃ next, assign world decisions store cell candidate = .committed next) ↔
        ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
          candidate.ReferencesValid world.references ∧
          Membership.value world.membership current.contents.typeTag candidate) ∧
    (∀ (store next : Store world) cell candidate,
      assign world decisions store cell candidate = .committed next →
        ∃ current, store cell = some current ∧
          (∃ replaced, next cell = some replaced ∧
            replaced.contents.typeTag = current.contents.typeTag ∧
            replaced.contents.value = candidate ∧ replaced.mutability = current.mutability) ∧
          ∀ other, other ≠ cell → next other = store other) ∧
    (∀ (store : Store world) cell candidate (current : Cell world), store cell = some current →
      current.mutability = .mutable → ¬ HasType world current.contents.typeTag candidate →
      assign world decisions store cell candidate =
        .failed (.typeError current.contents.typeTag candidate) store) ∧
    (∀ (store after : Store world) cell candidate failure,
      assign world decisions store cell candidate = .failed failure after →
        after = store ∧ failure.catchable = false) ∧
    (∀ (store : Store world) cell candidate,
      (∀ next, assign world decisions store cell candidate ≠ .committed next) →
        (assign world decisions store cell candidate).storeAfter = store) ∧
    (∀ (store : Store world) cell mutability type candidate,
      (∃ next, declare world decisions store cell mutability type candidate = .committed next) ↔
        store cell = none ∧ candidate.ReferencesValid world.references ∧
          Membership.value world.membership type candidate) ∧
    (∀ (store next : Store world) cell mutability type candidate,
      declare world decisions store cell mutability type candidate = .committed next →
        (∃ current, next cell = some current ∧ current.mutability = mutability ∧
          current.contents.typeTag = type ∧ current.contents.value = candidate) ∧
        ∀ other, other ≠ cell → next other = store other) ∧
    (∀ (store : Store world) cell mutability type candidate, store cell = none →
      ¬ HasType world type candidate →
      declare world decisions store cell mutability type candidate =
        .failed (.typeError type candidate) store) ∧
    (∀ expected candidate (store : Store world),
      recover (.failed (.typeError expected candidate) store) =
        .propagated (.typeError expected candidate) store) ∧
    (∀ failure (store : Store world),
      (∃ error, recover (.failed failure store) = .caught error store) ↔ failure.catchable = true)) ∧
    (∀ (store : Store world) cell right,
      assignPath world decisions raises operate store cell [] .set right = assign world decisions store cell right) ∧
    (∀ (store : Store world) cell path op right,
      (∃ next, assignPath world decisions raises operate store cell path op right = .committed next) ↔
        ∃ current updated, store cell = some current ∧ current.mutability = .mutable ∧
          update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
            .ok updated ∧ HasType world current.contents.typeTag updated) ∧
    (∀ (store next : Store world) cell path op right,
      assignPath world decisions raises operate store cell path op right = .committed next →
        ∃ current updated, store cell = some current ∧
          update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
            .ok updated ∧
          (∃ replaced, next cell = some replaced ∧ replaced.contents.typeTag = current.contents.typeTag ∧
            replaced.contents.value = updated ∧ replaced.mutability = current.mutability) ∧
          ∀ other, other ≠ cell → next other = store other) ∧
    (∀ (store : Store world) cell path op right,
      (∀ next, assignPath world decisions raises operate store cell path op right ≠ .committed next) →
        (assignPath world decisions raises operate store cell path op right).storeAfter = store) ∧
    (∀ (store after : Store world) cell path op right failure,
      assignPath world decisions raises operate store cell path op right = .failed failure after →
        after = store ∧ ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
          ((∃ error, failure = .raised error ∧
              update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path =
                .error (.raised error)) ∨
            ∃ updated, update raises (leafUpdate raises operate op right) current.contents.typeTag
                current.contents.value path = .ok updated ∧
              ¬ HasType world current.contents.typeTag updated ∧
              failure = .typeError current.contents.typeTag updated)) ∧
    (∀ path type (value updated right : Value Declaration Identity Source),
      update raises (leafUpdate raises operate .set right) type value path = .ok updated →
        readPath updated path = some right) ∧
    (∀ path type (value : Value Declaration Identity Source) op right error,
      update raises (leafUpdate raises operate op right) type value path = .error (.raised error) →
        error = raises.indexOutOfRange ∨ error = raises.schema ∨ error = raises.mapMissing ∨
          ∃ current, operate op current right = .error error) ∧
    (∀ (leaf : Option (Value Declaration Identity Source) →
        Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
      type element (value : Value Declaration Identity Source) items (position : Int) rest,
      elementType type = some element → listValues value = some items →
      ¬ (0 ≤ position ∧ position < items.length) →
        update raises leaf type value (.index position :: rest) = .error (.raised raises.indexOutOfRange)) ∧
    (∀ (leaf : Option (Value Declaration Identity Source) →
        Except (PathError Declaration Identity Source) (Value Declaration Identity Source))
      type element (value : Value Declaration Identity Source) entries (key : Xsh.Values.IntValue) rest,
      mapTypes type = some (.primitive .unsignedInteger, element) → value.eraseNominal = .map entries →
      key.val < 0 → update raises leaf type value (.key (.integer key) :: rest) = .error (.raised raises.schema)) ∧
    (∀ (store : Store world) cell op right (current : Cell world), op ≠ .set → store cell = some current →
      current.mutability = .mutable →
        assignPath world decisions raises operate store cell [] op right =
          match operate op current.contents.value right with
          | .ok result => assign world decisions store cell result
          | .error error => .failed (.raised error) store) ∧
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
      setEnvironment raises overlay name value = .error (.raised raises.envValue)) ∧
    (∀ (cleanups : List (Cleanup world)) failure (store : Store world),
      (∃ after, unwind cleanups (.failed failure store) = .failed failure after) ∨
        ∃ fault after, unwind cleanups (.failed failure store) = .internal fault after) ∧
    (∀ (cleanup : Cleanup world) rest failure (store : Store world),
      unwind (cleanup :: rest) (.failed failure store) =
        unwind rest (combine (.failed failure store) (cleanup store))) ∧
    (∀ (cleanups : List (Cleanup world)) expected candidate (store : Store world),
      (∃ after, tryBlock cleanups (.failed (.typeError expected candidate) store) =
          .propagated (.typeError expected candidate) after) ∨
        ∃ fault after, tryBlock cleanups (.failed (.typeError expected candidate) store) = .internal fault after) ∧
    (∀ (cleanups : List (Cleanup world)) error (store : Store world),
      (∃ after, tryBlock cleanups (.failed (.raised error) store) = .caught error after) ∨
        ∃ fault after, tryBlock cleanups (.failed (.raised error) store) = .internal fault after) := by
  refine ⟨environments_and_typed_stores world decisions, assignPath_whole_cell world decisions raises operate,
    assignPath_committed_iff world decisions raises operate,
    fun store next cell path op right committed =>
      assignPath_committed_effect world decisions raises operate store next cell path op right committed,
    assignPath_keeps_store world decisions raises operate,
    fun store after cell path op right failure failed =>
      assignPath_failed world decisions raises operate store after cell path op right failure failed,
    fun path type value updated right => update_set_reads raises operate right path type value updated,
    fun path type value op right error => update_raised raises operate op right path type value error,
    fun leaf _ _ _ _ position rest typed listed outside =>
      update_index_out_of_range raises leaf position rest typed listed outside,
    fun leaf _ _ _ _ key rest typed erased negative =>
      update_negative_unsigned_key raises leaf key rest typed erased negative,
    fun store cell op right current compound present mutableCell =>
      assignPath_compound_reads_at_commit world decisions raises operate store cell op compound right current present
        mutableCell,
    declareTarget_committed_iff world decisions, ?_, declareTarget_keeps_store world decisions,
    declareTarget_name world decisions, declareTarget_discard world decisions,
    setEnvironment_ok_iff raises, fun overlay next name value set => setEnvironment_effect raises overlay next name value set,
    fun overlay name value nul => setEnvironment_rejects_nul raises overlay name value nul,
    unwind_failed_primary, unwind_cleanup_sees_left_store, tryBlock_type_error, tryBlock_raised⟩
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
  schema := .scalar (.string "schema")
  mapMissing := .scalar (.string "map-missing")
  envValue := .scalar (.string "env-value")

def pair : Operate Unit Unit Unit := fun _ current right => .ok (.list [current, right])

def jobsType : ValueType Unit := .record (.rowField "jobs" (.primitive .unsignedInteger) .rowEmpty)

theorem jobs_member (value : Int) (inRange : Xsh.Values.signed64Min ≤ value ∧ value < Xsh.Values.signed64Limit)
    (nonnegative : 0 ≤ value) : HasType witnessWorld jobsType (.record [("jobs", integer value inRange)]) := by
  refine ⟨fun _ _ => trivial, ?_⟩
  · simpa [jobsType, integer, Membership.value, Membership.row, Membership.recordFields, Membership.primitive,
      Value.eraseNominal, List.lookup] using nonnegative

def config : Cell witnessWorld := ⟨⟨jobsType, .record [("jobs", zero)], jobs_member 0 (by decide) (by decide)⟩, .mutable⟩

def configStore : Store witnessWorld := fun cell => if cell = counterCell then some config else none

theorem configStore_present : configStore counterCell = some config := by simp [configStore]

theorem field_write_commits_and_reads_back :
    ∃ next, assignPath witnessWorld witnessDecisions raises pair configStore counterCell [.field "jobs"] .set five =
        .committed next ∧
      ∃ replaced, next counterCell = some replaced ∧ readPath replaced.contents.value [.field "jobs"] = some five := by
  have updated : update raises (leafUpdate raises pair .set five) jobsType (.record [("jobs", zero)])
      [.field "jobs"] = .ok (.record [("jobs", five)]) := by
    simp [update, recordRow, jobsType, rowEntries, Value.eraseNominal, List.lookup, leafUpdate, Except.map,
      rebuild, replaceField]
  obtain ⟨next, committed⟩ := (assignPath_committed_iff witnessWorld witnessDecisions raises pair configStore
    counterCell [.field "jobs"] .set five).mpr
    ⟨config, _, configStore_present, rfl, updated, jobs_member 5 (by decide) (by decide)⟩
  obtain ⟨current, written, present, updated', ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
    assignPath_committed_effect witnessWorld witnessDecisions raises pair configStore next counterCell
      [.field "jobs"] .set five committed
  refine ⟨next, committed, replaced, replacedAt, ?_⟩
  rw [replacedValue]
  exact update_set_reads raises pair five [.field "jobs"] _ _ _ updated'

theorem negative_field_write_is_a_type_error :
    assignPath witnessWorld witnessDecisions raises pair configStore counterCell [.field "jobs"] .set negativeOne =
        .failed (.typeError jobsType (.record [("jobs", negativeOne)])) configStore ∧
      update raises (leafUpdate raises pair .set negativeOne) jobsType (.record [("jobs", zero)]) [.field "jobs"] =
        .ok (.record [("jobs", negativeOne)]) ∧
      ¬ HasType witnessWorld jobsType (.record [("jobs", negativeOne)]) := by
  have updated : update raises (leafUpdate raises pair .set negativeOne) jobsType (.record [("jobs", zero)])
      [.field "jobs"] = .ok (.record [("jobs", negativeOne)]) := by
    simp [update, recordRow, jobsType, rowEntries, Value.eraseNominal, List.lookup, leafUpdate, Except.map,
      rebuild, replaceField]
  have rejected : ¬ HasType witnessWorld jobsType (.record [("jobs", negativeOne)]) := by
    intro typed
    have member := typed.2
    simp [jobsType, negativeOne, integer, Membership.value, Membership.row, Membership.recordFields,
      Membership.primitive, Value.eraseNominal, List.lookup] at member
  refine ⟨?_, updated, rejected⟩
  unfold assignPath
  simp only [configStore_present, config, updated]
  exact assign_rejects_nonmember witnessWorld witnessDecisions configStore counterCell _ config configStore_present rfl
    rejected

def listCell : Cell witnessWorld :=
  ⟨⟨.list (.primitive .integer), .list [zero], ⟨fun _ _ => trivial,
    by simp [zero, integer, Membership.value, Membership.primitive, Value.eraseNominal]⟩⟩, .mutable⟩

def listStore : Store witnessWorld := fun cell => if cell = counterCell then some listCell else none

theorem index_past_the_end_and_negative_index_are_out_of_range :
    assignPath witnessWorld witnessDecisions raises pair listStore counterCell [.index 1] .set five =
        .failed (.raised raises.indexOutOfRange) listStore ∧
      assignPath witnessWorld witnessDecisions raises pair listStore counterCell [.index (-1)] .set five =
        .failed (.raised raises.indexOutOfRange) listStore := by
  constructor <;>
    simp [assignPath, listStore, listCell, update, elementType, listValues, Value.eraseNominal]

def keyedCell : Cell witnessWorld :=
  ⟨⟨.map (.primitive .unsignedInteger) (.primitive .integer), .map [], ⟨fun _ _ => trivial,
    by simp [Membership.value, Value.eraseNominal]⟩⟩, .mutable⟩

def keyedStore : Store witnessWorld := fun cell => if cell = counterCell then some keyedCell else none

theorem negative_unsigned_key_is_a_schema_error :
    assignPath witnessWorld witnessDecisions raises pair keyedStore counterCell [.key (.integer ⟨-1, by decide⟩)]
        .set five = .failed (.raised raises.schema) keyedStore := by
  simp [assignPath, keyedStore, keyedCell, update, mapTypes, negativeUnsignedKey, Value.eraseNominal]

def anyCell (value : Value Unit Unit Unit) : Cell witnessWorld :=
  ⟨⟨.primitive .any, value, ⟨fun _ _ => trivial, by simp [Membership.value, Membership.primitive]⟩⟩, .mutable⟩

def ten : Value Unit Unit Unit := integer 10 (by decide)
def one : Value Unit Unit Unit := integer 1 (by decide)

def beforeRight : Store witnessWorld := fun cell => if cell = counterCell then some (anyCell one) else none
def afterRight : Store witnessWorld := fun cell => if cell = counterCell then some (anyCell ten) else none

theorem compound_reads_the_target_after_the_right_side :
    (∃ next, assignPath witnessWorld witnessDecisions raises pair afterRight counterCell [] .add one = .committed next ∧
      (next counterCell).map (fun cell => cell.contents.value) = some (.list [ten, one])) ∧
    (∃ next, assign witnessWorld witnessDecisions afterRight counterCell (.list [one, one]) = .committed next ∧
      (next counterCell).map (fun cell => cell.contents.value) = some (.list [one, one])) := by
  have anyTyped : ∀ value, HasType witnessWorld (.primitive .any) value :=
    fun _ => ⟨fun _ _ => trivial, by simp [Membership.value, Membership.primitive]⟩
  have present : afterRight counterCell = some (anyCell ten) := by simp [afterRight]
  constructor
  · rw [assignPath_compound_reads_at_commit witnessWorld witnessDecisions raises pair afterRight counterCell .add
      (by decide) one (anyCell ten) present rfl]
    simp only [pair, anyCell]
    obtain ⟨next, committed⟩ := (assign_committed_iff witnessWorld witnessDecisions afterRight counterCell
      (.list [ten, one])).mpr ⟨anyCell ten, present, rfl, anyTyped _⟩
    obtain ⟨_, _, ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
      assign_committed_effect witnessWorld witnessDecisions afterRight next counterCell _ committed
    exact ⟨next, committed, by simp [replacedAt, replacedValue]⟩
  · obtain ⟨next, committed⟩ := (assign_committed_iff witnessWorld witnessDecisions afterRight counterCell
      (.list [one, one])).mpr ⟨anyCell ten, present, rfl, anyTyped _⟩
    obtain ⟨_, _, ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
      assign_committed_effect witnessWorld witnessDecisions afterRight next counterCell _ committed
    exact ⟨next, committed, by simp [replacedAt, replacedValue]⟩

def emptyStore : Store witnessWorld := fun _ => none

def firstStore : Store witnessWorld := fun cell => if cell = counterCell then some ⟨⟨.primitive .unsignedInteger, zero,
  member_of_nonnegative 0 (by decide) (by decide)⟩, .immutable⟩ else none

def pairTarget : Target Unit :=
  .record [("a", .name counterCell (.primitive .unsignedInteger)), ("b", .name otherCell (.primitive .unsignedInteger))]
    true

def partialDeclare (store : Store witnessWorld) : List (Binding Unit Unit Unit) → Store witnessWorld
  | [] => store
  | (cell, type, value) :: rest =>
    partialDeclare (declare witnessWorld witnessDecisions store cell .immutable type value).storeAfter rest

theorem destructuring_commits_all_or_nothing :
    declareTarget witnessWorld witnessDecisions emptyStore .immutable pairTarget
        (.record [("a", zero), ("b", negativeOne), ("x", five)]) =
      .failed (.typeError (.primitive .unsignedInteger) negativeOne) emptyStore ∧
    (partialDeclare emptyStore [(counterCell, .primitive .unsignedInteger, zero),
        (otherCell, .primitive .unsignedInteger, negativeOne)] counterCell).isSome := by
  have first : declare witnessWorld witnessDecisions emptyStore counterCell .immutable (.primitive .unsignedInteger)
      zero = .committed firstStore := by
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
  have secondFresh : firstStore otherCell = none := by
    simp [firstStore, otherCell, counterCell, binder]
  have second := declare_rejects_nonmember witnessWorld witnessDecisions firstStore otherCell .immutable
    (.primitive .unsignedInteger) negativeOne secondFresh (nonmember_of_negative (-1) (by decide) (by decide))
  constructor
  · simp [declareTarget, pairTarget, select, selectFields, Membership.recordFields, Value.eraseNominal,
      List.lookup]
    unfold declareAll
    rw [first]
    simp only []
    unfold declareAll
    rw [second]
  · unfold partialDeclare
    rw [first]
    unfold partialDeclare
    rw [StoreStep.storeAfter, second]
    simp [partialDeclare, StoreStep.storeAfter, firstStore]

def keepLaterFailure {world : World Declaration Identity Source} : StoreStep world → StoreStep world → StoreStep world
  | _, cleanup@(.failed _ _) => cleanup
  | primary, cleanup => combine primary cleanup

theorem cleanup_failure_does_not_replace_a_type_error :
    tryBlock [fun store => .failed (.raised zero) store]
        (.failed (.typeError (.primitive .unsignedInteger) negativeOne) store) =
      .propagated (.typeError (.primitive .unsignedInteger) negativeOne) store ∧
    recover (keepLaterFailure (world := witnessWorld)
        (.failed (.typeError (.primitive .unsignedInteger) negativeOne) store) (.failed (.raised zero) store)) =
      .caught zero store :=
  ⟨rfl, rfl⟩

theorem environment_assignment_rejects_nul :
    setEnvironment raises [] [67, 67] [99, 0] = .error (.raised raises.envValue) ∧
      setEnvironment raises [([67, 67], [103])] [67, 67] [99] = .ok [([67, 67], [99])] := by
  constructor <;> simp [setEnvironment]

end Witness

end Xsh.Proofs.Core.Writes
