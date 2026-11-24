import Std

namespace Xsh.Proofs.Core.Profile

abbrev Site := Nat
abbrev Path := List (Site × Bool)

structure PathStats where
  calls : Nat := 0
  inclusiveNs : Nat := 0
  exclusiveNs : Nat := 0
  blockedNs : Nat := 0
  deriving Repr, DecidableEq

structure Frame where
  site : Site
  path : Path
  start : Nat
  childNs : Nat := 0
  blockedNs : Nat := 0
  childBlockedNs : Nat := 0
  deriving Repr, DecidableEq

structure State where
  clock : Nat := 0
  stack : List Frame := []
  paths : Path → PathStats := fun _ => {}
  dropped : Nat := 0

def updatePath (paths : Path → PathStats) (path : Path) (f : PathStats → PathStats) : Path → PathStats :=
  fun p => if p = path then f (paths p) else paths p

def parentPath (stack : List Frame) : Path :=
  match stack with
  | [] => []
  | frame :: _ => frame.path

def enter (state : State) (site : Site) : State :=
  let path := (site, false) :: parentPath state.stack
  let paths := updatePath state.paths path fun p => {p with calls := p.calls + 1}
  {state with stack := ⟨site, path, state.clock, 0, 0, 0⟩ :: state.stack, paths := paths}

def exit (state : State) : State :=
  match state.stack with
  | [] => state
  | frame :: rest =>
    let elapsed := state.clock - frame.start
    let active := elapsed - frame.childNs - frame.blockedNs
    let paths := updatePath state.paths frame.path fun p =>
      {p with inclusiveNs := p.inclusiveNs + elapsed, exclusiveNs := p.exclusiveNs + active, blockedNs := p.blockedNs + frame.blockedNs}
    let stack := match rest with
      | [] => []
      | parent :: tail =>
        {parent with childNs := parent.childNs + elapsed, childBlockedNs := parent.childBlockedNs + frame.blockedNs + frame.childBlockedNs} :: tail
    {state with stack := stack, paths := paths}

def opaqueBlocked (state : State) (wall : Nat) : State :=
  match state.stack with
  | [] => state
  | frame :: rest => {state with stack := {frame with blockedNs := frame.blockedNs + wall} :: rest}

def addStats (a b : PathStats) : PathStats :=
  ⟨a.calls + b.calls, a.inclusiveNs + b.inclusiveNs,
    a.exclusiveNs + b.exclusiveNs, a.blockedNs + b.blockedNs⟩

def workerPath (root : Path) (path : Path) : Path :=
  path.map (fun entry => (entry.1, true)) ++ root

def mergeWorker (state : State) (dropped openFrames : Nat) (paths : List (Path × PathStats)) : State :=
  let root := parentPath state.stack
  let merged := paths.foldl (fun acc (path, stats) =>
    updatePath acc (workerPath root path) (fun old => addStats old stats)) state.paths
  {state with paths := merged, dropped := state.dropped + dropped + openFrames}

def finish (state : State) : State :=
  {state with dropped := state.dropped + state.stack.length}

inductive Event (Obs : Type) where
  | sem : Obs → Event Obs
  | tick : Nat → Event Obs
  | enter : Site → Event Obs
  | exit : Event Obs
  | opaqueBlocked : Nat → Event Obs
  | opaqueOverlapping : Nat → Event Obs
  | mergeWorker : Nat → Nat → List (Path × PathStats) → Event Obs
  | finish : Event Obs

def profileEvent (state : State) : Event Obs → State
  | .sem _ => state
  | .tick amount => {state with clock := state.clock + amount}
  | .enter site => enter state site
  | .exit => exit state
  | .opaqueBlocked wall => opaqueBlocked state wall
  | .opaqueOverlapping _ => state
  | .mergeWorker dropped openFrames paths => mergeWorker state dropped openFrames paths
  | .finish => finish state

def run (step : S → Obs → S) : S × State → List (Event Obs) → S × State
  | state, [] => state
  | (semantic, profile), event :: rest =>
    let semantic := match event with
      | .sem obs => step semantic obs
      | _ => semantic
    run step (semantic, profileEvent profile event) rest

def erase : List (Event Obs) → List Obs
  | [] => []
  | .sem obs :: rest => obs :: erase rest
  | _ :: rest => erase rest

def runErased (step : S → Obs → S) : S → List Obs → S
  | state, [] => state
  | state, obs :: rest => runErased step (step state obs) rest

theorem profile_erasure (step : S → Obs → S) (events : List (Event Obs))
    (semantic : S) (profile : State) :
    (run step (semantic, profile) events).1 = runErased step semantic (erase events) := by
  induction events generalizing semantic profile with
  | nil => rfl
  | cons event events ih =>
    cases event <;> simp [run, erase, runErased, ih]


def runProfile (state : State) : List (Event Obs) → State
  | [] => state
  | event :: rest => runProfile (profileEvent state event) rest

theorem run_profile_projection (step : S → Obs → S) (events : List (Event Obs))
    (semantic : S) (profile : State) :
    (run step (semantic, profile) events).2 = runProfile profile events := by
  induction events generalizing semantic profile with
  | nil => rfl
  | cons event events ih => cases event <;> simp [run, runProfile, ih]

theorem event_dropped_monotone (state : State) (event : Event Obs) :
    state.dropped ≤ (profileEvent state event).dropped := by
  cases event with
  | sem _ | tick _ | opaqueOverlapping _ => simp [profileEvent]
  | enter site => simp [profileEvent, enter]
  | exit =>
    cases h : state.stack with
    | nil => simp [profileEvent, exit, h]
    | cons frame rest =>
      cases rest <;> simp [profileEvent, exit, h]
  | opaqueBlocked wall =>
    cases h : state.stack <;> simp [profileEvent, opaqueBlocked, h]
  | mergeWorker dropped openFrames paths => simp [profileEvent, mergeWorker]; omega
  | finish => simp [profileEvent, finish]

theorem run_dropped_monotone (state : State) (events : List (Event Obs)) :
    state.dropped ≤ (runProfile state events).dropped := by
  induction events generalizing state with
  | nil => simp [runProfile]
  | cons event rest ih =>
    exact Nat.le_trans (event_dropped_monotone state event) (ih (profileEvent state event))

def complete (state : State) : Prop := state.dropped = 0

theorem profile_incomplete_reported (state : State) (events : List (Event Obs)) :
    state.dropped ≤ (runProfile state events).dropped ∧
    (state.stack ≠ [] → ¬ complete (runProfile (finish state) events)) ∧
    (∀ dropped openFrames paths, dropped ≠ 0 ∨ openFrames ≠ 0 →
      ¬ complete (runProfile (mergeWorker state dropped openFrames paths) events)) ∧
    (∀ dropped openFrames paths, complete (runProfile (mergeWorker state dropped openFrames paths) events) →
      dropped = 0 ∧ openFrames = 0) ∧
    (complete (runProfile (finish state) events) → state.stack = []) := by
  refine ⟨run_dropped_monotone state events, ?_, ?_, ?_, ?_⟩
  · intro opened done
    have h := run_dropped_monotone (finish state) events
    have positive : 0 < state.stack.length := List.length_pos_iff.mpr opened
    simp only [complete, finish] at done
    simp only [finish] at h
    omega
  · intro dropped openFrames paths missing done
    have h := run_dropped_monotone (mergeWorker state dropped openFrames paths) events
    simp only [complete, mergeWorker] at done
    simp only [mergeWorker] at h
    omega
  · intro dropped openFrames paths done
    have h := run_dropped_monotone (mergeWorker state dropped openFrames paths) events
    simp only [complete, mergeWorker] at done
    simp only [mergeWorker] at h
    omega
  · intro done
    have h := run_dropped_monotone (finish state) events
    simp only [complete, finish] at done
    simp only [finish] at h
    have lengthZero : state.stack.length = 0 := by omega
    exact List.length_eq_zero_iff.mp lengthZero

end Xsh.Proofs.Core.Profile
