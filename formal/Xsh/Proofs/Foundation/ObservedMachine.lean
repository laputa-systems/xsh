import Xsh.Semantics.Foundation.ObservedMachine
import Xsh.Proofs.BaseMachine

namespace Xsh.Proofs.Foundation.ObservedMachine

open Xsh.Foundation
open Xsh.Values
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace
open Xsh.Semantics.Foundation

private theorem trace_append {start middle finish : MachineConfig}
    {first second : List BaseHostEvent}
    (prior : BaseHostTrace start first middle)
    (suffix : BaseHostTrace middle second finish) :
    BaseHostTrace start (first ++ second) finish := by
  induction prior with
  | refl _ => simpa using suffix
  | @cons start next finish event events transition rest inductionHypothesis =>
    simp only [List.cons_append]
    exact .cons transition (inductionHypothesis suffix)

private theorem base_transition_preserves_wellFormed
    {start finish : MachineConfig} {event : BaseHostEvent}
    (startWellFormed : Xsh.Semantics.BaseMachine.wellFormed start)
    (transition : BaseHostTransition start event finish) :
    Xsh.Semantics.BaseMachine.wellFormed finish := by
  cases transition with
  | internal _ takesStep =>
    exact Xsh.Proofs.BaseMachine.wellFormed_preserved_fragment startWellFormed takesStep
  | request evaluating =>
    exact Xsh.Proofs.BaseMachine.wellFormed_preserved_fragment startWellFormed
      (request_transition_matches_machine_step start evaluating)
  | acceptedResponse waiting checked =>
    exact accepted_host_response_preserves_wellFormed startWellFormed waiting checked
  | rejectedResponse waiting _ =>
    exact rejected_host_response_preserves_wellFormed startWellFormed waiting

theorem initial_configuration_is_wellFormed (machine : MachineConfig)
    (machineWellFormed : Xsh.Semantics.BaseMachine.wellFormed machine)
    (term : CoreTerm) (evaluating : machine.control = .evaluate term) :
    wellFormed (Configuration.start machine term evaluating) := by
  exact ⟨⟨term, evaluating⟩, machineWellFormed, machineWellFormed, .refl machine⟩

theorem observed_step_preserves_wellFormed {config next : Configuration}
    {event : BaseHostEvent} (configWellFormed : wellFormed config)
    (step : LabeledStep config event next) :
    wellFormed next := by
  cases step with
  | edge transition =>
    refine ⟨configWellFormed.1, configWellFormed.2.1,
      base_transition_preserves_wellFormed configWellFormed.2.2.1 transition, ?_⟩
    have edgeTrace : BaseHostTrace config.current [event] _ :=
      .cons transition (.refl _)
    have combined := trace_append configWellFormed.2.2.2 edgeTrace
    simpa using combined

theorem common_configuration_progress (config : Configuration)
    (configWellFormed : wellFormed config) :
    terminalOutcome config.current ≠ none ∨
      (∃ waiting, pendingHostWait? config.current = some waiting ∧
        (∃ term, config.entry.control = .evaluate term) ∧
        BaseHostTrace config.entry config.observations config.current) ∨
      ∃ event next, LabeledStep config event next := by
  rcases configWellFormed with ⟨entryEvaluating, _, _, trace⟩
  rcases Xsh.Proofs.BaseMachine.machine_progress_fragment config.current with
    final | ⟨next, takesStep⟩ | ⟨waiting, pending⟩
  · exact Or.inl final
  · right
    right
    by_cases requesting : config.current.control = .evaluate .timeNow
    · let requested := updateState config.current
        (.awaitingHost BaseHostRequest.timeNow) config.current.continuation
      have requestedStep := request_transition_matches_machine_step config.current requesting
      have nextIsRequested : next = requested := by
        have both := requestedStep.symm.trans takesStep
        exact (Option.some.inj both).symm
      subst next
      exact ⟨.request BaseHostRequest.timeNow, _, .edge (.request config.current requesting)⟩
    · have notRequest : config.current.control ≠ .evaluate .timeNow := by
        intro same
        exact requesting same
      exact ⟨.internal, _, .edge (.internal notRequest takesStep)⟩
  · exact Or.inr (Or.inl ⟨waiting, pending, entryEvaluating, trace⟩)

theorem wellFormed_runner_never_internalStuck {fuel : Nat} {config : Configuration}
    (configWellFormed : wellFormed config) :
    runFuel fuel config.current ≠ .internalStuck :=
  Xsh.Proofs.BaseMachine.machine_runner_never_stuck_fragment configWellFormed.2.2.1

private def arithmeticTerm : CoreTerm :=
  .binary .add (.integerLiteral 2) (.integerLiteral 3)

private def arithmeticMachine : MachineConfig :=
  initialConfig [] [] (fun _ => none) arithmeticTerm

private def smallFive : IntValue :=
  ⟨5, by unfold signed64Min signed64Limit; omega⟩

theorem accepted_arithmetic_term_runs_to_exact_value :
    checkBaseExpr [] [] (fun _ => none) arithmeticTerm = true ∧
      runFuel 5 arithmeticMachine = .completed (.value (typedInteger smallFive)) ∧
      wellFormed (Configuration.start arithmeticMachine arithmeticTerm rfl) := by
  have accepted : checkBaseExpr [] [] (fun _ => none) arithmeticTerm = true := by decide
  have machineWellFormed : Xsh.Semantics.BaseMachine.wellFormed arithmeticMachine := by
    simpa [Xsh.Semantics.BaseMachine.wellFormed, arithmeticMachine, initialConfig,
      checkBaseExpr, continuationChecked, controlFitsContinuation] using accepted
  exact ⟨accepted, rfl,
    initial_configuration_is_wellFormed arithmeticMachine machineWellFormed arithmeticTerm rfl⟩

theorem out_of_range_integer_term_is_rejected :
    checkBaseExpr [] [] (fun _ => none) (.integerLiteral signed64Limit) = false := by
  simp [checkBaseExpr, validateInt?, signed64Limit, signed64Min]

private def clockMachine : MachineConfig :=
  initialConfig [] [] (fun _ => none) .timeNow

private def clockWaiting : MachineConfig :=
  updateState clockMachine (.awaitingHost BaseHostRequest.timeNow) []

private def emptyTraceWaitConfiguration : Configuration :=
  ⟨clockMachine, clockWaiting, []⟩

theorem request_relabel_mutation_is_rejected :
    ¬ BaseHostTransition clockMachine .internal clockWaiting ∧
      Xsh.Semantics.BaseMachine.wellFormed clockMachine ∧
      Xsh.Semantics.BaseMachine.wellFormed clockWaiting ∧
      ¬ wellFormed emptyTraceWaitConfiguration := by
  constructor
  · intro transition
    cases transition with
    | internal notRequest _ => exact notRequest rfl
  constructor
  · simp [Xsh.Semantics.BaseMachine.wellFormed, clockMachine, initialConfig,
      checkBaseExpr, continuationChecked, controlFitsContinuation]
  constructor
  · simp [Xsh.Semantics.BaseMachine.wellFormed, clockWaiting, clockMachine,
      initialConfig, updateState, continuationChecked, controlFitsContinuation]
  · intro configWellFormed
    have trace := configWellFormed.2.2.2
    change BaseHostTrace clockMachine [] clockWaiting at trace
    cases trace

private def wrongClockResponse : BaseHostResponse :=
  ⟨⟨.boolean, .boolean true, trivial⟩⟩

private def clockFailure : MachineConfig :=
  updateState clockWaiting (.failed .hostResponseTypeMismatch) []

private def clockStartConfiguration : Configuration :=
  Configuration.start clockMachine .timeNow rfl

private def clockWaitingConfiguration : Configuration :=
  ⟨clockMachine, clockWaiting, [.request BaseHostRequest.timeNow]⟩

private def clockFailureConfiguration : Configuration :=
  ⟨clockMachine, clockFailure,
    [.request BaseHostRequest.timeNow,
      .response BaseHostRequest.timeNow wrongClockResponse]⟩

theorem wrong_host_response_is_a_preserved_specified_failure :
    checkedHostResponse? BaseHostRequest.timeNow wrongClockResponse = none ∧
      LabeledStep clockStartConfiguration (.request BaseHostRequest.timeNow)
        clockWaitingConfiguration ∧
      LabeledStep clockWaitingConfiguration
        (.response BaseHostRequest.timeNow wrongClockResponse) clockFailureConfiguration ∧
      wellFormed clockFailureConfiguration ∧
      clockFailureConfiguration.outcome? = some (.fault .hostResponseTypeMismatch) := by
  have initialWellFormed : wellFormed clockStartConfiguration := by
    exact initial_configuration_is_wellFormed clockMachine
      (by simp [Xsh.Semantics.BaseMachine.wellFormed, clockMachine, initialConfig,
        checkBaseExpr, continuationChecked, controlFitsContinuation]) .timeNow rfl
  have requestEdge :
      LabeledStep clockStartConfiguration (.request BaseHostRequest.timeNow)
        clockWaitingConfiguration := by
    exact .edge (.request clockMachine rfl)
  have waitingWellFormed := observed_step_preserves_wellFormed initialWellFormed requestEdge
  have responseRejected :
      checkedHostResponse? BaseHostRequest.timeNow wrongClockResponse = none := by
    rfl
  have responseEdge :
      LabeledStep clockWaitingConfiguration
        (.response BaseHostRequest.timeNow wrongClockResponse) clockFailureConfiguration := by
    exact .edge (.rejectedResponse rfl responseRejected)
  have failureWellFormed := observed_step_preserves_wellFormed waitingWellFormed responseEdge
  exact ⟨responseRejected, requestEdge, responseEdge, failureWellFormed, rfl⟩

end Xsh.Proofs.Foundation.ObservedMachine
