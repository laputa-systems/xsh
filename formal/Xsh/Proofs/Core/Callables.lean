import Xsh.Proofs.Core.Threading

namespace Xsh.Proofs.Core.Callables

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

variable [DecidableEq Declaration]

/-- Callable values (SPEC 9.1, 3.1). Declarations occur only at the top level
of a script or module ("inside a callable body or any block they are
`check.nested-declaration`"), so the machine refuses a function declaration
inside a block or a function body, and every callable value names a top-level
declaration. A name bound to a local function evaluates to a callable value
whose identity is the runtime's for the declaration's cell. `value.call(...)`
evaluates the receiver first, then places its arguments through
`invokeWith`, and runs the declaration the value's identity names through
`callBlock`, which returns
through `returnTo`, so the call writes the captured top-level `var`s through
the caller's frame; "First-class `Proc` values have `.call(...) ->
Result[Any]` and `Pure` values `.call(...) -> Any`": a `Proc` value's call is
the proc's own `Result` or `Ok` of its value, and a `Pure` value's call is its
value; a receiver naming a declared error family's variant is a constructor
instead. Where the identity oracle names each declaration's own cell, a callable
value made from a declaration calls that declaration. -/
theorem callable_values {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ fuel (frame : Frame world) kind name parameters effects result body metadata test caller,
      frame.callerStore = some caller →
        runStatement machine (fuel + 1) frame (.functionDef kind name parameters effects result body metadata test) =
          .stuck) ∧
    (∀ fuel (frame : Frame world) kind name parameters effects result body metadata test,
      (∀ only, frame.environment.names ≠ [only]) →
        runStatement machine (fuel + 1) frame (.functionDef kind name parameters effects result body metadata test) =
          .stuck) ∧
    (∀ fuel (frame : Frame world) name cell callable defining captured writable parameters body,
      frame.locals.lookup name = none →
        localClosure frame.closures frame.environment name =
          some (.mk cell callable defining captured writable parameters body) →
          evalExpr machine (fuel + 1) frame (.ident name) =
            .value (.callable (machine.callableReference cell) callable) frame) ∧
    (∀ fuel (frame : Frame world) receiver method arguments reference kind after cell callable defining captured
        writable parameters body,
      errorConstructor machine frame receiver method = none →
      machine.context.spelling method = "call" →
        evalExpr machine fuel frame receiver = .value (.callable reference kind) after →
          (machine.callableCell reference).bind (fun found => after.closures.lookup found) =
            some (.mk cell callable defining captured writable parameters body) →
            evalExpr machine (fuel + 1) frame (.call (.field receiver method) arguments) =
              (invokeWith machine (evalExpr machine fuel) (callBlock machine fuel) after
                (.mk cell callable defining captured writable parameters body) arguments).bind fun outcome final =>
                .value (callResult callable outcome) final) ∧
    (∀ value : Value Declaration Identity Source,
      callResult .procedure value = wrapOk value ∧ callResult .pure value = value) ∧
    ((∀ cell, machine.callableCell (machine.callableReference cell) = some cell) →
      ∀ (closures : List (CellId × Closure)) cell,
        (machine.callableCell (machine.callableReference cell)).bind (fun found => closures.lookup found) =
          closures.lookup cell) := by
  refine ⟨?_, ?_, ?_, ?_, fun _ => ⟨rfl, rfl⟩, fun roundTrip closures cell => by rw [roundTrip]; rfl⟩
  · intro fuel frame kind name parameters effects result body metadata test caller calling
    simp [runStatement, calling]
  · intro fuel frame kind name parameters effects result body metadata test nested
    simp only [runStatement]
    split
    · rename_i only _ names
      exact absurd names (nested only)
    · rfl
  · intro fuel frame name cell callable defining captured writable parameters body unshadowed found
    simp only [evalExpr, unshadowed, found]
  · intro fuel frame receiver method arguments reference kind after cell callable defining captured writable
      parameters body notError spelled received named
    have notEnv : envFunction machine frame receiver method = none := by
      unfold envFunction
      split
      · split
        · simp [spelled]
        · rfl
      · rfl
    simp only [evalExpr, notError, notEnv, spelled, Outcome.bind, received, Value.eraseNominal, named]

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Threading.Witness

def callName : Xsh.Syntax.RawName := name 95 (by decide)
def fnsName : Xsh.Syntax.RawName := name 96 (by decide)
def firstName : Xsh.Syntax.RawName := name 97 (by decide)
def secondName : Xsh.Syntax.RawName := name 98 (by decide)
def twiceName : Xsh.Syntax.RawName := name 99 (by decide)
def actionName : Xsh.Syntax.RawName := name 100 (by decide)
def doubleName : Xsh.Syntax.RawName := name 101 (by decide)

def callableProgram : Context Unit Unit Unit :=
  { fullProgram with spelling := fun named => if named = callName then "call" else fullProgram.spelling named }

/-- The declaration cell of a top-level function named `declared`. -/
def topCell (declared : Xsh.Syntax.RawName) : Option CellId :=
  (fullMachine.namesAt 0).declaration fullStart.environment declared |>.map fun found => found.2.1

/-- The witness world has one callable identity, so each witness machine's
oracle names the one declaration it calls through values. -/
def callingMachine (declared : Xsh.Syntax.RawName) : Machine witnessWorld :=
  { fullMachine with context := callableProgram, callableCell := fun _ => topCell declared }

def callOf (receiver : Xsh.Syntax.Expr) (arguments : List Xsh.Syntax.Expr) : Xsh.Syntax.Expr :=
  .call (.field receiver callName) (arguments.map .positional)

def letOf (target : Xsh.Syntax.RawName) (value : Xsh.Syntax.Expr) : Xsh.Syntax.Statement :=
  .binding .letBinding (.name target) none value

def firstCall : Xsh.Syntax.Expr := .tryExpr (callOf (.index (ident fnsName) (int "0") false) [])

/-- `var count = 0; proc bump() -> Int { count += 1; count }; let fns = [bump];
let first = fns[0].call()?; let second = fns[0].call()?`. -/
def storedCalls : List Xsh.Syntax.Statement :=
  [varOf countName "0", declareCounter, letOf fnsName (.list [ident tallyName]), letOf firstName firstCall,
    letOf secondName firstCall]

def runCalls (declared : Xsh.Syntax.RawName) (body : List Xsh.Syntax.Statement) : Exec witnessWorld :=
  runStatements (callingMachine declared) 90 fullStart body

/-- Calling the stored `bump` twice gives `1`, then `2`, and leaves
`count = 2`, as the verification binary prints. -/
theorem stored_callable_writes :
    readInt firstName (runCalls tallyName storedCalls) = some 1 ∧
      readInt secondName (runCalls tallyName storedCalls) = some 2 ∧
      readInt countName (runCalls tallyName storedCalls) = some 2 := by
  decide +kernel

/-- `let r = { action: bump }; let first = r.action.call()?`. -/
def recordCall : List Xsh.Syntax.Statement :=
  [varOf countName "0", declareCounter, letOf rName (.record [.named actionName (ident tallyName)]),
    letOf firstName (.tryExpr (callOf (.field (ident rName) actionName) []))]

theorem record_callable_writes :
    readInt firstName (runCalls tallyName recordCall) = some 1 ∧
      readInt countName (runCalls tallyName recordCall) = some 1 := by
  decide +kernel

/-- `proc twice(action) { let _ = action.call(); action.call()? }`. -/
def declareTwice : Xsh.Syntax.Statement :=
  .functionDef .proc twiceName [.mk actionName none none false] none none
    [.binding .letBinding .discard none (callOf (ident actionName) []),
      .expression (.tryExpr (callOf (ident actionName) []))] none false

def passedCall : List Xsh.Syntax.Statement :=
  [varOf countName "0", declareCounter, declareTwice, letOf rName (call twiceName [ident tallyName])]

/-- `bump` passed as an argument and called twice through `.call()` inside
`twice` gives `2`, and its writes to `count` reach the top level. -/
theorem passed_callable_writes :
    readInt rName (runCalls tallyName passedCall) = some 2 ∧
      readInt countName (runCalls tallyName passedCall) = some 2 := by
  decide +kernel

/-- `pure double(n) { n * 2 }; let r = [double][0].call(7)`: a `Pure` value's
call is its value, `14`, not a `Result`. -/
def pureCall : List Xsh.Syntax.Statement :=
  [.functionDef .pure doubleName [.mk nName none none false] none none
      [.expression (.binary .mul (ident nName) (int "2"))] none false,
    letOf rName (callOf (.index (.list [ident doubleName]) (int "0") false) [int "7"])]

theorem pure_callable_value : readInt rName (runCalls doubleName pureCall) = some 14 := by
  decide +kernel

/-- The frame after `var count = 0`, the counter's declaration and
`let fns = [bump]`. -/
def callsFrame : Option (Frame witnessWorld) :=
  match runCalls tallyName (storedCalls.take 3) with
  | .normal frame => some frame
  | _ => none

def secondOf (evaluate : Frame witnessWorld → Valued witnessWorld) : Option (List Int × Int) :=
  callsFrame.bind fun frame =>
    match evaluate frame with
    | .value value after => (intsOf value).bind fun ints => (countIn frame after.store).map fun count => (ints, count)
    | _ => none

def twiceThreaded (frame : Frame witnessWorld) : Valued witnessWorld :=
  (evalExpr (callingMachine tallyName) 60 frame firstCall).bind fun first after =>
    (evalExpr (callingMachine tallyName) 60 after firstCall).bind fun second last =>
      .value (.list [first, second]) last

/-- A callable value that snapshots the store when the value is made: each
call runs against the store before any call. -/
def twiceSnapshot (frame : Frame witnessWorld) : Valued witnessWorld :=
  (evalExpr (callingMachine tallyName) 60 frame firstCall).bind fun first after =>
    (evalExpr (callingMachine tallyName) 60 { after with store := frame.store } firstCall).bind fun second _ =>
      .value (.list [first, second]) { after with store := frame.store }

/-- Through the caller's frame the two calls give `[1, 2]` with `count = 2`;
the snapshot gives `[1, 1]` and leaves `count = 0`. -/
theorem snapshot_call_rejected :
    secondOf twiceThreaded = some ([1, 2], 2) ∧ secondOf twiceSnapshot = some ([1, 1], 0) := by
  decide +kernel

/-- A `proc` declared inside a block is refused. -/
theorem nested_callable_refused :
    isStuck (runCalls tallyName [varOf countName "0", block [declareCounter]]) = true := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Callables
