import Xsh.Proofs.BaseHostTrace
import Xsh.Semantics.BaseInvariant

namespace Xsh.Proofs.BaseInvariant

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace
open Xsh.Semantics.BaseInvariant

theorem step_is_an_update {config next : MachineConfig} (takesStep : step config = some next) :
    ∃ control continuation, next = updateState config control continuation := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression =>
      cases expression <;> simp only [step, Option.some.injEq] at takesStep <;>
        (subst takesStep; exact ⟨_, _, rfl⟩)
    | returned value =>
      cases continuation with
      | nil => simp [step] at takesStep
      | cons frame rest =>
        cases frame <;> simp only [step, Option.some.injEq] at takesStep
        · subst takesStep; exact ⟨_, _, rfl⟩
        · subst takesStep
          split <;> exact ⟨_, _, rfl⟩
        · subst takesStep; exact ⟨_, _, rfl⟩
    | awaitingHost request => simp [step] at takesStep
    | failed fault => simp [step] at takesStep

theorem step_keeps_environment {config next : MachineConfig} (takesStep : step config = some next) :
    next.names = config.names ∧ next.activations = config.activations ∧
      next.store = config.store := by
  obtain ⟨control, continuation, isUpdate⟩ := step_is_an_update takesStep
  subst isUpdate
  exact ⟨rfl, rfl, rfl⟩

private theorem controlWork_of_integer_result (result : IntegerResult) :
    controlWork (controlOfIntegerResult result) = 0 := by
  cases result <;> rfl

private theorem work_update_without_control (config : MachineConfig) (control : BaseControl)
    (continuation : List BaseFrame) (noWork : controlWork control = 0) :
    work (updateState config control continuation) = continuationWork continuation := by
  simp [work, updateState, noWork]

theorem step_decreases_work {config next : MachineConfig} (takesStep : step config = some next) :
    work next < work config := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression =>
      cases expression with
      | integerLiteral value =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        rw [work_update_without_control _ _ _ (controlWork_of_integer_result _)]
        simp [work, controlWork, exprWork]
      | scalarLiteral value =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        rw [work_update_without_control]
        · simp [work, controlWork, exprWork]
        · split <;> rfl
      | timeNow =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        rw [work_update_without_control _ _ _ rfl]
        simp [work, controlWork, exprWork]
      | identifier name =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        rw [work_update_without_control]
        · simp [work, controlWork, exprWork]
        · split <;> rfl
      | negate operand =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp [work, updateState, controlWork, continuationWork, frameWork, exprWork]
        omega
      | binary operator left right =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp [work, updateState, controlWork, continuationWork, frameWork, exprWork]
        omega
    | returned value =>
      cases continuation with
      | nil => simp [step] at takesStep
      | cons frame rest =>
        cases frame with
        | negate =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          rw [work_update_without_control]
          · simp [work, controlWork, continuationWork, frameWork]
          · split
            · exact controlWork_of_integer_result _
            · rfl
        | binaryLeft operator right =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          split
          · simp [work, updateState, controlWork, continuationWork, frameWork]
            omega
          · rw [work_update_without_control _ _ _ rfl]
            simp [work, controlWork, continuationWork, frameWork]
        | binaryRight operator left =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          rw [work_update_without_control]
          · simp [work, controlWork, continuationWork, frameWork]
          · split
            · exact controlWork_of_integer_result _
            · rfl
    | awaitingHost request => simp [step] at takesStep
    | failed fault => simp [step] at takesStep

theorem final_state_has_no_step {config : MachineConfig}
    (final : terminalOutcome config ≠ none) : step config = none := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression => simp [terminalOutcome] at final
    | returned value =>
      cases continuation with
      | nil => simp [step]
      | cons frame rest => simp [terminalOutcome] at final
    | awaitingHost request => simp [terminalOutcome] at final
    | failed fault => simp [step]

theorem waiting_state_has_no_step {config : MachineConfig} {waiting : BaseHostWait}
    (pending : pendingHostWait? config = some waiting) : step config = none := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | awaitingHost request => simp [step]
    | evaluate expression => simp [pendingHostWait?] at pending
    | returned value => simp [pendingHostWait?] at pending
    | failed fault => simp [pendingHostWait?] at pending

theorem awaiting_control_has_no_step {config : MachineConfig} {responseType : BaseType}
    {request : BaseHostRequest responseType} (awaiting : config.control = .awaitingHost request) :
    step config = none := by
  cases config with
  | mk names activations store control continuation =>
    change control = .awaitingHost request at awaiting
    subst awaiting
    simp [step]

theorem stuck_state_is_final_or_waiting {config : MachineConfig} (stuck : step config = none) :
    terminalOutcome config ≠ none ∨ ∃ waiting, pendingHostWait? config = some waiting := by
  rcases Xsh.Proofs.BaseMachine.machine_progress_fragment config with
    final | ⟨next, takesStep⟩ | ⟨waiting, pending⟩
  · exact Or.inl final
  · rw [stuck] at takesStep
    cases takesStep
  · exact Or.inr ⟨waiting, pending⟩

theorem exhausted_work_is_final_or_waiting {config : MachineConfig} (noWork : work config = 0) :
    terminalOutcome config ≠ none ∨ ∃ waiting, pendingHostWait? config = some waiting := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression =>
      cases expression <;> simp [work, controlWork, exprWork] at noWork
    | returned value =>
      cases continuation with
      | nil => simp [terminalOutcome]
      | cons frame rest =>
        cases frame <;> simp [work, controlWork, continuationWork, frameWork] at noWork
    | awaitingHost request =>
      exact Or.inr ⟨⟨_, request, ⟨names, activations, store, .awaitingHost request, continuation⟩,
        rfl⟩, rfl⟩
    | failed fault => simp [terminalOutcome]

private theorem integer_result_fault_is_a_program_fault (result : IntegerResult) (fault : BaseFault)
    (failed : controlOfIntegerResult result = .failed fault) : isProgramFault fault = true := by
  cases result with
  | value integer => simp [controlOfIntegerResult] at failed
  | fault integerFault =>
    simp only [controlOfIntegerResult, BaseControl.failed.injEq] at failed
    subst failed
    rfl

theorem step_failures_are_program_faults {config next : MachineConfig}
    (wellFormedConfig : wellFormed config) (takesStep : step config = some next) :
    faultsProgram next := by
  intro fault failed
  cases config with
  | mk names activations store control continuation =>
    obtain ⟨controlChecked, _, fits⟩ := wellFormedConfig
    cases control with
    | evaluate expression =>
      cases expression with
      | integerLiteral value =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        exact integer_result_fault_is_a_program_fault _ fault failed
      | scalarLiteral value =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        have accepted : (scalarTypedValue? value).isSome = true := controlChecked
        cases found : scalarTypedValue? value with
        | none => simp [found] at accepted
        | some typed => simp [updateState, found] at failed
      | timeNow =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp [updateState] at failed
      | identifier name =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        have accepted : (lookupBaseValue names activations store name).isOk = true :=
          controlChecked
        cases found : lookupBaseValue names activations store name with
        | error problem => simp [found, Except.isOk, Except.toBool] at accepted
        | ok typed => simp [updateState, found] at failed
      | negate operand =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp [updateState] at failed
      | binary operator left right =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp [updateState] at failed
    | returned value =>
      cases continuation with
      | nil => simp [step] at takesStep
      | cons frame rest =>
        have payload : (integerPayload? value).isSome = true := by
          simpa [controlFitsContinuation] using fits
        cases frame with
        | negate =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer =>
            simp only [updateState, found] at failed
            exact integer_result_fault_is_a_program_fault _ fault failed
        | binaryLeft operator right =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer => simp [updateState, found] at failed
        | binaryRight operator left =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer =>
            simp only [updateState, found] at failed
            exact integer_result_fault_is_a_program_fault _ fault failed
    | awaitingHost request => simp [step] at takesStep
    | failed fault => simp [step] at takesStep

private theorem integer_result_is_not_awaiting (result : IntegerResult)
    {responseType : BaseType} (request : BaseHostRequest responseType) :
    controlOfIntegerResult result ≠ .awaitingHost request := by
  cases result <;> simp [controlOfIntegerResult]

theorem step_keeps_faults_specified {config next : MachineConfig}
    (wellFormedConfig : wellFormed config) (takesStep : step config = some next) :
    faultsSpecified next := by
  intro fault failed
  have program := step_failures_are_program_faults wellFormedConfig takesStep fault failed
  cases fault <;> simp_all [isProgramFault, isSpecifiedFault]

theorem step_awaits_only_after_a_clock_read {config next : MachineConfig}
    {responseType : BaseType} {request : BaseHostRequest responseType}
    (takesStep : step config = some next) (awaiting : next.control = .awaitingHost request) :
    config.control = .evaluate .timeNow := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression =>
      cases expression with
      | integerLiteral value =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        exact absurd awaiting (integer_result_is_not_awaiting _ _)
      | scalarLiteral value =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp only [updateState] at awaiting
        split at awaiting <;> simp at awaiting
      | timeNow => rfl
      | identifier name =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp only [updateState] at awaiting
        split at awaiting <;> simp at awaiting
      | negate operand =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp [updateState] at awaiting
      | binary operator left right =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        simp [updateState] at awaiting
    | returned value =>
      cases continuation with
      | nil => simp [step] at takesStep
      | cons frame rest =>
        cases frame with
        | negate =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          simp only [updateState] at awaiting
          split at awaiting
          · exact absurd awaiting (integer_result_is_not_awaiting _ _)
          · simp at awaiting
        | binaryLeft operator right =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          split at awaiting <;> simp [updateState] at awaiting
        | binaryRight operator left =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          simp only [updateState] at awaiting
          split at awaiting
          · exact absurd awaiting (integer_result_is_not_awaiting _ _)
          · simp at awaiting
    | awaitingHost request => simp [step] at takesStep
    | failed fault => simp [step] at takesStep

theorem transition_preserves_sound {start next : MachineConfig} {event : BaseHostEvent}
    (sound : MachineSound start) (edge : BaseHostTransition start event next) :
    MachineSound next := by
  obtain ⟨startWellFormed, startSpecified⟩ := sound
  cases edge with
  | internal notRequest takesStep =>
    exact ⟨Xsh.Proofs.BaseMachine.wellFormed_preserved_fragment startWellFormed takesStep,
      step_keeps_faults_specified startWellFormed takesStep⟩
  | request evaluating =>
    refine ⟨Xsh.Proofs.BaseMachine.wellFormed_preserved_fragment startWellFormed
      (request_transition_matches_machine_step start evaluating), ?_⟩
    intro fault failed
    simp [updateState] at failed
  | acceptedResponse waiting checked =>
    refine ⟨accepted_host_response_preserves_wellFormed startWellFormed waiting checked, ?_⟩
    intro fault failed
    simp [updateState] at failed
  | rejectedResponse waiting rejected =>
    refine ⟨rejected_host_response_preserves_wellFormed startWellFormed waiting, ?_⟩
    intro fault failed
    simp only [updateState, BaseControl.failed.injEq] at failed
    subst failed
    rfl

theorem transition_awaits_only_by_request {start next : MachineConfig} {event : BaseHostEvent}
    {responseType : BaseType} {request : BaseHostRequest responseType}
    (edge : BaseHostTransition start event next)
    (awaiting : next.control = .awaitingHost request) : event = .request request := by
  cases request
  cases edge with
  | internal notRequest takesStep =>
    exact absurd (step_awaits_only_after_a_clock_read takesStep awaiting) notRequest
  | request evaluating => rfl
  | acceptedResponse waiting checked => simp [updateState] at awaiting
  | rejectedResponse waiting rejected => simp [updateState] at awaiting

theorem trace_ending_in_a_wait_ends_with_its_request {start finish : MachineConfig}
    {events : List BaseHostEvent} (trace : BaseHostTrace start events finish)
    {responseType : BaseType} {request : BaseHostRequest responseType}
    (awaiting : finish.control = .awaitingHost request) :
    (events = [] ∧ start = finish) ∨ ∃ earlier, events = earlier ++ [.request request] := by
  induction trace with
  | refl config => exact Or.inl ⟨rfl, rfl⟩
  | @cons start next finish event events edge rest inductionHypothesis =>
    rcases inductionHypothesis awaiting with ⟨noEvents, sameState⟩ | ⟨earlier, ending⟩
    · subst noEvents
      subst sameState
      have isRequest := transition_awaits_only_by_request edge awaiting
      subst isRequest
      exact Or.inr ⟨[], rfl⟩
    · exact Or.inr ⟨event :: earlier, by simp [ending]⟩

theorem trace_preserves_sound {start finish : MachineConfig} {events : List BaseHostEvent}
    (trace : BaseHostTrace start events finish) (sound : MachineSound start) :
    MachineSound finish := by
  induction trace with
  | refl config => exact sound
  | @cons start next finish event events edge rest inductionHypothesis =>
    exact inductionHypothesis (transition_preserves_sound sound edge)

theorem entry_state_is_sound {config : MachineConfig} {expression : BaseExpr}
    (evaluating : config.control = .evaluate expression)
    (wellFormedConfig : wellFormed config) : MachineSound config := by
  refine ⟨wellFormedConfig, ?_⟩
  intro fault failed
  rw [evaluating] at failed
  cases failed

theorem observed_state_is_sound {config : Xsh.Semantics.Foundation.Configuration}
    (observedWellFormed : Xsh.Semantics.Foundation.wellFormed config) :
    MachineSound config.current := by
  obtain ⟨⟨term, evaluating⟩, entryWellFormed, _, trace⟩ := observedWellFormed
  exact trace_preserves_sound trace (entry_state_is_sound evaluating entryWellFormed)

theorem observed_wait_was_requested {config : Xsh.Semantics.Foundation.Configuration}
    (observedWellFormed : Xsh.Semantics.Foundation.wellFormed config) :
    requestIsLastObservation config := by
  intro responseType request awaiting
  obtain ⟨⟨term, evaluating⟩, _, _, trace⟩ := observedWellFormed
  rcases trace_ending_in_a_wait_ends_with_its_request trace awaiting with
    ⟨_, sameState⟩ | found
  · rw [sameState, awaiting] at evaluating
    cases evaluating
  · exact found

theorem trace_without_rejected_responses_has_program_faults
    {start finish : MachineConfig} {events : List BaseHostEvent}
    (trace : BaseHostTrace start events finish) (sound : MachineSound start)
    (programFaults : faultsProgram start)
    (accepted : ∀ {responseType : BaseType} (request : BaseHostRequest responseType)
      (response : BaseHostResponse), BaseHostEvent.response request response ∈ events →
        (checkedHostResponse? request response).isSome = true) :
    faultsProgram finish := by
  induction trace with
  | refl config => exact programFaults
  | @cons start next finish event events edge rest inductionHypothesis =>
    refine inductionHypothesis (transition_preserves_sound sound edge) ?_
      (fun request response member => accepted request response (List.mem_cons_of_mem _ member))
    intro fault failed
    cases edge with
    | internal _ takesStep =>
      exact step_failures_are_program_faults sound.1 takesStep fault failed
    | request _ => simp [updateState] at failed
    | acceptedResponse _ _ => simp [updateState] at failed
    | rejectedResponse waiting rejected =>
      have inEvents := accepted _ _ List.mem_cons_self
      rw [rejected] at inEvents
      cases inEvents

theorem terminal_state_is_not_waiting {config : MachineConfig}
    (final : terminalOutcome config ≠ none) : pendingHostWait? config = none := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression => simp [terminalOutcome] at final
    | returned value => simp [pendingHostWait?]
    | awaitingHost request => simp [terminalOutcome] at final
    | failed fault => simp [pendingHostWait?]

theorem waiting_state_is_not_final {config : MachineConfig} {waiting : BaseHostWait}
    (pending : pendingHostWait? config = some waiting) : terminalOutcome config = none := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression => simp [pendingHostWait?] at pending
    | returned value => simp [pendingHostWait?] at pending
    | awaitingHost request => simp [terminalOutcome]
    | failed fault => simp [pendingHostWait?] at pending

end Xsh.Proofs.BaseInvariant
