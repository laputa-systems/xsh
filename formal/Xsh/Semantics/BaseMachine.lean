import Xsh.Syntax.Core
import Xsh.Values.Composite
import Xsh.Values.IntegerOps
import Xsh.Semantics.Environments

namespace Xsh.Semantics.BaseMachine

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Semantics.Environments

inductive BaseExpr where
  | integerLiteral (value : Int)
  | identifier (name : Xsh.Syntax.RawName)
  | negate (operand : BaseExpr)
  | binary (operator : IntegerBinaryOp) (left right : BaseExpr)
  deriving DecidableEq

inductive BaseType where
  | integer
  deriving DecidableEq

def hasBaseType : BaseType → CompositeValue → Prop
  | .integer, .integer _ => True
  | .integer, _ => False

abbrev BaseStore := TypedStore BaseType CompositeValue hasBaseType

inductive BaseFault where
  | unboundName (name : Xsh.Syntax.RawName)
  | inactiveBinder (binder : BinderId)
  | missingCell (cell : CellId)
  | invalidStoredValue (cell : CellId)
  | arithmetic (fault : IntegerFault)
  deriving DecidableEq

def lookupInteger (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (name : Xsh.Syntax.RawName) : Except BaseFault IntValue :=
  match resolve names name with
  | none => .error (.unboundName name)
  | some binder =>
    match resolveCell activations binder with
    | none => .error (.inactiveBinder binder)
    | some cell =>
      match store cell with
      | none => .error (.missingCell cell)
      | some stored =>
        match stored.contents.value with
        | .integer value => .ok value
        | _ => .error (.invalidStoredValue cell)

/-- This checker accepts only the closed integer syntax above and checks every
literal and identifier against the supplied lexical and runtime bindings. -/
def checkBaseExpr (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) : BaseExpr → Bool
  | .integerLiteral value => (validateInt? value).isSome
  | .identifier name => (lookupInteger names activations store name).isOk
  | .negate operand => checkBaseExpr names activations store operand
  | .binary _ left right =>
    checkBaseExpr names activations store left && checkBaseExpr names activations store right

inductive BaseControl where
  | evaluate (expression : BaseExpr)
  | returned (value : IntValue)
  | failed (fault : BaseFault)
  deriving DecidableEq

inductive BaseFrame where
  | negate
  | binaryLeft (operator : IntegerBinaryOp) (right : BaseExpr)
  | binaryRight (operator : IntegerBinaryOp) (left : IntValue)
  deriving DecidableEq

structure MachineConfig where
  names : LexicalEnvironment
  activations : ActivationEnvironment
  store : BaseStore
  control : BaseControl
  continuation : List BaseFrame

inductive BaseOutcome where
  | value (value : IntValue)
  | fault (fault : BaseFault)
  deriving DecidableEq

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
  | .value value => .returned value
  | .fault fault => .failed (.arithmetic fault)

def updateState (config : MachineConfig) (control : BaseControl)
    (continuation : List BaseFrame) : MachineConfig :=
  ⟨config.names, config.activations, config.store, control, continuation⟩

def step (config : MachineConfig) : Option MachineConfig :=
  match config.control with
  | .evaluate expression =>
    match expression with
    | .integerLiteral value =>
      some (updateState config (controlOfIntegerResult (checkedInt value)) config.continuation)
    | .identifier name =>
      some (updateState config
        (match lookupInteger config.names config.activations config.store name with
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
        some (updateState config (controlOfIntegerResult (IntegerOps.negate value)) rest)
      | .binaryLeft operator right =>
        some (updateState config (.evaluate right) (.binaryRight operator value :: rest))
      | .binaryRight operator left =>
        some (updateState config (controlOfIntegerResult (applyBinary operator left value)) rest)
  | .failed _ => none

def initialConfig (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) (expression : BaseExpr) : MachineConfig :=
  ⟨names, activations, store, .evaluate expression, []⟩

def continuationChecked (names : LexicalEnvironment) (activations : ActivationEnvironment)
    (store : BaseStore) : List BaseFrame → Bool
  | [] => true
  | .negate :: rest => continuationChecked names activations store rest
  | .binaryLeft _ right :: rest =>
    checkBaseExpr names activations store right && continuationChecked names activations store rest
  | .binaryRight _ _ :: rest => continuationChecked names activations store rest

def wellFormed (config : MachineConfig) : Prop :=
  (match config.control with
   | .evaluate expression =>
     checkBaseExpr config.names config.activations config.store expression = true
   | .returned _ | .failed _ => True) ∧
  continuationChecked config.names config.activations config.store config.continuation = true

inductive MachineSteps : Nat → MachineConfig → MachineConfig → Prop where
  | refl (config : MachineConfig) : MachineSteps 0 config config
  | step {fuel : Nat} {start next finish : MachineConfig}
      (first : step start = some next) (rest : MachineSteps fuel next finish) :
      MachineSteps (fuel + 1) start finish

inductive RunResult where
  | completed (outcome : BaseOutcome)
  | outOfFuel
  | internalStuck
  deriving DecidableEq

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
