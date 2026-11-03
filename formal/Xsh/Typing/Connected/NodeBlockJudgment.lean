import Xsh.Typing.Connected.NodeSound
import Xsh.Typing.Connected.BlockValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation

mutual
  inductive NodeBlockHasType (solution : Substitution) : Scope → MonoType → List StatementNode → Prop where
    | nil {scope result} : NodeBlockHasType solution scope result []
    | binding {scope result name binder kind rawType initializer rest} :
        NodeHasType scope solution initializer (substitute solution rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        NodeBlockHasType solution (⟨name, binder, kind, ⟨[], rawType, []⟩⟩ :: scope) result rest →
        NodeBlockHasType solution scope result (.binding binder kind initializer :: rest)
    | assignment {scope result name binding value rest} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .mutable →
        NodeHasType scope solution value (substitute solution binding.scheme.body) →
        NodeBlockHasType solution scope result rest →
        NodeBlockHasType solution scope result (.assignment binding.binder value :: rest)
    | returned {scope result value rest} :
        NodeHasType scope solution value result → NodeBlockHasType solution scope result rest →
        NodeBlockHasType solution scope result (.returned value :: rest)
    | evaluated {scope result value rest ty} :
        NodeHasType scope solution value ty → (rest = [] → ty = result) →
        NodeBlockHasType solution scope result rest →
        NodeBlockHasType solution scope result (.evaluated value :: rest)
    | conditional {scope result branches fallback rest} :
        NodeStatementBranchesHaveType solution scope result branches →
        NodeBlockHasType solution scope result fallback → NodeBlockHasType solution scope result rest →
        NodeBlockHasType solution scope result (.conditional branches fallback :: rest)

  inductive NodeStatementBranchesHaveType (solution : Substitution) :
      Scope → MonoType → List (Node × List StatementNode) → Prop where
    | nil {scope result} : NodeStatementBranchesHaveType solution scope result []
    | cons {scope result condition body rest} :
        NodeHasType scope solution condition (.primitive .boolean) →
        NodeBlockHasType solution scope result body →
        NodeStatementBranchesHaveType solution scope result rest →
        NodeStatementBranchesHaveType solution scope result ((condition, body) :: rest)
end

end Xsh.Typing.Connected
