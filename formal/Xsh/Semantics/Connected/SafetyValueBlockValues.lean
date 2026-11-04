import Xsh.Semantics.Connected.SafetyValueBlockStatements

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected
open Xsh.Values

theorem typed_machine_expression_value (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (node : Node) (value : Value) (event : Label)
    (step : Step { base with control := .expression node } event { base with control := .value value })
    (result : ∀ (context : TypingContext) (ty : MonoType) (runtimeType : RuntimeType),
      GroundNode context.solution context.runtimeSolution context.storage context.scope context.result node ty →
      monoRuntime ty = some runtimeType → HasType runtimeType value ∧ ValueCallables base.functions value)
    (typed : TypedMachine globals storage types { base with control := .expression node }) :
    TypedMachine globals storage types { base with control := .value value } := by
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | expression ground closed fresh =>
    rename_i context ty runtimeType
    obtain ⟨valueTyped, callable⟩ := result context ty runtimeType ground closed
    have newPending : PendingBindingsSafe { base with control := .value value } := by
      apply pending_safe_of_sublist { base with control := .expression node } { base with control := .value value } rfl
      · simp only [pendingMachineBinders, pendingControlBinders, List.map_nil, List.nil_append]
        exact List.sublist_append_right _ _
      · exact pending
    have oldStack : StackTyped { base with control := .value value } (.expression context ty) output base.stack := by
      apply stack_typing_survives_with_pending step stack newPending
      · simpa [TypedDemand.context] using activation.symm
      · rfl
    refine ⟨.expression context ty, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
      ?_, inputClosed, outputClosed, .value valueTyped closed callable, oldStack, newPending⟩
    simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_integer (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (text : String) (value : IntValue) (parsed : parseInteger text = some value)
    (typed : TypedMachine globals storage types { base with control := .expression (.integer text) }) :
    TypedMachine globals storage types { base with control := .value (.scalar (.integer value)) } := by
  apply typed_machine_expression_value globals storage types base (.integer text) (.scalar (.integer value)) .internal
    (Step.integer base text value parsed) ?_ typed
  intro context ty runtimeType ground closed
  cases ground with
  | integer parsed' => simp [monoRuntime] at closed; subst runtimeType; exact ⟨has_type_of_exact _ _ rfl, .scalar⟩

theorem typed_machine_boolean (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Bool)
    (typed : TypedMachine globals storage types { base with control := .expression (.boolean value) }) :
    TypedMachine globals storage types { base with control := .value (.scalar (.boolean value)) } := by
  apply typed_machine_expression_value globals storage types base (.boolean value) (.scalar (.boolean value)) .internal
    (Step.boolean base value) ?_ typed
  intro context ty runtimeType ground closed
  cases ground with
  | boolean => simp [monoRuntime] at closed; subst runtimeType; exact ⟨has_type_of_exact _ _ rfl, .scalar⟩

theorem typed_machine_string (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : String)
    (typed : TypedMachine globals storage types { base with control := .expression (.string value) }) :
    TypedMachine globals storage types { base with control := .value (.scalar (.string value)) } := by
  apply typed_machine_expression_value globals storage types base (.string value) (.scalar (.string value)) .internal
    (Step.string base value) ?_ typed
  intro context ty runtimeType ground closed
  cases ground with
  | string => simp [monoRuntime] at closed; subst runtimeType; exact ⟨has_type_of_exact _ _ rfl, .scalar⟩

end Xsh.Semantics.Connected
