import Xsh.Conformance.Connected.EffectFacts

namespace Xsh.Conformance.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Conformance.Transport

def joinEffects (first second : List Effect) : List Effect := (first ++ second).eraseDups

def sourceGlobalNames (unit : SourceUnit) : List NameId :=
  unit.declarations.filterMap fun declaration => match declaration with
    | .function function => some function.name
    | .statement (.binding _ (.name name) _ _) _ _ => some name
    | _ => none

mutual
  def classifyEffectExpression : Nat → SourceUnit → List NameId → Expr → Except AdmissionFailure (List Effect)
    | 0, _, _, _ => .error .exhausted
    | fuel + 1, unit, scope, source =>
      match source with
      | .literal (.integer _) | .literal (.bool _) | .literal (.string _) | .ident _ => .ok []
      | .record fields => classifyEffectFields fuel unit scope fields
      | .field base _ => classifyEffectExpression fuel unit scope base
      | .call (.field (.ident moduleName) operation) [] =>
          if sourceName unit.names "time" = some moduleName && sourceName unit.names "now" = some operation &&
              !(scope.contains moduleName) then .ok [.time] else .error .unsupported
      | _ => .error .unsupported

  def classifyEffectFields : Nat → SourceUnit → List NameId → List RecordField → Except AdmissionFailure (List Effect)
    | 0, _, _, _ => .error .exhausted
    | _ + 1, _, _, [] => .ok []
    | fuel + 1, unit, scope, .named _ value :: rest => do
        let first ← classifyEffectExpression fuel unit scope value
        let second ← classifyEffectFields fuel unit scope rest
        pure (joinEffects first second)
    | _ + 1, _, _, _ => .error .unsupported

  def classifyEffectBlock : Nat → SourceUnit → List NameId → List Statement → Except AdmissionFailure (List Effect)
    | 0, _, _, _ => .error .exhausted
    | _ + 1, _, _, [] => .ok []
    | fuel + 1, unit, scope, statement :: rest => do
      match statement with
      | .binding _ (.name name) _ initializer => do
          let first ← classifyEffectExpression fuel unit scope initializer
          let second ← classifyEffectBlock fuel unit (name :: scope) rest
          pure (joinEffects first second)
      | .assign (.name _) .set value | .expression value | .returnValue (some value) => do
          let first ← classifyEffectExpression fuel unit scope value
          let second ← classifyEffectBlock fuel unit scope rest
          pure (joinEffects first second)
      | .tailName _ => classifyEffectBlock fuel unit scope rest
      | _ => throw .unsupported
end

def classifyFunctionEffects (fuel : Nat) (unit : SourceUnit) (source : SourceFunction) :
    Except AdmissionFailure (List Effect) :=
  if source.kind = .proc then
    classifyEffectBlock fuel unit (source.parameters.map SourceParameter.name ++ sourceGlobalNames unit) source.body
  else .error .unsupported

def sameEffects (first second : List Effect) : Bool := decide (first.Perm second)

def clausePermits (clause : List Effect) (required : Effect) : Bool :=
  clause.contains required ||
    (clause.contains .io && [.fs, .net, .process, .env].contains required)

structure DerivedDeclarationEffect where
  identity : EffectIdentity
  required : List Effect
  effective : List Effect
  inferred : Bool
  redundantClause : Bool
  deriving DecidableEq

def deriveDeclarationEffect (fuel : Nat) (unit : SourceUnit) (declaration : Nat) :
    Except AdmissionFailure DerivedDeclarationEffect := do
  let some (.function source) := unit.declarations[declaration]? | throw .unsupported
  let required ← classifyFunctionEffects fuel unit source
  let effective := source.declaredEffects.getD required
  if required.all (clausePermits effective) then pure () else throw .malformed
  let redundant := match source.declaredEffects with
    | none => false
    | some declared => sameEffects declared required
  pure ⟨⟨none, source.bodySite⟩, required, effective, source.declaredEffects.isNone, redundant⟩

def validateEffectAgainstDerived (expected : DerivedDeclarationEffect) (fact : DeclarationEffectFact) :
    Except AdmissionFailure Unit := do
  let .known actualRequired [] := fact.required | throw .unsupported
  let .known actualEffective [] := fact.effective | throw .unsupported
  if fact.identity = expected.identity ∧ fact.owner = some expected.identity ∧ fact.parameters = [] ∧
      fact.inferred = expected.inferred ∧ fact.inferenceAllowed = true ∧
      fact.redundantClause = expected.redundantClause ∧ fact.unknownChain = [] ∧
      sameEffects actualRequired expected.required = true ∧ sameEffects actualEffective expected.effective = true then
    pure ()
  else throw .malformed

theorem validated_effect_matches_derived (expected : DerivedDeclarationEffect) (fact : DeclarationEffectFact)
    (accepted : validateEffectAgainstDerived expected fact = .ok ()) :
    ∃ required effective,
      fact.required = .known required [] ∧ fact.effective = .known effective [] ∧
      fact.identity = expected.identity ∧ fact.owner = some expected.identity ∧ fact.parameters = [] ∧
      fact.inferred = expected.inferred ∧ fact.inferenceAllowed = true ∧
      fact.redundantClause = expected.redundantClause ∧ fact.unknownChain = [] ∧
      required.Perm expected.required ∧ effective.Perm expected.effective := by
  unfold validateEffectAgainstDerived at accepted
  cases required : fact.required with
  | unknown => simp [required] at accepted
  | known requiredEffects requiredVariables =>
    cases requiredVariables with
    | cons head tail => simp [required] at accepted
    | nil =>
      cases effective : fact.effective with
      | unknown => simp [required, effective] at accepted
      | known effectiveEffects effectiveVariables =>
        cases effectiveVariables with
        | cons head tail => simp [required, effective] at accepted
        | nil =>
          simp only [required, effective] at accepted
          split at accepted
          · rename_i checked
            refine ⟨requiredEffects, effectiveEffects, rfl, rfl, ?_⟩
            simpa [sameEffects] using checked
          · cases accepted

def validateDeclarationEffectFact (fuel : Nat) (unit : SourceUnit) (declaration : Nat)
    (fact : DeclarationEffectFact) : Except AdmissionFailure Unit := do
  let expected ← deriveDeclarationEffect fuel unit declaration
  validateEffectAgainstDerived expected fact

def deriveSourceEffect (fuel : Nat) (sourceId : Nat) (text : String) (declaration : Nat) :
    Except AdmissionFailure DerivedDeclarationEffect := do
  let unit ← admitSource sourceId text
  deriveDeclarationEffect fuel unit declaration

def validateSourceEffectFacts (fuel : Nat) (sourceId : Nat) (text : String)
    (codes : ExchangeCodes) (declaration factIndex : Nat) : Except AdmissionFailure Unit := do
  let expected ← deriveSourceEffect fuel sourceId text declaration
  let some facts := (parseCodes codes).bind decodeDeclarationEffects | throw .malformed
  let some fact := facts[factIndex]? | throw .malformed
  validateEffectAgainstDerived expected fact

end Xsh.Conformance.Connected
