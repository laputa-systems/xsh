import Xsh.Core.Store

namespace Xsh.Semantics.CoreWorld

open Xsh.Foundation
open Xsh.Core
open Xsh.Semantics.Environments

variable {Declaration Identity Source : Type}

abbrev SemanticType (Declaration : Type) := ValueType Declaration

abbrev SemanticValue (Declaration Identity Source : Type) := Value Declaration Identity Source

abbrev World (Declaration Identity Source : Type) := Xsh.Core.Store.World Declaration Identity Source

abbrev Decisions (world : World Declaration Identity Source) := Xsh.Core.Store.Decisions world

abbrev HasType (world : World Declaration Identity Source) :=
  Xsh.Core.Store.HasType world

abbrev Store (world : World Declaration Identity Source) := Xsh.Core.Store.State world

abbrev Cell (world : World Declaration Identity Source) :=
  StoreCell (ValueType Declaration) (Value Declaration Identity Source) (HasType world)

abbrev Slot (world : World Declaration Identity Source) :=
  TypedValue (ValueType Declaration) (Value Declaration Identity Source) (HasType world)

noncomputable def classicalDecisions (world : World Declaration Identity Source) : Decisions world where
  contracts := {
    stream := fun _ _ => Classical.propDecidable _
    callable := fun _ _ => Classical.propDecidable _
    scheme := fun _ _ => Classical.propDecidable _
    generic := fun _ _ => Classical.propDecidable _
  }
  references := fun _ => Classical.propDecidable _

structure Environment where
  names : LexicalEnvironment
  cells : ActivationEnvironment

inductive LookupFault where
  | unboundName (name : NameId)
  | inactiveBinder (binder : BinderId)
  | missingCell (cell : CellId)
  deriving DecidableEq

def Environment.cellOf (environment : Environment) (name : NameId) : Except LookupFault CellId :=
  match resolve environment.names name with
  | none => .error (.unboundName name)
  | some binder =>
    match resolveCell environment.cells binder with
    | none => .error (.inactiveBinder binder)
    | some cell => .ok cell

def lookup (world : World Declaration Identity Source) (environment : Environment) (store : Store world)
    (name : NameId) : Except LookupFault (Slot world) :=
  match environment.cellOf name with
  | .error fault => .error fault
  | .ok cell =>
    match store cell with
    | none => .error (.missingCell cell)
    | some stored => .ok stored.contents

inductive Failure (Declaration Identity Source : Type) where
  | typeError (expected : ValueType Declaration) (candidate : Value Declaration Identity Source)
  | raised (error : Value Declaration Identity Source)
  | runtime (error : Value Declaration Identity Source)

def Failure.catchable : Failure Declaration Identity Source → Bool
  | .typeError _ _ => false
  | .raised _ => true
  | .runtime _ => false

inductive InternalFault where
  | missingCell (cell : CellId)
  | immutableCell (cell : CellId)
  | occupiedCell (cell : CellId)
  | malformedPath (cell : CellId)
  | malformedTarget
  | unresolvedName (fault : LookupFault)
  | unconvertibleEnvironment
  deriving DecidableEq

inductive StoreStep (world : World Declaration Identity Source) where
  | committed (store : Store world)
  | failed (failure : Failure Declaration Identity Source) (store : Store world)
  | internal (fault : InternalFault) (store : Store world)

def StoreStep.storeAfter {world : World Declaration Identity Source} : StoreStep world → Store world
  | .committed store | .failed _ store | .internal _ store => store

variable [DecidableEq Declaration]

def assign (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (cell : CellId) (candidate : Value Declaration Identity Source) : StoreStep world :=
  match store cell with
  | none => .internal (.missingCell cell) store
  | some current =>
    match current.mutability with
    | .immutable => .internal (.immutableCell cell) store
    | .mutable =>
      match Xsh.Core.Store.write world decisions store cell candidate with
      | .accepted next => .committed next
      | .rejected _ => .failed (.typeError current.contents.typeTag candidate) store

def declare (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (cell : CellId) (mutability : Mutability) (type : ValueType Declaration)
    (candidate : Value Declaration Identity Source) : StoreStep world :=
  match store cell with
  | some _ => .internal (.occupiedCell cell) store
  | none =>
    match Xsh.Core.Store.allocate world decisions store cell mutability type candidate with
    | some next => .committed next
    | none => .failed (.typeError type candidate) store

inductive Recovered (world : World Declaration Identity Source) where
  | completed (store : Store world)
  | caught (error : Value Declaration Identity Source) (store : Store world)
  | propagated (failure : Failure Declaration Identity Source) (store : Store world)
  | internal (fault : InternalFault) (store : Store world)

def recover {world : World Declaration Identity Source} : StoreStep world → Recovered world
  | .committed store => .completed store
  | .failed failure store =>
    match failure with
    | .raised error => .caught error store
    | .typeError expected candidate => .propagated (.typeError expected candidate) store
    | .runtime error => .propagated (.runtime error) store
  | .internal fault store => .internal fault store

end Xsh.Semantics.CoreWorld
