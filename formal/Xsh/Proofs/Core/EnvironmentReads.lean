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
            writable, overlay := caller.overlay } body)) ∧
    (∀ (caller callee : Frame world) kind value,
      frameOverlay (returnTo caller kind (.returned value callee)) = some callee.overlay) := by
  refine ⟨fun _ => rfl, fun name bytes found text decoded => ?_, fun name missing => ?_,
    fun name value => by simp [readEnvironment, List.lookup], fun _ _ _ _ _ _ _ => rfl,
    fun _ _ kind _ => by cases kind <;> rfl⟩
  · simp only [environmentString, found, decoded]
  · simp only [environmentString, missing]

/-- The `environment_scopes` fragment root (SPEC 10.4): `env (overlay) { body }`
evaluates its overlay, sets each converted field in order, and runs the body
as a value block, giving `Ok` of its tail; a NUL value is the scope's own
`Err`; and however the body ends, its frame returns with the environment the
scope entered with, the scope's overrides and the body's assignments undone. -/
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
      withOverlay overlay (.exec (.returned value last)) = .exec (.returned value { last with overlay })) :=
  ⟨fun _ _ => rfl, fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ _ => rfl⟩

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

def keySpelling (named : Xsh.Syntax.RawName) : String :=
  if named = homeKey then "XSH_PROBE_HOME" else if named = presetKey then "XSH_PROBE_PRESET"
  else if named = scopedKey then "XSH_PROBE_A" else if named = assignedKey then "XSH_PROBE_B"
  else if named = badKey then "XSH_PROBE_BAD" else scriptMachine.context.spelling named

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

end Witness

end Xsh.Proofs.Core.EnvironmentReads
