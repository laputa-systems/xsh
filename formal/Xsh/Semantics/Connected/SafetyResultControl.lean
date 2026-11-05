import Xsh.Semantics.Connected.Witness
import Xsh.Conformance.Connected.RawResultFixture

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Syntax.Foundation
open Xsh.Values
open Xsh.Typing.Connected
open Xsh.Conformance.Connected

theorem ok_scheme_instantiates (solution : Substitution) :
    instantiateFunction solution okScheme resultInstance =
      some (.argument .integer .argumentsEmpty, .result .integer .string, resultInstance) := by
  have different : ((⟨1, .value⟩ : Variable) == ⟨0, .value⟩) = false := by decide
  simp [instantiateFunction, resultInstance, instantiationKeysValid, instantiationSortsValid,
    witnessSubstitution, schemeSubstitution, okScheme, identityVariable, resultValueVariable,
    substitute, checksConstraints, checksWellFormed, typeSort, VariableKind.sort, monoRuntime, List.lookup, different]

theorem error_scheme_instantiates (solution : Substitution) :
    instantiateFunction solution errorScheme resultInstance =
      some (.argument .string .argumentsEmpty, .result .integer .string, resultInstance) := by
  have different : ((⟨1, .value⟩ : Variable) == ⟨0, .value⟩) = false := by decide
  simp [instantiateFunction, resultInstance, instantiationKeysValid, instantiationSortsValid,
    witnessSubstitution, schemeSubstitution, errorScheme, identityVariable, resultValueVariable,
    substitute, checksConstraints, checksWellFormed, typeSort, VariableKind.sort, monoRuntime, List.lookup, different]

structure ResultBuiltins (state : Machine) where
  okDeclaration : DeclarationId
  errorDeclaration : DeclarationId
  okFound : Connected.read state.store state.cells resultOkBinder = some (.schemeFunction okDeclaration okScheme)
  okFunction : state.functions okDeclaration = some .resultOk
  errorFound : Connected.read state.store state.cells resultErrorBinder =
    some (.schemeFunction errorDeclaration errorScheme)
  errorFunction : state.functions errorDeclaration = some .resultError

def okTarget (builtins : ResultBuiltins state) : Value :=
  .instantiatedFunction builtins.okDeclaration (.argument .integer .argumentsEmpty) (.result .integer .string)
    state.solution resultInstance

def errorTarget (builtins : ResultBuiltins state) : Value :=
  .instantiatedFunction builtins.errorDeclaration (.argument .string .argumentsEmpty) (.result .integer .string)
    state.solution resultInstance

theorem ok_construction_executes (state : Machine) (builtins : ResultBuiltins state) (value : Value)
    (argumentFound : Connected.read state.store state.cells resultFirstBinder = some value)
    (ordinary : ordinaryValue value = true) (integer : HasType .integer value) :
    Execution { state with control := .expression (okNode (.read resultFirstBinder [])) }
      [.internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .value (.ok value .string) } := by
  let withTarget := { state with stack := .callTarget [.read resultFirstBinder []] :: state.stack }
  let withArgument := { state with stack := .callArguments (okTarget builtins) [] [] :: state.stack }
  refine .next (Step.callStart state (.read resultOkBinder resultInstance) [.read resultFirstBinder []]) ?_
  refine .next (Step.readFunction withTarget resultOkBinder resultInstance builtins.okDeclaration okScheme _ _
    resultInstance builtins.okFound (ok_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument state (okTarget builtins) (.read resultFirstBinder []) []) ?_
  refine .next (Step.read withArgument resultFirstBinder [] value argumentFound ordinary) ?_
  refine .next (Step.callArgumentsComplete state (okTarget builtins) value []) ?_
  exact .next (Step.callOkInstantiated state builtins.okDeclaration value .integer .string state.solution
    resultInstance builtins.okFunction integer) (.done _)

theorem error_construction_executes (state : Machine) (builtins : ResultBuiltins state) :
    Execution { state with control := .expression (errorNode (.string "no")) }
      [.internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .value (.error (.scalar (.string "no")) .integer) } := by
  let withTarget := { state with stack := .callTarget [.string "no"] :: state.stack }
  let withArgument := { state with stack := .callArguments (errorTarget builtins) [] [] :: state.stack }
  refine .next (Step.callStart state (.read resultErrorBinder resultInstance) [.string "no"]) ?_
  refine .next (Step.readFunction withTarget resultErrorBinder resultInstance builtins.errorDeclaration errorScheme _ _
    resultInstance builtins.errorFound (error_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument state (errorTarget builtins) (.string "no") []) ?_
  refine .next (Step.string withArgument "no") ?_
  refine .next (Step.callArgumentsComplete state (errorTarget builtins) (.scalar (.string "no")) []) ?_
  exact .next (Step.callErrorInstantiated state builtins.errorDeclaration (.scalar (.string "no")) .integer .string
    state.solution resultInstance builtins.errorFunction (by decide)) (.done _)

theorem propagating_a_success_continues (state : Machine) (builtins : ResultBuiltins state) (value : Value)
    (argumentFound : Connected.read state.store state.cells resultFirstBinder = some (.ok value .string))
    (integer : HasType .integer value) :
    Execution
      { state with control := .expression (okNode (.propagate (.read resultFirstBinder []) (.primitive .string))) }
      [.internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .value (.ok value .string) } := by
  let withTarget := { state with
    stack := .callTarget [.propagate (.read resultFirstBinder []) (.primitive .string)] :: state.stack }
  let withArgument := { state with stack := .callArguments (okTarget builtins) [] [] :: state.stack }
  let withPropagate := { withArgument with stack := .propagate :: withArgument.stack }
  refine .next (Step.callStart state (.read resultOkBinder resultInstance)
    [.propagate (.read resultFirstBinder []) (.primitive .string)]) ?_
  refine .next (Step.readFunction withTarget resultOkBinder resultInstance builtins.okDeclaration okScheme _ _
    resultInstance builtins.okFound (ok_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument state (okTarget builtins)
    (.propagate (.read resultFirstBinder []) (.primitive .string)) []) ?_
  refine .next (Step.propagateStart withArgument (.read resultFirstBinder []) (.primitive .string)) ?_
  refine .next (Step.read withPropagate resultFirstBinder [] (.ok value .string) argumentFound rfl) ?_
  refine .next (Step.propagateOk withArgument value .string) ?_
  refine .next (Step.callArgumentsComplete state (okTarget builtins) value []) ?_
  exact .next (Step.callOkInstantiated state builtins.okDeclaration value .integer .string state.solution
    resultInstance builtins.okFunction integer) (.done _)

theorem propagating_a_failure_leaves_the_caller_with_the_failure (state : Machine) (builtins : ResultBuiltins state)
    (failure : Value) (cells : ActiveCells) (activation : Nat) (saved : Substitution)
    (argumentFound : Connected.read state.store state.cells resultFirstBinder = some (.error failure .integer))
    (typed : HasType .string failure) :
    Execution
      { state with
        control := .expression (okNode (.propagate (.read resultFirstBinder []) (.primitive .string))),
        stack := .caller cells activation saved (.result .integer .string) :: state.stack }
      [.internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .value (.error failure .integer), cells := cells, activation := activation,
                   solution := saved } := by
  let caller := Frame.caller cells activation saved (.result .integer .string)
  let outer := { state with stack := caller :: state.stack }
  let withTarget := { outer with
    stack := .callTarget [.propagate (.read resultFirstBinder []) (.primitive .string)] :: outer.stack }
  let withArgument := { outer with stack := .callArguments (okTarget builtins) [] [] :: outer.stack }
  let withPropagate := { withArgument with stack := .propagate :: withArgument.stack }
  refine .next (Step.callStart outer (.read resultOkBinder resultInstance)
    [.propagate (.read resultFirstBinder []) (.primitive .string)]) ?_
  refine .next (Step.readFunction withTarget resultOkBinder resultInstance builtins.okDeclaration okScheme _ _
    resultInstance builtins.okFound (ok_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument outer (okTarget builtins)
    (.propagate (.read resultFirstBinder []) (.primitive .string)) []) ?_
  refine .next (Step.propagateStart withArgument (.read resultFirstBinder []) (.primitive .string)) ?_
  refine .next (Step.read withPropagate resultFirstBinder [] (.error failure .integer) argumentFound rfl) ?_
  refine .next (Step.propagateError withArgument failure .integer) ?_
  refine .next (Step.propagateUnwind outer (.error failure .integer) (.callArguments (okTarget builtins) [] [])
    rfl) ?_
  exact .next (Step.propagatedToCaller state failure .integer .integer .string cells activation saved typed)
    (.done _)

theorem propagation_discards_the_rest_of_the_expression (state : Machine) (builtins : ResultBuiltins state)
    (failure : Value) (cells : ActiveCells) (activation : Nat) (saved : Substitution)
    (argumentFound : Connected.read state.store state.cells resultFirstBinder = some (.error failure .integer))
    (typed : HasType .string failure) :
    Execution
      { state with
        control := .expression (okNode (.binary .add
          (.propagate (.read resultFirstBinder []) (.primitive .string)) (.integer "1"))),
        stack := .caller cells activation saved (.result .integer .string) :: state.stack }
      [.internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal,
        .internal]
      { state with control := .value (.error failure .integer), cells := cells, activation := activation,
                   solution := saved } := by
  let caller := Frame.caller cells activation saved (.result .integer .string)
  let outer := { state with stack := caller :: state.stack }
  let operand := Node.binary .add (.propagate (.read resultFirstBinder []) (.primitive .string)) (.integer "1")
  let withTarget := { outer with stack := .callTarget [operand] :: outer.stack }
  let withArgument := { outer with stack := .callArguments (okTarget builtins) [] [] :: outer.stack }
  let withLeft := { withArgument with stack := .binaryLeft .add (.integer "1") :: withArgument.stack }
  let withPropagate := { withLeft with stack := .propagate :: withLeft.stack }
  refine .next (Step.callStart outer (.read resultOkBinder resultInstance) [operand]) ?_
  refine .next (Step.readFunction withTarget resultOkBinder resultInstance builtins.okDeclaration okScheme _ _
    resultInstance builtins.okFound (ok_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument outer (okTarget builtins) operand []) ?_
  refine .next (Step.binary withArgument .add
    (.propagate (.read resultFirstBinder []) (.primitive .string)) (.integer "1")) ?_
  refine .next (Step.propagateStart withLeft (.read resultFirstBinder []) (.primitive .string)) ?_
  refine .next (Step.read withPropagate resultFirstBinder [] (.error failure .integer) argumentFound rfl) ?_
  refine .next (Step.propagateError withLeft failure .integer) ?_
  refine .next (Step.propagateUnwind (withArgument) (.error failure .integer) (.binaryLeft .add (.integer "1"))
    rfl) ?_
  refine .next (Step.propagateUnwind outer (.error failure .integer) (.callArguments (okTarget builtins) [] [])
    rfl) ?_
  exact .next (Step.propagatedToCaller state failure .integer .integer .string cells activation saved typed)
    (.done _)

theorem propagating_a_success_feeds_the_rest_of_the_expression (state : Machine) (builtins : ResultBuiltins state)
    (argumentFound : Connected.read state.store state.cells resultFirstBinder =
      some (.ok (.scalar (.integer intOne)) .string)) :
    Execution
      { state with control := .expression (okNode (.binary .add
        (.propagate (.read resultFirstBinder []) (.primitive .string)) (.integer "1"))) }
      [.internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal,
        .internal, .internal, .internal]
      { state with control := .value (.ok (.scalar (.integer integerTwo)) .string) } := by
  let operand := Node.binary .add (.propagate (.read resultFirstBinder []) (.primitive .string)) (.integer "1")
  let withTarget := { state with stack := .callTarget [operand] :: state.stack }
  let withArgument := { state with stack := .callArguments (okTarget builtins) [] [] :: state.stack }
  let withLeft := { withArgument with stack := .binaryLeft .add (.integer "1") :: withArgument.stack }
  let withPropagate := { withLeft with stack := .propagate :: withLeft.stack }
  let withRight := { withArgument with
    stack := .binaryRight .add (.scalar (.integer intOne)) :: withArgument.stack }
  refine .next (Step.callStart state (.read resultOkBinder resultInstance) [operand]) ?_
  refine .next (Step.readFunction withTarget resultOkBinder resultInstance builtins.okDeclaration okScheme _ _
    resultInstance builtins.okFound (ok_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument state (okTarget builtins) operand []) ?_
  refine .next (Step.binary withArgument .add
    (.propagate (.read resultFirstBinder []) (.primitive .string)) (.integer "1")) ?_
  refine .next (Step.propagateStart withLeft (.read resultFirstBinder []) (.primitive .string)) ?_
  refine .next (Step.read withPropagate resultFirstBinder [] (.ok (.scalar (.integer intOne)) .string)
    argumentFound rfl) ?_
  refine .next (Step.propagateOk withLeft (.scalar (.integer intOne)) .string) ?_
  refine .next (Step.binaryLeft withArgument .add (.scalar (.integer intOne)) (.integer "1")
    (by decide)) ?_
  refine .next (Step.integer withRight "1" intOne (by decide +kernel)) ?_
  refine .next (Step.binaryResult withArgument .add (.scalar (.integer intOne)) (.scalar (.integer intOne))
    (.scalar (.integer integerTwo)) rfl) ?_
  refine .next (Step.callArgumentsComplete state (okTarget builtins) (.scalar (.integer integerTwo)) []) ?_
  exact .next (Step.callOkInstantiated state builtins.okDeclaration (.scalar (.integer integerTwo)) .integer .string
    state.solution resultInstance builtins.okFunction (by decide +kernel)) (.done _)

theorem result_control_returns_are_the_executed_nodes :
    resultControlCertified = true ∧
    returnedNode keepCandidate = some (okNode (.read resultFirstBinder [])) ∧
    returnedNode refuseCandidate = some (errorNode (.string "no")) ∧
    returnedNode throughCandidate =
      some (okNode (.propagate (.read resultFirstBinder []) (.primitive .string))) ∧
    returnedNode advanceCandidate =
      some (okNode (.binary .add (.propagate (.read resultFirstBinder []) (.primitive .string)) (.integer "1"))) :=
  ⟨result_control_is_certified, rfl, rfl, rfl, rfl⟩

end Xsh.Semantics.Connected
