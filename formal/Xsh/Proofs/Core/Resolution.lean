import Xsh.Proofs.Core.Threading

namespace Xsh.Proofs.Core.Resolution

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

variable {Declaration Identity Source : Type}

section Targets

theorem extend_represents :
    ∀ (declared : List Declared) (environment : Environment) (top : List Xsh.Syntax.RawName) (outer : StaticScopes),
      Represents environment.names (top :: outer) →
        (∀ entry ∈ declared, entry.2.1 = canonicalBinder entry.1 (outer.length + 1)) →
          Represents (extend environment declared).names (staticBind top (declared.map Prod.fst) :: outer)
  | [], _, _, _, carried, _ => carried
  | (name, binder, cell) :: rest, environment, top, outer, carried, binders => by
    have same : binder = canonicalBinder name (outer.length + 1) := binders (name, binder, cell) (by simp)
    subst same
    exact extend_represents rest _ (name :: top) outer (represents_declare carried name)
      fun entry inside => binders entry (List.mem_cons_of_mem _ inside)

mutual
  theorem resolveTarget_canonical {world : World Declaration Identity Source} (machine : Machine world)
      (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
      (resolved : machine.resolution = canonicalResolution base types) (activation : Nat) (environment : Environment) :
      ∀ target found declared, resolveTarget (machine.namesAt activation) environment target = some (found, declared) →
        declared.map Prod.fst = targetNames target ∧
          ∀ entry ∈ declared, entry.2.1 = canonicalBinder entry.1 environment.names.length
    | .name name, found, declared, ok => by
      simp only [resolveTarget, canonical_cells machine base types resolved activation environment name] at ok
      cases typed : types environment name with
      | none => simp [typed] at ok
      | some type =>
        simp only [typed, Option.map_some, Option.some.injEq, Prod.mk.injEq] at ok
        obtain ⟨_, same⟩ := ok
        subst same
        simp [targetNames]
    | .discard, found, declared, ok => by
      simp only [resolveTarget, Option.some.injEq, Prod.mk.injEq] at ok
      obtain ⟨_, same⟩ := ok
      subst same
      simp [targetNames]
    | .record fields hasRest, found, declared, ok => by
      simp only [resolveTarget] at ok
      cases fielded : resolveFields (machine.namesAt activation) environment fields with
      | none => simp [fielded] at ok
      | some pair =>
        simp only [fielded, Option.map_some, Option.some.injEq, Prod.mk.injEq] at ok
        obtain ⟨_, same⟩ := ok
        subst same
        simpa [targetNames] using
          resolveFields_canonical machine base types resolved activation environment fields pair.1 pair.2 fielded

  theorem resolveFields_canonical {world : World Declaration Identity Source} (machine : Machine world)
      (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
      (resolved : machine.resolution = canonicalResolution base types) (activation : Nat) (environment : Environment) :
      ∀ fields targets declared,
        resolveFields (machine.namesAt activation) environment fields = some (targets, declared) →
          declared.map Prod.fst = fieldNames fields ∧
            ∀ entry ∈ declared, entry.2.1 = canonicalBinder entry.1 environment.names.length
    | [], targets, declared, ok => by
      simp only [resolveFields, Option.some.injEq, Prod.mk.injEq] at ok
      obtain ⟨_, same⟩ := ok
      subst same
      simp [fieldNames]
    | (label, target) :: rest, targets, declared, ok => by
      simp only [resolveFields] at ok
      split at ok
      · rename_i first declaredFirst others declaredRest foundFirst foundRest
        simp only [Option.some.injEq, Prod.mk.injEq] at ok
        obtain ⟨_, same⟩ := ok
        subst same
        have head := resolveTarget_canonical machine base types resolved activation environment target first
          declaredFirst foundFirst
        have tail := resolveFields_canonical machine base types resolved activation environment rest others
          declaredRest foundRest
        refine ⟨by simp [fieldNames, head.1, tail.1], fun entry inside => ?_⟩
        rcases List.mem_append.mp inside with early | late
        · exact head.2 entry early
        · exact tail.2 entry late
      · cases ok
end

variable [DecidableEq Declaration]

/-- Under the canonical resolution, binding any target, a destructuring one
included, declares its names in the innermost static scope. -/
theorem bindTarget_resolves {world : World Declaration Identity Source} (machine : Machine world) (base : Oracles world)
    (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (frame next : Frame world)
    (mutability : Mutability) (target : Xsh.Syntax.BindingTarget) (value : Value Declaration Identity Source)
    (top : List Xsh.Syntax.RawName) (outer : StaticScopes) (carried : Represents frame.environment.names (top :: outer))
    (bound : bindTarget machine frame mutability target value = .normal next) :
    Represents next.environment.names (staticBind top (targetNames target) :: outer) := by
  obtain ⟨found, declared, resolvedTarget, _, environment, _, _⟩ :=
    bindTarget_binds machine frame next mutability target value bound
  obtain ⟨names, binders⟩ :=
    resolveTarget_canonical machine base types resolved frame.activation frame.environment target found declared
      resolvedTarget
  have depth : frame.environment.names.length = outer.length + 1 := by simpa using carried.1
  rw [environment, ← names]
  exact extend_represents declared frame.environment top outer carried fun entry inside => by
    rw [binders entry inside, depth]

end Targets

section Statements

/-- The scopes after a statement list. -/
def staticDeclareAll (scopes : StaticScopes) (body : List Xsh.Syntax.Statement) : StaticScopes :=
  body.foldl staticDeclare scopes

theorem staticDeclare_cons (top : List Xsh.Syntax.RawName) (outer : StaticScopes) :
    ∀ statement, ∃ inner, staticDeclare (top :: outer) statement = inner :: outer
  | .binding _ _ _ _ => ⟨_, rfl⟩
  | .guardStmt _ _ _ _ => ⟨_, rfl⟩
  | .guardHandled _ _ _ _ _ => ⟨_, rfl⟩
  | .functionDef _ _ _ _ _ _ _ _ => ⟨_, rfl⟩
  | .sugar _ expansion => by
    obtain ⟨inner, shape⟩ := staticDeclare_cons top outer expansion
    exact ⟨inner, by simp only [staticDeclare]; exact shape⟩
  | .export declaration => by
    obtain ⟨inner, shape⟩ := staticDeclare_cons top outer declaration
    exact ⟨inner, by simp only [staticDeclare]; exact shape⟩
  | .assign _ _ _ | .expression _ | .tailName _ | .returnValue _ | .yield _ | .yieldDelegate _ | .defer _ _
    | .ifStmt _ _ | .whileStmt _ _ | .forStmt _ _ _ | .withScope _ _ _ | .loopStmt _ | .assert _ _ | .breakStmt _
    | .continueStmt | .exit _ | .matchStmt _ _ | .use _ _ | .typeDef _ _ _ _ _ | .errorDef _ _
    | .signalHook _ _ _ _ | .command _ _ => ⟨top, rfl⟩

theorem guardElse_not_normal {world : World Declaration Identity Source} (outcome : Exec world) (last : Frame world) :
    guardElse outcome ≠ .normal last := by
  cases outcome <;> simp [guardElse]

theorem fromStep_environment {world : World Declaration Identity Source} (frame : Frame world)
    (environment : Environment) (locals : Locals Declaration Identity Source) (step : StoreStep world)
    (last : Frame world) (ran : fromStep frame environment locals step = .normal last) : last.environment = environment := by
  cases step <;> simp [fromStep] at ran
  subst ran
  rfl

variable [DecidableEq Declaration]

/-- Under the canonical resolution, a statement that completes normally leaves
a lexical environment that represents the pass's scopes after it: a binding,
a `guard let` (both forms) and a local function declare their names, a
destructuring target every name it binds, and every other statement leaves
the scopes and the environment as it found them. -/
theorem statement_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) :
    ∀ fuel (frame : Frame world) statement next top outer, Represents frame.environment.names (top :: outer) →
      runStatement machine fuel frame statement = .normal next →
        Represents next.environment.names (staticDeclare (top :: outer) statement)
  | 0, _, _, _, _, _, _, ran => by simp [runStatement] at ran
  | fuel + 1, frame, statement, next, top, outer, carried, ran => by
    have scopes := restoring machine fuel
    have keep : ∀ last : Frame world, last.environment = frame.environment →
        Represents last.environment.names (top :: outer) := fun last same => by rw [same]; exact carried
    have controlled : ∀ statement, IsControl statement → runStatement machine (fuel + 1) frame statement = .normal next →
        Represents next.environment.names (top :: outer) := fun statement control done =>
      keep next (control_restores machine (fuel + 1) frame statement control next (by rw [done]; rfl)).1
    cases statement with
    | binding kind target annotation initializer =>
      simp only [runStatement] at ran
      obtain ⟨value, middle, _, environment, _, bound⟩ := toExec_normal (scopes.value frame (expectTarget machine.context.spelling annotation
          (expectSchema machine.context.spelling machine.declarations annotation initializer))) ran
      exact bindTarget_resolves machine base types resolved middle next _ target value top outer
        (by rw [environment]; exact carried) bound
    | guardStmt target annotation initializer otherwise =>
      simp only [runStatement] at ran
      obtain ⟨subject, middle, _, environment, _, bound⟩ := toExec_normal (scopes.value frame initializer) ran
      split at bound
      · exact bindTarget_resolves machine base types resolved middle next _ target _ top outer
          (by rw [environment]; exact carried) bound
      · exact absurd bound (guardElse_not_normal _ _)
    | guardHandled target annotation initializer failure otherwise =>
      simp only [runStatement] at ran
      obtain ⟨subject, middle, _, environment, _, bound⟩ := toExec_normal (scopes.value frame initializer) ran
      split at bound
      · exact bindTarget_resolves machine base types resolved middle next _ target _ top outer
          (by rw [environment]; exact carried) bound
      · exact absurd bound (guardElse_not_normal _ _)
      · cases bound
    | functionDef kind name parameters effects result body metadata test =>
      simp only [runStatement] at ran
      split at ran
      · cases declared : (machine.namesAt frame.activation).declaration frame.environment name with
        | none => cases kind <;> simp [declared, callableKind] at ran
        | some entry =>
          obtain ⟨binder, cell, type⟩ := entry
          have shape := canonical_cells machine base types resolved frame.activation frame.environment name
          rw [declared] at shape
          cases typed : types frame.environment name with
          | none => simp [typed] at shape
          | some found =>
            simp only [typed, Option.map_some, Option.some.injEq, Prod.mk.injEq] at shape
            obtain ⟨same, _, _⟩ := shape
            have depth : frame.environment.names.length = outer.length + 1 := by simpa using carried.1
            have extended := extend_represents [(name, binder, cell)] frame.environment top outer carried
              fun entry inside => by
                simp only [List.mem_singleton] at inside
                subst inside
                rw [same, depth]
            cases kind <;> simp only [declared, callableKind] at ran <;> first
              | (cases ran; exact extended)
              | cases ran
      · cases ran
    | sugar kind expansion =>
      simp only [runStatement] at ran
      simp only [staticDeclare]
      exact statement_resolves machine base types resolved fuel frame expansion next top outer carried ran
    | «export» declaration =>
      simp only [runStatement] at ran
      simp only [staticDeclare]
      exact statement_resolves machine base types resolved fuel frame declaration next top outer carried ran
    | ifStmt branches otherwise => exact controlled (.ifStmt branches otherwise) trivial ran
    | whileStmt condition body => exact controlled (.whileStmt condition body) trivial ran
    | forStmt target iterable body => exact controlled (.forStmt target iterable body) trivial ran
    | loopStmt body => exact controlled (.loopStmt body) trivial ran
    | matchStmt subject arms => exact controlled (.matchStmt subject arms) trivial ran
    | expression value =>
      cases value
      case valueBlock body => exact controlled (.expression (.valueBlock body)) trivial ran
      all_goals
        simp only [runStatement] at ran
        obtain ⟨result, middle, _, environment, _, finished⟩ := toExec_normal (scopes.value frame _) ran
        split at finished
        · cases finished
        · cases finished
          exact keep _ environment
    | assign target op value =>
      cases target <;> cases op <;> simp only [runStatement] at ran <;> first
        | (obtain ⟨result, middle, _, environment, _, finished⟩ := toExec_normal (scopes.value frame value) ran
           split at finished
           · cases finished
           · split at finished
             · cases finished
               exact keep _ environment
             · cases finished
           done)
        | (split at ran
           · cases ran
           · obtain ⟨result, middle, _, environment, _, finished⟩ := toExec_normal (scopes.value frame value) ran
             split at finished
             · cases finished
             · split at finished
               · cases finished
                 exact keep _ environment
               · cases finished
           done)
        | (split at ran
           · cases ran
           · split at ran
             · cases ran
             · split at ran
               · cases ran
               · cases ran
               · obtain ⟨sides, selected, _, selectedEnvironment, _, assigned⟩ :=
                   toExec_normal (selectorsOf_restores scopes.expr frame _ _) ran
                 obtain ⟨result, middle, _, environment, _, finished⟩ :=
                   toExec_normal (scopes.value selected value) assigned
                 split at finished
                 · cases finished
                 · exact keep _ ((fromStep_environment _ _ _ _ _ finished).trans
                     (environment.trans selectedEnvironment))
           done)
        | (cases ran; done)
    | yield value =>
      simp only [runStatement] at ran
      obtain ⟨result, middle, _, environment, _, finished⟩ := toExec_normal (scopes.value frame value) ran
      split at finished
      · cases finished
        exact keep _ environment
      · split at finished <;> (try split at finished) <;> first
          | exact keep _ ((gather_normal finished).1.trans environment)
          | exact keep _ ((resumeStaged_normal finished).1.trans environment)
          | exact keep _ ((resumeProducer_normal finished).1.trans environment)
      · cases finished
    | yieldDelegate value =>
      simp only [runStatement] at ran
      obtain ⟨result, middle, _, environment, _, finished⟩ := toExec_normal (scopes.value frame value) ran
      split at finished
      · cases finished
        exact keep _ environment
      · split at finished
        · exact keep _ ((delegatedStream_normal
            (invokeSupplied_restores _ _ (callBlock_restores _ _) _ _ _) finished).1.trans environment)
        · cases finished
          exact keep _ environment
        · cases finished
      · exact keep _ ((delegateItems_normal (fun _ _ _ resumed => by
          split at resumed <;> (try split at resumed) <;> first
            | exact gather_normal resumed
            | exact resumeStaged_normal resumed
            | exact resumeProducer_normal resumed) _ _ _
          finished).1.trans environment)
      · cases finished
    | assert condition message =>
      simp only [runStatement] at ran
      obtain ⟨tested, middle, _, environment, _, finished⟩ := toExec_normal (scopes.expr frame condition) ran
      split at finished
      · cases finished
        exact keep _ environment
      · split at finished
        · cases finished
        · obtain ⟨_, _, _, _, _, failed⟩ := toExec_normal (scopes.expr middle _) finished
          cases failed
      · cases finished
    | defer kind action =>
      simp only [runStatement] at ran
      cases ran
      exact carried
    | returnValue value =>
      cases value with
      | none => simp only [runStatement] at ran; cases ran
      | some value =>
        simp only [runStatement] at ran
        obtain ⟨_, _, _, _, _, finished⟩ := toExec_normal (scopes.value frame value) ran
        cases finished
    | breakStmt value =>
      cases value with
      | none => simp only [runStatement] at ran; cases ran
      | some value =>
        simp only [runStatement] at ran
        obtain ⟨_, _, _, _, _, finished⟩ := toExec_normal (scopes.value frame value) ran
        cases finished
    | continueStmt => simp only [runStatement] at ran; cases ran
    | typeDef name parameters body nominal bounds =>
      simp only [runStatement] at ran
      split at ran
      · cases ran
        exact carried
      · cases ran
    | errorDef name variants =>
      simp only [runStatement] at ran
      split at ran
      · cases ran
        exact carried
      · cases ran
    | exit status =>
      simp only [runStatement] at ran
      obtain ⟨_, _, _, _, _, finished⟩ := toExec_normal (scopes.expr frame status) ran
      split at finished
      · split at finished <;> cases finished
      · cases finished
    | _ => simp only [runStatement] at ran; cases ran

/-- Under the canonical resolution, a statement list that completes normally
leaves a lexical environment representing the pass's scopes after it. -/
theorem statements_resolve {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) :
    ∀ fuel (frame : Frame world) body next top outer, Represents frame.environment.names (top :: outer) →
      runStatements machine fuel frame body = .normal next →
        Represents next.environment.names (staticDeclareAll (top :: outer) body)
  | 0, _, _, _, _, _, _, ran => by simp [runStatements] at ran
  | fuel + 1, frame, [], next, top, outer, carried, ran => by
    simp only [runStatements] at ran
    cases ran
    exact carried
  | fuel + 1, frame, statement :: rest, next, top, outer, carried, ran => by
    simp only [runStatements] at ran
    cases first : runStatement machine fuel frame statement with
    | normal middle =>
      rw [first] at ran
      simp only [Exec.andThen] at ran
      have reached := statement_resolves machine base types resolved fuel frame statement middle top outer carried first
      obtain ⟨inner, shape⟩ := staticDeclare_cons top outer statement
      rw [shape] at reached
      have later := statements_resolve machine base types resolved fuel middle rest next inner outer reached ran
      simpa [staticDeclareAll, List.foldl, shape] using later
    | _ => rw [first] at ran; cases ran

end Statements

section Captures

variable [DecidableEq Declaration]

omit [DecidableEq Declaration] in
theorem resolveCaptures_canonical {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (activation : Nat) (environment : Environment) :
    ∀ (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)) entries,
      resolveCaptures (machine.namesAt activation) environment captures = some entries →
        ∀ entry ∈ entries, entry.1.2.1 = canonicalBinder entry.1.1 environment.names.length
  | [], entries, found => by
    simp only [resolveCaptures, Option.some.injEq] at found
    subst found
    intro entry inside
    cases inside
  | (captured, value) :: rest, entries, found => by
    simp only [resolveCaptures] at found
    split at found
    · rename_i binder cell type others declared remaining
      simp only [Option.some.injEq] at found
      subst found
      have shape := canonical_cells machine base types resolved activation environment captured
      rw [declared] at shape
      cases typed : types environment captured with
      | none => simp [typed] at shape
      | some found =>
        simp only [typed, Option.map_some, Option.some.injEq, Prod.mk.injEq] at shape
        obtain ⟨same, _, _⟩ := shape
        intro entry inside
        rcases List.mem_cons.mp inside with first | later
        · subst first
          exact same
        · exact resolveCaptures_canonical machine base types resolved activation environment rest others remaining
            entry later
    · cases found

/-- Under the canonical resolution, pattern captures bound in a fresh scope,
as a match arm, `if let`, `while let`, a `??` handler or a `guard let` else
block binds them, declare their names in a new innermost static scope. -/
theorem bindCaptures_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (frame next : Frame world)
    (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)) (scopes : StaticScopes)
    (carried : Represents frame.environment.names scopes)
    (bound : bindCaptures machine frame.enter captures = .normal next) :
    Represents next.environment.names (staticBind [] (captures.map Prod.fst) :: scopes) := by
  obtain ⟨entries, found, names, _, environment, _, _⟩ := bindCaptures_binds machine frame.enter next captures bound
  have entered := represents_enter carried
  have depth : frame.enter.environment.names.length = scopes.length + 1 := by
    simpa [Frame.enter] using entered.1
  have binders := resolveCaptures_canonical machine base types resolved frame.enter.activation
    frame.enter.environment captures entries found
  rw [environment, ← names]
  have shaped : entries.map (·.1.1) = (entries.map Prod.fst).map Prod.fst := by simp
  rw [shaped]
  exact extend_represents (entries.map Prod.fst) frame.enter.environment [] scopes entered fun entry inside => by
    obtain ⟨original, member, same⟩ := List.mem_map.mp inside
    subst same
    rw [binders original member, depth]

/-- Under the canonical resolution, a comprehension or `for` target bound in
a fresh scope declares its names in a new innermost static scope. -/
theorem enteredTarget_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (frame next : Frame world)
    (target : Xsh.Syntax.BindingTarget) (item : Value Declaration Identity Source) (scopes : StaticScopes)
    (carried : Represents frame.environment.names scopes)
    (bound : bindTarget machine frame.enter .immutable target item = .normal next) :
    Represents next.environment.names (staticBind [] (targetNames target) :: scopes) :=
  bindTarget_resolves machine base types resolved frame.enter next .immutable target item [] scopes
    (represents_enter carried) bound

/-- A body run after its captures are bound, as a match arm's or an
`if let`'s: when it completes normally, its lexical environment represents
the pass's scopes after its statements, starting from the captures' scope. -/
theorem boundBody_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (fuel : Nat) (frame bound last : Frame world)
    (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)) (body : List Xsh.Syntax.Statement)
    (scopes : StaticScopes) (carried : Represents frame.environment.names scopes)
    (entered : bindCaptures machine frame.enter captures = .normal bound)
    (ran : runStatements machine fuel bound body = .normal last) :
    Represents last.environment.names (staticDeclareAll (staticBind [] (captures.map Prod.fst) :: scopes) body) :=
  statements_resolve machine base types resolved fuel bound body last _ scopes
    (bindCaptures_resolves machine base types resolved frame bound captures scopes carried entered) ran

end Captures

section Root

variable [DecidableEq Declaration]

/-- Scope preservation and the resolution pass across statements. Every
expression and value-block result leaves with a value or another exit, never
a bare normal completion, in a frame with the environment and locals it
started with, and a value block's tail is never a bare normal completion.
Under the canonical resolution, binding any target, destructuring ones
included, declares its names in the innermost static scope; a statement that
completes normally, a `guard let` of either form included, leaves a lexical
environment that represents the pass's scopes after it; and so does a
statement list. The pass declares a binding's and a `guard let`'s target
names in the innermost scope. -/
theorem statement_resolution {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ fuel (frame : Frame world) expression,
      VRestores frame (runValue machine fuel frame expression) ∧
        VRestores frame (evalExpr machine fuel frame expression)) ∧
    (∀ fuel (frame : Frame world) body, Settled (runValueTail machine fuel frame body)) ∧
    (∀ base types, machine.resolution = canonicalResolution base types →
      (∀ (frame next : Frame world) mutability target value top outer,
        Represents frame.environment.names (top :: outer) →
          bindTarget machine frame mutability target value = .normal next →
            Represents next.environment.names (staticBind top (targetNames target) :: outer)) ∧
      (∀ fuel (frame : Frame world) statement next top outer, Represents frame.environment.names (top :: outer) →
        runStatement machine fuel frame statement = .normal next →
          Represents next.environment.names (staticDeclare (top :: outer) statement)) ∧
      (∀ fuel (frame : Frame world) body next top outer, Represents frame.environment.names (top :: outer) →
        runStatements machine fuel frame body = .normal next →
          Represents next.environment.names (staticDeclareAll (top :: outer) body))) ∧
    (∀ top outer kind target annotation initializer,
      staticDeclare (top :: outer) (.binding kind target annotation initializer) =
        staticBind top (targetNames target) :: outer) ∧
    (∀ top outer target annotation initializer otherwise,
      staticDeclare (top :: outer) (.guardStmt target annotation initializer otherwise) =
        staticBind top (targetNames target) :: outer) ∧
    (∀ top outer target annotation initializer failure otherwise,
      staticDeclare (top :: outer) (.guardHandled target annotation initializer failure otherwise) =
        staticBind top (targetNames target) :: outer) :=
  ⟨fun fuel frame expression => ⟨(restoring machine fuel).value frame expression,
      (restoring machine fuel).expr frame expression⟩,
    fun fuel => (restoring machine fuel).valueTail,
    fun base types resolved =>
      ⟨fun frame next mutability target value top outer carried bound =>
          bindTarget_resolves machine base types resolved frame next mutability target value top outer carried bound,
        statement_resolves machine base types resolved,
        statements_resolve machine base types resolved⟩,
    fun _ _ _ _ _ _ => rfl,
    fun _ _ _ _ _ _ => rfl,
    fun _ _ _ _ _ _ _ => rfl⟩

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness

def pairRecordName : Xsh.Syntax.RawName := name 94 (by decide)

/-- `let {key: a, value: b} = pair; guard let x = okSeven else { return }`. -/
def destructured : List Xsh.Syntax.Statement :=
  [.binding .letBinding (.record [(kName, .name aName), (vName, .name bName)] false) none (ident pairRecordName),
    .guardStmt (.name xName) none (ident okLocalName) [.returnValue none]]

def pairStart : Frame witnessWorld :=
  { fullStart with locals := fullStart.locals ++
      [(pairRecordName, .record [("key", integer intOne), ("value", integer ⟨2, by decide⟩)])] }

def destructuredRun : Exec witnessWorld := runStatements canonicalMachine 60 pairStart destructured

/-- Whether each name resolves at run time to the binder `pass` gives it,
and to some binder. -/
def agrees (pass : StaticScopes) (outcome : Exec witnessWorld) (names : List Xsh.Syntax.RawName) : Bool :=
  match outcome with
  | .normal frame => names.all fun bound =>
    resolve frame.environment.names bound == staticResolve pass bound && (staticResolve pass bound).isSome
  | _ => false

/-- The destructuring binds `a = 1` and `b = 2` and the guard `x = 7`, and each
resolves to the binder the pass gives it. -/
theorem destructured_resolves :
    readInt aName destructuredRun = some 1 ∧ readInt bName destructuredRun = some 2 ∧
      readInt xName destructuredRun = some 7 ∧
      agrees (staticDeclareAll [[]] destructured) destructuredRun [aName, bName, xName] = true := by
  decide +kernel

/-- The earlier pass, which declared only `let` of a single name. -/
def namesOnlyDeclare : StaticScopes → Xsh.Syntax.Statement → StaticScopes
  | top :: outer, .binding _ (.name name) _ _ => (name :: top) :: outer
  | top :: outer, .functionDef _ name _ _ _ _ _ _ => (name :: top) :: outer
  | scopes, _ => scopes

/-- It leaves the destructured and guarded names unresolved. -/
theorem names_only_pass_rejected :
    agrees (destructured.foldl namesOnlyDeclare [[]]) destructuredRun [aName] = false ∧
      agrees (destructured.foldl namesOnlyDeclare [[]]) destructuredRun [xName] = false := by
  decide +kernel

/-- Captures `k = 1, v = 2` bound in a fresh scope, as a `{key: k, value: v}`
arm binds them, and a comprehension target `i` bound per item, resolve to the
binders of the pass's new innermost scope. -/
def armCaptures : Exec witnessWorld :=
  bindCaptures canonicalMachine start.enter [(kName, integer intOne), (vName, integer ⟨2, by decide⟩)]

def comprehensionTarget : Exec witnessWorld :=
  bindTarget canonicalMachine start.enter .immutable (.name iName) (integer intOne)

theorem captures_resolve :
    agrees (staticBind [] [kName, vName] :: [[]]) armCaptures [kName, vName] = true ∧
      agrees (staticBind [] [iName] :: [[]]) comprehensionTarget [iName] = true := by
  decide +kernel

/-- A pass that declares captures in the enclosing scope instead of a new one
gives them the wrong binders. -/
theorem outer_scope_captures_rejected :
    agrees [staticBind [] [kName, vName]] armCaptures [kName] = false ∧
      agrees [staticBind [] [iName]] comprehensionTarget [iName] = false := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Resolution
