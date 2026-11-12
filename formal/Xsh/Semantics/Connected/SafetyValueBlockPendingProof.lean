import Xsh.Semantics.Connected.SafetyValueBlockFramePreservation

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem pending_head_binders_fresh (state : Machine) (activation : Nat) (frame : Frame) (rest : List Frame)
    (pending : PendingBindingsSafe state) (active : state.activation = activation)
    (stack : state.stack = frame :: rest) :
    ∀ binder ∈ pendingFrameBinders frame, state.store ⟨activation, binder⟩ = none := by
  intro binder member
  apply pending.fresh (activation, binder)
  simp [pendingMachineBinders, pendingStackBinders, active, stack, member]

theorem pending_head_frame_fresh (state : Machine) (input : TypedDemand) (frame : Frame) (rest : List Frame)
    (pending : PendingBindingsSafe state) (active : state.activation = input.context.activation)
    (stack : state.stack = frame :: rest) : FutureFrameBindingsFresh state input frame := by
  have fresh := pending_head_binders_fresh state input.context.activation frame rest pending active stack
  cases frame <;> simp_all [FutureFrameBindingsFresh, FutureBindingsFresh, FutureBranchBindingsFresh,
    RemainingBranchBindingsFresh, pendingFrameBinders, List.mem_cons]
  case branch branches otherwise following =>
    cases branches with
    | nil => simp_all
    | cons first rest => cases first; simp_all

end Xsh.Semantics.Connected

namespace Xsh.Semantics.Connected

theorem frame_activation_follows_stack {state : Machine} {input output : TypedDemand} {frame : Frame}
    (typed : FrameTyped state input output frame) :
    output.context.activation = frameCallerActivation input.context.activation frame := by
  cases typed <;> simp_all [TypedDemand.context, frameCallerActivation]

theorem stack_typing_survives_pending {before after : Machine} {event : Label}
    (step : Step before event after) {input output : TypedDemand} {frames : List Frame}
    (typed : StackTyped before input output frames)
    (fresh : ∀ binding ∈ pendingStackBinders input.context.activation frames,
      after.store ⟨binding.1, binding.2⟩ = none) : StackTyped after input output frames := by
  induction typed with
  | nil => exact .nil
  | @cons input middle output frame rest frameTyped closed remaining ih =>
    have frameFresh : ∀ binder ∈ pendingFrameBinders frame,
        after.store ⟨input.context.activation, binder⟩ = none := by
      intro binder member
      apply fresh (input.context.activation, binder)
      simp [pendingStackBinders, member]
    have future : FutureFrameBindingsFresh after input frame := by
      cases frame <;> simp_all [FutureFrameBindingsFresh, FutureBindingsFresh,
        FutureBranchBindingsFresh, RemainingBranchBindingsFresh, pendingFrameBinders, List.mem_cons]
      case branch branches otherwise following =>
        cases branches with
        | nil => simp_all
        | cons first rest => cases first; simp_all
    refine .cons (frame_typing_survives_step step input middle frame frameTyped future) closed ?_
    apply ih
    intro binding member
    apply fresh binding
    simp only [pendingStackBinders, List.mem_append]
    right
    simpa [frame_activation_follows_stack frameTyped] using member

theorem stack_typing_survives_with_pending {before after : Machine} {event : Label}
    (step : Step before event after) {input output : TypedDemand} {frames : List Frame}
    (typed : StackTyped before input output frames) (pending : PendingBindingsSafe after)
    (active : after.activation = input.context.activation) (stack : after.stack = frames) :
    StackTyped after input output frames := by
  apply stack_typing_survives_pending step typed
  intro binding member
  apply pending.fresh binding
  simp only [pendingMachineBinders, List.mem_append]
  right
  simpa [active, stack] using member

end Xsh.Semantics.Connected
