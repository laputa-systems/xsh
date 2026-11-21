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
  /-- The checker's type for a type pattern's written type. -/
  typeOf : Xsh.Syntax.TypeExpr → Option (ValueType Declaration)
  /-- Whether the checker gives a `guard let` subject an outermost `Result`
  type (SPEC 8.6). -/
  resultSubject : Environment → Xsh.Syntax.Expr → Bool

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
  | mk (cell : CellId) (environment : Environment) (captured : List (CellId × Closure)) (writable : List CellId)
      (parameters : List Xsh.Syntax.RawName) (body : FunctionBody)

/-- What an expression sees besides its environment, store and locals: the
call activation, the local functions in scope, and the implicit `.` of the
innermost handler block. -/
structure Scope (Declaration Identity Source : Type) where
  activation : Nat
  closures : List (CellId × Closure)
  item : Option (Value Declaration Identity Source)

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
  /-- The caller's cells this frame may still write: a closure's captured
  mutable cells. -/
  writable : List CellId

def Frame.scope {world : World Declaration Identity Source} (frame : Frame world) : Scope Declaration Identity Source :=
  ⟨frame.activation, frame.closures, frame.item⟩

/-- A frame with nothing pending, in `scope`. -/
def Frame.ofScope {world : World Declaration Identity Source} (environment : Environment) (store : Store world)
    (locals : Locals Declaration Identity Source) (callerStore : Option (Store world))
    (scope : Scope Declaration Identity Source) : Frame world :=
  { environment, store, overlay := [], locals, callerStore, activation := scope.activation, deferred := [],
    reported := [], waited := [], breakValue := none, closures := scope.closures, item := scope.item,
    collected := none, writable := [] }

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

/-- The value a text-pattern hole takes (SPEC 6.10): a `Str`, or an `Int` for
`d`, `x`, `o` and `b`; the `Float` letters `f`, `e` and `g` need Float literal
rounding, which is outside the model. -/
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

/-- A condition's value: a `Bool`, or a `Result[Bool]`, whose `Err` leaves as
under `?` (SPEC 8.3). -/
def conditionValue (tested : Value Declaration Identity Source) : Option (Except (Value Declaration Identity Source) Bool) :=
  match tested.eraseNominal with
  | .scalar (.boolean value) => some (.ok value)
  | .ok inner =>
    match inner.eraseNominal with
    | .scalar (.boolean value) => some (.ok value)
    | _ => none
  | .err error => some (.error error)
  | _ => none

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

def resultExec {world : World Declaration Identity Source} {α : Type} (frame : Frame world) :
    Result Declaration Identity Source α → (α → Exec world) → Exec world
  | .value result, next => next result
  | .failure failure, _ => .failed failure frame
  | .stuck, _ => .stuck
  | .unsupported, _ => .unsupported
  | .outOfFuel, _ => .outOfFuel

/-- The selector an index value names in a container of static type `type`:
a list position, a map key, or a record field. -/
def selectorOf (type : Option (ValueType Declaration)) (index : Value Declaration Identity Source) : Option Selector :=
  match type.bind elementType, type.bind mapTypes, index.eraseNominal with
  | some _, _, .scalar (.integer position) => some (.index position.val)
  | none, some _, _ => (mapKeyOf index).map .key
  | none, none, .scalar (.string label) => some (.field label)
  | _, _, _ => none

/-- The selectors of an assignment path, index expressions evaluated through
`evaluate`, each given the static type of the container it selects in. -/
def selectorsOf {world : World Declaration Identity Source}
    (evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    Option (ValueType Declaration) → List PathSpec → Result Declaration Identity Source (List (SelectorSide world))
  | _, [] => .value []
  | type, .field label :: rest =>
    (selectorsOf evaluate (staticChild type (.field label)) rest).bind fun others =>
      .value ((fun store => .ok (.field label, store)) :: others)
  | type, .index selector :: rest =>
    (evaluate selector).bind fun index =>
      match selectorOf type index with
      | none => .unsupported
      | some chosen =>
        (selectorsOf evaluate (staticChild type (.index selector)) rest).bind fun others =>
          .value ((fun store => .ok (chosen, store)) :: others)

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

/-- A statement-level call of a local function returns to its caller with the
caller's cells as the callee left them, so a write to a captured mutable cell
is kept; the callee's own cells end with the call. -/
def returnTo {world : World Declaration Identity Source} (caller : Frame world) (kind : ReturnKind) :
    Exec world → Valued world
  | .returned value callee =>
    .value (match kind with | .result => wrapOk value | .plain => value)
      { caller with store := keep caller.store callee.store }
  | .failed (.raised error) callee =>
    match kind with
    | .result => .value (.err error) { caller with store := keep caller.store callee.store }
    | .plain => .exec (.failed (.raised error) { caller with store := keep caller.store callee.store })
  | .failed failure callee => .exec (.failed failure { caller with store := keep caller.store callee.store })
  | .unsupported => .exec .unsupported
  | .outOfFuel => .exec .outOfFuel
  | _ => .exec .stuck

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

/-- The parameter names of a declaration whose parameters have no defaults,
annotations aside, and no rest parameter. -/
def plainParameters : List Xsh.Syntax.Parameter → Option (List Xsh.Syntax.RawName)
  | [] => some []
  | .mk name _ none false :: rest => (plainParameters rest).map (name :: ·)
  | _ :: _ => none

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

mutual
  /-- Expressions, with block function bodies run as statements. -/
  def evalExpr {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Scope Declaration Identity Source → Store world → Environment → Locals Declaration Identity Source →
        Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)
    | 0, _, _, _, _, _ => .outOfFuel
    | fuel + 1, scope, store, environment, locals, expression =>
      match expression with
      | .tryExpr inner => (evalExpr machine fuel scope store environment locals inner).bind propagate
      | .convert inner target =>
        (evalExpr machine fuel scope store environment locals inner).bind fun value =>
          match machine.conversion value target with
          | some converted => propagate converted
          | none => .unsupported
      | .item =>
        match scope.item with
        | some value => .value value
        | none => .unsupported
      | .valueBlock _ | .capture _ | .retry _ _ _ _ | .binary .resultFallback _ _ | .loopExpr _ | .collect _
          | .listComp _ _ =>
        valuedResult (runValue machine fuel (Frame.ofScope environment store locals (some store) scope) expression)
      | .call (.ident name) arguments =>
        match locals.lookup name, localClosure scope.closures environment name, positional arguments with
        | none, some (.mk cell defining captured writable parameters body), some values =>
          (evaluateAll (evalExpr machine fuel scope store environment locals) values).bind fun evaluated =>
            if parameters.length = evaluated.length then
              let callee : Scope Declaration Identity Source :=
                ⟨scope.activation + machine.activationStep,
                  (cell, .mk cell defining captured writable parameters body) :: captured, none⟩
              match body with
              | .expression result .plain => evalExpr machine fuel callee store defining (parameters.zip evaluated) result
              | .expression result .result =>
                callBlock machine fuel callee store defining (parameters.zip evaluated) [.expression result] .result
              | .block statements kind => callBlock machine fuel callee store defining (parameters.zip evaluated) statements kind
            else .stuck
        | _, _, _ =>
          match locals.lookup name, machine.context.functions name, machine.context.spelling name, arguments with
          | none, none, "Ok", [] => .value (.ok (.scalar .unit))
          | none, none, "Ok", [.positional payload] =>
            (evalExpr machine fuel scope store environment locals payload).bind fun value => .value (.ok value)
          | none, none, "Err", [.positional payload] =>
            (evalExpr machine fuel scope store environment locals payload).bind fun value => .value (.err value)
          | _, _, _, _ =>
            step machine.context environment store
              (fun inner innerLocals => evalExpr machine fuel scope store inner innerLocals)
              (fun defining parameters body kind =>
                callBlock machine fuel ⟨scope.activation + machine.activationStep, [], none⟩ store defining parameters body
                  kind)
              locals expression
      | _ =>
        step machine.context environment store
          (fun inner innerLocals => evalExpr machine fuel scope store inner innerLocals)
          (fun defining parameters body kind =>
            callBlock machine fuel ⟨scope.activation + machine.activationStep, [], none⟩ store defining parameters body kind)
          locals expression

  /-- A block function body runs in its own activation, in its defining
  environment with its parameters as locals; it may not write the caller's
  cells, and its own cells end with the call. -/
  def callBlock {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Scope Declaration Identity Source → Store world → Environment → Locals Declaration Identity Source →
        List Xsh.Syntax.Statement → ReturnKind → Result Declaration Identity Source (Value Declaration Identity Source)
    | 0, _, _, _, _, _, _ => .outOfFuel
    | fuel + 1, scope, store, defining, parameters, body, kind =>
      boundary kind (runBody machine fuel (Frame.ofScope defining store parameters (some store) scope) body)

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
        resultThen frame (evaluateAll (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals) delays)
          fun scheduled => retryLoop machine fuel frame pattern body scheduled
      | .tryExpr inner =>
        (runValue machine fuel frame inner).bind fun value next => fromEval next (propagate value)
      | .convert inner target =>
        (runValue machine fuel frame inner).bind fun value next =>
          match machine.conversion value target with
          | some converted => fromEval next (propagate converted)
          | none => .exec .unsupported
      | .retry .backoff _ _ _ => .exec .unsupported
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
      | .listComp value qualifiers => comprehend machine fuel frame qualifiers value
      | .call (.ident name) arguments =>
        match frame.locals.lookup name, localClosure frame.closures frame.environment name, positional arguments with
        | none, some (.mk cell defining captured writable parameters body), some values =>
          resultThen frame (evaluateAll (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals) values)
            fun evaluated =>
              if parameters.length = evaluated.length then
                returnTo frame (bodyKind body)
                  (runBody machine fuel
                    { Frame.ofScope defining frame.store (parameters.zip evaluated) (some frame.store)
                        ⟨frame.activation + machine.activationStep,
                          (cell, .mk cell defining captured writable parameters body) :: captured, none⟩ with
                      writable }
                    (bodyStatements body))
              else .exec .stuck
        | _, _, _ => fromEval frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals expression)
      | .collect body =>
        collectWith frame (runBlock machine fuel { frame with collected := some [] } body)
      | expression => fromEval frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals expression)

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
        fromEval frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals condition) |>.bind
          fun tested next =>
            match tested.eraseNominal with
            | .scalar (.boolean true) => comprehend machine fuel next rest value
            | .scalar (.boolean false) => .value (.list []) next
            | _ => .exec .unsupported
      | .each target iterable :: rest =>
        fromEval frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals iterable) |>.bind
          fun source next =>
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
              resultExec frame
                (selectorsOf (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals)
                  (some current.contents.typeTag) path) fun sides =>
                (runValue machine fuel frame value).toExec fun result next =>
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
      | .forStmt target iterable body =>
        fromResult frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals iterable) fun source =>
          match iterationItems source with
          | some items => runFor machine fuel frame target items body
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
          match next.collected with
          | some gathered => .normal { next with collected := some (gathered ++ [result]) }
          | none => .unsupported
      | .yieldDelegate value =>
        (runValue machine fuel frame value).toExec fun result next =>
          match next.collected, listValues result with
          | some gathered, some items => .normal { next with collected := some (gathered ++ items) }
          | _, _ => .unsupported
      | .functionDef kind name parameters _ result body _ _ =>
        match kind, plainParameters parameters, (machine.namesAt frame.activation).declaration frame.environment name with
        | .stream, _, _ | .cliMain, _, _ | _, none, _ => .unsupported
        | _, _, none => .internal .malformedTarget frame
        | _, some names, some (binder, cell, _) =>
          let environment := extend frame.environment [(name, binder, cell)]
          .normal { frame with
            environment
            locals := hide [name] frame.locals
            closures := (cell, .mk cell environment frame.closures (mutableCaptures frame (captureNames names body)) names
              (.block body (returnKindOf machine result))) :: frame.closures }
      | .guardStmt target _ initializer otherwise =>
        (runValue machine fuel frame initializer).toExec fun subject next =>
          match guardValue (machine.resultSubject next.environment initializer) subject with
          | some bound => bindTarget machine next .immutable target bound
          | none => guardElse (runBlock machine fuel next otherwise)
      | .continueStmt => .continued frame
      | .matchStmt subject arms =>
        fromResult frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals subject) fun value =>
          runMatch machine fuel frame value arms
      | .assert condition message =>
        fromResult frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals condition) fun tested =>
          match tested.eraseNominal with
          | .scalar (.boolean true) => .normal frame
          | .scalar (.boolean false) =>
            match message with
            | none => .failed (.raised (machine.assertion none)) frame
            | some text =>
              fromResult frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals text) fun written =>
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
        fromResult frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals subject) fun value =>
          match conditionMatch machine.context machine.typeTest pattern value with
          | .matched captures => runBound machine fuel frame captures body
          | .failed => runIf machine fuel frame rest otherwise
          | .unsupported => .unsupported
      | (condition, body) :: rest, otherwise =>
        fromResult frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals condition) fun tested =>
          match conditionValue tested with
          | some (.ok true) => runBlock machine fuel frame body
          | some (.ok false) => runIf machine fuel frame rest otherwise
          | some (.error error) => .failed (.raised error) frame
          | none => .unsupported

  /-- `while` and `while let`: each iteration's body is its own scope; `break`
  ends this loop and `continue` starts its next test. -/
  def runWhile {world : World Declaration Identity Source} (machine : Machine world) :
      Nat → Frame world → Xsh.Syntax.Expr → List Xsh.Syntax.Statement → Exec world
    | 0, _, _, _ => .outOfFuel
    | fuel + 1, frame, condition, body =>
      match condition with
      | .patternCondition subject pattern =>
        fromResult frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals subject) fun value =>
          match conditionMatch machine.context machine.typeTest pattern value with
          | .matched captures =>
            (runBound machine fuel frame captures body).iterate fun next =>
              runWhile machine fuel next (.patternCondition subject pattern) body
          | .failed => .normal frame
          | .unsupported => .unsupported
      | condition =>
        fromResult frame (evalExpr machine fuel frame.scope frame.store frame.environment frame.locals condition) fun tested =>
          match conditionValue tested with
          | some (.ok true) =>
            (runBlock machine fuel frame body).iterate fun next => runWhile machine fuel next condition body
          | some (.ok false) => .normal frame
          | some (.error error) => .failed (.raised error) frame
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
      | [] => .stuck
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
              fromResult frame (evalExpr machine fuel bound.scope bound.store bound.environment bound.locals condition)
                fun tested =>
                  match tested.eraseNominal with
                  | .scalar (.boolean true) => runBound machine fuel frame captures body
                  | .scalar (.boolean false) => runMatch machine fuel frame value rest
                  | _ => .unsupported
            | other => leaveBlock machine fuel frame other
end

end Xsh.Semantics.CoreControl
