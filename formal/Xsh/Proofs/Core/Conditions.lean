import Xsh.Proofs.Core.Records

namespace Xsh.Proofs.Core.Conditions

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

/-- The `control_conditions` fragment root (SPEC 8.3): in a control position
a `Result[Bool]` condition gives the `Bool` of its `Ok` and raises the error of
its `Err`, as under `?`, and so does each operand of `!`, `and` and `or`
there; `and` with a false left operand and `or` with a true one give that
value without running the right operand, which therefore cannot fail; and
`if` and `while` test their conditions this way. -/
theorem control_conditions [DecidableEq Declaration] {world : World Declaration Identity Source}
    (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) (frame after : Frame world) :
    (∀ left right, controlValue evaluate frame left = .value (.scalar (.boolean false)) after →
      controlValue evaluate frame (.binary .and left right) = .value (.scalar (.boolean false)) after) ∧
    (∀ left right, controlValue evaluate frame left = .value (.scalar (.boolean true)) after →
      controlValue evaluate frame (.binary .or left right) = .value (.scalar (.boolean true)) after) ∧
    (∀ left right, controlValue evaluate frame left = .value (.scalar (.boolean true)) after →
      controlValue evaluate frame (.binary .and left right) = controlValue evaluate after right) ∧
    (∀ operand value, controlValue evaluate frame operand = .value (.scalar (.boolean value)) after →
      controlValue evaluate frame (.unary .not operand) = .value (.scalar (.boolean !value)) after) ∧
    (∀ condition (tested : Value Declaration Identity Source),
      (∀ left right, condition ≠ .binary .and left right) → (∀ left right, condition ≠ .binary .or left right) →
      (∀ operand, condition ≠ .unary .not operand) → evaluate frame condition = .value tested after →
        controlValue evaluate frame condition =
          match conditionValue tested with
          | some (.ok value) => .value (.scalar (.boolean value)) after
          | some (.error error) => .exec (.failed (.raised error) after)
          | none => .exec .unsupported) ∧
    (∀ (tested : Value Declaration Identity Source) error, tested.eraseNominal = .err error →
      conditionValue tested = some (.error error)) ∧
    (∀ (tested inner : Value Declaration Identity Source) value, tested.eraseNominal = .ok inner →
      inner.eraseNominal = .scalar (.boolean value) → conditionValue tested = some (.ok value)) := by
  refine ⟨fun left right ran => ?_, fun left right ran => ?_, fun left right ran => ?_,
    fun operand value ran => ?_, fun condition tested notAnd notOr notNot ran => ?_,
    fun tested error shape => by simp [conditionValue, shape],
    fun tested inner value shape inside => by simp [conditionValue, shape, inside]⟩
  · simp [controlValue, ran, Outcome.bind, Value.eraseNominal]
  · simp [controlValue, ran, Outcome.bind, Value.eraseNominal]
  · simp [controlValue, ran, Outcome.bind, Value.eraseNominal]
  · simp [controlValue, ran, Outcome.bind, Value.eraseNominal]
  · rw [controlValue.eq_4 evaluate frame condition (fun left right same => notAnd left right same)
      (fun left right same => notOr left right same) (fun operand same => notNot operand same), ran]
    rfl

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness

def boom : Value Unit Unit Unit := .scalar (.string "boom")

/-- A frame holding `Ok(false)`, `Ok(true)` and `Err("boom")`. -/
def holding : Frame witnessWorld :=
  { start with locals := [(rName, .ok (.scalar (.boolean false))), (fName, .ok (.scalar (.boolean true))),
      (gName, .err boom)] }

def runHolding (body : List Xsh.Syntax.Statement) : Exec witnessWorld := runStatements machine 40 holding body

def ifThen (condition : Xsh.Syntax.Expr) (lexeme : String) : List Xsh.Syntax.Statement :=
  [varOf totalName "0", .ifStmt [(condition, [set totalName lexeme])] none]

def raisedBoom : Exec witnessWorld → Bool
  | .failed (.raised (.scalar (.string "boom"))) _ => true
  | _ => false

/-- As the verification binary runs `if ok(false) and bad()` and
`if ok(true) or bad()` without calling `bad()`, prints `not` for
`if ! ok(false)`, and leaves `check` with `Err` at `if ok(true) and bad()`. -/
theorem result_conditions :
    readInt totalName (runHolding (ifThen (.binary .and (ident rName) (ident gName)) "1")) = some 0 ∧
    readInt totalName (runHolding (ifThen (.binary .or (ident fName) (ident gName)) "7")) = some 7 ∧
    readInt totalName (runHolding (ifThen (.unary .not (ident rName)) "8")) = some 8 ∧
    raisedBoom (runHolding (ifThen (.binary .and (ident fName) (ident gName)) "1")) = true := by
  decide +kernel

/-- As the verification binary counts `while ok(i < 2) { i += 1 }` to `2`. -/
theorem result_while :
    readInt totalName (runHolding [varOf totalName "0",
      .whileStmt (.binary .and (ident fName) (.binary .lt (ident totalName) (int "2"))) [bump totalName (int "1")]]) =
      some 2 := by
  decide +kernel

/-- Testing both operands of `and` before combining them raises `boom` where
the model skips the right operand. -/
theorem eager_and_rejected :
    raisedBoom (runHolding (ifThen (ident gName) "1")) = true ∧
      readInt totalName (runHolding (ifThen (.binary .and (ident rName) (ident gName)) "1")) = some 0 := by
  decide +kernel

/-- An `and` that tests its operands as plain `Bool`s cannot run on `Ok(false)`;
the model unwraps it. -/
theorem bool_only_operands_rejected :
    (match (Value.ok (.scalar (.boolean false)) : Value Unit Unit Unit) with
      | .scalar (.boolean _) => true
      | _ => false) = false ∧
      readInt totalName (runHolding (ifThen (.binary .or (ident rName) (ident fName)) "7")) = some 7 := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Conditions
