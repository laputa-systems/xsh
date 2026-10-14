/-!
This fragment models eager execution of a finite two-source List
comprehension. It records each source, filter, and projection invocation with
its outcome, and publishes only a completed result or a failure. It does not
model parsing, general expression effects, streams, cleanup, or Rust
correspondence.
-/

namespace Xsh.Semantics.ListExecution

/-- An expression call either returns a value or stops with its own error. -/
abbrev ExpressionResult (Error Value : Type) := Except Error Value

/-- A trace records the exact finite expression invocation and its outcome. -/
inductive Event (Outer Inner Output Error : Type) where
  | outerSource (result : ExpressionResult Error (List Outer))
  | innerSource (outer : Outer) (result : ExpressionResult Error (List Inner))
  | filter (index : Nat) (outer : Outer) (inner : Inner)
      (result : ExpressionResult Error Bool)
  | projection (outer : Outer) (inner : Inner)
      (result : ExpressionResult Error Output)

/-- Failures identify the expression and comprehension binding that failed. -/
inductive Failure (Outer Inner Error : Type) where
  | outerSource (error : Error)
  | innerSource (outer : Outer) (error : Error)
  | filter (index : Nat) (outer : Outer) (inner : Inner) (error : Error)
  | projection (outer : Outer) (inner : Inner) (error : Error)

/-- A total execution result keeps its public outcome separate from its trace. -/
structure Evaluation (Event Failure Value : Type) where
  outcome : Except Failure Value
  trace : List Event

abbrev Filter (Outer Inner Error : Type) :=
  Outer → Inner → ExpressionResult Error Bool

/-- The outer source is evaluated once; the inner source receives each outer
binding; filters and projection receive both current bindings. -/
structure Program (Outer Inner Output Error : Type) where
  outerSource : Unit → ExpressionResult Error (List Outer)
  innerSource : Outer → ExpressionResult Error (List Inner)
  filters : List (Filter Outer Inner Error)
  projection : Outer → Inner → ExpressionResult Error Output

abbrev Run (Outer Inner Output Error : Type) :=
  Evaluation (Event Outer Inner Output Error)
    (Failure Outer Inner Error) (List Output)

/-- Run filters in written order. A false result skips every later filter. -/
def evaluateFilters (program : Program Outer Inner Output Error)
    (outer : Outer) (inner : Inner) :
    Nat → List (Filter Outer Inner Error) →
      Evaluation (Event Outer Inner Output Error)
        (Failure Outer Inner Error) Bool
  | _, [] => ⟨.ok true, []⟩
  | index, predicate :: rest =>
      let result := predicate outer inner
      let event := Event.filter index outer inner result
      match result with
      | .error error => ⟨.error (.filter index outer inner error), [event]⟩
      | .ok false => ⟨.ok false, [event]⟩
      | .ok true =>
          let later := evaluateFilters program outer inner (index + 1) rest
          { later with trace := event :: later.trace }

/-- Evaluate one inner binding, projecting only after every filter survives. -/
def evaluateBinding (program : Program Outer Inner Output Error)
    (outer : Outer) (inner : Inner) :
    Evaluation (Event Outer Inner Output Error)
      (Failure Outer Inner Error) (Option Output) :=
  let filtered := evaluateFilters program outer inner 0 program.filters
  match filtered.outcome with
  | .error failure => ⟨.error failure, filtered.trace⟩
  | .ok false => ⟨.ok none, filtered.trace⟩
  | .ok true =>
      let result := program.projection outer inner
      let event := Event.projection outer inner result
      match result with
      | .error error =>
          ⟨.error (.projection outer inner error), filtered.trace ++ [event]⟩
      | .ok value => ⟨.ok (some value), filtered.trace ++ [event]⟩

/-- Walk one inner List in order, stopping immediately on failure. -/
def evaluateInnerItems (program : Program Outer Inner Output Error)
    (outer : Outer) : List Inner → Run Outer Inner Output Error
  | [] => ⟨.ok [], []⟩
  | inner :: rest =>
      let current := evaluateBinding program outer inner
      match current.outcome with
      | .error failure => ⟨.error failure, current.trace⟩
      | .ok selected =>
          let later := evaluateInnerItems program outer rest
          match later.outcome with
          | .error failure =>
              ⟨.error failure, current.trace ++ later.trace⟩
          | .ok laterValues =>
              ⟨.ok ((selected.toList) ++ laterValues),
                current.trace ++ later.trace⟩

/-- Evaluate a fresh inner source for every outer binding, preserving loop
nesting order in both trace and output. -/
def evaluateOuterItems (program : Program Outer Inner Output Error) :
    List Outer → Run Outer Inner Output Error
  | [] => ⟨.ok [], []⟩
  | outer :: rest =>
      let result := program.innerSource outer
      let event := Event.innerSource outer result
      match result with
      | .error error => ⟨.error (.innerSource outer error), [event]⟩
      | .ok innerValues =>
          let current := evaluateInnerItems program outer innerValues
          match current.outcome with
          | .error failure =>
              ⟨.error failure, event :: current.trace⟩
          | .ok currentValues =>
              let later := evaluateOuterItems program rest
              match later.outcome with
              | .error failure =>
                  ⟨.error failure, event :: current.trace ++ later.trace⟩
              | .ok laterValues =>
                  ⟨.ok (currentValues ++ laterValues),
                    event :: current.trace ++ later.trace⟩

/-- Evaluate the comprehension eagerly. A failure exposes no partial List. -/
def evaluate (program : Program Outer Inner Output Error) :
    Run Outer Inner Output Error :=
  let result := program.outerSource ()
  let event := Event.outerSource result
  match result with
  | .error error => ⟨.error (.outerSource error), [event]⟩
  | .ok outerValues =>
      let body := evaluateOuterItems program outerValues
      { body with trace := event :: body.trace }

/-- Reuse a single inner-source result for every outer binding. This is an
intentionally incorrect mutation used to witness why the source cannot be
hoisted when it depends on the current outer binding. -/
def evaluateHoistedInnerItems (program : Program Outer Inner Output Error)
    (innerValues : List Inner) : List Outer → Run Outer Inner Output Error
  | [] => ⟨.ok [], []⟩
  | outer :: rest =>
      let current := evaluateInnerItems program outer innerValues
      match current.outcome with
      | .error failure => ⟨.error failure, current.trace⟩
      | .ok currentValues =>
          let later := evaluateHoistedInnerItems program innerValues rest
          match later.outcome with
          | .error failure =>
              ⟨.error failure, current.trace ++ later.trace⟩
          | .ok laterValues =>
              ⟨.ok (currentValues ++ laterValues), current.trace ++ later.trace⟩

/-- Faulty source-hoisting mutation; the inner source is evaluated once using
the first outer binding, then that one result is reused. -/
def hoistedInnerSourceMutation (program : Program Outer Inner Output Error) :
    Run Outer Inner Output Error :=
  let outerResult := program.outerSource ()
  let outerEvent := Event.outerSource outerResult
  match outerResult with
  | .error error => ⟨.error (.outerSource error), [outerEvent]⟩
  | .ok [] => ⟨.ok [], [outerEvent]⟩
  | .ok (firstOuter :: outerRest) =>
      let innerResult := program.innerSource firstOuter
      let innerEvent := Event.innerSource firstOuter innerResult
      match innerResult with
      | .error error =>
          ⟨.error (.innerSource firstOuter error), [outerEvent, innerEvent]⟩
      | .ok innerValues =>
          let body := evaluateHoistedInnerItems program innerValues
            (firstOuter :: outerRest)
          { body with trace := outerEvent :: innerEvent :: body.trace }

/-- Concrete program whose inner source and projection both depend on the
current outer binding. -/
def dependentProgram : Program Nat Nat Nat String := {
  outerSource := fun _ => .ok [1, 2]
  innerSource := fun outer => .ok [outer]
  filters := []
  projection := fun outer inner => .ok (outer * 10 + inner)
}

/-- The outer source is the first traced expression invocation. -/
theorem outer_source_is_first_event (program : Program Outer Inner Output Error) :
    (evaluate program).trace.head? =
      some (.outerSource (program.outerSource ())) := by
  cases h : program.outerSource () <;> simp [evaluate, h]

/-- Each outer binding triggers a new inner-source evaluation using that
binding as its argument. -/
theorem dependent_inner_source_runs_once_per_outer_binding :
    (evaluate dependentProgram).trace =
      [.outerSource (.ok [1, 2]),
       .innerSource 1 (.ok [1]), .projection 1 1 (.ok 11),
       .innerSource 2 (.ok [2]), .projection 2 2 (.ok 22)] := by
  rfl

private def filteredDependentProgram : Program Nat Nat Nat String := {
  outerSource := fun _ => .ok [1, 2]
  innerSource := fun outer => .ok [outer, outer + 10]
  filters := [fun outer inner => .ok (inner == outer)]
  projection := fun outer inner => .ok (outer * 10 + inner)
}

/-- Each nested source runs before its binding's written filter and any
surviving projection, with the next outer source delayed until that work ends. -/
theorem inner_source_precedes_filter_and_projection_per_outer_binding :
    (evaluate filteredDependentProgram).trace =
      [.outerSource (.ok [1, 2]),
       .innerSource 1 (.ok [1, 11]),
       .filter 0 1 1 (.ok true), .projection 1 1 (.ok 11),
       .filter 0 1 11 (.ok false),
       .innerSource 2 (.ok [2, 12]),
       .filter 0 2 2 (.ok true), .projection 2 2 (.ok 22),
       .filter 0 2 12 (.ok false)] := by
  rfl

private def twoFilters : Program Nat Nat Nat String := {
  outerSource := fun _ => .ok [1]
  innerSource := fun _ => .ok [1]
  filters := [fun _ _ => .ok true, fun _ _ => .ok true]
  projection := fun outer inner => .ok (outer + inner)
}

/-- Filters are called in their written order before projection. -/
theorem filters_run_in_written_order :
    (evaluate twoFilters).trace =
      [.outerSource (.ok [1]), .innerSource 1 (.ok [1]),
       .filter 0 1 1 (.ok true), .filter 1 1 1 (.ok true),
       .projection 1 1 (.ok 2)] := by
  rfl

private def rejectingSecondFilter : Program Nat Nat Nat String := {
  twoFilters with
  filters := [fun _ _ => .ok false, fun _ _ => .ok true]
}

/-- A false earlier filter skips later filters and the projection. -/
theorem false_filter_skips_later_filters_and_projection :
    (evaluate rejectingSecondFilter).trace =
      [.outerSource (.ok [1]), .innerSource 1 (.ok [1]),
       .filter 0 1 1 (.ok false)] ∧
      (evaluate rejectingSecondFilter).outcome = .ok [] := by
  constructor <;> rfl

private def mixedSurvivors : Program Nat Nat Nat String := {
  outerSource := fun _ => .ok [1, 2, 3]
  innerSource := fun outer => .ok [outer, outer + 10]
  filters := [fun _ inner => .ok (inner < 10)]
  projection := fun outer inner => .ok (outer * 100 + inner)
}

/-- Projection is called once for each binding that survives all filters. -/
theorem projection_runs_once_per_survivor :
    (evaluate mixedSurvivors).trace.filterMap (fun event =>
      match event with | .projection _ _ _ => some event | _ => none) =
        [.projection 1 1 (.ok 101), .projection 2 2 (.ok 202),
          .projection 3 3 (.ok 303)] := by
  rfl

/-- The output follows outer-loop order and then each inner List's order. -/
theorem output_follows_loop_nesting_order :
    (evaluate mixedSurvivors).outcome = .ok [101, 202, 303] := by
  rfl

private def emptyOuterProgram : Program Nat Nat Nat String := {
  outerSource := fun _ => .ok []
  innerSource := fun _ => .error "inner must not run"
  filters := [fun _ _ => .error "filter must not run"]
  projection := fun _ _ => .error "projection must not run"
}

/-- An empty outer source succeeds without invoking any body expression. -/
theorem empty_outer_skips_inner_source_filters_and_projection :
    (evaluate emptyOuterProgram).outcome = .ok [] ∧
      (evaluate emptyOuterProgram).trace = [.outerSource (.ok [])] := by
  constructor <;> rfl

private def outerFailureProgram : Program Nat Nat Nat String := {
  emptyOuterProgram with outerSource := fun _ => .error "outer failed"
}

/-- Outer-source failure stops the comprehension before all inner work. -/
theorem outer_source_failure_stops_without_partial_publication :
    (evaluate outerFailureProgram).outcome =
      .error (.outerSource "outer failed") ∧
      (evaluate outerFailureProgram).trace =
        [.outerSource (.error "outer failed")] := by
  constructor <;> rfl

private def innerFailureProgram : Program Nat Nat Nat String := {
  dependentProgram with
  innerSource := fun outer => if outer == 1 then .ok [1] else .error "inner failed"
}

/-- An inner-source failure stops before evaluating later outer bindings. -/
theorem inner_source_failure_stops_later_outer_bindings :
    (evaluate innerFailureProgram).outcome =
      .error (.innerSource 2 "inner failed") ∧
      (evaluate innerFailureProgram).trace =
        [.outerSource (.ok [1, 2]), .innerSource 1 (.ok [1]),
         .projection 1 1 (.ok 11), .innerSource 2 (.error "inner failed")] := by
  constructor <;> rfl

private def filterFailureProgram : Program Nat Nat Nat String := {
  twoFilters with
  filters := [fun _ _ => .ok true, fun _ _ => .error "filter failed"]
}

/-- A filter failure stops the written filter sequence and projection. -/
theorem filter_failure_stops_later_filters_and_projection :
    (evaluate filterFailureProgram).outcome =
      .error (.filter 1 1 1 "filter failed") ∧
      (evaluate filterFailureProgram).trace =
        [.outerSource (.ok [1]), .innerSource 1 (.ok [1]),
         .filter 0 1 1 (.ok true), .filter 1 1 1 (.error "filter failed")] := by
  constructor <;> rfl

private def projectionFailureProgram : Program Nat Nat Nat String := {
  mixedSurvivors with
  projection := fun outer _ => if outer == 2 then .error "projection failed"
    else .ok outer
}

/-- A projection failure stops subsequent work and publishes no output prefix. -/
theorem projection_failure_stops_without_partial_publication :
    (evaluate projectionFailureProgram).outcome =
      .error (.projection 2 2 "projection failed") ∧
      (evaluate projectionFailureProgram).trace =
        [.outerSource (.ok [1, 2, 3]),
         .innerSource 1 (.ok [1, 11]), .filter 0 1 1 (.ok true),
         .projection 1 1 (.ok 1), .filter 0 1 11 (.ok false),
         .innerSource 2 (.ok [2, 12]), .filter 0 2 2 (.ok true),
         .projection 2 2 (.error "projection failed")] := by
  constructor <;> rfl

/-- Hoisting a binding-dependent inner source changes both the trace and
the observable result on a nonempty two-binding input. -/
theorem hoisted_inner_source_mutation_changes_trace_and_output :
    (evaluate dependentProgram).outcome = .ok [11, 22] ∧
      (hoistedInnerSourceMutation dependentProgram).outcome = .ok [11, 21] ∧
      (evaluate dependentProgram).trace ≠
        (hoistedInnerSourceMutation dependentProgram).trace := by
  constructor
  · rfl
  constructor
  · rfl
  · simp [evaluate, hoistedInnerSourceMutation, dependentProgram,
      evaluateOuterItems, evaluateHoistedInnerItems, evaluateInnerItems,
      evaluateBinding, evaluateFilters]

/-- If every finite inner binding is filtered out, evaluation succeeds with
an empty List and performs no projection. -/
theorem rejecting_all_bindings_returns_empty_list :
    (evaluate rejectingSecondFilter).outcome = .ok [] ∧
      ¬ (evaluate rejectingSecondFilter).trace.any
        (fun event => match event with | .projection _ _ _ => true | _ => false) := by
  constructor
  · rfl
  · simp [evaluate, rejectingSecondFilter, twoFilters, evaluateOuterItems,
      evaluateInnerItems, evaluateBinding, evaluateFilters]

end Xsh.Semantics.ListExecution
