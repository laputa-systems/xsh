import Xsh.Proofs.Core.Requires

namespace Xsh.Proofs.Core.Variants

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

/-- The `leading_dot_variants` fragment root (SPEC 5.5): where the expected
type is known, a leading-dot name selects a variant of the enum it names, an
optional `T?` selecting from `T`; an annotated binding and a function's
declared result, through its tail and each top-level `return`, give that type.
The selected variant is the declared one, with the enum's declaration
identity and its payload arguments evaluated once in order, as its bare
spelling gives; a leading-dot name with no expected type is outside the model
(`check.inferred-variant`), and one whose expected enum does not declare it
does not evaluate. -/
theorem leading_dot_variants [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ enum name arguments, expectTarget (some (.named enum)) (.dotVariant none name arguments) =
      .dotVariant (some enum) name arguments) ∧
    (∀ enum name arguments, expectTarget (some (.optional (.named enum))) (.dotVariant none name arguments) =
      .dotVariant (some enum) name arguments) ∧
    (∀ name arguments, evalExpr machine (fuel + 1) frame (.dotVariant none name arguments) = .exec .unsupported) ∧
    (∀ enum name declared variant wire, variantOf machine.context.spelling machine.declarations name =
        some (declared, .mk variant [] wire) → machine.context.spelling declared = machine.context.spelling enum →
      evalExpr machine (fuel + 1) frame (.dotVariant (some enum) name none) =
        .value (.tagged (machine.identity declared) (machine.context.spelling variant) []) frame) ∧
    (∀ enum name arguments declared variant fields wire, variantOf machine.context.spelling machine.declarations name =
        some (declared, .mk variant fields wire) → machine.context.spelling declared = machine.context.spelling enum →
      evalExpr machine (fuel + 1) frame (.dotVariant (some enum) name (some arguments)) =
        (threadArguments machine.context.spelling (evalExpr machine fuel) frame arguments).bind fun supplied after =>
          match supplied.mapM Supplied.positionalValue with
          | some values =>
            if values.length = fields.length then
              .value (.tagged (machine.identity declared) (machine.context.spelling variant) values) after
            else .exec .stuck
          | none => .exec .stuck) ∧
    (∀ enum name arguments declared variant, variantOf machine.context.spelling machine.declarations name =
        some (declared, variant) → machine.context.spelling declared ≠ machine.context.spelling enum →
      evalExpr machine (fuel + 1) frame (.dotVariant (some enum) name arguments) = .exec .stuck) := by
  refine ⟨fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ => rfl, fun enum name declared variant wire found named => ?_,
    fun enum name arguments declared variant fields wire found named => ?_,
    fun enum name arguments declared variant found other => ?_⟩
  · simp only [evalExpr, found, named, if_true]
  · cases fields <;> (simp only [evalExpr, found, named, if_true]; try rfl)
  · rcases variant with ⟨variant, fields, wire⟩
    cases arguments <;> cases fields <;> simp [evalExpr, found, other]

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness
open Xsh.Proofs.Core.Hoisting.Witness

def shapeName : Xsh.Syntax.RawName := name 302 (by decide)
def dotName : Xsh.Syntax.RawName := name 303 (by decide)
def ringName : Xsh.Syntax.RawName := name 304 (by decide)
def pickName : Xsh.Syntax.RawName := name 305 (by decide)
def pickRing : Xsh.Syntax.RawName := name 306 (by decide)

def slot (index : Nat) : Xsh.Syntax.RawName :=
  [name 307 (by decide), name 308 (by decide), name 309 (by decide), name 310 (by decide),
    name 311 (by decide)].getD index (name 312 (by decide))

/-- `enum Shape { Dot, Ring(Int) }`. -/
def declareShape : Xsh.Syntax.Statement :=
  .typeDef shapeName [] (.tagUnion [.mk dotName [] none, .mk ringName [anyType] none]) true none

def shapeProgram : Context Unit Unit Unit :=
  { enumProgram with
    spelling := fun named =>
      if named = shapeName then "Shape" else if named = dotName then "Dot" else if named = ringName then "Ring"
      else enumProgram.spelling named }

def shapeMachine : Machine witnessWorld :=
  { enumMachine with context := shapeProgram, declarations := enumMachine.declarations ++ [declareShape] }

def dot (variant : Xsh.Syntax.RawName) : Xsh.Syntax.Expr := .dotVariant none variant none

def dotCall (variant : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.Expr) : Xsh.Syntax.Expr :=
  .dotVariant none variant (some (arguments.map .positional))

def annotated (target : Xsh.Syntax.RawName) (annotation : Xsh.Syntax.TypeExpr) (value : Xsh.Syntax.Expr) :
    Xsh.Syntax.Statement :=
  .binding .letBinding (.name target) (some annotation) value

/-- `let a: Level = .Warn`, `let o: Level? = .Info`, `let s: Shape = .Ring(7)`,
and `proc pick() -> Level { .Warn }` and `proc ring() -> Shape { return .Ring(1) }`
called. -/
def variantScript : List Xsh.Syntax.Statement :=
  [declareLevel, declareShape,
   .functionDef .proc pickName [] none (some (.named levelName)) [.expression (dot warnName)] none false,
   .functionDef .proc pickRing [] none (some (.named shapeName)) [.returnValue (some (dotCall ringName [int "1"]))]
     none false,
   annotated (slot 0) (.named levelName) (dot warnName),
   annotated (slot 1) (.optional (.named levelName)) (dot infoName),
   annotated (slot 2) (.named shapeName) (dotCall ringName [int "7"]),
   letOf (slot 3) (.call (ident pickName) []),
   letOf (slot 4) (.call (ident pickRing) [])]

def variantRun : Exec witnessWorld := runScript shapeMachine 90 fullStart variantScript

/-- A variant's name and integer payload. -/
def variantShape : Option (Value Unit Unit Unit) → Option (String × List Int)
  | some (.tagged _ variant fields) => (fields.mapM intOf).map (variant, ·)
  | _ => none

/-- As the verification binary prints `true` for `a == Red`, `m == Mix(7)`,
`o == Blue`, `pick() == Blue` and `pick2() == Mix(1)` in the same program
over its own enum: the annotation or declared result selects the variant. -/
theorem leading_dot_witness :
    variantShape (valueOf (slot 0) variantRun) = some ("Warn", []) ∧
    variantShape (valueOf (slot 1) variantRun) = some ("Info", []) ∧
    variantShape (valueOf (slot 2) variantRun) = some ("Ring", [7]) ∧
    variantShape (valueOf (slot 3) variantRun) = some ("Warn", []) ∧
    variantShape (valueOf (slot 4) variantRun) = some ("Ring", [1]) := by
  decide +kernel

/-- `let a = .Info` has no expected type, which the binary rejects with
`check.inferred-variant`, and `let w: Shape = .Info` selects from an enum
without that variant; neither binds. -/
theorem untargeted_variant_rejected :
    valueOf (slot 0) (runScript shapeMachine 90 fullStart [declareLevel, letOf (slot 0) (dot infoName)]) = none ∧
    valueOf (slot 0) (runScript shapeMachine 90 fullStart
      [declareLevel, declareShape, annotated (slot 0) (.named shapeName) (dot infoName)]) = none ∧
    variantShape (valueOf (slot 0) variantRun) = some ("Warn", []) := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Variants
