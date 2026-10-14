import Xsh.Semantics.BaseHostTrace

/-!
An observation-bearing configuration for the existing checked scalar machine.
The underlying expression syntax stays untyped; the checker and machine carry
the separate type facts. Values, lexical bindings, activation cells, store,
host requests, failures, and arithmetic all come from the shared base machine.
-/

namespace Xsh.Semantics.Foundation

open Xsh.Semantics.BaseMachine
open Xsh.Semantics.BaseHostTrace

/-- The expression subset used by this configuration has no type annotations. -/
abbrev CoreTerm := BaseExpr

/-- A run keeps its exact entry state and the ordered observations produced so far. -/
structure Configuration where
  entry : MachineConfig
  current : MachineConfig
  observations : List BaseHostEvent

/-- A fresh configuration begins at an expression-evaluation state. -/
def Configuration.start (machine : MachineConfig) (term : CoreTerm)
    (_evaluating : machine.control = .evaluate term) : Configuration :=
  ⟨machine, machine, []⟩

/-- Final values and specified failures remain the base machine's closed outcomes. -/
def Configuration.outcome? (config : Configuration) : Option BaseOutcome :=
  terminalOutcome config.current

/-- A waiting configuration exposes the operation-indexed request to its host. -/
def Configuration.hostWait? (config : Configuration) : Option BaseHostWait :=
  pendingHostWait? config.current

/-- The invariant ties the current state and ordered host log to one checked entry state. -/
def wellFormed (config : Configuration) : Prop :=
  (∃ term, config.entry.control = .evaluate term) ∧
    Xsh.Semantics.BaseMachine.wellFormed config.entry ∧
    Xsh.Semantics.BaseMachine.wellFormed config.current ∧
    BaseHostTrace config.entry config.observations config.current

/-- Each labeled edge records exactly the host-boundary event it performs. -/
inductive LabeledStep : Configuration → BaseHostEvent → Configuration → Prop where
  | edge {config : Configuration} {next : MachineConfig} {event : BaseHostEvent}
      (transition : BaseHostTransition config.current event next) :
      LabeledStep config event
        ⟨config.entry, next, config.observations ++ [event]⟩

end Xsh.Semantics.Foundation
