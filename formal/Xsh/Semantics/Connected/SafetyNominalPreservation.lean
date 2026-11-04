import Xsh.Semantics.Connected.SafetyResultPreservation
import Xsh.Semantics.Connected.SafetyValueBlockFrames
import Xsh.Semantics.Connected.SafetyValueBlockPreservation

namespace Xsh.Semantics.Connected
open Xsh.Typing.Connected

theorem typed_machine_require_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (binder : Xsh.Foundation.BinderId) (identity : Xsh.Validation.Nominal.NominalId)
    (value : Node) (guard : Guard) (found : base.schemas binder = some (identity, guard))
    (typed : TypedMachine globals storage types { base with control := .expression (.requireValue binder identity value) }) :
    TypedMachine globals storage types
      { base with control := .expression value, stack := .requireGuard identity guard :: base.stack } := by
  apply typed_machine_expression_push globals storage types base (.requireValue binder identity value) value
    (.requireGuard identity guard) .internal (Step.requireStart base binder identity value guard found) ?_ rfl ?_ ?_ typed
  · intro context ty ground schemas
    cases ground with
    | requireValue nodeTyped valueTyped closed =>
      cases nodeTyped with
      | requireValue lookup kind quantified requirements body wellFormed inner =>
        obtain ⟨actualGuard, actualFound, baseType⟩ := schemas _ _ _ _ lookup kind body
        rw [found] at actualFound
        cases actualFound
        exact ⟨_, valueTyped, .requireGuard baseType⟩
  · intro binder member
    simpa [nodeDeclaredBinders] using member
  · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      nodeDeclaredBinders, frameCallerActivation]

theorem runtime_monotype_roundtrip (runtime : RuntimeType) (ty : MonoType)
    (converted : runtimeMonotype runtime = some ty) : monoRuntime ty = some runtime := by
  induction runtime generalizing ty with
  | unit | integer | boolean | string | error | rowEmpty | argumentsEmpty | schemeFunction =>
    simp [runtimeMonotype] at converted
    all_goals subst ty <;> rfl
  | record row ih | nominal _ row ih =>
    cases found : runtimeMonotype row <;> simp_all [runtimeMonotype]
    all_goals subst ty; simp_all [monoRuntime]
  | rowField _ left right leftIH rightIH | argument left right leftIH rightIH | result left right leftIH rightIH | function left right leftIH rightIH =>
    cases first : runtimeMonotype left <;> cases second : runtimeMonotype right <;>
      simp_all [runtimeMonotype]
    all_goals subst ty; simp_all [monoRuntime]

theorem erase_nominal_preserves_callables (functions : FunctionTable) (value : Value)
    (safe : ValueCallables functions value) : ValueCallables functions (eraseNominal value) := by
  cases value with
  | nominal identity base inner =>
    cases safe with
    | nominal innerSafe => exact erase_nominal_preserves_callables functions inner innerSafe
  | _ => exact safe
termination_by value

theorem accepted_guard_types_erased (guard : Guard) (value : Value) (runtime : RuntimeType)
    (typed : valueType value = some runtime) (accepted : guardDecision guard value = some true) :
    HasType (guardBase guard) (eraseNominal value) := by
  obtain ⟨erasedType, erasedTyped⟩ := erase_nominal_retains_a_type value runtime typed
  cases guard with
  | integerRange minimum maximum =>
    cases erased : eraseNominal value with
    | scalar scalar =>
      cases scalar with
      | integer number => exact has_type_of_exact _ _ (by simp [guardBase, valueType])
      | _ => simp [guardDecision, erased, guardPlain] at accepted
    | _ => simp [guardDecision, erased, guardPlain] at accepted
  | recordShape row =>
    cases erased : eraseNominal value with
    | record fields =>
      have found : valueType (.record fields) = some erasedType := by simpa [erased] using erasedTyped
      have fits : typeFits (.record row) erasedType = true := by
        simpa [guardDecision, guardPlain, erased, found] using accepted
      exact (has_type_iff_compatible _ _).mpr ⟨erasedType, by simpa [erased] using found, fits⟩
    | _ => simp [guardDecision, erased, guardPlain] at accepted

theorem typed_machine_guard_value (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard) (value result : Value)
    (step : Step { base with control := .value value, stack := .requireGuard identity guard :: base.stack } .internal
      { base with control := .value result })
    (resultTyped : HasType (.result (.nominal identity (guardBase guard)) .error) result)
    (resultCallable : ValueCallables base.functions result)
    (typed : TypedMachine globals storage types
      { base with control := .value value, stack := .requireGuard identity guard :: base.stack }) :
    TypedMachine globals storage types { base with control := .value result } := by
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed valueCallable =>
    cases stack with
    | cons frame middleClosed remaining =>
      cases frame with
      | requireGuard converted =>
        rename_i context inputType runtimeType baseType
        have baseClosed := runtime_monotype_roundtrip (guardBase guard) baseType converted
        have resultClosed : monoRuntime (.result (.nominal identity baseType) (.primitive .error)) =
            some (.result (.nominal identity (guardBase guard)) .error) := by simp [monoRuntime, baseClosed]
        have newPending : PendingBindingsSafe { base with control := .value result } := by
          constructor
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.unique
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.fresh
        have oldStack : StackTyped { base with control := .value result }
            (.expression context (.result (.nominal identity baseType) (.primitive .error))) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context] using activation.symm
          · rfl
        refine ⟨.expression context (.result (.nominal identity baseType) (.primitive .error)), output,
          runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, ?_, middleClosed, outputClosed,
          .value resultTyped resultClosed resultCallable, oldStack, newPending⟩
        simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_require_success (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard) (value : Value)
    (accepted : guardDecision guard value = some true)
    (typed : TypedMachine globals storage types
      { base with control := .value value, stack := .requireGuard identity guard :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .value (.ok (.nominal identity (guardBase guard) (eraseNominal value)) .error) } := by
  obtain ⟨_, _, _, _, _, _, _, _, _, _, control, _, _⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed callable =>
    obtain ⟨actual, actualTyped, fits⟩ := (has_type_iff_compatible _ _).mp valueTyped
    have erasedTyped := accepted_guard_types_erased guard value actual actualTyped accepted
    obtain ⟨erasedType, erasedAt, erasedFits⟩ := (has_type_iff_compatible _ _).mp erasedTyped
    apply typed_machine_guard_value globals storage types base identity guard value _
      (Step.requireSuccess base identity guard value accepted)
      (has_type_of_exact _ _ (by simp [valueType, erasedAt, erasedFits])) (.ok (.nominal (erase_nominal_preserves_callables _ _ callable))) typed

theorem typed_machine_require_failure (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard) (value : Value)
    (rejected : guardDecision guard value = some false)
    (typed : TypedMachine globals storage types
      { base with control := .value value, stack := .requireGuard identity guard :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .value (.error .validationError (.nominal identity (guardBase guard))) } := by
  exact typed_machine_guard_value globals storage types base identity guard value _
    (Step.requireFailure base identity guard value rejected) (has_type_of_exact _ _ rfl) (.error .validationError) typed

end Xsh.Semantics.Connected
