import Xsh.Semantics.BaseMachine

namespace Xsh.Semantics.BaseHostTrace

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Semantics.BaseMachine

/-- Events observable at the boundary between the base machine and its host. -/
inductive BaseHostEvent where
  | internal
  | request {responseType : BaseType} (request : BaseHostRequest responseType)
  | response {responseType : BaseType} (request : BaseHostRequest responseType)
      (response : BaseHostResponse)

/-- A machine edge labels ordinary evaluation, a yielded request, or a checked host answer. -/
inductive BaseHostTransition : MachineConfig → BaseHostEvent → MachineConfig → Prop where
  | internal {config next : MachineConfig}
      (notRequest : config.control ≠ .evaluate .timeNow)
      (takesStep : step config = some next) :
      BaseHostTransition config .internal next
  | request (config : MachineConfig)
      (evaluating : config.control = .evaluate .timeNow) :
      BaseHostTransition config (.request BaseHostRequest.timeNow)
        (updateState config (.awaitingHost BaseHostRequest.timeNow) config.continuation)
  | acceptedResponse {config : MachineConfig} {responseType : BaseType}
      {request : BaseHostRequest responseType} {response : BaseHostResponse}
      {value : BaseTypedValue}
      (waiting : config.control = .awaitingHost request)
      (checked : checkedHostResponse? request response = some value) :
      BaseHostTransition config (.response request response)
        (updateState config (.returned value) config.continuation)
  | rejectedResponse {config : MachineConfig} {responseType : BaseType}
      {request : BaseHostRequest responseType} {response : BaseHostResponse}
      (waiting : config.control = .awaitingHost request)
      (rejected : checkedHostResponse? request response = none) :
      BaseHostTransition config (.response request response)
        (updateState config (.failed .hostResponseTypeMismatch) config.continuation)

theorem request_transition_matches_machine_step (config : MachineConfig)
    (evaluating : config.control = .evaluate .timeNow) :
    step config = some
      (updateState config (.awaitingHost BaseHostRequest.timeNow) config.continuation) := by
  cases config with
  | mk names activations store control continuation =>
    have controlIsRequest : control = .evaluate .timeNow := evaluating
    subst control
    rfl

theorem accepted_response_transition_matches_resume {config : MachineConfig}
    {responseType : BaseType} {request : BaseHostRequest responseType}
    {response : BaseHostResponse} {value : BaseTypedValue}
    (waiting : config.control = .awaitingHost request)
    (checked : checkedHostResponse? request response = some value) :
    resumeHostRequest config response =
      some (updateState config (.returned value) config.continuation) := by
  cases config with
  | mk names activations store control continuation =>
    have controlIsWaiting : control = .awaitingHost request := waiting
    subst control
    simp [resumeHostRequest, checked]

theorem rejected_response_transition_matches_resume {config : MachineConfig}
    {responseType : BaseType} {request : BaseHostRequest responseType}
    {response : BaseHostResponse}
    (waiting : config.control = .awaitingHost request)
    (rejected : checkedHostResponse? request response = none) :
    resumeHostRequest config response =
      some (updateState config (.failed .hostResponseTypeMismatch) config.continuation) := by
  cases config with
  | mk names activations store control continuation =>
    have controlIsWaiting : control = .awaitingHost request := waiting
    subst control
    simp [resumeHostRequest, rejected]

/-- A finite sequence of labeled machine edges, including host resumption. -/
inductive BaseHostTrace : MachineConfig → List BaseHostEvent → MachineConfig → Prop where
  | refl (config : MachineConfig) : BaseHostTrace config [] config
  | cons {start next finish : MachineConfig} {event : BaseHostEvent}
      {events : List BaseHostEvent}
      (edge : BaseHostTransition start event next)
      (rest : BaseHostTrace next events finish) :
      BaseHostTrace start (event :: events) finish

/-- The final machine configuration and host events produced by a scripted run. -/
structure BaseHostRun where
  finalConfig : MachineConfig
  events : List BaseHostEvent

/-- Runs against a finite host script. Fuel counts internal steps, request yields, and consumed
responses. Missing responses and exhausted fuel stop at the current configuration. Clock accuracy
and all concrete host behavior remain external assumptions. -/
def runBaseHostFuel : (fuel : Nat) → List BaseHostResponse →
    MachineConfig → BaseHostRun
  | 0, _, config => ⟨config, []⟩
  | fuel + 1, responses, config =>
    match config.control with
    | .awaitingHost request =>
      match responses with
      | [] => ⟨config, []⟩
      | response :: remaining =>
        match checkedHostResponse? request response with
        | some value =>
          let next := updateState config (.returned value) config.continuation
          let rest := runBaseHostFuel fuel remaining next
          ⟨rest.finalConfig, .response request response :: rest.events⟩
        | none =>
          let next := updateState config (.failed .hostResponseTypeMismatch) config.continuation
          let rest := runBaseHostFuel fuel remaining next
          ⟨rest.finalConfig, .response request response :: rest.events⟩
    | .failed _ => ⟨config, []⟩
    | .evaluate .timeNow =>
      let next := updateState config (.awaitingHost BaseHostRequest.timeNow) config.continuation
      let rest := runBaseHostFuel fuel responses next
      ⟨rest.finalConfig, .request BaseHostRequest.timeNow :: rest.events⟩
    | _ =>
      match step config with
      | none => ⟨config, []⟩
      | some next =>
        let rest := runBaseHostFuel fuel responses next
        ⟨rest.finalConfig, .internal :: rest.events⟩

theorem checked_response_has_requested_type {responseType : BaseType}
    (request : BaseHostRequest responseType) (response : BaseHostResponse)
    (value : BaseTypedValue)
    (checked : checkedHostResponse? request response = some value) :
    value.typeTag = responseType := by
  cases request with
  | timeNow =>
    unfold checkedHostResponse? at checked
    split at checked
    · cases checked
      assumption
    · contradiction

private theorem integer_tag_has_integer_payload (value : BaseTypedValue)
    (tag : value.typeTag = .integer) : (integerPayload? value).isSome = true := by
  cases value with
  | mk typeTag payload wellTyped =>
    cases typeTag with
    | null | boolean | float | duration | string | bytes => simp at tag
    | integer =>
      cases payload <;> simp [hasBaseType, integerPayload?] at wellTyped ⊢

theorem accepted_host_response_preserves_wellFormed
    {config : MachineConfig} {responseType : BaseType}
    {request : BaseHostRequest responseType} {response : BaseHostResponse}
    {value : BaseTypedValue}
    (wellFormedConfig : wellFormed config)
    (waiting : config.control = .awaitingHost request)
    (checked : checkedHostResponse? request response = some value) :
    wellFormed (updateState config (.returned value) config.continuation) := by
  cases request with
  | timeNow =>
    have valueInteger : value.typeTag = .integer :=
      checked_response_has_requested_type .timeNow response value checked
    have integerPayload := integer_tag_has_integer_payload value valueInteger
    cases config with
    | mk names activations store control continuation =>
      have controlWaiting : control = .awaitingHost .timeNow := waiting
      subst control
      simp only [wellFormed] at wellFormedConfig
      rcases wellFormedConfig with ⟨_, continuationValid, _⟩
      simp only [wellFormed, updateState]
      refine ⟨trivial, continuationValid, ?_⟩
      cases continuation <;> simp [controlFitsContinuation, integerPayload]

theorem rejected_host_response_preserves_wellFormed
    {config : MachineConfig} {responseType : BaseType}
    {request : BaseHostRequest responseType}
    (wellFormedConfig : wellFormed config)
    (waiting : config.control = .awaitingHost request) :
    wellFormed (updateState config (.failed .hostResponseTypeMismatch) config.continuation) := by
  cases config with
  | mk names activations store control continuation =>
    simp only [wellFormed] at wellFormedConfig
    rcases wellFormedConfig with ⟨_, continuationValid, _⟩
    have controlWaiting : control = .awaitingHost request := waiting
    subst control
    simp only [wellFormed, updateState]
    refine ⟨trivial, continuationValid, ?_⟩
    cases continuation <;> rfl

theorem host_response_transition_preserves_wellFormed
    {config next : MachineConfig} {responseType : BaseType}
    {request : BaseHostRequest responseType} {response : BaseHostResponse}
    (wellFormedConfig : wellFormed config)
    (transition : BaseHostTransition config (.response request response) next) :
    wellFormed next := by
  cases transition with
  | acceptedResponse waiting checked =>
    exact accepted_host_response_preserves_wellFormed wellFormedConfig waiting checked
  | rejectedResponse waiting _ =>
    exact rejected_host_response_preserves_wellFormed wellFormedConfig waiting

private def twoInt : IntValue :=
  ⟨2, by unfold signed64Min signed64Limit; omega⟩

private def threeInt : IntValue :=
  ⟨3, by unfold signed64Min signed64Limit; omega⟩

private def fiveInt : IntValue :=
  ⟨5, by unfold signed64Min signed64Limit; omega⟩

private def traceTwo : BaseTypedValue := typedInteger twoInt
private def traceThree : BaseTypedValue := typedInteger threeInt
private def traceFive : BaseTypedValue := typedInteger fiveInt

private def twoRequestExpression : BaseExpr := .binary .add .timeNow .timeNow

private def twoRequestInitial : MachineConfig :=
  initialConfig [] [] (fun _ => none) twoRequestExpression

private def afterBinaryStart : MachineConfig :=
  updateState twoRequestInitial (.evaluate .timeNow) [.binaryLeft .add .timeNow]

private def afterFirstRequest : MachineConfig :=
  updateState afterBinaryStart (.awaitingHost .timeNow) [.binaryLeft .add .timeNow]

private def afterFirstResponse : MachineConfig :=
  updateState afterFirstRequest (.returned traceTwo) [.binaryLeft .add .timeNow]

private def beforeSecondRequest : MachineConfig :=
  updateState afterFirstResponse (.evaluate .timeNow) [.binaryRight .add twoInt]

private def afterSecondRequest : MachineConfig :=
  updateState beforeSecondRequest (.awaitingHost .timeNow) [.binaryRight .add twoInt]

private def afterSecondResponse : MachineConfig :=
  updateState afterSecondRequest (.returned traceThree) [.binaryRight .add twoInt]

private def afterAddition : MachineConfig :=
  updateState afterSecondResponse (.returned traceFive) []

private def scriptedResponses : List BaseHostResponse :=
  [⟨traceTwo⟩, ⟨traceThree⟩]

theorem scripted_two_request_trace_is_ordered :
    (runBaseHostFuel 7 scriptedResponses twoRequestInitial).finalConfig = afterAddition ∧
    (runBaseHostFuel 7 scriptedResponses twoRequestInitial).events =
      [.internal,
       .request BaseHostRequest.timeNow,
       .response BaseHostRequest.timeNow ⟨traceTwo⟩,
       .internal,
       .request BaseHostRequest.timeNow,
       .response BaseHostRequest.timeNow ⟨traceThree⟩,
       .internal] := by
  constructor <;> rfl

private theorem two_request_trace_witness :
    BaseHostTrace twoRequestInitial
      [.internal,
       .request BaseHostRequest.timeNow,
       .response BaseHostRequest.timeNow ⟨traceTwo⟩,
       .internal,
       .request BaseHostRequest.timeNow,
       .response BaseHostRequest.timeNow ⟨traceThree⟩,
       .internal]
      afterAddition := by
  apply BaseHostTrace.cons
  · apply BaseHostTransition.internal
    · simp [twoRequestInitial, twoRequestExpression, initialConfig]
    · rfl
  apply BaseHostTrace.cons
  · exact .request afterBinaryStart rfl
  apply BaseHostTrace.cons
  · exact .acceptedResponse rfl rfl
  apply BaseHostTrace.cons
  · apply BaseHostTransition.internal
    · change BaseControl.returned traceTwo ≠ .evaluate .timeNow
      intro same
      cases same
    · rfl
  apply BaseHostTrace.cons
  · exact .request beforeSecondRequest rfl
  apply BaseHostTrace.cons
  · exact .acceptedResponse rfl rfl
  apply BaseHostTrace.cons
  · apply BaseHostTransition.internal
    · change BaseControl.returned traceThree ≠ .evaluate .timeNow
      intro same
      cases same
    · rfl
  exact .refl afterAddition

theorem scripted_two_request_trace_is_valid :
    BaseHostTrace twoRequestInitial
      (runBaseHostFuel 7 scriptedResponses twoRequestInitial).events
      (runBaseHostFuel 7 scriptedResponses twoRequestInitial).finalConfig := by
  rw [(scripted_two_request_trace_is_ordered).2, (scripted_two_request_trace_is_ordered).1]
  exact two_request_trace_witness

private def booleanResponse : BaseHostResponse := ⟨⟨.boolean, .boolean false, trivial⟩⟩

theorem mismatched_response_is_labeled_and_rejected :
    checkedHostResponse? BaseHostRequest.timeNow booleanResponse = none ∧
      BaseHostTransition
        (updateState (initialConfig [] [] (fun _ => none) .timeNow)
          (.awaitingHost .timeNow) [])
        (.response BaseHostRequest.timeNow booleanResponse)
        (updateState (updateState (initialConfig [] [] (fun _ => none) .timeNow)
          (.awaitingHost .timeNow) []) (.failed .hostResponseTypeMismatch) []) := by
  exact ⟨rfl, .rejectedResponse rfl rfl⟩

end Xsh.Semantics.BaseHostTrace
