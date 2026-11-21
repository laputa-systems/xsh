import Xsh.Proofs.Core.EvalStatements
import Xsh.Proofs.Base.Num

namespace Xsh.Proofs.Core.EvalWitness

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.FloatOps
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreEval
open Xsh.Proofs.Core.Env.Witness

variable {Declaration Identity Source : Type}

section Order

variable (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
  (environment : Environment) (store : Store world)

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

namespace Witness

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

end Xsh.Proofs.Core.EvalWitness
