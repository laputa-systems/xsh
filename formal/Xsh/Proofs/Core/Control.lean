import Xsh.Semantics.CoreControl
import Xsh.Proofs.Core.EvalStatements

namespace Xsh.Proofs.Core.Control

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Semantics.CoreControl
open Xsh.Proofs.Core.Eval

variable {Declaration Identity Source : Type}

section Fuel

def Exec.Refines {world : World Declaration Identity Source} (first second : Exec world) : Prop :=
  first = .outOfFuel ∨ first = second

theorem exec_refl {world : World Declaration Identity Source} (outcome : Exec world) : Exec.Refines outcome outcome :=
  Or.inr rfl

theorem andThen_refines {world : World Declaration Identity Source} {first second : Exec world}
    {next next' : Frame world → Exec world} (head : Exec.Refines first second)
    (tail : ∀ frame, Exec.Refines (next frame) (next' frame)) :
    Exec.Refines (first.andThen next) (second.andThen next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | normal frame => exact tail frame
    | _ => exact Or.inr rfl

theorem iterate_refines {world : World Declaration Identity Source} {first second : Exec world}
    {next next' : Frame world → Exec world} (head : Exec.Refines first second)
    (tail : ∀ frame, Exec.Refines (next frame) (next' frame)) :
    Exec.Refines (first.iterate next) (second.iterate next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | normal frame => exact tail frame
    | continued frame => exact tail frame
    | _ => exact Or.inr rfl

theorem withinScope_refines {world : World Declaration Identity Source} (outer : Frame world)
    {first second : Exec world} (head : Exec.Refines first second) :
    Exec.Refines (withinScope outer first) (withinScope outer second) := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    exact Or.inr rfl

theorem fromResult_refines {world : World Declaration Identity Source} (frame : Frame world)
    {first second : Result Declaration Identity Source (Value Declaration Identity Source)}
    {next next' : Value Declaration Identity Source → Exec world} (head : Refines first second)
    (tail : ∀ value, Exec.Refines (next value) (next' value)) :
    Exec.Refines (fromResult frame first next) (fromResult frame second next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | value value => exact tail value
    | _ => exact Or.inr rfl

theorem bodyValue_refines {world : World Declaration Identity Source} {first second : Exec world}
    (head : Exec.Refines first second) : Refines (bodyValue first) (bodyValue second) := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    exact Or.inr rfl

variable [DecidableEq Declaration]

/-- Every machine function at `fuel` is refined by the same function at
`bigger`. -/
structure Refining {world : World Declaration Identity Source} (machine : Machine world) (fuel bigger : Nat) :
    Prop where
  expr : ∀ store environment locals expression,
    Refines (evalExpr machine fuel store environment locals expression)
      (evalExpr machine bigger store environment locals expression)
  call : ∀ store environment locals body,
    Refines (callBlock machine fuel store environment locals body) (callBlock machine bigger store environment locals body)
  body : ∀ frame statements,
    Exec.Refines (runBody machine fuel frame statements) (runBody machine bigger frame statements)
  tail : ∀ frame statements,
    Exec.Refines (runTail machine fuel frame statements) (runTail machine bigger frame statements)
  statements : ∀ frame statements,
    Exec.Refines (runStatements machine fuel frame statements) (runStatements machine bigger frame statements)
  block : ∀ frame statements,
    Exec.Refines (runBlock machine fuel frame statements) (runBlock machine bigger frame statements)
  bound : ∀ frame captures statements,
    Exec.Refines (runBound machine fuel frame captures statements) (runBound machine bigger frame captures statements)
  statement : ∀ frame statement,
    Exec.Refines (runStatement machine fuel frame statement) (runStatement machine bigger frame statement)
  branches : ∀ frame branches otherwise,
    Exec.Refines (runIf machine fuel frame branches otherwise) (runIf machine bigger frame branches otherwise)
  whiles : ∀ frame condition body,
    Exec.Refines (runWhile machine fuel frame condition body) (runWhile machine bigger frame condition body)
  fors : ∀ frame target items body,
    Exec.Refines (runFor machine fuel frame target items body) (runFor machine bigger frame target items body)
  loops : ∀ frame body, Exec.Refines (runLoop machine fuel frame body) (runLoop machine bigger frame body)
  arms : ∀ frame value arms,
    Exec.Refines (runMatch machine fuel frame value arms) (runMatch machine bigger frame value arms)

set_option hygiene false in
/-- Close a refinement goal between the same machine step at two fuels, given
`inner` for the lower fuels. -/
local macro "close_refines" : tactic => `(tactic| repeat' (first
  | exact exec_refl _
  | exact inner.statement _ _
  | exact inner.statements _ _
  | exact inner.block _ _
  | exact inner.bound _ _ _
  | exact inner.branches _ _ _
  | exact inner.whiles _ _ _
  | exact inner.fors _ _ _ _
  | exact inner.loops _ _
  | exact inner.arms _ _ _
  | exact inner.tail _ _
  | refine fromResult_refines _ (inner.expr _ _ _ _) fun _ => ?_
  | refine andThen_refines ?_ fun _ => ?_
  | refine iterate_refines ?_ fun _ => ?_
  | refine withinScope_refines _ ?_
  | split))

theorem refining {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel extra, Refining machine fuel (fuel + extra)
  | 0, _ => {
      expr := fun _ _ _ _ => Or.inl (by simp [evalExpr])
      call := fun _ _ _ _ => Or.inl (by simp [callBlock])
      body := fun _ _ => Or.inl (by simp [runBody])
      tail := fun _ _ => Or.inl (by simp [runTail])
      statements := fun _ _ => Or.inl (by simp [runStatements])
      block := fun _ _ => Or.inl (by simp [runBlock])
      bound := fun _ _ _ => Or.inl (by simp [runBound])
      statement := fun _ _ => Or.inl (by simp [runStatement])
      branches := fun _ _ _ => Or.inl (by simp [runIf])
      whiles := fun _ _ _ => Or.inl (by simp [runWhile])
      fors := fun _ _ _ _ => Or.inl (by simp [runFor])
      loops := fun _ _ => Or.inl (by simp [runLoop])
      arms := fun _ _ _ => Or.inl (by simp [runMatch]) }
  | fuel + 1, extra => by
    have inner := refining machine fuel extra
    rw [show fuel + 1 + extra = (fuel + extra) + 1 by omega]
    exact {
      expr := fun store environment locals expression => by
        simp only [evalExpr]
        exact step_refines machine.context environment store (inner.expr store) (inner.call store) locals expression
      call := fun store environment locals body => by
        simp only [callBlock]
        exact bodyValue_refines (inner.body _ _)
      body := fun frame statements => by
        simp only [runBody]
        exact withinScope_refines frame (inner.tail _ _)
      tail := fun frame statements => by simp only [runTail]; close_refines
      statements := fun frame statements => by simp only [runStatements]; close_refines
      block := fun frame statements => by simp only [runBlock]; close_refines
      bound := fun frame captures statements => by simp only [runBound]; close_refines
      statement := fun frame statement => by simp only [runStatement]; close_refines
      branches := fun frame branches otherwise => by simp only [runIf]; close_refines
      whiles := fun frame condition body => by simp only [runWhile]; close_refines
      fors := fun frame target items body => by simp only [runFor]; close_refines
      loops := fun frame body => by simp only [runLoop]; close_refines
      arms := fun frame value arms => by simp only [runMatch]; close_refines }

end Fuel

end Xsh.Proofs.Core.Control
