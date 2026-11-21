import Xsh.Proofs.Core.EvalTyping
import Xsh.Proofs.Core.Statements

namespace Xsh.Proofs.Core.EvalStatements

open Xsh.Foundation
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Typing.Core.Expressions
open Xsh.Proofs.Core.EvalTyping
open Xsh.Proofs.Core.Statements

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

section Instance

variable (world : World Declaration Identity Source) (decisions : Decisions world)
  (raises : Raises Declaration Identity Source) (context : Context Declaration Identity Source) (fuel : Nat)
  (fallback : Oracles world) (fallbackOperate : Operate Declaration Identity Source)

omit [DecidableEq Declaration] in
theorem instance_value (environment : Environment) (store : Store world) (expression : Xsh.Syntax.Expr)
    (value : Value Declaration Identity Source)
    (produced : evaluate context environment store fuel [] expression = .value value) :
    (Xsh.Semantics.CoreEval.oracles context fuel fallback).evaluate environment store expression = .ok (value, store) := by
  simp [Xsh.Semantics.CoreEval.oracles, produced]

omit [DecidableEq Declaration] in
theorem instance_failure (environment : Environment) (store : Store world) (expression : Xsh.Syntax.Expr)
    (failure : Failure Declaration Identity Source)
    (produced : evaluate context environment store fuel [] expression = .failure failure) :
    (Xsh.Semantics.CoreEval.oracles context fuel fallback).evaluate environment store expression = .error (failure, store) := by
  simp [Xsh.Semantics.CoreEval.oracles, produced]

theorem declare_evaluated (state : State world) (kind : Xsh.Syntax.BindingKind) (target : Xsh.Syntax.BindingTarget)
    (initializer : Xsh.Syntax.Expr) (value : Value Declaration Identity Source) (resolved : Target Declaration)
    (declared : List Declared)
    (produced : evaluate context state.environment state.store fuel [] initializer = .value value)
    (found : resolveTarget (Xsh.Semantics.CoreEval.oracles context fuel fallback) state.environment target =
      some (resolved, declared)) :
    declareStatement world decisions (Xsh.Semantics.CoreEval.oracles context fuel fallback) state kind target initializer =
      lift state (extend state.environment declared)
        (declareTarget world decisions state.store (bindingMutability kind) resolved value) :=
  Xsh.Proofs.Core.Statements.declareStatement_resolved world decisions _ state kind target initializer value state.store
    resolved declared (instance_value world context fuel fallback _ _ _ _ produced) found

theorem declare_failed (state : State world) (kind : Xsh.Syntax.BindingKind) (target : Xsh.Syntax.BindingTarget)
    (initializer : Xsh.Syntax.Expr) (failure : Failure Declaration Identity Source)
    (produced : evaluate context state.environment state.store fuel [] initializer = .failure failure) :
    declareStatement world decisions (Xsh.Semantics.CoreEval.oracles context fuel fallback) state kind target initializer =
      .failed failure state :=
  Xsh.Proofs.Core.Statements.declareStatement_initializer_fails world decisions _ state kind target initializer failure
    state.store (instance_failure world context fuel fallback _ _ _ _ produced)

theorem declare_typed_never_malformed (signatures : Signatures Declaration) (global : TypeContext Declaration)
    (state : State world) (consistent : Consistent world state.environment state.store global [])
    (functions : FunctionsTyped context signatures global) (kind : Xsh.Syntax.BindingKind)
    (target : Xsh.Syntax.BindingTarget) (initializer : Xsh.Syntax.Expr) (type : ValueType Declaration)
    (value : Value Declaration Identity Source) (resolved : Target Declaration) (declared : List Declared)
    (typed : Typed context.spelling signatures global initializer type)
    (produced : evaluate context state.environment state.store fuel [] initializer = .value value)
    (found : resolveTarget (Xsh.Semantics.CoreEval.oracles context fuel fallback) state.environment target =
      some (resolved, declared))
    (targetTyped : Xsh.Proofs.Core.Writes.TargetTyped type resolved) :
    ∀ after, declareStatement world decisions (Xsh.Semantics.CoreEval.oracles context fuel fallback) state kind target
      initializer ≠ .internal .malformedTarget after :=
  Xsh.Proofs.Core.Statements.declareStatement_never_malformed world decisions _ state kind target initializer value
    state.store resolved declared type (instance_value world context fuel fallback _ _ _ _ produced) found
    (preservation context world state.environment state.store signatures global consistent functions fuel global []
      initializer type value typed consistent produced)
    targetTyped

omit [DecidableEq Declaration] in
theorem evaluated_failure_propagates (environment : Environment) (store : Store world) (expression : Xsh.Syntax.Expr)
    (failure : Failure Declaration Identity Source) (after : Store world)
    (failed : evaluate context environment store fuel [] expression = .failure failure) :
    recover (StoreStep.failed failure after) = .propagated failure after := by
  obtain ⟨error, rfl⟩ := Xsh.Proofs.Core.Eval.evaluate_runtimeOnly context environment store fuel [] expression failure failed
  rfl

end Instance

section Root

omit [DecidableEq Declaration] in
/-- BASE-STEP's evaluation of the covered expressions, over every evaluation
context, Core world, environment and store: a finished evaluation is the same
at every fuel; a value is a member of the type the evaluator-independent
typing judgment gives, in a consistent typing context with typed let-free
functions; every failure is a runtime failure; a binary operator other than
`and`/`or` evaluates left, then right, then applies its meaning; and those
meanings are BASE-NUM's integer, float, comparison, IEEE-equality and negation
meanings, the `DurationOps` results, and list `concatenate`. -/
theorem covered_expression_evaluation (context : Context Declaration Identity Source)
    (world : World Declaration Identity Source) (environment : Environment) (store : Store world) :
    (∀ locals expression first second,
      evaluate context environment store first locals expression ≠ .outOfFuel →
      evaluate context environment store second locals expression ≠ .outOfFuel →
        evaluate context environment store first locals expression =
          evaluate context environment store second locals expression) ∧
    (∀ signatures global, Consistent world environment store global [] → FunctionsTyped context signatures global →
      ∀ fuel types locals expression type value, Typed context.spelling signatures types expression type →
        Consistent world environment store types locals →
        evaluate context environment store fuel locals expression = .value value →
          Membership.value world.membership type value) ∧
    (∀ fuel locals expression failure, evaluate context environment store fuel locals expression = .failure failure →
      ∃ error, failure = .runtime error) ∧
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
        durationResult context.errors (Xsh.Values.DurationOps.divideByInt left factor)) ∧
    (∀ left right result, Xsh.Validation.Core.Preserve.concatenate left right = some result →
      binary context .add left right = .value result) :=
  ⟨fun locals expression first second =>
      Xsh.Proofs.Core.Eval.evaluate_deterministic context environment store locals expression first second,
    fun signatures global consistent functions =>
      preservation context world environment store signatures global consistent functions,
    Xsh.Proofs.Core.Eval.evaluate_runtimeOnly context environment store,
    Xsh.Proofs.Core.Eval.binary_evaluation context environment store,
    Xsh.Proofs.Core.Eval.integer_operator_meaning context,
    Xsh.Proofs.Core.Eval.float_operator_meaning context,
    Xsh.Proofs.Core.Eval.float_ordering_meaning context,
    Xsh.Proofs.Core.Eval.float_equality_meaning context,
    Xsh.Proofs.Core.Eval.negation_meaning context,
    Xsh.Proofs.Core.Eval.duration_operator_results context,
    Xsh.Proofs.Core.Eval.list_addition_is_concatenation context⟩

/-- BASE-ENV's statement-level store-write meaning with `Oracles.evaluate`
and `Operate` answered by the covered-expression evaluator: the statement
theorem holds for that instance; where the evaluator produces a value or a
failure the instance returns it with the store unchanged, and every such
failure is a runtime one that `try` propagates; a declaration whose
initializer the evaluator answers commits that value or fails with that
failure; a well-typed initializer in a consistent context never reaches the
malformed-target fault; and compound `+=` on lists is `concatenate`. -/
theorem evaluated_store_writes (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (context : Context Declaration Identity Source) (fuel : Nat)
    (fallback : Oracles world) (fallbackOperate : Operate Declaration Identity Source) :
    (∀ state statement,
      (execute world decisions raises (operate context fallbackOperate) (oracles context fuel fallback) state
        statement).isSome ↔ IsStoreWrite statement) ∧
    (∀ environment store expression value, evaluate context environment store fuel [] expression = .value value →
      (oracles context fuel fallback).evaluate environment store expression = .ok (value, store)) ∧
    (∀ environment store expression failure, evaluate context environment store fuel [] expression = .failure failure →
      (oracles context fuel fallback).evaluate environment store expression = .error (failure, store)) ∧
    (∀ environment (store : Store world) expression failure (after : Store world),
      evaluate context environment store fuel [] expression = .failure failure →
        recover (StoreStep.failed failure after) = .propagated failure after) ∧
    (∀ state kind target initializer value resolved declared,
      evaluate context state.environment state.store fuel [] initializer = .value value →
      resolveTarget (oracles context fuel fallback) state.environment target = some (resolved, declared) →
        declareStatement world decisions (oracles context fuel fallback) state kind target initializer =
          lift state (extend state.environment declared)
            (declareTarget world decisions state.store (bindingMutability kind) resolved value)) ∧
    (∀ state kind target initializer failure,
      evaluate context state.environment state.store fuel [] initializer = .failure failure →
        declareStatement world decisions (oracles context fuel fallback) state kind target initializer =
          .failed failure state) ∧
    (∀ signatures global state, Consistent world state.environment state.store global [] →
      FunctionsTyped context signatures global →
      ∀ kind target initializer type value resolved declared,
        Typed context.spelling signatures global initializer type →
        evaluate context state.environment state.store fuel [] initializer = .value value →
        resolveTarget (oracles context fuel fallback) state.environment target = some (resolved, declared) →
        Xsh.Proofs.Core.Writes.TargetTyped type resolved →
          ∀ after, declareStatement world decisions (oracles context fuel fallback) state kind target initializer ≠
            .internal .malformedTarget after) ∧
    (∀ left right result, Xsh.Validation.Core.Preserve.concatenate left right = some result →
      operate context fallbackOperate .add left right = .ok result) :=
  ⟨(Xsh.Proofs.Core.Statements.syntax_directed_store_writes world decisions raises (operate context fallbackOperate)
      (oracles context fuel fallback)).1,
    instance_value world context fuel fallback,
    instance_failure world context fuel fallback,
    evaluated_failure_propagates world context fuel,
    declare_evaluated world decisions context fuel fallback,
    declare_failed world decisions context fuel fallback,
    fun signatures global state consistent functions kind target initializer type value resolved declared typed
        produced found targetTyped =>
      declare_typed_never_malformed world decisions context fuel fallback signatures global state consistent functions
        kind target initializer type value resolved declared typed produced found targetTyped,
    Xsh.Proofs.Core.Eval.operate_adds_lists context fallbackOperate⟩

end Root

end Xsh.Proofs.Core.EvalStatements
