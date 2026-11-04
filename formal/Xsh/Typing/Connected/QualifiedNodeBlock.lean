import Xsh.Typing.Connected.QualifiedNodeSound
import Xsh.Typing.Connected.NodeBlockJudgment

namespace Xsh.Typing.Connected

open Xsh.Foundation

mutual
  inductive QualifiedNodeBlockHasCompletion (principal : Substitution) (requirements : List Constraint) :
      Scope → MonoType → Option MonoType → List StatementNode → Prop where
    | nil {scope result completion} :
        CompletionAccepts completion (.primitive .unit) →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion []
    | binding {scope result completion name binder kind rawType initializer rest} :
        QualifiedNode scope principal requirements initializer (substitute principal rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        QualifiedNodeBlockHasCompletion principal requirements (⟨name, binder, kind, ⟨[], rawType, []⟩⟩ :: scope) result completion rest →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (.binding binder kind initializer :: rest)
    | assignment {scope result completion name binding value rest} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .mutable →
        QualifiedNode scope principal requirements value (substitute principal binding.scheme.body) →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion rest →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (.assignment binding.binder value :: rest)
    | returned {scope result completion value rest} :
        QualifiedNode scope principal requirements value result → QualifiedNodeBlockHasCompletion principal requirements scope result none rest →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (.returned value :: rest)
    | evaluatedTail {scope result completion value ty} :
        QualifiedNode scope principal requirements value ty → CompletionAccepts completion ty →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion [.evaluated value]
    | evaluatedNext {scope result completion value next rest ty} :
        QualifiedNode scope principal requirements value ty →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (next :: rest) →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (.evaluated value :: next :: rest)
    | scopedTail {scope result completion body} :
        QualifiedNodeBlockHasCompletion principal requirements scope result completion body →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion [.scoped body]
    | scopedNext {scope result completion body next rest} :
        QualifiedNodeBlockHasCompletion principal requirements scope result none body →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (next :: rest) →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (.scoped body :: next :: rest)
    | conditionalTail {scope result completion branches fallback} :
        QualifiedNodeBranchesHaveCompletion principal requirements scope result completion branches →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion fallback →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion [.conditional branches fallback]
    | conditionalNext {scope result completion branches fallback next rest} :
        QualifiedNodeBranchesHaveCompletion principal requirements scope result none branches →
        QualifiedNodeBlockHasCompletion principal requirements scope result none fallback →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (next :: rest) →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion (.conditional branches fallback :: next :: rest)

  inductive QualifiedNodeBranchesHaveCompletion (principal : Substitution) (requirements : List Constraint) :
      Scope → MonoType → Option MonoType → List (Node × List StatementNode) → Prop where
    | nil {scope result completion} : QualifiedNodeBranchesHaveCompletion principal requirements scope result completion []
    | cons {scope result completion condition body rest} :
        QualifiedNode scope principal requirements condition (.primitive .boolean) →
        QualifiedNodeBlockHasCompletion principal requirements scope result completion body →
        QualifiedNodeBranchesHaveCompletion principal requirements scope result completion rest →
        QualifiedNodeBranchesHaveCompletion principal requirements scope result completion ((condition, body) :: rest)
end

abbrev QualifiedNodeBlock (principal : Substitution) (requirements : List Constraint) (scope : Scope) (result : MonoType)
    (nodes : List StatementNode) : Prop :=
  QualifiedNodeBlockHasCompletion principal requirements scope result (some result) nodes

abbrev QualifiedNodeStatementBranches (principal : Substitution) (requirements : List Constraint) (scope : Scope) (result : MonoType)
    (branches : List (Node × List StatementNode)) : Prop :=
  QualifiedNodeBranchesHaveCompletion principal requirements scope result (some result) branches

theorem qualified_node_completion_instantiates (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (completion : Option MonoType)
    (nodes : List StatementNode)
    (typed : QualifiedNodeBlockHasCompletion principal requirements scope result completion nodes)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeBlockHasCompletion (compose later principal) scope (substitute later result)
      (completion.map (substitute later)) nodes := by
  refine @QualifiedNodeBlockHasCompletion.rec principal requirements
    (fun scope result completion nodes _ => NodeBlockHasCompletion (compose later principal) scope
      (substitute later result) (completion.map (substitute later)) nodes)
    (fun scope result completion nodes _ => NodeBranchesHaveCompletion (compose later principal) scope
      (substitute later result) (completion.map (substitute later)) nodes)
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ scope result completion nodes typed
  · intro scope result completion accepted
    exact .nil (completion_instantiates later completion (.primitive .unit) accepted)
  · intro scope result completion name binder kind rawType initializer rest value fresh restTyped restIH
    have valueTyped := qualified_node_instantiates scope principal later requirements _ _ value sorted solves
    rw [← substitute_compose] at valueTyped
    exact .binding valueTyped fresh restIH
  · intro scope result completion name binding value rest found mutable valueTyped restTyped restIH
    have valueType := qualified_node_instantiates scope principal later requirements _ _ valueTyped sorted solves
    rw [← substitute_compose] at valueType
    exact .assignment found mutable valueType restIH
  · intro scope result completion value rest valueTyped restTyped restIH
    exact .returned (qualified_node_instantiates scope principal later requirements _ _ valueTyped sorted solves) restIH
  · intro scope result completion value ty valueTyped accepted
    exact .evaluatedTail (qualified_node_instantiates scope principal later requirements _ _ valueTyped sorted solves)
      (completion_instantiates later completion ty accepted)
  · intro scope result completion value next rest ty valueTyped restTyped restIH
    exact .evaluatedNext (qualified_node_instantiates scope principal later requirements _ _ valueTyped sorted solves) restIH
  · intro scope result completion body typed bodyIH
    exact .scopedTail bodyIH
  · intro scope result completion body next rest bodyTyped restTyped bodyIH restIH
    exact .scopedNext bodyIH restIH
  · intro scope result completion branches fallback branchesTyped fallbackTyped branchesIH fallbackIH
    exact .conditionalTail branchesIH fallbackIH
  · intro scope result completion branches fallback next rest branchesTyped fallbackTyped restTyped branchesIH fallbackIH restIH
    exact .conditionalNext branchesIH fallbackIH restIH
  · intro scope result completion
    exact .nil
  · intro scope result completion condition body rest conditionTyped bodyTyped restTyped bodyIH restIH
    exact .cons (qualified_node_instantiates scope principal later requirements _ _ conditionTyped sorted solves) bodyIH restIH

theorem qualified_node_block_instantiates (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (nodes : List StatementNode)
    (typed : QualifiedNodeBlock principal requirements scope result nodes)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeBlockHasType (compose later principal) scope (substitute later result) nodes := by
  exact qualified_node_completion_instantiates principal later requirements scope result (some result) nodes typed sorted solves

end Xsh.Typing.Connected
