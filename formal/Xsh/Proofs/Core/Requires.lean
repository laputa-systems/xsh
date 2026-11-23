import Xsh.Proofs.Core.Conditions
import Xsh.Proofs.Core.Scripts

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
membership world, and its `Err` the schema error of the conversion's failure.
A contextual `.require()` in an annotated binding takes its target from the
annotation: the annotation's `Result` value type, or under `.require()?` the
annotation itself, as each `?` peels one `Result` layer; a function's declared
result is likewise the target of its body's tail and of each `return`, also
one nested in its statements. -/
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
          error = .error (Xsh.Validation.Core.Require.schemaError machine.schemaFamily failure))) ∧
    (∀ kind target annotation initializer, runStatement machine (fuel + 1) frame (.binding kind target annotation initializer) =
      (runValue machine fuel frame (expectTarget machine.context.spelling machine.declarations annotation initializer)).toExec fun value next =>
        bindTarget machine next (bindingMutability kind) target value) ∧
    (∀ value schema, expectTarget machine.context.spelling machine.declarations (some (.result schema)) (.require value none) = .require value (some schema)) ∧
    (∀ value schema, expectTarget machine.context.spelling machine.declarations (some schema) (.tryExpr (.require value none)) =
      .tryExpr (.require value (some schema))) ∧
    (∀ result tail, expectBody machine.context.spelling machine.declarations result [.expression tail] = [.expression (expectTarget machine.context.spelling machine.declarations result tail)]) ∧
    (∀ result value rest, expectBody machine.context.spelling machine.declarations result (.returnValue (some value) :: rest) =
      .returnValue (some (expectTarget machine.context.spelling machine.declarations result value)) :: expectBody machine.context.spelling machine.declarations result rest) := by
  refine ⟨fun value schema type typed => ?_, fun type candidate result wires wellFormed passed => ?_,
    fun type candidate error wires wellFormed => ?_, fun _ _ _ _ => rfl, fun _ _ => rfl,
    fun _ schema => by cases schema <;> rfl, fun _ _ => rfl, fun _ _ _ => rfl⟩
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
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Scripts.Witness

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

def plainInt : Value Unit Unit Unit → Option Int
  | .scalar (.integer value) => some value.val
  | _ => none

def annotatedRequire (annotation : Xsh.Syntax.TypeExpr) (initializer : Xsh.Syntax.Expr) : Option (Value Unit Unit Unit) :=
  readValue (runStatements intMachine 40 start [.binding .letBinding (.name xName) (some annotation) initializer])

/-- `let v: Result[Int] = "x".require()` and `let w: Int = 7.require()?`, as
the verification binary gives the schema `Err` and `7`: the annotation is the
contextual target, its `Result` peeled once per `?`. -/
theorem contextual_require_witness :
    (annotatedRequire (.result (.named nName)) (.require (.literal (.string "x")) none)).bind schemaVariant =
      some "schema" ∧
    (annotatedRequire (.named nName) (.tryExpr (.require (int "7") none))).bind plainInt = some 7 := by
  decide +kernel

/-- Without a target the contextual form has no meaning in the model: the
same binding unannotated does not bind. -/
theorem untargeted_require_rejected :
    readValue (runStatements intMachine 40 start [.binding .letBinding (.name xName) none (.require (int "7") none)]) = none ∧
      (annotatedRequire (.result (.named nName)) (.require (int "7") none)).bind okInt = some 7 := by
  decide +kernel

def requiring : Xsh.Syntax.RawName := name 300 (by decide)
def requiredOut : Xsh.Syntax.RawName := name 301 (by decide)

/-- A written `Result[...]` reads as `Result[Int, Error]` and any other type
as `Int`. -/
def resultOrInt : Xsh.Syntax.TypeExpr → Option (ValueType Unit)
  | .result _ => some (.result (.primitive .integer) (.primitive .error))
  | _ => some (.primitive .integer)

def returnMachine : Machine witnessWorld := { scriptMachine with typeOf := resultOrInt }

/-- `proc f() -> declared { body }` and `let out = try { f() }`. -/
def declaredCall (declared : Xsh.Syntax.TypeExpr) (body : List Xsh.Syntax.Statement) : Option (Value Unit Unit Unit) :=
  match runScript returnMachine 90 fullStart
      [.functionDef .proc requiring [] none (some declared) body none false,
       letOf requiredOut (.capture [.expression (.call (ident requiring) [])])] with
  | .normal frame =>
    match frame.environment.cellOf requiredOut with
    | .ok cell => (frame.store cell).map fun found => found.contents.value
    | .error _ => none
  | _ => none

def nestedSchema : Value Unit Unit Unit → Option String
  | .ok inner => schemaVariant inner
  | other => schemaVariant other

def nestedInt : Value Unit Unit Unit → Option Int
  | .ok inner => (okInt inner).orElse fun _ => plainInt inner
  | _ => none

/-- `proc a() -> Result[Int] { "x".require() }`, `proc b() -> Result[Int] {
return 7.require() }`, `proc c() -> Int { 7.require()? }` and `proc d() -> Int
{ return "x".require()? }`, as the verification binary prints `err`, `ok 7`,
`7` and `err`: the declared result is the target of the tail and of each
`return`. -/
theorem returned_require_witness :
    (declaredCall (.result (.named nName)) [.expression (.require (.literal (.string "x")) none)]).bind nestedSchema =
      some "schema" ∧
    (declaredCall (.result (.named nName)) [.returnValue (some (.require (int "7") none))]).bind nestedInt = some 7 ∧
    (declaredCall (.named nName) [.expression (.tryExpr (.require (int "7") none))]).bind nestedInt = some 7 ∧
    (declaredCall (.named nName) [.returnValue (some (.tryExpr (.require (.literal (.string "x")) none)))]).bind
      nestedSchema = some "schema" := by
  decide +kernel

/-- A function that ignored its declared result would leave the tail's
`.require()` without a target, and the call would not complete; undeclared,
it does not. -/
theorem undeclared_return_rejected :
    (match runScript returnMachine 90 fullStart
        [.functionDef .proc requiring [] none none [.expression (.require (int "7") none)] none false,
         letOf requiredOut (.capture [.expression (.call (ident requiring) [])])] with
      | .normal _ => true
      | _ => false) = false ∧
    (declaredCall (.result (.named nName)) [.returnValue (some (.require (int "7") none))]).bind nestedInt = some 7 := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Requires
