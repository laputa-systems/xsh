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

/-- The checked expression fragment; arithmetic operands are integers only. -/
inductive BaseExpr where
  | integerLiteral (value : Int)
  | scalarLiteral (value : CompositeValue)
  | identifier (name : Xsh.Syntax.RawName)
  | negate (operand : BaseExpr)
  | binary (operator : IntegerBinaryOp) (left right : BaseExpr)

/-- Failures are terminal outcomes of this fragment; exhaustion belongs to the runner. -/
inductive BaseFault where
  | unboundName (name : Xsh.Syntax.RawName)
  | inactiveBinder (binder : BinderId)
  | missingCell (cell : CellId)
  | unsupportedScalarLiteral
  | expectedInteger
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
def inferBaseType? (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) : BaseExpr → Option BaseType
  | .integerLiteral value =>
    (validateInt? value).map (fun _ => BaseType.integer)
  | .scalarLiteral value => (scalarTypedValue? value).map TypedValue.typeTag
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
  | .identifier name => (lookupBaseValue names activations store name).isOk
  | .negate operand => checkIntegerExpr names activations store operand
  | .binary _ left right =>
    checkIntegerExpr names activations store left && checkIntegerExpr names activations store right

inductive BaseControl where
  | evaluate (expression : BaseExpr)
  | returned (value : BaseTypedValue)
  | failed (fault : BaseFault)

inductive BaseFrame where
  | negate
  | binaryLeft (operator : IntegerBinaryOp) (right : BaseExpr)
  | binaryRight (operator : IntegerBinaryOp) (left : IntValue)

structure MachineConfig where
  names : LexicalEnvironment
  activations : ActivationEnvironment
  store : BaseStore
  control : BaseControl
  continuation : List BaseFrame

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
  | .evaluate _ => none

def controlOfIntegerResult (result : IntegerResult) : BaseControl :=
  match result with
  | .value value => .returned (typedInteger value)
  | .fault fault => .failed (.arithmetic fault)

def integerPayload? (value : BaseTypedValue) : Option IntValue :=
  match value.typeTag, value.value with
  | .integer, .integer integer => some integer
  | _, _ => none

def updateState (config : MachineConfig) (control : BaseControl)
    (continuation : List BaseFrame) : MachineConfig :=
  ⟨config.names, config.activations, config.store, control, continuation⟩

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
  | .failed _ => none

def initialConfig (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (expression : BaseExpr) : MachineConfig :=
  ⟨names, activations, store, .evaluate expression, []⟩

def continuationChecked (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) : List BaseFrame → Bool
  | [] => true
  | .negate :: rest => continuationChecked names activations store rest
  | .binaryLeft _ right :: rest =>
    checkIntegerExpr names activations store right && continuationChecked names activations store rest
  | .binaryRight _ _ :: rest => continuationChecked names activations store rest

/-- Pending arithmetic frames require integer controls until the frame is discharged. -/
def controlFitsContinuation (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (control : BaseControl) : List BaseFrame → Bool
  | [] => true
  | _ :: _ =>
    match control with
    | .evaluate expression => checkIntegerExpr names activations store expression
    | .returned value => (integerPayload? value).isSome
    | .failed _ => true

def wellFormed (config : MachineConfig) : Prop :=
  (match config.control with
   | .evaluate expression =>
     checkBaseExpr config.names config.activations config.store expression = true
   | .returned _ | .failed _ => True) ∧
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
  | outOfFuel
  | internalStuck

def runFuel : Nat → MachineConfig → RunResult
  | fuel, config =>
    match terminalOutcome config with
    | some outcome => .completed outcome
    | none =>
      match fuel with
      | 0 => .outOfFuel
      | remaining + 1 =>
        match step config with
        | some next => runFuel remaining next
        | none => .internalStuck

end Xsh.Semantics.BaseMachine
