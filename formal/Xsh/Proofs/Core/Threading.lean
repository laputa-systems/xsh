import Xsh.Proofs.Core.Control

namespace Xsh.Proofs.Core.Threading

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
open Xsh.Proofs.Core.Control

variable {Declaration Identity Source : Type}

section Bridge

/-- The forms `CoreEval.step` gives a meaning to, with no `??` among the
parts it evaluates. -/
inductive Evaluable : Xsh.Syntax.Expr → Prop
  | literal (value : Xsh.Syntax.Literal) : Evaluable (.literal value)
  | ident (name : Xsh.Syntax.RawName) : Evaluable (.ident name)
  | list (elements : List Xsh.Syntax.Expr) : (∀ element ∈ elements, Evaluable element) → Evaluable (.list elements)
  | record (fields : List Xsh.Syntax.RecordField) :
      (∀ name operand, Xsh.Syntax.RecordField.named name operand ∈ fields → Evaluable operand) →
        Evaluable (.record fields)
  | field (base : Xsh.Syntax.Expr) (name : Xsh.Syntax.RawName) : Evaluable base → Evaluable (.field base name)
  | index (base key : Xsh.Syntax.Expr) : Evaluable base → Evaluable key → Evaluable (.index base key false)
  | unary (operator : Xsh.Syntax.UnaryOp) (operand : Xsh.Syntax.Expr) :
      Evaluable operand → Evaluable (.unary operator operand)
  | binary (operator : Xsh.Syntax.BinaryOp) (left right : Xsh.Syntax.Expr) : operator ≠ .resultFallback →
      Evaluable left → Evaluable right → Evaluable (.binary operator left right)
  | ifExpr (conditions : List (Xsh.Syntax.Expr × Xsh.Syntax.Expr)) (otherwise : Xsh.Syntax.Expr) :
      (∀ condition value, (condition, value) ∈ conditions → Evaluable condition) →
        (∀ condition value, (condition, value) ∈ conditions → Evaluable value) →
        Evaluable otherwise → Evaluable (.ifExpr conditions otherwise)
  | call (name : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.CallArgument) :
      (∀ value, Xsh.Syntax.CallArgument.positional value ∈ arguments → Evaluable value) →
        Evaluable (.call (.ident name) arguments)

/-- The body of every expression-bodied plain function is evaluable. -/
def BodiesEvaluable (context : Context Declaration Identity Source) : Prop :=
  ∀ name defining parameters result,
    context.functions name = some (defining, parameters, .expression result .plain) → Evaluable result

theorem lift_bind {world : World Declaration Identity Source} {α β : Type} {store : Store world}
    {first : Evaluated world α} {result : Result Declaration Identity Source α}
    {next : α → Store world → Evaluated world β} {next' : α → Result Declaration Identity Source β}
    (head : result ≠ .unsupported → first = .lift store result)
    (tail : ∀ value, result = .value value → next' value ≠ .unsupported → next value store = .lift store (next' value))
    (supported : result.bind next' ≠ .unsupported) :
    first.bind next = .lift store (result.bind next') := by
  cases result with
  | value value =>
    rw [head (fun same => by cases same)]
    exact tail value rfl supported
  | failure _ => rw [head (fun same => by cases same)]; rfl
  | stuck => rw [head (fun same => by cases same)]; rfl
  | unsupported => exact absurd rfl supported
  | outOfFuel => rw [head (fun same => by cases same)]; rfl

theorem threadAll_agrees {world : World Declaration Identity Source} {α : Type} {store : Store world}
    {holds : α → Prop} {evaluate : Store world → α → Evaluated world (Value Declaration Identity Source)}
    {reference : α → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, holds source → reference source ≠ .unsupported →
      evaluate store source = .lift store (reference source)) :
    ∀ items, (∀ item ∈ items, holds item) → evaluateAll reference items ≠ .unsupported →
      threadAll evaluate store items = .lift store (evaluateAll reference items)
  | [], _, _ => rfl
  | first :: rest, all, supported => by
    simp only [threadAll, evaluateAll] at supported ⊢
    refine lift_bind (each first (all first (by simp))) (fun value _ restSupported => ?_) supported
    exact lift_bind (threadAll_agrees each rest fun item inside => all item (by simp [inside])) (fun _ _ _ => rfl)
      restSupported

theorem threadFields_agrees {world : World Declaration Identity Source} {store : Store world}
    (context : Context Declaration Identity Source)
    {evaluate : Store world → Xsh.Syntax.Expr → Evaluated world (Value Declaration Identity Source)}
    {reference : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, Evaluable source → reference source ≠ .unsupported →
      evaluate store source = .lift store (reference source)) :
    ∀ fields, (∀ name operand, Xsh.Syntax.RecordField.named name operand ∈ fields → Evaluable operand) →
      evaluateFields context reference fields ≠ .unsupported →
        threadFields context evaluate store fields = .lift store (evaluateFields context reference fields)
  | [], _, _ => rfl
  | .named name operand :: rest, all, supported => by
    simp only [threadFields, evaluateFields] at supported ⊢
    refine lift_bind (each operand (all name operand (by simp))) (fun value _ restSupported => ?_) supported
    exact lift_bind (threadFields_agrees context each rest fun label inner inside => all label inner (by simp [inside]))
      (fun _ _ _ => rfl) restSupported
  | .computed _ _ :: _, _, supported | .path _ _ :: _, _, supported | .shorthand _ :: _, _, supported
    | .spread _ :: _, _, supported => absurd rfl supported

theorem branch_agrees {world : World Declaration Identity Source} {store : Store world}
    (shape : Value Declaration Identity Source) {whenTrue whenFalse : Evaluated world (Value Declaration Identity Source)}
    {referenceTrue referenceFalse : Result Declaration Identity Source (Value Declaration Identity Source)}
    (onTrue : referenceTrue ≠ .unsupported → whenTrue = .lift store referenceTrue)
    (onFalse : referenceFalse ≠ .unsupported → whenFalse = .lift store referenceFalse)
    (supported : (match (generalizing := false) shape with
      | .scalar (.boolean true) => referenceTrue
      | .scalar (.boolean false) => referenceFalse
      | _ => .stuck) ≠ .unsupported) :
    (match (generalizing := false) shape with
      | .scalar (.boolean true) => whenTrue
      | .scalar (.boolean false) => whenFalse
      | _ => .stuck) =
    .lift store (match (generalizing := false) shape with
      | .scalar (.boolean true) => referenceTrue
      | .scalar (.boolean false) => referenceFalse
      | _ => .stuck) := by
  cases shape with
  | scalar scalar =>
    cases scalar with
    | boolean flag => cases flag <;> first | exact onTrue supported | exact onFalse supported
    | _ => rfl
  | _ => rfl

theorem threadBranches_agrees {world : World Declaration Identity Source} {store : Store world}
    {evaluate : Store world → Xsh.Syntax.Expr → Evaluated world (Value Declaration Identity Source)}
    {reference : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, Evaluable source → reference source ≠ .unsupported →
      evaluate store source = .lift store (reference source)) :
    ∀ conditions otherwise, (∀ condition value, (condition, value) ∈ conditions → Evaluable condition) →
      (∀ condition value, (condition, value) ∈ conditions → Evaluable value) →
      Evaluable otherwise → branches reference conditions otherwise ≠ .unsupported →
        threadBranches evaluate store conditions otherwise = .lift store (branches reference conditions otherwise)
  | [], otherwise, _, _, last, supported => each otherwise last supported
  | (condition, value) :: rest, otherwise, tests, values, last, supported => by
    simp only [threadBranches, branches] at supported ⊢
    refine lift_bind (each condition (tests condition value (by simp))) (fun tested _ testedSupported => ?_) supported
    exact branch_agrees tested.eraseNominal (each value (values condition value (by simp)))
      (threadBranches_agrees each rest otherwise (fun first second inside => tests first second (by simp [inside]))
        (fun first second inside => values first second (by simp [inside])) last)
      testedSupported

theorem positional_mem : ∀ (arguments : List Xsh.Syntax.CallArgument) (values : List Xsh.Syntax.Expr),
    positional arguments = some values → ∀ value ∈ values, Xsh.Syntax.CallArgument.positional value ∈ arguments
  | [], values, found, value, inside => by
    simp only [positional, Option.some.injEq] at found
    subst found
    cases inside
  | .positional first :: rest, values, found, value, inside => by
    simp only [positional] at found
    cases rested : positional rest with
    | none => rw [rested] at found; cases found
    | some others =>
      rw [rested] at found
      simp only [Option.map_some, Option.some.injEq] at found
      subst found
      rcases List.mem_cons.mp inside with same | later
      · subst same; simp
      · exact List.mem_cons_of_mem _ (positional_mem rest others rested value later)
  | .named _ _ :: _, _, found, _, _ | .namedSpread _ :: _, _, found, _, _ | .splice _ :: _, _, found, _, _ => by
    simp [positional] at found

theorem falseFirst_agrees {world : World Declaration Identity Source} {store : Store world}
    (shape : Value Declaration Identity Source) {whenFalse whenTrue : Evaluated world (Value Declaration Identity Source)}
    {referenceFalse referenceTrue : Result Declaration Identity Source (Value Declaration Identity Source)}
    (onFalse : referenceFalse ≠ .unsupported → whenFalse = .lift store referenceFalse)
    (onTrue : referenceTrue ≠ .unsupported → whenTrue = .lift store referenceTrue)
    (supported : (match (generalizing := false) shape with
      | .scalar (.boolean false) => referenceFalse
      | .scalar (.boolean true) => referenceTrue
      | _ => .stuck) ≠ .unsupported) :
    (match (generalizing := false) shape with
      | .scalar (.boolean false) => whenFalse
      | .scalar (.boolean true) => whenTrue
      | _ => .stuck) =
    .lift store (match (generalizing := false) shape with
      | .scalar (.boolean false) => referenceFalse
      | .scalar (.boolean true) => referenceTrue
      | _ => .stuck) := by
  cases shape with
  | scalar scalar =>
    cases scalar with
    | boolean flag => cases flag <;> first | exact onTrue supported | exact onFalse supported
    | _ => rfl
  | _ => rfl

theorem boolean_agrees {world : World Declaration Identity Source} {store : Store world}
    (shape : Value Declaration Identity Source) :
    (match (generalizing := false) shape with
      | .scalar (.boolean value) => Evaluated.value (boolean value : Value Declaration Identity Source) store
      | _ => .stuck) =
    .lift store (match (generalizing := false) shape with
      | .scalar (.boolean value) => Result.value (boolean value : Value Declaration Identity Source)
      | _ => .stuck) := by
  cases shape with
  | scalar scalar =>
    cases scalar with
    | boolean flag => rfl
    | _ => rfl
  | _ => rfl

theorem threadStep_agrees (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world)
    {recurse : Environment → Locals Declaration Identity Source → Store world → Xsh.Syntax.Expr →
      Evaluated world (Value Declaration Identity Source)}
    {callBlock : Environment → Locals Declaration Identity Source → List Xsh.Syntax.Statement → ReturnKind →
      Store world → Evaluated world (Value Declaration Identity Source)}
    {reference : Environment → Locals Declaration Identity Source → Xsh.Syntax.Expr →
      Result Declaration Identity Source (Value Declaration Identity Source)}
    (bodies : BodiesEvaluable context)
    (inner : ∀ environment locals expression, Evaluable expression →
      reference environment locals expression ≠ .unsupported →
        recurse environment locals store expression = .lift store (reference environment locals expression))
    (locals : Locals Declaration Identity Source) (expression : Xsh.Syntax.Expr) (evaluable : Evaluable expression)
    (supported : step context environment store reference (fun _ _ _ _ => .unsupported) locals expression ≠ .unsupported) :
    threadStep context environment recurse callBlock locals store expression =
      .lift store (step context environment store reference (fun _ _ _ _ => .unsupported) locals expression) := by
  cases evaluable with
  | literal value => rfl
  | ident name =>
    simp only [threadStep, step]
    cases locals.lookup name with
    | some value => rfl
    | none =>
      dsimp only
      cases environment.cellOf name with
      | ok cell =>
        dsimp only
        cases store cell <;> rfl
      | error _ => rfl
  | list elements all =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (threadAll_agrees (inner environment locals) elements all) (fun _ _ _ => rfl) supported
  | record fields all =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (threadFields_agrees context (inner environment locals) fields all) (fun _ _ _ => rfl) supported
  | field base name based =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (inner environment locals base based) (fun _ _ _ => rfl) supported
  | index base key based keyed =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (inner environment locals base based) (fun _ _ restSupported =>
      lift_bind (inner environment locals key keyed) (fun _ _ _ => rfl) restSupported) supported
  | unary operator operand evaluated =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (inner environment locals operand evaluated) (fun _ _ _ => rfl) supported
  | binary operator left right notFallback lefty righty =>
    cases operator
    case resultFallback => exact absurd rfl notFallback
    case and =>
      simp only [threadStep, step] at supported ⊢
      exact lift_bind (inner environment locals left lefty) (fun first _ firstSupported =>
        falseFirst_agrees first.eraseNominal (fun _ => rfl)
          (fun secondSupported => lift_bind (inner environment locals right righty)
            (fun second _ _ => boolean_agrees second.eraseNominal) secondSupported)
          firstSupported) supported
    case or =>
      simp only [threadStep, step] at supported ⊢
      exact lift_bind (inner environment locals left lefty) (fun first _ firstSupported =>
        branch_agrees first.eraseNominal (fun _ => rfl)
          (fun secondSupported => lift_bind (inner environment locals right righty)
            (fun second _ _ => boolean_agrees second.eraseNominal) secondSupported)
          firstSupported) supported
    all_goals
      simp only [threadStep, step] at supported ⊢
      exact lift_bind (inner environment locals left lefty) (fun _ _ restSupported =>
        lift_bind (inner environment locals right righty) (fun _ _ _ => rfl) restSupported) supported
  | ifExpr conditions otherwise tests values last =>
    simp only [threadStep, step] at supported ⊢
    exact threadBranches_agrees (inner environment locals) conditions otherwise tests values last supported
  | call name arguments all =>
    simp only [threadStep, step] at supported ⊢
    rcases hl : locals.lookup name with _ | found
    · rcases hf : context.functions name with _ | ⟨defining, parameters, body⟩
      · rfl
      · rcases hp : positional arguments with _ | values
        · rfl
        · simp only [hl, hf, hp] at supported
          refine lift_bind
            (threadAll_agrees (inner environment locals) values
              (fun value inside => all value (positional_mem arguments values hp value inside)))
            (fun evaluated _ evaluatedSupported => ?_) supported
          by_cases same : parameters.length = evaluated.length
          · simp only [same, ↓reduceIte] at evaluatedSupported ⊢
            cases body with
            | expression result kind =>
              cases kind with
              | result => exact absurd rfl evaluatedSupported
              | plain => exact inner defining _ result (bodies name defining parameters result hf) evaluatedSupported
            | block statements kind => exact absurd rfl evaluatedSupported
          · simp only [same, ↓reduceIte]
            rfl
    · rfl

theorem lift_bind_eq {world : World Declaration Identity Source} {α β : Type} {store : Store world}
    {result : Result Declaration Identity Source α} {next : α → Store world → Evaluated world β}
    {next' : α → Result Declaration Identity Source β} (tail : ∀ value, next value store = .lift store (next' value)) :
    (Evaluated.lift store result).bind next = .lift store (result.bind next') := by
  cases result <;> first | exact tail _ | rfl

/-- With defaults that leave the store as it is, the machine's threaded
binder is `bindArguments`, so `parameters_and_maps` describes it. -/
theorem bindArgumentsIn_lift {world : World Declaration Identity Source}
    (evaluateDefault : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    ∀ (store : Store world) parameters arguments,
      bindArgumentsIn (fun current default => .lift current (evaluateDefault default)) store parameters arguments =
        .lift store (bindArguments evaluateDefault parameters arguments)
  | _, [], [] => rfl
  | _, [], _ :: _ => rfl
  | store, .mk name annotation default rest :: more, arguments => by
    cases rest with
    | true =>
      cases more with
      | nil => rfl
      | cons _ _ => cases arguments <;> rfl
    | false =>
      cases arguments with
      | cons argument others =>
        rw [bindArgumentsIn, bindArguments, bindArgumentsIn_lift evaluateDefault store more others]
        exact lift_bind_eq fun _ => rfl
      | nil =>
        cases default with
        | none => rfl
        | some default =>
          rw [bindArgumentsIn, bindArguments]
          exact lift_bind_eq fun _ => by
            rw [bindArgumentsIn_lift evaluateDefault store more []]
            exact lift_bind_eq fun _ => rfl

theorem localClosure_none (environment : Environment) (name : Xsh.Syntax.RawName) :
    localClosure [] environment name = none := by
  unfold localClosure
  split <;> rfl

variable [DecidableEq Declaration]

/-- On a scope without local functions, wherever `CoreEval.evaluate` gives an
evaluable expression a meaning, the machine's store-threading step gives the
same result with the store unchanged. -/
theorem evalExpr_agrees {world : World Declaration Identity Source} (machine : Machine world)
    (bodies : BodiesEvaluable machine.context) :
    ∀ fuel scope store environment locals expression, scope.closures = [] → Evaluable expression →
      evaluate machine.context environment store fuel locals expression ≠ .unsupported →
        evalExpr machine fuel scope store environment locals expression =
          .lift store (evaluate machine.context environment store fuel locals expression)
  | 0, _, _, _, _, _, _, _, _ => rfl
  | fuel + 1, scope, store, environment, locals, expression, closureFree, evaluable, supported => by
    have agrees := threadStep_agrees machine.context environment store
      (recurse := fun inner innerLocals current => evalExpr machine fuel scope current inner innerLocals)
      (callBlock := fun defining parameters body kind current =>
        callBlock machine fuel ⟨scope.activation + machine.activationStep, [], none⟩ current defining parameters [] body kind)
      (reference := fun inner innerLocals => evaluate machine.context inner store fuel innerLocals) bodies
      (fun inner innerLocals source evaluated sourceSupported =>
        evalExpr_agrees machine bodies fuel scope store inner innerLocals source closureFree evaluated sourceSupported)
      locals expression evaluable supported
    refine Eq.trans ?_ agrees
    cases evaluable with
    | call name arguments all =>
      rcases hl : locals.lookup name with _ | found
      · rcases hf : machine.context.functions name with _ | entry
        · exfalso
          apply supported
          simp only [evaluate, step, hl, hf]
        · simp only [evalExpr, closureFree, localClosure_none, hl, hf]
      · simp only [evalExpr, closureFree, localClosure_none, hl]
    | binary operator left right notFallback _ _ =>
      cases operator <;> first | exact absurd rfl notFallback | simp only [evalExpr]
    | _ => simp only [evalExpr]

end Bridge

section Root

variable [DecidableEq Declaration]

omit [DecidableEq Declaration] in
theorem evaluated_finished_agree {world : World Declaration Identity Source} {α : Type} {run : Nat → Evaluated world α}
    (monotone : ∀ fuel extra, Evaluated.Refines (run fuel) (run (fuel + extra))) (first second : Nat)
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

theorem binary_threads {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (scope : Scope Declaration Identity Source) (store : Store world) (environment : Environment)
    (locals : Locals Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp) (left right : Xsh.Syntax.Expr)
    (notAnd : operator ≠ .and) (notOr : operator ≠ .or) (notFallback : operator ≠ .resultFallback) :
    evalExpr machine (fuel + 1) scope store environment locals (.binary operator left right) =
      (evalExpr machine fuel scope store environment locals left).bind fun first after =>
        (evalExpr machine fuel scope after environment locals right).bind fun second last =>
          .lift last (binary machine.context operator first second) := by
  cases operator <;> first
    | exact absurd rfl notAnd
    | exact absurd rfl notOr
    | exact absurd rfl notFallback
    | simp only [evalExpr, threadStep]

theorem closure_call_threads {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (scope : Scope Declaration Identity Source) (store : Store world) (environment : Environment)
    (locals : Locals Declaration Identity Source) (name : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.CallArgument)
    (cell : CellId) (defining : Environment) (captured : List (CellId × Closure)) (writable : List CellId)
    (parameters : List Xsh.Syntax.Parameter) (body : FunctionBody) (values : List Xsh.Syntax.Expr)
    (unshadowed : locals.lookup name = none)
    (found : localClosure scope.closures environment name =
      some (.mk cell defining captured writable parameters body))
    (supplied : positional arguments = some values) :
    evalExpr machine (fuel + 1) scope store environment locals (.call (.ident name) arguments) =
      (threadAll (fun current => evalExpr machine fuel scope current environment locals) store values).bind
        fun evaluated after =>
          (bindArgumentsIn
              (fun current default => evalExpr machine fuel ⟨scope.activation + machine.activationStep,
                (cell, .mk cell defining captured writable parameters body) :: captured, none⟩ current defining []
                default)
              after parameters evaluated).bind fun bound last =>
            callBlock machine fuel ⟨scope.activation + machine.activationStep,
              (cell, .mk cell defining captured writable parameters body) :: captured, none⟩ last defining bound
              writable (bodyStatements body) (bodyKind body) := by
  simp only [evalExpr, unshadowed, found, supplied]

/-- Store threading through the statement machine's expressions (SPEC 9.2: "A
writable capture must refer to mutable storage with exactly that type";
SPEC 1: "Evaluation order is source order"). On a scope without local
functions, wherever `CoreEval.evaluate` gives an evaluable expression a
meaning, the machine's step gives the same result with the store unchanged,
so the evaluator's operator, failure and typing results hold for the
machine's values there. A finished expression result is the same at every
fuel, and every result keeps the cells it started with at their types. A
binary operator's operands, a list's elements and a call's arguments evaluate
left to right, each in the store the one before it left; a local function's
call binds its arguments, evaluating omitted defaults in the store the
arguments left (with defaults that leave the store as it is, this is
`bindArguments`), and runs its body through `callBlock`, which returns through
`returnTo`, so the caller's cells come back as the callee left them from a
call nested anywhere in an expression; and a call in statement position is
the same evaluation. -/
theorem expression_store_threading {world : World Declaration Identity Source} (machine : Machine world) :
    (BodiesEvaluable machine.context →
      ∀ fuel scope store environment locals expression, scope.closures = [] → Evaluable expression →
        evaluate machine.context environment store fuel locals expression ≠ .unsupported →
          evalExpr machine fuel scope store environment locals expression =
            .lift store (evaluate machine.context environment store fuel locals expression)) ∧
    (∀ scope store environment locals expression first second,
      evalExpr machine first scope store environment locals expression ≠ .outOfFuel →
        evalExpr machine second scope store environment locals expression ≠ .outOfFuel →
          evalExpr machine first scope store environment locals expression =
            evalExpr machine second scope store environment locals expression) ∧
    (∀ fuel scope store environment locals expression,
      EKeeps store (evalExpr machine fuel scope store environment locals expression)) ∧
    (∀ fuel scope store environment locals operator left right, operator ≠ .and → operator ≠ .or →
      operator ≠ .resultFallback →
        evalExpr machine (fuel + 1) scope store environment locals (.binary operator left right) =
          (evalExpr machine fuel scope store environment locals left).bind fun first after =>
            (evalExpr machine fuel scope after environment locals right).bind fun second last =>
              .lift last (binary machine.context operator first second)) ∧
    (∀ fuel scope store environment locals elements,
      evalExpr machine (fuel + 1) scope store environment locals (.list elements) =
        (threadAll (fun current => evalExpr machine fuel scope current environment locals) store elements).bind
          fun values after => .value (.list values) after) ∧
    (∀ (evaluate : Store world → Xsh.Syntax.Expr → Evaluated world (Value Declaration Identity Source)) store first rest,
      threadAll evaluate store (first :: rest) =
        (evaluate store first).bind fun value after =>
          (threadAll evaluate after rest).bind fun values last => .value (value :: values) last) ∧
    (∀ fuel scope store environment locals name arguments cell defining captured writable parameters body values,
      locals.lookup name = none →
        localClosure scope.closures environment name = some (.mk cell defining captured writable parameters body) →
          positional arguments = some values →
            evalExpr machine (fuel + 1) scope store environment locals (.call (.ident name) arguments) =
              (threadAll (fun current => evalExpr machine fuel scope current environment locals) store values).bind
                fun evaluated after =>
                  (bindArgumentsIn
                      (fun current default => evalExpr machine fuel ⟨scope.activation + machine.activationStep,
                        (cell, .mk cell defining captured writable parameters body) :: captured, none⟩ current defining []
                        default)
                      after parameters evaluated).bind fun bound last =>
                    callBlock machine fuel ⟨scope.activation + machine.activationStep,
                      (cell, .mk cell defining captured writable parameters body) :: captured, none⟩ last defining bound
                      writable (bodyStatements body) (bodyKind body)) ∧
    (∀ (evaluateDefault : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source))
        (store : Store world) parameters arguments,
      bindArgumentsIn (fun current default => .lift current (evaluateDefault default)) store parameters arguments =
        .lift store (bindArguments evaluateDefault parameters arguments)) ∧
    (∀ fuel (scope : Scope Declaration Identity Source) store defining parameters writable body kind,
      callBlock machine (fuel + 1) scope store defining parameters writable body kind =
        fromValued (returnTo { Frame.ofScope defining store parameters (some store) scope with writable } kind
          (runBody machine fuel { Frame.ofScope defining store parameters (some store) scope with writable } body))) ∧
    (∀ (caller callee : Frame world) kind value,
      ∃ result, fromValued (returnTo caller kind (.returned value callee)) =
        .value result (keep caller.store callee.store)) ∧
    (∀ (before after : Store world) cell, before cell ≠ none → keep before after cell = after cell) ∧
    (∀ fuel (frame : Frame world) name arguments,
      runValue machine (fuel + 1) frame (.call (.ident name) arguments) =
        stored frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals
          (.call (.ident name) arguments))) := by
  refine ⟨fun bodies => evalExpr_agrees machine bodies,
    fun scope store environment locals expression =>
      evaluated_finished_agree fun fuel extra => (refining machine fuel extra).expr scope store environment locals expression,
    fun fuel => (persisting machine fuel).expr,
    fun fuel scope store environment locals operator left right notAnd notOr notFallback =>
      binary_threads machine fuel scope store environment locals operator left right notAnd notOr notFallback,
    fun _ _ _ _ _ _ => by simp only [evalExpr, threadStep],
    fun _ _ _ _ => rfl,
    fun fuel scope store environment locals name arguments cell defining captured writable parameters body values
        unshadowed found supplied =>
      closure_call_threads machine fuel scope store environment locals name arguments cell defining captured writable
        parameters body values unshadowed found supplied,
    bindArgumentsIn_lift,
    fun _ _ _ _ _ _ _ _ => rfl,
    fun _ _ kind _ => by cases kind <;> exact ⟨_, rfl⟩,
    fun before after cell held => ?_,
    fun _ _ _ _ => by simp only [runValue]⟩
  simp only [keep]
  cases present : before cell with
  | none => exact absurd present held
  | some _ => rfl

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness

def tallyName : Xsh.Syntax.RawName := name 92 (by decide)

/-- `proc counter() { count += 1; count }` over a captured `var count`. -/
def declareCounter : Xsh.Syntax.Statement :=
  .functionDef .proc tallyName [] none none [bump countName (int "1"), .expression (ident countName)] none false

def counted (value : Xsh.Syntax.Expr) : List Xsh.Syntax.Statement :=
  [varOf countName "0", declareCounter, letR value]

def pairOfCounts : Xsh.Syntax.Expr := .list [call tallyName [], call tallyName []]

def scaledSum : Xsh.Syntax.Expr := .binary .add (.binary .mul (call tallyName []) (int "7")) (call tallyName [])

/-- `let r = [counter(), counter()]` gives `[1, 2]` and leaves `count = 2`:
the second call sees the first call's write, so the elements run left to
right in the store the one before left. -/
theorem list_operands_thread :
    listInts rName (runFull (counted pairOfCounts)) = some [1, 2] ∧
      readInt countName (runFull (counted pairOfCounts)) = some 2 := by
  decide +kernel

/-- `let r = counter() * 7 + counter()` gives `9` and leaves `count = 2`:
the call nested in the left operand of `*`, itself the left operand of `+`,
writes back before the right operand runs. -/
theorem binary_operand_writes_back :
    readInt rName (runFull (counted scaledSum)) = some 9 ∧ readInt countName (runFull (counted scaledSum)) = some 2 := by
  decide +kernel

def defaultedName : Xsh.Syntax.RawName := name 93 (by decide)

/-- `proc defaulted(k = counter()) { k }`. -/
def declareDefaultedCall : Xsh.Syntax.Statement :=
  .functionDef .proc defaultedName [.mk kName none (some (call tallyName [])) false] none none
    [.expression (ident kName)] none false

def defaultedRun : Exec witnessWorld :=
  runFull [varOf countName "0", declareCounter, declareDefaultedCall,
    letR (.list [call defaultedName [], ident countName])]

/-- `let r = [defaulted(), count]` gives `[1, 1]`: the counter call in the
omitted default writes `count` back before the next element reads it. -/
theorem default_call_writes_back : listInts rName defaultedRun = some [1, 1] := by
  decide +kernel

/-- The frame after `var count = 0` and the counter's declaration. -/
def counterFrame : Option (Frame witnessWorld) :=
  match runStatements fullMachine 40 fullStart [varOf countName "0", declareCounter] with
  | .normal frame => some frame
  | _ => none

def countIn (frame : Frame witnessWorld) (store : Store witnessWorld) : Option Int :=
  match frame.environment.cellOf countName with
  | .ok cell =>
    (store cell).bind fun found =>
      match found.contents.value with
      | .scalar (.integer value) => some value.val
      | _ => none
  | .error _ => none

def intsOf : Value Unit Unit Unit → Option (List Int)
  | .list values => values.mapM fun value =>
    match value with
    | .scalar (.integer item) => some item.val
    | _ => none
  | .scalar (.integer item) => some [item.val]
  | _ => none

/-- An expression's integers and the `count` it leaves, read in the counter frame. -/
def observe (evaluate : Frame witnessWorld → Evaluated witnessWorld (Value Unit Unit Unit)) :
    Option (List Int × Int) :=
  counterFrame.bind fun frame =>
    match evaluate frame with
    | .value value store => (intsOf value).bind fun ints => (countIn frame store).map fun count => (ints, count)
    | _ => none

def threaded (frame : Frame witnessWorld) : Xsh.Syntax.Expr → Evaluated witnessWorld (Value Unit Unit Unit) :=
  evalExpr fullMachine 60 frame.scope frame.store frame.environment frame.locals

/-- A list step that evaluates every element in the store before the list and
keeps that store, so nested writes are dropped. -/
def droppingList (frame : Frame witnessWorld) (elements : List Xsh.Syntax.Expr) :
    Evaluated witnessWorld (Value Unit Unit Unit) :=
  (threadAll (fun _ element => evalExpr fullMachine 60 frame.scope frame.store frame.environment frame.locals element)
    frame.store elements).bind fun values _ => .value (.list values) frame.store

/-- A list step that threads the store through its elements right to left. -/
def rightToLeftList (frame : Frame witnessWorld) (elements : List Xsh.Syntax.Expr) :
    Evaluated witnessWorld (Value Unit Unit Unit) :=
  (threadAll (fun current element => evalExpr fullMachine 60 frame.scope current frame.environment frame.locals element)
    frame.store elements.reverse).bind fun values after => .value (.list values.reverse) after

/-- A binary step that evaluates its right operand first. -/
def rightFirstBinary (frame : Frame witnessWorld) (operator : Xsh.Syntax.BinaryOp) (left right : Xsh.Syntax.Expr) :
    Evaluated witnessWorld (Value Unit Unit Unit) :=
  (evalExpr fullMachine 60 frame.scope frame.store frame.environment frame.locals right).bind fun second after =>
    (evalExpr fullMachine 60 frame.scope after frame.environment frame.locals left).bind fun first last =>
      .lift last (binary fullMachine.context operator first second)

/-- In the same frame, the machine's step gives `[1, 2]` with `count = 2` and
`9` with `count = 2`. -/
theorem threaded_in_frame :
    observe (fun frame => threaded frame pairOfCounts) = some ([1, 2], 2) ∧
      observe (fun frame => threaded frame scaledSum) = some ([9], 2) := by
  decide +kernel

/-- Dropping nested writes gives `[1, 1]` and leaves `count = 0`. -/
theorem dropped_writes_rejected :
    observe (fun frame => droppingList frame [call tallyName [], call tallyName []]) = some ([1, 1], 0) := by
  decide +kernel

/-- Right-to-left elements give `[2, 1]`, and a right-first `+` gives `15`. -/
theorem right_to_left_rejected :
    observe (fun frame => rightToLeftList frame [call tallyName [], call tallyName []]) = some ([2, 1], 2) ∧
      observe (fun frame => rightFirstBinary frame .add (.binary .mul (call tallyName []) (int "7"))
        (call tallyName [])) = some ([15], 2) := by
  decide +kernel

def bridged : Xsh.Syntax.Expr :=
  .binary .add (.index (items ["7", "8"]) (.unary .neg (int "0")) false) (.binary .mul (int "2") (int "2"))

def supportedBy : Result Unit Unit Unit (Value Unit Unit Unit) → Bool
  | .unsupported => false
  | _ => true

def evaluatedInts : Result Unit Unit Unit (Value Unit Unit Unit) → Option (List Int)
  | .value value => intsOf value
  | _ => none

/-- The bridge's premises hold for `[7, 8][-0] + 2 * 2` in the witness
machine's empty frame, where the evaluator gives `11`; by
`expression_store_threading` the machine gives the same with the store
unchanged. -/
theorem bridge_instance :
    Evaluable bridged ∧ start.scope.closures = [] ∧
      supportedBy (evaluate machine.context start.environment start.store 10 [] bridged) = true ∧
      evaluatedInts (evaluate machine.context start.environment start.store 10 [] bridged) = some [11] := by
  refine ⟨.binary _ _ _ (by decide)
      (.index _ _ (.list _ fun element inside => ?_) (.unary _ _ (.literal _)))
      (.binary _ _ _ (by decide) (.literal _) (.literal _)), rfl, by decide +kernel, by decide +kernel⟩
  simp only [List.map, List.mem_cons, List.not_mem_nil, or_false] at inside
  rcases inside with same | same <;> (subst same; exact .literal _)

end Witness

end Xsh.Proofs.Core.Threading
