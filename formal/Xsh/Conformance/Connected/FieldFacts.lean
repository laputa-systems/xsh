import Xsh.Typing.Connected.Qualified

namespace Xsh.Conformance.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Typing.Connected

structure SourceSite where
  source : Nat
  start : Nat
  length : Nat
  deriving DecidableEq

structure FieldOperation where
  site : SourceSite
  receiver : MonoType
  name : NameId
  result : MonoType
  deriving DecidableEq

def FieldOperation.constraint (operation : FieldOperation) : Constraint :=
  .field operation.receiver operation.name operation.result

inductive OperationBinding where
  | field
  | other
  deriving DecidableEq

def checksFieldFacts (expected : List FieldOperation) (requirements : List Constraint)
    (operations : List FieldOperation) (bindings : List OperationBinding) : Bool :=
  decide (operations.Perm expected) &&
    decide ((operations.map FieldOperation.constraint).Perm requirements) &&
    decide (bindings = operations.map (fun _ => OperationBinding.field))

theorem checked_field_facts_hold (expected : List FieldOperation) (requirements : List Constraint)
    (operations : List FieldOperation) (bindings : List OperationBinding)
    (solution : Substitution) (checked : checksFieldFacts expected requirements operations bindings = true)
    (solves : Solves requirements solution) :
    operations.Perm expected ∧
      bindings = operations.map (fun _ => OperationBinding.field) ∧
      ∀ operation ∈ operations, satisfies solution operation.constraint := by
  simp [checksFieldFacts] at checked
  refine ⟨checked.1.1, checked.2, ?_⟩
  intro operation member
  apply solves
  apply checked.1.2.mem_iff.mp
  exact List.mem_map.mpr ⟨operation, member, rfl⟩

def validateFieldFunctionInstance (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (scheme : Scheme) (arguments : List (Variable × MonoType)) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (expected : List FieldOperation) (operations : List FieldOperation)
    (bindings : List OperationBinding) : Except ValidationError Substitution := do
  requireCheck (checksFieldFacts expected scheme.requirements operations bindings)
  validateFiniteFunctionInstance fuel names parameters globals result body ordered steps solvedCount
    scheme arguments candidate binderTypes

theorem field_function_instance_sound (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (scheme : Scheme) (arguments : List (Variable × MonoType)) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (expected : List FieldOperation) (operations : List FieldOperation)
    (bindings : List OperationBinding) (principal : Substitution)
    (accepted : validateFieldFunctionInstance fuel names parameters globals result body ordered steps
      solvedCount scheme arguments candidate binderTypes expected operations bindings = .ok principal) :
    ∃ generated state,
      (block fuel names (parameters ++ globals) result body).run
        (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
      QualifiedScheme .generalize (globalEnvironment globals)
        (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
      Solves state.constraints (compose (witnessSubstitution arguments) principal) ∧
      BlockHasType names (compose (witnessSubstitution arguments) principal) (parameters ++ globals)
        (substitute (compose (witnessSubstitution arguments) principal) result) body ∧
      operations.Perm expected ∧
      bindings = operations.map (fun _ => OperationBinding.field) ∧
      ∀ operation ∈ operations, satisfies (witnessSubstitution arguments) operation.constraint := by
  simp [validateFieldFunctionInstance, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨facts, checked⟩ := accepted
  have typed := finite_function_instance_sound fuel names parameters globals result body ordered steps
    solvedCount scheme arguments candidate binderTypes principal checked
  obtain ⟨generated, state, generatedBody, qualified, solved, bodyTyped⟩ := typed
  have requirements : Solves scheme.requirements (witnessSubstitution arguments) := by
    have principalSorted := qualified.1.1
    have argumentsSorted : WellSorted (witnessSubstitution arguments) := by
      simp [validateFiniteFunctionInstance, except_bind_ok, exists_require_ok] at checked
      exact finite_substitution_is_well_sorted arguments checked.1
    exact (qualified.1.2.1 _ argumentsSorted).mp solved
  have fields := checked_field_facts_hold expected scheme.requirements operations bindings
    (witnessSubstitution arguments) facts requirements
  exact ⟨generated, state, generatedBody, qualified, solved, bodyTyped, fields⟩

def projectionSite : SourceSite := ⟨0, 22, 12⟩

def projectionOperation : FieldOperation :=
  ⟨projectionSite, .variable generatedParameterVariable, generatedProjectionField,
    .variable generatedReturnVariable⟩

def additionalField : NameId := exampleName 42 (by decide)

def projectionFieldsAccepted (operation : FieldOperation) (binding : OperationBinding)
    (site : SourceSite) (row : MonoType) : Bool :=
  match validateFieldFunctionInstance 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedReturnVariable) generatedProjectionBody generatedProjectionConstraints [.bindLeft] 1
      generatedProjectionScheme [(generatedParameterVariable, .record row),
        (generatedReturnVariable, .primitive .integer)] generatedProjectionCandidate (fun _ => none)
      [{ projectionOperation with site := site }] [operation] [binding] with
  | .ok _ => true
  | _ => false

theorem field_facts_preserve_extra_fields_and_reject_mutations :
    let row := MonoType.rowField generatedProjectionField (.primitive .integer)
      (.rowField additionalField (.primitive .boolean) .rowEmpty)
    projectionFieldsAccepted projectionOperation .field projectionSite row = true ∧
    projectionFieldsAccepted { projectionOperation with name := additionalField } .field projectionSite row = false ∧
    projectionFieldsAccepted projectionOperation .other projectionSite row = false ∧
    projectionFieldsAccepted projectionOperation .field ⟨0, 23, 12⟩ row = false ∧
    projectionFieldsAccepted projectionOperation .field projectionSite
      (.rowField additionalField (.primitive .boolean) .rowEmpty) = false := by
  dsimp
  constructor <;> decide +kernel

theorem field_sites_remain_paired_with_operations :
    let first := projectionOperation
    let second := { projectionOperation with site := ⟨0, 40, 12⟩, name := additionalField }
    let swappedFirst := { first with site := second.site }
    let swappedSecond := { second with site := first.site }
    checksFieldFacts [first, second] [first.constraint, second.constraint]
      [first, second] [.field, .field] = true ∧
    checksFieldFacts [first, second] [first.constraint, second.constraint]
      [swappedFirst, swappedSecond] [.field, .field] = false := by
  dsimp
  constructor <;> decide +kernel

end Xsh.Conformance.Connected
