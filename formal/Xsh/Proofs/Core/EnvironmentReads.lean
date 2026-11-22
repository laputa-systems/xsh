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

def keySpelling (named : Xsh.Syntax.RawName) : String :=
  if named = homeKey then "XSH_PROBE_HOME" else if named = presetKey then "XSH_PROBE_PRESET"
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

end Witness

end Xsh.Proofs.Core.EnvironmentReads
