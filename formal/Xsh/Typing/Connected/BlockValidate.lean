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

def checkCompletion (expected : Option MonoType) (actual : MonoType) : Bool :=
  match expected with
  | none => true
  | some ty => decide (ty = actual)

theorem check_completion_sound (expected : Option MonoType) (actual : MonoType)
    (accepted : checkCompletion expected actual = true) : CompletionAccepts expected actual := by
  cases expected with
  | none => exact Or.inl rfl
  | some ty =>
    simp [checkCompletion] at accepted
    exact Or.inr (by cases accepted; rfl)

mutual
  def validateBlockCompletion : Nat → TypeNames → Scope → Substitution → BinderTypes → MonoType →
      Option MonoType → List Statement → List StatementNode → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, completion, [], [] =>
        requireCheck (checkCompletion completion (.primitive .unit))
    | fuel + 1, names, scope, solution, binderTypes, result, completion, statement :: rest, node :: nodes => do
      match statement, node with
      | .binding kind (.name name) declared initializer, .binding binder recordedKind candidate => do
          requireCheck (decide (bindingKind kind = recordedKind))
          requireCheck (!(scope.any fun item => decide (item.binder = binder)))
          let some rawType := binderTypes binder | throw .mismatch
          let valueType ← validateExpression fuel scope solution initializer candidate
          requireCheck (decide (valueType = substitute solution rawType))
          requireCheck (checkAnnotation names solution declared valueType)
          let binding : Binding := ⟨name, binder, recordedKind, ⟨[], rawType, []⟩⟩
          validateBlockCompletion fuel names (binding :: scope) solution binderTypes result completion rest nodes
      | .assign (.name name) .set value, .assignment binder candidate => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .mutable))
          let valueType ← validateExpression fuel scope solution value candidate
          requireCheck (decide (valueType = substitute solution binding.scheme.body))
          validateBlockCompletion fuel names scope solution binderTypes result completion rest nodes
      | .returnValue (some value), .returned candidate => do
          let valueType ← validateExpression fuel scope solution value candidate
          requireCheck (decide (valueType = result))
          validateBlockCompletion fuel names scope solution binderTypes result none rest nodes
      | .tailName name, .evaluated candidate => do
          requireCheck (rest.isEmpty && nodes.isEmpty)
          let valueType ← validateExpression fuel scope solution (.ident name) candidate
          requireCheck (checkCompletion completion valueType)
      | .expression (.valueBlock body), .scoped nested => do
          let nestedCompletion := if rest.isEmpty then completion else none
          validateBlockCompletion fuel names scope solution binderTypes result nestedCompletion body nested
          if rest.isEmpty then requireCheck nodes.isEmpty
          else validateBlockCompletion fuel names scope solution binderTypes result completion rest nodes
      | .expression value, .evaluated candidate => do
          let valueType ← validateExpression fuel scope solution value candidate
          if rest.isEmpty then do
            requireCheck nodes.isEmpty
            requireCheck (checkCompletion completion valueType)
          else validateBlockCompletion fuel names scope solution binderTypes result completion rest nodes
      | .ifStmt branches otherwise, .conditional candidates fallback => do
          let branchCompletion := if rest.isEmpty then completion else none
          validateStatementBranchesCompletion fuel names scope solution binderTypes result branchCompletion branches candidates
          match otherwise with
          | none => do
              requireCheck fallback.isEmpty
              requireCheck (checkCompletion branchCompletion (.primitive .unit))
          | some body => validateBlockCompletion fuel names scope solution binderTypes result branchCompletion body fallback
          if rest.isEmpty then requireCheck nodes.isEmpty
          else validateBlockCompletion fuel names scope solution binderTypes result completion rest nodes
      | _, _ => throw .mismatch
    | _ + 1, _, _, _, _, _, _, _, _ => .error .mismatch

  def validateStatementBranchesCompletion : Nat → TypeNames → Scope → Substitution → BinderTypes → MonoType →
      Option MonoType → List (Expr × List Statement) → List (Node × List StatementNode) →
      Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, names, scope, solution, binderTypes, result, completion,
        (condition, body) :: rest, (guard, nodes) :: candidates => do
      let guardType ← validateExpression fuel scope solution condition guard
      requireCheck (decide (guardType = .primitive .boolean))
      validateBlockCompletion fuel names scope solution binderTypes result completion body nodes
      validateStatementBranchesCompletion fuel names scope solution binderTypes result completion rest candidates
    | _ + 1, _, _, _, _, _, _, _, _ => .error .mismatch
end

def validateBlock (fuel : Nat) (names : TypeNames) (scope : Scope) (solution : Substitution)
    (binderTypes : BinderTypes) (result : MonoType) (source : List Statement)
    (candidate : List StatementNode) : Except ValidationError Unit :=
  validateBlockCompletion fuel names scope solution binderTypes result (some result) source candidate

def validateStatementBranches (fuel : Nat) (names : TypeNames) (scope : Scope) (solution : Substitution)
    (binderTypes : BinderTypes) (result : MonoType) (source : List (Expr × List Statement))
    (candidate : List (Node × List StatementNode)) : Except ValidationError Unit :=
  validateStatementBranchesCompletion fuel names scope solution binderTypes result (some result) source candidate

def BlockSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result source candidate,
    validateBlock fuel names scope solution binderTypes result source candidate = .ok () →
    BlockHasType names solution scope result source

def StatementBranchesSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result source candidate,
    validateStatementBranches fuel names scope solution binderTypes result source candidate = .ok () →
    StatementBranchesHaveType names solution scope result source

end Xsh.Typing.Connected
