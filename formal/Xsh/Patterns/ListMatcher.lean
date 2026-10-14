/-!
A bounded matcher for wildcard, binder, scalar-literal, exact-list, and
prefix/rest patterns over finite composite values. Attempts record element
matcher callbacks and tentative captures; only a complete match publishes
captures. The fragment omits records, enums, errors, type and text patterns,
alternatives, exhaustiveness, checker behavior, diagnostics, and runtime or Rust
correspondence.
-/

namespace Xsh.Patterns.ListMatcher

/-- Scalar payloads admitted by this finite matcher fragment. -/
inductive ScalarValue where
  | boolean (value : Bool)
  | integer (value : Int)
  | string (value : String)
  deriving DecidableEq, Repr

mutual
  /-- Finite recursive values used by the matcher. -/
  inductive Value where
    | scalar (value : ScalarValue)
    | list (values : ValueList)

  /-- A finite list representation whose elements may themselves be lists. -/
  inductive ValueList where
    | nil
    | cons (head : Value) (tail : ValueList)
end

deriving instance DecidableEq, Repr for Value
deriving instance DecidableEq, Repr for ValueList

mutual
  /-- The supported patterns, with list patterns matched from left to right. -/
  inductive Pattern where
    | wildcard
    | bind (name : String)
    | literal (value : ScalarValue)
    | exactList (elements : PatternList)
    | prefixRest (prefixPatterns : PatternList) (restName : String)

  /-- A finite sequence of child patterns. -/
  inductive PatternList where
    | nil
    | cons (head : Pattern) (tail : PatternList)
end

deriving instance DecidableEq, Repr for Pattern
deriving instance DecidableEq, Repr for PatternList

abbrev Captures := List (String × Value)

/-- Trace events distinguish child callbacks, tentative captures, and final publication. -/
inductive MatchEvent where
  | elementMatcherCalled (path : List Nat)
  | captureTentative (path : List Nat) (name : String) (value : Value)
  | capturesPublished (captures : Captures)
  | capturesDiscarded (captures : Captures)
  deriving DecidableEq, Repr

/-- A result keeps attempted captures separate from externally published captures. -/
structure MatchAttempt where
  matched : Bool
  tentativeCaptures : Captures
  publishedCaptures : Captures
  trace : List MatchEvent
  deriving DecidableEq, Repr

/-- Element callbacks appear in the trace in their actual invocation order. -/
def MatchAttempt.callbackTrace (attempt : MatchAttempt) : List (List Nat) :=
  attempt.trace.foldr
    (fun event paths =>
      match event with
      | .elementMatcherCalled path => path :: paths
      | _ => paths)
    []

private def valueListOf : List Value → ValueList
  | [] => .nil
  | value :: rest => .cons value (valueListOf rest)

private def listValue (values : List Value) : Value :=
  .list (valueListOf values)

private def patternListOf : List Pattern → PatternList
  | [] => .nil
  | pattern :: rest => .cons pattern (patternListOf rest)

private def PatternList.toList : PatternList → List Pattern
  | .nil => []
  | .cons pattern rest => pattern :: rest.toList

private def ValueList.toList : ValueList → List Value
  | .nil => []
  | .cons value rest => value :: rest.toList

mutual
  private def patternFuel : Pattern → Nat
    | .wildcard => 1
    | .bind _ => 1
    | .literal _ => 1
    | .exactList elements => 1 + patternListFuel elements
    | .prefixRest prefixPatterns _ => 1 + patternListFuel prefixPatterns

  private def patternListFuel : PatternList → Nat
    | .nil => 0
    | .cons pattern rest => 1 + patternFuel pattern + patternListFuel rest
end

private structure PartialMatch where
  matched : Bool
  tentativeCaptures : Captures
  trace : List MatchEvent

private def failedPartial : PartialMatch :=
  { matched := false, tentativeCaptures := [], trace := [] }

mutual
  private def evaluatePartial : Nat → Pattern → Value → List Nat → PartialMatch
    | 0, _, _, _ => failedPartial
    | _fuel + 1, .wildcard, _, _ =>
        { matched := true, tentativeCaptures := [], trace := [] }
    | _fuel + 1, .bind name, value, path =>
        { matched := true
          tentativeCaptures := [(name, value)]
          trace := [.captureTentative path name value] }
    | _fuel + 1, .literal expected, .scalar actual, _ =>
        if actual == expected then
          { matched := true, tentativeCaptures := [], trace := [] }
        else
          failedPartial
    | _fuel + 1, .literal _, .list _, _ => failedPartial
    | fuel + 1, .exactList patterns, .list values, path =>
        let patternValues := patterns.toList
        let subjectValues := values.toList
        if patternValues.length == subjectValues.length then
          evaluateChildren fuel (patternValues.zip subjectValues) path 0
        else
          failedPartial
    | _fuel + 1, .exactList _, .scalar _, _ => failedPartial
    | fuel + 1, .prefixRest prefixPatterns restName, .list values, path =>
        let patternValues := prefixPatterns.toList
        let subjectValues := values.toList
        if patternValues.length ≤ subjectValues.length then
          let prefixResult :=
            evaluateChildren fuel (patternValues.zip (subjectValues.take patternValues.length)) path 0
          if prefixResult.matched then
            let restValue := listValue (subjectValues.drop patternValues.length)
            { matched := true
              tentativeCaptures := prefixResult.tentativeCaptures ++ [(restName, restValue)]
              trace := prefixResult.trace ++
                [.captureTentative path restName restValue] }
          else
            prefixResult
        else
          failedPartial
    | _fuel + 1, .prefixRest _ _, .scalar _, _ => failedPartial

  private def evaluateChildren : Nat → List (Pattern × Value) → List Nat → Nat → PartialMatch
    | _, [], _, _ =>
        { matched := true, tentativeCaptures := [], trace := [] }
    | 0, _ :: _, _, _ => failedPartial
    | fuel + 1, (pattern, value) :: rest, parentPath, index =>
        let path := parentPath ++ [index]
        let child := evaluatePartial fuel pattern value path
        let callback := MatchEvent.elementMatcherCalled path
        if child.matched then
          let remaining := evaluateChildren fuel rest parentPath (index + 1)
          { matched := remaining.matched
            tentativeCaptures := child.tentativeCaptures ++ remaining.tentativeCaptures
            trace := callback :: child.trace ++ remaining.trace }
        else
          { matched := false
            tentativeCaptures := child.tentativeCaptures
            trace := callback :: child.trace }
end

/-- Run one pattern attempt and publish captures only after complete success. -/
def attemptPattern (pattern : Pattern) (value : Value) : MatchAttempt :=
  let attempted := evaluatePartial (patternFuel pattern + 1) pattern value []
  let finalEvent :=
    if attempted.matched then
      MatchEvent.capturesPublished attempted.tentativeCaptures
    else
      MatchEvent.capturesDiscarded attempted.tentativeCaptures
  { matched := attempted.matched
    tentativeCaptures := attempted.tentativeCaptures
    publishedCaptures := if attempted.matched then attempted.tentativeCaptures else []
    trace := attempted.trace ++ [finalEvent] }

/-- Mutation that incorrectly publishes captures considered before a late miss. -/
private def leakTentativeCapturesOnMiss (pattern : Pattern) (value : Value) : Captures :=
  let attempt := attemptPattern pattern value
  if attempt.matched then attempt.publishedCaptures else attempt.tentativeCaptures

theorem wildcard_matches_without_captures :
    let result := attemptPattern .wildcard (.scalar (.boolean true))
    result.matched = true ∧ result.tentativeCaptures = [] ∧
      result.publishedCaptures = [] ∧
      result.trace = [.capturesPublished []] := by
  decide

theorem binder_capture_publishes_value :
    let value := listValue [.scalar (.integer 4)]
    let captures := [("item", value)]
    let result := attemptPattern (.bind "item") value
    result.matched = true ∧ result.tentativeCaptures = captures ∧
      result.publishedCaptures = captures ∧
      result.trace = [.captureTentative [] "item" value, .capturesPublished captures] := by
  decide

theorem scalar_literal_matches_equal_value :
    let result := attemptPattern (.literal (.integer 17)) (.scalar (.integer 17))
    result.matched = true ∧ result.publishedCaptures = [] ∧
      result.trace = [.capturesPublished []] := by
  decide

theorem scalar_literal_mismatch_is_rejected :
    let result := attemptPattern (.literal (.integer 17)) (.scalar (.integer 18))
    result.matched = false ∧ result.publishedCaptures = [] ∧
      result.trace = [.capturesDiscarded []] := by
  decide

theorem exact_length_list_matches_each_element :
    let value := listValue [.scalar (.integer 4), .scalar (.integer 9)]
    let captures := [("head", .scalar (.integer 4))]
    let result := attemptPattern
      (.exactList (patternListOf [.bind "head", .literal (.integer 9)])) value
    result.matched = true ∧ result.tentativeCaptures = captures ∧
      result.publishedCaptures = captures ∧
      result.callbackTrace = [[0], [1]] ∧
      result.trace = [.elementMatcherCalled [0],
        .captureTentative [0] "head" (.scalar (.integer 4)),
        .elementMatcherCalled [1], .capturesPublished captures] := by
  decide

theorem exact_length_list_rejects_wrong_length :
    let result := attemptPattern
      (.exactList (patternListOf [.wildcard, .wildcard]))
      (listValue [.scalar (.boolean false)])
    result.matched = false ∧ result.publishedCaptures = [] ∧
      result.trace = [.capturesDiscarded []] := by
  decide

theorem exact_length_is_checked_before_element_callbacks :
    let result := attemptPattern
      (.exactList (patternListOf [.bind "wouldCapture", .wildcard]))
      (listValue [.scalar (.integer 1)])
    result.matched = false ∧ result.tentativeCaptures = [] ∧
      result.publishedCaptures = [] ∧ result.callbackTrace = [] ∧
      result.trace = [.capturesDiscarded []] := by
  decide

theorem prefix_rest_captures_suffix_after_prefix :
    let value := listValue [.scalar (.integer 3), .scalar (.integer 8), .scalar (.integer 13)]
    let captures := [("middle", .scalar (.integer 8)),
      ("tail", listValue [.scalar (.integer 13)])]
    let result := attemptPattern
      (.prefixRest (patternListOf [.literal (.integer 3), .bind "middle"]) "tail") value
    result.matched = true ∧ result.tentativeCaptures = captures ∧
      result.publishedCaptures = captures ∧ result.callbackTrace = [[0], [1]] ∧
      result.trace = [.elementMatcherCalled [0], .elementMatcherCalled [1],
        .captureTentative [1] "middle" (.scalar (.integer 8)),
        .captureTentative [] "tail" (listValue [.scalar (.integer 13)]),
        .capturesPublished captures] := by
  decide

theorem empty_prefix_and_rest_capture_edges :
    let value := listValue [.scalar (.boolean true)]
    let wholeCaptures := [("whole", value)]
    let emptyRestCaptures := [("rest", listValue [])]
    let whole := attemptPattern (.prefixRest (patternListOf []) "whole") value
    let emptyRest := attemptPattern
      (.prefixRest (patternListOf [.literal (.boolean true)]) "rest") value
    whole.matched = true ∧ whole.publishedCaptures = wholeCaptures ∧
      whole.callbackTrace = [] ∧
      emptyRest.matched = true ∧ emptyRest.publishedCaptures = emptyRestCaptures ∧
      emptyRest.callbackTrace = [[0]] := by
  decide

theorem nested_patterns_match_recursive_values :
    let value := listValue [
      .scalar (.integer 4),
      listValue [.scalar (.boolean true), .scalar (.string "nested")]]
    let captures := [("outer", .scalar (.integer 4)),
      ("inner", .scalar (.string "nested"))]
    let result := attemptPattern
      (.exactList (patternListOf [.bind "outer",
        .exactList (patternListOf [.literal (.boolean true), .bind "inner"])])) value
    result.matched = true ∧ result.publishedCaptures = captures ∧
      result.callbackTrace = [[0], [1], [1, 0], [1, 1]] := by
  decide

theorem captures_follow_pattern_order :
    let captures := [("first", .scalar (.integer 2)),
      ("second", .scalar (.integer 5))]
    let result := attemptPattern
      (.exactList (patternListOf [.bind "first", .bind "second"]))
      (listValue [.scalar (.integer 2), .scalar (.integer 5)])
    result.matched = true ∧ result.tentativeCaptures = captures ∧
      result.publishedCaptures = captures ∧
      result.trace = [.elementMatcherCalled [0],
        .captureTentative [0] "first" (.scalar (.integer 2)),
        .elementMatcherCalled [1],
        .captureTentative [1] "second" (.scalar (.integer 5)),
        .capturesPublished captures] := by
  decide

theorem late_miss_discards_prior_capture_mutation_witness :
    let pattern := .exactList (patternListOf [.bind "saved", .literal (.integer 2)])
    let value := listValue [.scalar (.integer 1), .scalar (.integer 9)]
    let tentative := [("saved", .scalar (.integer 1))]
    let result := attemptPattern pattern value
    result.matched = false ∧ result.tentativeCaptures = tentative ∧
      result.publishedCaptures = [] ∧ result.callbackTrace = [[0], [1]] ∧
      result.trace = [.elementMatcherCalled [0],
        .captureTentative [0] "saved" (.scalar (.integer 1)),
        .elementMatcherCalled [1], .capturesDiscarded tentative] ∧
      leakTentativeCapturesOnMiss pattern value = tentative ∧
      leakTentativeCapturesOnMiss pattern value ≠ result.publishedCaptures := by
  decide

end Xsh.Patterns.ListMatcher
