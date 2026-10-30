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

def FragmentOp.toInteger : FragmentOp → IntegerBinaryOp
  | .add => .add
  | .sub => .subtract
  | .mul => .multiply
  | .div => .divide
  | .rem => .remainder

def FragmentFailure.fault : FragmentFailure → IntegerFault
  | .divisionByZero => .divisionByZero
  | .integerOverflow => .overflow

/-- What the IR meaning of a function ends with: the integer it completes with, or the
specified failure the base machine terminates with. -/
inductive FunctionOutcome where
  | value (result : Int)
  | failure (kind : FragmentFailure)
  deriving DecidableEq

/-- Low word first, as the exchange preserves; the 64-bit pattern is two's complement. -/
def signedOfWords (low high : Word32) : Int :=
  let unsigned : Nat := low.val + high.val * 4294967296
  if unsigned < 9223372036854775808 then
    Int.ofNat unsigned
  else
    Int.ofNat unsigned - 18446744073709551616

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

/-- An expression row denotes a base expression. Operands must come from earlier rows, the
operation must be in the exchanged pool, and a slot read becomes an identifier whose name and
binder are that slot. -/
def expression? (tables : Tables) (ops : List FragmentOp) : Nat → Word32 → Option BaseExpr
  | 0, _ => none
  | fuel + 1, index => do
      let (tag, payload) ← readInstruction? tables index
      match tag, payload with
      | .exprInt, [low, high] => some (.integerLiteral (signedOfWords low high))
      | .exprParam, [slot] => some (.identifier (NameId.mk slot))
      | .exprBinary, [op, left, right, _] =>
          if left.val < index.val ∧ right.val < index.val then do
            let operation ← ops[op.val]?
            let leftExpression ← expression? tables ops fuel left
            let rightExpression ← expression? tables ops fuel right
            some (.binary operation.toInteger leftExpression rightExpression)
          else
            none
      | _, _ => none

structure LetBinding where
  slot : Word32
  value : IntValue

/-- A function body is zero or more `StmtLetInt` of an `IntInt` literal on distinct slots,
then one `StmtReturn` of an expression. Every statement refers only to earlier rows. -/
def body? (tables : Tables) (ops : List FragmentOp) :
    List Word32 → Option (List LetBinding × BaseExpr)
  | [] => none
  | statement :: rest => do
      let (tag, payload) ← readInstruction? tables statement
      match tag, payload, rest with
      | .stmtReturn, [operand], [] =>
          if operand.val < statement.val then do
            let result ← expression? tables ops (tables.tags.size + 1) operand
            some ([], result)
          else
            none
      | .stmtLetInt, [slot, valueRow], _ :: _ =>
          if valueRow.val < statement.val then do
            let (valueTag, valueWords) ← readInstruction? tables valueRow
            match valueTag, valueWords with
            | .intInt, [low, high] => do
                let value ← validateInt? (signedOfWords low high)
                let (bindings, result) ← body? tables ops rest
                if bindings.any (fun binding => binding.slot == slot) then none
                else some (⟨slot, value⟩ :: bindings, result)
            | _, _ => none
          else
            none
      | _, _, _ => none

/-- The statements of a function's body block: the block must be owned by the function and
its range must hold a count followed by exactly that many statement rows. -/
def statementRows? (artifact : RawArtifact) (tables : Tables) (function : FunctionRow) :
    Option (List Word32) := do
  let block ← artifact.blocks.find? fun block => block.id == function.body
  if block.owner = OptionalBridgeId.mk (some function.id) then pure () else none
  let words ← Decode.readPayload? tables ⟨DirectIndex.decode block.start, block.len⟩
  match words with
  | count :: statements => if count.val == statements.length then some statements else none
  | [] => none

def functionProgram? (artifact : RawArtifact) (function : FunctionRow) :
    Option (List LetBinding × BaseExpr) := do
  let tables := artifact.tables
  let statements ← statementRows? artifact tables function
  body? tables artifact.ops statements

/-- Each slot is a lexical name, a binder and a cell of the single activation, bound to an
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

def runBudget : Nat := 64

/-- The outcome the base machine completes with: an integer, or one of the two specified
arithmetic failures. Other faults, host requests and fuel exhaustion are not outcomes of this
subset. -/
def programOutcome? (bindings : List LetBinding) (expression : BaseExpr) :
    Option FunctionOutcome :=
  match runFuel runBudget (machineOf bindings expression) with
  | .completed (.value value) => (integerPayload? value).map fun result => .value result.val
  | .completed (.fault (.arithmetic .divisionByZero)) => some (.failure .divisionByZero)
  | .completed (.fault (.arithmetic .overflow)) => some (.failure .integerOverflow)
  | _ => none

def functionOutcome? (artifact : RawArtifact) (function : FunctionRow) :
    Option FunctionOutcome :=
  (functionProgram? artifact function).bind fun program =>
    programOutcome? program.1 program.2

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
          match tag, payload with
          | .intInt, _ => true
          | .exprInt, _ => true
          | .exprParam, _ => true
          | .exprBinary, _ => (expression? tables artifact.ops (tables.tags.size + 1) word).isSome
          | .stmtReturn, [operand] =>
              decide (operand.val < index) &&
                (expression? tables artifact.ops (tables.tags.size + 1) operand).isSome
          | .stmtLetInt, [_, valueRow] =>
              decide (valueRow.val < index) &&
                match readInstruction? tables valueRow with
                | some (.intInt, _) => true
                | _ => false
          | _, _ => false

/-- The artifact validator: ordered identifiers, every row well formed, every block owned by
the function that names it as body, every function with a defined outcome, and every
recorded observation, in order, equal to the outcome of the function it names. -/
def validateArtifact (artifact : RawArtifact) : Bool :=
  idsInOrder (artifact.rows.map (·.id)) &&
  idsInOrder (artifact.blocks.map (·.id)) &&
  idsInOrder (artifact.functions.map (·.id)) &&
  (List.range artifact.rows.length).all (rowWellFormed artifact artifact.tables) &&
  artifact.blocks.all (fun block =>
    artifact.functions.any fun function =>
      function.body == block.id && block.owner == OptionalBridgeId.mk (some function.id)) &&
  artifact.functions.all (fun function =>
    !function.name.isEmpty && (functionOutcome? artifact function).isSome) &&
  artifact.observations.all fun observation =>
    artifact.functions.any fun function =>
      function.id == observation.function &&
        (functionOutcome? artifact function).any fun outcome =>
          outcomeRecorded outcome observation.outcome

def namedResults (artifact : RawArtifact) : Option (List (String × FunctionOutcome)) :=
  artifact.observations.mapM fun observation => do
    let function ← artifact.functions.find? fun function => function.id == observation.function
    let outcome ← functionOutcome? artifact function
    pure (function.name, outcome)

/-- Decode both documents, require the raw-IR document to name the source document, validate
the artifact, and report each observed function with the outcome its IR denotes. -/
def certify (sourceText rawText : ExchangeCodes) : Option (List (String × FunctionOutcome)) := do
  let source ← decodeSourceDocument sourceText
  let raw ← decodeRawDocument rawText
  if raw.sourceId = source.id then pure () else none
  if validateArtifact raw then pure () else none
  namedResults raw

theorem certify_sound {sourceText rawText : ExchangeCodes}
    {results : List (String × FunctionOutcome)}
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
theorem program_outcome_is_a_completed_machine_run {bindings : List LetBinding}
    {expression : BaseExpr} {outcome : FunctionOutcome}
    (result : programOutcome? bindings expression = some outcome) :
    (∃ final value, runFuel runBudget (machineOf bindings expression) = .completed (.value final) ∧
      integerPayload? final = some value ∧ outcome = .value value.val) ∨
    (∃ kind, runFuel runBudget (machineOf bindings expression) =
        .completed (.fault (.arithmetic kind.fault)) ∧ outcome = .failure kind) := by
  unfold programOutcome? at result
  cases run : runFuel runBudget (machineOf bindings expression) with
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

/-- Every recorded observation of a validated artifact agrees with the outcome of a completed
base machine run of the named function's IR meaning. -/
theorem validated_observation_is_a_machine_run {artifact : RawArtifact}
    (valid : validateArtifact artifact = true) {observation : Observation}
    (recorded : observation ∈ artifact.observations) :
    ∃ function ∈ artifact.functions, function.id = observation.function ∧
      ∃ bindings expression outcome,
        functionProgram? artifact function = some (bindings, expression) ∧
        programOutcome? bindings expression = some outcome ∧
        outcomeRecorded outcome observation.outcome = true := by
  unfold validateArtifact at valid
  simp only [Bool.and_eq_true, List.all_eq_true] at valid
  have observed := valid.2 observation recorded
  simp only [List.any_eq_true, Bool.and_eq_true, beq_iff_eq] at observed
  obtain ⟨function, member, sameId, sameOutcome⟩ := observed
  refine ⟨function, member, sameId, ?_⟩
  unfold functionOutcome? at sameOutcome
  cases program : functionProgram? artifact function with
  | none => simp [program] at sameOutcome
  | some pair =>
      obtain ⟨bindings, expression⟩ := pair
      rw [program] at sameOutcome
      simp only [Option.bind_some, Option.any_eq_true] at sameOutcome
      obtain ⟨outcome, result, agrees⟩ := sameOutcome
      exact ⟨bindings, expression, outcome, rfl, result, agrees⟩

/-- A recorded outcome does not depend on the fuel budget: a run that settles within the
budget gives the same answer under any larger budget, failures included. -/
theorem machine_run_outcome_is_independent_of_extra_fuel
    {bindings : List LetBinding} {expression : BaseExpr} {outcome : FunctionOutcome}
    (settled : programOutcome? bindings expression = some outcome) (extra : Nat) :
    runFuel (runBudget + extra) (machineOf bindings expression) =
      runFuel runBudget (machineOf bindings expression) := by
  have notExhausted : runFuel runBudget (machineOf bindings expression) ≠ .outOfFuel := by
    intro exhausted
    unfold programOutcome? at settled
    rw [exhausted] at settled
    simp at settled
  exact Xsh.Proofs.Base.Runner.base_runner.2.2.2.2.2.2.1 runBudget
    (machineOf bindings expression) notExhausted extra

end Xsh.Conformance.Vertical
