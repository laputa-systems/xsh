import Xsh.Semantics.Connected.SafetyValueBlockBinary

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem function_has_exact_type (value : Value) (parameters result : RuntimeType)
    (typed : HasType (.function parameters result) value) :
    valueType value = some (.function parameters result) := by
  obtain ⟨actual, found, fits⟩ := (has_type_iff_compatible _ _).mp typed
  have same : actual = .function parameters result := by cases actual <;> simp_all [typeFits]
  simpa [same] using found

theorem closed_function_has_exact_type (value : Value) (parameters result : MonoType) (runtimeType : RuntimeType)
    (closed : monoRuntime (.function parameters result) = some runtimeType) (typed : HasType runtimeType value) :
    valueType value = some runtimeType := by
  cases parameterType : monoRuntime parameters with
  | none => simp [monoRuntime, parameterType] at closed
  | some parameterRuntime =>
    cases resultType : monoRuntime result with
    | none => simp [monoRuntime, parameterType, resultType] at closed
    | some resultRuntime =>
      simp [monoRuntime, parameterType, resultType] at closed
      subst runtimeType
      exact function_has_exact_type value parameterRuntime resultRuntime typed

theorem typed_machine_call_without_arguments (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (target : Value)
    (typed : TypedMachine globals storage types { base with control := .value target, stack := .callTarget [] :: base.stack }) :
    TypedMachine globals storage types { base with control := .invoke target [] } := by
  have step := Step.callWithoutArguments base target
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value targetTyped targetClosed targetCallable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | callTarget argumentsTyped =>
        rename_i context runtimeType argumentTypes result
        cases argumentsTyped with
        | nil =>
          cases resultClosed : monoRuntime result with
          | none => simp [monoRuntime, resultClosed] at targetClosed
          | some runtimeResult =>
            simp [monoRuntime, resultClosed] at targetClosed
            subst runtimeType
            have newPending : PendingBindingsSafe { base with control := .invoke target [] } := by
              constructor
              · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                  frameCallerActivation, argumentBinders] using pending.unique
              · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                  frameCallerActivation, argumentBinders] using pending.fresh
            have oldStack : StackTyped { base with control := .invoke target [] } (.expression context result) output base.stack := by
              apply stack_typing_survives_with_pending step remaining newPending
              · simpa [TypedDemand.context] using activation.symm
              · rfl
            refine ⟨.expression context result, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
              ?_, middleClosed, outputClosed, .invoke (function_has_exact_type target _ _ targetTyped) targetCallable .nil resultClosed, oldStack, newPending⟩
            simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_call_first_argument (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (target : Value) (first : Node) (rest : List Node)
    (typed : TypedMachine globals storage types
      { base with control := .value target, stack := .callTarget (first :: rest) :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .expression first, stack := .callArguments target [] rest :: base.stack } := by
  have step := Step.callFirstArgument base target first rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value targetTyped targetClosed targetCallable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | callTarget argumentsTyped =>
        rename_i context runtimeType argumentTypes result
        simp only [TypedDemand.context] at activation
        cases argumentsTyped with
        | cons firstTyped restTyped =>
          rename_i firstType restTypes
          have newPending : PendingBindingsSafe
              { base with control := .expression first, stack := .callArguments target [] rest :: base.stack } := by
            constructor
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation, argumentBinders, List.map_append, List.append_assoc] using pending.unique
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation, argumentBinders, List.map_append, List.append_assoc] using pending.fresh
          have firstFresh : FutureNodeBindingsFresh base.store context.activation first := by
            intro binder member
            apply newPending.fresh (context.activation, binder)
            simp [pendingMachineBinders, pendingControlBinders, TypedDemand.context] at *
            simp [activation, member]
          have oldStack : StackTyped
              { base with control := .expression first, stack := .callArguments target [] rest :: base.stack }
              (.expression context result) output base.stack := by
            apply stack_typing_survives_pending step remaining
            intro binding member
            apply newPending.fresh binding
            simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
            right
            right
            simpa [TypedDemand.context, activation, frameCallerActivation] using member
          have frame : FrameTyped
              { base with control := .expression first, stack := .callArguments target [] rest :: base.stack }
              (.expression context firstType) (.expression context result) (.callArguments target [] rest) := by
            refine FrameTyped.callArguments (state := { base with control := .expression first, stack := .callArguments target [] rest :: base.stack })
              TypedValues.nil restTyped ?_ (by simpa using targetCallable)
            exact ⟨runtimeType, by simpa using targetClosed,
              closed_function_has_exact_type target _ _ runtimeType targetClosed targetTyped⟩
          obtain ⟨firstRuntime, firstClosed⟩ := ground_node_is_closed firstTyped
          refine ⟨.expression context firstType, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, ⟨firstRuntime, firstClosed⟩, outputClosed, .expression firstTyped firstClosed firstFresh,
            .cons frame middleClosed oldStack, newPending⟩
          simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_values_append (functions : FunctionTable) (values : List Value) (types : List MonoType)
    (typed : TypedValues functions values types) (value : Value) (ty : MonoType) (runtimeType : RuntimeType)
    (valueTyped : HasType runtimeType value) (closed : monoRuntime ty = some runtimeType)
    (callable : ValueCallables functions value) : TypedValues functions (values ++ [value]) (types ++ [ty]) := by
  induction typed with
  | nil => exact .cons valueTyped closed callable .nil
  | cons headTyped headClosed headCallable tail ih => exact .cons headTyped headClosed headCallable ih

theorem typed_machine_call_next_argument (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (target value : Value) (done : List Value) (next : Node) (rest : List Node)
    (typed : TypedMachine globals storage types
      { base with control := .value value, stack := .callArguments target done (next :: rest) :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .expression next, stack := .callArguments target (done ++ [value]) rest :: base.stack } := by
  have step := Step.callNextArgument base target value done next rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed callable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | callArguments doneTyped nextArguments targetTyped targetCallable =>
        rename_i context inputType runtimeType doneTypes restTypes result
        simp only [TypedDemand.context] at activation
        cases nextArguments with
        | cons nextTyped restTyped =>
          rename_i nextType afterTypes
          have newPending : PendingBindingsSafe
              { base with control := .expression next, stack := .callArguments target (done ++ [value]) rest :: base.stack } := by
            constructor
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation, argumentBinders, List.map_append, List.append_assoc] using pending.unique
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation, argumentBinders, List.map_append, List.append_assoc] using pending.fresh
          have nextFresh : FutureNodeBindingsFresh base.store context.activation next := by
            intro binder member
            apply newPending.fresh (context.activation, binder)
            simp [pendingMachineBinders, pendingControlBinders, activation, member]
          have oldStack : StackTyped
              { base with control := .expression next, stack := .callArguments target (done ++ [value]) rest :: base.stack }
              (.expression context result) output base.stack := by
            apply stack_typing_survives_pending step remaining
            intro binding member
            apply newPending.fresh binding
            simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
            right
            right
            simpa [TypedDemand.context, activation, frameCallerActivation] using member
          have frame : FrameTyped
              { base with control := .expression next, stack := .callArguments target (done ++ [value]) rest :: base.stack }
              (.expression context nextType) (.expression context result) (.callArguments target (done ++ [value]) rest) := by
            refine FrameTyped.callArguments (state := { base with control := .expression next, stack := .callArguments target (done ++ [value]) rest :: base.stack })
              (typed_values_append _ _ _ doneTyped value inputType runtimeType valueTyped valueClosed callable) restTyped ?_ (by simpa using targetCallable)
            simpa [List.foldr_append] using targetTyped
          obtain ⟨nextRuntime, nextClosed⟩ := ground_node_is_closed nextTyped
          refine ⟨.expression context nextType, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, ⟨nextRuntime, nextClosed⟩, outputClosed, .expression nextTyped nextClosed nextFresh,
            .cons frame middleClosed oldStack, newPending⟩
          simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_values_runtime_arguments (functions : FunctionTable) (values : List Value) (types : List MonoType)
    (typed : TypedValues functions values types) (runtimeTypes : RuntimeType)
    (closed : monoRuntime (types.foldr MonoType.argument .argumentsEmpty) = some runtimeTypes) :
    RuntimeArguments functions values runtimeTypes := by
  induction typed generalizing runtimeTypes with
  | nil => simp [monoRuntime] at closed; subst runtimeTypes; exact .nil
  | @cons value values ty types runtimeType valueTyped valueClosed callable tail ih =>
    cases restClosed : monoRuntime (types.foldr MonoType.argument .argumentsEmpty) with
    | none => simp [monoRuntime, valueClosed, restClosed] at closed
    | some restRuntime =>
      simp [monoRuntime, valueClosed, restClosed] at closed
      subst runtimeTypes
      exact .cons valueTyped callable (ih restRuntime restClosed)

theorem typed_machine_call_arguments_complete (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (target value : Value) (done : List Value)
    (typed : TypedMachine globals storage types
      { base with control := .value value, stack := .callArguments target done [] :: base.stack }) :
    TypedMachine globals storage types { base with control := .invoke target (done ++ [value]) } := by
  have step := Step.callArgumentsComplete base target value done
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed callable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | callArguments doneTyped restTyped targetTyped targetCallable =>
        rename_i context inputType runtimeType doneTypes restTypes result
        cases restTyped with
        | nil =>
          have valuesTyped := typed_values_append _ _ _ doneTyped value inputType runtimeType valueTyped valueClosed callable
          obtain ⟨targetRuntime, targetClosed, targetValue⟩ := targetTyped
          have targetClosed' : monoRuntime (.function ((doneTypes ++ [inputType]).foldr MonoType.argument .argumentsEmpty) result) = some targetRuntime := by
            simpa [List.foldr_append] using targetClosed
          cases argumentsClosed : monoRuntime ((doneTypes ++ [inputType]).foldr MonoType.argument .argumentsEmpty) with
          | none =>
            simp only [monoRuntime, argumentsClosed] at targetClosed'
            cases targetClosed'
          | some argumentRuntime =>
            cases resultClosed : monoRuntime result with
            | none =>
              simp only [monoRuntime, argumentsClosed, resultClosed] at targetClosed'
              cases targetClosed'
            | some resultRuntime =>
              simp only [monoRuntime, argumentsClosed, resultClosed] at targetClosed'
              cases targetClosed'
              have argumentsTyped := typed_values_runtime_arguments _ _ _ valuesTyped argumentRuntime argumentsClosed
              have newPending : PendingBindingsSafe { base with control := .invoke target (done ++ [value]) } := by
                constructor
                · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                    frameCallerActivation, argumentBinders] using pending.unique
                · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                    frameCallerActivation, argumentBinders] using pending.fresh
              have oldStack : StackTyped { base with control := .invoke target (done ++ [value]) }
                  (.expression context result) output base.stack := by
                apply stack_typing_survives_with_pending step remaining newPending
                · simpa [TypedDemand.context] using activation.symm
                · rfl
              refine ⟨.expression context result, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
                ?_, middleClosed, outputClosed, .invoke targetValue targetCallable argumentsTyped resultClosed, oldStack, newPending⟩
              simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
