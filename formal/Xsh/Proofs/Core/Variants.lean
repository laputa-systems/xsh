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
declared result, through its tail and each `return` however deeply it sits in
the body's `if`, `while`, `for`, `loop`, `match` and `guard` statements or
in a value block or `if` or `match` branch of a binding or expression
statement, give that type, and so does a parameter's annotation to its default and to the
argument that fills it, positional
arguments filling the non-rest parameters in order and a named one the
parameter it names; an expected type reaches each branch of a value `if` or
`match`, a value block's tail, each element of an expected `List[T]`, each value
of a brace literal under an expected `Map[K, V]`, the payload of `Ok(...)`
under an expected `Result[T]` or `Result[T, E]` and of `Err(...)` under an
expected `Result[T, E]`, a leading-dot name no enum declares then being the
error family's constructor; a brace literal under an annotation naming a
record schema gives each named field's value its schema field's type, as
does a declared result's record schema to its tail and every `return`
operand, nested ones included; the
other operand of `==` or `!=`, when it is an enum's variant, targets a
leading-dot name on either side, and `null` there compares unequal; and
the same expected type targets a contextual `.require()`.
The selected variant is the declared one, with the enum's declaration
identity and its payload arguments evaluated once in order, as its bare
spelling gives; a leading-dot name with no expected type is outside the model
(`check.inferred-variant`), and one whose expected enum does not declare it
does not evaluate. -/
theorem leading_dot_variants [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ enum name arguments, expectTarget machine.context.spelling (some (.named enum)) (.dotVariant none name arguments) =
      .dotVariant (some enum) name arguments) ∧
    (∀ enum name arguments, expectTarget machine.context.spelling (some (.optional (.named enum))) (.dotVariant none name arguments) =
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
      evalExpr machine (fuel + 1) frame (.dotVariant (some enum) name arguments) = .exec .stuck) ∧
    (∀ evaluate callBlock closure arguments, invokeWith machine evaluate callBlock frame closure arguments =
      (threadArguments machine.context.spelling evaluate frame
          (expectArguments machine.context.spelling closure.parameters closure.parameters
            (schemaArguments machine.context.spelling machine.declarations closure.parameters closure.parameters arguments))).bind
        fun supplied after => invokeSupplied machine evaluate callBlock after closure supplied) ∧
    (∀ parameters name annotation default positions value rest,
      expectArguments machine.context.spelling parameters (.mk name annotation default false :: positions)
          (.positional value :: rest) =
        .positional (expectTarget machine.context.spelling annotation value) ::
          expectArguments machine.context.spelling parameters positions rest) ∧
    (∀ parameters positions label value rest,
      expectArguments machine.context.spelling parameters positions (.named label value :: rest) =
        .named label (expectTarget machine.context.spelling ((parameters.find? fun
            | .mk name _ _ _ => machine.context.spelling name = machine.context.spelling label).bind
          fun | .mk _ annotation _ _ => annotation) value) ::
          expectArguments machine.context.spelling parameters positions rest) ∧
    (∀ target branches otherwise, expectTarget machine.context.spelling (some target) (.ifExpr branches otherwise) =
      .ifExpr (expectBranches machine.context.spelling (some target) branches) (expectTarget machine.context.spelling (some target) otherwise)) ∧
    (∀ target subject arms, expectTarget machine.context.spelling (some target) (.matchExpr subject arms) =
      .matchExpr subject (expectArms machine.context.spelling (some target) arms)) ∧
    (∀ target body, expectTarget machine.context.spelling (some target) (.valueBlock body) = .valueBlock (expectTail machine.context.spelling (some target) body)) ∧
    (∀ element elements, expectTarget machine.context.spelling (some (.list element)) (.list elements) =
      .list (expectElements machine.context.spelling (some element) elements)) ∧
    (∀ expected tail, expectTail machine.context.spelling expected [.expression tail] = [.expression (expectTarget machine.context.spelling expected tail)]) ∧
    (∀ target constructor payload, machine.context.spelling constructor = "Ok" →
      expectTarget machine.context.spelling (some (.result target)) (.call (.ident constructor) [.positional payload]) =
        .call (.ident constructor) [.positional (expectTarget machine.context.spelling (some target) payload)]) ∧
    (∀ key value fields, expectTarget machine.context.spelling (some (.map key value)) (.record fields) =
      .record (expectFields machine.context.spelling (some value) fields)) ∧
    (∀ expected name value rest, expectFields machine.context.spelling expected (.named name value :: rest) =
      .named name (expectTarget machine.context.spelling expected value) :: expectFields machine.context.spelling expected rest) ∧
    (∀ expected key value rest, expectFields machine.context.spelling expected (.computed key value :: rest) =
      .computed key (expectTarget machine.context.spelling expected value) :: expectFields machine.context.spelling expected rest) ∧
    (∀ target error constructor payload, machine.context.spelling constructor = "Err" →
      machine.context.spelling constructor ≠ "Ok" →
      expectTarget machine.context.spelling (some (.resultWith target error)) (.call (.ident constructor) [.positional payload]) =
        .call (.ident constructor) [.positional (expectTarget machine.context.spelling (some error) payload)]) ∧
    (∀ holder schema nominal (fields : List Xsh.Syntax.RecordField), schemaOf machine.declarations holder = some (schema, nominal) →
      expectSchema machine.context.spelling machine.declarations (some (.named holder)) (.record fields) =
        .record (expectSchemaFields machine.context.spelling machine.declarations schema fields)) ∧
    (∀ schema label value rest,
      expectSchemaFields machine.context.spelling machine.declarations schema (.named label value :: rest) =
        let type := (schema.find? fun | .mk field _ _ => machine.context.spelling field = machine.context.spelling label).map
          fun | .mk _ type _ => type
        .named label (expectTarget machine.context.spelling type (expectSchema machine.context.spelling machine.declarations type value)) ::
          expectSchemaFields machine.context.spelling machine.declarations schema rest) ∧
    (∀ family name arguments, variantOf machine.context.spelling machine.declarations name = none →
      evalExpr machine (fuel + 1) frame (.dotVariant (some family) name arguments) =
        evalExpr machine fuel frame (.call (.field (.ident family) name) (arguments.getD []))) ∧
    (∀ result statement rest, (∀ tail, statement :: rest ≠ [.expression tail]) →
      expectBody machine.context.spelling result (statement :: rest) =
        expectReturn (expectTarget machine.context.spelling result) statement :: expectBody machine.context.spelling result rest) ∧
    (∀ result value, expectReturn (expectTarget machine.context.spelling result) (.returnValue (some value)) =
      .returnValue (some (expectTarget machine.context.spelling result value))) ∧
    (∀ result branches otherwise, expectReturn (expectTarget machine.context.spelling result) (.ifStmt branches otherwise) =
      .ifStmt (expectReturnBranches (expectTarget machine.context.spelling result) branches)
        (otherwise.map (expectReturns (expectTarget machine.context.spelling result)))) ∧
    (∀ result condition body rest, expectReturnBranches (expectTarget machine.context.spelling result) ((condition, body) :: rest) =
      (condition, expectReturns (expectTarget machine.context.spelling result) body) :: expectReturnBranches (expectTarget machine.context.spelling result) rest) ∧
    (∀ result condition body, expectReturn (expectTarget machine.context.spelling result) (.whileStmt condition body) =
      .whileStmt condition (expectReturns (expectTarget machine.context.spelling result) body)) ∧
    (∀ result target iterable body, expectReturn (expectTarget machine.context.spelling result) (.forStmt target iterable body) =
      .forStmt target iterable (expectReturns (expectTarget machine.context.spelling result) body)) ∧
    (∀ result body, expectReturn (expectTarget machine.context.spelling result) (.loopStmt body) =
      .loopStmt (expectReturns (expectTarget machine.context.spelling result) body)) ∧
    (∀ result subject arms, expectReturn (expectTarget machine.context.spelling result) (.matchStmt subject arms) =
      .matchStmt subject (expectReturnArms (expectTarget machine.context.spelling result) arms)) ∧
    (∀ result pattern guard body rest, expectReturnArms (expectTarget machine.context.spelling result) ((pattern, guard, body) :: rest) =
      (pattern, guard, expectReturns (expectTarget machine.context.spelling result) body) :: expectReturnArms (expectTarget machine.context.spelling result) rest) ∧
    (∀ result target annotation initializer otherwise,
      expectReturn (expectTarget machine.context.spelling result) (.guardStmt target annotation initializer otherwise) =
        .guardStmt target annotation initializer (expectReturns (expectTarget machine.context.spelling result) otherwise)) ∧
    (∀ result statement rest, expectReturns (expectTarget machine.context.spelling result) (statement :: rest) =
      expectReturn (expectTarget machine.context.spelling result) statement :: expectReturns (expectTarget machine.context.spelling result) rest) ∧
    (∀ name annotation default rest more,
      expectDefaults machine.context.spelling (.mk name annotation (some default) rest :: more) =
        .mk name annotation (some (expectTarget machine.context.spelling annotation default)) rest ::
          expectDefaults machine.context.spelling more) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
        (callBlock : Frame world → Scope Declaration Identity Source → Environment → Locals Declaration Identity Source →
          List CellId → List Xsh.Syntax.Statement → ReturnKind → Valued world)
        after cell callable defining captured writable parameters body supplied slots,
      placeCall machine.context.spelling (expectDefaults machine.context.spelling parameters) supplied = .ok slots →
        invokeSupplied machine evaluate callBlock after (.mk cell callable defining captured writable parameters body)
            supplied =
          (bindSlots
              (fun current default => returnValue current (evaluate
                { Frame.ofScope defining current.store [] (some current.store)
                    ⟨current.activation + machine.activationStep,
                      (cell, .mk cell callable defining captured writable parameters body) :: captured ++ current.closures,
                      none⟩ with
                  writable } default))
              after slots).bind fun bound last =>
            callBlock last ⟨last.activation + machine.activationStep,
              (cell, .mk cell callable defining captured writable parameters body) :: captured ++ last.closures, none⟩
              defining bound
              writable (bodyStatements body) (bodyKind body)) ∧
    (∀ result statement rest, (∀ tail, statement :: rest ≠ [.expression tail]) →
      expectSchemaBody machine.context.spelling machine.declarations result (statement :: rest) =
        expectReturn (expectSchema machine.context.spelling machine.declarations result) statement ::
          expectSchemaBody machine.context.spelling machine.declarations result rest) := by
  refine ⟨fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ => rfl, fun enum name declared variant wire found named => ?_,
    fun enum name arguments declared variant fields wire found named => ?_,
    fun enum name arguments declared variant found other => ?_, fun _ _ _ _ => rfl, fun _ _ _ _ _ _ _ => rfl,
    fun _ _ _ _ _ => rfl, fun target _ _ => by cases target <;> simp [expectTarget],
    fun target _ _ => by cases target <;> simp [expectTarget], fun target _ => by cases target <;> simp [expectTarget],
    fun _ _ => by simp [expectTarget], fun _ _ => by simp [expectTail],
    fun _ _ _ named => by simp [expectTarget, named], fun _ _ _ => by simp [expectTarget],
    fun _ _ _ _ => by simp [expectFields], fun _ _ _ _ => by simp [expectFields],
    fun _ _ _ _ isErr notOk => by simp [expectTarget, isErr, notOk],
    fun _ _ _ _ found => by simp only [expectSchema, found], fun _ _ _ _ => by simp only [expectSchemaFields]; rfl,
    fun _ _ arguments absent => by cases arguments <;> simp [evalExpr, absent],
    fun result statement rest notTail => by
      cases rest with
      | cons _ _ => cases statement <;> rfl
      | nil => cases statement <;> first | rfl | exact absurd rfl (notTail _), fun _ _ => rfl,
    fun _ _ otherwise => by cases otherwise <;> rfl, fun _ _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ _ _ => rfl,
    fun _ _ => rfl, fun _ _ _ => rfl, fun _ _ _ _ _ => rfl, fun _ _ _ _ _ => rfl, fun _ _ _ => rfl,
    fun _ _ _ _ _ => rfl, fun _ _ _ _ _ _ _ _ _ _ _ _ placed => by simp only [invokeSupplied, placed],
    fun result statement rest notTail => by
      cases rest with
      | cons _ _ => cases statement <;> rfl
      | nil => cases statement <;> first | rfl | exact absurd rfl (notTail _)⟩
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

def echoName : Xsh.Syntax.RawName := name 313 (by decide)
def echoParam : Xsh.Syntax.RawName := name 314 (by decide)

/-- `proc echo(c: Level) -> Level { c }`, called as `echo(.Warn)` and
`echo(c: .Info)`. -/
def argumentScript : List Xsh.Syntax.Statement :=
  [declareLevel,
   .functionDef .proc echoName [.mk echoParam (some (.named levelName)) none false] none (some (.named levelName))
     [.expression (ident echoParam)] none false,
   letOf (slot 0) (.call (ident echoName) [.positional (dot warnName)]),
   letOf (slot 1) (.call (ident echoName) [.named echoParam (dot infoName)])]

def argumentRun : Exec witnessWorld := runScript shapeMachine 90 fullStart argumentScript

/-- `proc take(r: Result[Int]) -> Result[Int] { r }`, called as
`take("x".require())` and `take(r: 7.require())`. -/
def requireArgument (argument : Xsh.Syntax.Expr → Xsh.Syntax.CallArgument) (value : Xsh.Syntax.Expr) :
    Option (Value Unit Unit Unit) :=
  valueOf (slot 0) (runScript Xsh.Proofs.Core.Requires.Witness.returnMachine 90 fullStart
    [.functionDef .proc echoName [.mk echoParam (some (.result (.named levelName))) none false] none none
       [.expression (ident echoParam)] none false,
     letOf (slot 0) (.call (ident echoName) [argument (.require value none)])])

/-- As the verification binary prints `blue mix err ok 7` for `show(.Blue, 1)`,
`show(k: 2, c: .Mix(3))`, `take("x".require())` and `take(r: 7.require())`:
a parameter's annotation is the expected type of the argument that fills it,
positional or named. -/
theorem argument_witness :
    variantShape (valueOf (slot 0) argumentRun) = some ("Warn", []) ∧
    variantShape (valueOf (slot 1) argumentRun) = some ("Info", []) ∧
    (requireArgument .positional (.literal (.string "x"))).bind Xsh.Proofs.Core.Requires.Witness.schemaVariant =
      some "schema" ∧
    (requireArgument (.named echoParam) (int "7")).bind Xsh.Proofs.Core.Requires.Witness.okInt = some 7 := by
  decide +kernel

/-- Arguments that ignored the parameters' annotations would leave the
variant untargeted and the call would not complete. -/
theorem untargeted_argument_rejected :
    valueOf (slot 0) (runScript shapeMachine 90 fullStart
      [declareLevel, .functionDef .proc echoName [.mk echoParam none none false] none none
          [.expression (ident echoParam)] none false,
        letOf (slot 0) (.call (ident echoName) [.positional (dot warnName)])]) = none ∧
    variantShape (valueOf (slot 0) argumentRun) = some ("Warn", []) := by
  decide +kernel

def yes : Xsh.Syntax.Expr := .literal (.bool true)

def block (value : Xsh.Syntax.Expr) : Xsh.Syntax.Expr := .valueBlock [.expression value]

/-- `let a: Level = if true { .Warn } else { .Info }`,
`let b: Shape = match 7 { 7 => .Ring(1), else => .Dot }` and
`let l: List[Level] = [.Info, .Warn]`. -/
def branchScript : List Xsh.Syntax.Statement :=
  [declareLevel, declareShape,
   annotated (slot 0) (.named levelName) (.ifExpr [(yes, block (dot warnName))] (block (dot infoName))),
   annotated (slot 1) (.named shapeName)
     (.matchExpr (int "7") [(.literal (.integer "7"), none, dotCall ringName [int "1"]), (.wildcard, none, dot dotName)]),
   annotated (slot 2) (.list (.named levelName)) (.list [dot infoName, dot warnName])]

def branchRun : Exec witnessWorld := runScript shapeMachine 90 fullStart branchScript

def variantNames : Option (Value Unit Unit Unit) → Option (List String)
  | some (.list items) => items.mapM fun
    | .tagged _ variant _ => some variant
    | _ => none
  | _ => none

/-- As the verification binary prints `true` for a value `if` and a `match`
selecting leading-dot variants under an annotation, and for
`let l: List[Color] = [.Red, .Blue]`: the expected type reaches each branch
and element. -/
theorem branch_witness :
    variantShape (valueOf (slot 0) branchRun) = some ("Warn", []) ∧
    variantShape (valueOf (slot 1) branchRun) = some ("Ring", [1]) ∧
    variantNames (valueOf (slot 2) branchRun) = some ["Info", "Warn"] := by
  decide +kernel

def okName : Xsh.Syntax.RawName := name 315 (by decide)

def okMachine : Machine witnessWorld :=
  { shapeMachine with
    context := { shapeProgram with
      spelling := fun named => if named = okName then "Ok" else shapeProgram.spelling named } }

/-- `proc ok() -> Result[Level] { Ok(.Warn) }` called, as the verification
binary prints `true` for `Ok(.Blue) == Ok(Blue)` from such a function. -/
theorem ok_payload_witness :
    (match valueOf (slot 0) (runScript okMachine 90 fullStart
        [declareLevel,
         .functionDef .proc pickName [] none (some (.result (.named levelName)))
           [.expression (.call (ident okName) [.positional (dot warnName)])] none false,
         letOf (slot 0) (.call (ident pickName) [])]) with
      | some (.ok (.tagged _ variant [])) => some variant
      | _ => none) = some "Warn" := by
  decide +kernel

/-- An expected type that stopped at the `if` would leave its branches'
variants untargeted, and the binding would not complete. -/
theorem branch_stopping_rejected :
    valueOf (slot 0) (runScript shapeMachine 90 fullStart
      [declareLevel, letOf (slot 0) (.ifExpr [(yes, block (dot warnName))] (block (dot infoName)))]) = none ∧
    variantShape (valueOf (slot 0) branchRun) = some ("Warn", []) := by
  decide +kernel

def flagName : Xsh.Syntax.RawName := name 364 (by decide)
def stepName : Xsh.Syntax.RawName := name 365 (by decide)
def fallName : Xsh.Syntax.RawName := name 366 (by decide)

/-- `proc early(c: Bool) -> Level { if c { return .Warn } for i in [1] { return .Info } .Warn }`,
`proc picked() -> Shape { match 7 { 7 => { return .Ring(2) } else => {} } .Dot }`
and `proc fall(c: Level = .Info) -> Level { c }`, called as `early(true)`,
`early(false)`, `picked()` and `fall()`. -/
def nestedScript : List Xsh.Syntax.Statement :=
  [declareLevel, declareShape,
   .functionDef .proc pickName [.mk flagName none none false] none (some (.named levelName))
     [.ifStmt [(ident flagName, [.returnValue (some (dot warnName))])] none,
      .forStmt (.name stepName) (.list [int "1"]) [.returnValue (some (dot infoName))],
      .expression (dot warnName)] none false,
   .functionDef .proc pickRing [] none (some (.named shapeName))
     [.matchStmt (int "7") [(.literal (.integer "7"), none, [.returnValue (some (dotCall ringName [int "2"]))]),
        (.wildcard, none, [])],
      .expression (dot dotName)] none false,
   .functionDef .proc fallName [.mk flagName (some (.named levelName)) (some (dot infoName)) false] none
     (some (.named levelName)) [.expression (ident flagName)] none false,
   letOf (slot 0) (.call (ident pickName) [.positional yes]),
   letOf (slot 1) (.call (ident pickName) [.positional (.literal (.bool false))]),
   letOf (slot 2) (.call (ident pickRing) []),
   letOf (slot 3) (.call (ident fallName) [])]

def nestedRun : Exec witnessWorld := runScript shapeMachine 120 fullStart nestedScript

/-- As the verification binary prints `true` for `f(true) == Red`,
`g(1) == Red` and `m(1) == Red`, whose `return .Red` sits in an `if`, a `for`
and a `match` arm, and `true` for `d()` and `k() == Green` with defaults
`.Red` and `.Green`: a nested `return` takes the declared result and a
default its parameter's annotation. -/
theorem nested_return_witness :
    variantShape (valueOf (slot 0) nestedRun) = some ("Warn", []) ∧
    variantShape (valueOf (slot 1) nestedRun) = some ("Info", []) ∧
    variantShape (valueOf (slot 2) nestedRun) = some ("Ring", [2]) ∧
    variantShape (valueOf (slot 3) nestedRun) = some ("Info", []) := by
  decide +kernel

/-- A default without its parameter's annotation leaves the variant
untargeted, and the call does not complete. -/
theorem untargeted_default_rejected :
    valueOf (slot 0) (runScript shapeMachine 90 fullStart
      [declareLevel, .functionDef .proc fallName [.mk flagName none (some (dot infoName)) false] none none
          [.expression (ident flagName)] none false,
        letOf (slot 0) (.call (ident fallName) [])]) = none ∧
    variantShape (valueOf (slot 3) nestedRun) = some ("Info", []) := by
  decide +kernel

def keyName : Xsh.Syntax.RawName := name 367 (by decide)
def entryName : Xsh.Syntax.RawName := name 368 (by decide)

/-- The variant names a map or record value holds, in order. -/
def entryVariants : Option (Value Unit Unit Unit) → Option (List String)
  | some (.map entries) => entries.mapM fun
    | (_, .tagged _ variant _) => some variant
    | _ => none
  | some (.record fields) => fields.mapM fun
    | (_, .tagged _ variant _) => some variant
    | _ => none
  | _ => none

/-- `let k = 2`, `let m: Map[Int, Level] = { [1]: .Warn, [k]: .Info }` and
`let n: Map[Str, Level] = { a: .Info }`. -/
def mapRun : Exec witnessWorld := runScript shapeMachine 90 fullStart
  [declareLevel, letOf keyName (int "2"),
   annotated (slot 0) (.map anyType (.named levelName))
     (.record [.computed (int "1") (dot warnName), .computed (ident keyName) (dot infoName)]),
   annotated (slot 1) (.map anyType (.named levelName)) (.record [.named entryName (dot infoName)])]

/-- As the verification binary prints `true` for `m[1] == Green` and
`m[2] == Red` with `let m: Map[Int, Color] = { [1]: .Green, [k]: .Red }`, and
for `m["a"] == Green` with `let m: Map[Str, Color] = { a: .Green }`: a
brace literal's named and computed values take the map's value type. -/
theorem map_value_witness :
    entryVariants (valueOf (slot 0) mapRun) = some ["Warn", "Info"] ∧
    entryVariants (valueOf (slot 1) mapRun) = some ["Info"] := by
  decide +kernel

/-- Without the map annotation the values are untargeted and the binding does
not complete. -/
theorem untargeted_map_value_rejected :
    valueOf (slot 0) (runScript shapeMachine 90 fullStart
      [declareLevel, letOf (slot 0) (.record [.named entryName (dot warnName)])]) = none ∧
    entryVariants (valueOf (slot 1) mapRun) = some ["Info"] := by
  decide +kernel

def errCtor : Xsh.Syntax.RawName := name 378 (by decide)
def failingName : Xsh.Syntax.RawName := name 379 (by decide)

/-- The constructor machine, with `Ok` and `Err` spelled. -/
def familyMachine : Machine witnessWorld :=
  { Xsh.Proofs.Core.Constructors.Witness.constructorMachine with
    context := { Xsh.Proofs.Core.Constructors.Witness.constructorProgram with
      spelling := fun named =>
        if named = okName then "Ok" else if named = errCtor then "Err"
        else Xsh.Proofs.Core.Constructors.Witness.constructorProgram.spelling named } }

def fetchType : Xsh.Syntax.TypeExpr := .named Xsh.Proofs.Core.Constructors.Witness.fetchName

/-- The message of the error an `Err` holds. -/
def errMessage : Option (Value Unit Unit Unit) → Option String
  | some (.err (.error data)) => some data.message
  | _ => none

/-- `let r: Result[Int, Fetch] = Err(.Usage())`, and
`proc fail_usage() -> Result[Int, Fetch] { return Err(.Usage()) }` called. -/
def familyRun : Exec witnessWorld := runScript familyMachine 90 fullStart
  [Xsh.Proofs.Core.Constructors.Witness.declareFetch,
   .functionDef .proc failingName [] none (some (.resultWith anyType fetchType))
     [.returnValue (some (.call (ident errCtor) [.positional (dotCall Xsh.Proofs.Core.Constructors.Witness.usageName [])]))]
     none false,
   annotated (slot 0) (.resultWith anyType fetchType)
     (.call (ident errCtor) [.positional (dotCall Xsh.Proofs.Core.Constructors.Witness.usageName [])]),
   letOf (slot 1) (.call (ident failingName) [])]

/-- As the verification binary prints `Fetch.Offline` for
`let r: Result[Int, Fetch] = Err(.Offline())` and for a function declared
`-> Result[Int, Fetch]` that returns `Err(.Timeout())` or fails with
`fail .Offline()`: the written error type is the expected type of an `Err`
payload, and a leading-dot name there is the family's constructor. -/
theorem error_variant_witness :
    errMessage (Xsh.Proofs.Core.Constructors.Witness.valueOf (slot 0) familyRun) = some "Fetch.Usage" ∧
    errMessage (Xsh.Proofs.Core.Constructors.Witness.valueOf (slot 1) familyRun) = some "Fetch.Usage" := by
  decide +kernel

/-- Under `Result[Int]`, whose error type is the broad `Error`, the variant
is untargeted (`check.inferred-variant`) and the binding does not complete. -/
theorem broad_error_variant_rejected :
    Xsh.Proofs.Core.Constructors.Witness.valueOf (slot 0) (runScript familyMachine 90 fullStart
      [Xsh.Proofs.Core.Constructors.Witness.declareFetch,
       annotated (slot 0) (.result anyType)
         (.call (ident errCtor) [.positional (dotCall Xsh.Proofs.Core.Constructors.Witness.usageName [])])]) = none := by
  decide +kernel

def holderName : Xsh.Syntax.RawName := name 380 (by decide)
def cField : Xsh.Syntax.RawName := name 381 (by decide)
def nField : Xsh.Syntax.RawName := name 382 (by decide)

/-- `type Holder = { c: Level, n: Int }`. -/
def declareHolder : Xsh.Syntax.Statement :=
  .typeDef holderName [] (.recordSchema [.mk cField (.named levelName) none, .mk nField anyType none]) false none

def holderMachine : Machine witnessWorld :=
  { shapeMachine with
    context := { shapeProgram with
      spelling := fun named =>
        if named = holderName then "Holder" else if named = cField then "c" else if named = nField then "n"
        else shapeProgram.spelling named }
    declarations := shapeMachine.declarations ++ [declareHolder] }

/-- The variant a record's `c` field holds. -/
def fieldVariant : Option (Value Unit Unit Unit) → Option String
  | some (.record fields) =>
    match fields.lookup "c" with
    | some (.tagged _ variant _) => some variant
    | _ => none
  | _ => none

/-- As the verification binary prints `true true` for
`let p: P = { c: .Red, n: 1 }` and `let q: P = { n: 2, c: .Green }` with
`type P = { c: Color, n: Int }`: a named field's value takes its schema
field's type, whatever order the fields are written in. -/
theorem schema_field_witness :
    fieldVariant (valueOf (slot 0) (runScript holderMachine 90 fullStart
      [declareLevel, declareHolder,
       annotated (slot 0) (.named holderName) (.record [.named cField (dot warnName), .named nField (int "1")])])) =
      some "Warn" ∧
    fieldVariant (valueOf (slot 0) (runScript holderMachine 90 fullStart
      [declareLevel, declareHolder,
       annotated (slot 0) (.named holderName) (.record [.named nField (int "2"), .named cField (dot infoName)])])) =
      some "Info" := by
  decide +kernel

/-- Without the schema the field's variant is untargeted and the binding does
not complete. -/
theorem untargeted_field_rejected :
    valueOf (slot 0) (runScript holderMachine 90 fullStart
      [declareLevel, letOf (slot 0) (.record [.named cField (dot warnName), .named nField (int "1")])]) = none := by
  decide +kernel

def truth : Option (Value Unit Unit Unit) → Option Bool
  | some (.scalar (.boolean value)) => some value
  | _ => none

/-- `let a: Level = .Warn`, `let o: Level? = null`, then `a == .Warn`,
`a != .Info`, `.Warn == a` and `o == .Warn`. -/
def equalityRun : Exec witnessWorld := runScript shapeMachine 90 fullStart
  [declareLevel, annotated (slot 0) (.named levelName) (dot warnName),
   annotated (slot 1) (.optional (.named levelName)) (.literal .null),
   letOf (slot 2) (.binary .eq (ident (slot 0)) (dot warnName)),
   letOf (slot 3) (.binary .ne (ident (slot 0)) (dot infoName)),
   letOf (slot 4) (.binary .eq (dot warnName) (ident (slot 0))),
   letOf (name 383 (by decide)) (.binary .eq (ident (slot 1)) (dot warnName))]

/-- As the verification binary prints `true true true false` for
`a == .Red`, `a != .Green`, `.Red == a` and `o == .Red` with `a = Red` and
`o: Color? = null`: the other operand of `==` and `!=` is the expected type
of a leading-dot variant, on either side, also inside another expression. -/
theorem equality_variant_witness :
    truth (valueOf (slot 2) equalityRun) = some true ∧
    truth (valueOf (slot 3) equalityRun) = some true ∧
    truth (valueOf (slot 4) equalityRun) = some true ∧
    truth (valueOf (name 383 (by decide)) equalityRun) = some false ∧
    (match valueOf (slot 0) (runScript shapeMachine 90 fullStart
        [declareLevel, annotated (slot 1) (.named levelName) (dot warnName),
         letOf (slot 0) (.list [.binary .eq (ident (slot 1)) (dot warnName)])]) with
      | some (.list [.scalar (.boolean true)]) => true
      | _ => false) = true := by
  decide +kernel

/-- Against a value that is no enum's, a leading-dot name has no expected
type and the comparison does not evaluate. -/
theorem untargeted_equality_rejected :
    valueOf (slot 0) (runScript shapeMachine 90 fullStart
      [declareLevel, letOf (slot 0) (.binary .eq (int "7") (dot warnName))]) = none := by
  decide +kernel

def outerName : Xsh.Syntax.RawName := name 395 (by decide)
def innerField : Xsh.Syntax.RawName := name 396 (by decide)

/-- `type Outer = { inner: Holder, n: Int }`. -/
def declareOuter : Xsh.Syntax.Statement :=
  .typeDef outerName [] (.recordSchema [.mk innerField (.named holderName) none, .mk nField anyType none]) false none

def outerMachine : Machine witnessWorld :=
  { holderMachine with
    context := { holderMachine.context with
      spelling := fun named =>
        if named = outerName then "Outer" else if named = innerField then "inner"
        else holderMachine.context.spelling named }
    declarations := holderMachine.declarations ++ [declareOuter] }

/-- As the verification binary prints `true true` for
`let o: Outer = { inner: { c: .Red }, n: 1 }` and
`let l: List[Inner] = [{ c: .Green }, { c: .Red }]`: schema field types reach
nested records and the elements of a list. -/
theorem nested_schema_witness :
    (match valueOf (slot 0) (runScript outerMachine 90 fullStart
        [declareLevel, declareHolder, declareOuter,
         annotated (slot 0) (.named outerName)
           (.record [.named innerField (.record [.named cField (dot warnName)]), .named nField (int "1")])]) with
      | some (.record fields) => fieldVariant (fields.lookup "inner")
      | _ => none) = some "Warn" ∧
    (match valueOf (slot 0) (runScript outerMachine 90 fullStart
        [declareLevel, declareHolder,
         annotated (slot 0) (.list (.named holderName))
           (.list [.record [.named cField (dot infoName)], .record [.named cField (dot warnName)]])]) with
      | some (.list [first, second]) => (fieldVariant (some first)).bind fun one =>
          (fieldVariant (some second)).map fun two => (one, two)
      | _ => none) = some ("Info", "Warn") := by
  decide +kernel

/-- `proc pick(c: Bool) -> Level { let x = if c { return .Warn } else { 1 }; return .Info }`
called as `pick(true)` and `pick(false)`, and
`proc ring() -> Level { let y = { return .Info }; return .Warn }` called. -/
def returnInRun : Exec witnessWorld := runScript shapeMachine 120 fullStart
  [declareLevel,
   .functionDef .proc pickName [.mk flagName none none false] none (some (.named levelName))
     [letOf stepName (.ifExpr [(ident flagName, .valueBlock [.returnValue (some (dot warnName))])] (block (int "1"))),
      .returnValue (some (dot infoName))] none false,
   .functionDef .proc pickRing [] none (some (.named levelName))
     [letOf stepName (.valueBlock [.returnValue (some (dot infoName))]), .returnValue (some (dot warnName))] none false,
   letOf (slot 0) (.call (ident pickName) [.positional yes]),
   letOf (slot 1) (.call (ident pickName) [.positional (.literal (.bool false))]),
   letOf (slot 2) (.call (ident pickRing) [])]

/-- As the verification binary prints `true true true` for the same shapes
over its own enum: a `return` inside a binding's `if` branch or value block
takes the declared result. -/
theorem return_in_expression_witness :
    variantShape (valueOf (slot 0) returnInRun) = some ("Warn", []) ∧
    variantShape (valueOf (slot 1) returnInRun) = some ("Info", []) ∧
    variantShape (valueOf (slot 2) returnInRun) = some ("Info", []) := by
  decide +kernel

def holderFn : Xsh.Syntax.RawName := name 397 (by decide)
def holderReturn : Xsh.Syntax.RawName := name 398 (by decide)
def holderParam : Xsh.Syntax.RawName := name 399 (by decide)
def holderTake : Xsh.Syntax.RawName := name 400 (by decide)

/-- `proc make() -> Holder { { c: .Warn } }`,
`proc back() -> Holder { return { c: .Info } }` and
`proc take(p: Holder) -> Holder { p }` called as `take({ c: .Warn })`. -/
def schemaCallRun : Exec witnessWorld := runScript holderMachine 90 fullStart
  [declareLevel, declareHolder,
   .functionDef .proc holderFn [] none (some (.named holderName))
     [.expression (.record [.named cField (dot warnName)])] none false,
   .functionDef .proc holderReturn [] none (some (.named holderName))
     [.returnValue (some (.record [.named cField (dot infoName)]))] none false,
   .functionDef .proc holderTake [.mk holderParam (some (.named holderName)) none false] none (some (.named holderName))
     [.expression (ident holderParam)] none false,
   letOf (slot 0) (.call (ident holderFn) []),
   letOf (slot 1) (.call (ident holderReturn) []),
   letOf (slot 2) (.call (ident holderTake) [.positional (.record [.named cField (dot warnName)])])]

/-- As the verification binary prints `true true true` for a record tail and a
returned record under `-> P`, and a record argument for a `P` parameter:
schema field types reach a declared result's tail and top-level `return`,
and the arguments of annotated parameters. -/
theorem schema_call_witness :
    fieldVariant (valueOf (slot 0) schemaCallRun) = some "Warn" ∧
    fieldVariant (valueOf (slot 1) schemaCallRun) = some "Info" ∧
    fieldVariant (valueOf (slot 2) schemaCallRun) = some "Warn" := by
  decide +kernel

def nestedHolder : Xsh.Syntax.RawName := name 411 (by decide)
def blockHolder : Xsh.Syntax.RawName := name 412 (by decide)

/-- `proc f(b: Bool) -> Holder { if b { return { c: .Warn } }; return { c:
.Info } }` called as `f(true)`, and `proc g() -> Holder { let y = { return {
c: .Warn } }; { c: .Info } }` called. -/
def nestedSchemaRun : Exec witnessWorld := runScript holderMachine 120 fullStart
  [declareLevel, declareHolder,
   .functionDef .proc nestedHolder [.mk flagName none none false] none (some (.named holderName))
     [.ifStmt [(ident flagName, [.returnValue (some (.record [.named cField (dot warnName)]))])] none,
      .returnValue (some (.record [.named cField (dot infoName)]))] none false,
   .functionDef .proc blockHolder [] none (some (.named holderName))
     [letOf stepName (.valueBlock [.returnValue (some (.record [.named cField (dot warnName)]))]),
      .expression (.record [.named cField (dot infoName)])] none false,
   letOf (slot 0) (.call (ident nestedHolder) [.positional yes]),
   letOf (slot 1) (.call (ident blockHolder) [])]

/-- As the verification binary prints `true true` for the same shapes over
its own schema: a `return` inside an `if` statement or a binding's value
block gives its record the declared result's schema. -/
theorem nested_schema_return_witness :
    fieldVariant (valueOf (slot 0) nestedSchemaRun) = some "Warn" ∧
    fieldVariant (valueOf (slot 1) nestedSchemaRun) = some "Warn" := by
  decide +kernel

def badFamily : Xsh.Syntax.RawName := name 401 (by decide)
def paintVariant : Xsh.Syntax.RawName := name 402 (by decide)

/-- `error Bad = Paint(c: Level)`. -/
def declareBad : Xsh.Syntax.Statement := .errorDef badFamily [.mk paintVariant [(cField, .named levelName)] []]

def constructorFieldMachine : Machine witnessWorld :=
  { holderMachine with
    context := { holderMachine.context with
      spelling := fun named =>
        if named = badFamily then "Bad" else if named = paintVariant then "Paint"
        else holderMachine.context.spelling named }
    declarations := holderMachine.declarations ++ [declareBad] }

def constructorFieldRun : Exec witnessWorld := runScript constructorFieldMachine 90 fullStart
  [declareLevel, declareHolder, declareBad,
   letOf (slot 0) (.call (ident holderName) [.named cField (dot warnName), .named nField (int "1")]),
   letOf (slot 1) (.call (ident holderName) [.positional (dot infoName), .positional (int "2")]),
   letOf (slot 2) (.call (.field (ident badFamily) paintVariant) [.named cField (dot warnName)])]

/-- The variant an error's `c` payload field holds. -/
def payloadVariant : Option (Value Unit Unit Unit) → Option String
  | some (.error data) =>
    match data.payload.lookup "c" with
    | some (.tagged _ variant _) => some variant
    | _ => none
  | _ => none

/-- As the verification binary prints `true true Bad.Paint` for
`P(c: .Red, n: 1)`, `P(.Green, 2)` and `Bad.Paint(c: .Green)`: a constructor's
field types are the expected types of the arguments that fill them, named or
positional. -/
theorem constructor_field_witness :
    fieldVariant (valueOf (slot 0) constructorFieldRun) = some "Warn" ∧
    fieldVariant (valueOf (slot 1) constructorFieldRun) = some "Info" ∧
    payloadVariant (valueOf (slot 2) constructorFieldRun) = some "Warn" := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Variants
