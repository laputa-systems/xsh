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
  expr : ∀ activation store environment locals expression,
    Refines (evalExpr machine fuel activation store environment locals expression)
      (evalExpr machine bigger activation store environment locals expression)
  call : ∀ activation store environment locals body,
    Refines (callBlock machine fuel activation store environment locals body)
      (callBlock machine bigger activation store environment locals body)
  leaves : ∀ outer first second, Exec.Refines first second →
    Exec.Refines (leaveBlock machine fuel outer first) (leaveBlock machine bigger outer second)
  deferreds : ∀ status actions,
    Exec.Refines (runDeferred machine fuel status actions) (runDeferred machine bigger status actions)
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
  | refine fromResult_refines _ (inner.expr _ _ _ _ _) fun _ => ?_
  | refine inner.leaves _ _ _ ?_
  | refine andThen_refines ?_ fun _ => ?_
  | refine iterate_refines ?_ fun _ => ?_
  | refine withinScope_refines _ ?_
  | split))

theorem runDeferred_outOfFuel {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel actions, runDeferred machine fuel .outOfFuel actions = .outOfFuel
  | 0, _ => by simp [runDeferred]
  | fuel + 1, [] => by simp [runDeferred]
  | fuel + 1, entry :: rest => by
    simp only [runDeferred]
    split
    · exact runDeferred_outOfFuel machine fuel rest
    · rfl

theorem refining {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel extra, Refining machine fuel (fuel + extra)
  | 0, _ => {
      expr := fun _ _ _ _ _ => Or.inl (by simp [evalExpr])
      call := fun _ _ _ _ _ => Or.inl (by simp [callBlock])
      leaves := fun _ _ _ _ => Or.inl (by simp [leaveBlock])
      deferreds := fun _ _ => Or.inl (by simp [runDeferred])
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
      expr := fun activation store environment locals expression => by
        simp only [evalExpr]
        exact step_refines machine.context environment store (inner.expr activation store)
          (inner.call (activation + machine.activationStep) store) locals expression
      call := fun activation store environment locals body => by
        simp only [callBlock]
        exact bodyValue_refines (inner.body _ _)
      leaves := fun outer first second refined => by
        simp only [leaveBlock]
        rcases refined with out | same
        · subst out
          show Exec.Refines (withinScope outer (runDeferred machine fuel .outOfFuel [])) _
          rw [runDeferred_outOfFuel]
          exact Or.inl rfl
        · subst same
          exact withinScope_refines outer (inner.deferreds _ _)
      deferreds := fun status actions => by
        simp only [runDeferred]
        split
        · exact exec_refl _
        · split
          · exact inner.deferreds _ _
          · split
            · exact exec_refl _
            · rename_i entry rest _ _ scope _
              rcases inner.block (entry.frame scope) entry.body with out | same
              · rw [out]
                exact Or.inl rfl
              · rw [← same]
                split <;> first | exact inner.deferreds _ _ | exact exec_refl _
      body := fun frame statements => by
        simp only [runBody]
        close_refines
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

theorem withStore_keeps {world : World Declaration Identity Source} {before store : Store world}
    (kept : Persists before store) (status : Exec world) : Keeps before (status.withStore store) := by
  cases status <;> (intro last present; cases present <;> exact kept)

theorem cleanupFailed_keeps {world : World Declaration Identity Source} {before store : Store world}
    (kept : Persists before store) (failure : Failure Declaration Identity Source) (status : Exec world) :
    Keeps before (cleanupFailed failure store status) := by
  cases status <;> (intro last present; cases present <;> exact kept)

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
  leaves : ∀ outer inner, Keeps outer.store inner → Keeps outer.store (leaveBlock machine fuel outer inner)
  deferreds : ∀ before status actions, Keeps before status → Keeps before (runDeferred machine fuel status actions)
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
  | refine inner.leaves _ _ ?_
  | refine andThen_persists ?_ fun _ => ?_
  | refine iterate_persists ?_ fun _ => ?_
  | refine withinScope_persists _ ?_
  | split))

theorem persisting {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel, Persisting machine fuel
  | 0 => {
      leaves := fun _ _ _ => by simp only [leaveBlock]; exact fun _ present => by cases present
      deferreds := fun _ _ _ _ => by simp only [runDeferred]; exact fun _ present => by cases present
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
      leaves := fun outer innerOutcome kept => by
        simp only [leaveBlock]
        exact withinScope_persists outer (inner.deferreds _ _ _ kept)
      deferreds := fun before status actions kept => by
        simp only [runDeferred]
        split
        · exact kept
        · rename_i entry rest
          split
          · exact inner.deferreds _ _ _ kept
          · cases scoping : frameOf status with
            | none => exact kept
            | some scope =>
              dsimp only
              have reached := kept scope scoping
              have ran := inner.block (entry.frame scope) entry.body
              cases result : runBlock machine fuel (entry.frame scope) entry.body with
              | normal after =>
                exact inner.deferreds _ _ _ (withStore_keeps (persists_trans reached (ran after (by rw [result]; rfl))) _)
              | failed failure after =>
                exact inner.deferreds _ _ _
                  (cleanupFailed_keeps (persists_trans reached (ran after (by rw [result]; rfl))) _ _)
              | internal fault after =>
                intro last present
                cases present
                exact persists_trans reached (ran after (by rw [result]; rfl))
              | _ => exact fun _ present => by cases present
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

section Scopes

/-- Every frame an outcome carries has `frame`'s environment and locals. -/
def Restores {world : World Declaration Identity Source} (frame : Frame world) (outcome : Exec world) : Prop :=
  ∀ last, frameOf outcome = some last → last.environment = frame.environment ∧ last.locals = frame.locals

theorem withinScope_restores {world : World Declaration Identity Source} (outer : Frame world) (inner : Exec world) :
    Restores outer (withinScope outer inner) := by
  cases inner <;> (intro last present; cases present <;> exact ⟨rfl, rfl⟩)

theorem iterate_restores {world : World Declaration Identity Source} {frame : Frame world} {first : Exec world}
    {next : Frame world → Exec world} (head : Restores frame first) (tail : ∀ middle, Restores middle (next middle)) :
    Restores frame (first.iterate next) := by
  cases first with
  | normal middle =>
    intro last present
    obtain ⟨environment, locals⟩ := head middle rfl
    obtain ⟨environment', locals'⟩ := tail middle last present
    exact ⟨environment'.trans environment, locals'.trans locals⟩
  | continued middle =>
    intro last present
    obtain ⟨environment, locals⟩ := head middle rfl
    obtain ⟨environment', locals'⟩ := tail middle last present
    exact ⟨environment'.trans environment, locals'.trans locals⟩
  | _ => exact head

theorem fromResult_restores {world : World Declaration Identity Source} (frame : Frame world)
    (result : Result Declaration Identity Source (Value Declaration Identity Source))
    {next : Value Declaration Identity Source → Exec world} (tail : ∀ value, Restores frame (next value)) :
    Restores frame (fromResult frame result next) := by
  cases result with
  | value value => exact tail value
  | _ => intro last present; cases present <;> exact ⟨rfl, rfl⟩

variable [DecidableEq Declaration]

theorem leave_restores {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel outer inner, Restores outer (leaveBlock machine fuel outer inner)
  | 0, _, _ => fun _ present => by simp [leaveBlock, frameOf] at present
  | _ + 1, outer, _ => withinScope_restores outer _

/-- The scoped statement forms at `fuel` leave their frame's environment and
locals in place: a block, a pattern-bound body, `if`, `while`, `for`, `loop`
and `match`. -/
structure Restoring {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat) : Prop where
  block : ∀ frame statements, Restores frame (runBlock machine fuel frame statements)
  bound : ∀ frame captures statements, Restores frame (runBound machine fuel frame captures statements)
  branches : ∀ frame branches otherwise, Restores frame (runIf machine fuel frame branches otherwise)
  whiles : ∀ frame condition body, Restores frame (runWhile machine fuel frame condition body)
  fors : ∀ frame target items body, Restores frame (runFor machine fuel frame target items body)
  loops : ∀ frame body, Restores frame (runLoop machine fuel frame body)
  arms : ∀ frame value arms, Restores frame (runMatch machine fuel frame value arms)

set_option hygiene false in
local macro "close_restores" : tactic => `(tactic| repeat' (first
  | exact inner.block _ _
  | exact inner.bound _ _ _
  | exact inner.branches _ _ _
  | exact inner.whiles _ _ _
  | exact inner.fors _ _ _ _
  | exact inner.loops _ _
  | exact inner.arms _ _ _
  | exact withinScope_restores _ _
  | exact leave_restores _ _ _ _
  | exact fun _ present => by cases present <;> exact ⟨rfl, rfl⟩
  | refine fromResult_restores _ _ fun _ => ?_
  | refine iterate_restores (withinScope_restores _ _) fun _ => ?_
  | refine iterate_restores (leave_restores _ _ _ _) fun _ => ?_
  | refine iterate_restores ?_ fun _ => ?_
  | split))

theorem restoring {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel, Restoring machine fuel
  | 0 => {
      block := fun _ _ => by simp only [runBlock]; exact fun _ present => by cases present
      bound := fun _ _ _ => by simp only [runBound]; exact fun _ present => by cases present
      branches := fun _ _ _ => by simp only [runIf]; exact fun _ present => by cases present
      whiles := fun _ _ _ => by simp only [runWhile]; exact fun _ present => by cases present
      fors := fun _ _ _ _ => by simp only [runFor]; exact fun _ present => by cases present
      loops := fun _ _ => by simp only [runLoop]; exact fun _ present => by cases present
      arms := fun _ _ _ => by simp only [runMatch]; exact fun _ present => by cases present }
  | fuel + 1 => by
    have inner := restoring machine fuel
    exact {
      block := fun frame statements => by simp only [runBlock]; close_restores
      bound := fun frame captures statements => by simp only [runBound]; close_restores
      branches := fun frame branches otherwise => by simp only [runIf]; close_restores
      whiles := fun frame condition body => by simp only [runWhile]; close_restores
      fors := fun frame target items body => by simp only [runFor]; close_restores
      loops := fun frame body => by simp only [runLoop]; close_restores
      arms := fun frame value arms => by simp only [runMatch]; close_restores }

end Scopes

section Preservation

/-- A typing context the frame satisfies: a local holds a member of its
type, and a name read through the environment resolves to a cell whose
members are members of the name's type. -/
def StoreConsistent (world : World Declaration Identity Source) (environment : Environment) (store : Store world)
    (types : Xsh.Typing.Core.Expressions.TypeContext Declaration) (locals : Locals Declaration Identity Source) :
    Prop :=
  (∀ name type, types name = some type →
    match locals.lookup name with
    | some value => Membership.value world.membership type value
    | none => ∃ cell found, environment.cellOf name = .ok cell ∧ store cell = some found ∧
        ∀ candidate, HasType world found.contents.typeTag candidate → Membership.value world.membership type candidate) ∧
  ∀ name value, locals.lookup name = some value → types name ≠ none

theorem storeConsistent_consistent (world : World Declaration Identity Source) (environment : Environment)
    (store : Store world) (types : Xsh.Typing.Core.Expressions.TypeContext Declaration)
    (locals : Locals Declaration Identity Source)
    (consistent : StoreConsistent world environment store types locals) :
    Xsh.Proofs.Core.EvalTyping.Consistent world environment store types locals := by
  refine ⟨fun name type typed => ?_, consistent.2⟩
  have promised := consistent.1 name type typed
  cases found : locals.lookup name with
  | some value =>
    simp only [found] at promised ⊢
    exact promised
  | none =>
    simp only [found] at promised ⊢
    obtain ⟨cell, held, resolved, present, members⟩ := promised
    exact ⟨cell, held, resolved, present, members _ held.contents.wellTyped⟩

theorem storeConsistent_persists (world : World Declaration Identity Source) (environment : Environment)
    {before after : Store world} (types : Xsh.Typing.Core.Expressions.TypeContext Declaration)
    (locals : Locals Declaration Identity Source) (kept : Persists before after)
    (consistent : StoreConsistent world environment before types locals) :
    StoreConsistent world environment after types locals := by
  refine ⟨fun name type typed => ?_, consistent.2⟩
  have promised := consistent.1 name type typed
  cases found : locals.lookup name with
  | some value =>
    simp only [found] at promised ⊢
    exact promised
  | none =>
    simp only [found] at promised ⊢
    obtain ⟨cell, held, resolved, present, members⟩ := promised
    obtain ⟨now, nowPresent, sameType⟩ := kept cell held present
    exact ⟨cell, now, resolved, nowPresent, by rw [sameType]; exact members⟩

/-- An outcome that keeps the frame's cells and restores its scope keeps it
consistent with any typing context it satisfied. -/
theorem scoped_preserves (world : World Declaration Identity Source) {frame : Frame world} {outcome : Exec world}
    (types : Xsh.Typing.Core.Expressions.TypeContext Declaration) (kept : Keeps frame.store outcome)
    (restored : Restores frame outcome)
    (consistent : StoreConsistent world frame.environment frame.store types frame.locals) :
    ∀ last, frameOf outcome = some last → StoreConsistent world last.environment last.store types last.locals ∧
      Xsh.Proofs.Core.EvalTyping.Consistent world last.environment last.store types last.locals := by
  intro last present
  obtain ⟨environment, locals⟩ := restored last present
  rw [environment, locals]
  have after := storeConsistent_persists world frame.environment types frame.locals (kept last present) consistent
  exact ⟨after, storeConsistent_consistent world _ _ types _ after⟩

end Preservation

section Laws

variable [DecidableEq Declaration]

theorem while_false {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (body : List Xsh.Syntax.Statement) :
    runWhile machine (fuel + 2) frame (.literal (.bool false)) body = .normal frame := by
  simp [runWhile, evalExpr, step, literal, fromResult, boolean, Value.eraseNominal]

theorem loop_ends_on_break {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame next : Frame world) (body : List Xsh.Syntax.Statement)
    (broke : runBlock machine fuel frame body = .broke next) :
    runLoop machine (fuel + 1) frame body = .normal next := by
  simp [runLoop, broke, Exec.iterate]

theorem for_ends_on_break {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame next : Frame world) (target : Xsh.Syntax.BindingTarget) (item : Value Declaration Identity Source)
    (rest : List (Value Declaration Identity Source)) (body : List Xsh.Syntax.Statement)
    (broke : leaveBlock machine fuel frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
      runStatements machine fuel bound body) = .broke next) :
    runFor machine (fuel + 1) frame target (item :: rest) body = .normal next := by
  simp only [runFor, broke, Exec.iterate]

theorem while_ends_on_break {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame next : Frame world) (condition : Xsh.Syntax.Expr) (body : List Xsh.Syntax.Statement)
    (plain : ∀ subject pattern, condition ≠ .patternCondition subject pattern)
    (holds : ∀ tested, evalExpr machine fuel frame.activation frame.store frame.environment frame.locals condition = .value tested →
      tested.eraseNominal = .scalar (.boolean true))
    (tested : Value Declaration Identity Source)
    (evaluated : evalExpr machine fuel frame.activation frame.store frame.environment frame.locals condition = .value tested)
    (broke : runBlock machine fuel frame body = .broke next) :
    runWhile machine (fuel + 1) frame condition body = .normal next := by
  have isTrue := holds tested evaluated
  cases condition
  case patternCondition subject pattern => exact absurd rfl (plain subject pattern)
  all_goals simp only [runWhile, evaluated, fromResult, isTrue, broke, Exec.iterate]

/-- A block with no deferred actions left passes a `break` out to the loop
around it, restoring its scope. -/
theorem block_passes_break {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame inner : Frame world) (body : List Xsh.Syntax.Statement)
    (broke : runStatements machine (fuel + 2) frame.enter body = .broke inner) (settled : inner.deferred = []) :
    runBlock machine (fuel + 3) frame body = .broke (frame.restore inner) := by
  simp [runBlock, leaveBlock, broke, pendingOf, settled, runDeferred, withinScope]

theorem return_skips_rest {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame last : Frame world) (statement : Xsh.Syntax.Statement) (rest : List Xsh.Syntax.Statement)
    (value : Value Declaration Identity Source)
    (returned : runStatement machine fuel frame statement = .returned value last) :
    runStatements machine (fuel + 1) frame (statement :: rest) = .returned value last := by
  simp [runStatements, returned, Exec.andThen]

theorem for_passes_return {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame last : Frame world) (target : Xsh.Syntax.BindingTarget) (item : Value Declaration Identity Source)
    (rest : List (Value Declaration Identity Source)) (body : List Xsh.Syntax.Statement)
    (value : Value Declaration Identity Source)
    (returned : leaveBlock machine fuel frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
      runStatements machine fuel bound body) = .returned value last) :
    runFor machine (fuel + 1) frame target (item :: rest) body = .returned value last := by
  simp only [runFor, returned, Exec.iterate]

end Laws

section Bindings

theorem resolveCaptures_aligned {world : World Declaration Identity Source} (resolution : Oracles world)
    (environment : Environment) :
    ∀ (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)) entries,
      resolveCaptures resolution environment captures = some entries →
        entries.map (·.1.1) = captures.map Prod.fst ∧ entries.map (·.2.2.2) = captures.map Prod.snd ∧
          ∀ entry ∈ entries, entry.2.1 = entry.1.2.2
  | [], entries, resolved => by
    simp only [resolveCaptures, Option.some.injEq] at resolved
    subst resolved
    simp
  | (captured, value) :: rest, entries, resolved => by
    simp only [resolveCaptures] at resolved
    split at resolved
    · rename_i binder cell type others declared remaining
      simp only [Option.some.injEq] at resolved
      subst resolved
      obtain ⟨names, values, cells⟩ := resolveCaptures_aligned resolution environment rest others remaining
      refine ⟨by simp [names], by simp [values], ?_⟩
      intro entry inside
      simp only [List.mem_cons] at inside
      rcases inside with same | later
      · subst same
        rfl
      · exact cells entry later
    · simp at resolved

variable [DecidableEq Declaration]

/-- Pattern captures (`if let`, `while let`, `match` arms) are bound as
immutable cells holding the captured values; a captured name hides a local of
that name and, when the checker's names and binders are distinct, resolves to
its cell. -/
theorem bindCaptures_binds {world : World Declaration Identity Source} (machine : Machine world) (frame next : Frame world)
    (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source))
    (bound : bindCaptures machine frame captures = .normal next) :
    ∃ entries, resolveCaptures (machine.namesAt frame.activation) frame.environment captures = some entries ∧
      entries.map (·.1.1) = captures.map Prod.fst ∧ entries.map (·.2.2.2) = captures.map Prod.snd ∧
      next.environment = extend frame.environment (entries.map Prod.fst) ∧
      next.locals = hide (captures.map Prod.fst) frame.locals ∧
      (∀ entry ∈ entries, ∃ held, next.store entry.1.2.2 = some held ∧ held.contents.value = entry.2.2.2 ∧
        held.mutability = .immutable) ∧
      (((entries.map Prod.fst).map Prod.fst).Nodup → ((entries.map Prod.fst).map (·.2.1)).Nodup →
        ∀ entry ∈ entries, next.environment.cellOf entry.1.1 = .ok entry.1.2.2) := by
  unfold bindCaptures at bound
  split at bound
  · cases bound
  · rename_i entries resolved
    obtain ⟨names, values, cells⟩ := resolveCaptures_aligned (machine.namesAt frame.activation) frame.environment captures entries resolved
    cases declared : declareAll world machine.decisions frame.store .immutable frame.store (entries.map Prod.snd) with
    | committed store =>
      rw [declared] at bound
      simp only [fromStep, Exec.normal.injEq] at bound
      subst bound
      obtain ⟨_, _, held, _⟩ := Xsh.Proofs.Core.Writes.declareAll_committed world machine.decisions frame.store
        .immutable (entries.map Prod.snd) frame.store store declared
      refine ⟨entries, resolved, names, values, rfl, rfl, ?_, ?_⟩
      · intro entry inside
        obtain ⟨found, present, mutability, _, value⟩ := held entry.2 (List.mem_map_of_mem inside)
        rw [cells entry inside] at present
        exact ⟨found, present, value, mutability⟩
      · intro distinctNames distinctBinders entry inside
        exact Xsh.Proofs.Core.Statements.declared_names_resolve frame.environment (entries.map Prod.fst)
          distinctNames distinctBinders entry.1.1 entry.1.2.1 entry.1.2.2 (List.mem_map_of_mem inside)
    | failed failure store => rw [declared] at bound; cases bound
    | internal fault store => rw [declared] at bound; cases bound

/-- A `for` variable or `let` target is bound through the checker's resolution
and `declareTarget`; with distinct names and binders each declared name
resolves to its cell. -/
theorem bindTarget_binds {world : World Declaration Identity Source} (machine : Machine world) (frame next : Frame world)
    (mutability : Mutability) (target : Xsh.Syntax.BindingTarget) (value : Value Declaration Identity Source)
    (bound : bindTarget machine frame mutability target value = .normal next) :
    ∃ resolved declared, resolveTarget (machine.namesAt frame.activation) frame.environment target = some (resolved, declared) ∧
      declareTarget world machine.decisions frame.store mutability resolved value = .committed next.store ∧
      next.environment = extend frame.environment declared ∧
      next.locals = hide (declared.map Prod.fst) frame.locals ∧
      ((declared.map Prod.fst).Nodup → (declared.map (·.2.1)).Nodup →
        ∀ name binder cell, (name, binder, cell) ∈ declared → next.environment.cellOf name = .ok cell) := by
  unfold bindTarget at bound
  split at bound
  · cases bound
  · rename_i resolved declared found
    cases committed : declareTarget world machine.decisions frame.store mutability resolved value with
    | committed store =>
      rw [committed] at bound
      simp only [fromStep, Exec.normal.injEq] at bound
      subst bound
      exact ⟨resolved, declared, found, committed, rfl, rfl,
        Xsh.Proofs.Core.Statements.declared_names_resolve frame.environment declared⟩
    | failed failure store => rw [committed] at bound; cases bound
    | internal fault store => rw [committed] at bound; cases bound

end Bindings

section Root

/-- The statements that open their own scope around everything they bind:
`if`, `while`, `for`, `loop`, `match` and a bare block. -/
def IsControl : Xsh.Syntax.Statement → Prop
  | .ifStmt _ _ | .whileStmt _ _ | .forStmt _ _ _ | .loopStmt _ | .matchStmt _ _ | .expression (.valueBlock _) => True
  | _ => False

variable [DecidableEq Declaration]

theorem control_restores {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel frame statement, IsControl statement → Restores frame (runStatement machine fuel frame statement)
  | 0, _, _, _ => fun _ present => by simp [runStatement, frameOf] at present
  | fuel + 1, frame, statement, control => by
    have scopes := restoring machine fuel
    cases statement with
    | ifStmt branches otherwise => exact scopes.branches frame branches otherwise
    | whileStmt condition body => exact scopes.whiles frame condition body
    | loopStmt body => exact scopes.loops frame body
    | forStmt target iterable body =>
      simp only [runStatement]
      refine fromResult_restores frame _ fun source => ?_
      split
      · exact scopes.fors frame target _ body
      · exact fun _ present => by cases present
    | matchStmt subject arms =>
      simp only [runStatement]
      exact fromResult_restores frame _ fun value => scopes.arms frame value arms
    | expression value =>
      cases value
      case valueBlock body => exact scopes.block frame body
      all_goals simp [IsControl] at control
    | _ => simp [IsControl] at control

omit [DecidableEq Declaration] in
theorem finished_agree {world : World Declaration Identity Source} {run : Nat → Exec world}
    (monotone : ∀ fuel extra, Exec.Refines (run fuel) (run (fuel + extra))) (first second : Nat)
    (firstFinished : run first ≠ .outOfFuel) (secondFinished : run second ≠ .outOfFuel) : run first = run second := by
  rcases Nat.le_total first second with smaller | larger
  · obtain ⟨extra, rfl⟩ := Nat.exists_eq_add_of_le smaller
    rcases monotone first extra with out | same
    · exact absurd out firstFinished
    · exact same
  · obtain ⟨extra, rfl⟩ := Nat.exists_eq_add_of_le larger
    rcases monotone second extra with out | same
    · exact absurd out secondFinished
    · exact same.symm

/-- BASE-STEP's statement control for the core fragment, over every machine:
a finished block or statement list gives the same outcome at every fuel;
every outcome keeps the cells it started with at their types; a block and the
control statements (`if`, `while`, `for`, `loop`, `match`, a bare block)
leave the environment and locals as they found them, and so keep a frame
consistent with any typing context it satisfied; `while false` completes with
its frame unchanged; a loop ends normally on `break` while a block passes the
`break` on; `return` skips the rest of a block and leaves a `for` loop; and
pattern captures and `for` targets are bound as cells that resolve inside the
bound body, whose scope is left on exit. -/
theorem statement_control {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ frame body first second, runBlock machine first frame body ≠ .outOfFuel →
      runBlock machine second frame body ≠ .outOfFuel →
        runBlock machine first frame body = runBlock machine second frame body) ∧
    (∀ frame body first second, runStatements machine first frame body ≠ .outOfFuel →
      runStatements machine second frame body ≠ .outOfFuel →
        runStatements machine first frame body = runStatements machine second frame body) ∧
    (∀ fuel frame statement, Keeps frame.store (runStatement machine fuel frame statement)) ∧
    (∀ fuel frame body, Restores frame (runBlock machine fuel frame body)) ∧
    (∀ fuel frame statement, IsControl statement → Restores frame (runStatement machine fuel frame statement)) ∧
    (∀ fuel frame statement types, IsControl statement →
      StoreConsistent world frame.environment frame.store types frame.locals →
        ∀ last, frameOf (runStatement machine fuel frame statement) = some last →
          StoreConsistent world last.environment last.store types last.locals ∧
            Xsh.Proofs.Core.EvalTyping.Consistent world last.environment last.store types last.locals) ∧
    (∀ fuel frame body, runWhile machine (fuel + 2) frame (.literal (.bool false)) body = .normal frame) ∧
    (∀ fuel frame next body, runBlock machine fuel frame body = .broke next →
      runLoop machine (fuel + 1) frame body = .normal next) ∧
    (∀ fuel frame next target item rest body,
      leaveBlock machine fuel frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
        runStatements machine fuel bound body) = .broke next →
          runFor machine (fuel + 1) frame target (item :: rest) body = .normal next) ∧
    (∀ fuel frame inner body, runStatements machine (fuel + 2) frame.enter body = .broke inner →
      inner.deferred = [] → runBlock machine (fuel + 3) frame body = .broke (frame.restore inner)) ∧
    (∀ fuel frame last statement rest value, runStatement machine fuel frame statement = .returned value last →
      runStatements machine (fuel + 1) frame (statement :: rest) = .returned value last) ∧
    (∀ fuel frame last target item rest body value,
      leaveBlock machine fuel frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
        runStatements machine fuel bound body) = .returned value last →
          runFor machine (fuel + 1) frame target (item :: rest) body = .returned value last) ∧
    (∀ frame next captures, bindCaptures machine frame captures = .normal next →
      ∃ entries, resolveCaptures (machine.namesAt frame.activation) frame.environment captures = some entries ∧
        entries.map (·.1.1) = captures.map Prod.fst ∧ entries.map (·.2.2.2) = captures.map Prod.snd ∧
        next.environment = extend frame.environment (entries.map Prod.fst) ∧
        next.locals = hide (captures.map Prod.fst) frame.locals ∧
        (∀ entry ∈ entries, ∃ held, next.store entry.1.2.2 = some held ∧ held.contents.value = entry.2.2.2 ∧
          held.mutability = .immutable) ∧
        (((entries.map Prod.fst).map Prod.fst).Nodup → ((entries.map Prod.fst).map (·.2.1)).Nodup →
          ∀ entry ∈ entries, next.environment.cellOf entry.1.1 = .ok entry.1.2.2)) ∧
    (∀ frame next mutability target value, bindTarget machine frame mutability target value = .normal next →
      ∃ resolved declared, resolveTarget (machine.namesAt frame.activation) frame.environment target = some (resolved, declared) ∧
        declareTarget world machine.decisions frame.store mutability resolved value = .committed next.store ∧
        next.environment = extend frame.environment declared ∧
        next.locals = hide (declared.map Prod.fst) frame.locals ∧
        ((declared.map Prod.fst).Nodup → (declared.map (·.2.1)).Nodup →
          ∀ name binder cell, (name, binder, cell) ∈ declared → next.environment.cellOf name = .ok cell)) ∧
    (∀ fuel frame captures body, Restores frame (runBound machine fuel frame captures body)) ∧
    (∀ fuel frame target items body, Restores frame (runFor machine fuel frame target items body)) :=
  ⟨fun frame body => finished_agree fun fuel extra => (refining machine fuel extra).block frame body,
    fun frame body => finished_agree fun fuel extra => (refining machine fuel extra).statements frame body,
    fun fuel => (persisting machine fuel).statement,
    fun fuel => (restoring machine fuel).block,
    control_restores machine,
    fun fuel frame statement types control consistent =>
      scoped_preserves world types ((persisting machine fuel).statement frame statement)
        (control_restores machine fuel frame statement control) consistent,
    while_false machine,
    loop_ends_on_break machine,
    for_ends_on_break machine,
    block_passes_break machine,
    return_skips_rest machine,
    for_passes_return machine,
    bindCaptures_binds machine,
    bindTarget_binds machine,
    fun fuel => (restoring machine fuel).bound,
    fun fuel => (restoring machine fuel).fors⟩

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness

def name (index : Nat) (bounded : index < 4294967296) : Xsh.Syntax.RawName := ⟨⟨index, bounded⟩⟩

def xName : Xsh.Syntax.RawName := name 30 (by decide)
def iName : Xsh.Syntax.RawName := name 31 (by decide)
def jName : Xsh.Syntax.RawName := name 32 (by decide)
def totalName : Xsh.Syntax.RawName := name 33 (by decide)
def seenName : Xsh.Syntax.RawName := name 34 (by decide)
def gName : Xsh.Syntax.RawName := name 35 (by decide)
def aName : Xsh.Syntax.RawName := name 36 (by decide)
def bName : Xsh.Syntax.RawName := name 37 (by decide)

def int (lexeme : String) : Xsh.Syntax.Expr := .literal (.integer lexeme)
def ident (target : Xsh.Syntax.RawName) : Xsh.Syntax.Expr := .ident target

def program : Context Unit Unit Unit :=
  { Xsh.Proofs.Core.EvalStatements.Witness.context with
    functions := fun called =>
      if called = gName then
        some (⟨emptyLexicalEnvironment, []⟩, [],
          .block [.forStmt (.name iName) (.list [int "1", int "2"]) [.returnValue (some (ident iName))], .expression (int "7")])
      else none }

/-- The checker's resolution in these programs: one binder per name and scope
depth, all at type `Any`. -/
def resolution : Oracles witnessWorld where
  spelling := fun _ => ""
  evaluate := fun _ store _ => .error (.runtime (.scalar .null), store)
  selector := fun _ store _ _ => .error (.runtime (.scalar .null), store)
  declaration := fun environment declared =>
    let binder : BinderId := ⟨⟨(declared.word.val * 64 + environment.names.length) % 4294967296, Nat.mod_lt _ (by decide)⟩⟩
    some (binder, ⟨0, binder⟩, .primitive .any)
  argv := fun _ => none

def machine : Machine witnessWorld where
  context := program
  decisions := witnessDecisions
  raises := Xsh.Proofs.Core.Writes.Witness.raises
  operate := operate program Xsh.Proofs.Core.Writes.Witness.pair
  resolution := resolution
  assertion := fun message => message.getD (.scalar (.string "assertion"))
  activationStep := 1

def start : Frame witnessWorld := ⟨⟨emptyLexicalEnvironment, []⟩, fun _ => none, [], [], none, 0, [], []⟩

def run (body : List Xsh.Syntax.Statement) : Exec witnessWorld := runStatements machine 40 start body

def readInt (target : Xsh.Syntax.RawName) : Exec witnessWorld → Option Int
  | .normal frame =>
    match frame.environment.cellOf target with
    | .ok cell =>
      (frame.store cell).bind fun found =>
        match found.contents.value with
        | .scalar (.integer value) => some value.val
        | _ => none
    | .error _ => none
  | _ => none

def resolves (target : Xsh.Syntax.RawName) : Exec witnessWorld → Bool
  | .normal frame =>
    match frame.environment.cellOf target with
    | .ok _ => true
    | .error _ => false
  | _ => false

def returnedInt : Exec witnessWorld → Option Int
  | .returned (.scalar (.integer value)) _ => some value.val
  | _ => none

def letX (lexeme : String) : Xsh.Syntax.Statement := .binding .letBinding (.name xName) none (int lexeme)

def shadowed : List Xsh.Syntax.Statement := [letX "1", .expression (.valueBlock [letX "2"])]

def varOf (target : Xsh.Syntax.RawName) (lexeme : String) : Xsh.Syntax.Statement :=
  .binding .var (.name target) none (int lexeme)

def bump (target : Xsh.Syntax.RawName) (by_ : Xsh.Syntax.Expr) : Xsh.Syntax.Statement :=
  .assign (.name target) .add by_

def items (lexemes : List String) : Xsh.Syntax.Expr := .list (lexemes.map int)

def loopOnly : List Xsh.Syntax.Statement := [.forStmt (.name iName) (items ["1", "2"]) []]

def skipping : List Xsh.Syntax.Statement := [
  varOf totalName "0", varOf seenName "0",
  .forStmt (.name iName) (items ["1", "2", "7"]) [
    bump seenName (int "1"),
    .ifStmt [(.binary .eq (ident iName) (int "2"), [.continueStmt])] none,
    bump totalName (ident iName)]]

def nested : List Xsh.Syntax.Statement := [
  varOf totalName "0",
  .forStmt (.name iName) (items ["1", "2"]) [
    .forStmt (.name jName) (items ["7", "8"]) [.breakStmt none],
    bump totalName (int "1")]]

def returning : List Xsh.Syntax.Statement := [
  .forStmt (.name iName) (items ["1", "2"]) [.returnValue (some (ident iName))],
  .returnValue (some (int "7"))]

def matching : List Xsh.Syntax.Statement := [
  varOf totalName "0",
  .matchStmt (items ["7", "8"]) [
    (.list [.literal (.integer "1"), .binding aName] none, none, [bump totalName (int "1")]),
    (.list [.binding aName, .binding bName] none, none, [bump totalName (.binary .add (ident aName) (ident bName))])]]

def ifLet : List Xsh.Syntax.Statement := [
  varOf totalName "0",
  .ifStmt [(.patternCondition (.literal .null) (.binding aName), [bump totalName (int "1")]),
    (.patternCondition (int "7") (.binding aName), [bump totalName (ident aName)])] none]

def calling : List Xsh.Syntax.Statement := [varOf totalName "0", bump totalName (.call (ident gName) [])]

/-- A `for` that leaves each iteration's bindings in place. -/
def leakyFor (frame : Frame witnessWorld) (target : Xsh.Syntax.BindingTarget) :
    List (Value Unit Unit Unit) → List Xsh.Syntax.Statement → Exec witnessWorld
  | [], _ => .normal frame
  | item :: rest, body =>
    ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
        runStatements machine 30 bound body).iterate fun next => leakyFor next target rest body

/-- A `for` whose `break` also leaves the loop around it. -/
def breakingFor (frame : Frame witnessWorld) (target : Xsh.Syntax.BindingTarget) :
    List (Value Unit Unit Unit) → List Xsh.Syntax.Statement → Exec witnessWorld
  | [], _ => .normal frame
  | item :: rest, body =>
    match withinScope frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
        runStatements machine 30 bound body) with
    | .normal next | .continued next => breakingFor next target rest body
    | other => other

/-- A `for` whose `return` only ends the loop. -/
def breakingReturn (frame : Frame witnessWorld) (target : Xsh.Syntax.BindingTarget) :
    List (Value Unit Unit Unit) → List Xsh.Syntax.Statement → Exec witnessWorld
  | [], _ => .normal frame
  | item :: rest, body =>
    match withinScope frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
        runStatements machine 30 bound body) with
    | .normal next | .continued next => breakingReturn next target rest body
    | .broke next | .returned _ next => .normal next
    | other => other

def values (lexemes : List String) : List (Value Unit Unit Unit) :=
  lexemes.filterMap fun lexeme => (Xsh.Proofs.Core.EvalStatements.Witness.integerLexeme lexeme).map integer

def afterVar (target : Xsh.Syntax.RawName) (lexeme : String) : Frame witnessWorld :=
  match runStatements machine 10 start [varOf target lexeme] with
  | .normal frame => frame
  | _ => start

def nestedMutant : Exec witnessWorld :=
  let outer := afterVar totalName "0"
  let rec iterations : List (Value Unit Unit Unit) → Frame witnessWorld → Exec witnessWorld
    | [], frame => .normal frame
    | item :: rest, frame =>
      (withinScope frame ((bindTarget machine frame.enter .immutable (.name iName) item).andThen fun bound =>
          (breakingFor bound (.name jName) (values ["7", "8"]) [.breakStmt none]).andThen fun inner =>
            runStatements machine 30 inner [bump totalName (int "1")])).iterate fun next => iterations rest next
  iterations (values ["1", "2"]) outer

def returnMutant : Exec witnessWorld :=
  (breakingReturn start (.name iName) (values ["1", "2"]) [.returnValue (some (ident iName))]).andThen fun next =>
    runStatements machine 30 next [.returnValue (some (int "7"))]

/-- `let x = 1; { let x = 2 }` reads `x` as `1` after the block. -/
theorem shadow_restores : readInt xName (run shadowed) = some 1 := by decide +kernel
/-- A `for` variable does not resolve after its loop. -/
theorem loop_variable_scoped : resolves iName (run loopOnly) = false := by decide +kernel
/-- The mutant `for` that keeps its bindings leaves the variable resolvable. -/
theorem leaky_loop_rejected : resolves iName (leakyFor start (.name iName) (values ["1", "2"]) []) = true := by decide +kernel
/-- `continue` skips the rest of the body (`total` misses `2`) but the loop still
advances (`seen` counts three iterations). -/
theorem continue_advances : readInt totalName (run skipping) = some 8 ∧ readInt seenName (run skipping) = some 3 := by decide +kernel
/-- A `break` in the inner loop ends only that loop: the outer body runs twice. -/
theorem break_innermost : readInt totalName (run nested) = some 2 := by decide +kernel
/-- The mutant whose inner `break` also ends the outer loop never counts. -/
theorem double_break_rejected : readInt totalName nestedMutant = some 0 := by decide +kernel
/-- `return` inside a `for` leaves the statement list with the first item. -/
theorem return_from_loop : returnedInt (run returning) = some 1 := by decide +kernel
/-- The mutant whose `return` only ends the loop reaches `return 7`. -/
theorem return_as_break_rejected : returnedInt returnMutant = some 7 := by decide +kernel
/-- `match [7, 8]` skips the arm `[1, a]` and binds `[a, b]`. -/
theorem match_list_pattern : readInt totalName (run matching) = some 15 := by decide +kernel
/-- `if let a = null` takes the next branch; `if let a = 7` binds `a`. -/
theorem if_let_optional : readInt totalName (run ifLet) = some 7 := by decide +kernel
/-- A block function body returns from inside its `for` loop. -/
theorem block_body_return : readInt totalName (run calling) = some 1 := by decide +kernel

end Witness

end Xsh.Proofs.Core.Control
