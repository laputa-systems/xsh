import Xsh.Semantics.Connected.SafetyValueBlockCompletion

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem noncaller_frame_context {state : Machine} {input output : TypedDemand} {frame : Frame}
    (typed : FrameTyped state input output frame) (notCaller : isCaller frame = false) :
    input.context.solution = output.context.solution ∧ input.context.runtimeSolution = output.context.runtimeSolution ∧
      input.context.storage = output.context.storage ∧ input.context.result = output.context.result ∧
      input.context.activation = output.context.activation := by
  cases typed <;> simp_all [TypedDemand.context, isCaller]

theorem pending_after_noncaller_unwind (base : Machine) (control : Control) (frame : Frame)
    (notCaller : isCaller frame = false) (noBinders : pendingControlBinders control = [])
    (pending : PendingBindingsSafe { base with control := control, stack := frame :: base.stack }) :
    PendingBindingsSafe { base with control := control } := by
  have unchanged : frameCallerActivation base.activation frame = base.activation := by
    cases frame <;> simp_all [isCaller, frameCallerActivation]
  have unique := pending.unique
  have fresh := pending.fresh
  simp only [pendingMachineBinders, noBinders, List.map_nil, List.nil_append, pendingStackBinders, unchanged] at unique fresh ⊢
  constructor
  · simpa [pendingMachineBinders, noBinders, pendingStackBinders] using (List.nodup_append.mp unique).2.1
  · intro binding member
    have member : binding ∈ pendingStackBinders base.activation base.stack := by
      simpa [pendingMachineBinders, noBinders, pendingStackBinders] using member
    apply fresh binding
    exact List.mem_append_right _ member

theorem typed_machine_return_unwind (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Value) (frame : Frame) (notCaller : isCaller frame = false)
    (typed : TypedMachine globals storage types { base with control := .returned value, stack := frame :: base.stack }) :
    TypedMachine globals storage types { base with control := .returned value } := by
  have step := Step.returnUnwind base value frame notCaller
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases stack with
  | cons frameTyped middleClosed remaining =>
    rename_i middle
    obtain ⟨solutionSame, runtimeSame, storageSame, resultSame, activationSame⟩ := noncaller_frame_context frameTyped notCaller
    have newPending := pending_after_noncaller_unwind base (.returned value) frame notCaller rfl pending
    have oldStack : StackTyped { base with control := .returned value } middle output base.stack := by
      apply stack_typing_survives_with_pending step remaining newPending
      · simpa [activationSame] using activation.symm
      · rfl
    cases control with
    | returned valueTyped valueClosed callable =>
      refine ⟨middle, output, ?_, ?_, ?_, ?_, runtimeSorted, ?_, middleClosed, outputClosed,
        .returned valueTyped ?_ callable, oldStack, newPending⟩
      · simpa [runtimeSame] using runtimeSolution
      · simpa [activationSame] using activation
      · simpa [storageSame] using storageEqual
      · simpa [solutionSame] using proofSorted
      · trivial
      · simpa [resultSame] using valueClosed

theorem typed_machine_propagation_unwind (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (error : Value) (oldType : RuntimeType) (frame : Frame) (notCaller : isCaller frame = false)
    (typed : TypedMachine globals storage types
      { base with control := .propagated (.error error oldType), stack := frame :: base.stack }) :
    TypedMachine globals storage types { base with control := .propagated (.error error oldType) } := by
  have step := Step.propagateUnwind base (.error error oldType) frame notCaller
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases stack with
  | cons frameTyped middleClosed remaining =>
    rename_i middle
    obtain ⟨solutionSame, runtimeSame, storageSame, resultSame, activationSame⟩ := noncaller_frame_context frameTyped notCaller
    have newPending := pending_after_noncaller_unwind base (.propagated (.error error oldType)) frame notCaller rfl pending
    have oldStack : StackTyped { base with control := .propagated (.error error oldType) } middle output base.stack := by
      apply stack_typing_survives_with_pending step remaining newPending
      · simpa [activationSame] using activation.symm
      · rfl
    cases control with
    | propagated matching errorTyped failureClosed callable =>
      rename_i success failure runtimeFailure
      refine ⟨middle, output, ?_, ?_, ?_, ?_, runtimeSorted, ?_, middleClosed, outputClosed,
        .propagated (result := success) ?_ errorTyped failureClosed callable, oldStack, newPending⟩
      · simpa [runtimeSame] using runtimeSolution
      · simpa [activationSame] using activation
      · simpa [storageSame] using storageEqual
      · simpa [solutionSame] using proofSorted
      · trivial
      · simpa [resultSame] using matching

end Xsh.Semantics.Connected
