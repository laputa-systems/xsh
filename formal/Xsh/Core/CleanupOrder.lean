import Xsh.Core.CleanupPolicy

namespace Xsh.Core.CleanupOrder

inductive Kind where
  | always | onError | managedRelease
  deriving DecidableEq, Repr

inductive Outcome where
  | ok | fail (forced : Bool)
  deriving DecidableEq, Repr

structure Action where
  kind : Kind
  outcome : Outcome
  deriving DecidableEq, Repr

def nextDue (actions : List Action) (next : Nat) (leavesWithError : Bool) : Option Nat :=
  match next with
  | 0 => none
  | n + 1 =>
    if (actions[n]?.map Action.kind) != some .onError || leavesWithError then some n
    else nextDue actions n leavesWithError

def recorded (action : Action) : Bool :=
  match action.outcome with
  | .ok => false
  | .fail forced => CleanupPolicy.failureIsRecorded (action.kind == .managedRelease) forced

theorem nextDue_lt (actions : List Action) (next : Nat) (flag : Bool) (i : Nat)
    (h : nextDue actions next flag = some i) : i < next := by
  induction next with
  | zero => simp [nextDue] at h
  | succ n ih =>
    simp only [nextDue] at h
    split at h
    · simp at h; omega
    · have := ih h; omega

def unwindFrom (actions : List Action) (next : Nat) (leavesWithError : Bool) : List Nat × Bool :=
  match h : nextDue actions next leavesWithError with
  | none => ([], leavesWithError)
  | some i =>
    let (rest, flag) := unwindFrom actions i (leavesWithError || (actions[i]?.map recorded).getD false)
    (i :: rest, flag)
termination_by next
decreasing_by exact nextDue_lt actions next leavesWithError i h

def unwind (actions : List Action) (leavesWithError : Bool) : List Nat × Bool :=
  unwindFrom actions actions.length leavesWithError

theorem unwindFrom_bound (actions : List Action) (next : Nat) (flag : Bool) :
    ∀ i ∈ (unwindFrom actions next flag).1, i < next := by
  induction next using Nat.strongRecOn generalizing flag with
  | ind next ih =>
    rw [unwindFrom]
    split
    · simp
    · rename_i i h
      have hi := nextDue_lt actions next flag i h
      simp only [List.mem_cons, Prod.fst]
      intro j hj
      rcases hj with rfl | hj
      · exact hi
      · have := ih i hi _ j hj
        omega

theorem unwind_order (actions : List Action) (flag : Bool) :
    ((unwind actions flag).1).Pairwise (· > ·) := by
  have aux : ∀ next flag, ((unwindFrom actions next flag).1).Pairwise (· > ·) := by
    intro next
    induction next using Nat.strongRecOn with
    | ind next ih =>
      intro flag
      rw [unwindFrom]
      split
      · simp
      · rename_i i h
        simp only [Prod.fst, List.pairwise_cons]
        exact ⟨unwindFrom_bound actions i _ , ih i (nextDue_lt actions next flag i h) _⟩
  exact aux _ _

theorem unwind_once (actions : List Action) (flag : Bool) :
    ((unwind actions flag).1).Nodup := by
  exact (unwind_order actions flag).imp (fun h => Nat.ne_of_gt h)

theorem unwindFrom_skip (actions : List Action) (n : Nat)
    (h : (actions[n]?.map Action.kind) = some .onError) :
    unwindFrom actions (n+1) false = unwindFrom actions n false := by
  have eq : nextDue actions (n+1) false = nextDue actions n false := by simp [nextDue, h]
  rw [unwindFrom, eq, unwindFrom]

theorem unwindFrom_run (actions : List Action) (n : Nat) (flag : Bool)
    (h : ((actions[n]?.map Action.kind) != some .onError || flag) = true) :
    unwindFrom actions (n+1) flag =
      (n :: (unwindFrom actions n (flag || (actions[n]?.map recorded).getD false)).1,
       (unwindFrom actions n (flag || (actions[n]?.map recorded).getD false)).2) := by
  have eq : nextDue actions (n+1) flag = some n := by simp [nextDue, h]
  rw [unwindFrom, eq]

theorem unwindFrom_membership (actions : List Action) (next : Nat) (flag : Bool) (i : Nat)
    (hi : i < next) :
    i ∈ (unwindFrom actions next flag).1 ↔
      (actions[i]?.map Action.kind) != some .onError ∨ flag = true ∨
      ∃ j ∈ (unwindFrom actions next flag).1, i < j ∧ (actions[j]?.map recorded).getD false = true := by
  induction next generalizing flag with
  | zero => omega
  | succ n ih =>
    by_cases due : ((actions[n]?.map Action.kind) != some .onError || flag) = true
    · rw [unwindFrom_run actions n flag due]
      simp only [List.mem_cons, Prod.fst]
      by_cases equal : i = n
      · subst i
        simp only [true_or, iff_true]
        cases flag <;> simp_all
      · have ilt : i < n := by omega
        specialize ih (flag || (actions[n]?.map recorded).getD false) ilt
        simp only [Bool.or_eq_true] at ih
        constructor
        · intro h
          have imem : i ∈ (unwindFrom actions n (flag || (actions[n]?.map recorded).getD false)).1 := by
            rcases h with h | h
            · omega
            · exact h
          rcases ih.mp imem with h | (h | h) | ⟨j, hj, hij, hr⟩
          · exact Or.inl h
          · exact Or.inr (Or.inl h)
          · exact Or.inr (Or.inr ⟨n, Or.inl rfl, ilt, h⟩)
          · exact Or.inr (Or.inr ⟨j, Or.inr hj, hij, hr⟩)
        · intro h
          apply Or.inr
          apply ih.mpr
          rcases h with h | h | ⟨j, hj, hij, hr⟩
          · exact Or.inl h
          · exact Or.inr (Or.inl (Or.inl h))
          · rcases hj with hj | hj
            · subst j
              exact Or.inr (Or.inl (Or.inr hr))
            · exact Or.inr (Or.inr ⟨j, hj, hij, hr⟩)
    · have hn : (actions[n]?.map Action.kind) = some .onError ∧ flag = false := by
        cases flag <;> simp_all
      rcases hn with ⟨hn, rfl⟩
      rw [unwindFrom_skip actions n hn]
      by_cases equal : i = n
      · subst i
        have absent : n ∉ (unwindFrom actions n false).1 := by
          intro h
          have := unwindFrom_bound actions n false n h
          omega
        simp only [absent, hn, bne_self_eq_false, Bool.false_eq_true, false_or, false_iff]
        intro ⟨j, hj, hjn, _⟩
        have := unwindFrom_bound actions n false j hj
        omega
      · exact ih false (by omega)

theorem unwind_unconditional (actions : List Action) (flag : Bool) (i : Nat) (action : Action)
    (hi : i < actions.length) (ha : actions[i]? = some action)
    (hk : action.kind = .always ∨ action.kind = .managedRelease) :
    i ∈ (unwind actions flag).1 := by
  apply (unwindFrom_membership actions actions.length flag i hi).mpr
  left
  rcases hk with h | h <;> simp [ha, h]

theorem unwind_onError_due (actions : List Action) (flag : Bool) (i : Nat) (action : Action)
    (hi : i < actions.length) (ha : actions[i]? = some action) (hk : action.kind = .onError) :
    i ∈ (unwind actions flag).1 ↔ flag = true ∨
      ∃ j ∈ (unwind actions flag).1, i < j ∧ (actions[j]?.map recorded).getD false = true := by
  simpa [unwind, ha, hk] using unwindFrom_membership actions actions.length flag i hi

end Xsh.Core.CleanupOrder
