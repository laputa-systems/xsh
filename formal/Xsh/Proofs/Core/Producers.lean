import Xsh.Proofs.Core.Exits

namespace Xsh.Proofs.Core.Producers

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

section Root

variable {Declaration Identity Source : Type}

/-- The `stream_producers` fragment root (SPEC 9.5): a `for` loop over a call
of a stream producer runs the producer with the loop as its consumer; each
`yield` runs the loop's body on the item in the loop's own scope, sharing the
store and environment, then resumes the producer; a body that completes or
continues resumes it, while one that breaks, returns or fails stops it,
unwinding its scopes, where its `defer`s run and its `errdefer`s and `try`s do
not see a failure of its own; and the loop then leaves as its body did. -/
theorem stream_producers [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) :
    (∀ (producer after : Frame world),
      resumeProducer producer (.normal after) = .normal (producer.resumedBy after) ∧
      resumeProducer producer (.continued after) = .normal (producer.resumedBy after) ∧
      resumeProducer producer (.broke after) = .consumed .broke (producer.resumedBy after)) ∧
    (∀ (producer after : Frame world) value,
      resumeProducer producer (.returned value after) =
        .consumed (.returned value) (producer.resumedBy after)) ∧
    (∀ (producer after : Frame world) failure,
      resumeProducer producer (.failed failure after) =
        .consumed (.failed failure) (producer.resumedBy after)) ∧
    (∀ exit (last : Frame world), exitsWithError (.consumed exit last) = false) ∧
    (∀ exit (last : Frame world), capture (.exec (.consumed exit last)) = .exec (.consumed exit last)) ∧
    (∀ (loop last : Frame world), consumeProducer loop (.exec (.consumed .broke last)) =
      .normal { last with consumers := loop.consumers }) ∧
    (∀ (loop last : Frame world) value, consumeProducer loop (.exec (.consumed (.returned value) last)) =
      .returned value { last with consumers := loop.consumers }) ∧
    (∀ (loop last : Frame world) failure, consumeProducer loop (.exec (.consumed (.failed failure) last)) =
      .failed failure { last with consumers := loop.consumers }) :=
  ⟨fun _ _ => ⟨rfl, rfl, rfl⟩, fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ => rfl, fun _ _ => rfl, fun _ _ => rfl,
    fun _ _ _ => rfl, fun _ _ _ => rfl⟩

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness
open Xsh.Proofs.Core.Hoisting.Witness
open Xsh.Proofs.Core.Scripts.Witness

def traceVar : Xsh.Syntax.RawName := name 270 (by decide)
def countFn : Xsh.Syntax.RawName := name 271 (by decide)
def nParam : Xsh.Syntax.RawName := name 272 (by decide)
def iVar : Xsh.Syntax.RawName := name 273 (by decide)
def xItem : Xsh.Syntax.RawName := name 274 (by decide)
def firstTwo : Xsh.Syntax.RawName := name 275 (by decide)
def findFn : Xsh.Syntax.RawName := name 276 (by decide)
def totalVar : Xsh.Syntax.RawName := name 277 (by decide)
def resultVar : Xsh.Syntax.RawName := name 278 (by decide)

def note (lexeme : String) : Xsh.Syntax.Statement := .assign (.name traceVar) .add (.list [int lexeme])

/-- `stream count(n: Int) -> Stream[Int] { defer { trace += [7] };
errdefer { trace += [8] }; var i = 0; while i < n { trace += [1]; yield i;
i += 1 } }`. -/
def declareCount : Xsh.Syntax.Statement :=
  .functionDef .stream countFn [.mk nParam none none false] none none
    [.defer .exit (.valueBlock [note "7"]), .defer .error (.valueBlock [note "8"]),
     .binding .var (.name iVar) none (int "0"),
     .whileStmt (.binary .lt (ident iVar) (ident nParam))
       [note "1", .yield (ident iVar), .assign (.name iVar) .add (int "1")]] none false

/-- `proc first_two() -> Int { var total = 0; for x in count(8) {
trace += [2]; total += x; if x == 1 { break } }; total }`. -/
def declareFirstTwo : Xsh.Syntax.Statement :=
  .functionDef .proc firstTwo [] none none
    [.binding .var (.name totalVar) none (int "0"),
     .forStmt (.name xItem) (.call (ident countFn) [.positional (int "8")])
       [note "2", .assign (.name totalVar) .add (ident xItem),
        .ifStmt [(.binary .eq (ident xItem) (int "1"), [.breakStmt none])] none],
     .expression (ident totalVar)] none false

/-- `proc find() -> Int { for x in count(8) { if x == 2 { return x } }; 0 }`. -/
def declareFind : Xsh.Syntax.Statement :=
  .functionDef .proc findFn [] none none
    [.forStmt (.name xItem) (.call (ident countFn) [.positional (int "8")])
       [.ifStmt [(.binary .eq (ident xItem) (int "2"), [.returnValue (some (ident xItem))])] none],
     .expression (int "0")] none false

def streamScript (call : Xsh.Syntax.RawName) : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount, declareFirstTwo, declareFind,
   letOf resultVar (.call (ident call) [])]

def readList (target : Xsh.Syntax.RawName) : Exec witnessWorld → Option (List Int)
  | .normal frame =>
    match frame.environment.cellOf target with
    | .ok cell => (frame.store cell).bind fun found =>
        match found.contents.value with
        | .list items => items.mapM fun
            | .scalar (.integer value) => some value.val
            | _ => none
        | _ => none
    | .error _ => none
  | _ => none

/-- As the verification binary prints `yield 0`, `got 0`, `yield 1`, `got 1`,
`producer defer` and `1` for `first_two()`: the producer and the loop body
alternate, the loop's `break` runs the producer's `defer` but not its
`errdefer`, and the total is `0 + 1`. -/
theorem consumer_break_witness :
    readList traceVar (runCanonical (streamScript firstTwo)) = some [1, 2, 1, 2, 7] ∧
      readInt resultVar (runCanonical (streamScript firstTwo)) = some 1 := by
  decide +kernel

/-- As the binary prints three `yield`s, `producer defer` and `2` for
`find()`: a `return` from the loop body stops the producer, runs its
`defer`, and returns from the consumer. -/
theorem consumer_return_witness :
    readList traceVar (runCanonical (streamScript findFn)) = some [1, 1, 1, 7] ∧
      readInt resultVar (runCanonical (streamScript findFn)) = some 2 := by
  decide +kernel

/-- Collecting the producer's items before running the loop would note every
`yield` first and run the producer to its end; the model alternates them. -/
theorem eager_producer_rejected :
    readList traceVar (runCanonical (streamScript firstTwo)) ≠ some [1, 1, 1, 1, 1, 1, 1, 1, 7, 2, 2] := by
  decide +kernel

def pairsFn : Xsh.Syntax.RawName := name 279 (by decide)
def delegating : Xsh.Syntax.RawName := name 280 (by decide)

/-- `stream pairs() -> Stream[Int] { defer { trace += [7] }; trace += [1];
yield @[7, 8, 2]; trace += [8] }` and a loop that notes `2` per item and
breaks after `8`. -/
def delegationScript (breaking : Bool) : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []),
   .functionDef .stream pairsFn [] none none
     [.defer .exit (.valueBlock [note "7"]), note "1", .yieldDelegate (items ["7", "8", "2"]), note "8"] none false,
   .functionDef .proc delegating [] none none
     [.forStmt (.name xItem) (.call (ident pairsFn) [])
       ([note "2"] ++ if breaking then [.ifStmt [(.binary .eq (ident xItem) (int "8"), [.breakStmt none])] none] else []),
      .expression (int "0")] none false,
   letOf resultVar (.call (ident delegating) [])]

/-- As the verification binary prints `before`, `got 7`, `got 8`,
`producer defer` for a loop that breaks at the second delegated item, and
`before`, three items, `after`, `producer defer` for one that does not:
`yield @list` yields each item in turn and stops when the loop does. -/
theorem delegation_witness :
    readList traceVar (runCanonical (delegationScript true)) = some [1, 2, 2, 7] ∧
      readList traceVar (runCanonical (delegationScript false)) = some [1, 2, 2, 2, 8, 7] := by
  decide +kernel

def streamLocal : Xsh.Syntax.RawName := name 281 (by decide)
def outerFn : Xsh.Syntax.RawName := name 282 (by decide)
def yItem : Xsh.Syntax.RawName := name 283 (by decide)

def recordItem (item : Xsh.Syntax.RawName) : Xsh.Syntax.Statement :=
  .assign (.name traceVar) .add (.list [ident item])

/-- `let s = count(8); trace += [8]; for x in s { trace += [x]; if x == 0 { break } };
for y in s { trace += [y] }`, as the verification binary prints `created`,
`yield 0`, `got 0`, `producer defer` and nothing for the second loop: the
call creates the stream without running the body, the break closes it, and
a consumed stream yields nothing again. -/
def storedScript : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount,
   letOf streamLocal (.call (ident countFn) [.positional (int "8")]),
   note "8",
   .forStmt (.name xItem) (ident streamLocal)
     [recordItem xItem, .ifStmt [(.binary .eq (ident xItem) (int "0"), [.breakStmt none])] none],
   .forStmt (.name yItem) (ident streamLocal) [recordItem yItem]]

/-- `stream outer() -> Stream[Int] { yield 7; yield @count(2); yield 8 }`
consumed by a loop recording each item, as the binary prints `z 7`,
`yield 0`, `z 0`, `yield 1`, `z 1`, `producer defer`, `z 8`: the delegated
stream is pulled on demand and closed before its parent goes on. -/
def delegatingScript : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount,
   .functionDef .stream outerFn [] none none
     [.yield (int "7"), .yieldDelegate (.call (ident countFn) [.positional (int "2")]), .yield (int "8")] none false,
   .forStmt (.name xItem) (.call (ident outerFn) []) [recordItem xItem]]

theorem stream_value_witness :
    readList traceVar (runCanonical storedScript) = some [8, 1, 0, 7] ∧
      readList traceVar (runCanonical delegatingScript) = some [7, 1, 0, 1, 1, 7, 8] := by
  decide +kernel

/-- A stream that restarts on each loop would run `count` again for the
second loop; the model's second loop yields nothing. -/
theorem restarting_stream_rejected :
    readList traceVar (runCanonical storedScript) ≠ some [8, 1, 0, 7, 1, 0, 1, 1] := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Producers
