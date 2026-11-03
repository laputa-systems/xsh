import Xsh.Conformance.Connected.SparseFacts
import Xsh.Typing.Connected.QualifiedFunction

namespace Xsh.Conformance.Connected

open Xsh.Typing.Connected

def qualifiedExpressionFactChecks (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (pairs : List PairedExpression)
    (fact : Nat × SourceSite × MonoType) : Bool :=
  match pairs.find? (fun pair => decide (pair.1.1 = fact.2.1)) with
  | none => false
  | some ((_, expression), node) =>
    match validateQualifiedExpression fuel scope principal requirements fields expression node with
    | .error _ => false
    | .ok actual => decide (actual = fact.2.2)

def checksQualifiedExpressionFacts (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : SourceFunction)
    (candidate : List StatementNode) (body : BodyDocument) : Bool :=
  match pairedExpressions source candidate with
  | none => false
  | some pairs => body.expressionTypes.all (qualifiedExpressionFactChecks fuel scope principal requirements fields pairs)

theorem checked_qualified_fact_types_every_solution (fuel : Nat) (scope : Scope)
    (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : SourceFunction) (candidate : List StatementNode) (body : BodyDocument)
    (checked : checksQualifiedExpressionFacts fuel scope principal requirements fields source candidate body = true)
    (fact : Nat × SourceSite × MonoType) (member : fact ∈ body.expressionTypes)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ∃ pairs expression node,
      pairedExpressions source candidate = some pairs ∧
      pairs.find? (fun pair => decide (pair.1.1 = fact.2.1)) = some ((fact.2.1, expression), node) ∧
      ExpressionHasType scope (compose later principal) expression (substitute later fact.2.2) := by
  unfold checksQualifiedExpressionFacts at checked
  cases paired : pairedExpressions source candidate with
  | none => simp [paired] at checked
  | some pairs =>
    simp only [paired] at checked
    have factChecked := (List.all_eq_true.mp checked) fact member
    unfold qualifiedExpressionFactChecks at factChecked
    cases found : pairs.find? (fun pair => decide (pair.1.1 = fact.2.1)) with
    | none => simp [found] at factChecked
    | some pair =>
      obtain ⟨⟨site, expression⟩, node⟩ := pair
      have siteMatches : site = fact.2.1 := by
        have foundSite := List.find?_some found
        simpa using foundSite
      subst site
      cases result : validateQualifiedExpression fuel scope principal requirements fields expression node with
      | error error => simp [found, result] at factChecked
      | ok actual =>
        simp [found, result] at factChecked
        subst actual
        exact ⟨pairs, expression, node, rfl, found,
          validated_qualified_expression_instantiates fuel scope principal later requirements fields
            expression node fact.2.2 result sorted solves⟩

end Xsh.Conformance.Connected
