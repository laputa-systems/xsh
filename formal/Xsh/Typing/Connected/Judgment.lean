import Xsh.Typing.Connected.Generation

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

inductive BinaryTyping : BinaryOp → MonoType → MonoType → MonoType → Prop where
  | add : BinaryTyping .add (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | sub : BinaryTyping .sub (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | mul : BinaryTyping .mul (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | div : BinaryTyping .div (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | rem : BinaryTyping .rem (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | lt : BinaryTyping .lt (.primitive .integer) (.primitive .integer) (.primitive .boolean)
  | le : BinaryTyping .le (.primitive .integer) (.primitive .integer) (.primitive .boolean)
  | gt : BinaryTyping .gt (.primitive .integer) (.primitive .integer) (.primitive .boolean)
  | ge : BinaryTyping .ge (.primitive .integer) (.primitive .integer) (.primitive .boolean)
  | and : BinaryTyping .and (.primitive .boolean) (.primitive .boolean) (.primitive .boolean)
  | or : BinaryTyping .or (.primitive .boolean) (.primitive .boolean) (.primitive .boolean)

theorem binary_signature_sound {operator : BinaryOp} {left right result : MonoType}
    (found : binarySignature operator = some (left, right, result)) :
    BinaryTyping operator left right result := by
  cases operator <;> simp [binarySignature] at found
  all_goals obtain ⟨rfl, rfl, rfl⟩ := found <;> constructor

theorem binary_signature_complete {operator : BinaryOp} {left right result : MonoType}
    (typed : BinaryTyping operator left right result) :
    binarySignature operator = some (left, right, result) := by
  cases typed <;> rfl

mutual
  inductive ExpressionHasType (scope : Scope) (solution : Substitution) : Expr → MonoType → Prop where
    | integer {lexeme : String} :
        ExpressionHasType scope solution (.literal (.integer lexeme)) (.primitive .integer)
    | boolean {value : Bool} :
        ExpressionHasType scope solution (.literal (.bool value)) (.primitive .boolean)
    | string {value : String} :
        ExpressionHasType scope solution (.literal (.string value)) (.primitive .string)
    | read {name : NameId} {binding : Binding} {arguments : Substitution}
        {ty : MonoType} {requirements : List Constraint} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        Instantiates binding.scheme arguments ty requirements →
        Solves requirements solution → WellFormed (substitute solution ty) →
        typeSort (substitute solution ty) = .value →
        ExpressionHasType scope solution (.ident name) (substitute solution ty)
    | binary {operator : BinaryOp} {left right : Expr} {leftType rightType result : MonoType} :
        ExpressionHasType scope solution left leftType → ExpressionHasType scope solution right rightType →
        BinaryTyping operator leftType rightType result →
        ExpressionHasType scope solution (.binary operator left right) result
    | call {callee : Expr} {arguments : List CallArgument} {argumentTypes result : MonoType} :
        ExpressionHasType scope solution callee (.function argumentTypes result) →
        ArgumentsHaveTypes scope solution arguments argumentTypes →
        ExpressionHasType scope solution (.call callee arguments) result
    | record {fields : List RecordField} {types : List (NameId × MonoType)} :
        RecordFieldsHaveTypes scope solution fields types → (types.map Prod.fst).Nodup →
        ExpressionHasType scope solution (.record fields)
          (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {base : Expr} {name : NameId} {row result : MonoType} :
        ExpressionHasType scope solution base (.record row) → FieldOccurs name result row →
        ExpressionHasType scope solution (.field base name) result
    | conditional {branches : List (Expr × Expr)} {otherwise : Expr} {result : MonoType} :
        BranchesHaveType scope solution branches result → ExpressionHasType scope solution otherwise result →
        ExpressionHasType scope solution (.ifExpr branches otherwise) result
    | propagate {value : Expr} {result error : MonoType} :
        ExpressionHasType scope solution value (.result result error) →
        ExpressionHasType scope solution (.tryExpr value) result

  inductive ArgumentsHaveTypes (scope : Scope) (solution : Substitution) : List CallArgument → MonoType → Prop where
    | nil : ArgumentsHaveTypes scope solution [] .argumentsEmpty
    | cons {expression : Expr} {rest : List CallArgument} {ty types : MonoType} :
        ExpressionHasType scope solution expression ty → ArgumentsHaveTypes scope solution rest types →
        ArgumentsHaveTypes scope solution (.positional expression :: rest) (.argument ty types)

  inductive RecordFieldsHaveTypes (scope : Scope) (solution : Substitution) : List RecordField → List (NameId × MonoType) → Prop where
    | nil : RecordFieldsHaveTypes scope solution [] []
    | cons {name : NameId} {expression : Expr} {rest : List RecordField}
        {ty : MonoType} {types : List (NameId × MonoType)} :
        ExpressionHasType scope solution expression ty → RecordFieldsHaveTypes scope solution rest types →
        RecordFieldsHaveTypes scope solution (.named name expression :: rest) ((name, ty) :: types)

  inductive BranchesHaveType (scope : Scope) (solution : Substitution) : List (Expr × Expr) → MonoType → Prop where
    | nil {result : MonoType} : BranchesHaveType scope solution [] result
    | cons {condition branch : Expr} {rest : List (Expr × Expr)} {result : MonoType} :
        ExpressionHasType scope solution condition (.primitive .boolean) →
        ExpressionHasType scope solution branch result → BranchesHaveType scope solution rest result →
        BranchesHaveType scope solution ((condition, branch) :: rest) result
end

end Xsh.Typing.Connected
