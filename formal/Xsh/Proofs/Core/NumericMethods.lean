import Xsh.Proofs.Core.Exports

namespace Xsh.Proofs.Core.NumericMethods

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
open Xsh.Values.FloatOps

section Root

variable {Declaration Identity Source : Type}

/-- The `numeric_methods` fragment root (SPEC 4.2, 4.3, 6.3): a numeric method
call evaluates its receiver, then its arguments in order, and gives the
BASE-NUM meanings: `.float()` is `IntToFloatMeaning`'s conversion, `.floor()`,
`.ceil()` and `.round()` are `Ok` of the `ToIntMeaning` result under their
bounds or `Err` of the conversion error, and the bitset methods are
`BitsetMeaning`'s result, a negative operand failing with `integer-bitset`. -/
theorem numeric_methods (errors : Errors Declaration Identity Source) :
    (∀ value : IntValue, numericMethod errors "float" (.scalar (.integer value)) [] = .value (float (intToFloat value)) ∧
      FloatOps.IntToFloatMeaning value (intToFloat value)) ∧
    (∀ bits, numericMethod errors "floor" (.scalar (.float bits)) [] =
      (match FloatOps.floatFloor bits with
        | some value => .value (.ok (integer value))
        | none => .value (.err errors.floatConversion)) ∧
      FloatOps.ToIntMeaning FloatOps.FloorBound bits (FloatOps.floatFloor bits)) ∧
    (∀ bits, numericMethod errors "ceil" (.scalar (.float bits)) [] =
      (match FloatOps.floatCeil bits with
        | some value => .value (.ok (integer value))
        | none => .value (.err errors.floatConversion)) ∧
      FloatOps.ToIntMeaning FloatOps.CeilBound bits (FloatOps.floatCeil bits)) ∧
    (∀ bits, numericMethod errors "round" (.scalar (.float bits)) [] =
      (match FloatOps.floatRound bits with
        | some value => .value (.ok (integer value))
        | none => .value (.err errors.floatConversion)) ∧
      FloatOps.ToIntMeaning FloatOps.RoundAwayBound bits (FloatOps.floatRound bits)) ∧
    (∀ name operator (value mask : IntValue), bitsetOperator name = some operator →
      numericMethod errors name (.scalar (.integer value)) [.scalar (.integer mask)] =
        (match IntegerOps.applyBitset operator value mask with
          | .value result => .value (integer result)
          | .negativeOperand => .failure (.runtime errors.bitset)) ∧
      IntegerOps.BitsetMeaning operator value mask (IntegerOps.applyBitset operator value mask)) := by
  refine ⟨fun value => ⟨rfl, FloatOps.int_to_float_meaning value⟩,
    fun bits => ⟨rfl, FloatOps.float_to_int_meaning _ _ FloatOps.floor_is_correct bits⟩,
    fun bits => ⟨rfl, FloatOps.float_to_int_meaning _ _ FloatOps.ceil_is_correct bits⟩,
    fun bits => ⟨rfl, FloatOps.float_to_int_meaning _ _ FloatOps.round_is_correct bits⟩,
    fun name operator value mask named => ⟨?_, IntegerOps.apply_bitset_meaning operator value mask⟩⟩
  unfold numericMethod
  simp only [List.map, Value.eraseNominal, named]
  split <;> simp_all [bitsetOperator]

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.MatchValues.Witness
open Xsh.Proofs.Core.Slices.Witness

def floatMethod : Xsh.Syntax.RawName := name 220 (by decide)
def roundMethod : Xsh.Syntax.RawName := name 221 (by decide)
def floorMethod : Xsh.Syntax.RawName := name 222 (by decide)
def ceilMethod : Xsh.Syntax.RawName := name 223 (by decide)
def andMethod : Xsh.Syntax.RawName := name 224 (by decide)
def orMethod : Xsh.Syntax.RawName := name 225 (by decide)
def clearMethod : Xsh.Syntax.RawName := name 226 (by decide)

def methodSpelling (named : Xsh.Syntax.RawName) : String :=
  if named = floatMethod then "float" else if named = roundMethod then "round"
  else if named = floorMethod then "floor" else if named = ceilMethod then "ceil"
  else if named = andMethod then "bit_and" else if named = orMethod then "bit_or"
  else if named = clearMethod then "clear_bits" else machine.context.spelling named

/-- The witness machine with the method names spelled and Float literals read
by `decimalFloat`. -/
def methodMachine : Machine witnessWorld :=
  { machine with context := { machine.context with
      spelling := methodSpelling
      literals := { machine.context.literals with float := Xsh.Values.FloatLiterals.decimalFloat } } }

def method (receiver : Xsh.Syntax.Expr) (named : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.Expr) :
    Xsh.Syntax.Expr :=
  .call (.field receiver named) (arguments.map .positional)

def evaluated (value : Xsh.Syntax.Expr) : Option (Value Unit Unit Unit) :=
  readValue (runStatements methodMachine 40 start [letValue value])

def floatBits : Option (Value Unit Unit Unit) → Option Nat
  | some (.scalar (.float bits)) => some bits.val
  | _ => none

def okInt : Option (Value Unit Unit Unit) → Option Int
  | some (.ok (.scalar (.integer value))) => some value.val
  | _ => none

def plainInt : Option (Value Unit Unit Unit) → Option Int
  | some (.scalar (.integer value)) => some value.val
  | _ => none

def isErr : Option (Value Unit Unit Unit) → Bool
  | some (.err _) => true
  | _ => false

def flt (lexeme : String) : Xsh.Syntax.Expr := .literal (.float lexeme)

/-- As the verification binary prints `true 3 -3 2 3`, `true` and `8 15 8`
for `7.float() == 7.0`, `2.5.round()?`, `(-2.5).round()?`, `2.5.floor()?`,
`2.5.ceil()?`, a NaN's `.floor()` being `Err`, and `12.bit_and(10)`,
`12.bit_or(3)` and `12.clear_bits(4)`; here `7.bit_and(2)`, `7.bit_or(8)` and
`7.clear_bits(2)` from the witness lexemes. -/
theorem numeric_method_witness :
    floatBits (evaluated (method (int "7") floatMethod [])) = some 4619567317775286272 ∧
    okInt (evaluated (method (flt "2.5") roundMethod [])) = some 3 ∧
    okInt (evaluated (method (.unary .neg (flt "2.5")) roundMethod [])) = some (-3) ∧
    okInt (evaluated (method (flt "2.5") floorMethod [])) = some 2 ∧
    okInt (evaluated (method (flt "2.5") ceilMethod [])) = some 3 ∧
    isErr (evaluated (method (.binary .div (flt "0.0") (flt "0.0")) floorMethod [])) = true ∧
    plainInt (evaluated (method (int "7") andMethod [int "2"])) = some 2 ∧
    plainInt (evaluated (method (int "7") orMethod [int "8"])) = some 15 ∧
    plainInt (evaluated (method (int "7") clearMethod [int "2"])) = some 5 := by
  decide +kernel

/-- `(-1).bit_and(1)` fails with `integer-bitset`, a runtime failure the
binary reports with exit 3 through `try`; a mutant that works on magnitudes
gives `1`. -/
theorem negative_bitset_fails :
    runtimeMessage (runStatements methodMachine 40 start
      [letValue (method (.unary .neg (int "1")) andMethod [int "1"])]) = some "integer-bitset" ∧
      (match IntegerOps.bitsetOnMagnitudes .bitAnd ⟨-1, by decide⟩ ⟨1, by decide⟩ with
        | .value result => some result.val
        | .negativeOperand => none) = some 1 := by
  decide +kernel

/-- Rounding `-2.5` with ties to even gives `-2`; the model rounds away from
zero to `-3`. -/
theorem ties_even_round_rejected :
    okInt (evaluated (method (.unary .neg (flt "2.5")) roundMethod [])) = some (-3) := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.NumericMethods
