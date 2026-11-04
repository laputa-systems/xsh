import Xsh.Semantics.Connected.SafetyValueBlockUnwind
import Xsh.Semantics.Connected.SafetyValueBlockFrames

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected
open Xsh.Values

def MachineTerminal (state : Machine) : Prop :=
  (∃ value, state.control = .value value ∧ state.stack = []) ∨
  (∃ value, state.control = .returned value ∧ state.stack = []) ∨
  (∃ value, state.control = .completed value ∧ state.stack = []) ∨
  (∃ value, state.control = .propagated value ∧ state.stack = []) ∨
  (∃ fault, state.control = .fault (.integer fault))

theorem typed_propagated_control_shape (state : Machine) (demand : TypedDemand) (value : Value)
    (typed : ControlTyped state demand (.propagated value)) : ∃ error oldType, value = .error error oldType := by
  cases typed with
  | propagated => exact ⟨_, _, rfl⟩

theorem typed_fault_control_shape (state : Machine) (demand : TypedDemand) (fault : Fault)
    (typed : ControlTyped state demand (.fault fault)) : ∃ integerFault, fault = .integer integerFault := by
  cases typed with
  | integerFault => exact ⟨_, rfl⟩

theorem typed_machine_local_progress (state : Machine) (globals : Scope) (storage : BindingStorageMap)
    (types : CellTypes) (response : IntValue) (invariant : TypedMachine globals storage types state) :
    MachineTerminal state ∨ ∃ event next, Step state event next := by
  cases current : state.control with
  | expression node =>
    obtain ⟨next, step, safe⟩ := typed_machine_expression_progress state globals storage types node invariant current
    exact Or.inr ⟨.internal, next, step⟩
  | statements body =>
    obtain ⟨next, step, safe⟩ := typed_machine_statements_progress state globals storage types body invariant current
    exact Or.inr ⟨.internal, next, step⟩
  | invoke target arguments =>
    obtain ⟨result, event, next, step, entered⟩ :=
      typed_machine_invocation_progress state globals storage types target arguments response invariant current
    exact Or.inr ⟨event, next, step⟩
  | value value =>
    rcases typed_machine_value_progress state globals storage types value invariant current with empty | ⟨next, step, safe⟩
    · exact Or.inl (Or.inl ⟨value, current, empty⟩)
    · exact Or.inr ⟨.internal, next, step⟩
  | returned value =>
    rcases typed_machine_return_progress state globals storage types value invariant current with empty | ⟨next, step⟩
    · exact Or.inl (Or.inr (Or.inl ⟨value, current, empty⟩))
    · exact Or.inr ⟨.internal, next, step⟩
  | completed value =>
    rcases typed_machine_completed_progress state globals storage types value invariant current with empty | ⟨next, step⟩
    · exact Or.inl (Or.inr (Or.inr (Or.inl ⟨value, current, empty⟩)))
    · exact Or.inr ⟨.internal, next, step⟩
  | propagated value =>
    obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
      inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
    rw [current] at control
    obtain ⟨error, oldType, rfl⟩ := typed_propagated_control_shape state input value control
    rcases typed_machine_propagated_progress state globals storage types error oldType invariant current with empty | ⟨next, step⟩
    · exact Or.inl (Or.inr (Or.inr (Or.inr (Or.inl ⟨.error error oldType, current, empty⟩))))
    · exact Or.inr ⟨.internal, next, step⟩
  | fault fault =>
    obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
      inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
    rw [current] at control
    obtain ⟨integerFault, rfl⟩ := typed_fault_control_shape state input fault control
    exact Or.inl (Or.inr (Or.inr (Or.inr (Or.inr ⟨integerFault, current⟩))))


theorem typed_machine_safe_progress (state : Machine) (globals : Scope) (storage : BindingStorageMap)
    (types : CellTypes) (response : IntValue) (invariant : TypedMachine globals storage types state) :
    MachineTerminal state ∨ ∃ event next, Step state event next ∧ PermittedControl next.control := by
  cases current : state.control with
  | expression node =>
    obtain ⟨next, step, safe⟩ := typed_machine_expression_progress state globals storage types node invariant current
    exact Or.inr ⟨.internal, next, step, by cases h : next.control <;> simp_all [ExpressionTarget, PermittedControl]⟩
  | statements body =>
    obtain ⟨next, step, safe⟩ := typed_machine_statements_progress state globals storage types body invariant current
    exact Or.inr ⟨.internal, next, step, by cases h : next.control <;> simp_all [StatementTarget, PermittedControl]⟩
  | invoke target arguments =>
    obtain ⟨result, event, next, step, entered⟩ :=
      typed_machine_invocation_progress state globals storage types target arguments response invariant current
    exact Or.inr ⟨event, next, step, by cases entered <;> trivial⟩
  | value value =>
    rcases typed_machine_value_progress state globals storage types value invariant current with empty | ⟨next, step, safe⟩
    · exact Or.inl (Or.inl ⟨value, current, empty⟩)
    · exact Or.inr ⟨.internal, next, step, safe⟩
  | returned value =>
    rcases typed_machine_return_progress state globals storage types value invariant current with empty | ⟨next, step⟩
    · exact Or.inl (Or.inr (Or.inl ⟨value, current, empty⟩))
    · exact Or.inr ⟨.internal, next, step, by cases step <;> simp_all [PermittedControl]⟩
  | completed value =>
    rcases typed_machine_completed_progress state globals storage types value invariant current with empty | ⟨next, step⟩
    · exact Or.inl (Or.inr (Or.inr (Or.inl ⟨value, current, empty⟩)))
    · exact Or.inr ⟨.internal, next, step, by cases step <;> simp_all [PermittedControl]⟩
  | propagated value =>
    obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
      inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
    rw [current] at control
    obtain ⟨error, oldType, rfl⟩ := typed_propagated_control_shape state input value control
    rcases typed_machine_propagated_progress state globals storage types error oldType invariant current with empty | ⟨next, step⟩
    · exact Or.inl (Or.inr (Or.inr (Or.inr (Or.inl ⟨.error error oldType, current, empty⟩))))
    · exact Or.inr ⟨.internal, next, step, by cases step <;> simp_all [PermittedControl]⟩
  | fault fault =>
    obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
      inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
    rw [current] at control
    obtain ⟨integerFault, rfl⟩ := typed_fault_control_shape state input fault control
    exact Or.inl (Or.inr (Or.inr (Or.inr (Or.inr ⟨integerFault, current⟩))))

end Xsh.Semantics.Connected
