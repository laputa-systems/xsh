import Xsh.Conformance.Connected.QualifiedFacts

namespace Xsh.Conformance.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Typing.Connected

def sourceFunctionIdentities (unit : SourceUnit) : List (Xsh.Foundation.NameId × SourceSite) :=
  unit.declarations.filterMap fun declaration => match declaration with
    | .function function => some (function.name, function.bodySite)
    | _ => none

def targetSourceIdentity (unit : SourceUnit) (target : FunctionTarget) : Option (Xsh.Foundation.NameId × SourceSite) := do
  if target.moduleName = none ∧ target.body.source = unit.source then pure () else none
  let name ← sourceNameResolver unit target.body.source target.name
  pure (name, target.body)

def checksSourceFunctions (unit : SourceUnit) (functions : List FunctionTarget) : Bool :=
  match functions.mapM (targetSourceIdentity unit) with
  | none => false
  | some identities =>
    decide ((functions.map FunctionTarget.definition).Nodup) && decide identities.Nodup &&
      decide (identities.Perm (sourceFunctionIdentities unit))

mutual
  def literalCandidate : Expr → Option Node
    | .literal (.integer text) => some (.integer text)
    | .literal (.bool value) => some (.boolean value)
    | .literal (.string value) => some (.string value)
    | .record fields => (literalRecordCandidates fields).map Node.record
    | _ => none

  def literalRecordCandidates : List RecordField → Option (List (NameId × Node))
    | [] => some []
    | .named name value :: rest => do
        let first ← literalCandidate value
        let others ← literalRecordCandidates rest
        pure ((name, first) :: others)
    | _ => none
end

def literalCallArguments : List CallArgument → Option (List Node)
  | [] => some []
  | .positional value :: rest => do
      let first ← literalCandidate value
      let others ← literalCallArguments rest
      pure (first :: others)
  | _ => none

def validatePublishedCall (fuel : Nat) (scope : Scope) (solution : Substitution)
    (targetName : NameId) (targetBinder : BinderId) (scheme : Scheme)
    (source : Expr) (arguments : List Node) (instantiation : List (Variable × MonoType)) :
    Except ValidationError MonoType := do
  let .call (.ident name) _ := source | throw .unsupported
  requireCheck (decide (name = targetName))
  let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
  requireCheck (decide (binding.binder = targetBinder) &&
    decide (binding.scheme.quantified = scheme.quantified) &&
    decide (binding.scheme.body = scheme.body) &&
    decide (binding.scheme.requirements = scheme.requirements))
  validateExpression fuel scope solution source (.call (.read targetBinder instantiation) arguments)

theorem validated_published_call_has_source_type (fuel : Nat) (scope : Scope) (solution : Substitution)
    (targetName : NameId) (targetBinder : BinderId) (scheme : Scheme) (source : Expr)
    (arguments : List Node) (instantiation : List (Variable × MonoType)) (ty : MonoType)
    (accepted : validatePublishedCall fuel scope solution targetName targetBinder scheme source arguments
      instantiation = .ok ty) : ExpressionHasType scope solution source ty := by
  cases source with
  | call callee args =>
    cases callee with
    | ident name =>
      cases found : scope.find? (fun item => decide (item.name = name)) with
      | none => simp [validatePublishedCall, found, except_bind_ok, exists_require_ok] at accepted
      | some binding =>
        simp [validatePublishedCall, found, except_bind_ok, exists_require_ok] at accepted
        exact accepted_expression_has_type fuel scope solution _ _ ty accepted.2.2
    | _ => simp [validatePublishedCall] at accepted
  | _ => simp [validatePublishedCall] at accepted

def schemeMatchesBody (variables : VariableResolver) (body : BodyDocument) (scheme : Scheme) : Bool :=
  match schemeFromBody variables body with
  | none => false
  | some published => decide (published.quantified = scheme.quantified ∧ published.body = scheme.body ∧
      published.requirements = scheme.requirements)

def validatePublishedSourceCall (fuel : Nat) (unit : SourceUnit) (caller : SourceFunction)
    (functions : List FunctionTarget) (scope : Scope) (solution : Substitution) (targetName : NameId) (targetBinder : BinderId)
    (scheme : Scheme) (variables : VariableResolver) (body : BodyDocument) (call : CallDocument) :
    Except ValidationError MonoType := do
  requireCheck (checksSourceFunctions unit functions && decide (call.target ∈ functions) &&
    schemeMatchesBody variables body scheme && decide (call.role = .call) && decide (call.target.body = body.site) &&
    decide (call.operationBindings = body.operations.map (fun _ => OperationBinding.field)) &&
    decide (sourceNameResolver unit call.target.body.source call.target.name = some targetName))
  let some (_, source) := caller.expressions.find? (fun (site, _) => decide (site = call.site)) | throw .mismatch
  let .call _ sourceArguments := source | throw .mismatch
  let some arguments := literalCallArguments sourceArguments | throw .unsupported
  let some instantiation := argumentsFromCall variables body call | throw .mismatch
  validatePublishedCall fuel scope solution targetName targetBinder scheme source arguments instantiation

theorem validated_published_source_call_has_type (fuel : Nat) (unit : SourceUnit) (caller : SourceFunction)
    (functions : List FunctionTarget) (scope : Scope) (solution : Substitution) (targetName : NameId) (targetBinder : BinderId)
    (scheme : Scheme) (variables : VariableResolver) (body : BodyDocument) (call : CallDocument) (ty : MonoType)
    (accepted : validatePublishedSourceCall fuel unit caller functions scope solution targetName targetBinder scheme
      variables body call = .ok ty) :
    ∃ source, caller.expressions.find? (fun (site, _) => decide (site = call.site)) = some (call.site, source) ∧
      ExpressionHasType scope solution source ty := by
  unfold validatePublishedSourceCall at accepted
  cases found : caller.expressions.find? (fun (site, _) => decide (site = call.site)) with
  | none => simp [found, except_bind_ok, exists_require_ok] at accepted
  | some pair =>
    obtain ⟨site, source⟩ := pair
    have sameSite : site = call.site := by
      have foundSite := List.find?_some found
      simpa using foundSite
    subst site
    cases source with
    | call callee sourceArguments =>
      cases arguments : literalCallArguments sourceArguments with
      | none => simp [found, arguments, except_bind_ok, exists_require_ok] at accepted
      | some nodes =>
        cases instantiation : argumentsFromCall variables body call with
        | none => simp [found, arguments, instantiation, except_bind_ok, exists_require_ok] at accepted
        | some types =>
          simp [found, arguments, instantiation, except_bind_ok, exists_require_ok] at accepted
          exact ⟨.call callee sourceArguments, rfl,
            validated_published_call_has_source_type fuel scope solution targetName targetBinder scheme
              (.call callee sourceArguments) nodes types ty accepted.2⟩
    | _ => simp [found, except_bind_ok, exists_require_ok] at accepted

def supportedCaller (caller : SourceFunction) (call : CallDocument) : Bool :=
  caller.parameters.isEmpty && match caller.body, caller.expressions with
    | [.expression (.call _ _)], (site, _) :: _
    | [.returnValue (some (.call _ _))], (site, _) :: _ => decide (site = call.site)
    | _, _ => false

def validatePublishedUnitCall (fuel : Nat) (unit : SourceUnit) (declaration : Nat)
    (functions : List FunctionTarget) (scope : Scope) (solution : Substitution) (targetName : NameId) (targetBinder : BinderId)
    (scheme : Scheme) (variables : VariableResolver) (body : BodyDocument) (call : CallDocument) :
    Except ValidationError MonoType := do
  let some (.function caller) := unit.declarations[declaration]? | throw .unsupported
  if supportedCaller caller call then pure () else throw .unsupported
  validatePublishedSourceCall fuel unit caller functions scope solution targetName targetBinder scheme variables body call

def validatePublishedCallFromSource (fuel : Nat) (sourceId : Nat) (text : String) (declaration : Nat)
    (functions : List FunctionTarget) (scope : Scope) (solution : Substitution) (targetName : NameId) (targetBinder : BinderId)
    (scheme : Scheme) (variables : VariableResolver) (body : BodyDocument) (call : CallDocument) :
    Except ValidationError MonoType := do
  let unit ← match admitSource sourceId text with
    | .ok unit => pure unit
    | .error .exhausted => throw .exhausted
    | .error .unsupported => throw .unsupported
    | .error .malformed => throw .mismatch
  validatePublishedUnitCall fuel unit declaration functions scope solution targetName targetBinder scheme variables body call

theorem validated_call_from_source_has_type (fuel : Nat) (sourceId : Nat) (text : String) (declaration : Nat)
    (functions : List FunctionTarget) (scope : Scope) (solution : Substitution) (targetName : NameId) (targetBinder : BinderId)
    (scheme : Scheme) (variables : VariableResolver) (body : BodyDocument) (call : CallDocument) (ty : MonoType)
    (accepted : validatePublishedCallFromSource fuel sourceId text declaration functions scope solution targetName targetBinder
      scheme variables body call = .ok ty) :
    ∃ unit caller source, admitSource sourceId text = .ok unit ∧
      unit.declarations[declaration]? = some (.function caller) ∧
      caller.expressions.find? (fun (site, _) => decide (site = call.site)) = some (call.site, source) ∧
      ExpressionHasType scope solution source ty := by
  unfold validatePublishedCallFromSource at accepted
  cases admitted : admitSource sourceId text with
  | error failure =>
    rw [admitted] at accepted
    cases failure <;> cases accepted
  | ok unit =>
    simp [admitted, validatePublishedUnitCall] at accepted
    cases member : unit.declarations[declaration]? with
    | none => simp [member] at accepted
    | some entry =>
      cases entry with
      | statement _ _ _ => simp [member] at accepted
      | function caller =>
        cases supported : supportedCaller caller call with
        | false =>
          simp only [member, supported, Bool.false_eq_true, ↓reduceIte] at accepted
          cases accepted
        | true =>
          simp [member, supported] at accepted
          obtain ⟨source, sourceSite, typed⟩ := validated_published_source_call_has_type fuel unit caller functions scope solution
            targetName targetBinder scheme variables body call ty accepted
          exact ⟨unit, caller, source, rfl, member, sourceSite, typed⟩

end Xsh.Conformance.Connected
