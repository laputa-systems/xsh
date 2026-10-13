import Xsh.Proofs.BaseMachine
import Xsh.Semantics.BaseHostTrace

namespace Xsh.Proofs.BaseHostTrace

open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace

/-- Every fuel-bounded scripted run records exactly a valid prefix of labeled host transitions.
Fuel exhaustion, a missing response, and machine failure end the trace at the current state. -/
theorem runBaseHostFuel_trace (fuel : Nat) (responses : List BaseHostResponse)
    (config : MachineConfig) :
    BaseHostTrace config (runBaseHostFuel fuel responses config).events
      (runBaseHostFuel fuel responses config).finalConfig := by
  induction fuel generalizing responses config with
  | zero =>
    change BaseHostTrace config [] config
    exact .refl config
  | succ fuel inductionHypothesis =>
    cases control : config.control with
    | awaitingHost request =>
      cases responses with
      | nil =>
        simp [runBaseHostFuel, control]
        exact .refl config
      | cons response remaining =>
        cases checked : checkedHostResponse? request response with
        | none =>
          have restTrace := inductionHypothesis remaining
            (updateState config (.failed .hostResponseTypeMismatch) config.continuation)
          simp only [runBaseHostFuel, control, checked]
          exact .cons (.rejectedResponse control checked) restTrace
        | some value =>
          have restTrace := inductionHypothesis remaining
            (updateState config (.returned value) config.continuation)
          simp only [runBaseHostFuel, control, checked]
          exact .cons (.acceptedResponse control checked) restTrace
    | failed fault =>
      simp [runBaseHostFuel, control]
      exact .refl config
    | evaluate expression =>
      cases expression with
      | timeNow =>
        have requestTrace := inductionHypothesis responses
          (updateState config (.awaitingHost .timeNow) config.continuation)
        simp only [runBaseHostFuel, control]
        exact .cons (.request config control) requestTrace
      | _ =>
        cases stepped : step config with
        | none =>
          simp [runBaseHostFuel, control, stepped]
          exact .refl config
        | some next =>
          have restTrace := inductionHypothesis responses next
          have notRequest : config.control ≠ .evaluate .timeNow := by
            rw [control]
            intro same
            cases same
          simp only [runBaseHostFuel, control, stepped]
          exact .cons (.internal notRequest stepped) restTrace
    | returned value =>
      cases stepped : step config with
      | none =>
        simp [runBaseHostFuel, control, stepped]
        exact .refl config
      | some next =>
        have restTrace := inductionHypothesis responses next
        have notRequest : config.control ≠ .evaluate .timeNow := by
          rw [control]
          intro same
          cases same
        simp only [runBaseHostFuel, control, stepped]
        exact .cons (.internal notRequest stepped) restTrace

private theorem baseHostTransition_preserves_wellFormed
    {start next : MachineConfig} {event : BaseHostEvent}
    (wellFormedStart : wellFormed start)
    (transition : BaseHostTransition start event next) :
    wellFormed next := by
  cases transition with
  | internal notRequest takesStep =>
    exact Xsh.Proofs.BaseMachine.wellFormed_preserved_fragment
      wellFormedStart takesStep
  | request evaluating =>
    exact Xsh.Proofs.BaseMachine.wellFormed_preserved_fragment
      wellFormedStart (request_transition_matches_machine_step start evaluating)
  | acceptedResponse waiting checked =>
    exact accepted_host_response_preserves_wellFormed wellFormedStart waiting checked
  | rejectedResponse waiting rejected =>
    exact rejected_host_response_preserves_wellFormed wellFormedStart waiting

/-- A valid host trace preserves the base-machine well-formedness invariant. -/
theorem trace_preserves_wellFormed
    {start finish : MachineConfig} {events : List BaseHostEvent}
    (trace : BaseHostTrace start events finish)
    (wellFormedStart : wellFormed start) :
    wellFormed finish := by
  induction trace with
  | refl _ => exact wellFormedStart
  | @cons start next finish event events transition rest inductionHypothesis =>
    exact inductionHypothesis
      (baseHostTransition_preserves_wellFormed wellFormedStart transition)

/-- Scripted host execution preserves well-formedness even when it stops at a fuel or script
boundary, because every recorded prefix ends at a well-formed machine state. -/
theorem runBaseHostFuel_preserves_wellFormed (fuel : Nat)
    (responses : List BaseHostResponse) (config : MachineConfig)
    (wellFormedConfig : wellFormed config) :
    wellFormed (runBaseHostFuel fuel responses config).finalConfig := by
  exact trace_preserves_wellFormed
    (runBaseHostFuel_trace fuel responses config) wellFormedConfig

private theorem empty_trace_implies_equal {start finish : MachineConfig}
    (trace : BaseHostTrace start [] finish) : start = finish := by
  cases trace
  rfl

/-- Removing a consumed mismatched response leaves no trace from the waiting state to failure. -/
theorem omitted_response_event_is_not_a_trace :
    ¬ BaseHostTrace
      (updateState (initialConfig [] [] (fun _ => none) .timeNow)
        (.awaitingHost .timeNow) [])
      []
      (updateState
        (updateState (initialConfig [] [] (fun _ => none) .timeNow)
          (.awaitingHost .timeNow) [])
        (.failed .hostResponseTypeMismatch) []) := by
  intro trace
  have equalStates := empty_trace_implies_equal trace
  have equalControls := congrArg MachineConfig.control equalStates
  simp [updateState] at equalControls

end Xsh.Proofs.BaseHostTrace
