import Xsh.Syntax.Core
import Xsh.Values.Composite
import Xsh.Values.IntegerOps
import Xsh.Semantics.Environments

namespace Xsh.Semantics.BaseMachine

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Semantics.Environments

/-- Type tags supported by the scalar-and-integer-expression machine fragment. -/
inductive BaseType where
  | null
  | boolean
  | integer
  | float
  | duration
  | string
  | bytes
  deriving DecidableEq

/-- `time.now()` requests signed epoch milliseconds from the host. Clock accuracy and the
correspondence between the answer and the host clock remain external assumptions. -/
inductive BaseHostRequest : BaseType → Type where
  | timeNow : BaseHostRequest .integer

def BaseHostRequest.responseType {responseType : BaseType}
    (_request : BaseHostRequest responseType) : BaseType := responseType

def hasBaseType : BaseType → CompositeValue → Prop
  | .null, .null => True
  | .boolean, .boolean _ => True
  | .integer, .integer _ => True
  | .float, .float _ => True
  | .duration, .duration _ => True
  | .string, .string _ => True
  | .bytes, .bytes _ => True
  | _, _ => False

abbrev BaseStore := TypedStore BaseType CompositeValue hasBaseType
abbrev BaseTypedValue := TypedValue BaseType CompositeValue hasBaseType

def typedInteger (value : IntValue) : BaseTypedValue := {
  typeTag := .integer
  value := .integer value
  wellTyped := trivial
}

/- A literal is accepted only when it is one of the existing exact scalar values. -/
def scalarTypedValue? : CompositeValue → Option BaseTypedValue
  | .null => some ⟨.null, .null, trivial⟩
  | .boolean value => some ⟨.boolean, .boolean value, trivial⟩
  | .integer value => some (typedInteger value)
  | .float bits => some ⟨.float, .float bits, trivial⟩
  | .duration value => some ⟨.duration, .duration value, trivial⟩
  | .string value => some ⟨.string, .string value, trivial⟩
  | .bytes value => some ⟨.bytes, .bytes value, trivial⟩
  | .list _ | .record _ | .tagged _ _ _ => none

/-- Integer comparisons produce booleans; they are the only boolean-producing operation. -/
inductive ComparisonOperator where
  | less
  | lessEqual
  | greater
  | greaterEqual
  | equal
  | notEqual
  deriving DecidableEq, Repr

def ComparisonOperator.holds : ComparisonOperator → Int → Int → Bool
  | .less, left, right => decide (left < right)
  | .lessEqual, left, right => decide (left ≤ right)
  | .greater, left, right => decide (left > right)
  | .greaterEqual, left, right => decide (left ≥ right)
  | .equal, left, right => decide (left = right)
  | .notEqual, left, right => decide (left ≠ right)

/-- The checked expression fragment; arithmetic operands are integers only.

`conditional` selects one integer branch by a boolean condition and evaluates only that branch.
`call` is a call of a pure integer function that has no recursion and no captures: the callee body
is part of the node, its `parameters` are bound to the integer values of `arguments` evaluated
left to right in the caller, and the body is checked to mention no name outside its parameters. -/
inductive BaseExpr where
  | integerLiteral (value : Int)
  | scalarLiteral (value : CompositeValue)
  | timeNow
  | identifier (name : Xsh.Syntax.RawName)
  | negate (operand : BaseExpr)
  | binary (operator : IntegerBinaryOp) (left right : BaseExpr)
  | compare (operator : ComparisonOperator) (left right : BaseExpr)
  | conditional (condition thenBranch elseBranch : BaseExpr)
  | call (parameters : List Xsh.Syntax.RawName) (body : BaseExpr) (arguments : List BaseExpr)

/-- Failures are terminal outcomes of this fragment; exhaustion belongs to the runner. -/
inductive BaseFault where
  | unboundName (name : Xsh.Syntax.RawName)
  | inactiveBinder (binder : BinderId)
  | missingCell (cell : CellId)
  | unsupportedScalarLiteral
  | expectedInteger
  | hostResponseTypeMismatch
  | arithmetic (fault : IntegerFault)
  deriving DecidableEq

/- Resolve a source name through its declaration and active cell before reading its typed value. -/
def lookupBaseValue (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (name : Xsh.Syntax.RawName) : Except BaseFault BaseTypedValue :=
  match resolve names name with
  | none => .error (.unboundName name)
  | some binder =>
    match resolveCell activations binder with
    | none => .error (.inactiveBinder binder)
    | some cell =>
      match store cell with
      | none => .error (.missingCell cell)
      | some stored => .ok stored.contents

/- Integer operators require integer operands; scalar literals and identifiers retain their tags. -/
mutual
def inferBaseType? (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) : BaseExpr → Option BaseType
  | .integerLiteral value =>
    (validateInt? value).map (fun _ => BaseType.integer)
  | .scalarLiteral value => (scalarTypedValue? value).map TypedValue.typeTag
  | .timeNow => some .integer
  | .identifier name =>
    match lookupBaseValue names activations store name with
    | .ok value => some value.typeTag
    | .error _ => none
  | .negate operand =>
    match inferBaseType? names activations store operand with
    | some .integer => some .integer
    | _ => none
  | .binary _ left right =>
    match inferBaseType? names activations store left,
        inferBaseType? names activations store right with
    | some .integer, some .integer => some .integer
    | _, _ => none
  | .compare _ left right =>
    match inferBaseType? names activations store left,
        inferBaseType? names activations store right with
    | some .integer, some .integer => some .boolean
    | _, _ => none
  | .conditional condition thenBranch elseBranch =>
    match inferBaseType? names activations store condition,
        inferBaseType? names activations store thenBranch,
        inferBaseType? names activations store elseBranch with
    | some .boolean, some .integer, some .integer => some .integer
    | _, _, _ => none
  | .call parameters body arguments =>
    if parameters.length = arguments.length &&
        inferArgumentsInteger names activations store arguments &&
        staticType parameters body == some .integer then
      some .integer
    else
      none

def inferArgumentsInteger (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) : List BaseExpr → Bool
  | [] => true
  | argument :: rest =>
    inferBaseType? names activations store argument == some .integer &&
      inferArgumentsInteger names activations store rest

/- The type of a callee body from its parameters alone: every parameter is an integer and no other
name is in scope. -/
def staticType (parameters : List Xsh.Syntax.RawName) : BaseExpr → Option BaseType
  | .integerLiteral value => (validateInt? value).map (fun _ => BaseType.integer)
  | .scalarLiteral value => (scalarTypedValue? value).map TypedValue.typeTag
  | .timeNow => some .integer
  | .identifier name => if name ∈ parameters then some .integer else none
  | .negate operand =>
    match staticType parameters operand with
    | some .integer => some .integer
    | _ => none
  | .binary _ left right =>
    match staticType parameters left, staticType parameters right with
    | some .integer, some .integer => some .integer
    | _, _ => none
  | .compare _ left right =>
    match staticType parameters left, staticType parameters right with
    | some .integer, some .integer => some .boolean
    | _, _ => none
  | .conditional condition thenBranch elseBranch =>
    match staticType parameters condition, staticType parameters thenBranch,
        staticType parameters elseBranch with
    | some .boolean, some .integer, some .integer => some .integer
    | _, _, _ => none
  | .call inner body arguments =>
    if inner.length = arguments.length && staticArgumentsInteger parameters arguments &&
        staticType inner body == some .integer then
      some .integer
    else
      none

def staticArgumentsInteger (parameters : List Xsh.Syntax.RawName) : List BaseExpr → Bool
  | [] => true
  | argument :: rest =>
    staticType parameters argument == some .integer && staticArgumentsInteger parameters rest
end

def checkIntegerExpr (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (expression : BaseExpr) : Bool :=
  match inferBaseType? names activations store expression with
  | some .integer => true
  | _ => false

/- The checker rejects unsupported composite payloads and ill-typed integer operations. -/
def checkBaseExpr (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) : BaseExpr → Bool
  | .integerLiteral value => (validateInt? value).isSome
  | .scalarLiteral value => (scalarTypedValue? value).isSome
  | .timeNow => true
  | .identifier name => (lookupBaseValue names activations store name).isOk
  | .negate operand => checkIntegerExpr names activations store operand
  | .binary _ left right =>
    checkIntegerExpr names activations store left && checkIntegerExpr names activations store right
  | expression@(.compare _ _ _) | expression@(.conditional _ _ _) | expression@(.call _ _ _) =>
    (inferBaseType? names activations store expression).isSome

inductive BaseControl where
  | evaluate (expression : BaseExpr)
  | returned (value : BaseTypedValue)
  | awaitingHost {responseType : BaseType} (request : BaseHostRequest responseType)
  | failed (fault : BaseFault)

inductive BaseFrame where
  | negate
  | binaryLeft (operator : IntegerBinaryOp) (right : BaseExpr)
  | binaryRight (operator : IntegerBinaryOp) (left : IntValue)
  | compareLeft (operator : ComparisonOperator) (right : BaseExpr)
  | compareRight (operator : ComparisonOperator) (left : IntValue)
  | branch (thenBranch elseBranch : BaseExpr)
  | callArgument (parameters : List Xsh.Syntax.RawName) (body : BaseExpr)
      (evaluated : List IntValue) (remaining : List BaseExpr)

structure MachineConfig where
  names : LexicalEnvironment
  activations : ActivationEnvironment
  store : BaseStore
  control : BaseControl
  continuation : List BaseFrame

structure BaseHostWait where
  responseType : BaseType
  request : BaseHostRequest responseType
  config : MachineConfig
  awaiting : config.control = .awaitingHost request

inductive BaseOutcome where
  | value (value : BaseTypedValue)
  | fault (fault : BaseFault)

def terminalOutcome (config : MachineConfig) : Option BaseOutcome :=
  match config.control with
  | .failed fault => some (.fault fault)
  | .returned value =>
    match config.continuation with
    | [] => some (.value value)
    | _ :: _ => none
  | .awaitingHost _ => none
  | .evaluate _ => none

def controlOfIntegerResult (result : IntegerResult) : BaseControl :=
  match result with
  | .value value => .returned (typedInteger value)
  | .fault fault => .failed (.arithmetic fault)

def integerPayload? (value : BaseTypedValue) : Option IntValue :=
  match value.typeTag, value.value with
  | .integer, .integer integer => some integer
  | _, _ => none

def booleanPayload? (value : BaseTypedValue) : Option Bool :=
  match value.typeTag, value.value with
  | .boolean, .boolean truth => some truth
  | _, _ => none

def typedBoolean (truth : Bool) : BaseTypedValue := {
  typeTag := .boolean
  value := .boolean truth
  wellTyped := trivial
}

/- Replace each bound parameter by the integer literal of its value. A callee body mentions only
its own parameters, so a call node's body is never rewritten; only its arguments are. -/
mutual
def substitute (bindings : List (Xsh.Syntax.RawName × IntValue)) : BaseExpr → BaseExpr
  | .integerLiteral value => .integerLiteral value
  | .scalarLiteral value => .scalarLiteral value
  | .timeNow => .timeNow
  | .identifier name =>
    match bindings.lookup name with
    | some value => .integerLiteral value.val
    | none => .identifier name
  | .negate operand => .negate (substitute bindings operand)
  | .binary operator left right =>
    .binary operator (substitute bindings left) (substitute bindings right)
  | .compare operator left right =>
    .compare operator (substitute bindings left) (substitute bindings right)
  | .conditional condition thenBranch elseBranch =>
    .conditional (substitute bindings condition) (substitute bindings thenBranch)
      (substitute bindings elseBranch)
  | .call parameters body arguments =>
    .call parameters body (substituteArguments bindings arguments)

def substituteArguments (bindings : List (Xsh.Syntax.RawName × IntValue)) :
    List BaseExpr → List BaseExpr
  | [] => []
  | argument :: rest => substitute bindings argument :: substituteArguments bindings rest
end

structure BaseHostResponse where
  value : BaseTypedValue

def checkedHostResponse? {responseType : BaseType}
    (request : BaseHostRequest responseType) (response : BaseHostResponse) :
    Option BaseTypedValue :=
  if response.value.typeTag = request.responseType then some response.value else none

def updateState (config : MachineConfig) (control : BaseControl)
    (continuation : List BaseFrame) : MachineConfig :=
  ⟨config.names, config.activations, config.store, control, continuation⟩

def resumeHostRequest (config : MachineConfig) (response : BaseHostResponse) :
    Option MachineConfig :=
  match config.control with
  | .awaitingHost request =>
    some (updateState config
      (match checkedHostResponse? request response with
       | some value => .returned value
       | none => .failed .hostResponseTypeMismatch)
      config.continuation)
  | _ => none

def pendingHostWait? (config : MachineConfig) : Option BaseHostWait :=
  match control : config.control with
  | .awaitingHost request => some ⟨_, request, config, control⟩
  | _ => none

def callBody (parameters : List Xsh.Syntax.RawName) (body : BaseExpr) (values : List IntValue) :
    BaseExpr :=
  substitute (parameters.zip values) body

def step (config : MachineConfig) : Option MachineConfig :=
  match config.control with
  | .evaluate expression =>
    match expression with
    | .integerLiteral value =>
      some (updateState config (controlOfIntegerResult (checkedInt value)) config.continuation)
    | .scalarLiteral value =>
      some (updateState config
        (match scalarTypedValue? value with
         | some typed => .returned typed
         | none => .failed .unsupportedScalarLiteral)
        config.continuation)
    | .timeNow => some (updateState config (.awaitingHost .timeNow) config.continuation)
    | .identifier name =>
      some (updateState config
        (match lookupBaseValue config.names config.activations config.store name with
         | .ok value => .returned value
         | .error fault => .failed fault)
        config.continuation)
    | .negate operand =>
      some (updateState config (.evaluate operand) (.negate :: config.continuation))
    | .binary operator left right =>
      some (updateState config (.evaluate left)
        (.binaryLeft operator right :: config.continuation))
    | .compare operator left right =>
      some (updateState config (.evaluate left)
        (.compareLeft operator right :: config.continuation))
    | .conditional condition thenBranch elseBranch =>
      some (updateState config (.evaluate condition)
        (.branch thenBranch elseBranch :: config.continuation))
    | .call parameters body arguments =>
      match arguments with
      | [] => some (updateState config (.evaluate (callBody parameters body [])) config.continuation)
      | argument :: remaining =>
        some (updateState config (.evaluate argument)
          (.callArgument parameters body [] remaining :: config.continuation))
  | .returned value =>
    match config.continuation with
    | [] => none
    | frame :: rest =>
      match frame with
      | .negate =>
        some (updateState config
          (match integerPayload? value with
           | some integer => controlOfIntegerResult (IntegerOps.negate integer)
           | none => .failed .expectedInteger)
          rest)
      | .binaryLeft operator right =>
        some (match integerPayload? value with
          | some integer =>
            updateState config (.evaluate right) (.binaryRight operator integer :: rest)
          | none => updateState config (.failed .expectedInteger) rest)
      | .binaryRight operator left =>
        some (updateState config
          (match integerPayload? value with
           | some integer => controlOfIntegerResult (applyBinary operator left integer)
           | none => .failed .expectedInteger)
          rest)
      | .compareLeft operator right =>
        some (match integerPayload? value with
          | some integer =>
            updateState config (.evaluate right) (.compareRight operator integer :: rest)
          | none => updateState config (.failed .expectedInteger) rest)
      | .compareRight operator left =>
        some (updateState config
          (match integerPayload? value with
           | some integer => .returned (typedBoolean (operator.holds left.val integer.val))
           | none => .failed .expectedInteger)
          rest)
      | .branch thenBranch elseBranch =>
        some (match booleanPayload? value with
          | some truth =>
            updateState config (.evaluate (if truth then thenBranch else elseBranch)) rest
          | none => updateState config (.failed .expectedInteger) rest)
      | .callArgument parameters body evaluated remaining =>
        some (match integerPayload? value with
          | some integer =>
            match remaining with
            | [] =>
              updateState config (.evaluate (callBody parameters body (evaluated ++ [integer]))) rest
            | argument :: more =>
              updateState config (.evaluate argument)
                (.callArgument parameters body (evaluated ++ [integer]) more :: rest)
          | none => updateState config (.failed .expectedInteger) rest)
  | .awaitingHost _ => none
  | .failed _ => none

def initialConfig (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (expression : BaseExpr) : MachineConfig :=
  ⟨names, activations, store, .evaluate expression, []⟩

/-- The type of the value a pending frame consumes: a branch consumes its boolean condition and
every other frame consumes an integer. -/
def BaseFrame.consumes : BaseFrame → BaseType
  | .branch _ _ => .boolean
  | .negate | .binaryLeft _ _ | .binaryRight _ _ | .compareLeft _ _ | .compareRight _ _
  | .callArgument _ _ _ _ => .integer

/-- The type of the value a pending frame hands to the frame below it. -/
def BaseFrame.produces : BaseFrame → BaseType
  | .compareLeft _ _ | .compareRight _ _ => .boolean
  | .negate | .binaryLeft _ _ | .binaryRight _ _ | .branch _ _ | .callArgument _ _ _ _ => .integer

/-- A frame hands the type the frame below it consumes; the bottom frame hands anything. -/
def producesWhatNextConsumes (frame : BaseFrame) : List BaseFrame → Bool
  | [] => true
  | next :: _ => frame.produces == next.consumes

def continuationChecked (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) : List BaseFrame → Bool
  | [] => true
  | .negate :: rest =>
    producesWhatNextConsumes .negate rest && continuationChecked names activations store rest
  | .binaryLeft operator right :: rest =>
    producesWhatNextConsumes (.binaryLeft operator right) rest &&
      checkIntegerExpr names activations store right &&
      continuationChecked names activations store rest
  | .binaryRight operator left :: rest =>
    producesWhatNextConsumes (.binaryRight operator left) rest &&
      continuationChecked names activations store rest
  | .compareLeft operator right :: rest =>
    producesWhatNextConsumes (.compareLeft operator right) rest &&
      checkIntegerExpr names activations store right &&
      continuationChecked names activations store rest
  | .compareRight operator left :: rest =>
    producesWhatNextConsumes (.compareRight operator left) rest &&
      continuationChecked names activations store rest
  | .branch thenBranch elseBranch :: rest =>
    producesWhatNextConsumes (.branch thenBranch elseBranch) rest &&
      checkIntegerExpr names activations store thenBranch &&
      checkIntegerExpr names activations store elseBranch &&
      continuationChecked names activations store rest
  | .callArgument parameters body evaluated remaining :: rest =>
    producesWhatNextConsumes (.callArgument parameters body evaluated remaining) rest &&
      decide (parameters.length = evaluated.length + 1 + remaining.length) &&
      inferArgumentsInteger names activations store remaining &&
      staticType parameters body == some .integer &&
      continuationChecked names activations store rest

/-- A pending frame requires its consumed type of the control until the frame is discharged. -/
def controlFitsContinuation (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (control : BaseControl) : List BaseFrame → Bool
  | [] => true
  | frame :: _ =>
    match control with
    | .evaluate expression => inferBaseType? names activations store expression == some frame.consumes
    | .returned value => value.typeTag == frame.consumes
    | .awaitingHost request => request.responseType == frame.consumes
    | .failed _ => true

def wellFormed (config : MachineConfig) : Prop :=
  (match config.control with
   | .evaluate expression =>
     checkBaseExpr config.names config.activations config.store expression = true
   | .returned _ | .awaitingHost _ | .failed _ => True) ∧
  continuationChecked config.names config.activations config.store config.continuation = true ∧
  controlFitsContinuation config.names config.activations config.store config.control
    config.continuation = true

inductive MachineSteps : Nat → MachineConfig → MachineConfig → Prop where
  | refl (config : MachineConfig) : MachineSteps 0 config config
  | step {fuel : Nat} {start next finish : MachineConfig}
      (first : step start = some next) (rest : MachineSteps fuel next finish) :
      MachineSteps (fuel + 1) start finish

inductive RunResult where
  | completed (outcome : BaseOutcome)
  | hostRequest (waiting : BaseHostWait)
  | outOfFuel
  | internalStuck

def runFuel : Nat → MachineConfig → RunResult
  | fuel, config =>
    match terminalOutcome config with
    | some outcome => .completed outcome
    | none =>
      match pendingHostWait? config with
      | some waiting => .hostRequest waiting
      | none =>
        match fuel with
        | 0 => .outOfFuel
        | remaining + 1 =>
          match step config with
          | some next => runFuel remaining next
          | none => .internalStuck

end Xsh.Semantics.BaseMachine
