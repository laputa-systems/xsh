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

def applyFn : Xsh.Syntax.RawName := name 230 (by decide)
def twiceFn : Xsh.Syntax.RawName := name 231 (by decide)
def fParam : Xsh.Syntax.RawName := name 232 (by decide)
def xParam : Xsh.Syntax.RawName := name 233 (by decide)
def nParam : Xsh.Syntax.RawName := name 234 (by decide)
def pickFn : Xsh.Syntax.RawName := name 235 (by decide)
def gLocal : Xsh.Syntax.RawName := name 236 (by decide)

/-- `pure apply(f: Op, x: Int) -> Int { f(x) }`, `pure twice(n: Int) -> Int
{ n * 2 }` and `pure pick() -> Op { twice }`, then `apply(twice, 7)`,
`let g = pick(); g(7)` and `apply([twice][0], 7)`. -/
def firstClass : List Xsh.Syntax.Statement :=
  [.functionDef .pure applyFn [.mk fParam none none false, .mk xParam none none false] none none
     [.expression (.call (ident fParam) [.positional (ident xParam)])] none false,
   .functionDef .pure twiceFn [.mk nParam none none false] none none
     [.expression (.binary .mul (ident nParam) (int "2"))] none false,
   .functionDef .pure pickFn [] none none [.expression (ident twiceFn)] none false,
   letOf (output 0) (.call (ident applyFn) [.positional (ident twiceFn), .positional (int "7")]),
   letOf gLocal (.call (ident pickFn) []),
   letOf (output 1) (.call (ident gLocal) [.positional (int "7")]),
   letOf (output 2) (.call (ident applyFn)
     [.positional (.index (.list [ident twiceFn]) (int "0") false), .positional (int "7")])]

/-- The cell hoisting binds `twice` to, which the witness world's single
callable identity names. -/
def twiceCell : Option CellId :=
  (hoist scriptMachine fullStart firstClass).bind fun hoisted =>
    match hoisted.environment.cellOf twiceFn with
    | .ok cell => some cell
    | .error _ => none

def firstClassMachine : Machine witnessWorld := { scriptMachine with callableCell := fun _ => twiceCell }

/-- Nested declarations are rejected (`check.nested-declaration`), so every
callable is a hoisted top-level function, and every call's closure table
holds the caller's, so a function value passed to, returned from or stored
by another function is found where it is called. As the verification binary
prints `14`, `8` and `2` for `apply(double, 7)`, `pick(false)(7)` and
`apply(ops[1], 1)`, the model gives `14` for each shape. -/
theorem function_values_travel :
    readInt (output 0) (runScript firstClassMachine 90 fullStart firstClass) = some 14 ∧
      readInt (output 1) (runScript firstClassMachine 90 fullStart firstClass) = some 14 ∧
      readInt (output 2) (runScript firstClassMachine 90 fullStart firstClass) = some 14 := by
  decide +kernel

/-- A callee whose closure table is only what its declaration saw cannot call
the later `twice` passed to the earlier `apply`. -/
theorem declaration_only_table_rejected :
    (match hoist scriptMachine fullStart firstClass with
      | some hoisted =>
        match localClosure hoisted.closures hoisted.environment applyFn with
        | some (.mk _ _ _ captured _ _ _) => twiceCell.bind fun cell => (captured.lookup cell).map fun _ => true
        | none => none
      | none => none) = none ∧
      readInt (output 0) (runScript firstClassMachine 90 fullStart firstClass) = some 14 := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Exports
