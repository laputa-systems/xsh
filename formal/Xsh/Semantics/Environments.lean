import Xsh.Foundation

/-!
This module formalizes lexical name resolution and typed-cell updates over a
caller-supplied value domain. It does not define every XSH value or claim that
the model is a conformance proof for Rust's indexed slots or evaluator.
-/

namespace Xsh.Semantics.Environments

open Xsh.Foundation

/-- One lexical frame maps source-name identities to resolved declaration identities. -/
abbrev LexicalFrame := NameId → Option BinderId

/-- Lexical environments keep the innermost frame first. -/
abbrev LexicalEnvironment := List LexicalFrame

private def emptyLexicalFrame : LexicalFrame := fun _ => none

def emptyLexicalEnvironment : LexicalEnvironment := [emptyLexicalFrame]

private def replaceAt {Key Value : Type} [DecidableEq Key]
    (mapping : Key → Value) (key : Key) (value : Value) : Key → Value :=
  fun candidate => if candidate = key then value else mapping candidate

def enterScope (environment : LexicalEnvironment) : LexicalEnvironment :=
  emptyLexicalFrame :: environment

def exitScope (environment : LexicalEnvironment) : LexicalEnvironment :=
  environment.drop 1

/-- Bind a resolved declaration in the current frame, creating one for an empty environment. -/
def bind (environment : LexicalEnvironment) (name : NameId) (binder : BinderId) :
    LexicalEnvironment :=
  match environment with
  | [] => [replaceAt emptyLexicalFrame name (some binder)]
  | frame :: outer => replaceAt frame name (some binder) :: outer

/-- Name lookup returns the statically resolved binder identity, never a name-table ID. -/
def resolve (environment : LexicalEnvironment) (name : NameId) : Option BinderId :=
  match environment with
  | [] => none
  | frame :: outer =>
    match frame name with
    | some binder => some binder
    | none => resolve outer name

theorem resolve_after_bind (environment : LexicalEnvironment) (name : NameId)
    (binder : BinderId) :
    resolve (bind environment name binder) name = some binder := by
  cases environment with
  | nil => simp [bind, resolve, replaceAt]
  | cons frame outer => simp [bind, resolve, replaceAt]

theorem resolve_inner_shadows_outer (environment : LexicalEnvironment) (name : NameId)
    (outerBinder innerBinder : BinderId) (different : outerBinder ≠ innerBinder) :
    resolve (bind (enterScope (bind environment name outerBinder)) name innerBinder) name =
      some innerBinder ∧
        resolve (bind (enterScope (bind environment name outerBinder)) name innerBinder) name ≠
          some outerBinder := by
  have innerResolution := resolve_after_bind (enterScope (bind environment name outerBinder))
    name innerBinder
  constructor
  · exact innerResolution
  · intro outerResolution
    exact different (Option.some.inj (innerResolution.symm.trans outerResolution)).symm

theorem resolve_after_scope_exit (environment : LexicalEnvironment) (name : NameId)
    (binder : BinderId) :
    resolve (exitScope (bind (enterScope environment) name binder)) name =
      resolve environment name := by
  simp [exitScope, bind, enterScope]

theorem scope_exit_restores_shadowed_outer (environment : LexicalEnvironment) (name : NameId)
    (outerBinder innerBinder : BinderId) :
    resolve
        (exitScope (bind (enterScope (bind environment name outerBinder)) name innerBinder)) name =
      some outerBinder := by
  rw [resolve_after_scope_exit]
  exact resolve_after_bind environment name outerBinder

private def roleWitnessName : NameId := ⟨⟨7, by decide⟩⟩

private def roleWitnessBinder : BinderId := ⟨⟨7, by decide⟩⟩

theorem same_numeric_name_and_binder_keep_roles :
    NameId.value roleWitnessName = BinderId.value roleWitnessBinder ∧
      resolve (bind emptyLexicalEnvironment roleWitnessName roleWitnessBinder) roleWitnessName =
        some roleWitnessBinder := by
  constructor
  · rfl
  · exact resolve_after_bind emptyLexicalEnvironment roleWitnessName roleWitnessBinder

/-- A dynamic cell is a source binder instantiated in one activation. -/
structure CellId where
  activation : Nat
  binder : BinderId
  deriving DecidableEq

theorem cell_ids_distinguish_activations (first second : Nat) (binder : BinderId)
    (different : first ≠ second) :
    CellId.mk first binder ≠ CellId.mk second binder := by
  intro equal
  have sameActivation := congrArg CellId.activation equal
  exact different sameActivation

/-- Active lexical frames map resolved binders to dynamic store-cell identities. -/
abbrev ActivationFrame := BinderId → Option CellId

abbrev ActivationEnvironment := List ActivationFrame

abbrev CapturedEnvironment := BinderId → Option CellId

private def emptyActivationFrame : ActivationFrame := fun _ => none

def resolveCell (environment : ActivationEnvironment) (binder : BinderId) : Option CellId :=
  match environment with
  | [] => none
  | frame :: outer =>
    match frame binder with
    | some cell => some cell
    | none => resolveCell outer binder

def bindCell (environment : ActivationEnvironment) (binder : BinderId) (cell : CellId) :
    ActivationEnvironment :=
  match environment with
  | [] => [replaceAt emptyActivationFrame binder (some cell)]
  | frame :: outer => replaceAt frame binder (some cell) :: outer

/-- A capture retains cell identity, so later writes remain visible through that capture. -/
def capture (environment : ActivationEnvironment) (binders : List BinderId) : CapturedEnvironment :=
  fun binder => if binder ∈ binders then resolveCell environment binder else none

theorem capture_retains_cell (environment : ActivationEnvironment) (binders : List BinderId)
    (binder : BinderId) (included : binder ∈ binders) :
    capture environment binders binder = resolveCell environment binder := by
  simp [capture, included]

theorem capture_keeps_cell_before_rebinding (environment : ActivationEnvironment)
    (binder : BinderId) (firstCell secondCell : CellId)
    (differentCells : firstCell ≠ secondCell) :
    capture (bindCell environment binder firstCell) [binder] binder = some firstCell ∧
      resolveCell (bindCell environment binder secondCell) binder = some secondCell ∧
      firstCell ≠ secondCell := by
  constructor
  · cases environment with
    | nil => unfold capture; simp [bindCell, resolveCell, replaceAt]
    | cons frame outer => unfold capture; simp [bindCell, resolveCell, replaceAt]
  constructor
  · cases environment with
    | nil => simp [bindCell, resolveCell, replaceAt]
    | cons frame outer => simp [bindCell, resolveCell, replaceAt]
  · exact differentCells

inductive Mutability where
  | immutable
  | mutable
  deriving DecidableEq

/-- A value carries its language type and evidence that the payload has that type. -/
structure TypedValue (TypeTag Value : Type) (hasType : TypeTag → Value → Prop) where
  typeTag : TypeTag
  value : Value
  wellTyped : hasType typeTag value

/-- Store cells retain both their typed contents and whether assignment is permitted. -/
structure StoreCell (TypeTag Value : Type) (hasType : TypeTag → Value → Prop) where
  contents : TypedValue TypeTag Value hasType
  mutability : Mutability

private def StoreCell.replaceContents {TypeTag Value : Type}
    {hasType : TypeTag → Value → Prop}
    (cell : StoreCell TypeTag Value hasType)
    (replacement : Value)
    (member : hasType cell.contents.typeTag replacement) :
    StoreCell TypeTag Value hasType := {
      contents := {
        typeTag := cell.contents.typeTag
        value := replacement
        wellTyped := member
      }
      mutability := cell.mutability
    }

abbrev TypedStore (TypeTag Value : Type) (hasType : TypeTag → Value → Prop) :=
  CellId → Option (StoreCell TypeTag Value hasType)

def readCaptured {TypeTag Value : Type} {hasType : TypeTag → Value → Prop}
    (captures : CapturedEnvironment) (store : TypedStore TypeTag Value hasType)
    (binder : BinderId) : Option (StoreCell TypeTag Value hasType) :=
  (captures binder).bind store

inductive StoreWriteResult (Store : Type) where
  | rejected (store : Store)
  | accepted (store : Store)

def StoreWriteResult.storeAfter {Store : Type} : StoreWriteResult Store → Store
  | .rejected store => store
  | .accepted store => store

/-- Assignment changes only a present mutable cell when its declared type admits the candidate. -/
def writeCell {TypeTag Value : Type} {hasType : TypeTag → Value → Prop}
    (decideHasType : (typeTag : TypeTag) → (candidate : Value) →
      Decidable (hasType typeTag candidate))
    (store : TypedStore TypeTag Value hasType) (cellId : CellId)
    (candidate : Value) :
    StoreWriteResult (TypedStore TypeTag Value hasType) :=
  match store cellId with
  | none => .rejected store
  | some current =>
    match current.mutability with
    | .immutable => .rejected store
    | .mutable =>
      match decideHasType current.contents.typeTag candidate with
      | .isTrue member =>
        .accepted (replaceAt store cellId
          (some (current.replaceContents candidate member)))
      | .isFalse _ => .rejected store

theorem write_rejects_missing_cell_atomically
    {TypeTag Value : Type} {hasType : TypeTag → Value → Prop}
    (decideHasType : (typeTag : TypeTag) → (candidate : Value) →
      Decidable (hasType typeTag candidate))
    (store : TypedStore TypeTag Value hasType) (cellId : CellId)
    (candidate : Value)
    (missing : store cellId = none) :
    (writeCell decideHasType store cellId candidate).storeAfter = store := by
  unfold writeCell
  rw [missing]
  simp [StoreWriteResult.storeAfter]

theorem write_rejects_immutable_cell_atomically
    {TypeTag Value : Type} {hasType : TypeTag → Value → Prop}
    (decideHasType : (typeTag : TypeTag) → (candidate : Value) →
      Decidable (hasType typeTag candidate))
    (store : TypedStore TypeTag Value hasType) (cellId : CellId)
    (candidate : Value)
    (current : StoreCell TypeTag Value hasType)
    (present : store cellId = some current)
    (immutableCell : current.mutability = .immutable) :
    (writeCell decideHasType store cellId candidate).storeAfter = store := by
  simp only [writeCell, present]
  rw [immutableCell]
  rfl

theorem write_rejects_nonmember_atomically
    {TypeTag Value : Type} {hasType : TypeTag → Value → Prop}
    (decideHasType : (typeTag : TypeTag) → (candidate : Value) →
      Decidable (hasType typeTag candidate))
    (store : TypedStore TypeTag Value hasType) (cellId : CellId)
    (candidate : Value)
    (current : StoreCell TypeTag Value hasType)
    (present : store cellId = some current)
    (mutableCell : current.mutability = .mutable)
    (notMember : ¬ hasType current.contents.typeTag candidate) :
    (writeCell decideHasType store cellId candidate).storeAfter = store := by
  simp only [writeCell, present]
  rw [mutableCell]
  cases decision : decideHasType current.contents.typeTag candidate with
  | isTrue member => exact False.elim (notMember member)
  | isFalse _ => rfl

theorem write_preserves_unrelated_cells
    {TypeTag Value : Type} {hasType : TypeTag → Value → Prop}
    (decideHasType : (typeTag : TypeTag) → (candidate : Value) →
      Decidable (hasType typeTag candidate))
    (store : TypedStore TypeTag Value hasType) (cellId otherCell : CellId)
    (candidate : Value) (differentCells : otherCell ≠ cellId) :
    (writeCell decideHasType store cellId candidate).storeAfter otherCell = store otherCell := by
  cases present : store cellId with
  | none => simp [writeCell, present, StoreWriteResult.storeAfter]
  | some current =>
    cases current with
    | mk contents mutability =>
      cases mutability with
      | immutable => simp [writeCell, present, StoreWriteResult.storeAfter]
      | mutable =>
        cases decision : decideHasType contents.typeTag candidate with
        | isTrue member =>
          simp [writeCell, present, decision, StoreWriteResult.storeAfter, replaceAt,
            differentCells]
        | isFalse notMember =>
          simp [writeCell, present, decision, StoreWriteResult.storeAfter]

theorem write_is_visible_through_capture
    {TypeTag Value : Type} {hasType : TypeTag → Value → Prop}
    (decideHasType : (typeTag : TypeTag) → (candidate : Value) →
      Decidable (hasType typeTag candidate))
    (captures : CapturedEnvironment) (store : TypedStore TypeTag Value hasType)
    (binder : BinderId) (cellId : CellId)
    (candidate : Value)
    (current : StoreCell TypeTag Value hasType)
    (capturedCell : captures binder = some cellId)
    (present : store cellId = some current)
    (mutableCell : current.mutability = .mutable)
    (member : hasType current.contents.typeTag candidate) :
    readCaptured captures (writeCell decideHasType store cellId candidate).storeAfter binder =
      some (current.replaceContents candidate member) := by
  unfold readCaptured
  rw [capturedCell]
  simp only [writeCell, present]
  rw [mutableCell]
  cases decision : decideHasType current.contents.typeTag candidate with
  | isTrue accepted =>
    simp [StoreWriteResult.storeAfter, replaceAt]
  | isFalse rejected => exact False.elim (rejected member)

inductive ExampleType where
  | any
  | integerOrText
  deriving DecidableEq

inductive ExampleValue where
  | integer (value : Nat)
  | text (value : String)
  | flag (value : Bool)
  deriving DecidableEq

def exampleHasType : ExampleType → ExampleValue → Prop
  | .any, _ => True
  | .integerOrText, .integer _ => True
  | .integerOrText, .text _ => True
  | .integerOrText, .flag _ => False

def exampleDecideHasType (typeTag : ExampleType) (candidate : ExampleValue) :
    Decidable (exampleHasType typeTag candidate) := by
  cases typeTag with
  | any =>
    cases candidate <;> exact isTrue trivial
  | integerOrText =>
    cases candidate with
    | integer _ => exact isTrue trivial
    | text _ => exact isTrue trivial
    | flag _ => exact isFalse (by simp [exampleHasType])

private def exampleCellId : CellId := ⟨0, roleWitnessBinder⟩

private def exampleCell (typeTag : ExampleType) : StoreCell ExampleType ExampleValue exampleHasType := {
  contents := {
    typeTag := typeTag
    value := .integer 0
    wellTyped := by cases typeTag <;> trivial
  }
  mutability := .mutable
}

private def exampleStore (typeTag : ExampleType) :
    TypedStore ExampleType ExampleValue exampleHasType :=
  fun _ => some (exampleCell typeTag)

theorem write_accepts_any_integer_member :
    ∃ cell,
      (writeCell exampleDecideHasType (exampleStore .any) exampleCellId
        (.integer 1)).storeAfter exampleCellId = some cell ∧
        cell.contents.value = .integer 1 := by
  refine ⟨(exampleCell .any).replaceContents (.integer 1) trivial, ?_, rfl⟩
  simp [writeCell, exampleStore, exampleCellId, exampleCell, exampleDecideHasType,
    exampleHasType, replaceAt, StoreWriteResult.storeAfter]

theorem write_accepts_union_integer_member :
    ∃ cell,
      (writeCell exampleDecideHasType (exampleStore .integerOrText) exampleCellId
        (.integer 1)).storeAfter exampleCellId = some cell ∧
        cell.contents.value = .integer 1 := by
  refine ⟨(exampleCell .integerOrText).replaceContents (.integer 1) trivial, ?_, rfl⟩
  simp [writeCell, exampleStore, exampleCellId, exampleCell, exampleDecideHasType,
    exampleHasType, replaceAt, StoreWriteResult.storeAfter]

theorem write_rejects_union_flag_nonmember_atomically :
    (writeCell exampleDecideHasType (exampleStore .integerOrText) exampleCellId
      (.flag true)).storeAfter = exampleStore .integerOrText := by
  apply write_rejects_nonmember_atomically exampleDecideHasType
    (exampleStore .integerOrText) exampleCellId (.flag true) (exampleCell .integerOrText)
  · rfl
  · rfl
  · simp [exampleHasType, exampleCell]

end Xsh.Semantics.Environments
