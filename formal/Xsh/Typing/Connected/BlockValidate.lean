import Xsh.Typing.Connected.BlockJudgment
import Xsh.Typing.Connected.Validate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

abbrev BinderTypes := BinderId → Option MonoType

def checkAnnotation (names : TypeNames) (solution : Substitution)
    (declared : Option TypeExpr) (ty : MonoType) : Bool :=
  match declared with
  | none => true
  | some source => match annotation names source with
    | none => false
    | some annotated => decide (substitute solution annotated = ty)


theorem check_annotation_sound (names : TypeNames) (solution : Substitution)
    (declared : Option TypeExpr) (ty : MonoType)
    (accepted : checkAnnotation names solution declared ty = true) :
    AnnotationAgrees names solution declared ty := by
  cases declared with
  | none => trivial
  | some source =>
    cases found : annotation names source with
    | none => simp [checkAnnotation, found] at accepted
    | some annotated =>
      simp [checkAnnotation, found] at accepted
      exact ⟨annotated, found, accepted⟩

mutual
  def validateBlock : Nat → TypeNames → Scope → Substitution → BinderTypes → MonoType →
      List Statement → List StatementNode → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, names, scope, solution, binderTypes, result, statement :: rest, node :: nodes => do
      match statement, node with
      | .binding kind (.name name) declared initializer, .binding binder recordedKind candidate => do
          requireCheck (decide (bindingKind kind = recordedKind))
          requireCheck (!(scope.any fun item => decide (item.binder = binder)))
          let some rawType := binderTypes binder | throw .mismatch
          let valueType ← validateExpression fuel scope solution initializer candidate
          requireCheck (decide (valueType = substitute solution rawType))
          requireCheck (checkAnnotation names solution declared valueType)
          let binding : Binding := ⟨name, binder, recordedKind, ⟨[], rawType, []⟩⟩
          validateBlock fuel names (binding :: scope) solution binderTypes result rest nodes
      | .assign (.name name) .set value, .assignment binder candidate => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .mutable))
          let valueType ← validateExpression fuel scope solution value candidate
          requireCheck (decide (valueType = substitute solution binding.scheme.body))
          validateBlock fuel names scope solution binderTypes result rest nodes
      | .returnValue (some value), .returned candidate => do
          let valueType ← validateExpression fuel scope solution value candidate
          requireCheck (decide (valueType = result))
          validateBlock fuel names scope solution binderTypes result rest nodes
      | .tailName name, .evaluated candidate => do
          requireCheck rest.isEmpty
          let valueType ← validateExpression fuel scope solution (.ident name) candidate
          requireCheck (decide (valueType = result))
          validateBlock fuel names scope solution binderTypes result rest nodes
      | .expression value, .evaluated candidate => do
          let valueType ← validateExpression fuel scope solution value candidate
          requireCheck (!rest.isEmpty || decide (valueType = result))
          validateBlock fuel names scope solution binderTypes result rest nodes
      | .ifStmt branches otherwise, .conditional candidates fallback => do
          validateStatementBranches fuel names scope solution binderTypes result branches candidates
          match otherwise with
          | none => requireCheck fallback.isEmpty
          | some body => validateBlock fuel names scope solution binderTypes result body fallback
          validateBlock fuel names scope solution binderTypes result rest nodes
      | _, _ => throw .mismatch
    | _ + 1, _, _, _, _, _, _, _ => .error .mismatch

  def validateStatementBranches : Nat → TypeNames → Scope → Substitution → BinderTypes → MonoType →
      List (Expr × List Statement) → List (Node × List StatementNode) → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, names, scope, solution, binderTypes, result,
        (condition, body) :: rest, (guard, nodes) :: candidates => do
      let guardType ← validateExpression fuel scope solution condition guard
      requireCheck (decide (guardType = .primitive .boolean))
      validateBlock fuel names scope solution binderTypes result body nodes
      validateStatementBranches fuel names scope solution binderTypes result rest candidates
    | _ + 1, _, _, _, _, _, _, _ => .error .mismatch
end


def BlockSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result source candidate,
    validateBlock fuel names scope solution binderTypes result source candidate = .ok () →
    BlockHasType names solution scope result source

def StatementBranchesSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result source candidate,
    validateStatementBranches fuel names scope solution binderTypes result source candidate = .ok () →
    StatementBranchesHaveType names solution scope result source

end Xsh.Typing.Connected
