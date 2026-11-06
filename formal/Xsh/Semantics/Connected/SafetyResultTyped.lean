import Xsh.Semantics.Connected.SafetyResultFunction
import Xsh.Semantics.Connected.SafetyReadGrounding
import Xsh.Semantics.Connected.SafetyScopeStorage
import Xsh.Semantics.Connected.SafetyScopedFixture
import Xsh.Semantics.Connected.SafetyInitialization
import Xsh.Semantics.Connected.SafetyValueBlockLocalProgress

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Values
open Xsh.Syntax.Foundation
open Xsh.Typing.Connected
open Xsh.Conformance.Connected

def callResultType : MonoType := .result (.primitive .integer) (.primitive .string)

def programStorage : BindingStorageMap := fun binder =>
  if binder = resultOkBinder ∨ binder = wrapBinder ∨ binder = liftBinder then .scheme else .ordinary

theorem ok_read_typed (solution : Substitution) (scope : Scope) (context : Option MonoType)
    (found : scope.find? (fun item => decide (item.name = callOkBinding.name)) = some callOkBinding) :
    NodeHasTypeIn solution context scope (.read resultOkBinder resultInstance)
      (.function (.argument (.primitive .integer) .argumentsEmpty) callResultType) := by
  have different : ((⟨1, .value⟩ : Variable) == ⟨0, .value⟩) = false := by decide
  have typed := NodeHasTypeIn.read (solution := solution) (context := context) (arguments := resultInstance)
    (ty := substitute (schemeSubstitution okScheme (witnessSubstitution resultInstance)) okScheme.body)
    (requirements := []) found (by decide) (by decide)
    (by simp [instantiationSortsValid, resultInstance, substitute, checksWellFormed, typeSort,
      identityVariable, resultValueVariable, VariableKind.sort])
    ⟨rfl, by simp [callOkBinding, okScheme]⟩ (by simp [Solves])
    (by simp [okScheme, schemeSubstitution, witnessSubstitution, resultInstance, substitute, WellFormed, typeSort,
      different, identityVariable, resultValueVariable, List.lookup])
    (by simp [okScheme, schemeSubstitution, witnessSubstitution, resultInstance, substitute, typeSort,
      different, identityVariable, resultValueVariable, List.lookup])
  simpa [okScheme, schemeSubstitution, witnessSubstitution, resultInstance, substitute, identityVariable,
    resultValueVariable, List.lookup, callResultType, callOkBinding, different] using typed

theorem monomorphic_read_typed (solution : Substitution) (scope : Scope) (context : Option MonoType)
    (binding : Binding) (raw : MonoType) (closed : ∀ other : Substitution, substitute other raw = raw)
    (shape : binding.scheme = ⟨[], raw, []⟩)
    (found : scope.find? (fun item => decide (item.name = binding.name)) = some binding)
    (notSchema : binding.kind ≠ .schema) (formed : WellFormed raw) (sort : typeSort raw = .value) :
    NodeHasTypeIn solution context scope (.read binding.binder []) raw := by
  have identityScheme : schemeSubstitution binding.scheme (witnessSubstitution []) = identity := by
    funext id
    simp [shape, schemeSubstitution, identity]
  have typed := NodeHasTypeIn.read (solution := solution) (context := context) (arguments := [])
    (ty := raw) (requirements := []) found notSchema (by simp [instantiationKeysValid, shape])
    (by simp [instantiationSortsValid])
    ⟨by rw [identityScheme, substitute_identity, shape], by simp [shape]⟩ (by simp [Solves])
    (by rw [closed]; exact formed) (by rw [closed]; exact sort)
  rwa [closed] at typed

theorem ok_read_grounded (solution runtime : Substitution) (scope : Scope)
    (unique : (scope.map Binding.binder).Nodup) (member : callOkBinding ∈ scope)
    (found : scope.find? (fun item => decide (item.name = callOkBinding.name)) = some callOkBinding) :
    GroundNode solution runtime programStorage scope callResultType (.read resultOkBinder resultInstance)
      (.function (.argument (.primitive .integer) .argumentsEmpty) callResultType) := by
  refine .read (ok_read_typed solution scope _ found) rfl ?_
  intro name binding foundBinding same _
  have equal := scope_binding_unique scope binding callOkBinding unique (List.mem_of_find?_eq_some foundBinding)
    member same
  subst equal
  exact ⟨_, _, _, ok_scheme_instantiates runtime, rfl⟩

theorem wrap_body_grounded (solution runtime : Substitution) :
    GroundBlock solution runtime programStorage wrapScope callResultType (some callResultType) wrapCandidate := by
  have unique := lift_scope_binders_are_distinct
  have okGround := ok_read_grounded solution runtime wrapScope unique (by simp [wrapScope, liftScope]) rfl
  have parameterTyped := monomorphic_read_typed solution wrapScope (some callResultType) callParameterBinding
    (.primitive .integer) (fun _ => rfl) rfl rfl (by decide) trivial rfl
  have parameterGround := ground_monomorphic_read wrapScope solution runtime programStorage callResultType
    callParameterBinding.name resultFirstBinder .immutable (.primitive .integer) (.primitive .integer) .integer
    parameterTyped unique (by simp [wrapScope, callParameterBinding]) rfl (by simp [programStorage]; decide)
  exact .returned (.call okGround (.cons parameterGround .nil) rfl) (.nil (.inl rfl))

theorem lift_body_grounded (solution runtime : Substitution) :
    GroundBlock solution runtime programStorage liftScope callResultType (some callResultType) liftCandidate := by
  have unique : (liftScope.map Binding.binder).Nodup := (List.nodup_cons.mp lift_scope_binders_are_distinct).2
  have okGround := ok_read_grounded solution runtime liftScope unique (by simp [liftScope]) rfl
  have wrapTyped := monomorphic_read_typed solution liftScope (some callResultType) callWrapBinding
    (.function (.argument (.primitive .integer) .argumentsEmpty) callResultType) (fun _ => rfl) rfl rfl (by decide)
    (by simp [WellFormed, typeSort, callResultType]) rfl
  have wrapGround : GroundNode solution runtime programStorage liftScope callResultType (.read wrapBinder [])
      (.function (.argument (.primitive .integer) .argumentsEmpty) callResultType) := by
    refine .read wrapTyped rfl ?_
    intro name binding foundBinding same _
    have equal := scope_binding_unique liftScope binding callWrapBinding unique
      (List.mem_of_find?_eq_some foundBinding) (by simp [liftScope]) same
    subst equal
    exact ⟨_, _, _, wrap_scheme_instantiates runtime, rfl⟩
  have call : GroundNode solution runtime programStorage liftScope callResultType
      (.call (.read wrapBinder []) [.integer "1"]) callResultType :=
    .call wrapGround (.cons (.integer (value := intOne) (by decide +kernel)) .nil) rfl
  have propagated : GroundNode solution runtime programStorage liftScope callResultType
      (.propagate (.call (.read wrapBinder []) [.integer "1"]) (.primitive .string)) (.primitive .integer) :=
    .propagate call rfl ⟨_, rfl⟩ rfl
  exact .returned (.call okGround (.cons propagated .nil) rfl) (.nil (.inl rfl))

theorem monomorphic_runtime_sorted (captured : Substitution) (sorted : WellSorted captured) (body : MonoType)
    (arguments : List (Variable × MonoType)) (elaboration : Substitution) (elaborated : WellSorted elaboration) :
    WellSorted (compose captured (compose (schemeSubstitution ⟨[], body, []⟩ (witnessSubstitution arguments))
      elaboration)) := by
  refine compose_well_sorted sorted (compose_well_sorted ?_ elaborated)
  intro id
  simp [schemeSubstitution, WellFormed, typeSort]

theorem program_store_is_immutable (cell : Environments.CellId) (entry : Environments.StoreCell RuntimeType Value HasType)
    (found : programStore cell = some entry) : entry.mutability = .immutable := by
  simp only [programStore] at found
  split at found
  · cases found; rfl
  · split at found
    · cases found; rfl
    · split at found
      · cases found; rfl
      · cases found

theorem program_scope_contracts (state : Machine) (solution : Substitution) (globals : state.globals = programGlobals)
    (store : state.store = programStore) :
    ScopeValuesAgree liftScope solution state.globals state.store ∧
      ScopeWritableContract liftScope solution state.globals state.store ∧
      MutableCellsAgree liftScope solution state.globals state.store ∧
      SchemaMatches liftScope solution state.schemas := by
  rw [globals, store]
  have immutable : ∀ binding ∈ liftScope, binding.kind = .immutable := fun binding member =>
    lift_scope_is_immutable binding (List.mem_cons_of_mem _ member)
  refine ⟨scope_values_agree_of_members _ _ _ _ (program_global_values _), ?_,
    mutable_cells_agree_of_immutable _ _ _ _ immutable, ?_⟩
  · intro _ _ cell entry _ _ _ present mutable
    rw [program_store_is_immutable cell entry present] at mutable
    cases mutable
  · intro _ binding _ _ found schema
    rw [immutable binding (List.mem_of_find?_eq_some found)] at schema
    cases schema

def wrapFunction : Function := certifiedFunction wrapSignature wrapCandidate

theorem wrap_function_parameters : wrapFunction.parameters = [resultFirstBinder] := by
  decide

theorem lift_function_parameters : liftFunction.parameters = [] := by
  decide

theorem wrap_body_declares_nothing : declaredBinders wrapFunction.body = [] := rfl

theorem lift_body_declares_nothing : declaredBinders liftFunction.body = [] := rfl

theorem wrap_function_certified (state : Machine) (globals : state.globals = programGlobals)
    (store : state.store = programStore) :
    FunctionBodyCertified liftScope programStorage state wrapFunction := by
  intro captured arguments parameters result normalized sorted instantiated
  obtain ⟨sourceParameters, sourceResult, shape, parametersClosed, resultClosed⟩ :=
    instantiate_function_components captured _ arguments parameters result normalized instantiated
  simp [wrapFunction, certifiedFunction, resultFunctionScheme, resultFunctionType, wrapSignature, integerStringResult,
    substitute] at shape
  obtain ⟨rfl, rfl⟩ := shape
  simp [monoRuntime] at parametersClosed resultClosed
  subst parametersClosed resultClosed
  have runtimeSorted := monomorphic_runtime_sorted captured sorted (resultFunctionType wrapSignature) arguments
    identity identity_well_sorted
  obtain ⟨values, writable, mutableCells, schemas⟩ := program_scope_contracts state _ globals store
  refine ⟨identity, [callParameterBinding], callResultType, identity_well_sorted, runtimeSorted,
    compose_well_sorted identity_well_sorted runtimeSorted, values, writable, mutableCells, schemas, ?_, ?_, ?_, ?_,
    ?_, wrap_body_grounded _ _, rfl, ?_, ?_, ?_⟩
  · intro binder member
    rw [wrap_function_parameters] at member
    simp at member
    subst member
    simp [programStorage]
    decide
  · intro binder member
    rw [wrap_body_declares_nothing] at member
    cases member
  · rw [wrap_function_parameters]
    rfl
  · exact lift_scope_binders_are_distinct
  · exact .cons rfl .nil
  · rw [wrap_body_declares_nothing]
    exact List.nodup_nil
  · intro binder member
    rw [wrap_body_declares_nothing] at member
    cases member
  · simp [PropagationMatches, blockPropagationErrors, statementPropagationErrors, propagationErrors, argumentErrors,
      wrapFunction, certifiedFunction, wrapCandidate, okNode]

theorem lift_function_certified (state : Machine) (globals : state.globals = programGlobals)
    (store : state.store = programStore) :
    FunctionBodyCertified liftScope programStorage state liftFunction := by
  intro captured arguments parameters result normalized sorted instantiated
  obtain ⟨sourceParameters, sourceResult, shape, parametersClosed, resultClosed⟩ :=
    instantiate_function_components captured _ arguments parameters result normalized instantiated
  simp [liftFunction, certifiedFunction, resultFunctionScheme, resultFunctionType, liftSignature, integerStringResult,
    substitute] at shape
  obtain ⟨rfl, rfl⟩ := shape
  simp [monoRuntime] at parametersClosed resultClosed
  subst parametersClosed resultClosed
  have runtimeSorted := monomorphic_runtime_sorted captured sorted (resultFunctionType liftSignature) arguments
    identity identity_well_sorted
  obtain ⟨values, writable, mutableCells, schemas⟩ := program_scope_contracts state _ globals store
  refine ⟨identity, [], callResultType, identity_well_sorted, runtimeSorted,
    compose_well_sorted identity_well_sorted runtimeSorted, values, writable, mutableCells, schemas, ?_, ?_, ?_, ?_,
    ?_, lift_body_grounded _ _, rfl, ?_, ?_, ?_⟩
  · intro binder member
    rw [lift_function_parameters] at member
    cases member
  · intro binder member
    rw [lift_body_declares_nothing] at member
    cases member
  · rw [lift_function_parameters]
    rfl
  · exact (List.nodup_cons.mp lift_scope_binders_are_distinct).2
  · exact .nil
  · rw [lift_body_declares_nothing]
    exact List.nodup_nil
  · intro binder member
    rw [lift_body_declares_nothing] at member
    cases member
  · simp [PropagationMatches, blockPropagationErrors, statementPropagationErrors, propagationErrors, argumentErrors,
      liftFunction, certifiedFunction, liftCandidate, okNode, propagationContext, callResultType, substitute]

theorem program_global_scheme_reads (binder : BinderId) (declaration : DeclarationId) (scheme : Scheme)
    (found : Connected.read programStore programGlobals binder = some (.schemeFunction declaration scheme)) :
    ∃ cell entry, programGlobals binder = some cell ∧ programStore cell = some entry ∧
      (binder = resultOkBinder ∨ binder = wrapBinder ∨ binder = liftBinder) := by
  unfold Connected.read at found
  cases cellFound : programGlobals binder with
  | none => simp [cellFound] at found
  | some cell =>
    cases entryFound : programStore cell with
    | none => simp [cellFound, entryFound] at found
    | some entry =>
      exact ⟨cell, entry, rfl, entryFound, (program_globals_cell binder cell cellFound).2⟩

theorem program_scheme_cells_are_immutable : SchemeCellsImmutable programGlobals programStore := by
  intro binder declaration scheme found
  obtain ⟨cell, entry, cellFound, entryFound, _⟩ := program_global_scheme_reads binder declaration scheme found
  exact ⟨cell, entry, cellFound, entryFound, program_store_is_immutable cell entry entryFound⟩

theorem program_storage_agrees : StorageAgreement programGlobals programStore programStorage := by
  refine ⟨fun binder declaration scheme found => ?_, fun binder isScheme => ?_⟩
  · obtain ⟨_, _, _, _, which⟩ := program_global_scheme_reads binder declaration scheme found
    simp [programStorage, which]
  · have which : binder = resultOkBinder ∨ binder = wrapBinder ∨ binder = liftBinder := by
      unfold programStorage at isScheme
      split at isScheme
      · assumption
      · cases isScheme
    rcases which with rfl | rfl | rfl
    · exact ⟨callOkDeclaration, okScheme, by simp [Connected.read, programGlobals, programStore]⟩
    · exact ⟨programWrapDeclaration, resultFunctionScheme wrapSignature,
        by simp [Connected.read, programGlobals, programStore, program_identities_are_distinct]⟩
    · exact ⟨programLiftDeclaration, resultFunctionScheme liftSignature,
        by simp [Connected.read, programGlobals, programStore, program_identities_are_distinct]⟩

theorem ok_scheme_is_callable : SchemeCallable programFunctions callOkDeclaration okScheme := by
  refine .inr (.inl ⟨by simp [programFunctions], ?_⟩)
  intro captured arguments parameters result normalized instantiated
  obtain ⟨sourceParameters, sourceResult, shape, parametersClosed, resultClosed⟩ :=
    instantiate_function_components captured okScheme arguments parameters result normalized instantiated
  have body : okScheme.body = .function (.argument (.variable identityVariable) .argumentsEmpty)
      (.result (.variable identityVariable) (.variable resultValueVariable)) := rfl
  rw [body] at shape
  simp only [substitute, MonoType.function.injEq] at shape
  generalize substitute captured (schemeSubstitution okScheme (witnessSubstitution arguments) identityVariable) = value
    at shape
  generalize substitute captured (schemeSubstitution okScheme (witnessSubstitution arguments) resultValueVariable) =
    error at shape
  obtain ⟨rfl, rfl⟩ := shape
  cases valueFound : monoRuntime value with
  | none => simp [monoRuntime, valueFound] at parametersClosed
  | some success =>
    cases errorFound : monoRuntime error with
    | none => simp [monoRuntime, valueFound, errorFound] at resultClosed
    | some failure =>
      simp [monoRuntime, valueFound] at parametersClosed
      simp [monoRuntime, valueFound, errorFound] at resultClosed
      exact ⟨success, failure, parametersClosed.symm, resultClosed.symm⟩

theorem program_store_callables : StoreCallables programFunctions programStore := by
  intro cell entry found
  simp only [programStore] at found
  split at found
  · cases found
    exact .scheme ok_scheme_is_callable
  · split at found
    · cases found
      exact .scheme (.inl ⟨wrapFunction, by simp [programFunctions, program_identities_are_distinct, wrapFunction],
        rfl⟩)
    · split at found
      · cases found
        exact .scheme (.inl ⟨liftFunction, by simp [programFunctions, program_identities_are_distinct, liftFunction],
          rfl⟩)
      · cases found

theorem program_functions_certified (state : Machine) (functions : state.functions = programFunctions)
    (globals : state.globals = programGlobals) (store : state.store = programStore) :
    FunctionTableCertified liftScope programStorage state := by
  intro declaration function found
  rw [functions] at found
  simp only [programFunctions] at found
  split at found
  · cases found
  · split at found
    · cases found
      exact wrap_function_certified state globals store
    · split at found
      · cases found
        exact lift_function_certified state globals store
      · cases found

def liftContext (base : Machine) : TypingContext :=
  ⟨liftScope, programStorage, base.solution, base.solution, callResultType, some callResultType, 0, ⟨_, rfl⟩⟩

theorem lift_invocation_is_typed (base : Machine) (sorted : WellSorted base.solution) :
    TypedMachine liftScope programStorage (fun _ => none) (liftInvocation base) := by
  obtain ⟨_, writable, _, schemas⟩ := program_scope_contracts (liftInvocation base) base.solution rfl rfl
  have activation : ActivationInvariant (liftInvocation base) :=
    ⟨by simp [liftInvocation, programMachine], by simp [liftInvocation, programMachine, StackActivationsBelow],
      program_store_is_below 1 (by decide)⟩
  have typedCells : CellsTyped programGlobals (fun _ => none) programStore := by
    intro _ _ declared
    cases declared
  refine ⟨⟨activation, ⟨typedCells, program_globals_are_present⟩⟩,
    program_functions_certified _ rfl rfl rfl, program_globals_are_distinct, program_scheme_cells_are_immutable,
    program_storage_agrees, program_store_callables, ?_⟩
  refine ⟨.expression (liftContext base) callResultType, .expression (liftContext base) callResultType, rfl, rfl, rfl,
    sorted, sorted, ?_, ⟨_, rfl⟩, ⟨_, rfl⟩, ?_, .nil, ?_⟩
  · refine ⟨a_top_level_state_agrees_with_the_lift_scope _ (fun _ _ => rfl) rfl, writable, schemas,
      program_scheme_cells_are_immutable, program_storage_agrees⟩
  · exact .invoke rfl (.instantiatedFunction (function := liftFunction)
      (by simp [liftInvocation, programMachine, programFunctions, program_identities_are_distinct, liftFunction])
      sorted (lift_scheme_instantiates _)) .nil rfl
  · refine ⟨?_, ?_⟩
    · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, liftInvocation, programMachine]
    · intro binding member
      simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, liftInvocation, programMachine] at member

theorem the_certified_lift_invocation_is_type_safe (base : Machine) (sorted : WellSorted base.solution)
    (response : IntValue) :
    (∀ final events, Execution (liftInvocation base) events final →
      TypedMachine liftScope programStorage (fun _ => none) final) ∧
    (∀ final events fault, Execution (liftInvocation base) events final →
      final.control = .fault fault → ∃ issue, fault = .integer issue) ∧
    (∀ final events, Execution (liftInvocation base) events final →
      MachineTerminal final ∨ ∃ event next, Step final event next ∧ PermittedControl next.control) := by
  have typed := lift_invocation_is_typed base sorted
  refine ⟨fun final events execution => ?_, fun final events fault execution failed => ?_,
    fun final events execution => ?_⟩
  · exact typed_machine_execution_preservation _ _ _ response execution typed
  · exact typed_execution_fault_is_integer _ _ _ response execution typed fault failed
  · exact typed_machine_safe_progress final _ _ _ response
      (typed_machine_execution_preservation _ _ _ response execution typed)

theorem the_returned_lift_state_is_typed (base : Machine) (sorted : WellSorted base.solution) (response : IntValue) :
    TypedMachine liftScope programStorage (fun _ => none) (liftReturned base) :=
  (the_certified_lift_invocation_is_type_safe base sorted response).1 _ _ (invoking_lift_runs_its_body_to_the_caller base)

theorem an_identity_solution_lift_invocation_is_typed (base : Machine) :
    TypedMachine liftScope programStorage (fun _ => none) (liftInvocation { base with solution := identity }) :=
  lift_invocation_is_typed _ identity_well_sorted

theorem storage_that_forgets_the_scheme_bindings_disagrees :
    ¬ StorageAgreement programGlobals programStore (fun _ => .ordinary) := by
  intro agrees
  have isScheme := agrees.1 resultOkBinder callOkDeclaration okScheme
    (by simp [Connected.read, programGlobals, programStore])
  cases isScheme

end Xsh.Semantics.Connected
