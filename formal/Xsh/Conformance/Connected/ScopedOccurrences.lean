import Xsh.Conformance.Connected.QualifiedFacts
import Xsh.Conformance.Connected.CallFacts
import Xsh.Conformance.Connected.SourceEffectModel

namespace Xsh.Conformance.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Typing.Connected
open Xsh.Conformance.Transport

structure ScopedOccurrence where
  site : SourceSite
  source : Expr
  candidate : Node
  scope : Scope
  declarations : List (BinderId × NameId × SourceSite × Option String × Bool × Bool)
  storeTarget : Option (BinderId × SourceSite)
  storeDeclaredHere : Bool
  declared : Option (BinderId × Xsh.Typing.Connected.BindingKind × SourceSite)

structure OccurrenceCursor where
  expressions : List (SourceSite × Expr)
  statements : List (SourceSite × Statement)

abbrev OccurrenceWalk := StateT OccurrenceCursor (Except ValidationError)

def expressionHeadMatches (left right : Expr) : Bool :=
  match left, right with
  | .literal (.integer x), .literal (.integer y) => x == y
  | .literal (.bool x), .literal (.bool y) => x == y
  | .literal (.string x), .literal (.string y) => x == y
  | .ident x, .ident y => decide (x = y)
  | .binary x _ _, .binary y _ _ => decide (x = y)
  | .field _ x, .field _ y => decide (x = y)
  | .record _, .record _ | .call _ _, .call _ _ | .tryExpr _, .tryExpr _ |
    .require _ _, .require _ _ | .ifExpr _ _, .ifExpr _ _ | .valueBlock _, .valueBlock _ => true
  | _, _ => false

def statementHeadMatches (left right : Statement) : Bool :=
  match left, right with
  | .binding kind (.name x) _ _, .binding other (.name y) _ _ => decide (kind = other ∧ x = y)
  | .assign (.name x) .set _, .assign (.name y) .set _ => decide (x = y)
  | .tailName x, .tailName y => decide (x = y)
  | .expression _, .expression _ | .returnValue (some _), .returnValue (some _) |
    .ifStmt _ _, .ifStmt _ _ => true
  | _, _ => false

def nextExpressionSite (expression : Expr) : OccurrenceWalk SourceSite := do
  let state ← get
  let (site, recorded) :: rest := state.expressions | throw .mismatch
  if expressionHeadMatches expression recorded then pure () else throw .mismatch
  set { state with expressions := rest }
  pure site

def nextStatementSite (statement : Statement) : OccurrenceWalk SourceSite := do
  let state ← get
  let (site, recorded) :: rest := state.statements | throw .mismatch
  if statementHeadMatches statement recorded then pure () else throw .mismatch
  set { state with statements := rest }
  pure site

mutual
  def scopeExpression : Nat → BinderTypes → Scope → List (BinderId × NameId × SourceSite × Option String × Bool × Bool) →
      Expr → Node → OccurrenceWalk (List ScopedOccurrence)
    | 0, _, _, _, _, _ => throw .exhausted
    | fuel + 1, types, scope, declarations, source, candidate => do
      let site ← nextExpressionSite source
      let current : ScopedOccurrence := ⟨site, source, candidate, scope, declarations, none, false, none⟩
      let children ← match source, candidate with
        | .literal (.integer left), .integer right =>
          if left == right then pure [] else throw .mismatch
        | .literal (.bool left), .boolean right =>
          if left == right then pure [] else throw .mismatch
        | .literal (.string left), .string right =>
          if left == right then pure [] else throw .mismatch
        | .ident name, .read binder _ => do
          let some binding := scope.find? (fun binding => decide (binding.name = name)) | throw .mismatch
          if binding.binder = binder then pure [] else throw .mismatch
        | .binary op left right, .binary recorded first second => do
          if op = recorded then pure () else throw .mismatch
          let a ← scopeExpression fuel types scope declarations left first
          let b ← scopeExpression fuel types scope declarations right second
          pure (a ++ b)
        | .field base name, .field recorded other => do
          if name = other then scopeExpression fuel types scope declarations base recorded else throw .mismatch
        | .tryExpr value, .propagate recorded _ => scopeExpression fuel types scope declarations value recorded
        | .require value (some (.named name)), .requireValue binder _ recorded => do
          let some binding := scope.find? (fun binding => decide (binding.name = name)) | throw .mismatch
          if binding.binder = binder then scopeExpression fuel types scope declarations value recorded else throw .mismatch
        | .record fields, .record recorded => scopeFields fuel types scope declarations fields recorded
        | .call callee arguments, .call recorded nodes => do
          let first ← scopeExpression fuel types scope declarations callee recorded
          let rest ← scopeArguments fuel types scope declarations arguments nodes
          pure (first ++ rest)
        | .ifExpr branches otherwise, .conditional recorded fallback => do
          let first ← scopeExpressionBranches fuel types scope declarations branches recorded
          let second ← scopeExpression fuel types scope declarations otherwise fallback
          pure (first ++ second)
        | .valueBlock body, .valueBlock _ nodes => scopeBlock fuel types scope declarations [] body nodes
        | _, _ => throw .unsupported
      pure (current :: children)

  def scopeArguments : Nat → BinderTypes → Scope → List (BinderId × NameId × SourceSite × Option String × Bool × Bool) →
      List CallArgument → List Node → OccurrenceWalk (List ScopedOccurrence)
    | 0, _, _, _, _, _ => throw .exhausted
    | _ + 1, _, _, _, [], [] => pure []
    | fuel + 1, types, scope, declarations, .positional value :: rest, node :: nodes => do
      let first ← scopeExpression fuel types scope declarations value node
      let others ← scopeArguments fuel types scope declarations rest nodes
      pure (first ++ others)
    | _ + 1, _, _, _, _, _ => throw .unsupported

  def scopeFields : Nat → BinderTypes → Scope → List (BinderId × NameId × SourceSite × Option String × Bool × Bool) →
      List RecordField → List (NameId × Node) → OccurrenceWalk (List ScopedOccurrence)
    | 0, _, _, _, _, _ => throw .exhausted
    | _ + 1, _, _, _, [], [] => pure []
    | fuel + 1, types, scope, declarations, .named name value :: rest, (recorded, node) :: nodes => do
      if name = recorded then pure () else throw .mismatch
      let first ← scopeExpression fuel types scope declarations value node
      let others ← scopeFields fuel types scope declarations rest nodes
      pure (first ++ others)
    | _ + 1, _, _, _, _, _ => throw .unsupported

  def scopeExpressionBranches : Nat → BinderTypes → Scope → List (BinderId × NameId × SourceSite × Option String × Bool × Bool) →
      List (Expr × Expr) → List (Node × Node) → OccurrenceWalk (List ScopedOccurrence)
    | 0, _, _, _, _, _ => throw .exhausted
    | _ + 1, _, _, _, [], [] => pure []
    | fuel + 1, types, scope, declarations, (condition, value) :: rest, (guard, node) :: nodes => do
      let first ← scopeExpression fuel types scope declarations condition guard
      let second ← scopeExpression fuel types scope declarations value node
      let others ← scopeExpressionBranches fuel types scope declarations rest nodes
      pure (first ++ second ++ others)
    | _ + 1, _, _, _, _, _ => throw .mismatch

  def scopeStatementBranches : Nat → BinderTypes → Scope → List (BinderId × NameId × SourceSite × Option String × Bool × Bool) →
      List (Expr × List Statement) → List (Node × List StatementNode) → OccurrenceWalk (List ScopedOccurrence)
    | 0, _, _, _, _, _ => throw .exhausted
    | _ + 1, _, _, _, [], [] => pure []
    | fuel + 1, types, scope, declarations, (condition, body) :: rest, (guard, nodes) :: candidates => do
      let first ← scopeExpression fuel types scope declarations condition guard
      let second ← scopeBlock fuel types scope declarations [] body nodes
      let others ← scopeStatementBranches fuel types scope declarations rest candidates
      pure (first ++ second ++ others)
    | _ + 1, _, _, _, _, _ => throw .mismatch

  def scopeBlock : Nat → BinderTypes → Scope → List (BinderId × NameId × SourceSite × Option String × Bool × Bool) →
      List BinderId → List Statement → List StatementNode → OccurrenceWalk (List ScopedOccurrence)
    | 0, _, _, _, _, _, _ => throw .exhausted
    | _ + 1, _, _, _, _, [], [] => pure []
    | fuel + 1, types, scope, declarations, localBinders, statement :: rest, node :: nodes => do
      let site ← nextStatementSite statement
      match statement, node with
      | .binding kind (.name name) _ initializer, .binding binder recorded storage value => do
          if bindingKind kind = recorded then pure () else throw .mismatch
          let some rawType := types binder | throw .mismatch
          if storage = rawType then pure () else throw .mismatch
          let initial ← scopeExpression fuel types scope declarations initializer value
          let head :: nested := initial | throw .mismatch
          let first := { head with declared := some (binder, recorded, site) } :: nested
          let binding : Binding := ⟨name, binder, recorded, ⟨[], rawType, []⟩⟩
          let others ← scopeBlock fuel types (binding :: scope) ((binder, name, site, none, false, false) :: declarations) (binder :: localBinders) rest nodes
          pure (first ++ others)
      | .assign (.name name) .set value, .assignment binder recorded => do
          let some binding := scope.find? (fun binding => decide (binding.name = name)) | throw .mismatch
          if binding.binder = binder then pure () else throw .mismatch
          let first ← scopeExpression fuel types scope declarations value recorded
          let head :: nested := first | throw .mismatch
          let others ← scopeBlock fuel types scope declarations localBinders rest nodes
          pure ({ head with storeTarget := some (binder, site), storeDeclaredHere := localBinders.contains binder } :: nested ++ others)
      | .expression value, .evaluated recorded | .returnValue (some value), .returned recorded => do
          let first ← scopeExpression fuel types scope declarations value recorded
          let others ← scopeBlock fuel types scope declarations localBinders rest nodes
          pure (first ++ others)
      | .tailName name, .evaluated recorded => do
          let first ← scopeExpression fuel types scope declarations (.ident name) recorded
          let others ← scopeBlock fuel types scope declarations localBinders rest nodes
          pure (first ++ others)
      | .ifStmt branches otherwise, .conditional recorded fallback => do
          let first ← scopeStatementBranches fuel types scope declarations branches recorded
          let second ← match otherwise with
            | none => if fallback.isEmpty then pure [] else throw .mismatch
            | some body => scopeBlock fuel types scope declarations [] body fallback
          let others ← scopeBlock fuel types scope declarations localBinders rest nodes
          pure (first ++ second ++ others)
      | _, _ => throw .unsupported
    | _ + 1, _, _, _, _, _, _ => throw .mismatch
end

def sourceScopedOccurrences (fuel : Nat) (source : SourceFunction) (parameters globals : List Binding)
    (globalSites : List (BinderId × NameId × SourceSite × Option String × Bool × Bool)) (types : BinderTypes)
    (candidate : List StatementNode) : Except ValidationError (List ScopedOccurrence) := do
  if source.parameters.length = parameters.length then pure () else throw .mismatch
  let parameterSites ← (source.parameters.zip parameters).mapM fun (parameter, binding) => do
    if parameter.name = binding.name then pure (binding.binder, parameter.name, parameter.site, none, false, false) else throw .mismatch
  let cursor : OccurrenceCursor := ⟨sourceOccurrences source, source.statements⟩
  let (occurrences, remaining) ← (scopeBlock fuel types (parameters ++ globals) (parameterSites ++ globalSites) []
    source.body candidate).run cursor
  if remaining.expressions.isEmpty && remaining.statements.isEmpty then pure occurrences else throw .mismatch

def scopedOccurrenceControl : Bool :=
  match admitSource 0 "pure f() -> Int { var x = 0; without time { x = 1 }; return x }\n" with
  | .ok unit => match unit.declarations with
    | [.function source] =>
      let binder : BinderId := ⟨⟨0, by decide⟩⟩
      let candidate : List StatementNode :=
        [.binding binder .mutable (.primitive .integer) (.integer "0"),
         .evaluated (.valueBlock (.primitive .unit) [.assignment binder (.integer "1")]),
         .returned (.read binder [])]
      match sourceScopedOccurrences 64 source [] [] [] (fun _ => some (.primitive .integer)) candidate with
      | .ok occurrences => match occurrences.reverse with
        | read :: _ => match read.declarations with
          | [(seen, _, site, _, _, _)] => decide (seen = binder) && decide (site = ⟨0, 18, 10⟩)
          | _ => false
        | _ => false
      | _ => false
    | _ => false
  | _ => false

theorem scoped_read_retains_its_local_declaration : scopedOccurrenceControl = true := by
  decide +kernel

def scopedStoreScopeControl : Bool :=
  let binder : BinderId := ⟨⟨0, by decide⟩⟩
  let types : BinderTypes := fun _ => some (.primitive .integer)
  let readStore (text : String) (candidate : List StatementNode) : Option Bool := do
    let unit ← (admitSource 0 text).toOption
    let [.function source] := unit.declarations | none
    let occurrences ← (sourceScopedOccurrences 64 source [] [] [] types candidate).toOption
    let occurrence ← occurrences.find? (fun occurrence => occurrence.storeTarget.isSome)
    pure occurrence.storeDeclaredHere
  let same := readStore "pure f() -> Int { var x = 0; x = 1; return x }\n"
    [.binding binder .mutable (.primitive .integer) (.integer "0"),
      .assignment binder (.integer "1"), .returned (.read binder [])]
  let outer := readStore "pure f() -> Int { var x = 0; without time { x = 1 }; return x }\n"
    [.binding binder .mutable (.primitive .integer) (.integer "0"),
      .evaluated (.valueBlock (.primitive .unit) [.assignment binder (.integer "1")]),
      .returned (.read binder [])]
  let inner : BinderId := ⟨⟨1, by decide⟩⟩
  let shadowed (assigned : BinderId) := readStore
    "pure f() -> Int { var x = 0; without time { var x = 1; x = 2 }; return x }\n"
    [.binding binder .mutable (.primitive .integer) (.integer "0"),
      .evaluated (.valueBlock (.primitive .unit)
        [.binding inner .mutable (.primitive .integer) (.integer "1"), .assignment assigned (.integer "2")]),
      .returned (.read binder [])]
  decide (same = some true ∧ outer = some false ∧ shadowed inner = some true ∧ shadowed binder = none)

theorem scoped_store_distinguishes_local_and_outer_bindings : scopedStoreScopeControl = true := by
  decide +kernel


def shadowedScopedReadControl : Bool :=
  match admitSource 0 "pure f() -> Int { var x = 0; without time { let x = 1; x }; return x }\n" with
  | .ok unit => match unit.declarations with
    | [.function source] =>
      let outer : BinderId := ⟨⟨0, by decide⟩⟩
      let inner : BinderId := ⟨⟨1, by decide⟩⟩
      let run (inside outside : BinderId) :=
        let candidate : List StatementNode :=
          [.binding outer .mutable (.primitive .integer) (.integer "0"),
           .evaluated (.valueBlock (.primitive .integer)
             [.binding inner .immutable (.primitive .integer) (.integer "1"), .evaluated (.read inside [])]),
           .returned (.read outside [])]
        sourceScopedOccurrences 64 source [] [] [] (fun _ => some (.primitive .integer)) candidate
      let accepted := match run inner outer with | .ok _ => true | _ => false
      let wrongInner := match run outer outer with | .error .mismatch => true | _ => false
      let wrongOuter := match run inner inner with | .error .mismatch => true | _ => false
      accepted && wrongInner && wrongOuter
    | _ => false
  | _ => false

theorem scoped_shadowing_rejects_wrong_binding : shadowedScopedReadControl = true := by
  decide +kernel


def scopedQualifiedFactChecks (fuel : Nat) (names : TypeNames) (result : MonoType) (types : BinderTypes)
    (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (occurrences : List ScopedOccurrence) (fact : Nat × SourceSite × MonoType) : Bool :=
  match occurrences.find? (fun occurrence => decide (occurrence.site = fact.2.1)) with
  | none => false
  | some occurrence =>
    match validateQualifiedExpressionIn fuel (some (names, result, types)) occurrence.scope principal requirements
        fields occurrence.source occurrence.candidate with
    | .error _ => false
    | .ok actual => decide (actual = fact.2.2)

theorem checked_scoped_fact_types_every_solution (fuel : Nat) (names : TypeNames) (result : MonoType)
    (types : BinderTypes) (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (occurrences : List ScopedOccurrence) (fact : Nat × SourceSite × MonoType)
    (checked : scopedQualifiedFactChecks fuel names result types principal requirements fields occurrences fact = true)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ∃ occurrence, occurrences.find? (fun occurrence => decide (occurrence.site = fact.2.1)) = some occurrence ∧
      ExpressionHasTypeIn (compose later principal) (some (names, substitute later result)) occurrence.scope
        occurrence.source (substitute later fact.2.2) := by
  unfold scopedQualifiedFactChecks at checked
  cases found : occurrences.find? (fun occurrence => decide (occurrence.site = fact.2.1)) with
  | none => simp [found] at checked
  | some occurrence =>
    cases actual : validateQualifiedExpressionIn fuel (some (names, result, types)) occurrence.scope principal
        requirements fields occurrence.source occurrence.candidate with
    | error error => simp [found, actual] at checked
    | ok ty =>
      simp [found, actual] at checked
      subst ty
      refine ⟨occurrence, rfl, ?_⟩
      have qualified := (c_validate_qualified_all_sound fuel).1 (some (names, result, types)) occurrence.scope
        principal requirements fields occurrence.source occurrence.candidate fact.2.2 actual
      exact qualified_context_instantiates principal later requirements (some (names, result)) occurrence.scope
        occurrence.source fact.2.2 qualified sorted solves


def scopedReadBindingCheck (names : List String) (occurrences : List ScopedOccurrence)
    (site : SourceSite) (published : ReadBinding) : Bool :=
  match occurrences.find? (fun occurrence => decide (occurrence.site = site)) with
  | some occurrence => match occurrence.source, occurrence.candidate with
    | .ident name, .read binder _ =>
      match occurrence.declarations.find? (fun (id, _, _, _, _, _) => decide (id = binder)) with
      | some (_, declared, declarationSite, ownerNamespace, global, exported) =>
        decide (name = declared) && decide (sourceName names published.name = some name) &&
          decide (published.site = declarationSite) && decide (published.moduleName = ownerNamespace) &&
          decide (published.global = global) && decide (published.exported = exported)
      | none => false
    | _, _ => false
  | none => false

def scopedResolvedReadCheck (names : List String) (occurrences : List ScopedOccurrence)
    (body : BodyDocument) (tail : Bool) (entry : Nat × ReadBinding) : Bool :=
  let fact := if tail then lookupTailFact body entry.1 else lookupExpressionFact body entry.1
  match fact with
  | none => false
  | some (_, site, _) => scopedReadBindingCheck names occurrences site entry.2

def checksScopedReadFacts (names : List String) (source : SourceFunction)
    (occurrences : List ScopedOccurrence) (body : BodyDocument) : Bool :=
  let resolvedSites := body.resolvedReads.filterMap (resolvedReadSite body)
  let resolvedTailSites := body.resolvedTails.filterMap (resolvedTailSite body)
  let readBindings := (body.resolvedReads.map Prod.snd ++ body.resolvedTails.map Prod.snd).eraseDups
  decide ((body.resolvedReads.map Prod.fst).Nodup) && decide ((body.resolvedTails.map Prod.fst).Nodup) &&
    decide (resolvedSites.length = body.resolvedReads.length) &&
    decide (resolvedTailSites.length = body.resolvedTails.length) &&
    decide (resolvedSites.Perm (sourceReadSites source)) &&
    decide (resolvedTailSites.Perm ((sourceTailOccurrences source).map Prod.fst)) &&
    decide (body.readBindings.Nodup) && decide (body.readBindings.Perm readBindings) &&
    body.resolvedReads.all (scopedResolvedReadCheck names occurrences body false) &&
    body.resolvedTails.all (scopedResolvedReadCheck names occurrences body true)


def checksScopedQualifiedFacts (fuel : Nat) (names : TypeNames) (result : MonoType) (types : BinderTypes)
    (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : SourceFunction) (occurrences : List ScopedOccurrence) (body : BodyDocument) : Bool :=
  decide ((occurrences.map ScopedOccurrence.site).Nodup) &&
    decide ((source.expressions.map Prod.fst).Nodup) &&
    decide ((body.expressionTypes.map Prod.fst).Nodup) &&
    decide ((body.expressionTypes.map (fun fact => fact.2.1)).Nodup) &&
    decide ((body.expressionTypes.map (fun fact => fact.2.1)).Perm (source.expressions.map Prod.fst)) &&
    decide ((body.tailTypes.map Prod.fst).Nodup) &&
    decide ((body.tailTypes.map (fun fact => fact.2.1)).Nodup) &&
    decide ((body.tailTypes.map (fun fact => fact.2.1)).Perm ((sourceTailOccurrences source).map Prod.fst)) &&
    body.expressionTypes.all (scopedQualifiedFactChecks fuel names result types principal requirements fields occurrences) &&
    body.tailTypes.all (scopedQualifiedFactChecks fuel names result types principal requirements fields occurrences)


def scopedReadFactControl : Bool :=
  match admitSource 0 "pure f() -> Int { var x = 0; without time { x = 1 }; return x }\n" with
  | .ok unit => match unit.declarations with
    | [.function source] =>
      let binder : BinderId := ⟨⟨0, by decide⟩⟩
      let candidate : List StatementNode :=
        [.binding binder .mutable (.primitive .integer) (.integer "0"),
         .evaluated (.valueBlock (.primitive .unit) [.assignment binder (.integer "1")]),
         .returned (.read binder [])]
      match sourceScopedOccurrences 64 source [] [] [] (fun _ => some (.primitive .integer)) candidate with
      | .ok occurrences => match occurrences.reverse with
        | read :: _ =>
          let binding : ReadBinding := ⟨"x", ⟨0, 18, 10⟩, none, false, false⟩
          scopedReadBindingCheck unit.names occurrences read.site binding &&
            !(scopedReadBindingCheck unit.names occurrences read.site { binding with site := ⟨0, 18, 9⟩ }) &&
            !(scopedReadBindingCheck unit.names occurrences read.site { binding with global := true })
        | _ => false
      | _ => false
    | _ => false
  | _ => false

theorem scoped_read_fact_rejects_wrong_owner : scopedReadFactControl = true := by
  decide +kernel


structure StoreOperation where
  site : SourceSite
  storage : MonoType
  replacement : MonoType
  deriving DecidableEq

inductive ScopedOperation where
  | field (value : FieldOperation)
  | store (value : StoreOperation)
  deriving DecidableEq

def ScopedOperation.site : ScopedOperation → SourceSite
  | .field operation => operation.site
  | .store operation => operation.site

def ScopedOperation.field? : ScopedOperation → Option FieldOperation
  | .field operation => some operation
  | .store _ => none

structure ScopedBodyDocument where
  body : BodyDocument
  operations : List ScopedOperation
  deriving DecidableEq

inductive ScopedOperationBinding where
  | field
  | store
  deriving DecidableEq

structure ScopedCallDocument where
  expression : Nat
  site : SourceSite
  role : InstantiationRole
  target : FunctionTarget
  typeArguments : List (Nat × MonoType)
  operationBindings : List ScopedOperationBinding
  deriving DecidableEq

structure ScopedFactsDocument where
  functions : List FunctionTarget
  bodies : List ScopedBodyDocument
  calls : List ScopedCallDocument
  deriving DecidableEq

def decodeScopedOperation (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option ScopedOperation := do
  let .object fields := value | none
  match textField fields "kind" with
  | some "field" => return .field (← decodeOperation fuel names variables value)
  | some "storeValue" =>
    let fields ← value.closedObject? ["kind", "storage", "replacement", "span"]
    return .store ⟨← siteField fields "span",
      ← (WireValue.field? fields "storage").bind (decodeType fuel names variables),
      ← (WireValue.field? fields "replacement").bind (decodeType fuel names variables)⟩
  | _ => none

def decodeScopedBody (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option ScopedBodyDocument := do
  let fields ← value.closedObject? ["span", "kind", "typeParameters", "parameters", "result", "explicitReturn",
    "effects", "inferredEffects", "operations", "expressionTypes", "tailTypes", "readBindings",
    "resolvedReads", "resolvedTails", "operationSites"]
  let kind ← match textField fields "kind" with
    | some "pure" => some CallableKind.pure
    | some "proc" => some CallableKind.proc
    | some "stream" => some CallableKind.stream
    | _ => none
  let operations ← listField fields "operations" (decodeScopedOperation fuel names variables)
  let body : BodyDocument := ⟨← siteField fields "span", kind, ← listField fields "typeParameters" decodeWord,
    ← listField fields "parameters" (decodeParameter fuel names variables),
    ← (WireValue.field? fields "result").bind (decodeType fuel names variables),
    ← boolField fields "explicitReturn", ← (WireValue.field? fields "effects").bind decodeEffects,
    ← boolField fields "inferredEffects", operations.filterMap ScopedOperation.field?,
    ← listField fields "expressionTypes" (decodeTypedSite fuel names variables),
    ← listField fields "tailTypes" (decodeTypedSite fuel names variables),
    ← listField fields "readBindings" decodeRead, ← listField fields "resolvedReads" decodeResolvedRead,
    ← listField fields "resolvedTails" decodeResolvedTail, ← listField fields "operationSites" decodeOperationSite⟩
  pure ⟨body, operations⟩

def decodeScopedBinding (value : WireValue) : Option ScopedOperationBinding :=
  match value.text? with
  | some "field" => some .field
  | some "storeValue" => some .store
  | _ => none

def decodeScopedCall (fuel : Nat) (names : NameMap) (variables : VariableMap)
    (value : WireValue) : Option ScopedCallDocument := do
  let fields ← value.closedObject? ["expression", "span", "role", "target", "typeArguments", "operationBindings"]
  let role ← match textField fields "role" with
    | some "call" => some InstantiationRole.call
    | some "value" => some InstantiationRole.value
    | _ => none
  pure ⟨← wordField fields "expression", ← siteField fields "span", role,
    ← (WireValue.field? fields "target").bind decodeTarget,
    ← listField fields "typeArguments" (decodeTypeArgument fuel names variables),
    ← listField fields "operationBindings" decodeScopedBinding⟩


def decodeScopedFactsValue (fuel : Nat) (names : NameResolver) (value : WireValue) :
    Option (ScopedFactsDocument × List (SourceSite × List Nat)) := do
  let scopes ← factScopesValue value
  let variables := scopedVariableResolver scopes
  let fields ← value.closedObject? ["schema", "functions", "bodies", "calls"]
  if textField fields "schema" = some "xsh-connected-field-facts-v1" then pure () else none
  let bodies ← listField fields "bodies" fun bodyValue => do
    let .object bodyFields := bodyValue | none
    let bodySite ← siteField bodyFields "span"
    decodeScopedBody fuel (names bodySite.source) (variables bodySite) bodyValue
  let calls ← listField fields "calls" fun callValue => do
    let .object callFields := callValue | none
    let callSite ← siteField callFields "span"
    let target ← (WireValue.field? callFields "target").bind decodeTarget
    decodeScopedCall fuel (names callSite.source) (variables target.body) callValue
  let document : ScopedFactsDocument := ⟨← listField fields "functions" decodeTarget, bodies, calls⟩
  pure (document, scopes)

def decodeScopedFactsDocument (fuel : Nat) (names : NameResolver) (codes : ExchangeCodes) :
    Option (ScopedFactsDocument × List (SourceSite × List Nat)) := do
  let value ← parseCodes codes
  decodeScopedFactsValue fuel names value


def ScopedOperation.binding : ScopedOperation → ScopedOperationBinding
  | .field _ => .field
  | .store _ => .store

def ScopedOperation.constraint : ScopedOperation → Constraint
  | .field operation => operation.constraint
  | .store operation => .store operation.storage operation.replacement

def schemeFromScopedBody (variables : VariableResolver) (document : ScopedBodyDocument) : Option Scheme := do
  let base ← schemeFromBody variables document.body
  pure { base with requirements := document.operations.map ScopedOperation.constraint }

def scopedOperationSiteCheck (fuel : Nat) (names : TypeNames) (result : MonoType) (types : BinderTypes)
    (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (occurrences : List ScopedOccurrence) (document : ScopedBodyDocument) (entry : Nat × Nat) : Bool :=
  match lookupExpressionFact document.body entry.1, document.operations[entry.2]? with
  | some (_, site, _), some operation =>
    decide (site = operation.site) && match operation with
    | .field field =>
      match occurrences.find? (fun occurrence => decide (occurrence.site = site)) with
      | some occurrence => match occurrence.source with
        | .field _ name => decide (name = field.name)
        | _ => false
      | none => false
    | .store store =>
      match occurrences.find? (fun occurrence => decide (occurrence.site = site)) with
      | none => false
      | some occurrence => match occurrence.storeTarget with
        | none => false
        | some (binder, _) => match occurrence.scope.find? (fun binding => decide (binding.binder = binder)) with
          | none => false
          | some binding =>
            decide (binding.kind = .mutable) &&
              decide (store.storage = substitute principal binding.scheme.body) &&
              decide (Constraint.store store.storage store.replacement ∈ requirements) &&
              match validateQualifiedExpressionIn fuel (some (names, result, types)) occurrence.scope principal
                  requirements fields occurrence.source occurrence.candidate with
              | .ok actual => decide (store.replacement = actual)
              | .error _ => false
  | _, _ => false

theorem checked_store_site_has_scoped_assignment (fuel : Nat) (names : TypeNames) (result : MonoType)
    (types : BinderTypes) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (occurrences : List ScopedOccurrence) (document : ScopedBodyDocument) (entry : Nat × Nat)
    (store : StoreOperation)
    (operation : document.operations[entry.2]? = some (.store store))
    (checked : scopedOperationSiteCheck fuel names result types principal requirements fields occurrences document entry = true) :
    ∃ fact occurrence binder statementSite binding,
      lookupExpressionFact document.body entry.1 = some fact ∧ fact.2.1 = store.site ∧
      occurrences.find? (fun item => decide (item.site = store.site)) = some occurrence ∧
      occurrence.storeTarget = some (binder, statementSite) ∧
      occurrence.scope.find? (fun item => decide (item.binder = binder)) = some binding ∧
      binding.kind = .mutable ∧ store.storage = substitute principal binding.scheme.body ∧
      Constraint.store store.storage store.replacement ∈ requirements ∧
      validateQualifiedExpressionIn fuel (some (names, result, types)) occurrence.scope principal requirements fields
        occurrence.source occurrence.candidate = .ok store.replacement := by
  unfold scopedOperationSiteCheck at checked
  cases fact : lookupExpressionFact document.body entry.1 with
  | none => simp [fact, operation] at checked
  | some foundFact =>
    simp only [fact, operation, ScopedOperation.site, Bool.and_eq_true] at checked
    obtain ⟨sameSiteCheck, checked⟩ := checked
    have sameSite : foundFact.2.1 = store.site := of_decide_eq_true sameSiteCheck
    rw [sameSite] at checked
    cases found : occurrences.find? (fun item => decide (item.site = store.site)) with
    | none => simp [found] at checked
    | some occurrence =>
      cases target : occurrence.storeTarget with
      | none => simp [found, target] at checked
      | some targetPair =>
        obtain ⟨binder, statementSite⟩ := targetPair
        cases binding : occurrence.scope.find? (fun item => decide (item.binder = binder)) with
        | none => simp [found, target, binding] at checked
        | some declared =>
          cases rhs : validateQualifiedExpressionIn fuel (some (names, result, types)) occurrence.scope principal
              requirements fields occurrence.source occurrence.candidate with
          | error error => simp [found, target, binding, rhs] at checked
          | ok ty =>
            simp [found, target, binding, rhs, Bool.and_eq_true] at checked
            obtain ⟨⟨⟨mutable, storage⟩, equal⟩, replacement⟩ := checked
            subst ty
            exact ⟨foundFact, occurrence, binder, statementSite, declared, rfl, sameSite, rfl, target, binding,
              mutable, storage, equal, rhs⟩

theorem checked_store_site_types_every_solution (fuel : Nat) (names : TypeNames) (result : MonoType)
    (types : BinderTypes) (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (occurrences : List ScopedOccurrence) (document : ScopedBodyDocument) (entry : Nat × Nat)
    (store : StoreOperation)
    (operation : document.operations[entry.2]? = some (.store store))
    (checked : scopedOperationSiteCheck fuel names result types principal requirements fields occurrences document entry = true)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ∃ occurrence binder statementSite binding,
      occurrences.find? (fun item => decide (item.site = store.site)) = some occurrence ∧
      occurrence.storeTarget = some (binder, statementSite) ∧
      occurrence.scope.find? (fun item => decide (item.binder = binder)) = some binding ∧
      binding.kind = .mutable ∧
      StoreCompatible (substitute (compose later principal) binding.scheme.body) (substitute later store.replacement) ∧
      ExpressionHasTypeIn (compose later principal) (some (names, substitute later result)) occurrence.scope
        occurrence.source (substitute later store.replacement) := by
  obtain ⟨_, occurrence, binder, statementSite, binding, _, _, found, target, selected, mutable, storage, member, typed⟩ :=
    checked_store_site_has_scoped_assignment fuel names result types principal requirements fields occurrences document
      entry store operation checked
  have compatible := solves _ member
  have expression := (c_validate_qualified_all_sound fuel).1 (some (names, result, types)) occurrence.scope
    principal requirements fields occurrence.source occurrence.candidate store.replacement typed
  refine ⟨occurrence, binder, statementSite, binding, found, target, selected, mutable, ?_, ?_⟩
  · simpa [satisfies, storage, substitute_compose] using compatible
  · exact qualified_context_instantiates principal later requirements (some (names, result)) occurrence.scope
      occurrence.source store.replacement expression sorted solves

def symbolicStoreOccurrence (principal : Substitution) (occurrence : ScopedOccurrence) : Bool :=
  match occurrence.storeTarget with
  | none => false
  | some (binder, _) => match occurrence.scope.find? (fun binding => decide (binding.binder = binder)) with
    | none => false
    | some binding => match binding.scheme.body with
      | .variable _ => !(freeVariables (substitute principal binding.scheme.body)).isEmpty
      | _ => false

def checksScopedOperationFacts (fuel : Nat) (names : TypeNames) (result : MonoType) (types : BinderTypes)
    (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : SourceFunction) (occurrences : List ScopedOccurrence) (document : ScopedBodyDocument) : Bool :=
  let fieldsPublished := document.operations.filterMap ScopedOperation.field?
  let requiredStores := occurrences.filter (symbolicStoreOccurrence principal)
  let publishedStores := document.operations.filterMap fun operation => match operation with
    | .store store => some store.site
    | _ => none
  decide ((fieldsPublished.map fun operation => (operation.site, operation.name)).Perm
    (sourceFieldSites source.expressions)) &&
    decide (publishedStores.Perm (requiredStores.map ScopedOccurrence.site)) &&
    decide (document.body.operationSites.Nodup) &&
    decide ((document.body.operationSites.map Prod.snd).Perm (List.range document.operations.length)) &&
    document.body.operationSites.all (scopedOperationSiteCheck fuel names result types principal requirements fields occurrences document)


def unitExpressions (unit : SourceUnit) : List (SourceSite × Expr) :=
  unit.declarations.flatMap fun declaration => match declaration with
    | .function source => source.expressions
    | .statement _ expressions _ => expressions

def scopedCallSites (unit : SourceUnit) : List SourceSite :=
  (unitExpressions unit).filterMap fun (site, expression) => match expression with
    | .call (.ident _) _ => some site
    | _ => none

def supportedScopedTopLevelCall (unit : SourceUnit) (site : SourceSite) (name : NameId) : Bool :=
  decide (((sourceFunctionIdentities unit).map Prod.fst).Nodup) &&
    unit.declarations.all (fun declaration => match declaration with
      | .function _ | .statement (.assert _ _) _ _ => true
      | _ => false) &&
    unit.declarations.any (fun declaration => match declaration with
      | .statement (.assert (.binary .eq (.call (.ident called) arguments) (.literal _)) none) expressions _ =>
        decide (called = name) && (literalCallArguments arguments).isSome &&
          expressions.any (fun (location, expression) => decide (location = site) && match expression with
            | .call (.ident selected) _ => decide (selected = name)
            | _ => false)
      | _ => false)

def supportedScopedCallSites (unit : SourceUnit) : Bool :=
  (unitExpressions unit).all fun (site, expression) => match expression with
    | .call (.ident name) _ => supportedScopedTopLevelCall unit site name
    | .call _ _ => false
    | _ => true

def scopedCallCheck (fuel : Nat) (unit : SourceUnit) (variables : VariableResolver)
    (body : ScopedBodyDocument) (scheme : Scheme) (functions : List FunctionTarget)
    (call : ScopedCallDocument) : Bool :=
  let projected : CallDocument := ⟨call.expression, call.site, call.role, call.target, call.typeArguments,
    call.operationBindings.filterMap fun binding => match binding with | .field => some .field | .store => none⟩
  match targetSourceIdentity unit call.target,
      (unitExpressions unit).find? (fun (site, _) => decide (site = call.site)),
      argumentsFromCall variables body.body projected with
  | some (name, targetSite), some (_, source@(.call (.ident called) sourceArguments)), some instantiation =>
    match literalCallArguments sourceArguments with
    | none => false
    | some arguments =>
      let binder : BinderId := ⟨⟨0, by decide⟩⟩
      let binding : Binding := ⟨name, binder, .immutable, scheme⟩
      supportedScopedTopLevelCall unit call.site name &&
        decide (called = name) && decide (targetSite = body.body.site) && decide (call.target ∈ functions) &&
        decide (call.role = .call) && decide (call.operationBindings = body.operations.map ScopedOperation.binding) &&
        match validatePublishedCall fuel [binding] identity name binder scheme source arguments instantiation with
        | .ok _ => true
        | .error _ => false
  | _, _, _ => false

def checksScopedCalls (fuel : Nat) (unit : SourceUnit) (variables : VariableResolver)
    (body : ScopedBodyDocument) (scheme : Scheme) (facts : ScopedFactsDocument) : Bool :=
  decide ((facts.calls.map ScopedCallDocument.site).Nodup) &&
    decide ((facts.calls.map ScopedCallDocument.site).Perm (scopedCallSites unit)) &&
    facts.calls.all (scopedCallCheck fuel unit variables body scheme facts.functions)

def validateScopedBodyFacts (fuel : Nat) (unit : SourceUnit) (source : SourceFunction)
    (variables : VariableResolver) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (document : ScopedBodyDocument) : Except ValidationError Substitution := do
  if globals.isEmpty then pure () else throw .unsupported
  requireCheck (decide (sourceName unit.names "Int" = some names.integer) &&
    decide (sourceName unit.names "Bool" = some names.boolean) &&
    decide (sourceName unit.names "Str" = some names.string) &&
    decide (sourceName unit.names "Result" = some names.result))
  requireCheck (decide (document.body.site = source.bodySite) && decide (document.body.kind = .pure) &&
    decide (source.kind = .pure) && decide (document.body.explicitReturn = source.result.isSome))
  let effects ← (classifyRegionFunctionEffects fuel unit source).mapError (fun _ => ValidationError.unsupported)
  requireCheck effects.isEmpty
  let some parameterNames := document.body.parameters.mapM (fun (name, _) => sourceNameResolver unit document.body.site.source name)
    | throw .mismatch
  requireCheck (decide (parameterNames = parameters.map Binding.name))
  let some scheme := schemeFromScopedBody variables document | throw .mismatch
  let principal ← validateQualifiedFunction fuel names parameters globals result source.body ordered steps
    solvedCount scheme candidate binderTypes fields inhabitant
  validateSourceParameters names principal source.parameters parameters
  validateSourceAnnotation names principal source.result (substitute principal result)
  let occurrences ← sourceScopedOccurrences fuel source parameters globals [] binderTypes candidate
  requireCheck (checksScopedQualifiedFacts fuel names (substitute principal result) binderTypes principal
    scheme.requirements fields source occurrences document.body &&
    checksScopedReadFacts unit.names source occurrences document.body &&
    checksScopedOperationFacts fuel names (substitute principal result) binderTypes principal
      scheme.requirements fields source occurrences document)
  pure principal

theorem validated_scoped_body_has_function_certificate (fuel : Nat) (unit : SourceUnit)
    (source : SourceFunction) (variables : VariableResolver) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType))
    (document : ScopedBodyDocument) (principal : Substitution)
    (accepted : validateScopedBodyFacts fuel unit source variables names parameters globals result ordered steps
      solvedCount candidate binderTypes fields inhabitant document = .ok principal) :
    ∃ scheme, schemeFromScopedBody variables document = some scheme ∧
      validateQualifiedFunction fuel names parameters globals result source.body ordered steps solvedCount
        scheme candidate binderTypes fields inhabitant = .ok principal := by
  unfold validateScopedBodyFacts at accepted
  cases globalEmpty : globals.isEmpty with
  | false => simp [globalEmpty, except_bind_ok] at accepted
  | true =>
    cases effectResult : classifyRegionFunctionEffects fuel unit source with
    | error failure => simp [globalEmpty, effectResult, Except.mapError, except_bind_ok] at accepted
    | ok effects =>
      cases parameterNames : document.body.parameters.mapM (fun (name, _) => sourceNameResolver unit document.body.site.source name) with
      | none => simp [globalEmpty, effectResult, parameterNames, except_bind_ok] at accepted
      | some decodedNames =>
        cases foundScheme : schemeFromScopedBody variables document with
        | none => simp [globalEmpty, effectResult, parameterNames, foundScheme, except_bind_ok] at accepted
        | some scheme =>
          simp [globalEmpty, effectResult, parameterNames, foundScheme, Except.mapError, except_bind_ok, except_map_ok, exists_require_ok] at accepted
          obtain ⟨_, _, _, _, selected, certified, _, _, _, _, _, same⟩ := accepted
          subst selected
          exact ⟨scheme, rfl, certified⟩

theorem validated_scoped_body_types_every_solution (fuel : Nat) (unit : SourceUnit)
    (source : SourceFunction) (variables : VariableResolver) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType))
    (document : ScopedBodyDocument) (principal : Substitution)
    (accepted : validateScopedBodyFacts fuel unit source variables names parameters globals result ordered steps
      solvedCount candidate binderTypes fields inhabitant document = .ok principal) :
    ∃ scheme, schemeFromScopedBody variables document = some scheme ∧
      ∀ later, WellSorted later → Solves scheme.requirements later →
        ∃ generated state,
          (block fuel names (parameters ++ globals) result source.body).run
            (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
          QualifiedScheme .generalize (globalEnvironment globals) (.function (parameterTypes parameters) result)
            state.constraints principal scheme ∧
          Solves state.constraints (compose later principal) ∧
          (declaredBinders candidate).Nodup ∧
          guaranteesFunctionResult candidate = true ∧
          WellSorted (witnessSubstitution inhabitant) ∧
          checksGroundScheme scheme inhabitant = true ∧
          Solves scheme.requirements (witnessSubstitution inhabitant) ∧
          BlockHasType names (compose later principal) (parameters ++ globals)
            (substitute (compose later principal) result) source.body := by
  obtain ⟨scheme, decoded, certified⟩ := validated_scoped_body_has_function_certificate fuel unit source variables
    names parameters globals result ordered steps solvedCount candidate binderTypes fields inhabitant document principal accepted
  refine ⟨scheme, decoded, ?_⟩
  intro later sorted solves
  exact qualified_function_types_every_solution fuel names parameters globals result source.body ordered steps solvedCount
    scheme candidate binderTypes fields principal later inhabitant certified sorted solves

def validateSourceScopedDecoded (fuel : Nat) (unit : SourceUnit) (facts : ScopedFactsDocument)
    (scopes : List (SourceSite × List Nat)) (declaration bodyIndex : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType)) :
    Except ValidationError Substitution := do
  let some (.function source) := unit.declarations[declaration]? | throw .mismatch
  requireCheck (checksSourceFunctions unit facts.functions)
  let some body := facts.bodies[bodyIndex]? | throw .mismatch
  let variables := scopedVariableResolver scopes
  let principal ← validateScopedBodyFacts fuel unit source variables names parameters globals result ordered
    steps solvedCount candidate binderTypes fields inhabitant body
  let some scheme := schemeFromScopedBody variables body | throw .mismatch
  if supportedScopedCallSites unit then pure () else throw .unsupported
  requireCheck (checksScopedCalls fuel unit variables body scheme facts)
  pure principal

def validateSourceScopedFacts (fuel : Nat) (sourceId : Nat) (text : String) (codes : ExchangeCodes)
    (declaration bodyIndex : Nat) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) : Except ValidationError Substitution := do
  let unit ← (admitSource sourceId text).mapError (fun _ => ValidationError.unsupported)
  let some (facts, scopes) := decodeScopedFactsDocument fuel (sourceNameResolver unit) codes | throw .mismatch
  validateSourceScopedDecoded fuel unit facts scopes declaration bodyIndex names parameters globals result ordered
    steps solvedCount candidate binderTypes fields inhabitant


end Xsh.Conformance.Connected
