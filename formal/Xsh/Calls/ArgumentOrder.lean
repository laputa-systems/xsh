/-!
This fragment orders events for an already-checked call layout. Event IDs name
pre-evaluated expressions; parameter IDs are already resolved, and a record
spread carries exactly the fields visible at its checked type. It models call
ordering and argument expansion only, not expression evaluation or checking.
-/

namespace Xsh.Calls.ArgumentOrder

abbrev EventId := Nat
abbrev ParameterId := Nat
abbrev ValueId := Nat

/-- An entry is present even when its value is null; `none` represents null. -/
structure SuppliedValue where
  parameter : ParameterId
  value : Option ValueId
  deriving DecidableEq, Repr

/-- An expanded slot retains the event of its written argument expression. -/
structure SuppliedOccurrence where
  sourceEvent : EventId
  value : SuppliedValue
  deriving DecidableEq, Repr

/-- A default records the event that evaluates its expression. -/
structure Parameter where
  id : ParameterId
  defaultEvent? : Option EventId
  deriving DecidableEq, Repr

/-- One written argument expression and the slots its checked shape supplies. -/
inductive ArgumentSource where
  | single (event : EventId) (value : SuppliedValue)
  | listSplice (event : EventId) (items : List SuppliedValue)
  | recordSpread (event : EventId) (visibleFields : List SuppliedValue)
  deriving DecidableEq, Repr

/-- The checked slot mapping is retained while source expressions stay ordered. -/
structure CheckedCallLayout where
  receiverEvent? : Option EventId
  parameters : List Parameter
  arguments : List ArgumentSource
  deriving DecidableEq, Repr

/-- Each argument source is evaluated once, regardless of its expansion. -/
def ArgumentSource.event : ArgumentSource → EventId
  | .single event _ => event
  | .listSplice event _ => event
  | .recordSpread event _ => event

/-- A splice or spread contributes its already-evaluated entries in their order. -/
def ArgumentSource.values : ArgumentSource → List SuppliedValue
  | .single _ value => [value]
  | .listSplice _ items => items
  | .recordSpread _ visibleFields => visibleFields

/-- Expanded slots retain the event of the one source expression that produced them. -/
def ArgumentSource.occurrences : ArgumentSource → List SuppliedOccurrence
  | .single event value => [{ sourceEvent := event, value := value }]
  | .listSplice event items =>
      items.map (fun value => { sourceEvent := event, value := value })
  | .recordSpread event visibleFields =>
      visibleFields.map (fun value => { sourceEvent := event, value := value })

/-- Argument source events remain in the order the expressions were written. -/
def sourceEvents (arguments : List ArgumentSource) : List EventId :=
  arguments.map ArgumentSource.event

/-- Expanded values retain the source order and each splice or spread order. -/
def suppliedValues (arguments : List ArgumentSource) : List SuppliedValue :=
  arguments.flatMap ArgumentSource.values

/-- Expanded slots retain their originating argument event in written order. -/
def suppliedOccurrences (arguments : List ArgumentSource) : List SuppliedOccurrence :=
  arguments.flatMap ArgumentSource.occurrences

/-- A parameter is supplied by slot presence, independently of its value. -/
def hasSuppliedValue (values : List SuppliedValue) (parameter : ParameterId) : Bool :=
  values.any (fun value => value.parameter == parameter)

/-- Select a default only when the checked layout leaves that parameter absent. -/
def selectedDefaultEvent (parameter : Parameter) (values : List SuppliedValue) :
    Option EventId :=
  if hasSuppliedValue values parameter.id then none else parameter.defaultEvent?

/-- Defaults are selected in declaration order after all written arguments. -/
def defaultEvents (parameters : List Parameter) (values : List SuppliedValue) :
    List EventId :=
  parameters.filterMap (fun parameter => selectedDefaultEvent parameter values)

/-- Receiver, written argument expressions, then omitted defaults form the event trace. -/
def evaluationEvents (layout : CheckedCallLayout) : List EventId :=
  layout.receiverEvent?.toList ++ sourceEvents layout.arguments ++
    defaultEvents layout.parameters (suppliedValues layout.arguments)

/-- A candidate faulty order that evaluates supplied values by parameter order. -/
def parameterOrderArgumentEvents (parameters : List Parameter)
    (arguments : List ArgumentSource) : List EventId :=
  parameters.filterMap (fun parameter =>
    match (suppliedOccurrences arguments).find?
        (fun occurrence => occurrence.value.parameter == parameter.id) with
    | some occurrence => some occurrence.sourceEvent
    | none => none)

theorem each_argument_source_has_one_evaluation (argument : ArgumentSource) :
    sourceEvents [argument] = [argument.event] := by
  cases argument <;> rfl

theorem source_events_preserve_written_append_order
    (before after : List ArgumentSource) :
    sourceEvents (before ++ after) = sourceEvents before ++ sourceEvents after := by
  simp [sourceEvents]

theorem supplied_values_preserve_written_append_order
    (before after : List ArgumentSource) :
    suppliedValues (before ++ after) = suppliedValues before ++ suppliedValues after := by
  simp [suppliedValues]

theorem list_splice_preserves_element_order
    (event : EventId) (items : List SuppliedValue) :
    ArgumentSource.values (.listSplice event items) = items := by
  rfl

theorem record_spread_preserves_visible_fields_once
    (event : EventId) (visibleFields : List SuppliedValue) :
    ArgumentSource.values (.recordSpread event visibleFields) = visibleFields := by
  rfl

theorem list_splice_evaluates_its_source_once
    (event : EventId) (items : List SuppliedValue) :
    sourceEvents [.listSplice event items] = [event] := by
  rfl

theorem record_spread_evaluates_its_source_once
    (event : EventId) (visibleFields : List SuppliedValue) :
    sourceEvents [.recordSpread event visibleFields] = [event] := by
  rfl

theorem receiver_precedes_written_arguments_and_defaults
    (receiver : EventId) (parameters : List Parameter)
    (arguments : List ArgumentSource) :
    evaluationEvents
        { receiverEvent? := some receiver, parameters := parameters, arguments := arguments } =
      receiver :: (sourceEvents arguments ++
        defaultEvents parameters (suppliedValues arguments)) := by
  rfl

theorem receiverless_call_orders_arguments_before_defaults
    (parameters : List Parameter) (arguments : List ArgumentSource) :
    evaluationEvents
        { receiverEvent? := none, parameters := parameters, arguments := arguments } =
      sourceEvents arguments ++ defaultEvents parameters (suppliedValues arguments) := by
  rfl

theorem default_events_follow_parameter_append_order
    (first second : List Parameter) (values : List SuppliedValue) :
    defaultEvents (first ++ second) values =
      defaultEvents first values ++ defaultEvents second values := by
  simp [defaultEvents]

theorem supplied_parameter_suppresses_its_default
    (parameter : Parameter) (values : List SuppliedValue)
    (present : hasSuppliedValue values parameter.id = true) :
    selectedDefaultEvent parameter values = none := by
  simp [selectedDefaultEvent, present]

theorem omitted_parameter_uses_its_default
    (parameter : Parameter) (values : List SuppliedValue)
    (absent : hasSuppliedValue values parameter.id = false) :
    selectedDefaultEvent parameter values = parameter.defaultEvent? := by
  simp [selectedDefaultEvent, absent]

theorem explicit_null_suppresses_default
    (parameter : ParameterId) (defaultEvent : EventId) :
    selectedDefaultEvent
        { id := parameter, defaultEvent? := some defaultEvent }
        [{ parameter := parameter, value := none }] = none := by
  simp [selectedDefaultEvent, hasSuppliedValue]

def namedArgumentReorderingWitness : CheckedCallLayout :=
  { receiverEvent? := none
    parameters :=
      [{ id := 0, defaultEvent? := none }, { id := 1, defaultEvent? := none }]
    arguments :=
      [ .single 21 { parameter := 1, value := some 1021 }
      , .single 20 { parameter := 0, value := some 1020 }
      ] }

/-- Sorting supplied named events by slot changes their observable evaluation order. -/
theorem parameter_order_mutation_reorders_source_event_trace :
    sourceEvents namedArgumentReorderingWitness.arguments = [21, 20] ∧
      parameterOrderArgumentEvents namedArgumentReorderingWitness.parameters
        namedArgumentReorderingWitness.arguments = [20, 21] ∧
      sourceEvents namedArgumentReorderingWitness.arguments ≠
        parameterOrderArgumentEvents namedArgumentReorderingWitness.parameters
          namedArgumentReorderingWitness.arguments := by
  decide

end Xsh.Calls.ArgumentOrder
