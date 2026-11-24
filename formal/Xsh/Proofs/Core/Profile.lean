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
    {state with stack := stack, paths := paths, closed := ⟨frame.path, elapsed, active, frame.blockedNs⟩ :: state.closed}

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


def activeStack : Nat → List Frame → List Nat
  | _, [] => []
  | now, frame :: rest => active now frame :: activeStack frame.start rest

theorem profile_single_active (state : State) (frame : Frame) (rest : List Frame)
    (stack : state.stack = frame :: rest) (valid : StackValid state.clock state.stack)
    (duration wall : Nat)
    (blockedValid : ValidEvent state (.opaqueBlocked wall : Event Obs)) :
    activeStack (state.clock + duration) state.stack =
      (active state.clock frame + duration) :: activeStack frame.start rest ∧
    activeStack (opaqueBlocked state wall).clock (opaqueBlocked state wall).stack =
      (active state.clock frame - wall) :: activeStack frame.start rest ∧
    active state.clock frame + frame.blockedNs + frame.childNs = elapsed state.clock frame := by
  have bound : frame.start + frame.childNs + frame.blockedNs ≤ state.clock := by
    rw [stack] at valid
    exact valid.1
  have blockedBound : frame.start + frame.childNs + frame.blockedNs + wall ≤ state.clock := by
    simpa [ValidEvent, stack] using blockedValid
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
  · exact (frame_accounting state.clock frame bound).2.2.symm


def StackPaths : List Frame → Prop
  | [] => True
  | frame :: rest => frame.path = (frame.site, false) :: parentPath rest ∧ StackPaths rest

theorem event_preserves_stack_paths (state : State) (event : Event Obs)
    (valid : StackPaths state.stack) : StackPaths (profileEvent state event).stack := by
  cases event with
  | sem _ | tick _ | opaqueOverlapping _ | mergeWorker _ _ _ | finish => exact valid
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
  | sem _ | tick _ | opaqueOverlapping _ | mergeWorker _ _ _ | finish => exact previous
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
  | sem _ | tick _ | opaqueOverlapping _ | finish => exact old
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

theorem profile_accounting (events : List (Event Obs)) (trace : ValidTrace initial events) :
    (∀ frame rest, (runProfile initial events).stack = frame :: rest →
      frame.childNs ≤ elapsed (runProfile initial events).clock frame ∧
      frame.blockedNs ≤ elapsed (runProfile initial events).clock frame - frame.childNs ∧
      elapsed (runProfile initial events).clock frame =
        active (runProfile initial events).clock frame + frame.blockedNs + frame.childNs) ∧
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
    exact frame_accounting _ frame valid.1
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

end Xsh.Proofs.Core.Profile
