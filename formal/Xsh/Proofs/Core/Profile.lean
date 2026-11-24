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


def StackValid : Nat → List Frame → Prop
  | _, [] => True
  | now, frame :: rest =>
    frame.start + frame.childNs + frame.blockedNs ≤ now ∧ StackValid frame.start rest

def ValidEvent (state : State) : Event Obs → Prop
  | .opaqueBlocked wall =>
    match state.stack with
    | [] => True
    | frame :: _ => frame.start + frame.childNs + frame.blockedNs + wall ≤ state.clock
  | _ => True

theorem stack_valid_advance {before after : Nat} {stack : List Frame}
    (valid : StackValid before stack) (later : before ≤ after) : StackValid after stack := by
  cases stack with
  | nil => trivial
  | cons frame rest =>
    simp only [StackValid] at valid ⊢
    exact ⟨Nat.le_trans valid.1 later, valid.2⟩

theorem event_preserves_stack_valid (state : State) (event : Event Obs)
    (valid : StackValid state.clock state.stack) (admitted : ValidEvent state event) :
    StackValid (profileEvent state event).clock (profileEvent state event).stack := by
  cases event with
  | sem _ | opaqueOverlapping _ => exact valid
  | tick amount =>
    simp only [profileEvent]
    exact stack_valid_advance valid (Nat.le_add_right _ _)
  | enter site =>
    simp only [profileEvent, enter, StackValid]
    exact ⟨by simp, valid⟩
  | exit =>
    cases h : state.stack with
    | nil => simp [profileEvent, exit, h, StackValid]
    | cons frame rest =>
      cases rest with
      | nil => simp [profileEvent, exit, h, StackValid]
      | cons parent tail =>
        simp only [h, StackValid] at valid
        simp only [profileEvent, exit, h, StackValid]
        constructor
        · have before := valid.1
          have parentBefore := valid.2.1
          omega
        · exact valid.2.2
  | opaqueBlocked wall =>
    cases h : state.stack with
    | nil => simp [profileEvent, opaqueBlocked, h, StackValid]
    | cons frame rest =>
      simp only [h, StackValid] at valid
      simp only [ValidEvent, h] at admitted
      simp only [profileEvent, opaqueBlocked, h, StackValid]
      exact ⟨by omega, valid.2⟩
  | mergeWorker dropped openFrames paths => exact valid
  | finish => exact valid

inductive ValidTrace : State → List (Event Obs) → Prop where
  | nil (state) : ValidTrace state []
  | cons (state) (event) (rest) : ValidEvent state event →
      ValidTrace (profileEvent state event) rest → ValidTrace state (event :: rest)

theorem trace_preserves_stack_valid (state : State) (events : List (Event Obs))
    (valid : StackValid state.clock state.stack) (trace : ValidTrace state events) :
    StackValid (runProfile state events).clock (runProfile state events).stack := by
  induction trace with
  | nil => exact valid
  | cons state event rest admitted trace ih =>
    exact ih (event_preserves_stack_valid state event valid admitted)

def elapsed (now : Nat) (frame : Frame) : Nat := now - frame.start

def active (now : Nat) (frame : Frame) : Nat := elapsed now frame - frame.childNs - frame.blockedNs

theorem frame_accounting (now : Nat) (frame : Frame)
    (valid : frame.start + frame.childNs + frame.blockedNs ≤ now) :
    frame.childNs ≤ elapsed now frame ∧ frame.blockedNs ≤ elapsed now frame - frame.childNs ∧
    elapsed now frame = active now frame + frame.blockedNs + frame.childNs := by
  simp only [elapsed, active]
  omega

end Xsh.Proofs.Core.Profile
