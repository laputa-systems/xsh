import Xsh.Semantics.Connected.SafetyValueBlockInvariant
namespace Xsh.Semantics.Connected
open Xsh.Foundation Xsh.Typing.Connected

theorem ground_node_type {solution runtimeSolution : Substitution} {storage : BindingStorageMap}
    {scope result node ty} (ground : GroundNode solution runtimeSolution storage scope result node ty) :
    NodeHasTypeIn solution (some result) scope node ty := by
  apply GroundNode.rec
    (motive_1 := fun scope result node ty _ => NodeHasTypeIn solution (some result) scope node ty)
    (motive_2 := fun scope result nodes types _ => NodesHaveTypesIn solution (some result) scope nodes types)
    (motive_3 := fun scope result nodes types _ => NodeFieldsHaveTypesIn solution (some result) scope nodes types)
    (motive_4 := fun scope result branches ty _ => NodeBranchesHaveTypeIn solution (some result) scope branches ty)
    (motive_5 := fun scope result completion body _ => NodeBlockHasCompletion solution scope result completion body)
    (motive_6 := fun scope result completion branches _ => NodeBranchesHaveCompletion solution scope result completion branches)
    <;> intros
    <;> first | assumption | solve | constructor <;> assumption


theorem ground_block_completion {solution runtimeSolution : Substitution} {storage : BindingStorageMap}
    {scope result completion body} (ground : GroundBlock solution runtimeSolution storage scope result completion body) :
    NodeBlockHasCompletion solution scope result completion body := by
  apply GroundBlock.rec
    (motive_1 := fun scope result node ty _ => NodeHasTypeIn solution (some result) scope node ty)
    (motive_2 := fun scope result nodes types _ => NodesHaveTypesIn solution (some result) scope nodes types)
    (motive_3 := fun scope result nodes types _ => NodeFieldsHaveTypesIn solution (some result) scope nodes types)
    (motive_4 := fun scope result branches ty _ => NodeBranchesHaveTypeIn solution (some result) scope branches ty)
    (motive_5 := fun scope result completion body _ => NodeBlockHasCompletion solution scope result completion body)
    (motive_6 := fun scope result completion branches _ => NodeBranchesHaveCompletion solution scope result completion branches)
    <;> intros
    <;> first | assumption | solve | constructor <;> assumption

theorem ground_propagation_requires_result {solution runtimeSolution : Substitution} {storage : BindingStorageMap}
    {scope functionResult value expectedError ty}
    (ground : GroundNode solution runtimeSolution storage scope functionResult (.propagate value expectedError) ty) :
    ∃ success, functionResult = .result success (substitute solution expectedError) := by
  cases ground with
  | propagate value matching result closed =>
    obtain ⟨success, result⟩ := result
    exact ⟨success, by simpa [matching] using result⟩

theorem boolean_result_rejects_propagation {solution runtimeSolution : Substitution} {storage : BindingStorageMap}
    {scope value expectedError ty} :
    ¬ GroundNode solution runtimeSolution storage scope (.primitive .boolean) (.propagate value expectedError) ty := by
  intro ground
  obtain ⟨success, impossible⟩ := ground_propagation_requires_result ground
  cases impossible

theorem ground_propagation_opens_frame {state : Machine} {context : TypingContext} {value expectedError ty}
    (ground : GroundNode context.solution context.runtimeSolution context.storage context.scope context.result
      (.propagate value expectedError) ty) :
    ∃ failure, GroundNode context.solution context.runtimeSolution context.storage context.scope context.result
      value (.result ty failure) ∧
      FrameTyped state (.expression context (.result ty failure)) (.expression context ty) .propagate := by
  cases ground with
  | propagate inner matching result closed =>
    obtain ⟨success, result⟩ := result
    exact ⟨_, inner, .propagate result⟩

theorem ground_node_is_closed {solution runtimeSolution : Substitution} {storage : BindingStorageMap}
    {scope functionResult node ty} (ground : GroundNode solution runtimeSolution storage scope functionResult node ty) :
    ∃ runtimeType, monoRuntime ty = some runtimeType := by
  cases ground <;> first | exact ⟨_, rfl⟩ | solve | exact ⟨_, by assumption⟩

theorem ground_propagation_opens_closed_frame {state : Machine} {context : TypingContext} {value expectedError ty}
    (ground : GroundNode context.solution context.runtimeSolution context.storage context.scope context.result
      (.propagate value expectedError) ty) :
    ∃ failure, GroundNode context.solution context.runtimeSolution context.storage context.scope context.result
      value (.result ty failure) ∧
      DemandClosed (.expression context (.result ty failure)) ∧
      FrameTyped state (.expression context (.result ty failure)) (.expression context ty) .propagate := by
  obtain ⟨failure, inner, frame⟩ := ground_propagation_opens_frame (state := state) ground
  exact ⟨failure, inner, ground_node_is_closed inner, frame⟩

end Xsh.Semantics.Connected
