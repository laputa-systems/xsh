import Xsh.Semantics.CoreControl
import Xsh.Proofs.Core.EvalStatements

namespace Xsh.Proofs.Core.ControlWitness

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Semantics.CoreControl
open Xsh.Proofs.Core.Env.Witness

def name (index : Nat) (bounded : index < 4294967296) : Xsh.Syntax.RawName := ⟨⟨index, bounded⟩⟩

def xName : Xsh.Syntax.RawName := name 30 (by decide)
def iName : Xsh.Syntax.RawName := name 31 (by decide)
def jName : Xsh.Syntax.RawName := name 32 (by decide)
def totalName : Xsh.Syntax.RawName := name 33 (by decide)
def seenName : Xsh.Syntax.RawName := name 34 (by decide)
def gName : Xsh.Syntax.RawName := name 35 (by decide)
def aName : Xsh.Syntax.RawName := name 36 (by decide)
def bName : Xsh.Syntax.RawName := name 37 (by decide)

def int (lexeme : String) : Xsh.Syntax.Expr := .literal (.integer lexeme)
def ident (target : Xsh.Syntax.RawName) : Xsh.Syntax.Expr := .ident target

def program : Context Unit Unit Unit :=
  { Xsh.Proofs.Core.EvalStatements.Witness.context with
    functions := fun called =>
      if called = gName then
        some (⟨emptyLexicalEnvironment, []⟩, [],
          .block [.forStmt (.name iName) (.list [int "1", int "2"]) [.returnValue (some (ident iName))], .expression (int "7")])
      else none }

/-- The checker's resolution in these programs: one binder per name, a cell
per scope depth, all at type `Any`. -/
def resolution : Oracles witnessWorld where
  spelling := fun _ => ""
  evaluate := fun _ store _ => .error (.runtime (.scalar .null), store)
  selector := fun _ store _ _ => .error (.runtime (.scalar .null), store)
  declaration := fun environment declared =>
    some (⟨declared.word⟩, ⟨environment.names.length, ⟨declared.word⟩⟩, .primitive .any)
  argv := fun _ => none

def machine : Machine witnessWorld where
  context := program
  decisions := witnessDecisions
  raises := Xsh.Proofs.Core.Writes.Witness.raises
  operate := operate program Xsh.Proofs.Core.Writes.Witness.pair
  resolution := resolution
  assertion := fun _ => .scalar (.string "assertion")

def start : Frame witnessWorld := ⟨⟨emptyLexicalEnvironment, []⟩, fun _ => none, [], [], none⟩

def run (body : List Xsh.Syntax.Statement) : Exec witnessWorld := runStatements machine 40 start body

def readInt (target : Xsh.Syntax.RawName) : Exec witnessWorld → Option Int
  | .normal frame =>
    match frame.environment.cellOf target with
    | .ok cell =>
      (frame.store cell).bind fun found =>
        match found.contents.value with
        | .scalar (.integer value) => some value.val
        | _ => none
    | .error _ => none
  | _ => none

def resolves (target : Xsh.Syntax.RawName) : Exec witnessWorld → Bool
  | .normal frame =>
    match frame.environment.cellOf target with
    | .ok _ => true
    | .error _ => false
  | _ => false

def returnedInt : Exec witnessWorld → Option Int
  | .returned (.scalar (.integer value)) _ => some value.val
  | _ => none

def letX (lexeme : String) : Xsh.Syntax.Statement := .binding .letBinding (.name xName) none (int lexeme)

def shadowed : List Xsh.Syntax.Statement := [letX "1", .expression (.valueBlock [letX "2"])]

def varOf (target : Xsh.Syntax.RawName) (lexeme : String) : Xsh.Syntax.Statement :=
  .binding .var (.name target) none (int lexeme)

def bump (target : Xsh.Syntax.RawName) (by_ : Xsh.Syntax.Expr) : Xsh.Syntax.Statement :=
  .assign (.name target) .add by_

def items (lexemes : List String) : Xsh.Syntax.Expr := .list (lexemes.map int)

def loopOnly : List Xsh.Syntax.Statement := [.forStmt (.name iName) (items ["1", "2"]) []]

def skipping : List Xsh.Syntax.Statement := [
  varOf totalName "0", varOf seenName "0",
  .forStmt (.name iName) (items ["1", "2", "7"]) [
    bump seenName (int "1"),
    .ifStmt [(.binary .eq (ident iName) (int "2"), [.continueStmt])] none,
    bump totalName (ident iName)]]

def nested : List Xsh.Syntax.Statement := [
  varOf totalName "0",
  .forStmt (.name iName) (items ["1", "2"]) [
    .forStmt (.name jName) (items ["7", "8"]) [.breakStmt none],
    bump totalName (int "1")]]

def returning : List Xsh.Syntax.Statement := [
  .forStmt (.name iName) (items ["1", "2"]) [.returnValue (some (ident iName))],
  .returnValue (some (int "7"))]

def matching : List Xsh.Syntax.Statement := [
  varOf totalName "0",
  .matchStmt (items ["7", "8"]) [
    (.list [.literal (.integer "1"), .binding aName] none, none, [bump totalName (int "1")]),
    (.list [.binding aName, .binding bName] none, none, [bump totalName (.binary .add (ident aName) (ident bName))])]]

def ifLet : List Xsh.Syntax.Statement := [
  varOf totalName "0",
  .ifStmt [(.patternCondition (.literal .null) (.binding aName), [bump totalName (int "1")]),
    (.patternCondition (int "7") (.binding aName), [bump totalName (ident aName)])] none]

def calling : List Xsh.Syntax.Statement := [varOf totalName "0", bump totalName (.call (ident gName) [])]

/-- A `for` that leaves each iteration's bindings in place. -/
def leakyFor (frame : Frame witnessWorld) (target : Xsh.Syntax.BindingTarget) :
    List (Value Unit Unit Unit) → List Xsh.Syntax.Statement → Exec witnessWorld
  | [], _ => .normal frame
  | item :: rest, body =>
    ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
        runStatements machine 30 bound body).iterate fun next => leakyFor next target rest body

/-- A `for` whose `break` also leaves the loop around it. -/
def breakingFor (frame : Frame witnessWorld) (target : Xsh.Syntax.BindingTarget) :
    List (Value Unit Unit Unit) → List Xsh.Syntax.Statement → Exec witnessWorld
  | [], _ => .normal frame
  | item :: rest, body =>
    match withinScope frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
        runStatements machine 30 bound body) with
    | .normal next | .continued next => breakingFor next target rest body
    | other => other

/-- A `for` whose `return` only ends the loop. -/
def breakingReturn (frame : Frame witnessWorld) (target : Xsh.Syntax.BindingTarget) :
    List (Value Unit Unit Unit) → List Xsh.Syntax.Statement → Exec witnessWorld
  | [], _ => .normal frame
  | item :: rest, body =>
    match withinScope frame ((bindTarget machine frame.enter .immutable target item).andThen fun bound =>
        runStatements machine 30 bound body) with
    | .normal next | .continued next => breakingReturn next target rest body
    | .broke next | .returned _ next => .normal next
    | other => other

def values (lexemes : List String) : List (Value Unit Unit Unit) :=
  lexemes.filterMap fun lexeme => (Xsh.Proofs.Core.EvalStatements.Witness.integerLexeme lexeme).map integer

def afterVar (target : Xsh.Syntax.RawName) (lexeme : String) : Frame witnessWorld :=
  match runStatements machine 10 start [varOf target lexeme] with
  | .normal frame => frame
  | _ => start

def nestedMutant : Exec witnessWorld :=
  let outer := afterVar totalName "0"
  let rec iterations : List (Value Unit Unit Unit) → Frame witnessWorld → Exec witnessWorld
    | [], frame => .normal frame
    | item :: rest, frame =>
      (withinScope frame ((bindTarget machine frame.enter .immutable (.name iName) item).andThen fun bound =>
          (breakingFor bound (.name jName) (values ["7", "8"]) [.breakStmt none]).andThen fun inner =>
            runStatements machine 30 inner [bump totalName (int "1")])).iterate fun next => iterations rest next
  iterations (values ["1", "2"]) outer

def returnMutant : Exec witnessWorld :=
  (breakingReturn start (.name iName) (values ["1", "2"]) [.returnValue (some (ident iName))]).andThen fun next =>
    runStatements machine 30 next [.returnValue (some (int "7"))]

example : readInt xName (run shadowed) = some 1 := by decide +kernel
example : resolves iName (run loopOnly) = false := by decide +kernel
example : resolves iName (leakyFor start (.name iName) (values ["1", "2"]) []) = true := by decide +kernel
example : readInt totalName (run skipping) = some 8 ∧ readInt seenName (run skipping) = some 3 := by decide +kernel
example : readInt totalName (run nested) = some 2 := by decide +kernel
example : readInt totalName nestedMutant = some 0 := by decide +kernel
example : returnedInt (run returning) = some 1 := by decide +kernel
example : returnedInt returnMutant = some 7 := by decide +kernel
example : readInt totalName (run matching) = some 15 := by decide +kernel
example : readInt totalName (run ifLet) = some 7 := by decide +kernel
example : readInt totalName (run calling) = some 1 := by decide +kernel

end Xsh.Proofs.Core.ControlWitness
