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
stage |> ...` over a list source passes each item through every stage before
the next item starts; a stage block binds the item to its parameter or,
without one, to `.`, in its own scope, and its tail is its value; `where`
keeps the item when that value is `true`, `map` replaces it and `flat-map`
passes each item of the value's list on in turn; the pipeline gives the list
of items that reach its end, or their count under a closing `count()`.
Whatever it gives, it leaves the environment and locals as it found them and
keeps every cell already in the store. -/
theorem structured_pipelines [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ input stages, evalExpr machine (fuel + 1) frame (.structuredPipeline input stages) =
      (evalExpr machine fuel frame input).bind fun source next => runPipeline (stageItems machine fuel) next source stages) ∧
    (∀ source stages items, listValues source = some items →
      runPipeline (stageItems machine fuel) frame source stages =
        (stageItems machine fuel frame (pipelineSplit stages).1 items).bind fun out last =>
          finishPipeline (pipelineSplit stages).2 out last) ∧
    (∀ stages item rest, stageItems machine (fuel + 1) frame stages (item :: rest) =
      (stageItem machine fuel frame stages item).bind fun first after =>
        (stageItems machine fuel after stages rest).bind fun later last =>
          match first, later with
          | .list head, .list tail => .value (.list (head ++ tail)) last
          | _, _ => .exec .stuck) ∧
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
  refine ⟨fun _ _ => rfl, fun source stages items listed => by simp only [runPipeline, listed],
    fun _ _ _ => rfl, fun _ => rfl, fun _ _ _ _ => ⟨rfl, rfl, rfl, rfl⟩,
    fun input stages => (restoring machine fuel).expr frame _, fun input stages => (persisting machine fuel).expr frame _⟩

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

end Witness

end Xsh.Proofs.Core.Pipelines
