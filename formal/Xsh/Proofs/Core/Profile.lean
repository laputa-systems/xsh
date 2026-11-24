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
  childElapsed : List Nat := []
  deriving Repr, DecidableEq

structure Receipt where
  path : Path
  elapsed : Nat
  active : Nat
  blocked : Nat
  deriving Repr, DecidableEq

structure State where
  clock : Nat := 0
  stack : List Frame := []
  paths : Path → PathStats := fun _ => {}
  dropped : Nat := 0
  closed : List Receipt := []
  work : Path → Nat := fun _ => 0

def updatePath (paths : Path → PathStats) (path : Path) (f : PathStats → PathStats) : Path → PathStats :=
  fun p => if p = path then f (paths p) else paths p

def parentPath (stack : List Frame) : Path :=
  match stack with
  | [] => []
  | frame :: _ => frame.path

def tick (state : State) (duration : Nat) : State :=
  let work := match state.stack with
    | [] => state.work
    | frame :: _ => fun path => state.work path + if path = frame.path then duration else 0
  {state with clock := state.clock + duration, work := work}

def enter (state : State) (site : Site) : State :=
  let path := (site, false) :: parentPath state.stack
  let paths := updatePath state.paths path fun p => {p with calls := p.calls + 1}
  {state with stack := ⟨site, path, state.clock, 0, 0, 0, []⟩ :: state.stack, paths := paths}

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
        {parent with childNs := parent.childNs + elapsed, childBlockedNs := parent.childBlockedNs + frame.blockedNs + frame.childBlockedNs, childElapsed := elapsed :: parent.childElapsed} :: tail
    {state with stack := stack, paths := paths, closed := ⟨frame.path, elapsed, active, frame.blockedNs⟩ :: state.closed}

def opaqueBlocked (state : State) (wall : Nat) : State :=
  match state.stack with
  | [] => {state with clock := state.clock + wall}
  | frame :: rest => {state with clock := state.clock + wall, stack := {frame with blockedNs := frame.blockedNs + wall} :: rest}

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
  | .tick amount => tick state amount
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
  | sem _ | opaqueOverlapping _ => simp [profileEvent]
  | tick _ => simp [profileEvent, tick]
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

theorem incomplete_local (state : State) (events : List (Event Obs)) :
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
  | .opaqueBlocked _ => state.stack ≠ []
  | .mergeWorker _ _ paths => ∀ entry ∈ paths, entry.1 ≠ []
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
    simp only [profileEvent, tick]
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


def activeStack : Nat → List Frame → List Nat
  | _, [] => []
  | now, frame :: rest => active now frame :: activeStack frame.start rest


theorem exit_freezes_ancestors (state : State) (frame : Frame) (rest : List Frame)
    (stack : state.stack = frame :: rest) (valid : StackValid state.clock state.stack) :
    activeStack (exit state).clock (exit state).stack = activeStack frame.start rest := by
  rw [stack] at valid
  cases rest with
  | nil => simp [exit, stack, activeStack]
  | cons parent tail =>
    simp only [StackValid] at valid
    simp only [exit, stack, activeStack]
    congr 1
    simp only [active, elapsed]
    omega

theorem single_active_step (state : State) (frame : Frame) (rest : List Frame)
    (stack : state.stack = frame :: rest) (valid : StackValid state.clock state.stack)
    (duration wall : Nat) :
    activeStack (state.clock + duration) state.stack =
      (active state.clock frame + duration) :: activeStack frame.start rest ∧
    activeStack (opaqueBlocked state wall).clock (opaqueBlocked state wall).stack =
      active state.clock frame :: activeStack frame.start rest ∧
    active state.clock frame + frame.blockedNs + frame.childNs = elapsed state.clock frame ∧
    activeStack (exit state).clock (exit state).stack = activeStack frame.start rest ∧
    (∀ site, activeStack (enter state site).clock (enter state site).stack =
      0 :: activeStack state.clock state.stack) := by
  have bound : frame.start + frame.childNs + frame.blockedNs ≤ state.clock := by
    rw [stack] at valid
    exact valid.1
  constructor
  · simp only [stack, activeStack]
    congr 1
    simp only [active, elapsed]
    omega
  constructor
  · simp only [opaqueBlocked, stack, activeStack]
    congr 1
    simp only [active, elapsed]
    omega
  · refine ⟨(frame_accounting state.clock frame bound).2.2.symm, ?_, ?_⟩
    · apply exit_freezes_ancestors state frame rest stack
      simpa [stack] using valid
    · intro site
      simp [enter, activeStack, active, elapsed]


def StackPaths : List Frame → Prop
  | [] => True
  | frame :: rest => frame.path = (frame.site, false) :: parentPath rest ∧ StackPaths rest

theorem event_preserves_stack_paths (state : State) (event : Event Obs)
    (valid : StackPaths state.stack) : StackPaths (profileEvent state event).stack := by
  cases event with
  | sem _ | opaqueOverlapping _ | mergeWorker _ _ _ | finish => exact valid
  | tick amount => simpa [profileEvent, tick] using valid
  | enter site => simp [profileEvent, enter, StackPaths, valid]
  | exit =>
    cases h : state.stack with
    | nil => simp [profileEvent, exit, h, StackPaths]
    | cons frame rest =>
      cases rest with
      | nil => simp [profileEvent, exit, h, StackPaths]
      | cons parent tail =>
        simp only [h, StackPaths, parentPath] at valid
        simp only [profileEvent, exit, h, StackPaths, parentPath]
        exact valid.2
  | opaqueBlocked wall =>
    cases h : state.stack with
    | nil => simp [profileEvent, opaqueBlocked, h, StackPaths]
    | cons frame rest =>
      simp only [h, StackPaths] at valid
      simp only [profileEvent, opaqueBlocked, h, StackPaths]
      exact valid

def serialChild (parent path : Path) : Bool :=
  match path with
  | (_, false) :: rest => rest == parent
  | _ => false

def receiptInclusive (path : Path) : List Receipt → Nat
  | [] => 0
  | receipt :: rest => (if receipt.path = path then receipt.elapsed else 0) + receiptInclusive path rest

def receiptActive (path : Path) : List Receipt → Nat
  | [] => 0
  | receipt :: rest => (if receipt.path = path then receipt.active else 0) + receiptActive path rest

def receiptBlocked (path : Path) : List Receipt → Nat
  | [] => 0
  | receipt :: rest => (if receipt.path = path then receipt.blocked else 0) + receiptBlocked path rest

def receiptChildren (path : Path) : List Receipt → Nat
  | [] => 0
  | receipt :: rest => (if serialChild path receipt.path then receipt.elapsed else 0) + receiptChildren path rest

def openChildren (path : Path) : List Frame → Nat
  | [] => 0
  | frame :: rest => (if frame.path = path then frame.childNs else 0) + openChildren path rest

def LedgerValid (state : State) : Prop :=
  ∀ path, path ≠ [] → receiptInclusive path state.closed + openChildren path state.stack =
    receiptActive path state.closed + receiptBlocked path state.closed + receiptChildren path state.closed


theorem event_preserves_ledger (state : State) (event : Event Obs)
    (timed : StackValid state.clock state.stack) (paths : StackPaths state.stack)
    (ledger : LedgerValid state) : LedgerValid (profileEvent state event) := by
  intro path nonempty
  have previous := ledger path nonempty
  cases event with
  | sem _ | opaqueOverlapping _ | mergeWorker _ _ _ | finish => exact previous
  | tick amount => simpa [profileEvent, tick] using previous
  | enter site =>
    simpa [profileEvent, enter, openChildren] using previous
  | opaqueBlocked wall =>
    cases h : state.stack with
    | nil => simpa [profileEvent, opaqueBlocked, h] using previous
    | cons frame rest =>
      simp only [profileEvent, opaqueBlocked, h, openChildren]
      simpa [h, openChildren] using previous
  | exit =>
    cases h : state.stack with
    | nil => simpa [profileEvent, exit, h] using previous
    | cons frame rest =>
      have bound : frame.start + frame.childNs + frame.blockedNs ≤ state.clock := by
        rw [h] at timed
        exact timed.1
      have accounting := (frame_accounting state.clock frame bound).2.2
      have key : frame.path = (frame.site, false) :: parentPath rest := by
        rw [h] at paths
        exact paths.1
      cases rest with
      | nil =>
        simp only [parentPath] at key
        simp only [h, openChildren] at previous
        simp only [profileEvent, exit, h, receiptInclusive, receiptActive, receiptBlocked,
          receiptChildren, openChildren, key, serialChild] at *
        simp only [beq_iff_eq] at *
        have rootNe : [] ≠ path := Ne.symm nonempty
        by_cases same : [(frame.site, false)] = path <;>
          simp only [same, rootNe, ↓reduceIte] at previous ⊢ <;>
          simp only [elapsed, active] at accounting <;> omega
      | cons parent tail =>
        simp only [parentPath] at key
        simp only [h, openChildren] at previous
        simp only [profileEvent, exit, h, receiptInclusive, receiptActive, receiptBlocked,
          receiptChildren, openChildren, key, serialChild] at *
        simp only [beq_iff_eq] at *
        have noSelf : ∀ xs : Path, (frame.site, false) :: xs ≠ xs := by
          intro xs equal
          have lengths := congrArg List.length equal
          simp at lengths
        by_cases same : (frame.site, false) :: parent.path = path <;>
          by_cases parentSame : parent.path = path <;>
          simp only [same, parentSame, noSelf, ↓reduceIte] at previous ⊢ <;>
          simp only [elapsed, active] at accounting <;> omega


def initial : State := {}

theorem trace_ledger (state : State) (events : List (Event Obs))
    (timed : StackValid state.clock state.stack) (paths : StackPaths state.stack)
    (ledger : LedgerValid state) (trace : ValidTrace state events) :
    StackValid (runProfile state events).clock (runProfile state events).stack ∧
    StackPaths (runProfile state events).stack ∧ LedgerValid (runProfile state events) := by
  induction trace with
  | nil => exact ⟨timed, paths, ledger⟩
  | cons state event rest admitted trace ih =>
    exact ih (event_preserves_stack_valid state event timed admitted)
      (event_preserves_stack_paths state event paths)
      (event_preserves_ledger state event timed paths ledger)

def SerialStats (state : State) : Prop :=
  ∀ site parent, let path := (site, false) :: parent
    (state.paths path).inclusiveNs = receiptInclusive path state.closed ∧
    (state.paths path).exclusiveNs = receiptActive path state.closed ∧
    (state.paths path).blockedNs = receiptBlocked path state.closed

theorem worker_path_ne_serial (root path : Path) (site : Site) (parent : Path)
    (nonempty : path ≠ []) : workerPath root path ≠ (site, false) :: parent := by
  cases path with
  | nil => contradiction
  | cons entry rest =>
    cases entry with
    | mk item flag => simp [workerPath]

theorem fold_worker_serial (root : Path) (entries : List (Path × PathStats))
    (valid : ∀ entry ∈ entries, entry.1 ≠ []) (acc : Path → PathStats)
    (site : Site) (parent : Path) :
    (entries.foldl (fun acc (path, stats) =>
      updatePath acc (workerPath root path) (fun old => addStats old stats)) acc)
      ((site, false) :: parent) = acc ((site, false) :: parent) := by
  induction entries generalizing acc with
  | nil => rfl
  | cons entry rest ih =>
    have good : entry.1 ≠ [] := valid entry (by simp)
    have tail : ∀ item ∈ rest, item.1 ≠ [] := by
      intro item member
      exact valid item (by simp [member])
    cases entry with
    | mk path stats =>
      simp only [List.foldl_cons]
      rw [ih tail]
      simp only [updatePath]
      have different := worker_path_ne_serial root path site parent good
      simp [Ne.symm different]

theorem event_preserves_serial_stats (state : State) (event : Event Obs)
    (admitted : ValidEvent state event) (valid : SerialStats state) :
    SerialStats (profileEvent state event) := by
  intro site parent
  have old := valid site parent
  cases event with
  | sem _ | opaqueOverlapping _ | finish => exact old
  | tick amount => simpa [profileEvent, tick] using old
  | enter entered =>
    simp only [profileEvent, enter, updatePath]
    split <;> simp_all
  | opaqueBlocked wall =>
    cases h : state.stack <;> simpa [profileEvent, opaqueBlocked, h] using old
  | mergeWorker dropped openFrames entries =>
    simp only [ValidEvent] at admitted
    simp only [profileEvent, mergeWorker]
    rw [fold_worker_serial (parentPath state.stack) entries admitted state.paths site parent]
    exact old
  | exit =>
    cases h : state.stack with
    | nil => simpa [profileEvent, exit, h] using old
    | cons frame rest =>
      cases rest with
      | nil =>
        simp only [profileEvent, exit, h, updatePath, receiptInclusive, receiptActive, receiptBlocked]
        by_cases same : (site, false) :: parent = frame.path
        · simp [same, Nat.add_comm]
          simpa [same] using old
        · have reverse : frame.path ≠ (site, false) :: parent := Ne.symm same
          simp [same, reverse, old]
      | cons previous tail =>
        simp only [profileEvent, exit, h, updatePath, receiptInclusive, receiptActive, receiptBlocked]
        by_cases same : (site, false) :: parent = frame.path
        · simp [same, Nat.add_comm]
          simpa [same] using old
        · have reverse : frame.path ≠ (site, false) :: parent := Ne.symm same
          simp [same, reverse, old]


theorem trace_serial_stats (state : State) (events : List (Event Obs))
    (valid : SerialStats state) (trace : ValidTrace state events) :
    SerialStats (runProfile state events) := by
  induction trace with
  | nil => exact valid
  | cons state event rest admitted trace ih =>
    exact ih (event_preserves_serial_stats state event admitted valid)

theorem initial_valid : StackValid initial.clock initial.stack ∧ StackPaths initial.stack ∧
    LedgerValid initial ∧ SerialStats initial := by
  simp [initial, StackValid, StackPaths, LedgerValid, SerialStats, receiptInclusive,
    receiptActive, receiptBlocked, receiptChildren, openChildren]

theorem exited_accounting (events : List (Event Obs)) (trace : ValidTrace initial events)
    (closed : (runProfile initial events).stack = []) (site : Site) (parent : Path) :
    let state := runProfile initial events
    let path := (site, false) :: parent
    (state.paths path).inclusiveNs = (state.paths path).exclusiveNs +
      (state.paths path).blockedNs + receiptChildren path state.closed := by
  have ledger := (trace_ledger initial events initial_valid.1 initial_valid.2.1 initial_valid.2.2.1 trace).2.2
  have totals := trace_serial_stats initial events initial_valid.2.2.2 trace site parent
  have balance := ledger ((site, false) :: parent) (by simp)
  simp only [closed, openChildren, Nat.add_zero] at balance
  simp only at totals ⊢
  omega


def sumSites (sites : List Site) (f : Site → Nat) : Nat :=
  match sites with
  | [] => 0
  | site :: rest => f site + sumSites rest f

@[simp] theorem sumSites_zero (sites : List Site) : sumSites sites (fun _ => 0) = 0 := by
  induction sites with
  | nil => rfl
  | cons site rest ih => simp [sumSites, ih]

theorem sumSites_add (sites : List Site) (f g : Site → Nat) :
    sumSites sites (fun site => f site + g site) = sumSites sites f + sumSites sites g := by
  induction sites with
  | nil => rfl
  | cons site rest ih => simp only [sumSites, ih]; omega

theorem sumSites_indicator (sites : List Site) (target amount : Nat)
    (unique : sites.Nodup) :
    sumSites sites (fun site => if target = site then amount else 0) =
      if target ∈ sites then amount else 0 := by
  induction sites with
  | nil => simp [sumSites]
  | cons site rest ih =>
    have parts := List.nodup_cons.mp unique
    by_cases equal : target = site
    · subst site
      simp [sumSites, ih parts.2, parts.1]
    · simp [sumSites, equal, ih parts.2]

def CoversChildren (path : Path) (receipts : List Receipt) (sites : List Site) : Prop :=
  ∀ receipt ∈ receipts, ∀ site, receipt.path = (site, false) :: path → site ∈ sites

theorem grouped_child_receipts (path : Path) (receipts : List Receipt) (sites : List Site)
    (unique : sites.Nodup) (covers : CoversChildren path receipts sites) :
    receiptChildren path receipts = sumSites sites (fun site => receiptInclusive ((site, false) :: path) receipts) := by
  induction receipts with
  | nil => simp [receiptChildren, receiptInclusive]
  | cons receipt rest ih =>
    have tail : CoversChildren path rest sites := by
      intro item member site eq
      exact covers item (by simp [member]) site eq
    simp only [receiptChildren, receiptInclusive, sumSites_add, ← ih tail]
    have one : (if serialChild path receipt.path then receipt.elapsed else 0) =
        sumSites sites (fun site => if receipt.path = (site, false) :: path then receipt.elapsed else 0) := by
      cases hp : receipt.path with
      | nil => simp [serialChild]
      | cons first remainder =>
        rcases first with ⟨firstSite, worker⟩
        cases worker with
        | true => simp [serialChild]
        | false =>
          by_cases parentEq : remainder = path
          · subst remainder
            have member : firstSite ∈ sites := covers receipt (by simp) firstSite hp
            simp [serialChild, sumSites_indicator sites firstSite receipt.elapsed unique, member]
          · simp [serialChild, parentEq]
    omega


def receiptSite (receipt : Receipt) : Site :=
  match receipt.path with
  | [] => 0
  | (site, _) :: _ => site

def siteBound : List Receipt → Nat
  | [] => 0
  | receipt :: rest => receiptSite receipt + 1 + siteBound rest

theorem receipt_site_below (receipts : List Receipt) (receipt : Receipt)
    (member : receipt ∈ receipts) : receiptSite receipt < siteBound receipts := by
  induction receipts with
  | nil => simp at member
  | cons first rest ih =>
    simp only [List.mem_cons] at member
    simp only [siteBound]
    rcases member with rfl | member
    · exact Nat.lt_add_right _ (Nat.lt_add_one _)
    · exact Nat.lt_add_left _ (ih member)

def childSites (receipts : List Receipt) : List Site := List.range (siteBound receipts)

theorem childSites_unique (receipts : List Receipt) : (childSites receipts).Nodup := by
  exact List.nodup_range

theorem childSites_cover (path : Path) (receipts : List Receipt) :
    CoversChildren path receipts (childSites receipts) := by
  intro receipt member site eq
  have bound := receipt_site_below receipts receipt member
  have siteEq : receiptSite receipt = site := by simp [receiptSite, eq]
  simp only [childSites, List.mem_range]
  rw [siteEq] at bound
  exact bound

theorem sumSites_congr (sites : List Site) (f g : Site → Nat) (equal : ∀ site, f site = g site) :
    sumSites sites f = sumSites sites g := by
  induction sites with
  | nil => rfl
  | cons site rest ih => simp [sumSites, equal, ih]


def FrameChildrenValid : List Frame → Prop
  | [] => True
  | frame :: rest => frame.childNs = frame.childElapsed.sum ∧ FrameChildrenValid rest

theorem event_preserves_children (state : State) (event : Event Obs)
    (valid : FrameChildrenValid state.stack) : FrameChildrenValid (profileEvent state event).stack := by
  cases event with
  | sem _ | opaqueOverlapping _ | mergeWorker _ _ _ | finish => exact valid
  | tick amount => simpa [profileEvent, tick] using valid
  | enter site => simp [profileEvent, enter, FrameChildrenValid, valid]
  | opaqueBlocked wall =>
    cases h : state.stack with
    | nil => simp [profileEvent, opaqueBlocked, h, FrameChildrenValid]
    | cons frame rest => simpa [profileEvent, opaqueBlocked, h, FrameChildrenValid] using valid
  | exit =>
    cases h : state.stack with
    | nil => simp [profileEvent, exit, h, FrameChildrenValid]
    | cons frame rest =>
      cases rest with
      | nil => simp [profileEvent, exit, h, FrameChildrenValid]
      | cons parent tail =>
        simp only [h, FrameChildrenValid] at valid
        simp only [profileEvent, exit, h, FrameChildrenValid, List.sum_cons]
        constructor
        · omega
        · exact valid.2.2

theorem trace_children (state : State) (events : List (Event Obs))
    (valid : FrameChildrenValid state.stack) (trace : ValidTrace state events) :
    FrameChildrenValid (runProfile state events).stack := by
  induction trace with
  | nil => exact valid
  | cons state event rest admitted trace ih =>
    exact ih (event_preserves_children state event valid)

theorem profile_accounting (events : List (Event Obs)) (trace : ValidTrace initial events) :
    (∀ frame rest, (runProfile initial events).stack = frame :: rest →
      frame.childNs ≤ elapsed (runProfile initial events).clock frame ∧
      frame.blockedNs ≤ elapsed (runProfile initial events).clock frame - frame.childNs ∧
      frame.childNs = frame.childElapsed.sum ∧
      elapsed (runProfile initial events).clock frame =
        active (runProfile initial events).clock frame + frame.blockedNs + frame.childElapsed.sum) ∧
    ((runProfile initial events).stack = [] → ∀ site parent,
      let state := runProfile initial events
      let path := (site, false) :: parent
      (state.paths path).inclusiveNs = (state.paths path).exclusiveNs + (state.paths path).blockedNs +
        sumSites (childSites state.closed) (fun child => (state.paths ((child, false) :: path)).inclusiveNs)) ∧
    (∀ state dropped openFrames entries, (∀ entry ∈ entries, entry.1 ≠ []) →
      (mergeWorker state dropped openFrames entries).stack = state.stack ∧
      ∀ site parent, (mergeWorker state dropped openFrames entries).paths ((site, false) :: parent) =
        state.paths ((site, false) :: parent)) ∧
    (∀ state wall, profileEvent state (.opaqueOverlapping wall : Event Obs) = state) := by
  refine ⟨?_, ?_, ?_, ?_⟩
  · intro frame rest stack
    have valid := trace_preserves_stack_valid initial events initial_valid.1 trace
    rw [stack] at valid
    have arithmetic := frame_accounting _ frame valid.1
    have children := trace_children initial events (by simp [initial, FrameChildrenValid]) trace
    rw [stack] at children
    exact ⟨arithmetic.1, arithmetic.2.1, children.1, by rw [← children.1]; exact arithmetic.2.2⟩
  · intro closed site parent
    have accounted := exited_accounting events trace closed site parent
    have stats := trace_serial_stats initial events initial_valid.2.2.2 trace
    let state := runProfile initial events
    let path := (site, false) :: parent
    have grouped := grouped_child_receipts path state.closed (childSites state.closed)
      (childSites_unique state.closed) (childSites_cover path state.closed)
    have equal : sumSites (childSites state.closed) (fun child => receiptInclusive ((child, false) :: path) state.closed) =
        sumSites (childSites state.closed) (fun child => (state.paths ((child, false) :: path)).inclusiveNs) := by
      apply sumSites_congr
      intro child
      exact (stats child path).1.symm
    change (state.paths path).inclusiveNs = (state.paths path).exclusiveNs + (state.paths path).blockedNs +
      sumSites (childSites state.closed) (fun child => (state.paths ((child, false) :: path)).inclusiveNs)
    rw [← equal, ← grouped]
    exact accounted
  · intro state dropped openFrames entries admitted
    constructor
    · rfl
    · intro site parent
      exact fold_worker_serial (parentPath state.stack) entries admitted state.paths site parent
  · intros
    rfl


def credited (state : State) (event : Event Obs) (path : Path) : Nat :=
  match event, state.stack with
  | .tick duration, frame :: _ => if path = frame.path then duration else 0
  | _, _ => 0

theorem event_work (state : State) (event : Event Obs) (path : Path) :
    (profileEvent state event).work path = state.work path + credited state event path := by
  cases event with
  | sem _ | opaqueOverlapping _ | finish => simp [profileEvent, credited, finish]
  | tick duration =>
    cases h : state.stack <;> simp [profileEvent, tick, credited, h]
  | enter site => simp [profileEvent, enter, credited]
  | exit =>
    cases h : state.stack with
    | nil => simp [profileEvent, exit, credited, h]
    | cons frame rest =>
      cases rest <;> simp [profileEvent, exit, credited, h]
  | opaqueBlocked wall =>
    cases h : state.stack <;> simp [profileEvent, opaqueBlocked, credited, h]
  | mergeWorker dropped openFrames entries => simp [profileEvent, mergeWorker, credited]

def intervalWork (state : State) (events : List (Event Obs)) (path : Path) : Nat :=
  match events with
  | [] => 0
  | event :: rest => credited state event path + intervalWork (profileEvent state event) rest path

theorem interval_work (state : State) (events : List (Event Obs)) (path : Path) :
    (runProfile state events).work path = state.work path + intervalWork state events path := by
  induction events generalizing state with
  | nil => simp [runProfile, intervalWork]
  | cons event rest ih =>
    simp only [runProfile, intervalWork, ih, event_work]
    omega


def openActive (path : Path) : Nat → List Frame → Nat
  | _, [] => 0
  | now, frame :: rest =>
    (if frame.path = path then active now frame else 0) + openActive path frame.start rest

def WorkValid (state : State) : Prop :=
  ∀ site parent, let path := (site, false) :: parent
    state.work path = (state.paths path).exclusiveNs + openActive path state.clock state.stack

theorem openActive_exit (state : State) (frame : Frame) (rest : List Frame) (path : Path)
    (stack : state.stack = frame :: rest) (valid : StackValid state.clock state.stack) :
    openActive path (exit state).clock (exit state).stack = openActive path frame.start rest := by
  rw [stack] at valid
  cases rest with
  | nil => simp [exit, stack, openActive]
  | cons parent tail =>
    simp only [StackValid] at valid
    simp only [exit, stack, openActive]
    by_cases same : parent.path = path
    · simp only [same, ↓reduceIte]
      simp only [active, elapsed]
      omega
    · simp [same]

theorem event_preserves_work (state : State) (event : Event Obs)
    (timed : StackValid state.clock state.stack) (admitted : ValidEvent state event)
    (valid : WorkValid state) : WorkValid (profileEvent state event) := by
  intro site parent
  have old := valid site parent
  dsimp only at old
  cases event with
  | sem _ | opaqueOverlapping _ | finish => exact old
  | enter entered =>
    simp only [profileEvent, enter, updatePath, openActive, active, elapsed]
    split <;> simpa using old
  | tick duration =>
    cases h : state.stack with
    | nil => simpa [profileEvent, tick, h, openActive] using old
    | cons frame rest =>
      have bound : frame.start + frame.childNs + frame.blockedNs ≤ state.clock := by
        rw [h] at timed
        exact timed.1
      have accrual : active (state.clock + duration) frame = active state.clock frame + duration := by
        simp only [active, elapsed]
        omega
      simp only [profileEvent, tick, h, openActive, accrual]
      simp only [h, openActive] at old
      by_cases same : ((site, false) :: parent) = frame.path
      · have reverse : frame.path = ((site, false) :: parent) := same.symm
        simp only [reverse, ↓reduceIte] at old ⊢
        omega
      · have reverse : frame.path ≠ ((site, false) :: parent) := Ne.symm same
        simp only [same, reverse, ↓reduceIte, Nat.add_zero] at *
        exact old
  | opaqueBlocked wall =>
    cases h : state.stack with
    | nil => simpa [profileEvent, opaqueBlocked, h, openActive] using old
    | cons frame rest =>
      have bound : frame.start + frame.childNs + frame.blockedNs ≤ state.clock := by
        rw [h] at timed
        exact timed.1
      have blocked : active (state.clock + wall) {frame with blockedNs := frame.blockedNs + wall} =
          active state.clock frame := by
        simp only [active, elapsed]
        omega
      simp only [profileEvent, opaqueBlocked, h, openActive, blocked]
      simpa [h, openActive] using old
  | mergeWorker dropped openFrames entries =>
    simp only [ValidEvent] at admitted
    simp only [profileEvent, mergeWorker]
    rw [fold_worker_serial (parentPath state.stack) entries admitted state.paths site parent]
    exact old
  | exit =>
    cases h : state.stack with
    | nil => simpa [profileEvent, exit, h] using old
    | cons frame rest =>
      have held := openActive_exit state frame rest ((site, false) :: parent) h timed
      simp only [h, openActive] at old
      cases rest with
      | nil =>
        simp only [profileEvent, exit, h, updatePath, openActive]
        by_cases same : ((site, false) :: parent) = frame.path
        · have reverse : frame.path = ((site, false) :: parent) := same.symm
          simp only [reverse, ↓reduceIte, Nat.add_zero] at old ⊢
          simp only [active, elapsed, openActive, Nat.add_zero] at old
          exact old
        · have reverse : frame.path ≠ ((site, false) :: parent) := Ne.symm same
          simp only [same, reverse, ↓reduceIte, Nat.add_zero] at *
          exact old
      | cons previous tail =>
        simp only [profileEvent, exit, h, updatePath]
        have held' : openActive ((site, false) :: parent) state.clock
            ({previous with childNs := previous.childNs + (state.clock - frame.start), childBlockedNs := previous.childBlockedNs + frame.blockedNs + frame.childBlockedNs, childElapsed := (state.clock - frame.start) :: previous.childElapsed} :: tail) =
            openActive ((site, false) :: parent) frame.start (previous :: tail) := by
          simpa [exit, h] using held
        rw [held']
        by_cases same : ((site, false) :: parent) = frame.path
        · have reverse : frame.path = ((site, false) :: parent) := same.symm
          simp only [reverse, ↓reduceIte] at old ⊢
          simp only [active, elapsed] at old
          omega
        · have reverse : frame.path ≠ ((site, false) :: parent) := Ne.symm same
          simp only [same, reverse, ↓reduceIte, Nat.zero_add] at *
          exact old


theorem trace_work (state : State) (events : List (Event Obs))
    (timed : StackValid state.clock state.stack) (valid : WorkValid state)
    (trace : ValidTrace state events) : WorkValid (runProfile state events) := by
  induction trace with
  | nil => exact valid
  | cons state event rest admitted trace ih =>
    exact ih (event_preserves_stack_valid state event timed admitted)
      (event_preserves_work state event timed admitted valid)

theorem initial_work : WorkValid initial := by
  simp [WorkValid, initial, openActive]

theorem credited_only_top (state : State) (event : Event Obs) (path : Path)
    (positive : 0 < credited state event path) :
    ∃ duration frame rest, event = .tick duration ∧ state.stack = frame :: rest ∧ frame.path = path := by
  cases event with
  | tick duration =>
    cases h : state.stack with
    | nil => simp [credited, h] at positive
    | cons frame rest =>
      by_cases same : path = frame.path
      · exact ⟨duration, frame, rest, rfl, rfl, same.symm⟩
      · simp [credited, h, same] at positive
  | sem _ | enter _ | exit | opaqueBlocked _ | opaqueOverlapping _ | mergeWorker _ _ _ | finish =>
    cases state.stack <;> simp [credited] at positive



def eventTime : Event Obs → Nat
  | .tick duration | .opaqueBlocked duration => duration
  | _ => 0

def childTime (state : State) (event : Event Obs) (path : Path) : Nat :=
  match state.stack with
  | [] => 0
  | frame :: _ => if frame.path = path then 0 else eventTime event

def blockedTime (state : State) (event : Event Obs) (path : Path) : Nat :=
  match event, state.stack with
  | .opaqueBlocked wall, frame :: _ => if frame.path = path then wall else 0
  | _, _ => 0

def intervalChildren (state : State) (events : List (Event Obs)) (path : Path) : Nat :=
  match events with
  | [] => 0
  | event :: rest => childTime state event path + intervalChildren (profileEvent state event) rest path

def intervalBlocked (state : State) (events : List (Event Obs)) (path : Path) : Nat :=
  match events with
  | [] => 0
  | event :: rest => blockedTime state event path + intervalBlocked (profileEvent state event) rest path

def OnStack (path : Path) (state : State) : Prop :=
  ∃ frame ∈ state.stack, frame.path = path

inductive FramePresent (path : Path) : State → List (Event Obs) → Prop where
  | nil (state) : OnStack path state → FramePresent path state []
  | cons (state) (event) (rest) : OnStack path state →
      FramePresent path (profileEvent state event) rest → FramePresent path state (event :: rest)

theorem event_clock (state : State) (event : Event Obs) :
    (profileEvent state event).clock = state.clock + eventTime event := by
  cases event with
  | sem _ | enter _ | opaqueOverlapping _ | mergeWorker _ _ _ | finish =>
    simp [profileEvent, eventTime, enter, mergeWorker, finish]
  | tick duration => simp [profileEvent, eventTime, tick]
  | opaqueBlocked wall =>
    cases h : state.stack <;> simp [profileEvent, eventTime, opaqueBlocked, h]
  | exit =>
    cases h : state.stack with
    | nil => simp [profileEvent, eventTime, exit, h]
    | cons frame rest => cases rest <;> simp [profileEvent, eventTime, exit, h]

theorem event_partition (state : State) (event : Event Obs) (path : Path)
    (present : OnStack path state) :
    eventTime event = credited state event path + blockedTime state event path + childTime state event path := by
  cases h : state.stack with
  | nil => simp [OnStack, h] at present
  | cons frame rest =>
    cases event <;> by_cases same : frame.path = path <;>
      simp [eventTime, credited, blockedTime, childTime, h, same]
    all_goals intro equal; exact (same equal.symm).elim

theorem interval_partition (state : State) (events : List (Event Obs)) (path : Path)
    (present : FramePresent path state events) :
    (runProfile state events).clock = state.clock + intervalWork state events path +
      intervalBlocked state events path + intervalChildren state events path := by
  induction present with
  | nil state _ => simp [runProfile, intervalWork, intervalBlocked, intervalChildren]
  | cons state event rest here follow ih =>
    have time := event_clock state event
    have partition := event_partition state event path here
    simp only [runProfile, intervalWork, intervalBlocked, intervalChildren, ih]
    omega

theorem stack_path_desc (frame : Frame) (rest : List Frame) (valid : StackPaths (frame :: rest)) :
    ∀ other ∈ rest, other.path.length < frame.path.length := by
  induction rest generalizing frame with
  | nil => simp
  | cons parent tail ih =>
    simp only [StackPaths, parentPath] at valid
    intro other member
    simp only [List.mem_cons] at member
    rcases member with rfl | member
    · rw [valid.1]
      simp
    · have bound := ih parent valid.2 other member
      rw [valid.1]
      simp only [List.length_cons]
      omega

theorem stack_paths_unique (frame : Frame) (rest : List Frame) (valid : StackPaths (frame :: rest)) :
    ∀ other ∈ rest, other.path ≠ frame.path := by
  intro other member equal
  have smaller := stack_path_desc frame rest valid other member
  rw [equal] at smaller
  exact Nat.lt_irrefl _ smaller

theorem profile_single_active (events : List (Event Obs)) (trace : ValidTrace initial events) :
    WorkValid (runProfile initial events) ∧
    (∀ state (interval : List (Event Obs)) path,
      (runProfile state interval).work path = state.work path + intervalWork state interval path) ∧
    (∀ state (interval : List (Event Obs)) path, FramePresent path state interval →
      (runProfile state interval).clock = state.clock + intervalWork state interval path +
        intervalBlocked state interval path + intervalChildren state interval path) ∧
    (∀ state (event : Event Obs) path, 0 < credited state event path →
      ∃ duration frame rest, event = .tick duration ∧ state.stack = frame :: rest ∧ frame.path = path) ∧
    (∀ frame rest, (runProfile initial events).stack = frame :: rest →
      ∀ other ∈ rest, other.path ≠ frame.path) ∧
    (∀ frame rest, (runProfile initial events).stack = frame :: rest → ∀ duration wall,
      let state := runProfile initial events
      activeStack (state.clock + duration) state.stack =
        (active state.clock frame + duration) :: activeStack frame.start rest ∧
      activeStack (opaqueBlocked state wall).clock (opaqueBlocked state wall).stack =
        active state.clock frame :: activeStack frame.start rest ∧
      active state.clock frame + frame.blockedNs + frame.childNs = elapsed state.clock frame ∧
      activeStack (exit state).clock (exit state).stack = activeStack frame.start rest ∧
      (∀ site, activeStack (enter state site).clock (enter state site).stack =
        0 :: activeStack state.clock state.stack)) ∧
    ((runProfile initial events).stack = [] → ∀ site parent,
      (runProfile initial events).work ((site, false) :: parent) =
        ((runProfile initial events).paths ((site, false) :: parent)).exclusiveNs) := by
  have valid := trace_work initial events initial_valid.1 initial_work trace
  refine ⟨valid, interval_work, interval_partition, credited_only_top, ?_, ?_, ?_⟩
  · intro frame rest stack
    have paths := (trace_ledger initial events initial_valid.1 initial_valid.2.1 initial_valid.2.2.1 trace).2.1
    rw [stack] at paths
    exact stack_paths_unique frame rest paths
  · intro frame rest stack duration wall
    exact single_active_step _ frame rest stack
      (trace_preserves_stack_valid initial events initial_valid.1 trace) duration wall
  · intro closed site parent
    have h := valid site parent
    simpa [closed, openActive] using h


def CleanEvent (state : State) : Event Obs → Prop
  | .finish => state.stack = []
  | .mergeWorker dropped openFrames _ => dropped = 0 ∧ openFrames = 0
  | _ => True

def CleanTrace (state : State) : List (Event Obs) → Prop
  | [] => True
  | event :: rest => CleanEvent state event ∧ CleanTrace (profileEvent state event) rest

theorem complete_clean_trace (state : State) (events : List (Event Obs))
    (done : complete (runProfile state events)) : CleanTrace state events := by
  induction events generalizing state with
  | nil => trivial
  | cons event rest ih =>
    have after : complete (runProfile (profileEvent state event) rest) := done
    constructor
    · cases event with
      | finish =>
        exact (incomplete_local state rest).2.2.2.2 after
      | mergeWorker dropped openFrames paths =>
        exact (incomplete_local state rest).2.2.2.1 dropped openFrames paths after
      | sem _ | tick _ | enter _ | exit | opaqueBlocked _ | opaqueOverlapping _ => trivial
    · exact ih _ after

theorem profile_incomplete_reported (state : State) (events : List (Event Obs)) :
    state.dropped ≤ (runProfile state events).dropped ∧
    (state.stack ≠ [] → ¬ complete (runProfile (finish state) events)) ∧
    (∀ dropped openFrames paths, dropped ≠ 0 ∨ openFrames ≠ 0 →
      ¬ complete (runProfile (mergeWorker state dropped openFrames paths) events)) ∧
    (∀ dropped openFrames paths, complete (runProfile (mergeWorker state dropped openFrames paths) events) →
      dropped = 0 ∧ openFrames = 0) ∧
    (complete (runProfile (finish state) events) → state.stack = []) ∧
    (complete (runProfile state events) → CleanTrace state events) := by
  have localProof := incomplete_local state events
  exact ⟨localProof.1, localProof.2.1, localProof.2.2.1, localProof.2.2.2.1,
    localProof.2.2.2.2, complete_clean_trace state events⟩

end Xsh.Proofs.Core.Profile
