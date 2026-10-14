/-!
This fragment models finite, loop-local control flow over an already chosen
sequence of items. It records each visited item and body outcome, consumes
breaks and continues aimed at the current loop, and propagates other targets,
returns, and failures. It does not prove checker target validity or model stage
callbacks, divergence, effects, cleanup, streams, or Rust correspondence.
-/

namespace Xsh.Loops.ControlFlow

/-- A loop either completes as a statement or yields its break value. -/
private inductive LoopCompletion (Value : Type) where
  | unit
  | value (payload : Value)
  deriving DecidableEq

/-- The control outcome produced by one loop-body item. -/
private inductive BodyStep (BreakValue ReturnValue Error : Type) where
  | next
  | continueTo (target : Nat)
  | breakTo (target : Nat) (value : LoopCompletion BreakValue)
  | returnFromLoop (value : ReturnValue)
  | failFromLoop (error : Error)
  deriving DecidableEq

/-- A loop consumes transfers aimed at itself and exposes every other exit. -/
private inductive Outcome (BreakValue ReturnValue Error : Type) where
  | completed (value : LoopCompletion BreakValue)
  | continueTo (target : Nat)
  | breakTo (target : Nat) (value : LoopCompletion BreakValue)
  | returned (value : ReturnValue)
  | failed (error : Error)
  deriving DecidableEq

/-- Each finite execution pairs its final outcome with the visited body steps. -/
private structure Evaluation (Item BreakValue ReturnValue Error : Type) where
  outcome : Outcome BreakValue ReturnValue Error
  trace : List (Item × BodyStep BreakValue ReturnValue Error)
  deriving DecidableEq

/-- Run a body over a finite item list, stopping at the first nonlocal exit. -/
private def runLoop {Item BreakValue ReturnValue Error : Type}
    (target : Nat) (items : List Item)
    (body : Item → BodyStep BreakValue ReturnValue Error) :
    Evaluation Item BreakValue ReturnValue Error :=
  match items with
  | [] => ⟨.completed .unit, []⟩
  | item :: rest =>
      let step := body item
      match step with
      | .next =>
          let later := runLoop target rest body
          { later with trace := (item, step) :: later.trace }
      | .continueTo destination =>
          if destination == target then
            let later := runLoop target rest body
            { later with trace := (item, step) :: later.trace }
          else
            ⟨.continueTo destination, [(item, step)]⟩
      | .breakTo destination value =>
          if destination == target then
            ⟨.completed value, [(item, step)]⟩
          else
            ⟨.breakTo destination value, [(item, step)]⟩
      | .returnFromLoop value => ⟨.returned value, [(item, step)]⟩
      | .failFromLoop error => ⟨.failed error, [(item, step)]⟩

/-- A nested execution retains each inner result alongside the outer result. -/
private structure NestedEvaluation
    (Outer Inner BreakValue ReturnValue Error : Type) where
  inner : List (Outer × Evaluation Inner BreakValue ReturnValue Error)
  outer : Evaluation Outer BreakValue ReturnValue Error
  deriving DecidableEq

/-- Run an inner loop at each outer item, translating only its unhandled exits
back into body steps for the outer loop. -/
private def runNested {Outer Inner BreakValue ReturnValue Error : Type}
    (outerTarget innerTarget : Nat) (outerItems : List Outer)
    (innerItems : List Inner)
    (innerBody : Outer → Inner → BodyStep BreakValue ReturnValue Error) :
    NestedEvaluation Outer Inner BreakValue ReturnValue Error :=
  match outerItems with
  | [] => ⟨[], ⟨.completed .unit, []⟩⟩
  | outer :: rest =>
      let inner := runLoop innerTarget innerItems (innerBody outer)
      let step := match inner.outcome with
        | .completed _ => BodyStep.next
        | .continueTo destination => .continueTo destination
        | .breakTo destination value => .breakTo destination value
        | .returned value => .returnFromLoop value
        | .failed error => .failFromLoop error
      match step with
      | .next =>
          let later := runNested outerTarget innerTarget rest innerItems innerBody
          { inner := (outer, inner) :: later.inner
            outer := { later.outer with trace := (outer, step) :: later.outer.trace } }
      | .continueTo destination =>
          if destination == outerTarget then
            let later := runNested outerTarget innerTarget rest innerItems innerBody
            { inner := (outer, inner) :: later.inner
              outer := { later.outer with trace := (outer, step) :: later.outer.trace } }
          else
            ⟨[(outer, inner)], ⟨.continueTo destination, [(outer, step)]⟩⟩
      | .breakTo destination value =>
          if destination == outerTarget then
            ⟨[(outer, inner)], ⟨.completed value, [(outer, step)]⟩⟩
          else
            ⟨[(outer, inner)], ⟨.breakTo destination value, [(outer, step)]⟩⟩
      | .returnFromLoop value =>
          ⟨[(outer, inner)], ⟨.returned value, [(outer, step)]⟩⟩
      | .failFromLoop error =>
          ⟨[(outer, inner)], ⟨.failed error, [(outer, step)]⟩⟩

/-- `range` produces one item for each count magnitude, counting down from zero
when the count is negative. -/
private def rangeItems (count : Int) : List Int :=
  (List.range count.natAbs).map fun index =>
    if count < 0 then -(Int.ofNat index) else Int.ofNat index

/-- A repeat observation exposes the count, its single evaluation state, and
the finite loop result. -/
private structure RepeatEvaluation
    (BreakValue ReturnValue Error : Type) where
  count : Int
  nextState : Nat
  countReads : List Nat
  evaluation : Evaluation Int BreakValue ReturnValue Error
  deriving DecidableEq

/-- Evaluate the repeat count once, then iterate over its finite range. -/
private def evaluateRepeat {BreakValue ReturnValue Error : Type}
    (state : Nat) (countExpression : Nat → Int × Nat)
    (body : Int → BodyStep BreakValue ReturnValue Error) :
    RepeatEvaluation BreakValue ReturnValue Error :=
  let (count, nextState) := countExpression state
  ⟨count, nextState, [state], runLoop 0 (rangeItems count) body⟩

/-- Counterfactual mutation that evaluates the count expression twice. -/
private def repeatedCountMutation
    (state : Nat) (countExpression : Nat → Int × Nat)
    (body : Int → BodyStep BreakValue ReturnValue Error) :
    RepeatEvaluation BreakValue ReturnValue Error :=
  let (_, intermediateState) := countExpression state
  let (count, nextState) := countExpression intermediateState
  ⟨count, nextState, [state, intermediateState],
    runLoop 0 (rangeItems count) body⟩

/-- A normal body result advances to the next item and records both visits. -/
theorem normal_step_advances_to_next_iteration :
    runLoop 4 [1, 2]
      (fun item : Nat =>
        ((if item == 1 then BodyStep.next else BodyStep.breakTo 4 .unit) :
          BodyStep Nat Nat String)) =
      ⟨Outcome.completed .unit,
        [(1, BodyStep.next), (2, BodyStep.breakTo 4 .unit)]⟩ := by
  decide

/-- A continue aimed at the current loop also advances to the next item. -/
theorem continue_targets_next_iteration :
    runLoop 4 [1, 2]
      (fun item : Nat =>
        ((if item == 1 then BodyStep.continueTo 4
          else BodyStep.breakTo 4 .unit) : BodyStep Nat Nat String)) =
      ⟨Outcome.completed .unit,
        [(1, BodyStep.continueTo 4), (2, BodyStep.breakTo 4 .unit)]⟩ := by
  decide

/-- A unit break completes the targeted loop with its unit result. -/
theorem break_unit_completes_loop :
    runLoop 4 [1, 2]
      (fun _ : Nat => (BodyStep.breakTo 4 .unit : BodyStep Nat Nat String)) =
      ⟨Outcome.completed .unit, [(1, BodyStep.breakTo 4 .unit)]⟩ := by
  decide

/-- A value break completes the loop with exactly its supplied value. -/
theorem break_value_completes_loop :
    runLoop 4 [1, 2]
      (fun item : Nat =>
        (BodyStep.breakTo 4 (.value (item + 40)) : BodyStep Nat Nat String)) =
      ⟨Outcome.completed (.value 41), [(1, BodyStep.breakTo 4 (.value 41))]⟩ := by
  decide

/-- Return leaves the loop immediately with its payload unchanged. -/
theorem return_propagates_through_loop :
    runLoop 4 [1, 2]
      (fun _ : Nat => (BodyStep.returnFromLoop 17 : BodyStep Nat Nat String)) =
      ⟨Outcome.returned 17, [(1, BodyStep.returnFromLoop 17)]⟩ := by
  decide

/-- Failure leaves the loop immediately with its error unchanged. -/
theorem failure_propagates_through_loop :
    runLoop 4 [1, 2]
      (fun _ : Nat =>
        (BodyStep.failFromLoop "stop" : BodyStep Nat Nat String)) =
      ⟨Outcome.failed "stop", [(1, BodyStep.failFromLoop "stop")]⟩ := by
  decide

private def innerBreakExample : NestedEvaluation Nat Nat Nat Nat String :=
  runNested 4 9 [10, 20] [0, 1] fun outer inner =>
    if inner == 0 then BodyStep.breakTo 9 (.value (outer + inner))
    else BodyStep.next

/-- An outer-target break in the first inner body ends the nested execution. -/
private def outerBreakExample : NestedEvaluation Nat Nat Nat Nat String :=
  runNested 4 9 [10, 20] [0, 1] fun outer inner =>
    if outer == 10 && inner == 0 then BodyStep.breakTo 4 (.value 77)
    else BodyStep.next

private def outerReturnExample : NestedEvaluation Nat Nat Nat Nat String :=
  runNested 4 9 [10, 20] [0, 1] fun outer inner =>
    if outer == 10 && inner == 0 then BodyStep.returnFromLoop 88
    else BodyStep.next

private def outerFailureExample : NestedEvaluation Nat Nat Nat Nat String :=
  runNested 4 9 [10, 20] [0, 1] fun outer inner =>
    if outer == 10 && inner == 0 then BodyStep.failFromLoop "nested-stop"
    else BodyStep.next

/-- An inner break stays local, while an outer break, return, or failure ends
both the inner-run trace and the outer execution at its first item. -/
theorem inner_break_targets_inner_loop :
    innerBreakExample.inner.map (fun entry => entry.2.outcome) =
        [Outcome.completed (.value 10), Outcome.completed (.value 20)] ∧
      innerBreakExample.outer.outcome = Outcome.completed .unit ∧
      innerBreakExample.outer.trace =
        [(10, BodyStep.next), (20, BodyStep.next)] ∧
      outerBreakExample.inner.map (fun entry => entry.2.trace) =
        [[(0, BodyStep.breakTo 4 (.value 77))]] ∧
      outerBreakExample.outer.outcome = Outcome.completed (.value 77) ∧
      outerBreakExample.outer.trace =
        [(10, BodyStep.breakTo 4 (.value 77))] ∧
      outerReturnExample.inner.map (fun entry => entry.2.trace) =
        [[(0, BodyStep.returnFromLoop 88)]] ∧
      outerReturnExample.outer.outcome = Outcome.returned 88 ∧
      outerFailureExample.inner.map (fun entry => entry.2.trace) =
        [[(0, BodyStep.failFromLoop "nested-stop")]] ∧
      outerFailureExample.outer.outcome = Outcome.failed "nested-stop" := by
  decide

private def innerContinueExample : NestedEvaluation Nat Nat Nat Nat String :=
  runNested 4 9 [10, 20] [0, 1] fun outer inner =>
    if inner == 0 then BodyStep.continueTo 9
    else BodyStep.breakTo 9 (.value (outer + inner))

private def outerContinueExample : NestedEvaluation Nat Nat Nat Nat String :=
  runNested 4 9 [10, 20] [0, 1] fun outer inner =>
    if outer == 10 then BodyStep.continueTo 4
    else if inner == 0 then BodyStep.breakTo 4 (.value 55)
    else BodyStep.next

/-- An inner continue advances within its loop. An outer-target continue ends
the current inner trace and resumes at the next outer item. -/
theorem inner_continue_targets_inner_loop :
    innerContinueExample.inner.map (fun entry => entry.2.outcome) =
        [Outcome.completed (.value 11), Outcome.completed (.value 21)] ∧
      innerContinueExample.inner.map (fun entry => entry.2.trace) =
        [[(0, BodyStep.continueTo 9), (1, BodyStep.breakTo 9 (.value 11))],
         [(0, BodyStep.continueTo 9), (1, BodyStep.breakTo 9 (.value 21))]] ∧
      innerContinueExample.outer.outcome = Outcome.completed .unit ∧
      innerContinueExample.outer.trace =
        [(10, BodyStep.next), (20, BodyStep.next)] ∧
      outerContinueExample.inner.map (fun entry => entry.2.trace) =
        [[(0, BodyStep.continueTo 4)],
         [(0, BodyStep.breakTo 4 (.value 55))]] ∧
      outerContinueExample.outer.outcome = Outcome.completed (.value 55) ∧
      outerContinueExample.outer.trace =
        [(10, BodyStep.continueTo 4), (20, BodyStep.breakTo 4 (.value 55))] := by
  decide

/-- A repeat count is read once. Re-evaluating a state-sensitive count changes
both its recorded reads and the resulting iteration count. -/
theorem repeat_evaluates_count_once_with_mutation_witness :
    let expression := fun state : Nat => (Int.ofNat state, state + 1)
    let body := fun _ : Int => (BodyStep.next : BodyStep Nat Nat String)
    let once := evaluateRepeat 2 expression body
    let mutated := repeatedCountMutation 2 expression body
    once.countReads = [2] ∧
      once.count = 2 ∧
      once.evaluation.trace.length = 2 ∧
      mutated.countReads = [2, 3] ∧
      mutated.count = 3 ∧
      mutated.evaluation.trace.length = 3 ∧
      once ≠ mutated := by
  decide

/-- A zero range has no items, so its body is never visited. -/
theorem zero_range_runs_no_body :
    runLoop 4 (rangeItems 0)
      (fun _ : Int => (BodyStep.next : BodyStep Nat Nat String)) =
      ⟨Outcome.completed .unit, []⟩ := by
  decide

/-- The finite range contains the absolute count magnitude in items. -/
theorem range_item_count_matches_absolute_count (count : Int) :
    (rangeItems count).length = count.natAbs := by
  simp [rangeItems]

/-- Continue reaches the next item, and a later break stops before the tail. -/
theorem continue_then_break_has_exact_trace :
    runLoop 7 [0, 1, 2, 3]
      (fun item : Nat =>
        ((if item == 0 then BodyStep.continueTo 7
          else if item == 2 then BodyStep.breakTo 7 (.value 22)
          else BodyStep.next) : BodyStep Nat Nat String)) =
      ⟨Outcome.completed (.value 22),
        [(0, BodyStep.continueTo 7), (1, BodyStep.next),
         (2, BodyStep.breakTo 7 (.value 22))]⟩ := by
  decide

end Xsh.Loops.ControlFlow
