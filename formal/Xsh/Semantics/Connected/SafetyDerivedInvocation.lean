import Xsh.Semantics.Connected.SafetyScopedInvocation

namespace Xsh.Semantics.Connected

open Xsh.Foundation Xsh.Syntax Xsh.Syntax.Foundation Xsh.Typing.Connected Xsh.Conformance.Connected

inductive LiteralScalar where
  | integer (value : Xsh.Values.IntValue)
  | boolean (value : Bool)
  deriving DecidableEq

def LiteralScalar.value : LiteralScalar → Value
  | .integer value => .scalar (.integer value)
  | .boolean value => .scalar (.boolean value)

def literalScalar : Expr → Option LiteralScalar
  | .literal (.integer text) => (parseInteger text).map .integer
  | .literal (.bool value) => some (.boolean value)
  | _ => none

def literalScalars : List CallArgument → Option (List LiteralScalar)
  | [] => some []
  | .positional value :: rest => do
      let first ← literalScalar value
      let others ← literalScalars rest
      pure (first :: others)
  | _ => none

def scalarsFit : RuntimeType → List LiteralScalar → Bool
  | .argumentsEmpty, [] => true
  | .argument ty rest, scalar :: scalars => decide (HasType ty scalar.value) && scalarsFit rest scalars
  | _, _ => false

structure SelectedInvocation where
  declaration : DeclarationId
  parameters : RuntimeType
  result : RuntimeType
  typeArguments : List (Variable × MonoType)
  arguments : List LiteralScalar
  deriving DecidableEq

def projectScopedCall (call : ScopedCallDocument) : CallDocument :=
  ⟨call.expression, call.site, call.role, call.target, call.typeArguments,
    call.operationBindings.filterMap fun binding => match binding with | .field => some .field | .store => none⟩

def selectInvocation (scheme : Scheme) (declaration : DeclarationId)
    (typeArguments : List (Variable × MonoType)) (sourceArguments : List CallArgument) :
    Option SelectedInvocation := do
  let arguments ← literalScalars sourceArguments
  let (parameters, result, _) ← instantiateFunction identity scheme typeArguments
  if scalarsFit parameters arguments then pure () else none
  pure ⟨declaration, parameters, result, typeArguments, arguments⟩

def deriveSelectedInvocation (unit : SourceUnit) (facts : ScopedFactsDocument)
    (scopes : List (SourceSite × List Nat)) (bodyIndex callIndex : Nat) : Option SelectedInvocation := do
  let call ← facts.calls[callIndex]?
  let body ← facts.bodies[bodyIndex]?
  let variables := scopedVariableResolver scopes
  let scheme ← schemeFromScopedBody variables body
  let typeArguments ← argumentsFromCall variables body.body (projectScopedCall call)
  let (name, targetSite) ← targetSourceIdentity unit call.target
  let binder ← BinderId.decode? call.target.definition
  match (unitExpressions unit).find? (fun (site, _) => decide (site = call.site)) with
  | some (_, .call (.ident called) sourceArguments) =>
      if called = name ∧ targetSite = body.body.site ∧ call.role = .call ∧ call.target ∈ facts.functions then
        selectInvocation scheme ⟨binder⟩ typeArguments sourceArguments
      else none
  | _ => none

def invocationMachine (base : Machine) (selected : SelectedInvocation) (function : Function) : Machine :=
  { booleanInitialMachine base with
    functions := fun declaration => if declaration = selected.declaration then some (.function function) else none
    control := .invoke (.instantiatedFunction selected.declaration selected.parameters selected.result identity
      selected.typeArguments) (selected.arguments.map LiteralScalar.value) }

theorem scalars_fit_runtime_arguments (functions : FunctionTable) (parameters : RuntimeType)
    (scalars : List LiteralScalar) (fit : scalarsFit parameters scalars = true) :
    RuntimeArguments functions (scalars.map LiteralScalar.value) parameters := by
  induction scalars generalizing parameters with
  | nil =>
    cases parameters <;> simp [scalarsFit] at fit
    exact .nil
  | cons scalar rest ih =>
    cases parameters <;> simp [scalarsFit] at fit
    cases scalar with
    | integer value => exact .cons fit.1 .scalar (ih _ fit.2)
    | boolean value => exact .cons fit.1 .scalar (ih _ fit.2)

theorem select_invocation_sound (scheme : Scheme) (declaration : DeclarationId)
    (typeArguments : List (Variable × MonoType)) (sourceArguments : List CallArgument)
    (selected : SelectedInvocation)
    (derived : selectInvocation scheme declaration typeArguments sourceArguments = some selected) :
    selected.declaration = declaration ∧ selected.typeArguments = typeArguments ∧
    literalScalars sourceArguments = some selected.arguments ∧
    (∃ normalized, instantiateFunction identity scheme typeArguments =
      some (selected.parameters, selected.result, normalized)) ∧
    scalarsFit selected.parameters selected.arguments = true := by
  unfold selectInvocation at derived
  cases literal : literalScalars sourceArguments with
  | none => simp [literal] at derived
  | some arguments =>
    cases instantiated : instantiateFunction identity scheme typeArguments with
    | none => simp [literal, instantiated] at derived
    | some triple =>
      obtain ⟨parameters, result, normalized⟩ := triple
      simp only [literal, instantiated] at derived
      by_cases fit : scalarsFit parameters arguments = true
      · simp [fit] at derived
        subst selected
        exact ⟨rfl, rfl, rfl, ⟨normalized, rfl⟩, fit⟩
      · simp [fit] at derived

theorem invocation_machine_typed (base : Machine) (selected : SelectedInvocation) (function : Function)
    (normalized : List (Variable × MonoType))
    (instantiated : instantiateFunction identity function.scheme selected.typeArguments =
      some (selected.parameters, selected.result, normalized))
    (fit : scalarsFit selected.parameters selected.arguments = true)
    (certified : FunctionBodyCertified [] (fun _ => .ordinary) (invocationMachine base selected function) function) :
    TypedMachine [] (fun _ => .ordinary) (fun _ => none) (invocationMachine base selected function) := by
  obtain ⟨sourceParameters, sourceResult, shape, parametersClosed, resultClosed⟩ :=
    instantiate_function_components identity function.scheme selected.typeArguments selected.parameters selected.result
      normalized instantiated
  let context : TypingContext := ⟨[], fun _ => .ordinary, identity, identity, sourceResult, some sourceResult, 0,
    ⟨selected.result, resultClosed⟩⟩
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · refine ⟨⟨Nat.zero_lt_succ 0, ?_, ?_⟩, ?_, ?_⟩
    · intro frame member
      simp [invocationMachine, booleanInitialMachine] at member
    · intro cell entry present
      simp [invocationMachine, booleanInitialMachine] at present
    · intro binder ty found
      cases found
    · intro binder cell found
      simp [invocationMachine, booleanInitialMachine, emptyCells] at found
  · intro declaration candidate found
    simp only [invocationMachine, booleanInitialMachine] at found
    split at found
    · cases found
      exact certified
    · contradiction
  · intro first second cell found
    simp [invocationMachine, booleanInitialMachine, emptyCells] at found
  · intro binder declaration scheme found
    simp [Connected.read, invocationMachine, booleanInitialMachine, emptyCells] at found
  · refine ⟨?_, ?_⟩
    · intro binder declaration scheme found
      simp [Connected.read, invocationMachine, booleanInitialMachine, emptyCells] at found
    · intro binder found
      cases found
  · intro cell entry present
    simp [invocationMachine, booleanInitialMachine] at present
  · refine ⟨.expression context sourceResult, .expression context sourceResult, rfl, rfl, rfl,
      identity_well_sorted, identity_well_sorted, ?_, ⟨selected.result, resultClosed⟩,
      ⟨selected.result, resultClosed⟩, ?_, .nil, ?_⟩
    · refine ⟨?_, ?_, ?_, ?_, ?_⟩
      · refine ⟨by simp [context, TypedDemand.context], ?_, ?_, ?_, ?_⟩
        · intro name binding found
          simp [context, TypedDemand.context] at found
        · intro name binding found
          simp [context, TypedDemand.context] at found
        · intro first second cell found
          simp [invocationMachine, booleanInitialMachine, emptyCells] at found
        · intro binder cell found
          simp [invocationMachine, booleanInitialMachine, emptyCells] at found
      · intro name binding cell entry found
        simp [context, TypedDemand.context] at found
      · intro name binding identity row found
        simp [context, TypedDemand.context] at found
      · intro binder declaration scheme found
        simp [Connected.read, invocationMachine, booleanInitialMachine, emptyCells] at found
      · refine ⟨?_, ?_⟩
        · intro binder declaration scheme found
          simp [Connected.read, invocationMachine, booleanInitialMachine, emptyCells] at found
        · intro binder found
          cases found
    · apply ControlTyped.invoke rfl
      · apply ValueCallables.instantiatedFunction (function := function) (normalized := normalized)
        · simp [invocationMachine]
        · exact identity_well_sorted
        · exact instantiated
      · exact scalars_fit_runtime_arguments _ _ _ fit
      · exact resultClosed
    · refine ⟨?_, ?_⟩
      · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, invocationMachine, booleanInitialMachine]
      · intro binding member
        simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, invocationMachine, booleanInitialMachine] at member

theorem derive_selected_invocation_sound (unit : SourceUnit) (facts : ScopedFactsDocument)
    (scopes : List (SourceSite × List Nat)) (bodyIndex callIndex : Nat) (selected : SelectedInvocation)
    (derived : deriveSelectedInvocation unit facts scopes bodyIndex callIndex = some selected) :
    ∃ call body scheme name sourceArguments,
      facts.calls[callIndex]? = some call ∧ facts.bodies[bodyIndex]? = some body ∧
      schemeFromScopedBody (scopedVariableResolver scopes) body = some scheme ∧
      argumentsFromCall (scopedVariableResolver scopes) body.body (projectScopedCall call) = some selected.typeArguments ∧
      targetSourceIdentity unit call.target = some (name, body.body.site) ∧
      BinderId.decode? call.target.definition = some selected.declaration.binder ∧
      call.role = .call ∧ call.target ∈ facts.functions ∧
      (unitExpressions unit).find? (fun (site, _) => decide (site = call.site)) =
        some (call.site, .call (.ident name) sourceArguments) ∧
      literalScalars sourceArguments = some selected.arguments ∧
      (∃ normalized, instantiateFunction identity scheme selected.typeArguments =
        some (selected.parameters, selected.result, normalized)) ∧
      scalarsFit selected.parameters selected.arguments = true := by
  unfold deriveSelectedInvocation at derived
  cases callAt : facts.calls[callIndex]? with
  | none => simp [callAt] at derived
  | some call =>
  cases bodyAt : facts.bodies[bodyIndex]? with
  | none => simp [callAt, bodyAt] at derived
  | some body =>
  cases schemeAt : schemeFromScopedBody (scopedVariableResolver scopes) body with
  | none => simp [callAt, bodyAt, schemeAt] at derived
  | some scheme =>
  cases argumentsAt : argumentsFromCall (scopedVariableResolver scopes) body.body (projectScopedCall call) with
  | none => simp [callAt, bodyAt, schemeAt, argumentsAt] at derived
  | some typeArguments =>
  cases identityAt : targetSourceIdentity unit call.target with
  | none => simp [callAt, bodyAt, schemeAt, argumentsAt, identityAt] at derived
  | some identityPair =>
  obtain ⟨name, targetSite⟩ := identityPair
  cases binderAt : BinderId.decode? call.target.definition with
  | none => simp [callAt, bodyAt, schemeAt, argumentsAt, identityAt, binderAt] at derived
  | some binder =>
  cases found : (unitExpressions unit).find? (fun (site, _) => decide (site = call.site)) with
  | none => simp [callAt, bodyAt, schemeAt, argumentsAt, identityAt, binderAt, found] at derived
  | some entry =>
    obtain ⟨site, source⟩ := entry
    have sameSite : site = call.site := by
      have foundSite := List.find?_some found
      simpa using foundSite
    subst site
    cases source with
    | call callee sourceArguments =>
      cases callee with
      | ident called =>
        by_cases checks : called = name ∧ targetSite = body.body.site ∧ call.role = .call ∧ call.target ∈ facts.functions
        · simp [callAt, bodyAt, schemeAt, argumentsAt, identityAt, binderAt, found] at derived
          obtain ⟨declaration, typeEq, literal, instantiated, fit⟩ := select_invocation_sound scheme ⟨binder⟩ typeArguments
            sourceArguments selected derived.2
          obtain ⟨calledEq, siteEq, role, member⟩ := checks
          subst called
          subst targetSite
          refine ⟨call, body, scheme, name, sourceArguments, rfl, rfl, schemeAt, ?_, identityAt, ?_, role,
            member, found, literal, by rw [typeEq]; exact instantiated, fit⟩
          · rw [typeEq]
            exact argumentsAt
          · rw [declaration]
            exact binderAt
        · simp [callAt, bodyAt, schemeAt, argumentsAt, identityAt, binderAt, found, checks] at derived
      | _ => simp [callAt, bodyAt, schemeAt, argumentsAt, identityAt, binderAt, found] at derived
    | _ => simp [callAt, bodyAt, schemeAt, argumentsAt, identityAt, binderAt, found] at derived

theorem derived_invocation_machine_typed (base : Machine) (unit : SourceUnit) (facts : ScopedFactsDocument)
    (scopes : List (SourceSite × List Nat)) (bodyIndex callIndex : Nat) (selected : SelectedInvocation)
    (function : Function)
    (derived : deriveSelectedInvocation unit facts scopes bodyIndex callIndex = some selected)
    (published : ∀ body, facts.bodies[bodyIndex]? = some body →
      schemeFromScopedBody (scopedVariableResolver scopes) body = some function.scheme)
    (certified : FunctionBodyCertified [] (fun _ => .ordinary) (invocationMachine base selected function) function) :
    TypedMachine [] (fun _ => .ordinary) (fun _ => none) (invocationMachine base selected function) := by
  obtain ⟨call, body, scheme, name, sourceArguments, callAt, bodyAt, schemeAt, _, _, _, _, _, _, _,
    ⟨normalized, instantiated⟩, fit⟩ := derive_selected_invocation_sound unit facts scopes bodyIndex callIndex selected derived
  have same : scheme = function.scheme := by
    have := published body bodyAt
    rw [schemeAt] at this
    exact Option.some.inj this
  subst scheme
  exact invocation_machine_typed base selected function normalized instantiated fit certified

theorem derived_invocation_execution_safe (base : Machine) (unit : SourceUnit) (facts : ScopedFactsDocument)
    (scopes : List (SourceSite × List Nat)) (bodyIndex callIndex : Nat) (selected : SelectedInvocation)
    (function : Function) (response : Xsh.Values.IntValue)
    (derived : deriveSelectedInvocation unit facts scopes bodyIndex callIndex = some selected)
    (published : ∀ body, facts.bodies[bodyIndex]? = some body →
      schemeFromScopedBody (scopedVariableResolver scopes) body = some function.scheme)
    (certified : FunctionBodyCertified [] (fun _ => .ordinary) (invocationMachine base selected function) function) :
    (∀ final events fault, Execution (invocationMachine base selected function) events final →
      final.control = .fault fault → ∃ issue, fault = .integer issue) ∧
    (∀ final events, Execution (invocationMachine base selected function) events final →
      MachineTerminal final ∨ ∃ event next, Step final event next ∧ PermittedControl next.control) := by
  have typed := derived_invocation_machine_typed base unit facts scopes bodyIndex callIndex selected function derived
    published certified
  refine ⟨?_, ?_⟩
  · intro final events fault execution failed
    exact typed_execution_fault_is_integer [] (fun _ => .ordinary) (fun _ => none) response execution typed fault failed
  · intro final events execution
    exact typed_machine_safe_progress final [] (fun _ => .ordinary) (fun _ => none) response
      (typed_machine_execution_preservation [] (fun _ => .ordinary) (fun _ => none) response execution typed)

end Xsh.Semantics.Connected
