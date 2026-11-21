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
  missingKey : Value Declaration Identity Source
  schema : Value Declaration Identity Source
  nulArgv : Value Declaration Identity Source

abbrev Operate (Declaration Identity Source : Type) :=
  Xsh.Syntax.AssignOp → Value Declaration Identity Source → Value Declaration Identity Source →
    Except (Value Declaration Identity Source) (Value Declaration Identity Source)

inductive PathError (Declaration Identity Source : Type) where
  | runtime (error : Value Declaration Identity Source)
  | malformed

def recordRow : ValueType Declaration → Option (RecordType Declaration)
  | .record row => some row
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

def mapEntries (keyType : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    Option (List (MapKey × Value Declaration Identity Source)) :=
  match candidate.eraseNominal, keyType with
  | .map entries, _ => some entries
  | .record fields, .primitive .string => some (fields.map fun field => (.string field.1, field.2))
  | _, _ => none

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

def keyRank : MapKey → Nat
  | .string _ => 0
  | .integer _ => 1
  | .boolean _ => 2
  | .bytes _ => 3
  | .path _ => 4
  | .duration _ => 5

def keyBefore : MapKey → MapKey → Bool
  | .string left, .string right => decide (left < right)
  | .integer left, .integer right => decide (left.val < right.val)
  | .boolean left, .boolean right => !left && right
  | .bytes left, .bytes right => decide (left < right)
  | .path left, .path right => decide (left.bytes < right.bytes)
  | .duration left, .duration right => decide (left.val < right.val)
  | left, right => decide (keyRank left < keyRank right)

def insertOrdered (key : MapKey) (inserted : Value Declaration Identity Source) :
    List (MapKey × Value Declaration Identity Source) → List (MapKey × Value Declaration Identity Source)
  | [] => [(key, inserted)]
  | (other, value) :: rest =>
    if keyBefore key other then (key, inserted) :: (other, value) :: rest
    else (other, value) :: insertOrdered key inserted rest

def negativeUnsignedKey : ValueType Declaration → MapKey → Bool
  | .primitive .unsignedInteger, .integer value => decide (value.val < 0)
  | _, _ => false

def keyFailure (raises : Raises Declaration Identity Source) :
    ValueType Declaration → List Selector → Option (Value Declaration Identity Source)
  | _, [] => none
  | type, .field name :: rest =>
    match recordRow type with
    | some row =>
      match (rowEntries row).lookup name with
      | some child => keyFailure raises child rest
      | none => none
    | none => none
  | type, .index _ :: rest =>
    match elementType type with
    | some element => keyFailure raises element rest
    | none => none
  | type, .key key :: rest =>
    match mapTypes type with
    | some (keyType, element) => if negativeUnsignedKey keyType key then some raises.schema else keyFailure raises element rest
    | none => none

def update (world : MembershipWorld Declaration Identity Source) (raises : Raises Declaration Identity Source)
    (leaf : Option (Value Declaration Identity Source) → Except (PathError Declaration Identity Source)
      (Value Declaration Identity Source)) :
    ValueType Declaration → Value Declaration Identity Source → List Selector →
      Except (PathError Declaration Identity Source) (Value Declaration Identity Source)
  | _, value, [] => leaf (some value)
  | type, value, .field name :: rest =>
    match recordRow type, Membership.recordFields world value with
    | some row, some fields =>
      match fields.lookup name, (rowEntries row).lookup name with
      | some child, some childType =>
        (update world raises leaf childType child rest).map fun replaced => .record (replaceField name replaced fields)
      | _, _ => .error .malformed
    | _, _ => .error .malformed
  | type, value, .index position :: rest =>
    match elementType type, listValues value with
    | some element, some items =>
      if inRange : 0 ≤ position ∧ position < items.length then
        (update world raises leaf element (items[position.toNat]'(by omega)) rest).map fun replaced =>
          .list (items.set position.toNat replaced)
      else .error (.runtime raises.indexOutOfRange)
    | _, _ => .error .malformed
  | type, value, .key key :: rest =>
    match mapTypes type with
    | some (keyType, element) =>
      match mapEntries keyType value with
      | some entries =>
        match entries.lookup key, rest with
        | some child, _ =>
          (update world raises leaf element child rest).map fun replaced => .map (replaceEntry key replaced entries)
        | none, [] => (leaf none).map fun inserted => .map (insertOrdered key inserted entries)
        | none, _ :: _ => .error (.runtime raises.missingKey)
      | none => .error .malformed
    | none => .error .malformed

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
    | _, none => .error (.runtime raises.missingKey)
    | compound, some value =>
      match operate compound value right with
      | .ok result => .ok result
      | .error error => .error (.runtime error)

variable [DecidableEq Declaration]

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
      match update world.membership raises (leafUpdate raises operate op right) current.contents.typeTag
          current.contents.value path with
      | .error (.runtime error) => .failed (.runtime error) store
      | .error .malformed => .internal (.malformedPath cell) store
      | .ok updated => assign world decisions store cell updated

abbrev RightSide (world : World Declaration Identity Source) :=
  Store world → Except (Failure Declaration Identity Source × Store world) (Value Declaration Identity Source × Store world)

/-- `target sel… op= right` with its selectors evaluated: a negative key into
`Map[UInt, V]` fails while the selectors evaluate, before the right side runs;
the update then commits on the store the right side left. -/
def assignStatement (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : RightSide world) : StoreStep world :=
  match store cell with
  | none => .internal (.missingCell cell) store
  | some current =>
    match keyFailure raises current.contents.typeTag path with
    | some error => .failed (.raised error) store
    | none =>
      match right store with
      | .error (failure, after) => .failed failure after
      | .ok (value, after) => assignPath world decisions raises operate after cell path op value

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
  if 0 ∈ value then .error (.runtime raises.nulArgv)
  else .ok ((name, value) :: overlay.filter fun entry => entry.1 ≠ name)

def readEnvironment (overlay : Overlay) (name : List UInt8) : Option (List UInt8) :=
  overlay.lookup name

def combine {world : World Declaration Identity Source} : StoreStep world → StoreStep world → StoreStep world
  | _, .internal fault store => .internal fault store
  | .failed failure _, cleanup => .failed failure cleanup.storeAfter
  | .internal fault store, _ => .internal fault store
  | .committed _, cleanup => cleanup

abbrev Cleanup (world : World Declaration Identity Source) := Store world → StoreStep world

def runCleanups {world : World Declaration Identity Source} : List (Cleanup world) → StoreStep world → StoreStep world
  | [], outcome => outcome
  | _ :: rest, .internal fault store => runCleanups rest (.internal fault store)
  | cleanup :: rest, outcome => runCleanups rest (combine outcome (cleanup outcome.storeAfter))

/-- Unwinding runs the cleanups registered in a block, in reverse order of
registration, on the store the body left; an earlier failure stays primary. -/
def unwind {world : World Declaration Identity Source} (registered : List (Cleanup world)) (body : StoreStep world) :
    StoreStep world :=
  runCleanups registered.reverse body

def tryBlock {world : World Declaration Identity Source} (registered : List (Cleanup world)) (body : StoreStep world) :
    Recovered world :=
  recover (unwind registered body)

end Xsh.Semantics.CoreWrites
