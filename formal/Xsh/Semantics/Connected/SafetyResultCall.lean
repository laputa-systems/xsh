import Xsh.Semantics.Connected.SafetyResultControl
import Xsh.Conformance.Connected.RawResultCallLiteralProofs

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Syntax.Foundation
open Xsh.Values
open Xsh.Typing.Connected
open Xsh.Conformance.Connected
open Xsh.Conformance.Transport

def wrapScheme : Scheme :=
  ⟨[], .function (.argument (.primitive .integer) .argumentsEmpty) (.result (.primitive .integer) (.primitive .string)),
    []⟩

theorem wrap_scheme_is_the_published_signature :
    ((resultFunctionBindingsFor resultCallSourceNames resultCallSignatures).bind List.head?).map
      (fun binding => (binding.binder, binding.scheme.quantified, binding.scheme.body)) =
      some (wrapBinder, wrapScheme.quantified, wrapScheme.body) := by
  decide +kernel

theorem wrap_scheme_instantiates (solution : Substitution) :
    instantiateFunction solution wrapScheme [] =
      some (.argument .integer .argumentsEmpty, .result .integer .string, []) := by
  simp [instantiateFunction, wrapScheme, instantiationKeysValid, instantiationSortsValid, substitute,
    checksConstraints, monoRuntime]

def callOne : Value := .scalar (.integer intOne)

theorem call_one_is_an_integer : HasType .integer callOne := by
  decide +kernel

def liftNode : Node := okNode (.propagate (.call (.read wrapBinder []) [.integer "1"]) (.primitive .string))

structure OkBuiltin (state : Machine) where
  okDeclaration : DeclarationId
  okFound : Connected.read state.store state.cells resultOkBinder = some (.schemeFunction okDeclaration okScheme)
  okFunction : state.functions okDeclaration = some .resultOk

structure ErrorBuiltin (state : Machine) where
  errorDeclaration : DeclarationId
  errorFound : Connected.read state.store state.cells resultErrorBinder =
    some (.schemeFunction errorDeclaration errorScheme)
  errorFunction : state.functions errorDeclaration = some .resultError
  errorVisible : state.globals resultErrorBinder = state.cells resultErrorBinder

def okCallTarget (ok : OkBuiltin state) : Value :=
  .instantiatedFunction ok.okDeclaration (.argument .integer .argumentsEmpty) (.result .integer .string)
    state.solution resultInstance

theorem ok_builtin_constructs (state : Machine) (ok : OkBuiltin state) (value : Value)
    (argumentFound : Connected.read state.store state.cells resultFirstBinder = some value)
    (ordinary : ordinaryValue value = true) (integer : HasType .integer value) :
    Execution { state with control := .expression (okNode (.read resultFirstBinder [])) }
      (List.replicate 6 .internal) { state with control := .value (.ok value .string) } := by
  let withTarget := { state with stack := .callTarget [.read resultFirstBinder []] :: state.stack }
  let withArgument := { state with stack := .callArguments (okCallTarget ok) [] [] :: state.stack }
  refine .next (Step.callStart state (.read resultOkBinder resultInstance) [.read resultFirstBinder []]) ?_
  refine .next (Step.readFunction withTarget resultOkBinder resultInstance ok.okDeclaration okScheme _ _
    resultInstance ok.okFound (ok_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument state (okCallTarget ok) (.read resultFirstBinder []) []) ?_
  refine .next (Step.read withArgument resultFirstBinder [] value argumentFound ordinary) ?_
  refine .next (Step.callArgumentsComplete state (okCallTarget ok) value []) ?_
  exact .next (Step.callOkInstantiated state ok.okDeclaration value .integer .string state.solution
    resultInstance ok.okFunction integer) (.done _)

structure ResultCallWorld (state : Machine) (body : List StatementNode) where
  ok : OkBuiltin state
  wrapDeclaration : DeclarationId
  wrapFunction : Function
  wrapFound : Connected.read state.store state.cells wrapBinder = some (.schemeFunction wrapDeclaration wrapScheme)
  wrapTable : state.functions wrapDeclaration = some (.function wrapFunction)
  wrapSchemeIs : wrapFunction.scheme = wrapScheme
  wrapParameters : wrapFunction.parameters = [resultFirstBinder]
  wrapBody : wrapFunction.body = body
  okVisible : state.globals resultOkBinder = state.cells resultOkBinder
  fresh : state.store ⟨state.nextActivation, resultFirstBinder⟩ = none

def calleeCell (state : Machine) : Environments.CellId := ⟨state.nextActivation, resultFirstBinder⟩

def calleeStore (state : Machine) : Store := fun other =>
  if other = calleeCell state then some ⟨⟨.integer, callOne, call_one_is_an_integer⟩, .immutable⟩
  else state.store other

def calleeCells (state : Machine) : ActiveCells :=
  withGlobals (bindActive emptyCells resultFirstBinder (calleeCell state)) state.globals

theorem wrap_parameters_bind (state : Machine) {body : List StatementNode} (world : ResultCallWorld state body) :
    bindParameters state.nextActivation [resultFirstBinder] [callOne] (.argument .integer .argumentsEmpty)
      state.store = .ok (bindActive emptyCells resultFirstBinder (calleeCell state), calleeStore state) := by
  have fresh : state.store (calleeCell state) = none := world.fresh
  have allocated : allocate state.store (calleeCell state) .immutable .integer callOne = some (calleeStore state) := by
    simp [allocate, fresh, call_one_is_an_integer]
    rfl
  simp [calleeCell] at allocated
  simp [bindParameters, call_one_is_an_integer, calleeCell, allocated]
  rfl

theorem a_callee_parameter_of_another_type_does_not_bind (activation : Nat) (store : Store) :
    bindParameters activation [resultFirstBinder] [callOne] (.argument .string .argumentsEmpty) store =
      .error .wrongArguments := by
  have mistyped : ¬ HasType .string callOne := by decide
  simp [bindParameters, mistyped]
  rfl

theorem callee_reads_its_parameter (state : Machine) :
    Connected.read (calleeStore state) (calleeCells state) resultFirstBinder = some callOne := by
  simp [Connected.read, calleeCells, withGlobals, bindActive, calleeStore]

theorem callee_reads_a_caller_global (state : Machine) (binder : BinderId) (value : Value)
    (other : binder ≠ resultFirstBinder) (visible : state.globals binder = state.cells binder)
    (fresh : state.store (calleeCell state) = none)
    (found : Connected.read state.store state.cells binder = some value) :
    Connected.read (calleeStore state) (calleeCells state) binder = some value := by
  unfold Connected.read at found ⊢
  cases cell : state.cells binder with
  | none => simp [cell] at found
  | some id =>
    cases entry : state.store id with
    | none => simp [cell, entry] at found
    | some contents =>
      have distinct : id ≠ calleeCell state := by
        intro same
        rw [same, fresh] at entry
        cases entry
      simp [cell, entry] at found
      simp [calleeCells, withGlobals, bindActive, emptyCells, other, visible, cell, calleeStore, distinct, entry, found]

def callResumed (state : Machine) : Machine :=
  { state with store := calleeStore state, nextActivation := state.nextActivation + 1 }

def wrapTarget (state : Machine) {body : List StatementNode} (world : ResultCallWorld state body) : Value :=
  .instantiatedFunction world.wrapDeclaration (.argument .integer .argumentsEmpty) (.result .integer .string)
    state.solution []

def calleeEntered (state : Machine) {body : List StatementNode} (world : ResultCallWorld state body) (stack : List Frame) : Machine :=
  { state with
    control := .statements body, cells := calleeCells state, store := calleeStore state,
    activation := state.nextActivation, nextActivation := state.nextActivation + 1,
    solution := compose state.solution
      (compose (schemeSubstitution wrapScheme (witnessSubstitution [])) world.wrapFunction.elaboration),
    stack := .caller state.cells state.activation state.solution (.result .integer .string) :: stack }

def calleeOk (state : Machine) {body : List StatementNode} (world : ResultCallWorld state body) (stack : List Frame) :
    OkBuiltin { calleeEntered state world stack with stack := .returnValue :: (calleeEntered state world stack).stack } :=
  ⟨world.ok.okDeclaration,
    callee_reads_a_caller_global state resultOkBinder _ (by decide) world.okVisible world.fresh world.ok.okFound,
    world.ok.okFunction⟩

theorem wrap_call_frame_returns_ok (state : Machine) (world : ResultCallWorld state wrapCandidate) (stack : List Frame) :
    Execution (calleeEntered state world stack)
      [.internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal]
      { callResumed state with control := .value (.ok callOne .string), stack := stack } := by
  let entered := calleeEntered state world stack
  let returning := { entered with stack := .returnValue :: entered.stack }
  have construction := ok_builtin_constructs returning (calleeOk state world stack) callOne
    (callee_reads_its_parameter state) rfl call_one_is_an_integer
  refine .next (Step.returnStart entered (okNode (.read resultFirstBinder [])) []) ?_
  refine execution_append (first := [.internal, .internal, .internal, .internal, .internal, .internal])
    (second := [.internal, .internal]) construction ?_
  refine .next (Step.returnValue entered (.ok callOne .string)) ?_
  exact .next (Step.returnedToCaller { entered with stack := stack } (.ok callOne .string) state.cells
    state.activation state.solution (.result .integer .string) (by decide +kernel)) (.done _)

theorem lift_enters_the_wrap_call_frame (state : Machine) {body : List StatementNode} (world : ResultCallWorld state body) :
    Execution { state with control := .expression liftNode }
      (List.replicate 10 .internal)
      (calleeEntered state world (.propagate :: .callArguments (okCallTarget world.ok) [] [] :: state.stack)) := by
  let propagated := Node.propagate (.call (.read wrapBinder []) [.integer "1"]) (.primitive .string)
  let withTarget := { state with stack := .callTarget [propagated] :: state.stack }
  let withArgument := { state with stack := .callArguments (okCallTarget world.ok) [] [] :: state.stack }
  let withPropagate := { withArgument with stack := .propagate :: withArgument.stack }
  let withCallee := { withPropagate with stack := .callTarget [.integer "1"] :: withPropagate.stack }
  let withWrapArgument := { withPropagate with stack := .callArguments (wrapTarget state world) [] [] :: withPropagate.stack }
  have signature : instantiateFunction state.solution world.wrapFunction.scheme [] =
      some (.argument .integer .argumentsEmpty, .result .integer .string, []) := by
    rw [world.wrapSchemeIs]
    exact wrap_scheme_instantiates _
  have bound : bindParameters state.nextActivation world.wrapFunction.parameters [callOne]
      (.argument .integer .argumentsEmpty) state.store =
      .ok (bindActive emptyCells resultFirstBinder (calleeCell state), calleeStore state) := by
    rw [world.wrapParameters]
    exact wrap_parameters_bind state world
  have entry := Step.callFunction withPropagate world.wrapDeclaration [callOne] (.argument .integer .argumentsEmpty)
    (.result .integer .string) state.solution [] [] world.wrapFunction _ _ world.wrapTable signature bound
  rw [world.wrapBody, world.wrapSchemeIs] at entry
  refine .next (Step.callStart state (.read resultOkBinder resultInstance) [propagated]) ?_
  refine .next (Step.readFunction withTarget resultOkBinder resultInstance world.ok.okDeclaration okScheme _ _
    resultInstance world.ok.okFound (ok_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument state (okCallTarget world.ok) propagated []) ?_
  refine .next (Step.propagateStart withArgument (.call (.read wrapBinder []) [.integer "1"]) (.primitive .string)) ?_
  refine .next (Step.callStart withPropagate (.read wrapBinder []) [.integer "1"]) ?_
  refine .next (Step.readFunction withCallee wrapBinder [] world.wrapDeclaration wrapScheme _ _ []
    world.wrapFound (wrap_scheme_instantiates _)) ?_
  refine .next (Step.callFirstArgument withPropagate (wrapTarget state world) (.integer "1") []) ?_
  refine .next (Step.integer withWrapArgument "1" intOne (by decide +kernel)) ?_
  refine .next (Step.callArgumentsComplete withPropagate (wrapTarget state world) callOne []) ?_
  exact .next entry (.done _)

theorem lift_runs_the_wrap_call_frame_and_propagates_its_success (state : Machine)
    (world : ResultCallWorld state wrapCandidate) :
    Execution { state with control := .expression liftNode }
      (List.replicate 22 .internal)
      { callResumed state with control := .value (.ok callOne .string) } := by
  refine execution_append (first := List.replicate 10 .internal) (second := List.replicate 12 .internal)
    (lift_enters_the_wrap_call_frame state world) ?_
  refine execution_append (first := List.replicate 9 .internal) (second := List.replicate 3 .internal)
    (wrap_call_frame_returns_ok state world _) ?_
  let resumed := callResumed state
  refine .next (Step.propagateOk { resumed with stack := .callArguments (okCallTarget world.ok) [] [] :: state.stack }
    callOne .string) ?_
  refine .next (Step.callArgumentsComplete resumed (okCallTarget world.ok) callOne []) ?_
  exact .next (Step.callOkInstantiated resumed world.ok.okDeclaration callOne .integer .string state.solution
    resultInstance world.ok.okFunction call_one_is_an_integer) (.done _)

def refusingBody : List StatementNode := [.returned (errorNode (.string "no"))]

def refusal : Value := .error (.scalar (.string "no")) .integer

theorem a_refusing_call_frame_returns_the_error (state : Machine) (world : ResultCallWorld state refusingBody)
    (error : ErrorBuiltin state) (stack : List Frame) :
    Execution (calleeEntered state world stack) (List.replicate 9 .internal)
      { callResumed state with control := .value refusal, stack := stack } := by
  let entered := calleeEntered state world stack
  let returning := { entered with stack := .returnValue :: entered.stack }
  have builtins : ResultBuiltins returning :=
    ⟨world.ok.okDeclaration, error.errorDeclaration, (calleeOk state world stack).okFound, world.ok.okFunction,
      callee_reads_a_caller_global state resultErrorBinder _ (by decide) error.errorVisible world.fresh
        error.errorFound,
      error.errorFunction⟩
  refine .next (Step.returnStart entered (errorNode (.string "no")) []) ?_
  refine execution_append (first := List.replicate 6 .internal) (second := List.replicate 2 .internal)
    (error_construction_executes returning builtins) ?_
  refine .next (Step.returnValue entered refusal) ?_
  exact .next (Step.returnedToCaller { entered with stack := stack } refusal state.cells
    state.activation state.solution (.result .integer .string) (by decide +kernel)) (.done _)

theorem lift_propagates_a_failing_call_to_its_caller (state : Machine) (world : ResultCallWorld state refusingBody)
    (error : ErrorBuiltin state)
    (cells : ActiveCells) (activation : Nat) (saved : Substitution) (rest : List Frame)
    (framed : state.stack = .caller cells activation saved (.result .integer .string) :: rest) :
    Execution { state with control := .expression liftNode }
      (List.replicate 22 .internal)
      { callResumed state with control := .value refusal, cells := cells, activation := activation,
                               solution := saved, stack := rest } := by
  refine execution_append (first := List.replicate 10 .internal) (second := List.replicate 12 .internal)
    (lift_enters_the_wrap_call_frame state world) ?_
  refine execution_append (first := List.replicate 9 .internal) (second := List.replicate 3 .internal)
    (a_refusing_call_frame_returns_the_error state world error _) ?_
  let resumed := { callResumed state with stack := rest }
  let caller := Frame.caller cells activation saved (.result .integer .string)
  let outer := { resumed with stack := caller :: rest }
  let pending := Frame.callArguments (okCallTarget world.ok) [] []
  rw [framed]
  refine .next (Step.propagateError { outer with stack := pending :: outer.stack } (.scalar (.string "no")) .integer) ?_
  refine .next (Step.propagateUnwind outer refusal pending rfl) ?_
  exact .next (Step.propagatedToCaller resumed (.scalar (.string "no")) .integer .integer .string cells activation saved
    (by decide +kernel)) (.done _)

def callOkDeclaration : DeclarationId := ⟨resultOkBinder⟩
def callErrorDeclaration : DeclarationId := ⟨resultErrorBinder⟩
def callWrapDeclaration : DeclarationId := ⟨wrapBinder⟩

def callFunctions (body : List StatementNode) : FunctionTable := fun declaration =>
  if declaration = callOkDeclaration then some .resultOk
  else if declaration = callErrorDeclaration then some .resultError
  else if declaration = callWrapDeclaration then some (.function ⟨[resultFirstBinder], body, wrapScheme, identity⟩)
  else none

def callGlobals : ActiveCells := fun binder =>
  if binder = resultOkBinder ∨ binder = resultErrorBinder ∨ binder = wrapBinder then some ⟨0, binder⟩ else none

def callStore : Store := fun cell =>
  if cell = ⟨0, resultOkBinder⟩ then
    some ⟨⟨.schemeFunction, .schemeFunction callOkDeclaration okScheme, by decide⟩, .immutable⟩
  else if cell = ⟨0, resultErrorBinder⟩ then
    some ⟨⟨.schemeFunction, .schemeFunction callErrorDeclaration errorScheme, by decide⟩, .immutable⟩
  else if cell = ⟨0, wrapBinder⟩ then
    some ⟨⟨.schemeFunction, .schemeFunction callWrapDeclaration wrapScheme, by decide⟩, .immutable⟩
  else none

def callWorldMachine (base : Machine) (body : List StatementNode) : Machine :=
  { base with
    functions := callFunctions body, globals := callGlobals, cells := callGlobals, store := callStore,
    activation := 0, nextActivation := 1 }

theorem call_binders_are_distinct :
    resultErrorBinder ≠ resultOkBinder ∧ wrapBinder ≠ resultOkBinder ∧ wrapBinder ≠ resultErrorBinder ∧
      callErrorDeclaration ≠ callOkDeclaration ∧ callWrapDeclaration ≠ callOkDeclaration ∧
      callWrapDeclaration ≠ callErrorDeclaration := by
  decide

def callWorld (base : Machine) (body : List StatementNode) : ResultCallWorld (callWorldMachine base body) body where
  ok := ⟨callOkDeclaration, by simp [Connected.read, callWorldMachine, callGlobals, callStore],
    by simp [callWorldMachine, callFunctions]⟩
  wrapDeclaration := callWrapDeclaration
  wrapFunction := ⟨[resultFirstBinder], body, wrapScheme, identity⟩
  wrapFound := by simp [Connected.read, callWorldMachine, callGlobals, callStore, call_binders_are_distinct]
  wrapTable := by simp [callWorldMachine, callFunctions, call_binders_are_distinct]
  wrapSchemeIs := rfl
  wrapParameters := rfl
  wrapBody := rfl
  okVisible := rfl
  fresh := by simp [callWorldMachine, callStore]

def callWorldError (base : Machine) (body : List StatementNode) : ErrorBuiltin (callWorldMachine base body) where
  errorDeclaration := callErrorDeclaration
  errorFound := by simp [Connected.read, callWorldMachine, callGlobals, callStore, call_binders_are_distinct]
  errorFunction := by simp [callWorldMachine, callFunctions, call_binders_are_distinct]
  errorVisible := rfl

theorem the_call_world_runs_lift_through_wrap (base : Machine) :
    Execution { callWorldMachine base wrapCandidate with control := .expression liftNode }
      (List.replicate 22 .internal)
      { callResumed (callWorldMachine base wrapCandidate) with control := .value (.ok callOne .string) } :=
  lift_runs_the_wrap_call_frame_and_propagates_its_success _ (callWorld base wrapCandidate)

theorem the_call_world_propagates_a_refusing_callee (base : Machine) (rest : List Frame)
    (framed : base.stack = .caller emptyCells 0 identity (.result .integer .string) :: rest) :
    Execution { callWorldMachine base refusingBody with control := .expression liftNode }
      (List.replicate 22 .internal)
      { callResumed (callWorldMachine base refusingBody) with
        control := .value refusal, cells := emptyCells, activation := 0, solution := identity, stack := rest } :=
  lift_propagates_a_failing_call_to_its_caller _ (callWorld base refusingBody) (callWorldError base refusingBody)
    emptyCells 0 identity rest framed

theorem result_call_return_is_the_executed_node :
    (parseCodes resultCallRawInstructionCodes).bind decodeRawInstructionRows = some resultCallRows ∧
    (parseCodes resultCallRawFunctionCodes).bind decodeRawFunctionRows = some resultCallFunctions ∧
    decodeResultSignatures resultCallSignatureCodes = some resultCallSignatures ∧
    resultCallCertified = true ∧
    returnedNode liftCandidate = some liftNode ∧ wrapCandidate = [.returned (okNode (.read resultFirstBinder []))] :=
  ⟨result_call_rows_decode_to_the_fixture, result_call_functions_decode_to_the_fixture,
    result_call_signatures_decode_to_the_fixture, result_call_is_certified, rfl, rfl⟩

end Xsh.Semantics.Connected
