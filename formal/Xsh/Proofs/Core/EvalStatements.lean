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

end Instance

section Root

/-- BASE-ENV's statement-level store-write meaning with `Oracles.evaluate`
and `Operate` answered by the covered-expression evaluator: the statement
theorem holds for that instance; where the evaluator produces a value or a
failure the instance returns it with the store unchanged; a declaration whose
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
    declare_evaluated world decisions context fuel fallback,
    declare_failed world decisions context fuel fallback,
    fun signatures global state consistent functions kind target initializer type value resolved declared typed
        produced found targetTyped =>
      declare_typed_never_malformed world decisions context fuel fallback signatures global state consistent functions
        kind target initializer type value resolved declared typed produced found targetTyped,
    Xsh.Proofs.Core.Eval.operate_adds_lists context fallbackOperate⟩

end Root

end Xsh.Proofs.Core.EvalStatements
