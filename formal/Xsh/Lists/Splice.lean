/-!
This fragment evaluates finite list literals whose elements and splices carry
one observable event and either a value or a failure. It preserves written
order, expands splices in place, and publishes only a complete list. It does
not model parsing, general expression evaluation, checking or type inference,
or XSH runtime and Rust correspondence.
-/

namespace Xsh.Lists.Splice

/-- The small value domain distinguishes a nested list from its elements. -/
inductive LiteralValue where
  | atom (value : Nat)
  | list (values : List LiteralValue)

abbrev Event := Nat
abbrev Failure := Nat

/-- An expression reports either its value or its failure when evaluated. -/
structure Expression (α : Type) where
  event : Event
  answer : Except Failure α

abbrev ElementExpression := Expression LiteralValue
abbrev SpliceExpression := Expression (List LiteralValue)

/-- An evaluation exposes a whole value on success and no value on failure. -/
inductive Evaluation (α : Type) where
  | published (value : α) (events : List Event)
  | failed (failure : Failure) (events : List Event)

inductive ListPart where
  | element (expression : ElementExpression)
  | splice (expression : SpliceExpression)

/-- Each expression invocation contributes exactly its own event. -/
def evaluateExpression {α : Type} (expression : Expression α) : Evaluation α :=
  match expression.answer with
  | .ok value => .published value [expression.event]
  | .error failure => .failed failure [expression.event]

/-- Keep the finite effect trace regardless of whether evaluation succeeded. -/
def eventsOf {α : Type} : Evaluation α → List Event
  | .published _ events => events
  | .failed _ events => events

/-- A failed evaluation exposes no partially accumulated list. -/
def publishedValue? {α : Type} : Evaluation α → Option α
  | .published value _ => some value
  | .failed _ _ => none

/-- Evaluate list parts from left to right and discard accumulated values on failure. -/
def evaluateListLiteral : List ListPart → Evaluation (List LiteralValue)
  | [] => .published [] []
  | .element expression :: rest =>
      match evaluateExpression expression with
      | .failed failure events => .failed failure events
      | .published value events =>
          match evaluateListLiteral rest with
          | .failed failure laterEvents => .failed failure (events ++ laterEvents)
          | .published laterValues laterEvents =>
              .published (value :: laterValues) (events ++ laterEvents)
  | .splice expression :: rest =>
      match evaluateExpression expression with
      | .failed failure events => .failed failure events
      | .published values events =>
          match evaluateListLiteral rest with
          | .failed failure laterEvents => .failed failure (events ++ laterEvents)
          | .published laterValues laterEvents =>
              .published (values ++ laterValues) (events ++ laterEvents)

/-- A faulty hoisting pass evaluates the following splice before its element. -/
def evaluateHoistedSplice (element : ElementExpression)
    (splice : SpliceExpression) : Evaluation (List LiteralValue) :=
  match evaluateExpression splice with
  | .failed failure events => .failed failure events
  | .published spread events =>
      match evaluateExpression element with
      | .failed failure laterEvents => .failed failure (events ++ laterEvents)
      | .published value laterEvents =>
          .published (value :: spread) (events ++ laterEvents)

theorem written_order_trace :
    evaluateListLiteral
      [.element ⟨10, .ok (.atom 1)⟩,
       .splice ⟨20, .ok [.atom 2, .atom 3]⟩,
       .element ⟨30, .ok (.atom 4)⟩] =
      .published [.atom 1, .atom 2, .atom 3, .atom 4] [10, 20, 30] := by
  rfl

theorem element_evaluated_once (expression : ElementExpression) :
    eventsOf (evaluateListLiteral [.element expression]) = [expression.event] := by
  cases expression with
  | mk event answer => cases answer <;> rfl

theorem splice_source_evaluated_once (expression : SpliceExpression) :
    eventsOf (evaluateListLiteral [.splice expression]) = [expression.event] := by
  cases expression with
  | mk event answer => cases answer <;> rfl

theorem splice_expands_in_place :
    evaluateListLiteral
      [.element ⟨1, .ok (.atom 7)⟩,
       .splice ⟨2, .ok [.atom 8, .atom 9]⟩,
       .element ⟨3, .ok (.atom 10)⟩] =
      .published [.atom 7, .atom 8, .atom 9, .atom 10] [1, 2, 3] := by
  rfl

theorem unmarked_list_remains_nested :
    evaluateListLiteral
      [.element ⟨4, .ok (.list [.atom 5, .atom 6])⟩] =
      .published [.list [.atom 5, .atom 6]] [4] := by
  rfl

theorem empty_splice_identity :
    evaluateListLiteral
      [.element ⟨1, .ok (.atom 7)⟩,
       .splice ⟨2, .ok []⟩,
       .element ⟨3, .ok (.atom 8)⟩] =
      .published [.atom 7, .atom 8] [1, 2, 3] := by
  rfl

theorem duplicates_are_retained :
    evaluateListLiteral
      [.element ⟨11, .ok (.atom 4)⟩,
       .splice ⟨12, .ok [.atom 4, .atom 9, .atom 4]⟩] =
      .published [.atom 4, .atom 4, .atom 9, .atom 4] [11, 12] := by
  rfl

theorem exact_result_and_length :
    let evaluation := evaluateListLiteral
      [.element ⟨21, .ok (.atom 1)⟩,
       .splice ⟨22, .ok [.atom 2, .atom 3]⟩]
    publishedValue? evaluation = some [.atom 1, .atom 2, .atom 3] ∧
      (publishedValue? evaluation).map List.length = some 3 := by
  simp [evaluateListLiteral, evaluateExpression, publishedValue?]

theorem element_failure_stops_the_suffix :
    evaluateListLiteral
      [.element ⟨31, .ok (.atom 1)⟩,
       .element ⟨32, .error 90⟩,
       .splice ⟨33, .ok [.atom 3]⟩] =
      .failed 90 [31, 32] := by
  rfl

theorem splice_failure_stops_the_suffix :
    evaluateListLiteral
      [.splice ⟨41, .ok [.atom 1, .atom 2]⟩,
       .splice ⟨42, .error 91⟩,
       .element ⟨43, .ok (.atom 3)⟩] =
      .failed 91 [41, 42] := by
  rfl

theorem failure_publishes_no_partial_list :
    let evaluation := evaluateListLiteral
      [.splice ⟨51, .ok [.atom 1, .atom 2]⟩,
       .element ⟨52, .error 92⟩,
       .element ⟨53, .ok (.atom 3)⟩]
    publishedValue? evaluation = none ∧ eventsOf evaluation = [51, 52] := by
  simp [evaluateListLiteral, evaluateExpression, publishedValue?, eventsOf]

theorem hoisting_splice_changes_an_effectful_witness :
    let element : ElementExpression := ⟨61, .ok (.atom 1)⟩
    let splice : SpliceExpression := ⟨62, .ok [.atom 2]⟩
    evaluateListLiteral [.element element, .splice splice] =
        .published [.atom 1, .atom 2] [61, 62] ∧
      evaluateHoistedSplice element splice =
        .published [.atom 1, .atom 2] [62, 61] ∧
      eventsOf (evaluateListLiteral [.element element, .splice splice]) = [61, 62] ∧
      eventsOf (evaluateHoistedSplice element splice) = [62, 61] ∧
      eventsOf (evaluateListLiteral [.element element, .splice splice]) ≠
        eventsOf (evaluateHoistedSplice element splice) := by
  refine ⟨rfl, rfl, rfl, rfl, ?_⟩
  decide

end Xsh.Lists.Splice
