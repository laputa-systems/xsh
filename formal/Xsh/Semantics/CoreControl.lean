import Xsh.Semantics.CoreEval

namespace Xsh.Semantics.CoreControl

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval

variable {Declaration Identity Source : Type}

/-- What the statement machine is given: the expression context, the store's
decisions, the runtime error values, compound assignment, the checker's
binder, cell and type for each declared name and argv conversion (from
`resolution`), and the error a failed `assert` raises. -/
structure Machine (world : World Declaration Identity Source) where
  context : Context Declaration Identity Source
  decisions : Decisions world
  raises : Raises Declaration Identity Source
  operate : Operate Declaration Identity Source
  resolution : Oracles world
  assertion : Option (Value Declaration Identity Source) → Value Declaration Identity Source

/-- The resolution oracles with the context's spellings. -/
def Machine.names {world : World Declaration Identity Source} (machine : Machine world) : Oracles world :=
  { machine.resolution with spelling := machine.context.spelling }

/-- A statement runs in an environment, a store, an environment-variable
overlay and the locals of the function it is in. Inside a function body
`callerStore` is the store at the call: its cells are read but not written. -/
structure Frame (world : World Declaration Identity Source) where
  environment : Environment
  store : Store world
  overlay : Overlay
  locals : Locals Declaration Identity Source
  callerStore : Option (Store world)

/-- The outcome of running statements. -/
inductive Exec (world : World Declaration Identity Source) where
  | normal (frame : Frame world)
  | broke (frame : Frame world)
  | continued (frame : Frame world)
  | returned (value : Value Declaration Identity Source) (frame : Frame world)
  | failed (failure : Failure Declaration Identity Source) (frame : Frame world)
  | internal (fault : InternalFault) (frame : Frame world)
  | stuck
  | unsupported
  | outOfFuel

def Exec.andThen {world : World Declaration Identity Source} :
    Exec world → (Frame world → Exec world) → Exec world
  | .normal frame, next => next frame
  | other, _ => other

/-- After one loop iteration: `normal` and `continue` go on, `break` ends the
loop normally, and anything else leaves it. -/
def Exec.iterate {world : World Declaration Identity Source} :
    Exec world → (Frame world → Exec world) → Exec world
  | .normal frame, next | .continued frame, next => next frame
  | .broke frame, _ => .normal frame
  | other, _ => other

def fromResult {world : World Declaration Identity Source} (frame : Frame world) :
    Result Declaration Identity Source (Value Declaration Identity Source) →
      (Value Declaration Identity Source → Exec world) → Exec world
  | .value value, next => next value
  | .failure failure, _ => .failed failure frame
  | .stuck, _ => .stuck
  | .unsupported, _ => .unsupported
  | .outOfFuel, _ => .outOfFuel

/-- A store write: committed continues in `environment` and `locals`. -/
def fromStep {world : World Declaration Identity Source} (frame : Frame world) (environment : Environment)
    (locals : Locals Declaration Identity Source) : StoreStep world → Exec world
  | .committed store => .normal { frame with environment := environment, store := store, locals := locals }
  | .failed failure store => .failed failure { frame with store := store }
  | .internal fault store => .internal fault { frame with store := store }

/-- Cells present in `before`, as `after` holds them; cells allocated since
are dropped. -/
def keep {world : World Declaration Identity Source} (before after : Store world) : Store world :=
  fun cell => match before cell with
    | some _ => after cell
    | none => none

def Frame.restore {world : World Declaration Identity Source} (outer inner : Frame world) : Frame world :=
  { inner with environment := outer.environment, locals := outer.locals, store := keep outer.store inner.store }

/-- Leaving a scope restores the outer environment and locals and drops the
cells allocated inside it, whatever the outcome. -/
def withinScope {world : World Declaration Identity Source} (outer : Frame world) : Exec world → Exec world
  | .normal inner => .normal (outer.restore inner)
  | .broke inner => .broke (outer.restore inner)
  | .continued inner => .continued (outer.restore inner)
  | .returned value inner => .returned value (outer.restore inner)
  | .failed failure inner => .failed failure (outer.restore inner)
  | .internal fault inner => .internal fault (outer.restore inner)
  | other => other

def Frame.enter {world : World Declaration Identity Source} (frame : Frame world) : Frame world :=
  { frame with environment := { frame.environment with names := enterScope frame.environment.names } }

/-- A name bound in the environment hides a local of the same name. -/
def hide (names : List Xsh.Syntax.RawName) (locals : Locals Declaration Identity Source) :
    Locals Declaration Identity Source :=
  locals.filter fun entry => !(names.contains entry.1)

inductive Match (Declaration Identity Source : Type) where
  | matched (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source))
  | failed
  | unsupported

def Match.both : Match Declaration Identity Source → (Unit → Match Declaration Identity Source) →
    Match Declaration Identity Source
  | .matched first, next =>
    match next () with
    | .matched second => .matched (first ++ second)
    | other => other
  | other, _ => other

mutual
  /-- The SPEC 6.10 matcher for wildcard, binding, literal, record and list
  patterns and groups. -/
  def matchPattern (context : Context Declaration Identity Source) :
      Xsh.Syntax.Pattern → Value Declaration Identity Source → Match Declaration Identity Source
    | .wildcard, _ => .matched []
    | .binding name, value => .matched [(name, value)]
    | .group inner, value => matchPattern context inner value
    | .literal written, value =>
      match literal context.literals written with
      | .value expected =>
        match binary context .eq value expected with
        | .value result =>
          match result.eraseNominal with
          | .scalar (.boolean true) => .matched []
          | .scalar (.boolean false) => .failed
          | _ => .unsupported
        | _ => .unsupported
      | _ => .unsupported
    | .record fields _, value =>
      match value.eraseNominal with
      | .record actual => matchFields context fields actual
      | _ => .failed
    | .list elements rest, value =>
      match listValues value with
      | some items =>
        match rest with
        | none => if elements.length = items.length then matchItems context elements items else .failed
        | some tail =>
          if elements.length ≤ items.length then
            (matchItems context elements (items.take elements.length)).both fun _ =>
              matchPattern context tail (.list (items.drop elements.length))
          else .failed
      | none => .failed
    | _, _ => .unsupported

  def matchFields (context : Context Declaration Identity Source) :
      List (Xsh.Syntax.RawName × Xsh.Syntax.Pattern) → List (String × Value Declaration Identity Source) →
        Match Declaration Identity Source
    | [], _ => .matched []
    | (label, pattern) :: rest, actual =>
      match actual.lookup (context.spelling label) with
      | none => .failed
      | some found => (matchPattern context pattern found).both fun _ => matchFields context rest actual

  def matchItems (context : Context Declaration Identity Source) :
      List Xsh.Syntax.Pattern → List (Value Declaration Identity Source) → Match Declaration Identity Source
    | [], _ => .matched []
    | pattern :: rest, item :: items => (matchPattern context pattern item).both fun _ => matchItems context rest items
    | _ :: _, [] => .failed
end

def catchAll : Xsh.Syntax.Pattern → Bool
  | .wildcard | .binding _ => true
  | .group inner => catchAll inner
  | _ => false

/-- `if let` and `while let`: a pattern that cannot fail is an optional
binding, which takes the other branch on `null` (SPEC 8.6). -/
def conditionMatch (context : Context Declaration Identity Source) (pattern : Xsh.Syntax.Pattern)
    (value : Value Declaration Identity Source) : Match Declaration Identity Source :=
  if catchAll pattern then
    match value.eraseNominal with
    | .scalar .null => .failed
    | _ => matchPattern context pattern value
  else matchPattern context pattern value

/-- The checker's binder, cell and type for each capture. -/
def resolveCaptures {world : World Declaration Identity Source} (resolution : Oracles world)
    (environment : Environment) :
    List (Xsh.Syntax.RawName × Value Declaration Identity Source) →
      Option (List (Declared × Binding Declaration Identity Source))
  | [] => some []
  | (captured, value) :: rest =>
    match resolution.declaration environment captured, resolveCaptures resolution environment rest with
    | some (binder, cell, type), some entries => some (((captured, binder, cell), (cell, type, value)) :: entries)
    | _, _ => none

variable [DecidableEq Declaration]

/-- Bind pattern captures as immutable cells in `frame`, which has already
entered its scope. -/
def bindCaptures {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)) : Exec world :=
  match resolveCaptures machine.resolution frame.environment captures with
  | none => .internal .malformedTarget frame
  | some entries =>
    fromStep frame (extend frame.environment (entries.map Prod.fst)) (hide (captures.map Prod.fst) frame.locals)
      (declareAll world machine.decisions frame.store .immutable frame.store (entries.map Prod.snd))

/-- Bind a `for` or `let` target in `frame`. -/
def bindTarget {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (mutability : Mutability) (target : Xsh.Syntax.BindingTarget) (value : Value Declaration Identity Source) :
    Exec world :=
  match resolveTarget machine.names frame.environment target with
  | none => .internal .malformedTarget frame
  | some (resolved, declared) =>
    fromStep frame (extend frame.environment declared) (hide (declared.map Prod.fst) frame.locals)
      (declareTarget world machine.decisions frame.store mutability resolved value)

/-- A name or field-path assignment target: its root and field labels. -/
def fieldPath (spelling : Xsh.Syntax.RawName → String) :
    Xsh.Syntax.AssignTarget → Option (Xsh.Syntax.RawName × List Selector)
  | .name name => some (name, [])
  | .field base label => (fieldPath spelling base).map fun (root, path) => (root, path ++ [.field (spelling label)])
  | _ => none

/-- The value of a function body that ends: its `return` value, or its tail
expression. -/
def bodyValue {world : World Declaration Identity Source} :
    Exec world → Result Declaration Identity Source (Value Declaration Identity Source)
  | .returned value _ => .value value
  | .failed failure _ => .failure failure
  | .unsupported => .unsupported
  | .outOfFuel => .outOfFuel
  | _ => .stuck

mutual
  /-- Expressions, with block function bodies run as statements. -/
  def evalExpr {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Store world → Environment → Locals Declaration Identity Source → Xsh.Syntax.Expr →
        Result Declaration Identity Source (Value Declaration Identity Source)
    | 0, _, _, _, _ => .outOfFuel
    | fuel + 1, store, environment, locals, expression =>
      step machine.context environment store (fun inner innerLocals => evalExpr machine fuel store inner innerLocals)
        (fun defining parameters body => callBlock machine fuel store defining parameters body) locals expression

  /-- A block function body runs in its defining environment with its
  parameters as locals; it may not write the caller's cells. -/
  def callBlock {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Store world → Environment → Locals Declaration Identity Source → List Xsh.Syntax.Statement →
        Result Declaration Identity Source (Value Declaration Identity Source)
    | 0, _, _, _, _ => .outOfFuel
    | fuel + 1, store, defining, parameters, body =>
      bodyValue (runBody machine fuel ⟨defining, store, [], parameters, some store⟩ body)

  /-- A function body: its statements in a scope, the last one a tail
  expression whose value is returned. -/
  def runBody {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, body =>
      withinScope frame (runTail machine fuel frame.enter body)

  def runTail {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | _ + 1, _, [] => .stuck
    | fuel + 1, frame, [.expression value] =>
      fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals value) fun result =>
        .returned result frame
    | fuel + 1, frame, [.tailName name] =>
      fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals (.ident name)) fun result =>
        .returned result frame
    | fuel + 1, frame, statement :: rest =>
      (runStatement machine fuel frame statement).andThen fun next => runTail machine fuel next rest

  def runStatements {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | _ + 1, frame, [] => .normal frame
    | fuel + 1, frame, statement :: rest =>
      (runStatement machine fuel frame statement).andThen fun next => runStatements machine fuel next rest

  /-- A block runs in its own scope. -/
  def runBlock {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, body => withinScope frame (runStatements machine fuel frame.enter body)

  /-- A body that first binds pattern captures, in its own scope. -/
  def runBound {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List (Xsh.Syntax.RawName × Value Declaration Identity Source) →
        List Xsh.Syntax.Statement → Exec world
    | 0, _, _, _ => .outOfFuel
    | fuel + 1, frame, captures, body =>
      withinScope frame ((bindCaptures machine frame.enter captures).andThen fun bound =>
        runStatements machine fuel bound body)

  def runStatement {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, statement =>
      match statement with
      | .binding kind target _ initializer =>
        fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals initializer) fun value =>
          bindTarget machine frame (bindingMutability kind) target value
      | .assign (.env key) .set value =>
        match frame.callerStore with
        | some _ => .unsupported
        | none =>
          fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals value) fun result =>
            match machine.resolution.argv result with
            | none => .internal .unconvertibleEnvironment frame
            | some bytes =>
              match setEnvironment machine.raises frame.overlay (machine.context.spelling key).toUTF8.toList bytes with
              | .ok overlay => .normal { frame with overlay := overlay }
              | .error failure => .failed failure frame
      | .assign target op value =>
        match fieldPath machine.context.spelling target with
        | none => .unsupported
        | some (root, path) =>
          match frame.environment.cellOf root with
          | .error fault => .internal (.unresolvedName fault) frame
          | .ok cell =>
            match frame.callerStore.bind fun caller => caller cell with
            | some _ => .unsupported
            | none =>
              fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals value) fun result =>
                fromStep frame frame.environment frame.locals
                  (assignPath world machine.decisions machine.raises machine.operate frame.store cell path op result)
      | .expression (.valueBlock body) => runBlock machine fuel frame body
      | .expression value =>
        fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals value) fun _ => .normal frame
      | .returnValue (some value) =>
        fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals value) fun result =>
          .returned result frame
      | .ifStmt branches otherwise => runIf machine fuel frame branches otherwise
      | .whileStmt condition body => runWhile machine fuel frame condition body
      | .forStmt target iterable body =>
        fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals iterable) fun source =>
          match listValues source with
          | some items => runFor machine fuel frame target items body
          | none => .unsupported
      | .loopStmt body => runLoop machine fuel frame body
      | .breakStmt none => .broke frame
      | .continueStmt => .continued frame
      | .matchStmt subject arms =>
        fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals subject) fun value =>
          runMatch machine fuel frame value arms
      | .assert condition message =>
        fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals condition) fun tested =>
          match tested.eraseNominal with
          | .scalar (.boolean true) => .normal frame
          | .scalar (.boolean false) =>
            match message with
            | none => .failed (.raised (machine.assertion none)) frame
            | some text =>
              fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals text) fun written =>
                .failed (.raised (machine.assertion (some written))) frame
          | _ => .unsupported
      | .sugar _ expansion => runStatement machine fuel frame expansion
      | _ => .unsupported

  /-- `if`, `else if` and `else`: each branch is its own scope. -/
  def runIf {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List (Xsh.Syntax.Expr × List Xsh.Syntax.Statement) →
        Option (List Xsh.Syntax.Statement) → Exec world
    | 0, _, _, _ => .outOfFuel
    | _ + 1, frame, [], none => .normal frame
    | fuel + 1, frame, [], some otherwise => runBlock machine fuel frame otherwise
    | fuel + 1, frame, (.patternCondition subject pattern, body) :: rest, otherwise =>
      fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals subject) fun value =>
        match conditionMatch machine.context pattern value with
        | .matched captures => runBound machine fuel frame captures body
        | .failed => runIf machine fuel frame rest otherwise
        | .unsupported => .unsupported
    | fuel + 1, frame, (condition, body) :: rest, otherwise =>
      fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals condition) fun tested =>
        match tested.eraseNominal with
        | .scalar (.boolean true) => runBlock machine fuel frame body
        | .scalar (.boolean false) => runIf machine fuel frame rest otherwise
        | _ => .unsupported

  /-- `while` and `while let`: each iteration's body is its own scope; `break`
  ends this loop and `continue` starts its next test. -/
  def runWhile {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.Expr → List Xsh.Syntax.Statement → Exec world
    | 0, _, _, _ => .outOfFuel
    | fuel + 1, frame, .patternCondition subject pattern, body =>
      fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals subject) fun value =>
        match conditionMatch machine.context pattern value with
        | .matched captures =>
          (runBound machine fuel frame captures body).iterate fun next =>
            runWhile machine fuel next (.patternCondition subject pattern) body
        | .failed => .normal frame
        | .unsupported => .unsupported
    | fuel + 1, frame, condition, body =>
      fromResult frame (evalExpr machine fuel frame.store frame.environment frame.locals condition) fun tested =>
        match tested.eraseNominal with
        | .scalar (.boolean true) =>
          (runBlock machine fuel frame body).iterate fun next => runWhile machine fuel next condition body
        | .scalar (.boolean false) => .normal frame
        | _ => .unsupported

  /-- `for` over a list snapshot: each item is bound immutably in that
  iteration's scope. -/
  def runFor {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.BindingTarget → List (Value Declaration Identity Source) →
        List Xsh.Syntax.Statement → Exec world
    | 0, _, _, _, _ => .outOfFuel
    | _ + 1, frame, _, [], _ => .normal frame
    | fuel + 1, frame, target, item :: rest, body =>
      (withinScope frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
          runStatements machine fuel bound body)).iterate fun next => runFor machine fuel next target rest body

  def runLoop {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, body =>
      (runBlock machine fuel frame body).iterate fun next => runLoop machine fuel next body

  /-- `match`: arms are tried in order; a false guard moves to the next arm. -/
  def runMatch {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Value Declaration Identity Source →
        List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × List Xsh.Syntax.Statement) → Exec world
    | 0, _, _, _ => .outOfFuel
    | _ + 1, _, _, [] => .stuck
    | fuel + 1, frame, value, (pattern, guard, body) :: rest =>
      match matchPattern machine.context pattern value with
      | .failed => runMatch machine fuel frame value rest
      | .unsupported => .unsupported
      | .matched captures =>
        match guard with
        | none => runBound machine fuel frame captures body
        | some condition =>
          match bindCaptures machine frame.enter captures with
          | .normal bound =>
            fromResult frame (evalExpr machine fuel bound.store bound.environment bound.locals condition) fun tested =>
              match tested.eraseNominal with
              | .scalar (.boolean true) => runBound machine fuel frame captures body
              | .scalar (.boolean false) => runMatch machine fuel frame value rest
              | _ => .unsupported
          | other => withinScope frame other
end

end Xsh.Semantics.CoreControl
