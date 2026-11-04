import Xsh.Typing.Connected.NodeBlockJudgment
import Xsh.Typing.Connected.BlockSound

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def NodeCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result completion source candidate,
    validateBlockCompletion fuel names scope solution binderTypes result completion source candidate = .ok () →
    NodeBlockHasCompletion solution scope result completion candidate

def NodeBranchCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result completion source candidate,
    validateStatementBranchesCompletion fuel names scope solution binderTypes result completion source candidate = .ok () →
    NodeBranchesHaveCompletion solution scope result completion candidate

theorem node_return_completion_sound (fuel : Nat) (ih : NodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    NodeBlockHasCompletion solution scope result completion (.returned node :: nodes) := by
  simp [validateBlockCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, expressionAccepted, rfl, remainingAccepted⟩ := accepted
  exact .returned (accepted_node_has_type fuel scope solution value node _ expressionAccepted)
    (ih names scope solution binderTypes _ none rest nodes remainingAccepted)

theorem node_assignment_completion_sound (fuel : Nat) (ih : NodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (value : Expr)
    (rest : List Statement) (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    NodeBlockHasCompletion solution scope result completion (.assignment binder node :: nodes) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateBlockCompletion, found] at accepted
  | some binding =>
    simp [validateBlockCompletion, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨same, mutable⟩, _, expressionAccepted, rfl, remainingAccepted⟩ := accepted
    subst binder
    exact .assignment found mutable
      (accepted_node_has_type fuel scope solution value node _ expressionAccepted)
      (ih names scope solution binderTypes result completion rest nodes remainingAccepted)

theorem node_binding_completion_sound (fuel : Nat) (ih : NodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (kind : Xsh.Syntax.BindingKind)
    (name : NameId) (declared : Option TypeExpr) (initializer : Expr) (rest : List Statement)
    (binder : BinderId) (recordedKind : BindingKind) (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.binding kind (.name name) declared initializer :: rest) (.binding binder recordedKind node :: nodes) = .ok ()) :
    NodeBlockHasCompletion solution scope result completion (.binding binder recordedKind node :: nodes) := by
  cases found : binderTypes binder with
  | none => simp [validateBlockCompletion, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateBlockCompletion, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, _, expressionAccepted, valueEqual, _, remainingAccepted⟩ := accepted
    subst recordedKind
    apply NodeBlockHasCompletion.binding (rawType := rawType) (name := name)
    · exact valueEqual ▸ accepted_node_has_type fuel scope solution initializer node _ expressionAccepted
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution binderTypes result completion rest nodes remainingAccepted

theorem node_tail_completion_sound (fuel : Nat)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.tailName name :: rest) (.evaluated node :: nodes) = .ok ()) :
    NodeBlockHasCompletion solution scope result completion (.evaluated node :: nodes) := by
  simp [validateBlockCompletion, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
  obtain ⟨⟨_, empty⟩, ty, expressionAccepted, completionAccepted⟩ := accepted
  subst nodes
  simp [requireCheck] at completionAccepted
  exact .evaluatedTail (accepted_node_has_type fuel scope solution (.ident name) node ty expressionAccepted)
    (check_completion_sound completion ty completionAccepted)

theorem node_evaluated_completion_sound (fuel : Nat) (ih : NodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    NodeBlockHasCompletion solution scope result completion (.evaluated node :: nodes) := by
  cases rest with
  | nil =>
    simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨ty, expressionAccepted, empty, completionAccepted⟩ := accepted
    subst nodes
    exact .evaluatedTail (accepted_node_has_type fuel scope solution value node ty expressionAccepted)
      (check_completion_sound completion ty completionAccepted)
  | cons next rest =>
    simp [validateBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨ty, expressionAccepted⟩, restAccepted⟩ := accepted
    cases nodes with
    | nil => cases fuel <;> cases restAccepted
    | cons candidate candidates =>
      exact .evaluatedNext (accepted_node_has_type fuel scope solution value node ty expressionAccepted)
        (ih names scope solution binderTypes result completion (next :: rest) (candidate :: candidates) restAccepted)

theorem node_scoped_completion_sound (fuel : Nat) (ih : NodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (body rest : List Statement)
    (nested nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.expression (.valueBlock body) :: rest) (.scoped nested :: nodes) = .ok ()) :
    NodeBlockHasCompletion solution scope result completion (.scoped nested :: nodes) := by
  cases rest with
  | nil =>
    simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨⟨⟨⟩, bodyAccepted⟩, empty⟩ := accepted
    subst nodes
    exact .scopedTail (ih names scope solution binderTypes result completion body nested bodyAccepted)
  | cons next rest =>
    simp [validateBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨⟨⟩, bodyAccepted⟩, restAccepted⟩ := accepted
    cases nodes with
    | nil => cases fuel <;> cases restAccepted
    | cons candidate candidates =>
      exact .scopedNext (ih names scope solution binderTypes result none body nested bodyAccepted)
        (ih names scope solution binderTypes result completion (next :: rest) (candidate :: candidates) restAccepted)

theorem node_branch_completion_sound (fuel : Nat) (blockIH : NodeCompletionSound fuel)
    (branchesIH : NodeBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateStatementBranchesCompletion (fuel + 1) names scope solution binderTypes result completion
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    NodeBranchesHaveCompletion solution scope result completion ((guard, nodes) :: candidates) := by
  simp [validateStatementBranchesCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (accepted_node_has_type fuel scope solution condition guard _ guardAccepted)
    (blockIH names scope solution binderTypes result completion body nodes bodyAccepted)
    (branchesIH names scope solution binderTypes result completion rest candidates remainingAccepted)

theorem node_conditional_completion_sound (fuel : Nat) (blockIH : NodeCompletionSound fuel)
    (branchesIH : NodeBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (branches : List (Expr × List Statement))
    (otherwise : Option (List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.ifStmt branches otherwise :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    NodeBlockHasCompletion solution scope result completion (.conditional candidates fallback :: nodes) := by
  cases rest with
  | nil =>
    cases otherwise with
    | none =>
      simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨_, fallbackEmpty⟩, completionAccepted, empty⟩ := accepted
      subst nodes
      exact .conditionalTail
        (branchesIH names scope solution binderTypes result completion branches candidates branchAccepted)
        (.nil (check_completion_sound completion (.primitive .unit) completionAccepted))
    | some body =>
      simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, empty⟩ := accepted
      subst nodes
      exact .conditionalTail
        (branchesIH names scope solution binderTypes result completion branches candidates branchAccepted)
        (blockIH names scope solution binderTypes result completion body fallback bodyAccepted)
  | cons next rest =>
    cases otherwise with
    | none =>
      simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, fallbackEmpty, _, remainingAccepted⟩ := accepted
      subst fallback
      cases nodes with
      | nil => cases fuel <;> cases remainingAccepted
      | cons candidate candidates' =>
        exact .conditionalNext
          (branchesIH names scope solution binderTypes result none branches candidates branchAccepted)
          (.nil (Or.inl rfl))
          (blockIH names scope solution binderTypes result completion (next :: rest) (candidate :: candidates') remainingAccepted)
    | some body =>
      simp [validateBlockCompletion, except_bind_ok] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
      cases nodes with
      | nil => cases fuel <;> cases remainingAccepted
      | cons candidate candidates' =>
        exact .conditionalNext
          (branchesIH names scope solution binderTypes result none branches candidates branchAccepted)
          (blockIH names scope solution binderTypes result none body fallback bodyAccepted)
          (blockIH names scope solution binderTypes result completion (next :: rest) (candidate :: candidates') remainingAccepted)

theorem validate_node_completion_blocks_sound : ∀ fuel, NodeCompletionSound fuel ∧ NodeBranchCompletionSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [NodeCompletionSound, NodeBranchCompletionSound, validateBlockCompletion, validateStatementBranchesCompletion]
  | succ fuel ih =>
    obtain ⟨blockIH, branchesIH⟩ := ih
    constructor
    · intro names scope solution binderTypes result completion source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil =>
          simp [validateBlockCompletion, requireCheck] at accepted
          exact .nil (check_completion_sound completion (.primitive .unit) accepted)
        | cons _ _ => cases accepted
      | cons statement rest =>
        cases candidate with
        | nil => cases accepted
        | cons node nodes =>
          cases statement with
          | binding kind target declared initializer =>
            cases target <;> cases node <;> first
              | exact node_binding_completion_sound fuel blockIH names scope solution binderTypes result completion
                  kind _ declared initializer rest _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact node_assignment_completion_sound fuel blockIH names scope solution binderTypes result completion
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact node_return_completion_sound fuel blockIH names scope solution binderTypes result completion
                  _ rest _ nodes accepted
              | cases accepted
          | tailName name =>
            cases node <;> first
              | exact node_tail_completion_sound fuel names scope solution binderTypes result completion
                  name rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node with
            | evaluated node =>
              exact node_evaluated_completion_sound fuel blockIH names scope solution binderTypes result completion
                value rest node nodes accepted
            | «scoped» nested =>
              cases value <;> first
                | exact node_scoped_completion_sound fuel blockIH names scope solution binderTypes result completion
                    _ rest nested nodes accepted
                | cases accepted
            | _ => cases value <;> cases accepted
          | ifStmt branches otherwise =>
            cases node <;> first
              | exact node_conditional_completion_sound fuel blockIH branchesIH names scope solution binderTypes
                  result completion branches otherwise rest _ _ nodes accepted
              | cases accepted
          | _ => cases node <;> cases accepted
    · intro names scope solution binderTypes result completion source candidate accepted
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
              exact node_branch_completion_sound fuel blockIH branchesIH names scope solution binderTypes result
                completion condition body rest guard nodes candidates accepted

def NodeBlockSound (fuel : Nat) : Prop := ∀ names scope solution binderTypes result source candidate,
  validateBlock fuel names scope solution binderTypes result source candidate = .ok () →
    NodeBlockHasType solution scope result candidate

def NodeStatementBranchesSound (fuel : Nat) : Prop := ∀ names scope solution binderTypes result source candidate,
  validateStatementBranches fuel names scope solution binderTypes result source candidate = .ok () →
    NodeStatementBranchesHaveType solution scope result candidate

theorem validate_node_blocks_sound (fuel : Nat) : NodeBlockSound fuel ∧ NodeStatementBranchesSound fuel := by
  constructor
  · intro names scope solution binderTypes result source candidate accepted
    exact (validate_node_completion_blocks_sound fuel).1 names scope solution binderTypes result (some result)
      source candidate accepted
  · intro names scope solution binderTypes result source candidate accepted
    exact (validate_node_completion_blocks_sound fuel).2 names scope solution binderTypes result (some result)
      source candidate accepted

theorem accepted_node_block_has_type (fuel : Nat) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (binderTypes : BinderTypes) (result : MonoType)
    (source : List Statement) (candidate : List StatementNode)
    (accepted : validateBlock fuel names scope solution binderTypes result source candidate = .ok ()) :
    NodeBlockHasType solution scope result candidate :=
  (validate_node_blocks_sound fuel).1 names scope solution binderTypes result source candidate accepted

end Xsh.Typing.Connected
