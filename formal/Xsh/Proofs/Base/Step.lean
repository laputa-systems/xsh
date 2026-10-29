import Xsh.Proofs.BaseInvariant
import Xsh.Proofs.BaseHostRunnerLaws

namespace Xsh.Proofs.Base.Step

open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace
open Xsh.Semantics.BaseInvariant
open Xsh.Semantics.Foundation
open Xsh.Proofs.BaseInvariant
open Xsh.Proofs.BaseHostRunnerLaws

theorem internal_edge_iff_step_off_the_clock (start next : MachineConfig) :
    BaseHostTransition start .internal next ↔
      (step start = some next ∧ start.control ≠ .evaluate .timeNow) := by
  constructor
  · intro edge
    cases edge with
    | internal notRequest takesStep => exact ⟨takesStep, notRequest⟩
  · rintro ⟨takesStep, notRequest⟩
    exact .internal notRequest takesStep

theorem request_edge_iff_step_on_the_clock (start next : MachineConfig) :
    BaseHostTransition start (.request BaseHostRequest.timeNow) next ↔
      (start.control = .evaluate .timeNow ∧ step start = some next) := by
  constructor
  · intro edge
    cases edge with
    | request evaluating =>
      exact ⟨evaluating, request_transition_matches_machine_step start evaluating⟩
  · rintro ⟨evaluating, takesStep⟩
    have stepped := request_transition_matches_machine_step start evaluating
    rw [stepped] at takesStep
    cases takesStep
    exact .request start evaluating

theorem response_edge_iff_resumption (start next : MachineConfig)
    {responseType : BaseType} (request : BaseHostRequest responseType)
    (response : BaseHostResponse) :
    BaseHostTransition start (.response request response) next ↔
      (start.control = .awaitingHost request ∧ resumeHostRequest start response = some next) := by
  constructor
  · intro edge
    cases edge with
    | acceptedResponse waiting checked =>
      exact ⟨waiting, accepted_response_transition_matches_resume waiting checked⟩
    | rejectedResponse waiting rejected =>
      exact ⟨waiting, rejected_response_transition_matches_resume waiting rejected⟩
  · rintro ⟨waiting, resumed⟩
    cases checked : checkedHostResponse? request response with
    | none =>
      rw [rejected_response_transition_matches_resume waiting checked] at resumed
      cases resumed
      exact .rejectedResponse waiting checked
    | some value =>
      rw [accepted_response_transition_matches_resume waiting checked] at resumed
      cases resumed
      exact .acceptedResponse waiting checked

theorem every_step_is_one_labeled_edge (start next : MachineConfig)
    (takesStep : step start = some next) :
    ∃ event, BaseHostTransition start event next ∧
      ∀ other, BaseHostTransition start other next → other = event := by
  by_cases onTheClock : start.control = .evaluate .timeNow
  · refine ⟨.request BaseHostRequest.timeNow,
      (request_edge_iff_step_on_the_clock start next).2 ⟨onTheClock, takesStep⟩, ?_⟩
    intro other edge
    cases edge with
    | internal notRequest _ => exact absurd onTheClock notRequest
    | request _ => rfl
    | acceptedResponse waiting _ => rw [onTheClock] at waiting; cases waiting
    | rejectedResponse waiting _ => rw [onTheClock] at waiting; cases waiting
  · refine ⟨.internal,
      (internal_edge_iff_step_off_the_clock start next).2 ⟨takesStep, onTheClock⟩, ?_⟩
    intro other edge
    cases edge with
    | internal _ _ => rfl
    | request evaluating => exact absurd evaluating onTheClock
    | acceptedResponse waiting _ =>
      rw [awaiting_control_has_no_step waiting] at takesStep
      cases takesStep
    | rejectedResponse waiting _ =>
      rw [awaiting_control_has_no_step waiting] at takesStep
      cases takesStep

theorem final_state_has_no_edge (start : MachineConfig)
    (final : terminalOutcome start ≠ none) (event : BaseHostEvent) (next : MachineConfig) :
    ¬ BaseHostTransition start event next := by
  intro edge
  cases edge with
  | internal _ takesStep => rw [final_state_has_no_step final] at takesStep; cases takesStep
  | request evaluating => simp [terminalOutcome, evaluating] at final
  | acceptedResponse waiting _ => simp [terminalOutcome, waiting] at final
  | rejectedResponse waiting _ => simp [terminalOutcome, waiting] at final

theorem waiting_state_only_takes_responses (start : MachineConfig) (waiting : BaseHostWait)
    (pending : pendingHostWait? start = some waiting) (event : BaseHostEvent)
    (next : MachineConfig) (edge : BaseHostTransition start event next) :
    ∃ (responseType : BaseType) (request : BaseHostRequest responseType)
      (response : BaseHostResponse), event = .response request response := by
  cases edge with
  | internal _ takesStep => rw [waiting_state_has_no_step pending] at takesStep; cases takesStep
  | request evaluating =>
    cases start with
    | mk names activations store control continuation =>
      change control = _ at evaluating
      subst evaluating
      simp [pendingHostWait?] at pending
  | acceptedResponse _ _ => exact ⟨_, _, _, rfl⟩
  | rejectedResponse _ _ => exact ⟨_, _, _, rfl⟩

theorem edge_keeps_environment {start next : MachineConfig} {event : BaseHostEvent}
    (edge : BaseHostTransition start event next) :
    next.names = start.names ∧ next.activations = start.activations ∧
      next.store = start.store := by
  cases edge with
  | internal _ takesStep => exact step_keeps_environment takesStep
  | request _ => exact ⟨rfl, rfl, rfl⟩
  | acceptedResponse _ _ => exact ⟨rfl, rfl, rfl⟩
  | rejectedResponse _ _ => exact ⟨rfl, rfl, rfl⟩

theorem observed_step_appends_exactly_its_event {config next : Configuration}
    {event : BaseHostEvent} (labeled : LabeledStep config event next) :
    BaseHostTransition config.current event next.current ∧ next.entry = config.entry ∧
      next.observations = config.observations ++ [event] := by
  cases labeled with
  | edge transition => exact ⟨transition, rfl, rfl⟩

/-- The labeled step relation of the base machine. Every machine step is exactly one labeled edge
whose label is fixed by the source state, and each label names the unique behavior it records:
internal work, a yielded host request, or the application of a host response. A finished
machine takes no edge, a waiting machine takes only response edges, an edge never touches the
lexical environment, the activation environment, or the store, and an edge from a sound state
reaches a sound state. -/
theorem base_step :
    (∀ start next,
      BaseHostTransition start .internal next ↔
        (step start = some next ∧ start.control ≠ .evaluate .timeNow)) ∧
    (∀ start next,
      BaseHostTransition start (.request BaseHostRequest.timeNow) next ↔
        (start.control = .evaluate .timeNow ∧ step start = some next)) ∧
    (∀ start next {responseType : BaseType} (request : BaseHostRequest responseType)
      (response : BaseHostResponse),
      BaseHostTransition start (.response request response) next ↔
        (start.control = .awaitingHost request ∧ resumeHostRequest start response = some next)) ∧
    (∀ start event first second,
      BaseHostTransition start event first → BaseHostTransition start event second →
        first = second) ∧
    (∀ start next, step start = some next →
      ∃ event, BaseHostTransition start event next ∧
        ∀ other, BaseHostTransition start other next → other = event) ∧
    (∀ start, terminalOutcome start ≠ none →
      ∀ event next, ¬ BaseHostTransition start event next) ∧
    (∀ start waiting, pendingHostWait? start = some waiting →
      ∀ event next, BaseHostTransition start event next →
        ∃ (responseType : BaseType) (request : BaseHostRequest responseType)
          (response : BaseHostResponse), event = .response request response) ∧
    (∀ start event next, BaseHostTransition start event next →
      next.names = start.names ∧ next.activations = start.activations ∧
        next.store = start.store) ∧
    (∀ start event next, MachineSound start → BaseHostTransition start event next →
      MachineSound next) ∧
    (∀ config event next, LabeledStep config event next →
      BaseHostTransition config.current event next.current ∧ next.entry = config.entry ∧
        next.observations = config.observations ++ [event]) :=
  ⟨internal_edge_iff_step_off_the_clock, request_edge_iff_step_on_the_clock,
    fun start next _ request response => response_edge_iff_resumption start next request response,
    fun _ _ _ _ first second => transition_deterministic first second,
    every_step_is_one_labeled_edge, final_state_has_no_edge, waiting_state_only_takes_responses,
    fun _ _ _ edge => edge_keeps_environment edge,
    fun _ _ _ sound edge => transition_preserves_sound sound edge,
    fun _ _ _ labeled => observed_step_appends_exactly_its_event labeled⟩

private def clockMachine : MachineConfig :=
  initialConfig [] [] (fun _ => none) .timeNow

private def clockWaiting : MachineConfig :=
  updateState clockMachine (.awaitingHost BaseHostRequest.timeNow) []

private def clockAnswer : BaseHostResponse := ⟨typedInteger intOne⟩

private def clockReturned : MachineConfig :=
  updateState clockWaiting (.returned (typedInteger intOne)) []

private def literalMachine : MachineConfig :=
  initialConfig [] [] (fun _ => none) (.integerLiteral 1)

private def clockMachineIsChecked :
    checkBaseExpr [] [] (fun _ => none) BaseExpr.timeNow = true := rfl

theorem base_step_witness :
    MachineSound clockMachine ∧
    BaseHostTransition literalMachine .internal
      (updateState literalMachine (.returned (typedInteger intOne)) []) ∧
    BaseHostTransition clockMachine (.request BaseHostRequest.timeNow) clockWaiting ∧
    BaseHostTransition clockWaiting (.response BaseHostRequest.timeNow clockAnswer)
      clockReturned ∧
    MachineSound clockReturned ∧
    terminalOutcome clockReturned = some (.value (typedInteger intOne)) := by
  have sound : MachineSound clockMachine :=
    entry_state_is_sound (expression := .timeNow) rfl
      (Xsh.Proofs.BaseMachine.accepted_initial_expression_is_wellFormed_fragment
        [] [] (fun _ => none) .timeNow clockMachineIsChecked)
  have requestEdge : BaseHostTransition clockMachine (.request BaseHostRequest.timeNow)
      clockWaiting := .request clockMachine rfl
  have responseEdge : BaseHostTransition clockWaiting
      (.response BaseHostRequest.timeNow clockAnswer) clockReturned :=
    .acceptedResponse rfl rfl
  exact ⟨sound, (internal_edge_iff_step_off_the_clock _ _).2 ⟨rfl, by simp [literalMachine,
    initialConfig]⟩, requestEdge, responseEdge,
    transition_preserves_sound (transition_preserves_sound sound requestEdge) responseEdge, rfl⟩

theorem relabeling_a_request_as_internal_work_is_rejected :
    ¬ BaseHostTransition clockMachine .internal clockWaiting := by
  intro edge
  exact ((internal_edge_iff_step_off_the_clock _ _).1 edge).2 rfl

theorem skipping_the_host_boundary_is_rejected (fabricated : BaseTypedValue)
    (event : BaseHostEvent) :
    ¬ BaseHostTransition clockMachine event (updateState clockMachine (.returned fabricated) []) := by
  intro edge
  cases edge with
  | internal notRequest _ => exact notRequest rfl
  | acceptedResponse waiting _ => exact absurd waiting (by simp [clockMachine, initialConfig])

theorem responding_to_a_machine_that_is_not_waiting_is_rejected (response : BaseHostResponse)
    (next : MachineConfig) :
    ¬ BaseHostTransition clockMachine (.response BaseHostRequest.timeNow response) next := by
  intro edge
  have waiting := ((response_edge_iff_resumption _ _ _ _).1 edge).1
  exact absurd waiting (by simp [clockMachine, initialConfig])

private def mismatchedAnswer : BaseHostResponse := ⟨⟨.boolean, .boolean true, trivial⟩⟩

theorem a_mistyped_response_is_never_accepted_as_a_value :
    ¬ BaseHostTransition clockWaiting
      (.response BaseHostRequest.timeNow mismatchedAnswer)
      (updateState clockWaiting (.returned ⟨.boolean, .boolean true, trivial⟩) []) := by
  intro edge
  cases edge with
  | acceptedResponse _ checked =>
    exact absurd checked (by simp [checkedHostResponse?, mismatchedAnswer,
      BaseHostRequest.responseType])

theorem a_finished_machine_takes_no_edge (event : BaseHostEvent) (next : MachineConfig) :
    ¬ BaseHostTransition clockReturned event next :=
  final_state_has_no_edge clockReturned (by simp [clockReturned, terminalOutcome, updateState])
    event next

end Xsh.Proofs.Base.Step
