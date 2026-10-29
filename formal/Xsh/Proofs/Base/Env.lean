import Xsh.Proofs.Foundation.ResolvedBinding

namespace Xsh.Proofs.Base.Env

open Xsh.Foundation
open Xsh.Values
open Xsh.Semantics.Environments
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.Foundation

theorem lookup_follows_name_binder_cell_chain (names : LexicalEnvironment)
    (activations : ActivationEnvironment) (store : BaseStore) (name : Xsh.Syntax.RawName)
    (value : BaseTypedValue) :
    lookupBaseValue names activations store name = .ok value ↔
      ∃ binder cell stored, resolve names name = some binder ∧
        resolveCell activations binder = some cell ∧ store cell = some stored ∧
        stored.contents = value := by
  unfold lookupBaseValue
  constructor
  · intro found
    cases boundName : resolve names name with
    | none => simp [boundName] at found
    | some binder =>
      cases activeCell : resolveCell activations binder with
      | none => simp [boundName, activeCell] at found
      | some cell =>
        cases stored : store cell with
        | none => simp [boundName, activeCell, stored] at found
        | some cellContents =>
          simp only [boundName, activeCell, stored, Except.ok.injEq] at found
          exact ⟨binder, cell, cellContents, rfl, activeCell, stored, found⟩
  · rintro ⟨binder, cell, stored, boundName, activeCell, present, contents⟩
    simp [boundName, activeCell, present, contents]

theorem lookup_failure_names_its_stage (names : LexicalEnvironment)
    (activations : ActivationEnvironment) (store : BaseStore) (name : Xsh.Syntax.RawName)
    (fault : BaseFault) (failed : lookupBaseValue names activations store name = .error fault) :
    (resolve names name = none ∧ fault = .unboundName name) ∨
    (∃ binder, resolve names name = some binder ∧ resolveCell activations binder = none ∧
      fault = .inactiveBinder binder) ∨
    (∃ binder cell, resolve names name = some binder ∧
      resolveCell activations binder = some cell ∧ store cell = none ∧
      fault = .missingCell cell) := by
  unfold lookupBaseValue at failed
  cases boundName : resolve names name with
  | none =>
    simp only [boundName, Except.error.injEq] at failed
    exact Or.inl ⟨rfl, failed.symm⟩
  | some binder =>
    cases activeCell : resolveCell activations binder with
    | none =>
      simp only [boundName, activeCell, Except.error.injEq] at failed
      exact Or.inr (Or.inl ⟨binder, rfl, activeCell, failed.symm⟩)
    | some cell =>
      cases stored : store cell with
      | none =>
        simp only [boundName, activeCell, stored, Except.error.injEq] at failed
        exact Or.inr (Or.inr ⟨binder, cell, rfl, activeCell, stored, failed.symm⟩)
      | some cellContents => simp [boundName, activeCell, stored] at failed

theorem accepted_write_is_visible_through_capture (captures : CapturedEnvironment)
    (store : BaseStore) (binder : BinderId) (cell : CellId) (candidate : CompositeValue)
    (updated : BaseStore)
    (capturedCell : captures binder = some cell)
    (accepted : writeCell decideHasBaseType store cell candidate = .accepted updated) :
    (readCaptured captures updated binder).map (fun stored => stored.contents.value) =
      some candidate := by
  obtain ⟨current, present, ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
    write_accepted_replaces_only_its_cell decideHasBaseType store cell candidate updated accepted
  simp [readCaptured, capturedCell, replacedAt, replacedValue]

/-- The environment and store of the base machine. Lexical resolution returns the innermost
binding of a name, leaves other names alone, and is restored by leaving a scope. Activation
cells are resolved the same way, are distinguished by activation, and a capture keeps exactly its
listed binders' cells. Reading a name follows the chain name, binder, cell, stored value and
reports the stage that failed. A typed store write is accepted exactly for a present mutable cell
whose type admits the candidate; the update replaces only that cell's value, keeps its type and
mutability, and is visible through a capture; every rejection returns the store unchanged. -/
theorem base_env :
    (∀ environment name binder, resolve (bind environment name binder) name = some binder) ∧
    (∀ environment name other binder, other ≠ name →
      resolve (bind environment name binder) other = resolve environment other) ∧
    (∀ environment name, resolve (enterScope environment) name = resolve environment name) ∧
    (∀ environment name binder other,
      resolve (exitScope (bind (enterScope environment) name binder)) other =
        resolve environment other) ∧
    (∀ environment binder cell,
      resolveCell (bindCell environment binder cell) binder = some cell) ∧
    (∀ environment binder other cell, other ≠ binder →
      resolveCell (bindCell environment binder cell) other = resolveCell environment other) ∧
    (∀ first second binder, first ≠ second → CellId.mk first binder ≠ CellId.mk second binder) ∧
    (∀ environment binders binder, binder ∈ binders →
      capture environment binders binder = resolveCell environment binder) ∧
    (∀ environment binders binder, binder ∉ binders →
      capture environment binders binder = none) ∧
    (∀ names activations store name value,
      lookupBaseValue names activations store name = .ok value ↔
        ∃ binder cell stored, resolve names name = some binder ∧
          resolveCell activations binder = some cell ∧ store cell = some stored ∧
          stored.contents = value) ∧
    (∀ names activations store name fault,
      lookupBaseValue names activations store name = .error fault →
        (resolve names name = none ∧ fault = .unboundName name) ∨
        (∃ binder, resolve names name = some binder ∧ resolveCell activations binder = none ∧
          fault = .inactiveBinder binder) ∨
        (∃ binder cell, resolve names name = some binder ∧
          resolveCell activations binder = some cell ∧ store cell = none ∧
          fault = .missingCell cell)) ∧
    (∀ (store : BaseStore) cell candidate,
      (∃ updated, writeCell decideHasBaseType store cell candidate = .accepted updated) ↔
        ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
          hasBaseType current.contents.typeTag candidate) ∧
    (∀ (store : BaseStore) cell candidate kept,
      writeCell decideHasBaseType store cell candidate = .rejected kept → kept = store) ∧
    (∀ (store : BaseStore) cell candidate (updated : BaseStore),
      writeCell decideHasBaseType store cell candidate = .accepted updated →
        ∃ current, store cell = some current ∧
          (∃ replaced, updated cell = some replaced ∧
            replaced.contents.typeTag = current.contents.typeTag ∧
            replaced.contents.value = candidate ∧ replaced.mutability = current.mutability) ∧
          ∀ other, other ≠ cell → updated other = store other) ∧
    (∀ captures (store : BaseStore) binder cell candidate (updated : BaseStore),
      captures binder = some cell →
        writeCell decideHasBaseType store cell candidate = .accepted updated →
          (readCaptured captures updated binder).map (fun stored => stored.contents.value) =
            some candidate) :=
  ⟨resolve_after_bind, resolve_bind_other, resolve_enter_scope,
    resolve_exit_scope_after_inner_bind, resolveCell_bindCell, resolveCell_bindCell_other,
    cell_ids_distinguish_activations, capture_retains_cell, capture_excludes_uncaptured,
    lookup_follows_name_binder_cell_chain, lookup_failure_names_its_stage,
    fun store cell candidate => write_accepted_iff decideHasBaseType store cell candidate,
    fun store cell candidate kept rejected =>
      write_rejected_keeps_store decideHasBaseType store cell candidate kept rejected,
    fun store cell candidate updated accepted =>
      write_accepted_replaces_only_its_cell decideHasBaseType store cell candidate updated
        accepted,
    fun captures store binder cell candidate updated capturedCell accepted =>
      accepted_write_is_visible_through_capture captures store binder cell candidate updated
        capturedCell accepted⟩

private def nameX : Xsh.Syntax.RawName := ⟨⟨7, by decide⟩⟩
private def nameY : Xsh.Syntax.RawName := ⟨⟨8, by decide⟩⟩
private def outerBinder : BinderId := ⟨⟨11, by decide⟩⟩
private def innerBinder : BinderId := ⟨⟨12, by decide⟩⟩
private def outerCell : CellId := ⟨1, outerBinder⟩
private def innerCell : CellId := ⟨2, innerBinder⟩

private def intTwo : IntValue := ⟨2, by unfold signed64Min signed64Limit; omega⟩

private def outerNames : LexicalEnvironment := bind emptyLexicalEnvironment nameX outerBinder

private def shadowingNames : LexicalEnvironment :=
  bind (enterScope outerNames) nameX innerBinder

private def activeCells : ActivationEnvironment :=
  bindCell (bindCell [] outerBinder outerCell) innerBinder innerCell

private def witnessStore : BaseStore := fun cell =>
  if cell = outerCell then some ⟨typedInteger intOne, .mutable⟩
  else if cell = innerCell then some ⟨typedInteger intZero, .immutable⟩
  else none

theorem base_env_witness :
    resolve shadowingNames nameX = some innerBinder ∧
    resolve (exitScope shadowingNames) nameX = some outerBinder ∧
    resolve shadowingNames nameY = none ∧
    lookupBaseValue shadowingNames activeCells witnessStore nameX = .ok (typedInteger intZero) ∧
    lookupBaseValue (exitScope shadowingNames) activeCells witnessStore nameX =
      .ok (typedInteger intOne) ∧
    lookupBaseValue shadowingNames activeCells witnessStore nameY = .error (.unboundName nameY) ∧
    (∃ updated, writeCell decideHasBaseType witnessStore outerCell (.integer intTwo) =
      .accepted updated ∧ (updated outerCell).map (fun cell => cell.contents.value) =
        some (.integer intTwo) ∧ updated innerCell = witnessStore innerCell) ∧
    writeCell decideHasBaseType witnessStore innerCell (.integer intTwo) =
      .rejected witnessStore ∧
    writeCell decideHasBaseType witnessStore outerCell (.string "not an integer") =
      .rejected witnessStore ∧
    writeCell decideHasBaseType witnessStore ⟨3, outerBinder⟩ (.integer intTwo) =
      .rejected witnessStore := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · exact resolve_after_bind _ _ _
  · exact scope_exit_restores_shadowed_outer emptyLexicalEnvironment nameX outerBinder innerBinder
  · decide
  · rfl
  · rfl
  · rfl
  · refine ⟨_, rfl, rfl, rfl⟩
  · rfl
  · rfl
  · rfl

def leakyScopeExit (environment : LexicalEnvironment) : LexicalEnvironment := environment

theorem a_scope_exit_that_keeps_inner_bindings_is_rejected :
    ¬ ∀ environment name binder other,
      resolve (leakyScopeExit (bind (enterScope environment) name binder)) other =
        resolve environment other := by
  intro restores
  have leaked := restores emptyLexicalEnvironment nameX innerBinder nameX
  have innerVisible : resolve (leakyScopeExit (bind (enterScope emptyLexicalEnvironment) nameX
      innerBinder)) nameX = some innerBinder := resolve_after_bind _ _ _
  rw [innerVisible] at leaked
  cases leaked

def lookupSkippingLexicalResolution (activations : ActivationEnvironment) (store : BaseStore)
    (name : Xsh.Syntax.RawName) : Except BaseFault BaseTypedValue :=
  match resolveCell activations ⟨name.word⟩ with
  | none => .error (.unboundName name)
  | some cell =>
    match store cell with
    | none => .error (.missingCell cell)
    | some stored => .ok stored.contents

private def numericallyEqualBinder : BinderId := ⟨nameX.word⟩

private def numericalActivations : ActivationEnvironment :=
  bindCell [] numericallyEqualBinder ⟨1, numericallyEqualBinder⟩

private def numericalStore : BaseStore := fun cell =>
  if cell = ⟨1, numericallyEqualBinder⟩ then some ⟨typedInteger intOne, .mutable⟩ else none

theorem reading_a_name_by_its_numeric_payload_is_rejected :
    ¬ ∀ (names : LexicalEnvironment) activations store name value,
      lookupSkippingLexicalResolution activations store name = .ok value ↔
        ∃ binder cell stored, resolve names name = some binder ∧
          resolveCell activations binder = some cell ∧ store cell = some stored ∧
          stored.contents = value := by
  intro faithful
  have mutantReads : lookupSkippingLexicalResolution numericalActivations numericalStore nameX =
      .ok (typedInteger intOne) := rfl
  obtain ⟨binder, _, _, unresolved, _⟩ := (faithful [] numericalActivations numericalStore nameX
    (typedInteger intOne)).1 mutantReads
  simp [resolve] at unresolved

def writeIgnoringMutability (store : BaseStore) (cell : CellId) (candidate : CompositeValue) :
    StoreWriteResult BaseStore :=
  writeCell decideHasBaseType
    (fun other => (store other).map fun stored => { stored with mutability := .mutable })
    cell candidate

theorem a_write_that_ignores_mutability_is_rejected :
    ¬ ∀ (store : BaseStore) cell candidate,
      (∃ updated, writeIgnoringMutability store cell candidate = .accepted updated) ↔
        ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
          hasBaseType current.contents.typeTag candidate := by
  intro exact
  obtain ⟨current, present, mutableCell, _⟩ :=
    (exact witnessStore innerCell (.integer intTwo)).1 ⟨_, rfl⟩
  have immutableCell : (witnessStore innerCell).map (fun stored => stored.mutability) =
      some .immutable := rfl
  rw [present] at immutableCell
  simp [mutableCell] at immutableCell

end Xsh.Proofs.Base.Env
