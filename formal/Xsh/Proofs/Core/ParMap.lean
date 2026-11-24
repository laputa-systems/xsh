import Std

namespace Xsh.Proofs.Core.ParMap

inductive ItemOutcome (Value Error Control : Type) where
  | value : Value → ItemOutcome Value Error Control
  | failure : Error → ItemOutcome Value Error Control
  | leave : Control → ItemOutcome Value Error Control
  deriving DecidableEq, Repr

inductive Stop (Error : Type) where
  | open
  | leave
  | failure
  | external : Error → Stop Error
  deriving DecidableEq, Repr

structure Completion (Value Error Control : Type) where
  index : Nat
  stopped : Bool
  outcome : ItemOutcome Value Error Control
  deriving DecidableEq, Repr

structure State (Value Error Control : Type) where
  running : List Nat
  started : List Nat
  finished : List (Completion Value Error Control)
  stop : Stop Error
  deriving DecidableEq, Repr

def initial : State Value Error Control := ⟨[], [], [], .open⟩

def nextStop (old : Stop Error) (outcome : ItemOutcome Value Error Control) : Stop Error :=
  match old, outcome with
  | .external reason, _ => .external reason
  | _, .failure _ => .failure
  | .open, .leave _ => .leave
  | _, _ => old

inductive Step (n jobs : Nat) (callback : Nat → Bool → ItemOutcome Value Error Control) :
    State Value Error Control → State Value Error Control → Prop where
  | start (s : State Value Error Control) (i : Nat) :
      s.stop = .open → i < n → i ∉ s.started → s.running.length < jobs →
      Step n jobs callback s {s with running := i :: s.running, started := i :: s.started}
  | complete (s : State Value Error Control) (i : Nat) :
      i ∈ s.running → (∀ reason, s.stop ≠ .external reason) →
      Step n jobs callback s
        {s with running := s.running.erase i, finished := ⟨i, false, callback i false⟩ :: s.finished, stop := nextStop s.stop (callback i false)}
  | stopItem (s : State Value Error Control) (i : Nat) :
      i ∈ s.running → (s.stop = .failure ∨ ∃ reason, s.stop = .external reason) →
      Step n jobs callback s
        {s with running := s.running.erase i, finished := ⟨i, true, callback i true⟩ :: s.finished}
  | giveUp (s : State Value Error Control) (reason : Error) :
      (∀ previous, s.stop ≠ .external previous) →
      Step n jobs callback s {s with stop := .external reason}

inductive Reachable (n jobs : Nat) (callback : Nat → Bool → ItemOutcome Value Error Control) :
    State Value Error Control → Prop where
  | initial : Reachable n jobs callback initial
  | step {before after} : Reachable n jobs callback before → Step n jobs callback before after →
      Reachable n jobs callback after

def Accounted (s : State Value Error Control) : Prop :=
  ∀ i, i ∈ s.started → i ∈ s.running ∨ ∃ c ∈ s.finished, c.index = i

theorem step_bound {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {before after : State Value Error Control}
    (bound : before.running.length ≤ jobs) (step : Step n jobs callback before after) :
    after.running.length ≤ jobs := by
  cases step with
  | start i _ _ _ room => simpa using Nat.succ_le_of_lt room
  | complete i _ _ => exact Nat.le_trans (List.length_erase_le) bound
  | stopItem i _ _ => exact Nat.le_trans (List.length_erase_le) bound
  | giveUp reason _ => exact bound

theorem bound {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s) :
    s.running.length ≤ jobs := by
  induction reachable with
  | initial => simp [initial]
  | step _ h ih => exact step_bound ih h

theorem step_accounted {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {before after : State Value Error Control}
    (accounted : Accounted before) (step : Step n jobs callback before after) : Accounted after := by
  cases step with
  | start i _ _ _ _ =>
      intro j hj
      simp only [List.mem_cons] at hj ⊢
      rcases hj with rfl | hj
      · exact Or.inl (Or.inl rfl)
      · rcases accounted j hj with running | ⟨c, member, eq⟩
        · exact Or.inl (Or.inr running)
        · exact Or.inr ⟨c, member, eq⟩
  | complete i running _ =>
      intro j hj
      rcases accounted j hj with active | ⟨c, member, eq⟩
      · by_cases same : j = i
        · subst j
          exact Or.inr ⟨⟨i, false, callback i false⟩, by simp, rfl⟩
        · exact Or.inl ((List.mem_erase_of_ne same).2 active)
      · exact Or.inr ⟨c, by simp [member], eq⟩
  | stopItem i running _ =>
      intro j hj
      rcases accounted j hj with active | ⟨c, member, eq⟩
      · by_cases same : j = i
        · subst j
          exact Or.inr ⟨⟨i, true, callback i true⟩, by simp, rfl⟩
        · exact Or.inl ((List.mem_erase_of_ne same).2 active)
      · exact Or.inr ⟨c, by simp [member], eq⟩
  | giveUp reason _ => exact accounted

theorem accounted {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s) : Accounted s := by
  induction reachable with
  | initial => simp [Accounted, initial]
  | step _ h ih => exact step_accounted ih h

def Returnable (n : Nat) (s : State Value Error Control) : Prop :=
  s.running = [] ∧ (s.stop ≠ .open ∨ ∀ i, i < n → i ∈ s.started)

theorem drain {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s)
    (returns : Returnable n s) :
    ∀ i ∈ s.started, ∃ c ∈ s.finished, c.index = i := by
  intro i started
  rcases accounted reachable i started with active | done
  · rw [returns.1] at active
    simp at active
  · exact done


def isDecision (c : Completion Value Error Control) : Bool :=
  !c.stopped && match c.outcome with
    | .value _ => false
    | .failure _ | .leave _ => true

def candidate (s : State Value Error Control) (i : Nat) : Bool :=
  s.finished.any fun c => c.index == i && isDecision c

def select (n : Nat) (s : State Value Error Control) : Option Nat :=
  (List.range n).find? (candidate s)

theorem select_earliest {n : Nat} {s : State Value Error Control} {i : Nat}
    (selected : select n s = some i) :
    i < n ∧ candidate s i = true ∧ ∀ j, j < i → candidate s j = false := by
  have h := List.find?_range_eq_some.mp selected
  refine ⟨List.mem_range.mp h.2.1, h.1, ?_⟩
  intro j before
  simpa using h.2.2 j before

theorem select_exists {n : Nat} {s : State Value Error Control} {i : Nat}
    (inside : i < n) (found : candidate s i = true) :
    ∃ first, select n s = some first := by
  cases h : select n s with
  | none =>
      have hnone := List.find?_range_eq_none.mp h i inside
      simp [found] at hnone
  | some first => exact ⟨first, rfl⟩

theorem candidate_iff {s : State Value Error Control} {i : Nat} :
    candidate s i = true ↔
      ∃ c ∈ s.finished, c.index = i ∧ c.stopped = false ∧
        ((∃ error, c.outcome = .failure error) ∨
         (∃ control, c.outcome = .leave control)) := by
  simp only [candidate, List.any_eq_true, Bool.and_eq_true, beq_iff_eq]
  constructor
  · rintro ⟨c, mem, index, decision⟩
    cases h : c.outcome with
    | value v => simp [isDecision, h] at decision
    | failure error =>
        exact ⟨c, mem, index, by simpa [isDecision, h] using decision, Or.inl ⟨error, h⟩⟩
    | leave control =>
        exact ⟨c, mem, index, by simpa [isDecision, h] using decision, Or.inr ⟨control, h⟩⟩
  · intro h
    rcases h with ⟨c, mem, index, stopped, ⟨error, outcome⟩ | ⟨control, outcome⟩⟩
    · exact ⟨c, mem, index, by simp [isDecision, stopped, outcome]⟩
    · exact ⟨c, mem, index, by simp [isDecision, stopped, outcome]⟩

def valueAt (s : State Value Error Control) (i : Nat) : Option Value :=
  s.finished.findSome? fun c =>
    if c.index == i && !c.stopped then
      match c.outcome with
      | .value value => some value
      | _ => none
    else none

def values (n : Nat) (s : State Value Error Control) : Option (List Value) :=
  (List.range n).mapM (valueAt s)

def OpenFinished (s : State Value Error Control)
    (callback : Nat → Bool → ItemOutcome Value Error Control) : Prop :=
  s.stop = .open → ∀ c ∈ s.finished,
    c.stopped = false ∧ ∃ value, c.outcome = .value value ∧ callback c.index false = .value value

theorem nextStop_open {old : Stop Error} {result : ItemOutcome Value Error Control}
    (h : nextStop old result = .open) :
    old = .open ∧ ∃ value, result = .value value := by
  cases old <;> cases result <;> simp_all [nextStop]

theorem step_openFinished {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {before after : State Value Error Control}
    (valid : OpenFinished before callback) (step : Step n jobs callback before after) :
    OpenFinished after callback := by
  cases step with
  | start i _ _ _ _ => exact valid
  | complete i running _ =>
      intro opened c member
      have old := nextStop_open opened
      simp only [List.mem_cons] at member
      rcases member with rfl | member
      · rcases old.2 with ⟨v, outcome⟩
        exact ⟨rfl, v, outcome, outcome⟩
      · exact valid old.1 c member
  | stopItem i running stopped =>
      intro opened c member
      rcases stopped with bad | ⟨reason, bad⟩ <;> simp_all
  | giveUp reason _ =>
      intro opened
      cases opened

theorem openFinished {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s) : OpenFinished s callback := by
  induction reachable with
  | initial => simp [OpenFinished, initial]
  | step _ h ih => exact step_openFinished ih h


theorem valueAt_open {s : State Value Error Control}
    {callback : Nat → Bool → ItemOutcome Value Error Control} {i : Nat} {v : Value}
    (good : OpenFinished s callback) (opened : s.stop = .open)
    (entry : ∃ c ∈ s.finished, c.index = i) (expected : callback i false = .value v) :
    valueAt s i = some v := by
  have hasValue : (valueAt s i).isSome = true := by
    apply List.findSome?_isSome_iff.mpr
    rcases entry with ⟨c, member, index⟩
    obtain ⟨stopped, value, result, resultCallback⟩ := good opened c member
    subst i
    rw [expected] at resultCallback
    cases resultCallback
    exact ⟨c, member, by simp [stopped, result]⟩
  obtain ⟨found, foundEq⟩ := Option.isSome_iff_exists.mp hasValue
  have existsFound := List.exists_of_findSome?_eq_some foundEq
  rcases existsFound with ⟨c, member, cvalue⟩
  have details := good opened c member
  obtain ⟨stopped, value, result, resultCallback⟩ := details
  simp only [stopped, Bool.not_false, Bool.and_true] at cvalue
  split at cvalue
  next index =>
    simp only [result, Option.some.injEq] at cvalue
    subst found
    have index' : c.index = i := by simpa using index
    rw [index', expected] at resultCallback
    cases resultCallback
    exact foundEq
  next _ => simp at cvalue

theorem values_open {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s)
    (returns : Returnable n s) (opened : s.stop = .open)
    (mapped : Nat → Value) (agrees : ∀ i, i < n → callback i false = .value (mapped i)) :
    values n s = some ((List.range n).map mapped) := by
  have every : ∀ i ∈ List.range n, valueAt s i = some (mapped i) := by
    intro i member
    have inside := List.mem_range.mp member
    have started : i ∈ s.started := by
      rcases returns.2 with notOpen | all
      · exact False.elim (notOpen opened)
      · exact all i inside
    exact valueAt_open (openFinished reachable) opened (drain reachable returns i started) (agrees i inside)
  unfold values
  have helper : ∀ (indices : List Nat), (∀ i ∈ indices, valueAt s i = some (mapped i)) →
      indices.mapM (valueAt s) = some (indices.map mapped) := by
    intro indices
    induction indices with
    | nil => simp
    | cons a rest ih =>
        intro all
        have first := all a (by simp)
        have tail : ∀ i ∈ rest, valueAt s i = some (mapped i) := by
          intro i member
          exact all i (by simp [member])
        simp [List.mapM_cons, first, ih tail]
  exact helper _ every


def selectedCompletion (s : State Value Error Control) (i : Nat) : Option (Completion Value Error Control) :=
  s.finished.find? fun c => c.index == i && isDecision c

inductive Result (Value Error Control : Type) where
  | values : List Value → Result Value Error Control
  | decided : Completion Value Error Control → Result Value Error Control
  | external : Error → Result Value Error Control
  | incomplete : Result Value Error Control
  deriving DecidableEq, Repr

def result (n : Nat) (s : State Value Error Control) : Result Value Error Control :=
  match s.stop with
  | .external error => .external error
  | _ =>
    match select n s with
    | some i =>
      match selectedCompletion s i with
      | some c => .decided c
      | none => .incomplete
    | none =>
      match values n s with
      | some vs => .values vs
      | none => .incomplete

theorem noCandidate_open {s : State Value Error Control}
    {callback : Nat → Bool → ItemOutcome Value Error Control}
    (good : OpenFinished s callback) (opened : s.stop = .open) (i : Nat) :
    candidate s i = false := by
  cases h : candidate s i with
  | false => rfl
  | true =>
    obtain ⟨c, member, _, _, failure | leave⟩ := candidate_iff.mp h
    · obtain ⟨v, value, result, _⟩ := good opened c member
      rcases failure with ⟨error, bad⟩
      rw [result] at bad
      cases bad
    · obtain ⟨v, value, result, _⟩ := good opened c member
      rcases leave with ⟨control, bad⟩
      rw [result] at bad
      cases bad

theorem select_none_open {s : State Value Error Control}
    {callback : Nat → Bool → ItemOutcome Value Error Control}
    (good : OpenFinished s callback) (opened : s.stop = .open) (n : Nat) :
    select n s = none := by
  apply List.find?_range_eq_none.mpr
  intro i _
  simp [noCandidate_open good opened i]

theorem result_open {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s)
    (returns : Returnable n s) (opened : s.stop = .open)
    (mapped : Nat → Value) (agrees : ∀ i, i < n → callback i false = .value (mapped i)) :
    result n s = .values ((List.range n).map mapped) := by
  simp [result, opened, select_none_open (openFinished reachable) opened n,
    values_open reachable returns opened mapped agrees]

theorem selectedCompletion_spec {s : State Value Error Control} {i : Nat} {c : Completion Value Error Control}
    (h : selectedCompletion s i = some c) :
    c ∈ s.finished ∧ c.index = i ∧ c.stopped = false ∧
      ((∃ error, c.outcome = .failure error) ∨ (∃ control, c.outcome = .leave control)) := by
  have member := List.mem_of_find?_eq_some h
  have test := List.find?_some h
  simp only [Bool.and_eq_true, beq_iff_eq] at test
  have stopFalse : c.stopped = false := by
    cases outcome : c.outcome <;> simp_all [isDecision]
  refine ⟨member, test.1, stopFalse, ?_⟩
  cases outcome : c.outcome with
  | value v => simp [isDecision, outcome] at test
  | failure error => exact Or.inl ⟨error, rfl⟩
  | leave control => exact Or.inr ⟨control, rfl⟩

theorem selection_result {n : Nat} {s : State Value Error Control} {i : Nat}
    (notExternal : ∀ error, s.stop ≠ .external error) (selected : select n s = some i) :
    ∃ c, result n s = .decided c ∧ c ∈ s.finished ∧ c.index = i ∧ c.stopped = false ∧
      ((∃ error, c.outcome = .failure error) ∨ (∃ control, c.outcome = .leave control)) ∧
      ∀ j, j < i → candidate s j = false := by
  have isSome : (selectedCompletion s i).isSome = true := by
    unfold selectedCompletion
    rw [List.find?_isSome]
    have h := (select_earliest selected).2.1
    simpa [selectedCompletion, candidate, List.any_eq_true] using h
  obtain ⟨c, hc⟩ := Option.isSome_iff_exists.mp isSome
  have spec := selectedCompletion_spec hc
  refine ⟨c, ?_, spec.1, spec.2.1, spec.2.2.1, spec.2.2.2, (select_earliest selected).2.2⟩
  cases st : s.stop with
  | «open» => simp [result, st, selected, hc]
  | leave => simp [result, st, selected, hc]
  | failure => simp [result, st, selected, hc]
  | external error => exact False.elim (notExternal error st)

theorem external_result {n : Nat} {s : State Value Error Control} {reason : Error}
    (givenUp : s.stop = .external reason) : result n s = .external reason := by
  simp [result, givenUp]


def RunningWithin (n : Nat) (s : State Value Error Control) : Prop :=
  ∀ i ∈ s.running, i < n

theorem runningWithin {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s) : RunningWithin n s := by
  induction reachable with
  | initial => simp [RunningWithin, initial]
  | step prior step ih =>
      cases step with
      | start i _ inside _ _ =>
          intro j member
          simp only [List.mem_cons] at member
          rcases member with rfl | member
          · exact inside
          · exact ih j member
      | complete i running _ =>
          intro j member
          exact ih j (List.mem_of_mem_erase member)
      | stopItem i running _ =>
          intro j member
          exact ih j (List.mem_of_mem_erase member)
      | giveUp reason _ => exact ih

def HasCause (n : Nat) (s : State Value Error Control) : Prop :=
  (s.stop = .leave ∨ s.stop = .failure) →
    ∃ c ∈ s.finished, c.index < n ∧ isDecision c = true

theorem hasCause {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s) : HasCause n s := by
  induction reachable with
  | initial => simp [HasCause, initial]
  | @step before after prior step ih =>
      have within := runningWithin prior
      cases step with
      | start i _ _ _ _ => exact ih
      | complete i running _ =>
          have inRange := within i running
          have oldPreserved : (before.stop = .leave ∨ before.stop = .failure) →
              ∃ c ∈ (⟨i, false, callback i false⟩ :: before.finished), c.index < n ∧ isDecision c = true := by
            intro old
            obtain ⟨c, member, inside, decision⟩ := ih old
            exact ⟨c, by simp [member], inside, decision⟩
          intro stopped
          cases old : before.stop <;> cases out : callback i false <;>
            simp_all [nextStop, isDecision] <;> try (first | exact oldPreserved (by simp [old]) | assumption)
      | stopItem i running stopped =>
          intro reason
          obtain ⟨c, member, inside, decision⟩ := ih reason
          exact ⟨c, by simp [member], inside, decision⟩
      | giveUp reason _ =>
          intro h
          simp at h

theorem candidate_from_cause {n : Nat} {s : State Value Error Control}
    (cause : HasCause n s) (stopped : s.stop = .leave ∨ s.stop = .failure) :
    ∃ i, i < n ∧ candidate s i = true := by
  obtain ⟨c, member, inside, decision⟩ := cause stopped
  refine ⟨c.index, inside, ?_⟩
  simp only [candidate, List.any_eq_true, Bool.and_eq_true, beq_iff_eq]
  exact ⟨c, member, rfl, decision⟩

theorem nonexternal_nonopen_selection {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s)
    (notExternal : ∀ error, s.stop ≠ .external error) (notOpen : s.stop ≠ .open) :
    ∃ i c, result n s = .decided c ∧ c ∈ s.finished ∧ c.index = i ∧ c.stopped = false ∧
      ((∃ error, c.outcome = .failure error) ∨ (∃ control, c.outcome = .leave control)) ∧
      ∀ j, j < i → candidate s j = false := by
  have stopped : s.stop = .leave ∨ s.stop = .failure := by
    cases st : s.stop with
    | «open» => exact False.elim (notOpen st)
    | leave => exact Or.inl rfl
    | failure => exact Or.inr rfl
    | external e => exact False.elim (notExternal e st)
  obtain ⟨i, inside, candidate⟩ := candidate_from_cause (hasCause reachable) stopped
  obtain ⟨first, selected⟩ := select_exists inside candidate
  obtain ⟨c, rest⟩ := selection_result notExternal selected
  exact ⟨first, c, rest⟩


def FinishedCorrect (s : State Value Error Control)
    (callback : Nat → Bool → ItemOutcome Value Error Control) : Prop :=
  ∀ c ∈ s.finished, c.outcome = callback c.index c.stopped

theorem finishedCorrect {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s) : FinishedCorrect s callback := by
  induction reachable with
  | initial => simp [FinishedCorrect, initial]
  | step prior step ih =>
      cases step with
      | start i _ _ _ _ => exact ih
      | complete i _ _ =>
          intro c member
          simp only [List.mem_cons] at member
          rcases member with rfl | member
          · rfl
          · exact ih c member
      | stopItem i _ _ =>
          intro c member
          simp only [List.mem_cons] at member
          rcases member with rfl | member
          · rfl
          · exact ih c member
      | giveUp reason _ => exact ih


theorem finishedWithin {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s) :
    ∀ c ∈ s.finished, c.index < n := by
  induction reachable with
  | initial => simp [initial]
  | step prior step ih =>
      have within := runningWithin prior
      cases step with
      | start i _ _ _ _ => exact ih
      | complete i running _ =>
          intro c mem
          simp only [List.mem_cons] at mem
          rcases mem with rfl | mem
          · exact within i running
          · exact ih c mem
      | stopItem i running _ =>
          intro c mem
          simp only [List.mem_cons] at mem
          rcases mem with rfl | mem
          · exact within i running
          · exact ih c mem
      | giveUp reason _ => exact ih

theorem no_signal_open {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {s : State Value Error Control} (reachable : Reachable n jobs callback s)
    (notExternal : ∀ error, s.stop ≠ .external error)
    (normal : ∀ i, i < n → ∃ v, callback i false = .value v) : s.stop = .open := by
  by_cases notOpen : s.stop = .open
  · exact notOpen
  ·
    obtain ⟨i, c, _, member, index, stopped, abnormal, _⟩ :=
      nonexternal_nonopen_selection reachable notExternal notOpen
    have selectedInside : i < n := by
      simpa [index] using finishedWithin reachable c member
    have correct := finishedCorrect reachable c member
    rw [stopped, index] at correct
    obtain ⟨v, value⟩ := normal i selectedInside
    rw [value] at correct
    rcases abnormal with ⟨e, bad⟩ | ⟨k, bad⟩ <;> rw [correct] at bad <;> cases bad


def indexed (inputs : List Input) (callback : Input → Bool → ItemOutcome Value Error Control)
    (fallback : ItemOutcome Value Error Control) (i : Nat) (stopped : Bool) : ItemOutcome Value Error Control :=
  match inputs[i]? with
  | some input => callback input stopped
  | none => fallback

theorem mapped_range (inputs : List Input) (fallback : Input) (f : Input → Value) :
    ((List.range inputs.length).map fun i => f (inputs[i]?.getD fallback)) = inputs.map f := by
  have indices := List.zipIdx_map_snd 0 inputs
  have entries := List.zipIdx_map_fst 0 inputs
  have same : (inputs.zipIdx.map fun pair => f (inputs[pair.2]?.getD fallback)) =
      (inputs.zipIdx.map fun pair => f pair.1) := by
    apply List.map_congr_left
    intro pair member
    have item := List.mem_zipIdx_iff_getElem?.mp member
    simp [item]
  have indices' : inputs.zipIdx.map Prod.snd = List.range inputs.length := by
    simpa only [List.range_eq_range'] using indices
  have left : (inputs.zipIdx.map Prod.snd).map (fun i => f (inputs[i]?.getD fallback)) =
      inputs.zipIdx.map (fun pair => f (inputs[pair.2]?.getD fallback)) := List.map_map
  have right : (inputs.zipIdx.map Prod.fst).map f = inputs.zipIdx.map (fun pair => f pair.1) := List.map_map
  rw [indices'] at left
  rw [entries] at right
  exact left.trans (same.trans right.symm)

theorem result_input_map {inputs : List Input} {jobs : Nat}
    {callback : Input → Bool → ItemOutcome Value Error Control}
    {fallback : ItemOutcome Value Error Control} {s : State Value Error Control}
    (reachable : Reachable inputs.length jobs (indexed inputs callback fallback) s)
    (returns : Returnable inputs.length s)
    (notExternal : ∀ error, s.stop ≠ .external error)
    (f : Input → Value) (normal : ∀ input ∈ inputs, callback input false = .value (f input)) :
    result inputs.length s = .values (inputs.map f) := by
  cases inputs with
  | nil =>
      have opened := no_signal_open reachable notExternal (by simp)
      simp [result, opened, select, values]
  | cons first rest =>
      let inputs := first :: rest
      let mapped : Nat → Value := fun i => f (inputs[i]?.getD first)
      have agrees : ∀ i, i < inputs.length → indexed inputs callback fallback i false = .value (mapped i) := by
        intro i inside
        have item : inputs[i]? = some inputs[i] := List.getElem?_eq_getElem inside
        have member : inputs[i] ∈ inputs := List.getElem_mem inside
        simpa [indexed, mapped, item] using normal inputs[i] member
      have opened := no_signal_open reachable notExternal (fun i inside => ⟨mapped i, agrees i inside⟩)
      have ordered := result_open reachable returns opened mapped agrees
      rw [mapped_range inputs first f] at ordered
      exact ordered


inductive Returns (n : Nat) : State Value Error Control → Result Value Error Control → Prop where
  | done (s : State Value Error Control) : Returnable n s → Returns n s (result n s)


inductive Following (n jobs : Nat) (callback : Nat → Bool → ItemOutcome Value Error Control)
    (start : State Value Error Control) : State Value Error Control → Prop where
  | refl : Following n jobs callback start start
  | step {before after} : Following n jobs callback start before → Step n jobs callback before after →
      Following n jobs callback start after

theorem finished_monotone {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {before after : State Value Error Control} (step : Step n jobs callback before after) :
    before.finished ⊆ after.finished := by
  cases step <;> simp_all [List.subset_def]

theorem external_step {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {before after : State Value Error Control} {reason : Error}
    (external : before.stop = .external reason) (step : Step n jobs callback before after) :
    after.stop = .external reason ∧
    ∀ i ∈ before.running, i ∈ after.running ∨
      ∃ c ∈ after.finished, c.index = i ∧ c.stopped = true := by
  cases step with
  | start i opened _ _ _ => simp [external] at opened
  | complete i running allowed => exact False.elim (allowed reason external)
  | stopItem i running stopped =>
      refine ⟨external, ?_⟩
      intro j member
      by_cases same : j = i
      · subst j
        exact Or.inr ⟨⟨i, true, callback i true⟩, by simp, rfl, rfl⟩
      · exact Or.inl ((List.mem_erase_of_ne same).2 member)
  | giveUp other allowed => exact False.elim (allowed reason external)

theorem external_following {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {start last : State Value Error Control} {reason : Error}
    (external : start.stop = .external reason) (follow : Following n jobs callback start last) :
    last.stop = .external reason ∧
      ∀ i ∈ start.running, i ∈ last.running ∨
        ∃ c ∈ last.finished, c.index = i ∧ c.stopped = true := by
  induction follow with
  | refl => exact ⟨external, fun i running => Or.inl running⟩
  | step prior step ih =>
      have advanced := external_step ih.1 step
      refine ⟨advanced.1, ?_⟩
      intro i member
      rcases ih.2 i member with running | ⟨c, found, index, stopped⟩
      · exact advanced.2 i running
      · exact Or.inr ⟨c, finished_monotone step found, index, stopped⟩

theorem external_drain {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {start last : State Value Error Control} {reason : Error} {output : Result Value Error Control}
    (external : start.stop = .external reason) (follow : Following n jobs callback start last)
    (returned : Returns n last output) :
    output = .external reason ∧ ∀ i ∈ start.running,
      ∃ c ∈ last.finished, c.index = i ∧ c.stopped = true := by
  have trace := external_following external follow
  cases returned with
  | done returnable =>
      refine ⟨external_result trace.1, ?_⟩
      intro i member
      rcases trace.2 i member with running | stopped
      · rw [returnable.1] at running
        simp at running
      · exact stopped


theorem no_start_after_stop {n jobs : Nat} {callback : Nat → Bool → ItemOutcome Value Error Control}
    {before after : State Value Error Control} (step : Step n jobs callback before after)
    (stopped : before.stop ≠ .open) : after.started = before.started := by
  cases step with
  | start i opened _ _ _ => exact False.elim (stopped opened)
  | complete i _ _ => rfl
  | stopItem i _ _ => rfl
  | giveUp reason _ => rfl

theorem par_bound (inputs : List Input)
    (callback : Input → Bool → ItemOutcome Value Error Control)
    (fallback : ItemOutcome Value Error Control) (jobs : Nat) (_positive : 1 ≤ jobs) :
    (∀ s, Reachable inputs.length jobs (indexed inputs callback fallback) s → s.running.length ≤ jobs) ∧
    (∀ s output, Reachable inputs.length jobs (indexed inputs callback fallback) s →
      Returns inputs.length s output →
      (∀ i ∈ s.started, ∃ c ∈ s.finished, c.index = i) ∧ s.running = []) ∧
    (∀ s output, Reachable inputs.length jobs (indexed inputs callback fallback) s →
      Returns inputs.length s output → (∀ error, s.stop ≠ .external error) →
      ∀ f : Input → Value, (∀ input ∈ inputs, callback input false = .value (f input)) →
      output = .values (inputs.map f)) ∧
    (∀ s output, Reachable inputs.length jobs (indexed inputs callback fallback) s →
      Returns inputs.length s output → (∀ error, s.stop ≠ .external error) → s.stop ≠ .open →
      ∃ i c, i < inputs.length ∧ output = .decided c ∧ c ∈ s.finished ∧ c.index = i ∧ c.stopped = false ∧
        c.outcome = indexed inputs callback fallback i false ∧
        ((∃ error, c.outcome = .failure error) ∨ (∃ control, c.outcome = .leave control)) ∧
        ∀ j, j < i → candidate s j = false) ∧
    (∀ (s : State Value Error Control) output reason, Returns inputs.length s output → s.stop = .external reason →
      output = .external reason) ∧
    (∀ (start last : State Value Error Control) output reason,
      start.stop = .external reason → Following inputs.length jobs (indexed inputs callback fallback) start last →
      Returns inputs.length last output → output = .external reason ∧
        ∀ i ∈ start.running, ∃ c ∈ last.finished, c.index = i ∧ c.stopped = true) ∧
    (∀ before after, Step inputs.length jobs (indexed inputs callback fallback) before after →
      before.stop ≠ .open → after.started = before.started) := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · intro s reachable
    exact bound reachable
  · intro s output reachable returned
    cases returned with
    | done returns => exact ⟨drain reachable returns, returns.1⟩
  · intro s output reachable returned notExternal f normal
    cases returned with
    | done returns => exact result_input_map reachable returns notExternal f normal
  · intro s output reachable returned notExternal notOpen
    cases returned with
    | done returns =>
        obtain ⟨i, c, resultEq, member, index, stopped, abnormal, earlier⟩ :=
          nonexternal_nonopen_selection reachable notExternal notOpen
        have correct := finishedCorrect reachable c member
        rw [stopped, index] at correct
        have inside : i < inputs.length := by simpa [index] using finishedWithin reachable c member
        exact ⟨i, c, inside, resultEq, member, index, stopped, correct, abnormal, earlier⟩
  · intro s output reason returned external
    cases returned with
    | done _ => exact external_result external
  · intro start last output reason external following returned
    exact external_drain external following returned
  · intro before after step stopped
    exact no_start_after_stop step stopped


namespace Witness

abbrev Outcome := ItemOutcome Nat String Unit
abbrev S := State Nat String Unit

def start (s : S) (i : Nat) : S :=
  {s with running := i :: s.running, started := i :: s.started}

def finish (callback : Nat → Bool → Outcome) (s : S) (i : Nat) : S :=
  {s with running := s.running.erase i, finished := ⟨i, false, callback i false⟩ :: s.finished, stop := nextStop s.stop (callback i false)}

def interrupt (callback : Nat → Bool → Outcome) (s : S) (i : Nat) : S :=
  {s with running := s.running.erase i, finished := ⟨i, true, callback i true⟩ :: s.finished}

def scale (item : Nat) (_ : Bool) : Outcome := .value (item * 10)
def scaleAt := indexed [3, 1, 2] scale (.value 0)

def p0 : S := initial
def p1 := start p0 0
def p2 := start p1 1
def p3 := finish scaleAt p2 1
def p4 := start p3 2
def p5 := finish scaleAt p4 2
def p6 := finish scaleAt p5 0

theorem parallel_trace : Reachable 3 2 scaleAt p6 := by
  apply Reachable.step (before := p5)
  · apply Reachable.step (before := p4)
    · apply Reachable.step (before := p3)
      · apply Reachable.step (before := p2)
        · apply Reachable.step (before := p1)
          · apply Reachable.step (before := p0)
            · exact Reachable.initial
            · exact Step.start _ 0 (by decide) (by decide) (by decide) (by decide)
          · exact Step.start _ 1 (by decide) (by decide) (by decide) (by decide)
        · exact Step.complete _ 1 (by decide) (by intro reason h; cases h)
      · exact Step.start _ 2 (by decide) (by decide) (by decide) (by decide)
    · exact Step.complete _ 2 (by decide) (by intro reason h; cases h)
  · exact Step.complete _ 0 (by decide) (by intro reason h; cases h)

def q0 : S := initial
def q1 := start q0 0
def q2 := finish scaleAt q1 0
def q3 := start q2 1
def q4 := finish scaleAt q3 1
def q5 := start q4 2
def q6 := finish scaleAt q5 2

theorem serial_trace : Reachable 3 1 scaleAt q6 := by
  apply Reachable.step (before := q5)
  · apply Reachable.step (before := q4)
    · apply Reachable.step (before := q3)
      · apply Reachable.step (before := q2)
        · apply Reachable.step (before := q1)
          · apply Reachable.step (before := q0)
            · exact Reachable.initial
            · exact Step.start _ 0 (by decide) (by decide) (by decide) (by decide)
          · exact Step.complete _ 0 (by decide) (by intro reason h; cases h)
        · exact Step.start _ 1 (by decide) (by decide) (by decide) (by decide)
      · exact Step.complete _ 1 (by decide) (by intro reason h; cases h)
    · exact Step.start _ 2 (by decide) (by decide) (by decide) (by decide)
  · exact Step.complete _ 2 (by decide) (by intro reason h; cases h)

theorem l1_returned_values :
    result 3 p6 = .values [30, 10, 20] ∧ result 3 q6 = .values [30, 10, 20] ∧
    ([30, 10, 20] : List Nat).take 2 = [30, 10] := by decide +kernel

def uneven (item : Nat) : Nat := (item * 10000 - 1) * (item * 10000) / 2
def unevenOutcome (item : Nat) (_ : Bool) : Outcome := .value (uneven item)
def unevenAt := indexed [20, 1, 3, 2] unevenOutcome (.value 0)
def u0 : S := initial
def u1 := start u0 0
def u2 := start u1 1
def u3 := finish unevenAt u2 1
def u4 := start u3 2
def u5 := finish unevenAt u4 2
def u6 := start u5 3
def u7 := finish unevenAt u6 3
def u8 := finish unevenAt u7 0

theorem claim_order_trace : Reachable 4 2 unevenAt u8 := by
  apply Reachable.step (before := u7)
  · apply Reachable.step (before := u6)
    · apply Reachable.step (before := u5)
      · apply Reachable.step (before := u4)
        · apply Reachable.step (before := u3)
          · apply Reachable.step (before := u2)
            · apply Reachable.step (before := u1)
              · apply Reachable.step (before := u0)
                · exact Reachable.initial
                · exact Step.start _ 0 (by decide) (by decide) (by decide) (by decide)
              · exact Step.start _ 1 (by decide) (by decide) (by decide) (by decide)
            · exact Step.complete _ 1 (by decide) (by intro reason h; cases h)
          · exact Step.start _ 2 (by decide) (by decide) (by decide) (by decide)
        · exact Step.complete _ 2 (by decide) (by intro reason h; cases h)
      · exact Step.start _ 3 (by decide) (by decide) (by decide) (by decide)
    · exact Step.complete _ 3 (by decide) (by intro reason h; cases h)
  · exact Step.complete _ 0 (by decide) (by intro reason h; cases h)

theorem claim_order_values :
    result 4 u8 = .values [19999900000, 49995000, 449985000, 199990000] := by decide +kernel

def failLater (item : Nat) (stopped : Bool) : Outcome :=
  if stopped then .failure "stopped"
  else if item == 1 then .failure "later item" else .value (uneven item)
def failAt := indexed [20, 1, 2, 3] failLater (.value 0)
def f0 : S := initial
def f1 := start f0 0
def f2 := start f1 1
def f3 := finish failAt f2 1
def f4 := interrupt failAt f3 0

theorem claim_failure_trace : Reachable 4 2 failAt f4 := by
  apply Reachable.step (before := f3)
  · apply Reachable.step (before := f2)
    · apply Reachable.step (before := f1)
      · apply Reachable.step (before := f0)
        · exact Reachable.initial
        · exact Step.start _ 0 (by decide) (by decide) (by decide) (by decide)
      · exact Step.start _ 1 (by decide) (by decide) (by decide) (by decide)
    · exact Step.complete _ 1 (by decide) (by intro reason h; cases h)
  · apply Step.stopItem _ 0 (by decide)
    left
    decide

theorem claim_failure_selected :
    result 4 f4 = .decided ⟨1, false, .failure "later item"⟩ ∧
    candidate f4 0 = false := by decide +kernel


theorem l1_returnable : Returnable 3 p6 ∧ Returnable 3 q6 := by
  constructor <;> constructor
  · rfl
  · right
    intro i inside
    simp [p6, p5, p4, p3, p2, p1, p0, finish, start, initial]
    omega
  · rfl
  · right
    intro i inside
    simp [q6, q5, q4, q3, q2, q1, q0, finish, start, initial]
    omega

theorem claim_returnable : Returnable 4 u8 ∧ Returnable 4 f4 := by
  constructor <;> constructor
  · rfl
  · right
    intro i inside
    simp [u8, u7, u6, u5, u4, u3, u2, u1, u0, finish, start, initial]
    omega
  · rfl
  · left
    decide

def bothFail (i : Nat) (_ : Bool) : Outcome :=
  if i == 0 then .failure "first" else .failure "second"
def b0 : S := initial
def b1 := start b0 0
def b2 := start b1 1
def b3 := finish bothFail b2 1
def b4 := finish bothFail b3 0

theorem independent_failures_trace : Reachable 2 2 bothFail b4 := by
  apply Reachable.step (before := b3)
  · apply Reachable.step (before := b2)
    · apply Reachable.step (before := b1)
      · apply Reachable.step (before := b0)
        · exact Reachable.initial
        · exact Step.start _ 0 (by decide) (by decide) (by decide) (by decide)
      · exact Step.start _ 1 (by decide) (by decide) (by decide) (by decide)
    · exact Step.complete _ 1 (by decide) (by intro reason h; cases h)
  · exact Step.complete _ 0 (by decide) (by intro reason h; cases h)

theorem independent_failures_selected :
    result 2 b4 = .decided ⟨0, false, .failure "first"⟩ ∧
    result 4 f4 ≠ .decided ⟨0, true, .failure "stopped"⟩ := by decide +kernel

def externalState : S := {f2 with stop := .external "deadline"}
def externalDrained := interrupt failAt (interrupt failAt externalState 0) 1

theorem external_trace : Reachable 4 2 failAt externalDrained := by
  apply Reachable.step (before := interrupt failAt externalState 0)
  · apply Reachable.step (before := externalState)
    · apply Reachable.step (before := f2)
      · apply Reachable.step (before := f1)
        · apply Reachable.step (before := f0)
          · exact Reachable.initial
          · exact Step.start _ 0 (by decide) (by decide) (by decide) (by decide)
        · exact Step.start _ 1 (by decide) (by decide) (by decide) (by decide)
      · exact Step.giveUp _ "deadline" (by intro previous h; cases h)
    · apply Step.stopItem _ 0 (by decide)
      right
      exact ⟨"deadline", rfl⟩
  · apply Step.stopItem _ 1 (by decide)
    right
    exact ⟨"deadline", rfl⟩

theorem external_selected : result 4 externalDrained = .external "deadline" := by decide +kernel

end Witness

end Xsh.Proofs.Core.ParMap
