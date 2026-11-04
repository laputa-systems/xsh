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
