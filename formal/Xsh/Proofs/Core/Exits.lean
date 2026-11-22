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

end Witness

end Xsh.Proofs.Core.Exits
