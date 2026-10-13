import Xsh.Proofs.BaseHostTrace

namespace Xsh.Proofs.BaseHostRunnerLaws

open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace
open Xsh.Values

/-- A fixed source and event label determine at most one host-transition destination. -/
theorem transition_deterministic {start nextLeft nextRight : MachineConfig}
    {event : BaseHostEvent}
    (left : BaseHostTransition start event nextLeft)
    (right : BaseHostTransition start event nextRight) :
    nextLeft = nextRight := by
  cases left <;> cases right <;> simp_all

/-- A fixed initial state and event list determine at most one trace endpoint. -/
theorem trace_deterministic {start finishLeft finishRight : MachineConfig}
    {events : List BaseHostEvent}
    (left : BaseHostTrace start events finishLeft)
    (right : BaseHostTrace start events finishRight) :
    finishLeft = finishRight := by
  induction left generalizing finishRight with
  | refl config =>
    cases right
    rfl
  | @cons start next finish event events edge rest inductionHypothesis =>
    cases right with
    | @cons _ rightNext rightFinish _ _ rightEdge rightRest =>
      have sameNext := transition_deterministic edge rightEdge
      subst rightNext
      exact inductionHypothesis rightRest

private theorem runBaseHostFuel_stops_at_terminal (fuel : Nat)
    (responses : List BaseHostResponse) (config : MachineConfig)
    (terminal : terminalOutcome config ≠ none) :
    runBaseHostFuel fuel responses config = ⟨config, []⟩ := by
  cases fuel with
  | zero => rfl
  | succ fuel =>
    cases control : config.control with
    | failed fault => simp [runBaseHostFuel, control]
    | returned value =>
      cases continuation : config.continuation with
      | nil => simp [runBaseHostFuel, control, continuation, step]
      | cons frame rest => simp [terminalOutcome, control, continuation] at terminal
    | awaitingHost request => simp [terminalOutcome, control] at terminal
    | evaluate expression => simp [terminalOutcome, control] at terminal

/-- Extending both fuel and the host script cannot change a run that already ended at a terminal
outcome. The appended responses are unused because terminal states stop the runner. -/
theorem runBaseHostFuel_terminal_extension (fuel extra : Nat)
    (responses suffix : List BaseHostResponse) (config : MachineConfig)
    (terminal : terminalOutcome (runBaseHostFuel fuel responses config).finalConfig ≠ none) :
    runBaseHostFuel (fuel + extra) (responses ++ suffix) config =
      runBaseHostFuel fuel responses config := by
  induction fuel generalizing extra responses suffix config with
  | zero =>
    have terminalConfig : terminalOutcome config ≠ none := by
      simpa only [runBaseHostFuel] using terminal
    rw [runBaseHostFuel_stops_at_terminal (fuel := 0 + extra)
      (responses := responses ++ suffix) (config := config) terminalConfig]
    rfl
  | succ fuel inductionHypothesis =>
    have extendedFuel : fuel + 1 + extra = fuel + extra + 1 := by omega
    rw [extendedFuel]
    cases control : config.control with
    | awaitingHost request =>
      cases responses with
      | nil =>
        simp [runBaseHostFuel, control, terminalOutcome] at terminal
      | cons response remaining =>
        cases checked : checkedHostResponse? request response with
        | none =>
          let next := updateState config (.failed .hostResponseTypeMismatch)
            config.continuation
          have terminalRest :
              terminalOutcome (runBaseHostFuel fuel remaining next).finalConfig ≠ none := by
            simpa only [runBaseHostFuel, control, checked] using terminal
          have restEqual := inductionHypothesis extra remaining suffix next terminalRest
          simpa only [runBaseHostFuel, control, checked, List.cons_append] using
            congrArg (fun rest : BaseHostRun =>
              (⟨rest.finalConfig,
                BaseHostEvent.response request response :: rest.events⟩ : BaseHostRun))
              restEqual
        | some value =>
          let next := updateState config (.returned value) config.continuation
          have terminalRest :
              terminalOutcome (runBaseHostFuel fuel remaining next).finalConfig ≠ none := by
            simpa only [runBaseHostFuel, control, checked] using terminal
          have restEqual := inductionHypothesis extra remaining suffix next terminalRest
          simpa only [runBaseHostFuel, control, checked, List.cons_append] using
            congrArg (fun rest : BaseHostRun =>
              (⟨rest.finalConfig,
                BaseHostEvent.response request response :: rest.events⟩ : BaseHostRun))
              restEqual
    | failed fault =>
      simp [runBaseHostFuel, control]
    | evaluate expression =>
      cases expression with
      | timeNow =>
        let next := updateState config (.awaitingHost .timeNow) config.continuation
        have terminalRest :
            terminalOutcome (runBaseHostFuel fuel responses next).finalConfig ≠ none := by
          simpa only [runBaseHostFuel, control] using terminal
        have restEqual := inductionHypothesis extra responses suffix next terminalRest
        simpa only [runBaseHostFuel, control] using
          congrArg (fun rest : BaseHostRun =>
            (⟨rest.finalConfig,
              BaseHostEvent.request BaseHostRequest.timeNow :: rest.events⟩ : BaseHostRun))
            restEqual
      | _ =>
        cases stepped : step config with
        | none =>
          have terminalConfig : terminalOutcome config ≠ none := by
            simpa only [runBaseHostFuel, control, stepped] using terminal
          rw [runBaseHostFuel_stops_at_terminal
            (fuel := fuel + extra + 1) (responses := responses ++ suffix)
            (config := config) terminalConfig]
          simp [runBaseHostFuel, control, stepped]
        | some next =>
          have terminalRest :
              terminalOutcome (runBaseHostFuel fuel responses next).finalConfig ≠ none := by
            simpa only [runBaseHostFuel, control, stepped] using terminal
          have restEqual := inductionHypothesis extra responses suffix next terminalRest
          simpa only [runBaseHostFuel, control, stepped] using
            congrArg (fun rest : BaseHostRun =>
              (⟨rest.finalConfig, BaseHostEvent.internal :: rest.events⟩ : BaseHostRun))
              restEqual
    | returned value =>
      cases stepped : step config with
      | none =>
        have terminalConfig : terminalOutcome config ≠ none := by
          simpa only [runBaseHostFuel, control, stepped] using terminal
        rw [runBaseHostFuel_stops_at_terminal
          (fuel := fuel + extra + 1) (responses := responses ++ suffix)
          (config := config) terminalConfig]
        simp [runBaseHostFuel, control, stepped]
      | some next =>
        have terminalRest :
            terminalOutcome (runBaseHostFuel fuel responses next).finalConfig ≠ none := by
          simpa only [runBaseHostFuel, control, stepped] using terminal
        have restEqual := inductionHypothesis extra responses suffix next terminalRest
        simpa only [runBaseHostFuel, control, stepped] using
          congrArg (fun rest : BaseHostRun =>
            (⟨rest.finalConfig, BaseHostEvent.internal :: rest.events⟩ : BaseHostRun))
            restEqual

/-- A waiting machine with no supplied response remains unchanged for every fuel amount. -/
theorem runBaseHostFuel_waiting_without_response (fuel : Nat) (config : MachineConfig)
    {responseType : BaseType} (request : BaseHostRequest responseType)
    (waiting : config.control = .awaitingHost request) :
    runBaseHostFuel fuel [] config = ⟨config, []⟩ := by
  cases fuel with
  | zero => rfl
  | succ fuel => simp [runBaseHostFuel, waiting]

/-- Extra fuel alone does not resume a waiting machine when the script still has no response. -/
theorem runBaseHostFuel_waiting_fuel_extension (fuel extra : Nat)
    (config : MachineConfig) {responseType : BaseType}
    (request : BaseHostRequest responseType)
    (waiting : config.control = .awaitingHost request) :
    runBaseHostFuel (fuel + extra) [] config = runBaseHostFuel fuel [] config := by
  rw [runBaseHostFuel_waiting_without_response (fuel + extra) config request waiting,
    runBaseHostFuel_waiting_without_response fuel config request waiting]

private def twoInt : IntValue :=
  ⟨2, by unfold signed64Min signed64Limit; omega⟩

private def fiveInt : IntValue :=
  ⟨5, by unfold signed64Min signed64Limit; omega⟩

private def responseTwoValue : BaseTypedValue := typedInteger twoInt

private def responseFiveValue : BaseTypedValue := typedInteger fiveInt

private def responseTwo : BaseHostResponse := ⟨responseTwoValue⟩

private def responseFive : BaseHostResponse := ⟨responseFiveValue⟩

private def timeNowConfig : MachineConfig := initialConfig [] [] (fun _ => none) .timeNow

private def waitingForTimeNow : MachineConfig :=
  updateState timeNowConfig (.awaitingHost .timeNow) []

private theorem responseValues_are_distinct : responseFiveValue ≠ responseTwoValue := by
  intro equal
  have payloadsEqual := congrArg integerPayload? equal
  simp [responseFiveValue, responseTwoValue, typedInteger, integerPayload?] at payloadsEqual
  have numericEqual := congrArg Subtype.val payloadsEqual
  simp [fiveInt, twoInt] at numericEqual

/-- A waiting run consumes the first response, so swapping distinct scripted responses changes its
result. -/
theorem response_order_changes_waiting_result :
    (runBaseHostFuel 1 [responseFive, responseTwo] waitingForTimeNow).finalConfig.control =
      .returned responseFiveValue ∧
    (runBaseHostFuel 1 [responseTwo, responseFive] waitingForTimeNow).finalConfig.control =
      .returned responseTwoValue := by
  constructor <;> rfl

/-- Unlike terminal outcomes, a waiting result can change when a response is appended. -/
theorem response_suffix_changes_waiting_result :
    (runBaseHostFuel 1 [responseFive] waitingForTimeNow).finalConfig.control =
      .returned responseFiveValue ∧
    (runBaseHostFuel 1 [] waitingForTimeNow).finalConfig.control =
      .awaitingHost .timeNow := by
  constructor <;> rfl

theorem response_order_changes_waiting_final_state :
    (runBaseHostFuel 1 [responseFive, responseTwo] waitingForTimeNow).finalConfig ≠
      (runBaseHostFuel 1 [responseTwo, responseFive] waitingForTimeNow).finalConfig := by
  intro equal
  have controlsEqual := congrArg MachineConfig.control equal
  rw [response_order_changes_waiting_result.1,
    response_order_changes_waiting_result.2] at controlsEqual
  injection controlsEqual with valuesEqual
  exact responseValues_are_distinct valuesEqual

end Xsh.Proofs.BaseHostRunnerLaws
