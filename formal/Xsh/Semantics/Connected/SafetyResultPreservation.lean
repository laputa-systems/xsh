import Xsh.Semantics.Connected.SafetyValueBlockRead

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem compatible_ok_inner (value : Value) (oldFailure success failure : RuntimeType)
    (typed : HasType (.result success failure) (.ok value oldFailure)) : HasType success value := by
  obtain ⟨actual, actualTyped, fits⟩ := (has_type_iff_compatible _ _).mp typed
  cases inner : valueType value with
  | none => simp [valueType, inner] at actualTyped
  | some actualSuccess =>
    simp [valueType, inner] at actualTyped
    subst actual
    obtain ⟨actualSuccess', actualFailure, shape, successFits, failureFits⟩ :=
      result_fit_has_components success failure (.result actualSuccess oldFailure) fits
    cases shape
    exact (has_type_iff_compatible _ _).mpr ⟨actualSuccess, inner, successFits⟩

theorem compatible_error_inner (value : Value) (oldSuccess success failure : RuntimeType)
    (typed : HasType (.result success failure) (.error value oldSuccess)) : HasType failure value := by
  obtain ⟨actual, actualTyped, fits⟩ := (has_type_iff_compatible _ _).mp typed
  cases inner : valueType value with
  | none => simp [valueType, inner] at actualTyped
  | some actualFailure =>
    simp [valueType, inner] at actualTyped
    subst actual
    obtain ⟨actualSuccess, actualFailure', shape, successFits, failureFits⟩ :=
      result_fit_has_components success failure (.result oldSuccess actualFailure) fits
    cases shape
    exact (has_type_iff_compatible _ _).mpr ⟨actualFailure, inner, failureFits⟩

theorem typed_machine_propagate_ok (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value) (oldFailure : RuntimeType)
    (typed : TypedMachine globals storage types
      { base with control := .value (.ok value oldFailure), stack := .propagate :: base.stack }) :
    TypedMachine globals storage types { base with control := .value value } := by
  have step := Step.propagateOk base value oldFailure
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
      | propagate matching =>
        rename_i context resultRuntime success failure functionResult
        cases successClosed : monoRuntime success with
        | none => simp [monoRuntime, successClosed] at valueClosed
        | some successType =>
          cases failureClosed : monoRuntime failure with
          | none => simp [monoRuntime, successClosed, failureClosed] at valueClosed
          | some failureType =>
            simp [monoRuntime, successClosed, failureClosed] at valueClosed
            subst resultRuntime
            have innerTyped := compatible_ok_inner value oldFailure successType failureType valueTyped
            cases valueCallable with
            | ok callable =>
              have newPending : PendingBindingsSafe { base with control := .value value } := by
                constructor
                · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                    frameCallerActivation] using pending.unique
                · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                    frameCallerActivation] using pending.fresh
              have oldStack : StackTyped { base with control := .value value } (.expression context success) output base.stack := by
                apply stack_typing_survives_with_pending step remaining newPending
                · simpa [TypedDemand.context] using activation.symm
                · rfl
              refine ⟨.expression context success, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
                ?_, middleClosed, outputClosed, .value innerTyped successClosed callable, oldStack, newPending⟩
              simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_propagate_error (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (error : Value) (oldSuccess : RuntimeType)
    (typed : TypedMachine globals storage types
      { base with control := .value (.error error oldSuccess), stack := .propagate :: base.stack }) :
    TypedMachine globals storage types { base with control := .propagated (.error error oldSuccess) } := by
  have step := Step.propagateError base error oldSuccess
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
      | propagate matching =>
        rename_i context resultRuntime success failure functionResult
        cases successClosed : monoRuntime success with
        | none => simp [monoRuntime, successClosed] at valueClosed
        | some successType =>
          cases failureClosed : monoRuntime failure with
          | none => simp [monoRuntime, successClosed, failureClosed] at valueClosed
          | some failureType =>
            simp [monoRuntime, successClosed, failureClosed] at valueClosed
            subst resultRuntime
            have innerTyped := compatible_error_inner error oldSuccess successType failureType valueTyped
            cases valueCallable with
            | error callable =>
              have newPending : PendingBindingsSafe { base with control := .propagated (.error error oldSuccess) } := by
                constructor
                · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                    frameCallerActivation] using pending.unique
                · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                    frameCallerActivation] using pending.fresh
              have oldStack : StackTyped { base with control := .propagated (.error error oldSuccess) }
                  (.expression context success) output base.stack := by
                apply stack_typing_survives_with_pending step remaining newPending
                · simpa [TypedDemand.context] using activation.symm
                · rfl
              refine ⟨.expression context success, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
                trivial, middleClosed, outputClosed, .propagated matching innerTyped failureClosed callable, oldStack, newPending⟩

end Xsh.Semantics.Connected
