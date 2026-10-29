import Xsh.Proofs.Base.Step

namespace Xsh.Proofs.Base.Runner

open Xsh.Values
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace
open Xsh.Semantics.BaseInvariant
open Xsh.Proofs.BaseInvariant
open Xsh.Proofs.BaseHostTrace

private theorem runFuel_at_a_final_state {fuel : Nat} {config : MachineConfig}
    {outcome : BaseOutcome} (final : terminalOutcome config = some outcome) :
    runFuel fuel config = .completed outcome := by
  unfold runFuel
  simp [final]

private theorem runFuel_at_a_waiting_state {fuel : Nat} {config : MachineConfig}
    {waiting : BaseHostWait} (notFinal : terminalOutcome config = none)
    (pending : pendingHostWait? config = some waiting) :
    runFuel fuel config = .hostRequest waiting := by
  unfold runFuel
  simp [notFinal, pending]

private theorem runFuel_with_no_fuel_at_a_running_state {config : MachineConfig}
    (notFinal : terminalOutcome config = none) (notWaiting : pendingHostWait? config = none) :
    runFuel 0 config = .outOfFuel := by
  unfold runFuel
  simp [notFinal, notWaiting]

private theorem runFuel_takes_one_step {fuel : Nat} {config next : MachineConfig}
    (notFinal : terminalOutcome config = none) (notWaiting : pendingHostWait? config = none)
    (takesStep : step config = some next) :
    runFuel (fuel + 1) config = runFuel fuel next := by
  rw [runFuel]
  simp [notFinal, notWaiting, takesStep]

private theorem stepping_state_is_running {config next : MachineConfig}
    (takesStep : step config = some next) :
    terminalOutcome config = none ∧ pendingHostWait? config = none := by
  constructor
  · cases found : terminalOutcome config with
    | none => rfl
    | some outcome =>
      rw [final_state_has_no_step (by simp [found])] at takesStep
      cases takesStep
  · cases found : pendingHostWait? config with
    | none => rfl
    | some waiting =>
      rw [waiting_state_has_no_step found] at takesStep
      cases takesStep

private theorem running_state_steps {config : MachineConfig}
    (notFinal : terminalOutcome config = none) (notWaiting : pendingHostWait? config = none) :
    ∃ next, step config = some next := by
  rcases Xsh.Proofs.BaseMachine.machine_progress_fragment config with
    final | found | ⟨waiting, pending⟩
  · exact absurd notFinal final
  · exact found
  · rw [notWaiting] at pending
    cases pending

theorem runner_never_internally_stuck (fuel : Nat) (config : MachineConfig) :
    runFuel fuel config ≠ .internalStuck := by
  induction fuel generalizing config with
  | zero =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal]; simp
    | none =>
      cases pending : pendingHostWait? config with
      | some waiting => rw [runFuel_at_a_waiting_state notFinal pending]; simp
      | none => rw [runFuel_with_no_fuel_at_a_running_state notFinal pending]; simp
  | succ fuel inductionHypothesis =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal]; simp
    | none =>
      cases pending : pendingHostWait? config with
      | some waiting => rw [runFuel_at_a_waiting_state notFinal pending]; simp
      | none =>
        obtain ⟨next, takesStep⟩ := running_state_steps notFinal pending
        rw [runFuel_takes_one_step notFinal pending takesStep]
        exact inductionHypothesis next

theorem completed_run_reached_its_outcome {fuel : Nat} {config : MachineConfig}
    {outcome : BaseOutcome} (completed : runFuel fuel config = .completed outcome) :
    ∃ used final, used ≤ fuel ∧ MachineSteps used config final ∧
      terminalOutcome final = some outcome :=
  Xsh.Proofs.BaseMachine.runner_soundness_fragment completed

theorem host_request_run_reached_its_wait {fuel : Nat} {config : MachineConfig}
    {waiting : BaseHostWait} (requested : runFuel fuel config = .hostRequest waiting) :
    ∃ used final, used ≤ fuel ∧ MachineSteps used config final ∧
      pendingHostWait? final = some waiting := by
  induction fuel generalizing config with
  | zero =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal] at requested; cases requested
    | none =>
      cases pending : pendingHostWait? config with
      | some found =>
        rw [runFuel_at_a_waiting_state notFinal pending] at requested
        cases requested
        exact ⟨0, config, Nat.le_refl _, .refl config, pending⟩
      | none =>
        rw [runFuel_with_no_fuel_at_a_running_state notFinal pending] at requested
        cases requested
  | succ fuel inductionHypothesis =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal] at requested; cases requested
    | none =>
      cases pending : pendingHostWait? config with
      | some found =>
        rw [runFuel_at_a_waiting_state notFinal pending] at requested
        cases requested
        exact ⟨0, config, Nat.zero_le _, .refl config, pending⟩
      | none =>
        obtain ⟨next, takesStep⟩ := running_state_steps notFinal pending
        rw [runFuel_takes_one_step notFinal pending takesStep] at requested
        obtain ⟨used, final, withinFuel, steps, isWaiting⟩ := inductionHypothesis requested
        exact ⟨used + 1, final, Nat.succ_le_succ withinFuel, .step takesStep steps, isWaiting⟩

theorem out_of_fuel_run_is_still_running {fuel : Nat} {config : MachineConfig}
    (exhausted : runFuel fuel config = .outOfFuel) :
    ∃ final, MachineSteps fuel config final ∧ terminalOutcome final = none ∧
      pendingHostWait? final = none := by
  induction fuel generalizing config with
  | zero =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal] at exhausted; cases exhausted
    | none =>
      cases pending : pendingHostWait? config with
      | some found => rw [runFuel_at_a_waiting_state notFinal pending] at exhausted; cases exhausted
      | none => exact ⟨config, .refl config, notFinal, pending⟩
  | succ fuel inductionHypothesis =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal] at exhausted; cases exhausted
    | none =>
      cases pending : pendingHostWait? config with
      | some found => rw [runFuel_at_a_waiting_state notFinal pending] at exhausted; cases exhausted
      | none =>
        obtain ⟨next, takesStep⟩ := running_state_steps notFinal pending
        rw [runFuel_takes_one_step notFinal pending takesStep] at exhausted
        obtain ⟨final, steps, isRunning⟩ := inductionHypothesis exhausted
        exact ⟨final, .step takesStep steps, isRunning⟩

theorem a_finished_path_is_completed_with_enough_fuel {steps : Nat} {start final : MachineConfig}
    {outcome : BaseOutcome} (path : MachineSteps steps start final)
    (finished : terminalOutcome final = some outcome) :
    ∀ fuel, steps ≤ fuel → runFuel fuel start = .completed outcome := by
  induction path with
  | refl config => intro fuel _; exact runFuel_at_a_final_state finished
  | @step count start next final takesStep rest inductionHypothesis =>
    intro fuel enough
    obtain ⟨notFinal, notWaiting⟩ := stepping_state_is_running takesStep
    cases fuel with
    | zero => omega
    | succ fuel =>
      rw [runFuel_takes_one_step notFinal notWaiting takesStep]
      exact inductionHypothesis finished fuel (by omega)

theorem a_waiting_path_is_a_host_request_with_enough_fuel {steps : Nat}
    {start final : MachineConfig} {waiting : BaseHostWait}
    (path : MachineSteps steps start final) (pending : pendingHostWait? final = some waiting) :
    ∀ fuel, steps ≤ fuel → runFuel fuel start = .hostRequest waiting := by
  induction path with
  | refl config =>
    intro fuel _
    exact runFuel_at_a_waiting_state (waiting_state_is_not_final pending) pending
  | @step count start next final takesStep rest inductionHypothesis =>
    intro fuel enough
    obtain ⟨notFinal, notWaiting⟩ := stepping_state_is_running takesStep
    cases fuel with
    | zero => omega
    | succ fuel =>
      rw [runFuel_takes_one_step notFinal notWaiting takesStep]
      exact inductionHypothesis pending fuel (by omega)

theorem enough_fuel_never_changes_a_settled_run {fuel : Nat} {config : MachineConfig}
    (settled : runFuel fuel config ≠ .outOfFuel) (extra : Nat) :
    runFuel (fuel + extra) config = runFuel fuel config := by
  induction fuel generalizing config with
  | zero =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal, runFuel_at_a_final_state notFinal]
    | none =>
      cases pending : pendingHostWait? config with
      | some found =>
        rw [runFuel_at_a_waiting_state notFinal pending,
          runFuel_at_a_waiting_state notFinal pending]
      | none =>
        exact absurd (runFuel_with_no_fuel_at_a_running_state notFinal pending) settled
  | succ fuel inductionHypothesis =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal, runFuel_at_a_final_state notFinal]
    | none =>
      cases pending : pendingHostWait? config with
      | some found =>
        rw [runFuel_at_a_waiting_state notFinal pending,
          runFuel_at_a_waiting_state notFinal pending]
      | none =>
        obtain ⟨next, takesStep⟩ := running_state_steps notFinal pending
        have extended : fuel + 1 + extra = (fuel + extra) + 1 := by omega
        rw [extended, runFuel_takes_one_step notFinal pending takesStep,
          runFuel_takes_one_step notFinal pending takesStep]
        rw [runFuel_takes_one_step notFinal pending takesStep] at settled
        exact inductionHypothesis settled

theorem work_bounds_the_fuel_a_run_needs {fuel : Nat} {config : MachineConfig}
    (enough : work config ≤ fuel) : runFuel fuel config ≠ .outOfFuel := by
  induction fuel generalizing config with
  | zero =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal]; simp
    | none =>
      cases pending : pendingHostWait? config with
      | some waiting => rw [runFuel_at_a_waiting_state notFinal pending]; simp
      | none =>
        have noWork : work config = 0 := by omega
        rcases exhausted_work_is_final_or_waiting noWork with final | ⟨waiting, found⟩
        · exact absurd notFinal final
        · rw [pending] at found
          cases found
  | succ fuel inductionHypothesis =>
    cases notFinal : terminalOutcome config with
    | some outcome => rw [runFuel_at_a_final_state notFinal]; simp
    | none =>
      cases pending : pendingHostWait? config with
      | some waiting => rw [runFuel_at_a_waiting_state notFinal pending]; simp
      | none =>
        obtain ⟨next, takesStep⟩ := running_state_steps notFinal pending
        rw [runFuel_takes_one_step notFinal pending takesStep]
        exact inductionHypothesis (by have := step_decreases_work takesStep; omega)

/-- The finite-fuel runner. It never reports an internal stuck state. A completed run reached a
final state of the machine and reports that state's outcome; a host-request run reached the
waiting state it reports; an out-of-fuel run is still running after exactly the fuel it was
given, so the runner answer is never a language result. Completeness holds in both directions: a
machine path to a final or waiting state is found by every fuel at least its length. Fuel beyond
a settled answer changes nothing, and the work measure bounds the fuel that suffices. The
scripted host driver records a valid labeled trace and preserves well-formedness. -/
theorem base_runner :
    (∀ fuel config, runFuel fuel config ≠ .internalStuck) ∧
    (∀ fuel config outcome, runFuel fuel config = .completed outcome →
      ∃ used final, used ≤ fuel ∧ MachineSteps used config final ∧
        terminalOutcome final = some outcome) ∧
    (∀ fuel config waiting, runFuel fuel config = .hostRequest waiting →
      ∃ used final, used ≤ fuel ∧ MachineSteps used config final ∧
        pendingHostWait? final = some waiting) ∧
    (∀ fuel config, runFuel fuel config = .outOfFuel →
      ∃ final, MachineSteps fuel config final ∧ terminalOutcome final = none ∧
        pendingHostWait? final = none) ∧
    (∀ steps start final outcome, MachineSteps steps start final →
      terminalOutcome final = some outcome →
        ∀ fuel, steps ≤ fuel → runFuel fuel start = .completed outcome) ∧
    (∀ steps start final waiting, MachineSteps steps start final →
      pendingHostWait? final = some waiting →
        ∀ fuel, steps ≤ fuel → runFuel fuel start = .hostRequest waiting) ∧
    (∀ fuel config, runFuel fuel config ≠ .outOfFuel →
      ∀ extra, runFuel (fuel + extra) config = runFuel fuel config) ∧
    (∀ fuel config, work config ≤ fuel → runFuel fuel config ≠ .outOfFuel) ∧
    (∀ fuel script config,
      BaseHostTrace config (runBaseHostFuel fuel script config).events
        (runBaseHostFuel fuel script config).finalConfig) ∧
    (∀ fuel script config, wellFormed config →
      wellFormed (runBaseHostFuel fuel script config).finalConfig) :=
  ⟨runner_never_internally_stuck,
    fun _ _ _ completed => completed_run_reached_its_outcome completed,
    fun _ _ _ requested => host_request_run_reached_its_wait requested,
    fun _ _ exhausted => out_of_fuel_run_is_still_running exhausted,
    fun _ _ _ _ path finished => a_finished_path_is_completed_with_enough_fuel path finished,
    fun _ _ _ _ path pending => a_waiting_path_is_a_host_request_with_enough_fuel path pending,
    fun _ _ settled extra => enough_fuel_never_changes_a_settled_run settled extra,
    fun _ _ enough => work_bounds_the_fuel_a_run_needs enough,
    runBaseHostFuel_trace, runBaseHostFuel_preserves_wellFormed⟩

private def smallFive : IntValue := ⟨5, by unfold signed64Min signed64Limit; omega⟩

private def additionTerm : BaseExpr := .binary .add (.integerLiteral 2) (.integerLiteral 3)

private def additionMachine : MachineConfig :=
  initialConfig [] [] (fun _ => none) additionTerm

private def clockMachine : MachineConfig := initialConfig [] [] (fun _ => none) .timeNow

private def clockWaiting : MachineConfig :=
  updateState clockMachine (.awaitingHost BaseHostRequest.timeNow) []

theorem base_runner_witness :
    work additionMachine = 5 ∧
    runFuel 4 additionMachine = .outOfFuel ∧
    runFuel 5 additionMachine = .completed (.value (typedInteger smallFive)) ∧
    runFuel 9 additionMachine = .completed (.value (typedInteger smallFive)) ∧
    (∃ waiting, runFuel 1 clockMachine = .hostRequest waiting ∧ waiting.config = clockWaiting) ∧
    runFuel 0 clockMachine = .outOfFuel := by
  refine ⟨rfl, rfl, rfl, rfl, ⟨_, rfl, rfl⟩, rfl⟩

def outOfFuelReportedAsAFault (fuel : Nat) (config : MachineConfig) : RunResult :=
  match runFuel fuel config with
  | .outOfFuel => .completed (.fault (.arithmetic .overflow))
  | other => other

theorem reporting_exhausted_fuel_as_a_language_result_is_rejected :
    ¬ ∀ fuel config outcome, outOfFuelReportedAsAFault fuel config = .completed outcome →
      ∃ used final, used ≤ fuel ∧ MachineSteps used config final ∧
        terminalOutcome final = some outcome := by
  intro soundness
  obtain ⟨used, final, withinFuel, path, finished⟩ :=
    soundness 4 additionMachine (.fault (.arithmetic .overflow)) rfl
  have completed := a_finished_path_is_completed_with_enough_fuel path finished 4 withinFuel
  have exhausted : runFuel 4 additionMachine = .outOfFuel := rfl
  rw [exhausted] at completed
  cases completed

end Xsh.Proofs.Base.Runner
