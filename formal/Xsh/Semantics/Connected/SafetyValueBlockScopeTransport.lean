import Xsh.Semantics.Connected.SafetyValueBlockInvariant

namespace Xsh.Semantics.Connected

theorem frame_block_scope_reindex {state : Machine} {inner : TypingContext} {output : TypedDemand} {frame : Frame}
    (typed : FrameTyped state (.block inner) output frame) (scope : Xsh.Typing.Connected.Scope) :
    FrameTyped state (.block { inner with scope := scope }) output frame := by
  cases typed with
  | continueBlock solution runtime storage result activation completion scopeAgrees writable schemes storageAgrees schemas rest fresh =>
    exact .continueBlock solution runtime storage result activation completion scopeAgrees writable schemes storageAgrees schemas rest fresh
  | valueBlock solution runtime storage result activation completion scopeAgrees writable schemes storageAgrees schemas =>
    exact .valueBlock solution runtime storage result activation completion scopeAgrees writable schemes storageAgrees schemas
  | caller storage proofSorted runtimeSorted activation completion innerClosed outerClosed scopeAgrees writable schemes storageAgrees schemas =>
    exact .caller storage proofSorted runtimeSorted activation completion innerClosed outerClosed scopeAgrees writable schemes storageAgrees schemas

theorem stack_block_scope_reindex {state : Machine} {context : TypingContext} {output : TypedDemand} {frames : List Frame}
    (typed : StackTyped state (.block context) output frames) (closed : DemandClosed output)
    (scope : Xsh.Typing.Connected.Scope) :
    ∃ nextOutput, DemandClosed nextOutput ∧ StackTyped state (.block { context with scope := scope }) nextOutput frames := by
  cases typed with
  | nil => exact ⟨_, by simpa [DemandClosed] using closed, .nil⟩
  | cons frameTyped middleClosed remaining =>
    exact ⟨_, closed, .cons (frame_block_scope_reindex frameTyped scope) middleClosed remaining⟩

end Xsh.Semantics.Connected
