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

end Xsh.Semantics.Connected
