import Xsh.Core.CleanupOrder

namespace Xsh.Core.CleanupUnwind

open CleanupOrder

def failure (actions : List Action) (i : Nat) : Option Bool :=
  (actions[i]?).bind fun a => match a.outcome with
    | .ok => none
    | .fail forced => some forced

def recordedFailures (actions : List Action) (flag : Bool) : List CleanupPolicy.Error :=
  (unwind actions flag).1.filterMap fun i =>
    (failure actions i).bind fun forced =>
      if (actions[i]?.map recorded).getD false then some ⟨i, forced⟩ else none

def directReports (actions : List Action) (flag : Bool) : List Nat :=
  (unwind actions flag).1.filter fun i =>
    (failure actions i).isSome && !(actions[i]?.map recorded).getD false

def outcome (actions : List Action) (flag : Bool) : Option CleanupPolicy.Error × List CleanupPolicy.Error :=
  CleanupPolicy.fold none (recordedFailures actions flag)

theorem outcome_conserves (actions : List Action) (flag : Bool) :
    List.Perm ((outcome actions flag).1.toList ++ (outcome actions flag).2)
      (recordedFailures actions flag) := by
  simpa [outcome] using CleanupPolicy.fold_conserves none (recordedFailures actions flag)

theorem outcome_selects (actions : List Action) (flag : Bool) :
    (outcome actions flag).1 =
      ((recordedFailures actions flag).find? (·.forced)).orElse
        (fun _ => (recordedFailures actions flag).head?) := by
  simpa [outcome, CleanupPolicy.selected] using CleanupPolicy.fold_selects none (recordedFailures actions flag)

private theorem unwindFrom_recorded_flag (actions : List Action) (next : Nat) (flag : Bool) :
    (flag = true ∨ ∃ i ∈ (unwindFrom actions next flag).1,
      (actions[i]?.map recorded).getD false = true) →
    (unwindFrom actions next flag).2 = true := by
  induction next using Nat.strongRecOn generalizing flag with
  | ind next ih =>
    rw [unwindFrom]
    split
    · simp
    · rename_i i h
      simp only [Prod.fst, Prod.snd, List.mem_cons]
      intro hyp
      apply ih i (nextDue_lt actions next flag i h)
      simp only [Bool.or_eq_true]
      rcases hyp with hf | ⟨j, hj, hr⟩
      · exact Or.inl (Or.inl hf)
      · rcases hj with rfl | hj
        · exact Or.inl (Or.inr hr)
        · exact Or.inr ⟨j, hj, hr⟩

theorem recorded_failure_sets_flag (actions : List Action) (flag : Bool)
    (h : (recordedFailures actions flag) ≠ []) : (unwind actions flag).2 = true := by
  obtain ⟨e, he⟩ := List.exists_mem_of_ne_nil _ h
  simp only [recordedFailures, List.mem_filterMap] at he
  obtain ⟨i, hi, he⟩ := he
  apply unwindFrom_recorded_flag actions actions.length flag
  right
  refine ⟨i, hi, ?_⟩
  cases hf : failure actions i <;> simp_all

theorem primary_none_iff (actions : List Action) (flag : Bool) :
    (outcome actions flag).1 = none ↔ recordedFailures actions flag = [] := by
  rw [outcome_selects]
  cases h : recordedFailures actions flag with
  | nil => simp
  | cons e es =>
    simp only [List.head?, List.cons_ne_nil, iff_false]
    cases (e :: es).find? (·.forced) <;> simp

private theorem accounting (actions : List Action) (xs : List Nat) (i : Nat) :
    ((xs.filterMap fun j => (failure actions j).bind fun forced =>
      if (actions[j]?.map recorded).getD false then some (CleanupPolicy.Error.mk j forced) else none).map
        CleanupPolicy.Error.id ++
      xs.filter (fun j => (failure actions j).isSome && !(actions[j]?.map recorded).getD false)).count i =
    (xs.filter fun j => (failure actions j).isSome).count i := by
  induction xs with
  | nil => simp
  | cons j js ih =>
    cases hf : failure actions j <;> cases hr : (actions[j]?.map recorded).getD false <;>
      simp [hf, hr, List.count_cons] at * <;> omega

theorem reports_count (actions : List Action) (flag : Bool) (i : Nat) :
    ((recordedFailures actions flag).map CleanupPolicy.Error.id ++ directReports actions flag).count i =
      ((unwind actions flag).1.filter fun j => (failure actions j).isSome).count i := by
  exact accounting actions _ i

theorem recorded_order (actions : List Action) (flag : Bool) :
    (recordedFailures actions flag).Pairwise (fun a b => a.id > b.id) := by
  apply List.pairwise_filterMap.mpr
  apply (CleanupOrder.unwind_order actions flag).imp
  intro i j hij a ha b hb
  cases hi : failure actions i <;> cases hj : failure actions j <;> simp_all
  rcases ha with ⟨_, rfl⟩
  rcases hb with ⟨_, rfl⟩
  exact hij

theorem failure_accounted (actions : List Action) (flag : Bool) (i : Nat)
    (hi : i ∈ (unwind actions flag).1) (hf : (failure actions i).isSome = true) :
    ((recordedFailures actions flag).map CleanupPolicy.Error.id ++ directReports actions flag).count i = 1 := by
  rw [recordedFailures, directReports, accounting]
  rw [List.count_filter (p := fun j => (failure actions j).isSome) hf]
  exact (CleanupOrder.unwind_once actions flag).count_of_mem hi

theorem managed_unforced_not_primary (actions : List Action) (flag : Bool) (i : Nat)
    (action : Action) (ha : actions[i]? = some action)
    (hk : action.kind = .managedRelease) (hf : action.outcome = .fail false) :
    ∀ forced, (outcome actions flag).1 ≠ some ⟨i, forced⟩ := by
  intro forced hp
  have hm : (⟨i, forced⟩ : CleanupPolicy.Error) ∈ recordedFailures actions flag := by
    apply (outcome_conserves actions flag).mem_iff.mp
    simp [hp]
  simp only [recordedFailures, List.mem_filterMap] at hm
  obtain ⟨j, _, hj⟩ := hm
  cases hfj : failure actions j with
  | none => simp [hfj] at hj
  | some f =>
    simp only [hfj, Option.bind_some] at hj
    split at hj
    · simp only [Option.some.injEq, CleanupPolicy.Error.mk.injEq] at hj
      obtain ⟨rfl, rfl⟩ := hj
      simp [ha, recorded, hk, hf, CleanupPolicy.failureIsRecorded] at *
    · simp at hj

end Xsh.Core.CleanupUnwind
