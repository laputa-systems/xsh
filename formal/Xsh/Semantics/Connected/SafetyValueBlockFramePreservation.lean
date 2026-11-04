import Xsh.Semantics.Connected.SafetyValueBlockWorld

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem saved_frame_scope_survives_step {before after : Machine} {event : Label}
    (step : Step before event after) (context : TypingContext) (cells : ActiveCells)
    (scope : ScopeStateAgrees context.scope { before with cells := cells, solution := context.solution })
    (writable : ScopeWritableContract context.scope context.solution cells before.store)
    (schemes : SchemeCellsImmutable cells before.store)
    (storage : StorageAgreement cells before.store context.storage)
    (schemas : SchemaMatches context.scope context.solution before.schemas) :
    ScopeStateAgrees context.scope { after with cells := cells, solution := context.solution } ∧
      ScopeWritableContract context.scope context.solution cells after.store ∧
      SchemeCellsImmutable cells after.store ∧ StorageAgreement cells after.store context.storage ∧
      SchemaMatches context.scope context.solution after.schemas := by
  have execution : Execution before [event] after := .next step (.done after)
  obtain ⟨savedScope, savedSchemes⟩ := saved_scope_survives_execution before after [event]
    context.scope context.solution cells execution scope schemes writable
  have layout := step_preserves_store_layout step
  have stable := step_preserves_immutable_cells step
  obtain ⟨sameFunctions, sameSchemas, sameGlobals⟩ := step_preserves_global_context step
  refine ⟨savedScope, ?_, savedSchemes, ?_, ?_⟩
  · exact scope_writable_contract_survives context.scope context.solution cells before.store after.store
      scope.2.2.2.2 writable layout
  · exact storage_agreement_survives_stable_store cells context.storage before.store after.store
      scope.2.2.2.2 schemes storage layout stable
  · rw [sameSchemas]
    exact schemas

def FutureFrameBindingsFresh (state : Machine) (input : TypedDemand) (frame : Frame) : Prop :=
  match frame with
  | .bind binder _ _ rest => state.store ⟨input.context.activation, binder⟩ = none ∧
      FutureBindingsFresh state.store input.context.activation rest
  | .assign _ rest | .discard rest => FutureBindingsFresh state.store input.context.activation rest
  | .branch branches otherwise rest => RemainingBranchBindingsFresh state.store input.context.activation branches ∧
      FutureBindingsFresh state.store input.context.activation otherwise ∧ FutureBindingsFresh state.store input.context.activation rest
  | .continueBlock _ rest => FutureBindingsFresh state.store input.context.activation rest
  | _ => True

theorem frame_typing_survives_step {before after : Machine} {event : Label}
    (step : Step before event after) (input output : TypedDemand) (frame : Frame)
    (typed : FrameTyped before input output frame) (fresh : FutureFrameBindingsFresh after input frame) :
    FrameTyped after input output frame := by
  obtain ⟨sameFunctions, sameSchemas, sameGlobals⟩ := step_preserves_global_context step
  cases typed with
  | binaryLeft right operation => exact .binaryLeft right operation
  | binaryRight valueTyped closed callable operation =>
    exact .binaryRight valueTyped closed (by simpa [sameFunctions] using callable) operation
  | callTarget arguments => exact .callTarget arguments
  | callArguments done remaining target callable =>
    exact .callArguments (by simpa [sameFunctions] using done) remaining target (by simpa [sameFunctions] using callable)
  | recordFields done remaining unique => exact .recordFields (by simpa [sameFunctions] using done) remaining unique
  | field occurs => exact .field occurs
  | conditional nonempty branches otherwise => exact .conditional nonempty branches otherwise
  | propagate result => exact .propagate result
  | requireGuard base => exact .requireGuard base
  | bind compatible closed kind storage scopeFresh oldCell body oldRest =>
    simp only [FutureFrameBindingsFresh, TypedDemand.context] at fresh
    exact .bind compatible closed kind storage scopeFresh fresh.1 body fresh.2
  | assign compatible found mutable body oldRest =>
    exact .assign compatible found mutable body (by simpa [FutureFrameBindingsFresh, TypedDemand.context] using fresh)
  | discard body oldRest => exact .discard body (by simpa [FutureFrameBindingsFresh, TypedDemand.context] using fresh)
  | returnValue => exact .returnValue
  | tailValue accepts => exact .tailValue accepts
  | branch nonempty branches fallback rest oldBranches oldFallback oldRest =>
    simp only [FutureFrameBindingsFresh, TypedDemand.context] at fresh
    exact .branch nonempty branches fallback rest fresh.1 fresh.2.1 fresh.2.2
  | continueBlock solution runtime storage result activation completion scope writable schemes storageAgrees schemas rest oldFresh =>
    rename_i inner outer cells following
    obtain ⟨scopeAfter, writableAfter, schemesAfter, storageAfter, schemasAfter⟩ :=
      saved_frame_scope_survives_step step outer cells scope writable schemes storageAgrees schemas
    have freshAfter : FutureBindingsFresh after.store outer.activation following := by
      simpa [FutureFrameBindingsFresh, TypedDemand.context, activation] using fresh
    exact .continueBlock solution runtime storage result activation completion scopeAfter writableAfter schemesAfter storageAfter schemasAfter rest freshAfter
  | valueBlock solution runtime storage result activation completion scope writable schemes storageAgrees schemas =>
    rename_i inner outer cells blockType
    obtain ⟨scopeAfter, writableAfter, schemesAfter, storageAfter, schemasAfter⟩ :=
      saved_frame_scope_survives_step step outer cells scope writable schemes storageAgrees schemas
    exact .valueBlock solution runtime storage result activation completion scopeAfter writableAfter schemesAfter storageAfter schemasAfter
  | caller storage proofSorted runtimeSorted activation completion result callType scope writable schemes storageAgrees schemas =>
    rename_i inner outer cells callerActivation resultType expected
    obtain ⟨scopeAfter, writableAfter, schemesAfter, storageAfter, schemasAfter⟩ :=
      saved_frame_scope_survives_step step outer cells scope writable schemes storageAgrees schemas
    exact .caller storage proofSorted runtimeSorted activation completion result callType scopeAfter writableAfter schemesAfter storageAfter schemasAfter

theorem stack_typing_survives_step {before after : Machine} {event : Label}
    (step : Step before event after) (input output : TypedDemand) (frames : List Frame)
    (typed : StackTyped before input output frames)
    (fresh : ∀ input output frame, FrameTyped before input output frame → frame ∈ frames →
      FutureFrameBindingsFresh after input frame) : StackTyped after input output frames := by
  induction typed with
  | nil => exact .nil
  | @cons input middle output frame rest frameTyped closed remaining ih =>
    refine .cons (frame_typing_survives_step step input middle frame frameTyped (fresh input middle frame frameTyped (by simp))) closed ?_
    apply ih
    intro nextInput nextOutput nextFrame nextTyped member
    exact fresh nextInput nextOutput nextFrame nextTyped (List.mem_cons_of_mem _ member)

end Xsh.Semantics.Connected
