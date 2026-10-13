import Xsh.Effects.Solver

/-!
The recursive summary solver observes each node's edge list through Boolean
union. Reordering a node's edges with its other metadata fixed therefore
preserves the equation, iteration, and final summary. A concrete mutation
shows why consuming only the first edge would violate that invariant.
These results concern the finite solver model, not Rust graph construction.
-/

namespace Xsh.Semantics.Effects.Solver

open Xsh.Syntax

private theorem any_permutation_invariant {α : Type} {left right : List α}
    (same : left.Perm right) (predicate : α → Bool) :
    left.any predicate = right.any predicate := by
  cases leftResult : left.any predicate <;>
    cases rightResult : right.any predicate
  · rfl
  · have found := List.any_eq_true.mp rightResult
    rcases found with ⟨item, member, contributes⟩
    have memberLeft : item ∈ left := (same.mem_iff).mpr member
    have foundLeft : left.any predicate = true :=
      List.any_eq_true.mpr ⟨item, memberLeft, contributes⟩
    rw [leftResult] at foundLeft
    cases foundLeft
  · have found := List.any_eq_true.mp leftResult
    rcases found with ⟨item, member, contributes⟩
    have memberRight : item ∈ right := (same.mem_iff).mp member
    have foundRight : right.any predicate = true :=
      List.any_eq_true.mpr ⟨item, memberRight, contributes⟩
    rw [rightResult] at foundRight
    cases foundRight
  · rfl

/-- Replacing only a node's edge list leaves its other graph metadata intact. -/
def reorderGraph (graph : Graph declarations)
    (edges : Fin declarations → List (Edge declarations)) : Graph declarations :=
  { node := fun source => { graph.node source with edges := edges source } }

theorem reorderGraph_preserves_direct (graph : Graph declarations)
    (edges : Fin declarations → List (Edge declarations)) (source : Fin declarations) :
    ((reorderGraph graph edges).node source).direct = (graph.node source).direct := rfl

theorem reorderGraph_preserves_declared_contract (graph : Graph declarations)
    (edges : Fin declarations → List (Edge declarations)) (source : Fin declarations) :
    ((reorderGraph graph edges).node source).declared = (graph.node source).declared := rfl

/-- Boolean union over edges is invariant under permutation, including duplicates. -/
theorem edgesContribution_permutation_invariant (graph : Graph declarations)
    (state : SolverState declarations) {left right : List (Edge declarations)}
    (same : left.Perm right) :
    edgesContribution graph state left = edgesContribution graph state right := by
  funext fact
  exact any_permutation_invariant same (fun edge => edgeContribution graph state edge fact)

theorem nodeEquation_edgePermutation (graph : Graph declarations)
    (state : SolverState declarations)
    (edges : Fin declarations → List (Edge declarations))
    (same : ∀ source, (graph.node source).edges.Perm (edges source))
    (source : Fin declarations) :
    nodeEquation graph state source =
      nodeEquation (reorderGraph graph edges) state source := by
  funext fact
  change ((graph.node source).direct fact ||
      edgesContribution graph state (graph.node source).edges fact) =
    ((graph.node source).direct fact ||
      edgesContribution graph state (edges source) fact)
  exact congrArg (fun contribution =>
      (graph.node source).direct fact || contribution)
    (congrFun
      (edgesContribution_permutation_invariant graph state (same source)) fact)

theorem step_edgePermutation (graph : Graph declarations)
    (state : SolverState declarations)
    (edges : Fin declarations → List (Edge declarations))
    (same : ∀ source, (graph.node source).edges.Perm (edges source)) :
    step graph state = step (reorderGraph graph edges) state := by
  funext source
  funext fact
  simp only [step]
  rw [nodeEquation_edgePermutation graph state edges same source]

theorem solve_edgePermutation (graph : Graph declarations)
    (edges : Fin declarations → List (Edge declarations))
    (same : ∀ source, (graph.node source).edges.Perm (edges source)) :
    solve graph = solve (reorderGraph graph edges) := by
  have sameStep : step graph = step (reorderGraph graph edges) := by
    funext state
    exact step_edgePermutation graph state edges same
  unfold solve
  rw [sameStep]

def edgeOrderNode0 : Fin 3 := ⟨0, by decide⟩
def edgeOrderNode1 : Fin 3 := ⟨1, by decide⟩
def edgeOrderNode2 : Fin 3 := ⟨2, by decide⟩

private def edgeOrderSummaryEdge (target : Fin 3) : Edge 3 :=
  { target := target, capturesError := false, readsContract := false }

/-- Two source edges contribute different facts, so the original solver sees both. -/
def edgeOrderWitnessGraph : Graph 3 where
  node source :=
    if source = edgeOrderNode0 then
      { direct := emptyFacts, declared := none,
        edges := [edgeOrderSummaryEdge edgeOrderNode1,
          edgeOrderSummaryEdge edgeOrderNode2] }
    else if source = edgeOrderNode1 then
      { direct := effectFact .net, declared := none, edges := [] }
    else
      { direct := emptyFacts, declared := none, edges := [] }

private def edgeOrderWitnessEdges : Fin 3 → List (Edge 3) := fun source =>
  if source = edgeOrderNode0 then
    [edgeOrderSummaryEdge edgeOrderNode2, edgeOrderSummaryEdge edgeOrderNode1]
  else
    (edgeOrderWitnessGraph.node source).edges

def edgeOrderWitnessGraphReversed : Graph 3 :=
  reorderGraph edgeOrderWitnessGraph edgeOrderWitnessEdges

/-- Both edge orders retain the network fact under the solver's union equation. -/
theorem reordered_edge_witness_preserves_solver_answer :
    solve edgeOrderWitnessGraph edgeOrderNode0 (some Effect.net) = true ∧
      solve edgeOrderWitnessGraphReversed edgeOrderNode0 (some Effect.net) = true := by
  decide

/-- A faulty graph mutation keeps only each node's first edge. -/
def firstEdgeOnlyGraph (graph : Graph declarations) : Graph declarations :=
  { node := fun source =>
      { graph.node source with edges := (graph.node source).edges.take 1 } }

/-- The first-edge-only mutation changes the answer when the same edges are reordered. -/
theorem first_edge_only_mutation_changes_answer :
    solve (firstEdgeOnlyGraph edgeOrderWitnessGraph) edgeOrderNode0
        (some Effect.net) = true ∧
      solve (firstEdgeOnlyGraph edgeOrderWitnessGraphReversed) edgeOrderNode0
        (some Effect.net) = false := by
  decide

end Xsh.Semantics.Effects.Solver
