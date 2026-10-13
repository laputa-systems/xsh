import Xsh.Semantics.BaseMachine

namespace Xsh.Proofs.BaseMachine

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Semantics.Environments
open Xsh.Semantics.BaseMachine

theorem accepted_initial_expression_is_wellFormed_fragment
    (names : LexicalEnvironment) (activations : ActivationEnvironment) (store : BaseStore)
    (expression : BaseExpr)
    (accepted : checkBaseExpr names activations store expression = true) :
    wellFormed (initialConfig names activations store expression) := by
  exact ⟨accepted, rfl, rfl⟩

theorem integer_checker_implies_supported_checker_fragment
    (names : LexicalEnvironment) (activations : ActivationEnvironment) (store : BaseStore)
    (expression : BaseExpr)
    (accepted : checkIntegerExpr names activations store expression = true) :
    checkBaseExpr names activations store expression = true := by
  cases expression with
  | integerLiteral value =>
    cases found : validateInt? value <;>
      simp [checkIntegerExpr, checkBaseExpr, inferBaseType?, found] at accepted ⊢
  | scalarLiteral value =>
    cases found : scalarTypedValue? value with
    | none => simp [checkIntegerExpr, inferBaseType?, found] at accepted
    | some typed =>
      cases typed.typeTag <;>
        simp [checkIntegerExpr, checkBaseExpr, inferBaseType?, found] at accepted ⊢
  | timeNow => rfl
  | identifier name =>
    cases found : lookupBaseValue names activations store name with
    | error fault => simp [checkIntegerExpr, inferBaseType?, found] at accepted
    | ok typed =>
      change (lookupBaseValue names activations store name).isOk = true
      rw [found]
      rfl
  | negate operand =>
    cases found : inferBaseType? names activations store operand with
    | none => simp [checkIntegerExpr, inferBaseType?, found] at accepted
    | some type =>
      cases type <;>
        simp [checkIntegerExpr, checkBaseExpr, inferBaseType?, found] at accepted ⊢
  | binary operator left right =>
    cases leftType : inferBaseType? names activations store left with
    | none => simp [checkIntegerExpr, inferBaseType?, leftType] at accepted
    | some leftTag =>
      cases rightType : inferBaseType? names activations store right with
      | none => simp [checkIntegerExpr, inferBaseType?, leftType, rightType] at accepted
      | some rightTag =>
        cases leftTag <;> cases rightTag <;>
          simp [checkIntegerExpr, checkBaseExpr, inferBaseType?, leftType, rightType] at accepted ⊢

private theorem typed_integer_payload_is_present (value : BaseTypedValue)
    (integerTag : value.typeTag = .integer) :
    (integerPayload? value).isSome = true := by
  cases value with
  | mk typeTag payload wellTyped =>
    cases typeTag with
    | null => simp at integerTag
    | boolean => simp at integerTag
    | integer =>
      cases payload with
      | null => simp [hasBaseType] at wellTyped
      | boolean _ => simp [hasBaseType] at wellTyped
      | integer _ => rfl
      | float _ => simp [hasBaseType] at wellTyped
      | duration _ => simp [hasBaseType] at wellTyped
      | string _ => simp [hasBaseType] at wellTyped
      | bytes _ => simp [hasBaseType] at wellTyped
      | list _ => simp [hasBaseType] at wellTyped
      | record _ => simp [hasBaseType] at wellTyped
      | tagged _ _ _ => simp [hasBaseType] at wellTyped
    | float => simp at integerTag
    | duration => simp at integerTag
    | string => simp at integerTag
    | bytes => simp at integerTag

private theorem checked_host_response_has_requested_type
    {responseType : BaseType} (request : BaseHostRequest responseType)
    (response : BaseHostResponse) (value : BaseTypedValue)
    (accepted : checkedHostResponse? request response = some value) :
    value.typeTag = responseType := by
  unfold checkedHostResponse? at accepted
  split at accepted
  · cases accepted
    assumption
  · contradiction

theorem machine_progress_fragment (config : MachineConfig) :
    terminalOutcome config ≠ none ∨ (∃ next, step config = some next) ∨
      ∃ waiting : BaseHostWait, pendingHostWait? config = some waiting := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression =>
      cases expression <;> simp [terminalOutcome, step]
    | returned value =>
      cases continuation with
      | nil => simp [terminalOutcome]
      | cons frame rest =>
        cases frame <;> simp [terminalOutcome, step]
    | awaitingHost request =>
      exact Or.inr (Or.inr ⟨⟨_, request, ⟨names, activations, store,
        .awaitingHost request, continuation⟩, rfl⟩, rfl⟩)
    | failed fault => simp [terminalOutcome]

theorem wellFormed_preserved_fragment {config next : MachineConfig}
    (wellFormedConfig : wellFormed config) (takesStep : step config = some next) :
    wellFormed next := by
  cases config with
  | mk names activations store control continuation =>
    simp [wellFormed] at wellFormedConfig
    rcases wellFormedConfig with ⟨controlWellFormed, continuationWellFormed, controlFits⟩
    cases control with
    | evaluate expression =>
      cases expression with
      | integerLiteral value =>
        simp only [step] at takesStep
        cases takesStep
        cases result : checkedInt value <;>
          cases continuation <;>
            simp_all [wellFormed, updateState, controlOfIntegerResult,
              controlFitsContinuation, integerPayload?, typedInteger]
      | scalarLiteral value =>
        cases continuation with
        | nil =>
          simp only [step] at takesStep
          cases takesStep
          cases result : scalarTypedValue? value <;>
            simp_all [wellFormed, updateState, controlFitsContinuation]
        | cons frame rest =>
          have expressionInteger :
              checkIntegerExpr names activations store (.scalarLiteral value) = true := by
            simpa [controlFitsContinuation] using controlFits
          simp only [step] at takesStep
          cases takesStep
          cases result : scalarTypedValue? value with
          | none => simp_all [wellFormed, updateState, controlFitsContinuation]
          | some typed =>
            cases type : typed.typeTag with
            | integer =>
              have typedPayloadPresent := typed_integer_payload_is_present typed type
              simp_all [wellFormed, updateState, controlFitsContinuation, integerPayload?,
                checkIntegerExpr, inferBaseType?]
            | null | boolean | float | duration | string | bytes =>
              simp_all [controlFitsContinuation, checkIntegerExpr, inferBaseType?]
      | timeNow =>
        simp only [step] at takesStep
        cases takesStep
        cases continuation <;>
          simp_all [wellFormed, updateState, controlFitsContinuation,
            BaseHostRequest.responseType]
      | identifier name =>
        cases continuation with
        | nil =>
          simp only [step] at takesStep
          cases takesStep
          cases result : lookupBaseValue names activations store name <;>
            simp_all [wellFormed, updateState, controlFitsContinuation]
        | cons frame rest =>
          have expressionInteger :
              checkIntegerExpr names activations store (.identifier name) = true := by
            simpa [controlFitsContinuation] using controlFits
          simp only [step] at takesStep
          cases takesStep
          cases result : lookupBaseValue names activations store name with
          | error fault => simp_all [wellFormed, updateState, controlFitsContinuation]
          | ok typed =>
            cases type : typed.typeTag with
            | integer =>
              have typedPayloadPresent := typed_integer_payload_is_present typed type
              simp_all [wellFormed, updateState, controlFitsContinuation, integerPayload?,
                checkIntegerExpr, inferBaseType?]
            | null | boolean | float | duration | string | bytes =>
              simp_all [controlFitsContinuation, checkIntegerExpr, inferBaseType?]
      | negate operand =>
        have operandInteger : checkIntegerExpr names activations store operand = true := by
          simpa [checkBaseExpr] using controlWellFormed
        have operandChecked := integer_checker_implies_supported_checker_fragment
          names activations store operand operandInteger
        simp only [step] at takesStep
        cases takesStep
        simp [wellFormed, updateState, continuationChecked, controlFitsContinuation,
          operandChecked, operandInteger, continuationWellFormed]
      | binary operator left right =>
        have operandsInteger :
            checkIntegerExpr names activations store left = true ∧
              checkIntegerExpr names activations store right = true := by
          simpa [checkBaseExpr, Bool.and_eq_true] using controlWellFormed
        have leftChecked := integer_checker_implies_supported_checker_fragment
          names activations store left operandsInteger.1
        simp only [step] at takesStep
        cases takesStep
        simp [wellFormed, updateState, continuationChecked, controlFitsContinuation,
          leftChecked, operandsInteger, continuationWellFormed]
    | returned value =>
      cases continuation with
      | nil => simp [step] at takesStep
      | cons frame rest =>
        cases frame with
        | negate =>
          have restWellFormed : continuationChecked names activations store rest = true := by
            simpa [continuationChecked] using continuationWellFormed
          simp only [step] at takesStep
          cases takesStep
          cases payload : integerPayload? value with
          | none =>
            simp_all [controlFitsContinuation]
          | some integer =>
            cases result : IntegerOps.negate integer <;>
              cases rest <;>
                simp_all [wellFormed, updateState, controlOfIntegerResult,
                  controlFitsContinuation, integerPayload?, typedInteger]
        | binaryLeft operator right =>
          have continuationParts :
              checkIntegerExpr names activations store right = true ∧
                continuationChecked names activations store rest = true := by
            simpa [continuationChecked, Bool.and_eq_true] using continuationWellFormed
          have rightInteger := continuationParts.1
          have restWellFormed := continuationParts.2
          have rightChecked := integer_checker_implies_supported_checker_fragment
            names activations store right rightInteger
          simp only [step] at takesStep
          cases takesStep
          cases payload : integerPayload? value <;>
            simp_all [wellFormed, updateState, continuationChecked,
              controlFitsContinuation]
        | binaryRight operator left =>
          have restWellFormed : continuationChecked names activations store rest = true := by
            simpa [continuationChecked] using continuationWellFormed
          simp only [step] at takesStep
          cases takesStep
          cases payload : integerPayload? value with
          | none =>
            simp_all [controlFitsContinuation]
          | some integer =>
            cases result : applyBinary operator left integer <;>
              cases rest <;>
                simp_all [wellFormed, updateState, controlOfIntegerResult,
                  controlFitsContinuation, integerPayload?, typedInteger]
    | awaitingHost request => simp [step] at takesStep
    | failed fault => simp [step] at takesStep

theorem resumeHostRequest_preserved_fragment {config next : MachineConfig}
    {response : BaseHostResponse}
    (wellFormedConfig : wellFormed config)
    (resumed : resumeHostRequest config response = some next) :
    wellFormed next := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression => simp [resumeHostRequest] at resumed
    | returned value => simp [resumeHostRequest] at resumed
    | failed fault => simp [resumeHostRequest] at resumed
    | awaitingHost request =>
      cases request with
      | timeNow =>
        have continuationWellFormed :
            continuationChecked names activations store continuation = true :=
          wellFormedConfig.2.1
        simp only [resumeHostRequest] at resumed
        cases accepted : checkedHostResponse? BaseHostRequest.timeNow response with
        | none =>
          simp [accepted] at resumed
          cases resumed
          cases continuation <;>
            simp_all [wellFormed, updateState, controlFitsContinuation]
        | some value =>
          simp [accepted] at resumed
          cases resumed
          have valueHasIntegerTag := checked_host_response_has_requested_type
            BaseHostRequest.timeNow response value accepted
          have valueHasIntegerPayload := typed_integer_payload_is_present value valueHasIntegerTag
          cases continuation <;>
            simp_all [wellFormed, updateState, controlFitsContinuation, integerPayload?]

theorem machine_runner_never_stuck_fragment {fuel : Nat} {config : MachineConfig}
    (wellFormedConfig : wellFormed config) :
    runFuel fuel config ≠ .internalStuck := by
  induction fuel generalizing config with
  | zero =>
    unfold runFuel
    cases terminal : terminalOutcome config with
    | some outcome => simp
    | none =>
      cases pending : pendingHostWait? config with
      | some waiting => simp
      | none => simp
  | succ fuel inductionHypothesis =>
    unfold runFuel
    cases terminal : terminalOutcome config with
    | some outcome => simp
    | none =>
      have notFinal : ¬ terminalOutcome config ≠ none := by simp [terminal]
      cases pending : pendingHostWait? config with
      | some waiting => simp
      | none =>
        have notRequest :
            ¬ ∃ waiting : BaseHostWait, pendingHostWait? config = some waiting := by
          intro request
          rcases request with ⟨waiting, isPending⟩
          rw [pending] at isPending
          cases isPending
        obtain ⟨next, takesStep⟩ :=
          (machine_progress_fragment config).resolve_left notFinal |>.resolve_right notRequest
        rw [takesStep]
        simpa [pending] using
          inductionHypothesis (wellFormed_preserved_fragment wellFormedConfig takesStep)

theorem runner_soundness_fragment {fuel : Nat} {config : MachineConfig}
    {outcome : BaseOutcome}
    (completed : runFuel fuel config = .completed outcome) :
    ∃ used final, used ≤ fuel ∧ MachineSteps used config final ∧
      terminalOutcome final = some outcome := by
  induction fuel generalizing config outcome with
  | zero =>
    unfold runFuel at completed
    cases terminal : terminalOutcome config with
    | some result =>
      simp [terminal] at completed
      subst outcome
      exact ⟨0, config, Nat.zero_le _, .refl config, terminal⟩
    | none =>
      cases pending : pendingHostWait? config with
      | some waiting => simp [terminal, pending] at completed
      | none => simp [terminal, pending] at completed
  | succ fuel inductionHypothesis =>
    unfold runFuel at completed
    cases terminal : terminalOutcome config with
    | some result =>
      simp [terminal] at completed
      subst outcome
      exact ⟨0, config, Nat.zero_le _, .refl config, terminal⟩
    | none =>
      cases pending : pendingHostWait? config with
      | some waiting => simp [terminal, pending] at completed
      | none =>
        cases takesStep : step config with
        | none => simp [terminal, pending, takesStep] at completed
        | some next =>
          simp [terminal, pending, takesStep] at completed
          obtain ⟨used, final, withinFuel, steps, isFinal⟩ := inductionHypothesis completed
          exact ⟨used + 1, final, Nat.succ_le_succ withinFuel, .step takesStep steps, isFinal⟩

private def exampleSmall (value : Int)
    (inRange : signed64Min ≤ value ∧ value < signed64Limit) : IntValue :=
  ⟨value, inRange⟩

private def exampleTwo : IntValue := exampleSmall 2 (by unfold signed64Min signed64Limit; omega)
private def exampleThree : IntValue := exampleSmall 3 (by unfold signed64Min signed64Limit; omega)
private def exampleFive : IntValue := exampleSmall 5 (by unfold signed64Min signed64Limit; omega)
private def exampleSix : IntValue := exampleSmall 6 (by unfold signed64Min signed64Limit; omega)
private def exampleEleven : IntValue := exampleSmall 11 (by unfold signed64Min signed64Limit; omega)
private def exampleTwentyTwo : IntValue := exampleSmall 22 (by unfold signed64Min signed64Limit; omega)

def acceptedArithmeticExample : BaseExpr :=
  .binary .add (.integerLiteral 2) (.integerLiteral 3)

theorem integer_checker_accepts_supported_arithmetic_fragment :
    checkBaseExpr [] [] (fun _ => none) acceptedArithmeticExample = true := by
  decide

theorem integer_checker_rejects_out_of_range_literal_fragment :
    checkBaseExpr [] [] (fun _ => none) (.integerLiteral signed64Limit) = false := by
  decide

private def missingName : NameId := ⟨⟨19, by decide⟩⟩

theorem integer_checker_rejects_unbound_name_fragment :
    checkBaseExpr [] [] (fun _ => none) (.identifier missingName) = false := by
  decide

theorem supported_arithmetic_runs_to_exact_value_fragment :
    runFuel 5 (initialConfig [] [] (fun _ => none) acceptedArithmeticExample) =
      .completed (.value (typedInteger exampleFive)) := by
  rfl

def overflowExample : BaseExpr :=
  .binary .multiply (.integerLiteral intMax.val) (.integerLiteral 2)

theorem checked_multiplication_overflow_is_explicit_fragment :
    runFuel 5 (initialConfig [] [] (fun _ => none) overflowExample) =
      .completed (.fault (.arithmetic .overflow)) := by
  rfl

def divisionByZeroExample : BaseExpr :=
  .binary .divide (.integerLiteral 1) (.integerLiteral 0)

theorem checked_division_by_zero_is_explicit_fragment :
    runFuel 5 (initialConfig [] [] (fun _ => none) divisionByZeroExample) =
      .completed (.fault (.arithmetic .divisionByZero)) := by
  rfl

def arithmeticMutantMultiply (left right : IntValue) : IntegerResult :=
  applyBinary .add left right

theorem multiplication_mutant_witness_fragment :
    applyBinary .multiply exampleTwo exampleThree = .value exampleSix ∧
      arithmeticMutantMultiply exampleTwo exampleThree = .value exampleFive ∧
      exampleSix ≠ exampleFive := by
  decide

private def positiveZeroTypedValue : BaseTypedValue :=
  ⟨.float, positiveZeroValue, trivial⟩

private def negativeZeroTypedValue : BaseTypedValue :=
  ⟨.float, negativeZeroValue, trivial⟩

private def typedNullValue : BaseTypedValue := ⟨.null, .null, trivial⟩
private def typedBooleanValue : BaseTypedValue := ⟨.boolean, .boolean false, trivial⟩
private def typedIntegerValue : BaseTypedValue := typedInteger exampleFive
private def typedDurationValue : BaseTypedValue :=
  ⟨.duration, .duration ⟨37, by decide⟩, trivial⟩
private def typedStringValue : BaseTypedValue := ⟨.string, .string "xsh", trivial⟩
private def typedBytesValue : BaseTypedValue := ⟨.bytes, .bytes [], trivial⟩

private def oldWellFormedInvariant (config : MachineConfig) : Prop :=
  (match config.control with
   | .evaluate expression =>
     checkBaseExpr config.names config.activations config.store expression = true
   | .returned _ | .awaitingHost _ | .failed _ => True) ∧
  continuationChecked config.names config.activations config.store config.continuation = true

private def booleanUnderNegate : MachineConfig :=
  ⟨[], [], (fun _ => none), .returned typedBooleanValue, [.negate]⟩

private def floatUnderBinaryRight : MachineConfig :=
  ⟨[], [], (fun _ => none), .returned positiveZeroTypedValue,
    [.binaryRight .add exampleTwo]⟩

theorem pending_integer_frames_reject_noninteger_mutations_fragment :
    oldWellFormedInvariant booleanUnderNegate ∧
      oldWellFormedInvariant floatUnderBinaryRight ∧
      ¬ wellFormed booleanUnderNegate ∧
      ¬ wellFormed floatUnderBinaryRight ∧
      step booleanUnderNegate = some
        (updateState booleanUnderNegate (.failed .expectedInteger) []) ∧
      step floatUnderBinaryRight = some
        (updateState floatUnderBinaryRight (.failed .expectedInteger) []) := by
  simp [oldWellFormedInvariant, booleanUnderNegate, floatUnderBinaryRight,
    typedBooleanValue, positiveZeroTypedValue, oldWellFormedInvariant, wellFormed,
    controlFitsContinuation, continuationChecked, integerPayload?, step, updateState]

private def timeNowConfig : MachineConfig :=
  initialConfig [] [] (fun _ => none) .timeNow

private def timeNowWaitingConfig : MachineConfig :=
  updateState timeNowConfig (.awaitingHost .timeNow) []

private def timeNowIntegerResponse : BaseHostResponse := ⟨typedInteger exampleFive⟩
private def timeNowBooleanResponse : BaseHostResponse := ⟨typedBooleanValue⟩

theorem time_now_is_a_typed_host_request_fragment :
    checkBaseExpr [] [] (fun _ => none) .timeNow = true ∧
      inferBaseType? [] [] (fun _ => none) .timeNow = some .integer ∧
      runFuel 1 timeNowConfig = .hostRequest
        ⟨BaseType.integer, BaseHostRequest.timeNow, timeNowWaitingConfig, rfl⟩ := by
  exact ⟨rfl, rfl, rfl⟩

theorem time_now_accepts_only_an_integer_host_response_fragment :
    checkedHostResponse? BaseHostRequest.timeNow timeNowIntegerResponse =
        some (typedInteger exampleFive) ∧
      checkedHostResponse? BaseHostRequest.timeNow timeNowBooleanResponse = none ∧
      resumeHostRequest timeNowWaitingConfig timeNowIntegerResponse =
        some (updateState timeNowWaitingConfig (.returned (typedInteger exampleFive)) []) ∧
      resumeHostRequest timeNowWaitingConfig timeNowBooleanResponse =
        some (updateState timeNowWaitingConfig (.failed .hostResponseTypeMismatch) []) := by
  exact ⟨rfl, rfl, rfl, rfl⟩

private def uncheckedHostResponseMutant? (_request : BaseHostRequest .integer)
    (response : BaseHostResponse) : Option BaseTypedValue := some response.value

theorem boolean_response_type_mutation_is_rejected_fragment :
    uncheckedHostResponseMutant? BaseHostRequest.timeNow timeNowBooleanResponse =
        some typedBooleanValue ∧
      checkedHostResponse? BaseHostRequest.timeNow timeNowBooleanResponse = none ∧
      runFuel 0 (updateState timeNowWaitingConfig
        (.failed .hostResponseTypeMismatch) []) =
        .completed (.fault .hostResponseTypeMismatch) := by
  exact ⟨rfl, rfl, rfl⟩

private def twoClockExpression : BaseExpr := .binary .add .timeNow .timeNow

private def twoClockInitialConfig : MachineConfig :=
  initialConfig [] [] (fun _ => none) twoClockExpression

private def firstClockWaitingConfig : MachineConfig :=
  updateState twoClockInitialConfig (.awaitingHost .timeNow)
    [.binaryLeft .add .timeNow]

private def afterFirstClockResponseConfig : MachineConfig :=
  updateState firstClockWaitingConfig (.returned (typedInteger exampleTwo))
    [.binaryLeft .add .timeNow]

private def evaluateSecondClockConfig : MachineConfig :=
  updateState afterFirstClockResponseConfig (.evaluate .timeNow)
    [.binaryRight .add exampleTwo]

private def secondClockWaitingConfig : MachineConfig :=
  updateState evaluateSecondClockConfig (.awaitingHost .timeNow)
    [.binaryRight .add exampleTwo]

theorem host_requests_are_observed_in_machine_order_fragment :
    runFuel 2 twoClockInitialConfig = .hostRequest
      ⟨BaseType.integer, BaseHostRequest.timeNow, firstClockWaitingConfig, rfl⟩ ∧
      resumeHostRequest firstClockWaitingConfig ⟨typedInteger exampleTwo⟩ =
        some afterFirstClockResponseConfig ∧
      step afterFirstClockResponseConfig = some evaluateSecondClockConfig ∧
      step evaluateSecondClockConfig = some secondClockWaitingConfig ∧
      runFuel 0 secondClockWaitingConfig = .hostRequest
        ⟨BaseType.integer, BaseHostRequest.timeNow, secondClockWaitingConfig, rfl⟩ := by
  exact ⟨rfl, rfl, rfl, rfl, rfl⟩

theorem scalar_literal_runs_to_its_typed_payload_fragment
    (value : CompositeValue) (typed : BaseTypedValue)
    (supported : scalarTypedValue? value = some typed) :
    runFuel 1 (initialConfig [] [] (fun _ => none) (.scalarLiteral value)) =
      .completed (.value typed) := by
  simp [runFuel, terminalOutcome, pendingHostWait?, initialConfig, step, updateState, supported]

theorem scalar_checker_accepts_exact_float_literal_fragment :
    checkBaseExpr [] [] (fun _ => none) (.scalarLiteral positiveZeroValue) = true := by
  decide

theorem positive_zero_literal_runs_with_exact_float_payload_fragment :
    runFuel 1 (initialConfig [] [] (fun _ => none)
      (.scalarLiteral positiveZeroValue)) =
        .completed (.value positiveZeroTypedValue) := by
  apply scalar_literal_runs_to_its_typed_payload_fragment
  rfl

theorem every_supported_scalar_kind_runs_with_exact_typed_payload_fragment :
    runFuel 1 (initialConfig [] [] (fun _ => none) (.scalarLiteral .null)) =
        .completed (.value typedNullValue) ∧
      runFuel 1 (initialConfig [] [] (fun _ => none) (.scalarLiteral (.boolean false))) =
        .completed (.value typedBooleanValue) ∧
      runFuel 1 (initialConfig [] [] (fun _ => none)
        (.scalarLiteral (.integer exampleFive))) =
        .completed (.value typedIntegerValue) ∧
      runFuel 1 (initialConfig [] [] (fun _ => none)
        (.scalarLiteral positiveZeroValue)) =
        .completed (.value positiveZeroTypedValue) ∧
      runFuel 1 (initialConfig [] [] (fun _ => none)
        (.scalarLiteral (.duration ⟨37, by decide⟩))) =
        .completed (.value typedDurationValue) ∧
      runFuel 1 (initialConfig [] [] (fun _ => none) (.scalarLiteral (.string "xsh"))) =
        .completed (.value typedStringValue) ∧
      runFuel 1 (initialConfig [] [] (fun _ => none) (.scalarLiteral (.bytes []))) =
        .completed (.value typedBytesValue) := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  all_goals
    apply scalar_literal_runs_to_its_typed_payload_fragment
    rfl

theorem scalar_checker_rejects_composite_literal_fragment :
    checkBaseExpr [] [] (fun _ => none) (.scalarLiteral (.list [])) = false := by
  decide

theorem composite_literal_failure_is_explicit_fragment :
    runFuel 1 (initialConfig [] [] (fun _ => none) (.scalarLiteral (.list []))) =
      .completed (.fault .unsupportedScalarLiteral) := by
  rfl

theorem float_sign_bit_mutation_changes_machine_payload_fragment :
    runFuel 1 (initialConfig [] [] (fun _ => none) (.scalarLiteral positiveZeroValue)) =
        .completed (.value positiveZeroTypedValue) ∧
      runFuel 1 (initialConfig [] [] (fun _ => none) (.scalarLiteral negativeZeroValue)) =
        .completed (.value negativeZeroTypedValue) ∧
      ¬ representationIdentity positiveZeroTypedValue.value negativeZeroTypedValue.value ∧
      positiveZeroTypedValue.value.semanticEq negativeZeroTypedValue.value := by
  constructor
  · exact positive_zero_literal_runs_with_exact_float_payload_fragment
  constructor
  · apply scalar_literal_runs_to_its_typed_payload_fragment
    rfl
  constructor
  · simpa [positiveZeroTypedValue, negativeZeroTypedValue] using signed_zero_representation_differs
  · simpa [positiveZeroTypedValue, negativeZeroTypedValue] using signed_zero_semantically_equal

private def scalarName : NameId := ⟨⟨31, by decide⟩⟩
private def scalarBinder : BinderId := ⟨⟨32, by decide⟩⟩
private def scalarCellId : CellId := ⟨1, scalarBinder⟩

private def scalarStoreCell : StoreCell BaseType CompositeValue hasBaseType := {
  contents := {
    typeTag := .float
    value := positiveZeroValue
    wellTyped := trivial
  }
  mutability := .immutable
}

private def scalarNames : LexicalEnvironment :=
  bind emptyLexicalEnvironment scalarName scalarBinder

private def scalarActivations : ActivationEnvironment :=
  bindCell [] scalarBinder scalarCellId

private def scalarStore : BaseStore := fun cell =>
  if cell = scalarCellId then some scalarStoreCell else none

theorem typed_identifier_reads_exact_scalar_from_store_fragment :
    checkBaseExpr scalarNames scalarActivations scalarStore (.identifier scalarName) = true ∧
      runFuel 1 (initialConfig scalarNames scalarActivations scalarStore
        (.identifier scalarName)) = .completed (.value positiveZeroTypedValue) := by
  constructor <;> rfl

private def redirectedName : NameId := ⟨⟨7, by decide⟩⟩
private def declaredBinder : BinderId := ⟨⟨11, by decide⟩⟩
private def redirectedBinder : BinderId := ⟨⟨12, by decide⟩⟩
private def declaredCell : CellId := ⟨0, declaredBinder⟩
private def redirectedCell : CellId := ⟨0, redirectedBinder⟩

private def integerCell (value : IntValue) : StoreCell BaseType CompositeValue hasBaseType := {
  contents := {
    typeTag := .integer
    value := .integer value
    wellTyped := trivial
  }
  mutability := .mutable
}

private def redirectionStore : BaseStore := fun cell =>
  if cell = declaredCell then some (integerCell exampleEleven)
  else if cell = redirectedCell then some (integerCell exampleTwentyTwo)
  else none

private def redirectionActivations : ActivationEnvironment :=
  bindCell (bindCell [] declaredBinder declaredCell) redirectedBinder redirectedCell

private def declaredNames : LexicalEnvironment :=
  bind emptyLexicalEnvironment redirectedName declaredBinder

private def mutatedNames : LexicalEnvironment :=
  bind emptyLexicalEnvironment redirectedName redirectedBinder

private def redirectionExpression : BaseExpr := .identifier redirectedName

theorem binder_redirection_changes_identifier_result_fragment :
    checkBaseExpr declaredNames redirectionActivations redirectionStore
      redirectionExpression = true ∧
    checkBaseExpr mutatedNames redirectionActivations redirectionStore
      redirectionExpression = true ∧
    runFuel 1 (initialConfig declaredNames redirectionActivations redirectionStore
      redirectionExpression) = .completed (.value (typedInteger exampleEleven)) ∧
    runFuel 1 (initialConfig mutatedNames redirectionActivations redirectionStore
      redirectionExpression) = .completed (.value (typedInteger exampleTwentyTwo)) ∧
    exampleEleven ≠ exampleTwentyTwo := by
  constructor
  · rfl
  constructor
  · rfl
  constructor
  · rfl
  constructor
  · rfl
  · decide

end Xsh.Proofs.BaseMachine
