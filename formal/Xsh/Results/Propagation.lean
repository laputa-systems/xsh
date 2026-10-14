import Xsh.Results.Fallback

/-!
This fragment models `?` completion over a finite propagation-boundary stack.
The error embedding stored at a Result-returning function is an explicit
compatibility premise. It does not prove typechecking, cleanup, effects,
tracebacks, Rust correspondence, or whole-language soundness.
-/

namespace Xsh.Results.Propagation

universe u

/-- An explicit conversion witnesses that a propagated error fits a function's
declared Result error type. -/
structure CompatibleError (Error Target : Type u) where
  inject : Error → Target

/-- Boundaries are ordered from the expression out toward the script top
level. A plain procedure turns a propagated Result error into a failed call;
caller Result functions do not absorb that runtime failure. -/
inductive Boundary (Error Target : Type u) where
  | ordinaryBlock
  | tryBlock
  | retryAttempt
  | resultFunction (compatible : CompatibleError Error Target)
  | plainProcedure

/-- Every lexical stack ends at the script's top-level failure handler. -/
inductive BoundaryStack (Error Target : Type u) where
  | topLevel
  | push (boundary : Boundary Error Target)
      (outer : BoundaryStack Error Target)

/-- Crossing a plain procedure changes an Err propagation into a failed call. -/
inductive FailureOrigin where
  | question
  | plainProcedureCall
  deriving DecidableEq, Repr

/-- The observable result after `?` finds its nearest propagation boundary. -/
inductive Exit (Error Value Target : Type u) where
  | value (payload : Value) (remaining : BoundaryStack Error Target)
  | tryError (error : Error) (origin : FailureOrigin)
  | retryFailure (error : Error) (origin : FailureOrigin)
  | functionError (error : Target)
  | topLevelFailure (error : Error) (origin : FailureOrigin)

/-- Resolve an Err by scanning boundaries from the expression outward. Ordinary
blocks pass it through. A Result function handles only a direct `?` failure;
once a plain procedure call has failed, that error keeps unwinding. -/
def resolveQuestionError {Error Value Target : Type u}
    (stack : BoundaryStack Error Target) (error : Error) :
    Exit Error Value Target :=
  scan stack .question
where
  scan (stack : BoundaryStack Error Target) (origin : FailureOrigin) :
      Exit Error Value Target :=
    match stack with
    | .topLevel => .topLevelFailure error origin
    | .push .ordinaryBlock outer => scan outer origin
    | .push .tryBlock _ => .tryError error origin
    | .push .retryAttempt _ => .retryFailure error origin
    | .push (.resultFunction compatible) outer =>
        match origin with
        | .question => .functionError (compatible.inject error)
        | .plainProcedureCall => scan outer origin
    | .push .plainProcedure outer => scan outer .plainProcedureCall

/-- Apply `?` to a Result. The shared completion shape keeps successful payloads
distinct from errors before boundary selection. -/
def questionCompletion {Error Value : Type u}
    (result : Except Error Value) : Xsh.Results.Fallback.Completion Error Value :=
  match result with
  | .ok payload => .value payload
  | .error error => .propagated error

/-- Deliver the completion of `?` to the nearest boundary. -/
def deliverQuestion {Error Value Target : Type u}
    (stack : BoundaryStack Error Target)
    (completion : Xsh.Results.Fallback.Completion Error Value) :
    Exit Error Value Target :=
  match completion with
  | .value payload => .value payload stack
  | .propagated error => resolveQuestionError stack error

/-- Evaluate the boundary behavior of `result?`. -/
def propagateResult {Error Value Target : Type u}
    (stack : BoundaryStack Error Target) (result : Except Error Value) :
    Exit Error Value Target :=
  deliverQuestion stack (questionCompletion result)

/-- A returned Result remains ordinary data until a `?` is applied. -/
inductive ExpressionResult (Error Value Target : Type u) where
  | resultData (result : Except Error Value)
  | completed (exit : Exit Error Value Target)

/-- Construct Result data without requesting propagation. -/
def returnResultData {Error Value Target : Type u}
    (result : Except Error Value) : ExpressionResult Error Value Target :=
  .resultData result

/-- Apply an explicit `?` to returned Result data. -/
def questionReturnedResult {Error Value Target : Type u}
    (stack : BoundaryStack Error Target)
    (returned : ExpressionResult Error Value Target) :
    ExpressionResult Error Value Target :=
  match returned with
  | .resultData result => .completed (propagateResult stack result)
  | .completed exit => .completed exit

/-- The faulty rule that automatically applies `?` to any returned Result. -/
def automaticPropagationMutation {Error Value Target : Type u}
    (stack : BoundaryStack Error Target)
    (returned : ExpressionResult Error Value Target) :
    ExpressionResult Error Value Target :=
  match returned with
  | .resultData result => .completed (propagateResult stack result)
  | .completed exit => .completed exit

/-- A successful Result yields its payload at top level. -/
theorem ok_propagation_yields_payload {Error Value Target : Type u}
    (payload : Value) :
    propagateResult (.topLevel : BoundaryStack Error Target) (.ok payload) =
      (.value payload .topLevel : Exit Error Value Target) := by
  rfl

/-- A successful Result completes locally while an enclosing boundary remains
available to later computation. -/
theorem ok_propagation_stays_inside_boundary {Error Value Target : Type u}
    (payload : Value) :
    propagateResult (.push .tryBlock .topLevel : BoundaryStack Error Target)
      (.ok payload) =
      (.value payload (.push .tryBlock .topLevel) : Exit Error Value Target) := by
  rfl

/-- An Err selects the nearest try even when an outer retry could also capture
it. -/
theorem err_propagation_leaves_nearest_try {Error Value Target : Type u}
    (error : Error) :
    propagateResult
      (.push .tryBlock (.push .retryAttempt .topLevel)) (.error error) =
      (.tryError error .question : Exit Error Value Target) := by
  rfl

/-- A retry attempt is the nearest boundary when it appears before an outer
try. -/
theorem err_propagation_fails_current_retry_attempt
    {Error Value Target : Type u} (error : Error) :
    propagateResult
      (.push .retryAttempt (.push .tryBlock .topLevel)) (.error error) =
      (.retryFailure error .question : Exit Error Value Target) := by
  rfl

/-- A Result-returning function returns its Err when source and target error
types agree. -/
theorem err_propagation_returns_from_result_function
    {Error Value : Type u} (error : Error) :
    propagateResult
      (.push (.resultFunction (⟨id⟩ : CompatibleError Error Error))
        (.push .tryBlock .topLevel))
      (.error error) = (.functionError error : Exit Error Value Error) := by
  rfl

/-- Returning an error through a differently typed Result function requires an
explicit embedding accepted by that function's target. -/
theorem function_error_target_must_accept_source_error
    {Error Value Target : Type u}
    (compatible : CompatibleError Error Target) (error : Error) :
    propagateResult
      (.push (.resultFunction compatible) .topLevel)
      (.error error) =
      (.functionError (compatible.inject error) : Exit Error Value Target) := by
  rfl

/-- A plain procedure cannot return the Err; it becomes a failed call on the
way to the top-level handler. -/
theorem err_propagation_escapes_plain_proc_as_failure
    {Error Value Target : Type u} (error : Error) :
    propagateResult (.push .plainProcedure .topLevel) (.error error) =
      (.topLevelFailure error .plainProcedureCall : Exit Error Value Target) := by
  rfl

/-- A failed plain-procedure call unwinds through a caller Result function and
is captured by the caller's surrounding try. -/
theorem escaped_proc_failure_reaches_enclosing_try
    {Error Value Target : Type u}
    (compatible : CompatibleError Error Target) (error : Error) :
    propagateResult
      (.push .plainProcedure
        (.push (.resultFunction compatible)
          (.push .tryBlock .topLevel)))
      (.error error) =
      (.tryError error .plainProcedureCall : Exit Error Value Target) := by
  rfl

/-- A top-level `?` leaves the script as an uncaught failure. -/
theorem top_level_propagation_remains_uncaught
    {Error Value Target : Type u} (error : Error) :
    propagateResult (.topLevel : BoundaryStack Error Target) (.error error) =
      (.topLevelFailure error .question : Exit Error Value Target) := by
  rfl

/-- An ordinary block passes an Err to the nearest enclosing try. -/
theorem ordinary_blocks_do_not_capture_propagation
    {Error Value Target : Type u} (error : Error) :
    propagateResult
      (.push .ordinaryBlock (.push .tryBlock .topLevel)) (.error error) =
      (.tryError error .question : Exit Error Value Target) := by
  rfl

/-- Constructing and returning Err yields Result data without consulting a
propagation boundary. -/
theorem returned_err_value_is_not_propagation
    {Error Value Target : Type u} (error : Error) :
    returnResultData (.error error) =
      (.resultData (.error error : Except Error Value) :
        ExpressionResult Error Value Target) := by
  rfl

/-- Automatically applying `?` to returned Err data changes a returned value
into a failure captured by a caller's try. -/
theorem wrong_boundary_mutation_changes_completion
    {Error Value Target : Type u} (error : Error) :
    returnResultData (.error error : Except Error Value) =
        (.resultData (.error error : Except Error Value) :
          ExpressionResult Error Value Target) ∧
      automaticPropagationMutation
        (.push .tryBlock .topLevel : BoundaryStack Error Target)
        (returnResultData (.error error : Except Error Value)) =
        (.completed (.tryError error .question : Exit Error Value Target) :
          ExpressionResult Error Value Target) := by
  constructor <;> rfl

end Xsh.Results.Propagation
