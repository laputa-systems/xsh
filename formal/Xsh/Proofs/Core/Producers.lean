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
not see a failure of its own; and the loop then leaves as its body did.
`.collect()` on a pending stream runs its producer with a sink consumer that
appends each yielded item to the stream's entry, then gives the gathered list
and closes the stream; on a consumed stream it gives the empty list. Inside a
`collect` block the loop's consumer gathers into a stream table entry: each
run of the body starts from the list there and leaves its list there, and the
loop's frame takes the list back when the producer has run. The body runs
in the loop's working directory and the producer resumes in its own, so a
producer that yields from inside a `cd` scope keeps its directory between
pulls (SPEC 10.4). -/
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
      .normal (loop.closedLoop last)) ∧
    (∀ (loop last : Frame world) value, consumeProducer loop (.exec (.consumed (.returned value) last)) =
      .returned value (loop.closedLoop last)) ∧
    (∀ (loop last : Frame world) failure, consumeProducer loop (.exec (.consumed (.failed failure) last)) =
      .failed failure (loop.closedLoop last)) ∧
    (∀ (fuel : Nat) (frame : Frame world) receiver method,
      errorConstructor machine frame receiver method = none → isEnvPathList receiver = false →
      machine.context.spelling method = "collect" →
      evalExpr machine (fuel + 1) frame (.call (.field receiver method) []) =
        (evalExpr machine fuel frame receiver).bind fun subject after =>
          match pendingStream machine after subject with
          | some (index, some (closure, supplied)) =>
            gathered after index (invokeSupplied machine (evalExpr machine fuel) (callBlock machine fuel)
              { after with streams := after.streams.set index (.gathering []),
                           consumers := sinkConsumer after index :: after.consumers } closure supplied)
          | some (_, none) => .value (.list []) after
          | none => .exec .unsupported) ∧
    (∀ (frame : Frame world) index items item, frame.streams[index]? = some (.gathering items) →
      frame.gather index item = .normal { frame with streams := frame.streams.set index (.gathering (items ++ [item])) }) ∧
    (∀ (loop last : Frame world) index items value, last.streams[index]? = some (.gathering items) →
      gathered loop index (.value value last) =
        .value (.list items) { last.consume index with consumers := loop.consumers, collected := loop.collected }) ∧
    (∀ (producer : Frame world) loop outer slot, producer.consumers = loop :: outer → loop.gathers = some slot →
      (consumerFrame producer loop outer).collected = gatheringAt producer.streams slot) ∧
    (∀ (producer after : Frame world) loop outer slot items, producer.consumers = loop :: outer →
      loop.gathers = some slot → after.collected = some items →
      (producer.resumedBy after).streams = after.streams.set slot (.gathering items)) ∧
    (∀ (loop last : Frame world) consumer rest slot, last.consumers = consumer :: rest → consumer.gathers = some slot →
      (loop.closedLoop last).collected = gatheringAt last.streams slot) ∧
    (∀ (producer : Frame world) loop outer, (consumerFrame producer loop outer).directory = loop.directory) ∧
    (∀ (producer after : Frame world), (producer.resumedBy after).directory = producer.directory) := by
  refine ⟨fun _ _ => ⟨rfl, rfl, rfl⟩, fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ => rfl, fun _ _ => rfl, fun _ _ => rfl,
    fun _ _ _ => rfl, fun _ _ _ => rfl, fun fuel frame receiver method plain notPath spelled => ?_,
    fun frame index items item held => by simp only [Frame.gather, held],
    fun loop last index items value held => by simp only [gathered, held],
    fun producer loop outer slot _ gathers => by simp only [consumerFrame, gathers],
    fun producer after loop outer slot items pushed gathers collected => by
      simp only [Frame.resumedBy, gatheredBack, pushed, gathers, collected],
    fun loop last consumer rest slot pushed gathers => by simp only [Frame.closedLoop, pushed, gathers],
    fun _ _ _ => rfl, fun _ _ => rfl⟩
  have notEnv : envFunction machine frame receiver method = none := by
    unfold envFunction
    split <;> simp_all
  simp only [evalExpr, plain, notPath, Bool.false_eq_true, if_false, notEnv, spelled]
  refine congrArg _ (funext fun subject => funext fun after => ?_)
  rcases pendingStream machine after subject with _ | ⟨index, _ | ⟨closure, supplied⟩⟩ <;> rfl

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

def collectName : Xsh.Syntax.RawName := name 284 (by decide)
def gatheredVar : Xsh.Syntax.RawName := name 285 (by decide)
def againVar : Xsh.Syntax.RawName := name 286 (by decide)
def listedFn : Xsh.Syntax.RawName := name 287 (by decide)

def collectMachine : Machine witnessWorld :=
  { scriptMachine with
    context := { scriptMachine.context with
      spelling := fun named => if named = collectName then "collect" else scriptMachine.context.spelling named } }

def collectOf (receiver : Xsh.Syntax.Expr) : Xsh.Syntax.Expr := .call (.field receiver collectName) []

def runCollect (script : List Xsh.Syntax.Statement) : Exec witnessWorld := runScript collectMachine 90 fullStart script

/-- `let s = count(2); trace += [2]; let xs = s.collect(); let ys = s.collect()`,
as the verification binary prints `created`, a `yield` per item, `producer
defer`, the items and `[]` (for `count(3)`, `[0,1,2]`): the call creates the stream, `.collect()` runs the
producer to its end gathering every item, and the consumed stream gathers
nothing again. -/
def collectScript : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount,
   letOf streamLocal (.call (ident countFn) [.positional (int "2")]),
   note "2",
   letOf gatheredVar (collectOf (ident streamLocal)),
   letOf againVar (collectOf (ident streamLocal))]

/-- `stream listed() -> Stream[Int] { yield 7; yield @[8, 2] }` collected:
a delegated list's items are gathered in turn, as the binary prints
`[7,0,1,8,9]` for a producer that also delegates `count(2)` between them. -/
def listedScript : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []),
   .functionDef .stream listedFn [] none none [.yield (int "7"), .yieldDelegate (items ["8", "2"])] none false,
   letOf gatheredVar (collectOf (.call (ident listedFn) []))]

theorem collect_witness :
    readList traceVar (runCollect collectScript) = some [2, 1, 1, 7] ∧
      readList gatheredVar (runCollect collectScript) = some [0, 1] ∧
      readList againVar (runCollect collectScript) = some [] ∧
      readList gatheredVar (runCollect listedScript) = some [7, 8, 2] := by
  decide +kernel

/-- A `.collect()` that restarted the producer would gather `[0, 1]` again
and run `count` twice; the model gathers nothing. -/
theorem recollecting_rejected :
    readList traceVar (runCollect collectScript) ≠ some [2, 1, 1, 7, 1, 1, 7] ∧
      readList againVar (runCollect collectScript) ≠ some [0, 1] := by
  decide +kernel

/-- `let xs = collect { for x in count(2) { yield x } }` and the same loop
breaking after its first item, as the verification binary prints `[0,1]` and
`[0]`: inside a `collect` block each run of the consuming loop's body appends
to the list the run before it left, while the producer holds the loop. -/
def collectLoopScript (breaking : Bool) : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount,
   letOf gatheredVar (.collect [.forStmt (.name xItem) (.call (ident countFn) [.positional (int "2")])
     ([.yield (ident xItem)] ++ if breaking then [.breakStmt none] else [])])]

theorem collect_loop_witness :
    readList gatheredVar (runCollect (collectLoopScript false)) = some [0, 1] ∧
      readList gatheredVar (runCollect (collectLoopScript true)) = some [0] := by
  decide +kernel

/-- A body that started each run from the list the loop began with, as the
machine did before, gathers nothing; the model gathers both items. -/
theorem forgetting_body_rejected :
    readList gatheredVar (runCollect (collectLoopScript false)) ≠ some [] := by
  decide +kernel

/-- `let zs = [x * 7 for x in s if x > 0]` over `let s = count(2)`, then the
same comprehension again, as the verification binary prints `[7]` for the
first: the comprehension pulls the stream through its qualifiers, and the
consumed stream gives `[]`. -/
def comprehensionScript : List Xsh.Syntax.Statement :=
  let comprehension := Xsh.Syntax.Expr.listComp (.binary .mul (ident xItem) (int "7"))
    [.each (.name xItem) (ident streamLocal), .when (.binary .gt (ident xItem) (int "0"))]
  [.binding .var (.name traceVar) none (.list []), declareCount,
   letOf streamLocal (.call (ident countFn) [.positional (int "2")]),
   letOf gatheredVar comprehension, letOf againVar comprehension]

theorem stream_comprehension_witness :
    readList gatheredVar (runCollect comprehensionScript) = some [7] ∧
      readList againVar (runCollect comprehensionScript) = some [] ∧
      readList traceVar (runCollect comprehensionScript) = some [1, 1, 7] := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Producers
