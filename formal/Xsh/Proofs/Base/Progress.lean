import Xsh.Proofs.Base.Step
import Xsh.Proofs.Foundation.ObservedMachine

namespace Xsh.Proofs.Base.Progress

open Xsh.Values
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace
open Xsh.Semantics.BaseInvariant
open Xsh.Semantics.Foundation
open Xsh.Proofs.BaseInvariant
open Xsh.Proofs.Base.Step
open Xsh.Proofs.Foundation.ObservedMachine

private theorem control_of_failed_outcome {config : MachineConfig} {fault : BaseFault}
    (outcome : terminalOutcome config = some (.fault fault)) : config.control = .failed fault := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression => simp [terminalOutcome] at outcome
    | returned value =>
      cases continuation <;> simp [terminalOutcome] at outcome
    | awaitingHost request => simp [terminalOutcome] at outcome
    | failed other =>
      simp only [terminalOutcome, Option.some.injEq, BaseOutcome.fault.injEq] at outcome
      subst outcome
      rfl

private theorem pending_wait_describes_its_configuration {config : MachineConfig}
    {waiting : BaseHostWait} (pending : pendingHostWait? config = some waiting) :
    waiting.config = config := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | awaitingHost request =>
      simp only [pendingHostWait?, Option.some.injEq] at pending
      subst pending
      rfl
    | evaluate expression => simp [pendingHostWait?] at pending
    | returned value => simp [pendingHostWait?] at pending
    | failed fault => simp [pendingHostWait?] at pending

theorem every_response_to_a_wait_has_an_edge (config : Configuration) {waiting : BaseHostWait}
    (pending : pendingHostWait? config.current = some waiting) (response : BaseHostResponse) :
    ∃ next, LabeledStep config (.response waiting.request response) next := by
  cases waiting with
  | mk responseType request waitingConfig awaiting =>
    cases config with
    | mk entry current observations =>
      cases current with
      | mk names activations store control continuation =>
        cases control with
        | awaitingHost found =>
          simp only [pendingHostWait?, Option.some.injEq, BaseHostWait.mk.injEq] at pending
          obtain ⟨sameType, sameRequest, sameConfig, _⟩ := pending
          subst sameType
          cases sameRequest
          cases checked : checkedHostResponse? found response with
          | none =>
            exact ⟨_, .edge (.rejectedResponse (config := ⟨names, activations, store,
              .awaitingHost found, continuation⟩) rfl checked)⟩
          | some value =>
            exact ⟨_, .edge (.acceptedResponse (config := ⟨names, activations, store,
              .awaitingHost found, continuation⟩) rfl checked)⟩
        | evaluate expression => simp [pendingHostWait?] at pending
        | returned value => simp [pendingHostWait?] at pending
        | failed fault => simp [pendingHostWait?] at pending

/-- A well-formed observed configuration is finished, steps, or waits on a request that its own
log shows was emitted. The three cases exclude one another. A waiting configuration accepts every
host response, either as a value or as the specified response-mismatch failure, and takes no other
kind of edge. Every failure a well-formed run reaches is specified. When every recorded response
is of the requested type, the failure is the program's own arithmetic failure. -/
theorem base_progress :
    ∀ config : Configuration, Xsh.Semantics.Foundation.wellFormed config →
      ((config.outcome? ≠ none ∧ config.hostWait? = none ∧
          ∀ event next, ¬ LabeledStep config event next) ∨
        (config.outcome? = none ∧ config.hostWait? = none ∧
          ∃ event next, LabeledStep config event next) ∨
        (config.outcome? = none ∧ ∃ waiting, config.hostWait? = some waiting ∧
          (∃ earlier, config.observations = earlier ++ [.request waiting.request]) ∧
          (∀ response, ∃ next, LabeledStep config (.response waiting.request response) next) ∧
          (∀ event next, LabeledStep config event next →
            ∃ (responseType : BaseType) (request : BaseHostRequest responseType)
              (response : BaseHostResponse), event = .response request response))) ∧
      (∀ fault, config.outcome? = some (.fault fault) → isSpecifiedFault fault = true) ∧
      ((∀ {responseType : BaseType} (request : BaseHostRequest responseType)
          (response : BaseHostResponse),
          BaseHostEvent.response request response ∈ config.observations →
            (checkedHostResponse? request response).isSome = true) →
        ∀ fault, config.outcome? = some (.fault fault) → isProgramFault fault = true) := by
  intro config observedWellFormed
  have sound := observed_state_is_sound observedWellFormed
  refine ⟨?_, ?_, ?_⟩
  · rcases Xsh.Proofs.BaseMachine.machine_progress_fragment config.current with
      final | ⟨next, takesStep⟩ | ⟨waiting, pending⟩
    · refine Or.inl ⟨final, terminal_state_is_not_waiting final, ?_⟩
      intro event next labeled
      cases labeled with
      | edge transition => exact final_state_has_no_edge _ final _ _ transition
    · have notFinal : terminalOutcome config.current = none := by
        cases found : terminalOutcome config.current with
        | none => rfl
        | some outcome =>
          rw [final_state_has_no_step (by simp [found])] at takesStep
          cases takesStep
      have notWaiting : pendingHostWait? config.current = none := by
        cases found : pendingHostWait? config.current with
        | none => rfl
        | some waiting =>
          rw [waiting_state_has_no_step found] at takesStep
          cases takesStep
      obtain ⟨event, edge, _⟩ := every_step_is_one_labeled_edge _ _ takesStep
      exact Or.inr (Or.inl ⟨notFinal, notWaiting, event, _, .edge edge⟩)
    · refine Or.inr (Or.inr ⟨waiting_state_is_not_final pending, waiting, pending, ?_, ?_, ?_⟩)
      · have awaiting := waiting.awaiting
        have pendingConfig : waiting.config = config.current :=
          pending_wait_describes_its_configuration pending
        rw [pendingConfig] at awaiting
        exact observed_wait_was_requested observedWellFormed waiting.request awaiting
      · exact every_response_to_a_wait_has_an_edge config pending
      · intro event next labeled
        cases labeled with
        | edge transition =>
          exact waiting_state_only_takes_responses _ waiting pending _ _ transition
  · intro fault outcome
    obtain ⟨_, specified⟩ := sound
    exact specified fault (control_of_failed_outcome outcome)
  · intro accepted fault outcome
    obtain ⟨⟨term, evaluating⟩, entryWellFormed, _, trace⟩ := observedWellFormed
    have entrySound := entry_state_is_sound evaluating entryWellFormed
    have programFaults : faultsProgram config.entry := by
      intro fault failed
      rw [evaluating] at failed
      cases failed
    exact trace_without_rejected_responses_has_program_faults trace entrySound programFaults
      accepted fault (control_of_failed_outcome outcome)

private def literalMachine : MachineConfig :=
  initialConfig [] [] (fun _ => none) (.integerLiteral 1)

private def clockMachine : MachineConfig :=
  initialConfig [] [] (fun _ => none) .timeNow

private def clockWaiting : MachineConfig :=
  updateState clockMachine (.awaitingHost BaseHostRequest.timeNow) []

private def clockAnswer : BaseHostResponse := ⟨typedInteger intOne⟩

private def mismatchedAnswer : BaseHostResponse := ⟨⟨.boolean, .boolean true, trivial⟩⟩

private def literalStart : Configuration :=
  Configuration.start literalMachine (.integerLiteral 1) rfl

private def clockStart : Configuration :=
  Configuration.start clockMachine .timeNow rfl

private def clockWaitingConfiguration : Configuration :=
  ⟨clockMachine, clockWaiting, [.request BaseHostRequest.timeNow]⟩

private def clockReturnedConfiguration : Configuration :=
  ⟨clockMachine, updateState clockWaiting (.returned (typedInteger intOne)) [],
    [.request BaseHostRequest.timeNow, .response BaseHostRequest.timeNow clockAnswer]⟩

private def clockMismatchConfiguration : Configuration :=
  ⟨clockMachine, updateState clockWaiting (.failed .hostResponseTypeMismatch) [],
    [.request BaseHostRequest.timeNow, .response BaseHostRequest.timeNow mismatchedAnswer]⟩

private theorem literalMachineIsChecked :
    Xsh.Semantics.BaseMachine.wellFormed literalMachine :=
  Xsh.Proofs.BaseMachine.accepted_initial_expression_is_wellFormed_fragment
    [] [] (fun _ => none) (.integerLiteral 1) (by decide)

private theorem clockMachineIsChecked :
    Xsh.Semantics.BaseMachine.wellFormed clockMachine :=
  Xsh.Proofs.BaseMachine.accepted_initial_expression_is_wellFormed_fragment
    [] [] (fun _ => none) .timeNow rfl

private theorem literalStartIsWellFormed : Xsh.Semantics.Foundation.wellFormed literalStart :=
  initial_configuration_is_wellFormed literalMachine literalMachineIsChecked _ rfl

private theorem clockWaitingIsWellFormed :
    Xsh.Semantics.Foundation.wellFormed clockWaitingConfiguration :=
  observed_step_preserves_wellFormed
    (initial_configuration_is_wellFormed clockMachine clockMachineIsChecked _ rfl)
    (event := .request BaseHostRequest.timeNow) (next := clockWaitingConfiguration)
    (.edge (.request clockMachine rfl))

private theorem clockReturnedIsWellFormed :
    Xsh.Semantics.Foundation.wellFormed clockReturnedConfiguration :=
  observed_step_preserves_wellFormed clockWaitingIsWellFormed
    (event := .response BaseHostRequest.timeNow clockAnswer)
    (next := clockReturnedConfiguration) (.edge (.acceptedResponse rfl rfl))

private theorem clockMismatchIsWellFormed :
    Xsh.Semantics.Foundation.wellFormed clockMismatchConfiguration :=
  observed_step_preserves_wellFormed clockWaitingIsWellFormed
    (event := .response BaseHostRequest.timeNow mismatchedAnswer)
    (next := clockMismatchConfiguration) (.edge (.rejectedResponse rfl rfl))

theorem base_progress_branches_are_inhabited :
    (Xsh.Semantics.Foundation.wellFormed literalStart ∧ literalStart.outcome? = none ∧
      literalStart.hostWait? = none ∧ ∃ event next, LabeledStep literalStart event next) ∧
    (Xsh.Semantics.Foundation.wellFormed clockWaitingConfiguration ∧
      clockWaitingConfiguration.outcome? = none ∧
      ∃ waiting, clockWaitingConfiguration.hostWait? = some waiting) ∧
    (Xsh.Semantics.Foundation.wellFormed clockReturnedConfiguration ∧
      clockReturnedConfiguration.outcome? = some (.value (typedInteger intOne)) ∧
      clockReturnedConfiguration.hostWait? = none ∧
      ∀ event next, ¬ LabeledStep clockReturnedConfiguration event next) := by
  refine ⟨⟨literalStartIsWellFormed, rfl, rfl, ?_⟩, ⟨clockWaitingIsWellFormed, rfl, ?_⟩,
    ⟨clockReturnedIsWellFormed, rfl, rfl, ?_⟩⟩
  · exact ⟨.internal, _, .edge ((internal_edge_iff_step_off_the_clock _ _).2
      ⟨rfl, by simp [literalStart, Configuration.start, literalMachine, initialConfig]⟩)⟩
  · exact ⟨_, rfl⟩
  · intro event next labeled
    cases labeled with
    | edge transition =>
      exact final_state_has_no_edge _ (by simp [clockReturnedConfiguration, terminalOutcome,
        updateState]) _ _ transition

theorem a_wait_without_its_emitted_request_is_rejected :
    ¬ Xsh.Semantics.Foundation.wellFormed ⟨clockMachine, clockWaiting, []⟩ ∧
    ¬ ∃ earlier, ([] : List BaseHostEvent) = earlier ++ [.request BaseHostRequest.timeNow] := by
  constructor
  · intro configWellFormed
    have trace := configWellFormed.2.2.2
    change BaseHostTrace clockMachine [] clockWaiting at trace
    cases trace
  · rintro ⟨earlier, impossible⟩
    cases earlier <;> simp at impossible

theorem an_internal_fault_is_never_reachable (observations : List BaseHostEvent) :
    ¬ Xsh.Semantics.Foundation.wellFormed
      ⟨literalMachine, updateState literalMachine (.failed .expectedInteger) [], observations⟩ := by
  intro configWellFormed
  have sound := observed_state_is_sound configWellFormed
  have specified := sound.2 .expectedInteger rfl
  simp [isSpecifiedFault] at specified

theorem a_rejected_host_response_is_specified_but_not_a_program_fault :
    Xsh.Semantics.Foundation.wellFormed clockMismatchConfiguration ∧
    clockMismatchConfiguration.outcome? = some (.fault .hostResponseTypeMismatch) ∧
    isSpecifiedFault .hostResponseTypeMismatch = true ∧
    isProgramFault .hostResponseTypeMismatch = false ∧
    ¬ (∀ {responseType : BaseType} (request : BaseHostRequest responseType)
        (response : BaseHostResponse),
        BaseHostEvent.response request response ∈ clockMismatchConfiguration.observations →
          (checkedHostResponse? request response).isSome = true) := by
  refine ⟨clockMismatchIsWellFormed, rfl, rfl, rfl, ?_⟩
  intro accepted
  have inLog : BaseHostEvent.response BaseHostRequest.timeNow mismatchedAnswer ∈
      clockMismatchConfiguration.observations := by
    simp [clockMismatchConfiguration]
  have isSome := accepted BaseHostRequest.timeNow mismatchedAnswer inLog
  simp [checkedHostResponse?, mismatchedAnswer, BaseHostRequest.responseType] at isSome

end Xsh.Proofs.Base.Progress
