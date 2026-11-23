import Xsh.Proofs.Core.Bodies

namespace Xsh.Proofs.Core.Scripts

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
open Xsh.Proofs.Core.Bodies

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

section Pass

/-- A statement with its sugar expanded and its `export` removed. -/
def unsugared : Xsh.Syntax.Statement → Xsh.Syntax.Statement
  | .sugar _ expansion => unsugared expansion
  | .export declaration => unsugared declaration
  | statement => statement

/-- Whether a top-level statement is inside the machine's model: any
statement but a `stream` or `cli main` declaration. -/
def modelled : Xsh.Syntax.Statement → Bool
  | .functionDef kind _ _ _ _ _ _ _ => (callableKind kind).isSome
  | .sugar _ expansion => modelled expansion
  | .export declaration => modelled declaration
  | _ => true

/-- The payload-free variants a program's enums declare (SPEC 4.8:
"payload-free variants are bare names"). -/
def variantNames (declarations : List Xsh.Syntax.Statement) : List Xsh.Syntax.RawName :=
  declarations.flatMap fun
    | .typeDef _ _ (.tagUnion variants) _ _ | .export (.typeDef _ _ (.tagUnion variants) _ _) =>
      variants.filterMap fun
        | .mk variant [] _ => some variant
        | .mk _ (_ :: _) _ => none
    | _ => []

/-- The pass's scopes where a script's statements start (SPEC 3.1:
"Declarations are visible throughout their script or module, including the
statements before them"): the script's top-level functions and its
program's payload-free variants, declared in the script's own scope. -/
def scriptScopes (declarations script : List Xsh.Syntax.Statement) : StaticScopes → StaticScopes
  | top :: outer => staticBind top (hoistedNames script ++ variantNames declarations) :: outer
  | [] => []

/-- `entry` is the closure hoisting binds for a `proc` or `pure` declaration
of `script`, over an environment representing the pass's scopes after that
declaration, the pass starting from `scopes`. -/
abbrev DeclaredAt {world : World Declaration Identity Source} (machine : Machine world)
    (script : List Xsh.Syntax.Statement) (scopes : StaticScopes) (entry : CellId × Closure) : Prop :=
  ∃ before statement after kind name parameters effects result body metadata test callable cell
      defining captured writable,
    script = before ++ statement :: after ∧
      unsugared statement = .functionDef kind name parameters effects result body metadata test ∧
      callableKind kind = some callable ∧
      entry = (cell, .mk cell callable defining captured writable parameters
        (.block (expectBody machine.context.spelling result
                  (expectSchemaBody machine.context.spelling machine.declarations result body)) (returnKindOf machine result))) ∧
      Represents defining.names (staticDeclareAll scopes (before ++ [statement]))

omit [DecidableEq Declaration] in
theorem bareVariants_names {world : World Declaration Identity Source} (machine : Machine world) :
    (bareVariants machine).map Prod.fst = variantNames machine.declarations := by
  simp only [bareVariants, variantNames, List.map_flatMap]
  congr 1
  funext statement
  have filtered : ∀ (enum : Xsh.Syntax.RawName) (variants : List Xsh.Syntax.TagVariant),
      (variants.filterMap fun
          | .mk variant [] _ =>
            some (variant, (.tagged (machine.identity enum) (machine.context.spelling variant) [] :
              Value Declaration Identity Source))
          | .mk _ (_ :: _) _ => none).map Prod.fst =
        variants.filterMap fun
          | .mk variant [] _ => some variant
          | .mk _ (_ :: _) _ => none := by
    intro enum variants
    induction variants with
    | nil => rfl
    | cons head rest more =>
      obtain ⟨variant, fields, facets⟩ := head
      cases fields <;> simp [more]
  cases statement <;> try rfl
  case typeDef name parameters body nominal bounds =>
    cases body <;> try rfl
    exact filtered _ _
  case «export» inner =>
    cases inner <;> try rfl
    rename_i name parameters body nominal bounds
    cases body <;> try rfl
    exact filtered _ _

theorem staticBind_append (scope first second : List Xsh.Syntax.RawName) :
    staticBind scope (first ++ second) = staticBind (staticBind scope first) second := by
  simp [staticBind, List.foldl_append]

theorem represents_empty : Represents emptyLexicalEnvironment [[]] :=
  ⟨rfl, fun _ => rfl⟩

end Pass

section Hoisted

omit [DecidableEq Declaration] in
theorem hoistNames_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (activation : Nat) :
    ∀ names (environment result : Environment) top outer, Represents environment.names (top :: outer) →
      hoistNames machine activation environment names = some result →
        Represents result.names (staticBind top names :: outer)
  | [], environment, result, top, outer, carried, hoisted => by
    simp only [hoistNames, Option.some.injEq] at hoisted
    subst hoisted
    exact carried
  | name :: rest, environment, result, top, outer, carried, hoisted => by
    simp only [hoistNames] at hoisted
    split at hoisted
    · rename_i binder cell type declared
      have shape := canonical_cells machine base types resolved activation environment name
      rw [declared] at shape
      cases typed : types environment name with
      | none => simp [typed] at shape
      | some found =>
        simp only [typed, Option.map_some, Option.some.injEq, Prod.mk.injEq] at shape
        obtain ⟨same, _, _⟩ := shape
        have depth : environment.names.length = outer.length + 1 := by simpa using carried.1
        have extended := extend_represents [(name, binder, cell)] environment top outer carried fun entry inside => by
          simp only [List.mem_singleton] at inside
          subst inside
          rw [same, depth]
        exact hoistNames_resolves machine base types resolved activation rest _ result (name :: top) outer extended
          hoisted
    · cases hoisted

/-- Pattern captures bound in the script's own scope, as hoisting binds the
payload-free variants, declare their names in that scope. -/
theorem bindCaptures_declares {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (frame next : Frame world)
    (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)) (top : List Xsh.Syntax.RawName)
    (outer : StaticScopes) (carried : Represents frame.environment.names (top :: outer))
    (bound : bindCaptures machine frame captures = .normal next) :
    Represents next.environment.names (staticBind top (captures.map Prod.fst) :: outer) := by
  obtain ⟨entries, found, names, _, environment, _, _⟩ := bindCaptures_binds machine frame next captures bound
  have depth : frame.environment.names.length = outer.length + 1 := by simpa using carried.1
  have binders := resolveCaptures_canonical machine base types resolved frame.activation frame.environment captures
    entries found
  rw [environment, ← names]
  have shaped : entries.map (·.1.1) = (entries.map Prod.fst).map Prod.fst := by simp
  rw [shaped]
  exact extend_represents (entries.map Prod.fst) frame.environment top outer carried fun entry inside => by
    obtain ⟨original, member, same⟩ := List.mem_map.mp inside
    subst same
    rw [binders original member, depth]

omit [DecidableEq Declaration] in
theorem extendTarget_represents {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (activation : Nat) (environment : Environment)
    (target : Xsh.Syntax.BindingTarget) found declared (top : List Xsh.Syntax.RawName) (outer : StaticScopes)
    (carried : Represents environment.names (top :: outer))
    (resolvedTarget : resolveTarget (machine.namesAt activation) environment target = some (found, declared)) :
    Represents (extend environment declared).names (staticBind top (targetNames target) :: outer) := by
  obtain ⟨names, binders⟩ :=
    resolveTarget_canonical machine base types resolved activation environment target found declared resolvedTarget
  have depth : environment.names.length = outer.length + 1 := by simpa using carried.1
  rw [← names]
  exact extend_represents declared environment top outer carried fun entry inside => by
    rw [binders entry inside, depth]

omit [DecidableEq Declaration] in
/-- Under the canonical resolution, each top-level statement's part in
hoisting leaves an environment representing the pass's scopes after the
statement, and binds a closure only for a `proc` or `pure` declaration,
over that environment. -/
theorem hoistStatement_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (activation : Nat) :
    ∀ statement (environment : Environment) mutable closures
        (result : Environment × List CellId × List (CellId × Closure)) top outer,
      modelled statement = true → Represents environment.names (top :: outer) →
        hoistStatement machine activation environment mutable closures statement = some result →
          Represents result.1.names (staticDeclare (top :: outer) statement) ∧
            (result.2.2 = closures ∨
              ∃ kind name parameters effects returned body metadata test callable cell captured writable,
                unsugared statement = .functionDef kind name parameters effects returned body metadata test ∧
                  callableKind kind = some callable ∧
                  result.2.2 = (cell, .mk cell callable result.1 captured writable parameters
                    (.block (expectBody machine.context.spelling returned
                  (expectSchemaBody machine.context.spelling machine.declarations returned body)) (returnKindOf machine returned))) :: closures)
  | .binding kind target annotation initializer, environment, mutable, closures, result, top, outer, _, carried,
      hoisted => by
    simp only [hoistStatement] at hoisted
    split at hoisted
    · rename_i found declared resolvedTarget
      simp only [Option.some.injEq] at hoisted
      subst hoisted
      exact ⟨extendTarget_represents machine base types resolved activation environment target found declared top
        outer carried resolvedTarget, Or.inl rfl⟩
    · cases hoisted
  | .guardStmt target annotation initializer otherwise, environment, mutable, closures, result, top, outer, _,
      carried, hoisted => by
    simp only [hoistStatement] at hoisted
    split at hoisted
    · rename_i found declared resolvedTarget
      simp only [Option.some.injEq] at hoisted
      subst hoisted
      exact ⟨extendTarget_represents machine base types resolved activation environment target found declared top
        outer carried resolvedTarget, Or.inl rfl⟩
    · cases hoisted
  | .guardHandled target annotation initializer failure otherwise, environment, mutable, closures, result, top,
      outer, _, carried, hoisted => by
    simp only [hoistStatement] at hoisted
    split at hoisted
    · rename_i found declared resolvedTarget
      simp only [Option.some.injEq] at hoisted
      subst hoisted
      exact ⟨extendTarget_represents machine base types resolved activation environment target found declared top
        outer carried resolvedTarget, Or.inl rfl⟩
    · cases hoisted
  | .functionDef kind name parameters effects returned body metadata test, environment, mutable, closures, result,
      top, outer, fits, carried, hoisted => by
    simp only [hoistStatement] at hoisted
    split at hoisted
    · rename_i callable binder cell type kinded declared
      simp only [Option.some.injEq] at hoisted
      subst hoisted
      refine ⟨?_, Or.inr ⟨kind, name, parameters, effects, returned, body, metadata, test, callable, cell, _, _, rfl,
        kinded, rfl⟩⟩
      have shape := canonical_cells machine base types resolved activation environment name
      rw [declared] at shape
      cases typed : types environment name with
      | none => simp [typed] at shape
      | some found =>
        simp only [typed, Option.map_some, Option.some.injEq, Prod.mk.injEq] at shape
        obtain ⟨same, _, _⟩ := shape
        have depth : environment.names.length = outer.length + 1 := by simpa using carried.1
        exact extend_represents [(name, binder, cell)] environment top outer carried fun entry inside => by
          simp only [List.mem_singleton] at inside
          subst inside
          rw [same, depth]
    · cases hoisted
    · rename_i unmodelled
      simp [modelled, unmodelled] at fits
  | .sugar form expansion, environment, mutable, closures, result, top, outer, fits, carried, hoisted => by
    simp only [hoistStatement] at hoisted
    simp only [modelled] at fits
    exact hoistStatement_resolves machine base types resolved activation expansion environment mutable closures result
      top outer fits carried hoisted
  | .export declaration, environment, mutable, closures, result, top, outer, fits, carried, hoisted => by
    simp only [hoistStatement] at hoisted
    simp only [modelled] at fits
    simp only [staticDeclare, unsugared]
    exact hoistStatement_resolves machine base types resolved activation declaration environment mutable closures result
      top outer fits carried hoisted
  | .assign _ _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .expression _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .tailName _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .returnValue _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .yield _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .yieldDelegate _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .defer _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .ifStmt _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .whileStmt _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .forStmt _ _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .withScope _ _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .loopStmt _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .assert _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .breakStmt _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .continueStmt, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .exit _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .matchStmt _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .use _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .typeDef _ _ _ _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .errorDef _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .signalHook _ _ _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted
  | .command _ _, environment, mutable, closures, result, top, outer, _, carried, hoisted => by
    simp only [hoistStatement, Option.some.injEq] at hoisted
    subst hoisted
    exact ⟨carried, Or.inl rfl⟩

omit [DecidableEq Declaration] in
theorem hoistClosures_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (activation : Nat) :
    ∀ script (environment : Environment) mutable closures result top outer,
      (∀ statement ∈ script, modelled statement = true) → Represents environment.names (top :: outer) →
        hoistClosures machine activation environment mutable closures script = some result →
          ∃ declared, result = declared ++ closures ∧ ∀ entry ∈ declared, DeclaredAt machine script (top :: outer) entry
  | [], environment, mutable, closures, result, top, outer, _, _, hoisted => by
    simp only [hoistClosures, Option.some.injEq] at hoisted
    exact ⟨[], by simp [hoisted], fun _ inside => by cases inside⟩
  | statement :: rest, environment, mutable, closures, result, top, outer, fits, carried, hoisted => by
    simp only [hoistClosures] at hoisted
    split at hoisted
    · rename_i environment' mutable' closures' step
      have first := hoistStatement_resolves machine base types resolved activation statement environment mutable
        closures (environment', mutable', closures') top outer (fits statement (by simp)) carried step
      dsimp only at first
      obtain ⟨inner, shape⟩ := staticDeclare_cons top outer statement
      have carriedNext : Represents environment'.names (inner :: outer) := by
        rw [← shape]
        exact first.1
      obtain ⟨declared, assembled, each⟩ :=
        hoistClosures_resolves machine base types resolved activation rest environment' mutable' closures' result inner
          outer (fun later inside => fits later (by simp [inside])) carriedNext hoisted
      have lifted : ∀ entry ∈ declared, DeclaredAt machine (statement :: rest) (top :: outer) entry := by
        intro entry inside
        obtain ⟨before, declaring, after, kind, name, parameters, effects, returned, body, metadata, test, callable,
          cell, defining, captured, writable, split, core, kinded, shaped, represents⟩ := each entry inside
        refine ⟨statement :: before, declaring, after, kind, name, parameters, effects, returned, body, metadata, test,
          callable, cell, defining, captured, writable, by rw [split]; rfl, core, kinded, shaped, ?_⟩
        show Represents defining.names (List.foldl staticDeclare (staticDeclare (top :: outer) statement)
          (before ++ [declaring]))
        rw [shape]
        exact represents
      rcases first.2 with same | ⟨kind, name, parameters, effects, returned, body, metadata, test, callable, cell,
          captured, writable, core, kinded, added⟩
      · exact ⟨declared, by rw [assembled, same], lifted⟩
      · refine ⟨declared ++ [(cell, .mk cell callable environment' captured writable parameters
            (.block (expectBody machine.context.spelling returned
                  (expectSchemaBody machine.context.spelling machine.declarations returned body)) (returnKindOf machine returned)))], by rw [assembled, added]; simp, fun entry inside => ?_⟩
        rcases List.mem_append.mp inside with early | late
        · exact lifted entry early
        · simp only [List.mem_singleton] at late
          subst late
          exact ⟨[], statement, rest, kind, name, parameters, effects, returned, body, metadata, test, callable, cell,
            environment', captured, writable, rfl, core, kinded, rfl, first.1⟩
    · cases hoisted

/-- Under the canonical resolution, a script's hoisted frame represents the
pass's scopes where its statements start, and every closure hoisting binds
is over an environment representing the pass's scopes after its
declaration. -/
theorem hoist_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (frame hoisted : Frame world)
    (script : List Xsh.Syntax.Statement) (top : List Xsh.Syntax.RawName) (outer : StaticScopes)
    (carried : Represents frame.environment.names (top :: outer))
    (fits : ∀ statement ∈ script, modelled statement = true) (hoisting : hoist machine frame script = some hoisted) :
    Represents hoisted.environment.names (scriptScopes machine.declarations script (top :: outer)) ∧
      ∃ declared, hoisted.closures = declared ++ frame.closures ∧
        ∀ entry ∈ declared, DeclaredAt machine script (scriptScopes machine.declarations script (top :: outer)) entry := by
  simp only [hoist] at hoisting
  cases named : hoistNames machine frame.activation frame.environment (hoistedNames script) with
  | none => rw [named] at hoisting; cases hoisting
  | some environment =>
    rw [named] at hoisting
    simp only [Option.bind_some] at hoisting
    have withNames := hoistNames_resolves machine base types resolved frame.activation (hoistedNames script)
      frame.environment environment top outer carried named
    split at hoisting
    · rename_i bound variants
      have withVariants := bindCaptures_declares machine base types resolved
        { frame with environment, locals := hide (hoistedNames script) frame.locals } bound (bareVariants machine)
        (staticBind top (hoistedNames script)) outer withNames variants
      rw [bareVariants_names, ← staticBind_append] at withVariants
      cases closed : hoistClosures machine frame.activation bound.environment [] frame.closures script with
      | none => rw [closed] at hoisting; cases hoisting
      | some closures =>
        rw [closed] at hoisting
        simp only [Option.map_some, Option.some.injEq] at hoisting
        subst hoisting
        obtain ⟨declared, assembled, each⟩ := hoistClosures_resolves machine base types resolved frame.activation script
          bound.environment [] frame.closures closures _ outer fits withVariants closed
        exact ⟨withVariants, declared, assembled, each⟩
    · cases hoisting

end Hoisted

section Declarations

theorem staticDeclareAll_cons (top : List Xsh.Syntax.RawName) (outer : StaticScopes) :
    ∀ body, ∃ inner, staticDeclareAll (top :: outer) body = inner :: outer
  | [] => ⟨top, rfl⟩
  | statement :: rest => by
    obtain ⟨middle, shape⟩ := staticDeclare_cons top outer statement
    obtain ⟨inner, last⟩ := staticDeclareAll_cons middle outer rest
    refine ⟨inner, ?_⟩
    show List.foldl staticDeclare (staticDeclare (top :: outer) statement) rest = _
    rw [shape]
    exact last

theorem staticDeclareAll_snoc (scopes : StaticScopes) (before : List Xsh.Syntax.Statement)
    (statement : Xsh.Syntax.Statement) :
    staticDeclareAll scopes (before ++ [statement]) = staticDeclare (staticDeclareAll scopes before) statement := by
  simp [staticDeclareAll, List.foldl_append]

theorem functionDef_binds_core {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel (frame next : Frame world) kind name parameters effects result body metadata test,
      runStatement machine fuel frame (.functionDef kind name parameters effects result body metadata test) =
          .normal next →
        ∃ callable cell captured writable, callableKind kind = some callable ∧
          next.closures = (cell, .mk cell callable next.environment captured writable parameters
            (.block (expectBody machine.context.spelling result
                  (expectSchemaBody machine.context.spelling machine.declarations result body)) (returnKindOf machine result))) :: frame.closures
  | 0, _, _, _, _, _, _, _, _, _, _, ran => by simp [runStatement] at ran
  | fuel + 1, frame, next, kind, name, parameters, effects, result, body, metadata, test, ran => by
    simp only [runStatement] at ran
    split at ran
    · cases declared : (machine.namesAt frame.activation).declaration frame.environment name with
      | none => cases kind <;> simp [declared, callableKind] at ran
      | some entry =>
        obtain ⟨binder, cell, type⟩ := entry
        cases kind <;> simp only [declared, callableKind] at ran <;> first
          | (cases ran; exact ⟨_, cell, _, _, rfl, rfl⟩)
          | cases ran
    · cases ran

/-- A `proc` or `pure` declaration that runs to normal completion binds its
closure over the environment it leaves. -/
theorem functionDef_binds {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ fuel (frame next : Frame world) statement kind name parameters effects result body metadata test,
      unsugared statement = .functionDef kind name parameters effects result body metadata test →
        runStatement machine fuel frame statement = .normal next →
          ∃ callable cell captured writable, callableKind kind = some callable ∧
            next.closures = (cell, .mk cell callable next.environment captured writable parameters
              (.block (expectBody machine.context.spelling result
                  (expectSchemaBody machine.context.spelling machine.declarations result body)) (returnKindOf machine result))) :: frame.closures
  | 0, _, _, _, _, _, _, _, _, _, _, _, _, ran => by simp [runStatement] at ran
  | fuel + 1, frame, next, statement, kind, name, parameters, effects, result, body, metadata, test, core, ran => by
    cases statement
    case functionDef kind' name' parameters' effects' result' body' metadata' test' =>
      simp only [unsugared] at core
      rw [core] at ran
      exact functionDef_binds_core machine (fuel + 1) frame next kind name parameters effects result body metadata test
        ran
    case sugar form expansion =>
      simp only [runStatement] at ran
      exact functionDef_binds machine fuel frame next expansion kind name parameters effects result body metadata test
        core ran
    case «export» declaration =>
      simp only [runStatement] at ran
      exact functionDef_binds machine fuel frame next declaration kind name parameters effects result body metadata test
        core ran
    all_goals simp [unsugared] at core

/-- What a call of a closure over `defining`, with `writable` cells and
`body`, resolves: the call runs the body through `callBlock` as `runTail` on
the callee frame entered once, and wherever a prefix of the body has
completed normally the environment represents the pass's scopes from a
fresh scope above `scopes`, and the locals are exactly the parameters that
scope does not declare. -/
def CallsResolve {world : World Declaration Identity Source} (machine : Machine world) (defining : Environment)
    (writable : List CellId) (body : List Xsh.Syntax.Statement) (kind : ReturnKind) (scopes : StaticScopes) : Prop :=
  (∀ fuel (caller : Frame world) scope (bound : Locals Declaration Identity Source),
    callBlock machine (fuel + 2) caller scope defining bound writable body kind =
      returnTo caller kind
        (leaveBlock machine fuel { Frame.ofScope defining caller.store bound (some caller.store) scope with writable, overlay := caller.overlay, directory := caller.directory, consumers := caller.consumers, streams := caller.streams }
          (runTail machine fuel
            ({ Frame.ofScope defining caller.store bound (some caller.store) scope with writable, overlay := caller.overlay, directory := caller.directory, consumers := caller.consumers, streams := caller.streams } : Frame world).enter
            body))) ∧
  ∀ fuel (store : Store world) (bound : Locals Declaration Identity Source) scope pre rest middle,
    body = pre ++ rest → rest ≠ [] →
      runStatements machine fuel ({ Frame.ofScope defining store bound (some store) scope with writable } : Frame world).enter
          pre =
        .normal middle →
        runTail machine fuel ({ Frame.ofScope defining store bound (some store) scope with writable } : Frame world).enter
            body =
          runTail machine (fuel - pre.length) middle rest ∧
        ∃ top, staticDeclareAll ([] :: scopes) pre = top :: scopes ∧
          Represents middle.environment.names (top :: scopes) ∧
          ∀ name, name ∈ middle.locals.map Prod.fst ↔ name ∈ bound.map Prod.fst ∧ name ∉ top

theorem callsResolve_of_represents {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (defining : Environment)
    (writable : List CellId) (body : List Xsh.Syntax.Statement) (kind : ReturnKind) (scopes : StaticScopes)
    (carried : Represents defining.names scopes) : CallsResolve machine defining writable body kind scopes := by
  obtain ⟨_, _, called, bodies⟩ := body_resolution machine
  refine ⟨fun fuel caller scope bound => called fuel caller scope defining bound writable body kind, ?_⟩
  intro fuel store bound scope pre rest middle divided nonempty ran
  subst divided
  exact bodies base types resolved fuel store defining bound scope writable scopes pre rest middle carried nonempty ran

/-- Under the canonical resolution, the closure a script's declaration binds
when it runs, after the statements before it completed normally from the
hoisted frame, is over an environment representing the pass's scopes after
that declaration. -/
theorem declared_resolves {world : World Declaration Identity Source} (machine : Machine world)
    (base : Oracles world) (types : Environment → Xsh.Syntax.RawName → Option (ValueType Declaration))
    (resolved : machine.resolution = canonicalResolution base types) (hoisted : Frame world)
    (scopes : StaticScopes) (top : List Xsh.Syntax.RawName) (outer : StaticScopes) (shaped : scopes = top :: outer)
    (start : Represents hoisted.environment.names scopes) :
    ∀ fuel fuel' before statement (reached next : Frame world) kind name parameters effects result body metadata test,
      unsugared statement = .functionDef kind name parameters effects result body metadata test →
        runStatements machine fuel hoisted before = .normal reached →
          runStatement machine fuel' reached statement = .normal next →
            ∃ callable cell captured writable, callableKind kind = some callable ∧
              next.closures = (cell, .mk cell callable next.environment captured writable parameters
                (.block (expectBody machine.context.spelling result
                  (expectSchemaBody machine.context.spelling machine.declarations result body)) (returnKindOf machine result))) :: reached.closures ∧
              Represents next.environment.names (staticDeclareAll scopes (before ++ [statement])) := by
  intro fuel fuel' before statement reached next kind name parameters effects result body metadata test core
    ranBefore ranStatement
  obtain ⟨callable, cell, captured, writable, kinded, closures⟩ :=
    functionDef_binds machine fuel' reached next statement kind name parameters effects result body metadata test core
      ranStatement
  subst shaped
  have atReached := statements_resolve machine base types resolved fuel hoisted before reached top outer start ranBefore
  obtain ⟨middle, shape⟩ := staticDeclareAll_cons top outer before
  rw [shape] at atReached
  have atNext := statement_resolves machine base types resolved fuel' reached statement next middle outer atReached
    ranStatement
  rw [← shape, ← staticDeclareAll_snoc] at atNext
  exact ⟨callable, cell, captured, writable, kinded, closures, atNext⟩

end Declarations

section Roots

/-- Hoisting and the resolution pass (SPEC 3.1). The pass's scopes where a
script's statements start declare the script's top-level functions and its
program's payload-free variants in the script's scope ("Declarations are
visible throughout their script or module, including the statements before
them"), and each statement then declares its names in order. Under the
canonical resolution, for a script whose top-level declarations are `proc`s
and `pure`s, `runScript` runs its statements from a frame representing those
scopes, so a script that completes normally leaves an environment
representing the pass's scopes after it. Every closure hoisting binds
belongs to one of the script's declarations and is over an environment
representing the pass's scopes after that declaration: the hoisted
functions and variants and the top-level bindings declared before it ("A
callable body resolves the top-level bindings declared before the
callable"), and none declared after it; so is the closure the declaration
binds when it runs. -/
theorem hoisted_scopes {world : World Declaration Identity Source} (machine : Machine world) :
    (bareVariants machine).map Prod.fst = variantNames machine.declarations ∧
    ∀ base types, machine.resolution = canonicalResolution base types →
      ∀ (frame hoisted : Frame world) script top outer, Represents frame.environment.names (top :: outer) →
        (∀ statement ∈ script, modelled statement = true) → hoist machine frame script = some hoisted →
          Represents hoisted.environment.names (scriptScopes machine.declarations script (top :: outer)) ∧
          (∀ fuel (last : Frame world), runScript machine fuel frame script = .normal last →
            Represents last.environment.names
              (staticDeclareAll (scriptScopes machine.declarations script (top :: outer)) script)) ∧
          (∃ declared, hoisted.closures = declared ++ frame.closures ∧
            ∀ entry ∈ declared, ∃ before statement after kind name parameters effects result body metadata test
                callable cell defining captured writable,
              script = before ++ statement :: after ∧
                unsugared statement = .functionDef kind name parameters effects result body metadata test ∧
                callableKind kind = some callable ∧
                entry = (cell, .mk cell callable defining captured writable parameters
                  (.block (expectBody machine.context.spelling result
                  (expectSchemaBody machine.context.spelling machine.declarations result body)) (returnKindOf machine result))) ∧
                Represents defining.names
                  (staticDeclareAll (scriptScopes machine.declarations script (top :: outer)) (before ++ [statement]))) ∧
          ∀ fuel fuel' before statement after (reached next : Frame world) kind name parameters effects result body
              metadata test,
            script = before ++ statement :: after →
              unsugared statement = .functionDef kind name parameters effects result body metadata test →
                runStatements machine fuel hoisted before = .normal reached →
                  runStatement machine fuel' reached statement = .normal next →
                    ∃ callable cell captured writable, callableKind kind = some callable ∧
                      next.closures = (cell, .mk cell callable next.environment captured writable parameters
                        (.block (expectBody machine.context.spelling result
                  (expectSchemaBody machine.context.spelling machine.declarations result body)) (returnKindOf machine result))) :: reached.closures ∧
                      Represents next.environment.names
                        (staticDeclareAll (scriptScopes machine.declarations script (top :: outer))
                          (before ++ [statement])) := by
  refine ⟨bareVariants_names machine, ?_⟩
  intro base types resolved frame hoisted script top outer carried fits hoisting
  obtain ⟨start, declared, assembled, each⟩ :=
    hoist_resolves machine base types resolved frame hoisted script top outer carried fits hoisting
  obtain ⟨inner, shape⟩ : ∃ inner, scriptScopes machine.declarations script (top :: outer) = inner :: outer :=
    ⟨_, rfl⟩
  refine ⟨start, fun fuel last ran => ?_, ⟨declared, assembled, each⟩, ?_⟩
  · simp only [runScript, hoisting] at ran
    rw [shape] at start ⊢
    exact statements_resolve machine base types resolved fuel hoisted script last inner outer start ran
  · intro fuel fuel' before statement after reached next kind name parameters effects result body metadata test _ core
      ranBefore ranStatement
    exact declared_resolves machine base types resolved hoisted _ inner outer shape start fuel fuel' before statement
      reached next kind name parameters effects result body metadata test core ranBefore ranStatement

/-- Calls of a script's top-level functions and the resolution pass (SPEC
3.1, 9.2), with no premise on the scopes at a declaration. Under the
canonical resolution, for a script started in the empty lexical environment
whose top-level declarations are `proc`s and `pure`s, `runScript` runs its
statements in the hoisted frame. Every closure hoisting binds, and the
closure each declaration binds when it runs, belongs to one of the script's
declarations, and a call of it resolves its body as the pass does
(`CallsResolve`) from the scopes after that declaration: the hoisted
functions and variants and the top-level bindings declared before it. -/
theorem script_body_resolution {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ base types, machine.resolution = canonicalResolution base types →
      ∀ (frame hoisted : Frame world) script, frame.environment.names = emptyLexicalEnvironment →
        (∀ statement ∈ script, modelled statement = true) → hoist machine frame script = some hoisted →
          (∀ fuel, runScript machine fuel frame script = runStatements machine fuel hoisted script) ∧
          (∃ declared, hoisted.closures = declared ++ frame.closures ∧
            ∀ entry ∈ declared, ∃ before statement after kind name parameters effects result body metadata test
                callable cell defining captured writable,
              script = before ++ statement :: after ∧
                unsugared statement = .functionDef kind name parameters effects result body metadata test ∧
                callableKind kind = some callable ∧
                entry = (cell, .mk cell callable defining captured writable parameters
                  (.block (expectBody machine.context.spelling result
                  (expectSchemaBody machine.context.spelling machine.declarations result body)) (returnKindOf machine result))) ∧
                CallsResolve machine defining writable body (returnKindOf machine result)
                  (staticDeclareAll (scriptScopes machine.declarations script [[]]) (before ++ [statement]))) ∧
          ∀ fuel fuel' before statement after (reached next : Frame world) kind name parameters effects result body
              metadata test,
            script = before ++ statement :: after →
              unsugared statement = .functionDef kind name parameters effects result body metadata test →
                runStatements machine fuel hoisted before = .normal reached →
                  runStatement machine fuel' reached statement = .normal next →
                    ∃ callable cell captured writable, callableKind kind = some callable ∧
                      next.closures = (cell, .mk cell callable next.environment captured writable parameters
                        (.block (expectBody machine.context.spelling result
                  (expectSchemaBody machine.context.spelling machine.declarations result body)) (returnKindOf machine result))) :: reached.closures ∧
                      CallsResolve machine next.environment writable body (returnKindOf machine result)
                        (staticDeclareAll (scriptScopes machine.declarations script [[]]) (before ++ [statement])) := by
  intro base types resolved frame hoisted script empty fits hoisting
  have carried : Represents frame.environment.names [[]] := by
    rw [empty]
    exact represents_empty
  obtain ⟨_, _, ⟨declared, assembled, each⟩, declaring⟩ :=
    (hoisted_scopes machine).2 base types resolved frame hoisted script [] [] carried fits hoisting
  refine ⟨fun fuel => by simp only [runScript, hoisting], ⟨declared, assembled, fun entry inside => ?_⟩, ?_⟩
  · obtain ⟨before, statement, after, kind, name, parameters, effects, result, body, metadata, test, callable, cell,
      defining, captured, writable, split, core, kinded, shaped, represents⟩ := each entry inside
    exact ⟨before, statement, after, kind, name, parameters, effects, result, body, metadata, test, callable, cell,
      defining, captured, writable, split, core, kinded, shaped,
      callsResolve_of_represents machine base types resolved defining writable body _ _ represents⟩
  · intro fuel fuel' before statement after reached next kind name parameters effects result body metadata test split
      core ranBefore ranStatement
    obtain ⟨callable, cell, captured, writable, kinded, closures, represents⟩ :=
      declaring fuel fuel' before statement after reached next kind name parameters effects result body metadata test
        split core ranBefore ranStatement
    exact ⟨callable, cell, captured, writable, kinded, closures,
      callsResolve_of_represents machine base types resolved next.environment writable body _ _ represents⟩

end Roots

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness
open Xsh.Proofs.Core.Hoisting.Witness

def countVar : Xsh.Syntax.RawName := name 200 (by decide)
def firstProc : Xsh.Syntax.RawName := name 201 (by decide)
def secondProc : Xsh.Syntax.RawName := name 202 (by decide)
def laterLet : Xsh.Syntax.RawName := name 203 (by decide)
def guardedName : Xsh.Syntax.RawName := name 204 (by decide)
def readGuarded : Xsh.Syntax.RawName := name 205 (by decide)

/-- `enum Level { Info, Warn }` in the program, with the canonical resolver. -/
def scriptMachine : Machine witnessWorld :=
  { enumMachine with resolution := canonicalResolution enumMachine.resolution fun _ _ => some (.primitive .any) }

def runCanonical (script : List Xsh.Syntax.Statement) : Exec witnessWorld :=
  runScript scriptMachine 90 fullStart script

/-- `var count = 7; print (first()); proc first() -> Int { second() + count };
proc second() -> Int { 1 }; let later = 2; print (first())`: an earlier
function calls a later one and reads a top-level binding, through its
hoisted closure and then through the one its declaration binds, and the
verification binary prints `8` twice. -/
def scriptProgram : List Xsh.Syntax.Statement :=
  [varOf countVar "7", letOf (output 0) (call firstProc []),
    .functionDef .proc firstProc [] none none [.expression (.binary .add (call secondProc []) (ident countVar))] none
      false,
    .functionDef .proc secondProc [] none none [.expression (int "1")] none false,
    letOf laterLet (int "2"), letOf (output 1) (call firstProc [])]

/-- The pass's scopes after `first`'s declaration. -/
def atFirst : StaticScopes :=
  staticDeclareAll (scriptScopes scriptMachine.declarations scriptProgram [[]]) (scriptProgram.take 3)

def hoistedScript : Option (Frame witnessWorld) := hoist scriptMachine fullStart scriptProgram

def definingOf (closures : List (CellId × Closure)) (environment : Environment) (callee : Xsh.Syntax.RawName) :
    Option Environment :=
  match localClosure closures environment callee with
  | some (.mk _ _ defining _ _ _ _) => some defining
  | none => none

def firstDefining : Option Environment :=
  hoistedScript.bind fun hoisted => definingOf hoisted.closures hoisted.environment firstProc

/-- Whether an environment resolves each name to the binder the pass gives
it in `scopes`. -/
def agreesOn (environment : Option Environment) (scopes : StaticScopes) (names : List Xsh.Syntax.RawName) : Bool :=
  match environment with
  | some environment =>
    environment.names.length == scopes.length &&
      names.all fun named => resolve environment.names named == staticResolve scopes named
  | none => false

/-- The closure `first`'s declaration bound when it ran, at the end of the
script. -/
def declaredFirst : Option Environment :=
  match runCanonical scriptProgram with
  | .normal last => definingOf last.closures last.environment firstProc
  | _ => none

/-- The roots' premises hold: the canonical resolver, the empty lexical
environment, `proc` declarations only, and a hoisting that binds two
closures beyond the start's none. The run prints `8` twice. `first`'s
hoisted closure and the one its declaration bound both resolve `count`, the
later `second`, `first`, the variant `Info` and `later` as the pass does
after `first`'s declaration, where `later` is unresolved and `second` and
`Info` are resolved. -/
theorem hoisted_script_resolves :
    scriptMachine.resolution = canonicalResolution enumMachine.resolution (fun _ _ => some (.primitive .any)) ∧
      fullStart.environment.names = emptyLexicalEnvironment ∧ scriptProgram.all modelled = true ∧
      fullStart.closures = [] ∧ (hoistedScript.map fun hoisted => hoisted.closures.length) = some 2 ∧
      readInt (output 0) (runCanonical scriptProgram) = some 8 ∧
      readInt (output 1) (runCanonical scriptProgram) = some 8 ∧
      agreesOn firstDefining atFirst [countVar, output 0, firstProc, secondProc, laterLet, infoName] = true ∧
      agreesOn declaredFirst atFirst [countVar, output 0, firstProc, secondProc, laterLet, infoName] = true ∧
      (staticResolve atFirst laterLet).isNone = true ∧ (staticResolve atFirst secondProc).isSome = true ∧
      (staticResolve atFirst infoName).isSome = true := by
  refine ⟨rfl, rfl, by decide +kernel, rfl, by decide +kernel, by decide +kernel, by decide +kernel,
    by decide +kernel, by decide +kernel, by decide +kernel, by decide +kernel, by decide +kernel⟩

theorem disagrees_rejected (environment : Option Environment) (scopes : StaticScopes) (witness : Xsh.Syntax.RawName)
    (differs : (environment.map fun found => resolve found.names witness == staticResolve scopes witness) = some false) :
    ∃ found, environment = some found ∧ ¬ Represents found.names scopes := by
  cases environment with
  | none => cases differs
  | some found =>
    simp only [Option.map_some, Option.some.injEq] at differs
    exact ⟨found, rfl, fun represents => by rw [represents.2 witness] at differs; simp at differs⟩

/-- The environment at the end of the hoisting walk over the whole script. -/
def walkEnd {world : World Declaration Identity Source} (machine : Machine world) (activation : Nat) :
    Environment → List CellId → List (CellId × Closure) → List Xsh.Syntax.Statement → Option Environment
  | environment, _, _, [] => some environment
  | environment, mutable, closures, statement :: rest =>
    match hoistStatement machine activation environment mutable closures statement with
    | some (environment, mutable, closures) => walkEnd machine activation environment mutable closures rest
    | none => none

/-- A hoisting that closes every function over the environment after all
the script's bindings. -/
def lateDefining : Option Environment :=
  hoistedScript.bind fun hoisted => walkEnd scriptMachine 0 hoisted.environment [] [] scriptProgram

/-- A hoisting that omits the functions declared after a closure: no names
hoisted before the walk. -/
def unhoistedDefining : Option Environment :=
  hoistedScript.bind fun hoisted =>
    (hoistClosures scriptMachine 0 fullStart.environment [] [] scriptProgram).bind fun closures =>
      definingOf closures hoisted.environment firstProc

/-- The statement rejects both: the late closure resolves `later`, which the
pass leaves unresolved at `first`'s declaration, and the unhoisted one
leaves `second` unresolved, which the pass resolves. -/
theorem hoisting_mutants_rejected :
    (∃ found, lateDefining = some found ∧ ¬ Represents found.names atFirst) ∧
      ∃ found, unhoistedDefining = some found ∧ ¬ Represents found.names atFirst :=
  ⟨disagrees_rejected lateDefining atFirst laterLet (by decide +kernel),
    disagrees_rejected unhoistedDefining atFirst secondProc (by decide +kernel)⟩

/-- `let ok = Ok(7); guard let x = ok else { return }; print (read());
proc read() -> Int { x }`: the verification binary prints `7`. -/
def guardProgram : List Xsh.Syntax.Statement :=
  [.guardStmt (.name guardedName) none (ident okLocalName) [.returnValue none],
    letOf (output 0) (call readGuarded []),
    .functionDef .proc readGuarded [] none none [.expression (ident guardedName)] none false]

def atRead : StaticScopes :=
  staticDeclareAll (scriptScopes scriptMachine.declarations guardProgram [[]]) guardProgram

/-- A hoisting that skips `guard let`, as the machine's did before. -/
def guardBlind {world : World Declaration Identity Source} (machine : Machine world) (activation : Nat) :
    Environment → List CellId → List (CellId × Closure) → List Xsh.Syntax.Statement →
      Option (List (CellId × Closure))
  | _, _, closures, [] => some closures
  | environment, mutable, closures, .guardStmt _ _ _ _ :: rest =>
    guardBlind machine activation environment mutable closures rest
  | environment, mutable, closures, statement :: rest =>
    match hoistStatement machine activation environment mutable closures statement with
    | some (environment, mutable, closures) => guardBlind machine activation environment mutable closures rest
    | none => none

def blindClosures : Option (Frame witnessWorld) :=
  (hoist scriptMachine fullStart guardProgram).bind fun hoisted =>
    (guardBlind scriptMachine 0 (hoisted.environment) [] [] guardProgram).map fun closures =>
      { hoisted with closures }

def guardedDefining : Option Environment :=
  (hoist scriptMachine fullStart guardProgram).bind fun hoisted =>
    definingOf hoisted.closures hoisted.environment readGuarded

def blindDefining : Option Environment :=
  blindClosures.bind fun hoisted => definingOf hoisted.closures hoisted.environment readGuarded

/-- `read`'s hoisted closure resolves the earlier `guard let` binding, and
the call before `read`'s declaration gives `7`; the guard-blind hoisting
leaves `x` unresolved in `read`'s body, which the statement rejects, and its
run gives no value. -/
theorem guarded_binding_hoisted :
    readInt (output 0) (runCanonical guardProgram) = some 7 ∧
      agreesOn guardedDefining atRead [guardedName, readGuarded, output 0] = true ∧
      (∃ found, blindDefining = some found ∧ ¬ Represents found.names atRead) ∧
      (blindClosures.map fun hoisted => readInt (output 0) (runStatements scriptMachine 90 hoisted guardProgram)) =
        some none :=
  ⟨by decide +kernel, by decide +kernel, disagrees_rejected blindDefining atRead guardedName (by decide +kernel),
    by decide +kernel⟩

end Witness

end Xsh.Proofs.Core.Scripts
