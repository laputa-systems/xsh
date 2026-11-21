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

theorem boundary_refines {world : World Declaration Identity Source} (kind : ReturnKind) {first second : Exec world}
    (head : Exec.Refines first second) : Refines (boundary kind first) (boundary kind second) := by
  rcases head with out | same
  · subst out
    cases kind <;> exact Or.inl rfl
  · subst same
    exact Or.inr rfl

def Valued.Refines {world : World Declaration Identity Source} (first second : Valued world) : Prop :=
  first = .exec .outOfFuel ∨ first = second

theorem valued_refl {world : World Declaration Identity Source} (outcome : Valued world) :
    Valued.Refines outcome outcome := Or.inr rfl

theorem valued_bind_refines {world : World Declaration Identity Source} {first second : Valued world}
    {next next' : Value Declaration Identity Source → Frame world → Valued world} (head : Valued.Refines first second)
    (tail : ∀ value frame, Valued.Refines (next value frame) (next' value frame)) :
    Valued.Refines (first.bind next) (second.bind next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | value value frame => exact tail value frame
    | exec _ => exact Or.inr rfl

theorem toExec_refines {world : World Declaration Identity Source} {first second : Valued world}
    {next next' : Value Declaration Identity Source → Frame world → Exec world} (head : Valued.Refines first second)
    (tail : ∀ value frame, Exec.Refines (next value frame) (next' value frame)) :
    Exec.Refines (first.toExec next) (second.toExec next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | value value frame => exact tail value frame
    | exec _ => exact Or.inr rfl

theorem thenValued_refines {world : World Declaration Identity Source} {first second : Exec world}
    {next next' : Frame world → Valued world} (head : Exec.Refines first second)
    (tail : ∀ frame, Valued.Refines (next frame) (next' frame)) :
    Valued.Refines (first.thenValued next) (second.thenValued next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | normal frame => exact tail frame
    | _ => exact Or.inr rfl

theorem resultThen_refines {world : World Declaration Identity Source} {α : Type} (frame : Frame world)
    {first second : Result Declaration Identity Source α} {next next' : α → Valued world}
    (head : Refines first second) (tail : ∀ result, Valued.Refines (next result) (next' result)) :
    Valued.Refines (resultThen frame first next) (resultThen frame second next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | value result => exact tail result
    | _ => exact Or.inr rfl

theorem fromEval_refines {world : World Declaration Identity Source} (frame : Frame world)
    {first second : Result Declaration Identity Source (Value Declaration Identity Source)}
    (head : Refines first second) : Valued.Refines (fromEval frame first) (fromEval frame second) :=
  resultThen_refines frame head fun _ => valued_refl _

theorem settle_refines {world : World Declaration Identity Source} {leave leave' : Exec world → Exec world}
    (leaving : ∀ first second, Exec.Refines first second → Exec.Refines (leave first) (leave' second))
    (leaveOut : leave .outOfFuel = .outOfFuel) {first second : Valued world} (head : Valued.Refines first second) :
    Valued.Refines (settle leave first) (settle leave' second) := by
  rcases head with out | same
  · subst out
    simp only [settle, leaveOut]
    exact Or.inl rfl
  · subst same
    cases first with
    | value result inner =>
      simp only [settle]
      rcases leaving (.normal inner) (.normal inner) (exec_refl _) with out | same
      · rw [out]
        exact Or.inl rfl
      · rw [← same]
        exact valued_refl _
    | exec other =>
      simp only [settle]
      rcases leaving other other (exec_refl _) with out | same
      · rw [out]
        exact Or.inl rfl
      · rw [← same]
        exact valued_refl _

theorem capture_refines {world : World Declaration Identity Source} {first second : Valued world}
    (head : Valued.Refines first second) : Valued.Refines (capture first) (capture second) := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    exact valued_refl _

theorem retryStep_refines {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool)
    (pattern : Option Xsh.Syntax.Pattern) (delays : List (Value Declaration Identity Source))
    {next next' : Frame world → List (Value Declaration Identity Source) → Valued world}
    (tail : ∀ frame rest, Valued.Refines (next frame rest) (next' frame rest)) {first second : Valued world}
    (head : Valued.Refines first second) :
    Valued.Refines (retryStep context test pattern delays next first) (retryStep context test pattern delays next' second) := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    unfold retryStep
    split
    · exact valued_refl _
    · split
      · split
        · exact tail _ _
        · exact valued_refl _
      · exact valued_refl _
      · exact valued_refl _
    · exact valued_refl _

theorem loopStep_refines {world : World Declaration Identity Source} {next next' : Frame world → Valued world}
    (tail : ∀ frame, Valued.Refines (next frame) (next' frame)) {first second : Exec world}
    (head : Exec.Refines first second) : Valued.Refines (loopStep next first) (loopStep next' second) := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | normal frame => exact tail frame
    | continued frame => exact tail frame
    | _ => exact valued_refl _

theorem resultExec_refines {world : World Declaration Identity Source} {α : Type} (frame : Frame world)
    {first second : Result Declaration Identity Source α} {next next' : α → Exec world}
    (head : Refines first second) (tail : ∀ result, Exec.Refines (next result) (next' result)) :
    Exec.Refines (resultExec frame first next) (resultExec frame second next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | value result => exact tail result
    | _ => exact Or.inr rfl

theorem selectorsOf_refines {world : World Declaration Identity Source}
    {evaluate evaluate' : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, Refines (evaluate source) (evaluate' source)) :
    ∀ type path, Refines (selectorsOf (world := world) evaluate type path) (selectorsOf evaluate' type path)
  | _, [] => refines_refl _
  | type, .field label :: rest => bind_refines (selectorsOf_refines each _ rest) fun _ => refines_refl _
  | type, .index selector :: rest => by
    simp only [selectorsOf]
    refine bind_refines (each selector) fun index => ?_
    split
    · exact refines_refl _
    · exact bind_refines (selectorsOf_refines each _ rest) fun _ => refines_refl _

theorem guardElse_refines {world : World Declaration Identity Source} {first second : Exec world}
    (head : Exec.Refines first second) : Exec.Refines (guardElse first) (guardElse second) := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    exact exec_refl _

theorem collectWith_refines {world : World Declaration Identity Source} (outer : Frame world)
    {first second : Exec world} (head : Exec.Refines first second) :
    Valued.Refines (collectWith outer first) (collectWith outer second) := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    exact valued_refl _

theorem returnTo_refines {world : World Declaration Identity Source} (caller : Frame world) (kind : ReturnKind)
    {first second : Exec world} (head : Exec.Refines first second) :
    Valued.Refines (returnTo caller kind first) (returnTo caller kind second) := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    exact valued_refl _

theorem valuedResult_refines {world : World Declaration Identity Source} {first second : Valued world}
    (head : Valued.Refines first second) : Refines (valuedResult first) (valuedResult second) := by
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
  expr : ∀ scope store environment locals expression,
    Refines (evalExpr machine fuel scope store environment locals expression)
      (evalExpr machine bigger scope store environment locals expression)
  call : ∀ scope store environment locals body kind,
    Refines (callBlock machine fuel scope store environment locals body kind)
      (callBlock machine bigger scope store environment locals body kind)
  value : ∀ frame expression, Valued.Refines (runValue machine fuel frame expression) (runValue machine bigger frame expression)
  scope : ∀ frame body, Valued.Refines (valueScope machine fuel frame body) (valueScope machine bigger frame body)
  valueTail : ∀ frame body,
    Valued.Refines (runValueTail machine fuel frame body) (runValueTail machine bigger frame body)
  retries : ∀ frame pattern body delays,
    Valued.Refines (retryLoop machine fuel frame pattern body delays) (retryLoop machine bigger frame pattern body delays)
  handlers : ∀ frame item captures body,
    Valued.Refines (handlerScope machine fuel frame item captures body) (handlerScope machine bigger frame item captures body)
  comprehends : ∀ frame qualifiers value,
    Valued.Refines (comprehend machine fuel frame qualifiers value) (comprehend machine bigger frame qualifiers value)
  comprehendItems : ∀ frame target items rest value,
    Valued.Refines (comprehendItems machine fuel frame target items rest value)
      (comprehendItems machine bigger frame target items rest value)
  loopValues : ∀ frame body, Valued.Refines (loopValue machine fuel frame body) (loopValue machine bigger frame body)
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
  | exact inner.value _ _
  | exact inner.scope _ _
  | exact inner.valueTail _ _
  | exact inner.retries _ _ _ _
  | exact valued_refl _
  | refine fromResult_refines _ (inner.expr _ _ _ _ _) fun _ => ?_
  | refine toExec_refines ?_ fun _ _ => ?_
  | refine thenValued_refines ?_ fun _ => ?_
  | refine valued_bind_refines ?_ fun _ _ => ?_
  | refine resultThen_refines _ (evaluateAll_refines (inner.expr _ _ _ _) _) fun _ => ?_
  | exact fromEval_refines _ (inner.expr _ _ _ _ _)
  | refine capture_refines ?_
  | refine retryStep_refines _ _ _ _ (fun _ _ => inner.retries _ _ _ _) ?_
  | exact inner.handlers _ _ _ _
  | exact inner.comprehends _ _ _
  | exact inner.comprehendItems _ _ _ _ _
  | exact inner.loopValues _ _
  | refine collectWith_refines _ ?_
  | refine returnTo_refines _ _ ?_
  | exact inner.body _ _
  | refine loopStep_refines (fun _ => inner.loopValues _ _) ?_
  | refine resultExec_refines _ (selectorsOf_refines (inner.expr _ _ _ _) _ _) fun _ => ?_
  | refine guardElse_refines ?_
  | refine settle_refines (inner.leaves _) (leaveBlock_outOfFuel _ _ _) ?_
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

theorem leaveBlock_outOfFuel {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel (outer : Frame world), leaveBlock machine fuel outer .outOfFuel = .outOfFuel
  | 0, _ => by simp [leaveBlock]
  | fuel + 1, outer => by
    show withinScope outer (runDeferred machine fuel .outOfFuel []) = .outOfFuel
    rw [runDeferred_outOfFuel]
    rfl

theorem refining {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel extra, Refining machine fuel (fuel + extra)
  | 0, _ => {
      expr := fun _ _ _ _ _ => Or.inl (by simp [evalExpr])
      call := fun _ _ _ _ _ kind => Or.inl (by cases kind <;> simp [callBlock])
      value := fun _ _ => Or.inl (by simp [runValue])
      scope := fun _ _ => Or.inl (by simp [valueScope])
      valueTail := fun _ _ => Or.inl (by simp [runValueTail])
      retries := fun _ _ _ _ => Or.inl (by simp [retryLoop])
      handlers := fun _ _ _ _ => Or.inl (by simp [handlerScope])
      comprehends := fun _ _ _ => Or.inl (by simp [comprehend])
      comprehendItems := fun _ _ _ _ _ => Or.inl (by simp [comprehendItems])
      loopValues := fun _ _ => Or.inl (by simp [loopValue])
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
      expr := fun scope store environment locals expression => by
        simp only [evalExpr]
        repeat' (first
          | exact refines_refl _
          | exact valuedResult_refines (inner.value _ _)
          | exact inner.expr _ _ _ _ _
          | exact inner.call _ _ _ _ _ _
          | exact step_refines machine.context environment store (inner.expr scope store)
              (inner.call _ store) locals _
          | refine bind_refines (evaluateAll_refines (inner.expr _ _ _ _) _) fun _ => ?_
          | refine bind_refines (inner.expr _ _ _ _ _) fun _ => ?_
          | split)
      call := fun scope store environment locals body kind => by
        simp only [callBlock]
        exact boundary_refines kind (inner.body _ _)
      value := fun frame expression => by simp only [runValue]; close_refines
      scope := fun frame body => by simp only [valueScope]; close_refines
      valueTail := fun frame body => by simp only [runValueTail]; close_refines
      retries := fun frame pattern body delays => by simp only [retryLoop]; close_refines
      handlers := fun frame item captures body => by simp only [handlerScope]; close_refines
      comprehends := fun frame qualifiers value => by simp only [comprehend]; close_refines
      comprehendItems := fun frame target items rest value => by simp only [comprehendItems]; close_refines
      loopValues := fun frame body => by simp only [loopValue]; close_refines
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

/-- A value keeps the cells of `before` in the frame it leaves, an outcome in
every frame it carries. -/
def VKeeps {world : World Declaration Identity Source} (before : Store world) : Valued world → Prop
  | .value _ frame => Persists before frame.store
  | .exec outcome => Keeps before outcome

theorem vkeeps_exec {world : World Declaration Identity Source} {before : Store world} {outcome : Exec world}
    (kept : Keeps before outcome) : VKeeps before (.exec outcome) := kept

theorem vkeeps_value {world : World Declaration Identity Source} {before : Store world}
    (value : Value Declaration Identity Source) {frame : Frame world} (kept : Persists before frame.store) :
    VKeeps before (.value value frame) := kept

theorem toExec_persists {world : World Declaration Identity Source} {before : Store world} {first : Valued world}
    {next : Value Declaration Identity Source → Frame world → Exec world} (head : VKeeps before first)
    (tail : ∀ value frame, Keeps frame.store (next value frame)) : Keeps before (first.toExec next) := by
  cases first with
  | value value frame =>
    intro last present
    exact persists_trans head (tail value frame last present)
  | exec outcome => exact head

theorem thenValued_persists {world : World Declaration Identity Source} {before : Store world} {first : Exec world}
    {next : Frame world → Valued world} (head : Keeps before first)
    (tail : ∀ frame, VKeeps frame.store (next frame)) : VKeeps before (first.thenValued next) := by
  cases first with
  | normal frame =>
    have reached := head frame rfl
    have later := tail frame
    simp only [Exec.thenValued]
    cases result : next frame with
    | value value last => rw [result] at later; exact persists_trans reached later
    | exec outcome =>
      rw [result] at later
      intro final present
      exact persists_trans reached (later final present)
  | _ => exact head

theorem vbind_persists {world : World Declaration Identity Source} {before : Store world} {first : Valued world}
    {next : Value Declaration Identity Source → Frame world → Valued world} (head : VKeeps before first)
    (tail : ∀ value frame, VKeeps frame.store (next value frame)) : VKeeps before (first.bind next) := by
  cases first with
  | value value frame =>
    have later := tail value frame
    simp only [Valued.bind]
    cases result : next value frame with
    | value result last => rw [result] at later; exact persists_trans head later
    | exec outcome =>
      rw [result] at later
      intro final present
      exact persists_trans head (later final present)
  | exec outcome => exact head

theorem resultThen_persists {world : World Declaration Identity Source} {α : Type} (frame : Frame world)
    (result : Result Declaration Identity Source α) {next : α → Valued world}
    (tail : ∀ result, VKeeps frame.store (next result)) : VKeeps frame.store (resultThen frame result next) := by
  cases result with
  | value result => exact tail result
  | failure failure => intro last present; cases present; exact persists_refl _
  | _ => intro last present; cases present

theorem fromEval_persists {world : World Declaration Identity Source} (frame : Frame world)
    (result : Result Declaration Identity Source (Value Declaration Identity Source)) :
    VKeeps frame.store (fromEval frame result) :=
  resultThen_persists frame result fun _ => persists_refl _

theorem settle_persists {world : World Declaration Identity Source} {before : Store world}
    {leave : Exec world → Exec world} (leaving : ∀ outcome, Keeps before outcome → Keeps before (leave outcome))
    {first : Valued world} (head : VKeeps before first) : VKeeps before (settle leave first) := by
  cases first with
  | value result inner =>
    have left := leaving (.normal inner) (fun last present => by cases present; exact head)
    simp only [settle]
    cases result : leave (.normal inner) with
    | normal outer => rw [result] at left; exact left outer rfl
    | _ => rw [result] at left; exact left
  | exec other => exact leaving other head

theorem capture_persists {world : World Declaration Identity Source} {before : Store world} {first : Valued world}
    (head : VKeeps before first) : VKeeps before (capture first) := by
  unfold Xsh.Semantics.CoreControl.capture
  split
  · exact head
  · exact head _ rfl
  · exact head

theorem retryStep_persists {world : World Declaration Identity Source} {before : Store world}
    (context : Context Declaration Identity Source)
    (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) (pattern : Option Xsh.Syntax.Pattern)
    (delays : List (Value Declaration Identity Source))
    {next : Frame world → List (Value Declaration Identity Source) → Valued world}
    (tail : ∀ frame rest, VKeeps frame.store (next frame rest)) {first : Valued world}
    (head : VKeeps before first) : VKeeps before (retryStep context test pattern delays next first) := by
  unfold retryStep
  split
  · exact head
  · rename_i error after
    have reached : Persists before after.store := head after rfl
    split
    · split
      · rename_i delay rest
        have later := tail { after with waited := after.waited ++ [delay] } rest
        cases result : next { after with waited := after.waited ++ [delay] } rest with
        | value result last => rw [result] at later; exact persists_trans reached later
        | exec outcome =>
          rw [result] at later
          intro final present
          exact persists_trans reached (later final present)
      · exact reached
    · exact reached
    · intro last present; cases present
  · exact head

theorem loopStep_persists {world : World Declaration Identity Source} {before : Store world}
    {next : Frame world → Valued world} (tail : ∀ frame, VKeeps frame.store (next frame)) {first : Exec world}
    (head : Keeps before first) : VKeeps before (loopStep next first) := by
  cases first with
  | normal frame =>
    have reached := head frame rfl
    have later := tail frame
    simp only [loopStep]
    cases result : next frame with
    | value value last => rw [result] at later; exact persists_trans reached later
    | exec outcome =>
      rw [result] at later
      intro final present
      exact persists_trans reached (later final present)
  | continued frame =>
    have reached := head frame rfl
    have later := tail frame
    simp only [loopStep]
    cases result : next frame with
    | value value last => rw [result] at later; exact persists_trans reached later
    | exec outcome =>
      rw [result] at later
      intro final present
      exact persists_trans reached (later final present)
  | broke frame => exact head frame rfl
  | _ => exact head

theorem resultExec_persists {world : World Declaration Identity Source} {α : Type} (frame : Frame world)
    (result : Result Declaration Identity Source α) {next : α → Exec world}
    (tail : ∀ result, Keeps frame.store (next result)) : Keeps frame.store (resultExec frame result next) := by
  cases result with
  | value result => exact tail result
  | failure failure => intro last present; cases present; exact persists_refl _
  | _ => intro last present; cases present

theorem guardElse_persists {world : World Declaration Identity Source} {before : Store world} {first : Exec world}
    (head : Keeps before first) : Keeps before (guardElse first) := by
  cases first with
  | normal frame => intro last present; cases present
  | _ => exact head

theorem collectWith_persists {world : World Declaration Identity Source} {before : Store world} (outer : Frame world)
    {first : Exec world} (head : Keeps before first) : VKeeps before (collectWith outer first) := by
  cases first with
  | normal inner => exact head inner rfl
  | broke inner => intro last present; cases present; exact head inner rfl
  | continued inner => intro last present; cases present; exact head inner rfl
  | returned value inner => intro last present; cases present; exact head inner rfl
  | failed failure inner => intro last present; cases present; exact head inner rfl
  | internal fault inner => intro last present; cases present; exact head inner rfl
  | stuck => exact head
  | unsupported => exact head
  | outOfFuel => exact head

theorem returnTo_persists {world : World Declaration Identity Source} (caller : Frame world) (kind : ReturnKind)
    {first : Exec world} (head : Keeps caller.store first) : VKeeps caller.store (returnTo caller kind first) := by
  unfold returnTo
  split
  · exact persists_keep (head _ rfl)
  · split
    · exact persists_keep (head _ rfl)
    · intro last present; cases present; exact persists_keep (head _ rfl)
  · intro last present; cases present; exact persists_keep (head _ rfl)
  · intro last present; cases present
  · intro last present; cases present
  · intro last present; cases present

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
  body : ∀ frame statements, Keeps frame.store (runBody machine fuel frame statements)
  tail : ∀ frame statements, Keeps frame.store (runTail machine fuel frame statements)
  value : ∀ frame expression, VKeeps frame.store (runValue machine fuel frame expression)
  scope : ∀ frame body, VKeeps frame.store (valueScope machine fuel frame body)
  valueTail : ∀ frame body, VKeeps frame.store (runValueTail machine fuel frame body)
  retries : ∀ frame pattern body delays, VKeeps frame.store (retryLoop machine fuel frame pattern body delays)
  handlers : ∀ frame item captures body, VKeeps frame.store (handlerScope machine fuel frame item captures body)
  comprehends : ∀ frame qualifiers value, VKeeps frame.store (comprehend machine fuel frame qualifiers value)
  comprehendItems : ∀ frame target items rest value,
    VKeeps frame.store (comprehendItems machine fuel frame target items rest value)
  loopValues : ∀ frame body, VKeeps frame.store (loopValue machine fuel frame body)
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
  | exact vkeeps_value _ (persists_refl _)
  | exact fun _ present => by cases present <;> exact persists_refl _
  | exact bindCaptures_entered _ _ _
  | exact bindCaptures_persists _ _ _
  | exact bindTarget_entered _ _ _ _ _
  | exact inner.value _ _
  | exact inner.scope _ _
  | exact inner.valueTail _ _
  | exact inner.retries _ _ _ _
  | exact fromEval_persists _ _
  | refine fromResult_persists _ _ fun _ => ?_
  | refine toExec_persists ?_ fun _ _ => ?_
  | refine thenValued_persists ?_ fun _ => ?_
  | refine vbind_persists ?_ fun _ _ => ?_
  | refine resultThen_persists _ _ fun _ => ?_
  | refine capture_persists ?_
  | refine retryStep_persists _ _ _ _ (fun _ _ => inner.retries _ _ _ _) ?_
  | exact inner.handlers _ _ _ _
  | exact inner.comprehends _ _ _
  | exact inner.comprehendItems _ _ _ _ _
  | exact inner.loopValues _ _
  | refine collectWith_persists _ ?_
  | refine returnTo_persists _ _ ?_
  | exact inner.body _ _
  | exact inner.tail _ _
  | refine loopStep_persists (fun _ => inner.loopValues _ _) ?_
  | refine resultExec_persists _ _ fun _ => ?_
  | refine guardElse_persists ?_
  | refine settle_persists (inner.leaves _) ?_
  | exact vkeeps_exec fun _ present => by cases present
  | refine inner.leaves _ _ ?_
  | refine andThen_persists ?_ fun _ => ?_
  | refine iterate_persists ?_ fun _ => ?_
  | refine withinScope_persists _ ?_
  | split))

theorem persisting {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel, Persisting machine fuel
  | 0 => {
      body := fun _ _ => by simp only [runBody]; exact fun _ present => by cases present
      tail := fun _ _ => by simp only [runTail]; exact fun _ present => by cases present
      value := fun _ _ => by simp only [runValue]; exact fun _ present => by cases present
      scope := fun _ _ => by simp only [valueScope]; exact fun _ present => by cases present
      valueTail := fun _ _ => by simp only [runValueTail]; exact fun _ present => by cases present
      retries := fun _ _ _ _ => by simp only [retryLoop]; exact fun _ present => by cases present
      handlers := fun _ _ _ _ => by simp only [handlerScope]; exact fun _ present => by cases present
      comprehends := fun _ _ _ => by simp only [comprehend]; exact fun _ present => by cases present
      comprehendItems := fun _ _ _ _ _ => by simp only [comprehendItems]; exact fun _ present => by cases present
      loopValues := fun _ _ => by simp only [loopValue]; exact fun _ present => by cases present
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
      body := fun frame statements => by simp only [runBody]; close_persists
      tail := fun frame statements => by simp only [runTail]; close_persists
      value := fun frame expression => by simp only [runValue]; close_persists
      scope := fun frame body => by simp only [valueScope]; close_persists
      valueTail := fun frame body => by simp only [runValueTail]; close_persists
      retries := fun frame pattern body delays => by simp only [retryLoop]; close_persists
      handlers := fun frame item captures body => by simp only [handlerScope]; close_persists
      comprehends := fun frame qualifiers value => by simp only [comprehend]; close_persists
      comprehendItems := fun frame target items rest value => by simp only [comprehendItems]; close_persists
      loopValues := fun frame body => by simp only [loopValue]; close_persists
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
  simp [runWhile, evalExpr, step, literal, fromResult, boolean, Value.eraseNominal, conditionValue]

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
    (tested : Value Declaration Identity Source)
    (evaluated : evalExpr machine fuel frame.scope frame.store frame.environment frame.locals condition = .value tested)
    (isTrue : conditionValue tested = some (.ok true))
    (broke : runBlock machine fuel frame body = .broke next) :
    runWhile machine (fuel + 1) frame condition body = .normal next := by
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

section Deferral

variable [DecidableEq Declaration]

/-- `defer` and `errdefer` register their action, newest first, with the
environment and locals the action resolves its names in; registration runs
nothing. -/
theorem defer_registers {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (kind : Xsh.Syntax.DeferKind) (action : Xsh.Syntax.Expr) :
    runStatement machine (fuel + 1) frame (.defer kind action) =
      .normal { frame with deferred := ⟨kind, action, frame.environment, frame.locals⟩ :: frame.deferred } := by
  simp [runStatement]

/-- Leaving a block runs its pending actions on every exit: normal completion,
`break`, `continue`, `return` and failure. -/
theorem every_exit_unwinds {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (outer frame : Frame world) (value : Value Declaration Identity Source)
    (failure : Failure Declaration Identity Source) :
    (∀ inner, leaveBlock machine (fuel + 1) outer inner = withinScope outer (runDeferred machine fuel inner (pendingOf inner))) ∧
    pendingOf (.normal frame) = frame.deferred ∧ pendingOf (.broke frame) = frame.deferred ∧
    pendingOf (.continued frame) = frame.deferred ∧ pendingOf (.returned value frame) = frame.deferred ∧
    pendingOf (.failed failure frame) = frame.deferred :=
  ⟨fun _ => rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- Unwinding ends when no action is left. -/
theorem unwind_done {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (status : Exec world) : runDeferred machine (fuel + 1) status [] = status := by
  simp [runDeferred]

/-- The newest pending action runs first, once, on the store the block left;
the rest then unwind on the store it left. -/
theorem unwind_runs_newest {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (status : Exec world) (entry : Deferred Declaration Identity Source) (rest : List (Deferred Declaration Identity Source))
    (scope after : Frame world) (due : entry.kind = .exit ∨ exitsWithError status = true)
    (scoping : frameOf status = some scope) (ran : runBlock machine fuel (entry.frame scope) entry.body = .normal after) :
    runDeferred machine (fuel + 1) status (entry :: rest) = runDeferred machine fuel (status.withStore after.store) rest := by
  have notSkipped : ¬ (entry.kind = .error ∧ exitsWithError status = false) := by
    rintro ⟨isError, clean⟩
    rcases due with isExit | errored
    · rw [isExit] at isError; cases isError
    · rw [errored] at clean; cases clean
  simp only [runDeferred, notSkipped, ite_false, scoping, ran]

/-- An `errdefer` whose block leaves without an error is skipped in its place. -/
theorem errdefer_skipped {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (status : Exec world) (entry : Deferred Declaration Identity Source) (rest : List (Deferred Declaration Identity Source))
    (isError : entry.kind = .error) (clean : exitsWithError status = false) :
    runDeferred machine (fuel + 1) status (entry :: rest) = runDeferred machine fuel status rest := by
  simp [runDeferred, isError, clean]

omit [DecidableEq Declaration] in
/-- A block leaves with an error exactly when a failure leaves it or the
function returns an `Err` through it. -/
theorem errdefer_due {world : World Declaration Identity Source} (frame : Frame world)
    (value : Value Declaration Identity Source) (failure : Failure Declaration Identity Source) :
    exitsWithError (.failed failure frame) = true ∧ exitsWithError (.returned value frame) = isErr value ∧
    exitsWithError (.normal frame) = false ∧ exitsWithError (.broke frame) = false ∧
    exitsWithError (.continued frame) = false :=
  ⟨rfl, rfl, rfl, rfl, rfl⟩

/-- A failing action is a cleanup failure, and the remaining actions still
run. -/
theorem unwind_cleanup_fails {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (status : Exec world) (entry : Deferred Declaration Identity Source) (rest : List (Deferred Declaration Identity Source))
    (scope after : Frame world) (failure : Failure Declaration Identity Source)
    (due : entry.kind = .exit ∨ exitsWithError status = true)
    (scoping : frameOf status = some scope) (ran : runBlock machine fuel (entry.frame scope) entry.body = .failed failure after) :
    runDeferred machine (fuel + 1) status (entry :: rest) =
      runDeferred machine fuel (cleanupFailed failure after.store status) rest := by
  have notSkipped : ¬ (entry.kind = .error ∧ exitsWithError status = false) := by
    rintro ⟨isError, clean⟩
    rcases due with isExit | errored
    · rw [isExit] at isError; cases isError
    · rw [errored] at clean; cases clean
  simp only [runDeferred, notSkipped, ite_false, scoping, ran]

omit [DecidableEq Declaration] in
/-- The precedence law: an earlier failure stays primary and the cleanup
failure is reported; without one the cleanup failure becomes primary, and
any later `errdefer` of the block is then due. -/
theorem cleanup_precedence {world : World Declaration Identity Source} (failure primary : Failure Declaration Identity Source)
    (store : Store world) (frame : Frame world) (value : Value Declaration Identity Source) :
    cleanupFailed failure store (.failed primary frame) =
      .failed primary { frame with store := store, reported := frame.reported ++ [failure] } ∧
    cleanupFailed failure store (.normal frame) = .failed failure { frame with store := store } ∧
    cleanupFailed failure store (.broke frame) = .failed failure { frame with store := store } ∧
    cleanupFailed failure store (.continued frame) = .failed failure { frame with store := store } ∧
    cleanupFailed failure store (.returned value frame) = .failed failure { frame with store := store } ∧
    ∀ status : Exec world, frameOf status ≠ none → exitsWithError (cleanupFailed failure store status) = true :=
  ⟨rfl, rfl, rfl, rfl, rfl, fun status present => by cases status <;> simp_all [cleanupFailed, exitsWithError, frameOf]⟩

/-- Once a failure is primary, unwinding never replaces it. -/
theorem primary_stays {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel (primary : Failure Declaration Identity Source) (frame : Frame world) actions failure last,
      runDeferred machine fuel (.failed primary frame) actions = .failed failure last → failure = primary
  | 0, _, _, _, _, _, unwound => by simp [runDeferred] at unwound
  | fuel + 1, primary, frame, [], failure, last, unwound => by
    simp only [runDeferred, Exec.failed.injEq] at unwound
    exact unwound.1.symm
  | fuel + 1, primary, frame, entry :: rest, failure, last, unwound => by
    simp only [runDeferred] at unwound
    split at unwound
    · exact primary_stays machine fuel primary frame rest failure last unwound
    · simp only [frameOf] at unwound
      split at unwound
      · exact primary_stays machine fuel primary _ rest failure last unwound
      · exact primary_stays machine fuel primary _ rest failure last unwound
      · cases unwound
      · cases unwound
      · cases unwound
      · cases unwound
      · rename_i other _ _ _ _ _ _
        cases other <;> simp_all

/-- Each call runs in an activation `activationStep` past its caller's, and a
declared name's cell is its binder in the current activation. -/
theorem call_activation {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (scope : Scope Declaration Identity Source) (store : Store world) (defining : Environment) (parameters : Locals Declaration Identity Source)
    (body : List Xsh.Syntax.Statement) (kind : ReturnKind) (environment : Environment) (declared : Xsh.Syntax.RawName) :
    callBlock machine (fuel + 1) scope store defining parameters body kind =
      boundary kind (runBody machine fuel (Frame.ofScope defining store parameters (some store) scope) body) ∧
    (machine.namesAt scope.activation).declaration environment declared =
      (machine.resolution.declaration environment declared).map fun found =>
        (found.1, ⟨scope.activation, found.1⟩, found.2.2) :=
  ⟨rfl, rfl⟩

omit [DecidableEq Declaration] in
/-- A callee's cells are fresh: when every cell of the caller's store belongs
to an activation at most the caller's, a cell of a later activation is free. -/
theorem callee_cells_fresh {world : World Declaration Identity Source} (store : Store world) (activation step : Nat)
    (binder : BinderId) (below : ∀ cell found, store cell = some found → cell.activation ≤ activation)
    (later : 0 < step) : store ⟨activation + step, binder⟩ = none := by
  cases present : store ⟨activation + step, binder⟩ with
  | none => rfl
  | some found =>
    have bound := below _ found present
    simp at bound
    omega

/-- A callee cannot write a cell its caller holds unless it is one of the
callee's captured mutable cells. -/
theorem callee_cannot_write_caller {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (target : Xsh.Syntax.AssignTarget) (op : Xsh.Syntax.AssignOp) (value : Xsh.Syntax.Expr)
    (root : Xsh.Syntax.RawName) (path : List PathSpec) (cell : CellId) (caller : Store world) (held : Cell world)
    (fielded : flatten machine.context.spelling target = some (root, path))
    (resolved : frame.environment.cellOf root = .ok cell) (calling : frame.callerStore = some caller)
    (holds : caller cell = some held) (notWritable : cell ∉ frame.writable) :
    runStatement machine (fuel + 1) frame (.assign target op value) = .unsupported := by
  cases target with
  | env key => simp [flatten] at fielded
  | _ => simp [runStatement, fielded, resolved, calling, holds, notWritable]

/-- A function body's cells end with the call: a cell absent from the store
at the call is absent from every frame the body leaves with. -/
theorem body_cells_dropped {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (body : List Xsh.Syntax.Statement) (last : Frame world)
    (left : frameOf (runBody machine (fuel + 1) frame body) = some last) (cell : CellId)
    (absent : frame.store cell = none) : last.store cell = none := by
  simp only [runBody] at left
  cases fuel with
  | zero => simp [leaveBlock, frameOf] at left
  | succ fuel =>
    simp only [leaveBlock] at left
    generalize runDeferred machine fuel _ _ = unwound at left
    cases unwound <;> simp [withinScope, frameOf] at left <;> (subst left; simp [Frame.restore, keep, absent])

end Deferral

section Errors

/-- `?` unwraps `Ok` and raises the error of `Err` (SPEC 8.3). -/
theorem propagate_cases (payload error : Value Declaration Identity Source) :
    propagate (.ok payload) = .value payload ∧ propagate (.err error) = .failure (.raised error) := ⟨rfl, rfl⟩

/-- `try { ... }` captures exactly the raised failures: they become `Err`,
normal completion is `Ok` of the tail, and runtime and type failures and every
other exit pass on uncaptured (SPEC 8.8). -/
theorem capture_cases {world : World Declaration Identity Source} (value error : Value Declaration Identity Source)
    (expected : ValueType Declaration) (frame : Frame world) :
    Xsh.Semantics.CoreControl.capture (.value value frame) = .value (.ok value) frame ∧
    Xsh.Semantics.CoreControl.capture (.exec (.failed (.raised error) frame)) = .value (.err error) frame ∧
    Xsh.Semantics.CoreControl.capture (.exec (.failed (.runtime error) frame)) = .exec (.failed (.runtime error) frame) ∧
    Xsh.Semantics.CoreControl.capture (.exec (.failed (.typeError expected error) frame)) =
      .exec (.failed (.typeError expected error) frame) ∧
    Xsh.Semantics.CoreControl.capture (.exec (.returned value frame)) = .exec (.returned value frame) ∧
    Xsh.Semantics.CoreControl.capture (.exec (.broke frame)) = .exec (.broke frame) :=
  ⟨rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- A function's call result: a `Result` function returns a raised failure,
including a primary cleanup failure, as its `Err` and wraps a plain value in
`Ok`; any other function fails the call. -/
theorem boundary_cases {world : World Declaration Identity Source} (value error : Value Declaration Identity Source)
    (failure : Failure Declaration Identity Source) (frame : Frame world) :
    boundary .result (.failed (.raised error) frame) = .value (.err error) ∧
    boundary .plain (.failed failure frame) = .failure failure ∧
    boundary .result (.returned value frame) = .value (wrapOk value) ∧
    boundary .plain (.returned value frame) = .value value ∧
    (∀ inner, boundary .result (.failed (.runtime inner) frame) = .failure (.runtime inner)) :=
  ⟨rfl, rfl, rfl, rfl, fun _ => rfl⟩

/-- One `retry` attempt: success is `Ok`; a raised failure the filter selects
takes the next delay, recorded as waited, and runs the next attempt; with no
delay left, or when the filter does not select it, it is `Err`; anything else
passes on. -/
theorem retryStep_cases {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) (pattern : Option Xsh.Syntax.Pattern) (next : Frame world → List (Value Declaration Identity Source) → Valued world)
    (value error delay : Value Declaration Identity Source) (rest : List (Value Declaration Identity Source))
    (after : Frame world) (inner : Value Declaration Identity Source) :
    retryStep context test pattern (delay :: rest) next (.value value after) = .value (.ok value) after ∧
    (retryable context test pattern error = some true →
      retryStep context test pattern (delay :: rest) next (.exec (.failed (.raised error) after)) =
        next { after with waited := after.waited ++ [delay] } rest) ∧
    (retryable context test pattern error = some true →
      retryStep context test pattern [] next (.exec (.failed (.raised error) after)) = .value (.err error) after) ∧
    (retryable context test pattern error = some false →
      retryStep context test pattern (delay :: rest) next (.exec (.failed (.raised error) after)) = .value (.err error) after) ∧
    retryStep context test pattern (delay :: rest) next (.exec (.failed (.runtime inner) after)) =
      .exec (.failed (.runtime inner) after) := by
  refine ⟨rfl, fun selected => ?_, fun selected => ?_, fun unselected => ?_, rfl⟩
  · simp [retryStep, selected]
  · simp [retryStep, selected]
  · simp [retryStep, unselected]

/-- A value block's `Err` tail is its value: the block is left as a normal
exit, so its `errdefer` actions are not due (SPEC 8.7). -/
theorem value_block_leaves_normally {world : World Declaration Identity Source} (leave : Exec world → Exec world)
    (value : Value Declaration Identity Source) (inner : Frame world) :
    settle leave (.value value inner) =
      (match leave (.normal inner) with
        | .normal outer => .value value outer
        | other => .exec other) ∧
    exitsWithError (.normal inner) = false :=
  ⟨rfl, rfl⟩

variable [DecidableEq Declaration]

/-- The error-handling forms: `?` and conversions propagate through
`propagate`, `try` is `capture` of a value block, a value block settles through
`leaveBlock`, and `retry` runs `retryStep` on each attempt. -/
theorem error_forms {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (inner : Xsh.Syntax.Expr) (target : Xsh.Syntax.TypeExpr) (body : List Xsh.Syntax.Statement)
    (pattern : Option Xsh.Syntax.Pattern) (delays : List (Value Declaration Identity Source)) :
    runValue machine (fuel + 1) frame (.tryExpr inner) =
      (runValue machine fuel frame inner).bind (fun value next => fromEval next (propagate value)) ∧
    runValue machine (fuel + 1) frame (.convert inner target) =
      (runValue machine fuel frame inner).bind (fun value next =>
        match machine.conversion value target with
        | some converted => fromEval next (propagate converted)
        | none => .exec .unsupported) ∧
    runValue machine (fuel + 1) frame (.capture body) = Xsh.Semantics.CoreControl.capture (valueScope machine fuel frame body) ∧
    runValue machine (fuel + 1) frame (.valueBlock body) = valueScope machine fuel frame body ∧
    valueScope machine (fuel + 1) frame body = settle (leaveBlock machine fuel frame) (runValueTail machine fuel frame.enter body) ∧
    retryLoop machine (fuel + 1) frame pattern body delays =
      retryStep machine.context machine.typeTest pattern delays (fun after rest => retryLoop machine fuel after pattern body rest)
        (valueScope machine fuel frame body) :=
  ⟨rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- A statement-position `Result[Unit]` propagates its failure: an `Err`
value fails the statement with its error, and any other value continues. -/
theorem statement_result {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame next : Frame world) (value : Xsh.Syntax.Expr) (plain : ∀ body, value ≠ .valueBlock body)
    (result error : Value Declaration Identity Source) (ran : runValue machine fuel frame value = .value result next) :
    (result.eraseNominal = .err error →
      runStatement machine (fuel + 1) frame (.expression value) = .failed (.raised error) next) ∧
    ((∀ error, result.eraseNominal ≠ .err error) → runStatement machine (fuel + 1) frame (.expression value) = .normal next) := by
  constructor
  · intro isErr
    cases value
    case valueBlock body => exact absurd rfl (plain body)
    all_goals simp [runStatement, ran, Valued.toExec, isErr]
  · intro notErr
    have normal : (match result.eraseNominal with
        | .err error => (Exec.failed (.raised error) next : Exec world)
        | _ => .normal next) = .normal next := by
      split
      · rename_i error same
        exact absurd same (notErr error)
      · rfl
    cases value
    case valueBlock body => exact absurd rfl (plain body)
    all_goals (simp only [runStatement, ran, Valued.toExec]; try exact normal)

/-- A call evaluates a block body in a fresh activation and applies its
function's return kind. -/
theorem call_boundary {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (scope : Scope Declaration Identity Source) (store : Store world) (defining : Environment) (parameters : Locals Declaration Identity Source)
    (body : List Xsh.Syntax.Statement) (kind : ReturnKind) :
    callBlock machine (fuel + 1) scope store defining parameters body kind =
      boundary kind (runBody machine fuel (Frame.ofScope defining store parameters (some store) scope) body) :=
  rfl

end Errors

section Resolver

/-- The binder the resolver gives a declaration of `name` at lexical depth
`depth`: SPEC `check.duplicate-name` allows one declaration of a name per
scope, so a name and its scope depth identify a live declaration within an
activation. -/
def canonicalBinder (name : Xsh.Syntax.RawName) (depth : Nat) : BinderId :=
  ⟨⟨(name.word.val * 65536 + depth) % 4294967296, Nat.mod_lt _ (by decide)⟩⟩

/-- The resolution the machine consults, computed from the name and the
lexical depth of its environment; only the declared type stays an oracle. -/
def canonicalResolution {world : World Declaration Identity Source} (base : Oracles world)
    (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration)) : Oracles world :=
  { base with
    declaration := fun environment declared =>
      (types environment declared).map fun type =>
        let binder := canonicalBinder declared environment.names.length
        (binder, ⟨0, binder⟩, type) }

/-- Distinct names, or the same name at distinct depths, get distinct binders
while names stay below 2^16 and scopes nest fewer than 2^16 deep. -/
theorem canonicalBinder_injective (first second : Xsh.Syntax.RawName) (depth depth' : Nat)
    (firstSmall : first.word.val < 65536) (secondSmall : second.word.val < 65536) (shallow : depth < 65536)
    (shallow' : depth' < 65536) (same : canonicalBinder first depth = canonicalBinder second depth') :
    first = second ∧ depth = depth' := by
  simp only [canonicalBinder, BinderId.mk.injEq, Fin.mk.injEq] at same
  have depths : depth = depth' := by omega
  have words : first.word.val = second.word.val := by omega
  refine ⟨?_, depths⟩
  cases first with
  | mk word =>
    cases second with
    | mk word' =>
      simp only [NameId.mk.injEq]
      exact Fin.ext words

/-- Entering a block deepens the lexical environment by one scope. -/
theorem enter_depth {world : World Declaration Identity Source} (frame : Frame world) :
    frame.enter.environment.names.length = frame.environment.names.length + 1 := by
  simp [Frame.enter, enterScope]

variable [DecidableEq Declaration]

omit [DecidableEq Declaration] in
/-- With the canonical resolution, a declaration's cell is its binder in the
current activation, so a name shadowed in an inner scope gets a cell distinct
from the outer one's. -/
theorem canonical_cells {world : World Declaration Identity Source} (machine : Machine world) (base : Oracles world)
    (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (activation : Nat) (environment : Environment)
    (declared : Xsh.Syntax.RawName) :
    (machine.namesAt activation).declaration environment declared =
      (types environment declared).map fun type =>
        (canonicalBinder declared environment.names.length,
          ⟨activation, canonicalBinder declared environment.names.length⟩, type) := by
  simp only [Machine.namesAt, resolved, canonicalResolution]
  cases types environment declared <;> rfl

theorem shadowing_cells_distinct (name : Xsh.Syntax.RawName) (outer inner activation : Nat)
    (small : name.word.val < 65536) (deeper : outer < inner) (shallow : inner < 65536) :
    (⟨activation, canonicalBinder name outer⟩ : CellId) ≠ ⟨activation, canonicalBinder name inner⟩ := by
  intro same
  simp only [CellId.mk.injEq, true_and] at same
  have := (canonicalBinder_injective name name outer inner small small (by omega) shallow same).2
  omega

end Resolver

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

omit [DecidableEq Declaration] in
/-- SPEC 8.7 cleanup and per-call activations in the statement machine:
`defer` and `errdefer` register newest first and run nothing; leaving a block
runs its pending actions on every exit (normal, `break`, `continue`, `return`
and failure), newest first and each once, on the store the block left; an
`errdefer` is skipped unless the block leaves with an error, which is a
failure or an `Err` returned through it or a cleanup failure of its own; a
failing action is a cleanup failure, an earlier failure stays primary with
the cleanup failure reported, and without one the cleanup failure becomes
primary and is never replaced; each call runs `activationStep` activations
past its caller with its cells keyed by that activation, so they are free in
any store of earlier activations, the callee cannot write a cell its caller
holds, and its cells end with the call. -/
theorem deferred_cleanup {world : World Declaration Identity Source} [DecidableEq Declaration] (machine : Machine world) :
    (∀ fuel frame kind action, runStatement machine (fuel + 1) frame (.defer kind action) =
      .normal { frame with deferred := ⟨kind, action, frame.environment, frame.locals⟩ :: frame.deferred }) ∧
    (∀ fuel outer (inner : Exec world),
      leaveBlock machine (fuel + 1) outer inner = withinScope outer (runDeferred machine fuel inner (pendingOf inner))) ∧
    (∀ (frame : Frame world) value failure, pendingOf (.normal frame) = frame.deferred ∧
      pendingOf (.broke frame) = frame.deferred ∧ pendingOf (.continued frame) = frame.deferred ∧
      pendingOf (.returned value frame) = frame.deferred ∧ pendingOf (.failed failure frame) = frame.deferred) ∧
    (∀ fuel (status : Exec world), runDeferred machine (fuel + 1) status [] = status) ∧
    (∀ fuel status entry rest scope after, (entry.kind = .exit ∨ exitsWithError status = true) →
      frameOf status = some scope → runBlock machine fuel (entry.frame scope) entry.body = .normal after →
        runDeferred machine (fuel + 1) status (entry :: rest) = runDeferred machine fuel (status.withStore after.store) rest) ∧
    (∀ fuel status (entry : Deferred Declaration Identity Source) rest, entry.kind = .error →
      exitsWithError status = false → runDeferred machine (fuel + 1) status (entry :: rest) = runDeferred machine fuel status rest) ∧
    (∀ (frame : Frame world) value failure, exitsWithError (.failed failure frame) = true ∧
      exitsWithError (.returned value frame) = isErr value ∧ exitsWithError (.normal frame) = false ∧
      exitsWithError (.broke frame) = false ∧ exitsWithError (.continued frame) = false) ∧
    (∀ fuel status entry rest scope after failure, (entry.kind = .exit ∨ exitsWithError status = true) →
      frameOf status = some scope → runBlock machine fuel (entry.frame scope) entry.body = .failed failure after →
        runDeferred machine (fuel + 1) status (entry :: rest) =
          runDeferred machine fuel (cleanupFailed failure after.store status) rest) ∧
    (∀ failure primary (store : Store world) (frame : Frame world) value,
      cleanupFailed failure store (.failed primary frame) =
        .failed primary { frame with store := store, reported := frame.reported ++ [failure] } ∧
      cleanupFailed failure store (.normal frame) = .failed failure { frame with store := store } ∧
      cleanupFailed failure store (.broke frame) = .failed failure { frame with store := store } ∧
      cleanupFailed failure store (.continued frame) = .failed failure { frame with store := store } ∧
      cleanupFailed failure store (.returned value frame) = .failed failure { frame with store := store } ∧
      ∀ status : Exec world, frameOf status ≠ none → exitsWithError (cleanupFailed failure store status) = true) ∧
    (∀ fuel primary (frame : Frame world) actions failure last,
      runDeferred machine fuel (.failed primary frame) actions = .failed failure last → failure = primary) ∧
    (∀ fuel (scope : Scope Declaration Identity Source) store defining parameters body kind environment declared,
      callBlock machine (fuel + 1) scope store defining parameters body kind =
        boundary kind (runBody machine fuel (Frame.ofScope defining store parameters (some store) scope) body) ∧
      (machine.namesAt scope.activation).declaration environment declared =
        (machine.resolution.declaration environment declared).map fun found =>
          (found.1, ⟨scope.activation, found.1⟩, found.2.2)) ∧
    (∀ (store : Store world) activation step binder,
      (∀ cell found, store cell = some found → cell.activation ≤ activation) → 0 < step →
        store ⟨activation + step, binder⟩ = none) ∧
    (∀ fuel frame target op value root path cell caller held,
      flatten machine.context.spelling target = some (root, path) → frame.environment.cellOf root = .ok cell →
        frame.callerStore = some caller → caller cell = some held → cell ∉ frame.writable →
          runStatement machine (fuel + 1) frame (.assign target op value) = .unsupported) ∧
    (∀ fuel frame body last, frameOf (runBody machine (fuel + 1) frame body) = some last →
      ∀ cell, frame.store cell = none → last.store cell = none) :=
  ⟨defer_registers machine,
    fun fuel outer inner => (every_exit_unwinds machine fuel outer outer (.scalar .null) (.runtime (.scalar .null))).1 inner,
    fun _ _ _ => ⟨rfl, rfl, rfl, rfl, rfl⟩,
    unwind_done machine,
    unwind_runs_newest machine,
    errdefer_skipped machine,
    errdefer_due,
    unwind_cleanup_fails machine,
    cleanup_precedence,
    primary_stays machine,
    fun fuel scope store defining parameters body kind environment declared =>
      call_activation machine fuel scope store defining parameters body kind environment declared,
    callee_cells_fresh,
    callee_cannot_write_caller machine,
    body_cells_dropped machine⟩

/-- The error-handling core of SPEC 8 in the statement machine. `?` and a
conversion unwrap `Ok` and raise the error of `Err`, in statements and inside
expressions; a statement-position `Err` fails its statement; `try { ... }` is
a value block whose raised failure becomes `Err` and whose normal completion
is `Ok` of the tail, while runtime and type failures and `return`, `break`
and `continue` pass on; a value block's tail is its value and an `Err` tail
leaves it normally; each `retry` attempt is a value block, a raised failure
the `on` filter selects takes the next delay as an observation and runs again,
and the result is `Ok` of the first success or `Err` of the last failure; and a
block function's return kind decides its call: a `Result` function returns a
raised failure, including a primary cleanup failure, as its `Err` and wraps a
plain value in `Ok`, and any other function fails the call. -/
theorem error_handling {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ payload error : Value Declaration Identity Source,
      propagate payload.ok = .value payload ∧ propagate (.err error) = .failure (.raised error)) ∧
    (∀ fuel scope store environment locals inner,
      evalExpr machine (fuel + 1) scope store environment locals (.tryExpr inner) =
        (evalExpr machine fuel scope store environment locals inner).bind propagate) ∧
    (∀ fuel frame inner target body pattern delays,
      runValue machine (fuel + 1) frame (.tryExpr inner) =
        (runValue machine fuel frame inner).bind (fun value next => fromEval next (propagate value)) ∧
      runValue machine (fuel + 1) frame (.convert inner target) =
        (runValue machine fuel frame inner).bind (fun value next =>
          match machine.conversion value target with
          | some converted => fromEval next (propagate converted)
          | none => .exec .unsupported) ∧
      runValue machine (fuel + 1) frame (.capture body) =
        Xsh.Semantics.CoreControl.capture (valueScope machine fuel frame body) ∧
      runValue machine (fuel + 1) frame (.valueBlock body) = valueScope machine fuel frame body ∧
      valueScope machine (fuel + 1) frame body =
        settle (leaveBlock machine fuel frame) (runValueTail machine fuel frame.enter body) ∧
      retryLoop machine (fuel + 1) frame pattern body delays =
        retryStep machine.context machine.typeTest pattern delays (fun after rest => retryLoop machine fuel after pattern body rest)
          (valueScope machine fuel frame body)) ∧
    (∀ fuel frame next value result error, (∀ body, value ≠ .valueBlock body) →
      runValue machine fuel frame value = .value result next →
        (result.eraseNominal = .err error →
          runStatement machine (fuel + 1) frame (.expression value) = .failed (.raised error) next) ∧
        ((∀ error, result.eraseNominal ≠ .err error) →
          runStatement machine (fuel + 1) frame (.expression value) = .normal next)) ∧
    (∀ (value error : Value Declaration Identity Source) expected (frame : Frame world),
      Xsh.Semantics.CoreControl.capture (.value value frame) = .value value.ok frame ∧
      Xsh.Semantics.CoreControl.capture (.exec (.failed (.raised error) frame)) = .value (.err error) frame ∧
      Xsh.Semantics.CoreControl.capture (.exec (.failed (.runtime error) frame)) =
        .exec (.failed (.runtime error) frame) ∧
      Xsh.Semantics.CoreControl.capture (.exec (.failed (.typeError expected error) frame)) =
        .exec (.failed (.typeError expected error) frame) ∧
      Xsh.Semantics.CoreControl.capture (.exec (.returned value frame)) = .exec (.returned value frame) ∧
      Xsh.Semantics.CoreControl.capture (.exec (.broke frame)) = .exec (.broke frame)) ∧
    (∀ (leave : Exec world → Exec world) value inner,
      settle leave (.value value inner) =
        (match leave (.normal inner) with
          | .normal outer => .value value outer
          | other => .exec other) ∧
      exitsWithError (.normal inner) = false) ∧
    (∀ pattern (next : Frame world → List (Value Declaration Identity Source) → Valued world) value error delay rest
        after inner,
      retryStep machine.context machine.typeTest pattern (delay :: rest) next (.value value after) = .value value.ok after ∧
      (retryable machine.context machine.typeTest pattern error = some true →
        retryStep machine.context machine.typeTest pattern (delay :: rest) next (.exec (.failed (.raised error) after)) =
          next { after with waited := after.waited ++ [delay] } rest) ∧
      (retryable machine.context machine.typeTest pattern error = some true →
        retryStep machine.context machine.typeTest pattern [] next (.exec (.failed (.raised error) after)) = .value (.err error) after) ∧
      (retryable machine.context machine.typeTest pattern error = some false →
        retryStep machine.context machine.typeTest pattern (delay :: rest) next (.exec (.failed (.raised error) after)) =
          .value (.err error) after) ∧
      retryStep machine.context machine.typeTest pattern (delay :: rest) next (.exec (.failed (.runtime inner) after)) =
        .exec (.failed (.runtime inner) after)) ∧
    (∀ fuel (scope : Scope Declaration Identity Source) store defining parameters body kind,
      callBlock machine (fuel + 1) scope store defining parameters body kind =
        boundary kind (runBody machine fuel (Frame.ofScope defining store parameters (some store) scope) body)) ∧
    (∀ (value error : Value Declaration Identity Source) failure (frame : Frame world),
      boundary .result (.failed (.raised error) frame) = .value (.err error) ∧
      boundary .plain (.failed failure frame) = .failure failure ∧
      boundary .result (.returned value frame) = .value (wrapOk value) ∧
      boundary .plain (.returned value frame) = .value value ∧
      ∀ inner, boundary .result (.failed (.runtime inner) frame) = .failure (.runtime inner)) :=
  ⟨propagate_cases,
    fun _ _ _ _ _ _ => rfl,
    fun fuel frame inner target body pattern delays => error_forms machine fuel frame inner target body pattern delays,
    fun fuel frame next value result error plain ran => statement_result machine fuel frame next value plain result error ran,
    capture_cases,
    value_block_leaves_normally,
    fun pattern next value error delay rest after inner =>
      retryStep_cases machine.context machine.typeTest pattern next value error delay rest after inner,
    call_boundary machine,
    boundary_cases⟩

omit [DecidableEq Declaration] in
/-- The pure binding and `Result` forms of the statement machine: `Result[Bool]`
conditions; `for` over a `Map` in key order, a `Str` and `Bytes`; `guard let`'s
bound value; `loop` expressions; selectors for index assignment; and the
alternation, alias and `Ok`/`Err` constructor patterns. -/
theorem pure_forms_laws {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
    (∀ (flag : Bool) (error : Value Declaration Identity Source),
      conditionValue (.scalar (.boolean flag) : Value Declaration Identity Source) = some (.ok flag) ∧
      conditionValue (.ok (.scalar (.boolean flag)) : Value Declaration Identity Source) = some (.ok flag) ∧
      conditionValue (.err error) = some (.error error)) ∧
    (∀ entries : List (MapKey × Value Declaration Identity Source),
      iterationItems (.map entries) =
        some ((entries.foldr (fun entry ordered => insertOrdered entry.1 entry.2 ordered) []).map fun entry =>
          .record [("key", keyValue entry.1), ("value", entry.2)])) ∧
    (∀ text : String, iterationItems (.scalar (.string text) : Value Declaration Identity Source) =
      some (text.toList.map fun character => .scalar (.string character.toString))) ∧
    (∀ (payload error : Value Declaration Identity Source),
      guardValue true payload.ok = some payload ∧ guardValue true (.err error) = none ∧
      guardValue false (.scalar .null : Value Declaration Identity Source) = none) ∧
    (∀ (next : Frame world → Valued world) (frame : Frame world),
      loopStep next (.broke frame) = .value (frame.breakValue.getD (.scalar .unit)) { frame with breakValue := none } ∧
      loopStep next (.normal frame) = next frame ∧ loopStep next (.continued frame) = next frame) ∧
    (∀ (inner : Xsh.Syntax.Pattern) (bound : Xsh.Syntax.RawName) (value : Value Declaration Identity Source),
      matchPattern context test (.alias inner bound) value =
        (matchPattern context test inner value).both fun _ => .matched [(bound, value)]) ∧
    (∀ (members : List Xsh.Syntax.Pattern) (value : Value Declaration Identity Source),
      matchPattern context test (.alternation members) value = matchAlternatives context test members value) ∧
    (∀ (member : Xsh.Syntax.Pattern) rest (value : Value Declaration Identity Source) captures,
      matchPattern context test member value = .matched captures →
        matchAlternatives context test (member :: rest) value = .matched captures) ∧
    (∀ (member : Xsh.Syntax.Pattern) rest (value : Value Declaration Identity Source),
      matchPattern context test member value = .failed →
        matchAlternatives context test (member :: rest) value = matchAlternatives context test rest value) ∧
    (∀ (name : Xsh.Syntax.RawName) (inner : Xsh.Syntax.Pattern) (payload : Value Declaration Identity Source),
      context.spelling name = "Ok" →
        matchPattern context test (.constructor name (some inner)) payload.ok = matchPattern context test inner payload) := by
  refine ⟨fun _ _ => ⟨rfl, rfl, rfl⟩, fun _ => rfl, fun _ => rfl, fun _ _ => ⟨rfl, rfl, rfl⟩,
    fun _ _ => ⟨rfl, rfl, rfl⟩, fun _ _ _ => rfl, fun _ _ => rfl, ?_, ?_, ?_⟩
  · intro member rest value captures matched
    simp [matchAlternatives, matched]
  · intro member rest value failed
    simp [matchAlternatives, failed]
  · intro name inner payload spelled
    simp [matchPattern, spelled, Value.eraseNominal]

/-- The binding forms added for BASE-ENV's pure clauses, and the resolver
that replaces the binder and cell choice of the resolution oracle. A local
function declaration binds its name in the current scope to a closure over
the declaring environment; an implicit `.` reads the scope's item; `collect`
yields its gathered list on normal completion; and the canonical resolver
gives each declaration the binder of its name at its lexical depth, so a
declaration's cell is that binder in the current activation, distinct
declarations get distinct binders within the name-table and nesting bounds,
entering a block deepens the scope by one, and a shadowing declaration gets
a cell distinct from the shadowed one. The declared type is still an oracle. -/
theorem local_bindings {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ fuel (frame : Frame world) kind name parameters effects result body metadata test names binder cell type,
      kind ≠ .stream → kind ≠ .cliMain → plainParameters parameters = some names →
        (machine.namesAt frame.activation).declaration frame.environment name = some (binder, cell, type) →
          runStatement machine (fuel + 1) frame (.functionDef kind name parameters effects result body metadata test) =
            .normal { frame with
              environment := extend frame.environment [(name, binder, cell)]
              locals := hide [name] frame.locals
              closures := (cell, .mk cell (extend frame.environment [(name, binder, cell)]) frame.closures
                (mutableCaptures frame (captureNames names body)) names (.block body (returnKindOf machine result))) ::
                frame.closures }) ∧
    (∀ fuel (scope : Scope Declaration Identity Source) store environment locals value,
      scope.item = some value → evalExpr machine (fuel + 1) scope store environment locals .item = .value value) ∧
    (∀ (outer inner : Frame world),
      collectWith outer (.normal inner) =
        .value (.list (inner.collected.getD [])) { inner with collected := outer.collected }) ∧
    (∀ first second (depth depth' : Nat), first.word.val < 65536 → second.word.val < 65536 → depth < 65536 →
      depth' < 65536 → canonicalBinder first depth = canonicalBinder second depth' → first = second ∧ depth = depth') ∧
    (∀ frame : Frame world, frame.enter.environment.names.length = frame.environment.names.length + 1) ∧
    (∀ base types, machine.resolution = canonicalResolution base types → ∀ activation environment declared,
      (machine.namesAt activation).declaration environment declared =
        (types environment declared).map fun type =>
          (canonicalBinder declared environment.names.length,
            ⟨activation, canonicalBinder declared environment.names.length⟩, type)) ∧
    (∀ name (outer inner activation : Nat), name.word.val < 65536 → outer < inner → inner < 65536 →
      (⟨activation, canonicalBinder name outer⟩ : CellId) ≠ ⟨activation, canonicalBinder name inner⟩) := by
  refine ⟨?_, ?_, fun _ _ => rfl, canonicalBinder_injective, enter_depth, fun base types resolved =>
    canonical_cells machine base types resolved, shadowing_cells_distinct⟩
  · intro fuel frame kind name parameters effects result body metadata test names binder cell type notStream notMain
      plain declared
    cases kind
    · simp [runStatement, plain, declared]
    · simp [runStatement, plain, declared]
    · exact absurd rfl notStream
    · exact absurd rfl notMain
  · intro fuel scope store environment locals value present
    simp [evalExpr, present]

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
def sumName : Xsh.Syntax.RawName := name 38 (by decide)
def nName : Xsh.Syntax.RawName := name 39 (by decide)
def accName : Xsh.Syntax.RawName := name 40 (by decide)
def fName : Xsh.Syntax.RawName := name 41 (by decide)
def rName : Xsh.Syntax.RawName := name 42 (by decide)
def attemptsName : Xsh.Syntax.RawName := name 44 (by decide)
def cleanupsName : Xsh.Syntax.RawName := name 45 (by decide)
def errName : Xsh.Syntax.RawName := name 46 (by decide)
def resultFnName : Xsh.Syntax.RawName := name 47 (by decide)
def plainFnName : Xsh.Syntax.RawName := name 48 (by decide)
def tailErrName : Xsh.Syntax.RawName := name 49 (by decide)
def pName : Xsh.Syntax.RawName := name 50 (by decide)

def int (lexeme : String) : Xsh.Syntax.Expr := .literal (.integer lexeme)
def ident (target : Xsh.Syntax.RawName) : Xsh.Syntax.Expr := .ident target

def program : Context Unit Unit Unit :=
  { Xsh.Proofs.Core.EvalStatements.Witness.context with
    literals := { Xsh.Proofs.Core.EvalStatements.Witness.context.literals with
      duration := fun lexeme => if lexeme = "1ms" then some ⟨1, by decide⟩ else none }
    functions := fun called =>
      if called = gName then
        some (⟨emptyLexicalEnvironment, []⟩, [],
          .block [.forStmt (.name iName) (.list [int "1", int "2"]) [.returnValue (some (ident iName))], .expression (int "7")] .plain)
      else if called = sumName then
        some (⟨emptyLexicalEnvironment, []⟩, [nName],
          .block [.binding .var (.name accName) none (ident nName),
            .ifStmt [(.binary .eq (ident nName) (int "0"), [.returnValue (some (int "0"))])] none,
            .assign (.name accName) .add (.call (ident sumName) [.positional (.binary .sub (ident nName) (int "1"))]),
            .expression (ident accName)] .plain)
      else if called = fName then
        some (⟨emptyLexicalEnvironment, []⟩, [],
          .block [.binding .var (.name xName) none (int "1"),
            .defer .exit (.valueBlock [.assign (.name xName) .set (int "2")]),
            .returnValue (some (ident xName))] .plain)
      else if called = resultFnName then
        some (⟨emptyLexicalEnvironment, []⟩, [],
          .block [.defer .exit (.valueBlock [.assert (.literal (.bool false)) (some (.literal (.string "cleanup")))]),
            .expression (int "7")] .result)
      else if called = plainFnName then
        some (⟨emptyLexicalEnvironment, []⟩, [],
          .block [.defer .exit (.valueBlock [.assert (.literal (.bool false)) (some (.literal (.string "cleanup")))]),
            .expression (int "7")] .plain)
      else if called = tailErrName then
        some (⟨emptyLexicalEnvironment, []⟩, [pName],
          .block [.defer .error (.valueBlock [.assert (.literal (.bool false)) (some (.literal (.string "ran")))]),
            .expression (ident pName)] .result)
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
  conversion := fun _ _ => none
  typeOf := fun _ => none
  resultSubject := fun _ _ => true

def start : Frame witnessWorld := Frame.ofScope ⟨emptyLexicalEnvironment, []⟩ (fun _ => none) [] none ⟨0, [], none⟩

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

def readAny (target : Xsh.Syntax.RawName) (outcome : Exec witnessWorld) : Option Int :=
  match frameOf outcome with
  | some frame =>
    match frame.environment.cellOf target with
    | .ok cell =>
      (frame.store cell).bind fun found =>
        match found.contents.value with
        | .scalar (.integer value) => some value.val
        | _ => none
    | .error _ => none
  | none => none

def primaryText : Exec witnessWorld → Option String
  | .failed (.raised (.scalar (.string text))) _ => some text
  | _ => none

def reportedTexts : Exec witnessWorld → List String
  | .failed _ frame => frame.reported.filterMap fun failure =>
    match failure with
    | .raised (.scalar (.string text)) => some text
    | _ => none
  | _ => []

def set (target : Xsh.Syntax.RawName) (lexeme : String) : Xsh.Syntax.Statement := .assign (.name target) .set (int lexeme)
def deferring (body : List Xsh.Syntax.Statement) : Xsh.Syntax.Statement := .defer .exit (.valueBlock body)
def erring (body : List Xsh.Syntax.Statement) : Xsh.Syntax.Statement := .defer .error (.valueBlock body)
def block (body : List Xsh.Syntax.Statement) : Xsh.Syntax.Statement := .expression (.valueBlock body)
def failing (text : String) : Xsh.Syntax.Statement := .assert (.literal (.bool false)) (some (.literal (.string text)))

def runLong (body : List Xsh.Syntax.Statement) : Exec witnessWorld := runStatements machine 80 start body

def countdown : List Xsh.Syntax.Statement :=
  [varOf totalName "0", bump totalName (.call (ident sumName) [.positional (int "2")])]

/-- The same machine with every call in its caller's activation. -/
def sharing : Machine witnessWorld := { machine with activationStep := 0 }

/-- `sum(2)`, whose block body declares `var acc = n` before recursing, is
`2 + 1 + 0`: each call has its own `acc`. -/
theorem recursion_fresh_cells : readInt totalName (runLong countdown) = some 3 := by decide +kernel

/-- Sharing the activation makes the inner call find its caller's `acc` cell
occupied, so the call is stuck. -/
theorem shared_activation_rejected : readInt totalName (runStatements sharing 80 start countdown) = none := by
  decide +kernel

def perIteration : List Xsh.Syntax.Statement :=
  [varOf totalName "0", .forStmt (.name iName) (items ["1", "2", "7"]) [deferring [bump totalName (int "1")]]]

/-- A `defer` in a loop body runs once per iteration. -/
theorem defer_per_iteration : readInt totalName (runLong perIteration) = some 3 := by decide +kernel

def returnFirst : List Xsh.Syntax.Statement :=
  [varOf xName "1", block [deferring [set xName "2"], .returnValue (some (ident xName))]]

/-- `return x` computes its value before the block's defers run: the block
returns `1` and the defer then sets `x` to `2`. -/
theorem return_before_defers : returnedInt (runLong returnFirst) = some 1 ∧ readAny xName (runLong returnFirst) = some 2 := by
  decide +kernel

def callReturnFirst : List Xsh.Syntax.Statement :=
  [varOf totalName "0", bump totalName (.call (ident fName) [])]

/-- The same through a block function body: `f()` returns `1`. -/
theorem call_return_before_defers : readInt totalName (runLong callReturnFirst) = some 1 := by decide +kernel

def errdeferClean : List Xsh.Syntax.Statement := [varOf totalName "0", block [erring [set totalName "1"]]]
def errdeferFailing : List Xsh.Syntax.Statement :=
  [varOf totalName "0", block [erring [set totalName "1"], failing "original"]]

/-- `errdefer` is skipped on a normal exit and runs on a failure exit. -/
theorem errdefer_on_error_only :
    readInt totalName (runLong errdeferClean) = some 0 ∧ readAny totalName (runLong errdeferFailing) = some 1 := by
  decide +kernel

def lifoBody : List Xsh.Syntax.Statement := [deferring [set xName "1"], deferring [set xName "2"]]
def lifo : List Xsh.Syntax.Statement := [varOf xName "0", block lifoBody]

/-- Two defers run newest first, so the first one registered writes last. -/
theorem defers_newest_first : readInt xName (runLong lifo) = some 1 := by decide +kernel

def breakDefer : List Xsh.Syntax.Statement :=
  [varOf totalName "0", .forStmt (.name iName) (items ["1", "2"]) [deferring [bump totalName (int "1")], .breakStmt none]]

/-- A `break` still runs the iteration's defer. -/
theorem defer_on_break : readInt totalName (runLong breakDefer) = some 1 := by decide +kernel

def precedence : List Xsh.Syntax.Statement := [block [deferring [failing "cleanup"], failing "original"]]

/-- The original failure stays primary and the cleanup failure is reported. -/
theorem original_failure_primary :
    primaryText (runLong precedence) = some "original" ∧ reportedTexts (runLong precedence) = ["cleanup"] := by
  decide +kernel

/-- Leave a block running its actions in registration order. -/
def fifoBlock (outer : Frame witnessWorld) (body : List Xsh.Syntax.Statement) : Exec witnessWorld :=
  let inner := runStatements machine 60 outer.enter body
  withinScope outer (runDeferred machine 60 inner (pendingOf inner).reverse)

/-- Leave a block running every action, `errdefer` included, as a `defer`. -/
def eagerBlock (outer : Frame witnessWorld) (body : List Xsh.Syntax.Statement) : Exec witnessWorld :=
  let inner := runStatements machine 60 outer.enter body
  withinScope outer (runDeferred machine 60 inner ((pendingOf inner).map fun entry => { entry with kind := .exit }))

/-- A `for` whose iterations skip their defers on `break`. -/
def skippingFor (frame : Frame witnessWorld) (target : Xsh.Syntax.BindingTarget) :
    List (Value Unit Unit Unit) → List Xsh.Syntax.Statement → Exec witnessWorld
  | [], _ => .normal frame
  | item :: rest, body =>
    let inner := (bindTarget machine frame.enter .immutable target item).andThen fun bound =>
      runStatements machine 60 bound body
    match inner with
    | .broke last => .normal (frame.restore last)
    | _ => (leaveBlock machine 60 frame inner).iterate fun next => skippingFor next target rest body

/-- An unwinding in which a cleanup failure replaces the original one. -/
def masking : Exec witnessWorld → Exec witnessWorld
  | .failed primary frame =>
    match frame.reported with
    | cleanup :: _ => .failed cleanup frame
    | [] => .failed primary frame
  | other => other

/-- Running the defers in registration order leaves `x` as `2`. -/
theorem fifo_rejected : readInt xName (fifoBlock (afterVar xName "0") lifoBody) = some 2 := by decide +kernel

/-- Skipping the defer on `break` never counts. -/
theorem skipped_on_break_rejected :
    readInt totalName (skippingFor (afterVar totalName "0") (.name iName) (values ["1", "2"])
      [deferring [bump totalName (int "1")], .breakStmt none]) = some 0 := by decide +kernel

/-- Running `errdefer` on a normal exit sets the flag. -/
theorem errdefer_on_success_rejected :
    readInt totalName (eagerBlock (afterVar totalName "0") [erring [set totalName "1"]]) = some 1 := by decide +kernel

/-- Letting the cleanup failure win makes it primary. -/
theorem masking_rejected : primaryText (masking (runLong precedence)) = some "cleanup" := by decide +kernel

def errStart : Frame witnessWorld := { start with locals := [(errName, .err (.scalar (.string "boom")))] }

def runErr (body : List Xsh.Syntax.Statement) : Exec witnessWorld := runStatements machine 80 errStart body

def errText (target : Xsh.Syntax.RawName) (outcome : Exec witnessWorld) : Option String :=
  match frameOf outcome with
  | some frame =>
    match frame.environment.cellOf target with
    | .ok cell =>
      (frame.store cell).bind fun found =>
        match found.contents.value with
        | .err (.scalar (.string text)) => some text
        | _ => none
    | .error _ => none
  | none => none

def okInt (target : Xsh.Syntax.RawName) (outcome : Exec witnessWorld) : Option Int :=
  match frameOf outcome with
  | some frame =>
    match frame.environment.cellOf target with
    | .ok cell =>
      (frame.store cell).bind fun found =>
        match found.contents.value with
        | .ok (.scalar (.integer value)) => some value.val
        | _ => none
    | .error _ => none
  | none => none

def runtimeFailure : Exec witnessWorld → Option String
  | .failed (.runtime (.scalar (.string text))) _ => some text
  | _ => none

def letR (value : Xsh.Syntax.Expr) : Xsh.Syntax.Statement := .binding .letBinding (.name rName) none value

def questionSkips : List Xsh.Syntax.Statement :=
  [varOf totalName "0", varOf seenName "0",
    letR (.capture [erring [set totalName "1"], .expression (.tryExpr (ident errName)), set seenName "1"])]

/-- `?` on an `Err` skips the rest of the `try` body, runs its `errdefer`,
and the `try` is that `Err`. -/
theorem question_skips_rest :
    errText rName (runErr questionSkips) = some "boom" ∧ readInt totalName (runErr questionSkips) = some 1 ∧
      readInt seenName (runErr questionSkips) = some 0 := by
  decide +kernel

def capturesRaised : List Xsh.Syntax.Statement := [letR (.capture [failing "raised"])]
def passesRuntime : List Xsh.Syntax.Statement :=
  [varOf seenName "0", letR (.capture [.expression (.index (.list []) (int "0") false)]), set seenName "1"]

/-- `try` captures a failed assertion as `Err`, but an `index-out-of-range`
passes through it and stops the statements after it. -/
theorem try_captures_raised_only :
    errText rName (runErr capturesRaised) = some "raised" ∧
      runtimeFailure (runErr passesRuntime) = some "index-out-of-range" ∧ readAny seenName (runErr passesRuntime) = some 0 := by
  decide +kernel

def retrying : List Xsh.Syntax.Statement :=
  [varOf attemptsName "0", varOf cleanupsName "0",
    letR (.retry .delays [.literal (.duration "1ms"), .literal (.duration "1ms")] none
      [bump attemptsName (int "1"), erring [bump cleanupsName (int "1")],
        .assert (.binary .gt (ident attemptsName) (int "2")) none, .expression (ident attemptsName)])]

def waitedCount : Exec witnessWorld → Nat
  | .normal frame => frame.waited.length
  | _ => 0

/-- `retry [1ms, 1ms]` whose third attempt succeeds runs the attempt
`errdefer` twice, waits both delays, and is `Ok(3)`. -/
theorem retry_third_attempt :
    okInt rName (runErr retrying) = some 3 ∧ readInt attemptsName (runErr retrying) = some 3 ∧
      readInt cleanupsName (runErr retrying) = some 2 ∧ waitedCount (runErr retrying) = 2 := by
  decide +kernel

def resultCleanup : List Xsh.Syntax.Statement := [letR (.call (ident resultFnName) [])]
def plainCleanup : List Xsh.Syntax.Statement := [letR (.call (ident plainFnName) [])]
def plainCleanupTried : List Xsh.Syntax.Statement := [letR (.capture [.expression (.call (ident plainFnName) [])])]

/-- A cleanup failure in a `Result` function is the `Err` its caller binds; in
any other function it fails the call, and `try` captures it. -/
theorem cleanup_failure_boundary :
    errText rName (runErr resultCleanup) = some "cleanup" ∧ primaryText (runErr plainCleanup) = some "cleanup" ∧
      errText rName (runErr plainCleanupTried) = some "cleanup" := by
  decide +kernel

def tailErr : List Xsh.Syntax.Statement := [letR (.call (ident tailErrName) [.positional (ident errName)])]
def tailErrBlock : List Xsh.Syntax.Statement :=
  [varOf totalName "0", letR (.valueBlock [erring [set totalName "1"], .expression (ident errName)])]

/-- A function whose tail is an `Err` leaves its body with an error, so its
`errdefer` runs (and its failure becomes the `Err`); a value block whose tail
is an `Err` leaves normally and its `errdefer` does not run. -/
theorem tail_err_function_and_block :
    errText rName (runErr tailErr) = some "ran" ∧ errText rName (runErr tailErrBlock) = some "boom" ∧
      readInt totalName (runErr tailErrBlock) = some 0 := by
  decide +kernel

/-- A `try` that also captures runtime failures. -/
def greedyCapture : Valued witnessWorld → Valued witnessWorld
  | .exec (.failed (.runtime error) frame) => .value (.err error) frame
  | other => Xsh.Semantics.CoreControl.capture other

def capturedValue : Valued witnessWorld → Bool
  | .value _ _ => true
  | _ => false

/-- The greedy `try` turns the `index-out-of-range` into a value, which SPEC
8.8 forbids. -/
theorem greedy_try_rejected :
    capturedValue (greedyCapture (valueScope machine 60 errStart [.expression (.index (.list []) (int "0") false)])) = true ∧
      capturedValue (Xsh.Semantics.CoreControl.capture
        (valueScope machine 60 errStart [.expression (.index (.list []) (int "0") false)])) = false := by
  decide +kernel

/-- `tailErr`'s body left as if its `Err` tail were a normal exit: the
`errdefer` is skipped and the caller sees `boom`. -/
def tailAsNormal : Option String :=
  let frame : Frame witnessWorld :=
    Frame.ofScope ⟨emptyLexicalEnvironment, []⟩ (fun _ => none) [(pName, .err (.scalar (.string "boom")))]
      (some (fun _ => none)) ⟨1, [], none⟩
  match runStatements machine 60 frame.enter [erring [failing "ran"]] with
  | .normal inner =>
    match leaveBlock machine 60 frame (.normal inner) with
    | .normal _ => some "boom"
    | .failed (.raised (.scalar (.string text))) _ => some text
    | _ => none
  | _ => none

theorem tail_err_errdefer_skip_rejected : tailAsNormal = some "boom" := by decide +kernel

/-- A `retry` whose attempts drop their defers without running them. -/
def skippingRetry (frame : Frame witnessWorld) (body : List Xsh.Syntax.Statement) :
    List (Value Unit Unit Unit) → Exec witnessWorld
  | [] =>
    match runValueTail machine 60 frame.enter body with
    | .value _ inner | .exec (.failed (.raised _) inner) => .normal (frame.restore inner)
    | _ => .stuck
  | _ :: rest =>
    match runValueTail machine 60 frame.enter body with
    | .value _ inner => .normal (frame.restore inner)
    | .exec (.failed (.raised _) inner) => skippingRetry (frame.restore inner) body rest
    | _ => .stuck

def retryBody : List Xsh.Syntax.Statement :=
  [bump attemptsName (int "1"), erring [bump cleanupsName (int "1")],
    .assert (.binary .gt (ident attemptsName) (int "2")) none, .expression (ident attemptsName)]

def afterTwoVars : Frame witnessWorld :=
  match runStatements machine 20 start [varOf attemptsName "0", varOf cleanupsName "0"] with
  | .normal frame => frame
  | _ => start

/-- Skipping the attempt defers never counts a cleanup. -/
theorem retry_skipping_defers_rejected :
    readInt cleanupsName (skippingRetry afterTwoVars retryBody [integer intOne, integer intOne]) = some 0 := by
  decide +kernel

/-- The `Result` function treated as a plain one: its cleanup failure escapes
as a call failure instead of the `Err` its caller binds. -/
def plainKinded : Machine witnessWorld :=
  { machine with
    context := { program with
      functions := fun called =>
        if called = resultFnName then
          some (⟨emptyLexicalEnvironment, []⟩, [], .block [deferring [failing "cleanup"], .expression (int "7")] .plain)
        else program.functions called } }

theorem cleanup_escape_rejected :
    primaryText (runStatements plainKinded 80 errStart resultCleanup) = some "cleanup" ∧
      errText rName (runStatements plainKinded 80 errStart resultCleanup) = none := by
  decide +kernel

def okLocalName : Xsh.Syntax.RawName := name 51 (by decide)
def mapLocalName : Xsh.Syntax.RawName := name 52 (by decide)
def bytesLocalName : Xsh.Syntax.RawName := name 53 (by decide)
def okTrueName : Xsh.Syntax.RawName := name 54 (by decide)
def kName : Xsh.Syntax.RawName := name 55 (by decide)
def vName : Xsh.Syntax.RawName := name 56 (by decide)
def wholeName : Xsh.Syntax.RawName := name 57 (by decide)
def okCtor : Xsh.Syntax.RawName := name 58 (by decide)
def errCtor : Xsh.Syntax.RawName := name 59 (by decide)
def xsName : Xsh.Syntax.RawName := name 60 (by decide)
def uintMapName : Xsh.Syntax.RawName := name 61 (by decide)
def hName : Xsh.Syntax.RawName := name 62 (by decide)

def richResolution : Oracles witnessWorld :=
  { resolution with
    declaration := fun environment declared =>
      let binder : BinderId := ⟨⟨(declared.word.val * 64 + environment.names.length) % 4294967296, Nat.mod_lt _ (by decide)⟩⟩
      some (binder, ⟨0, binder⟩,
        if declared = xsName then .list (.primitive .any)
        else if declared = uintMapName then .map (.primitive .unsignedInteger) (.primitive .any)
        else .primitive .any) }

def richProgram : Context Unit Unit Unit :=
  { program with
    spelling := fun named =>
      if named = okCtor then "Ok" else if named = errCtor then "Err" else if named = kName then "key"
      else if named = vName then "value" else program.spelling named
    functions := fun called =>
      if called = hName then some (⟨emptyLexicalEnvironment, []⟩, [pName], .expression (.tryExpr (ident pName)) .result)
      else program.functions called }

def richMachine : Machine witnessWorld := { machine with context := richProgram, resolution := richResolution }

def keyOne : MapKey := .integer intOne
def keyTwo : MapKey := .integer ⟨2, by decide⟩

def richStart : Frame witnessWorld :=
  { start with locals := [
      (errName, .err (.scalar (.string "boom"))), (okLocalName, .ok (integer ⟨7, by decide⟩)),
      (mapLocalName, .map [(keyTwo, integer ⟨8, by decide⟩), (keyOne, integer ⟨7, by decide⟩)]),
      (bytesLocalName, .scalar (.bytes [1, 2, 7])), (okTrueName, .ok (.scalar (.boolean true)))] }

def runRich (body : List Xsh.Syntax.Statement) : Exec witnessWorld := runStatements richMachine 80 richStart body

def resolvesAny (target : Xsh.Syntax.RawName) (outcome : Exec witnessWorld) : Bool :=
  match frameOf outcome with
  | some frame =>
    match frame.environment.cellOf target with
    | .ok _ => true
    | .error _ => false
  | none => false

def call (callee : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.Expr) : Xsh.Syntax.Expr :=
  .call (ident callee) (arguments.map .positional)

def constructors : List Xsh.Syntax.Statement :=
  [letR (call okCtor [int "7"]), .binding .letBinding (.name vName) none (call errCtor [.literal (.string "x")])]

/-- `Ok(7)` and `Err("x")` construct results. -/
theorem constructors_build :
    okInt rName (runRich constructors) = some 7 ∧ errText vName (runRich constructors) = some "x" := by
  decide +kernel

def expressionResult : List Xsh.Syntax.Statement := [letR (call hName [ident errName])]

/-- An expression-bodied `Result` function returns its `?` failure as `Err`. -/
theorem expression_result_kind : errText rName (runRich expressionResult) = some "boom" := by decide +kernel

def fallbackHandler : List Xsh.Syntax.Statement :=
  [letR (.binary .resultFallback (ident errName) (.handlerBlock (some kName) [.expression (ident kName)])),
    .binding .letBinding (.name vName) none (.binary .resultFallback (ident okLocalName) (int "1"))]

/-- `??` gives an `Ok` payload, or runs its handler with the error bound only
inside it. -/
theorem fallback_and_handler :
    readInt vName (runRich fallbackHandler) = some 7 ∧ resolvesAny kName (runRich fallbackHandler) = false := by
  decide +kernel

def handlerText (outcome : Exec witnessWorld) : Option String :=
  match frameOf outcome with
  | some frame =>
    match frame.environment.cellOf rName with
    | .ok cell =>
      (frame.store cell).bind fun found =>
        match found.contents.value with
        | .scalar (.string text) => some text
        | _ => none
    | .error _ => none
  | none => none

theorem handler_gets_error : handlerText (runRich fallbackHandler) = some "boom" := by decide +kernel

/-- A handler whose parameter stays bound after it. -/
def leakyHandler : Exec witnessWorld :=
  match bindCaptures richMachine richStart.enter [(kName, .err (.scalar (.string "boom")))] with
  | .normal bound => .normal bound
  | other => other

theorem leaky_handler_rejected : resolvesAny kName leakyHandler = true := by decide +kernel

def resultCondition : List Xsh.Syntax.Statement :=
  [varOf totalName "0", .ifStmt [(ident okTrueName, [set totalName "1"])] none]
def errCondition : List Xsh.Syntax.Statement := [.ifStmt [(ident errName, [])] none]

/-- `Ok(true)` selects a branch; an `Err` condition fails as under `?`. -/
theorem result_conditions :
    readInt totalName (runRich resultCondition) = some 1 ∧ primaryText (runRich errCondition) = some "boom" := by
  decide +kernel

def guarded : List Xsh.Syntax.Statement :=
  [varOf totalName "0", .guardStmt (.name aName) none (ident okLocalName) [.returnValue (some (int "0"))],
    .assign (.name totalName) .set (ident aName)]
def guardedElse : List Xsh.Syntax.Statement :=
  [.guardStmt (.name aName) none (ident errName) [.returnValue (some (int "0"))], .returnValue (some (int "1"))]

/-- `guard let` binds in the enclosing block, visible after it; on `Err` the
else block runs with nothing bound and leaves. -/
theorem guard_let_binds :
    readInt totalName (runRich guarded) = some 7 ∧ returnedInt (runRich guardedElse) = some 0 ∧
      resolvesAny aName (runRich guardedElse) = false := by
  decide +kernel

/-- A `guard let` whose binding is scoped to the guard itself. -/
def hiddenGuard : Exec witnessWorld :=
  match runStatements richMachine 20 richStart [varOf totalName "0"] with
  | .normal frame =>
    match bindTarget richMachine frame.enter .immutable (.name aName) (integer ⟨7, by decide⟩) with
    | .normal bound => runStatements richMachine 20 (frame.restore bound) [.assign (.name totalName) .set (ident aName)]
    | other => other
  | other => other

theorem hidden_guard_rejected : readInt totalName hiddenGuard = none := by decide +kernel

def mapLoop : List Xsh.Syntax.Statement :=
  [varOf totalName "0", varOf xName "0",
    .forStmt (.record [(kName, .name kName), (vName, .name vName)] false) (ident mapLocalName)
      [bump totalName (ident vName), .assign (.name xName) .set (ident kName)]]

/-- `for {key, value} in map` visits items in key order: the values sum to 15
and the last key seen is 2, though the map stores key 2 first. -/
theorem map_loop_in_key_order :
    readInt totalName (runRich mapLoop) = some 15 ∧ readInt xName (runRich mapLoop) = some 2 ∧
      resolvesAny kName (runRich mapLoop) = false := by
  decide +kernel

def storedOrder : Option Int :=
  match runStatements richMachine 20 richStart [varOf xName "0"] with
  | .normal frame =>
    match runFor richMachine 40 frame (.record [(kName, .name kName), (vName, .name vName)] false)
        [.record [("key", integer ⟨2, by decide⟩), ("value", integer ⟨8, by decide⟩)],
          .record [("key", integer intOne), ("value", integer ⟨7, by decide⟩)]]
        [.assign (.name xName) .set (ident kName)] with
    | outcome => readInt xName outcome
  | _ => none

/-- Iterating in stored order would leave the last key as 1. -/
theorem stored_order_rejected : storedOrder = some 1 := by decide +kernel

def textAndBytes : List Xsh.Syntax.Statement :=
  [varOf totalName "0", varOf seenName "0",
    .forStmt (.name iName) (.literal (.string "ab")) [bump seenName (int "1")],
    .forStmt (.name iName) (ident bytesLocalName) [bump totalName (ident iName)]]

/-- `for` over a `Str` visits its scalars and over `Bytes` its byte values. -/
theorem text_and_bytes_loops :
    readInt seenName (runRich textAndBytes) = some 2 ∧ readInt totalName (runRich textAndBytes) = some 10 := by
  decide +kernel

def matchForms : List Xsh.Syntax.Statement :=
  [varOf totalName "0", varOf seenName "0",
    .matchStmt (ident okLocalName) [(.alias (.constructor okCtor (some (.binding vName))) wholeName, none,
      [.assign (.name totalName) .set (ident vName)])],
    .matchStmt (int "8") [(.alternation [.literal (.integer "1"), .literal (.integer "8")], none, [set seenName "1"])]]

/-- `Ok(v) as whole` and `1 | 8` match; the arm's captures do not outlive it. -/
theorem match_alias_alternation :
    readInt totalName (runRich matchForms) = some 7 ∧ readInt seenName (runRich matchForms) = some 1 ∧
      resolvesAny vName (runRich matchForms) = false ∧ resolvesAny wholeName (runRich matchForms) = false := by
  decide +kernel

/-- A `match` arm whose captures stay bound after it. -/
def leakyArm : Exec witnessWorld :=
  match matchPattern richMachine.context richMachine.typeTest (.alias (.constructor okCtor (some (.binding vName))) wholeName)
      (.ok (integer ⟨7, by decide⟩)) with
  | .matched captures => bindCaptures richMachine richStart.enter captures
  | _ => .stuck

theorem leaky_arm_rejected : resolvesAny wholeName leakyArm = true := by decide +kernel

def ifLetOk : List Xsh.Syntax.Statement :=
  [varOf totalName "0",
    .ifStmt [(.patternCondition (ident okLocalName) (.constructor okCtor (some (.binding vName))),
      [.assign (.name totalName) .set (ident vName)])] none]

/-- `if let Ok(v) = result` binds `v` only in its branch. -/
theorem if_let_constructor :
    readInt totalName (runRich ifLetOk) = some 7 ∧ resolvesAny vName (runRich ifLetOk) = false := by decide +kernel

/-- An `if let` whose capture stays bound after the branch. -/
def leakyIfLet : Exec witnessWorld :=
  (bindCaptures richMachine richStart.enter [(vName, integer ⟨7, by decide⟩)]).andThen fun bound =>
    runStatements richMachine 20 bound []

theorem leaky_if_let_rejected : resolvesAny vName leakyIfLet = true := by decide +kernel

def breakValue : List Xsh.Syntax.Statement := [letR (.loopExpr [.breakStmt (some (int "7"))])]

/-- `loop { break 7 }` is `7`. -/
theorem break_with_value : readInt rName (runRich breakValue) = some 7 := by decide +kernel

def indexWrite : List Xsh.Syntax.Statement :=
  [.binding .var (.name xsName) none (.list [int "1", int "2"]),
    .assign (.index (.name xsName) (int "0")) .set (int "7"),
    .binding .letBinding (.name rName) none (.index (ident xsName) (int "0") false)]
def unsignedKey : List Xsh.Syntax.Statement :=
  [.binding .var (.name uintMapName) none (ident mapLocalName),
    .assign (.index (.name uintMapName) (.unary .neg (int "1"))) .set (int "7")]

/-- `xs[0] = 7` writes the item; `m[-1] = 7` on a `Map[UInt, Any]` fails at
its key with the `Schema` error. -/
theorem index_assignment :
    readInt rName (runRich indexWrite) = some 7 ∧ primaryText (runRich unsignedKey) = some "schema" := by
  decide +kernel

/-- A block that keeps its declarations after it. -/
def leakyBlock : Exec witnessWorld :=
  match runStatements richMachine 20 richStart [letX "1"] with
  | .normal frame => runStatements richMachine 20 frame.enter [letX "2"]
  | other => other

/-- The leaking block leaves `x` at its inner `2`. -/
theorem leaky_block_rejected : readInt xName leakyBlock = some 2 := by decide +kernel

def whileLet : List Xsh.Syntax.Statement :=
  [varOf totalName "0",
    .whileStmt (.patternCondition (ident okLocalName) (.constructor okCtor (some (.binding vName))))
      [.assign (.name totalName) .set (ident vName), .breakStmt none]]

/-- `while let Ok(v) = result` binds `v` in each iteration only. -/
theorem while_let_scoped :
    readInt totalName (runRich whileLet) = some 7 ∧ resolvesAny vName (runRich whileLet) = false := by
  decide +kernel

/-- A `while let` whose capture stays bound after the iteration. -/
def leakyWhileLet : Exec witnessWorld :=
  (bindCaptures richMachine richStart.enter [(vName, integer ⟨7, by decide⟩)]).andThen fun bound =>
    runStatements richMachine 20 bound [.breakStmt none]

theorem leaky_while_let_rejected : resolvesAny vName leakyWhileLet = true := by decide +kernel

def baseName : Xsh.Syntax.RawName := name 70 (by decide)
def addName : Xsh.Syntax.RawName := name 71 (by decide)
def factName : Xsh.Syntax.RawName := name 72 (by decide)
def dSpec : Xsh.Syntax.RawName := name 73 (by decide)
def pairCtor : Xsh.Syntax.RawName := name 74 (by decide)
def missingVariant : Xsh.Syntax.RawName := name 75 (by decide)
def pathLabel : Xsh.Syntax.RawName := name 76 (by decide)
def missingFacet : Xsh.Syntax.RawName := name 77 (by decide)
def familyName : Xsh.Syntax.RawName := name 78 (by decide)
def failureName : Xsh.Syntax.RawName := name 79 (by decide)
def errorLocalName : Xsh.Syntax.RawName := name 80 (by decide)
def pairLocalName : Xsh.Syntax.RawName := name 81 (by decide)

def fullProgram : Context Unit Unit Unit :=
  { richProgram with
    spelling := fun named =>
      if named = dSpec then "d" else if named = pairCtor then "Pair" else if named = missingVariant then "Missing"
      else if named = pathLabel then "path" else if named = missingFacet then "NotFound" else richProgram.spelling named }

def fullMachine : Machine witnessWorld := { richMachine with context := fullProgram }

def missingError : Value Unit Unit Unit :=
  .error (ErrorData.mk () "Missing" "fs" "missing" [("path", .scalar (.string "/x"))] ["NotFound"] none [] none none)

def fullStart : Frame witnessWorld :=
  { richStart with locals := richStart.locals ++ [(errorLocalName, missingError),
      (pairLocalName, .tagged () "Pair" [integer intOne, integer ⟨2, by decide⟩])] }

def runFull (body : List Xsh.Syntax.Statement) : Exec witnessWorld := runStatements fullMachine 90 fullStart body

def declareAdd : Xsh.Syntax.Statement :=
  .functionDef .pure addName [.mk pName none none false] none none
    [.expression (.binary .add (ident pName) (ident baseName))] none false

def capturing : List Xsh.Syntax.Statement :=
  [varOf baseName "1", declareAdd, set baseName "2", letR (call addName [int "1"])]

/-- A local function reads its captured `var` as it is at the call: after
`base = 2`, `add(1)` is `3`. -/
theorem closure_reads_at_call : readInt rName (runFull capturing) = some 3 := by decide +kernel

def scopedFunction : List Xsh.Syntax.Statement :=
  [varOf baseName "1", block [declareAdd], letR (call addName [int "1"])]

/-- A function declared in a block is not callable after it. -/
theorem function_scoped : readAny rName (runFull scopedFunction) = none := by decide +kernel

/-- The leaking declaration keeps `add` bound after its block. -/
def leakyFunction : Exec witnessWorld :=
  match runStatements fullMachine 30 fullStart [varOf baseName "1"] with
  | .normal frame =>
    (runStatement fullMachine 30 frame.enter declareAdd).andThen fun declared =>
      runStatements fullMachine 30 declared [letR (call addName [int "1"])]
  | other => other

theorem leaky_function_rejected : readInt rName leakyFunction = some 2 := by decide +kernel

/-- The hiding declaration binds `add` only for itself, so a call after it in
the same block fails. -/
def hiddenFunction : Exec witnessWorld :=
  match runStatements fullMachine 30 fullStart [varOf baseName "1"] with
  | .normal frame =>
    (runStatement fullMachine 30 frame declareAdd).andThen fun declared =>
      runStatements fullMachine 30 { declared with closures := frame.closures } [letR (call addName [int "1"])]
  | other => other

theorem hidden_function_rejected : readInt rName hiddenFunction = none ∧ readInt rName (runFull capturing) = some 3 := by
  decide +kernel

def declareFact : Xsh.Syntax.Statement :=
  .functionDef .pure factName [.mk pName none none false] none none
    [.ifStmt [(.binary .eq (ident pName) (int "0"), [.returnValue (some (int "1"))])] none,
      .expression (.binary .mul (ident pName) (call factName [.binary .sub (ident pName) (int "1")]))] none false

/-- A local function sees its own name: `fact(2)` is `2`. -/
theorem local_recursion : readInt rName (runFull [declareFact, letR (call factName [int "2"])]) = some 2 := by
  decide +kernel

def implicitItem : List Xsh.Syntax.Statement :=
  [letR (.binary .resultFallback (ident errName) (.handlerBlock none [.expression .item]))]

/-- A handler without a parameter reads the error as `.`, which means nothing
after the handler. -/
theorem implicit_item_handler :
    handlerText (runFull implicitItem) = some "boom" ∧
      readAny rName (runFull [letR .item]) = none := by
  decide +kernel

/-- A handler whose `.` stays set after it. -/
def leakyItem : Exec witnessWorld := runStatements fullMachine 30 { fullStart with item := some (integer intOne) } [letR .item]

theorem leaky_item_rejected : readInt rName leakyItem = some 1 := by decide +kernel

def comprehension : List Xsh.Syntax.Statement :=
  [letR (.listComp (.binary .mul (ident iName) (int "2"))
    [.each (.name iName) (items ["1", "2"]), .when (.binary .gt (ident iName) (int "1"))])]

def listInts (target : Xsh.Syntax.RawName) (outcome : Exec witnessWorld) : Option (List Int) :=
  match frameOf outcome with
  | some frame =>
    match frame.environment.cellOf target with
    | .ok cell =>
      (frame.store cell).bind fun found =>
        match found.contents.value with
        | .list values => values.mapM fun value =>
          match value with
          | .scalar (.integer item) => some item.val
          | _ => none
        | _ => none
    | .error _ => none
  | none => none

/-- `[i * 2 for i in [1, 2] when i > 1]` is `[4]`, and `i` is not bound after
it. -/
theorem comprehension_scoped :
    listInts rName (runFull comprehension) = some [4] ∧ resolvesAny iName (runFull comprehension) = false := by
  decide +kernel

/-- A comprehension that leaves its target bound. -/
def leakyComprehension : Exec witnessWorld := bindTarget fullMachine fullStart.enter .immutable (.name iName) (integer intOne)

theorem leaky_comprehension_rejected : resolvesAny iName leakyComprehension = true := by decide +kernel

def collecting : List Xsh.Syntax.Statement :=
  [letR (.collect [.forStmt (.name iName) (items ["1", "2"]) [.yield (ident iName)]])]

/-- `collect { for i in [1, 2] { yield i } }` is `[1, 2]`; a `yield` outside
any `collect` has no meaning. -/
theorem collect_yields :
    listInts rName (runFull collecting) = some [1, 2] ∧
      readAny rName (runFull [.yield (int "1")]) = none := by
  decide +kernel

def textMatch : List Xsh.Syntax.Statement :=
  [varOf rName "0", .matchStmt (.literal (.string "a=b=c"))
    [(.text [.hole (some kName) none, .text "=", .hole (some vName) none], none,
      [.assign (.name rName) .set (ident vName)])]]
def typedHole : List Xsh.Syntax.Statement :=
  [varOf rName "0", .matchStmt (.literal (.string "42 items")) [(.text [.hole (some kName) (some dSpec), .text " items"], none,
    [.assign (.name rName) .set (ident kName)])]]

/-- `f"{key}={value}"` splits `a=b=c` at its first `=`, and `{n:d}` binds an
`Int`; the holes are not bound after the arm. -/
theorem text_patterns :
    readInt rName (runFull typedHole) = some 42 ∧ resolvesAny vName (runFull textMatch) = false := by
  decide +kernel

/-- The split itself: `a=b=c` gives the second hole `b=c`. -/
theorem text_split_first : handlerText (runFull textMatch) = some "b=c" := by decide +kernel

def errorPatterns : List Xsh.Syntax.Statement :=
  [varOf totalName "0", varOf seenName "0",
    .matchStmt (ident errorLocalName) [(.errorVariant familyName missingVariant [(pathLabel, .binding vName)], none,
      [set totalName "1"])],
    .matchStmt (ident errorLocalName) [(.facet missingFacet, none, [set seenName "1"])],
    .matchStmt (ident pairLocalName) [(.constructor pairCtor (some (.tuple [.binding kName, .binding vName])), none,
      [.binding .letBinding (.name rName) none (.binary .add (ident kName) (ident vName))])]]

/-- `Family.Missing { path }`, `is NotFound` and `Pair(a, b)` match, and their
captures end with their arm. -/
theorem error_and_tuple_patterns :
    readInt totalName (runFull errorPatterns) = some 1 ∧ readInt seenName (runFull errorPatterns) = some 1 ∧
      resolvesAny vName (runFull errorPatterns) = false := by
  decide +kernel

def guardFailure : List Xsh.Syntax.Statement :=
  [.guardHandled (.name aName) none (ident errName) failureName [.returnValue (some (ident failureName))]]

def returnedText : Exec witnessWorld → Option String
  | .returned (.scalar (.string text)) _ => some text
  | _ => none

/-- `guard let a = result else { |failure| return failure }` binds the error
only inside the else block. -/
theorem guard_failure_parameter :
    returnedText (runFull guardFailure) = some "boom" ∧ resolvesAny failureName (runFull guardFailure) = false := by
  decide +kernel

/-- An else block whose failure parameter stays bound after it. -/
def leakyFailure : Exec witnessWorld :=
  bindCaptures fullMachine fullStart.enter [(failureName, .scalar (.string "boom"))]

theorem leaky_failure_rejected : resolvesAny failureName leakyFailure = true := by decide +kernel

/-- The witness machine with its binders and cells from the canonical
resolver instead of an oracle. -/
def canonicalMachine : Machine witnessWorld :=
  { fullMachine with resolution := canonicalResolution resolution fun _ _ => some (.primitive .any) }

theorem canonical_resolution_runs :
    readInt xName (runStatements canonicalMachine 40 start shadowed) = some 1 ∧
      readInt totalName (runStatements canonicalMachine 80 start countdown) = some 3 ∧
      readInt rName (runStatements canonicalMachine 90 fullStart capturing) = some 3 := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Control
