import Xsh.Syntax.Core
import Xsh.Core.Membership

namespace Xsh.Typing.Core.Expressions

open Xsh.Core

variable {Declaration : Type}

abbrev TypeContext (Declaration : Type) := Xsh.Syntax.RawName → Option (ValueType Declaration)
abbrev Signatures (Declaration : Type) := Xsh.Syntax.RawName → Option (List (ValueType Declaration) × ValueType Declaration)

def arithmetic : List Xsh.Syntax.BinaryOp := [.add, .sub, .mul, .div, .rem]
def floatArithmetic : List Xsh.Syntax.BinaryOp := [.add, .sub, .mul, .div]
def comparisons : List Xsh.Syntax.BinaryOp := [.lt, .le, .gt, .ge, .eq, .ne]
def equalities : List Xsh.Syntax.BinaryOp := [.eq, .ne]

def intType : ValueType Declaration := .primitive .integer
def floatType : ValueType Declaration := .primitive .float
def boolType : ValueType Declaration := .primitive .boolean
def strType : ValueType Declaration := .primitive .string
def durType : ValueType Declaration := .primitive .duration

def lookupRow : RecordType Declaration → String → Option (ValueType Declaration)
  | .rowField name type rest, wanted => if name = wanted then some type else lookupRow rest wanted
  | _, _ => none

def namesOf : RecordType Declaration → List String
  | .rowField name _ rest => name :: namesOf rest
  | _ => []

mutual
  /-- The typing judgment of the covered expression forms. It is defined from
  the operator table of SPEC 6.2, not from the evaluator. -/
  inductive Typed (spelling : Xsh.Syntax.RawName → String) (signatures : Signatures Declaration) :
      TypeContext Declaration → Xsh.Syntax.Expr → ValueType Declaration → Prop where
    | null {context} : Typed spelling signatures context (.literal .null) (.primitive .null)
    | boolType {context value} : Typed spelling signatures context (.literal (.bool value)) boolType
    | integer {context lexeme} : Typed spelling signatures context (.literal (.integer lexeme)) intType
    | float {context lexeme} : Typed spelling signatures context (.literal (.float lexeme)) floatType
    | duration {context lexeme} : Typed spelling signatures context (.literal (.duration lexeme)) durType
    | string {context value} : Typed spelling signatures context (.literal (.string value)) strType
    | ident {context name type} : context name = some type → Typed spelling signatures context (.ident name) type
    | list {context elements element} : (∀ member ∈ elements, Typed spelling signatures context member element) →
        Typed spelling signatures context (.list elements) (.list element)
    | record {context fields row} : FieldsTyped spelling signatures context fields row →
        Typed spelling signatures context (.record fields) (.record row)
    | field {context base name row type} : Typed spelling signatures context base (.record row) →
        lookupRow row (spelling name) = some type → Typed spelling signatures context (.field base name) type
    | listIndex {context base key element} : Typed spelling signatures context base (.list element) →
        Typed spelling signatures context key intType → Typed spelling signatures context (.index base key false) element
    | mapIndex {context base key keyType element} : Typed spelling signatures context base (.map keyType element) →
        Typed spelling signatures context key keyType → Typed spelling signatures context (.index base key false) element
    | not {context operand} : Typed spelling signatures context operand boolType →
        Typed spelling signatures context (.unary .not operand) boolType
    | negInt {context operand} : Typed spelling signatures context operand intType →
        Typed spelling signatures context (.unary .neg operand) intType
    | negFloat {context operand} : Typed spelling signatures context operand floatType →
        Typed spelling signatures context (.unary .neg operand) floatType
    | logical {context operator left right} : operator = .and ∨ operator = .or →
        Typed spelling signatures context left boolType → Typed spelling signatures context right boolType →
        Typed spelling signatures context (.binary operator left right) boolType
    | intArithmetic {context operator left right} : operator ∈ arithmetic →
        Typed spelling signatures context left intType → Typed spelling signatures context right intType →
        Typed spelling signatures context (.binary operator left right) intType
    | intComparison {context operator left right} : operator ∈ comparisons →
        Typed spelling signatures context left intType → Typed spelling signatures context right intType →
        Typed spelling signatures context (.binary operator left right) boolType
    | floatArithmetic {context operator left right} : operator ∈ floatArithmetic →
        Typed spelling signatures context left floatType → Typed spelling signatures context right floatType →
        Typed spelling signatures context (.binary operator left right) floatType
    | floatComparison {context operator left right} : operator ∈ comparisons →
        Typed spelling signatures context left floatType → Typed spelling signatures context right floatType →
        Typed spelling signatures context (.binary operator left right) boolType
    | durationSum {context operator left right} : operator = .add ∨ operator = .sub →
        Typed spelling signatures context left durType → Typed spelling signatures context right durType →
        Typed spelling signatures context (.binary operator left right) durType
    | durationRatio {context left right} : Typed spelling signatures context left durType →
        Typed spelling signatures context right durType → Typed spelling signatures context (.binary .div left right) intType
    | durationComparison {context operator left right} : operator ∈ comparisons →
        Typed spelling signatures context left durType → Typed spelling signatures context right durType →
        Typed spelling signatures context (.binary operator left right) boolType
    | durationScale {context operator left right} : operator = .mul ∨ operator = .div →
        Typed spelling signatures context left durType → Typed spelling signatures context right intType →
        Typed spelling signatures context (.binary operator left right) durType
    | durationScaleLeft {context left right} : Typed spelling signatures context left intType →
        Typed spelling signatures context right durType → Typed spelling signatures context (.binary .mul left right) durType
    | stringConcat {context left right} : Typed spelling signatures context left strType →
        Typed spelling signatures context right strType → Typed spelling signatures context (.binary .add left right) strType
    | stringComparison {context operator left right} : operator ∈ comparisons →
        Typed spelling signatures context left strType → Typed spelling signatures context right strType →
        Typed spelling signatures context (.binary operator left right) boolType
    | boolEquality {context operator left right} : operator ∈ equalities →
        Typed spelling signatures context left boolType → Typed spelling signatures context right boolType →
        Typed spelling signatures context (.binary operator left right) boolType
    | listConcat {context left right element} : Typed spelling signatures context left (.list element) →
        Typed spelling signatures context right (.list element) →
        Typed spelling signatures context (.binary .add left right) (.list element)
    | conditional {context conditions otherwise type} :
        (∀ branch ∈ conditions, Typed spelling signatures context branch.1 boolType) →
        (∀ branch ∈ conditions, Typed spelling signatures context branch.2 type) →
        Typed spelling signatures context otherwise type → Typed spelling signatures context (.ifExpr conditions otherwise) type
    | call {context name} {values : List Xsh.Syntax.Expr} {parameters : List (ValueType Declaration)} {result} :
        context name = none →
        signatures name = some (parameters, result) → values.length = parameters.length →
        (∀ pair ∈ values.zip parameters, Typed spelling signatures context pair.1 pair.2) →
        Typed spelling signatures context (.call (.ident name) (values.map .positional)) result

  inductive FieldsTyped (spelling : Xsh.Syntax.RawName → String) (signatures : Signatures Declaration) :
      TypeContext Declaration → List Xsh.Syntax.RecordField → RecordType Declaration → Prop where
    | nil {context} : FieldsTyped spelling signatures context [] .rowEmpty
    | cons {context name operand rest type row} : Typed spelling signatures context operand type →
        FieldsTyped spelling signatures context rest row → spelling name ∉ namesOf row →
        FieldsTyped spelling signatures context (.named name operand :: rest) (.rowField (spelling name) type row)
end

def overlay (parameters : List Xsh.Syntax.RawName) (types : List (ValueType Declaration))
    (context : TypeContext Declaration) : TypeContext Declaration :=
  fun name => ((parameters.zip types).lookup name).orElse fun _ => context name

end Xsh.Typing.Core.Expressions
