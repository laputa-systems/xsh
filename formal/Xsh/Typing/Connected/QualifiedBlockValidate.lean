import Xsh.Typing.Connected.QualifiedSound
import Xsh.Typing.Connected.BlockValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

mutual
  def validateQualifiedBlock : Nat → TypeNames → Scope → Substitution → List Constraint → FieldResults → BinderTypes → MonoType →
      List Statement → List StatementNode → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, names, scope, solution, requirements, fields, binderTypes, result, statement :: rest, node :: nodes => do
      match statement, node with
      | .binding kind (.name name) declared initializer, .binding binder recordedKind candidate => do
          requireCheck (decide (bindingKind kind = recordedKind))
          requireCheck (!(scope.any fun item => decide (item.binder = binder)))
          let some rawType := binderTypes binder | throw .mismatch
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields initializer candidate
          requireCheck (decide (valueType = substitute solution rawType))
          requireCheck (checkAnnotation names solution declared valueType)
          let binding : Binding := ⟨name, binder, recordedKind, ⟨[], rawType, []⟩⟩
          validateQualifiedBlock fuel names (binding :: scope) solution requirements fields binderTypes result rest nodes
      | .assign (.name name) .set value, .assignment binder candidate => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .mutable))
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields value candidate
          requireCheck (decide (valueType = substitute solution binding.scheme.body))
          validateQualifiedBlock fuel names scope solution requirements fields binderTypes result rest nodes
      | .returnValue (some value), .returned candidate => do
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields value candidate
          requireCheck (decide (valueType = result))
          validateQualifiedBlock fuel names scope solution requirements fields binderTypes result rest nodes
      | .tailName name, .evaluated candidate => do
          requireCheck rest.isEmpty
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields (.ident name) candidate
          requireCheck (decide (valueType = result))
          validateQualifiedBlock fuel names scope solution requirements fields binderTypes result rest nodes
      | .expression value, .evaluated candidate => do
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields value candidate
          requireCheck (!rest.isEmpty || decide (valueType = result))
          validateQualifiedBlock fuel names scope solution requirements fields binderTypes result rest nodes
      | .ifStmt branches otherwise, .conditional candidates fallback => do
          validateQualifiedStatementBranches fuel names scope solution requirements fields binderTypes result branches candidates
          match otherwise with
          | none => requireCheck fallback.isEmpty
          | some body => validateQualifiedBlock fuel names scope solution requirements fields binderTypes result body fallback
          validateQualifiedBlock fuel names scope solution requirements fields binderTypes result rest nodes
      | _, _ => throw .mismatch
    | _ + 1, _, _, _, _, _, _, _, _, _ => .error .mismatch

  def validateQualifiedStatementBranches : Nat → TypeNames → Scope → Substitution → List Constraint → FieldResults → BinderTypes → MonoType →
      List (Expr × List Statement) → List (Node × List StatementNode) → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, names, scope, solution, requirements, fields, binderTypes, result,
        (condition, body) :: rest, (guard, nodes) :: candidates => do
      let guardType ← validateQualifiedExpression fuel scope solution requirements fields condition guard
      requireCheck (decide (guardType = .primitive .boolean))
      validateQualifiedBlock fuel names scope solution requirements fields binderTypes result body nodes
      validateQualifiedStatementBranches fuel names scope solution requirements fields binderTypes result rest candidates
    | _ + 1, _, _, _, _, _, _, _, _, _ => .error .mismatch
end

end Xsh.Typing.Connected
