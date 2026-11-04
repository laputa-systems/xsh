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

abbrev EffectOccurrences := List (SourceSite × Expr)

def excludesEffect (excluded : List Effect) (required : Effect) : Bool :=
  excluded.contains required ||
    (excluded.contains .io && [.fs, .net, .process, .env].contains required)

def sameEffectExpressionHead (left right : Expr) : Bool :=
  match left, right with
  | .literal (.integer x), .literal (.integer y) => x == y
  | .literal (.bool x), .literal (.bool y) => x == y
  | .literal (.string x), .literal (.string y) => x == y
  | .ident x, .ident y => decide (x = y)
  | .binary x _ _, .binary y _ _ => decide (x = y)
  | .field _ x, .field _ y => decide (x = y)
  | .record _, .record _ | .call _ _, .call _ _ | .tryExpr _, .tryExpr _ |
    .require _ _, .require _ _ | .valueBlock _, .valueBlock _ => true
  | _, _ => false

mutual
  def classifyRegionExpression : Nat → SourceUnit → List SourceEffectRegion → List NameId → List Effect →
      Expr → EffectOccurrences → Except AdmissionFailure (List Effect × EffectOccurrences)
    | 0, _, _, _, _, _, _ => .error .exhausted
    | fuel + 1, unit, regions, scope, excluded, source, occurrences => do
      let (site, recorded) :: rest := occurrences | throw .malformed
      if sameEffectExpressionHead source recorded then pure () else throw .malformed
      match source with
      | .literal (.integer _) | .literal (.bool _) | .literal (.string _) | .ident _ => pure ([], rest)
      | .binary _ left right => do
          let (first, remaining) ← classifyRegionExpression fuel unit regions scope excluded left rest
          let (second, suffix) ← classifyRegionExpression fuel unit regions scope excluded right remaining
          pure (joinEffects first second, suffix)
      | .record fields => classifyRegionFields fuel unit regions scope excluded fields rest
      | .field base _ => classifyRegionExpression fuel unit regions scope excluded base rest
      | .call (.field (.ident moduleName) operation) [] => do
          if sourceName unit.names "time" = some moduleName && sourceName unit.names "now" = some operation &&
              !(scope.contains moduleName) then pure () else throw .unsupported
          if excludesEffect excluded .time then throw .malformed else pure ()
          let (_, remaining) ← classifyRegionExpression fuel unit regions scope excluded (.field (.ident moduleName) operation) rest
          pure ([.time], remaining)
      | .valueBlock body => do
          let selected := regions.find? fun region => decide (region.body = site)
          let nested := match selected with | some region => region.excluded | none => []
          if nested.contains .error then throw .malformed else pure ()
          classifyRegionBlock fuel unit regions scope (nested ++ excluded) body rest
      | _ => throw .unsupported

  def classifyRegionFields : Nat → SourceUnit → List SourceEffectRegion → List NameId → List Effect →
      List RecordField → EffectOccurrences → Except AdmissionFailure (List Effect × EffectOccurrences)
    | 0, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, [], occurrences => .ok ([], occurrences)
    | fuel + 1, unit, regions, scope, excluded, .named _ value :: rest, occurrences => do
      let (first, remaining) ← classifyRegionExpression fuel unit regions scope excluded value occurrences
      let (second, suffix) ← classifyRegionFields fuel unit regions scope excluded rest remaining
      pure (joinEffects first second, suffix)
    | _ + 1, _, _, _, _, _, _ => .error .unsupported

  def classifyRegionBlock : Nat → SourceUnit → List SourceEffectRegion → List NameId → List Effect →
      List Statement → EffectOccurrences → Except AdmissionFailure (List Effect × EffectOccurrences)
    | 0, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, [], occurrences => .ok ([], occurrences)
    | fuel + 1, unit, regions, scope, excluded, statement :: rest, occurrences => do
      match statement with
      | .binding _ (.name name) _ initializer => do
          let (first, remaining) ← classifyRegionExpression fuel unit regions scope excluded initializer occurrences
          let (second, suffix) ← classifyRegionBlock fuel unit regions (name :: scope) excluded rest remaining
          pure (joinEffects first second, suffix)
      | .assign (.name _) .set value | .expression value | .returnValue (some value) => do
          let (first, remaining) ← classifyRegionExpression fuel unit regions scope excluded value occurrences
          let (second, suffix) ← classifyRegionBlock fuel unit regions scope excluded rest remaining
          pure (joinEffects first second, suffix)
      | .tailName _ => classifyRegionBlock fuel unit regions scope excluded rest occurrences
      | _ => throw .unsupported
end

def classifyRegionFunctionEffects (fuel : Nat) (unit : SourceUnit) (source : SourceFunction) :
    Except AdmissionFailure (List Effect) := do
  if source.regions.all (fun region =>
      source.expressions.any fun (site, expression) =>
        decide (region.body = site) && match expression with | .valueBlock _ => true | _ => false) &&
      decide (source.regions.map SourceEffectRegion.body).Nodup then pure () else throw .malformed
  let (effects, remaining) ← classifyRegionBlock fuel unit source.regions
    (source.parameters.map SourceParameter.name ++ sourceGlobalNames unit) [] source.body source.expressions
  if remaining.isEmpty then pure effects else throw .malformed

def classifyFunctionEffects (fuel : Nat) (unit : SourceUnit) (source : SourceFunction) :
    Except AdmissionFailure (List Effect) :=
  if source.kind = .proc then
    if source.regions.isEmpty then
      classifyEffectBlock fuel unit (source.parameters.map SourceParameter.name ++ sourceGlobalNames unit) source.body
    else classifyRegionFunctionEffects fuel unit source
  else .error .unsupported

def regionEffectsControl : Bool :=
  let allowed := "proc f() [time] { without fs { time.now() } }\n"
  let denied := "proc f() [time] { without time { time.now() } }\n"
  let errorBound := "proc f() [time] { without error { 1 } }\n"
  let afterRegion := "proc f() [time] { without time { 1 }; time.now() }\n"
  let nested := "proc f() [time] { without fs { without time { time.now() } } }\n"
  let checks (text : String) (expected : Bool) :=
    match admitSource 0 text with
    | .ok unit => match unit.declarations with
      | [.function source] => match classifyFunctionEffects 64 unit source with
        | .ok effects => expected && decide (effects = [.time])
        | .error .malformed => !expected
        | _ => false
      | _ => false
    | _ => false
  checks allowed true && checks denied false && checks errorBound false &&
    checks afterRegion true && checks nested false

def regionOccurrenceMutationRejected : Bool :=
  match admitSource 0 "proc f() [time] { without fs { time.now() } }\n" with
  | .ok unit => match unit.declarations with
    | [.function source] => match source.expressions with
      | (site, _) :: rest =>
        let changed := { source with expressions := (site, .literal (.bool true)) :: rest }
        match classifyRegionFunctionEffects 64 unit changed with
        | .error .malformed => true
        | _ => false
      | _ => false
    | _ => false
  | _ => false

theorem changed_region_occurrence_is_rejected : regionOccurrenceMutationRejected = true := by
  decide +kernel

theorem lexical_effect_bounds_control_classification : regionEffectsControl = true := by
  decide +kernel

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
