import Xsh.Proofs.Core.Conditions

namespace Xsh.Proofs.Core.Requires

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Semantics.CoreControl
open Xsh.Proofs.Core.Control

section Root

variable {Declaration Identity Source : Type}

/-- The `require_expression` fragment root (SPEC 5.3): `value.require(T)`
evaluates `value` once and gives the validation model's `requireValue` for the
checker's `T`, so under the closed VAL-REQUIRE theorem its `Ok` holds exactly
the conversions of the value to `T`, each a member of `T` in the machine's
membership world, and its `Err` the schema error of the conversion's failure. -/
theorem require_expression [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ value schema type, machine.typeOf schema = some type →
      evalExpr machine (fuel + 1) frame (.require value (some schema)) =
        (evalExpr machine fuel frame value).bind fun candidate next => .value (requireValue machine type candidate) next) ∧
    (∀ type candidate result, Xsh.Validation.Core.WellFormed.WireWellFormed machine.enums →
      Xsh.Validation.Core.WellFormed.WellFormed type →
      requireValue machine type candidate = .ok result → Membership.value world.membership type result) ∧
    (∀ type candidate error, Xsh.Validation.Core.WellFormed.WireWellFormed machine.enums →
      Xsh.Validation.Core.WellFormed.WellFormed type →
      (requireValue machine type candidate = .err error ↔
        ∃ failure, Xsh.Validation.Core.Require.convert ⟨world.membership, machine.decisions.contracts, machine.enums⟩
            .keep [] type candidate = .error failure ∧
          error = .error (Xsh.Validation.Core.Require.schemaError machine.schemaFamily failure))) := by
  refine ⟨fun value schema type typed => ?_, fun type candidate result wires wellFormed passed => ?_,
    fun type candidate error wires wellFormed => ?_⟩
  · simp only [evalExpr, typed]
  · have laws := Xsh.Validation.Core.Require.recursive_require
      ⟨world.membership, machine.decisions.contracts, machine.enums⟩ wires machine.schemaFamily .keep type wellFormed
      candidate
    exact laws.2.1 result ((laws.1 result).mp passed)
  · exact (Xsh.Validation.Core.Require.recursive_require
      ⟨world.membership, machine.decisions.contracts, machine.enums⟩ wires machine.schemaFamily .keep type wellFormed
      candidate).2.2.2.1 error

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Slices.Witness

/-- The witness machine with every written type read as `Int`. -/
def intMachine : Machine witnessWorld := { machine with typeOf := fun _ => some (.primitive .integer) }

def requireInt (value : Xsh.Syntax.Expr) : Option (Value Unit Unit Unit) :=
  readValue (runStatements intMachine 40 start [letValue (.require value (some (.named nName)))])

def okInt : Value Unit Unit Unit → Option Int
  | .ok (.scalar (.integer value)) => some value.val
  | _ => none

def schemaVariant : Value Unit Unit Unit → Option String
  | .err (.error data) => some data.variant
  | _ => none

/-- As SPEC 5.3: `7.require(Int)` is `Ok(7)` and `"x".require(Int)` is the
`Err` of a schema error. -/
theorem require_witness :
    (requireInt (int "7")).bind okInt = some 7 ∧
      (requireInt (.literal (.string "x"))).bind schemaVariant = some "schema" := by
  decide +kernel

/-- A `.require` that passes its value through unchecked gives `Ok("x")`; the
model gives a schema error. -/
theorem unchecked_require_rejected :
    (match (Value.ok (.scalar (.string "x")) : Value Unit Unit Unit) with
      | .err _ => true
      | _ => false) = false ∧
      (requireInt (.literal (.string "x"))).bind schemaVariant = some "schema" := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Requires
