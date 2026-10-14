/-!
This fragment models finite ordered match-arm selection after the subject has
been evaluated. Pattern tests return abstract success or failure decisions,
successful tests supply complete captures, and guards supply their Boolean
result. The trace records arm tests, guard calls, and selection. It does not
model concrete pattern matching, exhaustiveness or type diagnostics, guard
expression semantics beyond the supplied result, parsing, or Rust/runtime
correspondence.
-/

namespace Xsh.Matches.Selection

universe u

/-- A failed pattern may have considered partial captures, but publishes none. -/
inductive PatternDecision (Capture : Type u) where
  | miss (candidate : Capture)
  | matched (captures : Capture)

/-- Arms contain only the decisions needed by ordered selection. The `else` arm
has no pattern, guard, or captures. -/
inductive MatchArm (Subject Capture Value : Type u) where
  | pattern (test : Subject → PatternDecision Capture)
      (guard : Option Bool) (value : Value)
  | elseArm (value : Value)

/-- Events expose evaluation order without modeling the expressions themselves. -/
inductive Event (Capture : Type u) where
  | subjectEvaluated
  | patternTested (arm : Nat)
  | guardEvaluated (arm : Nat) (captures : Capture) (accepted : Bool)
  | patternSelected (arm : Nat) (captures : Capture)
  | elseSelected (arm : Nat)
  | noArmFault
  deriving DecidableEq, Repr

/-- A selected ordinary arm carries complete captures; the catch-all carries none. -/
inductive Outcome (Capture Value : Type u) where
  | patternSelected (arm : Nat) (captures : Capture) (value : Value)
  | elseSelected (arm : Nat) (value : Value)
  | noArmFault
  deriving DecidableEq, Repr

/-- One finite selection result includes the evaluated subject and trace. -/
structure Evaluation (Subject Capture Value : Type u) where
  subject : Subject
  finalState : Nat
  outcome : Outcome Capture Value
  trace : List (Event Capture)
  deriving DecidableEq, Repr

/-- Try arms in list order, passing captures to a guard only after a full match. -/
def selectArms {Subject Capture Value : Type u} (subject : Subject) :
    Nat → List (MatchArm Subject Capture Value) →
      Outcome Capture Value × List (Event Capture)
  | _index, [] => (.noArmFault, [.noArmFault])
  | index, .elseArm value :: _ =>
      (.elseSelected index value, [.elseSelected index])
  | index, .pattern test guard value :: rest =>
      match test subject with
      | .miss _ =>
          let (outcome, trace) := selectArms subject (index + 1) rest
          (outcome, .patternTested index :: trace)
      | .matched captures =>
          match guard with
          | none =>
              (.patternSelected index captures value,
                [.patternTested index, .patternSelected index captures])
          | some accepted =>
              let guardEvent := .guardEvaluated index captures accepted
              if accepted then
                (.patternSelected index captures value,
                  [.patternTested index, guardEvent, .patternSelected index captures])
              else
                let (outcome, trace) := selectArms subject (index + 1) rest
                (outcome, .patternTested index :: guardEvent :: trace)

/-- Evaluate the subject expression once, then select over its single value. -/
def evaluate {Subject Capture Value : Type u}
    (subjectExpression : Nat → Subject × Nat) (state : Nat)
    (arms : List (MatchArm Subject Capture Value)) :
    Evaluation Subject Capture Value :=
  let (subject, finalState) := subjectExpression state
  let (outcome, armTrace) := selectArms subject 0 arms
  { subject := subject
    finalState := finalState
    outcome := outcome
    trace := .subjectEvaluated :: armTrace }

/-- Test-only mutation that moves a catch-all ahead of the first ordinary arm. -/
private def moveElseAhead {Subject Capture Value : Type u}
    (arms : List (MatchArm Subject Capture Value)) :
    List (MatchArm Subject Capture Value) :=
  match arms with
  | .pattern test guard value :: .elseArm fallback :: rest =>
      .elseArm fallback :: .pattern test guard value :: rest
  | _ => arms

private def incrementingSubject : Nat → Nat × Nat :=
  fun state => (state, state + 1)

/-- The stateful subject witness records one evaluation before arm selection. -/
theorem subject_expression_is_evaluated_once :
    let result := evaluate incrementingSubject 10
      [.pattern (fun subject => .matched subject) none 100]
    result.subject = 10 ∧ result.finalState = 11 ∧
      result.trace = [.subjectEvaluated, .patternTested 0,
        .patternSelected 0 10] := by
  decide

/-- Failed patterns are tested in their source order before the first match. -/
theorem pattern_tests_follow_arm_order :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .miss 81) none 10,
       .pattern (fun _ => .miss 82) none 20,
       .pattern (fun _ => .matched 7) none 30]
    result.outcome = .patternSelected 2 7 30 ∧
      result.trace = [.subjectEvaluated, .patternTested 0,
        .patternTested 1, .patternTested 2, .patternSelected 2 7] := by
  decide

/-- A pattern miss skips its supplied guard result. -/
theorem pattern_miss_skips_guard :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .miss 81) (some true) 10,
       .elseArm 20]
    result.outcome = .elseSelected 1 20 ∧
      result.trace = [.subjectEvaluated, .patternTested 0,
        .elseSelected 1] := by
  decide

/-- A failed first pattern advances to the next arm. -/
theorem pattern_miss_advances_to_next_arm :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .miss 81) none 10,
       .pattern (fun _ => .matched 7) none 20]
    result.outcome = .patternSelected 1 7 20 ∧
      result.trace = [.subjectEvaluated, .patternTested 0,
        .patternTested 1, .patternSelected 1 7] := by
  decide

/-- A successful pattern supplies its captures to the following guard. -/
theorem successful_pattern_precedes_guard :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .matched 7) (some true) 10]
    result.trace = [.subjectEvaluated, .patternTested 0,
      .guardEvaluated 0 7 true, .patternSelected 0 7] := by
  rfl

/-- A false guard rejects its arm and selection continues in source order. -/
theorem false_guard_advances_to_next_arm :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .matched 7) (some false) 10,
       .pattern (fun _ => .matched 8) none 20]
    result.outcome = .patternSelected 1 8 20 ∧
      result.trace = [.subjectEvaluated, .patternTested 0,
        .guardEvaluated 0 7 false, .patternTested 1,
        .patternSelected 1 8] := by
  decide

/-- A true guard selects the arm whose pattern just matched. -/
theorem true_guard_selects_current_arm :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .matched 7) (some true) 10,
       .elseArm 20]
    result.outcome = .patternSelected 0 7 10 := by
  rfl

/-- Selection stops before testing any later arm after a true guard. -/
theorem selected_arm_skips_later_tests :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .matched 7) (some true) 10,
       .pattern (fun _ => .matched 8) (some true) 20,
       .elseArm 30]
    result.trace = [.subjectEvaluated, .patternTested 0,
      .guardEvaluated 0 7 true, .patternSelected 0 7] := by
  rfl

/-- The `else` catch-all runs after earlier patterns miss or guards reject. -/
theorem else_runs_after_prior_rejections :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .miss 81) (some true) 10,
       .pattern (fun _ => .matched 8) (some false) 20,
       .elseArm 30]
    result.outcome = .elseSelected 2 30 ∧
      result.trace = [.subjectEvaluated, .patternTested 0,
        .patternTested 1, .guardEvaluated 1 8 false,
        .elseSelected 2] := by
  decide

/-- Exhausting a match without a catch-all produces the explicit no-arm fault. -/
theorem exhausted_arms_produce_no_arm_fault :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .miss 81) (some true) 10]
    result.outcome = .noArmFault ∧
      result.trace = [.subjectEvaluated, .patternTested 0, .noArmFault] := by
  decide

/-- Partial captures from a miss are discarded; only the later complete match
publishes captures to its guard and selected outcome. -/
theorem captures_are_published_only_after_pattern_success :
    let result := evaluate incrementingSubject 0
      [.pattern (fun _ => .miss 81) (some true) 10,
       .pattern (fun _ => .matched 7) (some true) 20]
    result.outcome = .patternSelected 1 7 20 ∧
      result.trace = [.subjectEvaluated, .patternTested 0,
        .patternTested 1, .guardEvaluated 1 7 true,
        .patternSelected 1 7] := by
  decide

/-- Moving an earlier `else` arm ahead changes selection and its trace. -/
theorem reordered_else_mutation_changes_selection :
    let arms : List (MatchArm Nat Nat Nat) :=
      [.pattern (fun _ => .matched 7) none 10, .elseArm 20]
    let original := evaluate incrementingSubject 0 arms
    let reordered := evaluate incrementingSubject 0 (moveElseAhead arms)
    original.outcome = .patternSelected 0 7 10 ∧
      original.trace = [.subjectEvaluated, .patternTested 0,
        .patternSelected 0 7] ∧
      reordered.outcome = .elseSelected 0 20 ∧
      reordered.trace = [.subjectEvaluated, .elseSelected 0] := by
  decide

end Xsh.Matches.Selection
