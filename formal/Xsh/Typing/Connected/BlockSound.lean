import Xsh.Typing.Connected.BlockValidate
import Xsh.Typing.Connected.Sound

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

theorem validate_return_sound (fuel : Nat) (ih : BlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (value : Expr) (rest : List Statement) (node : Node)
    (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    BlockHasType names solution scope result (.returnValue (some value) :: rest) := by
  simp [validateBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, expressionAccepted, rfl, remainingAccepted⟩ := accepted
  exact .returned (accepted_expression_has_type fuel scope solution value node _ expressionAccepted)
    (ih names scope solution binderTypes _ rest nodes remainingAccepted)


theorem validate_assignment_sound (fuel : Nat) (ih : BlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (name : NameId) (value : Expr) (rest : List Statement)
    (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    BlockHasType names solution scope result (.assign (.name name) .set value :: rest) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateBlock, found] at accepted
  | some binding =>
    simp [validateBlock, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨_, mutable⟩, _, expressionAccepted, rfl, remainingAccepted⟩ := accepted
    exact .assignment found mutable
      (accepted_expression_has_type fuel scope solution value node _ expressionAccepted)
      (ih names scope solution binderTypes result rest nodes remainingAccepted)

theorem validate_evaluated_sound (fuel : Nat) (ih : BlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (value : Expr) (rest : List Statement) (node : Node)
    (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    BlockHasType names solution scope result (.expression value :: rest) := by
  simp [validateBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨ty, expressionAccepted, tail, remainingAccepted⟩ := accepted
  apply BlockHasType.expression (accepted_expression_has_type fuel scope solution value node ty expressionAccepted)
  · intro empty
    subst rest
    simpa using tail
  · exact ih names scope solution binderTypes result rest nodes remainingAccepted

theorem validate_binding_sound (fuel : Nat) (ih : BlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (kind : Xsh.Syntax.BindingKind) (name : NameId) (declared : Option TypeExpr)
    (initializer : Expr) (rest : List Statement) (binder : BinderId) (recordedKind : BindingKind)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.binding kind (.name name) declared initializer :: rest)
      (.binding binder recordedKind node :: nodes) = .ok ()) :
    BlockHasType names solution scope result (.binding kind (.name name) declared initializer :: rest) := by
  cases found : binderTypes binder with
  | none => simp [validateBlock, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateBlock, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, _, expressionAccepted, valueEqual, annotationAccepted, remainingAccepted⟩ := accepted
    subst recordedKind
    apply BlockHasType.binding (rawType := rawType)
    · exact valueEqual ▸ accepted_expression_has_type fuel scope solution initializer node _ expressionAccepted
    · exact valueEqual ▸ check_annotation_sound names solution declared _ annotationAccepted
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution binderTypes result rest nodes remainingAccepted


theorem validate_conditional_without_fallback_sound (fuel : Nat)
    (blockIH : BlockSound fuel) (branchesIH : StatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (branches : List (Expr × List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.ifStmt branches none :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    BlockHasType names solution scope result (.ifStmt branches none :: rest) := by
  simp [validateBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨⟨⟨⟩, branchAccepted⟩, _, remainingAccepted⟩ := accepted
  exact .conditionalWithoutFallback
    (branchesIH names scope solution binderTypes result branches candidates branchAccepted)
    (blockIH names scope solution binderTypes result rest nodes remainingAccepted)

theorem validate_conditional_with_fallback_sound (fuel : Nat)
    (blockIH : BlockSound fuel) (branchesIH : StatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (branches : List (Expr × List Statement)) (otherwise rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.ifStmt branches (some otherwise) :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    BlockHasType names solution scope result (.ifStmt branches (some otherwise) :: rest) := by
  simp [validateBlock, except_bind_ok] at accepted
  obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, fallbackAccepted⟩, remainingAccepted⟩ := accepted
  exact .conditionalWithFallback
    (branchesIH names scope solution binderTypes result branches candidates branchAccepted)
    (blockIH names scope solution binderTypes result otherwise fallback fallbackAccepted)
    (blockIH names scope solution binderTypes result rest nodes remainingAccepted)

theorem validate_statement_branch_sound (fuel : Nat)
    (blockIH : BlockSound fuel) (branchesIH : StatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateStatementBranches (fuel + 1) names scope solution binderTypes result
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    StatementBranchesHaveType names solution scope result ((condition, body) :: rest) := by
  simp [validateStatementBranches, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (accepted_expression_has_type fuel scope solution condition guard _ guardAccepted)
    (blockIH names scope solution binderTypes result body nodes bodyAccepted)
    (branchesIH names scope solution binderTypes result rest candidates remainingAccepted)


theorem validate_blocks_sound : ∀ fuel, BlockSound fuel ∧ StatementBranchesSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [BlockSound, StatementBranchesSound, validateBlock, validateStatementBranches]
  | succ fuel ih =>
    obtain ⟨blockIH, branchesIH⟩ := ih
    constructor
    · intro names scope solution binderTypes result source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil => exact .nil
        | cons _ _ => cases accepted
      | cons statement rest =>
        cases candidate with
        | nil => cases accepted
        | cons node nodes =>
          cases statement with
          | binding kind target declared initializer =>
            cases target <;> cases node <;> first
              | exact validate_binding_sound fuel blockIH names scope solution binderTypes result
                  kind _ declared initializer rest _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact validate_assignment_sound fuel blockIH names scope solution binderTypes result
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact validate_return_sound fuel blockIH names scope solution binderTypes result
                  _ rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node <;> first
              | exact validate_evaluated_sound fuel blockIH names scope solution binderTypes result
                  value rest _ nodes accepted
              | cases accepted
          | ifStmt branches otherwise =>
            cases otherwise <;> cases node <;> first
              | exact validate_conditional_without_fallback_sound fuel blockIH branchesIH names scope
                  solution binderTypes result branches rest _ _ nodes accepted
              | exact validate_conditional_with_fallback_sound fuel blockIH branchesIH names scope
                  solution binderTypes result branches _ rest _ _ nodes accepted
              | cases accepted
          | _ => cases node <;> cases accepted
    · intro names scope solution binderTypes result source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil => exact .nil
        | cons _ _ => cases accepted
      | cons branch rest =>
        cases candidate with
        | nil => cases accepted
        | cons candidate candidates =>
          cases branch with
          | mk condition body =>
            cases candidate with
            | mk guard nodes =>
              exact validate_statement_branch_sound fuel blockIH branchesIH names scope solution
                binderTypes result condition body rest guard nodes candidates accepted

theorem accepted_block_has_type (fuel : Nat) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (binderTypes : BinderTypes) (result : MonoType)
    (source : List Statement) (candidate : List StatementNode)
    (accepted : validateBlock fuel names scope solution binderTypes result source candidate = .ok ()) :
    BlockHasType names solution scope result source :=
  (validate_blocks_sound fuel).1 names scope solution binderTypes result source candidate accepted

end Xsh.Typing.Connected
