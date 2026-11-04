import Xsh.Typing.Connected.QualifiedBlockValidate
import Xsh.Typing.Connected.QualifiedNodeBlock

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def QualifiedNodeCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution requirements fields binderTypes result completion source candidate,
    validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion source candidate = .ok () →
    QualifiedNodeBlockHasCompletion solution requirements scope result completion candidate

def QualifiedNodeBranchCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution requirements fields binderTypes result completion source candidate,
    validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result completion source candidate = .ok () →
    QualifiedNodeBranchesHaveCompletion solution requirements scope result completion candidate

theorem qualified_node_return_completion_sound (fuel : Nat) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletion solution requirements scope result completion (.returned node :: nodes) := by
  simp [validateQualifiedBlockCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, expressionAccepted, rfl, remainingAccepted⟩ := accepted
  exact .returned (accepted_qualified_node_has_type fuel scope solution requirements fields value node _ expressionAccepted)
    (ih names scope solution requirements fields binderTypes _ none rest nodes remainingAccepted)

theorem qualified_node_assignment_completion_sound (fuel : Nat) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (value : Expr)
    (rest : List Statement) (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletion solution requirements scope result completion (.assignment binder node :: nodes) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedBlockCompletion, found] at accepted
  | some binding =>
    simp [validateQualifiedBlockCompletion, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨same, mutable⟩, _, expressionAccepted, rfl, remainingAccepted⟩ := accepted
    subst binder
    exact .assignment found mutable
      (accepted_qualified_node_has_type fuel scope solution requirements fields value node _ expressionAccepted)
      (ih names scope solution requirements fields binderTypes result completion rest nodes remainingAccepted)

theorem qualified_node_binding_completion_sound (fuel : Nat) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (kind : Xsh.Syntax.BindingKind)
    (name : NameId) (declared : Option TypeExpr) (initializer : Expr) (rest : List Statement)
    (binder : BinderId) (recordedKind : BindingKind) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.binding kind (.name name) declared initializer :: rest) (.binding binder recordedKind node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletion solution requirements scope result completion (.binding binder recordedKind node :: nodes) := by
  cases found : binderTypes binder with
  | none => simp [validateQualifiedBlockCompletion, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateQualifiedBlockCompletion, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, _, expressionAccepted, valueEqual, _, remainingAccepted⟩ := accepted
    subst recordedKind
    apply QualifiedNodeBlockHasCompletion.binding (rawType := rawType) (name := name)
    · exact valueEqual ▸ accepted_qualified_node_has_type fuel scope solution requirements fields initializer node _ expressionAccepted
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution requirements fields binderTypes result completion rest nodes remainingAccepted

theorem qualified_node_tail_completion_sound (fuel : Nat)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.tailName name :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletion solution requirements scope result completion (.evaluated node :: nodes) := by
  simp [validateQualifiedBlockCompletion, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
  obtain ⟨⟨_, empty⟩, ty, expressionAccepted, completionAccepted⟩ := accepted
  subst nodes
  simp [requireCheck] at completionAccepted
  exact .evaluatedTail (accepted_qualified_node_has_type fuel scope solution requirements fields (.ident name) node ty expressionAccepted)
    (check_completion_sound completion ty completionAccepted)

theorem qualified_node_evaluated_completion_sound (fuel : Nat) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletion solution requirements scope result completion (.evaluated node :: nodes) := by
  cases rest with
  | nil =>
    simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨ty, expressionAccepted, empty, completionAccepted⟩ := accepted
    subst nodes
    exact .evaluatedTail (accepted_qualified_node_has_type fuel scope solution requirements fields value node ty expressionAccepted)
      (check_completion_sound completion ty completionAccepted)
  | cons next rest =>
    simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨ty, expressionAccepted⟩, restAccepted⟩ := accepted
    cases nodes with
    | nil => cases fuel <;> cases restAccepted
    | cons candidate candidates =>
      exact .evaluatedNext (accepted_qualified_node_has_type fuel scope solution requirements fields value node ty expressionAccepted)
        (ih names scope solution requirements fields binderTypes result completion (next :: rest) (candidate :: candidates) restAccepted)

theorem qualified_node_scoped_completion_sound (fuel : Nat) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (body rest : List Statement)
    (nested nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.expression (.valueBlock body) :: rest) (.scoped nested :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletion solution requirements scope result completion (.scoped nested :: nodes) := by
  cases rest with
  | nil =>
    simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨⟨⟨⟩, bodyAccepted⟩, empty⟩ := accepted
    subst nodes
    exact .scopedTail (ih names scope solution requirements fields binderTypes result completion body nested bodyAccepted)
  | cons next rest =>
    simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨⟨⟩, bodyAccepted⟩, restAccepted⟩ := accepted
    cases nodes with
    | nil => cases fuel <;> cases restAccepted
    | cons candidate candidates =>
      exact .scopedNext (ih names scope solution requirements fields binderTypes result none body nested bodyAccepted)
        (ih names scope solution requirements fields binderTypes result completion (next :: rest) (candidate :: candidates) restAccepted)

theorem qualified_node_branch_completion_sound (fuel : Nat) (blockIH : QualifiedNodeCompletionSound fuel)
    (branchesIH : QualifiedNodeBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateQualifiedBranchesCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    QualifiedNodeBranchesHaveCompletion solution requirements scope result completion ((guard, nodes) :: candidates) := by
  simp [validateQualifiedBranchesCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (accepted_qualified_node_has_type fuel scope solution requirements fields condition guard _ guardAccepted)
    (blockIH names scope solution requirements fields binderTypes result completion body nodes bodyAccepted)
    (branchesIH names scope solution requirements fields binderTypes result completion rest candidates remainingAccepted)

theorem qualified_node_conditional_completion_sound (fuel : Nat) (blockIH : QualifiedNodeCompletionSound fuel)
    (branchesIH : QualifiedNodeBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (branches : List (Expr × List Statement))
    (otherwise : Option (List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.ifStmt branches otherwise :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletion solution requirements scope result completion (.conditional candidates fallback :: nodes) := by
  cases rest with
  | nil =>
    cases otherwise with
    | none =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨_, fallbackEmpty⟩, completionAccepted, empty⟩ := accepted
      subst nodes
      exact .conditionalTail
        (branchesIH names scope solution requirements fields binderTypes result completion branches candidates branchAccepted)
        (.nil (check_completion_sound completion (.primitive .unit) completionAccepted))
    | some body =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, empty⟩ := accepted
      subst nodes
      exact .conditionalTail
        (branchesIH names scope solution requirements fields binderTypes result completion branches candidates branchAccepted)
        (blockIH names scope solution requirements fields binderTypes result completion body fallback bodyAccepted)
  | cons next rest =>
    cases otherwise with
    | none =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, fallbackEmpty, _, remainingAccepted⟩ := accepted
      subst fallback
      cases nodes with
      | nil => cases fuel <;> cases remainingAccepted
      | cons candidate candidates' =>
        exact .conditionalNext
          (branchesIH names scope solution requirements fields binderTypes result none branches candidates branchAccepted)
          (.nil (Or.inl rfl))
          (blockIH names scope solution requirements fields binderTypes result completion (next :: rest) (candidate :: candidates') remainingAccepted)
    | some body =>
      simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
      cases nodes with
      | nil => cases fuel <;> cases remainingAccepted
      | cons candidate candidates' =>
        exact .conditionalNext
          (branchesIH names scope solution requirements fields binderTypes result none branches candidates branchAccepted)
          (blockIH names scope solution requirements fields binderTypes result none body fallback bodyAccepted)
          (blockIH names scope solution requirements fields binderTypes result completion (next :: rest) (candidate :: candidates') remainingAccepted)

theorem validate_qualified_node_completion_blocks_sound : ∀ fuel, QualifiedNodeCompletionSound fuel ∧ QualifiedNodeBranchCompletionSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [QualifiedNodeCompletionSound, QualifiedNodeBranchCompletionSound, validateQualifiedBlockCompletion, validateQualifiedBranchesCompletion]
  | succ fuel ih =>
    obtain ⟨blockIH, branchesIH⟩ := ih
    constructor
    · intro names scope solution requirements fields binderTypes result completion source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil =>
          simp [validateQualifiedBlockCompletion, requireCheck] at accepted
          exact .nil (check_completion_sound completion (.primitive .unit) accepted)
        | cons _ _ => cases accepted
      | cons statement rest =>
        cases candidate with
        | nil => cases accepted
        | cons node nodes =>
          cases statement with
          | binding kind target declared initializer =>
            cases target <;> cases node <;> first
              | exact qualified_node_binding_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                  kind _ declared initializer rest _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact qualified_node_assignment_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact qualified_node_return_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                  _ rest _ nodes accepted
              | cases accepted
          | tailName name =>
            cases node <;> first
              | exact qualified_node_tail_completion_sound fuel names scope solution requirements fields binderTypes result completion
                  name rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node with
            | evaluated node =>
              exact qualified_node_evaluated_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                value rest node nodes accepted
            | «scoped» nested =>
              cases value <;> first
                | exact qualified_node_scoped_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                    _ rest nested nodes accepted
                | cases accepted
            | _ => cases value <;> cases accepted
          | ifStmt branches otherwise =>
            cases node <;> first
              | exact qualified_node_conditional_completion_sound fuel blockIH branchesIH names scope solution requirements fields binderTypes
                  result completion branches otherwise rest _ _ nodes accepted
              | cases accepted
          | _ => cases node <;> cases accepted
    · intro names scope solution requirements fields binderTypes result completion source candidate accepted
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
              exact qualified_node_branch_completion_sound fuel blockIH branchesIH names scope solution requirements fields binderTypes result
                completion condition body rest guard nodes candidates accepted

def QualifiedNodeBlockSound (fuel : Nat) : Prop :=
  ∀ names scope principal requirements fields binderTypes result source candidate,
    validateQualifiedBlock fuel names scope principal requirements fields binderTypes result source candidate = .ok () →
    QualifiedNodeBlock principal requirements scope result candidate

def QualifiedNodeStatementBranchesSound (fuel : Nat) : Prop :=
  ∀ names scope principal requirements fields binderTypes result source candidate,
    validateQualifiedStatementBranches fuel names scope principal requirements fields binderTypes result source candidate = .ok () →
    QualifiedNodeStatementBranches principal requirements scope result candidate

theorem validate_qualified_node_blocks_sound (fuel : Nat) :
    QualifiedNodeBlockSound fuel ∧ QualifiedNodeStatementBranchesSound fuel := by
  constructor
  · intro names scope principal requirements fields binderTypes result source candidate accepted
    exact (validate_qualified_node_completion_blocks_sound fuel).1 names scope principal requirements fields binderTypes
      result (some result) source candidate accepted
  · intro names scope principal requirements fields binderTypes result source candidate accepted
    exact (validate_qualified_node_completion_blocks_sound fuel).2 names scope principal requirements fields binderTypes
      result (some result) source candidate accepted

theorem validated_qualified_node_block_instantiates (fuel : Nat) (names : TypeNames) (scope : Scope)
    (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (binderTypes : BinderTypes) (result : MonoType) (source : List Statement) (candidate : List StatementNode)
    (accepted : validateQualifiedBlock fuel names scope principal requirements fields binderTypes result source candidate = .ok ())
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeBlockHasType (compose later principal) scope (substitute later result) candidate :=
  qualified_node_block_instantiates principal later requirements scope result candidate
    ((validate_qualified_node_blocks_sound fuel).1 names scope principal requirements fields binderTypes result source candidate accepted)
    sorted solves

end Xsh.Typing.Connected
