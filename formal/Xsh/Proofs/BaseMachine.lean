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

theorem checker_accepts_every_typed_expression
    {names : LexicalEnvironment} {activations : ActivationEnvironment} {store : BaseStore}
    {expression : BaseExpr} {type : BaseType}
    (typed : inferBaseType? names activations store expression = some type) :
    checkBaseExpr names activations store expression = true := by
  cases expression with
  | integerLiteral value =>
    cases found : validateInt? value <;> simp_all [checkBaseExpr, inferBaseType?]
  | scalarLiteral value =>
    cases found : scalarTypedValue? value <;> simp_all [checkBaseExpr, inferBaseType?]
  | timeNow => rfl
  | identifier name =>
    cases found : lookupBaseValue names activations store name with
    | error fault => simp [inferBaseType?, found] at typed
    | ok value =>
      change (lookupBaseValue names activations store name).isOk = true
      rw [found]
      rfl
  | negate operand =>
    cases found : inferBaseType? names activations store operand with
    | none => simp [inferBaseType?, found] at typed
    | some operandType =>
      cases operandType <;> simp [inferBaseType?, found] at typed <;>
        simp [checkBaseExpr, checkIntegerExpr, found]
  | binary operator left right =>
    cases leftType : inferBaseType? names activations store left <;>
      cases rightType : inferBaseType? names activations store right <;>
        simp_all [checkIntegerExpr, checkBaseExpr, inferBaseType?]
    all_goals
      rename_i leftTag rightTag
      cases leftTag <;> cases rightTag <;> simp_all
  | compare operator left right => simp [checkBaseExpr, typed]
  | conditional condition thenBranch elseBranch => simp [checkBaseExpr, typed]
  | call parameters body arguments => simp [checkBaseExpr, typed]

theorem integer_checker_implies_supported_checker_fragment
    (names : LexicalEnvironment) (activations : ActivationEnvironment) (store : BaseStore)
    (expression : BaseExpr)
    (accepted : checkIntegerExpr names activations store expression = true) :
    checkBaseExpr names activations store expression = true := by
  unfold checkIntegerExpr at accepted
  cases typed : inferBaseType? names activations store expression with
  | none => simp [typed] at accepted
  | some type => exact checker_accepts_every_typed_expression typed

theorem typed_integer_payload_is_present (value : BaseTypedValue)
    (integerTag : value.typeTag = .integer) :
    (integerPayload? value).isSome = true := by
  cases value with
  | mk typeTag payload wellTyped =>
    cases typeTag <;> simp at integerTag
    cases payload <;> simp [hasBaseType, integerPayload?] at wellTyped ⊢

theorem typed_boolean_payload_is_present (value : BaseTypedValue)
    (booleanTag : value.typeTag = .boolean) :
    (booleanPayload? value).isSome = true := by
  cases value with
  | mk typeTag payload wellTyped =>
    cases typeTag <;> simp at booleanTag
    cases payload <;> simp [hasBaseType, booleanPayload?] at wellTyped ⊢

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

private theorem static_negate {ps : List Xsh.Syntax.RawName} {o : BaseExpr} {t : BaseType}
    (h : staticType ps (.negate o) = some t) : staticType ps o = some .integer ∧ t = .integer := by
  simp only [staticType] at h
  split at h <;> simp_all

private theorem static_binary {ps : List Xsh.Syntax.RawName} {op l r} {t : BaseType}
    (h : staticType ps (.binary op l r) = some t) :
    staticType ps l = some .integer ∧ staticType ps r = some .integer ∧ t = .integer := by
  simp only [staticType] at h
  split at h <;> simp_all

private theorem static_compare {ps : List Xsh.Syntax.RawName} {op l r} {t : BaseType}
    (h : staticType ps (.compare op l r) = some t) :
    staticType ps l = some .integer ∧ staticType ps r = some .integer ∧ t = .boolean := by
  simp only [staticType] at h
  split at h <;> simp_all

private theorem static_conditional {ps : List Xsh.Syntax.RawName} {c a b : BaseExpr} {t : BaseType}
    (h : staticType ps (.conditional c a b) = some t) :
    staticType ps c = some .boolean ∧ staticType ps a = some .integer ∧
      staticType ps b = some .integer ∧ t = .integer := by
  simp only [staticType] at h
  split at h <;> simp_all

private theorem static_call {ps inner : List Xsh.Syntax.RawName} {body : BaseExpr}
    {args : List BaseExpr} {t : BaseType}
    (h : staticType ps (.call inner body args) = some t) :
    inner.length = args.length ∧ staticArgumentsInteger ps args = true ∧
      staticType inner body = some .integer ∧ t = .integer := by
  simp only [staticType] at h
  split at h <;> simp_all


theorem length_substituteArguments (bindings : List (Xsh.Syntax.RawName × IntValue)) :
    ∀ args : List BaseExpr, (substituteArguments bindings args).length = args.length
  | [] => rfl
  | a :: rest => by simp [substituteArguments, length_substituteArguments bindings rest]

private theorem lookup_zip_of_mem {ps : List Xsh.Syntax.RawName} {vs : List IntValue}
    {name : Xsh.Syntax.RawName} (member : name ∈ ps) (same : ps.length = vs.length) :
    ∃ value, (ps.zip vs).lookup name = some value := by
  induction ps generalizing vs with
  | nil => simp at member
  | cons p rest ih =>
    cases vs with
    | nil => simp at same
    | cons v more =>
      simp only [List.length_cons, Nat.add_right_cancel_iff] at same
      simp only [List.zip_cons_cons, List.lookup_cons]
      by_cases hit : name = p
      · subst hit; exact ⟨v, by simp⟩
      · have : name ∈ rest := by
          simp only [List.mem_cons] at member
          exact member.resolve_left hit
        obtain ⟨found, h⟩ := ih this same
        exact ⟨found, by rw [beq_false_of_ne hit]; exact h⟩

mutual
theorem infer_substitute (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (ps : List Xsh.Syntax.RawName) (vs : List IntValue)
    (same : ps.length = vs.length) :
    ∀ (e : BaseExpr) (t : BaseType), staticType ps e = some t →
      inferBaseType? names activations store (substitute (ps.zip vs) e) = some t
  | .integerLiteral v, t, h => by simpa [staticType, substitute, inferBaseType?] using h
  | .scalarLiteral v, t, h => by simpa [staticType, substitute, inferBaseType?] using h
  | .timeNow, t, h => by simpa [staticType, substitute, inferBaseType?] using h
  | .identifier name, t, h => by
      simp only [staticType] at h
      split at h
      · rename_i member
        obtain ⟨found, lookup⟩ := lookup_zip_of_mem member same
        simp only [Option.some.injEq] at h
        subst h
        simp [substitute, lookup, inferBaseType?, validateInt?, found.property]
      · cases h
  | .negate o, t, h => by
      obtain ⟨ho, rfl⟩ := static_negate h
      simp [substitute, inferBaseType?, infer_substitute names activations store ps vs same o _ ho]
  | .binary op l r, t, h => by
      obtain ⟨hl, hr, rfl⟩ := static_binary h
      simp [substitute, inferBaseType?, infer_substitute names activations store ps vs same l _ hl,
        infer_substitute names activations store ps vs same r _ hr]
  | .compare op l r, t, h => by
      obtain ⟨hl, hr, rfl⟩ := static_compare h
      simp [substitute, inferBaseType?, infer_substitute names activations store ps vs same l _ hl,
        infer_substitute names activations store ps vs same r _ hr]
  | .conditional c a b, t, h => by
      obtain ⟨hc, ha, hb, rfl⟩ := static_conditional h
      simp [substitute, inferBaseType?, infer_substitute names activations store ps vs same c _ hc,
        infer_substitute names activations store ps vs same a _ ha,
        infer_substitute names activations store ps vs same b _ hb]
  | .call inner body args, t, h => by
      obtain ⟨hl, ha, hb, rfl⟩ := static_call h
      have lengths := length_substituteArguments (ps.zip vs) args
      have arguments := infer_arguments_substitute names activations store ps vs same args ha
      simp [substitute, inferBaseType?, hl, lengths, arguments, hb]

theorem infer_arguments_substitute (names : LexicalEnvironment)
    (activations : ActivationEnvironment) (store : BaseStore) (ps : List Xsh.Syntax.RawName)
    (vs : List IntValue) (same : ps.length = vs.length) :
    ∀ (args : List BaseExpr), staticArgumentsInteger ps args = true →
      inferArgumentsInteger names activations store (substituteArguments (ps.zip vs) args) = true
  | [], _ => by simp [substituteArguments, inferArgumentsInteger]
  | a :: rest, h => by
      simp only [staticArgumentsInteger, Bool.and_eq_true, beq_iff_eq] at h
      simp [substituteArguments, inferArgumentsInteger,
        infer_substitute names activations store ps vs same a _ h.1,
        infer_arguments_substitute names activations store ps vs same rest h.2]
end

theorem machine_progress_fragment (config : MachineConfig) :
    terminalOutcome config ≠ none ∨ (∃ next, step config = some next) ∨
      ∃ waiting : BaseHostWait, pendingHostWait? config = some waiting := by
  cases config with
  | mk names activations store control continuation =>
    cases control with
    | evaluate expression =>
      cases expression with
      | call parameters body arguments => cases arguments <;> simp [terminalOutcome, step]
      | _ => simp [terminalOutcome, step]
    | returned value =>
      cases continuation with
      | nil => simp [terminalOutcome]
      | cons frame rest =>
        cases frame with
        | callArgument parameters body evaluated remaining =>
          cases remaining <;> simp [terminalOutcome, step]
        | _ => simp [terminalOutcome, step]
    | awaitingHost request =>
      exact Or.inr (Or.inr ⟨⟨_, request, ⟨names, activations, store,
        .awaitingHost request, continuation⟩, rfl⟩, rfl⟩)
    | failed fault => simp [terminalOutcome]

private theorem step_result_fits_when_typed {names : LexicalEnvironment}
    {activations : ActivationEnvironment} {store : BaseStore} {control : BaseControl}
    {continuation : List BaseFrame} (fits : controlFitsContinuation names activations store
      control continuation = true) :
    controlFitsContinuation names activations store control continuation = true := fits

private theorem fits_evaluate {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {expression : BaseExpr} {frame : BaseFrame} {rest : List BaseFrame} :
    controlFitsContinuation names activations store (.evaluate expression) (frame :: rest) = true ↔
      inferBaseType? names activations store expression = some frame.consumes := by
  simp [controlFitsContinuation]

private theorem fits_returned {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {value : BaseTypedValue} {frame : BaseFrame} {rest : List BaseFrame} :
    controlFitsContinuation names activations store (.returned value) (frame :: rest) = true ↔
      value.typeTag = frame.consumes := by
  simp [controlFitsContinuation]

private theorem fits_empty (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (control : BaseControl) :
    controlFitsContinuation names activations store control [] = true := rfl

private theorem fits_failed {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {fault : BaseFault} {continuation : List BaseFrame} :
    controlFitsContinuation names activations store (.failed fault) continuation = true := by
  cases continuation <;> rfl

private theorem infer_negate {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {operand : BaseExpr} {type : BaseType}
    (typed : inferBaseType? names activations store (.negate operand) = some type) :
    inferBaseType? names activations store operand = some .integer ∧ type = .integer := by
  simp only [inferBaseType?] at typed
  split at typed <;> simp_all

private theorem infer_binary {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {operator left right} {type : BaseType}
    (typed : inferBaseType? names activations store (.binary operator left right) = some type) :
    inferBaseType? names activations store left = some .integer ∧
      inferBaseType? names activations store right = some .integer ∧ type = .integer := by
  simp only [inferBaseType?] at typed
  split at typed <;> simp_all

private theorem infer_compare {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {operator left right} {type : BaseType}
    (typed : inferBaseType? names activations store (.compare operator left right) = some type) :
    inferBaseType? names activations store left = some .integer ∧
      inferBaseType? names activations store right = some .integer ∧ type = .boolean := by
  simp only [inferBaseType?] at typed
  split at typed <;> simp_all

private theorem infer_conditional {names : LexicalEnvironment}
    {activations : ActivationEnvironment} {store : BaseStore} {condition thenBranch elseBranch}
    {type : BaseType}
    (typed : inferBaseType? names activations store (.conditional condition thenBranch elseBranch) =
      some type) :
    inferBaseType? names activations store condition = some .boolean ∧
      inferBaseType? names activations store thenBranch = some .integer ∧
      inferBaseType? names activations store elseBranch = some .integer ∧ type = .integer := by
  simp only [inferBaseType?] at typed
  split at typed <;> simp_all

private theorem infer_call {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {parameters body arguments} {type : BaseType}
    (typed : inferBaseType? names activations store (.call parameters body arguments) =
      some type) :
    parameters.length = arguments.length ∧
      inferArgumentsInteger names activations store arguments = true ∧
      staticType parameters body = some .integer ∧ type = .integer := by
  simp only [inferBaseType?] at typed
  split at typed <;> simp_all

private theorem typed_of_checked_composite {names : LexicalEnvironment}
    {activations : ActivationEnvironment} {store : BaseStore} {expression : BaseExpr}
    (checked : (inferBaseType? names activations store expression).isSome = true) :
    ∃ type, inferBaseType? names activations store expression = some type := by
  cases found : inferBaseType? names activations store expression with
  | none => simp [found] at checked
  | some type => exact ⟨type, rfl⟩

private theorem infer_arguments_head {names : LexicalEnvironment}
    {activations : ActivationEnvironment} {store : BaseStore} {argument : BaseExpr}
    {remaining : List BaseExpr}
    (typed : inferArgumentsInteger names activations store (argument :: remaining) = true) :
    inferBaseType? names activations store argument = some .integer ∧
      inferArgumentsInteger names activations store remaining = true := by
  simpa [inferArgumentsInteger] using typed

private theorem wf_returned {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {value : BaseTypedValue} {continuation : List BaseFrame}
    (checked : continuationChecked names activations store continuation = true)
    (fits : ∀ frame rest, continuation = frame :: rest → value.typeTag = frame.consumes) :
    wellFormed (MachineConfig.mk names activations store (.returned value) continuation) := by
  refine ⟨trivial, checked, ?_⟩
  cases continuation with
  | nil => rfl
  | cons frame rest => exact fits_returned.mpr (fits frame rest rfl)

private theorem wf_failed {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {fault : BaseFault} {continuation : List BaseFrame}
    (checked : continuationChecked names activations store continuation = true) :
    wellFormed (MachineConfig.mk names activations store (.failed fault) continuation) :=
  ⟨trivial, checked, fits_failed⟩

private theorem wf_evaluate {names : LexicalEnvironment} {activations : ActivationEnvironment}
    {store : BaseStore} {expression : BaseExpr} {continuation : List BaseFrame}
    (accepted : checkBaseExpr names activations store expression = true)
    (checked : continuationChecked names activations store continuation = true)
    (fits : ∀ frame rest, continuation = frame :: rest →
      inferBaseType? names activations store expression = some frame.consumes) :
    wellFormed (MachineConfig.mk names activations store (.evaluate expression) continuation) := by
  refine ⟨accepted, checked, ?_⟩
  cases continuation with
  | nil => rfl
  | cons frame rest => exact fits_evaluate.mpr (fits frame rest rfl)

private theorem infer_of_checked_integer {names : LexicalEnvironment}
    {activations : ActivationEnvironment} {store : BaseStore} {expression : BaseExpr}
    (checked : checkIntegerExpr names activations store expression = true) :
    inferBaseType? names activations store expression = some .integer := by
  unfold checkIntegerExpr at checked
  split at checked <;> simp_all

private theorem produces_of_type {frame : BaseFrame} {continuation : List BaseFrame}
    (fits : ∀ next rest, continuation = next :: rest → frame.produces = next.consumes) :
    producesWhatNextConsumes frame continuation = true := by
  cases continuation with
  | nil => rfl
  | cons next rest => simpa [producesWhatNextConsumes] using fits next rest rfl

private theorem produces_type {frame next : BaseFrame} {rest : List BaseFrame}
    (produces : producesWhatNextConsumes frame (next :: rest) = true) :
    frame.produces = next.consumes := by
  simpa [producesWhatNextConsumes] using produces

private theorem produces_fits {frame : BaseFrame} {continuation : List BaseFrame}
    (produces : producesWhatNextConsumes frame continuation = true) {type : BaseType}
    (same : type = frame.produces) :
    ∀ next rest, continuation = next :: rest → type = next.consumes := by
  intro next rest eq
  subst eq
  rw [same]
  exact produces_type produces

private theorem checked_int_value_is_valid {value : Int} {integer : IntValue}
    (result : checkedInt value = .value integer) : validateInt? value = some integer := by
  unfold checkedInt at result
  split at result <;> simp_all

private theorem wf_integer_result {names : LexicalEnvironment}
    {activations : ActivationEnvironment} {store : BaseStore} {rest : List BaseFrame}
    (result : IntegerResult)
    (checked : continuationChecked names activations store rest = true)
    (fits : ∀ next remaining, rest = next :: remaining → BaseType.integer = next.consumes) :
    wellFormed (MachineConfig.mk names activations store (controlOfIntegerResult result) rest) := by
  cases result with
  | value integer => exact wf_returned checked fits
  | fault fault => exact wf_failed checked

theorem wellFormed_preserved_fragment {config next : MachineConfig}
    (wellFormedConfig : wellFormed config) (takesStep : step config = some next) :
    wellFormed next := by
  cases config with
  | mk names activations store control continuation =>
    obtain ⟨controlWellFormed, continuationWellFormed, controlFits⟩ := wellFormedConfig
    cases control with
    | evaluate expression =>
      have accepted : checkBaseExpr names activations store expression = true :=
        controlWellFormed
      have fitsFrame : ∀ frame rest, continuation = frame :: rest →
          inferBaseType? names activations store expression = some frame.consumes := by
        intro frame rest eq
        subst eq
        exact fits_evaluate.mp controlFits
      cases expression with
      | integerLiteral value =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        refine wf_integer_result (checkedInt value) continuationWellFormed fun frame rest eq => ?_
        have typed := fitsFrame frame rest eq
        cases found : validateInt? value <;> simp [inferBaseType?, found] at typed
        exact typed
      | scalarLiteral value =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        cases found : scalarTypedValue? value with
        | none => exact wf_failed continuationWellFormed
        | some typed =>
          refine wf_returned continuationWellFormed fun frame rest eq => ?_
          have typedExpression := fitsFrame frame rest eq
          simpa [inferBaseType?, found] using typedExpression
      | timeNow =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        refine ⟨trivial, continuationWellFormed, ?_⟩
        cases continuation with
        | nil => rfl
        | cons frame rest =>
          have typedExpression := fitsFrame frame rest rfl
          simp only [inferBaseType?, Option.some.injEq] at typedExpression
          show controlFitsContinuation names activations store
            (.awaitingHost .timeNow) (frame :: rest) = true
          simp [controlFitsContinuation, BaseHostRequest.responseType, typedExpression]
      | identifier name =>
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        cases found : lookupBaseValue names activations store name with
        | error fault => exact wf_failed continuationWellFormed
        | ok typed =>
          refine wf_returned continuationWellFormed fun frame rest eq => ?_
          have typedExpression := fitsFrame frame rest eq
          simpa [inferBaseType?, found] using typedExpression
      | negate operand =>
        have operandInteger := infer_of_checked_integer (by simpa [checkBaseExpr] using accepted)
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        refine wf_evaluate (checker_accepts_every_typed_expression operandInteger) ?_ ?_
        · have produces : producesWhatNextConsumes .negate continuation = true := by
            refine produces_of_type fun next rest eq => ?_
            have typedExpression := fitsFrame next rest eq
            have := (infer_negate typedExpression).2
            simp [BaseFrame.produces, this]
          simp [continuationChecked, produces, continuationWellFormed]
        · intro frame rest eq
          cases eq
          simpa [BaseFrame.consumes] using operandInteger
      | binary operator left right =>
        have operands : checkIntegerExpr names activations store left = true ∧
            checkIntegerExpr names activations store right = true := by
          simpa [checkBaseExpr, Bool.and_eq_true] using accepted
        have leftInteger := infer_of_checked_integer operands.1
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        refine wf_evaluate (checker_accepts_every_typed_expression leftInteger) ?_ ?_
        · have produces : producesWhatNextConsumes (.binaryLeft operator right) continuation =
              true := by
            refine produces_of_type fun next rest eq => ?_
            have typedExpression := fitsFrame next rest eq
            have := (infer_binary typedExpression).2.2
            simp [BaseFrame.produces, this]
          simp [continuationChecked, produces, continuationWellFormed, operands.2]
        · intro frame rest eq
          cases eq
          simpa [BaseFrame.consumes] using leftInteger
      | compare operator left right =>
        obtain ⟨type, typed⟩ := typed_of_checked_composite
          (by simpa [checkBaseExpr] using accepted)
        obtain ⟨leftInteger, rightInteger, rfl⟩ := infer_compare typed
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        refine wf_evaluate (checker_accepts_every_typed_expression leftInteger) ?_ ?_
        · have produces : producesWhatNextConsumes (.compareLeft operator right) continuation =
              true := by
            refine produces_of_type fun next rest eq => ?_
            have typedExpression := fitsFrame next rest eq
            rw [typed] at typedExpression
            simpa [BaseFrame.produces] using typedExpression
          simp [continuationChecked, produces, continuationWellFormed, checkIntegerExpr,
            rightInteger]
        · intro frame rest eq
          cases eq
          simpa [BaseFrame.consumes] using leftInteger
      | conditional condition thenBranch elseBranch =>
        obtain ⟨type, typed⟩ := typed_of_checked_composite
          (by simpa [checkBaseExpr] using accepted)
        obtain ⟨conditionBoolean, thenInteger, elseInteger, rfl⟩ := infer_conditional typed
        simp only [step, Option.some.injEq] at takesStep
        subst takesStep
        refine wf_evaluate (checker_accepts_every_typed_expression conditionBoolean) ?_ ?_
        · have produces : producesWhatNextConsumes (.branch thenBranch elseBranch)
              continuation = true := by
            refine produces_of_type fun next rest eq => ?_
            have typedExpression := fitsFrame next rest eq
            rw [typed] at typedExpression
            simpa [BaseFrame.produces] using typedExpression
          simp [continuationChecked, produces, continuationWellFormed, checkIntegerExpr,
            thenInteger, elseInteger]
        · intro frame rest eq
          cases eq
          simpa [BaseFrame.consumes] using conditionBoolean
      | call parameters body arguments =>
        obtain ⟨type, typed⟩ := typed_of_checked_composite
          (by simpa [checkBaseExpr] using accepted)
        obtain ⟨sameLength, argumentsInteger, bodyInteger, rfl⟩ := infer_call typed
        have produces : producesWhatNextConsumes
            (.callArgument parameters body [] (arguments.drop 1)) continuation = true := by
          refine produces_of_type fun next rest eq => ?_
          have typedExpression := fitsFrame next rest eq
          rw [typed] at typedExpression
          simpa [BaseFrame.produces] using typedExpression
        cases arguments with
        | nil =>
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          refine wf_evaluate ?_ continuationWellFormed ?_
          · refine checker_accepts_every_typed_expression (type := .integer) ?_
            exact infer_substitute names activations store parameters [] (by simpa using sameLength)
              body _ bodyInteger
          · intro frame rest eq
            have typedExpression := fitsFrame frame rest eq
            rw [typed] at typedExpression
            have consumed : BaseType.integer = frame.consumes := Option.some.inj typedExpression
            rw [← consumed]
            exact infer_substitute names activations store parameters [] (by simpa using sameLength)
              body _ bodyInteger
        | cons argument remaining =>
          obtain ⟨argumentInteger, remainingInteger⟩ := infer_arguments_head argumentsInteger
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          refine wf_evaluate (checker_accepts_every_typed_expression argumentInteger) ?_ ?_
          · simp only [continuationChecked, Bool.and_eq_true, decide_eq_true_eq, beq_iff_eq]
            simp only [List.drop_succ_cons, List.drop_zero] at produces
            refine ⟨⟨⟨⟨produces, ?_⟩, remainingInteger⟩, bodyInteger⟩, continuationWellFormed⟩
            simp only [List.length_cons, List.length_nil] at sameLength ⊢
            omega
          · intro frame rest eq
            cases eq
            simpa [BaseFrame.consumes] using argumentInteger
    | returned value =>
      cases continuation with
      | nil => simp [step] at takesStep
      | cons frame rest =>
        have valueType : value.typeTag = frame.consumes := fits_returned.mp controlFits
        cases frame with
        | negate =>
          simp only [continuationChecked, Bool.and_eq_true] at continuationWellFormed
          obtain ⟨produces, restChecked⟩ := continuationWellFormed
          have payload := typed_integer_payload_is_present value valueType
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer =>
            exact wf_integer_result _ restChecked (produces_fits produces rfl)
        | binaryLeft operator right =>
          simp only [continuationChecked, Bool.and_eq_true] at continuationWellFormed
          obtain ⟨⟨produces, rightInteger⟩, restChecked⟩ := continuationWellFormed
          have payload := typed_integer_payload_is_present value valueType
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer =>
            have rightTyped := infer_of_checked_integer rightInteger
            refine wf_evaluate (checker_accepts_every_typed_expression rightTyped) ?_ ?_
            · simp only [continuationChecked, Bool.and_eq_true]
              exact ⟨produces, restChecked⟩
            · intro frame remaining eq
              cases eq
              simpa [BaseFrame.consumes] using rightTyped
        | binaryRight operator left =>
          simp only [continuationChecked, Bool.and_eq_true] at continuationWellFormed
          obtain ⟨produces, restChecked⟩ := continuationWellFormed
          have payload := typed_integer_payload_is_present value valueType
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer =>
            exact wf_integer_result _ restChecked (produces_fits produces rfl)
        | compareLeft operator right =>
          simp only [continuationChecked, Bool.and_eq_true] at continuationWellFormed
          obtain ⟨⟨produces, rightInteger⟩, restChecked⟩ := continuationWellFormed
          have payload := typed_integer_payload_is_present value valueType
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer =>
            have rightTyped := infer_of_checked_integer rightInteger
            refine wf_evaluate (checker_accepts_every_typed_expression rightTyped) ?_ ?_
            · simp only [continuationChecked, Bool.and_eq_true]
              exact ⟨produces, restChecked⟩
            · intro frame remaining eq
              cases eq
              simpa [BaseFrame.consumes] using rightTyped
        | compareRight operator left =>
          simp only [continuationChecked, Bool.and_eq_true] at continuationWellFormed
          obtain ⟨produces, restChecked⟩ := continuationWellFormed
          have payload := typed_integer_payload_is_present value valueType
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer =>
            exact wf_returned restChecked (produces_fits (type := .boolean) produces rfl)
        | branch thenBranch elseBranch =>
          simp only [continuationChecked, Bool.and_eq_true] at continuationWellFormed
          obtain ⟨⟨⟨produces, thenInteger⟩, elseInteger⟩, restChecked⟩ := continuationWellFormed
          have payload := typed_boolean_payload_is_present value valueType
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : booleanPayload? value with
          | none => simp [found] at payload
          | some truth =>
            have thenTyped := infer_of_checked_integer thenInteger
            have elseTyped := infer_of_checked_integer elseInteger
            cases truth with
            | true =>
              exact wf_evaluate (checker_accepts_every_typed_expression thenTyped) restChecked
                fun frame remaining eq => by
                  show inferBaseType? names activations store thenBranch = some frame.consumes
                  rw [thenTyped]
                  exact congrArg some (produces_fits (type := .integer) produces rfl frame remaining eq)
            | false =>
              exact wf_evaluate (checker_accepts_every_typed_expression elseTyped) restChecked
                fun frame remaining eq => by
                  show inferBaseType? names activations store elseBranch = some frame.consumes
                  rw [elseTyped]
                  exact congrArg some (produces_fits (type := .integer) produces rfl frame remaining eq)
        | callArgument parameters body evaluated remaining =>
          simp only [continuationChecked, Bool.and_eq_true, decide_eq_true_eq, beq_iff_eq]
            at continuationWellFormed
          obtain ⟨⟨⟨⟨produces, sameLength⟩, remainingInteger⟩, bodyInteger⟩, restChecked⟩ :=
            continuationWellFormed
          have payload := typed_integer_payload_is_present value valueType
          simp only [step, Option.some.injEq] at takesStep
          subst takesStep
          cases found : integerPayload? value with
          | none => simp [found] at payload
          | some integer =>
            cases remaining with
            | nil =>
              refine wf_evaluate ?_ restChecked fun frame rest' eq => ?_
              · refine checker_accepts_every_typed_expression (type := .integer) ?_
                exact infer_substitute names activations store parameters (evaluated ++ [integer])
                  (by simp at sameLength ⊢; omega) body _ bodyInteger
              · have consumed := produces_fits (type := .integer) produces rfl frame rest' eq
                rw [← consumed]
                exact infer_substitute names activations store parameters (evaluated ++ [integer])
                  (by simp at sameLength ⊢; omega) body _ bodyInteger
            | cons argument more =>
              obtain ⟨argumentInteger, moreInteger⟩ := infer_arguments_head remainingInteger
              refine wf_evaluate (checker_accepts_every_typed_expression argumentInteger) ?_ ?_
              · simp only [continuationChecked, Bool.and_eq_true, decide_eq_true_eq, beq_iff_eq]
                refine ⟨⟨⟨⟨produces, ?_⟩, moreInteger⟩, bodyInteger⟩, restChecked⟩
                simp only [List.length_cons, List.length_append, List.length_nil] at sameLength ⊢
                omega
              · intro frame remaining' eq
                cases eq
                simpa [BaseFrame.consumes] using argumentInteger
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
          cases continuation with
          | nil => simp [wellFormed, updateState, controlFitsContinuation, continuationWellFormed]
          | cons frame rest =>
            have consumed : BaseType.integer = frame.consumes := by
              simpa [controlFitsContinuation, BaseHostRequest.responseType] using
                wellFormedConfig.2.2
            simp [wellFormed, updateState, controlFitsContinuation, continuationWellFormed,
              valueHasIntegerTag, consumed]

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
    controlFitsContinuation, continuationChecked, producesWhatNextConsumes, BaseFrame.consumes,
    integerPayload?, step, updateState]

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

private def exampleSeven : IntValue := exampleSmall 7 (by unfold signed64Min signed64Limit; omega)
private def exampleNegativeSeven : IntValue :=
  exampleSmall (-7) (by unfold signed64Min signed64Limit; omega)

private def firstParameter : NameId := ⟨⟨1, by decide⟩⟩
private def secondParameter : NameId := ⟨⟨2, by decide⟩⟩

def subtractionCall (minuend subtrahend : Int) : BaseExpr :=
  .call [firstParameter, secondParameter]
    (.binary .subtract (.identifier firstParameter) (.identifier secondParameter))
    [.integerLiteral minuend, .integerLiteral subtrahend]

theorem call_binds_each_argument_to_its_parameter_fragment :
    checkBaseExpr [] [] (fun _ => none) (subtractionCall 10 3) = true ∧
      runFuel 40 (initialConfig [] [] (fun _ => none) (subtractionCall 10 3)) =
        .completed (.value (typedInteger exampleSeven)) ∧
      runFuel 40 (initialConfig [] [] (fun _ => none) (subtractionCall 3 10)) =
        .completed (.value (typedInteger exampleNegativeSeven)) := by
  exact ⟨rfl, rfl, rfl⟩

theorem call_with_the_wrong_arity_or_a_free_name_is_rejected_fragment :
    checkBaseExpr [] [] (fun _ => none) (.call [firstParameter] (.identifier firstParameter) []) =
        false ∧
      checkBaseExpr [] [] (fun _ => none)
        (.call [firstParameter] (.identifier secondParameter) [.integerLiteral 1]) = false := by
  exact ⟨rfl, rfl⟩

def selectedBranchExample (condition : ComparisonOperator) : BaseExpr :=
  .conditional (.compare condition (.integerLiteral 1) (.integerLiteral 2))
    (.integerLiteral 7) divisionByZeroExample

theorem conditional_evaluates_only_the_selected_branch_fragment :
    checkBaseExpr [] [] (fun _ => none) (selectedBranchExample .less) = true ∧
      runFuel 40 (initialConfig [] [] (fun _ => none) (selectedBranchExample .less)) =
        .completed (.value (typedInteger exampleSeven)) ∧
      runFuel 40 (initialConfig [] [] (fun _ => none) (selectedBranchExample .greater)) =
        .completed (.fault (.arithmetic .divisionByZero)) := by
  exact ⟨rfl, rfl, rfl⟩

theorem conditional_with_a_non_boolean_condition_is_rejected_fragment :
    checkBaseExpr [] [] (fun _ => none)
      (.conditional (.integerLiteral 1) (.integerLiteral 7) (.integerLiteral 8)) = false := by
  rfl

end Xsh.Proofs.BaseMachine
