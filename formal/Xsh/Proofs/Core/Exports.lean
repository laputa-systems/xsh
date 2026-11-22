import Xsh.Proofs.Core.Scripts
import Xsh.Proofs.Core.Contexts

namespace Xsh.Proofs.Core.Exports

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

/-- The `exported_declarations` fragment root (SPEC 3.3): in a script an
exported declaration runs, hoists and declares names in the pass exactly as
the declaration it wraps; exporting affects only a module's interface, which
the machine does not load. An exported `proc` or `pure` is hoisted with the
script's other functions. -/
theorem exported_declarations [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) :
    (∀ fuel (frame : Frame world) declaration,
      runStatement machine (fuel + 1) frame (.export declaration) = runStatement machine fuel frame declaration) ∧
    (∀ activation environment mutable closures declaration,
      hoistStatement machine activation environment mutable closures (.export declaration) =
        hoistStatement machine activation environment mutable closures declaration) ∧
    (∀ scopes declaration, staticDeclare scopes (.export declaration) = staticDeclare scopes declaration) ∧
    (∀ kind name parameters effects result body metadata test rest,
      hoistedNames (.export (.functionDef kind name parameters effects result body metadata test) :: rest) =
        hoistedNames (.functionDef kind name parameters effects result body metadata test :: rest)) := by
  refine ⟨fun _ _ _ => rfl, fun _ _ _ _ _ => rfl, fun scopes _ => by cases scopes <;> rfl,
    fun _ _ _ _ _ _ _ _ _ => rfl⟩

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness
open Xsh.Proofs.Core.Hoisting.Witness
open Xsh.Proofs.Core.Scripts.Witness

def earlyProc : Xsh.Syntax.RawName := name 210 (by decide)
def sevenName : Xsh.Syntax.RawName := name 211 (by decide)

/-- `print (early()); export pure early() -> Int { 7 }; export let seven = 7;
print seven`: as the verification binary prints `3` for an exported `pure`
called above its declaration and `14` for `twice(seven)`, an exported
function is hoisted and an exported binding binds. -/
def exportingScript : List Xsh.Syntax.Statement :=
  [letOf (output 0) (call earlyProc []),
    .export (.functionDef .pure earlyProc [] none none [.expression (int "7")] none false),
    .export (letOf sevenName (int "7")), letOf (output 1) (ident sevenName)]

theorem exports_run :
    readInt (output 0) (runCanonical exportingScript) = some 7 ∧
      readInt (output 1) (runCanonical exportingScript) = some 7 := by
  decide +kernel

/-- Without hoisting the exported function the first call has no callee and
the script does not complete; the model hoists it. -/
theorem unhoisted_export_rejected :
    readInt (output 0) (runStatements scriptMachine 90 fullStart exportingScript) = none ∧
      readInt (output 0) (runCanonical exportingScript) = some 7 := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Exports
