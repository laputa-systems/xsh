import Xsh.Semantics.CoreWrites

namespace Xsh.Semantics.CoreStatements

open Xsh.Foundation
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites

variable {Declaration Identity Source : Type}

/-- What the store-write meaning takes from outside the environment layer:
the name table's spellings, expression and index evaluation (BASE-STEP), the
binder, activation cell and checker-assigned type of each declared name, and
the argv conversion of an environment value (SPEC 11.4). -/
structure Oracles (world : World Declaration Identity Source) where
  spelling : Xsh.Syntax.RawName → String
  evaluate : Environment → Store world → Xsh.Syntax.Expr →
    Except (Failure Declaration Identity Source × Store world) (Value Declaration Identity Source × Store world)
  selector : Environment → Store world → Option (ValueType Declaration) → Xsh.Syntax.Expr →
    Except (Failure Declaration Identity Source × Store world) (Selector × Store world)
  declaration : Environment → Xsh.Syntax.RawName → Option (BinderId × CellId × ValueType Declaration)
  argv : Value Declaration Identity Source → Option (List UInt8)

structure State (world : World Declaration Identity Source) where
  environment : Environment
  store : Store world
  overlay : Overlay

inductive Outcome (world : World Declaration Identity Source) where
  | completed (state : State world)
  | failed (failure : Failure Declaration Identity Source) (state : State world)
  | internal (fault : InternalFault) (state : State world)

def Outcome.state {world : World Declaration Identity Source} : Outcome world → State world
  | .completed state | .failed _ state | .internal _ state => state

abbrev Declared := Xsh.Syntax.RawName × BinderId × CellId

mutual
  def resolveTarget {world : World Declaration Identity Source} (oracles : Oracles world) (environment : Environment) :
      Xsh.Syntax.BindingTarget → Option (Target Declaration × List Declared)
    | .name name => (oracles.declaration environment name).map fun (binder, cell, type) =>
        (.name cell type, [(name, binder, cell)])
    | .discard => some (.discard, [])
    | .record fields hasRest => (resolveFields oracles environment fields).map fun (targets, declared) =>
        (.record targets hasRest, declared)

  def resolveFields {world : World Declaration Identity Source} (oracles : Oracles world) (environment : Environment) :
      List (Xsh.Syntax.RawName × Xsh.Syntax.BindingTarget) → Option (List (String × Target Declaration) × List Declared)
    | [] => some ([], [])
    | (label, target) :: rest =>
      match resolveTarget oracles environment target, resolveFields oracles environment rest with
      | some (first, declared), some (others, more) => some ((oracles.spelling label, first) :: others, declared ++ more)
      | _, _ => none
end

def extend (environment : Environment) : List Declared → Environment
  | [] => environment
  | (name, binder, cell) :: rest =>
    extend ⟨bind environment.names name binder, bindCell environment.cells binder cell⟩ rest

inductive PathSpec where
  | field (label : String)
  | index (selector : Xsh.Syntax.Expr)

def flatten (spelling : Xsh.Syntax.RawName → String) :
    Xsh.Syntax.AssignTarget → Option (Xsh.Syntax.RawName × List PathSpec)
  | .name name => some (name, [])
  | .field base label => (flatten spelling base).map fun (root, path) => (root, path ++ [.field (spelling label)])
  | .index base selector => (flatten spelling base).map fun (root, path) => (root, path ++ [.index selector])
  | .env _ => none

def staticChild : Option (ValueType Declaration) → PathSpec → Option (ValueType Declaration)
  | type, .field label => type.bind fun type => childType type (.field label)
  | type, .index _ => type.bind fun type => (elementType type).orElse fun _ => (mapTypes type).map Prod.snd

def sides {world : World Declaration Identity Source} (oracles : Oracles world) (environment : Environment) :
    Option (ValueType Declaration) → List PathSpec → List (SelectorSide world)
  | _, [] => []
  | type, .field label :: rest =>
    (fun store => .ok (.field label, store)) :: sides oracles environment (staticChild type (.field label)) rest
  | type, .index selector :: rest =>
    (fun store => oracles.selector environment store type selector) ::
      sides oracles environment (staticChild type (.index selector)) rest

def bindingMutability : Xsh.Syntax.BindingKind → Mutability
  | .var => .mutable
  | _ => .immutable

def lift {world : World Declaration Identity Source} (state : State world) (environment : Environment) :
    StoreStep world → Outcome world
  | .committed store => .completed ⟨environment, store, state.overlay⟩
  | .failed failure store => .failed failure ⟨state.environment, store, state.overlay⟩
  | .internal fault store => .internal fault ⟨state.environment, store, state.overlay⟩

variable [DecidableEq Declaration]

def declareStatement (world : World Declaration Identity Source) (decisions : Decisions world)
    (oracles : Oracles world) (state : State world) (kind : Xsh.Syntax.BindingKind)
    (target : Xsh.Syntax.BindingTarget) (initializer : Xsh.Syntax.Expr) : Outcome world :=
  match oracles.evaluate state.environment state.store initializer with
  | .error (failure, store) => .failed failure { state with store := store }
  | .ok (value, store) =>
    match resolveTarget oracles state.environment target with
    | none => .internal .malformedTarget { state with store := store }
    | some (resolved, declared) =>
      lift { state with store := store } (extend state.environment declared)
        (declareTarget world decisions store (bindingMutability kind) resolved value)

def assignStatementAt (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) (target : Xsh.Syntax.AssignTarget) (op : Xsh.Syntax.AssignOp)
    (value : Xsh.Syntax.Expr) : Outcome world :=
  match target with
  | .env key =>
    match op with
    | .set =>
      match oracles.evaluate state.environment state.store value with
      | .error (failure, store) => .failed failure { state with store := store }
      | .ok (result, store) =>
        match oracles.argv result with
        | none => .internal .unconvertibleEnvironment { state with store := store }
        | some bytes =>
          match setEnvironment raises state.overlay (oracles.spelling key).toUTF8.toList bytes with
          | .ok overlay => .completed { state with store := store, overlay := overlay }
          | .error failure => .failed failure { state with store := store }
    | _ => .internal .unconvertibleEnvironment state
  | target =>
    match flatten oracles.spelling target with
    | none => .internal .malformedTarget state
    | some (root, path) =>
      match state.environment.cellOf root with
      | .error fault => .internal (.unresolvedName fault) state
      | .ok cell =>
        lift state state.environment
          (assignStatement world decisions raises operate state.store cell
            (sides oracles state.environment ((state.store cell).map fun found => found.contents.typeTag) path) op
            (fun store => oracles.evaluate state.environment store value))

/-- The store-write meaning of a statement; `none` marks a statement whose
meaning is not a store write. -/
def execute (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) : Xsh.Syntax.Statement → Option (Outcome world)
  | .binding kind target _ initializer => some (declareStatement world decisions oracles state kind target initializer)
  | .export (.binding kind target _ initializer) =>
    some (declareStatement world decisions oracles state kind target initializer)
  | .assign target op value => some (assignStatementAt world decisions raises operate oracles state target op value)
  | _ => none

def runAll (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) : State world → List Xsh.Syntax.Statement → Option (Outcome world)
  | state, [] => some (.completed state)
  | state, statement :: rest =>
    match execute world decisions raises operate oracles state statement with
    | some (.completed next) => runAll world decisions raises operate oracles next rest
    | other => other

def leave {world : World Declaration Identity Source} : Outcome world → Outcome world
  | .completed state =>
    .completed { state with environment := { state.environment with names := exitScope state.environment.names } }
  | .failed failure state =>
    .failed failure { state with environment := { state.environment with names := exitScope state.environment.names } }
  | .internal fault state =>
    .internal fault { state with environment := { state.environment with names := exitScope state.environment.names } }

/-- A block of store-writing statements runs in a new lexical scope; leaving
the block drops its declarations from name resolution. -/
def block (world : World Declaration Identity Source) (decisions : Decisions world)
    (raises : Raises Declaration Identity Source) (operate : Operate Declaration Identity Source)
    (oracles : Oracles world) (state : State world) (body : List Xsh.Syntax.Statement) : Option (Outcome world) :=
  (runAll world decisions raises operate oracles
    { state with environment := { state.environment with names := enterScope state.environment.names } } body).map
    leave

end Xsh.Semantics.CoreStatements
