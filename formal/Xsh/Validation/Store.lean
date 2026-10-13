import Xsh.Validation.Boundary

/-!
Persistent writes to validation-constrained integer cells and the abstract
control result at the store boundary. A cell stores its base representation;
its declared type controls which replacements may be committed. Invalid cells
remain representable so the invariant and the effect of a missing write guard
can be stated directly.

The control result distinguishes a recoverable validation failure from an
uncatchable store type error. `require` itself remains a result; propagation
turns its rejected result into a recoverable failure. This fragment does not
model the source evaluator, unwind frames, or Rust store implementation.
-/

namespace Xsh.Validation.Store

open Xsh.Values

inductive StoredType where
  | int
  | uint
  | bounded (spec : BoundedSpec)

def StoredType.accepts : StoredType → IntValue → Prop
  | .int, _ => True
  | .uint, value => 0 ≤ value.val
  | .bounded spec, value => spec.accepts value.val

instance (typeTag : StoredType) (value : IntValue) : Decidable (typeTag.accepts value) := by
  cases typeTag with
  | int => exact isTrue trivial
  | uint => change Decidable (0 ≤ value.val); infer_instance
  | bounded spec => change Decidable (spec.accepts value.val); infer_instance

/-- A store cell retains the base value and the validation promised by its slot. -/
structure Cell where
  declared : StoredType
  contents : IntValue

def Cell.wellTyped (cell : Cell) : Prop := cell.declared.accepts cell.contents

abbrev CellId := Nat
abbrev TypedStore := CellId → Option Cell

/-- Stores are total maps so an update can preserve every other cell extensionally. -/
def put (store : TypedStore) (id : CellId) (cell : Cell) : TypedStore :=
  fun candidate => if candidate = id then some cell else store candidate

def WellTyped (store : TypedStore) : Prop :=
  ∀ id cell, store id = some cell → cell.wellTyped

theorem put_preserves_wellTyped (store : TypedStore) (id : CellId) (cell : Cell)
    (storeTyped : WellTyped store) (cellTyped : cell.wellTyped) :
    WellTyped (put store id cell) := by
  intro other otherCell present
  by_cases same : other = id
  · subst other
    simp [put] at present
    cases present
    exact cellTyped
  · have oldPresent : store other = some otherCell := by
      simpa [put, same] using present
    exact storeTyped other otherCell oldPresent

theorem put_preserves_other_cell (store : TypedStore) (id other : CellId)
    (cell : Cell) (different : other ≠ id) :
    put store id cell other = store other := by
  simp [put, different]

inductive BoundaryOutcome (Store Result : Type) where
  | returned (store : Store) (value : Result)
  | recoverableError (store : Store)
  | uncatchableTypeError (store : Store)

def BoundaryOutcome.storeAfter {Store Result : Type} :
    BoundaryOutcome Store Result → Store
  | .returned store _ => store
  | .recoverableError store => store
  | .uncatchableTypeError store => store

/-- Recovery handles propagated validation errors and leaves type errors uncaught. -/
def catchRecoverable {Store Result : Type}
    (recover : Store → BoundaryOutcome Store Result) :
    BoundaryOutcome Store Result → BoundaryOutcome Store Result
  | .returned store value => .returned store value
  | .recoverableError store => recover store
  | .uncatchableTypeError store => .uncatchableTypeError store

theorem catchRecoverable_does_not_catch_type_error
    {Store Result : Type} (recover : Store → BoundaryOutcome Store Result)
    (store : Store) :
    catchRecoverable recover (.uncatchableTypeError store) =
      .uncatchableTypeError store := rfl

inductive WriteDecision (Store : Type) where
  | committed (store : Store)
  | rejected (store : Store)

/-- A write starts from a resolved cell already present in the persistent store. -/
def checkedWrite (store : TypedStore) (id : CellId) (current : Cell)
    (_present : store id = some current) (candidate : IntValue) :
    WriteDecision TypedStore :=
  if current.declared.accepts candidate then
    .committed (put store id { current with contents := candidate })
  else
    .rejected store

/-- A failed typed write unwinds as an uncatchable type error at this boundary. -/
def writeAtBoundary (store : TypedStore) (id : CellId) (current : Cell)
    (present : store id = some current) (candidate : IntValue) :
    BoundaryOutcome TypedStore Unit :=
  match checkedWrite store id current present candidate with
  | .committed updated => .returned updated ()
  | .rejected _ => .uncatchableTypeError store

theorem checkedWrite_rejects_nonmember_atomically (store : TypedStore) (id : CellId)
    (candidate : IntValue) (current : Cell) (present : store id = some current)
    (notAccepted : ¬ current.declared.accepts candidate) :
    checkedWrite store id current present candidate = .rejected store := by
  simp [checkedWrite, notAccepted]

theorem writeAtBoundary_rejects_nonmember_uncatchably (store : TypedStore)
    (id : CellId) (candidate : IntValue) (current : Cell)
    (present : store id = some current)
    (notAccepted : ¬ current.declared.accepts candidate) :
    writeAtBoundary store id current present candidate =
      .uncatchableTypeError store := by
  rw [writeAtBoundary, checkedWrite_rejects_nonmember_atomically
    store id candidate current present notAccepted]

theorem failed_write_keeps_the_old_store (store : TypedStore) (id : CellId)
    (candidate : IntValue) (current : Cell) (present : store id = some current)
    (notAccepted : ¬ current.declared.accepts candidate) :
    (writeAtBoundary store id current present candidate).storeAfter = store := by
  rw [writeAtBoundary_rejects_nonmember_uncatchably
    store id candidate current present notAccepted]
  rfl

theorem accepted_write_preserves_store_invariant (store : TypedStore) (id : CellId)
    (candidate : IntValue) (current : Cell)
    (candidateAccepted : current.declared.accepts candidate)
    (storeTyped : WellTyped store) :
    WellTyped (put store id { current with contents := candidate }) := by
  apply put_preserves_wellTyped store id _ storeTyped
  exact candidateAccepted

theorem accepted_write_preserves_store_invariant_at_boundary
    (store : TypedStore) (id : CellId) (candidate : IntValue) (current : Cell)
    (present : store id = some current)
    (candidateAccepted : current.declared.accepts candidate)
    (storeTyped : WellTyped store) :
    WellTyped (writeAtBoundary store id current present candidate).storeAfter := by
  have accepted :
      writeAtBoundary store id current present candidate =
        .returned (put store id { current with contents := candidate }) () := by
    simp [writeAtBoundary, checkedWrite, candidateAccepted]
  rw [accepted]
  exact accepted_write_preserves_store_invariant store id candidate current
    candidateAccepted storeTyped

def propagateRequire {Store Result : Type} (store : Store) :
    RequireOutcome Result → BoundaryOutcome Store Result
  | .accepted value => .returned store value
  | .rejected => .recoverableError store

/-- This boundary models `.require(UInt)?`: rejection propagates recoverably. -/
def requireUIntAtBoundary (store : TypedStore) (candidate : IntValue) :
    BoundaryOutcome TypedStore UIntValue :=
  propagateRequire store (requireUInt candidate)

theorem negative_require_propagates_recoverably (store : TypedStore) :
    requireUIntAtBoundary store intNegativeOne = .recoverableError store := by
  simp [requireUIntAtBoundary, propagateRequire, require_uint_rejects_negative_one]

theorem negative_require_can_be_recovered (store : TypedStore) (fallback : UIntValue) :
    catchRecoverable (fun unchanged => .returned unchanged fallback)
      (requireUIntAtBoundary store intNegativeOne) = .returned store fallback := by
  rw [negative_require_propagates_recoverably]
  rfl

theorem zero_require_returns_validated_uint (store : TypedStore) :
    requireUIntAtBoundary store intZero = .returned store ⟨0, by decide⟩ := by
  simp [requireUIntAtBoundary, propagateRequire, require_uint_accepts_zero]

def witnessCellId : CellId := 7
def initialUIntCell : Cell := ⟨.uint, intZero⟩
def initialUIntStore : TypedStore := fun id =>
  if id = witnessCellId then some initialUIntCell else none
def updatedUIntCell : Cell := ⟨.uint, intOne⟩
def updatedUIntStore : TypedStore := put initialUIntStore witnessCellId updatedUIntCell

theorem initial_uint_store_is_well_typed : WellTyped initialUIntStore := by
  intro id cell present
  by_cases same : id = witnessCellId
  · subst id
    simp [initialUIntStore] at present
    cases present
    simp [Cell.wellTyped, initialUIntCell, StoredType.accepts, intZero]
  · simp [initialUIntStore, same] at present

theorem checked_uint_write_accepts_and_preserves_snapshot :
    writeAtBoundary initialUIntStore witnessCellId initialUIntCell
      (by simp [initialUIntStore]) intOne =
      .returned updatedUIntStore () ∧
    initialUIntStore witnessCellId = some initialUIntCell ∧
    updatedUIntStore witnessCellId = some updatedUIntCell ∧
    initialUIntCell.contents.val = 0 ∧ updatedUIntCell.contents.val = 1 := by
  constructor
  · simp [writeAtBoundary, checkedWrite, witnessCellId,
      initialUIntCell, updatedUIntStore, updatedUIntCell, StoredType.accepts,
      intOne]
  constructor
  · simp [initialUIntStore]
  constructor
  · simp [updatedUIntStore, put]
  constructor <;> rfl

theorem rejected_uint_write_unwinds_with_original_store :
    writeAtBoundary initialUIntStore witnessCellId initialUIntCell
      (by simp [initialUIntStore]) intNegativeOne =
      .uncatchableTypeError initialUIntStore ∧
    (writeAtBoundary initialUIntStore witnessCellId initialUIntCell
      (by simp [initialUIntStore]) intNegativeOne).storeAfter =
      initialUIntStore := by
  constructor
  · exact writeAtBoundary_rejects_nonmember_uncatchably
      initialUIntStore witnessCellId intNegativeOne initialUIntCell
      (by simp [initialUIntStore])
      (by simp [initialUIntCell, StoredType.accepts, intNegativeOne])
  · exact failed_write_keeps_the_old_store initialUIntStore witnessCellId
      intNegativeOne initialUIntCell (by simp [initialUIntStore])
      (by simp [initialUIntCell, StoredType.accepts, intNegativeOne])

def negativeUIntCell : Cell := ⟨.uint, intNegativeOne⟩
def missingGuardMutationStore : TypedStore :=
  put initialUIntStore witnessCellId negativeUIntCell

/-- A deliberately faulty update records a candidate without checking its type. -/
def writeWithoutTypeGuard (store : TypedStore) (id : CellId) (current : Cell)
    (_present : store id = some current) (candidate : IntValue) :
    BoundaryOutcome TypedStore Unit :=
  .returned (put store id { current with contents := candidate }) ()

theorem missing_guard_mutation_witness :
    writeWithoutTypeGuard initialUIntStore witnessCellId initialUIntCell
      (by simp [initialUIntStore]) intNegativeOne =
      .returned missingGuardMutationStore () ∧
    ¬ WellTyped missingGuardMutationStore := by
  constructor
  · rfl
  · intro mutatedTyped
    have stored := mutatedTyped witnessCellId negativeUIntCell (by
      simp [missingGuardMutationStore, put, negativeUIntCell])
    have impossible : ¬ negativeUIntCell.wellTyped := by
      simp [Cell.wellTyped, negativeUIntCell, StoredType.accepts, intNegativeOne]
    exact impossible stored

theorem recovery_does_not_catch_rejected_typed_write :
    catchRecoverable (fun _ => .returned initialUIntStore ())
      (writeAtBoundary initialUIntStore witnessCellId initialUIntCell
        (by simp [initialUIntStore]) intNegativeOne) =
      .uncatchableTypeError initialUIntStore := by
  rw [rejected_uint_write_unwinds_with_original_store.1]
  rfl

end Xsh.Validation.Store
