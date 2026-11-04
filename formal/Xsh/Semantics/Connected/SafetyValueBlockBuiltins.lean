import Xsh.Semantics.Connected.SafetyValueBlockCall

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected
open Xsh.Values

theorem typed_machine_invoke_value (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (target : Value) (arguments : List Value) (value : Value) (event : Label)
    (parameterTypes resultType : RuntimeType)
    (step : Step { base with control := .invoke target arguments } event { base with control := .value value })
    (targetType : valueType target = some (.function parameterTypes resultType))
    (resultTypeMatches : HasType resultType value) (callable : ValueCallables base.functions value)
    (typed : TypedMachine globals storage types { base with control := .invoke target arguments }) :
    TypedMachine globals storage types { base with control := .value value } := by
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | invoke found callableTarget argumentTypes closed =>
    rename_i context argumentsRuntime resultRuntime result
    rw [targetType] at found
    cases found
    have newPending : PendingBindingsSafe { base with control := .value value } := by
      constructor
      · simpa [pendingMachineBinders, pendingControlBinders] using pending.unique
      · simpa [pendingMachineBinders, pendingControlBinders] using pending.fresh
    have oldStack : StackTyped { base with control := .value value } (.expression context result) output base.stack := by
      apply stack_typing_survives_with_pending step stack newPending
      · simpa [TypedDemand.context] using activation.symm
      · rfl
    refine ⟨.expression context result, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
      ?_, inputClosed, outputClosed, .value resultTypeMatches closed callable, oldStack, newPending⟩
    simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_time_call (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (declaration : Xsh.Syntax.Foundation.DeclarationId) (response : IntValue)
    (found : base.functions declaration = some .timeNow)
    (typed : TypedMachine globals storage types
      { base with control := .invoke (.function declaration .argumentsEmpty .integer) [] }) :
    TypedMachine globals storage types { base with control := .value (.scalar (.integer response)) } := by
  exact typed_machine_invoke_value globals storage types base _ [] _ (.request .timeNow response) .argumentsEmpty .integer
    (Step.callTime base declaration response found) rfl (has_type_of_exact _ _ rfl) .scalar typed

theorem typed_machine_instantiated_time_call (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (declaration : Xsh.Syntax.Foundation.DeclarationId) (captured : Substitution)
    (arguments : List (Variable × MonoType)) (response : IntValue) (found : base.functions declaration = some .timeNow)
    (typed : TypedMachine globals storage types
      { base with control := .invoke (.instantiatedFunction declaration .argumentsEmpty .integer captured arguments) [] }) :
    TypedMachine globals storage types { base with control := .value (.scalar (.integer response)) } := by
  exact typed_machine_invoke_value globals storage types base _ [] _ (.request .timeNow response) .argumentsEmpty .integer
    (Step.callTimeInstantiated base declaration captured arguments response found) rfl (has_type_of_exact _ _ rfl) .scalar typed

theorem typed_machine_single_argument_callable (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (target value : Value)
    (typed : TypedMachine globals storage types { base with control := .invoke target [value] }) :
    ValueCallables base.functions value := by
  obtain ⟨_, _, _, _, _, _, _, _, _, _, control, _, _⟩ := typed.focus
  cases control with
  | invoke targetTyped targetCallable argumentsTyped closed =>
    cases argumentsTyped with
    | cons valueTyped callable rest => exact callable

theorem typed_machine_ok_call (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (declaration : Xsh.Syntax.Foundation.DeclarationId) (value : Value)
    (valueType' errorType : RuntimeType) (found : base.functions declaration = some .resultOk)
    (valueTyped : HasType valueType' value)
    (typed : TypedMachine globals storage types { base with control := .invoke (.function declaration (.argument valueType' .argumentsEmpty) (.result valueType' errorType)) [value] }) :
    TypedMachine globals storage types { base with control := .value (.ok value errorType) } := by
  have callable := typed_machine_single_argument_callable globals storage types base _ value typed
  apply typed_machine_invoke_value globals storage types base _ [value] _ .internal
    (.argument valueType' .argumentsEmpty) (.result valueType' errorType)
    (Step.callOk base declaration value valueType' errorType found valueTyped) rfl ?_ (.ok callable) typed
  exact compatible_ok_value value valueType' errorType valueTyped

theorem typed_machine_error_call (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (declaration : Xsh.Syntax.Foundation.DeclarationId) (value : Value)
    (valueType' errorType : RuntimeType) (found : base.functions declaration = some .resultError)
    (valueTyped : HasType errorType value)
    (typed : TypedMachine globals storage types { base with control := .invoke (.function declaration (.argument errorType .argumentsEmpty) (.result valueType' errorType)) [value] }) :
    TypedMachine globals storage types { base with control := .value (.error value valueType') } := by
  have callable := typed_machine_single_argument_callable globals storage types base _ value typed
  apply typed_machine_invoke_value globals storage types base _ [value] _ .internal
    (.argument errorType .argumentsEmpty) (.result valueType' errorType)
    (Step.callError base declaration value valueType' errorType found valueTyped) rfl ?_ (.error callable) typed
  exact compatible_error_value value valueType' errorType valueTyped

theorem typed_machine_instantiated_ok_call (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (declaration : Xsh.Syntax.Foundation.DeclarationId) (value : Value)
    (valueType' errorType : RuntimeType) (captured : Substitution) (arguments : List (Variable × MonoType))
    (found : base.functions declaration = some .resultOk) (valueTyped : HasType valueType' value)
    (typed : TypedMachine globals storage types { base with control := .invoke (.instantiatedFunction declaration (.argument valueType' .argumentsEmpty) (.result valueType' errorType) captured arguments) [value] }) :
    TypedMachine globals storage types { base with control := .value (.ok value errorType) } := by
  have callable := typed_machine_single_argument_callable globals storage types base _ value typed
  apply typed_machine_invoke_value globals storage types base _ [value] _ .internal
    (.argument valueType' .argumentsEmpty) (.result valueType' errorType)
    (Step.callOkInstantiated base declaration value valueType' errorType captured arguments found valueTyped) rfl ?_ (.ok callable) typed
  exact compatible_ok_value value valueType' errorType valueTyped

theorem typed_machine_instantiated_error_call (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (declaration : Xsh.Syntax.Foundation.DeclarationId) (value : Value)
    (valueType' errorType : RuntimeType) (captured : Substitution) (arguments : List (Variable × MonoType))
    (found : base.functions declaration = some .resultError) (valueTyped : HasType errorType value)
    (typed : TypedMachine globals storage types { base with control := .invoke (.instantiatedFunction declaration (.argument errorType .argumentsEmpty) (.result valueType' errorType) captured arguments) [value] }) :
    TypedMachine globals storage types { base with control := .value (.error value valueType') } := by
  have callable := typed_machine_single_argument_callable globals storage types base _ value typed
  apply typed_machine_invoke_value globals storage types base _ [value] _ .internal
    (.argument errorType .argumentsEmpty) (.result valueType' errorType)
    (Step.callErrorInstantiated base declaration value valueType' errorType captured arguments found valueTyped) rfl ?_ (.error callable) typed
  exact compatible_error_value value valueType' errorType valueTyped

end Xsh.Semantics.Connected
