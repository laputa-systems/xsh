import Init.Data.List.MinMaxOn
import Lean.Elab.Tactic.Omega

namespace Xsh.Core.DeadlinePolicy

structure Deadline (Identity Metadata : Type) where
  id : Identity
  tick : Nat
  metadata : Metadata
  delivered : Bool
  deriving DecidableEq, Repr

structure State (Identity Metadata : Type) where
  deadlines : List (Deadline Identity Metadata)
  stageAbandonDelivered : Bool
  deriving DecidableEq, Repr

def eligible (now : Nat) (d : Deadline Identity Metadata) : Bool :=
  !d.delivered && decide (d.tick ≤ now)

def candidates (now : Nat) (ds : List (Deadline Identity Metadata)) : List Nat :=
  (List.range ds.length).filter fun i => (ds[i]?.map (eligible now)).getD false

def key (ds : List (Deadline Identity Metadata)) (i : Nat) : Nat :=
  (ds[i]?.map (·.tick)).getD 0 * (ds.length + 1) + i

def choose (now : Nat) (ds : List (Deadline Identity Metadata)) : Option Nat :=
  (candidates now ds).minOn? (key ds)

def openDeadline (s : State Identity Metadata) (id : Identity) (tick : Nat)
    (metadata : Metadata) : State Identity Metadata :=
  { s with deadlines := s.deadlines ++ [⟨id, tick, metadata, false⟩] }

def deliver (now : Nat) (s : State Identity Metadata) :
    Option (Deadline Identity Metadata) × State Identity Metadata :=
  match choose now s.deadlines with
  | none => (none, s)
  | some i => match s.deadlines[i]? with
    | none => (none, s)
    | some d => (some d, { s with deadlines := s.deadlines.set i { d with delivered := true } })

def close [DecidableEq Identity] (id : Identity) (s : State Identity Metadata) : State Identity Metadata :=
  { s with deadlines := s.deadlines.filter fun d => d.id != id }

def finish [DecidableEq Identity] (id : Identity) (s : State Identity Metadata) :
    Option (Deadline Identity Metadata) × State Identity Metadata :=
  match s.deadlines.findIdx? (fun d => d.id == id) with
  | none => (none, s)
  | some i => (s.deadlines[i]?, { s with deadlines := s.deadlines.eraseIdx i })

def rearm (s : State Identity Metadata) : State Identity Metadata :=
  ⟨s.deadlines.map (fun d => { d with delivered := false }), false⟩

def shield (s : State Identity Metadata) : List (Deadline Identity Metadata) × State Identity Metadata :=
  (s.deadlines, { s with deadlines := [] })

def restore (saved : List (Deadline Identity Metadata)) (s : State Identity Metadata) : State Identity Metadata :=
  { s with deadlines := saved }

structure MonotonicTime (HostTime : Type) where
  tick : HostTime → Nat
  beforeOrEqual : HostTime → HostTime → Prop
  order_preserved : ∀ a b, beforeOrEqual a b ↔ tick a ≤ tick b

structure HostDeadline (HostTime Identity Metadata : Type) where
  id : Identity
  deadlineTime : HostTime
  metadata : Metadata
  delivered : Bool

def abstractDeadline (clock : MonotonicTime HostTime)
    (d : HostDeadline HostTime Identity Metadata) : Deadline Identity Metadata :=
  ⟨d.id, clock.tick d.deadlineTime, d.metadata, d.delivered⟩

def stateRel (clock : MonotonicTime HostTime)
    (host : List (HostDeadline HostTime Identity Metadata))
    (model : State Identity Metadata) : Prop :=
  model.deadlines = host.map (abstractDeadline clock)

theorem eligible_monotonic (d : Deadline Identity Metadata) (earlier later : Nat)
    (time_order : earlier ≤ later) (due : eligible earlier d = true) :
    eligible later d = true := by
  simp only [eligible, Bool.and_eq_true, decide_eq_true_eq] at *
  exact ⟨due.1, Nat.le_trans due.2 time_order⟩

private theorem candidate_iff (now : Nat) (ds : List (Deadline Identity Metadata)) (i : Nat) :
    i ∈ candidates now ds ↔ ∃ h : i < ds.length, eligible now ds[i] = true := by
  simp only [candidates, List.mem_filter, List.mem_range]
  constructor
  · rintro ⟨h, e⟩
    exact ⟨h, by simpa [List.getElem?_eq_getElem h] using e⟩
  · rintro ⟨h, e⟩
    exact ⟨h, by simpa [List.getElem?_eq_getElem h] using e⟩

private theorem key_at (ds : List (Deadline Identity Metadata)) (i : Nat) (h : i < ds.length) :
    key ds i = ds[i].tick * (ds.length + 1) + i := by
  simp [key, List.getElem?_eq_getElem h]

private theorem choose_law (now : Nat) (ds : List (Deadline Identity Metadata)) :
    ((choose now ds).isSome ↔ ∃ i, ∃ h : i < ds.length, eligible now ds[i] = true) ∧
    ∀ i, choose now ds = some i →
      ∃ h : i < ds.length, eligible now ds[i] = true ∧
        ∀ j (hj : j < ds.length), eligible now ds[j] = true →
          ds[i].tick ≤ ds[j].tick ∧ (ds[i].tick = ds[j].tick → i ≤ j) := by
  constructor
  · rw [choose, List.isSome_minOn?_iff]
    constructor
    · intro h
      obtain ⟨i, hi⟩ := List.exists_mem_of_ne_nil _ h
      exact ⟨i, (candidate_iff now ds i).mp hi⟩
    · rintro ⟨i, hi, ei⟩ empty
      have := (candidate_iff now ds i).mpr ⟨hi, ei⟩
      simp [empty] at this
  · intro i selected
    have imem : i ∈ candidates now ds := List.minOn?_mem selected
    obtain ⟨hi, ei⟩ := (candidate_iff now ds i).mp imem
    refine ⟨hi, ei, ?_⟩
    intro j hj ej
    have jmem : j ∈ candidates now ds := (candidate_iff now ds j).mpr ⟨hj, ej⟩
    have hmin := List.apply_get_minOn?_le_of_mem (f := key ds) jmem
    have hs : ((candidates now ds).minOn? (key ds)).get (List.isSome_minOn?_of_mem jmem) = i := by
      change (choose now ds).get _ = i
      simp [selected]
    rw [hs, key_at ds i hi, key_at ds j hj] at hmin
    constructor
    · by_cases hn : ds[i].tick ≤ ds[j].tick
      · exact hn
      · have hlt : ds[j].tick + 1 ≤ ds[i].tick := by omega
        have hm := Nat.mul_le_mul_right (ds.length + 1) hlt
        simp only [Nat.add_mul, Nat.one_mul] at hm
        omega
    · intro heq
      rw [heq] at hmin
      omega

theorem related_selection (clock : MonotonicTime HostTime)
    (host : List (HostDeadline HostTime Identity Metadata))
    (model : State Identity Metadata) (now : HostTime) (related : stateRel clock host model) :
    ((choose (clock.tick now) model.deadlines).isSome ↔
      ∃ i, ∃ h : i < host.length,
        host[i].delivered = false ∧ clock.beforeOrEqual host[i].deadlineTime now) ∧
    ∀ i, choose (clock.tick now) model.deadlines = some i →
      ∃ h : i < host.length,
        host[i].delivered = false ∧ clock.beforeOrEqual host[i].deadlineTime now ∧
        ∀ j (hj : j < host.length),
          host[j].delivered = false → clock.beforeOrEqual host[j].deadlineTime now →
          clock.beforeOrEqual host[i].deadlineTime host[j].deadlineTime ∧
            (clock.tick host[i].deadlineTime = clock.tick host[j].deadlineTime → i ≤ j) := by
  rcases model with ⟨deadlines, stage⟩
  simp only [stateRel] at related
  subst deadlines
  simpa [eligible, abstractDeadline, clock.order_preserved, and_assoc] using
    (choose_law (clock.tick now) (host.map (abstractDeadline clock)))

theorem policy_contract [DecidableEq Identity] (now : Nat) (s : State Identity Metadata)
    (id : Identity) (tick : Nat) (metadata : Metadata)
    (saved : List (Deadline Identity Metadata)) :
    (((choose now s.deadlines).isSome ↔ ∃ i, ∃ h : i < s.deadlines.length,
        eligible now s.deadlines[i] = true) ∧
      ∀ i, choose now s.deadlines = some i →
        ∃ h : i < s.deadlines.length, eligible now s.deadlines[i] = true ∧
          ∀ j (hj : j < s.deadlines.length), eligible now s.deadlines[j] = true →
            s.deadlines[i].tick ≤ s.deadlines[j].tick ∧
              (s.deadlines[i].tick = s.deadlines[j].tick → i ≤ j)) ∧
    (openDeadline s id tick metadata =
      ⟨s.deadlines ++ [⟨id, tick, metadata, false⟩], s.stageAbandonDelivered⟩) ∧
    (choose now s.deadlines = none → deliver now s = (none, s)) ∧
    (∀ i, choose now s.deadlines = some i →
      ∃ h : i < s.deadlines.length,
        deliver now s = (some s.deadlines[i],
          ⟨s.deadlines.set i { s.deadlines[i] with delivered := true }, s.stageAbandonDelivered⟩)) ∧
    (close id s = ⟨s.deadlines.filter (fun d => d.id != id), s.stageAbandonDelivered⟩) ∧
    (s.deadlines.findIdx? (fun d => d.id == id) = none → finish id s = (none, s)) ∧
    (∀ i, s.deadlines.findIdx? (fun d => d.id == id) = some i →
      ∃ h : i < s.deadlines.length, s.deadlines[i].id = id ∧
        (∀ j (hj : j < i), s.deadlines[j].id ≠ id) ∧
        finish id s = (some s.deadlines[i],
          ⟨s.deadlines.take i ++ s.deadlines.drop (i + 1), s.stageAbandonDelivered⟩)) ∧
    (rearm s = ⟨s.deadlines.map (fun d => { d with delivered := false }), false⟩) ∧
    (shield s = (s.deadlines, ⟨[], s.stageAbandonDelivered⟩)) ∧
    (restore saved s = ⟨saved, s.stageAbandonDelivered⟩) := by
  refine ⟨choose_law now s.deadlines, rfl, ?_, ?_, rfl, ?_, ?_, rfl, rfl, rfl⟩
  · intro h
    simp [deliver, h]
  · intro i h
    obtain ⟨hi, _, _⟩ := (choose_law now s.deadlines).2 i h
    refine ⟨hi, ?_⟩
    simp [deliver, h, List.getElem?_eq_getElem hi]
  · intro h
    simp [finish, h]
  · intro i h
    obtain ⟨hi, heq, earlier⟩ := List.findIdx?_eq_some_iff_getElem.mp h
    refine ⟨hi, by simpa using heq, ?_, ?_⟩
    · intro j hj
      simpa using earlier j hj
    · simp [finish, h, List.getElem?_eq_getElem hi, List.eraseIdx_eq_take_drop_succ]

private def a : Deadline Nat Nat := ⟨1, 5, 10, false⟩
private def b : Deadline Nat Nat := ⟨2, 3, 20, false⟩
private def c : Deadline Nat Nat := ⟨1, 3, 30, false⟩
private def d : Deadline Nat Nat := ⟨3, 1, 40, true⟩

example : choose 5 [a, b, c, d] = some 1 := by decide
example : choose 2 [a, b, c, d] = none := by decide
example : choose 5 [d, c, b] = some 1 := by decide
example : deliver 5 ⟨[a, b, c, d], true⟩ =
    (some b, ⟨[a, { b with delivered := true }, c, d], true⟩) := by decide
example : close 1 ⟨[a, b, c], true⟩ = ⟨[b], true⟩ := by decide
example : finish 1 ⟨[a, b, c], true⟩ = (some a, ⟨[b, c], true⟩) := by decide
example : rearm ⟨[d, b], true⟩ = ⟨[{ d with delivered := false }, b], false⟩ := by decide
example : shield ⟨[a, b], true⟩ = ([a, b], ⟨[], true⟩) := by decide
example : restore [a, b] ⟨[c], false⟩ = ⟨[a, b], false⟩ := by decide

end Xsh.Core.DeadlinePolicy
