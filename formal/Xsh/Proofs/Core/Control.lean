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

section Persistence

/-- Every cell of `before` is still in `after`, at the same type. -/
def Persists {world : World Declaration Identity Source} (before after : Store world) : Prop :=
  ∀ cell found, before cell = some found → ∃ kept, after cell = some kept ∧ kept.contents.typeTag = found.contents.typeTag

theorem persists_refl {world : World Declaration Identity Source} (store : Store world) : Persists store store :=
  fun _ found present => ⟨found, present, rfl⟩

theorem persists_trans {world : World Declaration Identity Source} {first second third : Store world}
    (head : Persists first second) (tail : Persists second third) : Persists first third := by
  intro cell found present
  obtain ⟨middle, middlePresent, middleType⟩ := head cell found present
  obtain ⟨last, lastPresent, lastType⟩ := tail cell middle middlePresent
  exact ⟨last, lastPresent, lastType.trans middleType⟩

theorem persists_keep {world : World Declaration Identity Source} {before after : Store world}
    (head : Persists before after) : Persists before (keep before after) := by
  intro cell found present
  obtain ⟨kept, keptPresent, keptType⟩ := head cell found present
  exact ⟨kept, by simp [keep, present, keptPresent], keptType⟩

def frameOf {world : World Declaration Identity Source} : Exec world → Option (Frame world)
  | .normal frame | .broke frame | .continued frame | .returned _ frame | .failed _ frame | .internal _ frame =>
    some frame
  | _ => none

/-- Every frame an outcome carries keeps the cells of `before`. -/
def Keeps {world : World Declaration Identity Source} (before : Store world) (outcome : Exec world) : Prop :=
  ∀ frame, frameOf outcome = some frame → Persists before frame.store

theorem andThen_persists {world : World Declaration Identity Source} {before : Store world} {first : Exec world}
    {next : Frame world → Exec world} (head : Keeps before first)
    (tail : ∀ frame, Keeps frame.store (next frame)) : Keeps before (first.andThen next) := by
  cases first with
  | normal frame =>
    intro last present
    exact persists_trans (head frame rfl) (tail frame last present)
  | _ => exact head

theorem iterate_persists {world : World Declaration Identity Source} {before : Store world} {first : Exec world}
    {next : Frame world → Exec world} (head : Keeps before first)
    (tail : ∀ frame, Keeps frame.store (next frame)) : Keeps before (first.iterate next) := by
  cases first with
  | normal frame =>
    intro last present
    exact persists_trans (head frame rfl) (tail frame last present)
  | continued frame =>
    intro last present
    exact persists_trans (head frame rfl) (tail frame last present)
  | broke frame => exact head
  | _ => exact head

theorem withinScope_persists {world : World Declaration Identity Source} (outer : Frame world) {inner : Exec world}
    (head : Keeps outer.store inner) : Keeps outer.store (withinScope outer inner) := by
  cases inner <;> first
    | (intro last present; cases present; exact persists_keep (head _ rfl))
    | exact head

theorem fromResult_persists {world : World Declaration Identity Source} (frame : Frame world)
    (result : Result Declaration Identity Source (Value Declaration Identity Source))
    {next : Value Declaration Identity Source → Exec world} (tail : ∀ value, Keeps frame.store (next value)) :
    Keeps frame.store (fromResult frame result next) := by
  cases result with
  | value value => exact tail value
  | failure failure => intro last present; cases present; exact persists_refl _
  | _ => intro last present; cases present

theorem fromStep_persists {world : World Declaration Identity Source} (frame : Frame world) (environment : Environment)
    (locals : Locals Declaration Identity Source) (step : StoreStep world) (kept : Persists frame.store step.storeAfter) :
    Keeps frame.store (fromStep frame environment locals step) := by
  cases step <;> (intro last present; cases present; exact kept)

theorem self_persists {world : World Declaration Identity Source} (frame : Frame world) (outcome : Exec world)
    (same : ∀ last, frameOf outcome = some last → last.store = frame.store) : Keeps frame.store outcome := by
  intro last present
  rw [same last present]
  exact persists_refl _

variable [DecidableEq Declaration]

theorem declareAll_persists (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (mutability : Mutability) (bindings : List (Binding Declaration Identity Source)) :
    Persists store (declareAll world decisions store mutability store bindings).storeAfter := by
  cases declared : declareAll world decisions store mutability store bindings with
  | committed next =>
    obtain ⟨fresh, _, _, others⟩ :=
      Xsh.Proofs.Core.Writes.declareAll_committed world decisions store mutability bindings store next declared
    intro cell found present
    have outside : cell ∉ bindings.map Prod.fst := by
      intro inside
      obtain ⟨binding, member, same⟩ := List.mem_map.mp inside
      have empty := (fresh binding member).1
      rw [same, present] at empty
      cases empty
    exact ⟨found, by simp only [StoreStep.storeAfter]; rw [others cell outside, present], rfl⟩
  | _ =>
    have kept := Xsh.Proofs.Core.Writes.declareAll_keeps_original world decisions store mutability bindings store
      (by intro next same; rw [declared] at same; cases same)
    rw [declared] at kept
    rw [kept]
    exact persists_refl _

theorem declareTarget_persists (world : World Declaration Identity Source) (decisions : Decisions world)
    (store : Store world) (mutability : Mutability) (target : Target Declaration)
    (value : Value Declaration Identity Source) :
    Persists store (declareTarget world decisions store mutability target value).storeAfter := by
  unfold declareTarget
  split
  · exact persists_refl _
  · exact declareAll_persists world decisions store mutability _

theorem assignPath_persists (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (store : Store world) (cell : CellId) (path : List Selector) (op : Xsh.Syntax.AssignOp)
    (right : Value Declaration Identity Source) :
    Persists store (assignPath world decisions raises operate store cell path op right).storeAfter := by
  cases assigned : assignPath world decisions raises operate store cell path op right with
  | committed next =>
    obtain ⟨current, _, present, _, ⟨replaced, replacedAt, replacedType, _, _⟩, others⟩ :=
      Xsh.Proofs.Core.Writes.assignPath_committed_effect world decisions raises operate store next cell path op right
        assigned
    intro other found otherPresent
    by_cases same : other = cell
    · subst same
      rw [present, Option.some.injEq] at otherPresent
      subst otherPresent
      exact ⟨replaced, replacedAt, replacedType⟩
    · exact ⟨found, by simp only [StoreStep.storeAfter]; rw [others other same, otherPresent], rfl⟩
  | _ =>
    have kept := Xsh.Proofs.Core.Writes.assignPath_keeps_store world decisions raises operate store cell path op right
      (by intro next same; rw [assigned] at same; cases same)
    rw [assigned] at kept
    rw [kept]
    exact persists_refl _

theorem bindTarget_persists {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (mutability : Mutability) (target : Xsh.Syntax.BindingTarget) (value : Value Declaration Identity Source) :
    Keeps frame.store (bindTarget machine frame mutability target value) := by
  unfold bindTarget
  split
  · exact self_persists frame _ fun last present => by cases present; rfl
  · exact fromStep_persists frame _ _ _ (declareTarget_persists world machine.decisions frame.store mutability _ value)

theorem bindCaptures_persists {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)) :
    Keeps frame.store (bindCaptures machine frame captures) := by
  unfold bindCaptures
  split
  · exact self_persists frame _ fun last present => by cases present; rfl
  · exact fromStep_persists frame _ _ _ (declareAll_persists world machine.decisions frame.store .immutable _)

theorem bindCaptures_entered {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)) :
    Keeps frame.store (bindCaptures machine frame.enter captures) :=
  bindCaptures_persists machine frame.enter captures

theorem bindTarget_entered {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (mutability : Mutability) (target : Xsh.Syntax.BindingTarget) (value : Value Declaration Identity Source) :
    Keeps frame.store (bindTarget machine frame.enter mutability target value) :=
  bindTarget_persists machine frame.enter mutability target value

/-- The statement functions at `fuel` keep every cell of the frame they start
from. -/
structure Persisting {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat) : Prop where
  statements : ∀ frame statements, Keeps frame.store (runStatements machine fuel frame statements)
  block : ∀ frame statements, Keeps frame.store (runBlock machine fuel frame statements)
  bound : ∀ frame captures statements, Keeps frame.store (runBound machine fuel frame captures statements)
  statement : ∀ frame statement, Keeps frame.store (runStatement machine fuel frame statement)
  branches : ∀ frame branches otherwise, Keeps frame.store (runIf machine fuel frame branches otherwise)
  whiles : ∀ frame condition body, Keeps frame.store (runWhile machine fuel frame condition body)
  fors : ∀ frame target items body, Keeps frame.store (runFor machine fuel frame target items body)
  loops : ∀ frame body, Keeps frame.store (runLoop machine fuel frame body)
  arms : ∀ frame value arms, Keeps frame.store (runMatch machine fuel frame value arms)

set_option hygiene false in
/-- Close a persistence goal for one machine step, given `inner` for the
lower fuel. -/
local macro "close_persists" : tactic => `(tactic| repeat' (first
  | exact inner.statement _ _
  | exact inner.statements _ _
  | exact inner.block _ _
  | exact inner.bound _ _ _
  | exact inner.branches _ _ _
  | exact inner.whiles _ _ _
  | exact inner.fors _ _ _ _
  | exact inner.loops _ _
  | exact inner.arms _ _ _
  | exact bindTarget_persists _ _ _ _ _
  | exact fromStep_persists _ _ _ _ (assignPath_persists _ _ _ _ _ _ _ _ _)
  | exact fun _ present => by cases present <;> exact persists_refl _
  | exact bindCaptures_entered _ _ _
  | exact bindTarget_entered _ _ _ _ _
  | refine fromResult_persists _ _ fun _ => ?_
  | refine andThen_persists ?_ fun _ => ?_
  | refine iterate_persists ?_ fun _ => ?_
  | refine withinScope_persists _ ?_
  | split))

theorem persisting {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel, Persisting machine fuel
  | 0 => {
      statements := fun _ _ => by simp only [runStatements]; exact fun _ present => by cases present
      block := fun _ _ => by simp only [runBlock]; exact fun _ present => by cases present
      bound := fun _ _ _ => by simp only [runBound]; exact fun _ present => by cases present
      statement := fun _ _ => by simp only [runStatement]; exact fun _ present => by cases present
      branches := fun _ _ _ => by simp only [runIf]; exact fun _ present => by cases present
      whiles := fun _ _ _ => by simp only [runWhile]; exact fun _ present => by cases present
      fors := fun _ _ _ _ => by simp only [runFor]; exact fun _ present => by cases present
      loops := fun _ _ => by simp only [runLoop]; exact fun _ present => by cases present
      arms := fun _ _ _ => by simp only [runMatch]; exact fun _ present => by cases present }
  | fuel + 1 => by
    have inner := persisting machine fuel
    exact {
      statements := fun frame statements => by simp only [runStatements]; close_persists
      block := fun frame statements => by simp only [runBlock]; close_persists
      bound := fun frame captures statements => by simp only [runBound]; close_persists
      statement := fun frame statement => by simp only [runStatement]; close_persists
      branches := fun frame branches otherwise => by simp only [runIf]; close_persists
      whiles := fun frame condition body => by simp only [runWhile]; close_persists
      fors := fun frame target items body => by simp only [runFor]; close_persists
      loops := fun frame body => by simp only [runLoop]; close_persists
      arms := fun frame value arms => by simp only [runMatch]; close_persists }

end Persistence

end Xsh.Proofs.Core.Control
