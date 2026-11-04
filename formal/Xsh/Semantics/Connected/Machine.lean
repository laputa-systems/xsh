import Xsh.Semantics.Connected.Values
import Xsh.Typing.Connected.Validate
import Xsh.Values.IntegerOps

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Syntax.Foundation
open Xsh.Semantics.Environments
open Xsh.Typing.Connected
open Xsh.Values
open Xsh.Values.IntegerOps

structure Function where
  parameters : List BinderId
  body : List StatementNode
  scheme : Scheme
  elaboration : Substitution


def runtimeMonotype : RuntimeType → Option MonoType
  | .unit => some (.primitive .unit)
  | .integer => some (.primitive .integer)
  | .boolean => some (.primitive .boolean)
  | .string => some (.primitive .string)
  | .error => some (.primitive .error)
  | .record row => (runtimeMonotype row).map MonoType.record
  | .rowEmpty => some .rowEmpty
  | .rowField name value tail => do
      some (.rowField name (← runtimeMonotype value) (← runtimeMonotype tail))
  | .argumentsEmpty => some .argumentsEmpty
  | .argument value tail => do
      some (.argument (← runtimeMonotype value) (← runtimeMonotype tail))
  | .result value failure => do
      some (.result (← runtimeMonotype value) (← runtimeMonotype failure))
  | .function arguments result => do
      some (.function (← runtimeMonotype arguments) (← runtimeMonotype result))
  | .schemeFunction => none
  | .nominal identity base => (runtimeMonotype base).map (.nominal identity)


def monoRuntime : MonoType → Option RuntimeType
  | .primitive .unit => some .unit
  | .primitive .integer => some .integer
  | .primitive .boolean => some .boolean
  | .primitive .string => some .string
  | .primitive .error => some .error
  | .record row => (monoRuntime row).map RuntimeType.record
  | .rowEmpty => some .rowEmpty
  | .rowField name value tail => do
      some (.rowField name (← monoRuntime value) (← monoRuntime tail))
  | .argumentsEmpty => some .argumentsEmpty
  | .argument value tail => do
      some (.argument (← monoRuntime value) (← monoRuntime tail))
  | .result value failure => do
      some (.result (← monoRuntime value) (← monoRuntime failure))
  | .function arguments result => do
      some (.function (← monoRuntime arguments) (← monoRuntime result))
  | .nominal identity base => (monoRuntime base).map (.nominal identity)
  | _ => none

def instantiateFunction (solution : Substitution) (scheme : Scheme)
    (arguments : List (Variable × MonoType)) :
    Option (RuntimeType × RuntimeType × List (Variable × MonoType)) := do
  if instantiationKeysValid scheme arguments && instantiationSortsValid solution arguments then pure () else none
  let substitution := schemeSubstitution scheme (witnessSubstitution arguments)
  if checksConstraints solution (scheme.requirements.map (mapConstraint substitution)) then pure () else none
  let .function parameters result := substitute solution (substitute substitution scheme.body) | none
  let parameterTypes ← monoRuntime parameters
  let resultType ← monoRuntime result
  some (parameterTypes, resultType, arguments.map fun (id, ty) => (id, substitute solution ty))

def ordinaryValue : Value → Bool
  | .schemeFunction _ _ => false
  | _ => true

def SignatureInstance (scheme : Scheme) (arguments result : RuntimeType) : Prop :=
  ∃ argumentType resultType substitution requirements,
    runtimeMonotype arguments = some argumentType ∧ runtimeMonotype result = some resultType ∧
    Instantiates scheme substitution (.function argumentType resultType) requirements ∧
    Solves requirements identity

inductive Callable where
  | function (body : Function)
  | resultOk
  | resultError
  | timeNow

abbrev FunctionTable := DeclarationId → Option Callable
abbrev ActiveCells := BinderId → Option CellId

def emptyCells : ActiveCells := fun _ => none

def bindActive (cells : ActiveCells) (binder : BinderId) (cell : CellId) : ActiveCells :=
  fun other => if other = binder then some cell else cells other

def withGlobals (locals globals : ActiveCells) : ActiveCells := fun binder =>
  (locals binder).orElse fun _ => globals binder

def allocate (store : Store) (cell : CellId) (mutability : Mutability) (ty : RuntimeType) (value : Value) : Option Store :=
  if (store cell).isSome then none else
    if typed : HasType ty value then
      let entry : StoreCell RuntimeType Value HasType := ⟨⟨ty, value, typed⟩, mutability⟩
      some (fun other => if other = cell then some entry else store other)
    else none

def read (store : Store) (cells : ActiveCells) (binder : BinderId) : Option Value := do
  let cell ← cells binder
  let entry ← store cell
  some entry.contents.value

inductive Fault where
  | unsupported
  | missingBinding (binder : BinderId)
  | badLiteral
  | expectedBoolean
  | expectedRecord
  | missingField (name : NameId)
  | expectedResult
  | expectedFunction
  | unknownFunction
  | wrongArguments
  | invalidValue
  | rejectedWrite
  | integer (fault : IntegerFault)


def integerOperator : BinaryOp → Option IntegerBinaryOp
  | .add => some .add
  | .sub => some .subtract
  | .mul => some .multiply
  | .div => some .divide
  | .rem => some .remainder
  | _ => none

def evaluateBinary (operator : BinaryOp) (left right : Value) : Except Fault Value :=
  match left, right with
  | .scalar (.integer first), .scalar (.integer second) =>
    match integerOperator operator with
    | some op => match applyBinary op first second with
      | .value result => .ok (.scalar (.integer result))
      | .fault fault => .error (.integer fault)
    | none => match operator with
      | .lt => .ok (.scalar (.boolean (decide (first.val < second.val))))
      | .le => .ok (.scalar (.boolean (decide (first.val ≤ second.val))))
      | .gt => .ok (.scalar (.boolean (decide (first.val > second.val))))
      | .ge => .ok (.scalar (.boolean (decide (first.val ≥ second.val))))
      | _ => .error .invalidValue
  | .scalar (.boolean first), .scalar (.boolean second) =>
    match operator with
    | .and => .ok (.scalar (.boolean (first && second)))
    | .or => .ok (.scalar (.boolean (first || second)))
    | _ => .error .invalidValue
  | _, _ => .error .invalidValue

def bindParameters (activation : Nat) : List BinderId → List Value → RuntimeType → Store →
    Except Fault (ActiveCells × Store)
  | [], [], .argumentsEmpty, store => .ok (emptyCells, store)
  | binder :: binders, value :: values, .argument ty rest, store => do
    if HasType ty value then pure () else throw .wrongArguments
    let cell : CellId := ⟨activation, binder⟩
    let some next := allocate store cell .immutable ty value | throw .invalidValue
    let (cells, final) ← bindParameters activation binders values rest next
    if (cells binder).isSome then throw .wrongArguments
    pure (bindActive cells binder cell, final)
  | _, _, _, _ => .error .wrongArguments

inductive Guard where
  | integerRange (minimum maximum : IntValue)
  | recordShape (row : RuntimeType)

def guardBase : Guard → RuntimeType
  | .integerRange _ _ => .integer
  | .recordShape row => .record row


def guardPlain (guard : Guard) (value : Value) : Option Bool :=
  match guard, value with
  | .integerRange minimum maximum, .scalar (.integer candidate) =>
      some (decide (minimum.val ≤ candidate.val ∧ candidate.val ≤ maximum.val))
  | .recordShape row, record@(.record _) =>
      (valueType record).map (typeFits (.record row))
  | _, _ => some false

def guardDecision (guard : Guard) (value : Value) : Option Bool :=
  guardPlain guard (eraseNominal value)

abbrev SchemaTable := BinderId → Option (Xsh.Validation.Nominal.NominalId × Guard)

inductive Request where
  | timeNow
  deriving DecidableEq

inductive Label where
  | internal
  | request (request : Request) (response : IntValue)

inductive Control where
  | expression (value : Node)
  | value (value : Value)
  | invoke (target : Value) (arguments : List Value)
  | statements (body : List StatementNode)
  | returned (value : Value)
  | completed (value : Value)
  | propagated (value : Value)
  | fault (fault : Fault)

inductive Frame where
  | binaryLeft (operator : BinaryOp) (right : Node)
  | binaryRight (operator : BinaryOp) (left : Value)
  | callTarget (arguments : List Node)
  | callArguments (target : Value) (done : List Value) (rest : List Node)
  | recordFields (done : List (NameId × Value)) (name : NameId) (rest : List (NameId × Node))
  | field (name : NameId)
  | conditional (branches : List (Node × Node)) (otherwise : Node)
  | propagate
  | requireGuard (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard)
  | bind (binder : BinderId) (kind : Xsh.Typing.Connected.BindingKind) (storage : RuntimeType) (rest : List StatementNode)
  | assign (binder : BinderId) (rest : List StatementNode)
  | discard (rest : List StatementNode)
  | returnValue
  | tailValue
  | branch (branches : List (Node × List StatementNode)) (otherwise rest : List StatementNode)
  | continueBlock (cells : ActiveCells) (rest : List StatementNode)
  | valueBlock (cells : ActiveCells)
  | caller (cells : ActiveCells) (activation : Nat) (solution : Substitution) (result : RuntimeType)

structure Machine where
  functions : FunctionTable
  schemas : SchemaTable
  globals : ActiveCells
  cells : ActiveCells
  store : Store
  activation : Nat
  nextActivation : Nat
  solution : Substitution
  control : Control
  stack : List Frame


def isCaller : Frame → Bool
  | .caller _ _ _ _ => true
  | _ => false

inductive Step : Machine → Label → Machine → Prop where
  | valueBlockStart (state : Machine) (completion : MonoType) (body : List StatementNode) :
      Step { state with control := .expression (.valueBlock completion body) } .internal
        { state with control := .statements body, stack := .valueBlock state.cells :: state.stack }
  | valueBlockComplete (state : Machine) (value : Value) (cells : ActiveCells) :
      Step { state with control := .completed value, stack := .valueBlock cells :: state.stack } .internal
        { state with control := .value value, cells := cells }
  | integer (state : Machine) (text : String) (value : IntValue)
      (parsed : parseInteger text = some value) :
      Step { state with control := .expression (.integer text) } .internal
        { state with control := .value (.scalar (.integer value)) }
  | integerInvalid (state : Machine) (text : String)
      (invalid : parseInteger text = none) :
      Step { state with control := .expression (.integer text) } .internal
        { state with control := .fault .badLiteral }
  | boolean (state : Machine) (value : Bool) :
      Step { state with control := .expression (.boolean value) } .internal
        { state with control := .value (.scalar (.boolean value)) }
  | string (state : Machine) (value : String) :
      Step { state with control := .expression (.string value) } .internal
        { state with control := .value (.scalar (.string value)) }
  | read (state : Machine) (binder : BinderId) (arguments : List (Variable × MonoType)) (value : Value)
      (found : Connected.read state.store state.cells binder = some value) (ordinary : ordinaryValue value = true) :
      Step { state with control := .expression (.read binder arguments) } .internal
        { state with control := .value value }
  | readFunction (state : Machine) (binder : BinderId) (arguments : List (Variable × MonoType))
      (declaration : DeclarationId) (scheme : Scheme) (parameters result : RuntimeType)
      (normalized : List (Variable × MonoType))
      (found : Connected.read state.store state.cells binder = some (.schemeFunction declaration scheme))
      (instantiated : instantiateFunction state.solution scheme arguments = some (parameters, result, normalized)) :
      Step { state with control := .expression (.read binder arguments) } .internal
        { state with control := .value (.instantiatedFunction declaration parameters result state.solution arguments) }
  | readFunctionUnsupported (state : Machine) (binder : BinderId) (arguments : List (Variable × MonoType))
      (declaration : DeclarationId) (scheme : Scheme)
      (found : Connected.read state.store state.cells binder = some (.schemeFunction declaration scheme))
      (unsupported : instantiateFunction state.solution scheme arguments = none) :
      Step { state with control := .expression (.read binder arguments) } .internal
        { state with control := .fault .unsupported }
  | readMissing (state : Machine) (binder : BinderId) (arguments : List (Variable × MonoType))
      (missing : Connected.read state.store state.cells binder = none) :
      Step { state with control := .expression (.read binder arguments) } .internal
        { state with control := .fault (.missingBinding binder) }
  | binary (state : Machine) (operator : BinaryOp) (left right : Node) :
      Step { state with control := .expression (.binary operator left right) } .internal
        { state with control := .expression left, stack := .binaryLeft operator right :: state.stack }
  | binaryLeft (state : Machine) (operator : BinaryOp) (left : Value) (right : Node)
      (notShortCircuit : operator ≠ .and ∧ operator ≠ .or) :
      Step { state with control := .value left, stack := .binaryLeft operator right :: state.stack } .internal
        { state with control := .expression right, stack := .binaryRight operator left :: state.stack }
  | andFalse (state : Machine) (right : Node) :
      Step { state with control := .value (.scalar (.boolean false)), stack := .binaryLeft .and right :: state.stack } .internal
        { state with control := .value (.scalar (.boolean false)) }
  | andTrue (state : Machine) (right : Node) :
      Step { state with control := .value (.scalar (.boolean true)), stack := .binaryLeft .and right :: state.stack } .internal
        { state with control := .expression right, stack := .binaryRight .and (.scalar (.boolean true)) :: state.stack }
  | orTrue (state : Machine) (right : Node) :
      Step { state with control := .value (.scalar (.boolean true)), stack := .binaryLeft .or right :: state.stack } .internal
        { state with control := .value (.scalar (.boolean true)) }
  | orFalse (state : Machine) (right : Node) :
      Step { state with control := .value (.scalar (.boolean false)), stack := .binaryLeft .or right :: state.stack } .internal
        { state with control := .expression right, stack := .binaryRight .or (.scalar (.boolean false)) :: state.stack }
  | binaryResult (state : Machine) (operator : BinaryOp) (left right result : Value)
      (evaluated : evaluateBinary operator left right = .ok result) :
      Step { state with control := .value right, stack := .binaryRight operator left :: state.stack } .internal
        { state with control := .value result }
  | binaryFault (state : Machine) (operator : BinaryOp) (left right : Value) (fault : Fault)
      (evaluated : evaluateBinary operator left right = .error fault) :
      Step { state with control := .value right, stack := .binaryRight operator left :: state.stack } .internal
        { state with control := .fault fault }
  | conditionalEmpty (state : Machine) (otherwise : Node) :
      Step { state with control := .expression (.conditional [] otherwise) } .internal
        { state with control := .expression otherwise }
  | conditionalStart (state : Machine) (condition branch : Node) (rest : List (Node × Node)) (otherwise : Node) :
      Step { state with control := .expression (.conditional ((condition, branch) :: rest) otherwise) } .internal
        { state with control := .expression condition, stack := .conditional ((condition, branch) :: rest) otherwise :: state.stack }
  | conditionalTrue (state : Machine) (condition branch : Node) (rest : List (Node × Node)) (otherwise : Node) :
      Step { state with control := .value (.scalar (.boolean true)), stack := .conditional ((condition, branch) :: rest) otherwise :: state.stack } .internal
        { state with control := .expression branch }
  | conditionalFalse (state : Machine) (condition branch : Node) (rest : List (Node × Node)) (otherwise : Node) :
      Step { state with control := .value (.scalar (.boolean false)), stack := .conditional ((condition, branch) :: rest) otherwise :: state.stack } .internal
        { state with control := .expression (.conditional rest otherwise) }
  | fieldStart (state : Machine) (base : Node) (name : NameId) :
      Step { state with control := .expression (.field base name) } .internal
        { state with control := .expression base, stack := .field name :: state.stack }
  | fieldValue (state : Machine) (name : NameId) (fields : List (NameId × Value)) (value : Value)
      (found : fields.lookup name = some value) :
      Step { state with control := .value (.record fields), stack := .field name :: state.stack } .internal
        { state with control := .value value }
  | fieldMissing (state : Machine) (name : NameId) (fields : List (NameId × Value))
      (missing : fields.lookup name = none) :
      Step { state with control := .value (.record fields), stack := .field name :: state.stack } .internal
        { state with control := .fault (.missingField name) }
  | propagateStart (state : Machine) (inner : Node) (error : MonoType) :
      Step { state with control := .expression (.propagate inner error) } .internal
        { state with control := .expression inner, stack := .propagate :: state.stack }
  | propagateOk (state : Machine) (value : Value) (errorType : RuntimeType) :
      Step { state with control := .value (.ok value errorType), stack := .propagate :: state.stack } .internal
        { state with control := .value value }
  | propagateError (state : Machine) (error : Value) (valueType : RuntimeType) :
      Step { state with control := .value (.error error valueType), stack := .propagate :: state.stack } .internal
        { state with control := .propagated (.error error valueType) }

  | requireStart (state : Machine) (binder : BinderId) (identity : Xsh.Validation.Nominal.NominalId)
      (value : Node) (guard : Guard) (found : state.schemas binder = some (identity, guard)) :
      Step { state with control := .expression (.requireValue binder identity value) } .internal
        { state with control := .expression value, stack := .requireGuard identity guard :: state.stack }
  | requireMissing (state : Machine) (binder : BinderId) (identity : Xsh.Validation.Nominal.NominalId)
      (value : Node) (missing : ∀ guard, state.schemas binder ≠ some (identity, guard)) :
      Step { state with control := .expression (.requireValue binder identity value) } .internal
        { state with control := .fault .invalidValue }
  | requireSuccess (state : Machine) (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard)
      (value : Value) (accepted : guardDecision guard value = some true) :
      Step { state with control := .value value, stack := .requireGuard identity guard :: state.stack } .internal
        { state with control := .value (.ok (.nominal identity (guardBase guard) (eraseNominal value)) .error) }
  | requireFailure (state : Machine) (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard)
      (value : Value) (rejected : guardDecision guard value = some false) :
      Step { state with control := .value value, stack := .requireGuard identity guard :: state.stack } .internal
        { state with control := .value (.error .validationError (.nominal identity (guardBase guard))) }
  | requireUnsupported (state : Machine) (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard)
      (value : Value) (unsupported : guardDecision guard value = none) :
      Step { state with control := .value value, stack := .requireGuard identity guard :: state.stack } .internal
        { state with control := .fault .unsupported }
  | recordEmpty (state : Machine) :
      Step { state with control := .expression (.record []) } .internal
        { state with control := .value (.record []) }
  | recordStart (state : Machine) (name : NameId) (value : Node) (rest : List (NameId × Node)) :
      Step { state with control := .expression (.record ((name, value) :: rest)) } .internal
        { state with control := .expression value, stack := .recordFields [] name rest :: state.stack }
  | recordNext (state : Machine) (done : List (NameId × Value)) (name nextName : NameId)
      (value : Value) (next : Node) (rest : List (NameId × Node)) :
      Step { state with control := .value value, stack := .recordFields done name ((nextName, next) :: rest) :: state.stack } .internal
        { state with control := .expression next, stack := .recordFields (done ++ [(name, value)]) nextName rest :: state.stack }
  | recordFinish (state : Machine) (done : List (NameId × Value)) (name : NameId) (value : Value) :
      Step { state with control := .value value, stack := .recordFields done name [] :: state.stack } .internal
        { state with control := .value (.record (done ++ [(name, value)])) }
  | bindingStart (state : Machine) (binder : BinderId) (kind : Xsh.Typing.Connected.BindingKind)
      (rawStorage : MonoType) (storage : RuntimeType) (initializer : Node) (rest : List StatementNode)
      (closed : monoRuntime (substitute state.solution rawStorage) = some storage) :
      Step { state with control := .statements (.binding binder kind rawStorage initializer :: rest) } .internal
        { state with control := .expression initializer, stack := .bind binder kind storage rest :: state.stack }
  | bindingStorageInvalid (state : Machine) (binder : BinderId) (kind : Xsh.Typing.Connected.BindingKind)
      (rawStorage : MonoType) (initializer : Node) (rest : List StatementNode)
      (unresolved : monoRuntime (substitute state.solution rawStorage) = none) :
      Step { state with control := .statements (.binding binder kind rawStorage initializer :: rest) } .internal
        { state with control := .fault .invalidValue }
  | bindingCommit (state : Machine) (binder : BinderId) (kind : Xsh.Typing.Connected.BindingKind)
      (storage : RuntimeType) (value : Value) (rest : List StatementNode) (next : Store)
      (allocated : allocate state.store ⟨state.activation, binder⟩
        (if kind = .mutable then .mutable else .immutable) storage value = some next) :
      Step { state with control := .value value, stack := .bind binder kind storage rest :: state.stack } .internal
        { state with control := .statements rest, store := next, cells := bindActive state.cells binder ⟨state.activation, binder⟩ }
  | bindingRejected (state : Machine) (binder : BinderId) (kind : Xsh.Typing.Connected.BindingKind)
      (storage : RuntimeType) (value : Value) (rest : List StatementNode)
      (rejected : allocate state.store ⟨state.activation, binder⟩
        (if kind = .mutable then .mutable else .immutable) storage value = none) :
      Step { state with control := .value value, stack := .bind binder kind storage rest :: state.stack } .internal
        { state with control := .fault .invalidValue }
  | assignmentStart (state : Machine) (binder : BinderId) (value : Node) (rest : List StatementNode) :
      Step { state with control := .statements (.assignment binder value :: rest) } .internal
        { state with control := .expression value, stack := .assign binder rest :: state.stack }
  | assignmentCommit (state : Machine) (binder : BinderId) (value : Value) (rest : List StatementNode)
      (cell : CellId) (next : Store) (active : state.cells binder = some cell)
      (written : write state.store cell value = .accepted next) :
      Step { state with control := .value value, stack := .assign binder rest :: state.stack } .internal
        { state with control := .statements rest, store := next }
  | assignmentRejected (state : Machine) (binder : BinderId) (value : Value) (rest : List StatementNode)
      (cell : CellId) (next : Store) (active : state.cells binder = some cell)
      (rejected : write state.store cell value = .rejected next) :
      Step { state with control := .value value, stack := .assign binder rest :: state.stack } .internal
        { state with control := .fault .rejectedWrite, store := next }
  | assignmentMissing (state : Machine) (binder : BinderId) (value : Value) (rest : List StatementNode)
      (missing : state.cells binder = none) :
      Step { state with control := .value value, stack := .assign binder rest :: state.stack } .internal
        { state with control := .fault (.missingBinding binder) }
  | evaluateTail (state : Machine) (value : Node) :
      Step { state with control := .statements [.evaluated value] } .internal
        { state with control := .expression value, stack := .tailValue :: state.stack }
  | evaluateAndContinue (state : Machine) (value : Node) (next : StatementNode) (rest : List StatementNode) :
      Step { state with control := .statements (.evaluated value :: next :: rest) } .internal
        { state with control := .expression value, stack := .discard (next :: rest) :: state.stack }
  | discarded (state : Machine) (value : Value) (rest : List StatementNode) :
      Step { state with control := .value value, stack := .discard rest :: state.stack } .internal
        { state with control := .statements rest }
  | tailValue (state : Machine) (value : Value) :
      Step { state with control := .value value, stack := .tailValue :: state.stack } .internal
        { state with control := .completed value }
  | tailBranchComplete (state : Machine) (value : Value) (cells : ActiveCells) :
      Step { state with control := .completed value, stack := .continueBlock cells [] :: state.stack } .internal
        { state with control := .completed value, cells := cells }
  | nonTailBranchComplete (state : Machine) (value : Value) (cells : ActiveCells)
      (next : StatementNode) (rest : List StatementNode) :
      Step { state with control := .completed value, stack := .continueBlock cells (next :: rest) :: state.stack } .internal
        { state with control := .statements (next :: rest), cells := cells }
  | returnStart (state : Machine) (value : Node) (rest : List StatementNode) :
      Step { state with control := .statements (.returned value :: rest) } .internal
        { state with control := .expression value, stack := .returnValue :: state.stack }
  | returnValue (state : Machine) (value : Value) :
      Step { state with control := .value value, stack := .returnValue :: state.stack } .internal
        { state with control := .returned value }

  | callStart (state : Machine) (callee : Node) (arguments : List Node) :
      Step { state with control := .expression (.call callee arguments) } .internal
        { state with control := .expression callee, stack := .callTarget arguments :: state.stack }
  | callWithoutArguments (state : Machine) (target : Value) :
      Step { state with control := .value target, stack := .callTarget [] :: state.stack } .internal
        { state with control := .invoke target [] }
  | callFirstArgument (state : Machine) (target : Value) (first : Node) (rest : List Node) :
      Step { state with control := .value target, stack := .callTarget (first :: rest) :: state.stack } .internal
        { state with control := .expression first, stack := .callArguments target [] rest :: state.stack }
  | callNextArgument (state : Machine) (target value : Value) (done : List Value) (next : Node) (rest : List Node) :
      Step { state with control := .value value, stack := .callArguments target done (next :: rest) :: state.stack } .internal
        { state with control := .expression next, stack := .callArguments target (done ++ [value]) rest :: state.stack }
  | callArgumentsComplete (state : Machine) (target value : Value) (done : List Value) :
      Step { state with control := .value value, stack := .callArguments target done [] :: state.stack } .internal
        { state with control := .invoke target (done ++ [value]) }
  | callFunction (state : Machine) (declaration : DeclarationId) (arguments : List Value)
      (parameterTypes resultType : RuntimeType) (captured : Substitution)
      (typeArguments normalized : List (Variable × MonoType)) (function : Function) (cells : ActiveCells) (store : Store)
      (found : state.functions declaration = some (.function function))
      (signature : instantiateFunction captured function.scheme typeArguments = some (parameterTypes, resultType, normalized))
      (bound : bindParameters state.nextActivation function.parameters arguments parameterTypes state.store = .ok (cells, store)) :
      Step { state with control := .invoke (.instantiatedFunction declaration parameterTypes resultType captured typeArguments) arguments } .internal
        { state with control := .statements function.body, cells := withGlobals cells state.globals, store := store, activation := state.nextActivation, nextActivation := state.nextActivation + 1, solution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration), stack := .caller state.cells state.activation state.solution resultType :: state.stack }
  | callOk (state : Machine) (declaration : DeclarationId) (value : Value) (valueType' errorType : RuntimeType)
      (found : state.functions declaration = some .resultOk) (typed : HasType valueType' value) :
      Step { state with control := .invoke (.function declaration (.argument valueType' .argumentsEmpty)
        (.result valueType' errorType)) [value] } .internal
        { state with control := .value (.ok value errorType) }
  | callError (state : Machine) (declaration : DeclarationId) (value : Value) (valueType' errorType : RuntimeType)
      (found : state.functions declaration = some .resultError) (typed : HasType errorType value) :
      Step { state with control := .invoke (.function declaration (.argument errorType .argumentsEmpty)
        (.result valueType' errorType)) [value] } .internal
        { state with control := .value (.error value valueType') }
  | callOkInstantiated (state : Machine) (declaration : DeclarationId) (value : Value)
      (valueType' errorType : RuntimeType) (captured : Substitution) (typeArguments : List (Variable × MonoType))
      (found : state.functions declaration = some .resultOk) (typed : HasType valueType' value) :
      Step { state with control := .invoke (.instantiatedFunction declaration (.argument valueType' .argumentsEmpty)
        (.result valueType' errorType) captured typeArguments) [value] } .internal
        { state with control := .value (.ok value errorType) }
  | callErrorInstantiated (state : Machine) (declaration : DeclarationId) (value : Value)
      (valueType' errorType : RuntimeType) (captured : Substitution) (typeArguments : List (Variable × MonoType))
      (found : state.functions declaration = some .resultError) (typed : HasType errorType value) :
      Step { state with control := .invoke (.instantiatedFunction declaration (.argument errorType .argumentsEmpty)
        (.result valueType' errorType) captured typeArguments) [value] } .internal
        { state with control := .value (.error value valueType') }
  | callTimeInstantiated (state : Machine) (declaration : DeclarationId) (captured : Substitution)
      (typeArguments : List (Variable × MonoType)) (response : IntValue)
      (found : state.functions declaration = some .timeNow) :
      Step { state with control := .invoke (.instantiatedFunction declaration .argumentsEmpty .integer captured typeArguments) [] }
        (.request .timeNow response) { state with control := .value (.scalar (.integer response)) }
  | callTime (state : Machine) (declaration : DeclarationId) (response : IntValue)
      (found : state.functions declaration = some .timeNow) :
      Step { state with control := .invoke (.function declaration .argumentsEmpty .integer) [] } (.request .timeNow response)
        { state with control := .value (.scalar (.integer response)) }
  | returnedToCaller (state : Machine) (value : Value) (cells : ActiveCells) (activation : Nat) (saved : Substitution) (ty : RuntimeType)
      (typed : HasType ty value) :
      Step { state with control := .returned value, stack := .caller cells activation saved ty :: state.stack } .internal
        { state with control := .value value, cells := cells, activation := activation, solution := saved }
  | completedToCaller (state : Machine) (value : Value) (cells : ActiveCells) (activation : Nat) (saved : Substitution) (ty : RuntimeType)
      (typed : HasType ty value) :
      Step { state with control := .completed value, stack := .caller cells activation saved ty :: state.stack } .internal
        { state with control := .value value, cells := cells, activation := activation, solution := saved }
  | propagatedToCaller (state : Machine) (error : Value) (oldType valueType' errorType : RuntimeType)
      (cells : ActiveCells) (activation : Nat) (saved : Substitution) (typed : HasType errorType error) :
      Step { state with control := .propagated (.error error oldType), stack := .caller cells activation saved (.result valueType' errorType) :: state.stack } .internal
        { state with control := .value (.error error valueType'), cells := cells, activation := activation, solution := saved }
  | returnUnwind (state : Machine) (value : Value) (frame : Frame) (notCaller : isCaller frame = false) :
      Step { state with control := .returned value, stack := frame :: state.stack } .internal
        { state with control := .returned value }
  | propagateUnwind (state : Machine) (value : Value) (frame : Frame) (notCaller : isCaller frame = false) :
      Step { state with control := .propagated value, stack := frame :: state.stack } .internal
        { state with control := .propagated value }

  | statementConditionalEmpty (state : Machine) (otherwise rest : List StatementNode) :
      Step { state with control := .statements (.conditional [] otherwise :: rest) } .internal
        { state with control := .statements otherwise, stack := .continueBlock state.cells rest :: state.stack }
  | statementConditionalStart (state : Machine) (condition : Node) (body : List StatementNode)
      (branches : List (Node × List StatementNode)) (otherwise rest : List StatementNode) :
      Step { state with control := .statements (.conditional ((condition, body) :: branches) otherwise :: rest) } .internal
        { state with control := .expression condition, stack := .branch ((condition, body) :: branches) otherwise rest :: state.stack }
  | statementConditionalTrue (state : Machine) (condition : Node) (body : List StatementNode)
      (branches : List (Node × List StatementNode)) (otherwise rest : List StatementNode) :
      Step { state with control := .value (.scalar (.boolean true)), stack := .branch ((condition, body) :: branches) otherwise rest :: state.stack } .internal
        { state with control := .statements body, stack := .continueBlock state.cells rest :: state.stack }
  | statementConditionalFalse (state : Machine) (condition : Node) (body : List StatementNode)
      (branches : List (Node × List StatementNode)) (otherwise rest : List StatementNode) :
      Step { state with control := .value (.scalar (.boolean false)), stack := .branch ((condition, body) :: branches) otherwise rest :: state.stack } .internal
        { state with control := .statements (.conditional branches otherwise :: rest) }
  | blockComplete (state : Machine) :
      Step { state with control := .statements [] } .internal
        { state with control := .completed .unit }


inductive Execution : Machine → List Label → Machine → Prop where
  | done (state : Machine) : Execution state [] state
  | next {initial intermediate final : Machine} {event : Label} {events : List Label} :
      Step initial event intermediate → Execution intermediate events final →
      Execution initial (event :: events) final

theorem execution_append {initial intermediate final : Machine} {first second : List Label}
    (firstRun : Execution initial first intermediate) (secondRun : Execution intermediate second final) :
    Execution initial (first ++ second) final := by
  induction firstRun with
  | done => exact secondRun
  | next step _ ih => exact .next step (ih secondRun)

theorem assignment_commit_requires_mutable_matching_cell
    (store next : Store) (cell : CellId) (candidate : Value)
    (accepted : write store cell candidate = .accepted next) :
    ∃ current, store cell = some current ∧ current.mutability = .mutable ∧
      HasType current.contents.typeTag candidate := by
  exact (write_accepted_iff (fun _ _ => inferInstance) store cell candidate).mp ⟨next, accepted⟩

theorem rejected_assignment_does_not_change_store
    (store next : Store) (cell : CellId) (candidate : Value)
    (rejected : write store cell candidate = .rejected next) : next = store := by
  exact write_rejected_keeps_store (fun _ _ => inferInstance) store cell candidate next rejected

theorem discarded_boolean_branch_does_not_evaluate_its_right_operand
    (state : Machine) (right : Node) :
    Step { state with control := .value (.scalar (.boolean false)), stack := .binaryLeft .and right :: state.stack }
      .internal { state with control := .value (.scalar (.boolean false)) } :=
  .andFalse state right


theorem read_after_assignment (store next : Store) (cells : ActiveCells)
    (binder : BinderId) (cell : CellId) (candidate : Value)
    (active : cells binder = some cell) (accepted : write store cell candidate = .accepted next) :
    Connected.read next cells binder = some candidate := by
  obtain ⟨_, _, ⟨replaced, replacedAt, _, value, _⟩, _⟩ :=
    write_accepted_replaces_only_its_cell (fun _ _ => inferInstance) store cell candidate next accepted
  simp [Connected.read, active, replacedAt, value]


theorem allocate_fresh_typed_cell (store : Store) (cell : CellId) (mutability : Mutability)
    (value : Value) (ty : RuntimeType) (fresh : store cell = none)
    (typed : HasType ty value) :
    ∃ next current, allocate store cell mutability ty value = some next ∧
      next cell = some current ∧ current.mutability = mutability ∧
      current.contents.typeTag = ty ∧ current.contents.value = value := by
  let current : StoreCell RuntimeType Value HasType := ⟨⟨ty, value, typed⟩, mutability⟩
  let next : Store := fun other => if other = cell then some current else store other
  refine ⟨next, current, ?_, ?_, rfl, rfl, rfl⟩
  · simp [allocate, fresh, typed, next, current]
  · simp [next]

end Xsh.Semantics.Connected
