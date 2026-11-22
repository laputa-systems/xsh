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
    recover (StoreStep.failed failure after) = .propagated failure after ∨
      ∃ error, failure = .raised error ∧ recover (StoreStep.failed failure after) = .caught error after := by
  obtain ⟨error, runtime | raised⟩ :=
    Xsh.Proofs.Core.Eval.evaluate_noTypeFailure context store fuel environment [] expression failure failed
  · subst runtime
    exact Or.inl rfl
  · subst raised
    exact Or.inr ⟨error, rfl, rfl⟩

end Instance

section Root

/-- BASE-ENV's statement-level store-write meaning with `Oracles.evaluate`
and `Operate` answered by the covered-expression evaluator: the statement
theorem holds for that instance; where the evaluator produces a value or a
failure the instance returns it with the store unchanged, and every such
failure is a runtime one that `try` propagates or the raised error of a
`Result[Bool]` condition that `try` catches; a declaration whose
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
        recover (StoreStep.failed failure after) = .propagated failure after ∨
          ∃ error, failure = .raised error ∧ recover (StoreStep.failed failure after) = .caught error after) ∧
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
  | "2" => validateInt? 2
  | "7" => validateInt? 7
  | "8" => validateInt? 8
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
    matchNoArm := message "match-no-arm"
    streamEmpty := message "stream was empty"
    bitset := message "integer-bitset"
    floatConversion := message "float-conversion"
    parseFloat := message "parse-float"
  }
  policy := quietNaNPolicy
  spelling := fun name => if name = aName then "a" else if name = bName then "b" else "m"
  functions := fun _ => none
  sameDeclaration := fun _ _ => true

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

def stringResult : Result Unit Unit Unit WitnessValue → Option String
  | .value (.scalar (.string text)) => some text
  | _ => none

theorem of_stringResult {result : Result Unit Unit Unit WitnessValue} {text : String}
    (found : stringResult result = some text) : result = .value (message text) := by
  unfold stringResult at found
  split at found
  · simp only [Option.some.injEq] at found
    subst found
    rfl
  · simp at found

def negativeZero : Xsh.Syntax.Expr := .index (.list [int "7", int "8"]) (.unary .neg (int "0")) false

def computedNegative : Xsh.Syntax.Expr := .index (.list [int "7", int "8"]) (.binary .sub (int "0") (int "1")) false

def negativeKey : Xsh.Syntax.Expr := .index (.ident mName) (.unary .neg (int "1")) false

def minusOne : IntValue := ⟨-1, by unfold signed64Min signed64Limit; omega⟩

def signedLocals : Locals Unit Unit Unit :=
  [(mName, .map [(.integer minusOne, message "minus"), (.integer intOne, message "plus")])]

/-- The previous indexing: a negative literal is read as a positive distance
from the end of a list, and any other receiver gets that positive distance as
its key. -/
def positiveDistance (locals : Locals Unit Unit Unit) : Xsh.Syntax.Expr → Result Unit Unit Unit WitnessValue
  | .index base (.unary .neg (.literal (.integer lexeme))) false =>
    (evaluate context environment store 7 locals base).bind fun container =>
      (literal context.literals (.integer lexeme)).bind fun distance =>
        match listValues container, distance with
        | some items, .scalar (.integer position) =>
          let index : Int := (items.length : Int) - position.val.natAbs
          if inRange : 0 ≤ index ∧ index < items.length then .value (items[index.toNat]'(by omega))
          else .failure (.runtime context.errors.indexOutOfRange)
        | _, _ => indexAt context false container distance
  | expression => run locals expression

/-- `[7, 8][-0]` reads item 0, `[7, 8][0 - 1]` fails because only a literal
counts from the end, and a map's `m[-1]` reads key `-1` (SPEC 6.6); the
previous indexing fails on `-0` and reads key `1`. -/
theorem negative_literal_index :
    run [] negativeZero = .value (integer ⟨7, by decide⟩) ∧
    run [] computedNegative = runtime "index-out-of-range" ∧
    positiveDistance [] negativeZero = runtime "index-out-of-range" ∧
    run signedLocals negativeKey = .value (message "minus") ∧
    positiveDistance signedLocals negativeKey = .value (message "plus") :=
  ⟨of_integerResultValue (by decide +kernel), of_runtimeText (by decide +kernel), of_runtimeText (by decide +kernel),
    of_stringResult (by decide +kernel), of_stringResult (by decide +kernel)⟩

def xName : Xsh.Syntax.RawName := ⟨⟨20, by decide⟩⟩
def fName : Xsh.Syntax.RawName := ⟨⟨21, by decide⟩⟩
def outerCell : CellId := ⟨0, binder 5 (by decide)⟩
def innerCell : CellId := ⟨0, binder 6 (by decide)⟩

def cells : ActivationEnvironment :=
  bindCell (bindCell [] (binder 5 (by decide)) outerCell) (binder 6 (by decide)) innerCell

/-- `let x = 1` at the top level. -/
def outer : Environment := ⟨bind emptyLexicalEnvironment xName (binder 5 (by decide)), cells⟩

/-- `{ let x = 2 ... }` inside it. -/
def inner : Environment := ⟨bind (enterScope outer.names) xName (binder 6 (by decide)), cells⟩

def holding (value : Int) (inRange : signed64Min ≤ value ∧ value < signed64Limit) (nonnegative : 0 ≤ value) :
    Cell witnessWorld :=
  ⟨⟨.primitive .unsignedInteger, Xsh.Proofs.Core.Env.Witness.integer value inRange,
    member_of_nonnegative value inRange nonnegative⟩, .mutable⟩

def scopeStore : Store witnessWorld := fun cell =>
  if cell = outerCell then some (holding 1 (by decide) (by decide))
  else if cell = innerCell then some (holding 2 (by decide) (by decide))
  else none

/-- `fn f() { x }`, defined at the top level. -/
def scopeContext : Context Unit Unit Unit :=
  { context with functions := fun name => if name = fName then some (outer, [], .expression (.ident xName) .plain) else none }

def callF : Xsh.Syntax.Expr := .call (.ident fName) []

/-- An evaluator that runs a function body in the caller's environment. -/
def dynamicScope (callSite : Environment) : Xsh.Syntax.Expr → Result Unit Unit Unit WitnessValue
  | .call (.ident name) arguments =>
    match scopeContext.functions name, positional arguments with
    | some (_, parameters, .expression body _), some values =>
      (evaluateAll (evaluate scopeContext callSite scopeStore 7 []) values).bind fun evaluated =>
        evaluate scopeContext callSite scopeStore 7 (parameters.zip evaluated) body
    | _, _ => .unsupported
  | expression => evaluate scopeContext callSite scopeStore 8 [] expression

/-- `let x = 1; fn f() { x }; { let x = 2; f() }` gives `1`: the body reads `x`
where `f` was defined. Running the body in the caller's environment gives
`2`. -/
theorem lexical_function_body :
    evaluate scopeContext inner scopeStore 8 [] callF = .value (integer intOne) ∧
    dynamicScope inner callF = .value (integer ⟨2, by decide⟩) ∧
    evaluate scopeContext inner scopeStore 8 [] (.ident xName) = .value (integer ⟨2, by decide⟩) :=
  ⟨of_integerResultValue (by decide +kernel), of_integerResultValue (by decide +kernel),
    of_integerResultValue (by decide +kernel)⟩

def scaled : Xsh.Syntax.Expr := .binary .mul (int "2") (.ident mName)
def durationLocals : Locals Unit Unit Unit := [(mName, duration ⟨1000, by decide⟩)]

def durationResultValue : Result Unit Unit Unit WitnessValue → Option Nat
  | .value (.scalar (.duration value)) => some value.val
  | _ => none

theorem of_durationResultValue {result : Result Unit Unit Unit WitnessValue} {value : DurationValue}
    (found : durationResultValue result = some value.val) : result = .value (duration value) := by
  unfold durationResultValue at found
  split at found
  · rename_i inner
    simp only [Option.some.injEq] at found
    have same : inner = value := Subtype.ext found
    subst same
    rfl
  · simp at found

/-- `2 * 1s` is `2s`, as `1s * 2` is; `2 / 1s` stays a type error. -/
theorem integer_times_duration :
    run durationLocals scaled = .value (duration ⟨2000, by decide⟩) ∧
    run durationLocals (.binary .mul (.ident mName) (int "2")) = .value (duration ⟨2000, by decide⟩) ∧
    run durationLocals (.binary .div (int "2") (.ident mName)) = .unsupported :=
  ⟨of_durationResultValue (by decide +kernel), of_durationResultValue (by decide +kernel), by
    have unsupported : (run durationLocals (.binary .div (int "2") (.ident mName)) matches .unsupported) = true := by
      decide +kernel
    revert unsupported
    cases run durationLocals (.binary .div (int "2") (.ident mName)) <;> simp⟩

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
