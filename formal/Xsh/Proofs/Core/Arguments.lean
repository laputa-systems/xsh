import Xsh.Proofs.Core.Constructors

namespace Xsh.Proofs.Core.Arguments

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

variable {Declaration Identity Source : Type}

section Root

/-- Call arguments (SPEC 6.3, 9.1, 9.2) for every call of a local function,
direct or through `.call(...)`. Arguments evaluate once in written order; "an
`@` splice" supplies a list's items positionally and a record spread
(`...opts`) the record's fields by label. Positional arguments fill
parameters in order and labelled ones bind by name; a parameter takes its
positional argument, else its label, else its default, evaluated after every
supplied argument, and a final rest parameter takes the remaining positional
arguments. The runtime checks the arguments ("their arguments are checked at
runtime"): a label given twice or to a parameter already filled by position,
a label no parameter has, a required parameter left out, and a positional
argument with no parameter left are runtime failures, which `try` does not
capture. -/
theorem call_arguments {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ (spelling : Xsh.Syntax.RawName → String) (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) frame value
        rest,
      threadArguments spelling evaluate frame (.splice value :: rest) =
        (evaluate frame value).bind fun result after =>
          match listValues result with
          | some items =>
            (threadArguments spelling evaluate after rest).bind fun others last =>
              .value (items.map .positional ++ others) last
          | none => .exec .stuck) ∧
    (∀ (spelling : Xsh.Syntax.RawName → String) (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) frame value
        rest,
      threadArguments spelling evaluate frame (.namedSpread value :: rest) =
        (evaluate frame value).bind fun result after =>
          match result.eraseNominal with
          | .record fields =>
            (threadArguments spelling evaluate after rest).bind fun others last =>
              .value (fields.map (fun field => .named field.1 field.2) ++ others) last
          | _ => .exec .stuck) ∧
    (∀ (spelling : Xsh.Syntax.RawName → String) (supplied : List (Supplied Declaration Identity Source)) parameters
        label,
      repeatedLabel (supplied.filterMap Supplied.namedValue) = some label →
        placeCall spelling parameters supplied = .error (.duplicate label)) ∧
    (∀ (spelling : Xsh.Syntax.RawName → String) name annotation default rest (value : Value Declaration Identity Source)
        values named found,
      named.lookup (spelling name) = some found →
        placeParameters spelling (.mk name annotation default false :: rest) (value :: values) named =
          .error (.duplicate (spelling name))) ∧
    (∀ (spelling : Xsh.Syntax.RawName → String) label (value : Value Declaration Identity Source) named,
      placeParameters spelling [] [] ((label, value) :: named) = .error (.unknownName label)) ∧
    (∀ (spelling : Xsh.Syntax.RawName → String) name annotation rest (named : List (String × Value Declaration Identity Source)),
      named.lookup (spelling name) = none →
        placeParameters spelling (.mk name annotation none false :: rest) [] named = .error (.missing (spelling name))) ∧
    (∀ (spelling : Xsh.Syntax.RawName → String) (value : Value Declaration Identity Source) values named,
      placeParameters spelling [] (value :: values) named = .error .tooMany) ∧
    (∀ (spelling : Xsh.Syntax.RawName → String) name annotation default rest (found : Value Declaration Identity Source)
        named,
      named.lookup (spelling name) = some found →
        placeParameters spelling (.mk name annotation default false :: rest) [] named =
          match placeParameters spelling rest [] (named.filter fun entry => entry.1 != spelling name) with
          | .ok slots => .ok ((name, .given found) :: slots)
          | .error fault => .error fault) ∧
    (∀ (evaluate : Frame world → Xsh.Syntax.Expr → Valued world)
        (callBlock : Frame world → Scope Declaration Identity Source → Environment → Locals Declaration Identity Source →
          List CellId → List Xsh.Syntax.Statement → ReturnKind → Valued world)
        frame cell callable defining captured writable parameters body arguments supplied after fault,
      threadArguments machine.context.spelling evaluate frame arguments = .value supplied after →
        placeCall machine.context.spelling parameters supplied = .error fault →
          invokeWith machine evaluate callBlock frame (.mk cell callable defining captured writable parameters body)
              arguments =
            .exec (.failed (.runtime (machine.argumentError fault (fixedCount parameters)
              (supplied.filterMap Supplied.positionalValue).length)) after)) ∧
    (∀ (error : Value Declaration Identity Source) (frame : Frame world),
      Xsh.Semantics.CoreControl.capture (.exec (.failed (.runtime error) frame)) =
        .exec (.failed (.runtime error) frame)) := by
  refine ⟨fun _ _ _ _ _ => rfl, fun _ _ _ _ _ => rfl, ?_, ?_, fun _ _ _ _ => rfl, ?_, fun _ _ _ _ => rfl, ?_, ?_,
    fun _ _ => rfl⟩
  · intro spelling supplied parameters label repeated
    simp only [placeCall, repeated]
  · intro spelling name annotation default rest value values named found present
    simp only [placeParameters, present]
  · intro spelling name annotation rest named absent
    simp only [placeParameters, absent]
  · intro spelling name annotation default rest found named present
    cases default <;> (simp only [placeParameters, present]; rfl)
  · intro evaluate callBlock frame cell callable defining captured writable parameters body arguments supplied after
      fault threaded placed
    simp only [invokeWith, threaded, Outcome.bind, placed]

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Threading.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness

def subName : Xsh.Syntax.RawName := name 150 (by decide)
def totalFn : Xsh.Syntax.RawName := name 151 (by decide)
def firstParam : Xsh.Syntax.RawName := name 152 (by decide)
def restParam : Xsh.Syntax.RawName := name 153 (by decide)
def itemName : Xsh.Syntax.RawName := name 154 (by decide)
def sumLocal : Xsh.Syntax.RawName := name 155 (by decide)
def pairLocal : Xsh.Syntax.RawName := name 156 (by decide)
def moreLocal : Xsh.Syntax.RawName := name 157 (by decide)
def otherLabel : Xsh.Syntax.RawName := name 158 (by decide)
def results : List Xsh.Syntax.RawName := [name 160 (by decide), name 161 (by decide), name 162 (by decide),
  name 163 (by decide), name 164 (by decide), name 165 (by decide)]

/-- `proc sub(left: Int, right: Int) -> Int { left - right }`. -/
def declareSub : Xsh.Syntax.Statement :=
  .functionDef .proc subName [.mk leftField none none false, .mk rightField none none false] none none
    [.expression (.binary .sub (ident leftField) (ident rightField))] none false

/-- `proc total(first: Int, ...rest: List[Int]) -> Int { var sum = first; for item in rest { sum += item }; sum }`. -/
def declareTotal : Xsh.Syntax.Statement :=
  .functionDef .proc totalFn [.mk firstParam none none false, .mk restParam none none true] none none
    [.binding .var (.name sumLocal) none (ident firstParam),
      .forStmt (.name itemName) (ident restParam) [bump sumLocal (ident itemName)],
      .expression (ident sumLocal)] none false

def argumentsMachine : Machine witnessWorld := { constructorMachine with callableCell := fun _ => topCell subName }

def named (label : Xsh.Syntax.RawName) (value : Xsh.Syntax.Expr) : Xsh.Syntax.CallArgument := .named label value

def nth (index : Nat) : Xsh.Syntax.RawName := results.getD index (name 169 (by decide))

/-- `sub(right: 2, left: 7)`, `sub(...pair)`, `sub(7, right: 2)`,
`total(1, @more)`, `total(1, 2, @more, 8)` and `[sub][0].call(7, 2)?` with
`pair = {left: 8, right: 1}` and `more = [2, 7]`; the verification binary
prints `5 7 5 10 20` and `5`. -/
def directCalls : List Xsh.Syntax.Statement :=
  [declareSub, declareTotal,
    letOf pairLocal (.record [.named leftField (int "8"), .named rightField (int "1")]),
    letOf moreLocal (items ["2", "7"]),
    letOf (nth 0) (.call (ident subName) [named rightField (int "2"), named leftField (int "7")]),
    letOf (nth 1) (.call (ident subName) [.namedSpread (ident pairLocal)]),
    letOf (nth 2) (.call (ident subName) [.positional (int "7"), named rightField (int "2")]),
    letOf (nth 3) (.call (ident totalFn) [.positional (int "1"), .splice (ident moreLocal)]),
    letOf (nth 4) (.call (ident totalFn) [.positional (int "1"), .positional (int "2"), .splice (ident moreLocal),
      .positional (int "8")]),
    letOf (nth 5) (.tryExpr (callOf (.index (.list [ident subName]) (int "0") false) [int "7", int "2"]))]

def runArguments (body : List Xsh.Syntax.Statement) : Exec witnessWorld :=
  runStatements argumentsMachine 90 fullStart body

theorem call_argument_forms :
    [readInt (nth 0) (runArguments directCalls), readInt (nth 1) (runArguments directCalls),
      readInt (nth 2) (runArguments directCalls), readInt (nth 3) (runArguments directCalls),
      readInt (nth 4) (runArguments directCalls), readInt (nth 5) (runArguments directCalls)] =
      [some 5, some 7, some 5, some 10, some 20, some 5] := by
  decide +kernel

/-- `[sub][0].call(1, 2, 7)?` inside `try`: the arity failure is a runtime
failure that escapes the `try`, as the binary's
`expected 2 arguments, found 3` with exit 3 shows. -/
def arityRun : Exec witnessWorld :=
  runArguments [declareSub,
    letOf rName (.capture [.expression (.tryExpr (callOf (.index (.list [ident subName]) (int "0") false)
      [int "1", int "2", int "7"]))])]

def fewerRun : Exec witnessWorld :=
  runArguments [declareSub, letOf rName (.tryExpr (callOf (.index (.list [ident subName]) (int "0") false) [int "1"]))]

theorem arity_checked_at_runtime :
    runtimeFailure arityRun = some "expected 2 arguments, found 3" ∧
      runtimeFailure fewerRun = some "expected 2 arguments, found 1" := by
  decide +kernel

/-- `[sub][0].call(right: 2, left: 7)?` binds by label, which SPEC 6.3 allows;
Rust does not compile a named argument to a dynamic `.call`. -/
def namedCallRun : Exec witnessWorld :=
  runArguments [declareSub,
    letOf rName (.tryExpr (.call (.field (.index (.list [ident subName]) (int "0") false) callName)
      [named rightField (int "2"), named leftField (int "7")]))]

theorem named_call_value : readInt rName namedCallRun = some 5 := by
  decide +kernel

def subParameters : List Xsh.Syntax.Parameter := [.mk leftField none none false, .mk rightField none none false]

def faultOf : Except ArgumentFault (List (Xsh.Syntax.RawName × Slot Unit Unit Unit)) → Option ArgumentFault
  | .error fault => some fault
  | .ok _ => none

def supplied (arguments : List (Option String × Int)) : List (Supplied Unit Unit Unit) :=
  arguments.map fun entry =>
    match entry.1 with
    | none => .positional (.scalar (.integer ((validateInt? entry.2).getD intOne)))
    | some label => .named label (.scalar (.integer ((validateInt? entry.2).getD intOne)))

def check (arguments : List (Option String × Int)) : Option ArgumentFault :=
  faultOf (placeCall constructorProgram.spelling subParameters (supplied arguments))

/-- Each check fires: `sub(left: 7, right: 2, other: 1)`, `sub(left: 7,
left: 2, right: 1)`, `sub(7, left: 2)`, `sub(left: 7)` and `sub(1, 2, 7)`. -/
theorem argument_checks :
    check [(some "left", 7), (some "right", 2), (some "other", 1)] = some (.unknownName "other") ∧
      check [(some "left", 7), (some "left", 2), (some "right", 1)] = some (.duplicate "left") ∧
      check [(none, 7), (some "left", 2)] = some (.duplicate "left") ∧
      check [(some "left", 7)] = some (.missing "right") ∧
      check [(none, 1), (none, 2), (none, 7)] = some .tooMany ∧
      check [(some "right", 2), (some "left", 7)] = none := by
  decide +kernel

/-- A placement that drops labels no parameter has. -/
def ignoringUnknown (arguments : List (Option String × Int)) : Option ArgumentFault :=
  faultOf (placeCall constructorProgram.spelling subParameters
    ((supplied arguments).filter fun entry => match entry with
      | .named label _ => label = "left" || label = "right"
      | .positional _ => true))

/-- A placement that keeps the first of two equal labels. -/
def firstLabelWins (arguments : List (Option String × Int)) : Option ArgumentFault :=
  faultOf (placeParameters constructorProgram.spelling subParameters
    ((supplied arguments).filterMap Supplied.positionalValue) ((supplied arguments).filterMap Supplied.namedValue))

/-- A placement that gives a missing parameter `null`. -/
def nullForMissing (arguments : List (Option String × Int)) : Option ArgumentFault :=
  faultOf (placeCall constructorProgram.spelling
    [.mk leftField none (some (.literal .null)) false, .mk rightField none (some (.literal .null)) false]
    (supplied arguments))

/-- A placement that drops positional arguments with no parameter left. -/
def droppingExtra (arguments : List (Option String × Int)) : Option ArgumentFault :=
  faultOf (placeCall constructorProgram.spelling subParameters
    ((supplied arguments).take 2))

theorem argument_check_mutants_rejected :
    ignoringUnknown [(some "left", 7), (some "right", 2), (some "other", 1)] = none ∧
      firstLabelWins [(some "left", 7), (some "left", 2), (some "right", 1)] = none ∧
      nullForMissing [(some "left", 7)] = none ∧
      droppingExtra [(none, 1), (none, 2), (none, 7)] = none := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Arguments
