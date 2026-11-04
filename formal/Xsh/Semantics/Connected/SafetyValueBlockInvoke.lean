import Xsh.Semantics.Connected.SafetyValueBlockProgress

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected
open Xsh.Values

theorem mono_runtime_is_not_scheme_function (ty : MonoType) : monoRuntime ty ≠ some .schemeFunction := by
  cases ty with
  | «variable» id => simp [monoRuntime]
  | primitive primitive => cases primitive <;> simp [monoRuntime]
  | argumentsEmpty | rowEmpty => simp [monoRuntime]
  | record row => cases found : monoRuntime row <;> simp [monoRuntime, found]
  | nominal identity base => cases found : monoRuntime base <;> simp [monoRuntime, found]
  | function arguments result | argument arguments result | result arguments result | rowField _ arguments result =>
    cases first : monoRuntime arguments <;> cases second : monoRuntime result <;> simp [monoRuntime, first, second]

theorem mono_typed_value_is_ordinary (value : Value) (ty : MonoType) (runtimeType : RuntimeType)
    (closed : monoRuntime ty = some runtimeType) (typed : HasType runtimeType value) :
    ordinaryValue value = true := by
  cases value with
  | schemeFunction declaration scheme =>
    have tag : runtimeType = .schemeFunction := by
      cases runtimeType <;> simp_all [HasType, valueType, typeFits]
    subst runtimeType
    exact False.elim (mono_runtime_is_not_scheme_function ty closed)
  | _ => rfl

theorem runtime_arguments_match_parameter_bindings (functions : FunctionTable) (solution : Substitution)
    (scope : Scope) (values : List Value) (types : RuntimeType)
    (parameters : ParameterTypesTyped solution scope types) (arguments : RuntimeArguments functions values types) :
    ParameterBindingsTyped solution scope values types := by
  induction parameters generalizing values with
  | nil => cases arguments; exact .nil
  | @cons name binder raw rest ty types closed tail ih =>
    cases arguments with
    | cons typed callable remaining =>
      exact .cons (mono_typed_value_is_ordinary _ _ _ closed typed) closed typed (ih _ remaining)

theorem callable_arguments_ready (state : Machine) (globals : Scope) (storage : BindingStorageMap)
    (target : Value) (arguments : List Value) (parameterTypes resultType : RuntimeType)
    (certified : FunctionTableCertified globals storage state)
    (below : StoreBelowActivation state.store state.nextActivation)
    (callable : ValueCallables state.functions target)
    (targetType : valueType target = some (.function parameterTypes resultType))
    (argumentTypes : RuntimeArguments state.functions arguments parameterTypes) :
    InvocationReady state target arguments resultType := by
  cases callable with
  | scalar =>
    rename_i scalar
    cases scalar <;> simp [valueType] at targetType
  | validationError => simp [valueType] at targetType
  | unit => simp [valueType] at targetType
  | record fields => simp [valueType] at targetType
  | ok inner =>
    rename_i value failure
    cases found : valueType value <;> simp [valueType, found] at targetType
  | error inner =>
    rename_i value success
    cases found : valueType value <;> simp [valueType, found] at targetType
  | nominal inner =>
    rename_i identity base value
    cases found : valueType value <;> simp [valueType, found] at targetType
  | scheme correct => simp [valueType] at targetType
  | functionOk found =>
    simp [valueType] at targetType
    obtain ⟨rfl, rfl⟩ := targetType
    cases argumentTypes with
    | cons typed callable rest => cases rest; exact .ok found typed
  | functionError found =>
    simp [valueType] at targetType
    obtain ⟨rfl, rfl⟩ := targetType
    cases argumentTypes with
    | cons typed callable rest => cases rest; exact .error found typed
  | functionTime found =>
    simp [valueType] at targetType
    obtain ⟨rfl, rfl⟩ := targetType
    cases argumentTypes
    exact .time found
  | instantiatedFunction found capturedSorted signature =>
    simp [valueType] at targetType
    obtain ⟨rfl, rfl⟩ := targetType
    rename_i declaration parameters result captured typeArguments normalized function
    obtain ⟨extension, parameterScope, bodyResult, extensionSorted, solutionSorted, proofSorted, globalValues,
      globalWritable, globalMutable, globalSchemas, parametersOrdinary, localsOrdinary, binders, unique, parameterTypesTyped,
      bodyTyped, resultClosed, bodyUnique, disjoint, propagation⟩ :=
      certified declaration function found captured typeArguments parameters result normalized capturedSorted signature
    have parameterBindings := runtime_arguments_match_parameter_bindings state.functions _ parameterScope arguments parameters
      parameterTypesTyped argumentTypes
    have runtimeParameters := parameter_bindings_have_values _ parameterScope arguments parameters parameterBindings
    have binderUnique : function.parameters.Nodup := by
      rw [binders]
      exact (List.nodup_append.mp (by simpa using unique)).1
    apply InvocationReady.function found signature
    · simpa [binders] using runtimeParameters
    · exact binderUnique
    · exact below
  | instantiatedOk found =>
    simp [valueType] at targetType
    obtain ⟨rfl, rfl⟩ := targetType
    cases argumentTypes with
    | cons typed callable rest => cases rest; exact .okInstantiated found typed
  | instantiatedError found =>
    simp [valueType] at targetType
    obtain ⟨rfl, rfl⟩ := targetType
    cases argumentTypes with
    | cons typed callable rest => cases rest; exact .errorInstantiated found typed
  | instantiatedTime found =>
    simp [valueType] at targetType
    obtain ⟨rfl, rfl⟩ := targetType
    cases argumentTypes
    exact .timeInstantiated found

theorem typed_machine_invocation_progress (state : Machine) (globals : Scope)
    (storage : BindingStorageMap) (types : CellTypes) (target : Value) (arguments : List Value)
    (response : IntValue) (invariant : TypedMachine globals storage types state)
    (current : state.control = .invoke target arguments) :
    ∃ resultType event next, Step state event next ∧ InvocationEntered resultType next := by
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active, inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
  rw [current] at control
  cases control with
  | invoke targetTyped callable argumentTypes resultClosed =>
    rename_i context argumentRuntime resultRuntime result
    have ready := callable_arguments_ready state globals storage target arguments argumentRuntime resultRuntime
      invariant.functions invariant.memory.activation.store callable targetTyped argumentTypes
    obtain ⟨event, next, step, entered⟩ := ready_invocation_steps state target arguments resultRuntime response ready
    have sameState : { state with control := .invoke target arguments } = state := by
      cases state
      simp_all
    rw [sameState] at step
    exact ⟨resultRuntime, event, next, step, entered⟩

end Xsh.Semantics.Connected
