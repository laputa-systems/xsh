import Xsh.Proofs.Core.EnvironmentReads

namespace Xsh.Proofs.Core.Exits

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

/-- The `script_exit` fragment root (SPEC 8.9): `exit status` evaluates its
status once and ends the script with it when it is from 0 to 255, else fails
with the runtime `exit-status`; leaving a scope, it runs the scope's
deferred actions, `errdefer` among them only for a nonzero status or after a
cleanup failure; a cleanup failure is reported and the exit stays primary;
`try` does not capture it; and a call's `exit` ends its caller too. -/
theorem script_exit [DecidableEq Declaration] {world : World Declaration Identity Source} (machine : Machine world)
    (fuel : Nat) (frame : Frame world) :
    (∀ status, runStatement machine (fuel + 1) frame (.exit status) =
      (evalExpr machine fuel frame status).toExec fun value next =>
        match value.eraseNominal with
        | .scalar (.integer code) =>
          if 0 ≤ code.val ∧ code.val ≤ 255 then .exited code.val.toNat next
          else .failed (.runtime machine.exitStatus) next
        | _ => .unsupported) ∧
    (∀ code (last : Frame world), exitsWithError (.exited code last) = (code != 0 || !last.reported.isEmpty)) ∧
    (∀ failure store code (last : Frame world),
      cleanupFailed failure store (.exited code last) =
        .exited code { last with store := store, reported := last.reported ++ [failure] }) ∧
    (∀ code (last : Frame world), capture (.exec (.exited code last)) = .exec (.exited code last)) ∧
    (∀ (caller callee : Frame world) kind code,
      returnTo caller kind (.exited code callee) =
        .exec (.exited code { caller with store := keep caller.store callee.store, overlay := callee.overlay })) := by
  exact ⟨fun _ => rfl, fun _ _ => rfl, fun _ _ _ _ => rfl, fun _ _ => rfl, fun _ _ _ _ => rfl⟩

/-- The `retry_backoff` fragment root (SPEC 8.8): `retry backoff first..cap
within limit` evaluates its three durations once, in order, and runs as a
written list of the delays it takes; the schedule is `first`, then twice the
delay before, never longer than `cap`; and a delay is taken only if it would
end within `limit` of the first attempt's start by the machine's clock, the
first one that would not ending the retries. -/
theorem retry_backoff [DecidableEq Declaration] {world : World Declaration Identity Source} (machine : Machine world)
    (fuel : Nat) (frame : Frame world) :
    (∀ durations pattern body, runValue machine (fuel + 1) frame (.retry .backoff durations pattern body) =
      (threadAll (evalExpr machine fuel) frame durations).bind fun written next =>
        match backoffDelays machine written with
        | some scheduled => retryLoop machine fuel next pattern body scheduled
        | none => .exec .unsupported) ∧
    (∀ first cap count, backoffSchedule first cap (count + 1) =
      min first cap :: backoffSchedule (min (2 * first) cap) cap count) ∧
    (∀ first cap count, ∀ delay ∈ backoffSchedule first cap count, delay ≤ cap) ∧
    (∀ clock limit index delay rest, clock index + delay ≤ limit →
      takenDelays clock limit index (delay :: rest) = delay :: takenDelays clock limit (index + 1) rest) ∧
    (∀ clock limit index delay rest, limit < clock index + delay →
      takenDelays clock limit index (delay :: rest) = []) := by
  refine ⟨fun _ _ _ => rfl, fun _ _ _ => rfl, ?_, fun clock limit index delay rest fits => by simp [takenDelays, fits],
    fun clock limit index delay rest late => by simp [takenDelays, Nat.not_le.mpr late]⟩
  intro first cap count
  induction count generalizing first with
  | zero => simp [backoffSchedule]
  | succ count ih =>
    intro delay inside
    simp only [backoffSchedule, List.mem_cons] at inside
    rcases inside with rfl | later
    · exact Nat.min_le_right _ _
    · exact ih _ delay later

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness
open Xsh.Proofs.Core.Hoisting.Witness
open Xsh.Proofs.Core.Scripts.Witness

def logVar : Xsh.Syntax.RawName := name 260 (by decide)
def exitingProc : Xsh.Syntax.RawName := name 261 (by decide)
def caught : Xsh.Syntax.RawName := name 262 (by decide)

def bumpBy (lexeme : String) : Xsh.Syntax.Statement := .assign (.name logVar) .add (int lexeme)

def code (lexeme : String) : Xsh.Syntax.Expr := .literal (.integer lexeme)

/-- `proc f() { defer { log += 1 }; errdefer { log += 7 }; exit status }`
called inside `try` in a block whose own `defer` adds `2`. -/
def exitScript (status : Xsh.Syntax.Expr) : List Xsh.Syntax.Statement :=
  [varOf logVar "0",
   .functionDef .proc exitingProc [] none none
     [.defer .exit (.valueBlock [bumpBy "1"]), .defer .error (.valueBlock [bumpBy "7"]), .exit status] none false,
   .expression (.valueBlock [.defer .exit (.valueBlock [bumpBy "2"]),
     letOf caught (.capture [.expression (.call (ident exitingProc) [])])])]

def exitRun (status : Xsh.Syntax.Expr) : Exec witnessWorld := runScript scriptMachine 90 fullStart (exitScript status)

/-- The status and the counter an exit leaves. -/
def exitedWith : Exec witnessWorld → Option (Nat × Int)
  | .exited status frame =>
    match frame.environment.cellOf logVar with
    | .ok cell => (frame.store cell).bind fun found =>
        match found.contents.value with
        | .scalar (.integer value) => some (status, value.val)
        | _ => none
    | .error _ => none
  | _ => none

def runtimeText : Exec witnessWorld → Option String
  | .failed (.runtime (.scalar (.string text))) _ => some text
  | _ => none

/-- As the verification binary prints `errdefer f`, `defer f` and
`defer top` and exits 3 for `exit 3` inside `try`, runs only `defer f` for
`exit 0`, and fails with `exit-status` for an out-of-range status: the
counter is `7 + 1 + 2` and `1 + 2`. -/
theorem exit_witness :
    exitedWith (exitRun (code "7")) = some (7, 10) ∧
    exitedWith (exitRun (code "0")) = some (0, 3) ∧
    runtimeText (exitRun (code "9223372036854775807")) = some "exit-status" := by
  decide +kernel

/-- `errdefer { log += 7 }; defer { assert false }; exit 0` in a block: as the
binary runs the `errdefer` after the cleanup failure and still exits 0, the
counter is `7`, the status `0`, and the failure reported. -/
theorem cleanup_failure_keeps_exit :
    (match runScript scriptMachine 90 fullStart
        [varOf logVar "0", .expression (.valueBlock [.defer .error (.valueBlock [bumpBy "7"]),
          .defer .exit (.valueBlock [.assert (.literal (.bool false)) none]), .exit (code "0")])] with
      | .exited status frame => some (status, frame.reported.length)
      | _ => none) = some (0, 1) ∧
    exitedWith (runScript scriptMachine 90 fullStart
        [varOf logVar "0", .expression (.valueBlock [.defer .error (.valueBlock [bumpBy "7"]),
          .defer .exit (.valueBlock [.assert (.literal (.bool false)) none]), .exit (code "0")])]) = some (0, 7) := by
  decide +kernel

/-- A `try` that captured the exit would continue the script normally; the
model's run ends with the exit. -/
theorem captured_exit_rejected :
    (match exitRun (code "7") with
      | .normal _ => true
      | _ => false) = false := by
  decide +kernel

def firstName : Xsh.Syntax.RawName := name 263 (by decide)
def capName : Xsh.Syntax.RawName := name 264 (by decide)
def limitName : Xsh.Syntax.RawName := name 265 (by decide)
def failingName : Xsh.Syntax.RawName := name 266 (by decide)

def millis (value : Nat) (bounded : value < signed64Width) : Value Unit Unit Unit := .scalar (.duration ⟨value, bounded⟩)

/-- Attempts that take no time: each one ends when the delays before it do. -/
def noTimeClock (index : Nat) : Nat := (backoffSchedule 100 250 index).sum

def backoffMachine : Machine witnessWorld := { machine with retryClock := noTimeClock }

/-- `first = 100ms`, `cap = 250ms`, `limit = 1s` and an attempt that always
raises. -/
def backoffFrame : Frame witnessWorld :=
  { start with locals := [(firstName, millis 100 (by decide)), (capName, millis 250 (by decide)),
      (limitName, millis 1000 (by decide)), (failingName, .err (.scalar (.string "boom")))] }

def backoffRun : Exec witnessWorld :=
  runStatements backoffMachine 60 backoffFrame
    [.binding .letBinding (.name xName) none
      (.retry .backoff [ident firstName, ident capName, ident limitName] none
        [.expression (.tryExpr (ident failingName))])]

def waitedMillis : Exec witnessWorld → Option (List Nat)
  | .normal frame => some (frame.waited.filterMap fun
      | .scalar (.duration value) => some value.val
      | _ => none)
  | _ => none

/-- With attempts that take no time the form waits `100ms`, `200ms`, then
`250ms` twice, as the schedule doubles to the cap; the next `250ms` would end
at `1050ms`, past the limit, so the fifth attempt is the last and its `Err`
is the result. -/
theorem backoff_witness : waitedMillis backoffRun = some [100, 200, 250, 250] := by
  decide +kernel

/-- A schedule without the cap waits `400ms` after the third attempt; the
model waits `250ms`. -/
theorem uncapped_backoff_rejected :
    (List.range 3).map (fun index => 100 * 2 ^ index) = [100, 200, 400] ∧
      waitedMillis backoffRun = some [100, 200, 250, 250] := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Exits
