/-!
This fragment models the selection and trace of XSH's `??` fallback only.
Its error type is abstract. It does not model `?` propagation or its targets,
cleanup, control-flow handlers, static effects, concrete runtime error
representations, or correspondence with Rust.
-/

namespace Xsh.Results.Fallback

universe u

/-- The two outcomes relevant to evaluating a fallback tail. -/
inductive Completion (Error Value : Type u) where
  | value (payload : Value)
  | propagated (error : Error)

/-- Result and optional inputs are distinguished only at the fallback site. -/
inductive Source (Error Value : Type u) where
  | resultOk (payload : Value)
  | resultErr (error : Error)
  | optionalSome (payload : Value)
  | optionalNull

/-- The context a Result handler receives, or the absence of such a context. -/
inductive FallbackContext (Error : Type u) where
  | plain
  | originalError (error : Error)

/-- A trace event records only that a fallback tail was evaluated and why. -/
inductive FallbackCall (Error : Type u) where
  | resultError (error : Error)
  | optionalNull
  | eagerOnResultSuccessMutation

/-- A denotation keeps the outcome and the fallback-only evaluation trace. -/
structure Evaluation (Error Value : Type u) where
  completion : Completion Error Value
  trace : List (FallbackCall Error)

/-- A fallback tail is delayed and can inspect a Result handler's error. -/
abbrev Tail (Error Value : Type u) :=
  FallbackContext Error → Unit → Evaluation Error Value

/-- Evaluate one selected tail and prepend its single invocation event. -/
def invoke {Error Value : Type u} (call : FallbackCall Error)
    (tail : Tail Error Value) (context : FallbackContext Error) :
    Evaluation Error Value :=
  let evaluated := tail context ()
  { evaluated with trace := call :: evaluated.trace }

/-- Apply `??` to a Result or optional source, evaluating only its missing arm. -/
def evalFallback {Error Value : Type u} (source : Source Error Value)
    (tail : Tail Error Value) : Evaluation Error Value :=
  match source with
  | .resultOk payload => ⟨.value payload, []⟩
  | .resultErr error => invoke (.resultError error) tail (.originalError error)
  | .optionalSome payload => ⟨.value payload, []⟩
  | .optionalNull => invoke .optionalNull tail .plain

/-- A plain right-associated chain evaluates its next source inside the tail. -/
def chainRight {Error Value : Type u} (first second : Source Error Value)
    (last : Tail Error Value) : Evaluation Error Value :=
  evalFallback first
    (fun _ _ => evalFallback second (fun context _ => last context ()))

/-- A faulty strict variant evaluates a Result fallback even when the left
side is already successful. The trace exposes that unnecessary evaluation. -/
def eagerResultMutation {Error Value : Type u}
    (source : Source Error Value) (tail : Tail Error Value) :
    Evaluation Error Value :=
  match source with
  | .resultOk payload =>
      let evaluated :=
        invoke .eagerOnResultSuccessMutation tail .plain
      match evaluated.completion with
      | .value _ => { evaluated with completion := .value payload }
      | .propagated _error => evaluated
  | .resultErr error => invoke (.resultError error) tail (.originalError error)
  | .optionalSome payload => ⟨.value payload, []⟩
  | .optionalNull => invoke .optionalNull tail .plain

/-- A successful Result yields its payload with no fallback call. -/
theorem result_ok_returns_payload_lazily {Error Value : Type u}
    (payload : Value) (tail : Tail Error Value) :
    (evalFallback (.resultOk payload) tail).completion = .value payload ∧
      (evalFallback (.resultOk payload) tail).trace = [] := by
  constructor <;> rfl

/-- A non-null optional yields its value with no fallback call. -/
theorem optional_some_returns_value_lazily {Error Value : Type u}
    (payload : Value) (tail : Tail Error Value) :
    (evalFallback (.optionalSome payload) tail).completion = .value payload ∧
      (evalFallback (.optionalSome payload) tail).trace = [] := by
  constructor <;> rfl

/-- A Result error is passed unchanged to the handler context. -/
theorem handler_receives_original_error {Error : Type u} (error : Error) :
    (evalFallback (.resultErr error)
      (fun context _ =>
        match context with
        | .plain => ⟨.propagated error, []⟩
        | .originalError received => ⟨.value received, []⟩)).completion =
      .value error := by
  rfl

/-- A Result error evaluates the tail once with its original error context. -/
theorem result_error_evaluates_tail_once {Error Value : Type u}
    (error : Error) (tail : Tail Error Value)
    (tailHasNoNestedFallback : (tail (.originalError error) ()).trace = []) :
    (evalFallback (.resultErr error) tail).trace = [.resultError error] := by
  simp [evalFallback, invoke, tailHasNoNestedFallback]

/-- An optional null evaluates one plain fallback tail. -/
theorem optional_null_evaluates_tail_once {Error Value : Type u}
    (tail : Tail Error Value) (tailHasNoNestedFallback :
      (tail .plain ()).trace = []) :
    (evalFallback .optionalNull tail).trace = [.optionalNull] := by
  simp [evalFallback, invoke, tailHasNoNestedFallback]

/-- An error raised by the Result fallback tail propagates unchanged. -/
theorem result_fallback_error_propagates {Error Value : Type u}
    (sourceError tailError : Error) :
    (@evalFallback Error Value (.resultErr sourceError)
      (fun _ _ => ⟨.propagated tailError, []⟩)).completion =
      .propagated tailError := by
  rfl

/-- An error raised by the optional fallback tail propagates unchanged. -/
theorem optional_fallback_error_propagates {Error Value : Type u}
    (tailError : Error) :
    (@evalFallback Error Value .optionalNull
      (fun _ _ => ⟨.propagated tailError, []⟩)).completion =
      .propagated tailError := by
  rfl

/-- A right-associated chain enters each missing arm in source order and
returns the outcome of its final tail. -/
theorem right_chain_nests_fallbacks_in_order {Error Value : Type u}
    (firstError secondError : Error) (last : Tail Error Value) :
    (chainRight (.resultErr firstError) (.resultErr secondError) last).completion =
        (last (.originalError secondError) ()).completion ∧
      (chainRight (.resultErr firstError) (.resultErr secondError) last).trace =
        .resultError firstError :: .resultError secondError ::
          (last (.originalError secondError) ()).trace := by
  constructor <;> rfl

/-- A successful left source short-circuits the right-associated chain. -/
theorem right_chain_short_circuits_after_left_success {Error Value : Type u}
    (payload : Value) (second : Source Error Value) (last : Tail Error Value) :
    (chainRight (.resultOk payload) second last).completion = .value payload ∧
      (chainRight (.resultOk payload) second last).trace = [] := by
  constructor <;> rfl

private def quietFalseTail : Tail Unit Bool :=
  fun _ _ => ⟨.value false, []⟩

/-- The eager-evaluation mutant records a real, unnecessary fallback call on
an `Ok` input while preserving that input's payload. -/
theorem eager_fallback_mutation_is_detectable :
    (evalFallback (.resultOk true) quietFalseTail).completion = .value true ∧
      (evalFallback (.resultOk true) quietFalseTail).trace = [] ∧
      (eagerResultMutation (.resultOk true) quietFalseTail).completion =
        .value true ∧
      (eagerResultMutation (.resultOk true) quietFalseTail).trace =
        [.eagerOnResultSuccessMutation] := by
  constructor
  · rfl
  constructor
  · rfl
  constructor
  · rfl
  · rfl

end Xsh.Results.Fallback
