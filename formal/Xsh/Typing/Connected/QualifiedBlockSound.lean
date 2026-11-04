import Xsh.Typing.Connected.QualifiedBlockValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def QualifiedCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution requirements fields binderTypes result completion source candidate,
    validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion source candidate = .ok () →
    QualifiedBlockHasCompletion names solution requirements scope result completion source

def QualifiedBranchCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution requirements fields binderTypes result completion source candidate,
    validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result completion source candidate = .ok () →
    QualifiedBranchesHaveCompletion names solution requirements scope result completion source

theorem qualified_return_completion_sound (fuel : Nat) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletion names solution requirements scope result completion (.returnValue (some value) :: rest) := by
  simp [validateQualifiedBlockCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, expressionAccepted, rfl, remainingAccepted⟩ := accepted
  exact .returned (accepted_qualified_expression_has_type fuel scope solution requirements fields value node _ expressionAccepted)
    (ih names scope solution requirements fields binderTypes _ none rest nodes remainingAccepted)

theorem qualified_assignment_completion_sound (fuel : Nat) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (value : Expr)
    (rest : List Statement) (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletion names solution requirements scope result completion (.assign (.name name) .set value :: rest) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedBlockCompletion, found] at accepted
  | some binding =>
    simp [validateQualifiedBlockCompletion, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨_, mutable⟩, _, expressionAccepted, rfl, remainingAccepted⟩ := accepted
    exact .assignment found mutable
      (accepted_qualified_expression_has_type fuel scope solution requirements fields value node _ expressionAccepted)
      (ih names scope solution requirements fields binderTypes result completion rest nodes remainingAccepted)

theorem qualified_binding_completion_sound (fuel : Nat) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (kind : Xsh.Syntax.BindingKind)
    (name : NameId) (declared : Option TypeExpr) (initializer : Expr) (rest : List Statement)
    (binder : BinderId) (recordedKind : BindingKind) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.binding kind (.name name) declared initializer :: rest) (.binding binder recordedKind node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletion names solution requirements scope result completion (.binding kind (.name name) declared initializer :: rest) := by
  cases found : binderTypes binder with
  | none => simp [validateQualifiedBlockCompletion, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateQualifiedBlockCompletion, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, _, expressionAccepted, valueEqual, annotationAccepted, remainingAccepted⟩ := accepted
    subst recordedKind
    apply QualifiedBlockHasCompletion.binding (rawType := rawType)
    · exact valueEqual ▸ accepted_qualified_expression_has_type fuel scope solution requirements fields initializer node _ expressionAccepted
    · exact valueEqual ▸ check_annotation_sound names solution declared _ annotationAccepted
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution requirements fields binderTypes result completion rest nodes remainingAccepted

theorem qualified_tail_name_completion_sound (fuel : Nat)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.tailName name :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletion names solution requirements scope result completion (.tailName name :: rest) := by
  simp [validateQualifiedBlockCompletion, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
  obtain ⟨⟨empty, _⟩, ty, expressionAccepted, completionAccepted⟩ := accepted
  subst rest
  simp [requireCheck] at completionAccepted
  exact .tailName (accepted_qualified_expression_has_type fuel scope solution requirements fields (.ident name) node ty expressionAccepted)
    (check_completion_sound completion ty completionAccepted)

theorem qualified_scoped_completion_sound (fuel : Nat) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (body rest : List Statement)
    (nested nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.expression (.valueBlock body) :: rest) (.scoped nested :: nodes) = .ok ()) :
    QualifiedBlockHasCompletion names solution requirements scope result completion (.expression (.valueBlock body) :: rest) := by
  cases rest with
  | nil =>
    simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨⟨⟨⟩, bodyAccepted⟩, rfl⟩ := accepted
    exact .scopedTail (ih names scope solution requirements fields binderTypes result completion body nested bodyAccepted)
  | cons next rest =>
    simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨⟨⟩, bodyAccepted⟩, restAccepted⟩ := accepted
    exact .scopedNext (ih names scope solution requirements fields binderTypes result none body nested bodyAccepted)
      (ih names scope solution requirements fields binderTypes result completion (next :: rest) nodes restAccepted)

theorem qualified_evaluated_completion_sound (fuel : Nat) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletion names solution requirements scope result completion (.expression value :: rest) := by
  cases rest with
  | nil =>
    simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨ty, expressionAccepted, _, completionAccepted⟩ := accepted
    exact .expressionTail (accepted_qualified_expression_has_type fuel scope solution requirements fields value node ty expressionAccepted)
      (check_completion_sound completion ty completionAccepted)
  | cons next rest =>
    simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨ty, expressionAccepted⟩, restAccepted⟩ := accepted
    exact .expressionNext (accepted_qualified_expression_has_type fuel scope solution requirements fields value node ty expressionAccepted)
      (ih names scope solution requirements fields binderTypes result completion (next :: rest) nodes restAccepted)

theorem qualified_branch_completion_sound (fuel : Nat) (blockIH : QualifiedCompletionSound fuel)
    (branchesIH : QualifiedBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateQualifiedBranchesCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    QualifiedBranchesHaveCompletion names solution requirements scope result completion ((condition, body) :: rest) := by
  simp [validateQualifiedBranchesCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (accepted_qualified_expression_has_type fuel scope solution requirements fields condition guard _ guardAccepted)
    (blockIH names scope solution requirements fields binderTypes result completion body nodes bodyAccepted)
    (branchesIH names scope solution requirements fields binderTypes result completion rest candidates remainingAccepted)

theorem qualified_conditional_completion_sound (fuel : Nat) (blockIH : QualifiedCompletionSound fuel)
    (branchesIH : QualifiedBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (branches : List (Expr × List Statement))
    (otherwise : Option (List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.ifStmt branches otherwise :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    QualifiedBlockHasCompletion names solution requirements scope result completion (.ifStmt branches otherwise :: rest) := by
  cases rest with
  | nil =>
    cases otherwise with
    | none =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨_, fallbackEmpty⟩, completionAccepted, _⟩ := accepted
      exact .conditionalNoFallbackTail
        (branchesIH names scope solution requirements fields binderTypes result completion branches candidates branchAccepted)
        (check_completion_sound completion (.primitive .unit) completionAccepted)
    | some body =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, _⟩ := accepted
      exact .conditionalTail
        (branchesIH names scope solution requirements fields binderTypes result completion branches candidates branchAccepted)
        (blockIH names scope solution requirements fields binderTypes result completion body fallback bodyAccepted)
  | cons next rest =>
    cases otherwise with
    | none =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, _, _, remainingAccepted⟩ := accepted
      exact .conditionalNoFallbackNext
        (branchesIH names scope solution requirements fields binderTypes result none branches candidates branchAccepted)
        (blockIH names scope solution requirements fields binderTypes result completion (next :: rest) nodes remainingAccepted)
    | some body =>
      simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
      exact .conditionalNext
        (branchesIH names scope solution requirements fields binderTypes result none branches candidates branchAccepted)
        (blockIH names scope solution requirements fields binderTypes result none body fallback bodyAccepted)
        (blockIH names scope solution requirements fields binderTypes result completion (next :: rest) nodes remainingAccepted)

theorem validate_qualified_completion_blocks_sound : ∀ fuel, QualifiedCompletionSound fuel ∧ QualifiedBranchCompletionSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [QualifiedCompletionSound, QualifiedBranchCompletionSound, validateQualifiedBlockCompletion, validateQualifiedBranchesCompletion]
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
              | exact qualified_binding_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                  kind _ declared initializer rest _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact qualified_assignment_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact qualified_return_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                  _ rest _ nodes accepted
              | cases accepted
          | tailName name =>
            cases node <;> first
              | exact qualified_tail_name_completion_sound fuel names scope solution requirements fields binderTypes result completion
                  name rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node with
            | evaluated node =>
              exact qualified_evaluated_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                value rest node nodes accepted
            | «scoped» nested =>
              cases value <;> first
                | exact qualified_scoped_completion_sound fuel blockIH names scope solution requirements fields binderTypes result completion
                    _ rest nested nodes accepted
                | cases accepted
            | _ => cases value <;> cases accepted
          | ifStmt branches otherwise =>
            cases node <;> first
              | exact qualified_conditional_completion_sound fuel blockIH branchesIH names scope solution requirements fields binderTypes
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
              exact qualified_branch_completion_sound fuel blockIH branchesIH names scope solution requirements fields binderTypes result
                completion condition body rest guard nodes candidates accepted

def QualifiedBlockSound (fuel : Nat) : Prop :=
  ∀ names scope principal requirements fields binderTypes result source candidate,
    validateQualifiedBlock fuel names scope principal requirements fields binderTypes result source candidate = .ok () →
    QualifiedBlock names principal requirements scope result source

def QualifiedStatementBranchesSound (fuel : Nat) : Prop :=
  ∀ names scope principal requirements fields binderTypes result source candidate,
    validateQualifiedStatementBranches fuel names scope principal requirements fields binderTypes result source candidate = .ok () →
    QualifiedStatementBranches names principal requirements scope result source

theorem validate_qualified_blocks_sound (fuel : Nat) : QualifiedBlockSound fuel ∧ QualifiedStatementBranchesSound fuel := by
  constructor
  · intro names scope principal requirements fields binderTypes result source candidate accepted
    exact (validate_qualified_completion_blocks_sound fuel).1 names scope principal requirements fields binderTypes
      result (some result) source candidate accepted
  · intro names scope principal requirements fields binderTypes result source candidate accepted
    exact (validate_qualified_completion_blocks_sound fuel).2 names scope principal requirements fields binderTypes
      result (some result) source candidate accepted

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
