import Xsh.Typing.Connected.NodeBlockJudgment

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def NodeBlockSound (fuel : Nat) : Prop := ∀ names scope solution binderTypes result source candidate,
  validateBlock fuel names scope solution binderTypes result source candidate = .ok () →
    NodeBlockHasType solution scope result candidate

def NodeStatementBranchesSound (fuel : Nat) : Prop := ∀ names scope solution binderTypes result source candidate,
  validateStatementBranches fuel names scope solution binderTypes result source candidate = .ok () →
    NodeStatementBranchesHaveType solution scope result candidate

theorem validate_node_return_sound (fuel : Nat) (ih : NodeBlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (value : Expr) (rest : List Statement) (node : Node)
    (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    NodeBlockHasType solution scope result (.returned node :: nodes) := by
  simp [validateBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, expressionAccepted, rfl, remainingAccepted⟩ := accepted
  exact .returned (accepted_node_has_type fuel scope solution value node _ expressionAccepted)
    (ih names scope solution binderTypes _ rest nodes remainingAccepted)


theorem validate_node_assignment_sound (fuel : Nat) (ih : NodeBlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (name : NameId) (value : Expr) (rest : List Statement)
    (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    NodeBlockHasType solution scope result (.assignment binder node :: nodes) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateBlock, found] at accepted
  | some binding =>
    simp [validateBlock, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨same, mutable⟩, _, expressionAccepted, rfl, remainingAccepted⟩ := accepted
    subst binder
    exact .assignment found mutable
      (accepted_node_has_type fuel scope solution value node _ expressionAccepted)
      (ih names scope solution binderTypes result rest nodes remainingAccepted)

theorem validate_node_evaluated_sound (fuel : Nat) (ih : NodeBlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (value : Expr) (rest : List Statement) (node : Node)
    (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    NodeBlockHasType solution scope result (.evaluated node :: nodes) := by
  simp [validateBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨ty, expressionAccepted, tail, remainingAccepted⟩ := accepted
  apply NodeBlockHasType.evaluated (accepted_node_has_type fuel scope solution value node ty expressionAccepted)
  · intro empty
    have restEmpty : rest = [] := by
      cases rest with
      | nil => rfl
      | cons statement rest =>
        subst nodes
        cases fuel <;> cases remainingAccepted
    subst rest
    simpa using tail
  · exact ih names scope solution binderTypes result rest nodes remainingAccepted

theorem validate_node_tail_name_sound (fuel : Nat)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (name : NameId) (rest : List Statement) (node : Node)
    (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.tailName name :: rest) (.evaluated node :: nodes) = .ok ()) :
    NodeBlockHasType solution scope result (.evaluated node :: nodes) := by
  simp [validateBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨empty, _, expressionAccepted, equal, remainingAccepted⟩ := accepted
  subst rest
  subst result
  cases nodes with
  | nil => exact .evaluated (accepted_node_has_type fuel scope solution (.ident name) node _ expressionAccepted) (fun _ => rfl) .nil
  | cons head tail => cases fuel <;> cases remainingAccepted

theorem validate_node_binding_sound (fuel : Nat) (ih : NodeBlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (kind : Xsh.Syntax.BindingKind) (name : NameId) (declared : Option TypeExpr)
    (initializer : Expr) (rest : List Statement) (binder : BinderId) (recordedKind : BindingKind)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.binding kind (.name name) declared initializer :: rest)
      (.binding binder recordedKind node :: nodes) = .ok ()) :
    NodeBlockHasType solution scope result (.binding binder recordedKind node :: nodes) := by
  cases found : binderTypes binder with
  | none => simp [validateBlock, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateBlock, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, _, expressionAccepted, valueEqual, annotationAccepted, remainingAccepted⟩ := accepted
    subst recordedKind
    apply NodeBlockHasType.binding (rawType := rawType) (name := name)
    · exact valueEqual ▸ accepted_node_has_type fuel scope solution initializer node _ expressionAccepted
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution binderTypes result rest nodes remainingAccepted


theorem validate_node_conditional_without_fallback_sound (fuel : Nat)
    (blockIH : NodeBlockSound fuel) (branchesIH : NodeStatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (branches : List (Expr × List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.ifStmt branches none :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    NodeBlockHasType solution scope result (.conditional candidates fallback :: nodes) := by
  simp [validateBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨⟨⟨⟩, branchAccepted⟩, fallbackEmpty, remainingAccepted⟩ := accepted
  have : fallback = [] := by simpa using fallbackEmpty
  subst fallback
  exact .conditional
    (branchesIH names scope solution binderTypes result branches candidates branchAccepted) .nil
    (blockIH names scope solution binderTypes result rest nodes remainingAccepted)

theorem validate_node_conditional_with_fallback_sound (fuel : Nat)
    (blockIH : NodeBlockSound fuel) (branchesIH : NodeStatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (branches : List (Expr × List Statement)) (otherwise rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateBlock (fuel + 1) names scope solution binderTypes result
      (.ifStmt branches (some otherwise) :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    NodeBlockHasType solution scope result (.conditional candidates fallback :: nodes) := by
  simp [validateBlock, except_bind_ok] at accepted
  obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, fallbackAccepted⟩, remainingAccepted⟩ := accepted
  exact .conditional
    (branchesIH names scope solution binderTypes result branches candidates branchAccepted)
    (blockIH names scope solution binderTypes result otherwise fallback fallbackAccepted)
    (blockIH names scope solution binderTypes result rest nodes remainingAccepted)

theorem validate_node_statement_branch_sound (fuel : Nat)
    (blockIH : NodeBlockSound fuel) (branchesIH : NodeStatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateStatementBranches (fuel + 1) names scope solution binderTypes result
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    NodeStatementBranchesHaveType solution scope result ((guard, nodes) :: candidates) := by
  simp [validateStatementBranches, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (accepted_node_has_type fuel scope solution condition guard _ guardAccepted)
    (blockIH names scope solution binderTypes result body nodes bodyAccepted)
    (branchesIH names scope solution binderTypes result rest candidates remainingAccepted)


theorem validate_node_blocks_sound : ∀ fuel, NodeBlockSound fuel ∧ NodeStatementBranchesSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [NodeBlockSound, NodeStatementBranchesSound, validateBlock, validateStatementBranches]
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
              | exact validate_node_binding_sound fuel blockIH names scope solution binderTypes result
                  kind _ declared initializer rest _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact validate_node_assignment_sound fuel blockIH names scope solution binderTypes result
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact validate_node_return_sound fuel blockIH names scope solution binderTypes result
                  _ rest _ nodes accepted
              | cases accepted
          | tailName name =>
            cases node <;> first
              | exact validate_node_tail_name_sound fuel names scope solution binderTypes result
                  name rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node <;> first
              | exact validate_node_evaluated_sound fuel blockIH names scope solution binderTypes result
                  value rest _ nodes accepted
              | cases accepted
          | ifStmt branches otherwise =>
            cases otherwise <;> cases node <;> first
              | exact validate_node_conditional_without_fallback_sound fuel blockIH branchesIH names scope
                  solution binderTypes result branches rest _ _ nodes accepted
              | exact validate_node_conditional_with_fallback_sound fuel blockIH branchesIH names scope
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
              exact validate_node_statement_branch_sound fuel blockIH branchesIH names scope solution
                binderTypes result condition body rest guard nodes candidates accepted

theorem accepted_node_block_has_type (fuel : Nat) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (binderTypes : BinderTypes) (result : MonoType)
    (source : List Statement) (candidate : List StatementNode)
    (accepted : validateBlock fuel names scope solution binderTypes result source candidate = .ok ()) :
    NodeBlockHasType solution scope result candidate :=
  (validate_node_blocks_sound fuel).1 names scope solution binderTypes result source candidate accepted

end Xsh.Typing.Connected
