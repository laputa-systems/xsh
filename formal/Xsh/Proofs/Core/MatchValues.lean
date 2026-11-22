import Xsh.Proofs.Core.Control

namespace Xsh.Proofs.Core.MatchValues

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

/-- The `match_values` fragment root (SPEC 6.8): `match` as an expression
evaluates its subject once and then tries its arms in order. An arm whose
pattern fails passes to the next; an unguarded arm that selects the subject
gives the value of its body in a scope holding the pattern's captures; with
no arm left the match fails with the runtime `match-no-arm`. Whatever it
gives, it leaves the environment and locals as it found them, so captures do
not escape, and keeps every cell already in the store. -/
theorem match_values [DecidableEq Declaration] {world : World Declaration Identity Source} (machine : Machine world)
    (fuel : Nat) (frame : Frame world) :
    (∀ subject arms, runValue machine (fuel + 1) frame (.matchExpr subject arms) =
      (evalExpr machine fuel frame subject).bind fun value next => matchValue machine fuel next value arms) ∧
    (∀ value, matchValue machine (fuel + 1) frame value [] =
      .exec (.failed (.runtime machine.context.errors.matchNoArm) frame)) ∧
    (∀ value pattern guard body rest, matchPattern machine.context machine.typeTest pattern value = .failed →
      matchValue machine (fuel + 1) frame value ((pattern, guard, body) :: rest) =
        matchValue machine fuel frame value rest) ∧
    (∀ value pattern body rest captures, matchPattern machine.context machine.typeTest pattern value = .matched captures →
      matchValue machine (fuel + 1) frame value ((pattern, none, body) :: rest) =
        settle (leaveBlock machine fuel frame)
          ((bindCaptures machine frame.enter captures).thenValued fun bound => runValue machine fuel bound body)) ∧
    (∀ value arms, VRestores frame (matchValue machine fuel frame value arms)) ∧
    (∀ value arms, VKeeps frame.store (matchValue machine fuel frame value arms)) := by
  refine ⟨fun _ _ => rfl, fun _ => rfl, fun value pattern guard body rest failed => ?_,
    fun value pattern body rest captures matched => ?_, fun value arms => (restoring machine fuel).matchValues frame value arms,
    fun value arms => (persisting machine fuel).matchValues frame value arms⟩
  · simp only [matchValue, failed]
  · simp only [matchValue, matched]

end Root

namespace Witness

open Xsh.Proofs.Core.Control.Witness

def letMatch (subject : Xsh.Syntax.Expr) (arms : List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × Xsh.Syntax.Expr)) :
    List Xsh.Syntax.Statement :=
  [.binding .letBinding (.name xName) none (.matchExpr subject arms)]

/-- `match subject { 1 => 1, n if n > 7 => n, else => 0 }`. -/
def guarded : List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × Xsh.Syntax.Expr) :=
  [(.literal (.integer "1"), none, int "1"), (.binding nName, some (.binary .gt (ident nName) (int "7")), ident nName),
    (.wildcard, none, int "0")]

/-- `match 1 { 1 => { let a = 7; a }, else => 0 }`. -/
def blockArm : List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × Xsh.Syntax.Expr) :=
  [(.literal (.integer "1"), none,
      .valueBlock [.binding .letBinding (.name aName) none (int "7"), .expression (ident aName)]),
    (.wildcard, none, int "0")]

def runtimeMessage : Exec witnessWorld → Option String
  | .failed (.runtime (.scalar (.string text))) _ => some text
  | _ => none

/-- As the verification binary does for `label(9)` and `label(3)` over
`match n { 1 => "one", 2 => "two", _ if n > 5 => "big", else => "other" }`:
a holding guard selects its arm, a failing guard passes to the next arm, and
the arm's capture does not resolve after the match. -/
theorem guarded_match :
    readInt xName (run (letMatch (int "8") guarded)) = some 8 ∧
    readInt xName (run (letMatch (int "7") guarded)) = some 0 ∧
    resolves nName (run (letMatch (int "8") guarded)) = false := by
  decide +kernel

/-- An arm body that is a block gives its tail. -/
theorem block_arm : readInt xName (run (letMatch (int "1") blockArm)) = some 7 := by decide +kernel

/-- With no arm that selects the subject the match fails with the runtime
`match-no-arm`, as the verification binary reports for a statement `match`
over `3` with only a `1` arm. -/
theorem no_arm_fails :
    runtimeMessage (run (letMatch (int "7") [(.literal (.integer "1"), none, int "1")])) = some "match-no-arm" := by
  decide +kernel

/-- A match that ignores guards gives `7` for subject `7`; the model gives `0`. -/
theorem guard_ignoring_rejected :
    readInt xName (run (letMatch (int "7") (guarded.map fun (pattern, _, body) => (pattern, none, body)))) = some 7 ∧
      readInt xName (run (letMatch (int "7") guarded)) = some 0 := by
  decide +kernel

/-- Binding the captures in the enclosing scope leaves `n` resolvable; the
model's match does not. -/
theorem leaking_captures_rejected :
    resolves nName (bindCaptures machine start [(nName, .scalar .null)]) = true ∧
      resolves nName (run (letMatch (int "8") guarded)) = false := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.MatchValues
