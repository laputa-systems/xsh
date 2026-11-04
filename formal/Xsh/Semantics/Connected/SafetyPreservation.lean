import Xsh.Semantics.Connected.SafetyNominalPreservation
import Xsh.Semantics.Connected.SafetyNoFault

namespace Xsh.Semantics.Connected
open Xsh.Typing.Connected

theorem typed_machine_step_preservation {before after : Machine} {event : Label}
    (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (response : Xsh.Values.IntValue) (typed : TypedMachine globals storage types before)
    (step : Step before event after) : TypedMachine globals storage types after := by
  have original := step
  cases step
  all_goals first
    | solve | apply typed_machine_value_block_start <;> assumption
    | solve | apply typed_machine_value_block_complete <;> assumption
    | solve | apply typed_machine_integer <;> assumption
    | solve | apply typed_machine_boolean <;> assumption
    | solve | apply typed_machine_string <;> assumption
    | solve | apply typed_machine_read <;> assumption
    | solve | apply typed_machine_read_function <;> assumption
    | solve | apply typed_machine_binary_start <;> assumption
    | solve | apply typed_machine_binary_right <;> assumption
    | solve | apply typed_machine_short_circuit <;> assumption
    | solve | apply typed_machine_empty_expression_branch <;> assumption
    | solve | apply typed_machine_conditional_start <;> assumption
    | solve | apply typed_machine_expression_branch <;> assumption
    | solve | apply typed_machine_field_start <;> assumption
    | solve | apply typed_machine_field_value <;> assumption
    | solve | apply typed_machine_propagation_start <;> assumption
    | solve | apply typed_machine_propagate_ok <;> assumption
    | solve | apply typed_machine_propagate_error <;> assumption
    | solve | apply typed_machine_require_start <;> assumption
    | solve | apply typed_machine_require_success <;> assumption
    | solve | apply typed_machine_require_failure <;> assumption
    | solve | apply typed_machine_empty_record <;> assumption
    | solve | apply typed_machine_record_start <;> assumption
    | solve | apply typed_machine_record_next <;> assumption
    | solve | apply typed_machine_record_finish <;> assumption
    | solve | apply compatible_machine_binding_start <;> assumption
    | solve | apply compatible_typed_machine_binding_commit <;> assumption
    | solve | apply compatible_machine_assignment_start <;> assumption
    | solve | apply compatible_machine_assignment_commit <;> assumption
    | solve | apply typed_machine_evaluate_tail <;> assumption
    | solve | apply typed_machine_evaluate_next <;> assumption
    | solve | apply typed_machine_discarded <;> assumption
    | solve | apply typed_machine_tail_value <;> assumption
    | solve | apply typed_machine_tail_branch_complete <;> assumption
    | solve | apply typed_machine_non_tail_branch_complete <;> assumption
    | solve | apply typed_machine_return_start <;> assumption
    | solve | apply typed_machine_return_value <;> assumption
    | solve | apply typed_machine_call_start <;> assumption
    | solve | apply typed_machine_call_without_arguments <;> assumption
    | solve | apply typed_machine_call_first_argument <;> assumption
    | solve | apply typed_machine_call_next_argument <;> assumption
    | solve | apply typed_machine_call_arguments_complete <;> assumption
    | solve | apply typed_machine_function_call <;> assumption
    | solve | apply typed_machine_ok_call <;> assumption
    | solve | apply typed_machine_error_call <;> assumption
    | solve | apply typed_machine_instantiated_ok_call <;> assumption
    | solve | apply typed_machine_instantiated_error_call <;> assumption
    | solve | apply typed_machine_instantiated_time_call <;> assumption
    | solve | apply typed_machine_time_call <;> assumption
    | solve | apply typed_machine_return_to_caller <;> assumption
    | solve | apply typed_machine_completion_to_caller <;> assumption
    | solve | apply typed_machine_propagation_to_caller <;> assumption
    | solve | apply typed_machine_return_unwind <;> assumption
    | solve | apply typed_machine_propagation_unwind <;> assumption
    | solve | apply typed_machine_empty_statement_branch <;> assumption
    | solve | apply typed_machine_statement_branch_start <;> assumption
    | solve | apply typed_machine_statement_branch_true <;> assumption
    | solve | apply typed_machine_statement_branch_false <;> assumption
    | solve | apply typed_machine_block_complete <;> assumption
    | skip
  case binaryResult state operator left right result evaluated =>
    simpa [binaryOutcomeControl] using typed_machine_binary_finish globals storage types state operator left right
      (.ok result) evaluated original typed
  case binaryFault state operator left right fault evaluated =>
    simpa [binaryOutcomeControl] using typed_machine_binary_finish globals storage types state operator left right
      (.error fault) evaluated original typed
  case conditionalTrue state condition branch rest otherwise =>
    simpa using typed_machine_expression_branch globals storage types state true condition branch rest otherwise typed
  case conditionalFalse state condition branch rest otherwise =>
    simpa using typed_machine_expression_branch globals storage types state false condition branch rest otherwise typed
  case propagateUnwind state value frame notCaller =>
    obtain ⟨input, output, _, _, _, _, _, _, _, _, control, _, _⟩ := typed.focus
    obtain ⟨error, oldType, same⟩ := typed_propagated_control_shape _ input value control
    subst value
    exact typed_machine_propagation_unwind globals storage types state error oldType frame notCaller typed
  all_goals
    exfalso
    obtain ⟨issue, impossible⟩ := typed_machine_excludes_internal_fault _ _ _ globals storage types typed original response _ rfl
    cases impossible
end Xsh.Semantics.Connected
