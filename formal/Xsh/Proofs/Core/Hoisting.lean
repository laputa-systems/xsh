import Xsh.Proofs.Core.Arguments

namespace Xsh.Proofs.Core.Hoisting

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

/-- Declaration hoisting (SPEC 3.1): "Declarations are visible throughout
their script or module, including the statements before them", so a script
runs with its top-level functions bound first, each closure over the
environment at its declaration; "A callable body resolves the top-level
bindings declared before the callable", so a binding declared after it stays
unresolved in its body; and "A call that reads such a binding before the
statement that initializes it has run is a runtime failure", which `try`
does not capture. Type and error constructors come from the program's
declarations, which are visible throughout. -/
theorem declaration_hoisting {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ fuel (frame hoisted : Frame world) body, hoist machine frame body = some hoisted →
      runScript machine fuel frame body = runStatements machine fuel hoisted body) ∧
    (∀ fuel (frame : Frame world) body, hoist machine frame body = none →
      runScript machine fuel frame body = .internal .malformedTarget frame) ∧
    (∀ (frame : Frame world) body,
      hoist machine frame body =
        (hoistNames machine frame.activation frame.environment (hoistedNames body)).bind fun environment =>
          (hoistClosures machine frame.activation environment [] frame.closures body).map fun closures =>
            { frame with environment, locals := hide (hoistedNames body) frame.locals, closures }) ∧
    (∀ activation environment mutable closures kind name parameters effects result body metadata test rest callable
        binder cell type,
      callableKind kind = some callable →
        (machine.namesAt activation).declaration environment name = some (binder, cell, type) →
          hoistClosures machine activation environment mutable closures
              (.functionDef kind name parameters effects result body metadata test :: rest) =
            hoistClosures machine activation (extend environment [(name, binder, cell)]) mutable
              ((cell, .mk cell callable (extend environment [(name, binder, cell)]) closures
                ((captureNames (parameterNames parameters) body).filterMap fun captured =>
                  match (extend environment [(name, binder, cell)]).cellOf captured with
                  | .ok found => if mutable.contains found then some found else none
                  | .error _ => none)
                parameters (.block body (returnKindOf machine result))) :: closures) rest) ∧
    (∀ fuel (frame : Frame world) name cell,
      frame.locals.lookup name = none → localClosure frame.closures frame.environment name = none →
        frame.environment.cellOf name = .ok cell → frame.store cell = none →
          evalExpr machine (fuel + 1) frame (.ident name) =
            .exec (.failed (.runtime (machine.uninitialized name)) frame)) ∧
    (∀ (error : Value Declaration Identity Source) (frame : Frame world),
      Xsh.Semantics.CoreControl.capture (.exec (.failed (.runtime error) frame)) =
        .exec (.failed (.runtime error) frame)) := by
  refine ⟨?_, ?_, fun _ _ => rfl, ?_, ?_, fun _ _ => rfl⟩
  · intro fuel frame hoisted body hoisted_
    simp only [runScript, hoisted_]
  · intro fuel frame body failed
    simp only [runScript, failed]
  · intro activation environment mutable closures kind name parameters effects result body metadata test rest callable
      binder cell type kinded declared
    simp only [hoistClosures, kinded, declared]
    rfl
  · intro fuel frame name cell unshadowed noClosure resolved absent
    simp only [evalExpr, unshadowed, noClosure, resolved, absent]

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Threading.Witness
open Xsh.Proofs.Core.Callables.Witness
open Xsh.Proofs.Core.Constructors.Witness

def earlyName : Xsh.Syntax.RawName := name 170 (by decide)
def readName : Xsh.Syntax.RawName := name 171 (by decide)
def valueParam : Xsh.Syntax.RawName := name 172 (by decide)
def doubleFn : Xsh.Syntax.RawName := name 173 (by decide)
def outputs : List Xsh.Syntax.RawName := [name 175 (by decide), name 176 (by decide), name 177 (by decide)]

def output (index : Nat) : Xsh.Syntax.RawName := outputs.getD index (name 179 (by decide))

def runHoisted (body : List Xsh.Syntax.Statement) : Exec witnessWorld :=
  runScript constructorMachine 90 fullStart body

/-- From `tests/xsh/declaration-hoisting.xsh`: `early()`, `double(7)` and
`Point(x: 1).y` above the declarations give `1`, `14` and `7`. -/
def hoistedProgram : List Xsh.Syntax.Statement :=
  [letOf (output 0) (call earlyName []),
    letOf (output 1) (call doubleFn [int "7"]),
    letOf (output 2) (.field (.call (ident pointName) [.named xField (int "1")]) yField),
    .functionDef .proc earlyName [] none none [.expression (int "1")] none false,
    .functionDef .pure doubleFn [.mk valueParam none none false] none none
      [.expression (.binary .mul (ident valueParam) (int "2"))] none false,
    declarePoint]

theorem declarations_visible_before :
    [readInt (output 0) (runHoisted hoistedProgram), readInt (output 1) (runHoisted hoistedProgram),
      readInt (output 2) (runHoisted hoistedProgram)] = [some 1, some 14, some 7] := by
  decide +kernel

def declareRead : Xsh.Syntax.Statement :=
  .functionDef .proc readName [] none none [.expression (ident countName)] none false

/-- `print (read()); var count = 7; proc read() -> Int { count }`: the read
of `count` before `var count` has run is a runtime failure, as the binary's
`unknown captured name` with exit 3 shows. -/
def uninitializedRead : Exec witnessWorld :=
  runHoisted [letOf rName (call readName []), varOf countName "7", declareRead]

/-- Inside `try`, the failure still escapes. -/
def uninitializedTried : Exec witnessWorld :=
  runHoisted [letOf rName (.capture [.expression (call readName [])]), varOf countName "7", declareRead]

theorem uninitialized_read_fails :
    runtimeFailure uninitializedRead = some "unknown captured name" ∧
      runtimeFailure uninitializedTried = some "unknown captured name" := by
  decide +kernel

/-- `proc read() -> Int { count }; var count = 7; print (read())`: `count`
is declared after `read`, so it stays unresolved in its body, which the
checker rejects (the binary exits 2). -/
def laterBinding : Exec witnessWorld :=
  runHoisted [declareRead, varOf countName "7", letOf rName (call readName [])]

theorem later_binding_unresolved : isStuck laterBinding = true := by
  decide +kernel

/-- `var count = 0; let r = bump(); proc bump() -> Int { count += 1; count }`:
the hoisted `bump` writes `count`, giving `1` and leaving `count = 1`. -/
def hoistedWrite : Exec witnessWorld :=
  runHoisted [varOf countName "0", letOf rName (call tallyName []), declareCounter]

theorem hoisted_callable_writes :
    readInt rName hoistedWrite = some 1 ∧ readInt countName hoistedWrite = some 1 := by
  decide +kernel

/-- The unhoisted machine does not see `early` before its declaration. -/
theorem unhoisted_call_rejected :
    readInt (output 0) (runStatements constructorMachine 90 fullStart hoistedProgram) = none := by
  decide +kernel

/-- The callee frame `read` runs in when it is called before `var count`. -/
def readFrame : Option (Frame witnessWorld) :=
  (hoist constructorMachine fullStart [letOf rName (call readName []), varOf countName "7", declareRead]).bind
    fun hoisted =>
      match localClosure hoisted.closures hoisted.environment readName with
      | some (.mk _ _ defining _ writable _ _) =>
        some { Frame.ofScope defining hoisted.store [] (some hoisted.store) ⟨1, [], none⟩ with writable }
      | none => none

def readsAsFailure : Valued witnessWorld → Bool
  | .exec (.failed (.runtime _) _) => true
  | _ => false

def readsAsStuck : Valued witnessWorld → Bool
  | .exec .stuck => true
  | _ => false

/-- Reading the uninitialized `count` is a failure; the mutant that reads it
through `CoreEval.step`'s own name rule is stuck instead, and one that reads a
default value gives `null`. -/
theorem uninitialized_mutants_rejected :
    (readFrame.map fun frame => readsAsFailure (evalExpr constructorMachine 10 frame (ident countName))) = some true ∧
      (readFrame.map fun frame => readsAsStuck
        (threadStep constructorMachine.context (evalExpr constructorMachine 9)
          (callFunctionWith constructorMachine (evalExpr constructorMachine 9) (callBlock constructorMachine 9))
          frame (ident countName))) = some true ∧
      (readFrame.map fun frame => readsAsFailure (fromEval frame (Result.value (Value.scalar .null)))) = some false := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Hoisting
