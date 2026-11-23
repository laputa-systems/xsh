import Xsh.Proofs.Core.Patterns

namespace Xsh.Proofs.Core.Bodies

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
open Xsh.Proofs.Core.Resolution
open Xsh.Proofs.Core.Patterns

variable {Declaration Identity Source : Type}

section Parameters

theorem placeParameters_names (spelling : Xsh.Syntax.RawName → String) :
    ∀ (parameters : List Xsh.Syntax.Parameter) (positional : List (Value Declaration Identity Source)) named slots,
      placeParameters spelling parameters positional named = .ok slots → slots.map Prod.fst = parameterNames parameters
  | [], [], [], slots, placed => by
    simp only [placeParameters, Except.ok.injEq] at placed
    subst placed
    rfl
  | [], [], _ :: _, _, placed => by simp [placeParameters] at placed
  | [], _ :: _, _, _, placed => by simp [placeParameters] at placed
  | .mk name annotation default rest :: more, positional, named, slots, placed => by
    cases rest with
    | true =>
      cases more with
      | nil =>
        cases named with
        | nil =>
          simp only [placeParameters, Except.ok.injEq] at placed
          subst placed
          rfl
        | cons _ _ => simp [placeParameters] at placed
      | cons _ _ => simp [placeParameters] at placed
    | false =>
      cases positional with
      | cons value values =>
        simp only [placeParameters] at placed
        split at placed
        · cases placed
        · split at placed
          · rename_i others tail
            simp only [Except.ok.injEq] at placed
            subst placed
            simp [parameterNames, placeParameters_names spelling more values named others tail]
          · cases placed
      | nil =>
        simp only [placeParameters] at placed
        split at placed
        · split at placed
          · rename_i others tail
            simp only [Except.ok.injEq] at placed
            subst placed
            simp [parameterNames, placeParameters_names spelling more [] _ others tail]
          · cases placed
        · split at placed
          · rename_i others tail
            simp only [Except.ok.injEq] at placed
            subst placed
            simp [parameterNames, placeParameters_names spelling more [] named others tail]
          · cases placed
        · cases placed

theorem bind_value {world : World Declaration Identity Source} {α β : Type} {first : Outcome world α}
    {next : α → Frame world → Outcome world β} {result : β} {last : Frame world}
    (ran : first.bind next = .value result last) :
    ∃ value middle, first = .value value middle ∧ next value middle = .value result last := by
  cases first with
  | value value middle => exact ⟨value, middle, rfl, ran⟩
  | exec _ => cases ran

theorem bindSlots_names {world : World Declaration Identity Source}
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) :
    ∀ (frame : Frame world) slots (bound : Locals Declaration Identity Source) last,
      bindSlots evaluate frame slots = .value bound last → bound.map Prod.fst = slots.map Prod.fst
  | _, [], bound, _, ran => by
    simp only [bindSlots, Outcome.value.injEq] at ran
    rw [← ran.1]
    rfl
  | frame, (name, slot) :: rest, bound, last, ran => by
    cases slot with
    | given value =>
      simp only [bindSlots] at ran
      obtain ⟨others, middle, tail, finish⟩ := bind_value ran
      simp only [Outcome.value.injEq] at finish
      rw [← finish.1]
      simp [bindSlots_names evaluate frame rest others middle tail]
    | defaulted default =>
      simp only [bindSlots] at ran
      obtain ⟨value, after, _, continued⟩ := bind_value ran
      obtain ⟨others, middle, tail, finish⟩ := bind_value continued
      simp only [Outcome.value.injEq] at finish
      rw [← finish.1]
      simp [bindSlots_names evaluate after rest others middle tail]
    | rest values =>
      simp only [bindSlots] at ran
      obtain ⟨others, middle, tail, finish⟩ := bind_value ran
      simp only [Outcome.value.injEq] at finish
      rw [← finish.1]
      simp [bindSlots_names evaluate frame rest others middle tail]


theorem placeCall_names (spelling : Xsh.Syntax.RawName → String) (parameters : List Xsh.Syntax.Parameter)
    (supplied : List (Supplied Declaration Identity Source)) (slots : List (Xsh.Syntax.RawName × Slot Declaration Identity Source))
    (placed : placeCall spelling parameters supplied = .ok slots) : slots.map Prod.fst = parameterNames parameters := by
  simp only [placeCall] at placed
  split at placed
  · cases placed
  · exact placeParameters_names spelling parameters _ _ slots placed

end Parameters

section Locals

/-- The names a statement declares in its scope, as the pass declares them. -/
def declaredBy : Xsh.Syntax.Statement → List Xsh.Syntax.RawName
  | .binding _ target _ _ => targetNames target
  | .guardStmt target _ _ _ => targetNames target
  | .guardHandled target _ _ _ _ => targetNames target
  | .functionDef _ name _ _ _ _ _ _ => [name]
  | .sugar _ expansion => declaredBy expansion
  | .export declaration => declaredBy declaration
  | _ => []

theorem staticDeclare_declared (top : List Xsh.Syntax.RawName) (outer : StaticScopes) :
    ∀ statement, staticDeclare (top :: outer) statement = staticBind top (declaredBy statement) :: outer
  | .binding _ _ _ _ => rfl
  | .guardStmt _ _ _ _ => rfl
  | .guardHandled _ _ _ _ _ => rfl
  | .functionDef _ _ _ _ _ _ _ _ => rfl
  | .sugar _ expansion => by
    simp only [staticDeclare, declaredBy]
    exact staticDeclare_declared top outer expansion
  | .export declaration => by
    simp only [staticDeclare, declaredBy]
    exact staticDeclare_declared top outer declaration
  | .assign _ _ _ | .expression _ | .tailName _ | .returnValue _ | .yield _ | .yieldDelegate _ | .defer _ _
    | .ifStmt _ _ | .whileStmt _ _ | .forStmt _ _ _ | .withScope _ _ _ | .loopStmt _ | .assert _ _ | .breakStmt _
    | .continueStmt | .exit _ | .matchStmt _ _ | .use _ _ | .typeDef _ _ _ _ _ | .errorDef _ _
    | .signalHook _ _ _ _ | .command _ _ => rfl

/-- `after` holds exactly the locals of `before` that `names` does not hide. -/
def Hides (before after : Locals Declaration Identity Source) (names : List Xsh.Syntax.RawName) : Prop :=
  ∀ name, name ∈ after.map Prod.fst ↔ name ∈ before.map Prod.fst ∧ name ∉ names

theorem hides_refl (locals : Locals Declaration Identity Source) : Hides locals locals [] := fun _ => by simp

theorem hides_hide (names : List Xsh.Syntax.RawName) (locals : Locals Declaration Identity Source) :
    Hides locals (hide names locals) names := fun name => by
  simp only [hide, List.mem_map, List.mem_filter]
  constructor
  · rintro ⟨entry, ⟨inside, kept⟩, rfl⟩
    exact ⟨⟨entry, inside, rfl⟩, by simpa using kept⟩
  · rintro ⟨⟨entry, inside, rfl⟩, outside⟩
    exact ⟨entry, ⟨inside, by simpa using outside⟩, rfl⟩

theorem fromStep_locals {world : World Declaration Identity Source} (frame : Frame world)
    (environment : Environment) (locals : Locals Declaration Identity Source) (step : StoreStep world)
    (last : Frame world) (ran : fromStep frame environment locals step = .normal last) : last.locals = locals := by
  cases step <;> simp [fromStep] at ran
  subst ran
  rfl

variable [DecidableEq Declaration]

theorem bindTarget_hides {world : World Declaration Identity Source} (machine : Machine world) (base : Oracles world)
    (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (frame next : Frame world)
    (mutability : Mutability) (target : Xsh.Syntax.BindingTarget) (value : Value Declaration Identity Source)
    (bound : bindTarget machine frame mutability target value = .normal next) :
    Hides frame.locals next.locals (targetNames target) := by
  obtain ⟨found, declared, resolvedTarget, _, _, locals, _⟩ :=
    bindTarget_binds machine frame next mutability target value bound
  obtain ⟨names, _⟩ :=
    resolveTarget_canonical machine base types resolved frame.activation frame.environment target found declared
      resolvedTarget
  rw [locals, ← names]
  exact hides_hide _ _

/-- A statement that completes normally hides exactly the locals named like
the names it declares. -/
theorem statement_hides {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) :
    ∀ fuel (frame : Frame world) statement next, runStatement machine fuel frame statement = .normal next →
      Hides frame.locals next.locals (declaredBy statement)
  | 0, _, _, _, ran => by simp [runStatement] at ran
  | fuel + 1, frame, statement, next, ran => by
    have scopes := restoring machine fuel
    have keep : ∀ last : Frame world, last.locals = frame.locals → Hides frame.locals last.locals [] :=
      fun last same => by rw [same]; exact hides_refl _
    have controlled : ∀ statement, IsControl statement → runStatement machine (fuel + 1) frame statement = .normal next →
        Hides frame.locals next.locals [] := fun statement control done =>
      keep next (control_restores machine (fuel + 1) frame statement control next (by rw [done]; rfl)).2
    cases statement with
    | binding kind target annotation initializer =>
      simp only [runStatement] at ran
      obtain ⟨value, middle, _, _, locals, bound⟩ := toExec_normal (scopes.value frame (expectTarget machine.context.spelling annotation
          (expectSchema machine.context.spelling machine.declarations annotation initializer))) ran
      have hidden := bindTarget_hides machine base types resolved middle next _ target value bound
      rw [locals] at hidden
      exact hidden
    | guardStmt target annotation initializer otherwise =>
      simp only [runStatement] at ran
      obtain ⟨subject, middle, _, _, locals, bound⟩ := toExec_normal (scopes.value frame initializer) ran
      split at bound
      · have hidden := bindTarget_hides machine base types resolved middle next _ target _ bound
        rw [locals] at hidden
        exact hidden
      · exact absurd bound (guardElse_not_normal _ _)
    | guardHandled target annotation initializer failure otherwise =>
      simp only [runStatement] at ran
      obtain ⟨subject, middle, _, _, locals, bound⟩ := toExec_normal (scopes.value frame initializer) ran
      split at bound
      · have hidden := bindTarget_hides machine base types resolved middle next _ target _ bound
        rw [locals] at hidden
        exact hidden
      · exact absurd bound (guardElse_not_normal _ _)
      · cases bound
    | functionDef kind name parameters effects result body metadata test =>
      simp only [runStatement] at ran
      split at ran
      · cases declared : (machine.namesAt frame.activation).declaration frame.environment name with
        | none => cases kind <;> simp [declared, callableKind] at ran
        | some entry =>
          obtain ⟨binder, cell, type⟩ := entry
          cases kind <;> simp only [declared, callableKind] at ran <;> first
            | (cases ran; exact hides_hide [name] frame.locals)
            | cases ran
      · cases ran
    | sugar kind expansion =>
      simp only [runStatement] at ran
      exact statement_hides machine base types resolved fuel frame expansion next ran
    | «export» declaration =>
      simp only [runStatement] at ran
      exact statement_hides machine base types resolved fuel frame declaration next ran
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
        obtain ⟨result, middle, _, _, locals, finished⟩ := toExec_normal (scopes.value frame _) ran
        split at finished
        · cases finished
        · cases finished
          exact keep _ locals
    | assign target op value =>
      cases target <;> cases op <;> simp only [runStatement] at ran <;> first
        | (obtain ⟨result, middle, _, _, locals, finished⟩ := toExec_normal (scopes.value frame value) ran
           split at finished
           · cases finished
           · split at finished
             · cases finished
               exact keep _ locals
             · cases finished
           done)
        | (split at ran
           · cases ran
           · obtain ⟨result, middle, _, _, locals, finished⟩ := toExec_normal (scopes.value frame value) ran
             split at finished
             · cases finished
             · split at finished
               · cases finished
                 exact keep _ locals
               · cases finished
           done)
        | (split at ran
           · cases ran
           · split at ran
             · cases ran
             · split at ran
               · cases ran
               · cases ran
               · obtain ⟨sides, selected, _, _, selectedLocals, assigned⟩ :=
                   toExec_normal (selectorsOf_restores scopes.expr frame _ _) ran
                 obtain ⟨result, middle, _, _, locals, finished⟩ :=
                   toExec_normal (scopes.value selected value) assigned
                 split at finished
                 · cases finished
                 · exact keep _ ((fromStep_locals _ _ _ _ _ finished).trans (locals.trans selectedLocals))
           done)
        | (cases ran; done)
    | yield value =>
      simp only [runStatement] at ran
      obtain ⟨result, middle, _, _, locals, finished⟩ := toExec_normal (scopes.value frame value) ran
      split at finished
      · cases finished
        exact keep _ locals
      · split at finished <;> (try split at finished) <;> first
          | exact keep _ ((gather_normal finished).2.trans locals)
          | exact keep _ ((resumeStaged_normal finished).2.trans locals)
          | exact keep _ ((resumeProducer_normal finished).2.trans locals)
      · cases finished
    | yieldDelegate value =>
      simp only [runStatement] at ran
      obtain ⟨result, middle, _, _, locals, finished⟩ := toExec_normal (scopes.value frame value) ran
      split at finished
      · cases finished
        exact keep _ locals
      · split at finished
        · exact keep _ ((delegatedStream_normal
            (invokeSupplied_restores _ _ (callBlock_restores _ _) _ _ _) finished).2.trans locals)
        · cases finished
          exact keep _ locals
        · cases finished
      · exact keep _ ((delegateItems_normal (fun _ _ _ resumed => by
          split at resumed <;> (try split at resumed) <;> first
            | exact gather_normal resumed
            | exact resumeStaged_normal resumed
            | exact resumeProducer_normal resumed) _ _ _
          finished).2.trans locals)
      · cases finished
    | assert condition message =>
      simp only [runStatement] at ran
      obtain ⟨tested, middle, _, _, locals, finished⟩ := toExec_normal (scopes.expr frame condition) ran
      split at finished
      · cases finished
        exact keep _ locals
      · split at finished
        · cases finished
        · obtain ⟨_, _, _, _, _, failed⟩ := toExec_normal (scopes.expr middle _) finished
          cases failed
      · cases finished
    | defer kind action =>
      simp only [runStatement] at ran
      cases ran
      exact hides_refl _
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
        exact hides_refl _
      · cases ran
    | errorDef name variants =>
      simp only [runStatement] at ran
      split at ran
      · cases ran
        exact hides_refl _
      · cases ran
    | exit status =>
      simp only [runStatement] at ran
      obtain ⟨_, _, _, _, _, finished⟩ := toExec_normal (scopes.expr frame status) ran
      split at finished
      · split at finished <;> cases finished
      · cases finished
    | _ => simp only [runStatement] at ran; cases ran

/-- A body's locals: exactly its parameters not declared in `top`, the body's
own scope. -/
def Visible (locals : Locals Declaration Identity Source) (parameters top : List Xsh.Syntax.RawName) : Prop :=
  ∀ name, name ∈ locals.map Prod.fst ↔ name ∈ parameters ∧ name ∉ top

/-- Under the canonical resolution, a statement list that completes normally
in a scope `top` leaves the pass's scopes after it in that same scope, an
environment representing them, and as locals the parameters its
declarations do not hide. -/
theorem statements_see {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) :
    ∀ fuel (frame : Frame world) body next top outer parameters,
      Represents frame.environment.names (top :: outer) → Visible frame.locals parameters top →
        runStatements machine fuel frame body = .normal next →
          ∃ inner, staticDeclareAll (top :: outer) body = inner :: outer ∧
            Represents next.environment.names (inner :: outer) ∧ Visible next.locals parameters inner
  | 0, _, _, _, _, _, _, _, _, ran => by simp [runStatements] at ran
  | fuel + 1, frame, [], next, top, outer, parameters, carried, visible, ran => by
    simp only [runStatements] at ran
    cases ran
    exact ⟨top, rfl, carried, visible⟩
  | fuel + 1, frame, statement :: rest, next, top, outer, parameters, carried, visible, ran => by
    simp only [runStatements] at ran
    cases first : runStatement machine fuel frame statement with
    | normal middle =>
      rw [first] at ran
      simp only [Exec.andThen] at ran
      have reached := statement_resolves machine base types resolved fuel frame statement middle top outer carried first
      have hidden := statement_hides machine base types resolved fuel frame statement middle first
      rw [staticDeclare_declared] at reached
      have seen : Visible middle.locals parameters (staticBind top (declaredBy statement)) := fun name => by
        rw [hidden name, visible name, staticBind_eq]
        simp only [List.mem_append, List.mem_reverse, not_or]
        exact ⟨fun ⟨⟨given, kept⟩, fresh⟩ => ⟨given, fresh, kept⟩, fun ⟨given, fresh, kept⟩ => ⟨⟨given, kept⟩, fresh⟩⟩
      obtain ⟨inner, shape, carriedLast, visibleLast⟩ :=
        statements_see machine base types resolved fuel middle rest next _ outer parameters reached seen ran
      refine ⟨inner, ?_, carriedLast, visibleLast⟩
      show List.foldl staticDeclare (staticDeclare (top :: outer) statement) rest = _
      rw [staticDeclare_declared]
      exact shape
    | _ => rw [first] at ran; cases ran

end Locals

section Tails

variable [DecidableEq Declaration]

theorem runTail_cons_cons {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (statement next : Xsh.Syntax.Statement) (more : List Xsh.Syntax.Statement) :
    runTail machine (fuel + 1) frame (statement :: next :: more) =
      (runStatement machine fuel frame statement).andThen fun after => runTail machine fuel after (next :: more) := by
  rw [runTail] <;> simp

/-- A body's statements before its tail run as a statement list: `runTail`
over `pre ++ rest`, with `rest` not empty, runs `pre` as `runStatements`
does and continues with `rest` in the frame `pre` left. -/
theorem runTail_append {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel (frame : Frame world) pre rest, rest ≠ [] →
      runTail machine fuel frame (pre ++ rest) =
        (runStatements machine fuel frame pre).andThen fun middle => runTail machine (fuel - pre.length) middle rest
  | 0, _, pre, _, _ => by cases pre <;> simp [runTail, runStatements, Exec.andThen]
  | fuel + 1, frame, [], rest, _ => by simp [runStatements, Exec.andThen]
  | fuel + 1, frame, statement :: pre, rest, nonempty => by
    obtain ⟨next, more, split⟩ : ∃ next more, pre ++ rest = next :: more := by
      cases shape : pre ++ rest with
      | nil => simp_all
      | cons next more => exact ⟨next, more, rfl⟩
    rw [List.cons_append, split, runTail_cons_cons, ← split]
    simp only [runStatements]
    cases runStatement machine fuel frame statement <;> simp only [Exec.andThen]
    rw [runTail_append machine fuel _ pre rest nonempty]
    simp [Exec.andThen]

end Tails

section Root

variable [DecidableEq Declaration]

/-- Function bodies and the resolution pass (SPEC 3.1, 9.2). A call's
arguments bind exactly its parameter names, and its body runs, through
`callBlock` and `runBody`, as `runTail` on the callee frame entered once: the
declaration's lexical environment with a fresh innermost scope and the
parameters as locals. Under the canonical resolution, wherever the body's
statements have completed normally the environment represents the pass's
scopes from a fresh scope above those of the declaration, and the locals are
exactly the parameters that scope does not declare, so a use reads a
parameter only where no binding in the body shadows it and resolves every
other name as the pass does. -/
theorem body_resolution {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ (spelling : Xsh.Syntax.RawName → String) parameters (supplied : List (Supplied Declaration Identity Source))
        slots,
      placeCall spelling parameters supplied = .ok slots → slots.map Prod.fst = parameterNames parameters) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) frame slots bound last,
      bindSlots evaluate frame slots = .value bound last → bound.map Prod.fst = slots.map Prod.fst) ∧
    (∀ fuel (caller : Frame world) scope defining (bound : Locals Declaration Identity Source) writable body kind,
      callBlock machine (fuel + 2) caller scope defining bound writable body kind =
        returnTo caller kind
          (leaveBlock machine fuel { Frame.ofScope defining caller.store bound (some caller.store) scope with writable, overlay := caller.overlay, consumers := caller.consumers, streams := caller.streams }
            (runTail machine fuel
              ({ Frame.ofScope defining caller.store bound (some caller.store) scope with writable, overlay := caller.overlay, consumers := caller.consumers, streams := caller.streams } : Frame world).enter
              body))) ∧
    (∀ base types, machine.resolution = canonicalResolution base types →
      ∀ fuel (store : Store world) (defining : Environment) (bound : Locals Declaration Identity Source) scope
          writable scopes pre rest middle,
        Represents defining.names scopes → rest ≠ [] →
          runStatements machine fuel
              ({ Frame.ofScope defining store bound (some store) scope with writable } : Frame world).enter pre =
            .normal middle →
            runTail machine fuel
                ({ Frame.ofScope defining store bound (some store) scope with writable } : Frame world).enter
                (pre ++ rest) =
              runTail machine (fuel - pre.length) middle rest ∧
            ∃ top, staticDeclareAll ([] :: scopes) pre = top :: scopes ∧
              Represents middle.environment.names (top :: scopes) ∧
              ∀ name, name ∈ middle.locals.map Prod.fst ↔ name ∈ bound.map Prod.fst ∧ name ∉ top) := by
  refine ⟨placeCall_names, bindSlots_names, fun fuel caller scope defining bound writable body kind => rfl, ?_⟩
  intro base types resolved fuel store defining bound scope writable scopes pre rest middle carried nonempty ran
  refine ⟨by rw [runTail_append machine fuel _ pre rest nonempty, ran]; rfl, ?_⟩
  exact statements_see machine base types resolved fuel _ pre middle [] scopes (bound.map Prod.fst)
    (represents_enter carried) (fun name => by simp [Frame.enter, Frame.ofScope]) ran

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness
open Xsh.Proofs.Core.Hoisting.Witness

def shadowFn : Xsh.Syntax.RawName := name 192 (by decide)
def nestedFn : Xsh.Syntax.RawName := name 193 (by decide)

def shadowX : Xsh.Syntax.Statement := letOf valueParam (int "2")

/-- `print (f(7))` and `print (g(7))` above `proc f(x: Int) -> Int { let x = 2;
return x }` and `proc g(x: Int) -> Int { if true { let x = 2 }; return x }`;
the verification binary prints `2` and `7`. -/
def shadowProgram : List Xsh.Syntax.Statement :=
  [letOf (output 0) (call shadowFn [int "7"]), letOf (output 1) (call nestedFn [int "7"]),
    .functionDef .proc shadowFn [.mk valueParam none none false] none none
      [shadowX, .returnValue (some (ident valueParam))] none false,
    .functionDef .proc nestedFn [.mk valueParam none none false] none none
      [.ifStmt [(.literal (.bool true), [shadowX])] none, .returnValue (some (ident valueParam))] none false]

theorem parameters_shadowed :
    readInt (output 0) (runHoisted shadowProgram) = some 2 ∧ readInt (output 1) (runHoisted shadowProgram) = some 7 := by
  decide +kernel

/-- `f`'s body after `let x = 2`, entered as `callBlock` enters it with `x`
bound as a parameter. -/
def afterShadow : Exec witnessWorld :=
  match hoist constructorMachine fullStart shadowProgram with
  | some hoisted =>
    match localClosure hoisted.closures hoisted.environment shadowFn with
    | some (.mk cell callable defining captured writable _ _) =>
      runStatements constructorMachine 60
        ({ Frame.ofScope defining hoisted.store [(valueParam, .scalar (.boolean true))] (some hoisted.store)
            ⟨1, (cell, .mk cell callable defining captured writable [] (.block [] .plain)) :: captured, none⟩ with
          writable } : Frame witnessWorld).enter [shadowX]
    | none => .unsupported
  | none => .unsupported

def parameterVisible : Exec witnessWorld → Bool
  | .normal frame => (frame.locals.lookup valueParam).isSome
  | _ => true

/-- A pass that resolves parameters above the body's own scope reads `x` as
the parameter after `let x = 2`; the machine has hidden it there, as
`Visible` with `x` in the body's scope states. -/
theorem parameters_innermost_rejected : parameterVisible afterShadow = false := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Bodies
