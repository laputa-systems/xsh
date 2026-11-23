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
and that names a list, map, set or text method, evaluates its receiver, then
each argument once in written order, and on a list, map or set receiver gives
`collectionMethod`'s value; a
list's `push` appends, `get` gives `Ok` of an element in range and the `Err`
of its index otherwise; a map's `set` is the ordered upsert and `get` gives
`Ok` of a present key's value and the `Err` of the missing key otherwise; and a
set's `add` inserts in map key order without repeating an element. -/
theorem collection_methods [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ receiver method arguments, errorConstructor machine frame receiver method = none →
      machine.context.spelling method ≠ "call" → machine.context.spelling method ≠ "collect" →
      isNumericMethod (machine.context.spelling method) = false →
      ((CollectionMethod.ofName (machine.context.spelling method)).isSome ∨
        (TextMethod.ofName (machine.context.spelling method)).isSome ∨
        (BytesMethod.ofName (machine.context.spelling method)).isSome) →
      evalExpr machine (fuel + 1) frame (.call (.field receiver method) arguments) =
        valueMethodCall machine.context (evalExpr machine fuel) frame receiver (machine.context.spelling method)
          arguments) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) receiver name arguments,
      valueMethodCall machine.context evaluate frame receiver name arguments =
        (evaluate frame receiver).bind fun target after =>
          (threadArguments machine.context.spelling evaluate after arguments).bind fun supplied last =>
            match target.eraseNominal, TextMethod.ofName name, CollectionMethod.ofName name with
            | .scalar (.string text), some method, _ => fromEval last (textMethod machine.context.errors method text supplied)
            | .scalar (.bytes bytes), _, _ =>
              match BytesMethod.ofName name, supplied.mapM Supplied.positionalValue with
              | some method, some values =>
                fromEval last (bytesMethod machine.context.errors method bytes (values.map Value.eraseNominal))
              | _, _ => .exec .unsupported
            | _, _, some method =>
              match supplied.mapM Supplied.positionalValue with
              | some values => fromEval last (collectionMethod machine.context.errors method target values)
              | none => .exec .unsupported
            | _, _, _ => .exec .unsupported) ∧
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
  refine ⟨fun receiver method arguments none_declared notCall notCollect notNumeric named => ?_,
    fun _ _ _ _ => by unfold valueMethodCall; rfl,
    fun errors receiver items item shape => ?_, fun errors receiver items index item shape found nonnegative => ?_,
    fun errors receiver items index shape missing => ?_, fun errors receiver entries key found value shape keyed => ?_,
    fun errors receiver entries key found shape keyed absent => ?_,
    fun errors receiver keys item found shape keyed => ?_⟩
  · simp only [evalExpr, none_declared]
    split
    · simp_all
    · split
      · simp_all
      · rfl
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

/-- A byte offset `find` reports starts an occurrence of the needle. -/
theorem findBytes_found (needle : List UInt8) :
    ∀ (index : Nat) (bytes : List UInt8) (offset : Nat), findBytes needle index bytes = some offset →
      index ≤ offset ∧ needle.isPrefixOf (bytes.drop (offset - index)) = true
  | index, [], offset, found => by
    simp only [findBytes] at found
    split at found
    · rename_i empty
      cases found
      simp_all
    · cases found
  | index, first :: rest, offset, found => by
    simp only [findBytes] at found
    split at found
    · rename_i matched
      cases found
      simpa using matched
    · obtain ⟨later, prefixed⟩ := findBytes_found needle (index + 1) rest offset found
      refine ⟨by omega, ?_⟩
      have shift : offset - index = (offset - (index + 1)) + 1 := by omega
      rw [shift, List.drop_succ_cons]
      exact prefixed

/-- The `text_methods` fragment root (SPEC 4.4): on a `Str` receiver,
`byte_len` and `count_chars` count UTF-8 bytes and Unicode scalar values;
`split` at a non-empty separator gives the pieces between its occurrences,
and with `maxsplit` stops after that many, a negative limit having none;
`base64_decode` and `base32_decode` decode the text's bytes without ASCII
whitespace, base32 upper-cased, into `Ok` bytes or the `Err` the decoder the
binary uses reports, and `Bytes.base64` and `Bytes.base32` encode with
padding;
`wrap(width)` packs each line's words greedily into lines of at most `width`
characters, cutting longer words, and a width below one is the runtime
`text-wrap` failure;
`replace(from, with: to)` replaces every non-overlapping occurrence left to
right; and an offset `find` reports starts an occurrence of the needle at or
after the byte `start`. -/
theorem text_methods (errors : Errors Declaration Identity Source) :
    (∀ text, textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .byteLen text [] =
      countedValue text.toUTF8.size) ∧
    (∀ text, textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .countChars text [] =
      countedValue text.length) ∧
    (∀ text separator, separator.isEmpty = false →
      textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .split text
          [.positional (.scalar (.string separator))] =
        .value (.list ((splitRun separator.toList (text.length + 1) [] text.toList).map textValue))) ∧
    (∀ text separator count,
      textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .split text
          [.positional (.scalar (.string separator)), .named "maxsplit" (.scalar (.integer count))] =
        .value (.list ((splitLimited separator.toList text.toList count.val).map textValue))) ∧
    (∀ text, textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .base64Decode text [] =
      baseDecoded errors .base64 (text.toUTF8.toList.filter fun byte => !isAsciiWhitespaceByte byte)) ∧
    (∀ text, textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .base32Decode text [] =
      baseDecoded errors .base32 ((text.toUTF8.toList.filter fun byte => !isAsciiWhitespaceByte byte).map fun byte =>
        if 97 ≤ byte ∧ byte ≤ 122 then byte - 32 else byte)) ∧
    (∀ bytes, bytesMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .base64 bytes [] =
      .value (.scalar (.string (String.ofList
        ((Xsh.Values.BaseCodec.encode .base64 bytes).map fun byte => Char.ofNat byte.toNat))))) ∧
    (∀ bytes, bytesMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .base32 bytes [] =
      .value (.scalar (.string (String.ofList
        ((Xsh.Values.BaseCodec.encode .base32 bytes).map fun byte => Char.ofNat byte.toNat))))) ∧
    (∀ text count, 0 < count.val →
      textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .wrap text
          [.positional (.scalar (.integer count))] =
        .value (.list ((wrapText count.val.toNat text.toList).map textValue))) ∧
    (∀ text count, ¬ 0 < count.val →
      textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .wrap text
          [.positional (.scalar (.integer count))] =
        .failure (.runtime (errors.described "text-wrap" "width must be positive"))) ∧
    (∀ text pattern replacement,
      textMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .replace text
          [.positional (.scalar (.string pattern)), .named "with" (.scalar (.string replacement))] =
        .value (textValue (replaceRun pattern.toList replacement.toList (text.length + 1) text.toList))) ∧
    (∀ needle index bytes offset, findBytes needle index bytes = some offset →
      index ≤ offset ∧ needle.isPrefixOf (bytes.drop (offset - index)) = true) := by
  refine ⟨fun _ => rfl, fun _ => rfl, fun text separator nonempty => ?_, fun _ _ _ => rfl, fun _ => rfl, fun _ => rfl, fun _ => rfl, fun _ => rfl,
    fun _ _ positive => by simp [textMethod, Value.eraseNominal, positive],
    fun _ _ nonpositive => by simp [textMethod, Value.eraseNominal, nonpositive], fun _ _ _ => ?_, findBytes_found⟩
  · simp [textMethod, Value.eraseNominal, nonempty]
  · simp [textMethod, Value.eraseNominal]

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

def textOf (text : String) : Supplied Unit Unit Unit := .positional (strOf text)

def textResult : Result Unit Unit Unit (Value Unit Unit Unit) → Option String
  | .value (.scalar (.string text)) => some text
  | _ => none

def textsResult : Result Unit Unit Unit (Value Unit Unit Unit) → Option (List String)
  | .value (.list items) => items.mapM fun
    | .scalar (.string text) => some text
    | _ => none
  | _ => none

def countResult : Result Unit Unit Unit (Value Unit Unit Unit) → Option (Option Int)
  | .value (.scalar (.integer value)) => some (some value.val)
  | .value (.scalar .null) => some none
  | _ => none

def boolResult : Result Unit Unit Unit (Value Unit Unit Unit) → Option Bool
  | .value (.scalar (.boolean value)) => some value
  | _ => none

def sample : String := "héllo wörld"

/-- As the verification binary prints `13 11 true true true`,
`dlröw olléh`, `["a","b","","c"]`, `["a","b","c"]`, `7 null 4`,
`["a","b"] ["a","b"] []`, `héLLo wörLd bbb`, `-a-b-c-`, `[""]`,
`["","a",""]` and `3` for the same calls. -/
theorem text_witness :
    countResult (textMethod collectionMachine.context.errors .byteLen sample []) = some (some 13) ∧
    countResult (textMethod collectionMachine.context.errors .countChars sample []) = some (some 11) ∧
    boolResult (textMethod collectionMachine.context.errors .isEmpty "" []) = some true ∧
    boolResult (textMethod collectionMachine.context.errors .startsWith sample [textOf "hé"]) = some true ∧
    boolResult (textMethod collectionMachine.context.errors .endsWith sample [textOf "ld"]) = some true ∧
    textResult (textMethod collectionMachine.context.errors .reverse sample []) = some "dlröw olléh" ∧
    textsResult (textMethod collectionMachine.context.errors .split "a,b,,c" [textOf ","]) = some ["a", "b", "", "c"] ∧
    textsResult (textMethod collectionMachine.context.errors .split "abc" [textOf ""]) = some ["a", "b", "c"] ∧
    countResult (textMethod collectionMachine.context.errors .find sample [textOf "w"]) = some (some 7) ∧
    countResult (textMethod collectionMachine.context.errors .find sample [textOf "z"]) = some none ∧
    countResult (textMethod collectionMachine.context.errors .find sample [textOf "l", .named "start" (intOf 4)]) = some (some 4) ∧
    textsResult (textMethod collectionMachine.context.errors .lines "a\nb\n" []) = some ["a", "b"] ∧
    textsResult (textMethod collectionMachine.context.errors .lines "a\r\nb" []) = some ["a", "b"] ∧
    textsResult (textMethod collectionMachine.context.errors .lines "" []) = some [] ∧
    textResult (textMethod collectionMachine.context.errors .replace sample [textOf "l", .named "with" (strOf "L")]) = some "héLLo wörLd" ∧
    textResult (textMethod collectionMachine.context.errors .replace "aaa" [textOf "a", .named "with" (strOf "b")]) = some "bbb" ∧
    textResult (textMethod collectionMachine.context.errors .replace "abc" [textOf "", .named "with" (strOf "-")]) = some "-a-b-c-" ∧
    textsResult (textMethod collectionMachine.context.errors .split "" [textOf ","]) = some [""] ∧
    textsResult (textMethod collectionMachine.context.errors .split ",a," [textOf ","]) = some ["", "a", ""] ∧
    countResult (textMethod collectionMachine.context.errors .find "héllo" [textOf "l", .named "start" (intOf 2)]) = some (some 3) := by
  decide +kernel

/-- As the verification binary prints `a|b,c`, `a,b,c`, `a|b|c`, `a|b|c`,
`a|b|c`, `abc`, `a|b`, `|a,` and `a|b` for `"a,b,c".split(",", n)` with `n`
`1`, `0`, `-1` and `7`, `"abc".split("", 2)` and `("", 0)`, `"ab".split("", 7)`,
`",a,".split(",", 1)` and `"a,b".split(",", maxsplit: 1)`. -/
theorem split_limit_witness :
    textsResult (textMethod collectionMachine.context.errors .split "a,b,c" [textOf ",", .positional (intOf 1)]) = some ["a", "b,c"] ∧
    textsResult (textMethod collectionMachine.context.errors .split "a,b,c" [textOf ",", .positional (intOf 0)]) = some ["a,b,c"] ∧
    textsResult (textMethod collectionMachine.context.errors .split "a,b,c" [textOf ",", .positional (intOf (-1))]) = some ["a", "b", "c"] ∧
    textsResult (textMethod collectionMachine.context.errors .split "a,b,c" [textOf ",", .positional (intOf 7)]) = some ["a", "b", "c"] ∧
    textsResult (textMethod collectionMachine.context.errors .split "abc" [textOf "", .positional (intOf 2)]) = some ["a", "b", "c"] ∧
    textsResult (textMethod collectionMachine.context.errors .split "abc" [textOf "", .positional (intOf 0)]) = some ["abc"] ∧
    textsResult (textMethod collectionMachine.context.errors .split "ab" [textOf "", .positional (intOf 7)]) = some ["a", "b"] ∧
    textsResult (textMethod collectionMachine.context.errors .split ",a," [textOf ",", .positional (intOf 1)]) = some ["", "a,"] ∧
    textsResult (textMethod collectionMachine.context.errors .split "a,b" [textOf ",", .named "maxsplit" (intOf 1)]) = some ["a", "b"] := by
  decide +kernel

/-- A limit that ignored the rest of the text after the last split would lose
`c` from `"a,b,c".split(",", 1)`. -/
theorem split_limit_rest_kept :
    textsResult (textMethod collectionMachine.context.errors .split "a,b,c" [textOf ",", .positional (intOf 1)]) ≠ some ["a", "b"] := by
  decide +kernel

def withName : Xsh.Syntax.RawName := name 358 (by decide)
def byteLenName : Xsh.Syntax.RawName := name 359 (by decide)
def replaceName : Xsh.Syntax.RawName := name 360 (by decide)
def sampleName : Xsh.Syntax.RawName := name 361 (by decide)

def textMachine : Machine witnessWorld :=
  { collectionMachine with
    context := { collectionMachine.context with
      spelling := fun named =>
        if named = withName then "with" else if named = byteLenName then "byte_len"
        else if named = replaceName then "replace" else collectionMachine.context.spelling named } }

/-- `s.byte_len()` and `s.replace("l", with: "L")` for `s = "héllo wörld"`
through the method dispatch. -/
def textRun : Exec witnessWorld :=
  runStatements textMachine 40 { start with locals := [(sampleName, strOf sample)] }
    [letOf (slot 0) (.call (.field (ident sampleName) byteLenName) []),
     letOf (slot 1) (.call (.field (ident sampleName) replaceName) [.positional (str "l"), .named withName (str "L")])]

def textSlot (index : Nat) : Option (Value Unit Unit Unit) :=
  match textRun with
  | .normal frame =>
    match frame.environment.cellOf (slot index) with
    | .ok cell => (frame.store cell).map fun found => found.contents.value
    | .error _ => none
  | _ => none

theorem text_dispatch_witness :
    intValue (textSlot 0) = some 13 ∧ textValue (textSlot 1) = some "héLLo wörLd" := by
  decide +kernel

def spaced : String := "\u3000 a\u00A0b\tc \u2028"

/-- As the verification binary prints `"a b\tc"` (the inner no-break space
kept), `["a","b","c"]` twice and `3` for `trim`, `words`, `fields` and
`count_words` on `"\u{3000} a\u{a0}b\tc \u{2028}"`; `["a","b","c"]` for
`"a:b::c".fields(":")`, `[]` for `"".words()` and `"  ".fields()`, `2 2 0 1`
for `count_lines` on `"a\nb\n"`, `"a\nb"`, `""` and `"\n"`; `"x\u{200b}"` for
trimming `"\u{85}x\u{200b}"`; and `["ab"]`, `["a","b"]` and `[]` for
`"ab".fields("")`, `"a::b".fields(delimiter: "::")` and `"".fields("")`. -/
theorem whitespace_witness :
    textResult (textMethod collectionMachine.context.errors .trim spaced []) = some "a\u00A0b\tc" ∧
    textsResult (textMethod collectionMachine.context.errors .words spaced []) = some ["a", "b", "c"] ∧
    textsResult (textMethod collectionMachine.context.errors .fields spaced []) = some ["a", "b", "c"] ∧
    countResult (textMethod collectionMachine.context.errors .countWords spaced []) = some (some 3) ∧
    textsResult (textMethod collectionMachine.context.errors .fields "a:b::c" [textOf ":"]) = some ["a", "b", "c"] ∧
    textsResult (textMethod collectionMachine.context.errors .words "" []) = some [] ∧
    textsResult (textMethod collectionMachine.context.errors .fields "  " []) = some [] ∧
    countResult (textMethod collectionMachine.context.errors .countLines "a\nb\n" []) = some (some 2) ∧
    countResult (textMethod collectionMachine.context.errors .countLines "a\nb" []) = some (some 2) ∧
    countResult (textMethod collectionMachine.context.errors .countLines "" []) = some (some 0) ∧
    countResult (textMethod collectionMachine.context.errors .countLines "\n" []) = some (some 1) ∧
    textResult (textMethod collectionMachine.context.errors .trim "\u0085x\u200B" []) = some "x\u200B" ∧
    textsResult (textMethod collectionMachine.context.errors .fields "ab" [textOf ""]) = some ["ab"] ∧
    textsResult (textMethod collectionMachine.context.errors .fields "a::b" [.named "delimiter" (strOf "::")]) = some ["a", "b"] ∧
    textsResult (textMethod collectionMachine.context.errors .fields "" [textOf ""]) = some [] := by
  decide +kernel

/-- A `trim` on Lean's ASCII `Char.isWhitespace` stops at the leading
ideographic space; the model removes it, as the binary does. -/
theorem ascii_trim_rejected :
    (spaced.toList.dropWhile Char.isWhitespace).head? = some '\u3000' ∧
      textResult (textMethod collectionMachine.context.errors .trim spaced []) = some "a\u00A0b\tc" := by
  decide +kernel

def failureText : Result Unit Unit Unit (Value Unit Unit Unit) → Option String
  | .failure (.runtime (.scalar (.string text))) => some text
  | _ => none

/-- As the verification binary prints `the quick|brown fox`, `abcd|efgh|ij`,
`a b|c|`, `0` and `ab cd` for `"the quick brown fox".wrap(10)`,
`"abcdefghij".wrap(4)`, `"a b\nc\n".wrap(3)`, `"".wrap(3).len()` and
`"  ab   cd ".wrap(5)`, and fails `"x".wrap(0)` with
`text-wrap: width must be positive`, which `try` does not catch. -/
theorem wrap_witness :
    textsResult (textMethod collectionMachine.context.errors .wrap "the quick brown fox" [.positional (intOf 10)]) =
      some ["the quick", "brown fox"] ∧
    textsResult (textMethod collectionMachine.context.errors .wrap "abcdefghij" [.positional (intOf 4)]) =
      some ["abcd", "efgh", "ij"] ∧
    textsResult (textMethod collectionMachine.context.errors .wrap "a b\nc\n" [.positional (intOf 3)]) = some ["a b", "c", ""] ∧
    textsResult (textMethod collectionMachine.context.errors .wrap "" [.positional (intOf 3)]) = some [] ∧
    textsResult (textMethod collectionMachine.context.errors .wrap "  ab   cd " [.positional (intOf 5)]) = some ["ab cd"] ∧
    failureText (textMethod collectionMachine.context.errors .wrap "x" [.positional (intOf 0)]) =
      some "width must be positive" := by
  decide +kernel

def doubled : String := "héllo  wörld"

/-- As the verification binary prints `"hélo wörld"`, `"abcc  d"`,
`"héo  wrd"`, `"héLLO  wörLd"`, `"x"`, `"xyc"` and `"abx"` for `squeeze`,
`delete` and `translate`; `104 195 null null` for `byte_at(0)`, `(1)`, `(99)`
and `(-1)`; `"hé"`, `"llo  wörld"` and `" wörld"` for
`byte_slice(0, length: 3)`, `(3)` and `(7, length: 99)`; and on `"héllo"`, `""`
for `byte_slice(6)` and the `text-byte-slice` failures `slice must align to
UTF-8 boundaries` for `(2)` and `(0, length: 2)`, `offset is past end of text`
for `(9)`, `offset cannot be negative` for `(-1)` and `length cannot be
negative` for `(1, length: -1)`. -/
theorem character_witness :
    textResult (textMethod collectionMachine.context.errors .squeeze doubled []) = some "hélo wörld" ∧
    textResult (textMethod collectionMachine.context.errors .squeeze "aabbcc  d" [textOf "ab"]) = some "abcc  d" ∧
    textResult (textMethod collectionMachine.context.errors .delete doubled [textOf "lö"]) = some "héo  wrd" ∧
    textResult (textMethod collectionMachine.context.errors .translate doubled [textOf "lo", textOf "LO"]) =
      some "héLLO  wörLd" ∧
    textResult (textMethod collectionMachine.context.errors .translate "abc" [textOf "abc", textOf "x"]) = some "x" ∧
    textResult (textMethod collectionMachine.context.errors .translate "abc" [textOf "ab", textOf "xyz"]) = some "xyc" ∧
    textResult (textMethod collectionMachine.context.errors .squeeze "aabbxx" []) = some "abx" ∧
    countResult (textMethod collectionMachine.context.errors .byteAt doubled [.positional (intOf 0)]) = some (some 104) ∧
    countResult (textMethod collectionMachine.context.errors .byteAt doubled [.positional (intOf 1)]) = some (some 195) ∧
    countResult (textMethod collectionMachine.context.errors .byteAt doubled [.positional (intOf 99)]) = some none ∧
    countResult (textMethod collectionMachine.context.errors .byteAt doubled [.positional (intOf (-1))]) = some none ∧
    textResult (textMethod collectionMachine.context.errors .byteSlice doubled
      [.positional (intOf 0), .named "length" (intOf 3)]) = some "hé" ∧
    textResult (textMethod collectionMachine.context.errors .byteSlice doubled [.positional (intOf 3)]) =
      some "llo  wörld" ∧
    textResult (textMethod collectionMachine.context.errors .byteSlice doubled
      [.positional (intOf 7), .named "length" (intOf 99)]) = some " wörld" ∧
    textResult (textMethod collectionMachine.context.errors .byteSlice "héllo" [.positional (intOf 6)]) = some "" ∧
    failureText (textMethod collectionMachine.context.errors .byteSlice "héllo" [.positional (intOf 2)]) =
      some "slice must align to UTF-8 boundaries" ∧
    failureText (textMethod collectionMachine.context.errors .byteSlice "héllo"
      [.positional (intOf 0), .named "length" (intOf 2)]) = some "slice must align to UTF-8 boundaries" ∧
    failureText (textMethod collectionMachine.context.errors .byteSlice "héllo" [.positional (intOf 9)]) =
      some "offset is past end of text" ∧
    failureText (textMethod collectionMachine.context.errors .byteSlice "héllo" [.positional (intOf (-1))]) =
      some "offset cannot be negative" ∧
    failureText (textMethod collectionMachine.context.errors .byteSlice "héllo"
      [.positional (intOf 1), .named "length" (intOf (-1))]) = some "length cannot be negative" := by
  decide +kernel

/-- A `byte_slice` that rounded a misaligned end down to a character boundary
would give `"h"` for `"héllo".byte_slice(0, length: 2)`; the binary and the
model fail with `text-byte-slice`. -/
theorem rounding_byte_slice_rejected :
    String.fromUTF8? ⟨#[104]⟩ = some "h" ∧
      failureText (textMethod collectionMachine.context.errors .byteSlice "héllo"
        [.positional (intOf 0), .named "length" (intOf 2)]) = some "slice must align to UTF-8 boundaries" := by
  decide +kernel

inductive Parsed where
  | ok (value : Int)
  | err (message : String)
  deriving DecidableEq

def parsed : Result Unit Unit Unit (Value Unit Unit Unit) → Option Parsed
  | .value (.ok (.scalar (.integer value))) => some (.ok value.val)
  | .value (.err (.scalar (.string text))) => some (.err text)
  | _ => none

def parseRow (text : String) : List (Option Parsed) :=
  [.parseInt, .parseIntDecimal, .parseUint, .parseUintPositive].map fun method =>
    parsed (textMethod collectionMachine.context.errors method text [])

/-- The verification binary's table for `parse_int`, `parse_int_decimal`,
`parse_uint` and `parse_uint_positive` on each input, row by row. -/
theorem parse_witness :
    parseRow "42" = [some (.ok 42), some (.ok 42), some (.ok 42), some (.ok 42)] ∧
    parseRow "-7" = [some (.ok (-7)), some (.err "invalid integer `-7`"), some (.err "expected unsigned integer"),
      some (.err "invalid positive integer `-7`")] ∧
    parseRow "+3" = [some (.ok 3), some (.err "invalid integer `+3`"), some (.err "expected unsigned integer"),
      some (.err "invalid positive integer `+3`")] ∧
    parseRow " 5 " = [some (.ok 5), some (.err "invalid integer ` 5 `"), some (.ok 5), some (.ok 5)] ∧
    parseRow "0x1f" = [some (.ok 31), some (.err "invalid integer `0x1f`"),
      some (.err "invalid unsigned integer `0x1f`"), some (.err "invalid positive integer `0x1f`")] ∧
    parseRow "0b11" = [some (.ok 3), some (.err "invalid integer `0b11`"),
      some (.err "invalid unsigned integer `0b11`"), some (.err "invalid positive integer `0b11`")] ∧
    parseRow "0o7" = [some (.ok 7), some (.err "invalid integer `0o7`"),
      some (.err "invalid unsigned integer `0o7`"), some (.err "invalid positive integer `0o7`")] ∧
    parseRow "1_000" = [some (.ok 1000), some (.err "invalid integer `1_000`"),
      some (.err "invalid unsigned integer `1_000`"), some (.err "invalid positive integer `1_000`")] ∧
    parseRow "" = [some (.err "expected integer"), some (.err "invalid integer ``"),
      some (.err "invalid unsigned integer ``"), some (.err "invalid positive integer ``")] ∧
    parseRow "9223372036854775808" = [some (.err "invalid integer `9223372036854775808`"),
      some (.err "invalid integer `9223372036854775808`"),
      some (.err "invalid unsigned integer `9223372036854775808`"),
      some (.err "invalid positive integer `9223372036854775808`")] ∧
    parseRow "-9223372036854775808" = [some (.err "invalid integer `-9223372036854775808`"),
      some (.err "invalid integer `-9223372036854775808`"), some (.err "expected unsigned integer"),
      some (.err "invalid positive integer `-9223372036854775808`")] ∧
    parseRow "007" = [some (.ok 7), some (.err "invalid integer `007`"), some (.ok 7), some (.ok 7)] ∧
    parseRow "x" = [some (.err "invalid integer `x`"), some (.err "invalid integer `x`"),
      some (.err "invalid unsigned integer `x`"), some (.err "invalid positive integer `x`")] ∧
    parseRow "4.0" = [some (.err "invalid integer `4.0`"), some (.err "invalid integer `4.0`"),
      some (.err "invalid unsigned integer `4.0`"), some (.err "invalid positive integer `4.0`")] ∧
    parseRow "-0" = [some (.ok 0), some (.err "invalid integer `-0`"), some (.err "expected unsigned integer"),
      some (.err "invalid positive integer `-0`")] := by
  decide +kernel

/-- A `parse_int` that parsed the sign with the digits would accept
`-9223372036854775808`; XSH parses the magnitude first, so it is invalid. -/
theorem signed_magnitude_rejected :
    int64FromRadix 10 "-9223372036854775808".toList = some (-9223372036854775808) ∧
      parsed (textMethod collectionMachine.context.errors .parseInt "-9223372036854775808" []) =
        some (.err "invalid integer `-9223372036854775808`") := by
  decide +kernel

def bytesOf (text : String) : List UInt8 := text.toUTF8.toList

def sampleBytes : List UInt8 := bytesOf "  AbC\r\nxy\n"

def bytesRun (method : BytesMethod) (data : List UInt8) (arguments : List (Value Unit Unit Unit)) :
    Result Unit Unit Unit (Value Unit Unit Unit) :=
  bytesMethod collectionMachine.context.errors method data arguments

def bytesResult : Result Unit Unit Unit (Value Unit Unit Unit) → Option (List UInt8)
  | .value (.scalar (.bytes data)) => some data
  | _ => none

def bytesList : Result Unit Unit Unit (Value Unit Unit Unit) → Option (List (List UInt8))
  | .value (.list items) => items.mapM fun
    | .scalar (.bytes data) => some data
    | _ => none
  | _ => none

def errText : Result Unit Unit Unit (Value Unit Unit Unit) → Option String
  | .value (.err (.scalar (.string text))) => some text
  | _ => none

def okBytes : Result Unit Unit Unit (Value Unit Unit Unit) → Option (List UInt8)
  | .value (.ok (.scalar (.bytes data))) => some data
  | _ => none

def decoded (method : TextMethod) (text : String) : Result Unit Unit Unit (Value Unit Unit Unit) :=
  textMethod collectionMachine.context.errors method text []

/-- As the verification binary prints, re-encoding each `Ok` with `base64()`:
`aGk=` for `"aGk="`, `"aGk"` and `"aG k=\n"`, `aGloaQ==` for `"aGk=aGk="`, and
the failures `non-zero trailing bits at 2` for `"aGl="`, `invalid length at 0`
for `"a"`, `invalid symbol at 3` for `"aGk*"`, `invalid padding length at 1`
for `"a==="` and `invalid symbol at 2` for `"aG=k"`; for base32 `aGVsbG8=`
from `"nbswy3dp"` and `"NBSWY3DP"`, `YQ==` from `"ME======"`, and
`invalid length at 2`, `non-zero trailing bits at 6` and `non-zero trailing
bits at 1` for `"aGk"`, `"NBSWY3D="` and `"MF"`; and `NBUQ====`, the empty text
and `NBSWY3DP` for `b"hi".base32()`, `b"".base64()` and `b"hello".base32()`. -/
theorem base_codec_witness :
    okBytes (decoded .base64Decode "aGk=") = some [104, 105] ∧
    okBytes (decoded .base64Decode "aGk") = some [104, 105] ∧
    okBytes (decoded .base64Decode "aG k=\n") = some [104, 105] ∧
    okBytes (decoded .base64Decode "aGk=aGk=") = some [104, 105, 104, 105] ∧
    errText (decoded .base64Decode "aGl=") = some "invalid base64: non-zero trailing bits at 2" ∧
    errText (decoded .base64Decode "a") = some "invalid base64: invalid length at 0" ∧
    errText (decoded .base64Decode "aGk*") = some "invalid base64: invalid symbol at 3" ∧
    errText (decoded .base64Decode "a===") = some "invalid base64: invalid padding length at 1" ∧
    errText (decoded .base64Decode "aG=k") = some "invalid base64: invalid symbol at 2" ∧
    okBytes (decoded .base32Decode "nbswy3dp") = some [104, 101, 108, 108, 111] ∧
    okBytes (decoded .base32Decode "NBSWY3DP") = some [104, 101, 108, 108, 111] ∧
    okBytes (decoded .base32Decode "ME======") = some [97] ∧
    errText (decoded .base32Decode "aGk") = some "invalid base32: invalid length at 2" ∧
    errText (decoded .base32Decode "NBSWY3D=") = some "invalid base32: non-zero trailing bits at 6" ∧
    errText (decoded .base32Decode "MF") = some "invalid base32: non-zero trailing bits at 1" ∧
    textResult (bytesRun .base32 [104, 105] []) = some "NBUQ====" ∧
    textResult (bytesRun .base64 [] []) = some "" ∧
    textResult (bytesRun .base32 [104, 101, 108, 108, 111] []) = some "NBSWY3DP" ∧
    textResult (bytesRun .base64 [104, 105] []) = some "aGk=" := by
  decide +kernel

/-- A decoder that ignored trailing bits would accept `"aGl="`. -/
theorem trailing_bits_rejected : okBytes (decoded .base64Decode "aGl=") = none := by
  decide +kernel

/-- As the verification binary prints for `b"  AbC\r\nxy\n"`: `10 2 2 false
true true`, `65 null`, `lower`, `trim`, `slice(2, length: 3)` and `slice(9)`
as `"  abc\r\nxy\n"`, `"AbC\r\nxy"`, `"AbC"` and `"\n"`, three `chunks(4)`
ending `"y\n"`, a first line `"  AbC"`, `utf8` of itself, `byte data is not
valid UTF-8 at byte 2` for `b"ab\xffc"`, `0` and `1` lines for `b""` and
`b"a"`, and the four `bytes-slice` and `bytes-chunks` failure messages. -/
theorem bytes_witness :
    countResult (bytesRun .len sampleBytes []) = some (some 10) ∧
    countResult (bytesRun .countLines sampleBytes []) = some (some 2) ∧
    (bytesList (bytesRun .lines sampleBytes [])).map List.length = some 2 ∧
    boolResult (bytesRun .isEmpty sampleBytes []) = some false ∧
    boolResult (bytesRun .startsWith sampleBytes [.scalar (.bytes (bytesOf "  A"))]) = some true ∧
    boolResult (bytesRun .endsWith sampleBytes [.scalar (.bytes (bytesOf "\n"))]) = some true ∧
    countResult (bytesRun .byteAt sampleBytes [intOf 2]) = some (some 65) ∧
    countResult (bytesRun .byteAt sampleBytes [intOf 99]) = some none ∧
    bytesResult (bytesRun .lower sampleBytes []) = some (bytesOf "  abc\r\nxy\n") ∧
    bytesResult (bytesRun .trim sampleBytes []) = some (bytesOf "AbC\r\nxy") ∧
    bytesResult (bytesRun .slice sampleBytes [intOf 2, intOf 3]) = some (bytesOf "AbC") ∧
    bytesResult (bytesRun .slice sampleBytes [intOf 9]) = some (bytesOf "\n") ∧
    (bytesList (bytesRun .chunks sampleBytes [intOf 4])).map List.length = some 3 ∧
    (bytesList (bytesRun .chunks sampleBytes [intOf 4])).bind List.getLast? = some (bytesOf "y\n") ∧
    (bytesList (bytesRun .lines sampleBytes [])).bind List.head? = some (bytesOf "  AbC") ∧
    (match bytesRun .utf8 sampleBytes [] with
      | .value (.ok (.scalar (.string text))) => some text
      | _ => none) = some "  AbC\r\nxy\n" ∧
    (match bytesRun .utf8 [97, 98, 255, 99] [] with
      | .value (.err (.scalar (.string text))) => some text
      | _ => none) = some "byte data is not valid UTF-8 at byte 2" ∧
    countResult (bytesRun .countLines [] []) = some (some 0) ∧
    countResult (bytesRun .countLines [97] []) = some (some 1) ∧
    failureText (bytesRun .slice [97, 98] [intOf (-1)]) = some "offset cannot be negative" ∧
    failureText (bytesRun .slice [97, 98] [intOf 3]) = some "offset is past end of byte data" ∧
    failureText (bytesRun .slice [97, 98] [intOf 0, intOf (-1)]) = some "length cannot be negative" ∧
    failureText (bytesRun .chunks [97, 98] [intOf 0]) = some "chunk size must be positive" := by
  decide +kernel

/-- As the verification binary prints `["a","b"]` for `{b: 1, a: 2}.keys()`,
`ok` and `missing field \`z\`` for `get("a")` and `get("z")`, and
`{"x":[1,2]}` and `{"x":[1],"y":[3]}` for pushing onto a map of lists. -/
theorem record_and_push_witness :
    keyTexts (some (match recordMethod collectionMachine.context.errors .keys [("b", intOf 1), ("a", intOf 2)] [] with
      | .value value => value
      | _ => .scalar .null)) = some ["a", "b"] ∧
    intValue (some (match recordMethod collectionMachine.context.errors .get [("b", intOf 1), ("a", intOf 2)]
        [strOf "a"] with
      | .value value => value
      | _ => .scalar .null)) = some 2 ∧
    textValue (some (match recordMethod collectionMachine.context.errors .get [("b", intOf 1)] [strOf "z"] with
      | .value value => value
      | _ => .scalar .null)) = some "missing field `z`" ∧
    (match mapMethod collectionMachine.context.errors .push [(.string "x", .list [intOf 1])] [strOf "x", intOf 2] with
      | .value (.map [(.string "x", .list items)]) => intsOf (some (.list items))
      | _ => none) = some [1, 2] ∧
    (match mapMethod collectionMachine.context.errors .push [(.string "x", .list [intOf 1])] [strOf "y", intOf 7] with
      | .value (.map entries) => some (entries.map fun entry => match entry.1 with
          | .string key => key
          | _ => "")
      | _ => none) = some ["x", "y"] := by
  decide +kernel

/-- A `Bytes.slice` that checked the length before the end would report
`length cannot be negative` for `b"ab".slice(3, length: -1)`; the runtime
checks the offset first. -/
theorem length_first_slice_rejected :
    failureText (bytesRun .slice [97, 98] [intOf 3, intOf (-1)]) = some "offset is past end of byte data" := by
  decide +kernel

def splitName : Xsh.Syntax.RawName := name 362 (by decide)
def parseIntName : Xsh.Syntax.RawName := name 363 (by decide)

def pipeMachine : Machine witnessWorld :=
  { textMachine with
    context := { textMachine.context with
      spelling := fun named =>
        if named = splitName then "split" else if named = parseIntName then "parse_int"
        else textMachine.context.spelling named } }

def pipe (input : Xsh.Syntax.Expr) (stages : List Xsh.Syntax.PipeStage) : Xsh.Syntax.Expr :=
  .structuredPipeline input stages

def keepAbove (bound : String) : Xsh.Syntax.PipeStage :=
  .stream .whereStage [] (some (none, [.expression (.binary .gt .item (int bound))]))

/-- `"a,b" |> split(",")`, `[7, 1, 2] |> where . > 1 |> len()`,
`"7" |> parse_int()?` and `[7, 1] |> [2].extend(_)`. -/
def pipeRun : Exec witnessWorld :=
  runStatements pipeMachine 60 start
    [letOf (slot 0) (pipe (str "a,b") [.value (.call (ident splitName) [.positional (str ",")])]),
     letOf (slot 1) (pipe (.list [int "7", int "1", int "2"]) [keepAbove "1", .value (.call (ident (methodName 0)) [])]),
     letOf (slot 2) (pipe (str "7") [.value (.tryExpr (.call (ident parseIntName) []))]),
     letOf (slot 3) (pipe (.list [int "7", int "1"])
       [.value (.call (.field (.list [int "2"]) (methodName 3)) [.positional .valuePipelineHole])])]

def pipeSlot (index : Nat) : Option (Value Unit Unit Unit) :=
  match pipeRun with
  | .normal frame =>
    match frame.environment.cellOf (slot index) with
    | .ok cell => (frame.store cell).map fun found => found.contents.value
    | .error _ => none
  | _ => none

/-- As the verification binary prints `["a","b"]`, `2`, `7` and `[2,7,1]`
for a bare method stage, a value stage after a stream stage, a trailing `?`,
and an explicit `_` that places the piped value as that argument. -/
theorem value_stage_witness :
    keyTexts (pipeSlot 0) = some ["a", "b"] ∧
    intValue (pipeSlot 1) = some 2 ∧
    intValue (pipeSlot 2) = some 7 ∧
    intsOf (pipeSlot 3) = some [2, 7, 1] := by
  decide +kernel

/-- A value stage that treated the call as a stream stage would split each
character; one that ignored `_` would pass `[7, 1]` as the receiver and give
`[7, 1, 2]`. The model gives `[2, 7, 1]`. -/
theorem receiver_hole_rejected :
    ([7, 1] ++ [2] : List Int) = [7, 1, 2] ∧ intsOf (pipeSlot 3) = some [2, 7, 1] := by
  decide +kernel

/-- A `byte_len` that counted characters gives `11`; the binary and the model
give `13`. -/
theorem character_byte_len_rejected :
    sample.length = 11 ∧ countResult (textMethod collectionMachine.context.errors .byteLen sample []) = some (some 13) := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Collections
