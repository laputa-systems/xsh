import Xsh.Typing.Connected.NodeSound
import Xsh.Typing.Connected.BlockValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation

mutual
  inductive NodeBlockHasCompletion (solution : Substitution) :
      Scope → MonoType → Option MonoType → List StatementNode → Prop where
    | nil {scope result completion} :
        CompletionAccepts completion (.primitive .unit) →
        NodeBlockHasCompletion solution scope result completion []
    | binding {scope result completion name binder kind rawType initializer rest} :
        NodeHasType scope solution initializer (substitute solution rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        NodeBlockHasCompletion solution (⟨name, binder, kind, ⟨[], rawType, []⟩⟩ :: scope) result completion rest →
        NodeBlockHasCompletion solution scope result completion (.binding binder kind initializer :: rest)
    | assignment {scope result completion name binding value rest} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .mutable →
        NodeHasType scope solution value (substitute solution binding.scheme.body) →
        NodeBlockHasCompletion solution scope result completion rest →
        NodeBlockHasCompletion solution scope result completion (.assignment binding.binder value :: rest)
    | returned {scope result completion value rest} :
        NodeHasType scope solution value result → NodeBlockHasCompletion solution scope result none rest →
        NodeBlockHasCompletion solution scope result completion (.returned value :: rest)
    | evaluatedTail {scope result completion value ty} :
        NodeHasType scope solution value ty → CompletionAccepts completion ty →
        NodeBlockHasCompletion solution scope result completion [.evaluated value]
    | evaluatedNext {scope result completion value next rest ty} :
        NodeHasType scope solution value ty →
        NodeBlockHasCompletion solution scope result completion (next :: rest) →
        NodeBlockHasCompletion solution scope result completion (.evaluated value :: next :: rest)
    | scopedTail {scope result completion body} :
        NodeBlockHasCompletion solution scope result completion body →
        NodeBlockHasCompletion solution scope result completion [.scoped body]
    | scopedNext {scope result completion body next rest} :
        NodeBlockHasCompletion solution scope result none body →
        NodeBlockHasCompletion solution scope result completion (next :: rest) →
        NodeBlockHasCompletion solution scope result completion (.scoped body :: next :: rest)
    | conditionalTail {scope result completion branches fallback} :
        NodeBranchesHaveCompletion solution scope result completion branches →
        NodeBlockHasCompletion solution scope result completion fallback →
        NodeBlockHasCompletion solution scope result completion [.conditional branches fallback]
    | conditionalNext {scope result completion branches fallback next rest} :
        NodeBranchesHaveCompletion solution scope result none branches →
        NodeBlockHasCompletion solution scope result none fallback →
        NodeBlockHasCompletion solution scope result completion (next :: rest) →
        NodeBlockHasCompletion solution scope result completion (.conditional branches fallback :: next :: rest)

  inductive NodeBranchesHaveCompletion (solution : Substitution) :
      Scope → MonoType → Option MonoType → List (Node × List StatementNode) → Prop where
    | nil {scope result completion} : NodeBranchesHaveCompletion solution scope result completion []
    | cons {scope result completion condition body rest} :
        NodeHasType scope solution condition (.primitive .boolean) →
        NodeBlockHasCompletion solution scope result completion body →
        NodeBranchesHaveCompletion solution scope result completion rest →
        NodeBranchesHaveCompletion solution scope result completion ((condition, body) :: rest)
end

abbrev NodeBlockHasType (solution : Substitution) (scope : Scope) (result : MonoType)
    (nodes : List StatementNode) : Prop :=
  NodeBlockHasCompletion solution scope result (some result) nodes

abbrev NodeStatementBranchesHaveType (solution : Substitution) (scope : Scope) (result : MonoType)
    (branches : List (Node × List StatementNode)) : Prop :=
  NodeBranchesHaveCompletion solution scope result (some result) branches

end Xsh.Typing.Connected
