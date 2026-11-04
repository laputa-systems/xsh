import Xsh.Semantics.Connected.SafetyValueBlockBuiltins

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem pending_safe_of_sublist (before after : Machine) (sameStore : after.store = before.store)
    (remaining : List.Sublist (pendingMachineBinders after) (pendingMachineBinders before))
    (pending : PendingBindingsSafe before) : PendingBindingsSafe after := by
  constructor
  · exact remaining.nodup pending.unique
  · intro binding member
    rw [sameStore]
    exact pending.fresh binding (remaining.subset member)

theorem typed_machine_block_complete (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (typed : TypedMachine globals storage types { base with control := .statements [] }) :
    TypedMachine globals storage types { base with control := .completed .unit } := by
  have step := Step.blockComplete base
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | statements ground fresh =>
    rename_i context
    cases ground with
    | nil accepts =>
      have newPending : PendingBindingsSafe { base with control := .completed .unit } := by
        constructor
        · simpa [pendingMachineBinders, pendingControlBinders, declaredBinders] using pending.unique
        · simpa [pendingMachineBinders, pendingControlBinders, declaredBinders] using pending.fresh
      have oldStack : StackTyped { base with control := .completed .unit } (.block context) output base.stack := by
        apply stack_typing_survives_with_pending step stack newPending
        · simpa [TypedDemand.context] using activation.symm
        · rfl
      refine ⟨.block context, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
        ?_, inputClosed, outputClosed, .completed (has_type_of_exact _ _ rfl) rfl accepts .unit, oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_return_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Node) (rest : List StatementNode)
    (typed : TypedMachine globals storage types { base with control := .statements (.returned value :: rest) }) :
    TypedMachine globals storage types { base with control := .expression value, stack := .returnValue :: base.stack } := by
  have step := Step.returnStart base value rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | statements ground fresh =>
    rename_i context
    simp only [TypedDemand.context] at activation
    cases ground with
    | returned valueTyped restTyped =>
      have subset : List.Sublist
          (pendingMachineBinders { base with control := .expression value, stack := .returnValue :: base.stack })
          (pendingMachineBinders { base with control := .statements (.returned value :: rest) }) := by
        simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
          frameCallerActivation, declaredBinders, List.map_nil, List.nil_append, List.map_append, List.append_assoc]
        exact (List.sublist_append_right _ _).append_left _
      have newPending := pending_safe_of_sublist
        { base with control := .statements (.returned value :: rest) }
        { base with control := .expression value, stack := .returnValue :: base.stack } rfl subset pending
      have valueFresh : FutureNodeBindingsFresh base.store context.activation value := by
        intro binder member
        apply fresh binder
        simp [declaredBinders, member]
      have oldStack : StackTyped { base with control := .expression value, stack := .returnValue :: base.stack }
          (.block context) output base.stack := by
        apply stack_typing_survives_pending step stack
        intro binding member
        apply newPending.fresh binding
        simp only [pendingMachineBinders, pendingStackBinders, pendingFrameBinders, frameCallerActivation,
          List.map_nil, List.nil_append, List.mem_append]
        right
        simpa [activation, TypedDemand.context] using member
      obtain ⟨valueRuntime, valueClosed⟩ := ground_node_is_closed valueTyped
      refine ⟨.expression context context.result, output, runtimeSolution, activation, storageEqual,
        proofSorted, runtimeSorted, ?_, ⟨valueRuntime, valueClosed⟩, outputClosed,
        .expression valueTyped valueClosed valueFresh, .cons .returnValue inputClosed oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_return_value (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value)
    (typed : TypedMachine globals storage types { base with control := .value value, stack := .returnValue :: base.stack }) :
    TypedMachine globals storage types { base with control := .returned value } := by
  have step := Step.returnValue base value
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
      | returnValue =>
        rename_i context runtimeType
        have newPending : PendingBindingsSafe { base with control := .returned value } := by
          constructor
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.unique
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.fresh
        have oldStack : StackTyped { base with control := .returned value } (.block context) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context] using activation.symm
          · rfl
        exact ⟨.block context, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
          trivial, middleClosed, outputClosed, .returned valueTyped valueClosed callable, oldStack, newPending⟩

theorem typed_machine_tail_value (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value)
    (typed : TypedMachine globals storage types { base with control := .value value, stack := .tailValue :: base.stack }) :
    TypedMachine globals storage types { base with control := .completed value } := by
  have step := Step.tailValue base value
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
      | tailValue accepts =>
        rename_i context ty runtimeType
        have newPending : PendingBindingsSafe { base with control := .completed value } := by
          constructor
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.unique
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.fresh
        have oldStack : StackTyped { base with control := .completed value } (.block context) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context] using activation.symm
          · rfl
        refine ⟨.block context, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
          ?_, middleClosed, outputClosed, .completed valueTyped valueClosed accepts callable, oldStack, newPending⟩
        simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_evaluate_tail (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Node)
    (typed : TypedMachine globals storage types { base with control := .statements [.evaluated value] }) :
    TypedMachine globals storage types { base with control := .expression value, stack := .tailValue :: base.stack } := by
  have step := Step.evaluateTail base value
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | statements ground fresh =>
    rename_i context
    simp only [TypedDemand.context] at activation
    cases ground with
    | evaluatedTail valueTyped accepts =>
      have newPending : PendingBindingsSafe { base with control := .expression value, stack := .tailValue :: base.stack } := by
        constructor
        · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, declaredBinders] using pending.unique
        · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, declaredBinders] using pending.fresh
      have valueFresh : FutureNodeBindingsFresh base.store context.activation value := by
        simpa [FutureNodeBindingsFresh, FutureBindingsFresh, declaredBinders] using fresh
      have oldStack : StackTyped { base with control := .expression value, stack := .tailValue :: base.stack }
          (.block context) output base.stack := by
        apply stack_typing_survives_pending step stack
        intro binding member
        apply newPending.fresh binding
        simp only [pendingMachineBinders, pendingStackBinders, pendingFrameBinders, frameCallerActivation,
          List.map_nil, List.nil_append, List.mem_append]
        right
        simpa [TypedDemand.context, activation] using member
      obtain ⟨runtimeType, closed⟩ := ground_node_is_closed valueTyped
      refine ⟨.expression context _, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
        ?_, ⟨runtimeType, closed⟩, outputClosed, .expression valueTyped closed valueFresh,
        .cons (.tailValue accepts) inputClosed oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_evaluate_next (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Node) (next : StatementNode) (rest : List StatementNode)
    (typed : TypedMachine globals storage types { base with control := .statements (.evaluated value :: next :: rest) }) :
    TypedMachine globals storage types { base with control := .expression value, stack := .discard (next :: rest) :: base.stack } := by
  have step := Step.evaluateAndContinue base value next rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | statements ground fresh =>
    rename_i context
    simp only [TypedDemand.context] at activation
    cases ground with
    | evaluatedNext valueTyped restTyped =>
      have newPending : PendingBindingsSafe { base with control := .expression value, stack := .discard (next :: rest) :: base.stack } := by
        constructor
        · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, declaredBinders, List.map_append, List.append_assoc] using pending.unique
        · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
            frameCallerActivation, declaredBinders, List.map_append, List.append_assoc] using pending.fresh
      have valueFresh : FutureNodeBindingsFresh base.store context.activation value := by
        intro binder member
        apply fresh binder
        simp [declaredBinders, member]
      have restFresh : FutureBindingsFresh base.store context.activation (next :: rest) := by
        intro binder member
        apply fresh binder
        simp [declaredBinders, member]
      have oldStack : StackTyped { base with control := .expression value, stack := .discard (next :: rest) :: base.stack }
          (.block context) output base.stack := by
        apply stack_typing_survives_pending step stack
        intro binding member
        apply newPending.fresh binding
        simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
        right
        right
        simpa [TypedDemand.context, activation, frameCallerActivation] using member
      obtain ⟨runtimeType, closed⟩ := ground_node_is_closed valueTyped
      refine ⟨.expression context _, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
        ?_, ⟨runtimeType, closed⟩, outputClosed, .expression valueTyped closed valueFresh,
        .cons (.discard restTyped restFresh) inputClosed oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_discarded (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value) (rest : List StatementNode)
    (typed : TypedMachine globals storage types { base with control := .value value, stack := .discard rest :: base.stack }) :
    TypedMachine globals storage types { base with control := .statements rest } := by
  have step := Step.discarded base value rest
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
      | discard restTyped restFresh =>
        rename_i context ty runtimeType
        have newPending : PendingBindingsSafe { base with control := .statements rest } := by
          constructor
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.unique
          · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation] using pending.fresh
        have oldStack : StackTyped { base with control := .statements rest } (.block context) output base.stack := by
          apply stack_typing_survives_with_pending step remaining newPending
          · simpa [TypedDemand.context] using activation.symm
          · rfl
        refine ⟨.block context, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
          ?_, middleClosed, outputClosed, .statements restTyped restFresh, oldStack, newPending⟩
        simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
