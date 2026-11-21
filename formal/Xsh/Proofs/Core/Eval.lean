import Xsh.Semantics.CoreEval
import Xsh.Proofs.Base.Num
import Xsh.Proofs.Core.Statements
import Xsh.Validation.Core.Preserve

namespace Xsh.Proofs.Core.Eval

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.FloatOps
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreEval
open Xsh.Proofs.Base.Num

variable {Declaration Identity Source : Type}

section Meanings

theorem integer_operator_meaning (context : Context Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (op : IntegerOps.IntegerBinaryOp) (found : integerOperator operator = some op) (left right : IntValue) :
    ∃ result, ArithmeticMeaning op left right result ∧
      binary context operator (integer left) (integer right) = integerResult context.errors result :=
  ⟨_, applied_operation_has_its_meaning op left right, by simp [binary, integer, found, Value.eraseNominal]⟩

theorem float_operator_meaning (context : Context Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (op : FloatBinaryOp) (found : floatOperator operator = some op) (left right : FloatBits) :
    ∃ result, FloatMeaning op left right result ∧ binary context operator (float left) (float right) = .value (float result) :=
  ⟨_, float_apply_meaning context.policy op left right, by simp [binary, float, found, Value.eraseNominal]⟩

theorem float_ordering_meaning (context : Context Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (comparison : FloatComparison) (found : ordering operator = some comparison) (left right : FloatBits) :
    ∃ result, ComparisonMeaning comparison left right result ∧
      binary context operator (float left) (float right) = .value (boolean result) := by
  have noArithmetic : floatOperator operator = none := by cases operator <;> simp_all [ordering, floatOperator]
  exact ⟨_, float_compare_meaning comparison left right, by simp [binary, float, noArithmetic, found, Value.eraseNominal]⟩

theorem float_equality_meaning (context : Context Declaration Identity Source) (left right : FloatBits) :
    binary context .eq (float left) (float right) = .value (boolean (floatValueEq left right)) ∧
      (floatValueEq left right = true ↔ ieeeFloatEq left right) :=
  ⟨by simp [binary, float, floatOperator, ordering, Value.eraseNominal], float_value_eq_spec left right⟩

theorem negation_meaning (context : Context Declaration Identity Source) (value : IntValue) (bits : FloatBits) :
    (∃ result, IntegerExact (-value.val) result ∧
      unary context .neg (integer value) = integerResult context.errors result) ∧
    (∃ result, NegationMeaning bits result ∧ unary context .neg (float bits) = .value (float result)) :=
  ⟨⟨_, negation_is_exact value, by simp [unary, integer, Value.eraseNominal]⟩,
    ⟨_, float_negate_meaning bits, by simp [unary, float, Value.eraseNominal]⟩⟩

theorem duration_operator_results (context : Context Declaration Identity Source) (left right : DurationValue)
    (factor : IntValue) :
    binary context .add (duration left) (duration right) = durationResult context.errors (DurationOps.add left right) ∧
    binary context .sub (duration left) (duration right) =
      durationResult context.errors (DurationOps.subtract left right) ∧
    binary context .div (duration left) (duration right) = durationResult context.errors (DurationOps.ratio left right) ∧
    binary context .mul (duration left) (integer factor) =
      durationResult context.errors (DurationOps.multiply left factor) ∧
    binary context .div (duration left) (integer factor) =
      durationResult context.errors (DurationOps.divideByInt left factor) := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> simp [binary, duration, integer, ordering, Value.eraseNominal]

theorem not_scalar_of_listItems {candidate : Value Declaration Identity Source}
    {items : List (Value Declaration Identity Source)}
    (listed : Xsh.Validation.Core.Preserve.listItems candidate = some items) : ∀ scalar, candidate ≠ .scalar scalar := by
  intro scalar same
  subst same
  simp [Xsh.Validation.Core.Preserve.listItems, Value.eraseNominal] at listed

theorem list_addition_is_concatenation (context : Context Declaration Identity Source)
    (left right result : Value Declaration Identity Source)
    (extended : Xsh.Validation.Core.Preserve.concatenate left right = some result) :
    binary context .add left right = .value result := by
  unfold Xsh.Validation.Core.Preserve.concatenate at extended
  split at extended
  · rename_i first second leftListed rightListed
    simp only [Option.some.injEq] at extended
    subst extended
    have leftNot := not_scalar_of_listItems leftListed
    have rightNot := not_scalar_of_listItems rightListed
    have leftValues : listValues left = some first := leftListed
    have rightValues : listValues right = some second := rightListed
    have leftErased : ∀ scalar, left.eraseNominal ≠ .scalar scalar := by
      intro scalar erased
      simp [Xsh.Validation.Core.Preserve.listItems, erased] at leftListed
    unfold binary
    split
    all_goals first
      | (rename_i a b leftShape _; exact absurd leftShape (leftErased _))
      | skip
    simp [leftValues, rightValues]
  · simp at extended

theorem operate_adds_lists (context : Context Declaration Identity Source) (fallback : Operate Declaration Identity Source)
    (left right result : Value Declaration Identity Source)
    (extended : Xsh.Validation.Core.Preserve.concatenate left right = some result) :
    operate context fallback .add left right = .ok result := by
  simp [operate, assignBinary, list_addition_is_concatenation context left right result extended]

end Meanings

section Fuel

def Refines {α : Type} (first second : Result Declaration Identity Source α) : Prop :=
  first = .outOfFuel ∨ first = second

theorem refines_refl {α : Type} (result : Result Declaration Identity Source α) : Refines result result := Or.inr rfl

theorem bind_refines {α β : Type} {first second : Result Declaration Identity Source α}
    {next next' : α → Result Declaration Identity Source β} (head : Refines first second)
    (tail : ∀ value, Refines (next value) (next' value)) : Refines (first.bind next) (second.bind next') := by
  rcases head with out | same
  · subst out
    exact Or.inl rfl
  · subst same
    cases first with
    | value value => exact tail value
    | _ => exact Or.inr rfl

theorem evaluateAll_refines {α : Type}
    {evaluate evaluate' : α → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ item, Refines (evaluate item) (evaluate' item)) :
    ∀ items : List α, Refines (evaluateAll evaluate items) (evaluateAll evaluate' items)
  | [] => refines_refl _
  | first :: rest => bind_refines (each first) fun _ =>
      bind_refines (evaluateAll_refines each rest) fun _ => refines_refl _

theorem evaluateFields_refines (context : Context Declaration Identity Source)
    {evaluate evaluate' : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ item, Refines (evaluate item) (evaluate' item)) :
    ∀ fields, Refines (evaluateFields context evaluate fields) (evaluateFields context evaluate' fields)
  | [] => refines_refl _
  | .named name operand :: rest => bind_refines (each operand) fun _ =>
      bind_refines (evaluateFields_refines context each rest) fun _ => refines_refl _
  | .computed _ _ :: _ | .path _ _ :: _ | .shorthand _ :: _ | .spread _ :: _ => refines_refl _

theorem branches_refines
    {evaluate evaluate' : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ item, Refines (evaluate item) (evaluate' item)) :
    ∀ conditions otherwise, Refines (branches evaluate conditions otherwise) (branches evaluate' conditions otherwise)
  | [], otherwise => each otherwise
  | (condition, value) :: rest, otherwise => by
    simp only [branches]
    refine bind_refines (each condition) fun result => ?_
    split
    · exact each value
    · exact branches_refines each rest otherwise
    · exact refines_refl _

theorem step_refines (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world)
    {recurse recurse' : Locals Declaration Identity Source → Xsh.Syntax.Expr →
      Result Declaration Identity Source (Value Declaration Identity Source)}
    (inner : ∀ locals expression, Refines (recurse locals expression) (recurse' locals expression))
    (locals : Locals Declaration Identity Source) (expression : Xsh.Syntax.Expr) :
    Refines (step context environment store recurse locals expression)
      (step context environment store recurse' locals expression) := by
  cases expression with
  | list elements => exact bind_refines (evaluateAll_refines (inner locals) elements) fun _ => refines_refl _
  | record fields => exact bind_refines (evaluateFields_refines context (inner locals) fields) fun _ => refines_refl _
  | field base name => exact bind_refines (inner locals base) fun _ => refines_refl _
  | index base key guarded =>
    cases guarded with
    | true => exact refines_refl _
    | false =>
      simp only [step]
      refine bind_refines (inner locals base) fun container => ?_
      split
      · exact bind_refines (refines_refl _) fun _ => refines_refl _
      · exact bind_refines (inner locals _) fun _ => refines_refl _
  | unary operator operand => exact bind_refines (inner locals operand) fun _ => refines_refl _
  | binary operator left right =>
    cases operator
    case and =>
      simp only [step]
      refine bind_refines (inner locals left) fun value => ?_
      split
      · exact refines_refl _
      · exact bind_refines (inner locals right) fun _ => refines_refl _
      · exact refines_refl _
    case or =>
      simp only [step]
      refine bind_refines (inner locals left) fun value => ?_
      split
      · exact refines_refl _
      · exact bind_refines (inner locals right) fun _ => refines_refl _
      · exact refines_refl _
    all_goals exact bind_refines (inner locals left) fun _ => bind_refines (inner locals right) fun _ => refines_refl _
  | ifExpr conditions otherwise => exact branches_refines (inner locals) conditions otherwise
  | call callee arguments =>
    cases callee
    case ident name =>
      simp only [step]
      split
      · exact bind_refines (evaluateAll_refines (inner locals) _) fun _ => by
          split
          · exact inner _ _
          · exact refines_refl _
      · exact refines_refl _
    all_goals exact refines_refl _
  | _ => exact refines_refl _

theorem evaluate_refines (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world) :
    ∀ (fuel extra : Nat) locals expression,
      Refines (evaluate context environment store fuel locals expression)
        (evaluate context environment store (fuel + extra) locals expression)
  | 0, _, _, _ => Or.inl rfl
  | fuel + 1, extra, locals, expression => by
    rw [show fuel + 1 + extra = (fuel + extra) + 1 by omega]
    exact step_refines context environment store
      (fun locals expression => evaluate_refines context environment store fuel extra locals expression) locals expression

/-- Evaluation is a function of the expression, the environment and the store:
a result other than running out of fuel is the same at every larger fuel, so
two fuels that both finish agree. -/
theorem evaluate_deterministic (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world) (locals : Locals Declaration Identity Source)
    (expression : Xsh.Syntax.Expr) (first second : Nat)
    (firstFinished : evaluate context environment store first locals expression ≠ .outOfFuel)
    (secondFinished : evaluate context environment store second locals expression ≠ .outOfFuel) :
    evaluate context environment store first locals expression =
      evaluate context environment store second locals expression := by
  rcases Nat.le_total first second with smaller | larger
  · obtain ⟨extra, rfl⟩ := Nat.exists_eq_add_of_le smaller
    rcases evaluate_refines context environment store first extra locals expression with out | same
    · exact absurd out firstFinished
    · exact same
  · obtain ⟨extra, rfl⟩ := Nat.exists_eq_add_of_le larger
    rcases evaluate_refines context environment store second extra locals expression with out | same
    · exact absurd out secondFinished
    · exact same.symm

end Fuel

end Xsh.Proofs.Core.Eval
