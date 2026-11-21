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
open Xsh.Values
open Xsh.Values.FloatOps

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
    (functions : FunctionsTyped context world state.store signatures) (kind : Xsh.Syntax.BindingKind)
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
    (preservation context world state.store signatures functions fuel state.environment global []
      initializer type value typed consistent produced)
    targetTyped

omit [DecidableEq Declaration] in
theorem evaluated_failure_propagates (environment : Environment) (store : Store world) (expression : Xsh.Syntax.Expr)
    (failure : Failure Declaration Identity Source) (after : Store world)
    (failed : evaluate context environment store fuel [] expression = .failure failure) :
    recover (StoreStep.failed failure after) = .propagated failure after := by
  obtain ⟨error, rfl⟩ := Xsh.Proofs.Core.Eval.evaluate_runtimeOnly context store fuel environment [] expression failure failed
  rfl

end Instance

section Root

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
    (∀ signatures global (state : State world), Consistent world state.environment state.store global [] →
      FunctionsTyped context world state.store signatures →
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

namespace Witness

open Xsh.Proofs.Core.Env.Witness

def aName : Xsh.Syntax.RawName := ⟨⟨10, by decide⟩⟩
def bName : Xsh.Syntax.RawName := ⟨⟨11, by decide⟩⟩
def mName : Xsh.Syntax.RawName := ⟨⟨12, by decide⟩⟩

def message (text : String) : WitnessValue := .scalar (.string text)

def integerMessage : IntegerOps.IntegerFault → WitnessValue
  | .overflow => message "integer-overflow"
  | .divisionByZero => message "division-by-zero"

def integerLexeme : String → Option IntValue
  | "0" => validateInt? 0
  | "1" => validateInt? 1
  | "9223372036854775807" => validateInt? 9223372036854775807
  | _ => none

def context : Context Unit Unit Unit where
  literals := {
    integer := integerLexeme
    float := fun _ => none
    duration := fun _ => none
  }
  errors := {
    integer := integerMessage
    duration := fun _ => message "duration-overflow"
    indexOutOfRange := message "index-out-of-range"
    missingField := message "missing-field"
    mapMissing := message "missing-key"
  }
  policy := quietNaNPolicy
  spelling := fun name => if name = aName then "a" else if name = bName then "b" else "m"
  functions := fun _ => none

def environment : Environment := ⟨emptyLexicalEnvironment, []⟩

def store : Store witnessWorld := fun _ => none

def run (locals : Locals Unit Unit Unit) (expression : Xsh.Syntax.Expr) : Result Unit Unit Unit WitnessValue :=
  evaluate context environment store 8 locals expression

def int (lexeme : String) : Xsh.Syntax.Expr := .literal (.integer lexeme)

def divisionByZero : Xsh.Syntax.Expr := .binary .div (int "1") (int "0")
def outOfRange : Xsh.Syntax.Expr := .index (.list []) (int "0") false
def overflow : Xsh.Syntax.Expr := .binary .add (int "9223372036854775807") (int "1")
def missingField : Xsh.Syntax.Expr := .field (.record [.named aName (int "1")]) bName
def missingKey : Xsh.Syntax.Expr := .index (.ident mName) (.literal (.string "k")) false
def mapLocals : Locals Unit Unit Unit := [(mName, .map [])]

def ordered : Xsh.Syntax.Expr := .binary .add divisionByZero outOfRange
def shortCircuit : Xsh.Syntax.Expr := .binary .and (.literal (.bool false)) outOfRange
def minimumRemainder : Xsh.Syntax.Expr := .binary .rem (.ident mName) (.unary .neg (int "1"))
def minimumLocals : Locals Unit Unit Unit := [(mName, integer intMin)]

/-- An evaluator that runs a non-logical binary operator's right operand
first. -/
def rightFirst (locals : Locals Unit Unit Unit) : Xsh.Syntax.Expr → Result Unit Unit Unit WitnessValue
  | .binary operator left right =>
    (evaluate context environment store 7 locals right).bind fun second =>
      (evaluate context environment store 7 locals left).bind fun first => binary context operator first second
  | expression => run locals expression

/-- An evaluator whose `and` evaluates both operands. -/
def strictAnd (locals : Locals Unit Unit Unit) : Xsh.Syntax.Expr → Result Unit Unit Unit WitnessValue
  | .binary .and left right =>
    (evaluate context environment store 7 locals left).bind fun first =>
      (evaluate context environment store 7 locals right).bind fun second =>
        match first.eraseNominal, second.eraseNominal with
        | .scalar (.boolean a), .scalar (.boolean b) => .value (boolean (a && b))
        | _, _ => .stuck
  | expression => run locals expression

/-- An evaluator whose `%` uses the remainder BASE-NUM rejects, without the
minimum-by-minus-one overflow. -/
def unguardedRemainder (locals : Locals Unit Unit Unit) : Xsh.Syntax.Expr → Result Unit Unit Unit WitnessValue
  | .binary .rem left right =>
    (evaluate context environment store 7 locals left).bind fun first =>
      (evaluate context environment store 7 locals right).bind fun second =>
        match first.eraseNominal, second.eraseNominal with
        | .scalar (.integer a), .scalar (.integer b) =>
          integerResult context.errors (Xsh.Proofs.Base.Num.remainderWithoutDivisionOverflow a b)
        | _, _ => .stuck
  | expression => run locals expression

def runtime (text : String) : Result Unit Unit Unit WitnessValue := .failure (.runtime (message text))

def runtimeText : Result Unit Unit Unit WitnessValue → Option String
  | .failure (.runtime (.scalar (.string text))) => some text
  | _ => none

def booleanResult : Result Unit Unit Unit WitnessValue → Option Bool
  | .value (.scalar (.boolean value)) => some value
  | _ => none

def integerResultValue : Result Unit Unit Unit WitnessValue → Option Int
  | .value (.scalar (.integer value)) => some value.val
  | _ => none

theorem of_runtimeText {result : Result Unit Unit Unit WitnessValue} {text : String}
    (found : runtimeText result = some text) : result = runtime text := by
  unfold runtimeText at found
  split at found
  · simp only [Option.some.injEq] at found
    subst found
    rfl
  · simp at found

theorem of_booleanResult {result : Result Unit Unit Unit WitnessValue} {value : Bool}
    (found : booleanResult result = some value) : result = .value (boolean value) := by
  unfold booleanResult at found
  split at found
  · simp only [Option.some.injEq] at found
    subst found
    rfl
  · simp at found

theorem of_integerResultValue {result : Result Unit Unit Unit WitnessValue} {value : IntValue}
    (found : integerResultValue result = some value.val) : result = .value (integer value) := by
  unfold integerResultValue at found
  split at found
  · rename_i inner
    simp only [Option.some.injEq] at found
    have same : inner = value := Subtype.ext found
    subst same
    rfl
  · simp at found

theorem failure_classes :
    run [] divisionByZero = runtime "division-by-zero" ∧
    run [] overflow = runtime "integer-overflow" ∧
    run [] outOfRange = runtime "index-out-of-range" ∧
    run [] missingField = runtime "missing-field" ∧
    run mapLocals missingKey = runtime "missing-key" := by
  refine ⟨of_runtimeText (by decide +kernel), of_runtimeText (by decide +kernel), of_runtimeText (by decide +kernel),
    of_runtimeText (by decide +kernel), of_runtimeText (by decide +kernel)⟩

/-- The covered failures are runtime failures, which `try` propagates rather
than catches. -/
theorem try_propagates (text : String) :
    tryBlock [] (StoreStep.failed (.runtime (message text)) store) = .propagated (.runtime (message text)) store := rfl

theorem evaluation_order :
    run [] ordered = runtime "division-by-zero" ∧ rightFirst [] ordered = runtime "index-out-of-range" :=
  ⟨of_runtimeText (by decide +kernel), of_runtimeText (by decide +kernel)⟩

theorem short_circuit :
    run [] shortCircuit = .value (boolean false) ∧ strictAnd [] shortCircuit = runtime "index-out-of-range" :=
  ⟨of_booleanResult (by decide +kernel), of_runtimeText (by decide +kernel)⟩

/-- `%` at the integer minimum by minus one overflows as BASE-NUM's
`ArithmeticMeaning` requires; the unguarded remainder returns zero, and
BASE-NUM rejects it. -/
theorem minimum_remainder :
    run minimumLocals minimumRemainder = runtime "integer-overflow" ∧
    unguardedRemainder minimumLocals minimumRemainder = .value (integer ⟨0, by decide⟩) ∧
    ¬ ∀ left right, Xsh.Proofs.Base.Num.ArithmeticMeaning .remainder left right
      (Xsh.Proofs.Base.Num.remainderWithoutDivisionOverflow left right) :=
  ⟨of_runtimeText (by decide +kernel), of_integerResultValue (by decide +kernel),
    Xsh.Proofs.Base.Num.remainder_without_division_overflow_is_rejected⟩

end Witness

end Xsh.Proofs.Core.EvalStatements
