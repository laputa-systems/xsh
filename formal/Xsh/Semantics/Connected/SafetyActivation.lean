import Xsh.Semantics.Connected.SafetyInvoke

namespace Xsh.Semantics.Connected

open Xsh.Semantics.Environments

theorem write_preserves_activation_bound (store next : Store) (cell : CellId) (value : Value)
    (limit : Nat) (below : StoreBelowActivation store limit)
    (written : write store cell value = .accepted next) : StoreBelowActivation next limit := by
  obtain ⟨current, present, ⟨replacement, atCell, _, _, _⟩, unchanged⟩ :=
    write_accepted_replaces_only_its_cell (fun _ _ => inferInstance) store cell value next written
  intro other entry after
  by_cases same : other = cell
  · subst other
    exact below cell current present
  · rw [unchanged other same] at after
    exact below other entry after

theorem step_preserves_store_activation_bound {before after : Machine} {event : Label}
    (step : Step before event after) (activeBelow : before.activation < before.nextActivation)
    (below : StoreBelowActivation before.store before.nextActivation) :
    StoreBelowActivation after.store after.nextActivation := by
  cases step <;> try exact below
  case bindingCommit state binder kind storage value rest next allocated =>
    exact allocate_preserves_activation_bound state.store next _ _ storage value state.nextActivation below activeBelow allocated
  case assignmentCommit state binder value rest cell next active written =>
    exact write_preserves_activation_bound state.store next cell value state.nextActivation below written
  case assignmentRejected state binder value rest cell next active rejected =>
    have same := rejected_assignment_does_not_change_store state.store next cell value rejected
    subst next
    exact below
  case callFunction state declaration arguments parameterTypes resultType captured typeArguments normalized function cells store found signature bound =>
    apply parameters_preserve_activation_bound state.nextActivation function.parameters arguments parameterTypes state.store store cells
    · intro cell entry present
      exact Nat.lt_succ_of_lt (below cell entry present)
    · exact Nat.lt_succ_self _
    · exact bound

def FrameActivationBelow (limit : Nat) : Frame → Prop
  | .caller _ activation _ _ => activation < limit
  | _ => True

def StackActivationsBelow (limit : Nat) (stack : List Frame) : Prop :=
  ∀ frame ∈ stack, FrameActivationBelow limit frame

theorem step_preserves_current_activation_bound {before after : Machine} {event : Label}
    (step : Step before event after) (active : before.activation < before.nextActivation)
    (stack : StackActivationsBelow before.nextActivation before.stack) :
    after.activation < after.nextActivation := by
  cases step <;> try exact active
  case callFunction => exact Nat.lt_succ_self _
  case returnedToCaller state value cells activation saved ty typed =>
    exact stack (.caller cells activation saved ty) (by simp)
  case completedToCaller state value cells activation saved ty typed =>
    exact stack (.caller cells activation saved ty) (by simp)
  case propagatedToCaller state error oldType valueType errorType cells activation saved typed =>
    exact stack (.caller cells activation saved (.result valueType errorType)) (by simp)

theorem frame_activation_bound_weaken {frame : Frame} {first second : Nat}
    (ordered : first ≤ second) (below : FrameActivationBelow first frame) : FrameActivationBelow second frame := by
  cases frame <;> simp_all [FrameActivationBelow]
  exact Nat.lt_of_lt_of_le below ordered

theorem step_preserves_stack_activation_bound {before after : Machine} {event : Label}
    (step : Step before event after) (active : before.activation < before.nextActivation)
    (stack : StackActivationsBelow before.nextActivation before.stack) :
    StackActivationsBelow after.nextActivation after.stack := by
  cases step <;> simp_all [StackActivationsBelow, FrameActivationBelow, List.mem_cons]
  case callFunction =>
    constructor
    · exact Nat.lt_succ_of_lt active
    · intro frame member
      exact frame_activation_bound_weaken (Nat.le_succ _) (stack frame member)

structure ActivationInvariant (state : Machine) : Prop where
  active : state.activation < state.nextActivation
  saved : StackActivationsBelow state.nextActivation state.stack
  store : StoreBelowActivation state.store state.nextActivation

theorem step_preserves_activation_invariant {before after : Machine} {event : Label}
    (step : Step before event after) (invariant : ActivationInvariant before) : ActivationInvariant after :=
  ⟨step_preserves_current_activation_bound step invariant.active invariant.saved,
    step_preserves_stack_activation_bound step invariant.active invariant.saved,
    step_preserves_store_activation_bound step invariant.active invariant.store⟩

theorem execution_preserves_activation_invariant {before after : Machine} {events : List Label}
    (execution : Execution before events after) (invariant : ActivationInvariant before) : ActivationInvariant after := by
  induction execution with
  | done => exact invariant
  | next step _ ih => exact ih (step_preserves_activation_invariant step invariant)

theorem empty_store_initial_activation (state : Machine) :
    ActivationInvariant { state with store := fun _ => none, activation := 0, nextActivation := 1, stack := [] } := by
  constructor
  · exact Nat.zero_lt_succ 0
  · intro frame member
    simp at member
  · intro cell entry present
    simp at present

theorem activation_invariant_rejects_occupied_next_activation (state : Machine)
    (binder : Xsh.Foundation.BinderId) (entry : StoreCell RuntimeType Value HasType)
    (present : state.store ⟨state.nextActivation, binder⟩ = some entry) :
    ¬ ActivationInvariant state := by
  intro invariant
  have impossible := invariant.store ⟨state.nextActivation, binder⟩ entry present
  exact Nat.lt_irrefl _ impossible

end Xsh.Semantics.Connected
