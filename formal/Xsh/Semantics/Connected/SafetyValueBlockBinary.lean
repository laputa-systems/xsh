import Xsh.Semantics.Connected.SafetyValueBlockConditional
import Xsh.Semantics.Connected.SafetyValueBlockCompatibility
import Xsh.Semantics.Connected.TypedBinary

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem integer_has_exact_type (value : Value) (typed : HasType .integer value) :
    valueType value = some .integer := by
  obtain ⟨actual, found, fits⟩ := (has_type_iff_compatible _ _).mp typed
  simpa [integer_fit_is_integer actual fits] using found

theorem boolean_has_exact_type (value : Value) (typed : HasType .boolean value) :
    valueType value = some .boolean := by
  obtain ⟨actual, found, fits⟩ := (has_type_iff_compatible _ _).mp typed
  simpa [boolean_fit_is_boolean actual fits] using found

theorem typed_machine_binary_right (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (operator : Xsh.Syntax.BinaryOp) (left : Value) (right : Node)
    (step : Step { base with control := .value left, stack := .binaryLeft operator right :: base.stack } .internal
      { base with control := .expression right, stack := .binaryRight operator left :: base.stack })
    (typed : TypedMachine globals storage types
      { base with control := .value left, stack := .binaryLeft operator right :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .expression right, stack := .binaryRight operator left :: base.stack } := by
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value leftTyped leftClosed callable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | binaryLeft rightTyped operation =>
        rename_i context leftType runtimeType rightType result
        simp only [TypedDemand.context] at activation
        have newPending : PendingBindingsSafe
            { base with control := .expression right, stack := .binaryRight operator left :: base.stack } := by
          constructor
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.unique
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.fresh
        have rightFresh : FutureNodeBindingsFresh base.store context.activation right := by
          intro binder member
          apply newPending.fresh (context.activation, binder)
          simp [pendingMachineBinders, pendingControlBinders, TypedDemand.context] at *
          simp [activation, member]
        have oldStack : StackTyped
            { base with control := .expression right, stack := .binaryRight operator left :: base.stack }
            (.expression context result) output base.stack := by
          apply stack_typing_survives_pending step remaining
          intro binding member
          apply newPending.fresh binding
          simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, List.map_nil, List.nil_append, List.mem_append]
          right
          simpa [TypedDemand.context, activation] using member
        obtain ⟨rightRuntime, rightClosed⟩ := ground_node_is_closed rightTyped
        refine ⟨.expression context rightType, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
          ?_, ⟨rightRuntime, rightClosed⟩, outputClosed, .expression rightTyped rightClosed rightFresh,
          .cons (.binaryRight leftTyped leftClosed callable operation) middleClosed oldStack, newPending⟩
        simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_short_circuit (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (operator : Xsh.Syntax.BinaryOp) (choice : Bool) (right : Node)
    (step : Step { base with control := .value (.scalar (.boolean choice)), stack := .binaryLeft operator right :: base.stack } .internal
      { base with control := .value (.scalar (.boolean choice)) })
    (typed : TypedMachine globals storage types
      { base with control := .value (.scalar (.boolean choice)), stack := .binaryLeft operator right :: base.stack }) :
    TypedMachine globals storage types { base with control := .value (.scalar (.boolean choice)) } := by
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value leftTyped leftClosed callable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | binaryLeft rightTyped operation =>
        rename_i context leftType runtimeType rightType result
        have resultClosed : monoRuntime result = some .boolean := by
          cases operation <;> simp_all [monoRuntime, HasType, valueType, typeFits]
        have subset : List.Sublist
            (pendingMachineBinders { base with control := .value (.scalar (.boolean choice)) })
            (pendingMachineBinders { base with control := .value (.scalar (.boolean choice)), stack := .binaryLeft operator right :: base.stack }) := by
          simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, List.map_nil, List.nil_append]
          exact List.sublist_append_right _ _
        have newPending := pending_safe_of_sublist
          { base with control := .value (.scalar (.boolean choice)), stack := .binaryLeft operator right :: base.stack }
          { base with control := .value (.scalar (.boolean choice)) } rfl subset pending
        have oldStack : StackTyped { base with control := .value (.scalar (.boolean choice)) }
            (.expression context result) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context] using activation.symm
          · rfl
        refine ⟨.expression context result, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
          ?_, middleClosed, outputClosed, .value (has_type_of_exact _ _ rfl) resultClosed .scalar, oldStack, newPending⟩
        simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem binary_result_runtime_shape (operator : Xsh.Syntax.BinaryOp) (left right result : MonoType) (runtimeType : RuntimeType)
    (typing : BinaryTyping operator left right result) (closed : monoRuntime result = some runtimeType) :
    runtimeType = .integer ∨ runtimeType = .boolean := by
  cases typing <;> simp_all [monoRuntime]

def binaryOutcomeControl : Except Fault Value → Control
  | .ok value => .value value
  | .error fault => .fault fault

theorem typed_machine_binary_finish (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (operator : Xsh.Syntax.BinaryOp) (left right : Value) (outcome : Except Fault Value)
    (evaluated : evaluateBinary operator left right = outcome)
    (step : Step { base with control := .value right, stack := .binaryRight operator left :: base.stack } .internal
      { base with control := binaryOutcomeControl outcome })
    (typed : TypedMachine globals storage types
      { base with control := .value right, stack := .binaryRight operator left :: base.stack }) :
    TypedMachine globals storage types
      { base with control := binaryOutcomeControl outcome } := by
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value rightTyped rightClosed rightCallable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | binaryRight leftTyped leftClosed leftCallable operation =>
        rename_i context rightType rightRuntime leftType result leftRuntime
        have leftExact : valueType left = monoRuntime leftType := by
          cases operation <;> simp only [monoRuntime, Option.some.injEq] at leftClosed ⊢ <;> subst leftRuntime <;>
            first | exact integer_has_exact_type left leftTyped | exact boolean_has_exact_type left leftTyped
        have rightExact : valueType right = monoRuntime rightType := by
          cases operation <;> simp only [monoRuntime, Option.some.injEq] at rightClosed ⊢ <;> subst rightRuntime <;>
            first | exact integer_has_exact_type right rightTyped | exact boolean_has_exact_type right rightTyped
        obtain ⟨resultRuntime, resultClosed, resultSafe⟩ := typed_binary_result_or_integer_fault operator leftType rightType result
          left right operation leftExact rightExact
        rw [evaluated] at resultSafe
        have newPending : PendingBindingsSafe
            { base with control := binaryOutcomeControl outcome } := by
          cases outcome <;> constructor
          all_goals first
            | solve | simpa [binaryOutcomeControl, pendingMachineBinders, pendingControlBinders, pendingStackBinders,
                pendingFrameBinders, frameCallerActivation] using pending.unique
            | solve | simpa [binaryOutcomeControl, pendingMachineBinders, pendingControlBinders, pendingStackBinders,
                pendingFrameBinders, frameCallerActivation] using pending.fresh
        have oldStack : StackTyped
            { base with control := binaryOutcomeControl outcome }
            (.expression context result) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context] using activation.symm
          · rfl
        cases outcome with
        | ok value =>
          have valueTyped : valueType value = some resultRuntime := resultSafe
          have valueCallable : ValueCallables base.functions value := by
            rcases binary_result_runtime_shape operator leftType rightType result resultRuntime operation resultClosed with integer | boolean
            · subst resultRuntime
              obtain ⟨number, rfl⟩ := integer_value_shape value valueTyped
              exact .scalar
            · subst resultRuntime
              obtain ⟨flag, rfl⟩ := boolean_value_shape value valueTyped
              exact .scalar
          refine ⟨.expression context result, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, middleClosed, outputClosed, .value (has_type_of_exact _ _ valueTyped) resultClosed valueCallable, oldStack, newPending⟩
          simpa [ActiveScopeTyped, binaryOutcomeControl, ScopeStateAgrees, TypedDemand.context] using active
        | error fault =>
          cases fault with
          | integer issue =>
            exact ⟨.expression context result, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
              trivial, middleClosed, outputClosed, .integerFault, oldStack, newPending⟩
          | _ => cases resultSafe

end Xsh.Semantics.Connected
