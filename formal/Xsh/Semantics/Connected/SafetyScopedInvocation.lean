import Xsh.Semantics.Connected.SafetyScopedFixture
import Xsh.Semantics.Connected.SafetyInitialization
import Xsh.Semantics.Connected.SafetyValueBlockLocalProgress

namespace Xsh.Semantics.Connected

open Xsh.Foundation Xsh.Typing.Connected Xsh.Conformance.Connected

def scopedFixtureDeclaration : Xsh.Syntax.Foundation.DeclarationId := ⟨scopedFixtureInput⟩

def scopedInvocationMachine (base : Machine) (principal : Substitution) (value : Xsh.Values.IntValue) : Machine :=
  { booleanInitialMachine base with
    functions := fun declaration => if declaration = scopedFixtureDeclaration then some (.function (scopedFixtureFunction principal)) else none
    control := .invoke (.instantiatedFunction scopedFixtureDeclaration (.argument .integer .argumentsEmpty)
      .integer identity scopedFixtureInhabitant) [.scalar (.integer value)] }

def scopedInvocationContext : TypingContext :=
  ⟨[], fun _ => .ordinary, identity, identity, .primitive .integer, some (.primitive .integer), 0, ⟨.integer, rfl⟩⟩

theorem actual_scoped_invocation_typed (base : Machine) (value : Xsh.Values.IntValue) : ∃ principal,
    validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal ∧
    TypedMachine [] (fun _ => .ordinary) (fun _ => none) (scopedInvocationMachine base principal value) := by
  obtain ⟨principal, accepted⟩ := scoped_fixture_accepted
  have certified := scoped_fixture_function_certified (scopedInvocationMachine base principal value) principal accepted
  refine ⟨principal, accepted, ?_⟩
  constructor
  · constructor
    · constructor
      · exact Nat.zero_lt_succ 0
      · intro frame member
        simp [scopedInvocationMachine, booleanInitialMachine] at member
      · intro cell entry present
        simp [scopedInvocationMachine, booleanInitialMachine] at present
    · constructor
      · intro binder ty found
        cases found
      · intro binder cell found
        simp [scopedInvocationMachine, booleanInitialMachine, emptyCells] at found
  · intro declaration function found
    simp only [scopedInvocationMachine, booleanInitialMachine] at found
    split at found
    · cases found
      exact certified
    · contradiction
  · intro first second cell found
    simp [scopedInvocationMachine, booleanInitialMachine, emptyCells] at found
  · intro binder declaration scheme found
    simp [Connected.read, scopedInvocationMachine, booleanInitialMachine, emptyCells] at found
  · constructor
    · intro binder declaration scheme found
      simp [Connected.read, scopedInvocationMachine, booleanInitialMachine, emptyCells] at found
    · intro binder found
      cases found
  · intro cell entry present
    simp [scopedInvocationMachine, booleanInitialMachine] at present
  · refine ⟨.expression scopedInvocationContext (.primitive .integer),
      .expression scopedInvocationContext (.primitive .integer), rfl, rfl, rfl,
      identity_well_sorted, identity_well_sorted, ?_, ⟨.integer, rfl⟩, ⟨.integer, rfl⟩, ?_, .nil, ?_⟩
    · refine ⟨?_, ?_, ?_, ?_, ?_⟩
      · refine ⟨by simp [scopedInvocationContext, TypedDemand.context], ?_, ?_, ?_, ?_⟩
        · intro name binding found
          simp [scopedInvocationContext, TypedDemand.context] at found
        · intro name binding found
          simp [scopedInvocationContext, TypedDemand.context] at found
        · intro first second cell found
          simp [scopedInvocationMachine, booleanInitialMachine, emptyCells] at found
        · intro binder cell found
          simp [scopedInvocationMachine, booleanInitialMachine, emptyCells] at found
      · intro name binding cell entry found
        simp [scopedInvocationContext, TypedDemand.context] at found
      · intro name binding identity row found
        simp [scopedInvocationContext, TypedDemand.context] at found
      · intro binder declaration scheme found
        simp [Connected.read, scopedInvocationMachine, booleanInitialMachine, emptyCells] at found
      · constructor
        · intro binder declaration scheme found
          simp [Connected.read, scopedInvocationMachine, booleanInitialMachine, emptyCells] at found
        · intro binder found
          cases found
    · apply ControlTyped.invoke rfl
      · apply ValueCallables.instantiatedFunction (function := scopedFixtureFunction principal) (normalized := scopedFixtureInhabitant)
        · simp [scopedInvocationMachine]
        · exact identity_well_sorted
        · simpa [scopedFixtureFunction] using scoped_selected_instantiates
      · exact .cons (by simp [HasType, valueType, typeFits]) .scalar .nil
      · rfl
    · constructor
      · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, scopedInvocationMachine, booleanInitialMachine]
      · intro binding member
        simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, scopedInvocationMachine, booleanInitialMachine] at member


def scopedFixtureArgument : Xsh.Values.IntValue :=
  ⟨7, by unfold Xsh.Values.signed64Min Xsh.Values.signed64Limit; omega⟩

theorem actual_scoped_invocation_execution_safe (base : Machine) (response : Xsh.Values.IntValue) :
    ∃ principal call,
      admitSource 0 actualScopedSource = .ok scopedUnit ∧
      decodeScopedFactsDocument 128 (sourceNameResolver scopedUnit) scopedFactsCodes = some (scopedDocument, scopedScopes) ∧
      scopedDocument.calls[0]? = some call ∧
      argumentsFromCall (scopedVariableResolver scopedScopes) scopedBodyDocument
        ⟨call.expression, call.site, call.role, call.target, call.typeArguments,
          call.operationBindings.filterMap (fun binding => match binding with | .field => some .field | .store => none)⟩ =
        some scopedFixtureInhabitant ∧
      validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
        scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
        scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal ∧
      TypedMachine [] (fun _ => .ordinary) (fun _ => none) (scopedInvocationMachine base principal scopedFixtureArgument) ∧
      (∀ final events fault, Execution (scopedInvocationMachine base principal scopedFixtureArgument) events final →
        final.control = .fault fault → ∃ issue, fault = .integer issue) ∧
      (∀ final events, Execution (scopedInvocationMachine base principal scopedFixtureArgument) events final →
        MachineTerminal final ∨ ∃ event next, Step final event next ∧ PermittedControl next.control) := by
  obtain ⟨principal, accepted, typed⟩ := actual_scoped_invocation_typed base scopedFixtureArgument
  obtain ⟨call, callAt, callArgs⟩ := scoped_fixture_selected_arguments
  refine ⟨principal, call, actual_scoped_source_is_admitted, scoped_document_decodes, callAt, callArgs, accepted, typed, ?_, ?_⟩
  · intro final events fault execution failed
    exact typed_execution_fault_is_integer [] (fun _ => .ordinary) (fun _ => none) response execution typed fault failed
  · intro final events execution
    exact typed_machine_safe_progress final [] (fun _ => .ordinary) (fun _ => none) response
      (typed_machine_execution_preservation [] (fun _ => .ordinary) (fun _ => none) response execution typed)

end Xsh.Semantics.Connected
