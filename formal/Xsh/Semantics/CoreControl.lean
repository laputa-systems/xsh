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
  /-- How far a call's activation is from its caller's. -/
  activationStep : Nat
  /-- The `Result` a conversion `value as T` gives (SPEC 6.11). -/
  conversion : Value Declaration Identity Source → Xsh.Syntax.TypeExpr → Option (Value Declaration Identity Source)

/-- The resolution oracles with the context's spellings; a declared name's
cell is its binder in the given activation. -/
def Machine.namesAt {world : World Declaration Identity Source} (machine : Machine world) (activation : Nat) :
    Oracles world :=
  { machine.resolution with
    spelling := machine.context.spelling
    declaration := fun environment declared =>
      (machine.resolution.declaration environment declared).map fun found =>
        (found.1, ⟨activation, found.1⟩, found.2.2) }

/-- A registered `defer` or `errdefer` action, with the environment and locals
it resolved its names in. -/
structure Deferred (Declaration Identity Source : Type) where
  kind : Xsh.Syntax.DeferKind
  action : Xsh.Syntax.Expr
  environment : Environment
  locals : Locals Declaration Identity Source

/-- A statement runs in an environment, a store, an environment-variable
overlay and the locals of the function it is in. Inside a function body
`callerStore` is the store at the call: its cells are read but not written.
`activation` numbers the call, `deferred` holds the current block's actions,
newest first, `reported` the cleanup failures that were not primary, and
`waited` the retry delays taken. -/
structure Frame (world : World Declaration Identity Source) where
  environment : Environment
  store : Store world
  overlay : Overlay
  locals : Locals Declaration Identity Source
  callerStore : Option (Store world)
  activation : Nat
  deferred : List (Deferred Declaration Identity Source)
  reported : List (Failure Declaration Identity Source)
  /-- The delays `retry` attempts waited, in order; a delay is observed, not
  slept. -/
  waited : List (Value Declaration Identity Source)

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
  { inner with
    environment := outer.environment
    locals := outer.locals
    store := keep outer.store inner.store
    deferred := outer.deferred }

/-- After a scope's deferred actions, leaving it restores the outer
environment, locals and deferred actions and drops the cells allocated inside
it, whatever the outcome. -/
def withinScope {world : World Declaration Identity Source} (outer : Frame world) : Exec world → Exec world
  | .normal inner => .normal (outer.restore inner)
  | .broke inner => .broke (outer.restore inner)
  | .continued inner => .continued (outer.restore inner)
  | .returned value inner => .returned value (outer.restore inner)
  | .failed failure inner => .failed failure (outer.restore inner)
  | .internal fault inner => .internal fault (outer.restore inner)
  | other => other

def Frame.enter {world : World Declaration Identity Source} (frame : Frame world) : Frame world :=
  { frame with environment := { frame.environment with names := enterScope frame.environment.names }, deferred := [] }

def frameOf {world : World Declaration Identity Source} : Exec world → Option (Frame world)
  | .normal frame | .broke frame | .continued frame | .returned _ frame | .failed _ frame | .internal _ frame =>
    some frame
  | _ => none

/-- The actions a scope runs as it is left: those of every exit but an
internal fault. -/
def pendingOf {world : World Declaration Identity Source} :
    Exec world → List (Deferred Declaration Identity Source)
  | .normal frame | .broke frame | .continued frame | .returned _ frame | .failed _ frame => frame.deferred
  | _ => []

def isErr (value : Value Declaration Identity Source) : Bool :=
  match value.eraseNominal with
  | .err _ => true
  | _ => false

/-- A block leaves with an error when a failure leaves it or the function
returns an `Err` through it (SPEC 8.7). -/
def exitsWithError {world : World Declaration Identity Source} : Exec world → Bool
  | .failed _ _ => true
  | .returned value _ => isErr value
  | _ => false

def Exec.withStore {world : World Declaration Identity Source} (store : Store world) : Exec world → Exec world
  | .normal frame => .normal { frame with store := store }
  | .broke frame => .broke { frame with store := store }
  | .continued frame => .continued { frame with store := store }
  | .returned value frame => .returned value { frame with store := store }
  | .failed failure frame => .failed failure { frame with store := store }
  | .internal fault frame => .internal fault { frame with store := store }
  | other => other

/-- A deferred action failed: an earlier failure stays primary and the new one
is reported; otherwise the new one becomes primary (SPEC 8.7). -/
def cleanupFailed {world : World Declaration Identity Source} (failure : Failure Declaration Identity Source)
    (store : Store world) : Exec world → Exec world
  | .failed primary frame => .failed primary { frame with store := store, reported := frame.reported ++ [failure] }
  | .normal frame | .broke frame | .continued frame | .returned _ frame | .internal _ frame =>
    .failed failure { frame with store := store }
  | other => other

/-- The frame an action runs in: the scope's store as it is left, with the
names the action resolved at registration. -/
def Deferred.frame {world : World Declaration Identity Source} (entry : Deferred Declaration Identity Source)
    (scope : Frame world) : Frame world :=
  { scope with environment := entry.environment, locals := entry.locals, deferred := [] }

def Deferred.body (entry : Deferred Declaration Identity Source) : List Xsh.Syntax.Statement :=
  match entry.action with
  | .valueBlock body => body
  | action => [.expression action]

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
  match resolveCaptures (machine.namesAt frame.activation) frame.environment captures with
  | none => .internal .malformedTarget frame
  | some entries =>
    fromStep frame (extend frame.environment (entries.map Prod.fst)) (hide (captures.map Prod.fst) frame.locals)
      (declareAll world machine.decisions frame.store .immutable frame.store (entries.map Prod.snd))

/-- Bind a `for` or `let` target in `frame`. -/
def bindTarget {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (mutability : Mutability) (target : Xsh.Syntax.BindingTarget) (value : Value Declaration Identity Source) :
    Exec world :=
  match resolveTarget (machine.namesAt frame.activation) frame.environment target with
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

/-- A plain value a `Result` function returns is wrapped in `Ok` (SPEC 8.5). -/
def wrapOk (value : Value Declaration Identity Source) : Value Declaration Identity Source :=
  match value.eraseNominal with
  | .ok _ | .err _ => value
  | _ => .ok value

/-- A function's call result from its body's outcome: a `Result` function
returns a raised failure, whether from `?`, an assertion or a primary cleanup
failure, as its `Err`; any other function fails the call with it. -/
def boundary {world : World Declaration Identity Source} :
    ReturnKind → Exec world → Result Declaration Identity Source (Value Declaration Identity Source)
  | .result, .returned value _ => .value (wrapOk value)
  | .plain, .returned value _ => .value value
  | .result, .failed (.raised error) _ => .value (.err error)
  | _, .failed failure _ => .failure failure
  | _, .unsupported => .unsupported
  | _, .outOfFuel => .outOfFuel
  | _, _ => .stuck

/-- An expression's value with the frame it leaves, or the outcome that
leaves instead. -/
inductive Valued (world : World Declaration Identity Source) where
  | value (value : Value Declaration Identity Source) (frame : Frame world)
  | exec (outcome : Exec world)

def Valued.bind {world : World Declaration Identity Source} :
    Valued world → (Value Declaration Identity Source → Frame world → Valued world) → Valued world
  | .value result frame, next => next result frame
  | .exec outcome, _ => .exec outcome

def Valued.toExec {world : World Declaration Identity Source} :
    Valued world → (Value Declaration Identity Source → Frame world → Exec world) → Exec world
  | .value result frame, next => next result frame
  | .exec outcome, _ => outcome

def Exec.thenValued {world : World Declaration Identity Source} :
    Exec world → (Frame world → Valued world) → Valued world
  | .normal frame, next => next frame
  | other, _ => .exec other

def resultThen {world : World Declaration Identity Source} {α : Type} (frame : Frame world) :
    Result Declaration Identity Source α → (α → Valued world) → Valued world
  | .value result, next => next result
  | .failure failure, _ => .exec (.failed failure frame)
  | .stuck, _ => .exec .stuck
  | .unsupported, _ => .exec .unsupported
  | .outOfFuel, _ => .exec .outOfFuel

def fromEval {world : World Declaration Identity Source} (frame : Frame world)
    (result : Result Declaration Identity Source (Value Declaration Identity Source)) : Valued world :=
  resultThen frame result fun value => .value value frame

/-- Leave a value block through `leave`: a tail value survives only a normal
exit. -/
def settle {world : World Declaration Identity Source} (leave : Exec world → Exec world) : Valued world → Valued world
  | .value result inner =>
    match leave (.normal inner) with
    | .normal outer => .value result outer
    | other => .exec other
  | .exec other => .exec (leave other)

/-- `?` (SPEC 8.3): `Ok` unwraps and `Err` raises its error. -/
def propagate (value : Value Declaration Identity Source) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  match value.eraseNominal with
  | .ok payload => .value payload
  | .err error => .failure (.raised error)
  | _ => .unsupported

/-- `try { ... }` (SPEC 8.8): a raised failure becomes `Err`, normal completion
`Ok` of the tail; runtime and type failures, and every other exit, pass on. -/
def capture {world : World Declaration Identity Source} : Valued world → Valued world
  | .value value frame => .value (.ok value) frame
  | .exec (.failed (.raised error) frame) => .value (.err error) frame
  | other => other

/-- The value of an effectful expression used inside a pure one. -/
def valuedResult {world : World Declaration Identity Source} :
    Valued world → Result Declaration Identity Source (Value Declaration Identity Source)
  | .value value _ => .value value
  | .exec (.failed failure _) => .failure failure
  | .exec .outOfFuel => .outOfFuel
  | .exec .stuck | .exec (.internal _ _) => .stuck
  | .exec _ => .unsupported

/-- Whether `retry ... on (pattern)` retries an error. -/
def retryable (context : Context Declaration Identity Source) :
    Option Xsh.Syntax.Pattern → Value Declaration Identity Source → Option Bool
  | none, _ => some true
  | some pattern, error =>
    match matchPattern context pattern error with
    | .matched _ => some true
    | .failed => some false
    | .unsupported => none

/-- One `retry` attempt's outcome: success is `Ok`, a selected raised failure
takes the next delay and goes on through `next`, and the last or an
unselected one is `Err`. -/
def retryStep {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (pattern : Option Xsh.Syntax.Pattern) (delays : List (Value Declaration Identity Source))
    (next : Frame world → List (Value Declaration Identity Source) → Valued world) : Valued world → Valued world
  | .value result after => .value (.ok result) after
  | .exec (.failed (.raised error) after) =>
    match retryable context pattern error with
    | some true =>
      match delays with
      | delay :: rest => next { after with waited := after.waited ++ [delay] } rest
      | [] => .value (.err error) after
    | some false => .value (.err error) after
    | none => .exec .unsupported
  | other => other

mutual
  /-- Expressions, with block function bodies run as statements. -/
  def evalExpr {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Nat → Store world → Environment → Locals Declaration Identity Source → Xsh.Syntax.Expr →
        Result Declaration Identity Source (Value Declaration Identity Source)
    | 0, _, _, _, _, _ => .outOfFuel
    | fuel + 1, activation, store, environment, locals, expression =>
      match expression with
      | .tryExpr inner => (evalExpr machine fuel activation store environment locals inner).bind propagate
      | .convert inner target =>
        (evalExpr machine fuel activation store environment locals inner).bind fun value =>
          match machine.conversion value target with
          | some converted => propagate converted
          | none => .unsupported
      | .valueBlock _ | .capture _ | .retry _ _ _ _ =>
        valuedResult (runValue machine fuel ⟨environment, store, [], locals, some store, activation, [], [], []⟩ expression)
      | _ =>
        step machine.context environment store
          (fun inner innerLocals => evalExpr machine fuel activation store inner innerLocals)
          (fun defining parameters body kind =>
            callBlock machine fuel (activation + machine.activationStep) store defining parameters body kind)
          locals expression

  /-- A block function body runs in its own activation, in its defining
  environment with its parameters as locals; it may not write the caller's
  cells, and its own cells end with the call. -/
  def callBlock {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Nat → Store world → Environment → Locals Declaration Identity Source → List Xsh.Syntax.Statement →
        ReturnKind → Result Declaration Identity Source (Value Declaration Identity Source)
    | 0, _, _, _, _, _, _ => .outOfFuel
    | fuel + 1, activation, store, defining, parameters, body, kind =>
      boundary kind (runBody machine fuel ⟨defining, store, [], parameters, some store, activation, [], [], []⟩ body)

  /-- An expression in a statement: `try`, `retry`, value blocks, `?` and
  conversions on them run here with the frame they change; anything else is
  evaluated. -/
  def runValue {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.Expr → Valued world
    | 0, _, _ => .exec .outOfFuel
    | fuel + 1, frame, expression =>
      match expression with
      | .valueBlock body => valueScope machine fuel frame body
      | .capture body => capture (valueScope machine fuel frame body)
      | .retry .delays delays pattern body =>
        resultThen frame (evaluateAll (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals) delays)
          fun scheduled => retryLoop machine fuel frame pattern body scheduled
      | .tryExpr inner =>
        (runValue machine fuel frame inner).bind fun value next => fromEval next (propagate value)
      | .convert inner target =>
        (runValue machine fuel frame inner).bind fun value next =>
          match machine.conversion value target with
          | some converted => fromEval next (propagate converted)
          | none => .exec .unsupported
      | .retry .backoff _ _ _ => .exec .unsupported
      | expression => fromEval frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals expression)

  /-- A value block (SPEC 6.9): its tail is computed before its defers run, and
  an `Err` tail is its value, so it leaves without an error. -/
  def valueScope {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Valued world
    | 0, _, _ => .exec .outOfFuel
    | fuel + 1, frame, body =>
      settle (leaveBlock machine fuel frame) (runValueTail machine fuel frame.enter body)

  def runValueTail {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Valued world
    | 0, _, _ => .exec .outOfFuel
    | fuel + 1, frame, body =>
      match body with
      | [] => .value (.scalar .unit) frame
      | [.expression value] => runValue machine fuel frame value
      | [.tailName name] => runValue machine fuel frame (.ident name)
      | statement :: rest =>
        (runStatement machine fuel frame statement).thenValued fun next => runValueTail machine fuel next rest

  /-- `retry [delays] on (pattern) { ... }` (SPEC 8.8): each attempt is a
  block; a raised failure the pattern selects takes the next delay and runs
  another attempt, and the result is `Ok` of the first success or `Err` of
  the last failure. -/
  def retryLoop {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Option Xsh.Syntax.Pattern → List Xsh.Syntax.Statement →
        List (Value Declaration Identity Source) → Valued world
    | 0, _, _, _, _ => .exec .outOfFuel
    | fuel + 1, frame, pattern, body, delays =>
      retryStep machine.context pattern delays (fun after rest => retryLoop machine fuel after pattern body rest)
        (valueScope machine fuel frame body)

  /-- A function body: its statements in a scope, the last one a tail
  expression whose value is returned. -/
  def runBody {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, body =>
      leaveBlock machine fuel frame (runTail machine fuel frame.enter body)

  def runTail {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, body =>
      match body with
      | [] => .returned (.scalar .unit) frame
      | [.expression value] => (runValue machine fuel frame value).toExec fun result next => .returned result next
      | [.tailName name] => (runValue machine fuel frame (.ident name)).toExec fun result next => .returned result next
      | statement :: rest =>
        (runStatement machine fuel frame statement).andThen fun next => runTail machine fuel next rest

  def runStatements {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, body =>
      match body with
      | [] => .normal frame
      | statement :: rest =>
        (runStatement machine fuel frame statement).andThen fun next => runStatements machine fuel next rest

  /-- Leave a scope: run its deferred actions, newest first, then restore the
  outer frame. -/
  def leaveBlock {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Exec world → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, outer, inner => withinScope outer (runDeferred machine fuel inner (pendingOf inner))

  /-- Run deferred actions in order against the outcome they leave with: an
  `errdefer` runs only when the block leaves with an error, and a failing
  action is a cleanup failure. -/
  def runDeferred {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Exec world → List (Deferred Declaration Identity Source) → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, status, actions =>
      match actions with
      | [] => status
      | entry :: rest =>
        if entry.kind = .error ∧ exitsWithError status = false then runDeferred machine fuel status rest
        else
          match frameOf status with
          | none => status
          | some scope =>
            match runBlock machine fuel (entry.frame scope) entry.body with
            | .normal after => runDeferred machine fuel (status.withStore after.store) rest
            | .failed failure after => runDeferred machine fuel (cleanupFailed failure after.store status) rest
            | .internal fault after => .internal fault { scope with store := after.store }
            | .broke _ | .continued _ | .returned _ _ => .stuck
            | other => other

  /-- A block runs in its own scope. -/
  def runBlock {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, body => leaveBlock machine fuel frame (runStatements machine fuel frame.enter body)

  /-- A body that first binds pattern captures, in its own scope. -/
  def runBound {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List (Xsh.Syntax.RawName × Value Declaration Identity Source) →
        List Xsh.Syntax.Statement → Exec world
    | 0, _, _, _ => .outOfFuel
    | fuel + 1, frame, captures, body =>
      leaveBlock machine fuel frame ((bindCaptures machine frame.enter captures).andThen fun bound =>
        runStatements machine fuel bound body)

  def runStatement {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.Statement → Exec world
    | 0, _, _ => .outOfFuel
    | fuel + 1, frame, statement =>
      match statement with
      | .binding kind target _ initializer =>
        (runValue machine fuel frame initializer).toExec fun value next =>
          bindTarget machine next (bindingMutability kind) target value
      | .assign (.env key) .set value =>
        match frame.callerStore with
        | some _ => .unsupported
        | none =>
          (runValue machine fuel frame value).toExec fun result next =>
            match machine.resolution.argv result with
            | none => .internal .unconvertibleEnvironment next
            | some bytes =>
              match setEnvironment machine.raises next.overlay (machine.context.spelling key).toUTF8.toList bytes with
              | .ok overlay => .normal { next with overlay := overlay }
              | .error failure => .failed failure next
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
              (runValue machine fuel frame value).toExec fun result next =>
                fromStep next next.environment next.locals
                  (assignPath world machine.decisions machine.raises machine.operate next.store cell path op result)
      | .expression (.valueBlock body) => runBlock machine fuel frame body
      | .expression value =>
        (runValue machine fuel frame value).toExec fun result next =>
          match result.eraseNominal with
          | .err error => .failed (.raised error) next
          | _ => .normal next
      | .returnValue (some value) =>
        (runValue machine fuel frame value).toExec fun result next => .returned result next
      | .returnValue none => .returned (.scalar .unit) frame
      | .ifStmt branches otherwise => runIf machine fuel frame branches otherwise
      | .whileStmt condition body => runWhile machine fuel frame condition body
      | .forStmt target iterable body =>
        fromResult frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals iterable) fun source =>
          match listValues source with
          | some items => runFor machine fuel frame target items body
          | none => .unsupported
      | .loopStmt body => runLoop machine fuel frame body
      | .breakStmt none => .broke frame
      | .continueStmt => .continued frame
      | .matchStmt subject arms =>
        fromResult frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals subject) fun value =>
          runMatch machine fuel frame value arms
      | .assert condition message =>
        fromResult frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals condition) fun tested =>
          match tested.eraseNominal with
          | .scalar (.boolean true) => .normal frame
          | .scalar (.boolean false) =>
            match message with
            | none => .failed (.raised (machine.assertion none)) frame
            | some text =>
              fromResult frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals text) fun written =>
                .failed (.raised (machine.assertion (some written))) frame
          | _ => .unsupported
      | .defer kind action =>
        .normal { frame with deferred := ⟨kind, action, frame.environment, frame.locals⟩ :: frame.deferred }
      | .sugar _ expansion => runStatement machine fuel frame expansion
      | _ => .unsupported

  /-- `if`, `else if` and `else`: each branch is its own scope. -/
  def runIf {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List (Xsh.Syntax.Expr × List Xsh.Syntax.Statement) →
        Option (List Xsh.Syntax.Statement) → Exec world
    | 0, _, _, _ => .outOfFuel
    | fuel + 1, frame, branches, otherwise =>
      match branches, otherwise with
      | [], none => .normal frame
      | [], some fallback => runBlock machine fuel frame fallback
      | (.patternCondition subject pattern, body) :: rest, otherwise =>
        fromResult frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals subject) fun value =>
          match conditionMatch machine.context pattern value with
          | .matched captures => runBound machine fuel frame captures body
          | .failed => runIf machine fuel frame rest otherwise
          | .unsupported => .unsupported
      | (condition, body) :: rest, otherwise =>
        fromResult frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals condition) fun tested =>
          match tested.eraseNominal with
          | .scalar (.boolean true) => runBlock machine fuel frame body
          | .scalar (.boolean false) => runIf machine fuel frame rest otherwise
          | _ => .unsupported

  /-- `while` and `while let`: each iteration's body is its own scope; `break`
  ends this loop and `continue` starts its next test. -/
  def runWhile {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.Expr → List Xsh.Syntax.Statement → Exec world
    | 0, _, _, _ => .outOfFuel
    | fuel + 1, frame, condition, body =>
      match condition with
      | .patternCondition subject pattern =>
        fromResult frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals subject) fun value =>
          match conditionMatch machine.context pattern value with
          | .matched captures =>
            (runBound machine fuel frame captures body).iterate fun next =>
              runWhile machine fuel next (.patternCondition subject pattern) body
          | .failed => .normal frame
          | .unsupported => .unsupported
      | condition =>
        fromResult frame (evalExpr machine fuel frame.activation frame.store frame.environment frame.locals condition) fun tested =>
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
    | fuel + 1, frame, target, items, body =>
      match items with
      | [] => .normal frame
      | item :: rest =>
        (leaveBlock machine fuel frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
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
    | fuel + 1, frame, value, arms =>
      match arms with
      | [] => .stuck
      | (pattern, guard, body) :: rest =>
        match matchPattern machine.context pattern value with
        | .failed => runMatch machine fuel frame value rest
        | .unsupported => .unsupported
        | .matched captures =>
          match guard with
          | none => runBound machine fuel frame captures body
          | some condition =>
            match bindCaptures machine frame.enter captures with
            | .normal bound =>
              fromResult frame (evalExpr machine fuel bound.activation bound.store bound.environment bound.locals condition)
                fun tested =>
                  match tested.eraseNominal with
                  | .scalar (.boolean true) => runBound machine fuel frame captures body
                  | .scalar (.boolean false) => runMatch machine fuel frame value rest
                  | _ => .unsupported
            | other => leaveBlock machine fuel frame other
end

end Xsh.Semantics.CoreControl
