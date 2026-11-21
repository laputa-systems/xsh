import Xsh.Semantics.CoreWorld

namespace Xsh.Proofs.Core.Env

open Xsh.Foundation
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

omit [DecidableEq Declaration] in
theorem lookup_ok_iff (world : World Declaration Identity Source) (environment : Environment)
    (store : Store world) (name : NameId) (slot : Slot world) :
    lookup world environment store name = .ok slot ↔
      ∃ binder cell stored, resolve environment.names name = some binder ∧
        resolveCell environment.cells binder = some cell ∧ store cell = some stored ∧
        stored.contents = slot := by
  unfold lookup Environment.cellOf
  constructor
  · intro found
    cases bound : resolve environment.names name with
    | none => simp [bound] at found
    | some binder =>
      cases active : resolveCell environment.cells binder with
      | none => simp [bound, active] at found
      | some cell =>
        cases present : store cell with
        | none => simp [bound, active, present] at found
        | some stored =>
          simp only [bound, active, present, Except.ok.injEq] at found
          exact ⟨binder, cell, stored, rfl, active, present, found⟩
  · rintro ⟨binder, cell, stored, bound, active, present, contents⟩
    simp [bound, active, present, contents]

theorem assign_committed_iff (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (cell : CellId) (candidate : Value Declaration Identity Source) :
    (∃ next, assign world decisions store cell candidate = .committed next) ↔
      ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
        HasType world current.contents.typeTag candidate := by
  constructor
  · rintro ⟨next, committed⟩
    unfold assign at committed
    cases present : store cell with
    | none => simp [present] at committed
    | some current =>
      rw [present] at committed
      cases mutability : current.mutability with
      | immutable => simp [mutability] at committed
      | mutable =>
        simp only [mutability] at committed
        cases written : Xsh.Core.Store.write world decisions store cell candidate with
        | rejected kept => simp [written] at committed
        | accepted updated =>
          obtain ⟨found, foundPresent, _, typed⟩ :=
            (Xsh.Semantics.Environments.write_accepted_iff _ store cell candidate).mp ⟨updated, written⟩
          rw [present, Option.some.injEq] at foundPresent
          subst foundPresent
          exact ⟨current, rfl, mutability, typed⟩
  · rintro ⟨current, present, mutableCell, typed⟩
    obtain ⟨updated, written⟩ :=
      (Xsh.Semantics.Environments.write_accepted_iff (Xsh.Core.Store.decideHasType world decisions)
        store cell candidate).mpr ⟨current, present, mutableCell, typed⟩
    refine ⟨updated, ?_⟩
    unfold assign
    simp only [present, mutableCell]
    rw [show Xsh.Core.Store.write world decisions store cell candidate = .accepted updated from written]

theorem assign_committed_effect (world : World Declaration Identity Source) (decisions : Decisions world)
    (store next : Store world) (cell : CellId) (candidate : Value Declaration Identity Source)
    (committed : assign world decisions store cell candidate = .committed next) :
    ∃ current, store cell = some current ∧
      (∃ replaced, next cell = some replaced ∧
        replaced.contents.typeTag = current.contents.typeTag ∧
        replaced.contents.value = candidate ∧ replaced.mutability = current.mutability) ∧
      ∀ other, other ≠ cell → next other = store other := by
  unfold assign at committed
  cases present : store cell with
  | none => simp [present] at committed
  | some current =>
    rw [present] at committed
    cases mutability : current.mutability with
    | immutable => simp [mutability] at committed
    | mutable =>
      simp only [mutability] at committed
      cases written : Xsh.Core.Store.write world decisions store cell candidate with
      | rejected kept => simp [written] at committed
      | accepted updated =>
        simp only [written, StoreStep.committed.injEq] at committed
        subst committed
        obtain ⟨found, foundPresent, rest⟩ :=
          write_accepted_replaces_only_its_cell _ store cell candidate updated written
        rw [present, Option.some.injEq] at foundPresent
        subst foundPresent
        exact ⟨current, rfl, rest⟩

theorem assign_rejects_nonmember (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (cell : CellId) (candidate : Value Declaration Identity Source)
    (current : Cell world) (present : store cell = some current) (mutableCell : current.mutability = .mutable)
    (rejected : ¬ HasType world current.contents.typeTag candidate) :
    assign world decisions store cell candidate = .failed (.typeError current.contents.typeTag candidate) store := by
  unfold assign
  simp only [present, mutableCell]
  cases written : Xsh.Core.Store.write world decisions store cell candidate with
  | accepted updated =>
    obtain ⟨found, foundPresent, _, typed⟩ :=
      (Xsh.Semantics.Environments.write_accepted_iff _ store cell candidate).mp ⟨updated, written⟩
    rw [present, Option.some.injEq] at foundPresent
    subst foundPresent
    exact absurd typed rejected
  | rejected kept => rfl

theorem assign_uncommitted_keeps_store (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (cell : CellId) (candidate : Value Declaration Identity Source)
    (uncommitted : ∀ next, assign world decisions store cell candidate ≠ .committed next) :
    (assign world decisions store cell candidate).storeAfter = store := by
  unfold assign at uncommitted ⊢
  cases present : store cell with
  | none => rfl
  | some current =>
    simp only [present] at uncommitted ⊢
    cases mutability : current.mutability with
    | immutable => rfl
    | mutable =>
      simp only [mutability] at uncommitted ⊢
      cases written : Xsh.Core.Store.write world decisions store cell candidate with
      | accepted updated => simp [written] at uncommitted
      | rejected kept => rfl

theorem assign_failure_is_type_error (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (cell : CellId) (candidate : Value Declaration Identity Source)
    (failure : Failure Declaration Identity Source) (after : Store world)
    (failed : assign world decisions store cell candidate = .failed failure after) :
    after = store ∧ ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
      ¬ HasType world current.contents.typeTag candidate ∧
      failure = .typeError current.contents.typeTag candidate := by
  unfold assign at failed
  cases present : store cell with
  | none => simp [present] at failed
  | some current =>
    rw [present] at failed
    cases mutability : current.mutability with
    | immutable => simp [mutability] at failed
    | mutable =>
      simp only [mutability] at failed
      cases written : Xsh.Core.Store.write world decisions store cell candidate with
      | accepted updated => simp [written] at failed
      | rejected kept =>
        simp only [written, StoreStep.failed.injEq] at failed
        obtain ⟨sameFailure, sameStore⟩ := failed
        refine ⟨sameStore.symm, current, rfl, mutability, ?_, sameFailure.symm⟩
        intro typed
        obtain ⟨updated, accepted⟩ :=
          (Xsh.Semantics.Environments.write_accepted_iff (Xsh.Core.Store.decideHasType world decisions)
            store cell candidate).mpr ⟨current, present, mutability, typed⟩
        have : Xsh.Core.Store.write world decisions store cell candidate = .accepted updated := accepted
        rw [this] at written
        cases written

theorem declare_committed_iff (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (cell : CellId) (mutability : Mutability) (type : ValueType Declaration)
    (candidate : Value Declaration Identity Source) :
    (∃ next, declare world decisions store cell mutability type candidate = .committed next) ↔
      store cell = none ∧ HasType world type candidate := by
  constructor
  · rintro ⟨next, committed⟩
    unfold declare at committed
    cases present : store cell with
    | some current => simp [present] at committed
    | none =>
      rw [present] at committed
      cases allocated : Xsh.Core.Store.allocate world decisions store cell mutability type candidate with
      | none => simp [allocated] at committed
      | some updated =>
        exact ⟨rfl, ((Xsh.Semantics.Environments.allocate_accepted_iff _ store cell mutability type candidate).mp
          ⟨updated, allocated⟩).2⟩
  · rintro ⟨fresh, typed⟩
    obtain ⟨updated, allocated⟩ :=
      (Xsh.Semantics.Environments.allocate_accepted_iff (Xsh.Core.Store.decideHasType world decisions)
        store cell mutability type candidate).mpr ⟨fresh, typed⟩
    refine ⟨updated, ?_⟩
    unfold declare
    simp only [fresh]
    rw [show Xsh.Core.Store.allocate world decisions store cell mutability type candidate = some updated
      from allocated]

theorem declare_rejects_nonmember (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (cell : CellId) (mutability : Mutability) (type : ValueType Declaration)
    (candidate : Value Declaration Identity Source) (fresh : store cell = none)
    (rejected : ¬ HasType world type candidate) :
    declare world decisions store cell mutability type candidate = .failed (.typeError type candidate) store := by
  unfold declare
  simp only [fresh]
  rw [show Xsh.Core.Store.allocate world decisions store cell mutability type candidate = none from
    Xsh.Semantics.Environments.allocate_rejects_nonmember _ store cell mutability type candidate rejected]

theorem declare_committed_effect (world : World Declaration Identity Source) (decisions : Decisions world)
    (store next : Store world) (cell : CellId) (mutability : Mutability) (type : ValueType Declaration)
    (candidate : Value Declaration Identity Source)
    (committed : declare world decisions store cell mutability type candidate = .committed next) :
    (∃ current, next cell = some current ∧ current.mutability = mutability ∧
      current.contents.typeTag = type ∧ current.contents.value = candidate) ∧
      ∀ other, other ≠ cell → next other = store other := by
  unfold declare at committed
  cases present : store cell with
  | some current => simp [present] at committed
  | none =>
    rw [present] at committed
    cases allocated : Xsh.Core.Store.allocate world decisions store cell mutability type candidate with
    | none => simp [allocated] at committed
    | some updated =>
      simp only [allocated, StoreStep.committed.injEq] at committed
      subst committed
      exact ⟨allocate_accepted_at_cell _ store updated cell mutability type candidate allocated,
        fun other different => allocate_preserves_unrelated_cells _ store updated cell other mutability type
          candidate different allocated⟩

omit [DecidableEq Declaration] in
theorem recover_propagates_type_error {world : World Declaration Identity Source}
    (expected : ValueType Declaration) (candidate : Value Declaration Identity Source) (store : Store world) :
    recover (.failed (.typeError expected candidate) store) = .propagated (.typeError expected candidate) store :=
  rfl

omit [DecidableEq Declaration] in
theorem recover_catches_iff_catchable {world : World Declaration Identity Source}
    (failure : Failure Declaration Identity Source) (store : Store world) :
    (∃ error, recover (.failed failure store) = .caught error store) ↔ failure.catchable = true := by
  cases failure <;> simp [recover, Failure.catchable]

/-- Environments and typed stores over every Core type and value. Lexical
resolution returns the innermost binder, leaves other names alone and is
restored by leaving a scope; activation cells resolve the same way; captures
keep exactly their listed binders' cells; lookup follows the name, binder,
cell and stored-slot chain. Every stored slot is a member of its declared
type with valid references. Assignment commits exactly for a present mutable
cell whose type admits the candidate, replaces only that cell and keeps its
type and mutability. Declaration commits exactly for a fresh cell and a member
candidate. A failed typed store or declaration raises the uncatchable
`type-error` and returns the store unchanged; every uncommitted step returns
the store unchanged; `try` recovery propagates the type error and catches only
catchable failures. -/
theorem environments_and_typed_stores
    (world : World Declaration Identity Source) (decisions : Decisions world) :
    (∀ (environment : LexicalEnvironment) name binder,
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
      (∃ error, recover (.failed failure store) = .caught error store) ↔ failure.catchable = true) := by
  refine ⟨resolve_after_bind, fun environment name other binder different =>
    resolve_bind_other environment name other binder different, fun environment name binder other =>
    resolve_exit_scope_after_inner_bind environment name binder other, resolveCell_bindCell,
    fun environment binder other cell different => resolveCell_bindCell_other environment binder other cell different,
    fun _ _ _ => rfl, lookup_ok_iff world, fun _ _ stored _ => stored.contents.wellTyped,
    ?_, assign_committed_effect world decisions, assign_rejects_nonmember world decisions, ?_,
    assign_uncommitted_keeps_store world decisions, ?_, declare_committed_effect world decisions,
    declare_rejects_nonmember world decisions, recover_propagates_type_error, recover_catches_iff_catchable⟩
  · intro store cell candidate
    rw [assign_committed_iff]
    simp only [HasType, Xsh.Core.Store.HasType]
  · intro store after cell candidate failure failed
    obtain ⟨same, _, _, _, _, isTypeError⟩ := assign_failure_is_type_error world decisions store cell candidate
      failure after failed
    exact ⟨same, by rw [isTypeError]; rfl⟩
  · intro store cell mutability type candidate
    rw [declare_committed_iff]
    simp only [HasType, Xsh.Core.Store.HasType]


namespace Witness

abbrev WitnessValue := Value Unit Unit Unit

def witnessWorld : World Unit Unit Unit where
  membership := {
    entryFields := fun _ => []
    stream := fun _ _ => False
    callable := fun _ _ => False
    scheme := fun _ _ => False
    generic := fun _ _ => False
  }
  references := fun _ => True

def witnessDecisions : Decisions witnessWorld where
  contracts := {
    stream := fun _ _ => instDecidableFalse
    callable := fun _ _ => instDecidableFalse
    scheme := fun _ _ => instDecidableFalse
    generic := fun _ _ => instDecidableFalse
  }
  references := fun _ => instDecidableTrue

def integer (value : Int) (inRange : Xsh.Values.signed64Min ≤ value ∧ value < Xsh.Values.signed64Limit) :
    WitnessValue :=
  .scalar (.integer ⟨value, inRange⟩)

def zero : WitnessValue := integer 0 (by decide)
def five : WitnessValue := integer 5 (by decide)
def negativeOne : WitnessValue := integer (-1) (by decide)

def binder (index : Nat) (bounded : index < 4294967296) : BinderId := ⟨⟨index, bounded⟩⟩
def counterCell : CellId := ⟨0, binder 1 (by decide)⟩
def otherCell : CellId := ⟨0, binder 2 (by decide)⟩
def counterName : NameId := ⟨⟨1, by decide⟩⟩

theorem member_of_nonnegative (value : Int) (inRange : Xsh.Values.signed64Min ≤ value ∧ value < Xsh.Values.signed64Limit)
    (nonnegative : 0 ≤ value) : HasType witnessWorld (.primitive .unsignedInteger) (integer value inRange) := by
  refine ⟨?_, ?_⟩
  · intro reference present
    simp [integer, Value.references] at present
  · simpa [integer, Membership.value, Membership.primitive, Value.eraseNominal] using nonnegative

theorem nonmember_of_negative (value : Int) (inRange : Xsh.Values.signed64Min ≤ value ∧ value < Xsh.Values.signed64Limit)
    (negative : value < 0) : ¬ HasType witnessWorld (.primitive .unsignedInteger) (integer value inRange) := by
  intro typed
  have member := typed.2
  simp [integer, Membership.value, Membership.primitive, Value.eraseNominal] at member
  omega

def counter : Cell witnessWorld :=
  ⟨⟨.primitive .unsignedInteger, zero, member_of_nonnegative 0 (by decide) (by decide)⟩, .mutable⟩

def store : Store witnessWorld := fun cell => if cell = counterCell then some counter else none

def environment : Environment where
  names := bind emptyLexicalEnvironment counterName (binder 1 (by decide))
  cells := bindCell [] (binder 1 (by decide)) counterCell

theorem store_counter : store counterCell = some counter := by simp [store]

theorem typed_store_accepts_unsigned_member :
    ∃ next, assign witnessWorld witnessDecisions store counterCell five = .committed next ∧
      (next counterCell).map (fun cell => cell.contents.value) = some five ∧
      next otherCell = store otherCell := by
  obtain ⟨next, committed⟩ := (assign_committed_iff witnessWorld witnessDecisions store counterCell five).mpr
    ⟨counter, store_counter, rfl, member_of_nonnegative 5 (by decide) (by decide)⟩
  obtain ⟨current, present, ⟨replaced, replacedAt, _, replacedValue, _⟩, others⟩ :=
    assign_committed_effect witnessWorld witnessDecisions store next counterCell five committed
  refine ⟨next, committed, by simp [replacedAt, replacedValue], others otherCell (by decide)⟩

theorem typed_store_rejects_negative_unsigned :
    assign witnessWorld witnessDecisions store counterCell negativeOne =
        .failed (.typeError (.primitive .unsignedInteger) negativeOne) store ∧
      recover (assign witnessWorld witnessDecisions store counterCell negativeOne) =
        .propagated (.typeError (.primitive .unsignedInteger) negativeOne) store := by
  have failed := assign_rejects_nonmember witnessWorld witnessDecisions store counterCell negativeOne counter
    store_counter rfl (nonmember_of_negative (-1) (by decide) (by decide))
  exact ⟨failed, by rw [failed]; rfl⟩

theorem catchable_failure_is_caught :
    recover (world := witnessWorld) (.failed (.raised zero) store) = .caught zero store := rfl

theorem lookup_reads_declared_cell :
    (lookup witnessWorld environment store counterName).toOption.map (fun slot => slot.value) = some zero := by
  simp [lookup, Environment.cellOf, environment, resolve_after_bind, resolveCell_bindCell, store_counter, counter,
    Except.toOption]

theorem typed_store_keeps_float_bits :
    ∃ next, declare witnessWorld witnessDecisions store otherCell .immutable (.primitive .float)
        (.scalar (.float Xsh.Values.floatQuietNaN)) = .committed next ∧
      (next otherCell).map (fun cell => cell.contents.value) =
        some (.scalar (.float Xsh.Values.floatQuietNaN)) := by
  obtain ⟨next, committed⟩ := (declare_committed_iff witnessWorld witnessDecisions store otherCell .immutable
    (.primitive .float) (.scalar (.float Xsh.Values.floatQuietNaN))).mpr
    ⟨by simp [store, otherCell, counterCell, binder],
      fun reference present => by simp [Value.references] at present,
      by simp [Membership.value, Membership.primitive, Value.eraseNominal]⟩
  obtain ⟨⟨current, present, _, _, value⟩, _⟩ :=
    declare_committed_effect witnessWorld witnessDecisions store next otherCell .immutable _ _ committed
  exact ⟨next, committed, by simp [present, value]⟩

abbrev UnguardedStore := CellId → Option (ValueType Unit × WitnessValue)

def unguardedAssign (raw : UnguardedStore) (cell : CellId) (candidate : WitnessValue) : UnguardedStore :=
  fun other => if other = cell then (raw cell).map (fun stored => (stored.1, candidate)) else raw other

def rawStore : UnguardedStore := fun cell => if cell = counterCell then some (.primitive .unsignedInteger, zero) else none

theorem missing_store_guard_is_detected :
    (∃ type stored, unguardedAssign rawStore counterCell negativeOne counterCell = some (type, stored) ∧
      ¬ HasType witnessWorld type stored) ∧
    ∀ next, assign witnessWorld witnessDecisions store counterCell negativeOne ≠ .committed next := by
  refine ⟨⟨.primitive .unsignedInteger, negativeOne, by simp [unguardedAssign, rawStore],
    nonmember_of_negative (-1) (by decide) (by decide)⟩, ?_⟩
  intro next committed
  rw [typed_store_rejects_negative_unsigned.1] at committed
  cases committed

def mutatedEnvironment : Environment where
  names := bind emptyLexicalEnvironment counterName (binder 2 (by decide))
  cells := bindCell (bindCell [] (binder 1 (by decide)) counterCell) (binder 2 (by decide)) otherCell

theorem binding_identity_mutation_is_detected :
    resolve mutatedEnvironment.names counterName ≠ resolve environment.names counterName ∧
      (lookup witnessWorld mutatedEnvironment store counterName).toOption.map (fun slot => slot.value) = none ∧
      (lookup witnessWorld environment store counterName).toOption.map (fun slot => slot.value) = some zero := by
  refine ⟨?_, ?_, lookup_reads_declared_cell⟩
  · simp [mutatedEnvironment, environment, resolve_after_bind, binder]
  · simp [lookup, Environment.cellOf, mutatedEnvironment, resolve_after_bind, resolveCell_bindCell, store,
      otherCell, counterCell, binder, Except.toOption]

end Witness

end Xsh.Proofs.Core.Env
