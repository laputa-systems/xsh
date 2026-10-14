import Xsh.Semantics.Foundation.ObservedMachine
import Xsh.Syntax.Foundation.ResolvedIdentity

/-!
Occurrence-labeled name resolution over the observed scalar machine's existing
lexical environment, activation cells, and typed store. The occurrence ID is
a source-site label; lookup uses the name, checked declaration, and active
machine cell. This module does not add an expression evaluator or model a Rust
correspondence.
-/

namespace Xsh.Semantics.Foundation

open Xsh.Foundation
open Xsh.Values
open Xsh.Semantics.Environments
open Xsh.Semantics.BaseMachine
open Xsh.Syntax.Foundation

/-- A source-site label and the declaration selected by lexical checking.
Lookup uses the name and checked declaration, not the label.
-/
structure ResolvedNameOccurrence where
  site : NameOccurrence
  declaration : DeclarationId
  deriving DecidableEq

/-- Convert an active machine cell to the stable dynamic binding identity. -/
def BindingId.ofCell (cell : CellId) : Xsh.Syntax.Foundation.BindingId :=
  ⟨cell.activation, ⟨cell.binder⟩⟩

/-- Recover the base machine's cell key from a dynamic binding identity. -/
def BindingId.toCell (binding : Xsh.Syntax.Foundation.BindingId) : CellId :=
  ⟨binding.activation, binding.declaration.binder⟩

/-- Resolve one occurrence only when its declaration agrees with lexical lookup. -/
def resolvedBinding? (config : Configuration) (use : ResolvedNameOccurrence) :
    Option Xsh.Syntax.Foundation.BindingId :=
  match resolve config.current.names use.site.name with
  | none => none
  | some binder =>
    if binder = use.declaration.binder then
      match resolveCell config.current.activations binder with
      | none => none
      | some cell =>
        if cell.binder = binder then some (BindingId.ofCell cell) else none
    else none

/-- Read through the declaration and active typed cell selected for this occurrence. -/
def readResolved? (config : Configuration) (use : ResolvedNameOccurrence) :
    Option BaseTypedValue :=
  (resolvedBinding? config use).bind fun binding =>
    (config.current.store (BindingId.toCell binding)).map StoreCell.contents

private def decideHasBaseType (typeTag : BaseType) (candidate : CompositeValue) :
    Decidable (hasBaseType typeTag candidate) := by
  cases typeTag <;> cases candidate <;> simp only [hasBaseType] <;>
    first
    | exact isTrue trivial
    | exact isFalse id

/-- Write through the occurrence's resolved declaration and active typed cell. -/
def writeResolved? (config : Configuration) (use : ResolvedNameOccurrence)
    (candidate : CompositeValue) : Option (StoreWriteResult BaseStore) :=
  (resolvedBinding? config use).map fun binding =>
    writeCell decideHasBaseType config.current.store (BindingId.toCell binding) candidate

end Xsh.Semantics.Foundation
