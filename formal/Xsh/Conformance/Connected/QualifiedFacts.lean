import Xsh.Conformance.Connected.SparseFacts
import Xsh.Typing.Connected.QualifiedFunction

namespace Xsh.Conformance.Connected

open Xsh.Typing.Connected
open Xsh.Conformance.Transport

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

def sourceFieldSites (nodes : List (SourceSite × Xsh.Syntax.Expr)) :
    List (SourceSite × Xsh.Foundation.NameId) :=
  nodes.filterMap fun (site, expression) => match expression with
    | .field _ name => some (site, name)
    | _ => none

def validateSourceAnnotation (names : TypeNames) (principal : Substitution)
    (source : Option Xsh.Syntax.TypeExpr) (ty : MonoType) : Except ValidationError Unit := do
  match source with
  | none => pure ()
  | some annotationSource =>
    let some annotated := annotation names annotationSource | throw .unsupported
    requireCheck (decide (substitute principal annotated = ty))

def validateSourceParameters (names : TypeNames) (principal : Substitution)
    (source : List SourceParameter) (parameters : List Binding) : Except ValidationError Unit := do
  if source.length = parameters.length then pure () else throw .mismatch
  for (parameter, binding) in source.zip parameters do
    requireCheck (decide (parameter.name = binding.name))
    validateSourceAnnotation names principal parameter.annotation (substitute principal binding.scheme.body)

def validateQualifiedBodyFactsFor (fuel : Nat) (unit : SourceUnit) (source : SourceFunction)
    (variables : VariableResolver) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (body : BodyDocument) : Except ValidationError Substitution := do
  requireCheck (decide (sourceName unit.names "Int" = some names.integer) &&
    decide (sourceName unit.names "Bool" = some names.boolean) &&
    decide (sourceName unit.names "Str" = some names.string) &&
    decide (sourceName unit.names "Result" = some names.result))
  requireCheck (decide (body.site = source.bodySite) && decide (body.kind = .pure) && decide (source.kind = .pure) &&
    decide (body.explicitReturn = source.result.isSome))
  let some parameterNames := body.parameters.mapM (fun (name, _) => sourceNameResolver unit body.site.source name)
    | throw .mismatch
  requireCheck (decide (parameterNames = parameters.map Binding.name) &&
    decide ((body.operations.map fun operation => (operation.site, operation.name)).Perm
      (sourceFieldSites source.expressions)))
  let some scheme := schemeFromBody variables body | throw .mismatch
  let principal ← validateQualifiedFunction fuel names parameters globals result source.body ordered steps
    solvedCount scheme candidate binderTypes fields inhabitant
  validateSourceParameters names principal source.parameters parameters
  validateSourceAnnotation names principal source.result (substitute principal result)
  requireCheck (checksQualifiedExpressionFacts fuel (parameters ++ globals) principal scheme.requirements
    fields source candidate body && checksQualifiedTailFacts fuel (parameters ++ globals) principal
      scheme.requirements fields source candidate body)
  validateSparseFacts fuel unit.names (parameters ++ globals) (compose (witnessSubstitution inhabitant) principal)
    (witnessSubstitution inhabitant) source candidate body
  pure principal

theorem validated_qualified_body_has_function_certificate (fuel : Nat) (unit : SourceUnit)
    (source : SourceFunction) (variables : VariableResolver) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType))
    (body : BodyDocument) (principal : Substitution)
    (accepted : validateQualifiedBodyFactsFor fuel unit source variables names parameters globals result ordered
      steps solvedCount candidate binderTypes fields inhabitant body = .ok principal) :
    ∃ scheme, schemeFromBody variables body = some scheme ∧
      validateQualifiedFunction fuel names parameters globals result source.body ordered steps solvedCount
        scheme candidate binderTypes fields inhabitant = .ok principal := by
  unfold validateQualifiedBodyFactsFor at accepted
  cases parameterNames : body.parameters.mapM (fun (name, _) => sourceNameResolver unit body.site.source name) with
  | none => simp [parameterNames, except_bind_ok, exists_require_ok] at accepted
  | some decodedNames =>
    cases foundScheme : schemeFromBody variables body with
    | none => simp [parameterNames, foundScheme, except_bind_ok, exists_require_ok] at accepted
    | some scheme =>
      simp [parameterNames, foundScheme, except_bind_ok, except_map_ok, exists_require_ok] at accepted
      obtain ⟨_, _, _, selected, certified, _, _, _, _, same⟩ := accepted
      subst selected
      exact ⟨scheme, rfl, certified⟩

theorem validated_qualified_body_types_every_solution (fuel : Nat) (unit : SourceUnit)
    (source : SourceFunction) (variables : VariableResolver) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType))
    (body : BodyDocument) (principal : Substitution)
    (accepted : validateQualifiedBodyFactsFor fuel unit source variables names parameters globals result ordered
      steps solvedCount candidate binderTypes fields inhabitant body = .ok principal) :
    ∃ scheme, schemeFromBody variables body = some scheme ∧
      ∀ later, WellSorted later → Solves scheme.requirements later →
        ∃ generated state,
          (block fuel names (parameters ++ globals) result source.body).run
            (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
          QualifiedScheme .generalize (globalEnvironment globals)
            (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
          BlockHasType names (compose later principal) (parameters ++ globals)
            (substitute (compose later principal) result) source.body := by
  obtain ⟨scheme, decoded, certified⟩ := validated_qualified_body_has_function_certificate
    fuel unit source variables names parameters globals result ordered steps solvedCount candidate binderTypes
    fields inhabitant body principal accepted
  refine ⟨scheme, decoded, ?_⟩
  intro later sorted solves
  obtain ⟨generated, state, generatedBody, qualified, _, _, _, _, _, _, typed⟩ :=
    qualified_function_types_every_solution fuel names parameters globals result source.body ordered steps
      solvedCount scheme candidate binderTypes fields principal later inhabitant certified sorted solves
  exact ⟨generated, state, generatedBody, qualified, typed⟩

def validateQualifiedBodyFacts (fuel : Nat) (unit : SourceUnit) (declaration : Nat)
    (variables : VariableResolver) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (body : BodyDocument) : Except ValidationError Substitution := do
  let some (.function source) := unit.declarations[declaration]? | throw .unsupported
  validateQualifiedBodyFactsFor fuel unit source variables names parameters globals result ordered steps
    solvedCount candidate binderTypes fields inhabitant body

def validateSourceQualifiedBodyFacts (fuel : Nat) (sourceId : Nat) (text : String) (declaration : Nat)
    (variables : VariableResolver) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (body : BodyDocument) : Except ValidationError Substitution := do
  let unit ← match admitSource sourceId text with
    | .ok unit => pure unit
    | .error .exhausted => throw .exhausted
    | .error .unsupported => throw .unsupported
    | .error .malformed => throw .mismatch
  validateQualifiedBodyFacts fuel unit declaration variables names parameters globals result ordered steps
    solvedCount candidate binderTypes fields inhabitant body

theorem validated_unit_body_belongs_to_unit (fuel : Nat) (unit : SourceUnit) (declaration : Nat)
    (variables : VariableResolver) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (body : BodyDocument) (principal : Substitution)
    (accepted : validateQualifiedBodyFacts fuel unit declaration variables names parameters globals result ordered
      steps solvedCount candidate binderTypes fields inhabitant body = .ok principal) :
    ∃ source, unit.declarations[declaration]? = some (.function source) ∧
      validateQualifiedBodyFactsFor fuel unit source variables names parameters globals result ordered steps
        solvedCount candidate binderTypes fields inhabitant body = .ok principal := by
  unfold validateQualifiedBodyFacts at accepted
  cases found : unit.declarations[declaration]? with
  | none => simp [found] at accepted
  | some entry =>
    cases entry with
    | statement _ _ _ => simp [found] at accepted
    | function source =>
      simp [found] at accepted
      exact ⟨source, rfl, accepted⟩

theorem validated_source_body_belongs_to_admitted_source (fuel : Nat) (sourceId : Nat) (text : String)
    (declaration : Nat) (variables : VariableResolver) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (body : BodyDocument) (principal : Substitution)
    (accepted : validateSourceQualifiedBodyFacts fuel sourceId text declaration variables names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant body = .ok principal) :
    ∃ unit source, admitSource sourceId text = .ok unit ∧ unit.declarations[declaration]? = some (.function source) ∧
      validateQualifiedBodyFactsFor fuel unit source variables names parameters globals result ordered steps
        solvedCount candidate binderTypes fields inhabitant body = .ok principal := by
  unfold validateSourceQualifiedBodyFacts at accepted
  cases admitted : admitSource sourceId text with
  | error failure =>
    rw [admitted] at accepted
    cases failure <;> cases accepted
  | ok unit =>
    simp [admitted] at accepted
    obtain ⟨source, member, checked⟩ := validated_unit_body_belongs_to_unit fuel unit declaration variables names
      parameters globals result ordered steps solvedCount candidate binderTypes fields inhabitant body principal accepted
    exact ⟨unit, source, rfl, member, checked⟩

theorem validated_source_body_types_every_solution (fuel : Nat) (sourceId : Nat) (text : String)
    (declaration : Nat) (variables : VariableResolver) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (body : BodyDocument) (principal : Substitution)
    (accepted : validateSourceQualifiedBodyFacts fuel sourceId text declaration variables names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant body = .ok principal) :
    ∃ unit source scheme, admitSource sourceId text = .ok unit ∧
      unit.declarations[declaration]? = some (.function source) ∧ schemeFromBody variables body = some scheme ∧
      ∀ later, WellSorted later → Solves scheme.requirements later →
        ∃ generated state,
          (block fuel names (parameters ++ globals) result source.body).run
            (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
          QualifiedScheme .generalize (globalEnvironment globals)
            (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
          BlockHasType names (compose later principal) (parameters ++ globals)
            (substitute (compose later principal) result) source.body := by
  obtain ⟨unit, source, admitted, member, certified⟩ := validated_source_body_belongs_to_admitted_source
    fuel sourceId text declaration variables names parameters globals result ordered steps solvedCount candidate
    binderTypes fields inhabitant body principal accepted
  obtain ⟨scheme, decoded, typed⟩ := validated_qualified_body_types_every_solution fuel unit source variables
    names parameters globals result ordered steps solvedCount candidate binderTypes fields inhabitant body principal certified
  exact ⟨unit, source, scheme, admitted, member, decoded, typed⟩

def validateSourceFactsBody (fuel : Nat) (sourceId : Nat) (text : String) (factsCodes : ExchangeCodes)
    (declaration bodyIndex : Nat) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) : Except ValidationError Substitution := do
  let unit ← match admitSource sourceId text with
    | .ok unit => pure unit
    | .error .exhausted => throw .exhausted
    | .error .unsupported => throw .unsupported
    | .error .malformed => throw .mismatch
  let some (facts, scopes) := decodeSelfScopedFactsDocument fuel (sourceNameResolver unit) factsCodes
    | throw .mismatch
  let some body := facts.bodies[bodyIndex]? | throw .mismatch
  validateQualifiedBodyFacts fuel unit declaration (scopedVariableResolver scopes) names parameters globals result
    ordered steps solvedCount candidate binderTypes fields inhabitant body

theorem validated_source_facts_body_types_every_solution (fuel : Nat) (sourceId : Nat) (text : String)
    (factsCodes : ExchangeCodes) (declaration bodyIndex : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType))
    (principal : Substitution)
    (accepted : validateSourceFactsBody fuel sourceId text factsCodes declaration bodyIndex names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant = .ok principal) :
    ∃ unit facts scopes source body scheme, admitSource sourceId text = .ok unit ∧
      decodeSelfScopedFactsDocument fuel (sourceNameResolver unit) factsCodes = some (facts, scopes) ∧
      facts.bodies[bodyIndex]? = some body ∧ unit.declarations[declaration]? = some (.function source) ∧
      schemeFromBody (scopedVariableResolver scopes) body = some scheme ∧
      ∀ later, WellSorted later → Solves scheme.requirements later →
        ∃ generated state,
          (block fuel names (parameters ++ globals) result source.body).run
            (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
          QualifiedScheme .generalize (globalEnvironment globals)
            (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
          BlockHasType names (compose later principal) (parameters ++ globals)
            (substitute (compose later principal) result) source.body := by
  unfold validateSourceFactsBody at accepted
  cases admitted : admitSource sourceId text with
  | error failure =>
    rw [admitted] at accepted
    cases failure <;> cases accepted
  | ok unit =>
    simp only [admitted] at accepted
    cases decoded : decodeSelfScopedFactsDocument fuel (sourceNameResolver unit) factsCodes with
    | none => simp [decoded] at accepted
    | some value =>
      obtain ⟨facts, scopes⟩ := value
      cases selected : facts.bodies[bodyIndex]? with
      | none => simp [decoded, selected] at accepted
      | some body =>
        simp [decoded, selected] at accepted
        obtain ⟨source, member, bodyChecked⟩ := validated_unit_body_belongs_to_unit fuel unit declaration
          (scopedVariableResolver scopes) names parameters globals result ordered steps solvedCount candidate binderTypes
          fields inhabitant body principal accepted
        obtain ⟨scheme, schemeDecoded, typed⟩ := validated_qualified_body_types_every_solution fuel unit source
          (scopedVariableResolver scopes) names parameters globals result ordered steps solvedCount candidate binderTypes
          fields inhabitant body principal bodyChecked
        exact ⟨unit, facts, scopes, source, body, scheme, rfl, decoded, selected, member, schemeDecoded, typed⟩

def genericIdentityBodyAccepted (argument : MonoType) (declaration : Nat) (detached : Bool) : Bool :=
  match admitSource 0 "pure identity(value) { value }\n" with
  | .error _ => false
  | .ok unit =>
    match unit.declarations with
    | [.function source] =>
      match source.parameters, sourceTailOccurrences source,
          sourceName unit.names "Int", sourceName unit.names "Bool",
          sourceName unit.names "Str", sourceName unit.names "Result" with
      | [parameter], [(tailSite, _)], some intName, some boolName, some stringName, some resultName =>
        let typeVariable : Variable := ⟨0, .value⟩
        let binder : Xsh.Foundation.BinderId := ⟨⟨0, by decide⟩⟩
        let binding : Binding := ⟨parameter.name, binder, .immutable, ⟨[], .variable typeVariable, []⟩⟩
        let read : ReadBinding := ⟨"value", parameter.site, none, false, false⟩
        let bodySite := if detached then { source.bodySite with start := source.bodySite.start + 1 } else source.bodySite
        let body : BodyDocument := ⟨bodySite, .pure, [0], [("value", .variable typeVariable)],
          .variable typeVariable, false, .unknown, false, [], [], [(0, tailSite, .variable typeVariable)],
          [read], [], [(0, read)], []⟩
        let variables : VariableResolver := fun _ id => if id = 0 then some typeVariable else none
        let names : TypeNames := ⟨intName, boolName, stringName, resultName⟩
        match validateQualifiedBodyFacts 32 unit declaration variables names [binding] [] (.variable typeVariable)
            [.equal (.variable typeVariable) (.variable typeVariable)] [.reflexive] 1
            [.evaluated (.read binder [])] (fun _ => none) (fun _ => none) [(typeVariable, argument)] body with
        | .ok _ => true | .error _ => false
      | _, _, _, _, _, _ => false
    | _ => false

theorem generic_identity_body_checks_two_instances :
    genericIdentityBodyAccepted (.primitive .integer) 0 false = true ∧
    genericIdentityBodyAccepted (.primitive .boolean) 0 false = true ∧
    genericIdentityBodyAccepted (.primitive .integer) 1 false = false ∧
    genericIdentityBodyAccepted (.primitive .integer) 0 true = false := by
  decide +kernel

def annotatedIdentityBodyAccepted (annotationName : String) (swapped : Bool) : Bool :=
  match admitSource 0 ("pure identity(value: " ++ annotationName ++ ") -> Int { value }\n") with
  | .error _ => false
  | .ok unit =>
    match unit.declarations with
    | [.function source] =>
      match source.parameters, sourceTailOccurrences source,
          sourceName unit.names "Int", sourceName unit.names "Bool",
          sourceName unit.names "Str", sourceName unit.names "Result" with
      | [parameter], [(tailSite, _)], some intName, some boolName, some stringName, some resultName =>
        let binder : Xsh.Foundation.BinderId := ⟨⟨0, by decide⟩⟩
        let observed := if swapped then MonoType.primitive .boolean else .primitive .integer
        let binding : Binding := ⟨parameter.name, binder, .immutable, ⟨[], observed, []⟩⟩
        let read : ReadBinding := ⟨"value", parameter.site, none, false, false⟩
        let body : BodyDocument := ⟨source.bodySite, .pure, [], [("value", observed)],
          observed, true, .unknown, false, [], [], [(0, tailSite, observed)],
          [read], [], [(0, read)], []⟩
        let names : TypeNames := if swapped then ⟨boolName, intName, stringName, resultName⟩
          else ⟨intName, boolName, stringName, resultName⟩
        match validateQualifiedBodyFacts 32 unit 0 (fun _ _ => none) names [binding] [] observed
            [.equal observed observed] [.reflexive] 1
            [.evaluated (.read binder [])] (fun _ => none) (fun _ => none) [] body with
        | .ok _ => true | .error _ => false
      | _, _, _, _, _, _ => false
    | _ => false

theorem admitted_parameter_annotation_is_checked :
    annotatedIdentityBodyAccepted "Int" false = true ∧ annotatedIdentityBodyAccepted "Bool" false = false ∧
    annotatedIdentityBodyAccepted "Int" true = false := by
  decide +kernel

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
