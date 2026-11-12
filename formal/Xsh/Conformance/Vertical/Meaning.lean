import Xsh.Conformance.Vertical.Artifact
import Xsh.IR.Raw.Laws
import Xsh.Semantics.BaseMachine
import Xsh.Proofs.Base.Runner

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport
open Xsh.IR.Raw
open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Semantics.Environments
open Xsh.Semantics.BaseMachine

/-- The base expression an operation denotes on two operands: arithmetic yields an integer and
the comparisons a boolean. -/
def FragmentOp.expression : FragmentOp → BaseExpr → BaseExpr → BaseExpr
  | .add => .binary .add
  | .sub => .binary .subtract
  | .mul => .binary .multiply
  | .div => .binary .divide
  | .rem => .binary .remainder
  | .gt => .compare .greater
  | .lt => .compare .less

/-- The `Ok` or `Err` constructor a source form names. -/
def constructorExpression : Bool → BaseExpr → BaseExpr
  | true, value => .resultOk value
  | false, value => .resultErr value

def FragmentFailure.fault : FragmentFailure → IntegerFault
  | .divisionByZero => .divisionByZero
  | .integerOverflow => .overflow

/-- What the IR meaning of a function ends with: the integer it completes with, the boolean a
comparison completes with, the `Ok` or `Err` of an integer a constructor completes with, or the
specified failure the base machine terminates with. -/
inductive FunctionOutcome where
  | value (result : Int)
  | truth (result : Bool)
  | constructed (isOk : Bool) (result : Int)
  | failure (kind : FragmentFailure)
  deriving DecidableEq

/-- The integer a 64-bit pattern denotes in two's complement. -/
def signedOfBits (unsigned : Nat) : Int :=
  if unsigned < 9223372036854775808 then
    Int.ofNat unsigned
  else
    Int.ofNat unsigned - 18446744073709551616

/-- Low word first, as the exchange preserves. -/
def signedOfWords (low high : Word32) : Int := signedOfBits (low.val + high.val * 4294967296)

def Observation.integerArguments (observation : Observation) : List Int :=
  observation.arguments.map fun argument => signedOfBits argument.val

/-- The exchanged instruction rows, as the raw tables the structural model decodes. -/
def RawArtifact.tables (artifact : RawArtifact) : Tables :=
  ⟨(artifact.rows.map fun row => row.tag.word).toArray,
    (artifact.rows.map fun row => (⟨row.lhs, row.rhs⟩ : RawData)).toArray,
    artifact.extra.toArray⟩

/-- Read one instruction and its payload; the tag must be known and the payload must have the
width the tag requires. -/
def readInstruction? (tables : Tables) (index : Word32) : Option (FragmentTag × List Word32) := do
  let row ← Decode.readRow? tables (DirectIndex.decode index)
  let tag ← FragmentTag.ofWord? row.tag
  let payload ← Decode.readPayload? tables row.data.range
  if payload.length == tag.payloadWords then pure (tag, payload) else none

mutual
/-- Replace each name by an expression. A call node's body is a parameter scope of its own, so it
is not rewritten; a binding value mentions only parameters and literals. -/
def substituteExpressions (substitution : List (Xsh.Syntax.RawName × BaseExpr)) :
    BaseExpr → BaseExpr
  | .integerLiteral value => .integerLiteral value
  | .scalarLiteral value => .scalarLiteral value
  | .timeNow => .timeNow
  | .identifier name => (substitution.lookup name).getD (.identifier name)
  | .negate operand => .negate (substituteExpressions substitution operand)
  | .binary operator left right =>
      .binary operator (substituteExpressions substitution left)
        (substituteExpressions substitution right)
  | .compare operator left right =>
      .compare operator (substituteExpressions substitution left)
        (substituteExpressions substitution right)
  | .conditional condition thenBranch elseBranch =>
      .conditional (substituteExpressions substitution condition)
        (substituteExpressions substitution thenBranch)
        (substituteExpressions substitution elseBranch)
  | .call parameters body arguments =>
      .call parameters body (substituteExpressionArguments substitution arguments)
  | .resultOk value => .resultOk (substituteExpressions substitution value)
  | .resultErr value => .resultErr (substituteExpressions substitution value)

def substituteExpressionArguments (substitution : List (Xsh.Syntax.RawName × BaseExpr)) :
    List BaseExpr → List BaseExpr
  | [] => []
  | argument :: rest =>
      substituteExpressions substitution argument ::
        substituteExpressionArguments substitution rest
end

/-- A `let` binding: the slot it binds and the integer expression that gives its value. A literal
binding holds an integer literal; a computed binding holds the expression the let evaluates. -/
structure LetBinding where
  slot : Word32
  value : BaseExpr

/-- The checked integer a literal binding holds. -/
def LetBinding.literalInt? (binding : LetBinding) : Option IntValue :=
  match binding.value with
  | .integerLiteral value => validateInt? value
  | _ => none

/-- The parameters of a callee as the names its body uses: slot `k` is the `k`-th parameter. -/
def parameterNames (count : Nat) : List NameId :=
  (List.range count).filterMap fun slot => (Word32.decode? slot).map NameId.mk

/-- A function already read: the number of integer parameters, the body expression, which mentions
its parameters by slot, and the bindings the call evaluates before the body. -/
structure Callee where
  parameters : Nat
  body : BaseExpr
  bindings : List LetBinding := []

/-- The call an application of a callee denotes: its parameters and its bindings are both bound by
the call, and each binding's value has the callee's parameter reads replaced by the argument
expressions. The machine evaluates every argument in order and binds all of them in the body. -/
def Callee.bindingNames (callee : Callee) : List NameId :=
  callee.bindings.map fun binding => NameId.mk binding.slot

def Callee.applyParameters (callee : Callee) : List NameId :=
  parameterNames callee.parameters ++ callee.bindingNames

/-- The applied values of a binding sequence, in order: each binding's value has the declared
substitution applied and every earlier binding's slot replaced by that earlier binding's applied
value, so a later binding reads what the earlier one denotes. -/
def applyLetBindings (substitution : List (Xsh.Syntax.RawName × BaseExpr)) :
    List LetBinding → List BaseExpr
  | [] => []
  | binding :: rest =>
      let value := substituteExpressions substitution binding.value
      value :: applyLetBindings (substitution ++ [(NameId.mk binding.slot, value)]) rest

def Callee.applyArguments (callee : Callee) (values : List BaseExpr) : List BaseExpr :=
  values ++ applyLetBindings ((parameterNames callee.parameters).zip values) callee.bindings

def Callee.apply (callee : Callee) (values : List BaseExpr) : BaseExpr :=
  .call callee.applyParameters callee.body (callee.applyArguments values)

/-- What reading one row needs beyond the tables: the operation pool, the block table and the
callees a call row may name. -/
structure MeaningContext where
  tables : Tables
  ops : List FragmentOp
  blocks : List Block
  callees : List (Option Callee)

def blockWords? (context : MeaningContext) (id : Word32) : Option (List Word32) := do
  let block ← context.blocks.find? fun block => block.id.raw == id
  Decode.readPayload? context.tables ⟨DirectIndex.decode block.start, block.len⟩

/-- The rows of a call's argument table: each entry is a single value (kind zero) that precedes
the call. -/
def argumentRows? (user : Word32) : List Word32 → Option (List Word32)
  | [] => some []
  | [_] => none
  | kind :: row :: rest => do
      check (kind.val == 0 && decide (row.val < user.val))
      let rows ← argumentRows? user rest
      some (row :: rows)

/-- An expression row denotes a base expression. Operands must come from earlier rows, the
operation must be in the exchanged pool, a slot read becomes an identifier whose name and
binder are that slot, a value block is the value of its one statement, a conditional is its one
branch with its else value, and a call carries its callee's body with the callee's parameters. -/
def expression? (context : MeaningContext) : Nat → Word32 → Option BaseExpr
  | 0, _ => none
  | fuel + 1, index => do
      let (tag, payload) ← readInstruction? context.tables index
      match tag, payload with
      | .exprInt, [low, high] => some (.integerLiteral (signedOfWords low high))
      | .exprParam, [slot] => some (.identifier (NameId.mk slot))
      | .exprBinary, [op, left, right, _] => do
          check (decide (left.val < index.val) && decide (right.val < index.val))
          let operation ← context.ops[op.val]?
          let leftExpression ← expression? context fuel left
          let rightExpression ← expression? context fuel right
          some (operation.expression leftExpression rightExpression)
      | .exprValueBlock, [block, _] => do
          match ← blockWords? context block with
          | [count, statement] => do
              check (count.val == 1 && decide (statement.val < index.val))
              match ← readInstruction? context.tables statement with
              | (.stmtValue, [value]) => do
                  check (decide (value.val < statement.val))
                  expression? context fuel value
              | _ => none
          | _ => none
      | .exprIf, [table, elseRow, _] => do
          match ← blockWords? context table with
          | [count, condition, thenRow] => do
              check (count.val == 1 && decide (condition.val < index.val) &&
                decide (thenRow.val < index.val) && decide (elseRow.val < index.val))
              let conditionExpression ← expression? context fuel condition
              let thenExpression ← expression? context fuel thenRow
              let elseExpression ← expression? context fuel elseRow
              some (.conditional conditionExpression thenExpression elseExpression)
          | _ => none
      | .exprDirectPureCall, [function, arguments, _] => do
          check (function.val != 0)
          let callee ← (← context.callees[function.val - 1]?)
          match ← blockWords? context arguments with
          | count :: entries => do
              check (count.val == callee.parameters)
              let rows ← argumentRows? index entries
              check (rows.length == callee.parameters)
              let values ← rows.mapM (expression? context fuel)
              some (callee.apply values)
          | [] => none
      | .exprOk, [value] => do
          check (decide (value.val < index.val))
          some (.resultOk (← expression? context fuel value))
      | .exprErr, [value, cause] => do
          check (decide (value.val < index.val) && cause.val == 0)
          some (.resultErr (← expression? context fuel value))
      | _, _ => none

/-- An integer row of the typed family that assignments use: a literal, a slot, or an arithmetic
operation on two earlier integer rows. The operation must be arithmetic. -/
def intExpression? (context : MeaningContext) : Nat → Word32 → Option BaseExpr
  | 0, _ => none
  | fuel + 1, index => do
      let (tag, payload) ← readInstruction? context.tables index
      match tag, payload with
      | .intInt, [low, high] => some (.integerLiteral (signedOfWords low high))
      | .intSlot, [slot] => some (.identifier (NameId.mk slot))
      | .intBinary, [op, left, right] => do
          check (decide (left.val < index.val) && decide (right.val < index.val))
          let operation ← context.ops[op.val]?
          check (!operation.isComparison)
          let leftExpression ← intExpression? context fuel left
          let rightExpression ← intExpression? context fuel right
          some (operation.expression leftExpression rightExpression)
      | _, _ => none

/-- The value an `IntInt` row's two payload words hold, which must be a signed 64-bit integer. -/
def literalOf : FragmentTag → List Word32 → Option IntValue
  | .intInt, [low, high] => validateInt? (signedOfWords low high)
  | _, _ => none

/-- The value of an `IntInt` row. -/
def literalValue? (context : MeaningContext) (row : Word32) : Option IntValue :=
  (readInstruction? context.tables row).bind fun instruction => literalOf instruction.1 instruction.2

/-- An assignment of an integer expression to a let slot. -/
structure Assignment where
  slot : Word32
  expression : BaseExpr

/-- A `let` binding under `?`: the target slot, the callee's parameters and body, and the
argument expressions evaluated in the enclosing function. -/
structure Propagation where
  slot : Word32
  parameters : List NameId
  body : BaseExpr
  arguments : List BaseExpr

/-- The expression a `StmtReturn` row returns, which must come from an earlier row. -/
def returnExpression? (context : MeaningContext) (statement : Word32) : Option BaseExpr := do
  match ← readInstruction? context.tables statement with
  | (.stmtReturn, [operand]) => do
      check (decide (operand.val < statement.val))
      expression? context (context.tables.tags.size + 1) operand
  | _ => none

/-- A propagation binding: a `StmtLet` over an `ExprTry` over a direct call, with an absent type
annotation. The call's expression is the callee and its argument expressions. -/
def propagationOf? (context : MeaningContext) (row : Word32) : Option Propagation := do
  let (tag, payload) ← readInstruction? context.tables row
  match tag, payload with
  | .stmtLet, [slot, valueRow, annotation] => do
      check (annotation.val == 0)
      check (decide (valueRow.val < row.val))
      let (valueTag, valuePayload) ← readInstruction? context.tables valueRow
      match valueTag, valuePayload with
      | .exprTry, [callRow, _] => do
          check (decide (callRow.val < valueRow.val))
          match ← expression? context (context.tables.tags.size + 1) callRow with
          | .call parameters body arguments => some ⟨slot, parameters, body, arguments⟩
          | _ => none
      | _, _ => none
  | _, _ => none

/-- One statement of a function body, given the meaning of the statements after it. -/
def bodyStep (context : MeaningContext) (statement : Word32) (rest : List Word32)
    (deeper : Option (List LetBinding × Option Assignment × Option Propagation × BaseExpr)) :
    Option (List LetBinding × Option Assignment × Option Propagation × BaseExpr) :=
  match readInstruction? context.tables statement with
  | none => none
  | some (tag, payload) =>
      match tag, payload, rest with
      | .stmtReturn, [_], [] => do
          let result ← returnExpression? context statement
          some ([], none, none, result)
      | .stmtLet, _, [returned] => do
          let propagation ← propagationOf? context statement
          let result ← returnExpression? context returned
          some ([], none, some propagation, result)
      | .stmtAssignInt, [slot, kind, valueRow, _], [returned] => do
          check (kind.val == 0 && decide (valueRow.val < statement.val))
          let value ← intExpression? context (context.tables.tags.size + 1) valueRow
          let result ← returnExpression? context returned
          some ([], some ⟨slot, value⟩, none, result)
      | .stmtLetInt, [slot, valueRow], _ :: _ => do
          check (decide (valueRow.val < statement.val))
          let value ← intExpression? context (context.tables.tags.size + 1) valueRow
          let (bindings, assignment, propagation, result) ← deeper
          check (!bindings.any fun binding => binding.slot.val == slot.val)
          some (⟨slot, value⟩ :: bindings, assignment, propagation, result)
      | _, _, _ => none

/-- A function body is zero or more `StmtLetInt` bindings of an integer-family value on distinct
slots, at most one `StmtAssignInt` of a plain integer value, then one `StmtReturn` of an
expression; or one propagation binding of a call under `?` followed by the return. Every statement
refers only to earlier rows. -/
def body? (context : MeaningContext) :
    List Word32 → Option (List LetBinding × Option Assignment × Option Propagation × BaseExpr)
  | [] => none
  | statement :: rest => bodyStep context statement rest (body? context rest)

/-- The statements of a function's body block: the block must be owned by the function and
its range must hold a count followed by exactly that many statement rows. -/
def statementRows? (artifact : RawArtifact) (tables : Tables) (function : FunctionRow) :
    Option (List Word32) := do
  let block ← artifact.blocks.find? fun block => block.id == function.body
  check (block.owner == OptionalBridgeId.mk (some function.id))
  let words ← Decode.readPayload? tables ⟨DirectIndex.decode block.start, block.len⟩
  match words with
  | count :: statements => do
      check (count.val == statements.length)
      some statements
  | [] => none

/-- A function as the base machine runs it: its parameter count, its let bindings, at most one
assignment or one propagation binding, and its returned expression. Parameters take the first
slots, and bindings use the slots recorded in their declaration rows. -/
structure FunctionProgram where
  parameters : Nat
  bindings : List LetBinding
  assignment : Option Assignment
  propagation : Option Propagation
  expression : BaseExpr

def RawArtifact.context (artifact : RawArtifact) (callees : List (Option Callee)) :
    MeaningContext :=
  ⟨artifact.tables, artifact.ops, artifact.blocks, callees⟩

def functionProgram? (artifact : RawArtifact) (callees : List (Option Callee))
    (function : FunctionRow) : Option FunctionProgram := do
  let statements ← statementRows? artifact artifact.tables function
  let (bindings, assignment, propagation, expression) ← body? (artifact.context callees) statements
  pure ⟨function.paramsLen.val, bindings, assignment, propagation, expression⟩

/-- An assigned literal binding is evaluated as a call argument before the returned expression.
The initial literal is substituted in the assigned value; the assigned value itself remains an
expression, so an arithmetic failure is preserved even when the return does not read its slot. -/
def assignedCallee? (program : FunctionProgram) (assignment : Assignment) : Option Callee :=
  if program.parameters == 0 then
    match program.bindings with
    | [binding] =>
      if binding.slot == assignment.slot then
        binding.literalInt?.map fun initial =>
          ⟨program.parameters, program.expression,
            [⟨assignment.slot, substitute [(NameId.mk binding.slot, initial)] assignment.expression⟩]⟩
      else none
    | _ => none
  else none

/-- The callee form of a function without propagation. Literal bindings are substituted, computed
bindings are carried as call arguments, and a nullary function assigning its one literal binding
carries the assigned value as its call argument. -/
def FunctionProgram.callee? (program : FunctionProgram) : Option Callee :=
  if program.propagation.isNone then
    match program.assignment with
    | some assignment => assignedCallee? program assignment
    | none =>
      if program.bindings.all (fun binding => binding.literalInt?.isSome) then
        some ⟨program.parameters,
          substitute (program.bindings.filterMap fun binding =>
            binding.literalInt?.map fun value => (NameId.mk binding.slot, value))
            program.expression, []⟩
      else
        some ⟨program.parameters, program.expression, program.bindings⟩
  else none

theorem assigned_literal_function_is_callable (slot : Word32) (initial : IntValue)
    (assigned returned : BaseExpr)
    (valid : (⟨slot, .integerLiteral initial.val⟩ : LetBinding).literalInt? = some initial) :
    (⟨0, [⟨slot, .integerLiteral initial.val⟩], some ⟨slot, assigned⟩, none,
      returned⟩ : FunctionProgram).callee? =
      some ⟨0, returned, [⟨slot, substitute [(NameId.mk slot, initial)] assigned⟩]⟩ := by
  simp [FunctionProgram.callee?, assignedCallee?, valid]

/-- The programs of the functions in table order; a call may name only a function before the
caller, so a function reads against the callees of the functions already read. -/
def programStep (artifact : RawArtifact) (programs : List (Option FunctionProgram))
    (function : FunctionRow) : List (Option FunctionProgram) :=
  programs ++ [functionProgram? artifact (programs.map fun program => program.bind (·.callee?))
    function]

def functionPrograms (artifact : RawArtifact) : List (Option FunctionProgram) :=
  artifact.functions.foldl (programStep artifact) []

def RawArtifact.allCallees (artifact : RawArtifact) : List (Option Callee) :=
  (functionPrograms artifact).map fun program => program.bind (·.callee?)

def programOf? (artifact : RawArtifact) (function : FunctionRow) : Option FunctionProgram :=
  if function.id.raw.val == 0 then none
  else ((functionPrograms artifact)[function.id.raw.val - 1]?).bind id

/-- Each let slot is a lexical name, a binder and a cell of the single activation, bound to an
immutable integer. Reading a slot no statement binds faults. -/
def machineOf (bindings : List LetBinding) (expression : BaseExpr) : MachineConfig :=
  initialConfig
    (bindings.foldl
      (fun names binding => bind names (NameId.mk binding.slot) (BinderId.mk binding.slot))
      emptyLexicalEnvironment)
    (bindings.foldl
      (fun activations binding =>
        bindCell activations (BinderId.mk binding.slot) ⟨0, BinderId.mk binding.slot⟩)
      [])
    (fun cell =>
      (bindings.find? fun binding => decide (cell = ⟨0, BinderId.mk binding.slot⟩)).bind
        fun binding => binding.literalInt?.map fun value => ⟨typedInteger value, .immutable⟩)
    expression

/-- A function without parameters runs as itself; a function with parameters runs as a call of
its body on the integer arguments. -/
def FunctionProgram.withArguments (program : FunctionProgram) (expression : BaseExpr)
    (arguments : List Int) : BaseExpr :=
  if program.parameters == 0 then expression
  else .call (parameterNames program.parameters) expression
    (arguments.map BaseExpr.integerLiteral)

def FunctionProgram.expressionAt (program : FunctionProgram) (arguments : List Int) : BaseExpr :=
  program.withArguments program.expression arguments

/-- The base machine run of a function body: one call binds the parameters and every let binding,
arguments in order, then the body. A binding's value has the parameters replaced by the argument
expressions and every earlier binding's slot replaced by that binding's applied value, which are
pure parameter reads or literals in the admitted programs. -/
def FunctionProgram.machineWith (program : FunctionProgram) (bindings : List LetBinding)
    (arguments : List Int) : MachineConfig :=
  let parameters := parameterNames program.parameters
  let values := arguments.map BaseExpr.integerLiteral
  let appliedBindings := applyLetBindings (parameters.zip values) bindings
  let applied :=
    if parameters.isEmpty && bindings.isEmpty then program.expression
    else .call (parameters ++ bindings.map fun binding => NameId.mk binding.slot)
      program.expression (values ++ appliedBindings)
  machineOf [] applied

def runBudget : Nat := 128

/-- The call a propagation evaluates: the callee body applied to the argument expressions. -/
def Propagation.call (propagation : Propagation) : BaseExpr :=
  .call propagation.parameters propagation.body propagation.arguments

/-- What a propagation does: `Ok` continues with the payload bound to its slot, `Err` ends the
enclosing function with the same payload, and a specified arithmetic failure of the callee or its
arguments ends it with that failure. -/
inductive PropagationOutcome where
  | continued (bindings : List LetBinding)
  | returned (payload : IntValue)
  | failed (kind : FragmentFailure)

def Propagation.run? (propagation : Propagation) (program : FunctionProgram)
    (bindings : List LetBinding) (arguments : List Int) : Option PropagationOutcome :=
  if bindings.any (fun binding => binding.slot == propagation.slot) then none
  else
    match runFuel runBudget
        (machineOf bindings (program.withArguments propagation.call arguments)) with
    | .completed (.value final) =>
        match resultPayload? final with
        | some (true, payload) =>
            some (.continued (bindings ++
              [⟨propagation.slot, .integerLiteral payload.val⟩]))
        | some (false, payload) => some (.returned payload)
        | none => none
    | .completed (.fault (.arithmetic .divisionByZero)) => some (.failed .divisionByZero)
    | .completed (.fault (.arithmetic .overflow)) => some (.failed .integerOverflow)
    | _ => none

/-- The bindings after an assignment: its right-hand side runs on the base machine in the store as
it stands, and the target slot takes the integer the run completes with. An arithmetic fault ends
the function with that failure. A right-hand side that completes with anything else, and a target
no binding holds, have no settled bindings. -/
def Assignment.settle? (assignment : Assignment) (bindings : List LetBinding) :
    Option (Except FragmentFailure (List LetBinding)) :=
  if bindings.any (fun binding => binding.slot == assignment.slot) then
    match runFuel runBudget (machineOf bindings assignment.expression) with
    | .completed (.value value) =>
        (integerPayload? value).map fun result =>
          .ok (bindings.map fun binding =>
            if binding.slot == assignment.slot then
              ⟨binding.slot, .integerLiteral result.val⟩
            else binding)
    | .completed (.fault (.arithmetic .divisionByZero)) => some (.error .divisionByZero)
    | .completed (.fault (.arithmetic .overflow)) => some (.error .integerOverflow)
    | _ => none
  else none

/-- The bindings the returned expression runs against: the declared ones, or those the function's
one assignment leaves. -/
def FunctionProgram.settled? (program : FunctionProgram) :
    Option (Except FragmentFailure (List LetBinding)) :=
  match program.assignment with
  | none => some (.ok program.bindings)
  | some assignment => assignment.settle? program.bindings

/-- The outcome of the returned expression run against the given bindings: an integer, a boolean,
a `Result` constructor, or one of the two specified arithmetic failures. Other faults, host
requests and fuel exhaustion are not outcomes of this subset. -/
def FunctionProgram.continuationOutcome? (program : FunctionProgram)
    (bindings : List LetBinding) (arguments : List Int) : Option FunctionOutcome :=
  match runFuel runBudget (program.machineWith bindings arguments) with
  | .completed (.value value) =>
      match integerPayload? value with
      | some result => some (.value result.val)
      | none =>
          match booleanPayload? value with
          | some truth => some (.truth truth)
          | none => (resultPayload? value).map fun payload =>
              FunctionOutcome.constructed payload.1 payload.2.val
  | .completed (.fault (.arithmetic .divisionByZero)) => some (.failure .divisionByZero)
  | .completed (.fault (.arithmetic .overflow)) => some (.failure .integerOverflow)
  | _ => none

/-- The outcome the base machine completes with: an integer, a boolean, a `Result` constructor, or
one of the two specified arithmetic failures, raised by the assignment, by a propagation's callee,
or by the returned expression. A propagation whose callee completes with `Ok` continues with the
payload bound; one whose callee completes with `Err` ends the function with the same payload.
Other faults, host requests and fuel exhaustion are not outcomes of this subset, and neither is a
call with the wrong number of arguments. -/
def FunctionProgram.outcome? (program : FunctionProgram) (arguments : List Int) :
    Option FunctionOutcome :=
  if arguments.length == program.parameters then
    match program.settled? with
    | some (.ok bindings) =>
        match program.propagation with
        | none => program.continuationOutcome? bindings arguments
        | some propagation =>
            match propagation.run? program bindings arguments with
            | some (.continued extended) => program.continuationOutcome? extended arguments
            | some (.returned payload) => some (.constructed false payload.val)
            | some (.failed kind) => some (.failure kind)
            | none => none
    | some (.error kind) => some (.failure kind)
    | none => none
  else
    none

def functionOutcome? (artifact : RawArtifact) (function : FunctionRow) (arguments : List Int) :
    Option FunctionOutcome :=
  (programOf? artifact function).bind fun program => program.outcome? arguments

def resultBits (value : Int) : Nat := (value % 18446744073709551616).toNat

/-- A recorded outcome agrees with a meaning when both are the same failure, or both are values
and the record holds that integer's 64-bit pattern, or both are booleans and the record holds one
bit (`1` for true), or both are the same constructor and the record holds the payload's 64-bit
pattern (`ok` for `Ok`, `err` for `Err`). -/
def outcomeRecorded : FunctionOutcome → ObservedOutcome → Bool
  | .value result, .value bits => resultBits result == bits.val
  | .truth result, .value bits => (if result then 1 else 0) == bits.val
  | .constructed isOk result, .ok bits => isOk && resultBits result == bits.val
  | .constructed isOk result, .err bits => !isOk && resultBits result == bits.val
  | .failure kind, .failure recorded => kind == recorded
  | _, _ => false

def idsInOrder (ids : List BridgeId) : Bool :=
  ((List.range ids.length).zip ids).all fun (index, id) => BridgeId.fromIndex? index == some id

def rowWellFormed (artifact : RawArtifact) (tables : Tables) (index : Nat) : Bool :=
  match Word32.decode? index with
  | none => false
  | some word =>
      match readInstruction? tables word with
      | none => false
      | some (tag, payload) =>
          let context := artifact.context artifact.allCallees
          let expressionAt : Word32 → Bool := fun row =>
            (expression? context (tables.tags.size + 1) row).isSome
          match tag, payload with
          | .intInt, _ => true
          | .exprInt, _ => true
          | .exprParam, _ => true
          | .exprBinary, _ => expressionAt word
          | .exprValueBlock, _ => expressionAt word
          | .exprIf, _ => expressionAt word
          | .exprDirectPureCall, _ => expressionAt word
          | .exprOk, _ => expressionAt word
          | .exprErr, _ => expressionAt word
          | .intSlot, _ => true
          | .intBinary, _ => (intExpression? context (tables.tags.size + 1) word).isSome
          | .stmtAssignInt, [_, kind, valueRow, _] =>
              kind.val == 0 && decide (valueRow.val < index) &&
                (intExpression? context (tables.tags.size + 1) valueRow).isSome
          | .stmtReturn, [operand] => decide (operand.val < index) && expressionAt operand
          | .stmtValue, [operand] => decide (operand.val < index) && expressionAt operand
          | .stmtLetInt, [_, valueRow] =>
              decide (valueRow.val < index) &&
                (intExpression? context (tables.tags.size + 1) valueRow).isSome
          | .exprTry, [value, _] =>
              decide (value.val < index) &&
                match readInstruction? tables value with
                | some (.exprDirectPureCall, _) => true
                | _ => false
          | .stmtLet, _ => (propagationOf? context word).isSome
          | _, _ => false

/-- The blocks the rows and function bodies name: a body, a value block, a branch table or an
argument table. Every block is named exactly once and every name is a block. -/
def blockReferences (artifact : RawArtifact) : List Word32 :=
  artifact.functions.map (fun function => function.body.raw) ++
    (List.range artifact.rows.length).flatMap fun index =>
      match (Word32.decode? index).bind (readInstruction? artifact.tables) with
      | some (.exprValueBlock, [block, _]) => [block]
      | some (.exprIf, [block, _, _]) => [block]
      | some (.exprDirectPureCall, [_, block, _]) => [block]
      | _ => []

def blocksAreNamedOnce (artifact : RawArtifact) : Bool :=
  let references := blockReferences artifact
  artifact.blocks.all (fun block => references.count block.id.raw == 1) &&
    references.all fun reference => artifact.blocks.any fun block => block.id.raw == reference

/-- Every block belongs to a function and every function owns its body block. -/
def blocksAreOwned (artifact : RawArtifact) : Bool :=
  artifact.blocks.all (fun block =>
    artifact.functions.any fun function => block.owner == OptionalBridgeId.mk (some function.id)) &&
  artifact.functions.all fun function =>
    artifact.blocks.any fun block =>
      block.id == function.body && block.owner == OptionalBridgeId.mk (some function.id)

/-- The artifact validator: ordered identifiers, every row well formed, every block named once
and owned by a function, every function with a program (and, without parameters, a defined
outcome), and every recorded observation, in order, equal to the outcome of the function it
names on the arguments it records. -/
def validateArtifact (artifact : RawArtifact) : Bool :=
  idsInOrder (artifact.rows.map (·.id)) &&
  idsInOrder (artifact.blocks.map (·.id)) &&
  idsInOrder (artifact.functions.map (·.id)) &&
  (List.range artifact.rows.length).all (rowWellFormed artifact artifact.tables) &&
  blocksAreNamedOnce artifact &&
  blocksAreOwned artifact &&
  artifact.functions.all (fun function =>
    !function.name.isEmpty && (programOf? artifact function).isSome &&
      (function.paramsLen.val != 0 || (functionOutcome? artifact function []).isSome)) &&
  artifact.observations.all fun observation =>
    artifact.functions.any fun function =>
      function.id == observation.function &&
        observation.arguments.length == function.paramsLen.val &&
        (functionOutcome? artifact function observation.integerArguments).any fun outcome =>
          outcomeRecorded outcome observation.outcome

/-- A recorded call and the outcome its function's IR denotes on the recorded arguments. -/
structure ObservedResult where
  function : String
  arguments : List Int
  outcome : FunctionOutcome
  deriving DecidableEq

def namedResults (artifact : RawArtifact) : Option (List ObservedResult) :=
  artifact.observations.mapM fun observation => do
    let function ← artifact.functions.find? fun function => function.id == observation.function
    let outcome ← functionOutcome? artifact function observation.integerArguments
    pure ⟨function.name, observation.integerArguments, outcome⟩

/-- Decode both documents, require the raw-IR document to name the source document, validate
the artifact, and report each observed call with the outcome its IR denotes. -/
def certify (sourceText rawText : ExchangeCodes) : Option (List ObservedResult) := do
  let source ← decodeSourceDocument sourceText
  let raw ← decodeRawDocument rawText
  if raw.sourceId = source.id then pure () else none
  if validateArtifact raw then pure () else none
  namedResults raw

theorem certify_sound {sourceText rawText : ExchangeCodes}
    {results : List ObservedResult}
    (certified : certify sourceText rawText = some results) :
    ∃ source raw, decodeSourceDocument sourceText = some source ∧
      decodeRawDocument rawText = some raw ∧ raw.sourceId = source.id ∧
      validateArtifact raw = true ∧ namedResults raw = some results := by
  unfold certify at certified
  cases sourceDecoded : decodeSourceDocument sourceText with
  | none => simp [sourceDecoded] at certified
  | some source =>
      cases rawDecoded : decodeRawDocument rawText with
      | none => simp [sourceDecoded, rawDecoded] at certified
      | some raw =>
          by_cases associated : raw.sourceId = source.id
          · by_cases valid : validateArtifact raw = true
            · refine ⟨source, raw, rfl, rfl, associated, valid, ?_⟩
              simpa [sourceDecoded, rawDecoded, associated, valid] using certified
            · simp [sourceDecoded, rawDecoded, associated, valid] at certified
          · simp [sourceDecoded, rawDecoded, associated] at certified

theorem settle_error_is_a_machine_fault {assignment : Assignment} {bindings : List LetBinding}
    {kind : FragmentFailure}
    (settled : assignment.settle? bindings = some (.error kind)) :
    runFuel runBudget (machineOf bindings assignment.expression) =
      .completed (.fault (.arithmetic kind.fault)) := by
  unfold Assignment.settle? at settled
  split at settled
  · cases run : runFuel runBudget (machineOf bindings assignment.expression) with
    | completed completed =>
        rw [run] at settled
        cases completed with
        | value final => cases payload : integerPayload? final <;> simp [payload] at settled
        | fault fault =>
            cases fault <;> try (simp at settled; done)
            rename_i arithmetic
            cases arithmetic with
            | overflow =>
                simp only [Option.some.injEq, Except.error.injEq] at settled
                subst settled; rfl
            | divisionByZero =>
                simp only [Option.some.injEq, Except.error.injEq] at settled
                subst settled; rfl
    | hostRequest waiting => rw [run] at settled; simp at settled
    | outOfFuel => rw [run] at settled; simp at settled
    | internalStuck => rw [run] at settled; simp at settled
  · simp at settled

/-- A continuation outcome is exactly a completed base machine run of the returned expression: an
integer, boolean or `Result` value, or one of the two specified arithmetic faults. -/
theorem continuation_outcome_is_a_completed_machine_run {program : FunctionProgram}
    {bindings : List LetBinding} {arguments : List Int} {outcome : FunctionOutcome}
    (result : program.continuationOutcome? bindings arguments = some outcome) :
    (∃ final value,
      runFuel runBudget (program.machineWith bindings arguments) = .completed (.value final) ∧
      integerPayload? final = some value ∧ outcome = .value value.val) ∨
    (∃ final truth,
      runFuel runBudget (program.machineWith bindings arguments) = .completed (.value final) ∧
      booleanPayload? final = some truth ∧ outcome = .truth truth) ∨
    (∃ final isOk payload,
      runFuel runBudget (program.machineWith bindings arguments) = .completed (.value final) ∧
      resultPayload? final = some (isOk, payload) ∧
      outcome = .constructed isOk payload.val) ∨
    (∃ kind,
      runFuel runBudget (program.machineWith bindings arguments) =
        .completed (.fault (.arithmetic kind.fault)) ∧ outcome = .failure kind) := by
  unfold FunctionProgram.continuationOutcome? at result
  cases run : runFuel runBudget (program.machineWith bindings arguments) with
  | completed completed =>
      rw [run] at result
      cases completed with
      | value final =>
          cases payload : integerPayload? final with
          | some value =>
              simp only [payload, Option.map_some, Option.some.injEq] at result
              exact .inl ⟨final, value, rfl, payload, result.symm⟩
          | none =>
              cases truth : booleanPayload? final with
              | some value =>
                  simp only [payload, truth, Option.map_some, Option.some.injEq] at result
                  exact .inr (.inl ⟨final, value, rfl, truth, result.symm⟩)
              | none =>
                  cases constructed : resultPayload? final with
                  | none => simp [payload, truth, constructed] at result
                  | some pair =>
                      obtain ⟨isOk, value⟩ := pair
                      simp only [payload, truth, constructed, Option.map_some,
                        Option.some.injEq] at result
                      exact .inr (.inr (.inl ⟨final, isOk, value, rfl, constructed, result.symm⟩))
      | fault fault =>
          cases fault <;> try (simp at result; done)
          rename_i arithmetic
          cases arithmetic with
          | overflow =>
              simp only [Option.some.injEq] at result
              exact .inr (.inr (.inr ⟨.integerOverflow, rfl, result.symm⟩))
          | divisionByZero =>
              simp only [Option.some.injEq] at result
              exact .inr (.inr (.inr ⟨.divisionByZero, rfl, result.symm⟩))
  | hostRequest waiting => rw [run] at result; simp at result
  | outOfFuel => rw [run] at result; simp at result
  | internalStuck => rw [run] at result; simp at result

/-- The three ways a propagation can end: the callee completes with `Ok` and the payload is bound
to its slot, completes with `Err` and that payload ends the function, or raises one of the two
specified arithmetic faults. -/
theorem propagation_run_cases {propagation : Propagation} {program : FunctionProgram}
    {bindings : List LetBinding} {arguments : List Int} {outcome : PropagationOutcome}
    (run : propagation.run? program bindings arguments = some outcome) :
    (∃ final payload,
      runFuel runBudget (machineOf bindings (program.withArguments propagation.call arguments)) =
        .completed (.value final) ∧ resultPayload? final = some (true, payload) ∧
      outcome = .continued (bindings ++ [⟨propagation.slot, .integerLiteral payload.val⟩])) ∨
    (∃ final payload,
      runFuel runBudget (machineOf bindings (program.withArguments propagation.call arguments)) =
        .completed (.value final) ∧ resultPayload? final = some (false, payload) ∧
      outcome = .returned payload) ∨
    (∃ kind,
      runFuel runBudget (machineOf bindings (program.withArguments propagation.call arguments)) =
        .completed (.fault (.arithmetic kind.fault)) ∧
      outcome = .failed kind) := by
  by_cases guard : (bindings.any fun binding => binding.slot == propagation.slot) = true
  · simp [Propagation.run?, guard] at run
  · have guardFalse : (bindings.any fun binding => binding.slot == propagation.slot) = false := by
      cases hb : (bindings.any fun binding => binding.slot == propagation.slot) with
      | false => rfl
      | true => exact absurd hb guard
    cases h : runFuel runBudget
        (machineOf bindings (program.withArguments propagation.call arguments)) with
    | completed completed =>
        cases completed with
        | value final =>
            cases payload : resultPayload? final with
            | none => simp [Propagation.run?, guardFalse, h, payload] at run
            | some pair =>
                obtain ⟨isOk, value⟩ := pair
                cases isOk
                · have runValue : propagation.run? program bindings arguments =
                      some (.returned value) := by
                    simp [Propagation.run?, guardFalse, h, payload]
                  rw [runValue] at run
                  have outcomeIs : outcome = .returned value := by simpa using run.symm
                  exact .inr (.inl ⟨final, value, rfl, payload, outcomeIs⟩)
                · have runValue : propagation.run? program bindings arguments =
                      some (.continued (bindings ++ [⟨propagation.slot, .integerLiteral value.val⟩])) := by
                    simp [Propagation.run?, guardFalse, h, payload]
                  rw [runValue] at run
                  have outcomeIs : outcome =
                      .continued (bindings ++ [⟨propagation.slot, .integerLiteral value.val⟩]) := by
                    simpa using run.symm
                  exact .inl ⟨final, value, rfl, payload, outcomeIs⟩
        | fault fault =>
            cases fault <;> try (simp [Propagation.run?, guardFalse, h] at run)
            rename_i arithmetic
            cases arithmetic
            · have outcomeIs : outcome = .failed .integerOverflow := by simpa using run.symm
              exact .inr (.inr ⟨.integerOverflow, rfl, outcomeIs⟩)
            · have outcomeIs : outcome = .failed .divisionByZero := by simpa using run.symm
              exact .inr (.inr ⟨.divisionByZero, rfl, outcomeIs⟩)
    | hostRequest waiting => simp [Propagation.run?, guardFalse, h] at run
    | outOfFuel => simp [Propagation.run?, guardFalse, h] at run
    | internalStuck => simp [Propagation.run?, guardFalse, h] at run

/-- A program without propagation has exactly a completed base machine run as its outcome: an
integer, boolean or `Result` value, or one of the two specified arithmetic faults of the returned
expression, run against the settled bindings, or the same fault raised by the assignment's
right-hand side. -/
theorem program_outcome_is_a_completed_machine_run {program : FunctionProgram}
    {arguments : List Int} {outcome : FunctionOutcome}
    (plain : program.propagation = none)
    (result : program.outcome? arguments = some outcome) :
    (∃ bindings, program.settled? = some (.ok bindings) ∧
      ((∃ final value,
          runFuel runBudget (program.machineWith bindings arguments) =
            .completed (.value final) ∧
          integerPayload? final = some value ∧ outcome = .value value.val) ∨
       (∃ final truth,
          runFuel runBudget (program.machineWith bindings arguments) =
            .completed (.value final) ∧
          booleanPayload? final = some truth ∧ outcome = .truth truth) ∨
       (∃ final isOk payload,
          runFuel runBudget (program.machineWith bindings arguments) =
            .completed (.value final) ∧
          resultPayload? final = some (isOk, payload) ∧
          outcome = .constructed isOk payload.val) ∨
       (∃ kind,
          runFuel runBudget (program.machineWith bindings arguments) =
            .completed (.fault (.arithmetic kind.fault)) ∧ outcome = .failure kind))) ∨
    (∃ assignment kind, program.assignment = some assignment ∧
      runFuel runBudget (machineOf program.bindings assignment.expression) =
        .completed (.fault (.arithmetic kind.fault)) ∧ outcome = .failure kind) := by
  unfold FunctionProgram.outcome? at result
  split at result
  · cases settled : program.settled? with
    | none => simp [settled] at result
    | some settledValue =>
      cases settledValue with
      | error kind =>
          rw [settled] at result
          dsimp only at result
          have outcomeIs : outcome = .failure kind := by simpa using result.symm
          cases assignmentCase : program.assignment with
          | none => simp [FunctionProgram.settled?, assignmentCase] at settled
          | some assignment =>
              have faultRun := settle_error_is_a_machine_fault (by
                simpa [FunctionProgram.settled?, assignmentCase] using settled)
              exact .inr ⟨assignment, kind, rfl, faultRun, outcomeIs⟩
      | ok bindings =>
          rw [settled] at result
          dsimp only at result
          rw [plain] at result
          exact .inl ⟨bindings, rfl,
            continuation_outcome_is_a_completed_machine_run result⟩
  · simp at result

/-- A program with a propagation has exactly one of its runs as its outcome: the callee completes
with `Err` and that payload ends the function, the callee raises a specified arithmetic fault, or
the callee completes with `Ok`, the payload is bound, and the returned expression's continuation
outcome follows. An assignment fault still ends the function first. -/
theorem propagated_program_outcome_is_a_completed_machine_run {program : FunctionProgram}
    {arguments : List Int} {outcome : FunctionOutcome} {propagation : Propagation}
    (propagated : program.propagation = some propagation)
    (result : program.outcome? arguments = some outcome) :
    (∃ kind, program.settled? = some (.error kind) ∧ outcome = .failure kind) ∨
    (∃ bindings, program.settled? = some (.ok bindings) ∧
      ((∃ final payload,
          runFuel runBudget
              (machineOf bindings (program.withArguments propagation.call arguments)) =
            .completed (.value final) ∧ resultPayload? final = some (false, payload) ∧
          outcome = .constructed false payload.val) ∨
       (∃ kind,
          runFuel runBudget
              (machineOf bindings (program.withArguments propagation.call arguments)) =
            .completed (.fault (.arithmetic kind.fault)) ∧ outcome = .failure kind) ∨
       (∃ extended final payload,
          runFuel runBudget
              (machineOf bindings (program.withArguments propagation.call arguments)) =
            .completed (.value final) ∧ resultPayload? final = some (true, payload) ∧
          extended = bindings ++ [⟨propagation.slot, .integerLiteral payload.val⟩] ∧
          program.continuationOutcome? extended arguments = some outcome))) := by
  unfold FunctionProgram.outcome? at result
  split at result
  · cases settled : program.settled? with
    | none => simp [settled] at result
    | some settledValue =>
      cases settledValue with
      | error kind =>
          rw [settled] at result
          dsimp only at result
          have outcomeIs : outcome = .failure kind := by simpa using result.symm
          exact .inl ⟨kind, rfl, outcomeIs⟩
      | ok bindings =>
          rw [settled] at result
          dsimp only at result
          rw [propagated] at result
          dsimp only at result
          cases runPropagation : propagation.run? program bindings arguments with
          | none => simp [runPropagation] at result
          | some propOutcome =>
              rcases propagation_run_cases runPropagation with
                ⟨final, payload, run, payloadEq, outcomeEq⟩ |
                ⟨final, payload, run, payloadEq, outcomeEq⟩ |
                ⟨kind, run, outcomeEq⟩
              · rw [runPropagation, outcomeEq] at result
                dsimp only at result
                exact .inr ⟨bindings, rfl, .inr (.inr
                  ⟨bindings ++ [⟨propagation.slot, .integerLiteral payload.val⟩], final, payload, run, payloadEq, rfl,
                    result⟩)⟩
              · rw [runPropagation, outcomeEq] at result
                dsimp only at result
                have outcomeIs : outcome = .constructed false payload.val := by
                  simpa using result.symm
                exact .inr ⟨bindings, rfl, .inl ⟨final, payload, run, payloadEq, outcomeIs⟩⟩
              · rw [runPropagation, outcomeEq] at result
                dsimp only at result
                have outcomeIs : outcome = .failure kind := by
                  simpa using result.symm
                exact .inr ⟨bindings, rfl, .inr (.inl ⟨kind, run, outcomeIs⟩)⟩
  · simp at result

/-- Every recorded observation of a validated artifact agrees with the outcome of a completed
base machine run of the named function's IR meaning on the recorded arguments. -/
theorem validated_observation_is_a_machine_run {artifact : RawArtifact}
    (valid : validateArtifact artifact = true) {observation : Observation}
    (recorded : observation ∈ artifact.observations) :
    ∃ function ∈ artifact.functions, function.id = observation.function ∧
      ∃ program outcome, programOf? artifact function = some program ∧
        program.outcome? observation.integerArguments = some outcome ∧
        outcomeRecorded outcome observation.outcome = true := by
  unfold validateArtifact at valid
  simp only [Bool.and_eq_true, List.all_eq_true] at valid
  have observed := valid.2 observation recorded
  simp only [List.any_eq_true, Bool.and_eq_true, beq_iff_eq] at observed
  obtain ⟨function, member, ⟨sameId, _⟩, sameOutcome⟩ := observed
  refine ⟨function, member, sameId, ?_⟩
  unfold functionOutcome? at sameOutcome
  cases program : programOf? artifact function with
  | none => simp [program] at sameOutcome
  | some found =>
      rw [program] at sameOutcome
      simp only [Option.bind_some, Option.any_eq_true] at sameOutcome
      obtain ⟨outcome, result, agrees⟩ := sameOutcome
      exact ⟨found, outcome, rfl, result, agrees⟩

/-- A recorded outcome does not depend on the fuel budget: a run of the returned expression that
settles within the budget gives the same answer under any larger budget, failures included. A
propagation runs the returned expression only after its callee, so this statement covers the
programs without one. -/
theorem machine_run_outcome_is_independent_of_extra_fuel
    {program : FunctionProgram} {arguments : List Int} {outcome : FunctionOutcome}
    {bindings : List LetBinding}
    (plain : program.propagation = none)
    (settledBindings : program.settled? = some (.ok bindings))
    (settled : program.outcome? arguments = some outcome) (extra : Nat) :
    runFuel (runBudget + extra) (program.machineWith bindings arguments) =
      runFuel runBudget (program.machineWith bindings arguments) := by
  have notExhausted : runFuel runBudget (program.machineWith bindings arguments) ≠ .outOfFuel := by
    intro exhausted
    unfold FunctionProgram.outcome? at settled
    split at settled
    · rw [settledBindings, plain] at settled
      dsimp only at settled
      unfold FunctionProgram.continuationOutcome? at settled
      rw [exhausted] at settled
      simp at settled
    · simp at settled
  exact Xsh.Proofs.Base.Runner.base_runner.2.2.2.2.2.2.1 runBudget
    (program.machineWith bindings arguments) notExhausted extra

def sequenceIntegerResults (operator : IntegerBinaryOp) : IntegerResult → IntegerResult → IntegerResult
  | .fault fault, _ => .fault fault
  | .value _, .fault fault => .fault fault
  | .value left, .value right => applyBinary operator left right

/-- Evaluation of pure integer literals, typed local reads and arithmetic. Both operand
expressions belong to this grammar; the combined result retains the left operand's first fault. -/
inductive PureIntegerEvaluation (reads : NameId → Option IntValue) :
    BaseExpr → IntegerResult → Prop where
  | literal (value : Int) : PureIntegerEvaluation reads (.integerLiteral value) (checkedInt value)
  | read {name : NameId} {value : IntValue} (found : reads name = some value) :
      PureIntegerEvaluation reads (.identifier name) (.value value)
  | binary {left right : BaseExpr} {leftValue rightValue : IntegerResult}
      (operator : IntegerBinaryOp) (leftRuns : PureIntegerEvaluation reads left leftValue)
      (rightRuns : PureIntegerEvaluation reads right rightValue) :
      PureIntegerEvaluation reads (.binary operator left right)
        (sequenceIntegerResults operator leftValue rightValue)

/-- A transition bound for expressions admitted by `PureIntegerEvaluation`. -/
def pureIntegerCost : BaseExpr → Nat
  | .binary _ left right => pureIntegerCost left + pureIntegerCost right + 3
  | _ => 1

theorem machine_steps_append {a b : Nat} {start middle finish : MachineConfig}
    (before : MachineSteps a start middle) (after : MachineSteps b middle finish) :
    MachineSteps (a + b) start finish := by
  induction before with
  | refl _ => simpa using after
  | step first rest ih =>
      simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using MachineSteps.step first (ih after)

theorem checked_integer_payload (value : IntValue) : checkedInt value.val = .value value := by
  simp [checkedInt, validateInt?, value.property]

theorem pure_integer_evaluation_substitute {values : List (NameId × IntValue)}
    {expression : BaseExpr} {result : IntegerResult}
    (evaluated : PureIntegerEvaluation (values.lookup ·) expression result) :
    PureIntegerEvaluation (fun _ => none) (substitute values expression) result := by
  induction evaluated with
  | literal value => exact .literal value
  | @read name value found =>
      simpa [substitute, found, checked_integer_payload] using
        (PureIntegerEvaluation.literal (reads := fun _ => none) value.val)
  | binary operator left right ihLeft ihRight =>
      exact .binary operator ihLeft ihRight

theorem pure_integer_evaluation_steps {reads : NameId → Option IntValue}
    {expression : BaseExpr} {result : IntegerResult}
    (evaluated : PureIntegerEvaluation reads expression result) (environment : MachineConfig)
    (agrees : ∀ name value, reads name = some value →
      lookupBaseValue environment.names environment.activations environment.store name =
        .ok (typedInteger value)) (continuation : List BaseFrame) :
    ∃ steps rest, steps ≤ pureIntegerCost expression ∧
      MachineSteps steps (updateState environment (.evaluate expression) continuation)
        (updateState environment (controlOfIntegerResult result) rest) ∧
      (∀ value, result = .value value → rest = continuation) := by
  induction evaluated generalizing continuation with
  | literal value =>
      refine ⟨1, continuation, by simp [pureIntegerCost], .step ?_ (.refl _), ?_⟩
      · rfl
      · intro _ _; rfl
  | read found =>
      refine ⟨1, continuation, by simp [pureIntegerCost], .step ?_ (.refl _), ?_⟩
      · simp [step, updateState, agrees _ _ found, controlOfIntegerResult]
      · intro _ _; rfl
  | @binary left right leftValue rightValue operator leftRuns rightRuns ihLeft ihRight =>
      obtain ⟨leftSteps, leftRest, leftBound, leftPath, leftStack⟩ :=
        ihLeft (.binaryLeft operator right :: continuation)
      have enter : step (updateState environment (.evaluate (.binary operator left right)) continuation) =
          some (updateState environment (.evaluate left) (.binaryLeft operator right :: continuation)) := rfl
      cases leftValue with
      | fault fault =>
          refine ⟨leftSteps + 1, leftRest, ?_, .step enter leftPath, ?_⟩
          · simp only [pureIntegerCost]; omega
          · intro value impossible; cases impossible
      | value leftValue =>
          have sameLeft := leftStack leftValue rfl
          subst leftRest
          obtain ⟨rightSteps, rightRest, rightBound, rightPath, rightStack⟩ :=
            ihRight (.binaryRight operator leftValue :: continuation)
          have next : step (updateState environment (controlOfIntegerResult (.value leftValue))
              (.binaryLeft operator right :: continuation)) =
              some (updateState environment (.evaluate right) (.binaryRight operator leftValue :: continuation)) := by
            rfl
          have combinedPath := machine_steps_append (.step enter leftPath) (.step next rightPath)
          cases rightValue with
          | fault fault =>
              refine ⟨_, rightRest, ?_, combinedPath, ?_⟩
              · simp only [pureIntegerCost]; omega
              · intro value impossible; cases impossible
          | value rightValue =>
              have sameRight := rightStack rightValue rfl
              subst rightRest
              have finish : step (updateState environment (controlOfIntegerResult (.value rightValue))
                  (.binaryRight operator leftValue :: continuation)) =
                  some (updateState environment (controlOfIntegerResult (applyBinary operator leftValue rightValue))
                    continuation) := rfl
              have path := machine_steps_append combinedPath (.step finish (.refl _))
              refine ⟨_, continuation, ?_, path, ?_⟩
              · simp only [pureIntegerCost]; omega
              · intro _ _; rfl

def integerBaseOutcome : IntegerResult → BaseOutcome
  | .value value => .value (typedInteger value)
  | .fault fault => .fault (.arithmetic fault)

def integerFunctionOutcome : IntegerResult → FunctionOutcome
  | .value value => .value value.val
  | .fault .overflow => .failure .integerOverflow
  | .fault .divisionByZero => .failure .divisionByZero

theorem integer_control_terminal (environment : MachineConfig) (result : IntegerResult)
    (rest : List BaseFrame) (returned : ∀ value, result = .value value → rest = []) :
    terminalOutcome (updateState environment (controlOfIntegerResult result) rest) =
      some (integerBaseOutcome result) := by
  cases result with
  | value value => rw [returned value rfl]; rfl
  | fault fault => rfl

theorem pure_integer_evaluation_completes {reads : NameId → Option IntValue}
    {expression : BaseExpr} {result : IntegerResult}
    (evaluated : PureIntegerEvaluation reads expression result) (environment : MachineConfig)
    (agrees : ∀ name value, reads name = some value →
      lookupBaseValue environment.names environment.activations environment.store name =
        .ok (typedInteger value)) {fuel : Nat} (enough : pureIntegerCost expression ≤ fuel) :
    runFuel fuel (updateState environment (.evaluate expression) []) =
      .completed (integerBaseOutcome result) := by
  obtain ⟨steps, rest, bound, path, stack⟩ := pure_integer_evaluation_steps evaluated environment agrees []
  exact Xsh.Proofs.Base.Runner.a_finished_path_is_completed_with_enough_fuel path
    (integer_control_terminal environment result rest stack) fuel (by omega)

theorem pure_integer_substitution_cost {values : List (NameId × IntValue)}
    {expression : BaseExpr} {result : IntegerResult}
    (evaluated : PureIntegerEvaluation (values.lookup ·) expression result) :
    pureIntegerCost (substitute values expression) = pureIntegerCost expression := by
  induction evaluated with
  | literal _ => rfl
  | read found => simp [substitute, found, pureIntegerCost]
  | binary operator left right ihLeft ihRight =>
      simp only [substitute, pureIntegerCost, ihLeft, ihRight]

theorem pure_integer_empty_expression_substitution {reads : NameId → Option IntValue}
    {expression : BaseExpr} {result : IntegerResult}
    (evaluated : PureIntegerEvaluation reads expression result) :
    substituteExpressions [] expression = expression := by
  induction evaluated with
  | literal _ => rfl
  | read _ => rfl
  | binary operator left right ihLeft ihRight =>
      simp [substituteExpressions, ihLeft, ihRight]

theorem pure_integer_empty_agrees (environment : MachineConfig) :
    ∀ name value, (fun _ : NameId => (none : Option IntValue)) name = some value →
      lookupBaseValue environment.names environment.activations environment.store name =
        .ok (typedInteger value) := by
  intro name value impossible
  cases impossible

theorem pure_integer_argument_fault (slot : NameId) {argument returned : BaseExpr} {fault : IntegerFault}
    (evaluated : PureIntegerEvaluation (fun _ => none) argument (.fault fault))
    {fuel : Nat} (enough : pureIntegerCost argument + 1 ≤ fuel) :
    runFuel fuel (machineOf [] (.call [slot] returned [argument])) =
      .completed (.fault (.arithmetic fault)) := by
  let environment := machineOf [] argument
  obtain ⟨steps, rest, bound, path, stack⟩ := pure_integer_evaluation_steps evaluated environment
    (pure_integer_empty_agrees environment) [.callArgument [slot] returned [] []]
  have start : step (machineOf [] (.call [slot] returned [argument])) =
      some (updateState environment (.evaluate argument) [.callArgument [slot] returned [] []]) := rfl
  exact Xsh.Proofs.Base.Runner.a_finished_path_is_completed_with_enough_fuel (.step start path)
    (by rfl) fuel (by omega)

theorem pure_integer_argument_value (slot : NameId) {argument returned : BaseExpr}
    {value : IntValue} {result : IntegerResult}
    (argumentRuns : PureIntegerEvaluation (fun _ => none) argument (.value value))
    (bodyRuns : PureIntegerEvaluation ([(slot, value)].lookup ·) returned result)
    {fuel : Nat} (enough : pureIntegerCost argument + pureIntegerCost returned + 2 ≤ fuel) :
    runFuel fuel (machineOf [] (.call [slot] returned [argument])) =
      .completed (integerBaseOutcome result) := by
  let environment := machineOf [] argument
  obtain ⟨steps, rest, bound, path, stack⟩ := pure_integer_evaluation_steps argumentRuns environment
    (pure_integer_empty_agrees environment) [.callArgument [slot] returned [] []]
  have same := stack value rfl
  subst rest
  have bodyEvaluated := pure_integer_evaluation_substitute bodyRuns
  obtain ⟨bodySteps, bodyRest, bodyBound, bodyPath, bodyStack⟩ :=
    pure_integer_evaluation_steps bodyEvaluated environment (pure_integer_empty_agrees environment) []
  have enter : step (machineOf [] (.call [slot] returned [argument])) =
      some (updateState environment (.evaluate argument) [.callArgument [slot] returned [] []]) := rfl
  have advance : step (updateState environment (controlOfIntegerResult (.value value))
      [.callArgument [slot] returned [] []]) =
      some (updateState environment (.evaluate (substitute [(slot, value)] returned)) []) := rfl
  have wholePath := machine_steps_append (.step enter path) (.step advance bodyPath)
  have cost := pure_integer_substitution_cost bodyRuns
  exact Xsh.Proofs.Base.Runner.a_finished_path_is_completed_with_enough_fuel wholePath
    (integer_control_terminal environment result bodyRest bodyStack) fuel (by omega)

theorem literal_machine_reads (slot : Word32) (value : IntValue) (expression : BaseExpr) :
    lookupBaseValue (machineOf [⟨slot, .integerLiteral value.val⟩] expression).names
      (machineOf [⟨slot, .integerLiteral value.val⟩] expression).activations
      (machineOf [⟨slot, .integerLiteral value.val⟩] expression).store (NameId.mk slot) =
      .ok (typedInteger value) := by
  simp [machineOf, initialConfig, lookupBaseValue, resolve_after_bind, resolveCell_bindCell,
    LetBinding.literalInt?, validateInt?, value.property]

theorem literal_machine_agrees (slot : Word32) (value : IntValue) (expression : BaseExpr) :
    ∀ name found, [(NameId.mk slot, value)].lookup name = some found →
      lookupBaseValue (machineOf [⟨slot, .integerLiteral value.val⟩] expression).names
        (machineOf [⟨slot, .integerLiteral value.val⟩] expression).activations
        (machineOf [⟨slot, .integerLiteral value.val⟩] expression).store name =
        .ok (typedInteger found) := by
  intro name found located
  by_cases same : name = NameId.mk slot
  · subst name
    have equalKey : (NameId.mk slot == NameId.mk slot) = true := by
      change decide (NameId.mk slot = NameId.mk slot) = true
      simp
    simp only [List.lookup, equalKey] at located
    cases located
    exact literal_machine_reads slot value expression
  · have unequalKey : (name == NameId.mk slot) = false := by
      change decide (name = NameId.mk slot) = false
      simp [same]
    simp [List.lookup, unequalKey] at located

def integerSettled (slot : Word32) : IntegerResult → Except FragmentFailure (List LetBinding)
  | .value value => .ok [⟨slot, .integerLiteral value.val⟩]
  | .fault .overflow => .error .integerOverflow
  | .fault .divisionByZero => .error .divisionByZero

theorem literal_assignment_settles (slot : Word32) (initial : IntValue)
    {assigned : BaseExpr} {result : IntegerResult}
    (evaluated : PureIntegerEvaluation ([(NameId.mk slot, initial)].lookup ·) assigned result)
    (enough : pureIntegerCost assigned ≤ runBudget) :
    (⟨slot, assigned⟩ : Assignment).settle? [⟨slot, .integerLiteral initial.val⟩] =
      some (integerSettled slot result) := by
  have completed := pure_integer_evaluation_completes evaluated
    (machineOf [⟨slot, .integerLiteral initial.val⟩] assigned)
    (literal_machine_agrees slot initial assigned) enough
  change runFuel runBudget (machineOf [⟨slot, .integerLiteral initial.val⟩] assigned) = _ at completed
  cases result with
  | value value => simp [Assignment.settle?, completed, integerBaseOutcome, integerSettled,
      integerPayload?, typedInteger]
  | fault fault =>
    cases fault <;> simp [Assignment.settle?, completed, integerBaseOutcome, integerSettled]

/-- Direct settlement uses its two existing bounded stages. A normalized call completes with the
same integer or arithmetic fault once its argument and body costs plus two administrative steps
fit the supplied call fuel. The body is pure for every possible assigned integer. -/
theorem assigned_literal_normalization_operationally_correct (slot : Word32) (initial : IntValue)
    {assigned returned : BaseExpr} {assignedResult : IntegerResult}
    (assignmentRuns : PureIntegerEvaluation ([(NameId.mk slot, initial)].lookup ·) assigned assignedResult)
    (returnedRuns : ∀ value, ∃ result,
      PureIntegerEvaluation ([(NameId.mk slot, value)].lookup ·) returned result)
    (assignedBudget : pureIntegerCost assigned ≤ runBudget)
    (returnedBudget : pureIntegerCost returned + 3 ≤ runBudget) :
    let program : FunctionProgram :=
      ⟨0, [⟨slot, .integerLiteral initial.val⟩], some ⟨slot, assigned⟩, none, returned⟩
    let callee : Callee :=
      ⟨0, returned, [⟨slot, substitute [(NameId.mk slot, initial)] assigned⟩]⟩
    program.callee? = some callee ∧ ∃ result,
      program.outcome? [] = some (integerFunctionOutcome result) ∧
      ∀ fuel, pureIntegerCost assigned + pureIntegerCost returned + 2 ≤ fuel →
        runFuel fuel (machineOf [] (callee.apply [])) = .completed (integerBaseOutcome result) := by
  dsimp only
  constructor
  · apply assigned_literal_function_is_callable
    simp [LetBinding.literalInt?, validateInt?, initial.property]
  · have settled := literal_assignment_settles slot initial assignmentRuns assignedBudget
    have normalized := pure_integer_evaluation_substitute assignmentRuns
    have normalizedCost := pure_integer_substitution_cost assignmentRuns
    have empty := pure_integer_empty_expression_substitution normalized
    cases assignedResult with
    | fault fault =>
      refine ⟨.fault fault, ?_, ?_⟩
      · cases fault <;> simp [FunctionProgram.outcome?, FunctionProgram.settled?, settled,
          integerSettled, integerFunctionOutcome]
      · intro fuel enough
        change runFuel fuel (machineOf [] (.call [NameId.mk slot] returned
          [substituteExpressions [] (substitute [(NameId.mk slot, initial)] assigned)])) = _
        rw [empty]
        exact pure_integer_argument_fault (NameId.mk slot) normalized (by omega)
    | value value =>
      obtain ⟨result, bodyRuns⟩ := returnedRuns value
      have argumentRuns : PureIntegerEvaluation (fun _ => none) (.integerLiteral value.val) (.value value) := by
        simpa [checked_integer_payload] using (PureIntegerEvaluation.literal (reads := fun _ => none) value.val)
      have directRun := pure_integer_argument_value (NameId.mk slot) argumentRuns bodyRuns
        (fuel := runBudget) (by simp only [pureIntegerCost]; omega)
      have outcome : (⟨0, [⟨slot, .integerLiteral initial.val⟩], some ⟨slot, assigned⟩, none,
          returned⟩ : FunctionProgram).outcome? [] = some (integerFunctionOutcome result) := by
        cases result with
        | value final =>
            simp [FunctionProgram.outcome?, FunctionProgram.settled?, settled, integerSettled,
              FunctionProgram.continuationOutcome?, FunctionProgram.machineWith, parameterNames,
              applyLetBindings, substituteExpressions, integerBaseOutcome, integerFunctionOutcome,
              integerPayload?, typedInteger, directRun]
        | fault fault =>
            cases fault <;>
              simp [FunctionProgram.outcome?, FunctionProgram.settled?, settled, integerSettled,
                FunctionProgram.continuationOutcome?, FunctionProgram.machineWith, parameterNames,
                applyLetBindings, substituteExpressions, integerBaseOutcome, integerFunctionOutcome, directRun]
      refine ⟨result, outcome, ?_⟩
      intro fuel enough
      change runFuel fuel (machineOf [] (.call [NameId.mk slot] returned
        [substituteExpressions [] (substitute [(NameId.mk slot, initial)] assigned)])) = _
      rw [empty]
      exact pure_integer_argument_value (NameId.mk slot) normalized bodyRuns (by omega)


end Xsh.Conformance.Vertical
