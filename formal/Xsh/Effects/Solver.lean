import Xsh.Effects.Lattice

/-!
A finite equation model for recursive effect summaries. Each declaration has
seven named effect facts and one unresolved fact. The solver keeps declared
contract edges separate from inferred-summary edges, and a capturing edge
filters only `error`. These theorems describe this equation model; they do not
establish that the Rust checker builds or solves the same graph. Diagnostic
provenance, source-site classification, and latent-effect behavior remain
outside this model.
-/

namespace Xsh.Semantics.Effects.Solver

open Xsh.Syntax

/-- A summary records the presence of each named effect and unresolved work. -/
abbrev FactSet := Option Effect → Bool

abbrev SolverState (declarations : Nat) := Fin declarations → FactSet

def emptyFacts : FactSet := fun _ => false

def effectFact (effect : Effect) : FactSet := fun fact =>
  if fact = some effect then true else false

/-- Convert the existing summary lattice value into the finite fact model. -/
noncomputable def summaryFacts (summary : EffectSummary) : FactSet := by
  classical
  exact fun fact =>
    match fact with
    | none => summary.unresolved
    | some effect => decide (summary.known effect)

/-- Convert solver facts back to the summary shape used by the effect lattice. -/
def toEffectSummary (facts : FactSet) : EffectSummary :=
  { known := fun effect => facts (some effect) = true
    unresolved := facts none }

/-- A missing declaration is unknown; an explicit empty clause has no facts. -/
def declaredFacts (effects : Option (Effect → Bool)) : FactSet :=
  match effects with
  | none => fun fact =>
      match fact with
      | none => true
      | some _ => false
  | some known => fun fact =>
      match fact with
      | none => false
      | some effect => known effect

structure Edge (declarations : Nat) where
  target : Fin declarations
  capturesError : Bool
  readsContract : Bool
  deriving DecidableEq

structure Node (declarations : Nat) where
  direct : FactSet
  declared : Option (Effect → Bool)
  edges : List (Edge declarations)

structure Graph (declarations : Nat) where
  node : Fin declarations → Node declarations

def factLe (left right : FactSet) : Prop :=
  ∀ fact, left fact = true → right fact = true

def stateLe {declarations : Nat}
    (left right : SolverState declarations) : Prop :=
  ∀ node fact, left node fact = true → right node fact = true

theorem factLe_refl (facts : FactSet) : factLe facts facts := by
  intro fact member
  exact member

theorem stateLe_refl {declarations : Nat} (state : SolverState declarations) :
    stateLe state state := by
  intro node fact member
  exact member

theorem stateLe_trans {declarations : Nat} {first middle last : SolverState declarations}
    (firstBelow : stateLe first middle) (middleBelow : stateLe middle last) :
    stateLe first last := by
  intro node fact member
  exact middleBelow node fact (firstBelow node fact member)

def readEdge (graph : Graph declarations) (state : SolverState declarations)
    (edge : Edge declarations) : FactSet :=
  if edge.readsContract then
    declaredFacts (graph.node edge.target).declared
  else
    state edge.target

def visibleEdgeFacts (edge : Edge declarations) (facts : FactSet) : FactSet :=
  if edge.capturesError then
    fun fact => if fact = some Effect.error then false else facts fact
  else
    facts

def edgeContribution (graph : Graph declarations) (state : SolverState declarations)
    (edge : Edge declarations) : FactSet :=
  visibleEdgeFacts edge (readEdge graph state edge)

/-- `any` makes union independent of edge-list order and duplicate entries. -/
def edgesContribution (graph : Graph declarations) (state : SolverState declarations)
    (edges : List (Edge declarations)) : FactSet :=
  fun fact => edges.any (fun edge => edgeContribution graph state edge fact)

def nodeEquation (graph : Graph declarations) (state : SolverState declarations)
    (source : Fin declarations) : FactSet :=
  fun fact => (graph.node source).direct fact ||
    edgesContribution graph state (graph.node source).edges fact

/-- A closure step retains previous facts and adds the current node equations. -/
def step (graph : Graph declarations) (state : SolverState declarations) :
    SolverState declarations :=
  fun source fact => state source fact || nodeEquation graph state source fact

private theorem bool_or_mono (leftA leftB rightA rightB : Bool)
    (leftBelow : leftA = true → rightA = true)
    (rightBelow : leftB = true → rightB = true)
    (member : (leftA || leftB) = true) : (rightA || rightB) = true := by
  apply Bool.or_eq_true_iff.mpr
  rcases Bool.or_eq_true_iff.mp member with left | right
  · exact Or.inl (leftBelow left)
  · exact Or.inr (rightBelow right)

theorem readEdge_mono (graph : Graph declarations) (edge : Edge declarations)
    {left right : SolverState declarations} (below : stateLe left right) :
    factLe (readEdge graph left edge) (readEdge graph right edge) := by
  intro fact member
  cases contract : edge.readsContract with
  | false =>
      have member' : left edge.target fact = true := by
        simpa [readEdge, contract] using member
      simpa [readEdge, contract] using below edge.target fact member'
  | true =>
      simpa [readEdge, contract] using member

theorem visibleEdgeFacts_mono (edge : Edge declarations)
    {left right : FactSet} (below : factLe left right) :
    factLe (visibleEdgeFacts edge left) (visibleEdgeFacts edge right) := by
  intro fact member
  cases captures : edge.capturesError with
  | false =>
      have member' : left fact = true := by
        simpa [visibleEdgeFacts, captures] using member
      simpa [visibleEdgeFacts, captures] using below fact member'
  | true =>
      by_cases isError : fact = some Effect.error
      · simp [visibleEdgeFacts, captures, isError] at member ⊢
      · have member' : left fact = true := by
          simpa [visibleEdgeFacts, captures, isError] using member
        simpa [visibleEdgeFacts, captures, isError] using below fact member'

theorem edgeContribution_mono (graph : Graph declarations) (edge : Edge declarations)
    {left right : SolverState declarations} (below : stateLe left right) :
    factLe (edgeContribution graph left edge) (edgeContribution graph right edge) := by
  exact visibleEdgeFacts_mono edge (readEdge_mono graph edge below)

theorem edgesContribution_mono (graph : Graph declarations)
    (edges : List (Edge declarations)) {left right : SolverState declarations}
    (below : stateLe left right) :
    factLe (edgesContribution graph left edges) (edgesContribution graph right edges) := by
  intro fact member
  change edges.any (fun edge => edgeContribution graph left edge fact) = true at member
  have found := List.any_eq_true.mp member
  rcases found with ⟨edge, inEdges, contributed⟩
  change edges.any (fun edge => edgeContribution graph right edge fact) = true
  apply List.any_eq_true.mpr
  exact ⟨edge, inEdges, edgeContribution_mono graph edge below fact contributed⟩

theorem nodeEquation_mono (graph : Graph declarations) (source : Fin declarations)
    {left right : SolverState declarations} (below : stateLe left right) :
    factLe (nodeEquation graph left source) (nodeEquation graph right source) := by
  intro fact member
  change ((graph.node source).direct fact ||
    edgesContribution graph left (graph.node source).edges fact) = true at member
  change ((graph.node source).direct fact ||
    edgesContribution graph right (graph.node source).edges fact) = true
  exact bool_or_mono
    ((graph.node source).direct fact)
    (edgesContribution graph left (graph.node source).edges fact)
    ((graph.node source).direct fact)
    (edgesContribution graph right (graph.node source).edges fact)
    (fun direct => direct)
    (edgesContribution_mono graph (graph.node source).edges below fact)
    member

theorem step_mono (graph : Graph declarations)
    {left right : SolverState declarations} (below : stateLe left right) :
    stateLe (step graph left) (step graph right) := by
  intro node fact member
  change (left node fact || nodeEquation graph left node fact) = true at member
  change (right node fact || nodeEquation graph right node fact) = true
  exact bool_or_mono
    (left node fact)
    (nodeEquation graph left node fact)
    (right node fact)
    (nodeEquation graph right node fact)
    (below node fact)
    (nodeEquation_mono graph node below fact)
    member

theorem step_inflationary (graph : Graph declarations) (state : SolverState declarations) :
    stateLe state (step graph state) := by
  intro node fact member
  exact Bool.or_eq_true_iff.mpr (Or.inl member)

theorem edgesContribution_order_invariant (graph : Graph declarations)
    (state : SolverState declarations) {first second : List (Edge declarations)}
    (sameEdges : first.Perm second) :
    edgesContribution graph state first = edgesContribution graph state second := by
  funext fact
  exact sameEdges.any_eq

namespace FiniteIteration

def iterate {State : Type} (operation : State → State) : Nat → State → State
  | 0, state => state
  | fuel + 1, state => operation (iterate operation fuel state)

theorem iterate_succ {State : Type} (operation : State → State) (fuel : Nat)
    (state : State) :
    iterate operation (fuel + 1) state = operation (iterate operation fuel state) := rfl

theorem iterate_stable_after_fixed {State : Type} (operation : State → State)
    (state : State) (fixedIndex extra : Nat)
    (fixed : iterate operation (fixedIndex + 1) state =
      iterate operation fixedIndex state) :
    iterate operation (fixedIndex + extra) state = iterate operation fixedIndex state := by
  induction extra with
  | zero => simp
  | succ extra ih =>
      calc
        iterate operation (fixedIndex + (extra + 1)) state =
            operation (iterate operation (fixedIndex + extra) state) := by
              rw [Nat.add_succ, iterate_succ]
        _ = operation (iterate operation fixedIndex state) := by rw [ih]
        _ = iterate operation (fixedIndex + 1) state := by rw [iterate_succ]
        _ = iterate operation fixedIndex state := fixed

end FiniteIteration

def factsUniverse : List (Option Effect) := none :: allEffects.map some

theorem factsUniverse_complete (fact : Option Effect) : fact ∈ factsUniverse := by
  cases fact with
  | none => simp [factsUniverse]
  | some effect => simp [factsUniverse, every_effect_is_listed effect]

def stateSlots (declarations : Nat) : List (Fin declarations × Option Effect) :=
  (List.finRange declarations).flatMap fun node =>
    factsUniverse.map fun fact => (node, fact)

theorem stateSlots_complete (node : Fin declarations) (fact : Option Effect) :
    (node, fact) ∈ stateSlots declarations := by
  simp only [stateSlots, List.mem_flatMap]
  refine ⟨node, List.mem_finRange node, ?_⟩
  simp only [List.mem_map]
  exact ⟨fact, factsUniverse_complete fact, rfl⟩

def stateRank {declarations : Nat} (state : SolverState declarations) : Nat :=
  (stateSlots declarations).countP fun slot => state slot.1 slot.2 = true

def iterationLimit (declarations : Nat) : Nat :=
  (stateSlots declarations).length

theorem stateRank_bounded {declarations : Nat} (state : SolverState declarations) :
    stateRank state ≤ iterationLimit declarations := by
  exact List.countP_le_length

private theorem countP_mono_on {α : Type} {items : List α} {left right : α → Bool}
    (pointwise : ∀ item, item ∈ items → left item = true → right item = true) :
    items.countP left ≤ items.countP right := by
  induction items with
  | nil => simp
  | cons head tail ih =>
      have tailBelow : tail.countP left ≤ tail.countP right := by
        apply ih
        intro item inTail member
        exact pointwise item (by simp [inTail]) member
      have headBelow : (if left head then 1 else 0) ≤
          (if right head then 1 else 0) := by
        have headPointwise := pointwise head (by simp)
        cases hleft : left head <;> cases hright : right head <;>
          simp_all [headPointwise]
      simpa only [List.countP_cons] using Nat.add_le_add tailBelow headBelow

private theorem countP_strict_of_member {α : Type} {items : List α}
    {left right : α → Bool} {witness : α}
    (pointwise : ∀ item, item ∈ items → left item = true → right item = true)
    (present : witness ∈ items) (leftFalse : left witness = false)
    (rightTrue : right witness = true) :
    items.countP left < items.countP right := by
  induction items with
  | nil => simp at present
  | cons head tail ih =>
      rcases List.mem_cons.mp present with atHead | inTail
      · subst witness
        have tailBelow : tail.countP left ≤ tail.countP right := by
          apply countP_mono_on
          intro item itemInTail member
          exact pointwise item (by simp [itemInTail]) member
        simpa [List.countP_cons, leftFalse, rightTrue] using
          Nat.lt_succ_of_le tailBelow
      · have tailPointwise : ∀ item, item ∈ tail → left item = true → right item = true := by
          intro item itemInTail member
          exact pointwise item (by simp [itemInTail]) member
        have tailStrict := ih tailPointwise inTail
        have headPointwise := pointwise head (by simp)
        have headBelow : (if left head then 1 else 0) ≤
            (if right head then 1 else 0) := by
          cases hleft : left head <;> cases hright : right head <;>
            simp_all [headPointwise]
        simp only [List.countP_cons]
        omega

theorem stateRank_mono {declarations : Nat} {left right : SolverState declarations}
    (below : stateLe left right) : stateRank left ≤ stateRank right := by
  apply countP_mono_on
  intro slot _ member
  have member' : left slot.1 slot.2 = true := by simpa using member
  have required := below slot.1 slot.2 member'
  simpa using required

theorem stateRank_strict_of_ne {declarations : Nat}
    {left right : SolverState declarations} (below : stateLe left right)
    (different : left ≠ right) : stateRank left < stateRank right := by
  have witness : ∃ node fact, left node fact = false ∧ right node fact = true := by
    classical
    by_cases hasWitness : ∃ node fact, left node fact = false ∧ right node fact = true
    · exact hasWitness
    · have noDifference : ∀ node fact,
          ¬ (left node fact = false ∧ right node fact = true) := by
        intro node fact pair
        exact hasWitness ⟨node, fact, pair.1, pair.2⟩
      have equal : left = right := by
        funext node
        funext fact
        cases leftValue : left node fact <;> cases rightValue : right node fact
        · rfl
        · exact False.elim (noDifference node fact ⟨leftValue, rightValue⟩)
        · have impossible := below node fact leftValue
          simp [rightValue] at impossible
        · rfl
      exact False.elim (different equal)
  rcases witness with ⟨node, fact, leftFalse, rightTrue⟩
  let witnessSlot : Fin declarations × Option Effect := (node, fact)
  have present : witnessSlot ∈ stateSlots declarations :=
    stateSlots_complete node fact
  change (stateSlots declarations).countP
      (fun slot => decide (left slot.1 slot.2 = true)) <
    (stateSlots declarations).countP
      (fun slot => decide (right slot.1 slot.2 = true))
  apply countP_strict_of_member
  · intro slot _ member
    have member' : left slot.1 slot.2 = true := by simpa using member
    have required := below slot.1 slot.2 member'
    simpa using required
  · exact present
  · simp [witnessSlot, leftFalse]
  · simp [witnessSlot, rightTrue]

def bottomState (declarations : Nat) : SolverState declarations := fun _ _ => false

theorem iterate_adjacent_le (graph : Graph declarations)
    (state : SolverState declarations) (fuel : Nat) :
    stateLe (FiniteIteration.iterate (step graph) fuel state)
      (FiniteIteration.iterate (step graph) (fuel + 1) state) := by
  induction fuel with
  | zero => simpa [FiniteIteration.iterate] using step_inflationary graph state
  | succ fuel ih =>
      simpa [FiniteIteration.iterate] using step_mono graph ih

theorem exists_fixed_by_iterationLimit (graph : Graph declarations) :
    ∃ index, index ≤ iterationLimit declarations ∧
      FiniteIteration.iterate (step graph) (index + 1) (bottomState declarations) =
        FiniteIteration.iterate (step graph) index (bottomState declarations) := by
  classical
  by_cases fixedExists : ∃ index, index ≤ iterationLimit declarations ∧
      FiniteIteration.iterate (step graph) (index + 1) (bottomState declarations) =
        FiniteIteration.iterate (step graph) index (bottomState declarations)
  · exact fixedExists
  ·
      have notFixedAt : ∀ index, index ≤ iterationLimit declarations →
          FiniteIteration.iterate (step graph) (index + 1) (bottomState declarations) ≠
            FiniteIteration.iterate (step graph) index (bottomState declarations) := by
        intro index bound equal
        exact fixedExists ⟨index, bound, equal⟩
      have rankLower : ∀ index,
          index ≤ iterationLimit declarations + 1 →
          index ≤ stateRank (FiniteIteration.iterate (step graph) index
            (bottomState declarations)) := by
        intro index
        induction index with
        | zero =>
            simp [stateRank, bottomState, FiniteIteration.iterate]
        | succ index ih =>
            intro bound
            have previousBound : index ≤ iterationLimit declarations := by omega
            have previousLower := ih (by omega)
            have adjacent := iterate_adjacent_le graph (bottomState declarations) index
            have unequal := notFixedAt index previousBound
            have rises := stateRank_strict_of_ne adjacent unequal.symm
            omega
      have lower := rankLower (iterationLimit declarations + 1) (by omega)
      have upper := stateRank_bounded
        (FiniteIteration.iterate (step graph) (iterationLimit declarations + 1)
          (bottomState declarations))
      have impossible : False := by omega
      exact False.elim impossible

def solve (graph : Graph declarations) : SolverState declarations :=
  FiniteIteration.iterate (step graph) (iterationLimit declarations)
    (bottomState declarations)

theorem solve_closed (graph : Graph declarations) :
    step graph (solve graph) = solve graph := by
  obtain ⟨fixedIndex, fixedBound, fixed⟩ := exists_fixed_by_iterationLimit graph
  let limit := iterationLimit declarations
  have stableAtLimit := FiniteIteration.iterate_stable_after_fixed
    (step graph) (bottomState declarations) fixedIndex (limit - fixedIndex) fixed
  have stableAfterLimit := FiniteIteration.iterate_stable_after_fixed
    (step graph) (bottomState declarations) fixedIndex (limit - fixedIndex + 1) fixed
  have atLimit : fixedIndex + (limit - fixedIndex) = limit := by omega
  have afterLimit : fixedIndex + (limit - fixedIndex + 1) = limit + 1 := by omega
  rw [atLimit] at stableAtLimit
  rw [afterLimit] at stableAfterLimit
  have stableSuccessor :
      FiniteIteration.iterate (step graph) (limit + 1) (bottomState declarations) =
        FiniteIteration.iterate (step graph) limit (bottomState declarations) :=
    stableAfterLimit.trans stableAtLimit.symm
  simpa [solve, limit, FiniteIteration.iterate_succ] using stableSuccessor

theorem iterate_below_closed (graph : Graph declarations)
    (upper : SolverState declarations) (closed : stateLe (step graph upper) upper) :
    ∀ fuel, stateLe (FiniteIteration.iterate (step graph) fuel
      (bottomState declarations)) upper := by
  intro fuel
  induction fuel with
  | zero =>
      intro node fact member
      exact False.elim (by simpa [bottomState, FiniteIteration.iterate] using member)
  | succ fuel ih =>
      exact stateLe_trans (step_mono graph ih) closed

theorem solve_least_closed (graph : Graph declarations)
    (upper : SolverState declarations) (closed : stateLe (step graph upper) upper) :
    stateLe (solve graph) upper := by
  exact iterate_below_closed graph upper closed (iterationLimit declarations)

structure NodeRenaming (declarations : Nat) where
  forward : Fin declarations → Fin declarations
  backward : Fin declarations → Fin declarations
  backward_forward : ∀ node, backward (forward node) = node
  forward_backward : ∀ node, forward (backward node) = node

def reindexEdge (relabel : NodeRenaming declarations)
    (edge : Edge declarations) : Edge declarations :=
  { edge with target := relabel.forward edge.target }

def reindexNode (relabel : NodeRenaming declarations)
    (node : Node declarations) : Node declarations :=
  { node with edges := node.edges.map (reindexEdge relabel) }

def reindexGraph (relabel : NodeRenaming declarations)
    (graph : Graph declarations) : Graph declarations :=
  { node := fun index => reindexNode relabel (graph.node (relabel.backward index)) }

def reindexState (relabel : NodeRenaming declarations)
    (state : SolverState declarations) : SolverState declarations :=
  fun index => state (relabel.backward index)

theorem edgeContribution_reindex (graph : Graph declarations)
    (state : SolverState declarations) (relabel : NodeRenaming declarations)
    (edge : Edge declarations) :
    edgeContribution (reindexGraph relabel graph) (reindexState relabel state)
      (reindexEdge relabel edge) = edgeContribution graph state edge := by
  cases edge with
  | mk target capturesError readsContract =>
      cases capturesError <;> cases readsContract <;> funext fact <;>
        simp [edgeContribution, visibleEdgeFacts, readEdge, reindexGraph, reindexNode,
          reindexEdge, reindexState, relabel.backward_forward]

theorem nodeEquation_reindex (graph : Graph declarations)
    (state : SolverState declarations) (relabel : NodeRenaming declarations)
    (source : Fin declarations) :
    nodeEquation (reindexGraph relabel graph) (reindexState relabel state)
        (relabel.forward source) = nodeEquation graph state source := by
  funext fact
  change
    ((reindexNode relabel
      (graph.node (relabel.backward (relabel.forward source)))).direct fact ||
      edgesContribution (reindexGraph relabel graph) (reindexState relabel state)
        (reindexNode relabel
          (graph.node (relabel.backward (relabel.forward source)))).edges fact) =
    ((graph.node source).direct fact ||
      edgesContribution graph state (graph.node source).edges fact)
  rw [relabel.backward_forward]
  simp only [reindexNode, edgesContribution, List.any_map]
  simp only [edgeContribution_reindex]

theorem step_reindex (graph : Graph declarations)
    (state : SolverState declarations) (relabel : NodeRenaming declarations) :
    reindexState relabel (step graph state) =
      step (reindexGraph relabel graph) (reindexState relabel state) := by
  funext index
  funext fact
  have equation := nodeEquation_reindex graph state relabel (relabel.backward index)
  rw [relabel.forward_backward] at equation
  simpa [step, reindexState] using congrArg (fun facts =>
    state (relabel.backward index) fact || facts fact) equation.symm

theorem iterate_reindex (graph : Graph declarations)
    (state : SolverState declarations) (relabel : NodeRenaming declarations)
    (fuel : Nat) :
    reindexState relabel (FiniteIteration.iterate (step graph) fuel state) =
      FiniteIteration.iterate (step (reindexGraph relabel graph)) fuel
        (reindexState relabel state) := by
  induction fuel with
  | zero => rfl
  | succ fuel ih =>
      calc
        reindexState relabel
            (step graph (FiniteIteration.iterate (step graph) fuel state)) =
            step (reindexGraph relabel graph)
              (reindexState relabel (FiniteIteration.iterate (step graph) fuel state)) :=
                step_reindex graph _ relabel
        _ = step (reindexGraph relabel graph)
              (FiniteIteration.iterate (step (reindexGraph relabel graph)) fuel
                (reindexState relabel state)) := by rw [ih]
        _ = FiniteIteration.iterate (step (reindexGraph relabel graph)) (fuel + 1)
              (reindexState relabel state) := rfl

theorem solve_reindex (graph : Graph declarations)
    (relabel : NodeRenaming declarations) :
    solve (reindexGraph relabel graph) = reindexState relabel (solve graph) := by
  have iterated := iterate_reindex graph (bottomState declarations) relabel
    (iterationLimit declarations)
  have bottomReindexed :
      reindexState relabel (bottomState declarations) = bottomState declarations := rfl
  rw [bottomReindexed] at iterated
  simpa [solve] using iterated.symm

theorem unresolved_fact_is_preserved (facts : FactSet) :
    (toEffectSummary facts).unresolved = facts none := rfl

def node0 : Fin 3 := ⟨0, by decide⟩
def node1 : Fin 3 := ⟨1, by decide⟩
def node2 : Fin 3 := ⟨2, by decide⟩

def summaryEdge (target : Fin declarations) : Edge declarations :=
  { target := target, capturesError := false, readsContract := false }

/-- A recursive pair reaches a leaf that contributes `time`. -/
def recursiveEffectGraph : Graph 3 where
  node source :=
    if source = node0 then
      { direct := emptyFacts, declared := none, edges := [summaryEdge node1] }
    else if source = node1 then
      { direct := emptyFacts, declared := none,
        edges := [summaryEdge node0, summaryEdge node2] }
    else
      { direct := effectFact .time, declared := none, edges := [] }

/-- Omitting the leaf edge leaves the recursive pair effect-free. -/
def recursiveGraphWithoutLeaf : Graph 3 where
  node source :=
    if source = node0 then
      { direct := emptyFacts, declared := none, edges := [summaryEdge node1] }
    else if source = node1 then
      { direct := emptyFacts, declared := none, edges := [summaryEdge node0] }
    else
      { direct := effectFact .time, declared := none, edges := [] }

theorem recursive_cycle_propagates_leaf_effect :
    solve recursiveEffectGraph node0 (some Effect.time) = true := by
  decide

theorem omitted_edge_changes_recursive_summary :
    solve recursiveGraphWithoutLeaf node0 (some Effect.time) = false := by
  decide

def contractFilesystem : Effect → Bool
  | .fs => true
  | _ => false

def contractVsSummaryGraph : Graph 2 where
  node source :=
    if source = 0 then
      { direct := emptyFacts, declared := none,
        edges := [{ target := 1, capturesError := false, readsContract := true }] }
    else
      { direct := effectFact .time, declared := some contractFilesystem, edges := [] }

theorem declared_contract_is_read_instead_of_body_summary :
    solve contractVsSummaryGraph 0 (some Effect.fs) = true ∧
      solve contractVsSummaryGraph 0 (some Effect.time) = false := by
  decide

def missingDeclaredContractGraph : Graph 2 where
  node source :=
    if source = 0 then
      { direct := emptyFacts, declared := none,
        edges := [{ target := 1, capturesError := false, readsContract := true }] }
    else
      { direct := effectFact .time, declared := none, edges := [] }

theorem missing_contract_stays_unresolved :
    solve missingDeclaredContractGraph 0 none = true ∧
      solve missingDeclaredContractGraph 0 (some Effect.time) = false := by
  decide

def emptyClause : Effect → Bool := fun _ => false

def explicitlyEmptyContractGraph : Graph 2 where
  node source :=
    if source = 0 then
      { direct := emptyFacts, declared := none,
        edges := [{ target := 1, capturesError := false, readsContract := true }] }
    else
      { direct := effectFact .time, declared := some emptyClause, edges := [] }

theorem explicit_empty_contract_stays_empty :
    solve explicitlyEmptyContractGraph 0 none = false ∧
      solve explicitlyEmptyContractGraph 0 (some Effect.time) = false := by
  decide

def errorCapturingEdge : Edge 1 :=
  { target := 0, capturesError := true, readsContract := false }

theorem captured_edge_removes_only_error :
    visibleEdgeFacts errorCapturingEdge (effectFact .error) (some Effect.error) = false ∧
      visibleEdgeFacts errorCapturingEdge (effectFact .time) (some Effect.time) = true := by
  decide

end Xsh.Semantics.Effects.Solver
