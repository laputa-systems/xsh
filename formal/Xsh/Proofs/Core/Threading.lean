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
      Xsh.Semantics.CoreControl.namedOnly fields = true →
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

/-- Every name `environment` resolves to a cell has its cell in `store`: the
statement that declares it has run. -/
def Initialized {world : World Declaration Identity Source} (environment : Environment) (store : Store world) : Prop :=
  ∀ name cell, environment.cellOf name = .ok cell → store cell ≠ none

/-- Every expression-bodied plain function's defining environment is
initialized in `store`. -/
def DefinitionsInitialized {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (store : Store world) : Prop :=
  ∀ name defining parameters result,
    context.functions name = some (defining, parameters, .expression result .plain) → Initialized defining store

/-- The body of every expression-bodied plain function is evaluable. -/
def BodiesEvaluable (context : Context Declaration Identity Source) : Prop :=
  ∀ name defining parameters result,
    context.functions name = some (defining, parameters, .expression result .plain) → Evaluable result

theorem lift_bind {world : World Declaration Identity Source} {α β : Type} {frame : Frame world}
    {first : Outcome world α} {result : Result Declaration Identity Source α}
    {next : α → Frame world → Outcome world β} {next' : α → Result Declaration Identity Source β}
    (head : result ≠ .unsupported → first = fromEval frame result)
    (tail : ∀ value, result = .value value → next' value ≠ .unsupported → next value frame = fromEval frame (next' value))
    (supported : result.bind next' ≠ .unsupported) :
    first.bind next = fromEval frame (result.bind next') := by
  cases result with
  | value value =>
    rw [head (fun same => by cases same)]
    exact tail value rfl supported
  | failure _ => rw [head (fun same => by cases same)]; rfl
  | stuck => rw [head (fun same => by cases same)]; rfl
  | unsupported => exact absurd rfl supported
  | outOfFuel => rw [head (fun same => by cases same)]; rfl

theorem threadAll_agrees {world : World Declaration Identity Source} {α : Type} {frame : Frame world}
    {holds : α → Prop} {evaluate : Frame world → α → Valued world}
    {reference : α → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, holds source → reference source ≠ .unsupported →
      evaluate frame source = fromEval frame (reference source)) :
    ∀ items, (∀ item ∈ items, holds item) → evaluateAll reference items ≠ .unsupported →
      threadAll evaluate frame items = fromEval frame (evaluateAll reference items)
  | [], _, _ => rfl
  | first :: rest, all, supported => by
    simp only [threadAll, evaluateAll] at supported ⊢
    refine lift_bind (each first (all first (by simp))) (fun value _ restSupported => ?_) supported
    exact lift_bind (threadAll_agrees each rest fun item inside => all item (by simp [inside])) (fun _ _ _ => rfl)
      restSupported

theorem threadFields_agrees {world : World Declaration Identity Source} {frame : Frame world}
    (context : Context Declaration Identity Source)
    {evaluate : Frame world → Xsh.Syntax.Expr → Valued world}
    {reference : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, Evaluable source → reference source ≠ .unsupported →
      evaluate frame source = fromEval frame (reference source)) :
    ∀ fields, (∀ name operand, Xsh.Syntax.RecordField.named name operand ∈ fields → Evaluable operand) →
      evaluateFields context reference fields ≠ .unsupported →
        threadFields context evaluate frame fields = fromEval frame (evaluateFields context reference fields)
  | [], _, _ => rfl
  | .named name operand :: rest, all, supported => by
    simp only [threadFields, evaluateFields] at supported ⊢
    refine lift_bind (each operand (all name operand (by simp))) (fun value _ restSupported => ?_) supported
    exact lift_bind (threadFields_agrees context each rest fun label inner inside => all label inner (by simp [inside]))
      (fun _ _ _ => rfl) restSupported
  | .computed _ _ :: _, _, supported | .path _ _ :: _, _, supported | .shorthand _ :: _, _, supported
    | .spread _ :: _, _, supported => absurd rfl supported

theorem branch_agrees {world : World Declaration Identity Source} {frame : Frame world}
    (shape : Value Declaration Identity Source) {whenTrue whenFalse : Valued world}
    {referenceTrue referenceFalse : Result Declaration Identity Source (Value Declaration Identity Source)}
    (onTrue : referenceTrue ≠ .unsupported → whenTrue = fromEval frame referenceTrue)
    (onFalse : referenceFalse ≠ .unsupported → whenFalse = fromEval frame referenceFalse)
    (supported : (match (generalizing := false) shape with
      | .scalar (.boolean true) => referenceTrue
      | .scalar (.boolean false) => referenceFalse
      | _ => .stuck) ≠ .unsupported) :
    (match (generalizing := false) shape with
      | .scalar (.boolean true) => whenTrue
      | .scalar (.boolean false) => whenFalse
      | _ => .exec .stuck) =
    fromEval frame (match (generalizing := false) shape with
      | .scalar (.boolean true) => referenceTrue
      | .scalar (.boolean false) => referenceFalse
      | _ => .stuck) := by
  cases shape with
  | scalar scalar =>
    cases scalar with
    | boolean flag => cases flag <;> first | exact onTrue supported | exact onFalse supported
    | _ => rfl
  | _ => rfl

theorem threadBranches_agrees {world : World Declaration Identity Source} {frame : Frame world}
    {evaluate : Frame world → Xsh.Syntax.Expr → Valued world}
    {reference : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, Evaluable source → reference source ≠ .unsupported →
      evaluate frame source = fromEval frame (reference source)) :
    ∀ conditions otherwise, (∀ condition value, (condition, value) ∈ conditions → Evaluable condition) →
      (∀ condition value, (condition, value) ∈ conditions → Evaluable value) →
      Evaluable otherwise → branches reference conditions otherwise ≠ .unsupported →
        threadBranches evaluate frame conditions otherwise = fromEval frame (branches reference conditions otherwise)
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

theorem falseFirst_agrees {world : World Declaration Identity Source} {frame : Frame world}
    (shape : Value Declaration Identity Source) {whenFalse whenTrue : Valued world}
    {referenceFalse referenceTrue : Result Declaration Identity Source (Value Declaration Identity Source)}
    (onFalse : referenceFalse ≠ .unsupported → whenFalse = fromEval frame referenceFalse)
    (onTrue : referenceTrue ≠ .unsupported → whenTrue = fromEval frame referenceTrue)
    (supported : (match (generalizing := false) shape with
      | .scalar (.boolean false) => referenceFalse
      | .scalar (.boolean true) => referenceTrue
      | _ => .stuck) ≠ .unsupported) :
    (match (generalizing := false) shape with
      | .scalar (.boolean false) => whenFalse
      | .scalar (.boolean true) => whenTrue
      | _ => .exec .stuck) =
    fromEval frame (match (generalizing := false) shape with
      | .scalar (.boolean false) => referenceFalse
      | .scalar (.boolean true) => referenceTrue
      | _ => .stuck) := by
  cases shape with
  | scalar scalar =>
    cases scalar with
    | boolean flag => cases flag <;> first | exact onTrue supported | exact onFalse supported
    | _ => rfl
  | _ => rfl

theorem boolean_agrees {world : World Declaration Identity Source} {frame : Frame world}
    (shape : Value Declaration Identity Source) :
    (match (generalizing := false) shape with
      | .scalar (.boolean value) => Outcome.value (boolean value : Value Declaration Identity Source) frame
      | _ => .exec .stuck) =
    fromEval frame (match (generalizing := false) shape with
      | .scalar (.boolean value) => Result.value (boolean value : Value Declaration Identity Source)
      | _ => .stuck) := by
  cases shape with
  | scalar scalar =>
    cases scalar with
    | boolean flag => rfl
    | _ => rfl
  | _ => rfl

theorem threadStep_agrees (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (frame : Frame world) {recurse : Frame world → Xsh.Syntax.Expr → Valued world}
    {callFunction : Environment → Locals Declaration Identity Source → FunctionBody → Frame world → Valued world}
    {reference : Environment → Locals Declaration Identity Source → Xsh.Syntax.Expr →
      Result Declaration Identity Source (Value Declaration Identity Source)}
    (bodies : BodiesEvaluable context)
    (inner : ∀ expression, Evaluable expression →
      reference frame.environment frame.locals expression ≠ .unsupported →
        recurse frame expression = fromEval frame (reference frame.environment frame.locals expression))
    (calls : ∀ name defining parameters locals result,
      context.functions name = some (defining, parameters, .expression result .plain) → Evaluable result →
        reference defining locals result ≠ .unsupported →
          callFunction defining locals (.expression result .plain) frame = fromEval frame (reference defining locals result))
    (expression : Xsh.Syntax.Expr) (evaluable : Evaluable expression)
    (supported : step context frame.environment frame.store reference (fun _ _ _ _ => .unsupported) frame.locals
      expression ≠ .unsupported) :
    threadStep context recurse callFunction frame expression =
      fromEval frame (step context frame.environment frame.store reference (fun _ _ _ _ => .unsupported) frame.locals
        expression) := by
  cases evaluable with
  | literal value => rfl
  | ident name =>
    simp only [threadStep, step]
    cases frame.locals.lookup name with
    | some value => rfl
    | none =>
      dsimp only
      cases frame.environment.cellOf name with
      | ok cell =>
        dsimp only
        cases frame.store cell <;> rfl
      | error _ => rfl
  | list elements all =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (threadAll_agrees inner elements all) (fun _ _ _ => rfl) supported
  | record fields _ all =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (threadFields_agrees context inner fields all) (fun _ _ _ => rfl) supported
  | field base name based =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (inner base based) (fun _ _ _ => rfl) supported
  | index base key based keyed =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (inner base based) (fun _ _ restSupported =>
      lift_bind (inner key keyed) (fun _ _ _ => rfl) restSupported) supported
  | unary operator operand evaluated =>
    simp only [threadStep, step] at supported ⊢
    exact lift_bind (inner operand evaluated) (fun _ _ _ => rfl) supported
  | binary operator left right notFallback lefty righty =>
    cases operator
    case resultFallback => exact absurd rfl notFallback
    case and =>
      simp only [threadStep, step] at supported ⊢
      exact lift_bind (inner left lefty) (fun first _ firstSupported =>
        falseFirst_agrees first.eraseNominal (fun _ => rfl)
          (fun secondSupported => lift_bind (inner right righty)
            (fun second _ _ => boolean_agrees second.eraseNominal) secondSupported)
          firstSupported) supported
    case or =>
      simp only [threadStep, step] at supported ⊢
      exact lift_bind (inner left lefty) (fun first _ firstSupported =>
        branch_agrees first.eraseNominal (fun _ => rfl)
          (fun secondSupported => lift_bind (inner right righty)
            (fun second _ _ => boolean_agrees second.eraseNominal) secondSupported)
          firstSupported) supported
    all_goals
      simp only [threadStep, step] at supported ⊢
      exact lift_bind (inner left lefty) (fun _ _ restSupported =>
        lift_bind (inner right righty) (fun _ _ _ => rfl) restSupported) supported
  | ifExpr conditions otherwise tests values last =>
    simp only [threadStep, step] at supported ⊢
    exact threadBranches_agrees inner conditions otherwise tests values last supported
  | call name arguments all =>
    simp only [threadStep, step] at supported ⊢
    rcases hl : frame.locals.lookup name with _ | found
    · rcases hf : context.functions name with _ | ⟨defining, parameters, body⟩
      · rfl
      · rcases hp : positional arguments with _ | values
        · rfl
        · simp only [hl, hf, hp] at supported
          refine lift_bind
            (threadAll_agrees inner values
              (fun value inside => all value (positional_mem arguments values hp value inside)))
            (fun evaluated _ evaluatedSupported => ?_) supported
          by_cases same : parameters.length = evaluated.length
          · simp only [same, ↓reduceIte] at evaluatedSupported ⊢
            cases body with
            | expression result kind =>
              cases kind with
              | result => exact absurd rfl evaluatedSupported
              | plain =>
                exact calls name defining parameters _ result hf (bodies name defining parameters result hf)
                  evaluatedSupported
            | block statements kind => exact absurd rfl evaluatedSupported
          · simp only [same, ↓reduceIte]
            rfl
    · rfl

theorem lift_bind_eq {world : World Declaration Identity Source} {α β : Type} {frame : Frame world}
    {result : Result Declaration Identity Source α} {next : α → Frame world → Outcome world β}
    {next' : α → Result Declaration Identity Source β} (tail : ∀ value, next value frame = fromEval frame (next' value)) :
    (fromEval frame result).bind next = fromEval frame (result.bind next') := by
  cases result <;> first | exact tail _ | rfl

/-- With positional arguments only and defaults that leave the frame as it
is, the machine's placement and binding is `bindArguments`, so
`parameters_and_maps` describes it. -/
theorem bindSlots_lift {world : World Declaration Identity Source} (spelling : Xsh.Syntax.RawName → String)
    (evaluateDefault : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    ∀ (frame : Frame world) parameters values slots,
      placeParameters spelling parameters values [] = .ok slots →
        bindSlots (fun current default => fromEval current (evaluateDefault default)) frame slots =
          fromEval frame (bindArguments evaluateDefault parameters values)
  | _, [], [], slots, placed => by
    simp only [placeParameters, Except.ok.injEq] at placed
    subst placed
    rfl
  | _, [], _ :: _, _, placed => by simp [placeParameters] at placed
  | frame, .mk name annotation default rest :: more, values, slots, placed => by
    cases rest with
    | true =>
      cases more with
      | nil =>
        simp only [placeParameters, Except.ok.injEq] at placed
        subst placed
        rfl
      | cons _ _ => simp [placeParameters] at placed
    | false =>
      cases values with
      | cons value values =>
        simp only [placeParameters, List.lookup] at placed
        cases tail : placeParameters spelling more values [] with
        | error _ => rw [tail] at placed; cases placed
        | ok others =>
          rw [tail] at placed
          simp only [Except.ok.injEq] at placed
          subst placed
          simp only [bindSlots, bindArguments]
          rw [bindSlots_lift spelling evaluateDefault frame more values others tail]
          exact lift_bind_eq fun _ => rfl
      | nil =>
        cases default with
        | none => simp [placeParameters, List.lookup] at placed
        | some default =>
          simp only [placeParameters, List.lookup] at placed
          cases tail : placeParameters spelling more [] [] with
          | error _ => rw [tail] at placed; cases placed
          | ok others =>
            rw [tail] at placed
            simp only [Except.ok.injEq] at placed
            subst placed
            simp only [bindSlots, bindArguments]
            exact lift_bind_eq fun _ => by
              rw [bindSlots_lift spelling evaluateDefault frame more [] others tail]
              exact lift_bind_eq fun _ => rfl

theorem localClosure_none (environment : Environment) (name : Xsh.Syntax.RawName) :
    localClosure [] environment name = none := by
  unfold localClosure
  split <;> rfl

theorem keep_self {world : World Declaration Identity Source} (store : Store world) : keep store store = store := by
  funext cell
  simp only [keep]
  split <;> simp_all

/-- A result in a frame with the caller's store, returned to the caller as a
plain function's result, is that result in the caller. -/
theorem returnValue_fromEval {world : World Declaration Identity Source} (caller inner : Frame world)
    (sameStore : inner.store = caller.store) (result : Result Declaration Identity Source (Value Declaration Identity Source)) :
    returnValue caller (fromEval inner result) = fromEval caller result := by
  cases result with
  | value value =>
    show Outcome.value value { caller with store := keep caller.store inner.store } = _
    rw [sameStore, keep_self]
    rfl
  | failure failure =>
    show returnTo caller .plain (.failed failure inner) = _
    cases failure <;> (simp only [returnTo]; rw [sameStore, keep_self]; rfl)
  | _ => rfl

variable [DecidableEq Declaration]

/-- On a frame without local functions, wherever `CoreEval.evaluate` gives an
evaluable expression a meaning, the machine's frame-threading step gives the
same result with the frame unchanged. -/
theorem evalExpr_agrees {world : World Declaration Identity Source} (machine : Machine world)
    (bodies : BodiesEvaluable machine.context) :
    ∀ fuel (frame : Frame world) expression, frame.closures = [] → Initialized frame.environment frame.store →
      DefinitionsInitialized machine.context frame.store → Evaluable expression →
      evaluate machine.context frame.environment frame.store fuel frame.locals expression ≠ .unsupported →
        evalExpr machine fuel frame expression =
          fromEval frame (evaluate machine.context frame.environment frame.store fuel frame.locals expression)
  | 0, _, _, _, _, _, _, _ => rfl
  | fuel + 1, frame, expression, closureFree, initialized, definitions, evaluable, supported => by
    have agrees := threadStep_agrees machine.context frame
      (recurse := evalExpr machine fuel)
      (callFunction := callFunctionWith machine (evalExpr machine fuel) (callBlock machine fuel))
      (reference := fun environment locals => evaluate machine.context environment frame.store fuel locals) bodies
      (fun source evaluated sourceSupported =>
        evalExpr_agrees machine bodies fuel frame source closureFree initialized definitions evaluated sourceSupported)
      (fun name defining parameters locals result found evaluated resultSupported => by
        simp only [callFunctionWith]
        rw [evalExpr_agrees machine bodies fuel
          { frame with environment := defining, locals := locals, callerStore := some frame.store, writable := [] }
          result closureFree (definitions name defining parameters result found) definitions evaluated resultSupported]
        exact returnValue_fromEval frame
          { frame with environment := defining, locals := locals, callerStore := some frame.store, writable := [] } rfl _)
      expression evaluable supported
    refine Eq.trans ?_ agrees
    cases evaluable with
    | call name arguments all =>
      rcases hl : frame.locals.lookup name with _ | found
      · rcases hf : machine.context.functions name with _ | entry
        · exfalso
          apply supported
          simp only [evaluate, step, hl, hf]
        · simp only [evalExpr, closureFree, localClosure_none, hl, hf]
      · simp only [evalExpr, closureFree, localClosure_none, hl]
    | binary operator left right notFallback _ _ =>
      cases operator <;> first | exact absurd rfl notFallback | simp only [evalExpr]
    | ident name =>
      simp only [evalExpr, closureFree, localClosure_none]
      cases frame.locals.lookup name with
      | some _ => rfl
      | none =>
        cases resolved : frame.environment.cellOf name with
        | error _ => rfl
        | ok cell =>
          dsimp only
          cases held : frame.store cell with
          | none => exact absurd held (initialized name cell resolved)
          | some _ => rfl
    | record fields named _ => simp only [evalExpr, named, if_true]
    | _ => simp only [evalExpr]

end Bridge

section Root

theorem finished_valued {world : World Declaration Identity Source} {α : Type} {run : Nat → Outcome world α}
    (monotone : ∀ fuel extra, Valued.Refines (run fuel) (run (fuel + extra))) (first second : Nat)
    (firstFinished : run first ≠ .exec .outOfFuel) (secondFinished : run second ≠ .exec .outOfFuel) :
    run first = run second := by
  rcases Nat.le_total first second with smaller | larger
  · obtain ⟨extra, rfl⟩ := Nat.exists_eq_add_of_le smaller
    rcases monotone first extra with out | same
    · exact absurd out firstFinished
    · exact same
  · obtain ⟨extra, rfl⟩ := Nat.exists_eq_add_of_le larger
    rcases monotone second extra with out | same
    · exact absurd out secondFinished
    · exact same.symm

variable [DecidableEq Declaration]

theorem binary_threads {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (operator : Xsh.Syntax.BinaryOp) (left right : Xsh.Syntax.Expr)
    (notAnd : operator ≠ .and) (notOr : operator ≠ .or) (notFallback : operator ≠ .resultFallback) :
    evalExpr machine (fuel + 1) frame (.binary operator left right) =
      (evalExpr machine fuel frame left).bind fun first after =>
        (evalExpr machine fuel after right).bind fun second last =>
          fromEval last (binary machine.context operator first second) := by
  cases operator <;> first
    | exact absurd rfl notAnd
    | exact absurd rfl notOr
    | exact absurd rfl notFallback
    | simp only [evalExpr, threadStep]

theorem closure_call_threads {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (name : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.CallArgument) (closure : Closure)
    (unshadowed : frame.locals.lookup name = none)
    (found : localClosure frame.closures frame.environment name = some closure) :
    evalExpr machine (fuel + 1) frame (.call (.ident name) arguments) =
      invokeWith machine (evalExpr machine fuel) (callBlock machine fuel) frame closure arguments := by
  simp only [evalExpr, unshadowed, found]

/-- Store threading through the statement machine's expressions (SPEC 9.2: "A
writable capture must refer to mutable storage with exactly that type";
SPEC 1: "Evaluation order is source order"). On a frame without local
functions whose names, and those of every expression-bodied function's
defining environment, are initialized, wherever `CoreEval.evaluate` gives an
evaluable expression a meaning, the machine's step gives the same result with the frame unchanged,
so the evaluator's operator, failure and typing results hold for the
machine's values there. A finished expression result is the same at every
fuel, and every result keeps the cells it started with at their types and
leaves the environment and locals as it found them. A binary operator's
operands, a list's elements and a call's arguments evaluate left to right,
each in the frame the one before it left; a value block nested in an
expression runs as a block in the current frame; a local function's call
places its arguments through `invokeWith`, evaluating omitted defaults in the
frame the arguments left (with positional arguments only and defaults that
leave the frame as it is, this is `bindArguments`), and runs its body through `callBlock`, which returns through `returnTo`, so
the caller's cells come back as the callee left them from a call nested
anywhere in an expression; and a call in statement position is the same
evaluation. -/
theorem expression_store_threading {world : World Declaration Identity Source} (machine : Machine world) :
    (BodiesEvaluable machine.context →
      ∀ fuel (frame : Frame world) expression, frame.closures = [] → Initialized frame.environment frame.store →
        DefinitionsInitialized machine.context frame.store → Evaluable expression →
        evaluate machine.context frame.environment frame.store fuel frame.locals expression ≠ .unsupported →
          evalExpr machine fuel frame expression =
            fromEval frame (evaluate machine.context frame.environment frame.store fuel frame.locals expression)) ∧
    (∀ (frame : Frame world) expression first second,
      evalExpr machine first frame expression ≠ .exec .outOfFuel →
        evalExpr machine second frame expression ≠ .exec .outOfFuel →
          evalExpr machine first frame expression = evalExpr machine second frame expression) ∧
    (∀ fuel (frame : Frame world) expression, VKeeps frame.store (evalExpr machine fuel frame expression)) ∧
    (∀ fuel (frame : Frame world) expression, VRestores frame (evalExpr machine fuel frame expression)) ∧
    (∀ fuel (frame : Frame world) operator left right, operator ≠ .and → operator ≠ .or →
      operator ≠ .resultFallback →
        evalExpr machine (fuel + 1) frame (.binary operator left right) =
          (evalExpr machine fuel frame left).bind fun first after =>
            (evalExpr machine fuel after right).bind fun second last =>
              fromEval last (binary machine.context operator first second)) ∧
    (∀ fuel (frame : Frame world) elements,
      evalExpr machine (fuel + 1) frame (.list elements) =
        (threadAll (evalExpr machine fuel) frame elements).bind fun values after => .value (.list values) after) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) frame first rest,
      threadAll evaluate frame (first :: rest) =
        (evaluate frame first).bind fun value after =>
          (threadAll evaluate after rest).bind fun values last => .value (value :: values) last) ∧
    (∀ fuel (frame : Frame world) body, evalExpr machine (fuel + 1) frame (.valueBlock body) =
      runValue machine fuel frame (.valueBlock body)) ∧
    (∀ fuel (frame : Frame world) name arguments closure,
      frame.locals.lookup name = none → localClosure frame.closures frame.environment name = some closure →
        evalExpr machine (fuel + 1) frame (.call (.ident name) arguments) =
          invokeWith machine (evalExpr machine fuel) (callBlock machine fuel) frame closure arguments) ∧
    (∀ (evaluateDefault : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source))
        (frame : Frame world) parameters values slots,
      placeParameters machine.context.spelling parameters values [] = .ok slots →
        bindSlots (fun current default => fromEval current (evaluateDefault default)) frame slots =
          fromEval frame (bindArguments evaluateDefault parameters values)) ∧
    (∀ fuel (caller : Frame world) scope defining parameters writable body kind,
      callBlock machine (fuel + 1) caller scope defining parameters writable body kind =
        returnTo caller kind
          (runBody machine fuel { Frame.ofScope defining caller.store parameters (some caller.store) scope with writable }
            body)) ∧
    (∀ (caller callee : Frame world) kind value,
      ∃ result, returnTo caller kind (.returned value callee) =
        .value result { caller with store := keep caller.store callee.store }) ∧
    (∀ (before after : Store world) cell, before cell ≠ none → keep before after cell = after cell) ∧
    (∀ fuel (frame : Frame world) name arguments,
      runValue machine (fuel + 1) frame (.call (.ident name) arguments) =
        evalExpr machine fuel frame (.call (.ident name) arguments)) := by
  refine ⟨fun bodies => evalExpr_agrees machine bodies,
    fun frame expression => finished_valued fun fuel extra => (refining machine fuel extra).expr frame expression,
    fun fuel => (persisting machine fuel).expr,
    fun fuel => (restoring machine fuel).expr,
    fun fuel frame operator left right notAnd notOr notFallback =>
      binary_threads machine fuel frame operator left right notAnd notOr notFallback,
    fun _ _ _ => by simp only [evalExpr, threadStep],
    fun _ _ _ _ => rfl,
    fun _ _ _ => by simp only [evalExpr],
    fun fuel frame name arguments closure unshadowed found =>
      closure_call_threads machine fuel frame name arguments closure unshadowed found,
    fun evaluateDefault => bindSlots_lift machine.context.spelling evaluateDefault,
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
def observe (evaluate : Frame witnessWorld → Valued witnessWorld) : Option (List Int × Int) :=
  counterFrame.bind fun frame =>
    match evaluate frame with
    | .value value after => (intsOf value).bind fun ints => (countIn frame after.store).map fun count => (ints, count)
    | _ => none

def threaded (frame : Frame witnessWorld) : Xsh.Syntax.Expr → Valued witnessWorld :=
  evalExpr fullMachine 60 frame

/-- A list step that evaluates every element in the frame before the list and
keeps that frame, so nested writes are dropped. -/
def droppingList (frame : Frame witnessWorld) (elements : List Xsh.Syntax.Expr) : Valued witnessWorld :=
  (threadAll (fun _ element => evalExpr fullMachine 60 frame element) frame elements).bind fun values _ =>
    .value (.list values) frame

/-- A list step that threads the frame through its elements right to left. -/
def rightToLeftList (frame : Frame witnessWorld) (elements : List Xsh.Syntax.Expr) : Valued witnessWorld :=
  (threadAll (evalExpr fullMachine 60) frame elements.reverse).bind fun values after =>
    .value (.list values.reverse) after

/-- A binary step that evaluates its right operand first. -/
def rightFirstBinary (frame : Frame witnessWorld) (operator : Xsh.Syntax.BinaryOp) (left right : Xsh.Syntax.Expr) :
    Valued witnessWorld :=
  (evalExpr fullMachine 60 frame right).bind fun second after =>
    (evalExpr fullMachine 60 after left).bind fun first last =>
      fromEval last (binary fullMachine.context operator first second)

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

/-- `[{ count = 2; 1 }]`, a value block nested in a list. -/
def nestedBlock : Xsh.Syntax.Expr := .list [.valueBlock [set countName "2", .expression (int "1")]]

/-- `var count = 0; let r = [{ count = 2; 1 }]` gives `[1]` and leaves
`count = 2`: the block runs in the current frame and writes the cell that
existed before it. -/
theorem nested_block_writes :
    listInts rName (runFull [varOf countName "0", letR nestedBlock]) = some [1] ∧
      readInt countName (runFull [varOf countName "0", letR nestedBlock]) = some 2 ∧
      observe (fun frame => threaded frame nestedBlock) = some ([1], 2) := by
  decide +kernel

/-- Running the nested block as a call, in a frame whose caller store holds
every cell, loses the write: the assignment is refused. -/
def blockAsCall (frame : Frame witnessWorld) : Valued witnessWorld :=
  runValue fullMachine 60 (Frame.ofScope frame.environment frame.store frame.locals (some frame.store) frame.scope)
    (.valueBlock [set countName "2", .expression (int "1")])

/-- Running the block and keeping the frame before it also loses the write. -/
def blockDroppingStore (frame : Frame witnessWorld) : Valued witnessWorld :=
  (runValue fullMachine 60 frame (.valueBlock [set countName "2", .expression (int "1")])).bind fun value _ =>
    .value (.list [value]) frame

theorem block_write_loss_rejected :
    observe blockAsCall = none ∧ observe blockDroppingStore = some ([1], 0) := by
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
`expression_store_threading` the machine gives the same with the frame
unchanged. -/
theorem bridge_instance :
    Evaluable bridged ∧ start.closures = [] ∧
      supportedBy (evaluate machine.context start.environment start.store 10 start.locals bridged) = true ∧
      evaluatedInts (evaluate machine.context start.environment start.store 10 start.locals bridged) = some [11] := by
  refine ⟨.binary _ _ _ (by decide)
      (.index _ _ (.list _ fun element inside => ?_) (.unary _ _ (.literal _)))
      (.binary _ _ _ (by decide) (.literal _) (.literal _)), rfl, by decide +kernel, by decide +kernel⟩
  simp only [List.map, List.mem_cons, List.not_mem_nil, or_false] at inside
  rcases inside with same | same <;> (subst same; exact .literal _)

end Witness

end Xsh.Proofs.Core.Threading
