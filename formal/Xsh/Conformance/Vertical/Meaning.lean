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
the comparison a boolean. -/
def FragmentOp.expression : FragmentOp → BaseExpr → BaseExpr → BaseExpr
  | .add => .binary .add
  | .sub => .binary .subtract
  | .mul => .binary .multiply
  | .div => .binary .divide
  | .rem => .binary .remainder
  | .gt => .compare .greater

def FragmentFailure.fault : FragmentFailure → IntegerFault
  | .divisionByZero => .divisionByZero
  | .integerOverflow => .overflow

/-- What the IR meaning of a function ends with: the integer it completes with, or the
specified failure the base machine terminates with. -/
inductive FunctionOutcome where
  | value (result : Int)
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

/-- A function already read: the number of integer parameters and the body expression, which
mentions its parameters by slot and nothing else. -/
structure Callee where
  parameters : Nat
  body : BaseExpr

/-- What reading one row needs beyond the tables: the operation pool, the block table and the
callees a call row may name. -/
structure MeaningContext where
  tables : Tables
  ops : List FragmentOp
  blocks : List Block
  callees : List (Option Callee)

/-- The parameters of a callee as the names its body uses: slot `k` is the `k`-th parameter. -/
def parameterNames (count : Nat) : List NameId :=
  (List.range count).filterMap fun slot => (Word32.decode? slot).map NameId.mk

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
              some (.call (parameterNames callee.parameters) callee.body values)
          | [] => none
      | _, _ => none

structure LetBinding where
  slot : Word32
  value : IntValue

/-- The value an `IntInt` row's two payload words hold, which must be a signed 64-bit integer. -/
def literalOf : FragmentTag → List Word32 → Option IntValue
  | .intInt, [low, high] => validateInt? (signedOfWords low high)
  | _, _ => none

/-- The value of an `IntInt` row. -/
def literalValue? (context : MeaningContext) (row : Word32) : Option IntValue :=
  (readInstruction? context.tables row).bind fun instruction => literalOf instruction.1 instruction.2

/-- A function body is zero or more `StmtLetInt` of an `IntInt` literal on distinct slots,
then one `StmtReturn` of an expression. Every statement refers only to earlier rows. -/
def body? (context : MeaningContext) :
    List Word32 → Option (List LetBinding × BaseExpr)
  | [] => none
  | statement :: rest => do
      let (tag, payload) ← readInstruction? context.tables statement
      match tag, payload, rest with
      | .stmtReturn, [operand], [] => do
          check (decide (operand.val < statement.val))
          let result ← expression? context (context.tables.tags.size + 1) operand
          some ([], result)
      | .stmtLetInt, [slot, valueRow], _ :: _ => do
          check (decide (valueRow.val < statement.val))
          let value ← literalValue? context valueRow
          let (bindings, result) ← body? context rest
          check (!bindings.any (fun binding => binding.slot.val == slot.val))
          some (⟨slot, value⟩ :: bindings, result)
      | _, _, _ => none

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

/-- A function as the base machine runs it: its parameter count, its let bindings and its
returned expression. A function has parameters or lets, not both: the slot a let takes beside
parameters is not among the observed rows. -/
structure FunctionProgram where
  parameters : Nat
  bindings : List LetBinding
  expression : BaseExpr

def RawArtifact.context (artifact : RawArtifact) (callees : List (Option Callee)) :
    MeaningContext :=
  ⟨artifact.tables, artifact.ops, artifact.blocks, callees⟩

def functionProgram? (artifact : RawArtifact) (callees : List (Option Callee))
    (function : FunctionRow) : Option FunctionProgram := do
  let statements ← statementRows? artifact artifact.tables function
  let (bindings, expression) ← body? (artifact.context callees) statements
  check (function.paramsLen.val == 0 || bindings.isEmpty)
  pure ⟨function.paramsLen.val, bindings, expression⟩

def FunctionProgram.callee? (program : FunctionProgram) : Option Callee :=
  if program.bindings.isEmpty then some ⟨program.parameters, program.expression⟩ else none

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
      (bindings.find? fun binding => decide (cell = ⟨0, BinderId.mk binding.slot⟩)).map
        fun binding => ⟨typedInteger binding.value, .immutable⟩)
    expression

/-- A function without parameters runs as itself; a function with parameters runs as a call of
its body on the integer arguments. -/
def FunctionProgram.expressionAt (program : FunctionProgram) (arguments : List Int) : BaseExpr :=
  if program.parameters == 0 then program.expression
  else .call (parameterNames program.parameters) program.expression
    (arguments.map BaseExpr.integerLiteral)

def FunctionProgram.machine (program : FunctionProgram) (arguments : List Int) : MachineConfig :=
  machineOf program.bindings (program.expressionAt arguments)

def runBudget : Nat := 128

/-- The outcome the base machine completes with: an integer, or one of the two specified
arithmetic failures. Other faults, host requests and fuel exhaustion are not outcomes of this
subset, and neither is a call with the wrong number of arguments. -/
def FunctionProgram.outcome? (program : FunctionProgram) (arguments : List Int) :
    Option FunctionOutcome :=
  if arguments.length == program.parameters then
    match runFuel runBudget (program.machine arguments) with
    | .completed (.value value) => (integerPayload? value).map fun result => .value result.val
    | .completed (.fault (.arithmetic .divisionByZero)) => some (.failure .divisionByZero)
    | .completed (.fault (.arithmetic .overflow)) => some (.failure .integerOverflow)
    | _ => none
  else
    none

def functionOutcome? (artifact : RawArtifact) (function : FunctionRow) (arguments : List Int) :
    Option FunctionOutcome :=
  (programOf? artifact function).bind fun program => program.outcome? arguments

def resultBits (value : Int) : Nat := (value % 18446744073709551616).toNat

/-- A recorded outcome agrees with a meaning when both are the same failure, or both are values
and the record holds that integer's 64-bit pattern. -/
def outcomeRecorded : FunctionOutcome → ObservedOutcome → Bool
  | .value result, .value bits => resultBits result == bits.val
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
          | .stmtReturn, [operand] => decide (operand.val < index) && expressionAt operand
          | .stmtValue, [operand] => decide (operand.val < index) && expressionAt operand
          | .stmtLetInt, [_, valueRow] =>
              decide (valueRow.val < index) &&
                match readInstruction? tables valueRow with
                | some (.intInt, _) => true
                | _ => false
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

/-- A program outcome is exactly a completed base machine run: an integer value, or one of
the two specified arithmetic faults. -/
theorem program_outcome_is_a_completed_machine_run {program : FunctionProgram}
    {arguments : List Int} {outcome : FunctionOutcome}
    (result : program.outcome? arguments = some outcome) :
    (∃ final value, runFuel runBudget (program.machine arguments) = .completed (.value final) ∧
      integerPayload? final = some value ∧ outcome = .value value.val) ∨
    (∃ kind, runFuel runBudget (program.machine arguments) =
        .completed (.fault (.arithmetic kind.fault)) ∧ outcome = .failure kind) := by
  unfold FunctionProgram.outcome? at result
  split at result
  · cases run : runFuel runBudget (program.machine arguments) with
    | completed completed =>
        rw [run] at result
        cases completed with
        | value final =>
            cases payload : integerPayload? final with
            | none => simp [payload] at result
            | some value =>
                simp only [payload, Option.map_some, Option.some.injEq] at result
                exact .inl ⟨final, value, rfl, payload, result.symm⟩
        | fault fault =>
            cases fault <;> try (simp at result; done)
            rename_i arithmetic
            cases arithmetic with
            | overflow =>
                simp only [Option.some.injEq] at result
                exact .inr ⟨.integerOverflow, rfl, result.symm⟩
            | divisionByZero =>
                simp only [Option.some.injEq] at result
                exact .inr ⟨.divisionByZero, rfl, result.symm⟩
    | hostRequest waiting => rw [run] at result; simp at result
    | outOfFuel => rw [run] at result; simp at result
    | internalStuck => rw [run] at result; simp at result
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

/-- A recorded outcome does not depend on the fuel budget: a run that settles within the
budget gives the same answer under any larger budget, failures included. -/
theorem machine_run_outcome_is_independent_of_extra_fuel
    {program : FunctionProgram} {arguments : List Int} {outcome : FunctionOutcome}
    (settled : program.outcome? arguments = some outcome) (extra : Nat) :
    runFuel (runBudget + extra) (program.machine arguments) =
      runFuel runBudget (program.machine arguments) := by
  have notExhausted : runFuel runBudget (program.machine arguments) ≠ .outOfFuel := by
    intro exhausted
    unfold FunctionProgram.outcome? at settled
    split at settled
    · rw [exhausted] at settled
      simp at settled
    · simp at settled
  exact Xsh.Proofs.Base.Runner.base_runner.2.2.2.2.2.2.1 runBudget
    (program.machine arguments) notExhausted extra

end Xsh.Conformance.Vertical
