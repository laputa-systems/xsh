import Xsh.Results.Propagation

/-!
This fragment models one optional access hop over an already evaluated nullable
receiver and the Result propagation path selected by the receiver type. Traces
record the callbacks that run and their order. It does not model checker
diagnostics, cleanup, effect accounting, or Rust correspondence.
-/

namespace Xsh.Control.OptionalAccess

universe u

/-- Observable callback evaluations in the finite access model. -/
inductive Event where
  | receiver
  | argument (position : Nat)
  | field
  | method
  | index
  | range
  deriving DecidableEq, Repr

/-- A callback result together with the events evaluated to produce it. -/
structure Evaluation (Value : Type u) where
  events : List Event
  value : Value
  deriving DecidableEq, Repr

instance : Monad Evaluation where
  pure value := ⟨[], value⟩
  bind earlier next :=
    let later := next earlier.value
    ⟨earlier.events ++ later.events, later.value⟩

/-- Record one callback event without constraining the value universe. -/
def emit.{v} (event : Event) : Evaluation.{v} (PUnit.{v + 1}) :=
  ⟨[event], PUnit.unit⟩

/-- A receiver thunk records one evaluation when the access invokes it. -/
def traceReceiver {Value : Type u} (value : Value) : Unit → Evaluation Value :=
  fun _ => do
    let _ ← emit .receiver
    pure value

/-- An argument thunk records its position when the access invokes it. -/
def traceArgument {Value : Type u} (position : Nat) (value : Value) :
    Unit → Evaluation Value :=
  fun _ => do
    let _ ← emit (.argument position)
    pure value

/-- A nullable field access skips the field callback for a null receiver and
wraps the ordinary field result for a present receiver. -/
def optionalField {Receiver Value : Type u}
    (receiver : Unit → Evaluation (Option Receiver))
    (field : Receiver → Evaluation Value) : Evaluation (Option Value) := do
  let receiverValue ← receiver ()
  match receiverValue with
  | none => pure none
  | some value =>
      let fieldValue ← field value
      pure (some fieldValue)

/-- A nullable method access evaluates its arguments only after the receiver is
known to be present, then wraps the ordinary method result. -/
def optionalMethod {Receiver Argument Value : Type u}
    (receiver : Unit → Evaluation (Option Receiver))
    (argument : Unit → Evaluation Argument)
    (method : Receiver → Argument → Evaluation Value) : Evaluation (Option Value) := do
  let receiverValue ← receiver ()
  match receiverValue with
  | none => pure none
  | some value =>
      let argumentValue ← argument ()
      let methodValue ← method value argumentValue
      pure (some methodValue)

/-- A nullable index access skips its index expression for a null receiver and
wraps the ordinary indexed value for a present receiver. -/
def optionalIndex {Receiver Index Value : Type u}
    (receiver : Unit → Evaluation (Option Receiver))
    (indexExpression : Unit → Evaluation Index)
    (index : Receiver → Index → Evaluation Value) : Evaluation (Option Value) := do
  let receiverValue ← receiver ()
  match receiverValue with
  | none => pure none
  | some value =>
      let indexValue ← indexExpression ()
      let indexedValue ← index value indexValue
      pure (some indexedValue)

/-- A nullable range access evaluates its bounds from left to right only after
the receiver is known to be present, then wraps the ordinary range result. -/
def optionalRange {Receiver Start Stop Value : Type u}
    (receiver : Unit → Evaluation (Option Receiver))
    (startExpression : Unit → Evaluation Start)
    (stopExpression : Unit → Evaluation Stop)
    (range : Receiver → Start → Stop → Evaluation Value) :
    Evaluation (Option Value) := do
  let receiverValue ← receiver ()
  match receiverValue with
  | none => pure none
  | some value =>
      let startValue ← startExpression ()
      let stopValue ← stopExpression ()
      let rangeValue ← range value startValue stopValue
      pure (some rangeValue)

/-- A Result receiver propagates Err through the nearest `?` boundary before
running its ordinary access callback. -/
def resultAccess {Error Receiver Value Target : Type u}
    (stack : Xsh.Results.Propagation.BoundaryStack Error Target)
    (receiver : Unit → Evaluation (Except Error Receiver))
    (access : Receiver → Evaluation Value) :
    Evaluation (Xsh.Results.Propagation.Exit Error Value Target) := do
  let result ← receiver ()
  match result with
  | .error error =>
      pure (Xsh.Results.Propagation.propagateResult stack (.error error))
  | .ok value =>
      let accessedValue ← access value
      pure (.value accessedValue stack)

/-- A Result method's arguments run only after its receiver has propagated
successfully. Its result remains the ordinary, unoptionalized value. -/
def resultMethod {Error Receiver Argument Value Target : Type u}
    (stack : Xsh.Results.Propagation.BoundaryStack Error Target)
    (receiver : Unit → Evaluation (Except Error Receiver))
    (argument : Unit → Evaluation Argument)
    (method : Receiver → Argument → Evaluation Value) :
    Evaluation (Xsh.Results.Propagation.Exit Error Value Target) :=
  resultAccess stack receiver fun value => do
    let argumentValue ← argument ()
    method value argumentValue

/-- Apply the Result question hop first, then one nullable method hop to its
optional payload. This models `(result?)?.method(argument)`. -/
def resultThenOptionalMethod
    {Error Receiver Argument Value Target : Type u}
    (stack : Xsh.Results.Propagation.BoundaryStack Error Target)
    (receiver : Unit → Evaluation (Except Error (Option Receiver)))
    (argument : Unit → Evaluation Argument)
    (method : Receiver → Argument → Evaluation Value) :
    Evaluation (Xsh.Results.Propagation.Exit Error (Option Value) Target) :=
  resultAccess stack receiver fun optionalValue =>
    optionalMethod (fun _ => pure optionalValue) argument method

/-- Mutant that evaluates a method argument before checking a nullable
receiver. -/
def eagerArgumentOptionalMethod {Receiver Argument Value : Type u}
    (receiver : Unit → Evaluation (Option Receiver))
    (argument : Unit → Evaluation Argument)
    (method : Receiver → Argument → Evaluation Value) : Evaluation (Option Value) := do
  let receiverValue ← receiver ()
  let argumentValue ← argument ()
  match receiverValue with
  | none => pure none
  | some value =>
      let methodValue ← method value argumentValue
      pure (some methodValue)

theorem null_field_returns_null_without_reading_field :
    optionalField (traceReceiver (none : Option Nat))
      (fun _ => do
        let _ ← emit .field
        pure 8) = ⟨[.receiver], none⟩ := by
  rfl

theorem present_field_evaluates_receiver_once_and_optionalizes_value :
    optionalField (traceReceiver (some 11 : Option Nat))
      (fun value => do
        let _ ← emit .field
        pure (value + 1)) = ⟨[.receiver, .field], some 12⟩ := by
  rfl

theorem null_method_skips_argument_and_call :
    optionalMethod (traceReceiver (none : Option Nat))
      (traceArgument 0 5)
      (fun _ _ => do
        let _ ← emit .method
        pure 99) = ⟨[.receiver], none⟩ := by
  rfl

theorem present_method_evaluates_once_in_source_order_and_optionalizes :
    optionalMethod (traceReceiver (some 10 : Option Nat))
      (traceArgument 0 5)
      (fun receiver argument => do
        let _ ← emit .method
        pure (receiver + argument)) =
      ⟨[.receiver, .argument 0, .method], some 15⟩ := by
  rfl

theorem null_index_skips_expression_and_access :
    optionalIndex (traceReceiver (none : Option Nat))
      (traceArgument 0 4)
      (fun _ _ => do
        let _ ← emit .index
        pure 99) = ⟨[.receiver], none⟩ := by
  rfl

theorem present_index_evaluates_expression_then_optionalizes_value :
    optionalIndex (traceReceiver (some 10 : Option Nat))
      (traceArgument 0 2)
      (fun receiver index => do
        let _ ← emit .index
        pure (receiver + index)) =
      ⟨[.receiver, .argument 0, .index], some 12⟩ := by
  rfl

theorem null_range_skips_bounds_and_access :
    optionalRange (traceReceiver (none : Option Nat))
      (traceArgument 0 2)
      (traceArgument 1 3)
      (fun _ _ _ => do
        let _ ← emit .range
        pure 99) = ⟨[.receiver], none⟩ := by
  rfl

theorem present_range_evaluates_bounds_in_order_and_optionalizes_value :
    optionalRange (traceReceiver (some 4 : Option Nat))
      (traceArgument 0 2)
      (traceArgument 1 3)
      (fun receiver start stop => do
        let _ ← emit .range
        pure (receiver + start + stop)) =
      ⟨[.receiver, .argument 0, .argument 1, .range], some 9⟩ := by
  rfl

theorem result_error_propagates_before_method_arguments :
    resultMethod
      (.push .tryBlock .topLevel :
        Xsh.Results.Propagation.BoundaryStack String Unit)
      (traceReceiver (.error "missing" : Except String Nat))
      (traceArgument 0 4)
      (fun _ _ => do
        let _ ← emit .method
        pure 8) =
      ⟨[.receiver], .tryError "missing" .question⟩ := by
  rfl

theorem result_ok_method_result_is_not_optionalized :
    resultMethod
      (.topLevel : Xsh.Results.Propagation.BoundaryStack String Unit)
      (traceReceiver (.ok 7 : Except String Nat))
      (traceArgument 0 2)
      (fun receiver argument => do
        let _ ← emit .method
        pure (receiver + argument)) =
      ⟨[.receiver, .argument 0, .method], .value 9 .topLevel⟩ := by
  rfl

theorem result_optional_payload_uses_question_then_optional_hop :
    let stack :=
      (.push .tryBlock .topLevel :
        Xsh.Results.Propagation.BoundaryStack String Unit)
    Xsh.Results.Propagation.propagateResult stack
        (.ok (some 7 : Option Nat)) =
        (.value (some 7) stack :
          Xsh.Results.Propagation.Exit String (Option Nat) Unit) ∧
      resultThenOptionalMethod stack
        (traceReceiver (.ok (some 7) : Except String (Option Nat)))
        (traceArgument 0 2)
        (fun receiver argument => do
          let _ ← emit .method
          pure (receiver + argument)) =
        ⟨[.receiver, .argument 0, .method],
          .value (some 9) stack⟩ := by
  constructor
  · exact Xsh.Results.Propagation.ok_propagation_stays_inside_boundary (some 7)
  · rfl

theorem eager_argument_mutation_is_detectable :
    let receiver := traceReceiver (none : Option Nat)
    let argument := traceArgument 0 5
    let method := fun (_ : Nat) (_ : Nat) => do
      let _ ← emit .method
      pure 8
    optionalMethod receiver argument method = ⟨[.receiver], none⟩ ∧
      eagerArgumentOptionalMethod receiver argument method =
        ⟨[.receiver, .argument 0], none⟩ ∧
      optionalMethod receiver argument method ≠
        eagerArgumentOptionalMethod receiver argument method := by
  decide

end Xsh.Control.OptionalAccess
