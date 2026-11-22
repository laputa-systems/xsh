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
      durationResult context.errors (DurationOps.divideByInt left factor) ∧
    binary context .mul (integer factor) (duration right) =
      durationResult context.errors (DurationOps.multiply right factor) ∧
    binary context .div (integer factor) (duration right) = .unsupported := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> simp [binary, duration, integer, Value.eraseNominal]

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
  simp [operate, compoundBinary, list_addition_is_concatenation context left right result extended]

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
    (each : ∀ source, Refines (evaluate source) (evaluate' source)) :
    ∀ items : List α, Refines (evaluateAll evaluate items) (evaluateAll evaluate' items)
  | [] => refines_refl _
  | first :: rest => bind_refines (each first) fun _ =>
      bind_refines (evaluateAll_refines each rest) fun _ => refines_refl _

theorem evaluateFields_refines (context : Context Declaration Identity Source)
    {evaluate evaluate' : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, Refines (evaluate source) (evaluate' source)) :
    ∀ fields, Refines (evaluateFields context evaluate fields) (evaluateFields context evaluate' fields)
  | [] => refines_refl _
  | .named _ operand :: rest => bind_refines (each operand) fun _ =>
      bind_refines (evaluateFields_refines context each rest) fun _ => refines_refl _
  | .computed _ _ :: _ | .path _ _ :: _ | .shorthand _ :: _ | .spread _ :: _ => refines_refl _

theorem controlCondition_refines
    {evaluate evaluate' : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, Refines (evaluate source) (evaluate' source)) :
    ∀ condition, Refines (controlCondition evaluate condition) (controlCondition evaluate' condition) := by
  intro condition
  fun_induction controlCondition evaluate condition with
  | case1 left right ihLeft ihRight | case2 left right ihLeft ihRight =>
    simp only [controlCondition]
    refine bind_refines ihLeft fun first => ?_
    split <;> first | exact refines_refl _ | exact ihRight
  | case3 operand ih =>
    simp only [controlCondition]
    exact bind_refines ih fun _ => refines_refl _
  | case4 condition notAnd notOr notNot =>
    rw [controlCondition.eq_4 evaluate' condition notAnd notOr notNot]
    exact bind_refines (each _) fun _ => refines_refl _

theorem branches_refines
    {evaluate evaluate' : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, Refines (evaluate source) (evaluate' source)) :
    ∀ conditions otherwise, Refines (branches evaluate conditions otherwise) (branches evaluate' conditions otherwise)
  | [], otherwise => each otherwise
  | (condition, value) :: rest, otherwise => by
    simp only [branches]
    refine bind_refines (controlCondition_refines each condition) fun result => ?_
    split
    · exact each value
    · exact branches_refines each rest otherwise
    · exact refines_refl _

theorem step_refines (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world)
    {recurse recurse' : Environment → Locals Declaration Identity Source → Xsh.Syntax.Expr →
      Result Declaration Identity Source (Value Declaration Identity Source)}
    (inner : ∀ environment locals expression,
      Refines (recurse environment locals expression) (recurse' environment locals expression))
    {callBlock callBlock' : Environment → Locals Declaration Identity Source → List Xsh.Syntax.Statement → ReturnKind →
      Result Declaration Identity Source (Value Declaration Identity Source)}
    (blocks : ∀ environment locals statements kind,
      Refines (callBlock environment locals statements kind) (callBlock' environment locals statements kind))
    (locals : Locals Declaration Identity Source) (expression : Xsh.Syntax.Expr) :
    Refines (step context environment store recurse callBlock locals expression)
      (step context environment store recurse' callBlock' locals expression) := by
  cases expression with
  | list elements => exact bind_refines (evaluateAll_refines (inner environment locals) elements) fun _ => refines_refl _
  | record fields => exact bind_refines (evaluateFields_refines context (inner environment locals) fields) fun _ => refines_refl _
  | field base name => exact bind_refines (inner environment locals base) fun _ => refines_refl _
  | index base key guarded =>
    cases guarded with
    | true => exact refines_refl _
    | false =>
      simp only [step]
      exact bind_refines (inner environment locals base) fun _ =>
        bind_refines (inner environment locals key) fun _ => refines_refl _
  | unary operator operand => exact bind_refines (inner environment locals operand) fun _ => refines_refl _
  | binary operator left right =>
    cases operator
    case and =>
      simp only [step]
      refine bind_refines (inner environment locals left) fun value => ?_
      split
      · exact refines_refl _
      · exact bind_refines (inner environment locals right) fun _ => refines_refl _
      · exact refines_refl _
    case or =>
      simp only [step]
      refine bind_refines (inner environment locals left) fun value => ?_
      split
      · exact refines_refl _
      · exact bind_refines (inner environment locals right) fun _ => refines_refl _
      · exact refines_refl _
    all_goals exact bind_refines (inner environment locals left) fun _ => bind_refines (inner environment locals right) fun _ => refines_refl _
  | ifExpr conditions otherwise => exact branches_refines (inner environment locals) conditions otherwise
  | call callee arguments =>
    cases callee
    case ident name =>
      simp only [step]
      split
      · exact bind_refines (evaluateAll_refines (inner environment locals) _) fun _ => by
          split
          · split <;> first | exact inner _ _ _ | exact blocks _ _ _ _
          · exact refines_refl _
      · exact refines_refl _
    all_goals exact refines_refl _
  | _ => exact refines_refl _

theorem evaluate_refines (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (store : Store world) :
    ∀ (fuel extra : Nat) environment locals expression,
      Refines (evaluate context environment store fuel locals expression)
        (evaluate context environment store (fuel + extra) locals expression)
  | 0, _, _, _, _ => Or.inl rfl
  | fuel + 1, extra, environment, locals, expression => by
    rw [show fuel + 1 + extra = (fuel + extra) + 1 by omega]
    exact step_refines context environment store
      (fun environment locals expression => evaluate_refines context store fuel extra environment locals expression)
      (fun _ _ _ _ => refines_refl _) locals expression

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
    rcases evaluate_refines context store first extra environment locals expression with out | same
    · exact absurd out firstFinished
    · exact same
  · obtain ⟨extra, rfl⟩ := Nat.exists_eq_add_of_le larger
    rcases evaluate_refines context store second extra environment locals expression with out | same
    · exact absurd out secondFinished
    · exact same.symm

end Fuel

section Failures

/-- Every failure the result carries is a runtime failure, which `try` does
not capture, or the raised error of a `Result[Bool]` condition's `Err`; none is
a type failure. -/
def NoTypeFailure {α : Type} (result : Result Declaration Identity Source α) : Prop :=
  ∀ failure, result = .failure failure → ∃ error, failure = .runtime error ∨ failure = .raised error

theorem noTypeFailure_of_no_failure {α : Type} {result : Result Declaration Identity Source α}
    (never : ∀ failure, result ≠ .failure failure) : NoTypeFailure result :=
  fun failure failed => absurd failed (never failure)

theorem bind_noTypeFailure {α β : Type} {first : Result Declaration Identity Source α}
    {next : α → Result Declaration Identity Source β} (head : NoTypeFailure first) (tail : ∀ value, NoTypeFailure (next value)) :
    NoTypeFailure (first.bind next) := by
  intro failure failed
  cases first with
  | value value => exact tail value failure failed
  | failure inner =>
    simp only [Result.bind, Result.failure.injEq] at failed
    subst failed
    exact head inner rfl
  | _ => simp [Result.bind] at failed

theorem integerResult_noTypeFailure (errors : Errors Declaration Identity Source) (result : IntegerOps.IntegerResult) :
    NoTypeFailure (integerResult errors result) := by
  intro failure failed
  cases result with
  | value value => simp [integerResult] at failed
  | fault fault =>
    simp only [integerResult, Result.failure.injEq] at failed
    exact ⟨_, Or.inl failed.symm⟩

theorem durationResult_noTypeFailure (errors : Errors Declaration Identity Source) (result : DurationOps.DurationResult) :
    NoTypeFailure (durationResult errors result) := by
  intro failure failed
  cases result with
  | fault fault =>
    simp only [durationResult, Result.failure.injEq] at failed
    exact ⟨_, Or.inl failed.symm⟩
  | _ => simp [durationResult] at failed

theorem literal_noTypeFailure (literals : Literals) (value : Xsh.Syntax.Literal) :
    NoTypeFailure (Declaration := Declaration) (Identity := Identity) (Source := Source) (literal literals value) := by
  intro failure failed
  unfold literal at failed
  repeat' split at failed
  all_goals simp at failed

theorem decided_noTypeFailure (decision : Option Bool) :
    NoTypeFailure (Declaration := Declaration) (Identity := Identity) (Source := Source) (decided decision) := by
  intro failure failed
  cases decision <;> simp [decided] at failed

theorem structural_noTypeFailure (same : Declaration → Declaration → Bool) (operator : Xsh.Syntax.BinaryOp)
    (left right : Value Declaration Identity Source) :
    NoTypeFailure (structural same operator left right) := by
  intro failure failed
  unfold structural at failed
  repeat' split at failed
  all_goals first
    | exact decided_noTypeFailure _ failure failed
    | simp at failed

theorem binary_noTypeFailure (context : Context Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (left right : Value Declaration Identity Source) : NoTypeFailure (binary context operator left right) := by
  intro failure failed
  unfold binary at failed
  repeat' split at failed
  all_goals first
    | exact integerResult_noTypeFailure _ _ failure failed
    | exact durationResult_noTypeFailure _ _ failure failed
    | exact decided_noTypeFailure _ failure failed
    | exact structural_noTypeFailure _ _ _ _ failure failed
    | simp at failed

theorem unary_noTypeFailure (context : Context Declaration Identity Source) (operator : Xsh.Syntax.UnaryOp)
    (operand : Value Declaration Identity Source) : NoTypeFailure (unary context operator operand) := by
  intro failure failed
  unfold unary at failed
  repeat' split at failed
  all_goals first
    | exact integerResult_noTypeFailure _ _ failure failed
    | simp at failed

theorem fieldOf_noTypeFailure (context : Context Declaration Identity Source) (name : Xsh.Syntax.RawName)
    (base : Value Declaration Identity Source) : NoTypeFailure (fieldOf context name base) := by
  intro failure failed
  simp only [fieldOf] at failed
  repeat' split at failed
  all_goals first
    | (simp only [Result.failure.injEq] at failed; exact ⟨_, Or.inl failed.symm⟩)
    | simp at failed

theorem indexAt_noTypeFailure (context : Context Declaration Identity Source) (fromEnd : Bool)
    (base key : Value Declaration Identity Source) : NoTypeFailure (indexAt context fromEnd base key) := by
  intro failure failed
  unfold indexAt at failed
  split at failed
  · dsimp only at failed
    repeat' split at failed
    all_goals first
      | (simp only [Result.failure.injEq] at failed; exact ⟨_, Or.inl failed.symm⟩)
      | simp at failed
  · repeat' split at failed
    all_goals first
      | (simp only [Result.failure.injEq] at failed; exact ⟨_, Or.inl failed.symm⟩)
      | simp at failed

theorem evaluateAll_noTypeFailure {α : Type} {evaluate : α → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ item, NoTypeFailure (evaluate item)) : ∀ items, NoTypeFailure (evaluateAll evaluate items)
  | [] => noTypeFailure_of_no_failure (by simp [evaluateAll])
  | first :: rest => bind_noTypeFailure (each first) fun _ =>
      bind_noTypeFailure (evaluateAll_noTypeFailure each rest) fun _ => noTypeFailure_of_no_failure (by simp)

theorem evaluateFields_noTypeFailure (context : Context Declaration Identity Source)
    {evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, NoTypeFailure (evaluate source)) : ∀ fields, NoTypeFailure (evaluateFields context evaluate fields)
  | [] => noTypeFailure_of_no_failure (by simp [evaluateFields])
  | .named _ operand :: rest => bind_noTypeFailure (each operand) fun _ =>
      bind_noTypeFailure (evaluateFields_noTypeFailure context each rest) fun _ => noTypeFailure_of_no_failure (by simp)
  | .computed _ _ :: _ | .path _ _ :: _ | .shorthand _ :: _ | .spread _ :: _ =>
      noTypeFailure_of_no_failure (by simp [evaluateFields])

theorem controlCondition_noTypeFailure
    {evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, NoTypeFailure (evaluate source)) :
    ∀ condition, NoTypeFailure (controlCondition evaluate condition) := by
  intro condition
  fun_induction controlCondition evaluate condition with
  | case1 left right ihLeft ihRight | case2 left right ihLeft ihRight =>
    exact bind_noTypeFailure ihLeft fun first => by
      split
      · exact noTypeFailure_of_no_failure (by simp)
      · exact ihRight
      · exact noTypeFailure_of_no_failure (by simp)
  | case3 operand ih =>
    exact bind_noTypeFailure ih fun tested => by
      split <;> exact noTypeFailure_of_no_failure (by simp)
  | case4 condition notAnd notOr notNot =>
    exact bind_noTypeFailure (each condition) fun tested => by
      split
      · exact noTypeFailure_of_no_failure (by simp)
      · exact fun failure failed => by
          simp only [Result.failure.injEq] at failed
          exact ⟨_, Or.inr failed.symm⟩
      · exact noTypeFailure_of_no_failure (by simp)

theorem branches_noTypeFailure
    {evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)}
    (each : ∀ source, NoTypeFailure (evaluate source)) :
    ∀ conditions otherwise, NoTypeFailure (branches evaluate conditions otherwise)
  | [], otherwise => each otherwise
  | (condition, value) :: rest, otherwise => bind_noTypeFailure (controlCondition_noTypeFailure each condition) fun tested => by
      split
      · exact each value
      · exact branches_noTypeFailure each rest otherwise
      · exact noTypeFailure_of_no_failure (by simp)

theorem step_noTypeFailure (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world)
    {recurse : Environment → Locals Declaration Identity Source → Xsh.Syntax.Expr →
      Result Declaration Identity Source (Value Declaration Identity Source)}
    (inner : ∀ environment locals expression, NoTypeFailure (recurse environment locals expression))
    {callBlock : Environment → Locals Declaration Identity Source → List Xsh.Syntax.Statement → ReturnKind →
      Result Declaration Identity Source (Value Declaration Identity Source)}
    (blocks : ∀ environment locals statements kind, NoTypeFailure (callBlock environment locals statements kind))
    (locals : Locals Declaration Identity Source) (expression : Xsh.Syntax.Expr) :
    NoTypeFailure (step context environment store recurse callBlock locals expression) := by
  cases expression with
  | literal value => exact literal_noTypeFailure context.literals value
  | ident name =>
    simp only [step]
    split
    · exact noTypeFailure_of_no_failure (by simp)
    · split
      · split
        · exact noTypeFailure_of_no_failure (by simp)
        · exact noTypeFailure_of_no_failure (by simp)
      · exact noTypeFailure_of_no_failure (by simp)
  | list elements =>
    exact bind_noTypeFailure (evaluateAll_noTypeFailure (inner environment locals) elements) fun _ => noTypeFailure_of_no_failure (by simp)
  | record fields =>
    exact bind_noTypeFailure (evaluateFields_noTypeFailure context (inner environment locals) fields) fun _ =>
      noTypeFailure_of_no_failure (by simp)
  | field base name => exact bind_noTypeFailure (inner environment locals base) (fieldOf_noTypeFailure context name)
  | index base key guarded =>
    cases guarded with
    | true => exact noTypeFailure_of_no_failure (by simp [step])
    | false =>
      simp only [step]
      exact bind_noTypeFailure (inner environment locals base) fun _ =>
        bind_noTypeFailure (inner environment locals key) fun _ => indexAt_noTypeFailure _ _ _ _
  | unary operator operand => exact bind_noTypeFailure (inner environment locals operand) (unary_noTypeFailure context operator)
  | binary operator left right =>
    cases operator
    case and =>
      simp only [step]
      refine bind_noTypeFailure (inner environment locals left) fun value => ?_
      split
      · exact noTypeFailure_of_no_failure (by simp)
      · refine bind_noTypeFailure (inner environment locals right) fun _ => ?_
        split
        · exact noTypeFailure_of_no_failure (by simp)
        · exact noTypeFailure_of_no_failure (by simp)
      · exact noTypeFailure_of_no_failure (by simp)
    case or =>
      simp only [step]
      refine bind_noTypeFailure (inner environment locals left) fun value => ?_
      split
      · exact noTypeFailure_of_no_failure (by simp)
      · refine bind_noTypeFailure (inner environment locals right) fun _ => ?_
        split
        · exact noTypeFailure_of_no_failure (by simp)
        · exact noTypeFailure_of_no_failure (by simp)
      · exact noTypeFailure_of_no_failure (by simp)
    all_goals exact bind_noTypeFailure (inner environment locals left) fun _ =>
      bind_noTypeFailure (inner environment locals right) fun _ => binary_noTypeFailure _ _ _ _
  | ifExpr conditions otherwise => exact branches_noTypeFailure (inner environment locals) conditions otherwise
  | call callee arguments =>
    cases callee
    case ident name =>
      simp only [step]
      split
      · exact bind_noTypeFailure (evaluateAll_noTypeFailure (inner environment locals) _) fun _ => by
          split
          · split <;> first | exact inner _ _ _ | exact blocks _ _ _ _
          · exact noTypeFailure_of_no_failure (by simp)
      · exact noTypeFailure_of_no_failure (by simp)
    all_goals exact noTypeFailure_of_no_failure (by simp [step])
  | _ => exact noTypeFailure_of_no_failure (by simp [step])

/-- The covered expressions fail only with runtime failures: SPEC 8.8 puts
index-out-of-range, division-by-zero, integer overflow and a missing key or
field outside what `try` captures. -/
theorem evaluate_noTypeFailure (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (store : Store world) :
    ∀ fuel environment locals expression, NoTypeFailure (evaluate context environment store fuel locals expression)
  | 0, _, _, _ => noTypeFailure_of_no_failure (by simp [evaluate])
  | fuel + 1, environment, locals, expression =>
    step_noTypeFailure context environment store (evaluate_noTypeFailure context store fuel)
      (fun _ _ _ _ => noTypeFailure_of_no_failure (by simp)) locals expression

end Failures

section Order

variable (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
  (environment : Environment) (store : Store world)

/-- A binary operator other than `and`/`or` evaluates its left operand, then
its right operand, then applies its value meaning. -/
theorem binary_evaluation (fuel : Nat) (locals : Locals Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (left right : Xsh.Syntax.Expr) (notAnd : operator ≠ .and) (notOr : operator ≠ .or) :
    evaluate context environment store (fuel + 1) locals (.binary operator left right) =
      (evaluate context environment store fuel locals left).bind fun first =>
        (evaluate context environment store fuel locals right).bind fun second => binary context operator first second := by
  cases operator
  case and => exact absurd rfl notAnd
  case or => exact absurd rfl notOr
  all_goals rfl

/-- A binary operator other than `and`/`or` reports its left operand's
failure without evaluating the right operand. -/
theorem left_failure_first (fuel : Nat) (locals : Locals Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (left right : Xsh.Syntax.Expr) (failure : Failure Declaration Identity Source) (notAnd : operator ≠ .and)
    (notOr : operator ≠ .or) (failed : evaluate context environment store fuel locals left = .failure failure) :
    evaluate context environment store (fuel + 1) locals (.binary operator left right) = .failure failure := by
  cases operator
  case and => exact absurd rfl notAnd
  case or => exact absurd rfl notOr
  all_goals simp [evaluate, step, failed, Result.bind]

/-- The right operand is evaluated only after the left one has produced a
value, and its failure is then the result. -/
theorem right_failure_second (fuel : Nat) (locals : Locals Declaration Identity Source)
    (operator : Xsh.Syntax.BinaryOp) (left right : Xsh.Syntax.Expr) (first : Value Declaration Identity Source)
    (failure : Failure Declaration Identity Source) (notAnd : operator ≠ .and) (notOr : operator ≠ .or)
    (produced : evaluate context environment store fuel locals left = .value first)
    (failed : evaluate context environment store fuel locals right = .failure failure) :
    evaluate context environment store (fuel + 1) locals (.binary operator left right) = .failure failure := by
  cases operator
  case and => exact absurd rfl notAnd
  case or => exact absurd rfl notOr
  all_goals simp [evaluate, step, produced, failed, Result.bind]

end Order

section Root

/-- BASE-STEP's evaluation of the covered expressions, over every evaluation
context, Core world, environment and store: a finished evaluation is the same
at every fuel; every failure is a runtime failure or the raised error of a `Result[Bool]`
condition; a binary operator other than
`and`/`or` evaluates left, then right, then applies its meaning; and those
meanings are BASE-NUM's integer, float, comparison, IEEE-equality and negation
meanings, the `DurationOps` results in both `Duration * Int` orders with
`Int / Duration` left unsupported, and list `concatenate`. -/
theorem covered_expression_evaluation (context : Context Declaration Identity Source)
    {world : World Declaration Identity Source} (environment : Environment) (store : Store world) :
    (∀ locals expression first second,
      evaluate context environment store first locals expression ≠ .outOfFuel →
      evaluate context environment store second locals expression ≠ .outOfFuel →
        evaluate context environment store first locals expression =
          evaluate context environment store second locals expression) ∧
    (∀ fuel locals expression failure, evaluate context environment store fuel locals expression = .failure failure →
      ∃ error, failure = .runtime error ∨ failure = .raised error) ∧
    (∀ fuel locals operator left right, operator ≠ .and → operator ≠ .or →
      evaluate context environment store (fuel + 1) locals (.binary operator left right) =
        (evaluate context environment store fuel locals left).bind fun first =>
          (evaluate context environment store fuel locals right).bind fun second =>
            binary context operator first second) ∧
    (∀ operator op, integerOperator operator = some op → ∀ left right,
      ∃ result, Xsh.Proofs.Base.Num.ArithmeticMeaning op left right result ∧
        binary context operator (integer left) (integer right) = integerResult context.errors result) ∧
    (∀ operator op, floatOperator operator = some op → ∀ left right,
      ∃ result, Xsh.Values.FloatOps.FloatMeaning op left right result ∧
        binary context operator (float left) (float right) = .value (float result)) ∧
    (∀ operator comparison, ordering operator = some comparison → ∀ left right,
      ∃ result, Xsh.Values.FloatOps.ComparisonMeaning comparison left right result ∧
        binary context operator (float left) (float right) = .value (boolean result)) ∧
    (∀ left right, binary context .eq (float left) (float right) =
        .value (boolean (Xsh.Values.floatValueEq left right)) ∧
      (Xsh.Values.floatValueEq left right = true ↔ Xsh.Values.ieeeFloatEq left right)) ∧
    (∀ value bits,
      (∃ result, Xsh.Proofs.Base.Num.IntegerExact (-value.val) result ∧
        unary context .neg (integer value) = integerResult context.errors result) ∧
      (∃ result, Xsh.Values.FloatOps.NegationMeaning bits result ∧
        unary context .neg (float bits) = .value (float result))) ∧
    (∀ left right factor,
      binary context .add (duration left) (duration right) =
        durationResult context.errors (Xsh.Values.DurationOps.add left right) ∧
      binary context .sub (duration left) (duration right) =
        durationResult context.errors (Xsh.Values.DurationOps.subtract left right) ∧
      binary context .div (duration left) (duration right) =
        durationResult context.errors (Xsh.Values.DurationOps.ratio left right) ∧
      binary context .mul (duration left) (integer factor) =
        durationResult context.errors (Xsh.Values.DurationOps.multiply left factor) ∧
      binary context .div (duration left) (integer factor) =
        durationResult context.errors (Xsh.Values.DurationOps.divideByInt left factor) ∧
      binary context .mul (integer factor) (duration right) =
        durationResult context.errors (Xsh.Values.DurationOps.multiply right factor) ∧
      binary context .div (integer factor) (duration right) = .unsupported) ∧
    (∀ left right result, Xsh.Validation.Core.Preserve.concatenate left right = some result →
      binary context .add left right = .value result) :=
  ⟨fun locals expression first second =>
      evaluate_deterministic context environment store locals expression first second,
    fun fuel => evaluate_noTypeFailure context store fuel environment,
    binary_evaluation context environment store,
    integer_operator_meaning context,
    float_operator_meaning context,
    float_ordering_meaning context,
    float_equality_meaning context,
    negation_meaning context,
    duration_operator_results context,
    list_addition_is_concatenation context⟩

end Root

end Xsh.Proofs.Core.Eval
