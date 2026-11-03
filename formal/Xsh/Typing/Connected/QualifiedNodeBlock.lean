import Xsh.Typing.Connected.QualifiedNodeSound
import Xsh.Typing.Connected.NodeBlockJudgment

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

mutual
  inductive QualifiedNodeBlock (principal : Substitution) (requirements : List Constraint) :
      Scope → MonoType → List StatementNode → Prop where
    | nil {scope result} : QualifiedNodeBlock principal requirements scope result []
    | binding {scope result name binder kind rawType initializer rest} :
        QualifiedNode scope principal requirements initializer (substitute principal rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        QualifiedNodeBlock principal requirements (⟨name, binder, kind, ⟨[], rawType, []⟩⟩ :: scope) result rest →
        QualifiedNodeBlock principal requirements scope result (.binding binder kind initializer :: rest)
    | assignment {scope result name binding value rest} :
        scope.find? (fun item => decide (item.name = name)) = some binding → binding.kind = .mutable →
        QualifiedNode scope principal requirements value (substitute principal binding.scheme.body) →
        QualifiedNodeBlock principal requirements scope result rest →
        QualifiedNodeBlock principal requirements scope result (.assignment binding.binder value :: rest)
    | returned {scope result value rest} :
        QualifiedNode scope principal requirements value result →
        QualifiedNodeBlock principal requirements scope result rest →
        QualifiedNodeBlock principal requirements scope result (.returned value :: rest)
    | evaluated {scope result value rest ty} :
        QualifiedNode scope principal requirements value ty → (rest = [] → ty = result) →
        QualifiedNodeBlock principal requirements scope result rest →
        QualifiedNodeBlock principal requirements scope result (.evaluated value :: rest)
    | conditional {scope result branches fallback rest} :
        QualifiedNodeStatementBranches principal requirements scope result branches →
        QualifiedNodeBlock principal requirements scope result fallback →
        QualifiedNodeBlock principal requirements scope result rest →
        QualifiedNodeBlock principal requirements scope result (.conditional branches fallback :: rest)

  inductive QualifiedNodeStatementBranches (principal : Substitution) (requirements : List Constraint) :
      Scope → MonoType → List (Node × List StatementNode) → Prop where
    | nil {scope result} : QualifiedNodeStatementBranches principal requirements scope result []
    | cons {scope result condition body rest} :
        QualifiedNode scope principal requirements condition (.primitive .boolean) →
        QualifiedNodeBlock principal requirements scope result body →
        QualifiedNodeStatementBranches principal requirements scope result rest →
        QualifiedNodeStatementBranches principal requirements scope result ((condition, body) :: rest)
end

theorem qualified_node_block_instantiates (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (nodes : List StatementNode)
    (typed : QualifiedNodeBlock principal requirements scope result nodes)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeBlockHasType (compose later principal) scope (substitute later result) nodes := by
  refine @QualifiedNodeBlock.rec principal requirements
    (fun scope result nodes _ => NodeBlockHasType (compose later principal) scope (substitute later result) nodes)
    (fun scope result nodes _ => NodeStatementBranchesHaveType (compose later principal) scope (substitute later result) nodes)
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ scope result nodes typed
  · intro scope result
    exact .nil
  · intro scope result name binder kind rawType initializer rest value fresh restTyped restIH
    have valueTyped := qualified_node_instantiates scope principal later requirements _ _ value sorted solves
    rw [← substitute_compose] at valueTyped
    exact .binding valueTyped fresh restIH
  · intro scope result name binding value rest found mutable valueTyped restTyped restIH
    have valueType := qualified_node_instantiates scope principal later requirements _ _ valueTyped sorted solves
    rw [← substitute_compose] at valueType
    exact .assignment found mutable valueType restIH
  · intro scope result value rest valueTyped restTyped restIH
    exact .returned (qualified_node_instantiates scope principal later requirements _ _ valueTyped sorted solves) restIH
  · intro scope result value rest ty valueTyped tail restTyped restIH
    exact .evaluated (qualified_node_instantiates scope principal later requirements _ _ valueTyped sorted solves)
      (fun empty => by rw [tail empty]) restIH
  · intro scope result branches fallback rest branchesTyped fallbackTyped restTyped branchesIH fallbackIH restIH
    exact .conditional branchesIH fallbackIH restIH
  · intro scope result
    exact .nil
  · intro scope result condition body rest conditionTyped bodyTyped restTyped bodyIH restIH
    exact .cons (qualified_node_instantiates scope principal later requirements _ _ conditionTyped sorted solves) bodyIH restIH

end Xsh.Typing.Connected
