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

def checksQualifiedFacts (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : SourceFunction)
    (candidate : List StatementNode) (facts : List (Nat × SourceSite × MonoType)) : Bool :=
  match pairedExpressions source candidate with
  | none => false
  | some pairs => facts.all (qualifiedExpressionFactChecks fuel scope principal requirements fields pairs)

def checksQualifiedExpressionFacts (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : SourceFunction)
    (candidate : List StatementNode) (body : BodyDocument) : Bool :=
  checksQualifiedFacts fuel scope principal requirements fields source candidate body.expressionTypes

def checksQualifiedTailFacts (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : SourceFunction)
    (candidate : List StatementNode) (body : BodyDocument) : Bool :=
  checksQualifiedFacts fuel scope principal requirements fields source candidate body.tailTypes

theorem checked_qualified_fact_types_every_solution (fuel : Nat) (scope : Scope)
    (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : SourceFunction) (candidate : List StatementNode) (facts : List (Nat × SourceSite × MonoType))
    (checked : checksQualifiedFacts fuel scope principal requirements fields source candidate facts = true)
    (fact : Nat × SourceSite × MonoType) (member : fact ∈ facts)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ∃ pairs expression node,
      pairedExpressions source candidate = some pairs ∧
      pairs.find? (fun pair => decide (pair.1.1 = fact.2.1)) = some ((fact.2.1, expression), node) ∧
      ExpressionHasType scope (compose later principal) expression (substitute later fact.2.2) := by
  unfold checksQualifiedFacts at checked
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

def identityTailFactCheck (tailType : MonoType) : Bool :=
  match admitSource 0 "pure identity(value) { value }\n" with
  | .ok unit =>
    match unit.declarations with
    | [.function source] =>
      match source.parameters with
      | [parameter] =>
        let binder : Xsh.Foundation.BinderId := ⟨⟨0, by decide⟩⟩
        let binding : Binding := ⟨parameter.name, binder, .immutable, ⟨[], .variable ⟨0, .value⟩, []⟩⟩
        checksQualifiedFacts 24 [binding] identity [] (fun _ => none) source
          [.evaluated (.read binder [])] [(0, ⟨0, 23, 5⟩, tailType)]
      | _ => false
    | _ => false
  | .error _ => false

theorem symbolic_tail_facts_reject_a_concrete_collapse :
    identityTailFactCheck (.variable ⟨0, .value⟩) = true ∧
    identityTailFactCheck (.primitive .integer) = false := by
  decide +kernel

end Xsh.Conformance.Connected
