import Xsh.Proofs.Core.Producers

namespace Xsh.Proofs.Core.Pipelines

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

/-- The `structured_pipelines` fragment root (SPEC 13.1-13.3): `source |>
stage |> ...` passes each item through every stage before the next item
starts, a list source's items in order and a pending stream's as its
producer yields them to a sink consumer that carries the stages, gathering
what each item becomes into the stream's entry and closing the stream at the
end; a source inside one outer `Result` is its `Ok` collection, and an
`Err` raises its error. A stage block binds the item to its parameter or, without one, to `.`,
in its own scope, and its tail is its value; `where` keeps the item when that
value is `true`, `map` replaces it and `flat-map` passes each item of the
value's list on in turn. A `take(n)` ending the transformations is evaluated
once before the source is pulled and stops pulling once `n` items have
reached it: a list's later items are not run, and a producer is stopped as
by its consumer's `break`, so its `defer`s run. The pipeline gives the list
of items that reach its end, or their count under a closing `count()`.
Whatever it gives, it leaves the environment and locals as it found them and
keeps every cell already in the store. -/
theorem structured_pipelines [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ input stages, evalExpr machine (fuel + 1) frame (.structuredPipeline input stages) =
      (evalExpr machine fuel frame input).bind fun result next =>
        match pipelineSource result with
        | .error error => .exec (.failed (.raised error) next)
        | .ok source =>
          (pipelineLimit (evalExpr machine fuel) next (pipelineSplit stages).2.1).bind fun limit after =>
            match pendingStream machine after source, limit with
            | some (index, some _), some 0 => finishPipeline (pipelineSplit stages).2.2 (.list []) (after.consume index)
            | some (index, some (closure, supplied)), _ =>
              (gathered after index (invokeSupplied machine (evalExpr machine fuel) (callBlock machine fuel)
                  { after with
                    streams := after.streams.set index (.gathering [])
                    consumers := { sinkConsumer after index with stages := (pipelineSplit stages).1, limit } ::
                      after.consumers }
                  closure supplied)).bind fun out last => finishPipeline (pipelineSplit stages).2.2 out last
            | some (_, none), _ => finishPipeline (pipelineSplit stages).2.2 (.list []) after
            | none, _ => runPipeline (stageItems machine fuel) after source stages limit) ∧
    (∀ source stages limit items, listValues source = some items →
      runPipeline (stageItems machine fuel) frame source stages limit =
        (stageItems machine fuel frame (pipelineSplit stages).1 limit items).bind fun out last =>
          finishPipeline (pipelineSplit stages).2.2 out last) ∧
    (∀ stages items, stageItems machine (fuel + 1) frame stages (some 0) items = .value (.list []) frame) ∧
    (∀ stages item rest limit, limit ≠ some 0 → stageItems machine (fuel + 1) frame stages limit (item :: rest) =
      (stageItem machine fuel frame stages item).bind fun first after =>
        match first with
        | .list head =>
          match afterItems limit head.length with
          | none => .value (.list (limitTake limit head)) after
          | some remaining =>
            (stageItems machine fuel after stages remaining rest).bind fun later last =>
              match later with
              | .list tail => .value (.list (head ++ tail)) last
              | _ => .exec .stuck
        | _ => .exec .stuck) ∧
    (∀ (inner error : Value Declaration Identity Source),
      pipelineSource (.ok inner) = .ok inner ∧ pipelineSource (.err error) = .error error) ∧
    (∀ bound count, bound ≤ count → afterItems (some bound) count = none) ∧
    (∀ (gathering : Frame world) index bound out items gathered, listValues out = some items →
      gathering.streams[index]? = some (.gathering gathered) → bound ≤ gathered.length + items.length →
      gathering.gatherAll index (some bound) out =
        .broke { gathering with streams := gathering.streams.set index (.gathering ((gathered ++ items).take bound)) }) ∧
    (∀ (loop last : Frame world) index items, last.streams[index]? = some (.gathering items) →
      gathered loop index (.exec (.consumed .broke last)) =
        .value (.list items) { last.consume index with consumers := loop.consumers, collected := loop.collected }) ∧
    (∀ item, stageItem machine (fuel + 1) frame [] item = .value (.list [item]) frame) ∧
    (∀ parameter body rest item, stageAction (.stream .map [] (some (parameter, body))) = some (.replace, parameter, body) ∧
      stageAction (.stream .whereStage [] (some (parameter, body))) = some (.keep, parameter, body) ∧
      stageAction (.stream .flatMap [] (some (parameter, body))) = some (.expand, parameter, body) ∧
      stageItem machine (fuel + 1) frame (.stream .map [] (some (parameter, body)) :: rest) item =
        (handlerScope machine fuel frame (parameter.elim (some item) fun _ => none)
            (parameter.toList.map fun name => (name, item)) body).bind fun result after =>
          stageItem machine fuel after rest result) ∧
    (∀ input stages, VRestores frame (evalExpr machine fuel frame (.structuredPipeline input stages))) ∧
    (∀ input stages, VKeeps frame.store (evalExpr machine fuel frame (.structuredPipeline input stages))) := by
  refine ⟨fun input stages => ?_, fun source stages limit items listed => by simp only [runPipeline, listed],
    fun stages items => by cases items <;> rfl,
    fun stages item rest limit nonzero => ?_,
    fun _ _ => ⟨rfl, rfl⟩,
    fun bound count reached => by simp [afterItems, reached],
    fun gathering index bound out items gathered listed held reached => by
      simp only [Frame.gatherAll, listed, held, reached, if_true],
    fun loop last index items held => by simp only [gathered, held],
    fun _ => rfl, fun _ _ _ _ => ⟨rfl, rfl, rfl, rfl⟩,
    fun input stages => (restoring machine fuel).expr frame _, fun input stages => (persisting machine fuel).expr frame _⟩
  · simp only [evalExpr]
    refine congrArg _ (funext fun result => funext fun next => ?_)
    cases sourced : pipelineSource result with
    | error error => simp only [sourced]
    | ok source =>
      simp only [sourced]
      refine congrArg _ (funext fun limit => funext fun after => ?_)
      rcases pendingStream machine after source with _ | ⟨index, _ | ⟨closure, supplied⟩⟩ <;>
        rcases limit with _ | _ | bound <;> rfl
  · rcases limit with _ | _ | bound
    · rfl
    · exact absurd rfl nonzero
    · rfl

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness
open Xsh.Proofs.Core.Hoisting.Witness
open Xsh.Proofs.Core.Scripts.Witness
open Xsh.Proofs.Core.Producers.Witness

def outVar : Xsh.Syntax.RawName := name 290 (by decide)
def yParam : Xsh.Syntax.RawName := name 291 (by decide)
def countVar : Xsh.Syntax.RawName := name 292 (by decide)

def mapStage (parameter : Option Xsh.Syntax.RawName) (body : List Xsh.Syntax.Statement) : Xsh.Syntax.PipeStage :=
  .stream .map [] (some (parameter, body))

/-- `var trace = []; let out = [1, 2] |> map { trace += [.]; . * 7 } |> map { |y| trace += [y]; y + 1 }`,
as the verification binary records `a1 b10 a2 b20` and gives `[11, 21]` for
the same shape: each item passes both stages before the next starts. -/
def mapScript : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []),
   letOf outVar (.structuredPipeline (items ["1", "2"])
     [mapStage none [.assign (.name traceVar) .add (.list [.item]), .expression (.binary .mul .item (int "7"))],
      mapStage (some yParam) [recordItem yParam, .expression (.binary .add (ident yParam) (int "1"))]])]

/-- `[1, 2, 7] |> where { |x| x > 1 } |> count()`, as the binary gives `2`. -/
def countScript : List Xsh.Syntax.Statement :=
  [letOf countVar (.structuredPipeline (items ["1", "2", "7"])
     [.stream .whereStage [] (some (some xItem, [.expression (.binary .gt (ident xItem) (int "1"))])),
      .stream .count [] none])]

theorem pipeline_witness :
    readList traceVar (runCanonical mapScript) = some [1, 7, 2, 14] ∧
      readList outVar (runCanonical mapScript) = some [8, 15] ∧
      readInt countVar (runCanonical countScript) = some 2 := by
  decide +kernel

/-- Running each stage over every item before the next stage, as a list
pipeline that materialized between stages would, records `1 2 7 14`; the
model records `1 7 2 14`. -/
theorem stagewise_rejected :
    readList traceVar (runCanonical mapScript) ≠ some [1, 2, 7, 14] := by
  decide +kernel

def streamOut : Xsh.Syntax.RawName := name 293 (by decide)
def storedCount : Xsh.Syntax.RawName := name 294 (by decide)
def storedAgain : Xsh.Syntax.RawName := name 295 (by decide)

/-- `let out = count(2) |> map { |x| trace += [x]; x * 7 }`, then a stored
`count(2)` piped through `where { |x| x > 0 } |> count` and again through
`map { |x| x } |> collect()`, as the verification binary records `1, 20, 1,
21, 7` (the producer's notes, the stage's, the producer's `defer`), gives
`[0,7]`, and gives `1` and an empty list: the producer and the stage
alternate per item, and the piped stream is consumed. -/
def streamPipelineScript : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount,
   letOf streamOut (.structuredPipeline (.call (ident countFn) [.positional (int "2")])
     [mapStage (some xItem) [recordItem xItem, .expression (.binary .mul (ident xItem) (int "7"))]])]

/-- The witness world has one stream identity, so the stored stream runs in
a script of its own. -/
def storedPipelineScript : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount,
   letOf streamLocal (.call (ident countFn) [.positional (int "2")]),
   letOf storedCount (.structuredPipeline (ident streamLocal)
     [.stream .whereStage [] (some (some xItem, [.expression (.binary .gt (ident xItem) (int "0"))])),
      .stream .count [] none]),
   letOf storedAgain (.structuredPipeline (ident streamLocal) [mapStage (some xItem) [.expression (ident xItem)],
     .stream .collect [] none])]

theorem stream_pipeline_witness :
    readList traceVar (runCanonical streamPipelineScript) = some [1, 0, 1, 1, 7] ∧
      readList streamOut (runCanonical streamPipelineScript) = some [0, 7] ∧
      readInt storedCount (runCanonical storedPipelineScript) = some 1 ∧
      readList storedAgain (runCanonical storedPipelineScript) = some [] := by
  decide +kernel

/-- Collecting the stream before the stage would note both yields and the
producer's `defer` first; the model alternates them. -/
theorem collected_source_rejected :
    readList traceVar (runCanonical streamPipelineScript) ≠ some [1, 1, 7, 0, 1] := by
  decide +kernel

def takenVar : Xsh.Syntax.RawName := name 296 (by decide)

def take (lexeme : String) : Xsh.Syntax.PipeStage := .stream .take [.positional (int lexeme)] none

/-- `[1, 2, 7] |> map { |x| trace += [x]; x } |> take(2)`, as the
verification binary records `[1,2]` and gives `[1,2]`: the third item is
never mapped. -/
def listTakeScript : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []),
   letOf takenVar (.structuredPipeline (items ["1", "2", "7"])
     [mapStage (some xItem) [recordItem xItem, .expression (ident xItem)], take "2"])]

/-- `count(8) |> map { |x| trace += [x]; x } |> take(2)`, as the binary
records `1, 20, 1, 21, 7` and gives `[0,1]`: the producer stops after the
second item and its `defer` runs; and `count(8) |> take(0)` never runs it. -/
def streamTakeScript (count : String) : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount,
   letOf takenVar (.structuredPipeline (.call (ident countFn) [.positional (int "8")])
     ((if count = "0" then [] else [mapStage (some xItem) [recordItem xItem, .expression (ident xItem)]]) ++
       [take count]))]

theorem take_witness :
    readList traceVar (runCanonical listTakeScript) = some [1, 2] ∧
      readList takenVar (runCanonical listTakeScript) = some [1, 2] ∧
      readList traceVar (runCanonical (streamTakeScript "2")) = some [1, 0, 1, 1, 7] ∧
      readList takenVar (runCanonical (streamTakeScript "2")) = some [0, 1] ∧
      readList traceVar (runCanonical (streamTakeScript "0")) = some [] := by
  decide +kernel

/-- A `take` that let the producer run on would note all eight yields;
one that mapped every list item before cutting would note `7`. -/
theorem eager_take_rejected :
    readList traceVar (runCanonical (streamTakeScript "2")) ≠ some [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 7] ∧
      readList traceVar (runCanonical listTakeScript) ≠ some [1, 2, 7] := by
  decide +kernel

def okList : Xsh.Syntax.RawName := name 297 (by decide)
def errList : Xsh.Syntax.RawName := name 298 (by decide)

/-- Locals holding `Ok([1, 2])` and `Err("boom")`. -/
def resultHolding : Frame witnessWorld :=
  { start with locals := [(okList, .ok (.list [.scalar (.integer ⟨1, by decide⟩), .scalar (.integer ⟨2, by decide⟩)])),
      (errList, .err (.scalar (.string "boom")))] }

def countOf (source : Xsh.Syntax.RawName) : List Xsh.Syntax.Statement :=
  [letOf countVar (.structuredPipeline (ident source) [.stream .count [] none])]

def raisedBoom : Exec witnessWorld → Bool
  | .failed (.raised (.scalar (.string "boom"))) _ => true
  | _ => false

/-- `Ok([1, 2]) |> count()` and `Err(boom) |> count()`: as the verification
binary counts `2` items through an `Ok` source and fails with the `Err`'s
error at the pipeline, the model removes one outer `Result`. -/
theorem result_source_witness :
    readInt countVar (runStatements machine 40 resultHolding (countOf okList)) = some 2 ∧
      raisedBoom (runStatements machine 40 resultHolding (countOf errList)) = true := by
  decide +kernel

/-- Treating a `Result` source as a one-item collection would count `1`. -/
theorem result_as_item_rejected :
    readInt countVar (runStatements machine 40 resultHolding (countOf okList)) ≠ some 1 := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Pipelines
