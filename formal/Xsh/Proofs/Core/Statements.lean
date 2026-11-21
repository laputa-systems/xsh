import Xsh.Proofs.Core.Writes
import Xsh.Semantics.CoreStatements

namespace Xsh.Proofs.Core.Statements

open Xsh.Foundation
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Proofs.Core.Writes

variable {Declaration Identity Source : Type}

section Environments

theorem extend_resolve_other (name : Xsh.Syntax.RawName) :
    ∀ (environment : Environment) (declared : List Declared), name ∉ declared.map Prod.fst →
      resolve (extend environment declared).names name = resolve environment.names name
  | _, [], _ => rfl
  | environment, (other, binder, cell) :: rest, absent => by
    simp only [List.map_cons, List.mem_cons, not_or] at absent
    simp only [extend]
    rw [extend_resolve_other name _ rest absent.2]
    exact resolve_bind_other environment.names other name binder absent.1

theorem extend_resolveCell_other (binder : BinderId) :
    ∀ (environment : Environment) (declared : List Declared), binder ∉ declared.map (·.2.1) →
      resolveCell (extend environment declared).cells binder = resolveCell environment.cells binder
  | _, [], _ => rfl
  | environment, (name, other, cell) :: rest, absent => by
    simp only [List.map_cons, List.mem_cons, not_or] at absent
    simp only [extend]
    rw [extend_resolveCell_other binder _ rest absent.2]
    exact resolveCell_bindCell_other environment.cells other binder cell absent.1

theorem extend_cellOf :
    ∀ (environment : Environment) (declared : List Declared), (declared.map Prod.fst).Nodup →
      (declared.map (·.2.1)).Nodup → ∀ name binder cell, (name, binder, cell) ∈ declared →
        (extend environment declared).cellOf name = .ok cell
  | _, [], _, _, _, _, _, inside => by simp at inside
  | environment, (first, firstBinder, firstCell) :: rest, names, binders, name, binder, cell, inside => by
    simp only [List.map_cons, List.nodup_cons] at names binders
    simp only [List.mem_cons] at inside
    rcases inside with same | later
    · simp only [Prod.mk.injEq] at same
      obtain ⟨rfl, rfl, rfl⟩ := same
      simp only [extend, Environment.cellOf, extend_resolve_other name _ rest names.1, resolve_after_bind,
        extend_resolveCell_other binder _ rest binders.1, resolveCell_bindCell]
    · exact extend_cellOf _ rest names.2 binders.2 name binder cell later

theorem exitScope_bind (names : LexicalEnvironment) (nonempty : names ≠ []) (name : Xsh.Syntax.RawName)
    (binder : BinderId) : exitScope (bind names name binder) = exitScope names ∧ bind names name binder ≠ [] := by
  cases names with
  | nil => exact absurd rfl nonempty
  | cons frame outer => simp [Xsh.Semantics.Environments.bind, exitScope]

theorem extend_scope :
    ∀ (environment : Environment) (declared : List Declared), environment.names ≠ [] →
      exitScope (extend environment declared).names = exitScope environment.names ∧ (extend environment declared).names ≠ []
  | _, [], nonempty => ⟨rfl, nonempty⟩
  | environment, (name, binder, cell) :: rest, nonempty => by
    simp only [extend]
    obtain ⟨same, still⟩ := exitScope_bind environment.names nonempty name binder
    obtain ⟨inner, innerNonempty⟩ := extend_scope ⟨bind environment.names name binder, bindCell environment.cells binder cell⟩
      rest still
    exact ⟨inner.trans same, innerNonempty⟩

end Environments

def IsStoreWrite : Xsh.Syntax.Statement → Prop
  | .binding _ _ _ _ => True
  | .export (.binding _ _ _ _) => True
  | .assign _ _ _ => True
  | _ => False

variable [DecidableEq Declaration]

section Statements

theorem execute_some_iff (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) (statement : Xsh.Syntax.Statement) :
    (execute world decisions raises operate oracles state statement).isSome ↔ IsStoreWrite statement := by
  cases statement with
  | «export» declaration => cases declaration <;> simp [execute, IsStoreWrite]
  | _ => simp [execute, IsStoreWrite]

theorem declareStatement_initializer_fails (world : World Declaration Identity Source) (decisions : Decisions world)
    (oracles : Oracles world) (state : State world) (kind : Xsh.Syntax.BindingKind) (target : Xsh.Syntax.BindingTarget)
    (initializer : Xsh.Syntax.Expr) (failure : Failure Declaration Identity Source) (store : Store world)
    (failed : oracles.evaluate state.environment state.store initializer = .error (failure, store)) :
    declareStatement world decisions oracles state kind target initializer = .failed failure { state with store := store } := by
  simp [declareStatement, failed]

theorem declareStatement_resolved (world : World Declaration Identity Source) (decisions : Decisions world)
    (oracles : Oracles world) (state : State world) (kind : Xsh.Syntax.BindingKind) (target : Xsh.Syntax.BindingTarget)
    (initializer : Xsh.Syntax.Expr) (value : Value Declaration Identity Source) (store : Store world)
    (resolved : Target Declaration) (declared : List Declared)
    (evaluated : oracles.evaluate state.environment state.store initializer = .ok (value, store))
    (found : resolveTarget oracles state.environment target = some (resolved, declared)) :
    declareStatement world decisions oracles state kind target initializer =
      lift { state with store := store } (extend state.environment declared)
        (declareTarget world decisions store (bindingMutability kind) resolved value) := by
  simp [declareStatement, evaluated, found]

theorem declareStatement_commits (world : World Declaration Identity Source) (decisions : Decisions world)
    (oracles : Oracles world) (state next : State world) (kind : Xsh.Syntax.BindingKind)
    (target : Xsh.Syntax.BindingTarget) (initializer : Xsh.Syntax.Expr)
    (committed : declareStatement world decisions oracles state kind target initializer = .completed next) :
    ∃ value store resolved declared, oracles.evaluate state.environment state.store initializer = .ok (value, store) ∧
      resolveTarget oracles state.environment target = some (resolved, declared) ∧
      declareTarget world decisions store (bindingMutability kind) resolved value = .committed next.store ∧
      next.environment = extend state.environment declared ∧ next.overlay = state.overlay := by
  unfold declareStatement at committed
  split at committed
  · simp at committed
  · rename_i value store evaluated
    split at committed
    · simp at committed
    · rename_i resolved declared found
      refine ⟨value, store, resolved, declared, evaluated, found, ?_⟩
      unfold lift at committed
      split at committed
      · rename_i after declaredStep
        simp only [Outcome.completed.injEq] at committed
        subst committed
        exact ⟨declaredStep, rfl, rfl⟩
      · simp at committed
      · simp at committed

theorem declared_names_resolve (environment : Environment) (declared : List Declared)
    (names : (declared.map Prod.fst).Nodup) (binders : (declared.map (·.2.1)).Nodup) :
    ∀ name binder cell, (name, binder, cell) ∈ declared → (extend environment declared).cellOf name = .ok cell :=
  extend_cellOf environment declared names binders

theorem declareStatement_never_malformed (world : World Declaration Identity Source) (decisions : Decisions world)
    (oracles : Oracles world) (state : State world) (kind : Xsh.Syntax.BindingKind) (target : Xsh.Syntax.BindingTarget)
    (initializer : Xsh.Syntax.Expr) (value : Value Declaration Identity Source) (store : Store world)
    (resolved : Target Declaration) (declared : List Declared) (type : ValueType Declaration)
    (evaluated : oracles.evaluate state.environment state.store initializer = .ok (value, store))
    (found : resolveTarget oracles state.environment target = some (resolved, declared))
    (member : Membership.value world.membership type value) (typed : TargetTyped type resolved) :
    ∀ after, declareStatement world decisions oracles state kind target initializer ≠ .internal .malformedTarget after := by
  intro after internal
  rw [declareStatement_resolved world decisions oracles state kind target initializer value store resolved declared
    evaluated found] at internal
  unfold lift at internal
  split at internal
  · simp at internal
  · simp at internal
  · rename_i fault kept step
    simp only [Outcome.internal.injEq] at internal
    rw [internal.1] at step
    exact declareTarget_never_malformed world decisions store (bindingMutability kind) type resolved value member typed
      kept step

theorem assignStatementAt_resolves (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) (target : Xsh.Syntax.AssignTarget) (op : Xsh.Syntax.AssignOp)
    (value : Xsh.Syntax.Expr) (root : Xsh.Syntax.RawName) (path : List PathSpec) (cell : CellId)
    (notEnvironment : ∀ key, target ≠ .env key) (flattened : flatten oracles.spelling target = some (root, path))
    (resolved : state.environment.cellOf root = .ok cell) :
    assignStatementAt world decisions raises operate oracles state target op value =
      lift state state.environment
        (assignStatement world decisions raises operate state.store cell
          (sides oracles state.environment ((state.store cell).map fun found => found.contents.typeTag) path) op
          (fun store => oracles.evaluate state.environment store value)) := by
  cases target with
  | env key => exact absurd rfl (notEnvironment key)
  | _ => simp [assignStatementAt, flattened, resolved]

theorem assignStatementAt_unresolved (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) (target : Xsh.Syntax.AssignTarget) (op : Xsh.Syntax.AssignOp)
    (value : Xsh.Syntax.Expr) (root : Xsh.Syntax.RawName) (path : List PathSpec) (fault : LookupFault)
    (notEnvironment : ∀ key, target ≠ .env key) (flattened : flatten oracles.spelling target = some (root, path))
    (unresolved : state.environment.cellOf root = .error fault) :
    assignStatementAt world decisions raises operate oracles state target op value =
      .internal (.unresolvedName fault) state := by
  cases target with
  | env key => exact absurd rfl (notEnvironment key)
  | _ => simp [assignStatementAt, flattened, unresolved]

theorem assignStatementAt_environment (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) (key : Xsh.Syntax.RawName) (value : Xsh.Syntax.Expr)
    (result : Value Declaration Identity Source) (store : Store world) (bytes : List UInt8)
    (evaluated : oracles.evaluate state.environment state.store value = .ok (result, store))
    (converted : oracles.argv result = some bytes) :
    assignStatementAt world decisions raises operate oracles state (.env key) .set value =
      match setEnvironment raises state.overlay (oracles.spelling key).toUTF8.toList bytes with
      | .ok overlay => .completed { state with store := store, overlay := overlay }
      | .error failure => .failed failure { state with store := store } := by
  simp only [assignStatementAt, evaluated, converted]
  cases setEnvironment raises state.overlay (oracles.spelling key).toUTF8.toList bytes <;> rfl

omit [DecidableEq Declaration] in
theorem lift_store {world : World Declaration Identity Source} (state : State world) (environment : Environment)
    (step : StoreStep world) : (lift state environment step).state.store = step.storeAfter := by
  cases step <;> rfl

end Statements

section Blocks

theorem execute_scope (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) (statement : Xsh.Syntax.Statement) (outcome : Outcome world)
    (nonempty : state.environment.names ≠ [])
    (executed : execute world decisions raises operate oracles state statement = some outcome) :
    exitScope outcome.state.environment.names = exitScope state.environment.names ∧
      outcome.state.environment.names ≠ [] := by
  have declared : ∀ kind target initializer,
      exitScope (declareStatement world decisions oracles state kind target initializer).state.environment.names =
        exitScope state.environment.names ∧
      (declareStatement world decisions oracles state kind target initializer).state.environment.names ≠ [] := by
    intro kind target initializer
    unfold declareStatement
    split
    · exact ⟨rfl, nonempty⟩
    · split
      · exact ⟨rfl, nonempty⟩
      · rename_i resolved declared _
        unfold lift
        split
        · exact extend_scope state.environment declared nonempty
        · exact ⟨rfl, nonempty⟩
        · exact ⟨rfl, nonempty⟩
  have assigned : ∀ target op value,
      (assignStatementAt world decisions raises operate oracles state target op value).state.environment =
        state.environment := by
    intro target op value
    unfold assignStatementAt
    split
    · split
      · split
        · rfl
        · split
          · rfl
          · split <;> rfl
      · rfl
    · split
      · rfl
      · split
        · rfl
        · unfold lift
          split <;> rfl
  cases statement with
  | binding kind target annotation initializer =>
    simp only [execute, Option.some.injEq] at executed
    subst executed
    exact declared kind target initializer
  | «export» declaration =>
    cases declaration with
    | binding kind target annotation initializer =>
      simp only [execute, Option.some.injEq] at executed
      subst executed
      exact declared kind target initializer
    | _ => simp [execute] at executed
  | assign target op value =>
    simp only [execute, Option.some.injEq] at executed
    subst executed
    rw [assigned]
    exact ⟨rfl, nonempty⟩
  | _ => simp [execute] at executed

theorem runAll_scope (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) :
    ∀ (state : State world) (body : List Xsh.Syntax.Statement) (outcome : Outcome world),
      state.environment.names ≠ [] → runAll world decisions raises operate oracles state body = some outcome →
        exitScope outcome.state.environment.names = exitScope state.environment.names
  | state, [], outcome, _, ran => by
    simp only [runAll, Option.some.injEq] at ran
    subst ran
    rfl
  | state, statement :: rest, outcome, nonempty, ran => by
    simp only [runAll] at ran
    cases executed : execute world decisions raises operate oracles state statement with
    | none => simp [executed] at ran
    | some first =>
      obtain ⟨same, still⟩ := execute_scope world decisions raises operate oracles state statement first nonempty executed
      cases first with
      | completed next =>
        simp only [executed] at ran
        exact (runAll_scope world decisions raises operate oracles next rest outcome still ran).trans same
      | failed failure next =>
        simp only [executed, Option.some.injEq] at ran
        subst ran
        exact same
      | internal fault next =>
        simp only [executed, Option.some.injEq] at ran
        subst ran
        exact same

theorem block_restores_names (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) (body : List Xsh.Syntax.Statement) (outcome : Outcome world)
    (ran : block world decisions raises operate oracles state body = some outcome) :
    outcome.state.environment.names = state.environment.names := by
  unfold block at ran
  cases inner : runAll world decisions raises operate oracles
      { state with environment := { state.environment with names := enterScope state.environment.names } } body with
  | none => simp [inner] at ran
  | some result =>
    simp only [inner, Option.map_some, Option.some.injEq] at ran
    subst ran
    have restored := runAll_scope world decisions raises operate oracles _ body result (by simp [enterScope]) inner
    cases result <;> simpa [leave, Outcome.state, enterScope, exitScope] using restored

end Blocks

section Root

/-- The syntax-directed store-write meaning of the frozen core's
store-writing statements, `let`/`var`/`const` (also exported) and assignment
to a name path or an environment variable, over every Core world, given the
name table's spellings, BASE-STEP's expression and index evaluation, the
checker's binder, cell and type for each declared name, and argv conversion.
Only those statements have a store-write meaning. A declaration evaluates its
initializer, resolves its `BindingTarget`, and then is `declareTarget` on the
store the initializer left; on commit each declared name resolves through
`cellOf` to its cell, and a well-typed destructuring never reaches
`malformedTarget`. An assignment resolves its root through `cellOf` and is then
`assignStatement` on its selector path, so every `store_writing_forms` fact
applies to the resulting store; an unresolved root is an internal fault. An
environment assignment converts its value and sets the overlay. A block runs
in a new lexical scope and leaving it restores name resolution. -/
theorem syntax_directed_store_writes (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) :
    (∀ state statement,
      (execute world decisions raises operate oracles state statement).isSome ↔ IsStoreWrite statement) ∧
    (∀ state kind target initializer failure store,
      oracles.evaluate state.environment state.store initializer = .error (failure, store) →
        declareStatement world decisions oracles state kind target initializer =
          .failed failure { state with store := store }) ∧
    (∀ state kind target initializer value store resolved declared,
      oracles.evaluate state.environment state.store initializer = .ok (value, store) →
      resolveTarget oracles state.environment target = some (resolved, declared) →
        declareStatement world decisions oracles state kind target initializer =
          lift { state with store := store } (extend state.environment declared)
            (declareTarget world decisions store (bindingMutability kind) resolved value)) ∧
    (∀ state next kind target initializer,
      declareStatement world decisions oracles state kind target initializer = .completed next →
        ∃ value store resolved declared, oracles.evaluate state.environment state.store initializer = .ok (value, store) ∧
          resolveTarget oracles state.environment target = some (resolved, declared) ∧
          declareTarget world decisions store (bindingMutability kind) resolved value = .committed next.store ∧
          next.environment = extend state.environment declared ∧ next.overlay = state.overlay) ∧
    (∀ environment (declared : List Declared), (declared.map Prod.fst).Nodup → (declared.map (·.2.1)).Nodup →
      ∀ name binder cell, (name, binder, cell) ∈ declared → (extend environment declared).cellOf name = .ok cell) ∧
    (∀ state kind target initializer value store resolved declared type,
      oracles.evaluate state.environment state.store initializer = .ok (value, store) →
      resolveTarget oracles state.environment target = some (resolved, declared) →
      Membership.value world.membership type value → TargetTyped type resolved →
        ∀ after, declareStatement world decisions oracles state kind target initializer ≠ .internal .malformedTarget after) ∧
    (∀ state target op value root path cell, (∀ key, target ≠ .env key) →
      flatten oracles.spelling target = some (root, path) → state.environment.cellOf root = .ok cell →
        assignStatementAt world decisions raises operate oracles state target op value =
          lift state state.environment
            (assignStatement world decisions raises operate state.store cell
              (sides oracles state.environment ((state.store cell).map fun found => found.contents.typeTag) path) op
              (fun store => oracles.evaluate state.environment store value))) ∧
    (∀ state target op value root path fault, (∀ key, target ≠ .env key) →
      flatten oracles.spelling target = some (root, path) → state.environment.cellOf root = .error fault →
        assignStatementAt world decisions raises operate oracles state target op value =
          .internal (.unresolvedName fault) state) ∧
    (∀ state key value result store bytes,
      oracles.evaluate state.environment state.store value = .ok (result, store) → oracles.argv result = some bytes →
        assignStatementAt world decisions raises operate oracles state (.env key) .set value =
          match setEnvironment raises state.overlay (oracles.spelling key).toUTF8.toList bytes with
          | .ok overlay => .completed { state with store := store, overlay := overlay }
          | .error failure => .failed failure { state with store := store }) ∧
    (∀ (state : State world) environment step, (lift state environment step).state.store = step.storeAfter) ∧
    (∀ state body outcome, block world decisions raises operate oracles state body = some outcome →
      outcome.state.environment.names = state.environment.names) :=
  ⟨execute_some_iff world decisions raises operate oracles,
    declareStatement_initializer_fails world decisions oracles,
    declareStatement_resolved world decisions oracles,
    declareStatement_commits world decisions oracles,
    declared_names_resolve,
    declareStatement_never_malformed world decisions oracles,
    assignStatementAt_resolves world decisions raises operate oracles,
    assignStatementAt_unresolved world decisions raises operate oracles,
    assignStatementAt_environment world decisions raises operate oracles,
    lift_store,
    block_restores_names world decisions raises operate oracles⟩

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Writes.Witness
open Xsh.Proofs.Core.Env

def oracles : Oracles witnessWorld where
  spelling := fun _ => "n"
  evaluate := fun _ store expression =>
    match expression with
    | .literal (.integer "5") => .ok (five, store)
    | .literal (.integer "-1") => .ok (negativeOne, store)
    | _ => .ok (zero, store)
  selector := fun _ store _ _ => .ok (.field "n", store)
  declaration := fun _ name => if name = counterName then some (binder 1 (by decide), counterCell, .primitive .unsignedInteger)
    else none
  argv := fun _ => none

def start : State witnessWorld := ⟨⟨emptyLexicalEnvironment, []⟩, fun _ => none, []⟩

def program : List Xsh.Syntax.Statement := [
  .binding .var (.name counterName) none (.literal (.integer "0")),
  .assign (.name counterName) .set (.literal (.integer "5")),
  .assign (.name counterName) .set (.literal (.integer "-1"))
]

def scopelessBlock (state : State witnessWorld) (body : List Xsh.Syntax.Statement) : Option (Outcome witnessWorld) :=
  runAll witnessWorld witnessDecisions raises pair oracles
    { state with environment := { state.environment with names := enterScope state.environment.names } } body

theorem unsigned_program_keeps_the_last_member_and_leaves_its_scope :
    ∃ outcome, block witnessWorld witnessDecisions raises pair oracles start program = some outcome ∧
      (∃ failure, outcome = .failed failure outcome.state ∧
        failure = .typeError (.primitive .unsignedInteger) negativeOne) ∧
      (outcome.state.store counterCell).map (fun cell => cell.contents.value) = some five ∧
      outcome.state.environment.cellOf counterName = .error (.unboundName counterName) ∧
      ∃ leaked, scopelessBlock start program = some leaked ∧ leaked.state.environment.cellOf counterName = .ok counterCell := by
  let entered : State witnessWorld := { start with environment := { start.environment with names := enterScope start.environment.names } }
  let declared : List Declared := [(counterName, binder 1 (by decide), counterCell)]
  have resolved : resolveTarget oracles entered.environment (.name counterName) =
      some (.name counterCell (.primitive .unsignedInteger), declared) := by
    simp [resolveTarget, oracles, declared]
  obtain ⟨first, firstCommitted⟩ := (declare_committed_iff witnessWorld witnessDecisions entered.store counterCell .mutable
    (.primitive .unsignedInteger) zero).mpr ⟨rfl, member_of_nonnegative 0 (by decide) (by decide)⟩
  obtain ⟨⟨firstCell, firstAt, firstMutability, firstType, firstValue⟩, _⟩ :=
    declare_committed_effect witnessWorld witnessDecisions entered.store first counterCell .mutable _ _ firstCommitted
  let afterFirst : State witnessWorld := ⟨extend entered.environment declared, first, []⟩
  have firstStep : execute witnessWorld witnessDecisions raises pair oracles entered program[0] =
      some (.completed afterFirst) := by
    simp only [program, List.getElem_cons_zero, execute]
    rw [declareStatement_resolved witnessWorld witnessDecisions oracles entered .var (.name counterName)
      (.literal (.integer "0")) zero entered.store _ declared (by simp [oracles, entered, start]) resolved,
      declareTarget_name, show bindingMutability .var = .mutable from rfl, firstCommitted]
    rfl
  have resolvesAfter : afterFirst.environment.cellOf counterName = .ok counterCell :=
    extend_cellOf entered.environment declared (by simp [declared]) (by simp [declared]) counterName (binder 1 (by decide)) counterCell
      (by simp [declared])
  have assignTo : ∀ (state : State witnessWorld) (cell : Cell witnessWorld) (literal : String) (value : Value Unit Unit Unit),
      state.environment = afterFirst.environment → state.store counterCell = some cell →
      oracles.evaluate state.environment state.store (.literal (.integer literal)) = .ok (value, state.store) →
      execute witnessWorld witnessDecisions raises pair oracles state
          (.assign (.name counterName) .set (.literal (.integer literal))) =
        some (lift state state.environment (assign witnessWorld witnessDecisions state.store counterCell value)) := by
    intro state cell literal value sameEnvironment present evaluated
    simp only [execute]
    rw [assignStatementAt_resolves witnessWorld witnessDecisions raises pair oracles state _ .set _ counterName []
      counterCell (by intro key; simp) rfl (by rw [sameEnvironment]; exact resolvesAfter)]
    simp only [sides]
    rw [assignStatement_commits_after_right witnessWorld witnessDecisions raises pair state.store state.store state.store
      counterCell [] [] .set cell present rfl _ value evaluated, assignPath_whole_cell]
  have firstMutable : firstCell.mutability = .mutable := firstMutability
  have firstPresent : first counterCell = some firstCell := firstAt
  obtain ⟨second, secondCommitted⟩ := (assign_committed_iff witnessWorld witnessDecisions first counterCell five).mpr
    ⟨firstCell, firstPresent, firstMutable, by rw [firstType]; exact member_of_nonnegative 5 (by decide) (by decide)⟩
  obtain ⟨_, foundFirst, ⟨secondCell, secondAt, secondType, secondValue, secondMutability⟩, _⟩ :=
    assign_committed_effect witnessWorld witnessDecisions first second counterCell five secondCommitted
  rw [firstPresent, Option.some.injEq] at foundFirst
  subst foundFirst
  let afterSecond : State witnessWorld := ⟨afterFirst.environment, second, []⟩
  have secondStep : execute witnessWorld witnessDecisions raises pair oracles afterFirst program[1] =
      some (.completed afterSecond) := by
    rw [show program[1] = .assign (.name counterName) .set (.literal (.integer "5")) from rfl,
      assignTo afterFirst firstCell "5" five rfl firstPresent (by simp [oracles]), secondCommitted]
    rfl
  have rejected : ¬ HasType witnessWorld secondCell.contents.typeTag negativeOne := by
    rw [secondType, firstType]
    exact nonmember_of_negative (-1) (by decide) (by decide)
  have thirdFails := assign_rejects_nonmember witnessWorld witnessDecisions second counterCell negativeOne secondCell
    secondAt (secondMutability.trans firstMutable) rejected
  let afterThird : State witnessWorld := ⟨afterFirst.environment, second, []⟩
  have thirdStep : execute witnessWorld witnessDecisions raises pair oracles afterSecond program[2] =
      some (.failed (.typeError secondCell.contents.typeTag negativeOne) afterThird) := by
    rw [show program[2] = .assign (.name counterName) .set (.literal (.integer "-1")) from rfl,
      assignTo afterSecond secondCell "-1" negativeOne rfl secondAt (by simp [oracles]), thirdFails]
    rfl
  have ran : runAll witnessWorld witnessDecisions raises pair oracles entered program =
      some (.failed (.typeError secondCell.contents.typeTag negativeOne) afterThird) := by
    simp only [program, runAll]
    rw [show execute witnessWorld witnessDecisions raises pair oracles entered
        (.binding .var (.name counterName) none (.literal (.integer "0"))) = some (.completed afterFirst) from firstStep]
    simp only []
    rw [show execute witnessWorld witnessDecisions raises pair oracles afterFirst
        (.assign (.name counterName) .set (.literal (.integer "5"))) = some (.completed afterSecond) from secondStep]
    simp only []
    rw [show execute witnessWorld witnessDecisions raises pair oracles afterSecond
        (.assign (.name counterName) .set (.literal (.integer "-1"))) =
          some (.failed (.typeError secondCell.contents.typeTag negativeOne) afterThird) from thirdStep]
  have blocked : block witnessWorld witnessDecisions raises pair oracles start program =
      some (leave (.failed (.typeError secondCell.contents.typeTag negativeOne) afterThird)) := by
    simp only [block]
    rw [show { start with environment := { start.environment with names := enterScope start.environment.names } } =
      entered from rfl, ran]
    rfl
  have restored := block_restores_names witnessWorld witnessDecisions raises pair oracles start program _ blocked
  have unbound : resolve emptyLexicalEnvironment counterName = none := rfl
  refine ⟨_, blocked, ⟨_, rfl, by rw [secondType, firstType]⟩,
    by simp [leave, Outcome.state, afterThird, secondAt, secondValue], ?_,
    _, by simp only [scopelessBlock]; exact ran, by simpa [Outcome.state, afterThird] using resolvesAfter⟩
  simp only [Environment.cellOf]
  rw [restored]
  simp only [start, unbound]

theorem flatten_keeps_selector_order (x a : Xsh.Syntax.RawName) (selector : Xsh.Syntax.Expr) :
    flatten (fun _ => "a") (.index (.field (.name x) a) selector) = some (x, [.field "a", .index selector]) := rfl

end Witness

end Xsh.Proofs.Core.Statements
