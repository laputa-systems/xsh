import Xsh.Semantics.BaseHostTrace
import Xsh.Semantics.Foundation.ObservedMachine

/-!
The shared invariant and the work measure of the base machine.

`isSpecifiedFault` separates the failures the language specifies from the
checker-class failures that a well-formed state must never reach.
`work` bounds the number of machine steps a configuration can still take.
-/

namespace Xsh.Semantics.BaseInvariant

open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace

/-- Arithmetic faults and a rejected host response are specified outcomes. The remaining faults
describe states that the checker excludes. -/
def isSpecifiedFault : BaseFault → Bool
  | .arithmetic _ => true
  | .hostResponseTypeMismatch => true
  | .unboundName _ => false
  | .inactiveBinder _ => false
  | .missingCell _ => false
  | .unsupportedScalarLiteral => false
  | .expectedInteger => false

/-- Failures that a program causes by its own arithmetic, with no host misbehavior. -/
def isProgramFault : BaseFault → Bool
  | .arithmetic _ => true
  | _ => false

def faultsSpecified (config : MachineConfig) : Prop :=
  ∀ fault, config.control = .failed fault → isSpecifiedFault fault = true

def faultsProgram (config : MachineConfig) : Prop :=
  ∀ fault, config.control = .failed fault → isProgramFault fault = true

/-- A configuration that is well formed and carries only specified failures. -/
def MachineSound (config : MachineConfig) : Prop :=
  wellFormed config ∧ faultsSpecified config

def exprWork : BaseExpr → Nat
  | .integerLiteral _ => 1
  | .scalarLiteral _ => 1
  | .timeNow => 1
  | .identifier _ => 1
  | .negate operand => exprWork operand + 2
  | .binary _ left right => exprWork left + exprWork right + 3

def frameWork : BaseFrame → Nat
  | .negate => 1
  | .binaryLeft _ right => exprWork right + 2
  | .binaryRight _ _ => 1

def continuationWork : List BaseFrame → Nat
  | [] => 0
  | frame :: rest => frameWork frame + continuationWork rest

def controlWork : BaseControl → Nat
  | .evaluate expression => exprWork expression
  | .returned _ => 0
  | .awaitingHost _ => 0
  | .failed _ => 0

/-- An upper bound on the machine steps remaining before the configuration is final or waiting. -/
def work (config : MachineConfig) : Nat :=
  controlWork config.control + continuationWork config.continuation

/-- The most recent observation of a run is the request the machine now awaits. -/
def requestIsLastObservation (config : Xsh.Semantics.Foundation.Configuration) : Prop :=
  ∀ {responseType : BaseType} (request : BaseHostRequest responseType),
    config.current.control = .awaitingHost request →
      ∃ earlier, config.observations = earlier ++ [.request request]

end Xsh.Semantics.BaseInvariant
