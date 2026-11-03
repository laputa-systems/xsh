import Xsh.Typing.Connected.QualifiedBlockValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def QualifiedBlockSound (fuel : Nat) : Prop :=
  ∀ names scope principal requirements fields binderTypes result source candidate,
    validateQualifiedBlock fuel names scope principal requirements fields binderTypes result source candidate = .ok () →
    QualifiedBlock names principal requirements scope result source

def QualifiedStatementBranchesSound (fuel : Nat) : Prop :=
  ∀ names scope principal requirements fields binderTypes result source candidate,
    validateQualifiedStatementBranches fuel names scope principal requirements fields binderTypes result source candidate = .ok () →
    QualifiedStatementBranches names principal requirements scope result source

theorem validate_qualified_return_sound (fuel : Nat) (ih : QualifiedBlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (value : Expr) (rest : List Statement) (node : Node)
    (nodes : List StatementNode)
    (accepted : validateQualifiedBlock (fuel + 1) names scope solution requirements fields binderTypes result
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    QualifiedBlock names solution requirements scope result (.returnValue (some value) :: rest) := by
  simp [validateQualifiedBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, expressionAccepted, rfl, remainingAccepted⟩ := accepted
  exact .returned (accepted_qualified_expression_has_type fuel scope solution requirements fields value node _ expressionAccepted)
    (ih names scope solution requirements fields binderTypes _ rest nodes remainingAccepted)


theorem validate_qualified_assignment_sound (fuel : Nat) (ih : QualifiedBlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (name : NameId) (value : Expr) (rest : List Statement)
    (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlock (fuel + 1) names scope solution requirements fields binderTypes result
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    QualifiedBlock names solution requirements scope result (.assign (.name name) .set value :: rest) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedBlock, found] at accepted
  | some binding =>
    simp [validateQualifiedBlock, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨_, mutable⟩, _, expressionAccepted, rfl, remainingAccepted⟩ := accepted
    exact .assignment found mutable
      (accepted_qualified_expression_has_type fuel scope solution requirements fields value node _ expressionAccepted)
      (ih names scope solution requirements fields binderTypes result rest nodes remainingAccepted)

theorem validate_qualified_evaluated_sound (fuel : Nat) (ih : QualifiedBlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (value : Expr) (rest : List Statement) (node : Node)
    (nodes : List StatementNode)
    (accepted : validateQualifiedBlock (fuel + 1) names scope solution requirements fields binderTypes result
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedBlock names solution requirements scope result (.expression value :: rest) := by
  simp [validateQualifiedBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨ty, expressionAccepted, tail, remainingAccepted⟩ := accepted
  apply QualifiedBlock.expression (accepted_qualified_expression_has_type fuel scope solution requirements fields value node ty expressionAccepted)
  · intro empty
    subst rest
    simpa using tail
  · exact ih names scope solution requirements fields binderTypes result rest nodes remainingAccepted

theorem validate_qualified_tail_name_sound (fuel : Nat)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (name : NameId) (rest : List Statement) (node : Node)
    (nodes : List StatementNode)
    (accepted : validateQualifiedBlock (fuel + 1) names scope solution requirements fields binderTypes result
      (.tailName name :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedBlock names solution requirements scope result (.tailName name :: rest) := by
  simp [validateQualifiedBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨empty, _, expressionAccepted, equal, _⟩ := accepted
  subst rest
  subst result
  exact .tailName (accepted_qualified_expression_has_type fuel scope solution requirements fields (.ident name) node _ expressionAccepted)

theorem validate_qualified_binding_sound (fuel : Nat) (ih : QualifiedBlockSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (kind : Xsh.Syntax.BindingKind) (name : NameId) (declared : Option TypeExpr)
    (initializer : Expr) (rest : List Statement) (binder : BinderId) (recordedKind : BindingKind)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlock (fuel + 1) names scope solution requirements fields binderTypes result
      (.binding kind (.name name) declared initializer :: rest)
      (.binding binder recordedKind node :: nodes) = .ok ()) :
    QualifiedBlock names solution requirements scope result (.binding kind (.name name) declared initializer :: rest) := by
  cases found : binderTypes binder with
  | none => simp [validateQualifiedBlock, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateQualifiedBlock, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, _, expressionAccepted, valueEqual, annotationAccepted, remainingAccepted⟩ := accepted
    subst recordedKind
    apply QualifiedBlock.binding (rawType := rawType)
    · exact valueEqual ▸ accepted_qualified_expression_has_type fuel scope solution requirements fields initializer node _ expressionAccepted
    · exact valueEqual ▸ check_annotation_sound names solution declared _ annotationAccepted
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution requirements fields binderTypes result rest nodes remainingAccepted


theorem validate_qualified_conditional_without_fallback_sound (fuel : Nat)
    (blockIH : QualifiedBlockSound fuel) (branchesIH : QualifiedStatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (branches : List (Expr × List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateQualifiedBlock (fuel + 1) names scope solution requirements fields binderTypes result
      (.ifStmt branches none :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    QualifiedBlock names solution requirements scope result (.ifStmt branches none :: rest) := by
  simp [validateQualifiedBlock, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨⟨⟨⟩, branchAccepted⟩, _, remainingAccepted⟩ := accepted
  exact .conditionalWithoutFallback
    (branchesIH names scope solution requirements fields binderTypes result branches candidates branchAccepted)
    (blockIH names scope solution requirements fields binderTypes result rest nodes remainingAccepted)

theorem validate_qualified_conditional_with_fallback_sound (fuel : Nat)
    (blockIH : QualifiedBlockSound fuel) (branchesIH : QualifiedStatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (branches : List (Expr × List Statement)) (otherwise rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateQualifiedBlock (fuel + 1) names scope solution requirements fields binderTypes result
      (.ifStmt branches (some otherwise) :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    QualifiedBlock names solution requirements scope result (.ifStmt branches (some otherwise) :: rest) := by
  simp [validateQualifiedBlock, except_bind_ok] at accepted
  obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, fallbackAccepted⟩, remainingAccepted⟩ := accepted
  exact .conditionalWithFallback
    (branchesIH names scope solution requirements fields binderTypes result branches candidates branchAccepted)
    (blockIH names scope solution requirements fields binderTypes result otherwise fallback fallbackAccepted)
    (blockIH names scope solution requirements fields binderTypes result rest nodes remainingAccepted)

theorem validate_qualified_statement_branch_sound (fuel : Nat)
    (blockIH : QualifiedBlockSound fuel) (branchesIH : QualifiedStatementBranchesSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateQualifiedStatementBranches (fuel + 1) names scope solution requirements fields binderTypes result
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    QualifiedStatementBranches names solution requirements scope result ((condition, body) :: rest) := by
  simp [validateQualifiedStatementBranches, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (accepted_qualified_expression_has_type fuel scope solution requirements fields condition guard _ guardAccepted)
    (blockIH names scope solution requirements fields binderTypes result body nodes bodyAccepted)
    (branchesIH names scope solution requirements fields binderTypes result rest candidates remainingAccepted)


theorem validate_qualified_blocks_sound : ∀ fuel, QualifiedBlockSound fuel ∧ QualifiedStatementBranchesSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [QualifiedBlockSound, QualifiedStatementBranchesSound, validateQualifiedBlock, validateQualifiedStatementBranches]
  | succ fuel ih =>
    obtain ⟨blockIH, branchesIH⟩ := ih
    constructor
    · intro names scope solution requirements fields binderTypes result source candidate accepted
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
              | exact validate_qualified_binding_sound fuel blockIH names scope solution requirements fields binderTypes result
                  kind _ declared initializer rest _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact validate_qualified_assignment_sound fuel blockIH names scope solution requirements fields binderTypes result
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact validate_qualified_return_sound fuel blockIH names scope solution requirements fields binderTypes result
                  _ rest _ nodes accepted
              | cases accepted
          | tailName name =>
            cases node <;> first
              | exact validate_qualified_tail_name_sound fuel names scope solution requirements fields binderTypes result
                  name rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node <;> first
              | exact validate_qualified_evaluated_sound fuel blockIH names scope solution requirements fields binderTypes result
                  value rest _ nodes accepted
              | cases accepted
          | ifStmt branches otherwise =>
            cases otherwise <;> cases node <;> first
              | exact validate_qualified_conditional_without_fallback_sound fuel blockIH branchesIH names scope
                  solution requirements fields binderTypes result branches rest _ _ nodes accepted
              | exact validate_qualified_conditional_with_fallback_sound fuel blockIH branchesIH names scope
                  solution requirements fields binderTypes result branches _ rest _ _ nodes accepted
              | cases accepted
          | _ => cases node <;> cases accepted
    · intro names scope solution requirements fields binderTypes result source candidate accepted
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
              exact validate_qualified_statement_branch_sound fuel blockIH branchesIH names scope solution requirements fields
                binderTypes result condition body rest guard nodes candidates accepted


theorem validated_qualified_block_instantiates (fuel : Nat) (names : TypeNames) (scope : Scope)
    (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (binderTypes : BinderTypes) (result : MonoType) (source : List Statement) (candidate : List StatementNode)
    (accepted : validateQualifiedBlock fuel names scope principal requirements fields binderTypes result source candidate = .ok ())
    (sorted : WellSorted later) (solves : Solves requirements later) :
    BlockHasType names (compose later principal) scope (substitute later result) source :=
  qualified_block_instantiates names principal later requirements scope result source
    ((validate_qualified_blocks_sound fuel).1 names scope principal requirements fields binderTypes result source candidate accepted)
    sorted solves

end Xsh.Typing.Connected
