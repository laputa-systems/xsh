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
            | .scalar (.path path), _, _ =>
              match PathMethod.ofName name, supplied.mapM Supplied.positionalValue with
              | some method, some values => fromEval last (pathMethod machine.context.errors method path (values.map Value.eraseNominal))
              | _, _ => .exec .unsupported
            | .scalar (.digest digest), _, _ =>
              match supplied.mapM Supplied.positionalValue with
              | some values => fromEval last (digestMethod name digest (values.map Value.eraseNominal))
              | none => .exec .unsupported
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
padding; `Bytes.strings` gives the printable ASCII runs at least four bytes
long, `compare` the first difference's position, line and bytes, and `dump`
the canonical hex dump; and `Bytes.trim` of data that is not all ASCII
removes whole UTF-8 encoded Unicode `White_Space` characters;
the `Path` methods read its native bytes: `bytes`, the POSIX `dirname` and
`basename`, `ext_or`'s fallback when the final name has no extension, and
the lexical `normalize`; and the `std::path` methods read its components:
`starts_with` component by component, `strip_prefix` refusing an empty
prefix, and `relative_to` giving the path itself when it does not start with
the base; and `Bytes.sha256`, like `md5`, `sha1` and `sha512`, gives the
standard digest tagged with its algorithm;
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
    (∀ bytes, bytesMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .strings bytes [] =
      .value (.list ((printableRuns 4 bytes).map fun run =>
        .scalar (.string (String.ofList (run.map fun byte => Char.ofNat byte.toNat)))))) ∧
    (∀ bytes right fields, compareFields bytes right = some fields →
      bytesMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .compare bytes
          [.scalar (.bytes right)] = .value (.record fields)) ∧
    (∀ bytes, bytesMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .dump bytes [] =
      .value (.scalar (.string (String.ofList (canonicalDump bytes))))) ∧
    (∀ bytes, bytes.all (· < 128) = false →
      bytesMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .trim bytes [] =
        .value (.scalar (.bytes (dropWhitespaceEncodings whitespaceEncodingsReversed (bytes.length + 1)
          (dropWhitespaceEncodings whitespaceEncodings (bytes.length + 1) bytes).reverse).reverse))) ∧
    (∀ path, pathMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .bytes path [] =
      .value (.scalar (.bytes path.bytes))) ∧
    (∀ path made, Xsh.Values.NativePath.fromBytes? (Xsh.Paths.Fields.posixDirname 47 path.bytes) = some made →
      pathMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .dirname path [] =
        .value (.scalar (.path made))) ∧
    (∀ path made, Xsh.Values.NativePath.fromBytes? (Xsh.Paths.Normalize.normalizePath path.bytes) = some made →
      pathMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .normalize path [] =
        .value (.scalar (.path made))) ∧
    (∀ path, pathMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .basename path [] =
        .value (.scalar (.string (Xsh.Values.LossyUtf8.text (Xsh.Paths.Fields.posixBasename 47 path.bytes))))) ∧
    (∀ path fallback, Xsh.Paths.Fields.extension? 47 path.bytes = none →
      pathMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .extOr path
          [.scalar (.string fallback)] = .value (.scalar (.string fallback))) ∧
    (∀ path other, pathMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .startsWith path
        [.scalar (.path other)] = .value (.scalar (.boolean (Xsh.Paths.StdComponents.startsWith path.bytes other.bytes)))) ∧
    (∀ path other, other.bytes = [] →
      pathMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .stripPrefix path
          [.scalar (.path other)] = .value (.err (errors.described "path-prefix" "prefix is empty"))) ∧
    (∀ path other, Xsh.Paths.StdComponents.stripPrefix? path.bytes other.bytes = none →
      pathMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .relativeTo path
          [.scalar (.path other)] = .value (.scalar (.path path))) ∧
    (∀ bytes, bytesMethod (Declaration := Declaration) (Identity := Identity) (Source := Source) errors .sha256 bytes [] =
      .value (.scalar (.digest ⟨"sha256", Xsh.Values.Digests.sha256 bytes⟩))) ∧
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
  refine ⟨fun _ => rfl, fun _ => rfl, fun text separator nonempty => ?_, fun _ _ _ => rfl, fun _ => rfl, fun _ => rfl, fun _ => rfl, fun _ => rfl, fun _ => rfl,
    fun _ _ _ found => by simp [bytesMethod, found], fun _ => rfl,
    fun _ unicode => by simp [bytesMethod, unicode],
    fun _ => rfl, fun _ _ made => by simp [pathMethod, made], fun _ _ made => by simp [pathMethod, made],
    fun _ => rfl, fun _ _ absent => by simp [pathMethod, absent],
    fun _ _ => rfl, fun _ _ empty => by simp [pathMethod, empty], fun _ _ absent => by simp [pathMethod, absent],
    fun _ => rfl,
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

def textsOf : Result Unit Unit Unit (Value Unit Unit Unit) → Option (List String)
  | .value (.list items) => items.mapM fun
    | .scalar (.string text) => some text
    | _ => none
  | _ => none

/-- A `compare` record's `equal` flag and its `byte`, `line`, `left` and
`right` fields, in that order. -/
def compared : Result Unit Unit Unit (Value Unit Unit Unit) → Option (Bool × List Int)
  | .value (.record [("equal", .scalar (.boolean equal)), ("byte", byte), ("line", line), ("left", left),
      ("right", right)]) =>
    [byte, line, left, right].mapM (Xsh.Proofs.Core.Constructors.Witness.intOf) |>.map (equal, ·)
  | _ => none

def helloWorld : List UInt8 := "hello world, this is!".toUTF8.toList

/-- As the verification binary prints `cdef|xyz  w`, `ab|cdef` and `0` for
`b"ab\x00cdef\x01xyz  w".strings()`, `b"ab\x00cdef".strings(2)` and
`b"".strings().len()`; `false 5 2 100 101`, `false 3 1 -1 99` and
`true 0 0 -1 -1` for `b"ab\ncd".compare(b"ab\nce")`, `b"ab".compare(b"abc")`
and `b"ab".compare(b"ab")`; `0000000 68 69 0a`, `0000000 150 151 012` and
`0000000` for the `hex-u8` and `octal-u8` dumps of `b"hi\n"` and of `b""`; the
canonical dump of `b"hello world, this is!"` and `00000000` for `b""`; and
the `bytes-strings` and `bytes-dump` failures for `strings(0)` and
`dump("x")`. -/
theorem bytes_report_witness :
    textsOf (bytesRun .strings [97, 98, 0, 99, 100, 101, 102, 1, 120, 121, 122, 32, 32, 119] []) =
      some ["cdef", "xyz  w"] ∧
    textsOf (bytesRun .strings [97, 98, 0, 99, 100, 101, 102] [intOf 2]) = some ["ab", "cdef"] ∧
    textsOf (bytesRun .strings [] []) = some [] ∧
    compared (bytesRun .compare [97, 98, 10, 99, 100] [.scalar (.bytes [97, 98, 10, 99, 101])]) =
      some (false, [5, 2, 100, 101]) ∧
    compared (bytesRun .compare [97, 98] [.scalar (.bytes [97, 98, 99])]) = some (false, [3, 1, -1, 99]) ∧
    compared (bytesRun .compare [97, 98] [.scalar (.bytes [97, 98])]) = some (true, [0, 0, -1, -1]) ∧
    textResult (bytesRun .dump [104, 105, 10] [strOf "hex-u8"]) = some "0000000 68 69 0a" ∧
    textResult (bytesRun .dump [104, 105, 10] [strOf "octal-u8"]) = some "0000000 150 151 012" ∧
    textResult (bytesRun .dump [] [strOf "hex-u8"]) = some "0000000" ∧
    textResult (bytesRun .dump helloWorld []) = some
      ("00000000  68 65 6c 6c 6f 20 77 6f  72 6c 64 2c 20 74 68 69  |hello world, thi|\n" ++
       "00000010  73 20 69 73 21                                    |s is!|\n00000015") ∧
    textResult (bytesRun .dump [] []) = some "00000000" ∧
    failureText (bytesRun .strings [97] [intOf 0]) = some "min_len must be positive" ∧
    failureText (bytesRun .dump [97] [strOf "x"]) = some "dump format must be canonical, hex-u8, or octal-u8" := by
  decide +kernel

/-- As the verification binary prints `5`, `4`, `2`, `6`, `1` and `1` for the
trimmed lengths of `b"\x0b a \x0b"`, `b"\x0b a \xc3\xa9"`,
`b"\xc2\xa0\xe2\x80\x80 \xc3\xa9 \xe3\x80\x80"`, `b"\xff \xc3\xa9 \xff"`,
`b" \xff\xc2\xa0"` and `b"\xc2\x85\x0bx\xe2\x80\xa8"`: ASCII data keeps a
vertical tab, other data loses whole Unicode whitespace characters, and an
invalid byte stops the trim. -/
theorem unicode_bytes_trim_witness :
    bytesResult (bytesRun .trim [11, 32, 97, 32, 11] []) = some [11, 32, 97, 32, 11] ∧
    bytesResult (bytesRun .trim [11, 32, 97, 32, 0xC3, 0xA9] []) = some [97, 32, 0xC3, 0xA9] ∧
    bytesResult (bytesRun .trim [0xC2, 0xA0, 0xE2, 0x80, 0x80, 32, 0xC3, 0xA9, 32, 0xE3, 0x80, 0x80] []) =
      some [0xC3, 0xA9] ∧
    bytesResult (bytesRun .trim [0xFF, 32, 0xC3, 0xA9, 32, 0xFF] []) = some [0xFF, 32, 0xC3, 0xA9, 32, 0xFF] ∧
    bytesResult (bytesRun .trim [32, 0xFF, 0xC2, 0xA0] []) = some [0xFF] ∧
    bytesResult (bytesRun .trim [0xC2, 0x85, 11, 120, 0xE2, 0x80, 0xA8] []) = some [120] := by
  decide +kernel

def pathRun (method : PathMethod) (text : String) (arguments : List (Value Unit Unit Unit)) :
    Result Unit Unit Unit (Value Unit Unit Unit) :=
  match Xsh.Values.NativePath.fromBytes? text.toUTF8.toList with
  | some path => pathMethod collectionMachine.context.errors method path arguments
  | none => .unsupported

def pathResult : Result Unit Unit Unit (Value Unit Unit Unit) → Option String
  | .value (.scalar (.path path)) => String.fromUTF8? ⟨path.bytes.toArray⟩
  | _ => none

/-- A path's `basename`, `dirname`, `ext_or("-")` and `normalize`. -/
def pathFields (text : String) : Option (String × String × String × String) := do
  let basename ← textResult (pathRun .basename text [])
  let dirname ← pathResult (pathRun .dirname text [])
  let extension ← textResult (pathRun .extOr text [strOf "-"])
  let normalized ← pathResult (pathRun .normalize text [])
  pure (basename, dirname, extension, normalized)

/-- As the verification binary prints, for each path below, its `basename()`,
`dirname().display()`, `ext_or("-")` and `normalize().display()`. -/
theorem path_fields_witness :
    ["", "/", "///", "a", "a/", "/a/b//", "a//b", "//a", ".", "..", "a/b/..", "/../a/./b/", "../../a/..", ".bashrc", "a.tar.gz", "x/.e", "d/f."].mapM pathFields = some [
      ("", ".", "-", "."),
      ("/", "/", "-", "/"),
      ("/", "/", "-", "/"),
      ("a", ".", "-", "a"),
      ("a", ".", "-", "a"),
      ("b", "/a", "-", "/a/b"),
      ("b", "a", "-", "a/b"),
      ("a", "/", "-", "/a"),
      (".", ".", "-", "."),
      ("..", ".", "", ".."),
      ("..", "a/b", "", "a"),
      ("b", "/../a/.", "-", "/a/b"),
      ("..", "../../a", "", "../.."),
      (".bashrc", ".", "-", ".bashrc"),
      ("a.tar.gz", ".", "gz", "a.tar.gz"),
      (".e", "x", "-", "x/.e"),
      ("f.", "d", "", "d/f.")] := by
  decide +kernel

/-- `normalize` as the validation model's lexical `normalize`, which drops the
root and lets `..` empty a relative path, would give `a/b` for `/a/b//` and
`.` for `../../a/..`. -/
theorem validation_normalize_rejected :
    pathResult (pathRun .normalize "/a/b//" []) ≠ some "a/b" ∧
    pathResult (pathRun .normalize "../../a/.." []) ≠ some "." := by
  decide +kernel

def pathOf (text : String) : Value Unit Unit Unit :=
  match Xsh.Values.NativePath.fromBytes? text.toUTF8.toList with
  | some path => .scalar (.path path)
  | none => .scalar .null

def pathsResult : Result Unit Unit Unit (Value Unit Unit Unit) → Option (List String)
  | .value (.list items) => items.mapM fun
    | .scalar (.path path) => String.fromUTF8? ⟨path.bytes.toArray⟩
    | _ => none
  | _ => none

/-- A path's `components()`, `name()`, `ext()`, `parent()` and
`with_ext("md")`. -/
def componentFields (text : String) : Option (List String × String × String × String × String) := do
  let parts ← pathsResult (pathRun .components text [])
  let name ← textResult (pathRun .name text [])
  let extension ← textResult (pathRun .ext text [])
  let parent ← pathResult (pathRun .parent text [])
  let renamed ← pathResult (pathRun .withExt text [strOf "md"])
  pure (parts, name, extension, parent, renamed)

/-- As the verification binary prints, for each path below, its components'
displays joined by `,`, `name()`, `ext()`, `parent().display()` and
`with_ext("md").display()`. -/
theorem path_components_witness :
    ["", "/", "a", "a/", "/a/b//", "a//c/b", "./a", "a/./b", "a/.", ".", "..", "a/..", "/a", ".bashrc", "a.tar.gz", "d/f.", "x/...", "//a", "/.", "a/b/", "./", ".//a", "a/./", "//", "/a/./"].mapM componentFields = some [
      ([], "", "", ".", ""),
      (["/"], "", "", "/", "/"),
      (["a"], "a", "", ".", "a.md"),
      (["a"], "a", "", ".", "a.md"),
      (["/", "a", "b"], "b", "", "/a", "/a/b.md"),
      (["a", "c", "b"], "b", "", "a//c", "a//c/b.md"),
      ([".", "a"], "a", "", ".", "./a.md"),
      (["a", "b"], "b", "", "a", "a/./b.md"),
      (["a"], "a", "", ".", "a.md"),
      (["."], "", "", ".", "."),
      ([".."], "", "", ".", ".."),
      (["a", ".."], "", "", "a", "a/.."),
      (["/", "a"], "a", "", "/", "/a.md"),
      ([".bashrc"], ".bashrc", "", ".", ".bashrc.md"),
      (["a.tar.gz"], "a.tar.gz", "gz", ".", "a.tar.md"),
      (["d", "f."], "f.", "", "d", "d/f.md"),
      (["x", "..."], "...", "", "x", "x/...md"),
      (["/", "a"], "a", "", "/", "//a.md"),
      (["/"], "", "", "/", "/."),
      (["a", "b"], "b", "", "a", "a/b.md"),
      (["."], "", "", ".", "./"),
      ([".", "a"], "a", "", ".", ".//a.md"),
      (["a"], "a", "", ".", "a.md"),
      (["/"], "", "", "/", "//"),
      (["/", "a"], "a", "", "/", "/a.md")] := by
  decide +kernel

def okPath : Result Unit Unit Unit (Value Unit Unit Unit) → Option String
  | .value (.ok (.scalar (.path path))) => String.fromUTF8? ⟨path.bytes.toArray⟩
  | _ => none

/-- As the verification binary prints `ok b/./c`, `ok b`, `ok a/b`, `ok .`,
`err path does not start with prefix`, `err prefix is empty` and
`err remainder climbs above the prefix` for `p"a//b/./c/".strip_prefix(p"a")`,
`p"/a/b"` without `p"/a"` and `p"/"`, `p"/a"` without `p"/a"`, `p"a/b"`
without `p"c"` and `p""`, and `p"a/../b"` without `p"a"`; `b/c`, `a/b/c` and
`.` for `relative_to`; and `true false true false` for `p"a/b/c"` starting with
`p"a/b"` and `p"a/bc"`, ending with `p"b/c"`, and `p"./a"` starting with
`p"a"`. -/
theorem path_prefix_witness :
    okPath (pathRun .stripPrefix "a//b/./c/" [pathOf "a"]) = some "b/./c" ∧
    okPath (pathRun .stripPrefix "/a/b" [pathOf "/a"]) = some "b" ∧
    okPath (pathRun .stripPrefix "/a/b" [pathOf "/"]) = some "a/b" ∧
    okPath (pathRun .stripPrefix "/a" [pathOf "/a"]) = some "." ∧
    errText (pathRun .stripPrefix "a/b" [pathOf "c"]) = some "path does not start with prefix" ∧
    errText (pathRun .stripPrefix "a/b" [pathOf ""]) = some "prefix is empty" ∧
    errText (pathRun .stripPrefix "a/../b" [pathOf "a"]) = some "remainder climbs above the prefix" ∧
    pathResult (pathRun .relativeTo "a/b/c" [pathOf "a"]) = some "b/c" ∧
    pathResult (pathRun .relativeTo "a/b/c" [pathOf "x"]) = some "a/b/c" ∧
    pathResult (pathRun .relativeTo "a/b" [pathOf "a/b"]) = some "." ∧
    boolResult (pathRun .startsWith "a/b/c" [pathOf "a/b"]) = some true ∧
    boolResult (pathRun .startsWith "a/b/c" [pathOf "a/bc"]) = some false ∧
    boolResult (pathRun .endsWith "a/b/c" [pathOf "b/c"]) = some true ∧
    boolResult (pathRun .startsWith "./a" [pathOf "a"]) = some false := by
  decide +kernel

/-- A `parent` that rebuilt the path from its components would give `a/c`
for `a//c/b`, where the binary keeps the original `a//c`. -/
theorem rebuilt_parent_rejected : pathResult (pathRun .parent "a//c/b" []) ≠ some "a/c" := by
  decide +kernel

/-- As the verification binary displays the file names `61 ff 62`, `e2 82`,
`c0 af`, `e2 82 78`, `f0 9f 98`, `e0 80 80 7a`, `ed a0 80` and
`f4 90 80 80`: one U+FFFD (`ef bf bd`) for each maximal prefix of a
well-formed sequence and for each byte that cannot begin one. -/
theorem lossy_display_witness :
    Xsh.Values.LossyUtf8.bytes [0x61, 0xFF, 0x62] = [0x61, 0xEF, 0xBF, 0xBD, 0x62] ∧
    Xsh.Values.LossyUtf8.bytes [0xE2, 0x82] = [0xEF, 0xBF, 0xBD] ∧
    Xsh.Values.LossyUtf8.bytes [0xC0, 0xAF] = [0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD] ∧
    Xsh.Values.LossyUtf8.bytes [0xE2, 0x82, 0x78] = [0xEF, 0xBF, 0xBD, 0x78] ∧
    Xsh.Values.LossyUtf8.bytes [0xF0, 0x9F, 0x98] = [0xEF, 0xBF, 0xBD] ∧
    Xsh.Values.LossyUtf8.bytes [0xE0, 0x80, 0x80, 0x7A] =
      [0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD, 0x7A] ∧
    Xsh.Values.LossyUtf8.bytes [0xED, 0xA0, 0x80] = [0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD] ∧
    Xsh.Values.LossyUtf8.bytes [0xF4, 0x90, 0x80, 0x80] =
      [0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD] ∧
    Xsh.Values.LossyUtf8.bytes "héllo €𝄞".toUTF8.toList = "héllo €𝄞".toUTF8.toList := by
  decide +kernel

/-- A decoder that replaced each invalid byte separately would give two
U+FFFD for the truncated `e2 82`. -/
theorem per_byte_replacement_rejected :
    Xsh.Values.LossyUtf8.bytes [0xE2, 0x82] ≠ [0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD] := by
  decide +kernel

/-- The `Digest` a `Bytes` method gives, through `hex()` or `base64()`. -/
def digestText (method : BytesMethod) (data : List UInt8) (render : String) : Option String :=
  match bytesRun method data [] with
  | .value (.scalar (.digest digest)) => textResult (digestMethod render digest [])
  | _ => none

/-- As the verification binary prints `2816…5a0e` for the SHA-256 of a
hundred `a` bytes, two blocks, `900150983cd24fb0d6963f7d28e17f72` for
`b"abc".md5().hex()`, `qZk+NkcGgWq6PiVxeFDCbJzQ2J0=` for
`b"abc".sha1().base64()` and `cf83…da3e` for `b"".sha512().hex()`, and as
FIPS 180-4 gives for SHA-256 and SHA-512 of `abc`. -/
theorem digest_witness :
    digestText .sha256 (List.replicate 100 97) "hex" =
      some "2816597888e4a0d3a36b82b83316ab32680eb8f00f8cd3b904d681246d285a0e" ∧
    digestText .md5 [97, 98, 99] "hex" = some "900150983cd24fb0d6963f7d28e17f72" ∧
    digestText .sha1 [97, 98, 99] "base64" = some "qZk+NkcGgWq6PiVxeFDCbJzQ2J0=" ∧
    digestText .sha512 [] "hex" = some
      "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e" ∧
    digestText .sha256 [97, 98, 99] "hex" =
      some "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" ∧
    digestText .sha512 [97, 98, 99] "hex" = some
      "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f" := by
  decide +kernel

/-- MD5 with big-endian words, as SHA uses, would not give the RFC 1321
digest of `abc`. -/
theorem md5_word_order_rejected :
    Xsh.Values.Digests.toBigEndian 16 (Xsh.Values.Digests.littleEndian (Xsh.Values.Digests.md5 [97, 98, 99])) ≠
      Xsh.Values.Digests.md5 [97, 98, 99] := by
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
