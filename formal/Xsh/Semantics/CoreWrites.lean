import Xsh.Semantics.CoreWorld
import Xsh.Syntax.Core

namespace Xsh.Semantics.CoreWrites

open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld

variable {Declaration Identity Source : Type}

inductive Selector where
  | field (name : String)
  | key (key : MapKey)
  | index (position : Int)

structure Raises (Declaration Identity Source : Type) where
  indexOutOfRange : Value Declaration Identity Source
  schema : Value Declaration Identity Source
  mapMissing : Value Declaration Identity Source
  envValue : Value Declaration Identity Source

abbrev Operate (Declaration Identity Source : Type) :=
  Xsh.Syntax.AssignOp → Value Declaration Identity Source → Value Declaration Identity Source →
    Except (Value Declaration Identity Source) (Value Declaration Identity Source)

inductive PathError (Declaration Identity Source : Type) where
  | raised (error : Value Declaration Identity Source)
  | malformed

def recordRow : ValueType Declaration → Option (RecordType Declaration)
  | .record row => some row
  | .nominal _ row => some row
  | _ => none

def elementType : ValueType Declaration → Option (ValueType Declaration)
  | .list element => some element
  | .nonEmpty element => some element
  | _ => none

def mapTypes : ValueType Declaration → Option (ValueType Declaration × ValueType Declaration)
  | .map key element => some (key, element)
  | _ => none

def listValues (candidate : Value Declaration Identity Source) : Option (List (Value Declaration Identity Source)) :=
  match candidate.eraseNominal with
  | .list values => some values
  | .environmentPaths paths => some (paths.map fun path => .scalar (.path path))
  | _ => none

def rebuild : Value Declaration Identity Source → Value Declaration Identity Source → Value Declaration Identity Source
  | .nominal declaration contents, inner => .nominal declaration (rebuild contents inner)
  | _, inner => inner

def replaceField (name : String) (replacement : Value Declaration Identity Source) :
    List (String × Value Declaration Identity Source) → List (String × Value Declaration Identity Source)
  | [] => []
  | (other, value) :: rest =>
    if other = name then (other, replacement) :: rest else (other, value) :: replaceField name replacement rest

def replaceEntry (key : MapKey) (replacement : Value Declaration Identity Source) :
    List (MapKey × Value Declaration Identity Source) → List (MapKey × Value Declaration Identity Source)
  | [] => []
  | (other, value) :: rest =>
    if other = key then (other, replacement) :: rest else (other, value) :: replaceEntry key replacement rest

def negativeUnsignedKey : ValueType Declaration → MapKey → Bool
  | .primitive .unsignedInteger, .integer value => decide (value.val < 0)
  | _, _ => false

def update (raises : Raises Declaration Identity Source)
    (leaf : Option (Value Declaration Identity Source) → Except (PathError Declaration Identity Source)
      (Value Declaration Identity Source)) :
    ValueType Declaration → Value Declaration Identity Source → List Selector →
      Except (PathError Declaration Identity Source) (Value Declaration Identity Source)
  | _, value, [] => leaf (some value)
  | type, value, .field name :: rest =>
    match recordRow type, value.eraseNominal with
    | some row, .record fields =>
      match fields.lookup name, (rowEntries row).lookup name with
      | some child, some childType =>
        (update raises leaf childType child rest).map fun replaced =>
          rebuild value (.record (replaceField name replaced fields))
      | _, _ => .error .malformed
    | _, _ => .error .malformed
  | type, value, .index position :: rest =>
    match elementType type, listValues value with
    | some element, some items =>
      if inRange : 0 ≤ position ∧ position < items.length then
        (update raises leaf element (items[position.toNat]'(by omega)) rest).map fun replaced =>
          .list (items.set position.toNat replaced)
      else .error (.raised raises.indexOutOfRange)
    | _, _ => .error .malformed
  | type, value, .key key :: rest =>
    match mapTypes type, value.eraseNominal with
    | some (keyType, element), .map entries =>
      if negativeUnsignedKey keyType key then .error (.raised raises.schema)
      else
        match entries.lookup key, rest with
        | some child, _ =>
          (update raises leaf element child rest).map fun replaced => .map (replaceEntry key replaced entries)
        | none, [] => (leaf none).map fun inserted => .map (entries ++ [(key, inserted)])
        | none, _ :: _ => .error (.raised raises.mapMissing)
    | _, _ => .error .malformed

def readPath : Value Declaration Identity Source → List Selector → Option (Value Declaration Identity Source)
  | value, [] => some value
  | value, .field name :: rest =>
    match value.eraseNominal with
    | .record fields => (fields.lookup name).bind fun child => readPath child rest
    | _ => none
  | value, .index position :: rest =>
    match listValues value with
    | some items => if position < 0 then none else items[position.toNat]?.bind fun child => readPath child rest
    | none => none
  | value, .key key :: rest =>
    match value.eraseNominal with
    | .map entries => (entries.lookup key).bind fun child => readPath child rest
    | _ => none

def leafUpdate (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (op : Xsh.Syntax.AssignOp) (right : Value Declaration Identity Source) :
    Option (Value Declaration Identity Source) → Except (PathError Declaration Identity Source)
      (Value Declaration Identity Source)
  | current =>
    match op, current with
    | .set, _ => .ok right
    | _, none => .error (.raised raises.mapMissing)
    | compound, some value =>
      match operate compound value right with
      | .ok result => .ok result
      | .error error => .error (.raised error)

variable [DecidableEq Declaration]

/-- `target = right` and `target op= right` after selectors and the right
side have been evaluated; `store` is the store at commit. -/
def assignPath (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source) : StoreStep world :=
  match store cell with
  | none => .internal (.missingCell cell) store
  | some current =>
    match current.mutability with
    | .immutable => .internal (.immutableCell cell) store
    | .mutable =>
      match update raises (leafUpdate raises operate op right) current.contents.typeTag current.contents.value path with
      | .error (.raised error) => .failed (.raised error) store
      | .error .malformed => .internal (.malformedPath cell) store
      | .ok updated => assign world decisions store cell updated

inductive Target (Declaration : Type) where
  | name (cell : CellId) (type : ValueType Declaration)
  | discard
  | record (fields : List (String × Target Declaration)) (hasRest : Bool)

abbrev Binding (Declaration Identity Source : Type) := CellId × ValueType Declaration × Value Declaration Identity Source

mutual
  def select (world : MembershipWorld Declaration Identity Source) :
      Target Declaration → Value Declaration Identity Source → Option (List (Binding Declaration Identity Source))
    | .name cell type, source => some [(cell, type, source)]
    | .discard, _ => some []
    | .record fields _, source =>
      match Membership.recordFields world source with
      | some actual => selectFields world fields actual
      | none => none

  def selectFields (world : MembershipWorld Declaration Identity Source) :
      List (String × Target Declaration) → List (String × Value Declaration Identity Source) →
        Option (List (Binding Declaration Identity Source))
    | [], _ => some []
    | (name, target) :: rest, actual =>
      match actual.lookup name with
      | none => none
      | some found =>
        match select world target found, selectFields world rest actual with
        | some first, some others => some (first ++ others)
        | _, _ => none
end

def declareAll (world : World Declaration Identity Source) (decisions : Decisions world)
    (original : Store world) (mutability : Mutability) :
    Store world → List (Binding Declaration Identity Source) → StoreStep world
  | current, [] => .committed current
  | current, (cell, type, value) :: rest =>
    match declare world decisions current cell mutability type value with
    | .committed next => declareAll world decisions original mutability next rest
    | .failed failure _ => .failed failure original
    | .internal fault _ => .internal fault original

/-- `let target = source` and `var target = source` after the source has been
evaluated: every field is selected before any cell is allocated, and the
declaration commits all cells or none. -/
def declareTarget (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (mutability : Mutability) (target : Target Declaration)
    (source : Value Declaration Identity Source) : StoreStep world :=
  match select world.membership target source with
  | none => .internal .malformedTarget store
  | some bindings => declareAll world decisions store mutability store bindings

abbrev Overlay := List (List UInt8 × List UInt8)

def setEnvironment (raises : Raises Declaration Identity Source) (overlay : Overlay) (name value : List UInt8) :
    Except (Failure Declaration Identity Source) Overlay :=
  if 0 ∈ value then .error (.raised raises.envValue)
  else .ok ((name, value) :: overlay.filter fun entry => entry.1 ≠ name)

def readEnvironment (overlay : Overlay) (name : List UInt8) : Option (List UInt8) :=
  overlay.lookup name

def combine {world : World Declaration Identity Source} : StoreStep world → StoreStep world → StoreStep world
  | _, .internal fault store => .internal fault store
  | .failed failure _, cleanup => .failed failure cleanup.storeAfter
  | .internal fault store, _ => .internal fault store
  | .committed _, cleanup => cleanup

abbrev Cleanup (world : World Declaration Identity Source) := Store world → StoreStep world

/-- Unwinding runs registered cleanups innermost first on the store the body
left; an earlier failure stays primary. -/
def unwind {world : World Declaration Identity Source} : List (Cleanup world) → StoreStep world → StoreStep world
  | [], outcome => outcome
  | _ :: rest, .internal fault store => unwind rest (.internal fault store)
  | cleanup :: rest, outcome => unwind rest (combine outcome (cleanup outcome.storeAfter))

def tryBlock {world : World Declaration Identity Source} (cleanups : List (Cleanup world)) (body : StoreStep world) :
    Recovered world :=
  recover (unwind cleanups body)

end Xsh.Semantics.CoreWrites
