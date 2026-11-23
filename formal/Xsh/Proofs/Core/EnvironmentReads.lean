import Xsh.Proofs.Core.NumericMethods

namespace Xsh.Proofs.Core.EnvironmentReads

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

/-- The environment a value outcome leaves. -/
def frameOverlay {world : World Declaration Identity Source} {α : Type} : Outcome world α → Option Overlay
  | .value _ frame => some frame.overlay
  | .exec _ => none

/-- The `environment_reads` fragment root (SPEC 10.5): `e"NAME"` reads the
variable as the script's assignments leave it, else as the process started
with it, `Ok` of its text when it is UTF-8, `Err` of `invalid-utf8` when it is
not, and `Err` of `env-missing` when it is unset; and the environment is
evaluator state, not a binding, so a call starts with its caller's
assignments and its caller continues with the call's. -/
theorem environment_reads [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ key, evalExpr machine (fuel + 1) frame (.envString key) =
      .value (environmentString machine frame.overlay (machine.context.spelling key)) frame) ∧
    (∀ (name : String) bytes, (readEnvironment frame.overlay name.toUTF8.toList).or (machine.hostEnvironment name.toUTF8.toList) =
      some bytes → ∀ text, String.fromUTF8? ⟨bytes.toArray⟩ = some text →
        environmentString machine frame.overlay name = .ok (.scalar (.string text))) ∧
    (∀ name : String, (readEnvironment frame.overlay name.toUTF8.toList).or (machine.hostEnvironment name.toUTF8.toList) = none →
      environmentString machine frame.overlay name = .err (machine.envMissing name)) ∧
    (∀ (name : String) value, readEnvironment ((name.toUTF8.toList, value) :: frame.overlay) name.toUTF8.toList = some value) ∧
    (∀ caller scope defining parameters writable body kind,
      callBlock machine (fuel + 1) caller scope defining parameters writable body kind =
        returnTo caller kind (runBody machine fuel
          { Frame.ofScope defining caller.store parameters (some caller.store) scope with
            writable, overlay := caller.overlay, directory := caller.directory, consumers := caller.consumers, streams := caller.streams } body)) ∧
    (∀ (caller callee : Frame world) kind value,
      frameOverlay (returnTo caller kind (.returned value callee)) = some callee.overlay) ∧
    (∀ paths, environmentPaths machine frame.overlay = some paths →
      evalExpr machine (fuel + 1) frame .envPathList = .value (.environmentPaths paths) frame) ∧
    (∀ receiver method arguments function, errorConstructor machine frame receiver method = none →
      isEnvPathList receiver = false → envFunction machine frame receiver method = some function →
        evalExpr machine (fuel + 1) frame (.call (.field receiver method) arguments) =
          (threadArguments machine.context.spelling (evalExpr machine fuel) frame arguments).bind fun supplied after =>
            fromEval after (envCall machine after.overlay (currentDirectory machine after) function supplied)) := by
  refine ⟨fun _ => rfl, fun name bytes found text decoded => ?_, fun name missing => ?_,
    fun name value => by simp [readEnvironment, List.lookup], fun _ _ _ _ _ _ _ => rfl,
    fun _ _ kind _ => by cases kind <;> rfl, fun _ split => by simp only [evalExpr, split],
    fun _ _ arguments _ notError notPath found => by
      simp only [evalExpr, notError, notPath, Bool.false_eq_true, if_false, found]⟩
  · simp only [environmentString, found, decoded]
  · simp only [environmentString, missing]

/-- The `environment_scopes` fragment root (SPEC 10.4): `env (overlay) { body }`
evaluates its overlay, sets each converted field in order, and runs the body
as a value block, giving `Ok` of its tail; a NUL value is the scope's own
`Err`; and however the body ends, its frame returns with the environment the
scope entered with, the scope's overrides and the body's assignments undone.
`cd (target) { body }` (SPEC 10.4) likewise runs the body as a value block in
the directory `cdTarget` joins onto the current one, giving `Ok` of its tail,
the target's `Err` when the host has no directory there, and the directory it
entered with however the body ends; `fs.cwd()` gives `Ok` of the current
directory. -/
theorem environment_scopes [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ input body, runValue machine (fuel + 1) frame (.contextScope .env input body) =
      (evalExpr machine fuel frame input).bind fun overrides next =>
        match scopeOverrides machine overrides with
        | none => .exec .unsupported
        | some entries =>
          match applyOverrides machine next.overlay entries with
          | .error (.runtime error) => .value (.err error) next
          | .error failure => .exec (.failed failure next)
          | .ok applied =>
            (withOverlay next.overlay (valueScope machine fuel { next with overlay := applied } body)).bind
              fun tail last => .value (.ok tail) last) ∧
    (∀ (overlay : Overlay) (value : Value Declaration Identity Source) (last : Frame world),
      withOverlay overlay (.value value last) = .value value { last with overlay }) ∧
    (∀ (overlay : Overlay) failure (last : Frame world),
      withOverlay overlay (.exec (.failed failure last)) = .exec (.failed failure { last with overlay })) ∧
    (∀ (overlay : Overlay) value (last : Frame world),
      withOverlay overlay (.exec (.returned value last)) = .exec (.returned value { last with overlay })) ∧
    (∀ input body, runValue machine (fuel + 1) frame (.contextScope .cwd input body) =
      (evalExpr machine fuel frame input).bind fun target next =>
        match cdTarget machine (currentDirectory machine next) target with
        | none => .exec .unsupported
        | some (.error error) => .value (.err error) next
        | some (.ok directory) =>
          (withDirectory next.directory (valueScope machine fuel { next with directory := some directory } body)).bind
            fun tail last => .value (.ok tail) last) ∧
    (∀ directory (value : Value Declaration Identity Source) (last : Frame world),
      withDirectory directory (.value value last) = .value value { last with directory }) ∧
    (∀ directory failure (last : Frame world),
      withDirectory directory (.exec (.failed failure last)) = .exec (.failed failure { last with directory })) ∧
    (∀ directory value (last : Frame world),
      withDirectory directory (.exec (.returned value last)) = .exec (.returned value { last with directory })) ∧
    (∀ overlay directory, envCall machine overlay directory .cwd [] = .value (.ok (.scalar (.path directory)))) :=
  ⟨fun _ _ => rfl, fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ => rfl, fun _ _ _ => rfl,
    fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ => rfl⟩

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness
open Xsh.Proofs.Core.Hoisting.Witness
open Xsh.Proofs.Core.Scripts.Witness

def homeKey : Xsh.Syntax.RawName := name 240 (by decide)
def presetKey : Xsh.Syntax.RawName := name 241 (by decide)
def badKey : Xsh.Syntax.RawName := name 242 (by decide)
def setHome : Xsh.Syntax.RawName := name 243 (by decide)

def slot (index : Nat) : Xsh.Syntax.RawName :=
  [name 244 (by decide), name 245 (by decide), name 246 (by decide), name 247 (by decide),
    name 248 (by decide)].getD index (name 249 (by decide))

def scopedKey : Xsh.Syntax.RawName := name 250 (by decide)
def assignedKey : Xsh.Syntax.RawName := name 251 (by decide)
def envModule : Xsh.Syntax.RawName := name 384 (by decide)
def getName : Xsh.Syntax.RawName := name 385 (by decide)
def getOrName : Xsh.Syntax.RawName := name 386 (by decide)
def boolName : Xsh.Syntax.RawName := name 387 (by decide)
def intName : Xsh.Syntax.RawName := name 388 (by decide)
def pathName : Xsh.Syntax.RawName := name 389 (by decide)
def pathListName : Xsh.Syntax.RawName := name 390 (by decide)
def pathEntriesName : Xsh.Syntax.RawName := name 391 (by decide)
def prependName : Xsh.Syntax.RawName := name 392 (by decide)
def appendName : Xsh.Syntax.RawName := name 393 (by decide)
def popName : Xsh.Syntax.RawName := name 394 (by decide)
def regexModule : Xsh.Syntax.RawName := name 403 (by decide)
def compileName : Xsh.Syntax.RawName := name 404 (by decide)
def matchesName : Xsh.Syntax.RawName := name 405 (by decide)
def basenameName : Xsh.Syntax.RawName := name 406 (by decide)
def md5Name : Xsh.Syntax.RawName := name 407 (by decide)
def hexName : Xsh.Syntax.RawName := name 408 (by decide)
def fsModule : Xsh.Syntax.RawName := name 409 (by decide)
def cwdName : Xsh.Syntax.RawName := name 410 (by decide)

def keySpelling (named : Xsh.Syntax.RawName) : String :=
  if named = homeKey then "XSH_PROBE_HOME" else if named = presetKey then "XSH_PROBE_PRESET"
  else if named = scopedKey then "XSH_PROBE_A" else if named = assignedKey then "XSH_PROBE_B"
  else if named = badKey then "XSH_PROBE_BAD" else if named = envModule then "env" else if named = getName then "get"
  else if named = getOrName then "get_or" else if named = boolName then "bool" else if named = intName then "int"
  else if named = pathName then "path" else if named = pathListName then "path_list"
  else if named = pathEntriesName then "path_entries" else if named = prependName then "prepend"
  else if named = appendName then "append" else if named = popName then "pop"
  else if named = regexModule then "regex" else if named = compileName then "compile"
  else if named = matchesName then "matches" else if named = basenameName then "basename"
  else if named = md5Name then "md5" else if named = hexName then "hex"
  else if named = fsModule then "fs" else if named = cwdName then "cwd"
  else scriptMachine.context.spelling named

/-- The process started with `XSH_PROBE_PRESET=preset` and `XSH_PROBE_BAD`
holding the byte `0xFF`, and without `XSH_PROBE_HOME`. -/
def startEnvironment (name : List UInt8) : Option (List UInt8) :=
  if name = "XSH_PROBE_PRESET".toUTF8.toList then some "preset".toUTF8.toList
  else if name = "XSH_PROBE_BAD".toUTF8.toList then some [0xFF] else none

/-- A `Str` converts to its UTF-8 bytes as one argv item (SPEC 11.4). -/
def strArgv : Value Unit Unit Unit → Option (List UInt8)
  | .scalar (.string text) => some text.toUTF8.toList
  | _ => none

def envMachine : Machine witnessWorld :=
  { scriptMachine with
    context := { scriptMachine.context with spelling := keySpelling }
    resolution := { scriptMachine.resolution with argv := strArgv }
    hostEnvironment := startEnvironment }

def textOf : Option (Value Unit Unit Unit) → Option String
  | some (.ok (.scalar (.string text))) => some text
  | _ => none

def errOf : Option (Value Unit Unit Unit) → Option String
  | some (.err (.scalar (.string text))) => some text
  | _ => none

def readOutput (index : Nat) (outcome : Exec witnessWorld) : Option (Value Unit Unit Unit) :=
  match outcome with
  | .normal frame =>
    match frame.environment.cellOf (slot index) with
    | .ok cell => (frame.store cell).map fun found => found.contents.value
    | .error _ => none
  | _ => none

/-- `proc set_home() { e"XSH_PROBE_HOME" = "set-in-proc" }`, then a read
before and after calling it, an assignment at the top, and reads of the
preset and the non-UTF-8 variables, as the verification binary prints
`true`, `set-in-proc`, `top preset` and `true`. -/
def envScript : List Xsh.Syntax.Statement :=
  [.functionDef .proc setHome [] none none [.assign (.env homeKey) .set (.literal (.string "set-in-proc"))] none false,
   letOf (slot 0) (.envString homeKey),
   .expression (.call (ident setHome) []),
   letOf (slot 1) (.envString homeKey),
   .assign (.env homeKey) .set (.literal (.string "top")),
   letOf (slot 2) (.envString homeKey),
   letOf (slot 3) (.envString presetKey),
   letOf (slot 4) (.envString badKey)]

def envRun : Exec witnessWorld := runScript envMachine 90 fullStart envScript

theorem environment_witness :
    errOf (readOutput 0 envRun) = some "env-missing XSH_PROBE_HOME" ∧
    textOf (readOutput 1 envRun) = some "set-in-proc" ∧
    textOf (readOutput 2 envRun) = some "top" ∧
    textOf (readOutput 3 envRun) = some "preset" ∧
    errOf (readOutput 4 envRun) = some "invalid-utf8 XSH_PROBE_BAD" := by
  decide +kernel

/-- A call that starts with an empty overlay and drops its own on return, as
the machine did before, loses the proc's assignment: the read after the call
is the unset variable's `Err`. -/
theorem local_overlay_rejected :
    errOf (some (environmentString envMachine [] "XSH_PROBE_HOME")) = some "env-missing XSH_PROBE_HOME" ∧
      textOf (readOutput 1 envRun) = some "set-in-proc" := by
  decide +kernel

def isErrValue : Option (Value Unit Unit Unit) → Bool
  | some (.err _) => true
  | _ => false

def inScope (value : String) (body : List Xsh.Syntax.Statement) : Xsh.Syntax.Expr :=
  .contextScope .env (.record [.named scopedKey (.literal (.string value))]) body

/-- `let v = env ({XSH_PROBE_A: "inner"}) { e"XSH_PROBE_B" = "assigned"; e"XSH_PROBE_A"? }?`,
then reads of both variables after the scope, and a scope whose value holds
a NUL, as the verification binary prints `inner`, `true true` and `true`. -/
def scopeScript : List Xsh.Syntax.Statement :=
  [letOf (slot 0) (.tryExpr (inScope "inner"
      [.assign (.env assignedKey) .set (.literal (.string "assigned")), .expression (.tryExpr (.envString scopedKey))])),
   letOf (slot 1) (.envString scopedKey),
   letOf (slot 2) (.envString assignedKey),
   letOf (slot 3) (inScope "x\u0000y" [.expression (int "1")])]

def scopeRun : Exec witnessWorld := runScript envMachine 90 fullStart scopeScript

def plainText : Option (Value Unit Unit Unit) → Option String
  | some (.scalar (.string text)) => some text
  | _ => none

theorem environment_scope_witness :
    plainText (readOutput 0 scopeRun) = some "inner" ∧
    isErrValue (readOutput 1 scopeRun) = true ∧
    isErrValue (readOutput 2 scopeRun) = true ∧
    isErrValue (readOutput 3 scopeRun) = true := by
  decide +kernel

/-- A scope that keeps its overrides after it ends leaves `XSH_PROBE_A`
set; the model restores the environment it entered with. -/
theorem leaking_scope_rejected :
    textOf (some (environmentString envMachine [("XSH_PROBE_A".toUTF8.toList, "inner".toUTF8.toList)] "XSH_PROBE_A")) =
      some "inner" ∧ isErrValue (readOutput 1 scopeRun) = true := by
  decide +kernel

def cwdNow : Xsh.Syntax.Expr := .tryExpr (.call (.field (ident fsModule) cwdName) [])

def pathText : Option (Value Unit Unit Unit) → Option String
  | some (.scalar (.path path)) => String.fromUTF8? ⟨path.bytes.toArray⟩
  | _ => none

/-- In a program started in `/w`, where the host has no directory `/w/m`:
`cd (p"sub") { fs.cwd()? }?`, `cd ("sub") { cd (p"..") { fs.cwd()? }? }?`,
`fs.cwd()?` after them, `cd (p"m") { 1 }` and `cd (p"/") { cd (p"") {
fs.cwd()? }? }?`, as the verification binary, started in `PROBE`, prints
`PROBE/sub`, `PROBE/sub/..`, `PROBE`, the `No such file or directory` `Err`
and `/`. -/
def cdScript : List Xsh.Syntax.Statement :=
  [letOf (slot 0) (.tryExpr (.contextScope .cwd (.literal (.path "sub")) [.expression cwdNow])),
   letOf (slot 1) (.tryExpr (.contextScope .cwd (.literal (.string "sub"))
      [.expression (.tryExpr (.contextScope .cwd (.literal (.path "..")) [.expression cwdNow]))])),
   letOf (slot 2) cwdNow,
   letOf (slot 3) (.contextScope .cwd (.literal (.path "m")) [.expression (int "1")]),
   letOf (slot 4) (.tryExpr (.contextScope .cwd (.literal (.path "/"))
      [.expression (.tryExpr (.contextScope .cwd (.literal (.path "")) [.expression cwdNow]))]))]

def cdRun : Exec witnessWorld := runScript envMachine 90 fullStart cdScript

theorem directory_scope_witness :
    pathText (readOutput 0 cdRun) = some "/w/sub" ∧
    pathText (readOutput 1 cdRun) = some "/w/sub/.." ∧
    pathText (readOutput 2 cdRun) = some "/w" ∧
    errOf (readOutput 3 cdRun) = some "cwd missing" ∧
    pathText (readOutput 4 cdRun) = some "/" := by
  decide +kernel

/-- Normalizing the joined path, as `fs.resolve` would, loses the binary's
lexical `PROBE/sub/..`; joining an absolute target onto the directory, rather
than replacing it, loses its `/`. -/
theorem normalized_directory_rejected :
    (joinPath ⟨[47, 119, 47, 115, 117, 98], by decide⟩ ⟨[46, 46], by decide⟩).map NativePath.bytes =
      some "/w/sub/..".toUTF8.toList ∧
    (joinPath ⟨[47, 119], by decide⟩ ⟨[47], by decide⟩).map NativePath.bytes =
      some "/".toUTF8.toList := by
  decide +kernel

/-- The process started with `PATH=a::b`. -/
def pathMachine (path : Option (List UInt8)) : Machine witnessWorld :=
  { envMachine with
    hostEnvironment := fun name => if name = "PATH".toUTF8.toList then path else startEnvironment name }

def pathTexts (machine : Machine witnessWorld) : Option (List String) :=
  (environmentPaths machine []).bind fun paths => paths.mapM fun path => String.fromUTF8? ⟨path.bytes.toArray⟩

/-- As the verification binary prints `true true false` for `p"b"`, `p""` and
`p"c"` `in env.PATH` with `PATH=a::b`, and `false true false` with `PATH`
unset: the value splits at every `:`, an empty segment and an unset value
each giving an empty entry. -/
theorem environment_path_witness :
    pathTexts (pathMachine (some "a::b".toUTF8.toList)) = some ["a", "", "b"] ∧
    pathTexts (pathMachine none) = some [""] ∧
    pathTexts (pathMachine (some [])) = some [""] := by
  decide +kernel

/-- Dropping empty segments, as a whitespace split would, loses the binary's
empty entry. -/
theorem nonempty_segments_rejected : pathTexts (pathMachine none) ≠ some [] := by
  decide +kernel

def envGet (function : Xsh.Syntax.RawName) (arguments : List String) : Xsh.Syntax.Expr :=
  .call (.field (ident envModule) function) (arguments.map fun text => .positional (.literal (.string text)))

/-- `env.get` and `env.get_or` with computed names, as the verification
binary prints `ok preset`, the unset error, `ok fb`, `ok preset` and the
non-UTF-8 error twice for `env.get(k)` with `k = "XSH_PROBE_PRESET"`,
`env.get("XSH_PROBE_HOME")`, `env.get_or("XSH_PROBE_HOME", "fb")`,
`env.get_or(k, "fb")`, `env.get("XSH_PROBE_BAD")` and
`env.get_or("XSH_PROBE_BAD", "fb")`: `env.get` reads as `e"NAME"` does, and
`get_or` gives its fallback only when the variable is unset. -/
theorem env_get_witness :
    textOf (readOutput 0 (runScript envMachine 90 fullStart
      [letOf (slot 0) (envGet getName ["XSH_PROBE_PRESET"])])) = some "preset" ∧
    errOf (readOutput 0 (runScript envMachine 90 fullStart
      [letOf (slot 0) (envGet getName ["XSH_PROBE_HOME"])])) = some "env-missing XSH_PROBE_HOME" ∧
    textOf (readOutput 0 (runScript envMachine 90 fullStart
      [letOf (slot 0) (envGet getOrName ["XSH_PROBE_HOME", "fb"])])) = some "fb" ∧
    textOf (readOutput 0 (runScript envMachine 90 fullStart
      [letOf (slot 0) (envGet getOrName ["XSH_PROBE_PRESET", "fb"])])) = some "preset" ∧
    errOf (readOutput 0 (runScript envMachine 90 fullStart
      [letOf (slot 0) (envGet getName ["XSH_PROBE_BAD"])])) = some "invalid-utf8 XSH_PROBE_BAD" ∧
    errOf (readOutput 0 (runScript envMachine 90 fullStart
      [letOf (slot 0) (envGet getOrName ["XSH_PROBE_BAD", "fb"])])) = some "invalid-utf8 XSH_PROBE_BAD" := by
  decide +kernel

/-- A `get_or` that also fell back on undecodable bytes would hide the
binary's `invalid-utf8` error. -/
theorem lenient_get_or_rejected :
    textOf (readOutput 0 (runScript envMachine 90 fullStart
      [letOf (slot 0) (envGet getOrName ["XSH_PROBE_BAD", "fb"])])) ≠ some "fb" := by
  decide +kernel

/-- The process also started with `XSH_PROBE_FLAG=" Yes "`,
`XSH_PROBE_WORD=abc` and `XSH_PROBE_NUM=" -7 "`. -/
def typedMachine : Machine witnessWorld :=
  { envMachine with
    hostEnvironment := fun name =>
      if name = "XSH_PROBE_FLAG".toUTF8.toList then some " Yes ".toUTF8.toList
      else if name = "XSH_PROBE_WORD".toUTF8.toList then some "abc".toUTF8.toList
      else if name = "XSH_PROBE_NUM".toUTF8.toList then some " -7 ".toUTF8.toList
      else if name = "XSH_PROBE_LIST".toUTF8.toList then some "a::b".toUTF8.toList
      else startEnvironment name }

def readTyped (call : Xsh.Syntax.Expr) : Option (Value Unit Unit Unit) :=
  readOutput 0 (runScript typedMachine 90 fullStart [letOf (slot 0) call])

def envRead (function : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.Expr) : Xsh.Syntax.Expr :=
  .call (.field (ident envModule) function) (arguments.map .positional)

def text (value : String) : Xsh.Syntax.Expr := .literal (.string value)

def okBool : Option (Value Unit Unit Unit) → Option Bool
  | some (.ok (.scalar (.boolean value))) => some value
  | _ => none

def okInt : Option (Value Unit Unit Unit) → Option Int
  | some (.ok (.scalar (.integer value))) => some value.val
  | _ => none

def okPath : Option (Value Unit Unit Unit) → Option (List UInt8)
  | some (.ok (.scalar (.path path))) => some path.bytes
  | _ => none

/-- As the verification binary prints `true false true -7`, the
`not an integer` error, `7`, `abc`, the invalid-name error, an empty
`get_or` and the `invalid-utf8` error for `env.bool` of `" Yes "` and `abc`,
`env.bool` of an unset variable with fallback `true`, `env.int` of `" -7 "` and
`abc`, `env.int` of an unset variable with fallback `7`, `env.path` of `abc`,
`env.get("A=B")`, `env.get_or` of an unset variable without a fallback and
`env.int` of the non-UTF-8 variable. -/
theorem typed_env_witness :
    okBool (readTyped (envRead boolName [text "XSH_PROBE_FLAG"])) = some true ∧
    okBool (readTyped (envRead boolName [text "XSH_PROBE_WORD"])) = some false ∧
    okBool (readTyped (envRead boolName [text "XSH_PROBE_HOME", .literal (.bool true)])) = some true ∧
    okInt (readTyped (envRead intName [text "XSH_PROBE_NUM"])) = some (-7) ∧
    errOf (readTyped (envRead intName [text "XSH_PROBE_WORD"])) = some "environment value is not an integer" ∧
    okInt (readTyped (envRead intName [text "XSH_PROBE_HOME", int "7"])) = some 7 ∧
    okPath (readTyped (envRead pathName [text "XSH_PROBE_WORD"])) = some "abc".toUTF8.toList ∧
    errOf (readTyped (envRead getName [text "A=B"])) =
      some "environment names cannot be empty or contain NUL or `=`" ∧
    textOf (readTyped (envRead getOrName [text "XSH_PROBE_HOME"])) = some "" ∧
    errOf (readTyped (envRead intName [text "XSH_PROBE_BAD"])) = some "invalid-utf8 XSH_PROBE_BAD" := by
  decide +kernel

/-- A `bool` that compared the text without trimming and lowercasing would
read `" Yes "` as `false`. -/
theorem untrimmed_bool_rejected : okBool (readTyped (envRead boolName [text "XSH_PROBE_FLAG"])) ≠ some false := by
  decide +kernel

def okPaths : Option (Value Unit Unit Unit) → Option (List (List UInt8))
  | some (.ok (.list items)) => items.mapM fun
    | .scalar (.path path) => some path.bytes
    | _ => none
  | _ => none

/-- Each `path_entries` record's `raw` text and path bytes. -/
def okEntries : Option (Value Unit Unit Unit) → Option (List (String × List UInt8))
  | some (.ok (.list items)) => items.mapM fun
    | .record [("empty", _), ("index", _), ("path", .scalar (.path path)), ("raw", .scalar (.string raw))] =>
      some (raw, path.bytes)
    | _ => none
  | _ => none

/-- As the verification binary prints `3`, `[["a","a"],["","."],["b","b"]]`
and the unset error for `env.path_list` and `env.path_entries` of
`XSH_PROBE_LIST=a::b` and `env.path_list` of an unset variable: both split at
every `:`, and an empty entry's path is `.`. -/
theorem env_path_list_witness :
    okPaths (readTyped (envRead pathListName [text "XSH_PROBE_LIST"])) =
      some ["a".toUTF8.toList, [], "b".toUTF8.toList] ∧
    okEntries (readTyped (envRead pathEntriesName [text "XSH_PROBE_LIST"])) =
      some [("a", "a".toUTF8.toList), ("", ".".toUTF8.toList), ("b", "b".toUTF8.toList)] ∧
    errOf (readTyped (envRead pathListName [text "XSH_PROBE_HOME"])) = some "env-missing XSH_PROBE_HOME" := by
  decide +kernel

def pathCall (method : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.Expr) : Xsh.Syntax.Expr :=
  .call (.field .envPathList method) (arguments.map .positional)

/-- `PATH` as given, and `XSH_PROBE_Z=z`, `XSH_PROBE_Y=y` and
`XSH_PROBE_XY=x:y` for the paths the edits add. -/
def editMachine (path : Option (List UInt8)) : Machine witnessWorld :=
  { pathMachine path with
    hostEnvironment := fun name =>
      if name = "XSH_PROBE_Z".toUTF8.toList then some "z".toUTF8.toList
      else if name = "XSH_PROBE_Y".toUTF8.toList then some "y".toUTF8.toList
      else if name = "XSH_PROBE_XY".toUTF8.toList then some "x:y".toUTF8.toList
      else (pathMachine path).hostEnvironment name }

def editedRun (path : Option (List UInt8)) (script : List Xsh.Syntax.Statement) : Exec witnessWorld :=
  runScript (editMachine path) 90 fullStart script

def pathOf (key : String) : Xsh.Syntax.Expr := .tryExpr (envRead pathName [text key])

/-- With `PATH=a:b`: `env.PATH.prepend(p"z")`, `env.PATH.append(p"y")` and
`env.PATH.pop()`, reading `PATH` after each. -/
def pathEdits : List Xsh.Syntax.Statement :=
  [.expression (pathCall prependName [pathOf "XSH_PROBE_Z"]),
   letOf (slot 0) (envRead getName [text "PATH"]),
   .expression (pathCall appendName [pathOf "XSH_PROBE_Y"]),
   letOf (slot 1) (envRead getName [text "PATH"]),
   letOf (slot 2) (pathCall popName []),
   letOf (slot 3) (envRead getName [text "PATH"])]


/-- As the verification binary prints `z:a:b`, `z:a:b:y`, `y` and `z:a:b`
for the edits over `PATH=a:b`, an empty path and then an empty `PATH` for
`pop()` over an unset `PATH`, and fails `append(p"x:y")` with
`PATH entry contains invalid bytes`: the methods rewrite `PATH` joined with
`:`, and an entry holding `:` is refused. -/
theorem path_edit_witness :
    textOf (readOutput 0 (editedRun (some "a:b".toUTF8.toList) pathEdits)) = some "z:a:b" ∧
    textOf (readOutput 1 (editedRun (some "a:b".toUTF8.toList) pathEdits)) = some "z:a:b:y" ∧
    okPath (readOutput 2 (editedRun (some "a:b".toUTF8.toList) pathEdits)) = some "y".toUTF8.toList ∧
    textOf (readOutput 3 (editedRun (some "a:b".toUTF8.toList) pathEdits)) = some "z:a:b" ∧
    okPath (readOutput 0 (editedRun none [letOf (slot 0) (pathCall popName []),
      letOf (slot 1) (envRead getName [text "PATH"])])) = some [] ∧
    textOf (readOutput 1 (editedRun none [letOf (slot 0) (pathCall popName []),
      letOf (slot 1) (envRead getName [text "PATH"])])) = some "" ∧
    (match editedRun none [.expression (pathCall appendName [pathOf "XSH_PROBE_XY"])] with
      | .failed (.runtime (.scalar (.string message))) _ => message == "PATH entry contains invalid bytes"
      | _ => false) = true := by
  decide +kernel

/-- As the verification binary prints `true` for
`regex.compile("a=(b+)")?.matches("xa=bb")`, `b` for `p"a/b".basename()` and
`900150983cd24fb0d6963f7d28e17f72` for `b"abc".md5().hex()`: `regex.compile`
gives `Ok` of the regex its text spells, an `=` in it no environment-name
check, and method calls on a `Regex`, `Path` or `Digest` reach their
methods. -/
theorem regex_compile_witness :
    (match readTyped (.call (.field (.tryExpr (.call (.field (ident regexModule) compileName)
        [.positional (text "a=(b+)")])) matchesName) [.positional (text "xa=bb")]) with
      | some (.scalar (.boolean value)) => some value
      | _ => none) = some true ∧
    (match readTyped (.call (.field (.literal (.path "a/b")) basenameName) []) with
      | some (.scalar (.string value)) => some value
      | _ => none) = some "b" ∧
    (match readTyped (.call (.field (.call (.field (.literal (.bytes [97, 98, 99])) md5Name) []) hexName) []) with
      | some (.scalar (.string value)) => some value
      | _ => none) = some "900150983cd24fb0d6963f7d28e17f72" := by
  decide +kernel

def l1Trace : Xsh.Syntax.RawName := name 270 (by decide)
def l1Note (n : String) : Xsh.Syntax.Statement :=
  .assign (.name l1Trace) .add (.list [int n])

def l1Scopes : List Xsh.Syntax.Statement :=
  [.binding .var (.name l1Trace) none (.list []),
   .expression (.tryExpr (inScope "inner"
     [.defer .exit (.valueBlock [l1Note "1"]),
      .expression (.tryExpr (.contextScope .cwd (.literal (.path "/"))
        [.defer .exit (.valueBlock [l1Note "2"]), l1Note "7"]))]))]

def l1ReadTrace (outcome : Exec witnessWorld) : Option (List Int) :=
  match outcome with
  | .normal frame =>
    match frame.environment.cellOf l1Trace with
    | .ok cell => (frame.store cell).bind fun found =>
      match found.contents.value with
      | .list items => items.mapM fun
        | .scalar (.integer value) => some value.val
        | _ => none
      | _ => none
    | .error _ => none
  | _ => none

theorem l1_scoped_defer_witness :
    l1ReadTrace (runScript envMachine 90 fullStart l1Scopes) = some [7, 2, 1] := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.EnvironmentReads
