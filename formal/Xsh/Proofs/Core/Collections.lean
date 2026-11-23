import Xsh.Proofs.Core.Variants

namespace Xsh.Proofs.Core.Collections

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Semantics.CoreControl
open Xsh.Proofs.Core.Control

section Root

variable {Declaration Identity Source : Type}

/-- The `collection_methods` fragment root (SPEC 4.5): a method that no
error family declares, other than `call`, `collect` and the numeric methods,
and that names a list, map or set method, evaluates its receiver, then each
argument once in written order, and gives `collectionMethod`'s value; a
list's `push` appends, `get` gives `Ok` of an element in range and the `Err`
of its index otherwise; a map's `set` is the ordered upsert and `get` gives
`Ok` of a present key's value and the `Err` of the missing key otherwise; and a
set's `add` inserts in map key order without repeating an element. -/
theorem collection_methods [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ receiver method arguments, errorConstructor machine frame receiver method = none →
      machine.context.spelling method ≠ "call" → machine.context.spelling method ≠ "collect" →
      isNumericMethod (machine.context.spelling method) = false →
      ∀ collection, CollectionMethod.ofName (machine.context.spelling method) = some collection →
      evalExpr machine (fuel + 1) frame (.call (.field receiver method) arguments) =
        collectionCall machine.context (evalExpr machine fuel) frame receiver collection arguments) ∧
    (∀ errors (receiver : Value Declaration Identity Source) items item, receiver.eraseNominal = .list items →
      collectionMethod errors .push receiver [item] = .value (.list (items ++ [item.eraseNominal]))) ∧
    (∀ errors (receiver : Value Declaration Identity Source) items (index : IntValue) item,
      receiver.eraseNominal = .list items → items[index.val.toNat]? = some item → 0 ≤ index.val →
        collectionMethod errors .get receiver [.scalar (.integer index)] = .value (.ok item)) ∧
    (∀ errors (receiver : Value Declaration Identity Source) items (index : IntValue),
      receiver.eraseNominal = .list items → items[index.val.toNat]? = none →
        collectionMethod errors .get receiver [.scalar (.integer index)] = .value (.err (errors.listGet index.val))) ∧
    (∀ errors (receiver : Value Declaration Identity Source) entries key found value,
      receiver.eraseNominal = .map entries → mapKeyOf key = some found →
        collectionMethod errors .set receiver [key, value] =
          .value (.map (upsert found value.eraseNominal entries))) ∧
    (∀ errors (receiver : Value Declaration Identity Source) entries key found,
      receiver.eraseNominal = .map entries → mapKeyOf key = some found → entries.lookup found = none →
        collectionMethod errors .get receiver [key] = .value (.err (errors.mapGet found))) ∧
    (∀ errors (receiver : Value Declaration Identity Source) keys item found,
      receiver.eraseNominal = .set keys → mapKeyOf item = some found →
        collectionMethod errors .add receiver [item] = .value (.set (insertKey found keys))) := by
  refine ⟨fun receiver method arguments none_declared notCall notCollect notNumeric collection named => ?_,
    fun errors receiver items item shape => ?_, fun errors receiver items index item shape found nonnegative => ?_,
    fun errors receiver items index shape missing => ?_, fun errors receiver entries key found value shape keyed => ?_,
    fun errors receiver entries key found shape keyed absent => ?_,
    fun errors receiver keys item found shape keyed => ?_⟩
  · simp only [evalExpr, none_declared]
    split
    · simp_all
    · simp only [named]
  · simp [collectionMethod, listMethod, shape]
  · simp [collectionMethod, listMethod, shape, found, nonnegative, Value.eraseNominal]
  · by_cases nonnegative : 0 ≤ index.val
    · simp [collectionMethod, listMethod, shape, missing, nonnegative, Value.eraseNominal]
    · simp [collectionMethod, listMethod, shape, nonnegative, Value.eraseNominal]
  · have erased : mapKeyOf key.eraseNominal = some found := by
      simpa [mapKeyOf, Value.eraseNominal_idempotent] using keyed
    simp [collectionMethod, mapMethod, shape, erased]
  · have erased : mapKeyOf key.eraseNominal = some found := by
      simpa [mapKeyOf, Value.eraseNominal_idempotent] using keyed
    simp [collectionMethod, mapMethod, shape, erased, absent]
  · have erased : mapKeyOf item.eraseNominal = some found := by
      simpa [mapKeyOf, Value.eraseNominal_idempotent] using keyed
    simp [collectionMethod, setMethod, shape, erased]

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness

def methodNames : List String :=
  ["len", "is_empty", "push", "extend", "get", "to_set", "join", "keys", "values", "set", "remove", "add", "to_list"]

def methodName (index : Nat) : Xsh.Syntax.RawName :=
  [name 320 (by decide), name 321 (by decide), name 322 (by decide), name 323 (by decide), name 324 (by decide),
    name 325 (by decide), name 326 (by decide), name 327 (by decide), name 328 (by decide), name 329 (by decide),
    name 330 (by decide), name 331 (by decide), name 332 (by decide)].getD index (name 333 (by decide))

def listName : Xsh.Syntax.RawName := name 334 (by decide)
def mapName : Xsh.Syntax.RawName := name 335 (by decide)
def setName : Xsh.Syntax.RawName := name 336 (by decide)
def wordsName : Xsh.Syntax.RawName := name 337 (by decide)

def slot (index : Nat) : Xsh.Syntax.RawName :=
  [name 340 (by decide), name 341 (by decide), name 342 (by decide), name 343 (by decide), name 344 (by decide),
    name 345 (by decide), name 346 (by decide), name 347 (by decide), name 348 (by decide), name 349 (by decide),
    name 350 (by decide), name 351 (by decide), name 352 (by decide), name 353 (by decide), name 354 (by decide),
    name 355 (by decide), name 356 (by decide)].getD index (name 357 (by decide))

def methodSpelling (named : Xsh.Syntax.RawName) : Option String :=
  ((List.range methodNames.length).find? fun index => methodName index = named).bind fun index => methodNames[index]?

def collectionMachine : Machine witnessWorld :=
  { machine with
    context := { program with spelling := fun named => (methodSpelling named).getD (program.spelling named) } }

def intOf (value : Int) : Value Unit Unit Unit :=
  .scalar (match validateInt? value with
    | some valid => .integer valid
    | none => .null)

def strOf (text : String) : Value Unit Unit Unit := .scalar (.string text)

/-- `let l = [7, 1, 2]`, `let m = {b: 2, a: 1}` as a map, `let s =
set.from([7, 1])` and `let w = ["a", "b"]`. -/
def collectionFrame : Frame witnessWorld :=
  { start with locals := [(listName, .list [intOf 7, intOf 1, intOf 2]),
      (mapName, .map [(.string "a", intOf 1), (.string "b", intOf 2)]),
      (setName, .set (setOf [.integer ⟨7, by decide⟩, .integer ⟨1, by decide⟩])),
      (wordsName, .list [strOf "a", strOf "b"])] }

def invoke (receiver : Xsh.Syntax.RawName) (method : Nat) (arguments : List Xsh.Syntax.Expr) : Xsh.Syntax.Expr :=
  .call (.field (ident receiver) (methodName method)) (arguments.map .positional)

def str (text : String) : Xsh.Syntax.Expr := .literal (.string text)

def collectionScript : List Xsh.Syntax.Statement :=
  [letOf (slot 0) (invoke listName 0 []),
   letOf (slot 1) (invoke listName 2 [int "8"]),
   letOf (slot 2) (invoke listName 3 [.list [int "7", int "8"]]),
   letOf (slot 3) (.call (.field (invoke listName 5 []) (methodName 12)) []),
   letOf (slot 4) (invoke listName 4 [int "1"]),
   letOf (slot 5) (invoke listName 4 [int "8"]),
   letOf (slot 6) (invoke mapName 7 []),
   letOf (slot 7) (invoke mapName 8 []),
   letOf (slot 8) (invoke mapName 9 [str "c", int "7"]),
   letOf (slot 9) (invoke mapName 9 [str "a", int "9223372036854775807"]),
   letOf (slot 10) (invoke mapName 10 [str "a"]),
   letOf (slot 11) (invoke mapName 4 [str "z"]),
   letOf (slot 12) (.call (.field (invoke setName 11 [int "2"]) (methodName 12)) []),
   letOf (slot 13) (.call (.field (invoke setName 10 [int "7"]) (methodName 12)) []),
   letOf (slot 14) (invoke wordsName 6 [str ","]),
   letOf (slot 15) (invoke wordsName 6 [])]

def collectionRun : Exec witnessWorld := runStatements collectionMachine 60 collectionFrame collectionScript

def slotValue (index : Nat) : Option (Value Unit Unit Unit) :=
  match collectionRun with
  | .normal frame =>
    match frame.environment.cellOf (slot index) with
    | .ok cell => (frame.store cell).map fun found => found.contents.value
    | .error _ => none
  | _ => none

def intsOf : Option (Value Unit Unit Unit) → Option (List Int)
  | some (.list items) => items.mapM fun
    | .scalar (.integer value) => some value.val
    | _ => none
  | _ => none

def intValue : Option (Value Unit Unit Unit) → Option Int
  | some (.scalar (.integer value)) => some value.val
  | some (.ok (.scalar (.integer value))) => some value.val
  | _ => none

def textValue : Option (Value Unit Unit Unit) → Option String
  | some (.scalar (.string text)) => some text
  | some (.err (.scalar (.string text))) => some text
  | _ => none

def mapView : Option (Value Unit Unit Unit) → Option (List (String × Int))
  | some (.map entries) => entries.mapM fun
    | (.string key, .scalar (.integer value)) => some (key, value.val)
    | _ => none
  | _ => none

def keyTexts : Option (Value Unit Unit Unit) → Option (List String)
  | some (.list items) => items.mapM fun
    | .scalar (.string text) => some text
    | _ => none
  | _ => none

/-- As the verification binary prints, for `[3, 1, 2]` (here `[7, 1, 2]`), `3`,
`[3,1,2,9]`, `[3,1,2,7,8]`, `[1,2,3]`, `ok 1` and `list index 5 is out of
bounds`; for `{b: 2, a: 1}`, `["a","b"]`, `[1,2]`, `{"a":1,"b":2,"c":3}`,
`{"a":9,"b":2}`, `{"b":2}` and a missing-key `Err`; for `set.from([3, 1])`,
`[1,2,3]` after `add(2)` and `[1]` after `remove(3)`; and `a,b` and `ab` for
`join`. -/
theorem collection_witness :
    intValue (slotValue 0) = some 3 ∧
    intsOf (slotValue 1) = some [7, 1, 2, 8] ∧
    intsOf (slotValue 2) = some [7, 1, 2, 7, 8] ∧
    intsOf (slotValue 3) = some [1, 2, 7] ∧
    intValue (slotValue 4) = some 1 ∧
    textValue (slotValue 5) = some "list index 8 is out of bounds" ∧
    keyTexts (slotValue 6) = some ["a", "b"] ∧
    intsOf (slotValue 7) = some [1, 2] ∧
    mapView (slotValue 8) = some [("a", 1), ("b", 2), ("c", 7)] ∧
    mapView (slotValue 9) = some [("a", 9223372036854775807), ("b", 2)] ∧
    mapView (slotValue 10) = some [("b", 2)] ∧
    textValue (slotValue 11) = some "map has no key" ∧
    intsOf (slotValue 12) = some [1, 2, 7] ∧
    intsOf (slotValue 13) = some [1] ∧
    textValue (slotValue 14) = some "a,b" ∧
    textValue (slotValue 15) = some "ab" := by
  decide +kernel

/-- A `Map.set` that appended a new key would leave `{a, b}.set("0", 7)`
with keys `a, b, 0`; the model places the key in key order, `0, a, b`. -/
theorem appending_map_set_rejected :
    (([("a", 1), ("b", 2)] : List (String × Int)) ++ [("0", 7)]).map Prod.fst = ["a", "b", "0"] ∧
      (match collectionMethod collectionMachine.context.errors .set
          (.map [(.string "a", intOf 1), (.string "b", intOf 2)]) [strOf "0", intOf 7] with
        | .value (.map entries) => some (entries.map fun entry => match entry.1 with
            | .string key => key
            | _ => "")
        | _ => none) = some ["0", "a", "b"] := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Collections
