import Xsh.Conformance.Connected.FactsDocument

namespace Xsh.Conformance.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Typing.Connected

private def functionArguments (parameters : List (String × MonoType)) : MonoType :=
  parameters.foldr (fun parameter rest => .argument parameter.2 rest) .argumentsEmpty

def schemeFromBody (variables : VariableResolver) (body : BodyDocument) : Option Scheme := do
  let quantified ← body.typeParameters.mapM (variables body.site)
  if body.typeParameters.Nodup ∧ quantified.Nodup ∧
      quantified.all (fun typeVariable => decide (typeVariable.kind = .value)) then pure () else none
  pure ⟨quantified, .function (functionArguments body.parameters) body.result,
    body.operations.map FieldOperation.constraint⟩

def argumentsFromCall (variables : VariableResolver) (body : BodyDocument)
    (call : CallDocument) : Option (List (Variable × MonoType)) := do
  if (call.typeArguments.map Prod.fst).Nodup ∧
      (call.typeArguments.map Prod.fst).Perm body.typeParameters then pure () else none
  call.typeArguments.mapM fun (id, ty) => do
    let typeVariable ← variables body.site id
    pure (typeVariable, ty)

def validateDecodedFieldInstance (fuel : Nat) (names : NameResolver) (variables : VariableResolver)
    (typeNames : TypeNames) (parameters globals : List Binding) (result : MonoType)
    (source : List Statement) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (expectedTarget : FunctionTarget)
    (expectedOperations : List FieldOperation) (body : BodyDocument) (call : CallDocument) :
    Except ValidationError Substitution := do
  requireCheck (decide (body.kind = .pure) && decide (call.role = .call) &&
    decide (call.target = expectedTarget) && decide (call.target.body = body.site))
  let some parameterNames := body.parameters.mapM (fun (name, _) => names body.site.source name)
    | throw .mismatch
  requireCheck (decide (parameterNames = parameters.map Binding.name))
  let some scheme := schemeFromBody variables body | throw .mismatch
  let some arguments := argumentsFromCall variables body call | throw .mismatch
  validateFieldFunctionInstance fuel typeNames parameters globals result source ordered steps solvedCount
    scheme arguments candidate binderTypes expectedOperations body.operations call.operationBindings

theorem decoded_field_instance_has_typed_body (fuel : Nat) (names : NameResolver)
    (variables : VariableResolver) (typeNames : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (source : List Statement) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (expectedTarget : FunctionTarget) (expectedOperations : List FieldOperation)
    (body : BodyDocument) (call : CallDocument) (principal : Substitution)
    (accepted : validateDecodedFieldInstance fuel names variables typeNames parameters globals result source
      ordered steps solvedCount candidate binderTypes expectedTarget expectedOperations body call = .ok principal) :
    ∃ scheme arguments generated state,
      schemeFromBody variables body = some scheme ∧ argumentsFromCall variables body call = some arguments ∧
      call.target = expectedTarget ∧ call.target.body = body.site ∧
      (block fuel typeNames (parameters ++ globals) result source).run
        (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
      QualifiedScheme .generalize (globalEnvironment globals)
        (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
      Solves state.constraints (compose (witnessSubstitution arguments) principal) ∧
      BlockHasType typeNames (compose (witnessSubstitution arguments) principal) (parameters ++ globals)
        (substitute (compose (witnessSubstitution arguments) principal) result) source ∧
      body.operations.Perm expectedOperations ∧
      call.operationBindings = body.operations.map (fun _ => OperationBinding.field) ∧
      ∀ operation ∈ body.operations, satisfies (witnessSubstitution arguments) operation.constraint := by
  unfold validateDecodedFieldInstance at accepted
  cases parameterNames : body.parameters.mapM (fun (name, _) => names body.site.source name) with
  | none => simp [parameterNames, except_bind_ok, exists_require_ok] at accepted
  | some decodedNames =>
    cases foundScheme : schemeFromBody variables body with
    | none => simp [parameterNames, foundScheme, except_bind_ok, exists_require_ok] at accepted
    | some scheme =>
      cases foundArguments : argumentsFromCall variables body call with
      | none => simp [parameterNames, foundScheme, foundArguments, except_bind_ok, exists_require_ok] at accepted
      | some arguments =>
        simp [parameterNames, foundScheme, foundArguments, except_bind_ok, exists_require_ok,
          Bool.and_eq_true] at accepted
        obtain ⟨⟨⟨⟨kind, role⟩, target⟩, bodySite⟩, namesMatch, checked⟩ := accepted
        obtain ⟨generated, state, generatedBody, qualified, solved, typed, operations, bindings, fields⟩ :=
          field_function_instance_sound fuel typeNames parameters globals result source ordered steps solvedCount
            scheme arguments candidate binderTypes expectedOperations body.operations call.operationBindings principal checked
        exact ⟨scheme, arguments, generated, state, rfl, rfl, target, bodySite, generatedBody,
          qualified, solved, typed, operations, bindings, fields⟩

private def testSite : SourceSite := ⟨0, 20, 16⟩
private def testTarget : FunctionTarget := ⟨0, "project", none, testSite⟩
private def testNames : NameResolver := fun _ name =>
  if name = "value" then some generatedProjectionName else none
private def testVariables : VariableResolver := fun _ id => some ⟨id, .value⟩
private def testBody : BodyDocument :=
  ⟨testSite, .pure, [0, 1], [("value", .variable generatedParameterVariable)],
    .variable generatedReturnVariable, false, .unknown, false, [projectionOperation], [], [], [], [], [], []⟩
private def testCall : CallDocument :=
  ⟨0, ⟨0, 55, 20⟩, .call, testTarget,
    [(0, .record (.rowField generatedProjectionField (.primitive .integer)
      (.rowField additionalField (.primitive .boolean) .rowEmpty))),
     (1, .primitive .integer)], [.field]⟩
private def testValidation (body : BodyDocument) (call : CallDocument) : Bool :=
  match validateDecodedFieldInstance 24 testNames testVariables exampleTypeNames
      [generatedProjectionParameter] [] (.variable generatedReturnVariable) generatedProjectionBody
      generatedProjectionConstraints [.bindLeft] 1 generatedProjectionCandidate (fun _ => none)
      testTarget [projectionOperation] body call with
  | .ok _ => true
  | _ => false

theorem decoded_field_scheme_accepts_extra_fields_and_rejects_mutations :
    testValidation testBody testCall = true ∧
    testValidation { testBody with operations := [{ projectionOperation with name := additionalField }] } testCall = false ∧
    testValidation testBody { testCall with role := .value } = false ∧
    testValidation testBody { testCall with operationBindings := [.other] } = false ∧
    testValidation testBody { testCall with target := { testTarget with name := "other" } } = false ∧
    testValidation { testBody with typeParameters := [0] } testCall = false := by
  constructor <;> decide +kernel

end Xsh.Conformance.Connected
