import Xsh.Semantics.CoreStatements
import Xsh.Values.IntegerOps
import Xsh.Values.DurationOps
import Xsh.Values.FloatOps
import Xsh.Values.FloatEquality

namespace Xsh.Semantics.CoreEval

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Values.FloatOps

variable {Declaration Identity Source : Type}

/-- The interpretation of literal lexemes, which the lexer owns. -/
structure Literals where
  integer : String → Option IntValue
  float : String → Option FloatBits
  duration : String → Option DurationValue

/-- The runtime error values of the failures that `try` does not capture. -/
structure Errors (Declaration Identity Source : Type) where
  integer : IntegerOps.IntegerFault → Value Declaration Identity Source
  duration : DurationOps.DurationFault → Value Declaration Identity Source
  indexOutOfRange : Value Declaration Identity Source
  missingField : Value Declaration Identity Source
  mapMissing : Value Declaration Identity Source

/-- A let-free function: its parameter names and its result expression. -/
abbrev Functions := Xsh.Syntax.RawName → Option (List Xsh.Syntax.RawName × Xsh.Syntax.Expr)

structure Context (Declaration Identity Source : Type) where
  literals : Literals
  errors : Errors Declaration Identity Source
  policy : NaNPolicy
  spelling : Xsh.Syntax.RawName → String
  functions : Functions

inductive Result (Declaration Identity Source : Type) (α : Type) where
  | value (value : α)
  | failure (failure : Failure Declaration Identity Source)
  | stuck
  | unsupported
  | outOfFuel

def Result.bind {α β : Type} : Result Declaration Identity Source α → (α → Result Declaration Identity Source β) →
    Result Declaration Identity Source β
  | .value inner, next => next inner
  | .failure inner, _ => .failure inner
  | .stuck, _ => .stuck
  | .unsupported, _ => .unsupported
  | .outOfFuel, _ => .outOfFuel

abbrev Locals (Declaration Identity Source : Type) := List (Xsh.Syntax.RawName × Value Declaration Identity Source)

def integer (value : IntValue) : Value Declaration Identity Source := .scalar (.integer value)
def float (bits : FloatBits) : Value Declaration Identity Source := .scalar (.float bits)
def boolean (value : Bool) : Value Declaration Identity Source := .scalar (.boolean value)
def duration (value : DurationValue) : Value Declaration Identity Source := .scalar (.duration value)

def literal (literals : Literals) : Xsh.Syntax.Literal → Result Declaration Identity Source (Value Declaration Identity Source)
  | .null => .value (.scalar .null)
  | .bool value => .value (boolean value)
  | .integer lexeme => match literals.integer lexeme with | some value => .value (integer value) | none => .stuck
  | .float lexeme => match literals.float lexeme with | some bits => .value (float bits) | none => .stuck
  | .duration lexeme => match literals.duration lexeme with | some value => .value (duration value) | none => .stuck
  | .string value => .value (.scalar (.string value))
  | _ => .unsupported

def integerResult (errors : Errors Declaration Identity Source) :
    IntegerOps.IntegerResult → Result Declaration Identity Source (Value Declaration Identity Source)
  | .value value => .value (integer value)
  | .fault fault => .failure (.runtime (errors.integer fault))

def durationResult (errors : Errors Declaration Identity Source) :
    DurationOps.DurationResult → Result Declaration Identity Source (Value Declaration Identity Source)
  | .millis value => .value (duration value)
  | .count value => .value (integer value)
  | .fault fault => .failure (.runtime (errors.duration fault))

def integerOperator : Xsh.Syntax.BinaryOp → Option IntegerOps.IntegerBinaryOp
  | .add => some .add
  | .sub => some .subtract
  | .mul => some .multiply
  | .div => some .divide
  | .rem => some .remainder
  | _ => none

def floatOperator : Xsh.Syntax.BinaryOp → Option FloatBinaryOp
  | .add => some .add
  | .sub => some .subtract
  | .mul => some .multiply
  | .div => some .divide
  | _ => none

def ordering : Xsh.Syntax.BinaryOp → Option FloatComparison
  | .lt => some .less
  | .le => some .lessEqual
  | .gt => some .greater
  | .ge => some .greaterEqual
  | _ => none

def orders (comparison : FloatComparison) (less equal : Bool) : Bool :=
  match comparison with
  | .less => less
  | .lessEqual => less || equal
  | .greater => !less && !equal
  | .greaterEqual => !less

def codePoints (text : String) : List Nat := text.toList.map Char.toNat

def scalarEqual : Scalar → Scalar → Option Bool
  | .null, .null => some true
  | .boolean left, .boolean right => some (left == right)
  | .integer left, .integer right => some (decide (left.val = right.val))
  | .float left, .float right => some (floatValueEq left right)
  | .string left, .string right => some (decide (left = right))
  | .duration left, .duration right => some (decide (left.val = right.val))
  | _, _ => none

/-- The value meaning of a binary operator on two evaluated operands. -/
def binary (context : Context Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (left right : Value Declaration Identity Source) : Result Declaration Identity Source (Value Declaration Identity Source) :=
  match left, right with
  | .scalar (.integer a), .scalar (.integer b) =>
    match integerOperator operator, ordering operator with
    | some op, _ => integerResult context.errors (IntegerOps.applyBinary op a b)
    | none, some comparison => .value (boolean (orders comparison (decide (a.val < b.val)) (decide (a.val = b.val))))
    | none, none => match operator with
      | .eq => .value (boolean (decide (a.val = b.val)))
      | .ne => .value (boolean (!decide (a.val = b.val)))
      | _ => .unsupported
  | .scalar (.float a), .scalar (.float b) =>
    match floatOperator operator, ordering operator with
    | some op, _ => .value (float (floatApply context.policy op a b))
    | none, some comparison => .value (boolean (floatCompare comparison a b))
    | none, none => match operator with
      | .eq => .value (boolean (floatValueEq a b))
      | .ne => .value (boolean (floatValueNeq a b))
      | _ => .unsupported
  | .scalar (.duration a), .scalar (.duration b) =>
    match operator, ordering operator with
    | .add, _ => durationResult context.errors (DurationOps.add a b)
    | .sub, _ => durationResult context.errors (DurationOps.subtract a b)
    | .div, _ => durationResult context.errors (DurationOps.ratio a b)
    | .eq, _ => .value (boolean (decide (a.val = b.val)))
    | .ne, _ => .value (boolean (!decide (a.val = b.val)))
    | _, some comparison => .value (boolean (orders comparison (decide (a.val < b.val)) (decide (a.val = b.val))))
    | _, none => .unsupported
  | .scalar (.duration a), .scalar (.integer b) =>
    match operator with
    | .mul => durationResult context.errors (DurationOps.multiply a b)
    | .div => durationResult context.errors (DurationOps.divideByInt a b)
    | _ => .unsupported
  | .scalar (.string a), .scalar (.string b) =>
    match operator, ordering operator with
    | .add, _ => .value (.scalar (.string (a ++ b)))
    | .eq, _ => .value (boolean (decide (a = b)))
    | .ne, _ => .value (boolean (!decide (a = b)))
    | _, some comparison =>
      .value (boolean (orders comparison (lexBefore (codePoints a) (codePoints b)) (decide (a = b))))
    | _, none => .unsupported
  | .scalar a, .scalar b =>
    match operator, scalarEqual a b with
    | .eq, some same => .value (boolean same)
    | .ne, some same => .value (boolean (!same))
    | _, _ => .unsupported
  | left, right =>
    match operator, listValues left, listValues right with
    | .add, some first, some second => .value (.list (first ++ second))
    | _, _, _ => .unsupported

def unary (context : Context Declaration Identity Source) :
    Xsh.Syntax.UnaryOp → Value Declaration Identity Source → Result Declaration Identity Source (Value Declaration Identity Source)
  | .not, .scalar (.boolean value) => .value (boolean !value)
  | .neg, .scalar (.integer value) => integerResult context.errors (IntegerOps.negate value)
  | .neg, .scalar (.float bits) => .value (float (floatNegate bits))
  | _, _ => .unsupported

def fieldOf (context : Context Declaration Identity Source) (name : Xsh.Syntax.RawName) :
    Value Declaration Identity Source → Result Declaration Identity Source (Value Declaration Identity Source)
  | base =>
    match base.eraseNominal with
    | .record fields =>
      match fields.lookup (context.spelling name) with
      | some found => .value found
      | none => .failure (.runtime context.errors.missingField)
    | _ => .unsupported

def mapKeyOf (candidate : Value Declaration Identity Source) : Option MapKey :=
  match candidate.eraseNominal with
  | .scalar (.string value) => some (.string value)
  | .scalar (.integer value) => some (.integer value)
  | .scalar (.boolean value) => some (.boolean value)
  | .scalar (.bytes value) => some (.bytes value)
  | .scalar (.path value) => some (.path value)
  | .scalar (.duration value) => some (.duration value)
  | _ => none

def indexAt (context : Context Declaration Identity Source) (fromEnd : Bool)
    (base key : Value Declaration Identity Source) : Result Declaration Identity Source (Value Declaration Identity Source) :=
  match listValues base, key with
  | some items, .scalar (.integer position) =>
    let index : Int := if fromEnd then (items.length : Int) - position.val.natAbs else position.val
    if inRange : 0 ≤ index ∧ index < items.length then .value (items[index.toNat]'(by omega))
    else .failure (.runtime context.errors.indexOutOfRange)
  | _, _ =>
    match base.eraseNominal, mapKeyOf key with
    | .map entries, some mapKey =>
      match entries.lookup mapKey with
      | some found => .value found
      | none => .failure (.runtime context.errors.mapMissing)
    | .record fields, some (.string label) =>
      match fields.lookup label with
      | some found => .value found
      | none => .failure (.runtime context.errors.missingField)
    | _, _ => .unsupported

def negativeLiteral : Xsh.Syntax.Expr → Bool
  | .unary .neg (.literal (.integer _)) => true
  | _ => false

def evaluateAll {α : Type} (evaluate : α → Result Declaration Identity Source (Value Declaration Identity Source)) :
    List α → Result Declaration Identity Source (List (Value Declaration Identity Source))
  | [] => .value []
  | first :: rest => (evaluate first).bind fun value => (evaluateAll evaluate rest).bind fun values => .value (value :: values)

def evaluateFields (context : Context Declaration Identity Source)
    (evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    List Xsh.Syntax.RecordField → Result Declaration Identity Source (List (String × Value Declaration Identity Source))
  | [] => .value []
  | .named name operand :: rest =>
    (evaluate operand).bind fun value =>
      (evaluateFields context evaluate rest).bind fun fields => .value ((context.spelling name, value) :: fields)
  | _ :: _ => .unsupported

def positional : List Xsh.Syntax.CallArgument → Option (List Xsh.Syntax.Expr)
  | [] => some []
  | .positional value :: rest => (positional rest).map (value :: ·)
  | _ :: _ => none

def branches (evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    List (Xsh.Syntax.Expr × Xsh.Syntax.Expr) → Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)
  | [], otherwise => evaluate otherwise
  | (condition, value) :: rest, otherwise =>
    (evaluate condition).bind fun
      | .scalar (.boolean true) => evaluate value
      | .scalar (.boolean false) => branches evaluate rest otherwise
      | _ => .stuck

/-- One evaluation step over `recurse`, which evaluates subterms one fuel
level down. Operands evaluate once, left to right. -/
def step (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world)
    (recurse : Locals Declaration Identity Source → Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source))
    (locals : Locals Declaration Identity Source) :
    Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)
  | .literal value => literal context.literals value
  | .ident name =>
    match locals.lookup name with
    | some value => .value value
    | none =>
      match environment.cellOf name with
      | .ok cell => match store cell with
        | some found => .value found.contents.value
        | none => .stuck
      | .error _ => .stuck
  | .list elements => (evaluateAll (recurse locals) elements).bind fun values => .value (.list values)
  | .record fields => (evaluateFields context (recurse locals) fields).bind fun values => .value (.record values)
  | .field base name => (recurse locals base).bind (fieldOf context name)
  | .index base key false =>
    (recurse locals base).bind fun container =>
      match key with
      | .unary .neg (.literal (.integer lexeme)) =>
        (literal context.literals (.integer lexeme)).bind fun distance => indexAt context true container distance
      | key => (recurse locals key).bind fun selector => indexAt context false container selector
  | .unary operator operand => (recurse locals operand).bind (unary context operator)
  | .binary .and left right =>
    (recurse locals left).bind fun
      | .scalar (.boolean false) => .value (boolean false)
      | .scalar (.boolean true) =>
        (recurse locals right).bind fun
          | .scalar (.boolean value) => .value (boolean value)
          | _ => .stuck
      | _ => .stuck
  | .binary .or left right =>
    (recurse locals left).bind fun
      | .scalar (.boolean true) => .value (boolean true)
      | .scalar (.boolean false) =>
        (recurse locals right).bind fun
          | .scalar (.boolean value) => .value (boolean value)
          | _ => .stuck
      | _ => .stuck
  | .binary operator left right =>
    (recurse locals left).bind fun first => (recurse locals right).bind fun second => binary context operator first second
  | .ifExpr conditions otherwise => branches (recurse locals) conditions otherwise
  | .call (.ident name) arguments =>
    match locals.lookup name, context.functions name, positional arguments with
    | none, some (parameters, body), some values =>
      (evaluateAll (recurse locals) values).bind fun evaluated =>
        if parameters.length = evaluated.length then recurse (parameters.zip evaluated) body else .stuck
    | _, _, _ => .unsupported
  | _ => .unsupported

def evaluate (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world) :
    Nat → Locals Declaration Identity Source → Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)
  | 0, _, _ => .outOfFuel
  | fuel + 1, locals, expression => step context environment store (evaluate context environment store fuel) locals expression

def assignBinary : Xsh.Syntax.AssignOp → Xsh.Syntax.BinaryOp
  | .set => .add
  | .add => .add
  | .sub => .sub
  | .mul => .mul
  | .div => .div
  | .rem => .rem

/-- The compound-assignment operator meaning the evaluator gives where it is
defined; `fallback` stands for the forms it does not cover. -/
def operate (context : Context Declaration Identity Source) (fallback : Operate Declaration Identity Source) :
    Operate Declaration Identity Source := fun op current right =>
  match binary context (assignBinary op) current right with
  | .value result => .ok result
  | .failure (.runtime error) => .error error
  | _ => fallback op current right

/-- `Oracles.evaluate` answered by the evaluator where it gives a result;
`fallback` stands for the forms it does not cover. -/
def oracles {world : World Declaration Identity Source} (context : Context Declaration Identity Source) (fuel : Nat)
    (fallback : Xsh.Semantics.CoreStatements.Oracles world) : Xsh.Semantics.CoreStatements.Oracles world :=
  { fallback with
    evaluate := fun environment store expression =>
      match evaluate context environment store fuel [] expression with
      | .value value => .ok (value, store)
      | .failure failure => .error (failure, store)
      | _ => fallback.evaluate environment store expression }

end Xsh.Semantics.CoreEval
