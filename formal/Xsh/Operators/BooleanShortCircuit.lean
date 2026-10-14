/-!
This fragment evaluates a finite Boolean expression tree with observable state
writes and failures. Its twelve results cover `&&` and `||` short-circuiting,
left-to-right order, nested evaluation, and a mutation witness against eager
right-hand evaluation. It does not model checker behavior, diagnostics,
general expression semantics, the XSH runtime, or Rust correspondence.
-/

namespace Xsh.Operators.BooleanShortCircuit

universe u

/-- A finite expression either returns a Boolean, writes state, fails, or
combines two expressions with a short-circuiting Boolean operator. -/
inductive Expr (Error : Type u) where
  | literal (value : Bool)
  | writeState (nextState : Nat) (value : Bool)
  | fail (error : Error)
  | and (left right : Expr Error)
  | or (left right : Expr Error)
  deriving DecidableEq, Repr

/-- State writes are retained in the observable evaluation trace. -/
inductive Event where
  | writeState (value : Nat)
  deriving DecidableEq, Repr

/-- The final state, ordered effects, and result of one finite evaluation. -/
structure Evaluation (Error : Type u) where
  trace : List Event
  finalState : Nat
  result : Except Error Bool
  deriving Repr

/-- Evaluate the finite tree from left to right, visiting only the selected
right operand of `&&` or `||`. -/
def evaluate {Error : Type u} : Expr Error → Nat → Evaluation Error
  | .literal value, state => ⟨[], state, .ok value⟩
  | .writeState nextState value, _ =>
      ⟨[.writeState nextState], nextState, .ok value⟩
  | .fail error, state => ⟨[], state, .error error⟩
  | .and left right, state =>
      let leftEvaluation := evaluate left state
      match leftEvaluation.result with
      | .error _ => leftEvaluation
      | .ok false => { leftEvaluation with result := .ok false }
      | .ok true =>
          let rightEvaluation := evaluate right leftEvaluation.finalState
          { rightEvaluation with trace := leftEvaluation.trace ++ rightEvaluation.trace }
  | .or left right, state =>
      let leftEvaluation := evaluate left state
      match leftEvaluation.result with
      | .error _ => leftEvaluation
      | .ok true => { leftEvaluation with result := .ok true }
      | .ok false =>
          let rightEvaluation := evaluate right leftEvaluation.finalState
          { rightEvaluation with trace := leftEvaluation.trace ++ rightEvaluation.trace }

/-- A strict mutant evaluates both operands before combining their Boolean
results, so writes from an unnecessary right operand remain observable. -/
private def evaluateEagerAnd {Error : Type u} (left right : Expr Error)
    (state : Nat) : Evaluation Error :=
  let leftEvaluation := evaluate left state
  let rightEvaluation := evaluate right leftEvaluation.finalState
  let result := match leftEvaluation.result with
    | .error error => .error error
    | .ok leftValue =>
        match rightEvaluation.result with
        | .error error => .error error
        | .ok rightValue => .ok (leftValue && rightValue)
  ⟨leftEvaluation.trace ++ rightEvaluation.trace,
    rightEvaluation.finalState, result⟩

/-- Effectful left operands leave one write in the trace for either operator. -/
theorem left_evaluated_once (initialState : Nat) :
    evaluate (Error := Unit)
        (.and (.writeState 11 false) (.writeState 12 true)) initialState =
        ⟨[.writeState 11], 11, .ok false⟩ ∧
      evaluate (Error := Unit)
          (.or (.writeState 11 true) (.writeState 12 false)) initialState =
        ⟨[.writeState 11], 11, .ok true⟩ := by
  simp [evaluate]

/-- A false left operand of `&&` preserves every part of the state and outcome. -/
theorem false_and_skips_right {Error : Type u} (right : Expr Error)
    (initialState : Nat) :
    evaluate (.and (.literal false) right) initialState =
      ⟨[], initialState, .ok false⟩ := by
  rfl

/-- A true left operand of `&&` produces exactly the single right evaluation. -/
theorem true_and_evaluates_right_once {Error : Type u} (right : Expr Error)
    (initialState : Nat) :
    evaluate (.and (.literal true) right) initialState =
      evaluate right initialState := by
  rfl

/-- A true left operand of `||` preserves every part of the state and outcome. -/
theorem true_or_skips_right {Error : Type u} (right : Expr Error)
    (initialState : Nat) :
    evaluate (.or (.literal true) right) initialState =
      ⟨[], initialState, .ok true⟩ := by
  rfl

/-- A false left operand of `||` produces exactly the single right evaluation. -/
theorem false_or_evaluates_right_once {Error : Type u} (right : Expr Error)
    (initialState : Nat) :
    evaluate (.or (.literal false) right) initialState =
      evaluate right initialState := by
  rfl

/-- A left failure from `&&` is returned unchanged and prevents right effects. -/
theorem and_left_failure_propagates {Error : Type u} (error : Error)
    (right : Expr Error) (initialState : Nat) :
    evaluate (.and (.fail error) right) initialState =
      ⟨[], initialState, .error error⟩ := by
  rfl

/-- A left failure from `||` is returned unchanged and prevents right effects. -/
theorem or_left_failure_propagates {Error : Type u} (error : Error)
    (right : Expr Error) (initialState : Nat) :
    evaluate (.or (.fail error) right) initialState =
      ⟨[], initialState, .error error⟩ := by
  rfl

/-- A reached failing right operand of `&&` propagates its error. -/
theorem and_reached_right_failure_propagates {Error : Type u} (error : Error)
    (initialState : Nat) :
    evaluate (.and (.literal true) (.fail error)) initialState =
      ⟨[], initialState, .error error⟩ := by
  rfl

/-- A reached failing right operand of `||` propagates its error. -/
theorem or_reached_right_failure_propagates {Error : Type u} (error : Error)
    (initialState : Nat) :
    evaluate (.or (.literal false) (.fail error)) initialState =
      ⟨[], initialState, .error error⟩ := by
  rfl

/-- Reached operand writes appear in source order and leave the last state. -/
theorem operand_effects_preserve_left_to_right_trace :
    evaluate (Error := Unit)
        (.and (.writeState 21 true) (.writeState 22 false)) 0 =
      ⟨[.writeState 21, .writeState 22], 22, .ok false⟩ := by
  rfl

/-- Nested operators finish the inner left-to-right trace before the outer
operator evaluates its reached right operand. -/
theorem nested_boolean_evaluation_is_left_to_right :
    evaluate (Error := Unit)
      (.or
        (.and (.writeState 31 true) (.writeState 32 false))
        (.writeState 33 true)) 0 =
      ⟨[.writeState 31, .writeState 32, .writeState 33], 33, .ok true⟩ := by
  rfl

/-- Eager right evaluation mutates state even though false `&&` skips it. -/
theorem eager_rhs_mutation_is_detectable :
    let lazy : Evaluation Unit :=
      evaluate (.and (.literal false) (.writeState 41 true)) 0
    let eager : Evaluation Unit :=
      evaluateEagerAnd (.literal false) (.writeState 41 true) 0
    lazy = ⟨[], 0, .ok false⟩ ∧
      eager = ⟨[.writeState 41], 41, .ok false⟩ ∧
      lazy ≠ eager := by
  constructor
  · rfl
  constructor
  · rfl
  · intro equal
    have impossible : (0 : Nat) = 41 := by
      simpa [evaluate, evaluateEagerAnd] using
        congrArg Evaluation.finalState equal
    cases impossible

end Xsh.Operators.BooleanShortCircuit
