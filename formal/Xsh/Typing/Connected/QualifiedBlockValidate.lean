import Xsh.Typing.Connected.QualifiedSound
import Xsh.Typing.Connected.BlockValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

mutual
  def validateQualifiedBlockCompletion : Nat → TypeNames → Scope → Substitution → List Constraint →
      FieldResults → BinderTypes → MonoType → Option MonoType → List Statement → List StatementNode →
      Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, _, completion, [], [] =>
        requireCheck (checkCompletion completion (.primitive .unit))
    | fuel + 1, names, scope, solution, requirements, fields, binderTypes, result, completion, statement :: rest, node :: nodes => do
      match statement, node with
      | .binding kind (.name name) declared initializer, .binding binder recordedKind candidate => do
          requireCheck (decide (bindingKind kind = recordedKind))
          requireCheck (!(scope.any fun item => decide (item.binder = binder)))
          let some rawType := binderTypes binder | throw .mismatch
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields initializer candidate
          requireCheck (decide (valueType = substitute solution rawType))
          requireCheck (checkAnnotation names solution declared valueType)
          let binding : Binding := ⟨name, binder, recordedKind, ⟨[], rawType, []⟩⟩
          validateQualifiedBlockCompletion fuel names (binding :: scope) solution requirements fields binderTypes result completion rest nodes
      | .assign (.name name) .set value, .assignment binder candidate => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .mutable))
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields value candidate
          requireCheck (decide (valueType = substitute solution binding.scheme.body))
          validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion rest nodes
      | .returnValue (some value), .returned candidate => do
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields value candidate
          requireCheck (decide (valueType = result))
          validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result none rest nodes
      | .tailName name, .evaluated candidate => do
          requireCheck (rest.isEmpty && nodes.isEmpty)
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields (.ident name) candidate
          requireCheck (checkCompletion completion valueType)
      | .expression (.valueBlock body), .scoped nested => do
          let nestedCompletion := if rest.isEmpty then completion else none
          validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result nestedCompletion body nested
          if rest.isEmpty then requireCheck nodes.isEmpty
          else validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion rest nodes
      | .expression value, .evaluated candidate => do
          let valueType ← validateQualifiedExpression fuel scope solution requirements fields value candidate
          if rest.isEmpty then do
            requireCheck nodes.isEmpty
            requireCheck (checkCompletion completion valueType)
          else validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion rest nodes
      | .ifStmt branches otherwise, .conditional candidates fallback => do
          let branchCompletion := if rest.isEmpty then completion else none
          validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result branchCompletion branches candidates
          match otherwise with
          | none => do
              requireCheck fallback.isEmpty
              requireCheck (checkCompletion branchCompletion (.primitive .unit))
          | some body => validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result branchCompletion body fallback
          if rest.isEmpty then requireCheck nodes.isEmpty
          else validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion rest nodes
      | _, _ => throw .mismatch
    | _ + 1, _, _, _, _, _, _, _, _, _, _ => .error .mismatch

  def validateQualifiedBranchesCompletion : Nat → TypeNames → Scope → Substitution → List Constraint →
      FieldResults → BinderTypes → MonoType → Option MonoType → List (Expr × List Statement) →
      List (Node × List StatementNode) → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, names, scope, solution, requirements, fields, binderTypes, result, completion,
        (condition, body) :: rest, (guard, nodes) :: candidates => do
      let guardType ← validateQualifiedExpression fuel scope solution requirements fields condition guard
      requireCheck (decide (guardType = .primitive .boolean))
      validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion body nodes
      validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result completion rest candidates
    | _ + 1, _, _, _, _, _, _, _, _, _, _ => .error .mismatch
end

def validateQualifiedBlock (fuel : Nat) (names : TypeNames) (scope : Scope) (solution : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (source : List Statement) (candidate : List StatementNode) : Except ValidationError Unit :=
  validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result (some result) source candidate

def validateQualifiedStatementBranches (fuel : Nat) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (source : List (Expr × List Statement))
    (candidate : List (Node × List StatementNode)) : Except ValidationError Unit :=
  validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result (some result) source candidate

end Xsh.Typing.Connected
