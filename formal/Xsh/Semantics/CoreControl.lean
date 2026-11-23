import Xsh.Semantics.CoreEval
import Xsh.Validation.Core.Require
import Xsh.Values.BaseCodec
import Xsh.Values.Digests
import Xsh.Paths.Fields
import Xsh.Paths.Normalize
import Xsh.Paths.StdComponents

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

/-- How a call's arguments fail to fit its parameters (SPEC 6.3, 9.1): a label
no parameter has, a parameter supplied twice, a required parameter left out,
or more positional arguments than parameters to take them. -/
inductive ArgumentFault where
  | unknownName (label : String)
  | duplicate (label : String)
  | missing (label : String)
  | tooMany
  deriving DecidableEq

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
  /-- The checker's type for a type pattern's written type. -/
  typeOf : Xsh.Syntax.TypeExpr → Option (ValueType Declaration)
  /-- Whether the checker gives a `guard let` subject an outermost `Result`
  type (SPEC 8.6). -/
  resultSubject : Environment → Xsh.Syntax.Expr → Bool
  /-- The runtime's identity for the callable value a top-level function
  declaration gives, by the cell its name is bound to. -/
  callableReference : CellId → Reference Identity .callable
  /-- The declaration cell a callable value's identity names. -/
  callableCell : Reference Identity .callable → Option CellId
  /-- The program's top-level declarations, whose record schemas and error
  families give its constructors; the runtime makes them visible throughout
  the program. -/
  declarations : List Xsh.Syntax.Statement
  /-- The checker's declaration identity for a type or error family name. -/
  identity : Xsh.Syntax.RawName → Declaration
  /-- The runtime error a call's arguments raise when they do not fit its
  parameters, given the parameters a call fills by position and the
  positional arguments it was given. -/
  argumentError : ArgumentFault → Nat → Nat → Value Declaration Identity Source
  /-- The runtime error a callable raises when it reads a top-level binding
  whose statement has not run yet (SPEC 3.1). -/
  uninitialized : Xsh.Syntax.RawName → Value Declaration Identity Source
  /-- The wire strings of Str-backed enums, which `.require` converts. -/
  enums : WireEnums Declaration
  /-- The clock a `retry backoff` form reads: the milliseconds since its first
  attempt started, at the end of each attempt, a host observation (SPEC 8.8). -/
  retryClock : Nat → Nat
  /-- The runtime's identity for the stream created at an index. -/
  streamReference : Nat → Reference Identity .stream
  /-- The creation index a stream identity names. -/
  streamIndex : Reference Identity .stream → Option Nat
  /-- The runtime error an `exit` status outside 0 to 255 raises (SPEC 8.9). -/
  exitStatus : Value Declaration Identity Source
  /-- The checker's declaration of the error family a failed `.require` raises
  its schema error in (SPEC 5.3). -/
  schemaFamily : Declaration
  /-- The environment the process started with, by variable name: a host
  input the machine reads through and never writes (SPEC 10.5). -/
  hostEnvironment : List UInt8 → Option (List UInt8)
  /-- The `env-missing` error a read of an unset variable gives. -/
  envMissing : String → Value Declaration Identity Source
  /-- The `invalid-utf8` error a `Str` read of a value that is not UTF-8 gives. -/
  invalidUtf8 : String → Value Declaration Identity Source

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

/-- A local function (SPEC 9): the cell its name is bound to, the environment
it was declared in, the local functions visible there, the mutable cells it
captures, its parameters and body. Its captured names are read through that
environment when it is called, so a captured `var` is read as it is at the
call, and it may write a captured mutable cell (SPEC 9.2). -/
inductive Closure where
  | mk (cell : CellId) (kind : CallableKind) (environment : Environment) (captured : List (CellId × Closure))
      (writable : List CellId)
      (parameters : List Xsh.Syntax.Parameter) (body : FunctionBody)

def Closure.parameters : Closure → List Xsh.Syntax.Parameter
  | .mk _ _ _ _ _ parameters _ => parameters

/-- What an expression sees besides its environment, store and locals: the
call activation, the local functions in scope, and the implicit `.` of the
innermost handler block. -/
structure Scope (Declaration Identity Source : Type) where
  activation : Nat
  closures : List (CellId × Closure)
  item : Option (Value Declaration Identity Source)

/-- An argument as a call receives it: positional, or under a label. -/
inductive Supplied (Declaration Identity Source : Type) where
  | positional (value : Value Declaration Identity Source)
  | named (label : String) (value : Value Declaration Identity Source)

/-- A created stream (SPEC 9.5): a producer call not yet consumed, with its
arguments evaluated at creation, one `.collect()` is running, with the items
it has gathered, or one that has run. Streams are one-pass, so a consumed
stream yields nothing again. -/
inductive StreamState (Declaration Identity Source : Type) where
  | pending (closure : Closure) (supplied : List (Supplied Declaration Identity Source))
  | gathering (items : List (Value Declaration Identity Source))
  | done

/-- A `for` loop consuming a stream producer: its frame without the store
and environment variables, which the producer threads, and its target and
body; or, with a `sink`, a `.collect()` gathering the items into that
stream's entry. Inside a `collect` block, `gathers` names the stream table
entry that holds the block's list while the producer runs, so each run of
the body appends to the list the one before it left. -/
structure Consumer (world : World Declaration Identity Source) where
  environment : Environment
  locals : Locals Declaration Identity Source
  callerStore : Option (Store world)
  activation : Nat
  deferred : List (Deferred Declaration Identity Source)
  closures : List (CellId × Closure)
  item : Option (Value Declaration Identity Source)
  collected : Option (List (Value Declaration Identity Source))
  writable : List CellId
  target : Xsh.Syntax.BindingTarget
  body : List Xsh.Syntax.Statement
  sink : Option Nat
  stages : List Xsh.Syntax.PipeStage
  limit : Option Nat
  gathers : Option Nat

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
  /-- The value a `break` gives the `loop` expression it leaves. -/
  breakValue : Option (Value Declaration Identity Source)
  /-- The local functions in scope, by the cell their name is bound to. -/
  closures : List (CellId × Closure)
  /-- The implicit `.` of the innermost handler block. -/
  item : Option (Value Declaration Identity Source)
  /-- What the enclosing `collect` has gathered from `yield`, if there is one. -/
  collected : Option (List (Value Declaration Identity Source))
  /-- The value a pipeline's value-call stage receives, which `_` reads. -/
  piped : Option (Value Declaration Identity Source) := none
  /-- A pipeline's stages as one item left them: a `drop` that consumed the
  item is gone and an `enumerate` counts one more, which the next item sees. -/
  restaged : Option (List Xsh.Syntax.PipeStage) := none
  /-- The caller's cells this frame may still write: a closure's captured
  mutable cells. -/
  writable : List CellId
  /-- The `for` loops consuming the stream producers this frame runs in,
  innermost first: each `yield` runs the innermost loop's body (SPEC 9.5). -/
  consumers : List (Consumer world)
  /-- The streams the program has created, by creation order: evaluator
  state that calls share, as the store is. -/
  streams : List (StreamState Declaration Identity Source)

def Frame.scope {world : World Declaration Identity Source} (frame : Frame world) : Scope Declaration Identity Source :=
  ⟨frame.activation, frame.closures, frame.item⟩

/-- A frame with nothing pending, in `scope`. -/
def Frame.ofScope {world : World Declaration Identity Source} (environment : Environment) (store : Store world)
    (locals : Locals Declaration Identity Source) (callerStore : Option (Store world))
    (scope : Scope Declaration Identity Source) : Frame world :=
  { environment, store, overlay := [], locals, callerStore, activation := scope.activation, deferred := [],
    reported := [], waited := [], breakValue := none, closures := scope.closures, item := scope.item,
    collected := none, writable := [], consumers := [], streams := [] }

/-- How a `for` loop's body left while it consumed a stream producer: the
producer's scopes unwind and the loop then leaves this way (SPEC 9.5). -/
inductive ConsumerExit (Declaration Identity Source : Type) where
  | broke
  | returned (value : Value Declaration Identity Source)
  | failed (failure : Failure Declaration Identity Source)

/-- The outcome of running statements. -/
inductive Exec (world : World Declaration Identity Source) where
  | normal (frame : Frame world)
  | broke (frame : Frame world)
  | continued (frame : Frame world)
  | returned (value : Value Declaration Identity Source) (frame : Frame world)
  | failed (failure : Failure Declaration Identity Source) (frame : Frame world)
  | internal (fault : InternalFault) (frame : Frame world)
  /-- `exit status` (SPEC 8.9): the script ends with `status`, unwinding every
  scope; `try` does not capture it. -/
  | exited (status : Nat) (frame : Frame world)
  /-- The consuming loop's body left at a `yield`: the producer unwinds,
  running its `defer`s, and the loop leaves as `exit` says. -/
  | consumed (exit : ConsumerExit Declaration Identity Source) (frame : Frame world)
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
    deferred := outer.deferred
    closures := outer.closures
    item := outer.item }

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
  | .exited status inner => .exited status (outer.restore inner)
  | .consumed exit inner => .consumed exit (outer.restore inner)
  | other => other

def Frame.enter {world : World Declaration Identity Source} (frame : Frame world) : Frame world :=
  { frame with environment := { frame.environment with names := enterScope frame.environment.names }, deferred := [] }

def frameOf {world : World Declaration Identity Source} : Exec world → Option (Frame world)
  | .normal frame | .broke frame | .continued frame | .returned _ frame | .failed _ frame | .internal _ frame
  | .exited _ frame | .consumed _ frame =>
    some frame
  | _ => none

/-- The actions a scope runs as it is left: those of every exit but an
internal fault. -/
def pendingOf {world : World Declaration Identity Source} :
    Exec world → List (Deferred Declaration Identity Source)
  | .normal frame | .broke frame | .continued frame | .returned _ frame | .failed _ frame | .exited _ frame
  | .consumed _ frame =>
    frame.deferred
  | _ => []

def isErr (value : Value Declaration Identity Source) : Bool :=
  match value.eraseNominal with
  | .err _ => true
  | _ => false

/-- A block leaves with an error when a failure leaves it, the function
returns an `Err` through it (SPEC 8.7), or an `exit` leaves it with a nonzero
status or after a cleanup failure (SPEC 8.9). -/
def exitsWithError {world : World Declaration Identity Source} : Exec world → Bool
  | .failed _ _ => true
  | .returned value _ => isErr value
  | .exited status frame => status != 0 || !frame.reported.isEmpty
  | _ => false

def Exec.withStore {world : World Declaration Identity Source} (store : Store world) : Exec world → Exec world
  | .normal frame => .normal { frame with store := store }
  | .broke frame => .broke { frame with store := store }
  | .continued frame => .continued { frame with store := store }
  | .returned value frame => .returned value { frame with store := store }
  | .failed failure frame => .failed failure { frame with store := store }
  | .internal fault frame => .internal fault { frame with store := store }
  | .exited status frame => .exited status { frame with store := store }
  | .consumed exit frame => .consumed exit { frame with store := store }
  | other => other

/-- A deferred action failed: an earlier failure or an `exit` stays primary
and the new failure is reported; otherwise the new one becomes primary
(SPEC 8.7, 8.9). -/
def cleanupFailed {world : World Declaration Identity Source} (failure : Failure Declaration Identity Source)
    (store : Store world) : Exec world → Exec world
  | .failed primary frame => .failed primary { frame with store := store, reported := frame.reported ++ [failure] }
  | .exited status frame => .exited status { frame with store := store, reported := frame.reported ++ [failure] }
  | .normal frame | .broke frame | .continued frame | .returned _ frame | .internal _ frame | .consumed _ frame =>
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

/-- The first position at which `needle` occurs in `haystack`. -/
def firstIndex (needle : List Char) : List Char → Option Nat
  | [] => if needle.isEmpty then some 0 else none
  | character :: rest =>
    if needle.isPrefixOf (character :: rest) then some 0 else (firstIndex needle rest).map (· + 1)

/-- Decimal digits with an optional sign. -/
def decimal : List Char → Option Int
  | '-' :: digits => (natural digits).map fun value => -(value : Int)
  | '+' :: digits => (natural digits).map fun value => (value : Int)
  | digits => (natural digits).map fun value => (value : Int)
where
  natural (digits : List Char) : Option Nat :=
    if digits.isEmpty then none
    else digits.foldl (fun total digit =>
      total.bind fun sum => if digit.isDigit then some (sum * 10 + (digit.toNat - '0'.toNat)) else none) (some 0)

/-- The value of a digit in `base`. -/
def digitValue (base : Nat) (digit : Char) : Option Nat :=
  let lower := digit.toLower
  let value :=
    if digit.isDigit then some (digit.toNat - '0'.toNat)
    else if 'a' ≤ lower ∧ lower ≤ 'f' then some (lower.toNat - 'a'.toNat + 10)
    else none
  value.bind fun found => if found < base then some found else none

/-- Digits in `base` with an optional sign and an optional `0x`, `0o` or `0b`
prefix (SPEC 6.10, Python's `parse` reading of the format letter). -/
def radix (base : Nat) (marker : Char) : List Char → Option Int
  | '-' :: rest => (unsigned rest).map fun value => -(value : Int)
  | '+' :: rest => (unsigned rest).map fun value => (value : Int)
  | rest => (unsigned rest).map fun value => (value : Int)
where
  unsigned : List Char → Option Nat
    | '0' :: letter :: digits => if letter.toLower = marker then natural digits else natural ('0' :: letter :: digits)
    | digits => natural digits
  natural (digits : List Char) : Option Nat :=
    if digits.isEmpty then none
    else digits.foldl (fun total digit =>
      total.bind fun sum => (digitValue base digit).map fun value => sum * base + value) (some 0)

def integerHole (parsed : Option Int) : Match Declaration Identity Source :=
  match parsed.bind validateInt? with
  | some value => .matched [(⟨⟨0, by decide⟩⟩, .scalar (.integer value))]
  | none => .failed

/-- The value a text-pattern hole takes (SPEC 6.10): a `Str`, an `Int` for
`d`, `x`, `o` and `b`, or a finite `Float` for `f`, `e` and `g`, rounded as a
literal is. -/
def holeValue (spelling : Xsh.Syntax.RawName → String) (spec : Option Xsh.Syntax.RawName) (taken : List Char) :
    Match Declaration Identity Source :=
  match spec.map spelling with
  | none | some "s" => .matched [(⟨⟨0, by decide⟩⟩, .scalar (.string (String.ofList taken)))]
  | some "d" =>
    match (decimal taken).bind validateInt? with
    | some value => .matched [(⟨⟨0, by decide⟩⟩, .scalar (.integer value))]
    | none => .failed
  | some "x" => integerHole (radix 16 'x' taken)
  | some "o" => integerHole (radix 8 'o' taken)
  | some "b" => integerHole (radix 2 'b' taken)
  | some "f" | some "e" | some "g" =>
    match Xsh.Values.FloatLiterals.holeFloat taken with
    | some bits => .matched [(⟨⟨0, by decide⟩⟩, .scalar (.float bits))]
    | none => .failed
  | some _ => .unsupported

/-- A hole's capture under its name, or none for `{_}`. -/
def holeCapture (spelling : Xsh.Syntax.RawName → String) (name : Option Xsh.Syntax.RawName)
    (spec : Option Xsh.Syntax.RawName) (taken : List Char) : Match Declaration Identity Source :=
  match holeValue spelling spec taken, name with
  | .matched [(_, value)], some bound => .matched [(bound, value)]
  | .matched _, none => .matched []
  | other, _ => other

/-- A text pattern (SPEC 6.10): literal parts occur in order, a hole ends at
the first place the literal after it occurs, the literal that ends the pattern
is matched at the end of the text, and the last hole takes the rest; no other
split is tried. -/
def matchText (spelling : Xsh.Syntax.RawName → String) :
    List Xsh.Syntax.TextPatternPart → List Char → Match Declaration Identity Source
  | [], [] => .matched []
  | [], _ :: _ => .failed
  | .text literal :: rest, characters =>
    if literal.toList.isPrefixOf characters then matchText spelling rest (characters.drop literal.length) else .failed
  | .hole name spec :: rest, characters =>
    match rest with
    | [] => holeCapture spelling name spec characters
    | [.text literal] =>
      if literal.toList.isSuffixOf characters then
        holeCapture spelling name spec (characters.take (characters.length - literal.length))
      else .failed
    | .text literal :: more =>
      match firstIndex literal.toList characters with
      | none => .failed
      | some position =>
        (holeCapture spelling name spec (characters.take position)).both fun _ =>
          matchText spelling (.text literal :: more) (characters.drop position)
    | .hole _ _ :: _ => .unsupported

mutual
  /-- The SPEC 6.10 matcher for wildcard, binding, literal, record, list,
  alternation, alias, `Ok`/`Err` and variant constructor and type patterns and
  groups; `test` decides a type pattern. -/
  def matchPattern (context : Context Declaration Identity Source)
      (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
      Xsh.Syntax.Pattern → Value Declaration Identity Source → Match Declaration Identity Source
    | .wildcard, _ => .matched []
    | .binding name, value => .matched [(name, value)]
    | .group inner, value => matchPattern context test inner value
    | .alias inner name, value => (matchPattern context test inner value).both fun _ => .matched [(name, value)]
    | .alternation members, value => matchAlternatives context test members value
    | .constructor name argument, value =>
      match context.spelling name, value.eraseNominal with
      | "Ok", .ok payload | "Err", .err payload =>
        match argument with
        | none => .unsupported
        | some inner => matchPattern context test inner payload
      | "Ok", _ | "Err", _ => .failed
      | variant, .tagged _ actual fields =>
        if variant = actual then
          match argument, fields with
          | none, [] => .matched []
          | some (.tuple elements), fields =>
            if elements.length = fields.length then matchItems context test elements fields else .failed
          | some inner, [field] => matchPattern context test inner field
          | _, _ => .unsupported
        else .failed
      | _, _ => .unsupported
    | .errorVariant _ variant fields, value =>
      match value.eraseNominal with
      | .error data =>
        if context.spelling variant = data.variant then matchFields context test fields data.payload else .failed
      | _ => .failed
    | .facet name, value =>
      match value.eraseNominal with
      | .error data => if data.facets.contains (context.spelling name) then .matched [] else .failed
      | _ => .failed
    | .text parts, value =>
      match value.eraseNominal with
      | .scalar (.string text) => matchText context.spelling parts text.toList
      | _ => .failed
    | .testName name type, value =>
      match test type value with
      | some true => .matched [(name, value)]
      | some false => .failed
      | none => .unsupported
    | .typeBind name type, value =>
      match test type value with
      | some true => .matched (name.toList.map fun bound => (bound, value))
      | some false => .failed
      | none => .unsupported
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
      | .record actual => matchFields context test fields actual
      | _ => .failed
    | .list elements rest, value =>
      match listValues value with
      | some items =>
        match rest with
        | none => if elements.length = items.length then matchItems context test elements items else .failed
        | some tail =>
          if elements.length ≤ items.length then
            (matchItems context test elements (items.take elements.length)).both fun _ =>
              matchPattern context test tail (.list (items.drop elements.length))
          else .failed
      | none => .failed
    | _, _ => .unsupported

  def matchFields (context : Context Declaration Identity Source)
      (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
      List (Xsh.Syntax.RawName × Xsh.Syntax.Pattern) → List (String × Value Declaration Identity Source) →
        Match Declaration Identity Source
    | [], _ => .matched []
    | (label, pattern) :: rest, actual =>
      match actual.lookup (context.spelling label) with
      | none => .failed
      | some found => (matchPattern context test pattern found).both fun _ => matchFields context test rest actual

  def matchItems (context : Context Declaration Identity Source)
      (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
      List Xsh.Syntax.Pattern → List (Value Declaration Identity Source) → Match Declaration Identity Source
    | [], _ => .matched []
    | pattern :: rest, item :: items =>
      (matchPattern context test pattern item).both fun _ => matchItems context test rest items
    | _ :: _, [] => .failed

  /-- `p | q`: the first alternative that matches. -/
  def matchAlternatives (context : Context Declaration Identity Source)
      (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
      List Xsh.Syntax.Pattern → Value Declaration Identity Source → Match Declaration Identity Source
    | [], _ => .failed
    | member :: rest, value =>
      match matchPattern context test member value with
      | .matched captures => .matched captures
      | .failed => matchAlternatives context test rest value
      | .unsupported => .unsupported
end

def catchAll : Xsh.Syntax.Pattern → Bool
  | .wildcard | .binding _ => true
  | .group inner => catchAll inner
  | _ => false

/-- `if let` and `while let`: a pattern that cannot fail is an optional
binding, which takes the other branch on `null` (SPEC 8.6). -/
def conditionMatch (context : Context Declaration Identity Source)
    (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) (pattern : Xsh.Syntax.Pattern)
    (value : Value Declaration Identity Source) : Match Declaration Identity Source :=
  if catchAll pattern then
    match value.eraseNominal with
    | .scalar .null => .failed
    | _ => matchPattern context test pattern value
  else matchPattern context test pattern value

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

def keyValue (key : MapKey) : Value Declaration Identity Source := .scalar key.scalar

/-- What `for` iterates (SPEC 8.6): a list's items, a map's `{key, value}`
items in key order, a string's one-scalar strings, and the bytes of `Bytes`
as `Int`. -/
def iterationItems (source : Value Declaration Identity Source) : Option (List (Value Declaration Identity Source)) :=
  match listValues source with
  | some items => some items
  | none =>
    match source.eraseNominal with
    | .map entries =>
      some ((entries.foldr (fun entry ordered => insertOrdered entry.1 entry.2 ordered) []).map fun entry =>
        .record [("key", keyValue entry.1), ("value", entry.2)])
    | .scalar (.string text) => some (text.toList.map fun character => .scalar (.string character.toString))
    | .scalar (.bytes bytes) =>
      some (bytes.filterMap fun byte => (validateInt? byte.toNat).map fun value => .scalar (.integer value))
    | _ => none

/-- What a `guard let` binds (SPEC 8.6): the `Ok` payload of a `Result`
subject, or a non-`null` optional value; `none` runs the else block. -/
def guardValue (result : Bool) (subject : Value Declaration Identity Source) : Option (Value Declaration Identity Source) :=
  if result then
    match subject.eraseNominal with
    | .ok payload => some payload
    | _ => none
  else
    match subject.eraseNominal with
    | .scalar .null => none
    | _ => some subject

/-- The selector an index value names in a container of static type `type`:
a list position, a map key, or a record field. -/
def selectorOf (type : Option (ValueType Declaration)) (index : Value Declaration Identity Source) : Option Selector :=
  match type.bind elementType, type.bind mapTypes, index.eraseNominal with
  | some _, _, .scalar (.integer position) => some (.index position.val)
  | none, some _, _ => (mapKeyOf index).map .key
  | none, none, .scalar (.string label) => some (.field label)
  | _, _, _ => none

/-- A type pattern's test: the checker's type for the written type, decided on
the value. -/
def Machine.typeTest {world : World Declaration Identity Source} (machine : Machine world)
    (written : Xsh.Syntax.TypeExpr) (value : Value Declaration Identity Source) : Option Bool :=
  (machine.typeOf written).map fun type =>
    @decide (HasType world type value) (Xsh.Core.Store.decideHasType world machine.decisions type value)

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

/-- A `guard let` else block must leave the enclosing continuation; one that
completes is stuck (SPEC 8.6, `check.guard-fallthrough`). -/
def guardElse {world : World Declaration Identity Source} : Exec world → Exec world
  | .normal _ => .stuck
  | other => other

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

/-- A result with the frame it leaves, or the outcome that leaves instead. -/
inductive Outcome (world : World Declaration Identity Source) (α : Type) where
  | value (value : α) (frame : Frame world)
  | exec (outcome : Exec world)

/-- An expression's value with the frame it leaves, or the outcome that
leaves instead. -/
abbrev Valued (world : World Declaration Identity Source) := Outcome world (Value Declaration Identity Source)

def Outcome.bind {world : World Declaration Identity Source} {α β : Type} :
    Outcome world α → (α → Frame world → Outcome world β) → Outcome world β
  | .value result frame, next => next result frame
  | .exec outcome, _ => .exec outcome

def Outcome.toExec {world : World Declaration Identity Source} {α : Type} :
    Outcome world α → (α → Frame world → Exec world) → Exec world
  | .value result frame, next => next result frame
  | .exec outcome, _ => outcome

def Exec.thenValued {world : World Declaration Identity Source} {α : Type} :
    Exec world → (Frame world → Outcome world α) → Outcome world α
  | .normal frame, next => next frame
  | other, _ => .exec other

def resultThen {world : World Declaration Identity Source} {α β : Type} (frame : Frame world) :
    Result Declaration Identity Source α → (α → Outcome world β) → Outcome world β
  | .value result, next => next result
  | .failure failure, _ => .exec (.failed failure frame)
  | .stuck, _ => .exec .stuck
  | .unsupported, _ => .exec .unsupported
  | .outOfFuel, _ => .exec .outOfFuel

/-- A store-free result in `frame`, which it leaves as it is. -/
def fromEval {world : World Declaration Identity Source} {α : Type} (frame : Frame world)
    (result : Result Declaration Identity Source α) : Outcome world α :=
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

/-- The receiver of `?.` or `?[` (SPEC 6.7): `none` for a `null`, which makes
the whole operation `null`; a `Result` propagates as `?` does; anything else
is the receiver itself. An unguarded receiver is itself. -/
def hopOf (guarded : Bool) (receiver : Value Declaration Identity Source) :
    Result Declaration Identity Source (Option (Value Declaration Identity Source)) :=
  if guarded then
    match receiver.eraseNominal with
    | .scalar .null => .value none
    | .ok _ | .err _ => (propagate receiver).bind fun present => .value (some present)
    | _ => .value (some receiver)
  else .value (some receiver)

/-- `try { ... }` (SPEC 8.8): a raised failure becomes `Err`, normal completion
`Ok` of the tail; runtime and type failures, and every other exit, pass on. -/
def capture {world : World Declaration Identity Source} : Valued world → Valued world
  | .value value frame => .value (.ok value) frame
  | .exec (.failed (.raised error) frame) => .value (.err error) frame
  | other => other

/-- A result reached in a scope inside `outer`, back in `outer`: its
environment, locals, deferred actions, closures and `.`, with the cells of
`outer` as the scope left them. -/
def Outcome.leave {world : World Declaration Identity Source} {α : Type} (outer : Frame world) :
    Outcome world α → Outcome world α
  | .value result inner => .value result (outer.restore inner)
  | .exec outcome => .exec (withinScope outer outcome)

/-- Operands evaluated once each, left to right, each in the frame the one
before it left. -/
def threadAll {world : World Declaration Identity Source} {α : Type}
    (evaluate : Frame world → α → Valued world) :
    Frame world → List α → Outcome world (List (Value Declaration Identity Source))
  | frame, [] => .value [] frame
  | frame, first :: rest =>
    (evaluate frame first).bind fun value after =>
      (threadAll evaluate after rest).bind fun values last => .value (value :: values) last

def threadFields {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) :
    Frame world → List Xsh.Syntax.RecordField → Outcome world (List (String × Value Declaration Identity Source))
  | frame, [] => .value [] frame
  | frame, .named name operand :: rest =>
    (evaluate frame operand).bind fun value after =>
      (threadFields context evaluate after rest).bind fun fields last => .value ((context.spelling name, value) :: fields) last
  | _, _ :: _ => .exec .unsupported

/-- `value.require(T)` (SPEC 5.3), on the machine's membership world and
contract decisions: the validation model's `requireValue`, which keeps extra
record fields. -/
def requireValue {world : World Declaration Identity Source} (machine : Machine world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) : Value Declaration Identity Source :=
  Xsh.Validation.Core.Require.requireValue ⟨world.membership, machine.decisions.contracts, machine.enums⟩
    machine.schemaFamily .keep type candidate

/-- An error value with one more context frame describing the region it left;
its identity, payload and earlier contexts are unchanged (SPEC 8.8). -/
def withContext (text : String) : Value Declaration Identity Source → Value Declaration Identity Source
  | .error data => .error { data with contexts := data.contexts ++ [⟨"ctx", some text, none⟩] }
  | .processError data => .processError { data with contexts := data.contexts ++ [⟨"ctx", some text, none⟩] }
  | .nominal declaration inner => .nominal declaration (withContext text inner)
  | other => other

/-- A raised or runtime failure leaving a `ctx` body gains its context; every
other outcome, a returned `Err` included, leaves unchanged. -/
def annotate {world : World Declaration Identity Source} (text : String) : Valued world → Valued world
  | .exec (.failed (.raised error) last) => .exec (.failed (.raised (withContext text error)) last)
  | .exec (.failed (.runtime error) last) => .exec (.failed (.runtime (withContext text error)) last)
  | other => other

/-- `e"NAME"` (SPEC 10.5): `Ok` of the variable's value as the script's
assignments leave it, else as the process started with it, `Err` of
`env-missing` when it is unset, and `Err` of `invalid-utf8` when its bytes
are not UTF-8; bytes are never decoded lossily. -/
def environmentString {world : World Declaration Identity Source} (machine : Machine world) (overlay : Overlay)
    (name : String) : Value Declaration Identity Source :=
  match (readEnvironment overlay name.toUTF8.toList).or (machine.hostEnvironment name.toUTF8.toList) with
  | none => .err (machine.envMissing name)
  | some bytes =>
    match String.fromUTF8? ⟨bytes.toArray⟩ with
    | some text => .ok (.scalar (.string text))
    | none => .err (machine.invalidUtf8 name)

/-- The overrides an `env (overlay) { ... }` scope applies (SPEC 10.4): each
field or `Str` key with its value converted as an argv item. `none` is a
value the conversion does not cover. -/
def scopeOverrides {world : World Declaration Identity Source} (machine : Machine world)
    (overrides : Value Declaration Identity Source) : Option (List (List UInt8 × List UInt8)) :=
  let convert := fun (entries : List (String × Value Declaration Identity Source)) =>
    entries.mapM fun (name, value) => (machine.resolution.argv value).map fun bytes => (name.toUTF8.toList, bytes)
  match overrides.eraseNominal with
  | .record fields => convert fields
  | .map entries =>
    (entries.mapM fun (entry : MapKey × Value Declaration Identity Source) => match entry.1 with
      | .string name => some (name, entry.2)
      | _ => none).bind convert
  | _ => none

/-- The overlay with every override set in order; a NUL value fails. -/
def applyOverrides {world : World Declaration Identity Source} (machine : Machine world) (overlay : Overlay) :
    List (List UInt8 × List UInt8) → Except (Failure Declaration Identity Source) Overlay
  | [] => .ok overlay
  | (name, value) :: rest =>
    match setEnvironment machine.raises overlay name value with
    | .ok next => applyOverrides machine next rest
    | .error failure => .error failure

/-- A scope's outcome with the environment it entered with: the scope's
assignments and overrides end with it however it is left (SPEC 10.4). -/
def withOverlay {world : World Declaration Identity Source} (overlay : Overlay) : Valued world → Valued world
  | .value value frame => .value value { frame with overlay }
  | .exec (.normal frame) => .exec (.normal { frame with overlay })
  | .exec (.broke frame) => .exec (.broke { frame with overlay })
  | .exec (.continued frame) => .exec (.continued { frame with overlay })
  | .exec (.returned value frame) => .exec (.returned value { frame with overlay })
  | .exec (.failed failure frame) => .exec (.failed failure { frame with overlay })
  | .exec (.internal fault frame) => .exec (.internal fault { frame with overlay })
  | .exec (.exited status frame) => .exec (.exited status { frame with overlay })
  | .exec (.consumed exit frame) => .exec (.consumed exit { frame with overlay })
  | other => other

/-- The first `count` delays of `backoff first..cap` (SPEC 8.8): `first`,
then each twice the one before, never longer than `cap`. -/
def backoffSchedule (first cap : Nat) : Nat → List Nat
  | 0 => []
  | count + 1 => min first cap :: backoffSchedule (min (2 * first) cap) cap count

/-- The delays a backoff form takes: the delay after attempt `index` only if
it would end within `limit` of the first attempt's start, as `clock` gives
the end of each attempt; the first delay not taken makes that attempt the
last. -/
def takenDelays (clock : Nat → Nat) (limit : Nat) : Nat → List Nat → List Nat
  | _, [] => []
  | index, delay :: rest => if clock index + delay ≤ limit then delay :: takenDelays clock limit (index + 1) rest else []

/-- The delays of `retry backoff first..cap within limit`: no more than
`limit / first + 1` can fit, as each attempt starts after the delays before
it. A zero `first` is outside the model. -/
def backoffDelays {world : World Declaration Identity Source} (machine : Machine world)
    (durations : List (Value Declaration Identity Source)) : Option (List (Value Declaration Identity Source)) :=
  match durations.map Value.eraseNominal with
  | [.scalar (.duration first), .scalar (.duration cap), .scalar (.duration limit)] =>
    if first.val = 0 then none
    else
      some ((takenDelays machine.retryClock limit.val 0 (backoffSchedule first.val cap.val (limit.val / first.val + 1))).filterMap
        fun delay => (validateDuration? delay).map fun valid => .scalar (.duration valid))
  | _ => none

/-- The consumer a `for` loop over a producer pushes: its frame's scope, target
and body. -/
def consumerOf {world : World Declaration Identity Source} (frame : Frame world) (target : Xsh.Syntax.BindingTarget)
    (body : List Xsh.Syntax.Statement) : Consumer world :=
  { environment := frame.environment, locals := frame.locals, callerStore := frame.callerStore,
    activation := frame.activation, deferred := frame.deferred, closures := frame.closures, item := frame.item,
    collected := frame.collected, writable := frame.writable, target, body, sink := none, stages := [], limit := none, gathers := none }

/-- The items a stream table entry gathers. -/
def gatheringAt {Declaration Identity Source : Type} (streams : List (StreamState Declaration Identity Source)) (slot : Nat) :
    Option (List (Value Declaration Identity Source)) :=
  match streams[slot]? with
  | some (.gathering items) => some items
  | _ => none

/-- A stream table with `entry` at `slot`, appended when `slot` is the next
index. -/
def placeEntry {Declaration Identity Source : Type} (streams : List (StreamState Declaration Identity Source)) (slot : Nat)
    (entry : StreamState Declaration Identity Source) : List (StreamState Declaration Identity Source) :=
  if slot < streams.length then streams.set slot entry else streams ++ [entry]

/-- The frame a `for` loop over a producer runs it in: its consumer pushed
and, inside a `collect` block, the block's list moved to the entry `slot`. -/
def Frame.consuming {world : World Declaration Identity Source} (frame : Frame world) (slot : Nat)
    (target : Xsh.Syntax.BindingTarget) (body : List Xsh.Syntax.Statement) : Frame world :=
  { frame with
    streams := match frame.collected with
      | some items => placeEntry frame.streams slot (.gathering items)
      | none => frame.streams
    consumers := { consumerOf frame target body with gathers := frame.collected.map fun _ => slot } :: frame.consumers }

/-- The frame a `yield` runs the consuming loop's body in: the loop's scope
with the producer's store and environment variables. -/
def consumerFrame {world : World Declaration Identity Source} (producer : Frame world) (loop : Consumer world)
    (outer : List (Consumer world)) : Frame world :=
  { environment := loop.environment, store := producer.store, overlay := producer.overlay, locals := loop.locals,
    callerStore := loop.callerStore, activation := loop.activation, deferred := loop.deferred,
    reported := producer.reported, waited := producer.waited, breakValue := none, closures := loop.closures,
    item := loop.item,
    collected := match loop.gathers with
      | some slot => gatheringAt producer.streams slot
      | none => loop.collected,
    writable := loop.writable, consumers := outer, streams := producer.streams }

/-- The stream table after a consumer's body ran: a body inside a `collect`
block leaves its list in the entry its consumer gathers into. -/
def gatheredBack {world : World Declaration Identity Source} (consumers : List (Consumer world))
    (collected : Option (List (Value Declaration Identity Source))) (streams : List (StreamState Declaration Identity Source)) :
    List (StreamState Declaration Identity Source) :=
  match consumers, collected with
  | loop :: _, some items =>
    match loop.gathers with
    | some slot => streams.set slot (.gathering items)
    | none => streams
  | _, _ => streams

/-- The producer's frame after its consumer's body ran: the store, the
environment variables and the streams as the body left them. -/
def Frame.resumedBy {world : World Declaration Identity Source} (producer after : Frame world) : Frame world :=
  { producer with store := after.store, overlay := after.overlay,
                  streams := gatheredBack producer.consumers after.collected after.streams }

/-- After the loop body a `yield` ran: a body that completed or continued
resumes the producer with the store and environment it left; one that broke,
returned or failed stops the producer, which unwinds as `consumed`. -/
def resumeProducer {world : World Declaration Identity Source} (producer : Frame world) : Exec world → Exec world
  | .normal after | .continued after => .normal (producer.resumedBy after)
  | .broke after => .consumed .broke (producer.resumedBy after)
  | .returned value after => .consumed (.returned value) (producer.resumedBy after)
  | .failed failure after => .consumed (.failed failure) (producer.resumedBy after)
  | .exited status after => .exited status (producer.resumedBy after)
  | .internal fault after => .internal fault (producer.resumedBy after)
  | .consumed exit after => .consumed exit (producer.resumedBy after)
  | other => other

/-- The not yet consumed stream a value names, with its creation index. -/
def pendingStream {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (value : Value Declaration Identity Source) : Option (Nat × Option (Closure × List (Supplied Declaration Identity Source))) :=
  match value.eraseNominal with
  | .stream reference =>
    (machine.streamIndex reference).bind fun index =>
      match frame.streams[index]? with
      | some (.pending closure supplied) => some (index, some (closure, supplied))
      | some _ => some (index, none)
      | none => none
  | _ => none

/-- A frame whose stream at `index` has been consumed. -/
def Frame.consume {world : World Declaration Identity Source} (frame : Frame world) (index : Nat) : Frame world :=
  { frame with streams := frame.streams.set index .done }

/-- `.collect()` gathering one yielded item into its stream's entry. -/
def Frame.gather {world : World Declaration Identity Source} (frame : Frame world) (index : Nat)
    (item : Value Declaration Identity Source) : Exec world :=
  match frame.streams[index]? with
  | some (.gathering items) => .normal { frame with streams := frame.streams.set index (.gathering (items ++ [item])) }
  | _ => .stuck

/-- A pipeline gathering the items one yielded item became into its
stream's entry. -/
def Frame.gatherAll {world : World Declaration Identity Source} (frame : Frame world) (index : Nat)
    (limit : Option Nat) (out : Value Declaration Identity Source) : Exec world :=
  match listValues out, frame.streams[index]? with
  | some items, some (.gathering gathered) =>
    match limit with
    | some count =>
      if count ≤ gathered.length + items.length then
        .broke { frame with streams := frame.streams.set index (.gathering ((gathered ++ items).take count)) }
      else .normal { frame with streams := frame.streams.set index (.gathering (gathered ++ items)) }
    | none => .normal { frame with streams := frame.streams.set index (.gathering (gathered ++ items)) }
  | _, _ => .stuck

/-- The consumer `.collect()` pushes for the stream at `index`. -/
def sinkConsumer {world : World Declaration Identity Source} (frame : Frame world) (index : Nat) : Consumer world :=
  { consumerOf frame .discard [] with sink := some index }

/-- A delegated stream's run, back in its parent producer: a finished child
lets the parent go on; anything else leaves with it. -/
def delegatedStream {world : World Declaration Identity Source} : Valued world → Exec world
  | .value _ last => .normal last
  | .exec outcome => outcome

/-- `yield @items` to a consuming loop: each item yielded in order, the
producer going on only while each one resumes it. -/
def delegateItems {world : World Declaration Identity Source} (yieldOne : Frame world → Value Declaration Identity Source → Exec world) :
    Frame world → List (Value Declaration Identity Source) → Exec world
  | frame, [] => .normal frame
  | frame, item :: rest => (yieldOne frame item).andThen fun next => delegateItems yieldOne next rest

/-- An outcome whose frame holds `collected` as its `collect` block's list. -/
def Exec.withCollected {world : World Declaration Identity Source}
    (collected : Option (List (Value Declaration Identity Source))) : Exec world → Exec world
  | .normal frame => .normal { frame with collected }
  | .broke frame => .broke { frame with collected }
  | .continued frame => .continued { frame with collected }
  | .returned value frame => .returned value { frame with collected }
  | .failed failure frame => .failed failure { frame with collected }
  | .internal fault frame => .internal fault { frame with collected }
  | .exited status frame => .exited status { frame with collected }
  | .consumed exit frame => .consumed exit { frame with collected }
  | other => other

/-- A loop's frame once its producer has run: its consumers its own again
and, inside a `collect` block, the block's list back from the entry its
consumer gathered into. -/
def Frame.closedLoop {world : World Declaration Identity Source} (loop last : Frame world) : Frame world :=
  { last with
    consumers := loop.consumers
    collected := match last.consumers with
      | consumer :: _ =>
        match consumer.gathers with
        | some slot => gatheringAt last.streams slot
        | none => last.collected
      | [] => last.collected
    streams := match last.consumers with
      | consumer :: _ =>
        match consumer.gathers with
        | some slot => last.streams.set slot .done
        | none => last.streams
      | [] => last.streams }

/-- A `for` loop over a producer call, once the producer has run: a producer
that finished ends the loop, a consumed exit leaves the loop as its body did,
and anything else leaves with it; the loop's frame is closed. -/
def consumeProducer {world : World Declaration Identity Source} (loop : Frame world) : Valued world → Exec world
  | .value _ last => .normal (loop.closedLoop last)
  | .exec (.consumed .broke last) => .normal (loop.closedLoop last)
  | .exec (.consumed (.returned value) last) => .returned value (loop.closedLoop last)
  | .exec (.consumed (.failed failure) last) => .failed failure (loop.closedLoop last)
  | .exec (.normal last) => .normal (loop.closedLoop last)
  | .exec (.failed failure last) => .failed failure (loop.closedLoop last)
  | .exec (.exited status last) => .exited status (loop.closedLoop last)
  | .exec (.internal fault last) => .internal fault (loop.closedLoop last)
  | .exec (.returned value last) => .returned value (loop.closedLoop last)
  | .exec (.broke last) => .broke (loop.closedLoop last)
  | .exec (.continued last) => .continued (loop.closedLoop last)
  | .exec other => other

/-- `.collect()` once the producer has run: a producer that finished gives
the items it gathered and closes the stream; anything else leaves with it. -/
def gathered {world : World Declaration Identity Source} (loop : Frame world) (index : Nat) : Valued world → Valued world
  | .value _ last =>
    match last.streams[index]? with
    | some (.gathering items) =>
      .value (.list items) { last.consume index with consumers := loop.consumers, collected := loop.collected }
    | _ => .exec .stuck
  | .exec (.consumed .broke last) =>
    match last.streams[index]? with
    | some (.gathering items) =>
      .value (.list items) { last.consume index with consumers := loop.consumers, collected := loop.collected }
    | _ => .exec .stuck
  | .exec (.normal _) => .exec .stuck
  | .exec outcome => .exec ((consumeProducer loop (.exec outcome)).withCollected loop.collected)

/-- What a serial stage does with its block's value: `where` and a closing
`any` keep the item when it is `true`, a closing `all` keeps it when it is
`false`, `map` replaces it, and `flat-map` expands it into the value's
items. -/
inductive StageAction where
  | keep
  | reject
  | replace
  | expand

/-- A serial stage block's one parameter, or `none` for the implicit `.`;
a block with more parameters has no single-item action. -/
def singleParameter : List Xsh.Syntax.RawName → Option (Option Xsh.Syntax.RawName)
  | [] => some none
  | [parameter] => some (some parameter)
  | _ => none

/-- A serial stage's action, block parameter and block body. -/
def stageAction : Xsh.Syntax.PipeStage → Option (StageAction × Option Xsh.Syntax.RawName × List Xsh.Syntax.Statement)
  | .stream kind [] (some (parameters, body)) =>
    (singleParameter parameters).bind fun parameter =>
      match kind with
      | .whereStage | .any => some (.keep, parameter, body)
      | .all => some (.reject, parameter, body)
      | .map => some (.replace, parameter, body)
      | .flatMap => some (.expand, parameter, body)
      | _ => none
  | _ => none

/-- `sum()` over the items that reached it: checked `Int` addition from `0`,
failing as `+` does (SPEC 13.3). -/
def pipelineSum (context : Context Declaration Identity Source) :
    List (Value Declaration Identity Source) →
      Option (Except (Failure Declaration Identity Source) (Value Declaration Identity Source))
  | items =>
    items.foldl (fun total item =>
      total.bind fun
        | .ok sum =>
          match binary context .add sum item with
          | .value next => some (.ok next)
          | .failure failure => some (.error failure)
          | _ => none
        | failed => some failed)
      ((validateInt? 0).map fun zero => .ok (.scalar (.integer zero)))

/-- `min()` or `max()` over the items that reached it, ordered by `<`: `min`
keeps the earlier of equal items and `max` the later, as the runtime's
`min_by` and `max_by` do. Items `<` does not order are outside the model
(SPEC 13.3 and the XSH-180 ordering draft). -/
def extremum (context : Context Declaration Identity Source) (largest : Bool) :
    Value Declaration Identity Source → List (Value Declaration Identity Source) →
      Option (Value Declaration Identity Source)
  | chosen, [] => some chosen
  | chosen, item :: rest =>
    match binary context .lt item chosen with
    | .value (.scalar (.boolean before)) =>
      extremum context largest (if before != largest then item else chosen) rest
    | _ => none

/-- A count as an `Int`, when it is in range. -/
def countValue (count : Nat) : Option (Value Declaration Identity Source) :=
  (validateInt? count).map fun value => .scalar (.integer value)

inductive PipelineEnd where
  | collected
  | counted
  | summed
  | firstItem
  | anyMatch
  | allMatch
  | lastItem
  | minimum
  | maximum
  /-- `fold(initial) { |accumulator, item| ... }`, which `reduce` spells too. -/
  | folded (initial : Xsh.Syntax.Expr) (accumulator item : Xsh.Syntax.RawName) (body : List Xsh.Syntax.Statement)

/-- A structured pipeline's stages split before a closing `count()` or
`collect()`, which ends the pipeline with its own value (SPEC 13.1), and
before a `take(n)` that ends its transformations, which stops pulling once
`n` items have reached it. -/
def pipelineSplit (stages : List Xsh.Syntax.PipeStage) :
    List Xsh.Syntax.PipeStage × Option Xsh.Syntax.Expr × PipelineEnd :=
  let (body, ending) : List Xsh.Syntax.PipeStage × PipelineEnd :=
    match stages.getLast? with
    | some (.stream .count [] none) => (stages.dropLast, .counted)
    | some (.stream .collect [] none) => (stages.dropLast, .collected)
    | some (.stream .sum [] none) => (stages.dropLast, .summed)
    | some (.stream .first [] none) => (stages.dropLast, .firstItem)
    | some (.stream .last [] none) => (stages.dropLast, .lastItem)
    | some (.stream .min [] none) => (stages.dropLast, .minimum)
    | some (.stream .max [] none) => (stages.dropLast, .maximum)
    | some (.stream .any [] (some _)) => (stages, .anyMatch)
    | some (.stream .all [] (some _)) => (stages, .allMatch)
    | some (.stream .fold [.positional initial] (some ([accumulator, item], body)))
    | some (.stream .reduce [.positional initial] (some ([accumulator, item], body))) =>
      (stages.dropLast, .folded initial accumulator item body)
    | _ => (stages, .collected)
  match body.getLast? with
  | some (.stream .take [.positional count] none) => (body.dropLast, some count, ending)
  | _ => (body, none, ending)

/-- A pipeline's first value-call stage, with the stages before and after it. -/
def valueStage : List Xsh.Syntax.PipeStage →
    Option (List Xsh.Syntax.PipeStage × Xsh.Syntax.Expr × List Xsh.Syntax.PipeStage)
  | [] => none
  | .value call :: rest => some ([], call, rest)
  | stage :: rest => (valueStage rest).map fun (before, call, after) => (stage :: before, call, after)

def isHoleArgument : Xsh.Syntax.CallArgument → Bool
  | .positional .valuePipelineHole | .named _ .valuePipelineHole => true
  | _ => false

/-- A value-call stage as a call on the piped value `_` (SPEC 13.1): an
argument written `_` receives it; otherwise a bare method name takes it as its
receiver and any other callee as its first argument; a trailing `?` stays. -/
def valueStageCall : Xsh.Syntax.Expr → Xsh.Syntax.Expr
  | .tryExpr inner => .tryExpr (valueStageCall inner)
  | .call callee arguments =>
    if arguments.any isHoleArgument then .call callee arguments
    else
      match callee with
      | .ident method => .call (.field .valuePipelineHole method) arguments
      | other => .call other (.positional .valuePipelineHole :: arguments)
  | other => other

/-- The items a closing stage needs before its answer is known: one for
`first()`, `any` and `all` (SPEC 13.2). -/
def endingLimit : PipelineEnd → Option Nat
  | .firstItem | .anyMatch | .allMatch => some 1
  | _ => none

/-- Two `take` limits together. -/
def bothLimits : Option Nat → Option Nat → Option Nat
  | some first, some second => some (min first second)
  | some first, none | none, some first => some first
  | none, none => none

mutual
  /-- An expression in a position with a known expected type, its contextual
  forms given that type. The target of a contextual `.require()` (SPEC 5.3) is
  the expected `Result` value type, and each `?` peels one `Result` layer, so
  under `.require()?` it is the expected type itself. A leading-dot variant
  (SPEC 5.5) selects from the enum the expected type names, `T?` selecting
  from `T`. The expected type reaches each branch of a value `if` or `match`,
  a value block's tail, each element of an expected `List[T]`, each value of
  a brace literal under an expected `Map[K, V]`, and the
  payload of `Ok(...)` under an expected `Result[T]`. -/
  def expectTarget (spelling : Xsh.Syntax.RawName → String) : Option Xsh.Syntax.TypeExpr → Xsh.Syntax.Expr → Xsh.Syntax.Expr
    | some (.result target), .require value none => .require value (some target)
    | some target, .tryExpr (.require value none) => .tryExpr (.require value (some target))
    | some (.named enum), .dotVariant none name arguments => .dotVariant (some enum) name arguments
    | some (.optional (.named enum)), .dotVariant none name arguments => .dotVariant (some enum) name arguments
    | some target, .ifExpr branches otherwise =>
      .ifExpr (expectBranches spelling (some target) branches) (expectTarget spelling (some target) otherwise)
    | some target, .matchExpr subject arms => .matchExpr subject (expectArms spelling (some target) arms)
    | some target, .valueBlock body => .valueBlock (expectTail spelling (some target) body)
    | some (.list element), .list elements => .list (expectElements spelling (some element) elements)
    | some (.map _ value), .record fields => .record (expectFields spelling (some value) fields)
    | some (.result target), .call (.ident constructor) [.positional payload] =>
      if spelling constructor = "Ok" then .call (.ident constructor) [.positional (expectTarget spelling (some target) payload)]
      else .call (.ident constructor) [.positional payload]
    | _, initializer => initializer

  def expectBranches (spelling : Xsh.Syntax.RawName → String) (expected : Option Xsh.Syntax.TypeExpr) :
      List (Xsh.Syntax.Expr × Xsh.Syntax.Expr) → List (Xsh.Syntax.Expr × Xsh.Syntax.Expr)
    | [] => []
    | (condition, value) :: rest => (condition, expectTarget spelling expected value) :: expectBranches spelling expected rest

  def expectArms (spelling : Xsh.Syntax.RawName → String) (expected : Option Xsh.Syntax.TypeExpr) :
      List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × Xsh.Syntax.Expr) →
        List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × Xsh.Syntax.Expr)
    | [] => []
    | (pattern, guard, value) :: rest => (pattern, guard, expectTarget spelling expected value) :: expectArms spelling expected rest

  def expectElements (spelling : Xsh.Syntax.RawName → String) (expected : Option Xsh.Syntax.TypeExpr) : List Xsh.Syntax.Expr → List Xsh.Syntax.Expr
    | [] => []
    | value :: rest => expectTarget spelling expected value :: expectElements spelling expected rest

  /-- A brace literal's field values under an expected `Map[K, V]`: a named or
  computed entry's value takes `V`. -/
  def expectFields (spelling : Xsh.Syntax.RawName → String) (expected : Option Xsh.Syntax.TypeExpr) :
      List Xsh.Syntax.RecordField → List Xsh.Syntax.RecordField
    | [] => []
    | .named name value :: rest => .named name (expectTarget spelling expected value) :: expectFields spelling expected rest
    | .computed key value :: rest =>
      .computed key (expectTarget spelling expected value) :: expectFields spelling expected rest
    | field :: rest => field :: expectFields spelling expected rest

  /-- A value block's tail, its last statement when that is an expression. -/
  def expectTail (spelling : Xsh.Syntax.RawName → String) (expected : Option Xsh.Syntax.TypeExpr) : List Xsh.Syntax.Statement → List Xsh.Syntax.Statement
    | [] => []
    | [.expression tail] => [.expression (expectTarget spelling expected tail)]
    | statement :: rest => statement :: expectTail spelling expected rest
end

/-- A call's arguments with each parameter's annotation as the expected type
of the argument that fills it (SPEC 5.5): positional arguments take the
non-rest parameters in order and a named argument takes the parameter it
names; after a splice, positions are no longer known. -/
def expectArguments (spelling : Xsh.Syntax.RawName → String) (parameters : List Xsh.Syntax.Parameter) :
    List Xsh.Syntax.Parameter → List Xsh.Syntax.CallArgument → List Xsh.Syntax.CallArgument
  | _, [] => []
  | .mk _ annotation _ false :: positions, .positional value :: rest =>
    .positional (expectTarget spelling annotation value) :: expectArguments spelling parameters positions rest
  | _, .positional value :: rest => .positional value :: expectArguments spelling parameters [] rest
  | positions, .named label value :: rest =>
    .named label (expectTarget spelling ((parameters.find? fun | .mk name _ _ _ => spelling name = spelling label).bind
        fun | .mk _ annotation _ _ => annotation) value) :: expectArguments spelling parameters positions rest
  | _, .splice value :: rest => .splice value :: expectArguments spelling parameters [] rest
  | positions, .namedSpread value :: rest => .namedSpread value :: expectArguments spelling parameters positions rest

mutual
  /-- A statement whose `return` operands, also those nested in its `if`,
  `while`, `for`, `loop`, `match`, `guard` and `with` bodies, take the declared
  result type as their expected type (SPEC 5.3). Declarations are allowed only
  at the top level, so no nested body has a result of its own. -/
  def expectReturn (spelling : Xsh.Syntax.RawName → String) (result : Option Xsh.Syntax.TypeExpr) :
      Xsh.Syntax.Statement → Xsh.Syntax.Statement
    | .returnValue (some value) => .returnValue (some (expectTarget spelling result value))
    | .ifStmt branches none => .ifStmt (expectReturnBranches spelling result branches) none
    | .ifStmt branches (some fallback) =>
      .ifStmt (expectReturnBranches spelling result branches) (some (expectReturns spelling result fallback))
    | .whileStmt condition body => .whileStmt condition (expectReturns spelling result body)
    | .forStmt target iterable body => .forStmt target iterable (expectReturns spelling result body)
    | .loopStmt body => .loopStmt (expectReturns spelling result body)
    | .matchStmt subject arms => .matchStmt subject (expectReturnArms spelling result arms)
    | .guardStmt target annotation initializer otherwise =>
      .guardStmt target annotation initializer (expectReturns spelling result otherwise)
    | .guardHandled target annotation initializer failure otherwise =>
      .guardHandled target annotation initializer failure (expectReturns spelling result otherwise)
    | .withScope bindings body otherwise =>
      .withScope bindings (expectReturns spelling result body) (expectReturns spelling result otherwise)
    | .sugar form expansion => .sugar form (expectReturn spelling result expansion)
    | statement => statement

  def expectReturns (spelling : Xsh.Syntax.RawName → String) (result : Option Xsh.Syntax.TypeExpr) :
      List Xsh.Syntax.Statement → List Xsh.Syntax.Statement
    | [] => []
    | statement :: rest => expectReturn spelling result statement :: expectReturns spelling result rest

  def expectReturnBranches (spelling : Xsh.Syntax.RawName → String) (result : Option Xsh.Syntax.TypeExpr) :
      List (Xsh.Syntax.Expr × List Xsh.Syntax.Statement) → List (Xsh.Syntax.Expr × List Xsh.Syntax.Statement)
    | [] => []
    | (condition, body) :: rest => (condition, expectReturns spelling result body) :: expectReturnBranches spelling result rest

  def expectReturnArms (spelling : Xsh.Syntax.RawName → String) (result : Option Xsh.Syntax.TypeExpr) :
      List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × List Xsh.Syntax.Statement) →
        List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × List Xsh.Syntax.Statement)
    | [] => []
    | (pattern, guard, body) :: rest => (pattern, guard, expectReturns spelling result body) :: expectReturnArms spelling result rest
end

/-- A function body whose tail and `return` operands take the declared result
type as their expected type (SPEC 5.3). -/
def expectBody (spelling : Xsh.Syntax.RawName → String) (result : Option Xsh.Syntax.TypeExpr) :
    List Xsh.Syntax.Statement → List Xsh.Syntax.Statement
  | [] => []
  | [.expression tail] => [.expression (expectTarget spelling result tail)]
  | statement :: rest => expectReturn spelling result statement :: expectBody spelling result rest

/-- Parameters whose defaults take the parameter's annotation as their
expected type (SPEC 5.5). -/
def expectDefaults (spelling : Xsh.Syntax.RawName → String) : List Xsh.Syntax.Parameter → List Xsh.Syntax.Parameter
  | [] => []
  | .mk name annotation (some default) rest :: more =>
    .mk name annotation (some (expectTarget spelling annotation default)) rest :: expectDefaults spelling more
  | parameter :: more => parameter :: expectDefaults spelling more

/-- A pipeline's source view (SPEC 13.1): one outer `Result` is removed, an
`Ok` giving its collection and an `Err` raising its error. -/
def pipelineSource (source : Value Declaration Identity Source) :
    Except (Value Declaration Identity Source) (Value Declaration Identity Source) :=
  match source.eraseNominal with
  | .ok inner => .ok inner
  | .err error => .error error
  | _ => .ok source

/-- A `take(n)` count, evaluated once before the source is pulled. -/
def pipelineLimit {world : World Declaration Identity Source}
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) (frame : Frame world) :
    Option Xsh.Syntax.Expr → Outcome world (Option Nat)
  | none => .value none frame
  | some count =>
    (evaluate frame count).bind fun value after =>
      match value.eraseNominal with
      | .scalar (.integer bound) => if 0 ≤ bound.val then .value (some bound.val.toNat) after else .exec .stuck
      | _ => .exec .stuck

/-- What is left of a `take` limit after an item became `count` items:
`none` once the limit is reached. -/
def afterItems (limit : Option Nat) (count : Nat) : Option (Option Nat) :=
  match limit with
  | none => some none
  | some bound => if bound ≤ count then none else some (some (bound - count))

/-- The items an item became, cut to a `take` limit. -/
def limitTake {α : Type} (limit : Option Nat) (items : List α) : List α :=
  match limit with
  | none => items
  | some bound => items.take bound

/-- A pipeline's stages ready to receive items (SPEC 13.3): each `drop(n)`
argument evaluated once, in stage order, before any item is pulled, and the
stage written as `n` stages that each drop one item, none for a negative
`n`. -/
def prepareStage {world : World Declaration Identity Source}
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) (frame : Frame world) :
    Xsh.Syntax.PipeStage → Outcome world (List Xsh.Syntax.PipeStage)
  | .stream .drop [.positional count] none =>
    (evaluate frame count).bind fun value after =>
      match value.eraseNominal with
      | .scalar (.integer bound) => .value (List.replicate bound.val.toNat (.stream .drop [] none)) after
      | _ => .exec .stuck
  | stage => .value [stage] frame

def prepareStages {world : World Declaration Identity Source}
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) (frame : Frame world) :
    List Xsh.Syntax.PipeStage → Outcome world (List Xsh.Syntax.PipeStage)
  | [] => .value [] frame
  | stage :: rest =>
    (prepareStage evaluate frame stage).bind fun first after =>
      (prepareStages evaluate after rest).bind fun later last => .value (first ++ later) last

/-- A stage that keeps state from item to item, as `prepareStages` and earlier
items left it: one of the stages a `drop(n)` became, or an `enumerate` that
has numbered as many items as it has arguments. -/
inductive StatefulStep (Declaration Identity Source : Type) where
  | dropOne
  | enumerateAt (counted : List Xsh.Syntax.CallArgument) (index : Value Declaration Identity Source)

def statefulStep : Xsh.Syntax.PipeStage → Option (StatefulStep Declaration Identity Source)
  | .stream .drop [] none => some .dropOne
  | .stream .enumerate counted none => (countValue counted.length).map (.enumerateAt counted)
  | _ => none

/-- A stage that needs every item before it gives any (SPEC 13.2): `sort`,
`sort-by` and `batch`. -/
def isMaterializing : Xsh.Syntax.PipeStage → Bool
  | .stream .sort _ _ | .stream .sortBy _ _ | .stream .batch _ _ => true
  | _ => false

/-- Whether a stage keeps state from item to item, or needs every item. -/
def isStateful : Xsh.Syntax.PipeStage → Bool
  | .stream .drop _ _ | .stream .enumerate _ _ => true
  | stage => isMaterializing stage

/-- The stages before the first materializing one, it, and the stages after. -/
def splitMaterializing : List Xsh.Syntax.PipeStage →
    Option (List Xsh.Syntax.PipeStage × Xsh.Syntax.PipeStage × List Xsh.Syntax.PipeStage)
  | [] => none
  | stage :: rest =>
    if isMaterializing stage then some ([], stage, rest)
    else (splitMaterializing rest).map fun (before, found, after) => (stage :: before, found, after)

/-- `entry` placed after every entry whose key is not greater than its own,
by `<`; keys `<` does not order are outside the model. -/
def insertByKey (context : Context Declaration Identity Source) :
    Value Declaration Identity Source × Value Declaration Identity Source →
      List (Value Declaration Identity Source × Value Declaration Identity Source) →
        Option (List (Value Declaration Identity Source × Value Declaration Identity Source))
  | entry, [] => some [entry]
  | entry, first :: rest =>
    match binary context .lt entry.1 first.1 with
    | .value (.scalar (.boolean true)) => some (entry :: first :: rest)
    | .value (.scalar (.boolean false)) => (insertByKey context entry rest).map (first :: ·)
    | _ => none

/-- A stable sort of keyed items by key (SPEC 13.3). -/
def sortByKeys (context : Context Declaration Identity Source)
    (entries : List (Value Declaration Identity Source × Value Declaration Identity Source)) :
    Option (List (Value Declaration Identity Source × Value Declaration Identity Source)) :=
  entries.foldl (fun sorted entry => sorted.bind (insertByKey context entry)) (some [])

/-- Items cut into lists of `size`, the last one shorter. -/
def chunkValues {α : Type} (size : Nat) : Nat → List α → List (List α)
  | 0, _ => []
  | _ + 1, [] => []
  | fuel + 1, items => items.take size :: chunkValues size fuel (items.drop size)

/-- Each item with its key, the block run with the item bound, in order. -/
def keyItems {world : World Declaration Identity Source}
    (scope : Frame world → Option (Value Declaration Identity Source) →
      List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world)
    (parameter : Option Xsh.Syntax.RawName) (body : List Xsh.Syntax.Statement) :
    Frame world → List (Value Declaration Identity Source) →
      Outcome world (List (Value Declaration Identity Source × Value Declaration Identity Source))
  | frame, [] => .value [] frame
  | frame, item :: rest =>
    (scope frame (parameter.elim (some item) fun _ => none) (parameter.toList.map fun name => (name, item)) body).bind
      fun key after => (keyItems scope parameter body after rest).bind fun keyed last => .value ((key, item) :: keyed) last

/-- Keyed items in key order, reversed when `descending`. -/
def orderedBy {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (descending : Bool) (keyed : List (Value Declaration Identity Source × Value Declaration Identity Source))
    (last : Frame world) : Valued world :=
  match sortByKeys context keyed with
  | some sorted => .value (.list ((if descending then sorted.reverse else sorted).map Prod.snd)) last
  | none => .exec .stuck

/-- What a materializing stage makes of every item that reached it (SPEC
13.3, 13.5): its options evaluated once as it starts, `sort` ordering items
by `<` and `sort-by` by the key its block gives, both stably, `desc: true`
reversing the order, and `batch(count: n)` cutting them into lists of `n`. -/
def materialize {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
    (scope : Frame world → Option (Value Declaration Identity Source) →
      List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world)
    (stage : Xsh.Syntax.PipeStage) (frame : Frame world) (items : List (Value Declaration Identity Source)) :
    Valued world :=
  let ordered := orderedBy context
  match stage with
  | .stream .sort [] none => ordered false (items.map fun item => (item, item)) frame
  | .stream .sortBy [] (some (parameters, body)) =>
    match singleParameter parameters with
    | some parameter => (keyItems scope parameter body frame items).bind fun keyed last => ordered false keyed last
    | none => .exec .unsupported
  | .stream .sortBy [.named label flag] (some (parameters, body)) =>
    match singleParameter parameters, context.spelling label == "desc" with
    | some parameter, true =>
      (evaluate frame flag).bind fun chosen after =>
        match chosen.eraseNominal with
        | .scalar (.boolean descending) =>
          (keyItems scope parameter body after items).bind fun keyed last => ordered descending keyed last
        | _ => .exec .stuck
    | _, _ => .exec .unsupported
  | .stream .batch [.named label count] none =>
    if context.spelling label != "count" then .exec .unsupported else
    (evaluate frame count).bind fun bound after =>
      match bound.eraseNominal with
      | .scalar (.integer size) =>
        if 0 < size.val then .value (.list ((chunkValues size.val.toNat (items.length + 1) items).map .list)) after
        else .exec .unsupported
      | _ => .exec .stuck
  | _ => .exec .unsupported

/-- Items through stages that may include materializing ones: the stages
before the first run on every item through `run`, the materializing stage
takes what they gave, and the stages after it continue from its list; a
`take` limit applies once no materializing stage is left. -/
def runSegments {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
    (scope : Frame world → Option (Value Declaration Identity Source) →
      List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world)
    (run : Frame world → List Xsh.Syntax.PipeStage → Option Nat → List (Value Declaration Identity Source) → Valued world) :
    Nat → Frame world → List Xsh.Syntax.PipeStage → Option Nat → List (Value Declaration Identity Source) → Valued world
  | 0, frame, stages, limit, items => run frame stages limit items
  | fuel + 1, frame, stages, limit, items =>
    match splitMaterializing stages with
    | none => run frame stages limit items
    | some (before, stage, after) =>
      (run frame before none items).bind fun gathered last =>
        match listValues gathered with
        | some reached =>
          (materialize context evaluate scope stage last reached).bind fun arranged next =>
            match listValues arranged with
            | some rearranged => runSegments context evaluate scope run fuel next after limit rearranged
            | none => .exec .stuck
        | none => .exec .stuck

/-- An item's outcome with the stage it passed put back in front of the
stages after it. -/
def restageWith {world : World Declaration Identity Source} (stage : Xsh.Syntax.PipeStage)
    (rest : List Xsh.Syntax.PipeStage) : Valued world → Valued world
  | .value value frame => .value value { frame with restaged := some (stage :: frame.restaged.getD rest) }
  | other => other

/-- A pipeline's value once its items have passed the stages: the
collected list, or the count a closing `count()` gives. -/
def finishPipeline {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (ending : PipelineEnd) (out : Value Declaration Identity Source) (last : Frame world) : Valued world :=
  match ending with
  | .collected => .value out last
  | .counted =>
    match listValues out >>= fun results => countValue results.length with
    | some count => .value count last
    | none => .exec .stuck
  | .summed =>
    match listValues out >>= fun results => pipelineSum context results with
    | some (.ok total) => .value total last
    | some (.error failure) => .exec (.failed failure last)
    | none => .exec .stuck
  | .firstItem =>
    match listValues out with
    | some (item :: _) => .value (.ok item) last
    | some [] => .value (.err context.errors.streamEmpty) last
    | none => .exec .stuck
  | .anyMatch =>
    match listValues out with
    | some results => .value (.scalar (.boolean !results.isEmpty)) last
    | none => .exec .stuck
  | .allMatch =>
    match listValues out with
    | some results => .value (.scalar (.boolean results.isEmpty)) last
    | none => .exec .stuck
  | .lastItem =>
    match listValues out with
    | some results =>
      match results.getLast? with
      | some item => .value (.ok item) last
      | none => .value (.err context.errors.streamEmpty) last
    | none => .exec .stuck
  | .minimum | .maximum =>
    match listValues out with
    | some (item :: rest) =>
      match extremum context (ending matches .maximum) item rest with
      | some chosen => .value (.ok chosen) last
      | none => .exec .stuck
    | some [] => .value (.err context.errors.streamEmpty) last
    | none => .exec .stuck
  | .folded _ _ _ _ => .exec .unsupported

/-- A fold's accumulator over the items that reached it: each runs the
callback with the accumulator and the item bound, in its own scope, and the
callback's value is the next accumulator (SPEC 13.3). -/
def foldValues {world : World Declaration Identity Source}
    (scope : Frame world → List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world)
    (accumulator item : Xsh.Syntax.RawName) (body : List Xsh.Syntax.Statement) :
    Value Declaration Identity Source → Frame world → List (Value Declaration Identity Source) → Valued world
  | current, frame, [] => .value current frame
  | current, frame, next :: rest =>
    (scope frame [(accumulator, current), (item, next)] body).bind fun updated after =>
      foldValues scope accumulator item body updated after rest

/-- A pipeline's value once its items have passed the stages: a closing
`fold` or `reduce` evaluates its initial value once every item has reached
it, as the binary does, then folds them in order; any other ending is
`finishPipeline`. -/
def closePipeline {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
    (scope : Frame world → List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world)
    (ending : PipelineEnd) (out : Value Declaration Identity Source) (last : Frame world) : Valued world :=
  match ending with
  | .folded initial accumulator item body =>
    match listValues out with
    | some items =>
      (evaluate last initial).bind fun start after => foldValues scope accumulator item body start after items
    | none => .exec .stuck
  | other => finishPipeline context other out last

/-- A structured pipeline over a list source (SPEC 13.1): its items pass the
stages through `run`, then the closing stage gives the value. -/
def runPipeline {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (run : Frame world → List Xsh.Syntax.PipeStage → Option Nat → List (Value Declaration Identity Source) → Valued world)
    (next : Frame world) (source : Value Declaration Identity Source) (body : List Xsh.Syntax.PipeStage)
    (ending : PipelineEnd) (limit : Option Nat) : Valued world :=
  match listValues source with
  | some items => (run next body limit items).bind fun out last => finishPipeline context ending out last
  | none => .exec .unsupported

/-- A condition in a control position (SPEC 8.3): a `Result[Bool]` gives the
`Bool` of its `Ok` and raises the error of its `Err`, and so does each operand
of `!`, `and` and `or` there; `and` and `or` skip the operand they do not
need, which then does not run or fail. -/
def controlValue {world : World Declaration Identity Source}
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) : Frame world → Xsh.Syntax.Expr → Valued world
  | frame, .binary .and left right =>
    (controlValue evaluate frame left).bind fun first after =>
      match first.eraseNominal with
      | .scalar (.boolean false) => .value (.scalar (.boolean false)) after
      | .scalar (.boolean true) => controlValue evaluate after right
      | _ => .exec .unsupported
  | frame, .binary .or left right =>
    (controlValue evaluate frame left).bind fun first after =>
      match first.eraseNominal with
      | .scalar (.boolean true) => .value (.scalar (.boolean true)) after
      | .scalar (.boolean false) => controlValue evaluate after right
      | _ => .exec .unsupported
  | frame, .unary .not operand =>
    (controlValue evaluate frame operand).bind fun tested after =>
      match tested.eraseNominal with
      | .scalar (.boolean value) => .value (.scalar (.boolean !value)) after
      | _ => .exec .unsupported
  | frame, condition =>
    (evaluate frame condition).bind fun tested after =>
      match conditionValue tested with
      | some (.ok value) => .value (.scalar (.boolean value)) after
      | some (.error error) => .exec (.failed (.raised error) after)
      | none => .exec .unsupported

/-- A brace literal's entries evaluated once each in source order, a
computed key before its value (SPEC 6.4); a shorthand field reads its name. -/
def threadEntries {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) :
    Frame world → List Xsh.Syntax.RecordField → Outcome world (List (BraceEntry Declaration Identity Source))
  | frame, [] => .value [] frame
  | frame, .named name operand :: rest =>
    (evaluate frame operand).bind fun value after =>
      (threadEntries context evaluate after rest).bind fun entries last =>
        .value (.field (context.spelling name) value :: entries) last
  | frame, .shorthand name :: rest =>
    (evaluate frame (.ident name)).bind fun value after =>
      (threadEntries context evaluate after rest).bind fun entries last =>
        .value (.field (context.spelling name) value :: entries) last
  | frame, .computed key operand :: rest =>
    (evaluate frame key).bind fun computed after =>
      (evaluate after operand).bind fun value middle =>
        (threadEntries context evaluate middle rest).bind fun entries last =>
          .value (.key computed value :: entries) last
  | frame, .spread operand :: rest =>
    (evaluate frame operand).bind fun value after =>
      (threadEntries context evaluate after rest).bind fun entries last => .value (.spread value :: entries) last
  | frame, .path names operand :: rest =>
    (evaluate frame operand).bind fun value after =>
      (threadEntries context evaluate after rest).bind fun entries last =>
        .value (.path (names.map context.spelling) value :: entries) last

def namedOnly : List Xsh.Syntax.RecordField → Bool
  | [] => true
  | .named _ _ :: rest => namedOnly rest
  | _ :: _ => false

def threadBranches {world : World Declaration Identity Source}
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) :
    Frame world → List (Xsh.Syntax.Expr × Xsh.Syntax.Expr) → Xsh.Syntax.Expr → Valued world
  | frame, [], otherwise => evaluate frame otherwise
  | frame, (condition, value) :: rest, otherwise =>
    (controlValue evaluate frame condition).bind fun tested after =>
      match tested.eraseNominal with
      | .scalar (.boolean true) => evaluate after value
      | .scalar (.boolean false) => threadBranches evaluate after rest otherwise
      | _ => .exec .stuck

/-- `CoreEval.step` with the frame threaded: the same forms and operator
meanings (`binary`, `unary`, `indexAt`, `fieldOf`), each operand evaluated in
the frame the one before it left, in source order. `recurse` evaluates a
subterm one fuel level down, and `callFunction` runs a named function's body
with its arguments bound. -/
def threadStep (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (recurse : Frame world → Xsh.Syntax.Expr → Valued world)
    (callFunction : Environment → Locals Declaration Identity Source → FunctionBody → Frame world → Valued world)
    (frame : Frame world) : Xsh.Syntax.Expr → Valued world
  | .literal value => fromEval frame (literal context.literals value)
  | .ident name =>
    match frame.locals.lookup name with
    | some value => .value value frame
    | none =>
      match frame.environment.cellOf name with
      | .ok cell => match frame.store cell with
        | some found => .value found.contents.value frame
        | none => .exec .stuck
      | .error _ => .exec .stuck
  | .list elements => (threadAll recurse frame elements).bind fun values after => .value (.list values) after
  | .record fields => (threadFields context recurse frame fields).bind fun values after => .value (.record values) after
  | .field base name => (recurse frame base).bind fun value after => fromEval after (fieldOf context name value)
  | .index base key false =>
    (recurse frame base).bind fun container after =>
      (recurse after key).bind fun selector last => fromEval last (indexAt context (negativeLiteral key) container selector)
  | .unary operator operand => (recurse frame operand).bind fun value after => fromEval after (unary context operator value)
  | .binary .and left right =>
    (recurse frame left).bind fun first after =>
      match first.eraseNominal with
      | .scalar (.boolean false) => .value (boolean false) after
      | .scalar (.boolean true) =>
        (recurse after right).bind fun second last =>
          match second.eraseNominal with
          | .scalar (.boolean value) => .value (boolean value) last
          | _ => .exec .stuck
      | _ => .exec .stuck
  | .binary .or left right =>
    (recurse frame left).bind fun first after =>
      match first.eraseNominal with
      | .scalar (.boolean true) => .value (boolean true) after
      | .scalar (.boolean false) =>
        (recurse after right).bind fun second last =>
          match second.eraseNominal with
          | .scalar (.boolean value) => .value (boolean value) last
          | _ => .exec .stuck
      | _ => .exec .stuck
  | .binary operator left right =>
    (recurse frame left).bind fun first after =>
      (recurse after right).bind fun second last => fromEval last (binary context operator first second)
  | .ifExpr conditions otherwise => threadBranches recurse frame conditions otherwise
  | .call (.ident name) arguments =>
    match frame.locals.lookup name, context.functions name, positional arguments with
    | none, some (defining, parameters, body), some values =>
      (threadAll recurse frame values).bind fun evaluated after =>
        if parameters.length = evaluated.length then callFunction defining (parameters.zip evaluated) body after
        else .exec .stuck
    | _, _, _ => .exec .unsupported
  | _ => .exec .unsupported

/-- A call's arguments evaluated once each in written order (SPEC 6.3), each
in the frame the one before it left: an `@` splice supplies a list's items
positionally and a `...` spread a record's fields by label. -/
def threadArguments {world : World Declaration Identity Source} (spelling : Xsh.Syntax.RawName → String)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) :
    Frame world → List Xsh.Syntax.CallArgument → Outcome world (List (Supplied Declaration Identity Source))
  | frame, [] => .value [] frame
  | frame, .positional value :: rest =>
    (evaluate frame value).bind fun result after =>
      (threadArguments spelling evaluate after rest).bind fun others last => .value (.positional result :: others) last
  | frame, .named label value :: rest =>
    (evaluate frame value).bind fun result after =>
      (threadArguments spelling evaluate after rest).bind fun others last =>
        .value (.named (spelling label) result :: others) last
  | frame, .splice value :: rest =>
    (evaluate frame value).bind fun result after =>
      match listValues result with
      | some items =>
        (threadArguments spelling evaluate after rest).bind fun others last =>
          .value (items.map .positional ++ others) last
      | none => .exec .stuck
  | frame, .namedSpread value :: rest =>
    (evaluate frame value).bind fun result after =>
      match result.eraseNominal with
      | .record fields =>
        (threadArguments spelling evaluate after rest).bind fun others last =>
          .value (fields.map (fun field => .named field.1 field.2) ++ others) last
      | _ => .exec .stuck

def Supplied.positionalValue : Supplied Declaration Identity Source → Option (Value Declaration Identity Source)
  | .positional value => some value
  | .named _ _ => none

/-- A numeric method call (SPEC 6.3): the receiver evaluates first, then each
argument once in written order. -/
def numericCall {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) (frame : Frame world) (receiver : Xsh.Syntax.Expr)
    (name : String) (arguments : List Xsh.Syntax.CallArgument) : Valued world :=
  (evaluate frame receiver).bind fun target after =>
    (threadArguments context.spelling evaluate after arguments).bind fun supplied last =>
      match supplied.mapM Supplied.positionalValue with
      | some values => fromEval last (numericMethod context.errors name target values)
      | none => .exec .unsupported

/-- A list, map or set method (SPEC 4.5). -/
inductive CollectionMethod where
  | len | isEmpty | push | extend | get | toSet | join | keys | values | set | remove | add | toList

def CollectionMethod.ofName : String → Option CollectionMethod
  | "len" => some .len
  | "is_empty" => some .isEmpty
  | "push" => some .push
  | "extend" => some .extend
  | "get" => some .get
  | "to_set" => some .toSet
  | "join" => some .join
  | "keys" => some .keys
  | "values" => some .values
  | "set" => some .set
  | "remove" => some .remove
  | "add" => some .add
  | "to_list" => some .toList
  | _ => none

/-- The methods of lists, maps and sets (SPEC 4.5): `len`, `is_empty`; a
list's `push`, `extend`, `get`, `to_set` and a `Str` list's `join`; a map's
`keys` and `values` in key order, `set` replacing or inserting a key in key
order, `remove` and `get`; and a set's `add`, `remove` and `to_list`. A
`get` gives `Ok` of the element or entry, or `Err` of its miss. -/
def countedValue (count : Nat) : Result Declaration Identity Source (Value Declaration Identity Source) :=
  match countValue count with
  | some value => .value value
  | none => .unsupported

def listMethod (errors : Errors Declaration Identity Source) :
    CollectionMethod → List (Value Declaration Identity Source) → List (Value Declaration Identity Source) →
      Result Declaration Identity Source (Value Declaration Identity Source)
  | .len, items, [] => countedValue items.length
  | .isEmpty, items, [] => .value (.scalar (.boolean items.isEmpty))
  | .push, items, [item] => .value (.list (items ++ [item]))
  | .extend, items, [.list more] => .value (.list (items ++ more))
  | .get, items, [.scalar (.integer index)] =>
    if 0 ≤ index.val then
      match items[index.val.toNat]? with
      | some item => .value (.ok item)
      | none => .value (.err (errors.listGet index.val))
    else .value (.err (errors.listGet index.val))
  | .toSet, items, [] =>
    match items.mapM mapKeyOf with
    | some keys => .value (.set (setOf keys))
    | none => .unsupported
  | .join, items, separator =>
    match items.mapM (fun item => match item.eraseNominal with
        | .scalar (.string text) => some text
        | _ => none), separator with
    | some texts, [] => .value (.scalar (.string (String.join texts)))
    | some texts, [.scalar (.string between)] => .value (.scalar (.string (between.intercalate texts)))
    | _, _ => .unsupported
  | _, _, _ => .unsupported

def mapMethod (errors : Errors Declaration Identity Source) :
    CollectionMethod → List (MapKey × Value Declaration Identity Source) → List (Value Declaration Identity Source) →
      Result Declaration Identity Source (Value Declaration Identity Source)
  | .len, entries, [] => countedValue entries.length
  | .isEmpty, entries, [] => .value (.scalar (.boolean entries.isEmpty))
  | .keys, entries, [] => .value (.list (entries.map fun entry => keyValue entry.1))
  | .values, entries, [] => .value (.list (entries.map Prod.snd))
  | .set, entries, [key, value] =>
    match mapKeyOf key with
    | some found => .value (.map (upsert found value entries))
    | none => .unsupported
  | .remove, entries, [key] =>
    match mapKeyOf key with
    | some found => .value (.map (entries.filter fun entry => entry.1 != found))
    | none => .unsupported
  | .push, entries, [key, value] =>
    match mapKeyOf key with
    | some found =>
      match entries.lookup found with
      | some held =>
        match held.eraseNominal with
        | .list items => .value (.map (upsert found (.list (items ++ [value])) entries))
        | _ => .unsupported
      | none => .value (.map (upsert found (.list [value]) entries))
    | none => .unsupported
  | .get, entries, [key] =>
    match mapKeyOf key with
    | some found =>
      match entries.lookup found with
      | some value => .value (.ok value)
      | none => .value (.err (errors.mapGet found))
    | none => .unsupported
  | _, _, _ => .unsupported

def setMethod : CollectionMethod → List MapKey → List (Value Declaration Identity Source) →
    Result Declaration Identity Source (Value Declaration Identity Source)
  | .len, keys, [] => countedValue keys.length
  | .isEmpty, keys, [] => .value (.scalar (.boolean keys.isEmpty))
  | .add, keys, [item] =>
    match mapKeyOf item with
    | some found => .value (.set (insertKey found keys))
    | none => .unsupported
  | .remove, keys, [item] =>
    match mapKeyOf item with
    | some found => .value (.set (keys.filter fun key => key != found))
    | none => .unsupported
  | .toList, keys, [] => .value (.list (keys.map keyValue))
  | _, _, _ => .unsupported

def insertLabel (label : String) : List String → List String
  | [] => [label]
  | first :: rest => if label ≤ first then label :: first :: rest else first :: insertLabel label rest

/-- Labels in ascending order. -/
def sortLabels : List String → List String
  | [] => []
  | label :: rest => insertLabel label (sortLabels rest)

/-- A record's `keys`, its labels in sorted order, and `get`, `Ok` of a
field's value or the `missing-field` `Err` (SPEC 4.6). -/
def recordMethod (errors : Errors Declaration Identity Source) :
    CollectionMethod → List (String × Value Declaration Identity Source) → List (Value Declaration Identity Source) →
      Result Declaration Identity Source (Value Declaration Identity Source)
  | .keys, fields, [] =>
    .value (.list ((sortLabels (fields.map Prod.fst)).map fun label => .scalar (.string label)))
  | .get, fields, [.scalar (.string label)] =>
    match fields.lookup label with
    | some value => .value (.ok value)
    | none => .value (.err (errors.described "missing-field" s!"missing field `{label}`"))
  | _, _, _ => .unsupported

def collectionMethod (errors : Errors Declaration Identity Source) (method : CollectionMethod)
    (receiver : Value Declaration Identity Source) (arguments : List (Value Declaration Identity Source)) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  match receiver.eraseNominal with
  | .list items => listMethod errors method items (arguments.map Value.eraseNominal)
  | .map entries => mapMethod errors method entries (arguments.map Value.eraseNominal)
  | .set keys => setMethod method keys (arguments.map Value.eraseNominal)
  | .record fields => recordMethod errors method fields (arguments.map Value.eraseNominal)
  | _ => .unsupported

/-- A text method (SPEC 4.4). -/
inductive TextMethod where
  | byteLen | countChars | isEmpty | startsWith | endsWith | reverse | split | find | replace | lines
  | trim | words | fields | countWords | countLines | squeeze | delete | translate | byteAt | byteSlice
  | parseInt | parseIntDecimal | parseUint | parseUintPositive | wrap | base64Decode | base32Decode

def TextMethod.ofName : String → Option TextMethod
  | "byte_len" => some .byteLen
  | "count_chars" => some .countChars
  | "is_empty" => some .isEmpty
  | "starts_with" => some .startsWith
  | "ends_with" => some .endsWith
  | "reverse" => some .reverse
  | "split" => some .split
  | "wrap" => some .wrap
  | "base64_decode" => some .base64Decode
  | "base32_decode" => some .base32Decode
  | "find" => some .find
  | "replace" => some .replace
  | "lines" => some .lines
  | "trim" => some .trim
  | "words" => some .words
  | "fields" => some .fields
  | "count_words" => some .countWords
  | "count_lines" => some .countLines
  | "squeeze" => some .squeeze
  | "delete" => some .delete
  | "translate" => some .translate
  | "byte_at" => some .byteAt
  | "byte_slice" => some .byteSlice
  | "parse_int" => some .parseInt
  | "parse_int_decimal" => some .parseIntDecimal
  | "parse_uint" => some .parseUint
  | "parse_uint_positive" => some .parseUintPositive
  | _ => none

/-- Unicode `White_Space` (UCD PropList), the set Rust's `char::is_whitespace`
tests. -/
def isUnicodeWhitespace (char : Char) : Bool :=
  let code := char.val
  (9 ≤ code ∧ code ≤ 13) || code == 32 || code == 0x85 || code == 0xA0 || code == 0x1680 ||
    (0x2000 ≤ code ∧ code ≤ 0x200A) || code == 0x2028 || code == 0x2029 || code == 0x202F || code == 0x205F ||
    code == 0x3000

/-- The maximal runs of non-whitespace characters. -/
def wordRuns (text : List Char) : List (List Char) :=
  (text.splitBy fun first second => isUnicodeWhitespace first == isUnicodeWhitespace second).filter fun run =>
    match run with
    | first :: _ => !isUnicodeWhitespace first
    | [] => false

/-- Every non-overlapping occurrence of `pattern` replaced left to right; an
empty pattern matches before each character and at the end. -/
def replaceRun (pattern replacement : List Char) : Nat → List Char → List Char
  | 0, text => text
  | _ + 1, [] => if pattern.isEmpty then replacement else []
  | fuel + 1, first :: rest =>
    if pattern.isEmpty then replacement ++ first :: replaceRun pattern replacement fuel rest
    else if pattern.isPrefixOf (first :: rest) then
      replacement ++ replaceRun pattern replacement fuel ((first :: rest).drop pattern.length)
    else first :: replaceRun pattern replacement fuel rest

/-- The pieces between occurrences of a non-empty `separator`, an empty piece
where two occurrences meet or one ends the text. -/
def splitRun (separator : List Char) : Nat → List Char → List Char → List (List Char)
  | 0, current, text => [current ++ text]
  | _ + 1, current, [] => [current]
  | fuel + 1, current, first :: rest =>
    if separator.isPrefixOf (first :: rest) then
      current :: splitRun separator fuel [] ((first :: rest).drop separator.length)
    else splitRun separator fuel (current ++ [first]) rest

/-- `splitRun` stopping after `splits` occurrences, the rest of the text its
last piece. -/
def splitAtMost (separator : List Char) : Nat → Nat → List Char → List Char → List (List Char)
  | 0, _, current, text => [current ++ text]
  | _ + 1, 0, current, text => [current ++ text]
  | _ + 1, _ + 1, current, [] => [current]
  | fuel + 1, splits + 1, current, first :: rest =>
    if separator.isPrefixOf (first :: rest) then
      current :: splitAtMost separator fuel splits [] ((first :: rest).drop separator.length)
    else splitAtMost separator fuel (splits + 1) (current ++ [first]) rest

/-- `text.split(separator, maxsplit)`: a negative limit splits at every
occurrence; otherwise at most `limit` times, an empty separator giving the
first `limit` characters singly and the rest as one piece. -/
def splitLimited (separator text : List Char) (limit : Int) : List (List Char) :=
  if limit < 0 then
    if separator.isEmpty then text.map fun char => [char] else splitRun separator (text.length + 1) [] text
  else if separator.isEmpty then
    if limit = 0 then [text]
    else (text.take limit.toNat).map (fun char => [char]) ++ (if limit.toNat < text.length then [text.drop limit.toNat] else [])
  else splitAtMost separator (text.length + 1) limit.toNat [] text

/-- A word cut into pieces of `width` characters, the last one shorter. -/
def chunksOf (width : Nat) : Nat → List Char → List (List Char)
  | 0, _ => []
  | _ + 1, [] => []
  | fuel + 1, word => word.take width :: chunksOf width fuel (word.drop width)

/-- One piece added to the lines wrapped so far: onto the current line after
a space when it fits in `width`, else starting the next line. -/
def appendChunk (width : Nat) (state : List (List Char) × List Char) (chunk : List Char) :
    List (List Char) × List Char :=
  match state with
  | (output, []) => (output, chunk)
  | (output, current) =>
    if current.length + 1 + chunk.length ≤ width then (output, current ++ ' ' :: chunk)
    else (output ++ [current], chunk)

/-- One line wrapped: its whitespace-separated words, each cut into pieces of
at most `width` characters, packed greedily with single spaces. -/
def wrapLine (width : Nat) (line : List Char) : List (List Char) :=
  let (output, current) := ((wordRuns line).flatMap (chunksOf width (line.length + 1))).foldl (appendChunk width) ([], [])
  output ++ [current]

/-- The byte offset of the first occurrence of `needle` at or after `index`. -/
def findBytes (needle : List UInt8) : Nat → List UInt8 → Option Nat
  | index, [] => if needle.isEmpty then some index else none
  | index, first :: rest => if needle.isPrefixOf (first :: rest) then some index else findBytes needle (index + 1) rest

/-- A digit's value in `base`: `0`–`9`, then `a`–`z` and `A`–`Z` from ten. -/
def radixDigit (base : Nat) (char : Char) : Option Nat :=
  let value :=
    if '0' ≤ char ∧ char ≤ '9' then some (char.toNat - '0'.toNat)
    else if 'a' ≤ char ∧ char ≤ 'z' then some (char.toNat - 'a'.toNat + 10)
    else if 'A' ≤ char ∧ char ≤ 'Z' then some (char.toNat - 'A'.toNat + 10)
    else none
  value.bind fun digit => if digit < base then some digit else none

/-- The non-empty digits' value in `base`. -/
def digitsValue (base : Nat) (chars : List Char) : Option Nat :=
  if chars.isEmpty then none
  else chars.foldl (fun total char => total.bind fun sum => (radixDigit base char).map (sum * base + ·)) (some 0)

/-- Rust's `i64::from_str_radix`: one optional sign, then non-empty digits,
in range. -/
def int64FromRadix (base : Nat) : List Char → Option Int
  | '+' :: rest => (digitsValue base rest).bind fun value =>
      if (value : Int) ≤ 9223372036854775807 then some value else none
  | '-' :: rest => (digitsValue base rest).bind fun value =>
      if -9223372036854775808 ≤ -(value : Int) then some (-(value : Int)) else none
  | chars => (digitsValue base chars).bind fun value =>
      if (value : Int) ≤ 9223372036854775807 then some value else none

def trimWhitespace (chars : List Char) : List Char :=
  ((chars.dropWhile isUnicodeWhitespace).reverse.dropWhile isUnicodeWhitespace).reverse

def asciiDigits (chars : List Char) : Bool := chars.all fun char => '0' ≤ char ∧ char ≤ '9'

/-- `Str.parse_int()`: trimmed, one leading `-` then one `+` removed, an
optional `0x`, `0o` or `0b` base prefix in either case, and every `_` removed,
then Rust's `i64::from_str_radix`. -/
def parseIntText (text : String) : Except String Int :=
  let trimmed := trimWhitespace text.toList
  let (negative, signed) := match trimmed with
    | '-' :: rest => (true, rest)
    | rest => (false, rest)
  let unsigned := match signed with
    | '+' :: rest => rest
    | rest => rest
  let (base, digits) := match unsigned with
    | '0' :: 'x' :: rest | '0' :: 'X' :: rest => (16, rest)
    | '0' :: 'o' :: rest | '0' :: 'O' :: rest => (8, rest)
    | '0' :: 'b' :: rest | '0' :: 'B' :: rest => (2, rest)
    | rest => (10, rest)
  let cleaned := digits.filter (· != '_')
  if cleaned.isEmpty then .error "expected integer"
  else
    match int64FromRadix base cleaned with
    | some value =>
      if negative then
        if -9223372036854775808 ≤ -value then .ok (-value) else .error s!"invalid integer `{text}`"
      else .ok value
    | none => .error s!"invalid integer `{text}`"

/-- Each run of one repeated character that `squeezed` admits collapsed to one. -/
def squeezeRuns (squeezed : Char → Bool) : List Char → List Char
  | [] => []
  | [char] => [char]
  | first :: second :: rest =>
    if first = second ∧ squeezed first then squeezeRuns squeezed (second :: rest)
    else first :: squeezeRuns squeezed (second :: rest)

/-- Each character of `source` mapped to the character at its first position
in `source` within `target`, or deleted when `target` is shorter. -/
def translateChars (source target : List Char) (text : List Char) : List Char :=
  text.flatMap fun char =>
    match source.idxOf? char with
    | some index => target[index]?.toList
    | none => [char]

/-- `Str.lines()`: the text split at each `\n`, a `\r` before it removed and
no empty line after a final `\n`. -/
def textLines (text : List Char) : List (List Char) :=
  let pieces := splitRun ['\n'] (text.length + 1) [] text
  let complete := if pieces.getLast? = some [] then pieces.dropLast else pieces
  complete.map fun line => if line.getLast? = some '\r' then line.dropLast else line

/-- `text.wrap(width)`: each line wrapped, and an empty last line when the
text ends with a newline. -/
def wrapText (width : Nat) (text : List Char) : List (List Char) :=
  (textLines text).flatMap (wrapLine width) ++ (if text.getLast? = some '\n' then [[]] else [])

def textValue (chars : List Char) : Value Declaration Identity Source := .scalar (.string (String.ofList chars))

/-- `Str.byte_slice(offset, length:)` (SPEC 4.4): the bytes from `offset`,
`length` of them or to the end, clamped to the text, as text; a negative
offset or length, an offset past the end, or a slice whose ends are not UTF-8
character boundaries is the runtime `text-byte-slice` failure. -/
def byteSliceOf (errors : Errors Declaration Identity Source) (text : String) (offset : Int) (length : Option Int) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  let bytes := text.toUTF8.toList
  if offset < 0 then .failure (.runtime (errors.textByteSlice "offset cannot be negative"))
  else if length.any (· < 0) then .failure (.runtime (errors.textByteSlice "length cannot be negative"))
  else if bytes.length < offset.toNat then .failure (.runtime (errors.textByteSlice "offset is past end of text"))
  else
    let taken := match length with
      | some count => (bytes.drop offset.toNat).take count.toNat
      | none => bytes.drop offset.toNat
    match String.fromUTF8? ⟨taken.toArray⟩ with
    | some piece => .value (.scalar (.string piece))
    | none => .failure (.runtime (errors.textByteSlice "slice must align to UTF-8 boundaries"))

def isAsciiWhitespaceByte (byte : UInt8) : Bool :=
  byte == 32 || byte == 9 || byte == 10 || byte == 12 || byte == 13

/-- `Str.base64_decode()` and `Str.base32_decode()` on the text's bytes with
ASCII whitespace removed: `Ok` of the decoded bytes, or the `invalid-base64`
or `invalid-base32` `Err` naming the failure and its position. -/
def baseDecoded (errors : Errors Declaration Identity Source) (alphabet : Xsh.Values.BaseCodec.Alphabet)
    (data : List UInt8) : Result Declaration Identity Source (Value Declaration Identity Source) :=
  match Xsh.Values.BaseCodec.decode alphabet data with
  | .ok decoded => .value (.ok (.scalar (.bytes decoded)))
  | .error (failure, position) =>
    .value (.err (errors.described s!"invalid-{alphabet.name}" s!"invalid {alphabet.name}: {failure.text} at {position}"))

/-- The text methods (SPEC 4.4): `byte_len` and `count_chars` count UTF-8
bytes and Unicode scalar values; `starts_with`, `ends_with`, `reverse` by
scalar value; `split` at a separator, an empty one giving each character;
`find` the byte offset of a needle at or after an optional byte `start`, or
`null`; `replace(from, with)` every non-overlapping occurrence; and `lines`. -/
def textMethod (errors : Errors Declaration Identity Source) (method : TextMethod) (text : String)
    (supplied : List (Supplied Declaration Identity Source)) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  let integerParse := fun (kind : String) (parsed : Except String Int) =>
    match parsed with
    | .ok value =>
      match validateInt? value with
      | some valid => Result.value (Value.ok (.scalar (.integer valid)))
      | none => Result.unsupported
    | .error message => Result.value (Value.err (errors.described kind message))
  let stringOf := fun (value : Value Declaration Identity Source) => match value.eraseNominal with
    | .scalar (.string found) => some found
    | _ => none
  match method, supplied with
  | .byteLen, [] => countedValue text.toUTF8.size
  | .countChars, [] => countedValue text.length
  | .isEmpty, [] => .value (.scalar (.boolean text.isEmpty))
  | .startsWith, [.positional head] =>
    match stringOf head with
    | some found => .value (.scalar (.boolean (found.toList.isPrefixOf text.toList)))
    | none => .unsupported
  | .endsWith, [.positional suffix] =>
    match stringOf suffix with
    | some found => .value (.scalar (.boolean (found.toList.reverse.isPrefixOf text.toList.reverse)))
    | none => .unsupported
  | .reverse, [] => .value (textValue text.toList.reverse)
  | .split, [.positional separator] =>
    match stringOf separator with
    | some found =>
      if found.isEmpty then .value (.list (text.toList.map fun char => textValue [char]))
      else .value (.list ((splitRun found.toList (text.length + 1) [] text.toList).map textValue))
    | none => .unsupported
  | .base64Decode, [] => baseDecoded errors .base64 (text.toUTF8.toList.filter fun byte => !isAsciiWhitespaceByte byte)
  | .base32Decode, [] =>
    baseDecoded errors .base32 ((text.toUTF8.toList.filter fun byte => !isAsciiWhitespaceByte byte).map fun byte =>
      if 97 ≤ byte ∧ byte ≤ 122 then byte - 32 else byte)
  | .wrap, [.positional width] =>
    match width.eraseNominal with
    | .scalar (.integer count) =>
      if 0 < count.val then .value (.list ((wrapText count.val.toNat text.toList).map textValue))
      else .failure (.runtime (errors.described "text-wrap" "width must be positive"))
    | _ => .unsupported
  | .split, [.positional separator, limit] =>
    let bound := match limit with
      | .positional value | .named "maxsplit" value =>
        match value.eraseNominal with
        | .scalar (.integer count) => some count.val
        | _ => none
      | _ => none
    match stringOf separator, bound with
    | some found, some count => .value (.list ((splitLimited found.toList text.toList count).map textValue))
    | _, _ => .unsupported
  | .find, .positional needle :: start =>
    let begin := match start with
      | [] => some 0
      | [.positional offset] | [.named "start" offset] =>
        match offset.eraseNominal with
        | .scalar (.integer index) => if 0 ≤ index.val then some index.val.toNat else none
        | _ => none
      | _ => none
    match stringOf needle, begin with
    | some found, some origin =>
      if origin ≤ text.toUTF8.size then
        match findBytes found.toUTF8.toList origin (text.toUTF8.toList.drop origin) with
        | some offset => countedValue offset
        | none => .value (.scalar .null)
      else .unsupported
    | _, _ => .unsupported
  | .replace, [.positional pattern, replacement] =>
    let replaced := match replacement with
      | .positional value | .named "with" value => stringOf value
      | _ => none
    match stringOf pattern, replaced with
    | some source, some target => .value (textValue (replaceRun source.toList target.toList (text.length + 1) text.toList))
    | _, _ => .unsupported
  | .lines, [] => .value (.list ((textLines text.toList).map textValue))
  | .countLines, [] => countedValue (textLines text.toList).length
  | .trim, [] => .value (textValue (trimWhitespace text.toList))
  | .words, [] | .fields, [] => .value (.list ((wordRuns text.toList).map textValue))
  | .countWords, [] => countedValue (wordRuns text.toList).length
  | .fields, [delimiter] =>
    let written := match delimiter with
      | .positional value | .named "delimiter" value => stringOf value
      | _ => none
    match written with
    | some found =>
      if found.isEmpty then .value (.list (if text.isEmpty then [] else [textValue text.toList]))
      else .value (.list (((splitRun found.toList (text.length + 1) [] text.toList).filter
        fun piece => !piece.isEmpty).map textValue))
    | none => .unsupported
  | .squeeze, [] => .value (textValue (squeezeRuns (fun _ => true) text.toList))
  | .squeeze, [.positional chars] | .squeeze, [.named "chars" chars] =>
    match stringOf chars with
    | some set => .value (textValue (squeezeRuns (fun char => set.toList.contains char) text.toList))
    | none => .unsupported
  | .delete, [.positional chars] | .delete, [.named "chars" chars] =>
    match stringOf chars with
    | some set => .value (textValue (text.toList.filter fun char => !set.toList.contains char))
    | none => .unsupported
  | .translate, [.positional source, target] =>
    let replaced := match target with
      | .positional value | .named "to" value => stringOf value
      | _ => none
    match stringOf source, replaced with
    | some from', some to' => .value (textValue (translateChars from'.toList to'.toList text.toList))
    | _, _ => .unsupported
  | .byteAt, [.positional index] =>
    match index.eraseNominal with
    | .scalar (.integer offset) =>
      if 0 ≤ offset.val then
        match text.toUTF8.toList[offset.val.toNat]? with
        | some byte => countedValue byte.toNat
        | none => .value (.scalar .null)
      else .value (.scalar .null)
    | _ => .unsupported
  | .parseInt, [] => integerParse "parse-int" (parseIntText text)
  | .parseIntDecimal, [] =>
    let chars := text.toList
    if !chars.isEmpty && asciiDigits chars && (chars = ['0'] || chars.head? != some '0') then
      match int64FromRadix 10 chars with
      | some value => integerParse "parse-int" (.ok value)
      | none => integerParse "parse-int" (.error s!"invalid integer `{text}`")
    else integerParse "parse-int" (.error s!"invalid integer `{text}`")
  | .parseUint, [] =>
    let trimmed := trimWhitespace text.toList
    if trimmed.head? = some '+' || trimmed.head? = some '-' then
      integerParse "parse-uint" (.error "expected unsigned integer")
    else if trimmed.isEmpty || !asciiDigits trimmed then
      integerParse "parse-uint" (.error s!"invalid unsigned integer `{text}`")
    else
      match int64FromRadix 10 trimmed with
      | some value => integerParse "parse-uint" (.ok value)
      | none => integerParse "parse-uint" (.error s!"invalid unsigned integer `{text}`")
  | .parseUintPositive, [] =>
    let trimmed := trimWhitespace text.toList
    if trimmed.isEmpty || !asciiDigits trimmed then
      integerParse "parse-uint-positive" (.error s!"invalid positive integer `{text}`")
    else
      match int64FromRadix 10 trimmed with
      | some 0 => integerParse "parse-uint-positive" (.error "expected positive integer")
      | some value => integerParse "parse-uint-positive" (.ok value)
      | none => integerParse "parse-uint-positive" (.error s!"invalid positive integer `{text}`")
  | .byteSlice, .positional offset :: span =>
    let length := match span with
      | [] => some none
      | [.positional value] | [.named "length" value] =>
        match value.eraseNominal with
        | .scalar (.integer count) => some (some count.val)
        | _ => none
      | _ => none
    match offset.eraseNominal, length with
    | .scalar (.integer start), some count => byteSliceOf errors text start.val count
    | _, _ => .unsupported
  | _, _ => .unsupported

/-- A `Bytes` method (SPEC 4.4). -/
inductive BytesMethod where
  | len | isEmpty | byteAt | startsWith | endsWith | slice | chunks | lines | countLines | lower | trim | utf8
  | base64 | base32 | strings | compare | dump | md5 | sha1 | sha256 | sha512

def BytesMethod.ofName : String → Option BytesMethod
  | "len" => some .len
  | "is_empty" => some .isEmpty
  | "byte_at" => some .byteAt
  | "starts_with" => some .startsWith
  | "ends_with" => some .endsWith
  | "slice" => some .slice
  | "chunks" => some .chunks
  | "lines" => some .lines
  | "count_lines" => some .countLines
  | "lower" => some .lower
  | "trim" => some .trim
  | "utf8" => some .utf8
  | "base64" => some .base64
  | "base32" => some .base32
  | "strings" => some .strings
  | "compare" => some .compare
  | "dump" => some .dump
  | "md5" => some .md5
  | "sha1" => some .sha1
  | "sha256" => some .sha256
  | "sha512" => some .sha512
  | _ => none


/-- The pieces of `bytes` between each `0x0A`, a `0x0D` before it removed and
no empty piece after a final newline. -/
def byteLines : Nat → List UInt8 → List (List UInt8)
  | 0, _ => []
  | _ + 1, [] => []
  | fuel + 1, bytes =>
    let line := bytes.takeWhile (· != 10)
    let trimmed := if line.getLast? = some 13 then line.dropLast else line
    match bytes.drop line.length with
    | _ :: rest => trimmed :: byteLines fuel rest
    | [] => [trimmed]

/-- Fixed-size pieces of `bytes`, the last one shorter. -/
def byteChunks (size : Nat) : Nat → List UInt8 → List (List UInt8)
  | 0, _ => []
  | _ + 1, [] => []
  | fuel + 1, bytes => bytes.take size :: byteChunks size fuel (bytes.drop size)

/-- The UTF-8 encodings of the Unicode `White_Space` characters. -/
def whitespaceEncodings : List (List UInt8) :=
  ([9, 10, 11, 12, 13, 32, 0x85, 0xA0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005, 0x2006, 0x2007,
    0x2008, 0x2009, 0x200A, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000] : List Nat).map fun code =>
      String.utf8EncodeChar (Char.ofNat code)

def whitespaceEncodingsReversed : List (List UInt8) := whitespaceEncodings.map List.reverse

/-- `bytes` without the leading run of the given encodings. -/
def dropWhitespaceEncodings (encodings : List (List UInt8)) : Nat → List UInt8 → List UInt8
  | 0, bytes => bytes
  | fuel + 1, bytes =>
    match encodings.find? fun encoding => encoding.isPrefixOf bytes with
    | some encoding => dropWhitespaceEncodings encodings fuel (bytes.drop encoding.length)
    | none => bytes

/-- A printable ASCII byte: a graphic character or a space. -/
def isPrintableByte (byte : UInt8) : Bool := 32 ≤ byte && byte ≤ 126

/-- The runs of printable bytes at least `least` long, in order (`strings`). -/
def printableRuns (least : Nat) (bytes : List UInt8) : List (List UInt8) :=
  (bytes.splitBy fun first second => isPrintableByte first == isPrintableByte second).filter fun run =>
    match run with
    | first :: _ => isPrintableByte first && least ≤ run.length
    | [] => false

/-- `value` in `radix` digits, zero-padded on the left to `width`. -/
def paddedDigits (radix width value : Nat) : List Char :=
  let digits := Nat.toDigits radix value
  List.replicate (width - digits.length) '0' ++ digits

/-- `bytes.dump()`: 16 bytes a line, each line its eight-digit hex offset, the
bytes in hex with an extra space after the eighth, and the printable ones
between bars with a `.` for the rest; then the total length. -/
def canonicalDump (bytes : List UInt8) : List Char :=
  let lines := (byteChunks 16 (bytes.length + 1) bytes).zipIdx.map fun (chunk, index) =>
    paddedDigits 16 8 (index * 16) ++ [' ', ' '] ++
      ((List.range 16).flatMap fun column =>
        (if column = 8 then [' '] else []) ++
          match chunk[column]? with
          | some byte => paddedDigits 16 2 byte.toNat ++ [' ']
          | none => [' ', ' ', ' ']) ++
      [' ', '|'] ++ chunk.map (fun byte => if isPrintableByte byte then Char.ofNat byte.toNat else '.') ++ ['|', '\n']
  lines.flatten ++ paddedDigits 16 8 bytes.length

/-- `bytes.dump("hex-u8")` and `("octal-u8")`: 16 bytes a line, each line its
seven-digit octal offset and the bytes as two hex or three octal digits. -/
def radixDump (hex : Bool) (bytes : List UInt8) : List Char :=
  if bytes.isEmpty then paddedDigits 8 7 0
  else
    ['\n'].intercalate ((byteChunks 16 (bytes.length + 1) bytes).zipIdx.map fun (chunk, index) =>
      paddedDigits 8 7 (index * 16) ++ chunk.flatMap fun byte =>
        ' ' :: (if hex then paddedDigits 16 2 byte.toNat else paddedDigits 8 3 byte.toNat))

/-- `left.compare(right)`: the one-based position of the first differing
byte, the line it is on and the two bytes there, `-1` past an end; equal
data has position and line `0` and bytes `-1`. -/
def compareFields (left right : List UInt8) : Option (List (String × Value Declaration Identity Source)) :=
  let shared := (left.zip right).takeWhile fun (one, other) => one == other
  let line := (shared.filter fun (one, _) => one == 10).length + 1
  let differing := fun (bytes : List UInt8) => match bytes[shared.length]? with
    | some byte => (byte.toNat : Int)
    | none => -1
  let int := fun (value : Int) => (validateInt? value).map fun valid => (Value.scalar (.integer valid) : Value Declaration Identity Source)
  let equal := left.length = right.length ∧ shared.length = left.length
  if equal then
    (int 0).bind fun zero => (int (-1)).map fun none =>
      [("equal", .scalar (.boolean true)), ("byte", zero), ("line", zero), ("left", none), ("right", none)]
  else
    (int (shared.length + 1)).bind fun byte => (int line).bind fun line =>
      (int (differing left)).bind fun leftByte => (int (differing right)).map fun rightByte =>
        [("equal", .scalar (.boolean false)), ("byte", byte), ("line", line), ("left", leftByte), ("right", rightByte)]

/-- The length of the longest prefix of `bytes` that is valid UTF-8. -/
def validPrefix (bytes : List UInt8) : Nat :=
  ((List.range (bytes.length + 1)).reverse.find? fun length =>
    (String.fromUTF8? ⟨(bytes.take length).toArray⟩).isSome).getD 0

/-- The `Bytes` methods (SPEC 4.4): `len`, `is_empty`, `byte_at`,
`starts_with`, `ends_with`; `slice` and `chunks`, whose bad arguments are the
runtime `bytes-slice` and `bytes-chunks` failures; `lines` and `count_lines`;
ASCII `lower`; `trim` of ASCII whitespace from ASCII data, and otherwise of
whole UTF-8 encoded Unicode `White_Space` characters; and `utf8`, `Ok` of
the text or the `invalid-utf8` `Err` at the first invalid byte. -/
def bytesMethod (errors : Errors Declaration Identity Source) (method : BytesMethod) (bytes : List UInt8)
    (arguments : List (Value Declaration Identity Source)) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  let bytesValue := fun (data : List UInt8) => (Value.scalar (.bytes data) : Value Declaration Identity Source)
  let failed := fun (kind message : String) => Result.failure (.runtime (errors.described kind message))
  let bytesText := fun (data : List UInt8) =>
    (Value.scalar (.string (String.ofList (data.map fun byte => Char.ofNat byte.toNat))) : Value Declaration Identity Source)
  match method, arguments with
  | .len, [] => countedValue bytes.length
  | .isEmpty, [] => .value (.scalar (.boolean bytes.isEmpty))
  | .byteAt, [.scalar (.integer index)] =>
    if 0 ≤ index.val then
      match bytes[index.val.toNat]? with
      | some byte => countedValue byte.toNat
      | none => .value (.scalar .null)
    else .value (.scalar .null)
  | .startsWith, [.scalar (.bytes head)] => .value (.scalar (.boolean (head.isPrefixOf bytes)))
  | .endsWith, [.scalar (.bytes tail)] => .value (.scalar (.boolean (tail.reverse.isPrefixOf bytes.reverse)))
  | .slice, .scalar (.integer offset) :: span =>
    let length := match span with
      | [] => some none
      | [.scalar (.integer count)] => some (some count.val)
      | _ => none
    match length with
    | some count =>
      if offset.val < 0 then failed "bytes-slice" "offset cannot be negative"
      else if bytes.length < offset.val.toNat then failed "bytes-slice" "offset is past end of byte data"
      else if count.any (· < 0) then failed "bytes-slice" "length cannot be negative"
      else
        let rest := bytes.drop offset.val.toNat
        .value (bytesValue (match count with
          | some taken => rest.take taken.toNat
          | none => rest))
    | none => .unsupported
  | .chunks, [.scalar (.integer size)] =>
    if size.val ≤ 0 then failed "bytes-chunks" "chunk size must be positive"
    else .value (.list ((byteChunks size.val.toNat (bytes.length + 1) bytes).map bytesValue))
  | .lines, [] => .value (.list ((byteLines (bytes.length + 1) bytes).map bytesValue))
  | .countLines, [] =>
    countedValue (if bytes.isEmpty then 0
      else (bytes.filter (· == 10)).length + (if bytes.getLast? = some 10 then 0 else 1))
  | .lower, [] => .value (bytesValue (bytes.map fun byte => if 65 ≤ byte ∧ byte ≤ 90 then byte + 32 else byte))
  | .trim, [] =>
    if bytes.all (· < 128) then
      .value (bytesValue ((bytes.dropWhile isAsciiWhitespaceByte).reverse.dropWhile isAsciiWhitespaceByte).reverse)
    else
      .value (bytesValue (dropWhitespaceEncodings whitespaceEncodingsReversed (bytes.length + 1)
        (dropWhitespaceEncodings whitespaceEncodings (bytes.length + 1) bytes).reverse).reverse)
  | .utf8, [] =>
    match String.fromUTF8? ⟨bytes.toArray⟩ with
    | some text => .value (.ok (.scalar (.string text)))
    | none => .value (.err (errors.described "invalid-utf8"
        s!"byte data is not valid UTF-8 at byte {validPrefix bytes}"))
  | .md5, [] => .value (.scalar (.digest ⟨"md5", Xsh.Values.Digests.md5 bytes⟩))
  | .sha1, [] => .value (.scalar (.digest ⟨"sha1", Xsh.Values.Digests.sha1 bytes⟩))
  | .sha256, [] => .value (.scalar (.digest ⟨"sha256", Xsh.Values.Digests.sha256 bytes⟩))
  | .sha512, [] => .value (.scalar (.digest ⟨"sha512", Xsh.Values.Digests.sha512 bytes⟩))
  | .strings, [] => .value (.list ((printableRuns 4 bytes).map bytesText))
  | .strings, [.scalar (.integer least)] =>
    if least.val ≤ 0 then failed "bytes-strings" "min_len must be positive"
    else .value (.list ((printableRuns least.val.toNat bytes).map bytesText))
  | .compare, [.scalar (.bytes right)] =>
    match compareFields bytes right with
    | some fields => .value (.record fields)
    | none => .unsupported
  | .dump, [] => .value (.scalar (.string (String.ofList (canonicalDump bytes))))
  | .dump, [.scalar (.string format)] =>
    if format = "canonical" then .value (.scalar (.string (String.ofList (canonicalDump bytes))))
    else if format = "hex-u8" then .value (.scalar (.string (String.ofList (radixDump true bytes))))
    else if format = "octal-u8" then .value (.scalar (.string (String.ofList (radixDump false bytes))))
    else failed "bytes-dump" "dump format must be canonical, hex-u8, or octal-u8"
  | .base64, [] => .value (.scalar (.string (String.ofList
      ((Xsh.Values.BaseCodec.encode .base64 bytes).map fun byte => Char.ofNat byte.toNat))))
  | .base32, [] => .value (.scalar (.string (String.ofList
      ((Xsh.Values.BaseCodec.encode .base32 bytes).map fun byte => Char.ofNat byte.toNat))))
  | _, _ => .unsupported

inductive PathMethod where
  | bytes | basename | dirname | extOr | normalize | display
  | components | name | ext | withExt | parent | startsWith | endsWith | stripPrefix | relativeTo

def PathMethod.ofName : String → Option PathMethod
  | "bytes" => some .bytes
  | "basename" => some .basename
  | "dirname" => some .dirname
  | "ext_or" => some .extOr
  | "normalize" => some .normalize
  | "display" => some .display
  | "components" => some .components
  | "name" => some .name
  | "ext" => some .ext
  | "with_ext" => some .withExt
  | "parent" => some .parent
  | "starts_with" => some .startsWith
  | "ends_with" => some .endsWith
  | "strip_prefix" => some .stripPrefix
  | "relative_to" => some .relativeTo
  | _ => none

/-- The lexical `Path` methods (SPEC 4.4) on its native bytes: `bytes`; the
POSIX `basename` and `dirname`; `ext_or(fallback)`, the bytes after the last
dot that does not begin the final name, or the fallback; `normalize`, which
drops `.` and empty components and lets `..` cancel a name, keeping it at
the front of a relative path and dropping it above the root; and `display`.
The rest follow Rust's `std::path` components, which `StdComponents` models:
`components`; `name` and `ext`, empty when absent; `with_ext`; `parent`, `/`
or `.` when there is none; component-wise `starts_with` and `ends_with`;
`strip_prefix`, `Ok` of the remaining bytes or the `path-prefix` `Err` for an
empty prefix, a mismatch or a remainder that is not a `RelPath`; and
`relative_to`, the path itself on a mismatch. A text result decodes the
bytes lossily, as the binary does. -/
def pathMethod (errors : Errors Declaration Identity Source) (method : PathMethod) (path : Xsh.Values.NativePath)
    (arguments : List (Value Declaration Identity Source)) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  let pathValue := fun (bytes : List UInt8) =>
    match Xsh.Values.NativePath.fromBytes? bytes with
    | some made => Result.value (Value.scalar (.path made))
    | none => .unsupported
  let textOf := fun (bytes : List UInt8) =>
    (Result.value (Value.scalar (.string (Xsh.Values.LossyUtf8.text bytes))) : Result Declaration Identity Source _)
  match method, arguments with
  | .bytes, [] => .value (.scalar (.bytes path.bytes))
  | .basename, [] => textOf (Xsh.Paths.Fields.posixBasename 47 path.bytes)
  | .dirname, [] => pathValue (Xsh.Paths.Fields.posixDirname 47 path.bytes)
  | .extOr, [.scalar (.string fallback)] =>
    match Xsh.Paths.Fields.extension? 47 path.bytes with
    | some extension => textOf extension
    | none => .value (.scalar (.string fallback))
  | .normalize, [] => pathValue (Xsh.Paths.Normalize.normalizePath path.bytes)
  | .display, [] => textOf path.bytes
  | .components, [] =>
    match (Xsh.Paths.StdComponents.parts path.bytes).mapM fun spanned =>
        (Xsh.Values.NativePath.fromBytes? spanned.part.bytes).map fun made => (Value.scalar (.path made) : Value Declaration Identity Source) with
    | some items => .value (.list items)
    | none => .unsupported
  | .name, [] =>
    textOf (((Xsh.Paths.StdComponents.fileName? path.bytes).map fun spanned => spanned.part.bytes).getD [])
  | .ext, [] => textOf ((Xsh.Paths.StdComponents.extension? path.bytes).getD [])
  | .withExt, [.scalar (.string extension)] =>
    pathValue (Xsh.Paths.StdComponents.withExtension path.bytes extension.toUTF8.toList)
  | .parent, [] =>
    match Xsh.Paths.StdComponents.parent? path.bytes with
    | some [] => pathValue [46]
    | some bytes => pathValue bytes
    | none => pathValue (if path.bytes.head? = some 47 then [47] else [46])
  | .startsWith, [.scalar (.path other)] =>
    .value (.scalar (.boolean (Xsh.Paths.StdComponents.startsWith path.bytes other.bytes)))
  | .endsWith, [.scalar (.path other)] =>
    .value (.scalar (.boolean (Xsh.Paths.StdComponents.endsWith path.bytes other.bytes)))
  | .stripPrefix, [.scalar (.path other)] =>
    let refused := fun (message : String) => Result.value (Value.err (errors.described "path-prefix" message))
    if other.bytes.isEmpty then refused "prefix is empty"
    else
      match Xsh.Paths.StdComponents.stripPrefix? path.bytes other.bytes with
      | none => refused "path does not start with prefix"
      | some rest =>
        if (Xsh.Validation.RelPath.scanPath rest).isSome then
          match Xsh.Values.NativePath.fromBytes? rest with
          | some made => .value (.ok (.scalar (.path made)))
          | none => .unsupported
        else refused "remainder climbs above the prefix"
  | .relativeTo, [.scalar (.path other)] =>
    match Xsh.Paths.StdComponents.stripPrefix? path.bytes other.bytes with
    | some rest => pathValue rest
    | none => .value (.scalar (.path path))
  | _, _ => .unsupported

/-- A `Digest`'s methods (SPEC 4.4): `hex`, two lowercase hex digits a byte,
and `base64`, padded. -/
def digestMethod (name : String) (digest : Digest) (arguments : List (Value Declaration Identity Source)) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  match name, arguments with
  | "hex", [] => .value (.scalar (.string (String.ofList (digest.bytes.flatMap fun byte => paddedDigits 16 2 byte.toNat))))
  | "base64", [] => .value (.scalar (.string (String.ofList
      ((Xsh.Values.BaseCodec.encode .base64 digest.bytes).map fun byte => Char.ofNat byte.toNat))))
  | _, _ => .unsupported

/-- A list, map, set or text method call: the receiver evaluates first, then
each argument once in written order; a text method applies to a `Str`
receiver and a collection method to a list, map or set. -/
@[irreducible] def valueMethodCall {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) (frame : Frame world) (receiver : Xsh.Syntax.Expr)
    (name : String) (arguments : List Xsh.Syntax.CallArgument) : Valued world :=
  (evaluate frame receiver).bind fun target after =>
    (threadArguments context.spelling evaluate after arguments).bind fun supplied last =>
      match target.eraseNominal, TextMethod.ofName name, CollectionMethod.ofName name with
      | .scalar (.string text), some method, _ => fromEval last (textMethod context.errors method text supplied)
      | .scalar (.bytes bytes), _, _ =>
        match BytesMethod.ofName name, supplied.mapM Supplied.positionalValue with
        | some method, some values =>
          fromEval last (bytesMethod context.errors method bytes (values.map Value.eraseNominal))
        | _, _ => .exec .unsupported
      | .scalar (.path path), _, _ =>
        match PathMethod.ofName name, supplied.mapM Supplied.positionalValue with
        | some method, some values => fromEval last (pathMethod context.errors method path (values.map Value.eraseNominal))
        | _, _ => .exec .unsupported
      | .scalar (.digest digest), _, _ =>
        match supplied.mapM Supplied.positionalValue with
        | some values => fromEval last (digestMethod name digest (values.map Value.eraseNominal))
        | none => .exec .unsupported
      | _, _, some method =>
        match supplied.mapM Supplied.positionalValue with
        | some values => fromEval last (collectionMethod context.errors method target values)
        | none => .exec .unsupported
      | _, _, _ => .exec .unsupported

def Supplied.namedValue : Supplied Declaration Identity Source → Option (String × Value Declaration Identity Source)
  | .positional _ => none
  | .named label value => some (label, value)

/-- What a parameter receives: an argument, its default, or, for a final rest
parameter, the remaining positional arguments. -/
inductive Slot (Declaration Identity Source : Type) where
  | given (value : Value Declaration Identity Source)
  | defaulted (default : Xsh.Syntax.Expr)
  | rest (values : List (Value Declaration Identity Source))

/-- Place positional arguments on parameters in order and labelled ones by
name (SPEC 9.2): a parameter takes its positional argument, else its label,
else its default; a final rest parameter takes the remaining positional
arguments. A label given to a parameter already filled by position, a label
no parameter has, a required parameter left out, or a positional argument
with no parameter left is a fault. -/
def placeParameters (spelling : Xsh.Syntax.RawName → String) :
    List Xsh.Syntax.Parameter → List (Value Declaration Identity Source) →
      List (String × Value Declaration Identity Source) → Except ArgumentFault (List (Xsh.Syntax.RawName × Slot Declaration Identity Source))
  | [], [], [] => .ok []
  | [], [], (label, _) :: _ => .error (.unknownName label)
  | [], _ :: _, _ => .error .tooMany
  | [.mk name _ _ true], values, [] => .ok [(name, .rest values)]
  | [.mk _ _ _ true], _, (label, _) :: _ => .error (.unknownName label)
  | .mk name _ _ false :: rest, value :: values, named =>
    match named.lookup (spelling name) with
    | some _ => .error (.duplicate (spelling name))
    | none =>
      match placeParameters spelling rest values named with
      | .ok slots => .ok ((name, .given value) :: slots)
      | .error fault => .error fault
  | .mk name _ default false :: rest, [], named =>
    match named.lookup (spelling name), default with
    | some value, _ =>
      match placeParameters spelling rest [] (named.filter fun entry => entry.1 != spelling name) with
      | .ok slots => .ok ((name, .given value) :: slots)
      | .error fault => .error fault
    | none, some expression =>
      match placeParameters spelling rest [] named with
      | .ok slots => .ok ((name, .defaulted expression) :: slots)
      | .error fault => .error fault
    | none, none => .error (.missing (spelling name))
  | .mk _ _ _ true :: _ :: _, _, _ => .error .tooMany

/-- The first label given twice. -/
def repeatedLabel : List (String × Value Declaration Identity Source) → Option String
  | [] => none
  | (label, _) :: rest => if (rest.lookup label).isSome then some label else repeatedLabel rest

/-- Where a call's arguments go: a label given twice is a fault, and the rest
is `placeParameters`. -/
def placeCall (spelling : Xsh.Syntax.RawName → String) (parameters : List Xsh.Syntax.Parameter)
    (supplied : List (Supplied Declaration Identity Source)) :
    Except ArgumentFault (List (Xsh.Syntax.RawName × Slot Declaration Identity Source)) :=
  let named := supplied.filterMap Supplied.namedValue
  match repeatedLabel named with
  | some label => .error (.duplicate label)
  | none => placeParameters spelling parameters (supplied.filterMap Supplied.positionalValue) named

/-- The parameters a call can fill by position. -/
def fixedCount (parameters : List Xsh.Syntax.Parameter) : Nat :=
  (parameters.filter fun | .mk _ _ _ rest => !rest).length

/-- The parameters' values in parameter order: an argument as given, a default
evaluated by `evaluateDefault` in the frame the one before it left, after
every supplied argument (SPEC 9.2), and a rest parameter's `List`. -/
def bindSlots {world : World Declaration Identity Source}
    (evaluateDefault : Frame world → Xsh.Syntax.Expr → Valued world) :
    Frame world → List (Xsh.Syntax.RawName × Slot Declaration Identity Source) →
      Outcome world (Locals Declaration Identity Source)
  | frame, [] => .value [] frame
  | frame, (name, .given value) :: rest =>
    (bindSlots evaluateDefault frame rest).bind fun bound after => .value ((name, value) :: bound) after
  | frame, (name, .defaulted default) :: rest =>
    (evaluateDefault frame default).bind fun value after =>
      (bindSlots evaluateDefault after rest).bind fun bound last => .value ((name, value) :: bound) last
  | frame, (name, .rest values) :: rest =>
    (bindSlots evaluateDefault frame rest).bind fun bound after => .value ((name, .list values) :: bound) after

/-- The selectors of an assignment path, index expressions evaluated in path
order through `evaluate`, each in the frame the one before it left and each
given the static type of the container it selects in. -/
def selectorsOf {world : World Declaration Identity Source}
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) :
    Frame world → Option (ValueType Declaration) → List PathSpec → Outcome world (List (SelectorSide world))
  | frame, _, [] => .value [] frame
  | frame, type, .field label :: rest =>
    (selectorsOf evaluate frame (staticChild type (.field label)) rest).bind fun others after =>
      .value ((fun current => .ok (.field label, current)) :: others) after
  | frame, type, .index selector :: rest =>
    (evaluate frame selector).bind fun index after =>
      match selectorOf type index with
      | none => .exec .unsupported
      | some chosen =>
        (selectorsOf evaluate after (staticChild type (.index selector)) rest).bind fun others last =>
          .value ((fun current => .ok (chosen, current)) :: others) last

/-- Whether `retry ... on (pattern)` retries an error. -/
def retryable (context : Context Declaration Identity Source)
    (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
    Option Xsh.Syntax.Pattern → Value Declaration Identity Source → Option Bool
  | none, _ => some true
  | some pattern, error =>
    match matchPattern context test pattern error with
    | .matched _ => some true
    | .failed => some false
    | .unsupported => none

/-- One `retry` attempt's outcome: success is `Ok`, a selected raised failure
takes the next delay and goes on through `next`, and the last or an
unselected one is `Err`. -/
def retryStep {world : World Declaration Identity Source} (context : Context Declaration Identity Source)
    (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) (pattern : Option Xsh.Syntax.Pattern) (delays : List (Value Declaration Identity Source))
    (next : Frame world → List (Value Declaration Identity Source) → Valued world) : Valued world → Valued world
  | .value result after => .value (.ok result) after
  | .exec (.failed (.raised error) after) =>
    match retryable context test pattern error with
    | some true =>
      match delays with
      | delay :: rest => next { after with waited := after.waited ++ [delay] } rest
      | [] => .value (.err error) after
    | some false => .value (.err error) after
    | none => .exec .unsupported
  | other => other

/-- The names a local function captures: its body's free names other than
its parameters. -/
def captureNames (parameters : List Xsh.Syntax.RawName) (body : List Xsh.Syntax.Statement) : List Xsh.Syntax.RawName :=
  (Xsh.Syntax.freeStatementsFuel 256 parameters body).eraseDups

/-- The captured names that resolve to mutable cells of the frame. -/
def mutableCaptures {world : World Declaration Identity Source} (frame : Frame world)
    (names : List Xsh.Syntax.RawName) : List CellId :=
  names.filterMap fun name =>
    match frame.environment.cellOf name with
    | .ok cell =>
      match frame.store cell with
      | some held => if held.mutability = .mutable then some cell else none
      | none => none
    | .error _ => none

/-- A call returns to its caller with the caller's cells as the callee left
them, so a write to a captured mutable cell is kept; the callee's own cells
end with the call. A `Result` function returns a raised failure, whether from
`?`, an assertion or a primary cleanup failure, as its `Err` and wraps a plain
value in `Ok`; any other function fails the call with it. -/
def returnTo {world : World Declaration Identity Source} (caller : Frame world) (kind : ReturnKind) :
    Exec world → Valued world
  | .returned value callee =>
    .value (match kind with | .result => wrapOk value | .plain => value)
      { caller with store := keep caller.store callee.store, overlay := callee.overlay, streams := callee.streams }
  | .failed (.raised error) callee =>
    match kind with
    | .result => .value (.err error) { caller with store := keep caller.store callee.store, overlay := callee.overlay, streams := callee.streams }
    | .plain => .exec (.failed (.raised error) { caller with store := keep caller.store callee.store, overlay := callee.overlay, streams := callee.streams })
  | .failed failure callee => .exec (.failed failure { caller with store := keep caller.store callee.store, overlay := callee.overlay, streams := callee.streams })
  | .exited status callee =>
    .exec (.exited status { caller with store := keep caller.store callee.store, overlay := callee.overlay, streams := callee.streams })
  | .consumed exit callee =>
    .exec (.consumed exit { caller with store := keep caller.store callee.store, overlay := callee.overlay, streams := callee.streams })
  | .unsupported => .exec .unsupported
  | .outOfFuel => .exec .outOfFuel
  | _ => .exec .stuck

/-- An expression evaluated in another frame for a call from `caller`, back
in the caller as a plain function's result. -/
def returnValue {world : World Declaration Identity Source} (caller : Frame world) (result : Valued world) :
    Valued world :=
  returnTo caller .plain (result.toExec fun value inner => .returned value inner)

/-- A named function's call: an expression-bodied plain function's body is
evaluated through `evaluate` in its defining environment with its
parameters, in the caller's activation, without writing the caller's cells;
any other body runs as a block through `callBlock`. -/
def callFunctionWith {world : World Declaration Identity Source} (machine : Machine world)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
    (callBlock : Frame world → Scope Declaration Identity Source → Environment → Locals Declaration Identity Source →
      List CellId → List Xsh.Syntax.Statement → ReturnKind → Valued world)
    (defining : Environment) (parameters : Locals Declaration Identity Source) (body : FunctionBody)
    (caller : Frame world) : Valued world :=
  match body with
  | .expression result .plain =>
    returnValue caller
      (evaluate { caller with environment := defining, locals := parameters, callerStore := some caller.store, writable := [] }
        result)
  | .expression result .result =>
    callBlock caller ⟨caller.activation + machine.activationStep, [], none⟩ defining parameters [] [.expression result]
      .result
  | .block statements kind =>
    callBlock caller ⟨caller.activation + machine.activationStep, [], none⟩ defining parameters [] statements kind

/-- A function declaration's callable kind; streams and `cli main` are not
called as values. -/
def callableKind : Xsh.Syntax.FunctionKind → Option CallableKind
  | .proc => some .procedure
  | .pure => some .pure
  | .stream => some .producer
  | _ => none

/-- The callable value a local or a binding holds, which a call of that name
invokes with the callee's own result (SPEC 9.4). -/
def heldCallable {world : World Declaration Identity Source} (frame : Frame world) (name : Xsh.Syntax.RawName) :
    Option (Reference Identity .callable) :=
  match frame.locals.lookup name, frame.environment.cellOf name with
  | some value, _ =>
    match value.eraseNominal with
    | .callable reference _ => some reference
    | _ => none
  | none, .ok cell =>
    match frame.store cell with
    | some found =>
      match found.contents.value.eraseNominal with
      | .callable reference _ => some reference
      | _ => none
    | none => none
  | none, .error _ => none

/-- What `.call(...)` gives (SPEC 9.1): a `Proc` value's call is the proc's
own `Result` when it returns one and `Ok` of any other value, `Ok(())` for
none; a `Pure` value's call is its value. -/
def callResult (kind : CallableKind) (value : Value Declaration Identity Source) : Value Declaration Identity Source :=
  match kind with
  | .procedure => wrapOk value
  | _ => value

/-- The enum variant a name is, and the enum that declares it (SPEC 4.8). -/
def variantOf (spelling : Xsh.Syntax.RawName → String) (declarations : List Xsh.Syntax.Statement)
    (name : Xsh.Syntax.RawName) : Option (Xsh.Syntax.RawName × Xsh.Syntax.TagVariant) :=
  declarations.findSome? fun
    | .typeDef enum _ (.tagUnion variants) _ _ | .export (.typeDef enum _ (.tagUnion variants) _ _) =>
      (variants.find? fun | .mk variant _ _ => spelling variant = spelling name).map (enum, ·)
    | _ => none

/-- The payload-free variants a program's enums declare, each with its value
(SPEC 4.8: "payload-free variants are bare names"). -/
def bareVariants {world : World Declaration Identity Source} (machine : Machine world) :
    List (Xsh.Syntax.RawName × Value Declaration Identity Source) :=
  machine.declarations.flatMap fun
    | .typeDef enum _ (.tagUnion variants) _ _ | .export (.typeDef enum _ (.tagUnion variants) _ _) =>
      variants.filterMap fun
        | .mk variant [] _ => some (variant, .tagged (machine.identity enum) (machine.context.spelling variant) [])
        | .mk _ (_ :: _) _ => none
    | _ => []

/-- The top-level record schema a name declares (SPEC 4.7): its fields and
whether it is nominal. -/
def schemaOf (declarations : List Xsh.Syntax.Statement) (name : Xsh.Syntax.RawName) :
    Option (List Xsh.Syntax.SchemaField × Bool) :=
  declarations.findSome? fun
    | .typeDef declared _ (.recordSchema fields) nominal _ | .export (.typeDef declared _ (.recordSchema fields) nominal _) =>
      if declared = name then some (fields, nominal) else none
    | _ => none

/-- The top-level error family a name declares (SPEC 4.10): its variants. -/
def familyOf (declarations : List Xsh.Syntax.Statement) (name : Xsh.Syntax.RawName) :
    Option (List Xsh.Syntax.ErrorVariant) :=
  declarations.findSome? fun
    | .errorDef declared variants | .export (.errorDef declared variants) =>
      if declared = name then some variants else none
    | _ => none

/-- Where constructor arguments go (SPEC 4.7, 4.10): positional ones fill
fields in declaration order, then labelled ones, given by name, pun or a
record spread, go to the field with their label; `none` when there are too
many positional ones, a label is supplied twice, or a label names no
remaining field, which the checker rejects. -/
def placeArguments (spelling : Xsh.Syntax.RawName → String) (fields : List Xsh.Syntax.RawName)
    (supplied : List (Supplied Declaration Identity Source)) :
    Option (List (Option (Value Declaration Identity Source))) :=
  let positional := supplied.filterMap Supplied.positionalValue
  let named := supplied.filterMap Supplied.namedValue
  let remaining := (fields.drop positional.length).map spelling
  if positional.length ≤ fields.length ∧ (named.map Prod.fst).Nodup ∧
      (named.all fun entry => remaining.contains entry.1) = true then
    some (positional.map some ++ remaining.map fun label => named.lookup label)
  else none

/-- The fields' values in declaration order: a supplied one as given, a
missing one its default, evaluated through `evaluate` in the frame the one
before left; a missing field without a default is stuck, which the checker
rejects. -/
def fillFields {world : World Declaration Identity Source} (spelling : Xsh.Syntax.RawName → String)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) :
    Frame world → List (Xsh.Syntax.RawName × Option Xsh.Syntax.Expr) → List (Option (Value Declaration Identity Source)) →
      Outcome world (List (String × Value Declaration Identity Source))
  | frame, [], [] => .value [] frame
  | frame, (name, _) :: rest, some value :: values =>
    (fillFields spelling evaluate frame rest values).bind fun others after => .value ((spelling name, value) :: others) after
  | frame, (name, some default) :: rest, none :: values =>
    (evaluate frame default).bind fun value after =>
      (fillFields spelling evaluate after rest values).bind fun others last => .value ((spelling name, value) :: others) last
  | _, _, _ => .exec .stuck

def schemaField : Xsh.Syntax.SchemaField → Xsh.Syntax.RawName × Option Xsh.Syntax.Expr
  | .mk name _ default => (name, default)

/-- The error a constructor builds (SPEC 4.10): its family's declaration
identity, its variant and facets, its payload, and its message. -/
def declaredError {world : World Declaration Identity Source} (machine : Machine world) (family : Xsh.Syntax.RawName)
    (variant : Xsh.Syntax.RawName) (facets : List Xsh.Syntax.RawName) (message : String)
    (payload : List (String × Value Declaration Identity Source)) : Value Declaration Identity Source :=
  .error (ErrorData.mk (machine.identity family) (machine.context.spelling variant) (machine.context.spelling family)
    message payload (facets.map machine.context.spelling) none [] none none)

/-- `Family.Variant`, an error's message when nothing else gives one. -/
def qualifiedName {world : World Declaration Identity Source} (machine : Machine world)
    (family variant : Xsh.Syntax.RawName) : String :=
  machine.context.spelling family ++ "." ++ machine.context.spelling variant

/-- A payload variant's message: its `message: Str` field when it declares
one, `otherwise` when it does not. -/
def payloadMessage (payload : List (String × Value Declaration Identity Source)) (otherwise : String) : String :=
  match payload.lookup "message" with
  | some (.scalar (.string text)) => text
  | _ => otherwise

/-- The family and variant `Family.Variant` names, when `Family` is a declared
error family not hidden by a local. -/
def errorConstructor {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (receiver : Xsh.Syntax.Expr) (method : Xsh.Syntax.RawName) : Option (Xsh.Syntax.RawName × Xsh.Syntax.ErrorVariant) :=
  match receiver with
  | .ident family =>
    match frame.locals.lookup family, familyOf machine.declarations family with
    | none, some variants =>
      (variants.find? fun | .mk variant _ _ => machine.context.spelling variant = machine.context.spelling method).map
        (family, ·)
    | _, _ => none
  | _ => none

def bodyKind : FunctionBody → ReturnKind
  | .expression _ kind | .block _ kind => kind

def bodyStatements : FunctionBody → List Xsh.Syntax.Statement
  | .expression result _ => [.expression result]
  | .block statements _ => statements

/-- The local function a name resolves to. -/
def localClosure (closures : List (CellId × Closure)) (environment : Environment) (name : Xsh.Syntax.RawName) :
    Option Closure :=
  match environment.cellOf name with
  | .ok cell => closures.lookup cell
  | .error _ => none

def parameterNames (parameters : List Xsh.Syntax.Parameter) : List Xsh.Syntax.RawName :=
  parameters.map fun | .mk name _ _ _ => name

/-- Bind a call's arguments to a declaration's parameters (SPEC 9.2): each
supplied argument to its parameter in order, then each omitted one to its
default, evaluated by `evaluateDefault` in the declaration scope without the
other parameters, in parameter order; a final rest parameter takes the
remaining arguments as a `List`. -/
def bindArguments (evaluateDefault : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    List Xsh.Syntax.Parameter → List (Value Declaration Identity Source) →
      Result Declaration Identity Source (Locals Declaration Identity Source)
  | [], [] => .value []
  | [.mk name _ _ true], arguments => .value [(name, .list arguments)]
  | .mk name _ _ false :: rest, argument :: arguments =>
    (bindArguments evaluateDefault rest arguments).bind fun bound => .value ((name, argument) :: bound)
  | .mk name _ (some default) false :: rest, [] =>
    (evaluateDefault default).bind fun value =>
      (bindArguments evaluateDefault rest []).bind fun bound => .value ((name, value) :: bound)
  | _, _ => .stuck

/-- The map a map comprehension builds from its key and value projections:
a later projection of a key replaces an earlier one (SPEC 6.4, 6.5). -/
def mapOfPairs (pairs : List (Value Declaration Identity Source)) : Option (List (MapKey × Value Declaration Identity Source)) :=
  pairs.foldl (fun built pair =>
    built.bind fun entries =>
      match pair with
      | .list [key, value] => (mapKeyOf key).map fun mapKey => upsert mapKey value entries
      | _ => none) (some [])

/-- A declared return type's kind: a `Result` type or any other. -/
def returnKindOf {world : World Declaration Identity Source} (machine : Machine world) (result : Option Xsh.Syntax.TypeExpr) :
    ReturnKind :=
  match result.bind machine.typeOf with
  | some (.result _ _) => .result
  | _ => .plain

/-- The value of `collect { ... }` (SPEC 6.9): the list its `yield`s gathered,
when its block completes; any other exit leaves and no list is produced. -/
def collectWith {world : World Declaration Identity Source} (outer : Frame world) : Exec world → Valued world
  | .normal inner => .value (.list (inner.collected.getD [])) { inner with collected := outer.collected }
  | .broke inner => .exec (.broke { inner with collected := outer.collected })
  | .continued inner => .exec (.continued { inner with collected := outer.collected })
  | .returned value inner => .exec (.returned value { inner with collected := outer.collected })
  | .failed failure inner => .exec (.failed failure { inner with collected := outer.collected })
  | .internal fault inner => .exec (.internal fault { inner with collected := outer.collected })
  | other => .exec other

/-- After one iteration of a `loop` expression: `break` gives the loop its
value. -/
def loopStep {world : World Declaration Identity Source} (next : Frame world → Valued world) : Exec world → Valued world
  | .normal frame | .continued frame => next frame
  | .broke frame => .value (frame.breakValue.getD (.scalar .unit)) { frame with breakValue := none }
  | other => .exec other

/-- A call of `closure` with its arguments already evaluated (SPEC 6.3, 9.2):
they fill the parameters by position and label, omitted defaults evaluate in
the declaration scope, and the body runs through `callBlock`. -/
def invokeSupplied {world : World Declaration Identity Source} (machine : Machine world)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
    (callBlock : Frame world → Scope Declaration Identity Source → Environment → Locals Declaration Identity Source →
      List CellId → List Xsh.Syntax.Statement → ReturnKind → Valued world)
    (after : Frame world) (closure : Closure) (supplied : List (Supplied Declaration Identity Source)) : Valued world :=
  match closure with
  | .mk cell callable defining captured writable parameters body =>
      match placeCall machine.context.spelling (expectDefaults machine.context.spelling parameters) supplied with
      | .error fault =>
        .exec (.failed (.runtime (machine.argumentError fault (fixedCount parameters)
          (supplied.filterMap Supplied.positionalValue).length)) after)
      | .ok slots =>
        (bindSlots
            (fun current default => returnValue current (evaluate
              { Frame.ofScope defining current.store [] (some current.store)
                  ⟨current.activation + machine.activationStep,
                    (cell, .mk cell callable defining captured writable parameters body) :: captured ++ current.closures,
                    none⟩ with
                writable } default))
            after slots).bind fun bound last =>
          callBlock last ⟨last.activation + machine.activationStep,
            (cell, .mk cell callable defining captured writable parameters body) :: captured ++ last.closures, none⟩
            defining bound
            writable (bodyStatements body) (bodyKind body)

/-- A call of a local function: its arguments evaluated in written order and
placed on its parameters, a misfit raising the runtime argument error (SPEC
9.1: "their arguments are checked at runtime"), omitted defaults evaluated in
the declaration scope, and its body run through `callBlock` in a fresh
activation with its captured cells writable. The body sees the closures its
caller sees: every function is declared at the top level, so every body sees
every top-level function. -/
def invokeWith {world : World Declaration Identity Source} (machine : Machine world)
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
    (callBlock : Frame world → Scope Declaration Identity Source → Environment → Locals Declaration Identity Source →
      List CellId → List Xsh.Syntax.Statement → ReturnKind → Valued world)
    (frame : Frame world) (closure : Closure) (arguments : List Xsh.Syntax.CallArgument) : Valued world :=
  (threadArguments machine.context.spelling evaluate frame
      (expectArguments machine.context.spelling closure.parameters closure.parameters arguments)).bind fun supplied after =>
    invokeSupplied machine evaluate callBlock after closure supplied

mutual
  /-- Expressions, with the frame threaded through them: operands, arguments,
  indices and branches evaluate left to right, each in the frame the one
  before it left; a value block nested in an expression runs as a block in
  the current frame; and every call that runs a body returns through
  `returnTo`, so a nested call's writes to captured cells reach the caller. -/
  def evalExpr {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.Expr → Valued world
    | 0, _, _ => .exec .outOfFuel
    | fuel + 1, frame, expression =>
      match expression with
      | .tryExpr inner =>
        (evalExpr machine fuel frame inner).bind fun value next => fromEval next (propagate value)
      | .convert inner target =>
        (evalExpr machine fuel frame inner).bind fun value next =>
          match machine.conversion value target with
          | some converted => fromEval next (propagate converted)
          | none => .exec .unsupported
      | .dotVariant none _ _ => .exec .unsupported
      | .dotVariant (some enum) name arguments =>
        match variantOf machine.context.spelling machine.declarations name, arguments with
        | some (declared, .mk variant [] _), none =>
          if machine.context.spelling declared = machine.context.spelling enum then
            .value (.tagged (machine.identity declared) (machine.context.spelling variant) []) frame
          else .exec .stuck
        | some (declared, .mk variant fields _), some arguments =>
          if machine.context.spelling declared = machine.context.spelling enum then
            (threadArguments machine.context.spelling (evalExpr machine fuel) frame arguments).bind fun supplied after =>
              match supplied.mapM Supplied.positionalValue with
              | some values =>
                if values.length = fields.length then
                  .value (.tagged (machine.identity declared) (machine.context.spelling variant) values) after
                else .exec .stuck
              | none => .exec .stuck
          else .exec .stuck
        | _, _ => .exec .stuck
      | .item =>
        match frame.item with
        | some value => .value value frame
        | none => .exec .unsupported
      | .valuePipelineHole =>
        match frame.piped with
        | some value => .value value frame
        | none => .exec .unsupported
      | .record fields =>
        if namedOnly fields then
          threadStep machine.context (evalExpr machine fuel) (callFunctionWith machine (evalExpr machine fuel) (callBlock machine fuel))
            frame expression
        else
          (threadEntries machine.context (evalExpr machine fuel) frame fields).bind fun entries after =>
            match braceValue entries with
            | some value => .value value after
            | none => .exec .unsupported
      | .require value (some schema) =>
        (evalExpr machine fuel frame value).bind fun candidate next =>
          match machine.typeOf schema with
          | some type => .value (requireValue machine type candidate) next
          | none => .exec .unsupported
      | .envString key =>
        let name := machine.context.spelling key
        .value (environmentString machine frame.overlay name) frame
      | .nullSafeField base name =>
        (evalExpr machine fuel frame base).bind fun receiver next =>
          (fromEval next (hopOf true receiver)).bind fun hop after =>
            match hop with
            | none => .value (.scalar .null) after
            | some present => fromEval after (fieldOf machine.context name present)
      | .index base key true =>
        (evalExpr machine fuel frame base).bind fun receiver next =>
          (fromEval next (hopOf true receiver)).bind fun hop after =>
            match hop with
            | none => .value (.scalar .null) after
            | some present =>
              (evalExpr machine fuel after key).bind fun selector last =>
                fromEval last (indexAt machine.context (negativeLiteral key) present selector)
      | .slice base start finish guarded =>
        (evalExpr machine fuel frame base).bind fun receiver next =>
          (fromEval next (hopOf guarded receiver)).bind fun hop after =>
            match hop with
            | none => .value (.scalar .null) after
            | some present =>
              (match start with
                | none => Outcome.value (α := Option (Value Declaration Identity Source)) none after
                | some bound => (evalExpr machine fuel after bound).bind fun low middle => Outcome.value (some low) middle).bind
                fun low middle =>
                  (match finish with
                    | none => Outcome.value (α := Option (Value Declaration Identity Source)) none middle
                    | some bound => (evalExpr machine fuel middle bound).bind fun high last => Outcome.value (some high) last).bind
                    fun high last => fromEval last (sliceOf present low high)
      | .valueBlock _ | .capture _ | .retry _ _ _ _ | .binary .resultFallback _ _ | .loopExpr _ | .collect _
          | .listComp _ _ | .mapComp _ _ _ | .matchExpr _ _ | .errorContext _ _ | .contextScope .env _ _ =>
        runValue machine fuel frame expression
      | .ident name =>
        match frame.locals.lookup name, localClosure frame.closures frame.environment name,
            frame.environment.cellOf name with
        | none, some (.mk cell callable _ _ _ _ _), _ =>
          .value (.callable (machine.callableReference cell) callable) frame
        | none, none, .ok cell =>
          match frame.store cell with
          | none => .exec (.failed (.runtime (machine.uninitialized name)) frame)
          | some _ =>
            threadStep machine.context (evalExpr machine fuel)
              (callFunctionWith machine (evalExpr machine fuel) (callBlock machine fuel)) frame expression
        | _, _, _ =>
          threadStep machine.context (evalExpr machine fuel) (callFunctionWith machine (evalExpr machine fuel) (callBlock machine fuel)) frame
            expression
      | .structuredPipeline input stages =>
        match valueStage stages with
        | some (before, call, after) =>
          (evalExpr machine fuel frame (if before.isEmpty then input else .structuredPipeline input before)).bind
            fun piped next =>
              (evalExpr machine fuel { next with piped := some piped } (valueStageCall call)).bind fun called last =>
                if after.isEmpty then .value called last
                else evalExpr machine fuel { last with piped := some called } (.structuredPipeline .valuePipelineHole after)
        | none =>
        (evalExpr machine fuel frame input).bind fun result next =>
          match pipelineSource result with
          | .error error => .exec (.failed (.raised error) next)
          | .ok source =>
          (prepareStages (evalExpr machine fuel) next (pipelineSplit stages).1).bind fun prepared ready =>
          (pipelineLimit (evalExpr machine fuel) ready (pipelineSplit stages).2.1).bind fun explicit after =>
            let limit := bothLimits explicit (endingLimit (pipelineSplit stages).2.2)
            match pendingStream machine after source, limit with
            | some (index, some _), some 0 => closePipeline machine.context (evalExpr machine fuel) (fun scope captures body => handlerScope machine fuel scope none captures body)
                (pipelineSplit stages).2.2 (.list []) (after.consume index)
            | some (index, some (closure, supplied)), _ =>
              if (pipelineSplit stages).1.any isStateful then .exec .unsupported else
              (gathered after index (invokeSupplied machine (evalExpr machine fuel) (callBlock machine fuel)
                  { after with
                    streams := after.streams.set index (.gathering [])
                    consumers := { sinkConsumer after index with stages := (pipelineSplit stages).1, limit } ::
                      after.consumers }
                  closure supplied)).bind fun out last => closePipeline machine.context (evalExpr machine fuel) (fun scope captures body => handlerScope machine fuel scope none captures body)
                (pipelineSplit stages).2.2 out last
            | some (_, none), _ => closePipeline machine.context (evalExpr machine fuel) (fun scope captures body => handlerScope machine fuel scope none captures body)
                (pipelineSplit stages).2.2 (.list []) after
            | none, _ =>
              (runPipeline machine.context
                  (fun current body bound items => runSegments machine.context (evalExpr machine fuel) (handlerScope machine fuel)
                    (stageItems machine fuel) (body.length + 1) current body bound items)
                  after source prepared .collected limit).bind
                fun out last => closePipeline machine.context (evalExpr machine fuel) (fun scope captures body => handlerScope machine fuel scope none captures body)
                  (pipelineSplit stages).2.2 out last
      | .call (.field receiver method) arguments =>
        match errorConstructor machine frame receiver method, arguments with
        | some (family, .mk variant [] facets), [] =>
          .value (declaredError machine family variant facets (qualifiedName machine family variant)
            [("message", .scalar (.string (qualifiedName machine family variant)))]) frame
        | some (family, .mk variant [] facets), [.positional message] =>
          (evalExpr machine fuel frame message).bind fun written after =>
            match written.eraseNominal with
            | .scalar (.string text) =>
              .value (declaredError machine family variant facets text [("message", .scalar (.string text))]) after
            | _ => .exec .stuck
        | some (family, .mk variant fields facets), _ =>
          (threadArguments machine.context.spelling (evalExpr machine fuel) frame arguments).bind fun supplied after =>
            match placeArguments machine.context.spelling (fields.map Prod.fst) supplied with
            | none => .exec .stuck
            | some placed =>
              (fillFields machine.context.spelling (evalExpr machine fuel) after
                  (fields.map fun field => (field.1, none)) placed).bind fun payload last =>
                .value (declaredError machine family variant facets
                  (payloadMessage payload (qualifiedName machine family variant)) payload) last
        | _, _ =>
          match machine.context.spelling method with
          | "call" =>
            (evalExpr machine fuel frame receiver).bind fun callee after =>
              match callee.eraseNominal with
              | .callable reference _ =>
                match (machine.callableCell reference).bind fun cell => after.closures.lookup cell with
                | some closure =>
                  (invokeWith machine (evalExpr machine fuel) (callBlock machine fuel) after closure arguments).bind
                    fun result final =>
                      match closure with
                      | .mk _ callable _ _ _ _ _ => .value (callResult callable result) final
                | none => .exec .stuck
              | _ => .exec .unsupported
          | "collect" =>
            match arguments with
            | [] =>
              (evalExpr machine fuel frame receiver).bind fun subject after =>
                match pendingStream machine after subject with
                | some (index, some (closure, supplied)) =>
                  gathered after index (invokeSupplied machine (evalExpr machine fuel) (callBlock machine fuel)
                    { after with streams := after.streams.set index (.gathering []),
                                 consumers := sinkConsumer after index :: after.consumers } closure supplied)
                | some (_, none) => .value (.list []) after
                | none => .exec .unsupported
            | _ => .exec .unsupported
          | name =>
            if isNumericMethod name then numericCall machine.context (evalExpr machine fuel) frame receiver name arguments
            else
              match CollectionMethod.ofName name, TextMethod.ofName name, BytesMethod.ofName name with
              | none, none, none =>
                threadStep machine.context (evalExpr machine fuel)
                  (callFunctionWith machine (evalExpr machine fuel) (callBlock machine fuel)) frame expression
              | _, _, _ => valueMethodCall machine.context (evalExpr machine fuel) frame receiver name arguments
      | .call (.ident name) arguments =>
        match frame.locals.lookup name, localClosure frame.closures frame.environment name with
        | none, some closure@(.mk _ callable _ _ _ _ _) =>
          if callable = .producer then
            (threadArguments machine.context.spelling (evalExpr machine fuel) frame arguments).bind fun supplied after =>
              .value (.stream (machine.streamReference after.streams.length))
                { after with streams := after.streams ++ [.pending closure supplied] }
          else invokeWith machine (evalExpr machine fuel) (callBlock machine fuel) frame closure arguments
        | _, _ =>
          match frame.locals.lookup name, machine.context.functions name, machine.context.spelling name, arguments with
          | none, none, "Ok", [] => .value (.ok (.scalar .unit)) frame
          | none, none, "Ok", [.positional payload] =>
            (evalExpr machine fuel frame payload).bind fun value next => .value (.ok value) next
          | none, none, "Err", [.positional payload] =>
            (evalExpr machine fuel frame payload).bind fun value next => .value (.err value) next
          | _, _, _, _ =>
            match frame.locals.lookup name, machine.context.functions name, schemaOf machine.declarations name,
                variantOf machine.context.spelling machine.declarations name with
            | none, none, some (fields, nominal), _ =>
              (threadArguments machine.context.spelling (evalExpr machine fuel) frame arguments).bind fun supplied after =>
                match placeArguments machine.context.spelling (fields.map fun field => (schemaField field).1) supplied with
                | none => .exec .stuck
                | some placed =>
                  (fillFields machine.context.spelling (evalExpr machine fuel) after (fields.map schemaField) placed).bind
                    fun entries last =>
                      .value (if nominal then .nominal (machine.identity name) (.record entries) else .record entries) last
            | none, none, none, some (enum, .mk variant fields _) =>
              (threadArguments machine.context.spelling (evalExpr machine fuel) frame arguments).bind fun supplied after =>
                match supplied.mapM Supplied.positionalValue with
                | some values =>
                  if values.length = fields.length then
                    .value (.tagged (machine.identity enum) (machine.context.spelling variant) values) after
                  else .exec .stuck
                | none => .exec .stuck
            | _, _, _, _ =>
              match machine.context.functions name, heldCallable frame name with
              | none, some reference =>
                match (machine.callableCell reference).bind fun cell => frame.closures.lookup cell with
                | some closure => invokeWith machine (evalExpr machine fuel) (callBlock machine fuel) frame closure arguments
                | none => .exec .stuck
              | _, _ =>
                threadStep machine.context (evalExpr machine fuel)
                  (callFunctionWith machine (evalExpr machine fuel) (callBlock machine fuel)) frame expression
      | _ => threadStep machine.context (evalExpr machine fuel) (callFunctionWith machine (evalExpr machine fuel) (callBlock machine fuel)) frame
              expression

  /-- A call runs its body in its own activation, in its defining environment
  with its parameters as locals and `writable` as the caller's cells it may
  write, and returns to the caller through `returnTo`: the caller's cells as
  the body left them, the body's own cells ending with the call. -/
  def callBlock {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Scope Declaration Identity Source → Environment → Locals Declaration Identity Source →
        List CellId → List Xsh.Syntax.Statement → ReturnKind → Valued world
    | 0, _, _, _, _, _, _, _ => .exec .outOfFuel
    | fuel + 1, caller, scope, defining, parameters, writable, body, kind =>
      returnTo caller kind
        (runBody machine fuel
          { Frame.ofScope defining caller.store parameters (some caller.store) scope with
              writable, overlay := caller.overlay, consumers := caller.consumers, streams := caller.streams }
          body)

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
        (threadAll (evalExpr machine fuel) frame delays).bind fun scheduled next =>
          retryLoop machine fuel next pattern body scheduled
      | .tryExpr inner =>
        (runValue machine fuel frame inner).bind fun value next => fromEval next (propagate value)
      | .convert inner target =>
        (runValue machine fuel frame inner).bind fun value next =>
          match machine.conversion value target with
          | some converted => fromEval next (propagate converted)
          | none => .exec .unsupported
      | .retry .backoff durations pattern body =>
        (threadAll (evalExpr machine fuel) frame durations).bind fun written next =>
          match backoffDelays machine written with
          | some scheduled => retryLoop machine fuel next pattern body scheduled
          | none => .exec .unsupported
      | .binary .resultFallback left right =>
        (runValue machine fuel frame left).bind fun value next =>
          match value.eraseNominal, right with
          | .ok payload, _ => .value payload next
          | .err error, .handlerBlock parameter body =>
            handlerScope machine fuel next (if parameter.isNone then some error else next.item)
              (parameter.toList.map fun bound => (bound, error)) body
          | .scalar .null, .handlerBlock none body => handlerScope machine fuel next next.item [] body
          | .scalar .null, .handlerBlock (some _) _ => .exec .unsupported
          | .err _, fallback | .scalar .null, fallback => runValue machine fuel next fallback
          | _, _ => .value value next
      | .loopExpr body => loopValue machine fuel frame body
      | .contextScope .env input body =>
        (evalExpr machine fuel frame input).bind fun overrides next =>
          match scopeOverrides machine overrides with
          | none => .exec .unsupported
          | some entries =>
            match applyOverrides machine next.overlay entries with
            | .error (.runtime error) => .value (.err error) next
            | .error failure => .exec (.failed failure next)
            | .ok applied =>
              (withOverlay next.overlay (valueScope machine fuel { next with overlay := applied } body)).bind
                fun tail last => .value (.ok tail) last
      | .errorContext message body =>
        (evalExpr machine fuel frame message).bind fun described next =>
          match described.eraseNominal with
          | .scalar (.string text) => annotate text (valueScope machine fuel next body)
          | _ => .exec .unsupported
      | .matchExpr subject arms =>
        (evalExpr machine fuel frame subject).bind fun value next => matchValue machine fuel next value arms
      | .listComp value qualifiers => comprehend machine fuel frame qualifiers value
      | .mapComp key value qualifiers =>
        (comprehend machine fuel frame qualifiers (.list [key, value])).bind fun pairs next =>
          match listValues pairs with
          | some items =>
            match mapOfPairs items with
            | some entries => .value (.map entries) next
            | none => .exec .unsupported
          | none => .exec .stuck
      | .collect body =>
        collectWith frame (runBlock machine fuel { frame with collected := some [] } body)
      | expression => evalExpr machine fuel frame expression

  /-- A value block (SPEC 6.9): its tail is computed before its defers run, and
  an `Err` tail is its value, so it leaves without an error. -/
  def valueScope {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Valued world
    | 0, _, _ => .exec .outOfFuel
    | fuel + 1, frame, body =>
      settle (leaveBlock machine fuel frame) (runValueTail machine fuel frame.enter body)

  /-- A `??` handler block: the error bound to its parameter in its own scope,
  with its tail as the value. -/
  def handlerScope {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Option (Value Declaration Identity Source) →
        List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world
    | 0, _, _, _, _ => .exec .outOfFuel
    | fuel + 1, frame, item, captures, body =>
      settle (leaveBlock machine fuel frame)
        ((bindCaptures machine { frame.enter with item := item } captures).thenValued fun bound =>
          runValueTail machine fuel bound body)

  /-- A list comprehension's qualifiers (SPEC 6.5): each `for` target is bound
  per item in its own scope, visible in later qualifiers and the value; a
  `when` filters. -/
  def comprehend {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Qualifier → Xsh.Syntax.Expr → Valued world
    | 0, _, _, _ => .exec .outOfFuel
    | fuel + 1, frame, qualifiers, value =>
      match qualifiers with
      | [] => (runValue machine fuel frame value).bind fun result next => .value (.list [result]) next
      | .when condition :: rest =>
        (evalExpr machine fuel frame condition).bind
          fun tested next =>
            match tested.eraseNominal with
            | .scalar (.boolean true) => comprehend machine fuel next rest value
            | .scalar (.boolean false) => .value (.list []) next
            | _ => .exec .unsupported
      | .each target iterable :: rest =>
        (evalExpr machine fuel frame iterable).bind
          fun source next =>
            match pendingStream machine next source with
            | some (index, some (closure, supplied)) =>
              gathered next index (invokeSupplied machine (evalExpr machine fuel) (callBlock machine fuel)
                { next with
                  streams := next.streams.set index (.gathering [])
                  consumers := { consumerOf next target [.yieldDelegate (.listComp value rest)] with gathers := some index } ::
                    next.consumers } closure supplied)
            | some (_, none) => .value (.list []) next
            | none =>
              match iterationItems source with
              | some items => comprehendItems machine fuel next target items rest value
              | none => .exec .unsupported

  def comprehendItems {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.BindingTarget → List (Value Declaration Identity Source) →
        List Xsh.Syntax.Qualifier → Xsh.Syntax.Expr → Valued world
    | 0, _, _, _, _, _ => .exec .outOfFuel
    | fuel + 1, frame, target, items, rest, value =>
      match items with
      | [] => .value (.list []) frame
      | item :: others =>
        (settle (leaveBlock machine fuel frame)
          ((bindTarget machine frame.enter .immutable target item).thenValued fun bound =>
            comprehend machine fuel bound rest value)).bind fun first next =>
          (comprehendItems machine fuel next target others rest value).bind fun later last =>
            match first, later with
            | .list head, .list tail => .value (.list (head ++ tail)) last
            | _, _ => .exec .stuck

  /-- One item through a structured pipeline's stages (SPEC 13.1, 13.3): a
  stage block binds the item to its parameter or, without one, to `.`, in its
  own scope, and its tail is its value; `where` keeps the item when the tail
  is `true`, `map` passes the tail on, and `flat-map` passes each item of the
  tail's list on in turn. The value is the list of items the item becomes. -/
  def stageItem {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.PipeStage → Value Declaration Identity Source → Valued world
    | 0, _, _, _ => .exec .outOfFuel
    | fuel + 1, frame, stages, item =>
      match stages with
      | [] => .value (.list [item]) { frame with restaged := some [] }
      | stage :: rest =>
        match stageAction stage with
        | some (action, parameter, body) =>
          restageWith stage rest ((handlerScope machine fuel frame (parameter.elim (some item) fun _ => none)
              (parameter.toList.map fun name => (name, item)) body).bind fun result after =>
            match action with
            | .keep =>
              match result.eraseNominal with
              | .scalar (.boolean true) => stageItem machine fuel after rest item
              | .scalar (.boolean false) => .value (.list []) { after with restaged := some rest }
              | _ => .exec .stuck
            | .reject =>
              match result.eraseNominal with
              | .scalar (.boolean false) => stageItem machine fuel after rest item
              | .scalar (.boolean true) => .value (.list []) { after with restaged := some rest }
              | _ => .exec .stuck
            | .replace => stageItem machine fuel after rest result
            | .expand =>
              match listValues result with
              | some items => stageItems machine fuel after rest none items
              | none => .exec .stuck)
        | none =>
          match statefulStep stage with
          | some .dropOne => .value (.list []) { frame with restaged := some rest }
          | some (.enumerateAt counted index) =>
            restageWith (.stream .enumerate (.positional .item :: counted) none) rest
              (stageItem machine fuel frame rest (.record [("index", index), ("value", item)]))
          | none => .exec .unsupported

  /-- Items through the stages one at a time, each reaching the end of the
  stages before the next starts, as live and list sources alike are pulled
  (SPEC 13.2); under a `take` limit no item is pulled once the limit is
  reached. -/
  def stageItems {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.PipeStage → Option Nat → List (Value Declaration Identity Source) → Valued world
    | 0, _, _, _, _ => .exec .outOfFuel
    | fuel + 1, frame, stages, limit, items =>
      match items, limit with
      | _, some 0 | [], _ => .value (.list []) { frame with restaged := some stages }
      | item :: rest, _ =>
        (stageItem machine fuel frame stages item).bind fun first after =>
          match first with
          | .list head =>
            match afterItems limit head.length with
            | none => .value (.list (limitTake limit head)) after
            | some remaining =>
              (stageItems machine fuel after (after.restaged.getD stages) remaining rest).bind fun later last =>
                match later with
                | .list tail => .value (.list (head ++ tail)) last
                | _ => .exec .stuck
          | _ => .exec .stuck

  /-- `loop { ... }` as an expression: its value is what `break` gives it. -/
  def loopValue {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → List Xsh.Syntax.Statement → Valued world
    | 0, _, _ => .exec .outOfFuel
    | fuel + 1, frame, body => loopStep (fun next => loopValue machine fuel next body) (runBlock machine fuel frame body)

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
      retryStep machine.context machine.typeTest pattern delays (fun after rest => retryLoop machine fuel after pattern body rest)
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
      | .binding kind target annotation initializer =>
        (runValue machine fuel frame (expectTarget machine.context.spelling annotation initializer)).toExec fun value next =>
          bindTarget machine next (bindingMutability kind) target value
      | .assign (.env key) .set value =>
        (runValue machine fuel frame value).toExec fun result next =>
          match machine.resolution.argv result with
          | none => .internal .unconvertibleEnvironment next
          | some bytes =>
            match setEnvironment machine.raises next.overlay (machine.context.spelling key).toUTF8.toList bytes with
            | .ok overlay => .normal { next with overlay := overlay }
            | .error failure => .failed failure next
      | .assign target op value =>
        match flatten machine.context.spelling target with
        | none => .unsupported
        | some (root, path) =>
          match frame.environment.cellOf root with
          | .error fault => .internal (.unresolvedName fault) frame
          | .ok cell =>
            match frame.callerStore.bind (fun caller => caller cell), frame.writable.contains cell, frame.store cell with
            | some _, false, _ => .unsupported
            | _, _, none => .internal (.missingCell cell) frame
            | _, _, some current =>
              (selectorsOf (evalExpr machine fuel) frame (some current.contents.typeTag) path).toExec
                fun sides selected =>
                (runValue machine fuel selected value).toExec fun result next =>
                  match evaluateSelectors machine.raises (some current.contents.typeTag) sides next.store with
                  | .error (failure, _) => .failed failure next
                  | .ok (selectors, _) =>
                    fromStep next next.environment next.locals
                      (assignPath world machine.decisions machine.raises machine.operate next.store cell selectors op result)
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
      | .forStmt target (.call (.ident producer) arguments) body =>
        match frame.locals.lookup producer, localClosure frame.closures frame.environment producer with
        | none, some closure@(.mk _ .producer _ _ _ _ _) =>
          consumeProducer frame
            (invokeWith machine (evalExpr machine fuel) (callBlock machine fuel)
              (frame.consuming frame.streams.length target body) closure arguments)
        | _, _ =>
          (evalExpr machine fuel frame (.call (.ident producer) arguments)).toExec fun source next =>
            match iterationItems source with
            | some items => runFor machine fuel next target items body
            | none => .unsupported
      | .forStmt target iterable body =>
        (evalExpr machine fuel frame iterable).toExec fun source next =>
          match pendingStream machine next source with
          | some (index, some (closure, supplied)) =>
            consumeProducer (next.consume index)
              (invokeSupplied machine (evalExpr machine fuel) (callBlock machine fuel)
                ((next.consume index).consuming index target body) closure supplied)
          | some (_, none) => .normal next
          | none =>
            match iterationItems source with
            | some items => runFor machine fuel next target items body
            | none => .unsupported
      | .loopStmt body => runLoop machine fuel frame body
      | .breakStmt none => .broke frame
      | .breakStmt (some value) =>
        (runValue machine fuel frame value).toExec fun result next => .broke { next with breakValue := some result }
      | .guardHandled target _ initializer failure otherwise =>
        (runValue machine fuel frame initializer).toExec fun subject next =>
          match subject.eraseNominal with
          | .ok payload => bindTarget machine next .immutable target payload
          | .err error => guardElse (runBound machine fuel next [(failure, error)] otherwise)
          | _ => .unsupported
      | .yield value =>
        (runValue machine fuel frame value).toExec fun result next =>
          match next.collected, next.consumers with
          | some gathered, _ => .normal { next with collected := some (gathered ++ [result]) }
          | none, loop :: outer =>
            match loop.sink, loop.stages with
            | some index, [] => next.gather index result
            | some index, stages =>
              resumeProducer next
                ((stageItem machine fuel (consumerFrame next loop outer) stages result).toExec fun out after =>
                  after.gatherAll index loop.limit out)
            | none, _ =>
              let consuming := consumerFrame next loop outer
              resumeProducer next
                (leaveBlock machine fuel consuming
                  ((bindTarget machine consuming.enter .immutable loop.target result).andThen fun bound =>
                    runStatements machine fuel bound loop.body))
          | none, [] => .unsupported
      | .yieldDelegate value =>
        (runValue machine fuel frame value).toExec fun result next =>
          match next.collected, listValues result, next.consumers with
          | some gathered, some items, _ => .normal { next with collected := some (gathered ++ items) }
          | none, none, _ :: _ =>
            match pendingStream machine next result with
            | some (index, some (closure, supplied)) =>
              delegatedStream (invokeSupplied machine (evalExpr machine fuel) (callBlock machine fuel) (next.consume index)
                closure supplied)
            | some (_, none) => .normal next
            | none => .unsupported
          | none, some items, loop :: outer =>
            delegateItems (fun current item =>
              match loop.sink, loop.stages with
              | some index, [] => current.gather index item
              | some index, stages =>
                resumeProducer current
                  ((stageItem machine fuel (consumerFrame current loop outer) stages item).toExec fun out after =>
                    after.gatherAll index loop.limit out)
              | none, _ =>
                resumeProducer current
                  (leaveBlock machine fuel (consumerFrame current loop outer)
                    ((bindTarget machine (consumerFrame current loop outer).enter .immutable loop.target item).andThen
                      fun bound => runStatements machine fuel bound loop.body))) next items
          | _, _, _ => .unsupported
      | .functionDef kind name parameters _ result body _ _ =>
        match frame.callerStore, frame.environment.names with
        | none, [_] =>
          match callableKind kind, (machine.namesAt frame.activation).declaration frame.environment name with
          | none, _ => .unsupported
          | _, none => .internal .malformedTarget frame
          | some callable, some (binder, cell, _) =>
            let environment := extend frame.environment [(name, binder, cell)]
            .normal { frame with
              environment
              locals := hide [name] frame.locals
              closures := (cell, .mk cell callable environment frame.closures
                (mutableCaptures frame (captureNames (parameterNames parameters) body)) parameters
                (.block (expectBody machine.context.spelling result body) (returnKindOf machine result))) :: frame.closures }
        | _, _ => .stuck
      | .guardStmt target _ initializer otherwise =>
        (runValue machine fuel frame initializer).toExec fun subject next =>
          match guardValue (machine.resultSubject next.environment initializer) subject with
          | some bound => bindTarget machine next .immutable target bound
          | none => guardElse (runBlock machine fuel next otherwise)
      | .continueStmt => .continued frame
      | .exit status =>
        (evalExpr machine fuel frame status).toExec fun value next =>
          match value.eraseNominal with
          | .scalar (.integer code) =>
            if 0 ≤ code.val ∧ code.val ≤ 255 then .exited code.val.toNat next
            else .failed (.runtime machine.exitStatus) next
          | _ => .unsupported
      | .matchStmt subject arms =>
        (evalExpr machine fuel frame subject).toExec fun value next =>
          runMatch machine fuel next value arms
      | .assert condition message =>
        (evalExpr machine fuel frame condition).toExec fun tested next =>
          match tested.eraseNominal with
          | .scalar (.boolean true) => .normal next
          | .scalar (.boolean false) =>
            match message with
            | none => .failed (.raised (machine.assertion none)) next
            | some text =>
              (evalExpr machine fuel next text).toExec fun written last =>
                .failed (.raised (machine.assertion (some written))) last
          | _ => .unsupported
      | .defer kind action =>
        .normal { frame with deferred := ⟨kind, action, frame.environment, frame.locals⟩ :: frame.deferred }
      | .sugar _ expansion => runStatement machine fuel frame expansion
      | .export declaration => runStatement machine fuel frame declaration
      | .typeDef _ _ _ _ _ | .errorDef _ _ =>
        match frame.callerStore, frame.environment.names with
        | none, [_] => .normal frame
        | _, _ => .stuck
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
        (evalExpr machine fuel frame subject).toExec fun value next =>
          match conditionMatch machine.context machine.typeTest pattern value with
          | .matched captures => runBound machine fuel next captures body
          | .failed => runIf machine fuel next rest otherwise
          | .unsupported => .unsupported
      | (condition, body) :: rest, otherwise =>
        (controlValue (evalExpr machine fuel) frame condition).toExec fun tested next =>
          match conditionValue tested with
          | some (.ok true) => runBlock machine fuel next body
          | some (.ok false) => runIf machine fuel next rest otherwise
          | some (.error error) => .failed (.raised error) next
          | none => .unsupported

  /-- `while` and `while let`: each iteration's body is its own scope; `break`
  ends this loop and `continue` starts its next test. -/
  def runWhile {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.Expr → List Xsh.Syntax.Statement → Exec world
    | 0, _, _, _ => .outOfFuel
    | fuel + 1, frame, condition, body =>
      match condition with
      | .patternCondition subject pattern =>
        (evalExpr machine fuel frame subject).toExec fun value tested =>
          match conditionMatch machine.context machine.typeTest pattern value with
          | .matched captures =>
            (runBound machine fuel tested captures body).iterate fun next =>
              runWhile machine fuel next (.patternCondition subject pattern) body
          | .failed => .normal tested
          | .unsupported => .unsupported
      | condition =>
        (controlValue (evalExpr machine fuel) frame condition).toExec fun tested after =>
          match conditionValue tested with
          | some (.ok true) =>
            (runBlock machine fuel after body).iterate fun next => runWhile machine fuel next condition body
          | some (.ok false) => .normal after
          | some (.error error) => .failed (.raised error) after
          | none => .unsupported

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
      | [] => .failed (.runtime machine.context.errors.matchNoArm) frame
      | (pattern, guard, body) :: rest =>
        match matchPattern machine.context machine.typeTest pattern value with
        | .failed => runMatch machine fuel frame value rest
        | .unsupported => .unsupported
        | .matched captures =>
          match guard with
          | none => runBound machine fuel frame captures body
          | some condition =>
            match bindCaptures machine frame.enter captures with
            | .normal bound =>
              ((evalExpr machine fuel bound condition).leave frame).toExec fun tested next =>
                  match tested.eraseNominal with
                  | .scalar (.boolean true) => runBound machine fuel next captures body
                  | .scalar (.boolean false) => runMatch machine fuel next value rest
                  | _ => .unsupported
            | other => leaveBlock machine fuel frame other

  /-- `match` as an expression (SPEC 6.8): the first arm whose pattern selects
  the subject and whose guard holds gives the value of its body, evaluated in
  the arm's own scope with the pattern's captures bound; with no such arm the
  match fails with `match-no-arm`. -/
  def matchValue {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Value Declaration Identity Source →
        List (Xsh.Syntax.Pattern × Option Xsh.Syntax.Expr × Xsh.Syntax.Expr) → Valued world
    | 0, _, _, _ => .exec .outOfFuel
    | fuel + 1, frame, value, arms =>
      match arms with
      | [] => .exec (.failed (.runtime machine.context.errors.matchNoArm) frame)
      | (pattern, guard, body) :: rest =>
        match matchPattern machine.context machine.typeTest pattern value with
        | .failed => matchValue machine fuel frame value rest
        | .unsupported => .exec .unsupported
        | .matched captures =>
          match guard with
          | none =>
            settle (leaveBlock machine fuel frame)
              ((bindCaptures machine frame.enter captures).thenValued fun bound => runValue machine fuel bound body)
          | some condition =>
            match bindCaptures machine frame.enter captures with
            | .normal bound =>
              ((evalExpr machine fuel bound condition).leave frame).bind fun tested next =>
                match tested.eraseNominal with
                | .scalar (.boolean true) =>
                  settle (leaveBlock machine fuel next)
                    ((bindCaptures machine next.enter captures).thenValued fun rebound =>
                      runValue machine fuel rebound body)
                | .scalar (.boolean false) => matchValue machine fuel next value rest
                | _ => .exec .unsupported
            | other => settle (leaveBlock machine fuel frame) (.exec other)
end

/-- The functions a script declares at its top level. -/
def hoistedNames : List Xsh.Syntax.Statement → List Xsh.Syntax.RawName
  | [] => []
  | .functionDef kind name _ _ _ _ _ _ :: rest | .export (.functionDef kind name _ _ _ _ _ _) :: rest =>
    if (callableKind kind).isSome then name :: hoistedNames rest else hoistedNames rest
  | _ :: rest => hoistedNames rest

/-- Bind each hoisted function's name, as its declaration will. -/
def hoistNames {world : World Declaration Identity Source} (machine : Machine world) (activation : Nat) :
    Environment → List Xsh.Syntax.RawName → Option Environment
  | environment, [] => some environment
  | environment, name :: rest =>
    match (machine.namesAt activation).declaration environment name with
    | some (binder, cell, _) => hoistNames machine activation (extend environment [(name, binder, cell)]) rest
    | none => none

/-- One top-level statement's part in hoisting, as the statement itself will
bind when it runs: a binding or a `guard let` declares its target's names, the
`var`s among them writable; a `proc` or `pure` declares its name and its
closure over the environment at its declaration; sugar hoists as its
expansion does. -/
def hoistStatement {world : World Declaration Identity Source} (machine : Machine world) (activation : Nat) :
    Environment → List CellId → List (CellId × Closure) → Xsh.Syntax.Statement →
      Option (Environment × List CellId × List (CellId × Closure))
  | environment, mutable, closures, .binding kind target _ _ =>
    match resolveTarget (machine.namesAt activation) environment target with
    | some (_, declared) =>
      some (extend environment declared,
        if kind = .var then mutable ++ declared.map fun entry => entry.2.2 else mutable, closures)
    | none => none
  | environment, mutable, closures, .guardStmt target _ _ _ =>
    match resolveTarget (machine.namesAt activation) environment target with
    | some (_, declared) => some (extend environment declared, mutable, closures)
    | none => none
  | environment, mutable, closures, .guardHandled target _ _ _ _ =>
    match resolveTarget (machine.namesAt activation) environment target with
    | some (_, declared) => some (extend environment declared, mutable, closures)
    | none => none
  | environment, mutable, closures, .functionDef kind name parameters _ result body _ _ =>
    match callableKind kind, (machine.namesAt activation).declaration environment name with
    | some callable, some (binder, cell, _) =>
      some (extend environment [(name, binder, cell)], mutable,
        (cell, .mk cell callable (extend environment [(name, binder, cell)]) closures
          ((captureNames (parameterNames parameters) body).filterMap fun captured =>
            match (extend environment [(name, binder, cell)]).cellOf captured with
            | .ok found => if mutable.contains found then some found else none
            | .error _ => none)
          parameters (.block (expectBody machine.context.spelling result body) (returnKindOf machine result))) :: closures)
    | some _, none => none
    | none, _ => some (environment, mutable, closures)
  | environment, mutable, closures, .sugar _ expansion =>
    hoistStatement machine activation environment mutable closures expansion
  | environment, mutable, closures, .export declaration =>
    hoistStatement machine activation environment mutable closures declaration
  | environment, mutable, closures, _ => some (environment, mutable, closures)

/-- Each top-level function's closure before any statement runs: over the
environment at its declaration, which holds the top-level bindings declared
before it, with the `var`s among them writable. -/
def hoistClosures {world : World Declaration Identity Source} (machine : Machine world) (activation : Nat) :
    Environment → List CellId → List (CellId × Closure) → List Xsh.Syntax.Statement → Option (List (CellId × Closure))
  | _, _, closures, [] => some closures
  | environment, mutable, closures, statement :: rest =>
    match hoistStatement machine activation environment mutable closures statement with
    | some (environment, mutable, closures) => hoistClosures machine activation environment mutable closures rest
    | none => none

/-- A script's frame with its top-level declarations hoisted (SPEC 3.1:
"Declarations are visible throughout their script or module, including the
statements before them"): its functions' names, its enums' payload-free
variants as immutable bindings, then its functions' closures. -/
def hoist {world : World Declaration Identity Source} (machine : Machine world) (frame : Frame world)
    (body : List Xsh.Syntax.Statement) : Option (Frame world) :=
  (hoistNames machine frame.activation frame.environment (hoistedNames body)).bind fun environment =>
    match bindCaptures machine { frame with environment, locals := hide (hoistedNames body) frame.locals }
        (bareVariants machine) with
    | .normal bound =>
      (hoistClosures machine frame.activation bound.environment [] frame.closures body).map fun closures =>
        { bound with closures }
    | _ => none

/-- A script: its top-level functions hoisted, then its statements in order. -/
def runScript {world : World Declaration Identity Source} (machine : Machine world) (fuel : Nat)
    (frame : Frame world) (body : List Xsh.Syntax.Statement) : Exec world :=
  match hoist machine frame body with
  | some hoisted => runStatements machine fuel hoisted body
  | none => .internal .malformedTarget frame

end Xsh.Semantics.CoreControl
