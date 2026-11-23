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
value's list on in turn. Every item reaches a `sort`, `sort-by` or `batch`
before any leaves it: the stages before it run over every item, it orders
them stably or cuts batches, and the stages after continue from its list.
Each `drop(n)` argument is evaluated once, in stage
order, before any item is pulled, and the stage becomes `n` stages that each
drop one item and leave; an `enumerate` passes `{index, value}` on and counts
one more; each item leaves the stages as the next item finds them, so these
counts hold across items. A `take(n)` ending the transformations is evaluated
once after them, before the source is pulled and stops pulling once `n` items have
reached it: a list's later items are not run, and a producer is stopped as
by its consumer's `break`, so its `defer`s run. The pipeline gives the list
of items that reach its end, or their count under a closing `count()`.
Whatever it gives, it leaves the environment and locals as it found them and
keeps every cell already in the store. -/
theorem structured_pipelines [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ input stages before call after, valueStage stages = some (before, call, after) →
      evalExpr machine (fuel + 1) frame (.structuredPipeline input stages) =
        (evalExpr machine fuel frame (if before.isEmpty then input else .structuredPipeline input before)).bind
          fun piped next =>
            (evalExpr machine fuel { next with piped := some piped } (valueStageCall call)).bind fun called last =>
              if after.isEmpty then .value called last
              else evalExpr machine fuel { last with piped := some called } (.structuredPipeline .valuePipelineHole after)) ∧
    (∀ (method : Xsh.Syntax.RawName) arguments, arguments.any isHoleArgument = false →
      valueStageCall (.call (.ident method) arguments) = .call (.field .valuePipelineHole method) arguments) ∧
    (∀ callee arguments, arguments.any isHoleArgument = true →
      valueStageCall (.call callee arguments) = .call callee arguments) ∧
    (∀ input stages, valueStage stages = none → evalExpr machine (fuel + 1) frame (.structuredPipeline input stages) =
      (evalExpr machine fuel frame input).bind fun result next =>
        match pipelineSource result with
        | .error error => .exec (.failed (.raised error) next)
        | .ok source =>
          (prepareStages (evalExpr machine fuel) next (pipelineSplit stages).1).bind fun prepared ready =>
          (pipelineLimit (evalExpr machine fuel) ready (pipelineSplit stages).2.1).bind fun explicit after =>
            let limit := bothLimits explicit (endingLimit (pipelineSplit stages).2.2)
            match pendingStream machine after source, limit with
            | some (index, some _), some 0 => closePipeline machine.context (evalExpr machine fuel) (handlerScope machine fuel)
                (pipelineSplit stages).2.2 (.list []) (after.consume index)
            | some (index, some (closure, supplied)), _ =>
              if (pipelineSplit stages).1.any isStateful then .exec .unsupported else
              (gathered after index (invokeSupplied machine (evalExpr machine fuel) (callBlock machine fuel)
                  { after with
                    streams := after.streams.set index (.gathering [])
                    consumers := { sinkConsumer after index with stages := (pipelineSplit stages).1, limit } ::
                      after.consumers }
                  closure supplied)).bind fun out last => closePipeline machine.context (evalExpr machine fuel) (handlerScope machine fuel)
                (pipelineSplit stages).2.2 out last
            | some (_, none), _ => closePipeline machine.context (evalExpr machine fuel) (handlerScope machine fuel)
                (pipelineSplit stages).2.2 (.list []) after
            | none, _ =>
              (runPipeline machine.context
                  (fun current body bound items => runSegments machine.context (evalExpr machine fuel) (handlerScope machine fuel)
                    (stageItems machine fuel) (body.length + 1) current body bound items)
                  after source prepared .collected limit).bind
                fun out last => closePipeline machine.context (evalExpr machine fuel) (handlerScope machine fuel)
                  (pipelineSplit stages).2.2 out last) ∧
    (∀ source body ending limit items, listValues source = some items →
      runPipeline machine.context (stageItems machine fuel) frame source body ending limit =
        (stageItems machine fuel frame body limit items).bind fun out last =>
          finishPipeline machine.context ending out last) ∧
    (∀ stages items, stageItems machine (fuel + 1) frame stages (some 0) items =
      .value (.list []) { frame with restaged := some stages }) ∧
    (∀ stages item rest limit, limit ≠ some 0 → stageItems machine (fuel + 1) frame stages limit (item :: rest) =
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
    (∀ item, stageItem machine (fuel + 1) frame [] item = .value (.list [item]) { frame with restaged := some [] }) ∧
    (∀ (parameter : Option Xsh.Syntax.RawName) body rest item,
      stageAction (.stream .map [] (some (parameter.toList, body))) = some (.replace, parameter, body) ∧
      stageAction (.stream .whereStage [] (some (parameter.toList, body))) = some (.keep, parameter, body) ∧
      stageAction (.stream .flatMap [] (some (parameter.toList, body))) = some (.expand, parameter, body) ∧
      stageItem machine (fuel + 1) frame (.stream .map [] (some (parameter.toList, body)) :: rest) item =
        restageWith (.stream .map [] (some (parameter.toList, body))) rest
          ((handlerScope machine fuel frame (parameter.elim (some item) fun _ => none)
              (parameter.toList.map fun name => (name, item)) body).bind fun result after =>
            stageItem machine fuel after rest result)) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) stage rest,
      prepareStages evaluate frame (stage :: rest) =
        (prepareStage evaluate frame stage).bind fun first after =>
          (prepareStages evaluate after rest).bind fun later last => .value (first ++ later) last) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) count,
      prepareStage evaluate frame (.stream .drop [.positional count] none) =
        (evaluate frame count).bind fun value after =>
          match value.eraseNominal with
          | .scalar (.integer bound) => .value (List.replicate bound.val.toNat (.stream .drop [] none)) after
          | _ => .exec .stuck) ∧
    (∀ rest item, stageItem machine (fuel + 1) frame (.stream .drop [] none :: rest) item =
      .value (.list []) { frame with restaged := some rest }) ∧
    (∀ init accumulator item body before, pipelineSplit (before ++ [.stream .fold [.positional init]
        (some ([accumulator, item], body))]) = (before, none, .folded init accumulator item body) ∨
      (∃ count, before.getLast? = some (.stream .take [.positional count] none))) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) scope initial accumulator item body out last items,
      listValues out = some items →
        closePipeline machine.context evaluate scope (.folded initial accumulator item body) out last =
          (evaluate last initial).bind fun start after => foldValues scope accumulator item body start after items) ∧
    (∀ (scope : Frame world → Option (Value Declaration Identity Source) →
          List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world)
        accumulator item body current next rest,
      foldValues scope accumulator item body current frame (next :: rest) =
        (scope frame none [(accumulator, current), (item, next)] body).bind fun updated after =>
          foldValues scope accumulator item body updated after rest) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
        (scope : Frame world → Option (Value Declaration Identity Source) →
          List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world)
        (run : Frame world → List Xsh.Syntax.PipeStage → Option Nat → List (Value Declaration Identity Source) → Valued world)
        budget stages limit items before stage after,
      splitMaterializing stages = some (before, stage, after) →
        runSegments machine.context evaluate scope run (budget + 1) frame stages limit items =
          (run frame before none items).bind fun gathered last =>
            match listValues gathered with
            | some reached =>
              (materialize machine.context evaluate scope stage last reached).bind fun arranged next =>
                match listValues arranged with
                | some rearranged => runSegments machine.context evaluate scope run budget next after limit rearranged
                | none => .exec .stuck
            | none => .exec .stuck) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
        (scope : Frame world → Option (Value Declaration Identity Source) →
          List (Xsh.Syntax.RawName × Value Declaration Identity Source) → List Xsh.Syntax.Statement → Valued world) items,
      materialize machine.context evaluate scope (.stream .sort [] none) frame items =
        orderedBy machine.context false (items.map fun item => (item, item)) frame) ∧
    (∀ counted, statefulStep (Declaration := Declaration) (Identity := Identity) (Source := Source)
      (.stream .enumerate counted none) = (countValue counted.length).map (.enumerateAt counted)) ∧
    (∀ (outcome : Valued world) stage rest value last, outcome = .value value last →
      restageWith stage rest outcome = .value value { last with restaged := some (stage :: last.restaged.getD rest) }) ∧
    (∀ input stages, VRestores frame (evalExpr machine fuel frame (.structuredPipeline input stages))) ∧
    (∀ input stages, VKeeps frame.store (evalExpr machine fuel frame (.structuredPipeline input stages))) := by
  refine ⟨fun input stages before call after staged => by simp only [evalExpr, staged],
    fun method arguments unheld => by simp [valueStageCall, unheld],
    fun callee arguments held => by simp [valueStageCall, held],
    fun input stages streamOnly => ?_, fun source body ending limit items listed => by simp only [runPipeline, listed],
    fun stages items => by cases items <;> simp [stageItems],
    fun stages item rest limit nonzero => ?_,
    fun _ _ => ⟨rfl, rfl⟩,
    fun bound count reached => by simp [afterItems, reached],
    fun gathering index bound out items gathered listed held reached => by
      simp only [Frame.gatherAll, listed, held, reached, if_true],
    fun loop last index items held => by simp only [gathered, held],
    fun _ => by simp [stageItem], fun parameter _ _ _ => by
      cases parameter <;> exact ⟨rfl, rfl, rfl, by simp [stageItem, stageAction, singleParameter]⟩,
    fun _ _ _ => rfl, fun _ _ => rfl,
    fun _ _ => by simp [stageItem, stageAction, statefulStep],
    fun init accumulator item body before => ?_,
    fun _ _ _ _ _ _ _ _ _ listed => by simp only [closePipeline, listed],
    fun _ _ _ _ _ _ _ => rfl,
    fun _ _ _ _ _ _ _ _ _ _ split => by rw [runSegments.eq_2, split]; rfl, fun _ _ _ => rfl,
    fun _ => rfl,
    fun _ _ _ _ _ given => by subst given; rfl,
    fun input stages => (restoring machine fuel).expr frame _, fun input stages => (persisting machine fuel).expr frame _⟩
  · simp only [evalExpr, streamOnly]
    refine congrArg _ (funext fun result => funext fun next => ?_)
    cases sourced : pipelineSource result with
    | error error => simp only [sourced]
    | ok source =>
      simp only [sourced]
      refine congrArg _ (funext fun prepared => funext fun ready => ?_)
      refine congrArg _ (funext fun explicit => funext fun after => ?_)
      rcases pendingStream machine after source with _ | ⟨index, _ | ⟨closure, supplied⟩⟩ <;>
        rcases bothLimits explicit (endingLimit (pipelineSplit stages).2.2) with _ | _ | bound <;> rfl
  · rcases limit with _ | _ | bound
    · rfl
    · exact absurd rfl nonzero
    · rfl
  · unfold pipelineSplit
    simp only [List.getLast?_append, List.getLast?_singleton, Option.some_or, List.dropLast_concat]
    split
    · next taken => exact Or.inr ⟨_, taken⟩
    · exact Or.inl rfl

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
  .stream .map [] (some (parameter.toList, body))

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
     [.stream .whereStage [] (some ([xItem], [.expression (.binary .gt (ident xItem) (int "1"))])),
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
     [.stream .whereStage [] (some ([xItem], [.expression (.binary .gt (ident xItem) (int "0"))])),
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

def closingVar : Xsh.Syntax.RawName := name 299 (by decide)

def closing (source : Xsh.Syntax.Expr) (stages : List Xsh.Syntax.PipeStage) : List Xsh.Syntax.Statement :=
  [.binding .var (.name traceVar) none (.list []), declareCount, letOf closingVar (.structuredPipeline source stages)]

def readValue (outcome : Exec witnessWorld) : Option (Value Unit Unit Unit) :=
  match outcome with
  | .normal frame =>
    match frame.environment.cellOf closingVar with
    | .ok cell => (frame.store cell).map fun found => found.contents.value
    | .error _ => none
  | _ => none

def intValue : Option (Value Unit Unit Unit) → Option Int
  | some (.scalar (.integer value)) => some value.val
  | some (.ok (.scalar (.integer value))) => some value.val
  | _ => none

def errText : Option (Value Unit Unit Unit) → Option String
  | some (.err (.scalar (.string text))) => some text
  | _ => none

def boolValue : Option (Value Unit Unit Unit) → Option Bool
  | some (.scalar (.boolean value)) => some value
  | _ => none

def noted (test : Xsh.Syntax.Expr) : Option (List Xsh.Syntax.RawName × List Xsh.Syntax.Statement) :=
  some ([xItem], [recordItem xItem, .expression test])

/-- As the verification binary gives `11` for `[1, 2, 8] |> sum()`, `Ok(7)`
for `[7, 8] |> first()`, the `Err` "stream was empty" for an empty list's
`first()`, and `true` for `any`: `sum()` adds the items with checked `+`,
`first()` takes one item, and `any` over `count(8)` stops the producer once
an item passes, so the producer notes two yields and runs its `defer`; `all`
over `[1, 2, 7]` stops at the first item that fails, never testing `7`. -/
theorem closing_stage_witness :
    intValue (readValue (runCanonical (closing (items ["1", "2", "8"]) [.stream .sum [] none]))) = some 11 ∧
    intValue (readValue (runCanonical (closing (items ["7", "8"]) [.stream .first [] none]))) = some 7 ∧
    errText (readValue (runCanonical (closing (.list []) [.stream .first [] none]))) = some "stream was empty" ∧
    boolValue (readValue (runCanonical (closing (.call (ident countFn) [.positional (int "8")])
      [.stream .any [] (noted (.binary .gt (ident xItem) (int "0")))]))) = some true ∧
    readList traceVar (runCanonical (closing (.call (ident countFn) [.positional (int "8")])
      [.stream .any [] (noted (.binary .gt (ident xItem) (int "0")))])) = some [1, 0, 1, 1, 7] ∧
    boolValue (readValue (runCanonical (closing (items ["1", "2", "7"])
      [.stream .all [] (noted (.binary .lt (ident xItem) (int "2")))]))) = some false ∧
    readList traceVar (runCanonical (closing (items ["1", "2", "7"])
      [.stream .all [] (noted (.binary .lt (ident xItem) (int "2")))])) = some [1, 2] := by
  decide +kernel

/-- An `any` that tested every item would note all eight yields. -/
theorem exhaustive_any_rejected :
    readList traceVar (runCanonical (closing (.call (ident countFn) [.positional (int "8")])
      [.stream .any [] (noted (.binary .gt (ident xItem) (int "0")))])) ≠ some [1, 0, 1, 1, 1, 2] := by
  decide +kernel

/-- As the verification binary gives `1 7 1` for `min()`, `max()` and `last()`
over `[2, 7, 1]` and the `Err` "stream was empty" for an empty `max()`. -/
theorem extremum_witness :
    intValue (readValue (runCanonical (closing (items ["2", "7", "1"]) [.stream .min [] none]))) = some 1 ∧
    intValue (readValue (runCanonical (closing (items ["2", "7", "1"]) [.stream .max [] none]))) = some 7 ∧
    intValue (readValue (runCanonical (closing (items ["2", "7", "1"]) [.stream .last [] none]))) = some 1 ∧
    errText (readValue (runCanonical (closing (.list []) [.stream .max [] none]))) = some "stream was empty" := by
  decide +kernel

/-- A `max()` that kept its first item, as an ordering that compared nothing
would, gives `2`; the model gives `7`. -/
theorem unordered_extremum_rejected :
    intValue (readValue (runCanonical (closing (items ["2", "7", "1"]) [.stream .max [] none]))) ≠ some 2 := by
  decide +kernel

def drop (count : Xsh.Syntax.Expr) : Xsh.Syntax.PipeStage := .stream .drop [.positional count] none

def enumerated : Xsh.Syntax.PipeStage := .stream .enumerate [] none

/-- The `{index, value}` records a pipeline gave, as index and value pairs. -/
def indexed : Option (Value Unit Unit Unit) → Option (List (Int × Int))
  | some (.list items) => items.mapM fun
    | .record [("index", .scalar (.integer index)), ("value", .scalar (.integer value))] => some (index.val, value.val)
    | _ => none
  | _ => none

/-- `[1, 2, 7, 8] |> map { trace += [.]; . * 2 } |> drop({ trace += [0]; 1 })
|> map { |x| trace += [x]; x + 1 }`. -/
def dropTraced : List Xsh.Syntax.Statement :=
  closing (items ["1", "2", "7", "8"])
    [mapStage none [.assign (.name traceVar) .add (.list [.item]), .expression (.binary .mul .item (int "2"))],
     drop (.valueBlock [.assign (.name traceVar) .add (.list [int "0"]), .expression (int "1")]),
     mapStage (some xItem) [recordItem xItem, .expression (.binary .add (ident xItem) (int "1"))]]

/-- As the verification binary gives `[21,31,41]` and logs `dmmnmnmn` for the
same shape over `[1, 2, 3, 4]`, its `drop` argument logging `d`: the
argument is evaluated before any item, and each kept item passes every
stage before the next is pulled. It gives
`[{"index":0,"value":"x"},{"index":1,"value":"y"}]` for
`["x", "y"] |> enumerate`, indices `1` and `2` for `enumerate |> drop(1)`
over three items, no items for `drop(5)` over three, all three for
`drop(-1)`, and `[1,2]` for `drop(0) |> take(2)`. -/
theorem drop_enumerate_witness :
    readList closingVar (runCanonical dropTraced) = some [5, 15, 17] ∧
    readList traceVar (runCanonical dropTraced) = some [0, 1, 2, 4, 7, 14, 8, 16] ∧
    indexed (readValue (runCanonical (closing (items ["7", "8"]) [enumerated]))) = some [(0, 7), (1, 8)] ∧
    indexed (readValue (runCanonical (closing (items ["1", "2", "7"]) [enumerated, drop (int "1")]))) =
      some [(1, 2), (2, 7)] ∧
    readList closingVar (runCanonical (closing (items ["1", "2", "8"]) [drop (int "7")])) = some [] ∧
    readList closingVar (runCanonical (closing (items ["1", "2", "8"]) [drop (.unary .neg (int "1"))])) =
      some [1, 2, 8] ∧
    readList closingVar (runCanonical (closing (items ["1", "2", "8"]) [drop (int "0"), take "2"])) = some [1, 2] := by
  decide +kernel

/-- A `drop` applied after the stages ran on every item, as a pipeline that
materialized before it would, logs every first-stage item before any second
one: `0 1 2 7 8 4 14 16`. -/
theorem materialized_drop_rejected :
    readList traceVar (runCanonical dropTraced) ≠ some [0, 1, 2, 7, 8, 4, 14, 16] := by
  decide +kernel

def foldVar : Xsh.Syntax.RawName := name 369 (by decide)
def accName : Xsh.Syntax.RawName := name 370 (by decide)

def foldStage (kind : Xsh.Syntax.StreamStageKind) (initial : Xsh.Syntax.Expr) (body : List Xsh.Syntax.Statement) :
    Xsh.Syntax.PipeStage :=
  .stream kind [.positional initial] (some ([accName, xItem], body))

/-- `[1, 2, 7] |> map { trace += [.]; . } |> fold({ trace += [0]; 8 }) { |acc, x| trace += [x]; acc + x }`. -/
def foldTraced : List Xsh.Syntax.Statement :=
  closing (items ["1", "2", "7"])
    [mapStage none [.assign (.name traceVar) .add (.list [.item]), .expression .item],
     foldStage .fold (.valueBlock [.assign (.name traceVar) .add (.list [int "0"]), .expression (int "8")])
       [recordItem xItem, .expression (.binary .add (ident accName) (ident xItem))]]

/-- As the verification binary gives `20` and logs `mmmifff` for the same
shape with an initial value of `10` that logs `i`: every item reaches the
`fold` before its initial value is evaluated, then the callback folds them in
order. It gives `14` for `[1, 2, 7] |> reduce(1) { |acc, x| acc * x }`, `9`
for `[8] |> reduce(1) { |acc, x| acc + x }` and `5` for an empty
`fold(5)`. -/
theorem fold_witness :
    intValue (readValue (runCanonical foldTraced)) = some 18 ∧
    readList traceVar (runCanonical foldTraced) = some [1, 2, 7, 0, 1, 2, 7] ∧
    intValue (readValue (runCanonical (closing (items ["1", "2", "7"])
      [foldStage .reduce (int "1") [.expression (.binary .mul (ident accName) (ident xItem))]]))) = some 14 ∧
    intValue (readValue (runCanonical (closing (items ["8"])
      [foldStage .reduce (int "1") [.expression (.binary .add (ident accName) (ident xItem))]]))) = some 9 ∧
    intValue (readValue (runCanonical (closing (.list [])
      [foldStage .fold (int "7") [.expression (.binary .add (ident accName) (ident xItem))]]))) = some 7 := by
  decide +kernel

/-- A `fold` that evaluated its initial value before pulling, as a lazy one
might, logs `0` first. -/
theorem eager_fold_initial_rejected :
    readList traceVar (runCanonical foldTraced) ≠ some [0, 1, 1, 2, 2, 7, 7] := by
  decide +kernel

def descName : Xsh.Syntax.RawName := name 371 (by decide)
def batchCountName : Xsh.Syntax.RawName := name 372 (by decide)

/-- The canonical script machine, with `desc` and `count` spelled for stage
options. -/
def optionMachine : Machine witnessWorld :=
  { scriptMachine with
    context := { scriptMachine.context with
      spelling := fun named =>
        if named = descName then "desc" else if named = batchCountName then "count"
        else scriptMachine.context.spelling named } }

def runOptions (script : List Xsh.Syntax.Statement) : Exec witnessWorld := runScript optionMachine 90 fullStart script

/-- The integer lists a pipeline gave. -/
def readLists (outcome : Exec witnessWorld) : Option (List (List Int)) :=
  match readValue outcome with
  | some (.list items) => items.mapM fun
    | .list inner => inner.mapM fun
      | .scalar (.integer value) => some value.val
      | _ => none
    | _ => none
  | _ => none

def keyBlock (key : Xsh.Syntax.Expr) : Option (List Xsh.Syntax.RawName × List Xsh.Syntax.Statement) :=
  some ([], [.expression key])

/-- `[7, 1, 8, 2] |> map { trace += [.]; . } |> sort |> map { |x| trace += [x]; x }`. -/
def sortTraced : List Xsh.Syntax.Statement :=
  closing (items ["7", "1", "8", "2"])
    [mapStage none [.assign (.name traceVar) .add (.list [.item]), .expression .item],
     .stream .sort [] none,
     mapStage (some xItem) [recordItem xItem, .expression (ident xItem)]]

/-- As the verification binary gives `[1,2,7,8]` and logs `mmmmnnnn` for the
same shape: every item reaches `sort` before any leaves it. It gives
`[8,7,2]` for `[8, 2, 7] |> sort-by { 0 - . }`, `[2,1,2]` for the stable
`sort-by { 0 }`, `[[2,7],[1,7],[8,0]]` for
`sort-by(desc: true) { .[1] }` over `[[1, 7], [2, 7], [8, 0]]`, the reverse of
the stable order, and `[[1,2],[7,8],[1]]` for `batch(count: 2)` over
`[1, 2, 7, 8, 1]`. -/
theorem materializing_witness :
    readList closingVar (runCanonical sortTraced) = some [1, 2, 7, 8] ∧
    readList traceVar (runCanonical sortTraced) = some [7, 1, 8, 2, 1, 2, 7, 8] ∧
    readList closingVar (runCanonical (closing (items ["8", "2", "7"])
      [.stream .sortBy [] (keyBlock (.binary .sub (int "0") .item))])) = some [8, 7, 2] ∧
    readList closingVar (runCanonical (closing (items ["2", "1", "2"]) [.stream .sortBy [] (keyBlock (int "0"))])) =
      some [2, 1, 2] ∧
    readLists (runOptions (closing (.list [items ["1", "7"], items ["2", "7"], items ["8", "0"]])
      [.stream .sortBy [.named descName (.literal (.bool true))] (keyBlock (.index .item (int "1") false))])) =
      some [[2, 7], [1, 7], [8, 0]] ∧
    readLists (runOptions (closing (items ["1", "2", "7", "8", "1"])
      [.stream .batch [.named batchCountName (int "2")] none])) = some [[1, 2], [7, 8], [1]] := by
  decide +kernel

/-- A `sort` that passed each item on as it arrived, as a serial stage
would, logs `7 7 1 1 8 8 2 2`. -/
theorem serial_sort_rejected :
    readList traceVar (runCanonical sortTraced) ≠ some [7, 7, 1, 1, 8, 8, 2, 2] := by
  decide +kernel

/-- The `{items, key}` records a `group-by` gave, as key and item lists. -/
def readGroups (outcome : Exec witnessWorld) : Option (List (Int × List Int)) :=
  match readValue outcome with
  | some (.list groups) => groups.mapM fun
    | .record [("items", .list members), ("key", .scalar (.integer key))] =>
      (members.mapM fun (member : Value Unit Unit Unit) =>
        match member with
        | .scalar (.integer value) => some value.val
        | _ => none).map (key.val, ·)
    | _ => none
  | _ => none

/-- The `{left, right}` records a `zip` gave. -/
def readPairs (outcome : Exec witnessWorld) : Option (List (Int × Int)) :=
  match readValue outcome with
  | some (.list pairs) => pairs.mapM fun
    | .record [("left", .scalar (.integer left)), ("right", .scalar (.integer right))] => some (left.val, right.val)
    | _ => none
  | _ => none

/-- `[1, 2, 1, 7] |> map { trace += [.]; . } |> unique-by { . } |> map { |x| trace += [x]; x }`. -/
def uniqueTraced : List Xsh.Syntax.Statement :=
  closing (items ["1", "2", "1", "7"])
    [mapStage none [.assign (.name traceVar) .add (.list [.item]), .expression .item],
     .stream .uniqueBy [] (keyBlock .item),
     mapStage (some xItem) [recordItem xItem, .expression (ident xItem)]]

/-- As the verification binary gives `[1,2,7]` and logs `mmmmkkkknnn` for
`unique-by` over `[1, 2, 1, 7]`, every item reaching it first;
`[{"items":[1,7],"key":1},{"items":[2,8],"key":0}]` for
`[1, 2, 7, 8] |> group-by { . % 2 }`; `[1,2,1,2]` for `[1, 2] |> repeat(2)`;
and `{left, right}` pairs up to the shorter list for `zip`. -/
theorem keyed_stage_witness :
    readList closingVar (runCanonical uniqueTraced) = some [1, 2, 7] ∧
    readList traceVar (runCanonical uniqueTraced) = some [1, 2, 1, 7, 1, 2, 7] ∧
    readGroups (runCanonical (closing (items ["1", "2", "7", "8"])
      [.stream .groupBy [] (keyBlock (.binary .rem .item (int "2")))])) = some [(1, [1, 7]), (0, [2, 8])] ∧
    readList closingVar (runCanonical (closing (items ["1", "2"]) [.stream .repeatStage [.positional (int "2")] none])) =
      some [1, 2, 1, 2] ∧
    readPairs (runCanonical (closing (items ["1", "2", "7"]) [.stream .zip [.positional (items ["8", "0"])] none])) =
      some [(1, 8), (2, 0)] := by
  decide +kernel

/-- A `unique-by` that kept the last item per key would give `2 1 7`. -/
theorem last_per_key_rejected : readList closingVar (runCanonical uniqueTraced) ≠ some [2, 1, 7] := by
  decide +kernel

/-- A map of text keys to integer counts. -/
def readCounts (outcome : Exec witnessWorld) : Option (List (String × Int)) :=
  match readValue outcome with
  | some (.map entries) => entries.mapM fun
    | (.string key, .scalar (.integer count)) => some (key, count.val)
    | _ => none
  | _ => none

/-- `[1, 2] |> tee { trace += [.] } |> map { |x| trace += [x]; x }`. -/
def teeTraced : List Xsh.Syntax.Statement :=
  closing (items ["1", "2"])
    [.stream .tee [] (some ([], [.assign (.name traceVar) .add (.list [.item])])),
     mapStage (some xItem) [recordItem xItem, .expression (ident xItem)]]

/-- `[1, 2] |> map { trace += [.]; . } |> each { |x| trace += [x] }`. -/
def eachTraced : List Xsh.Syntax.Statement :=
  closing (items ["1", "2"])
    [mapStage none [.assign (.name traceVar) .add (.list [.item]), .expression .item],
     .stream .each [] (some ([xItem], [recordItem xItem]))]

/-- As the verification binary logs `tntn` for `tee |> map` and `meme` for
`map |> each`, each item passing both before the next, gives `[1,2]` through
the `tee`, and gives `{"2":1,"7":2}`, `{"false":1,"true":2}` and
`{"a":1,"b":2}` for `count { . }` over `[7, 2, 7]`, `[true, false, true]` and
`["b", "a", "b"]`: keys by their text, in key order. -/
theorem tee_each_count_witness :
    readList closingVar (runCanonical teeTraced) = some [1, 2] ∧
    readList traceVar (runCanonical teeTraced) = some [1, 1, 2, 2] ∧
    readList traceVar (runCanonical eachTraced) = some [1, 1, 2, 2] ∧
    (match readValue (runCanonical eachTraced) with | some (.scalar .unit) => true | _ => false) = true ∧
    readCounts (runCanonical (closing (items ["7", "2", "7"]) [.stream .count [] (keyBlock .item)])) =
      some [("2", 1), ("7", 2)] ∧
    readCounts (runCanonical (closing (.list [.literal (.bool true), .literal (.bool false), .literal (.bool true)])
      [.stream .count [] (keyBlock .item)])) = some [("false", 1), ("true", 2)] ∧
    readCounts (runCanonical (closing (.list [.literal (.string "b"), .literal (.string "a"), .literal (.string "b")])
      [.stream .count [] (keyBlock .item)])) = some [("a", 1), ("b", 2)] := by
  decide +kernel

/-- An `each` run after every item passed the earlier stages, as a
materializing stage would, logs `1 2 1 2`. -/
theorem materialized_each_rejected : readList traceVar (runCanonical eachTraced) ≠ some [1, 2, 1, 2] := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Pipelines
