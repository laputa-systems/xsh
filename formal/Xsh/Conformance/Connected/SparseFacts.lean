import Xsh.Conformance.Connected.SourceAdmission

namespace Xsh.Conformance.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Typing.Connected

mutual
  def nodeOccurrences : Node → List Node
    | node@(.integer _) | node@(.boolean _) | node@(.string _) | node@(.read _ _) => [node]
    | node@(.binary _ left right) => node :: (nodeOccurrences left ++ nodeOccurrences right)
    | node@(.call target arguments) => node :: (nodeOccurrences target ++ argumentOccurrences arguments)
    | node@(.record fields) => node :: recordOccurrences fields
    | node@(.field base _) => node :: nodeOccurrences base
    | node@(.conditional branches otherwise) => node :: (branchOccurrences branches ++ nodeOccurrences otherwise)
    | node@(.propagate value _) | node@(.requireValue _ _ value) => node :: nodeOccurrences value
    | node@(.valueBlock _ body) => node :: statementOccurrences body

  def argumentOccurrences : List Node → List Node
    | [] => []
    | node :: rest => nodeOccurrences node ++ argumentOccurrences rest

  def recordOccurrences : List (NameId × Node) → List Node
    | [] => []
    | (_, node) :: rest => nodeOccurrences node ++ recordOccurrences rest

  def branchOccurrences : List (Node × Node) → List Node
    | [] => []
    | (condition, value) :: rest => nodeOccurrences condition ++ nodeOccurrences value ++ branchOccurrences rest

  def statementOccurrences : List StatementNode → List Node
    | [] => []
    | .binding _ _ value :: rest | .assignment _ value :: rest |
      .evaluated value :: rest | .returned value :: rest => nodeOccurrences value ++ statementOccurrences rest
    | .conditional branches fallback :: rest =>
      statementBranchOccurrences branches ++ statementOccurrences fallback ++ statementOccurrences rest

  def statementBranchOccurrences : List (Node × List StatementNode) → List Node
    | [] => []
    | (condition, body) :: rest =>
      nodeOccurrences condition ++ statementOccurrences body ++ statementBranchOccurrences rest
end

abbrev PairedExpression := (SourceSite × Expr) × Node

def sourceTailOccurrences (source : SourceFunction) : List (SourceSite × Expr) :=
  source.statements.filterMap fun (site, statement) => match statement with
    | .tailName name => some (site, .ident name)
    | _ => none

def insertSourceOccurrence (item : SourceSite × Expr) : List (SourceSite × Expr) → List (SourceSite × Expr)
  | [] => [item]
  | head :: rest =>
    if item.1.start < head.1.start ||
        (item.1.start = head.1.start && item.1.length ≥ head.1.length) then item :: head :: rest
    else head :: insertSourceOccurrence item rest

def sourceOccurrences (source : SourceFunction) : List (SourceSite × Expr) :=
  (sourceTailOccurrences source).foldr insertSourceOccurrence source.expressions

def pairedExpressions (source : SourceFunction) (candidate : List StatementNode) : Option (List PairedExpression) :=
  let nodes := statementOccurrences candidate
  let expressions := sourceOccurrences source
  if expressions.length = nodes.length then some (expressions.zip nodes) else none

def expressionFactChecks (fuel : Nat) (scope : Scope) (solution instantiation : Substitution)
    (pairs : List PairedExpression) (fact : Nat × SourceSite × MonoType) : Bool :=
  match pairs.find? (fun pair => decide (pair.1.1 = fact.2.1)) with
  | none => false
  | some ((_, expression), node) =>
    match validateExpression fuel scope solution expression node with
    | .error _ => false
    | .ok actual => decide (actual = substitute instantiation fact.2.2)

def checksExpressionFacts (fuel : Nat) (scope : Scope) (solution instantiation : Substitution)
    (source : SourceFunction) (candidate : List StatementNode) (body : BodyDocument) : Bool :=
  match pairedExpressions source candidate with
  | none => false
  | some pairs =>
    decide ((source.expressions.map Prod.fst).Nodup) &&
    decide ((body.expressionTypes.map Prod.fst).Nodup) &&
    decide ((body.expressionTypes.map (fun fact => fact.2.1)).Nodup) &&
    decide ((body.expressionTypes.map (fun fact => fact.2.1)).Perm (source.expressions.map Prod.fst)) &&
    body.expressionTypes.all (expressionFactChecks fuel scope solution instantiation pairs)

theorem checked_expression_fact_matches (fuel : Nat) (scope : Scope) (solution instantiation : Substitution)
    (source : SourceFunction) (candidate : List StatementNode) (body : BodyDocument)
    (checked : checksExpressionFacts fuel scope solution instantiation source candidate body = true)
    (fact : Nat × SourceSite × MonoType) (member : fact ∈ body.expressionTypes) :
    ∃ pairs, pairedExpressions source candidate = some pairs ∧
      expressionFactChecks fuel scope solution instantiation pairs fact = true := by
  unfold checksExpressionFacts at checked
  cases pairs : pairedExpressions source candidate with
  | none => simp [pairs] at checked
  | some values =>
    simp only [pairs] at checked
    simp only [Bool.and_eq_true] at checked
    have all := checked.2
    exact ⟨values, rfl, (List.all_eq_true.mp all) fact member⟩

theorem checked_expression_fact_has_source_type (fuel : Nat) (scope : Scope)
    (solution instantiation : Substitution) (source : SourceFunction) (candidate : List StatementNode)
    (body : BodyDocument)
    (checked : checksExpressionFacts fuel scope solution instantiation source candidate body = true)
    (fact : Nat × SourceSite × MonoType) (member : fact ∈ body.expressionTypes) :
    ∃ pairs expression node actual,
      pairedExpressions source candidate = some pairs ∧
      pairs.find? (fun pair => decide (pair.1.1 = fact.2.1)) = some ((fact.2.1, expression), node) ∧
      validateExpression fuel scope solution expression node = .ok actual ∧
      actual = substitute instantiation fact.2.2 ∧ ExpressionHasType scope solution expression actual := by
  obtain ⟨pairs, paired, factChecked⟩ :=
    checked_expression_fact_matches fuel scope solution instantiation source candidate body checked fact member
  unfold expressionFactChecks at factChecked
  cases found : pairs.find? (fun pair => decide (pair.1.1 = fact.2.1)) with
  | none => simp [found] at factChecked
  | some pair =>
    obtain ⟨⟨site, expression⟩, node⟩ := pair
    have siteMatches : site = fact.2.1 := by
      have foundSite := List.find?_some found
      simpa using foundSite
    subst site
    cases result : validateExpression fuel scope solution expression node with
    | error error => simp [found, result] at factChecked
    | ok actual =>
      simp [found, result] at factChecked
      exact ⟨pairs, expression, node, actual, paired, found, result, factChecked,
        accepted_expression_has_type fuel scope solution expression node actual result⟩

def lookupExpressionFact (body : BodyDocument) (id : Nat) : Option (Nat × SourceSite × MonoType) :=
  body.expressionTypes.find? fun fact => decide (fact.1 = id)

def operationSiteChecks (body : BodyDocument) (entry : Nat × Nat) : Bool :=
  match lookupExpressionFact body entry.1, body.operations[entry.2]? with
  | some fact, some operation => decide (fact.2.1 = operation.site)
  | _, _ => false

def checksOperationSites (body : BodyDocument) : Bool :=
  decide ((body.operationSites.map Prod.fst).Nodup) &&
    decide ((body.operationSites.map Prod.snd).Perm (List.range body.operations.length)) &&
    body.operationSites.all (operationSiteChecks body)

def checksTailFacts (fuel : Nat) (scope : Scope) (solution instantiation : Substitution)
    (source : SourceFunction) (candidate : List StatementNode) (body : BodyDocument) : Bool :=
  match pairedExpressions source candidate with
  | none => false
  | some pairs =>
    decide ((body.tailTypes.map Prod.fst).Nodup) &&
    decide ((body.tailTypes.map (fun fact => fact.2.1)).Nodup) &&
    decide ((body.tailTypes.map (fun fact => fact.2.1)).Perm ((sourceTailOccurrences source).map Prod.fst)) &&
    body.tailTypes.all (expressionFactChecks fuel scope solution instantiation pairs)

def lookupTailFact (body : BodyDocument) (id : Nat) : Option (Nat × SourceSite × MonoType) :=
  body.tailTypes.find? fun fact => decide (fact.1 = id)

def resolvedTailSite (body : BodyDocument) (entry : Nat × ReadBinding) : Option SourceSite :=
  (lookupTailFact body entry.1).map fun fact => fact.2.1

def tailBindingChecks (names : List String) (source : SourceFunction) (body : BodyDocument)
    (entry : Nat × ReadBinding) : Bool :=
  match lookupTailFact body entry.1 with
  | none => false
  | some fact =>
    match source.statements.find? (fun item => decide (item.1 = fact.2.1)) with
    | some (_, .tailName name) =>
      match source.parameters.find? (fun parameter => decide (parameter.name = name)) with
      | none => false
      | some parameter =>
        decide (sourceName names entry.2.name = some name) && decide (entry.2.site = parameter.site) &&
          decide (entry.2.moduleName = none) && !entry.2.global && !entry.2.exported
    | _ => false

def sourceReadSites (source : SourceFunction) : List SourceSite :=
  source.expressions.filterMap fun (site, expression) => match expression with
    | .ident _ => some site
    | _ => none

def resolvedReadSite (body : BodyDocument) (entry : Nat × ReadBinding) : Option SourceSite :=
  (lookupExpressionFact body entry.1).map fun fact => fact.2.1

def readBindingChecks (names : List String) (source : SourceFunction) (body : BodyDocument)
    (entry : Nat × ReadBinding) : Bool :=
  match lookupExpressionFact body entry.1 with
  | none => false
  | some fact =>
    match source.expressions.find? (fun item => decide (item.1 = fact.2.1)) with
    | some (_, .ident name) =>
      match source.parameters.find? (fun parameter => decide (parameter.name = name)) with
      | none => false
      | some parameter =>
        decide (sourceName names entry.2.name = some name) && decide (entry.2.site = parameter.site) &&
          decide (entry.2.moduleName = none) && !entry.2.global && !entry.2.exported
    | _ => false

def checksReadFacts (names : List String) (source : SourceFunction) (body : BodyDocument) : Bool :=
  let resolvedSites := body.resolvedReads.filterMap (resolvedReadSite body)
  let resolvedTailSites := body.resolvedTails.filterMap (resolvedTailSite body)
  let readBindings := (body.resolvedReads.map Prod.snd ++ body.resolvedTails.map Prod.snd).eraseDups
  decide ((body.resolvedReads.map Prod.fst).Nodup) && decide ((body.resolvedTails.map Prod.fst).Nodup) &&
    decide (resolvedSites.length = body.resolvedReads.length) &&
    decide (resolvedTailSites.length = body.resolvedTails.length) &&
    decide (resolvedSites.Perm (sourceReadSites source)) &&
    decide (resolvedTailSites.Perm ((sourceTailOccurrences source).map Prod.fst)) &&
    decide (body.readBindings.Nodup) && decide (body.readBindings.Perm readBindings) &&
    body.resolvedReads.all (readBindingChecks names source body) &&
    body.resolvedTails.all (tailBindingChecks names source body)

def checksSparseFacts (fuel : Nat) (names : List String) (scope : Scope) (solution instantiation : Substitution)
    (source : SourceFunction) (candidate : List StatementNode) (body : BodyDocument) : Bool :=
  checksExpressionFacts fuel scope solution instantiation source candidate body &&
    checksTailFacts fuel scope solution instantiation source candidate body &&
    checksOperationSites body && checksReadFacts names source body

def validateExpressionFact (fuel : Nat) (scope : Scope) (solution instantiation : Substitution)
    (pairs : List PairedExpression) (fact : Nat × SourceSite × MonoType) : Except ValidationError Unit := do
  let some ((_, expression), node) := pairs.find? (fun pair => decide (pair.1.1 = fact.2.1))
    | throw .mismatch
  let actual ← validateExpression fuel scope solution expression node
  requireCheck (decide (actual = substitute instantiation fact.2.2))

theorem expression_fact_check_iff (fuel : Nat) (scope : Scope) (solution instantiation : Substitution)
    (pairs : List PairedExpression) (fact : Nat × SourceSite × MonoType) :
    expressionFactChecks fuel scope solution instantiation pairs fact = true ↔
      validateExpressionFact fuel scope solution instantiation pairs fact = .ok () := by
  unfold expressionFactChecks validateExpressionFact
  cases found : pairs.find? (fun pair => decide (pair.1.1 = fact.2.1)) with
  | none => simp [found]
  | some pair =>
    obtain ⟨⟨site, expression⟩, node⟩ := pair
    cases result : validateExpression fuel scope solution expression node with
    | error error => simp [found, result, except_bind_ok]
    | ok actual => simp [found, result, except_bind_ok, exists_require_ok, requireCheck]

def validateExpressionFactList (fuel : Nat) (scope : Scope) (solution instantiation : Substitution)
    (pairs : List PairedExpression) : List (Nat × SourceSite × MonoType) → Except ValidationError Unit
  | [] => .ok ()
  | fact :: rest => do
    validateExpressionFact fuel scope solution instantiation pairs fact
    validateExpressionFactList fuel scope solution instantiation pairs rest

theorem expression_fact_list_check_iff (fuel : Nat) (scope : Scope) (solution instantiation : Substitution)
    (pairs : List PairedExpression) (facts : List (Nat × SourceSite × MonoType)) :
    facts.all (expressionFactChecks fuel scope solution instantiation pairs) = true ↔
      validateExpressionFactList fuel scope solution instantiation pairs facts = .ok () := by
  induction facts with
  | nil => simp [validateExpressionFactList]
  | cons fact rest ih =>
    simp only [List.all_cons, Bool.and_eq_true, validateExpressionFactList, except_bind_ok]
    constructor
    · rintro ⟨first, remaining⟩
      exact ⟨(), (expression_fact_check_iff fuel scope solution instantiation pairs fact).mp first,
        ih.mp remaining⟩
    · rintro ⟨value, first, remaining⟩
      cases value
      exact ⟨(expression_fact_check_iff fuel scope solution instantiation pairs fact).mpr first,
        ih.mpr remaining⟩

def parameterReadsSupported (source : SourceFunction) : Bool :=
  (sourceOccurrences source).all fun (_, expression) => match expression with
    | .ident name => source.parameters.any fun parameter => decide (parameter.name = name)
    | _ => true

def validateSparseFacts (fuel : Nat) (names : List String) (scope : Scope)
    (solution instantiation : Substitution) (source : SourceFunction)
    (candidate : List StatementNode) (body : BodyDocument) : Except ValidationError Unit := do
  if parameterReadsSupported source then pure () else throw .unsupported
  let some pairs := pairedExpressions source candidate | throw .mismatch
  requireCheck (decide ((source.expressions.map Prod.fst).Nodup) &&
    decide ((body.expressionTypes.map Prod.fst).Nodup) &&
    decide ((body.expressionTypes.map (fun fact => fact.2.1)).Nodup) &&
    decide ((body.expressionTypes.map (fun fact => fact.2.1)).Perm (source.expressions.map Prod.fst)))
  validateExpressionFactList fuel scope solution instantiation pairs body.expressionTypes
  requireCheck (decide ((body.tailTypes.map Prod.fst).Nodup) &&
    decide ((body.tailTypes.map (fun fact => fact.2.1)).Nodup) &&
    decide ((body.tailTypes.map (fun fact => fact.2.1)).Perm ((sourceTailOccurrences source).map Prod.fst)))
  validateExpressionFactList fuel scope solution instantiation pairs body.tailTypes
  requireCheck (checksOperationSites body && checksReadFacts names source body)

theorem validated_sparse_facts_are_checked (fuel : Nat) (names : List String) (scope : Scope)
    (solution instantiation : Substitution) (source : SourceFunction)
    (candidate : List StatementNode) (body : BodyDocument)
    (accepted : validateSparseFacts fuel names scope solution instantiation source candidate body = .ok ()) :
    checksSparseFacts fuel names scope solution instantiation source candidate body = true := by
  unfold validateSparseFacts at accepted
  cases supported : parameterReadsSupported source with
  | false => simp [supported, except_bind_ok] at accepted
  | true =>
    cases found : pairedExpressions source candidate with
    | none => simp [supported, found, except_bind_ok] at accepted
    | some pairs =>
      simp [supported, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
      obtain ⟨value, listChecked⟩ := accepted.2.1
      cases value
      have all := (expression_fact_list_check_iff fuel scope solution instantiation pairs body.expressionTypes).mpr listChecked
      obtain ⟨tailValue, tailChecked⟩ := accepted.2.2.2.1
      cases tailValue
      have tailAll := (expression_fact_list_check_iff fuel scope solution instantiation pairs body.tailTypes).mpr tailChecked
      have checks : checksOperationSites body = true ∧ checksReadFacts names source body = true := by
        simpa [requireCheck, Bool.and_eq_true] using accepted.2.2.2.2
      simp [checksSparseFacts, checksExpressionFacts, checksTailFacts, found, Bool.and_eq_true, List.all_eq_true] at *
      exact ⟨⟨⟨⟨accepted.1, all⟩, ⟨accepted.2.2.1, tailAll⟩⟩, checks.1⟩, checks.2⟩

end Xsh.Conformance.Connected
