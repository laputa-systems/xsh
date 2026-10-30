import Xsh.Conformance.Vertical.Frontend
import Xsh.Conformance.Vertical.Meaning

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport
open Xsh.Values
open Xsh.Semantics.BaseMachine

def maxIntLiteral : Nat := 9223372036854775807

def literalWords (value : Nat) : List Nat := [value % 4294967296, value / 4294967296]

def letSlot : Nat := 1

/-- The base-machine program a source function denotes, straight from the parsed tree: its one
integer let is bound at the first slot, a name resolves to that let or fails, and the return is
one binary operation over atoms. A function of any other shape has no meaning here. -/
def SourceFunction.program? (function : SourceFunction) : Option (List LetBinding × BaseExpr) := do
  let atom : String → SourceAtom → Option BaseExpr := fun letName atom =>
    match atom with
    | .int value => if value ≤ maxIntLiteral then some (.integerLiteral value) else none
    | .var name => if name == letName then some (.identifier (NameId.mk ⟨letSlot, by decide⟩)) else none
  match function.lets, function.result with
  | [(letName, value)], .binary op left right =>
      let bound ← validateInt? value
      let leftExpression ← atom letName left
      let rightExpression ← atom letName right
      some ([⟨⟨letSlot, by decide⟩, bound⟩], .binary op.toInteger leftExpression rightExpression)
  | _, _ => none

def SourceFunction.outcome? (function : SourceFunction) : Option FunctionOutcome :=
  function.program?.bind fun program => programOutcome? program.1 program.2

/-- Rows, payload words, operation pool and per-function blocks, as plain naturals before they
are range-checked into the exchange types. -/
structure LoweringState where
  rows : List (FragmentTag × Nat × Nat) := []
  extra : List Nat := []
  ops : List FragmentOp := []
  blocks : List (Nat × Nat) := []
  functionNames : List String := []

def operationIndex (ops : List FragmentOp) (op : FragmentOp) : Nat × List FragmentOp :=
  match ops.findIdx? (· == op) with
  | some index => (index, ops)
  | none => (ops.length, ops ++ [op])

def lowerAtom (letName : String) : SourceAtom → Option (FragmentTag × List Nat)
  | .int value => if value ≤ maxIntLiteral then some (.exprInt, literalWords value) else none
  | .var name => if name == letName then some (.exprParam, [letSlot]) else none

/-- Lower one function in the observed row order: its integer let, the operand rows left then
right, the binary row, the return, and then its block of two statements. `span` is the span
word the exporter stored in the binary row; the source tree does not determine it. -/
def lowerFunction (state : LoweringState) (span : Nat) (function : SourceFunction) :
    Option LoweringState :=
  match function.lets, function.result with
  | [(letName, value)], .binary op left right => do
      if value ≤ maxIntLiteral then pure () else none
      let (leftTag, leftWords) ← lowerAtom letName left
      let (rightTag, rightWords) ← lowerAtom letName right
      let base := state.rows.length
      let start := state.extra.length
      let (opIndex, ops) := operationIndex state.ops op
      let operandsAt := start + 4
      let binaryAt := operandsAt + leftWords.length + rightWords.length
      let returnAt := binaryAt + 4
      let blockAt := returnAt + 1
      some { rows := state.rows ++
                [(.intInt, start, 2), (.stmtLetInt, start + 2, 2),
                  (leftTag, operandsAt, leftWords.length),
                  (rightTag, operandsAt + leftWords.length, rightWords.length),
                  (.exprBinary, binaryAt, 4), (.stmtReturn, returnAt, 1)]
             extra := state.extra ++ literalWords value ++ [letSlot, base] ++ leftWords ++
                rightWords ++ [opIndex, base + 2, base + 3, span] ++ [base + 4] ++
                [2, base + 1, base + 5]
             ops := ops
             blocks := state.blocks ++ [(blockAt, state.functionNames.length + 1)]
             functionNames := state.functionNames ++ [function.name] }
  | _, _ => none

def lowerFunctions (spans : List Nat) (functions : List SourceFunction) :
    Option LoweringState :=
  if spans.length = functions.length then lowerEach spans functions else none
where
  lowerEach (spans : List Nat) (functions : List SourceFunction) : Option LoweringState :=
    (spans.zip functions).foldlM (fun state (span, function) => lowerFunction state span function) {}

def indexed (items : List α) : List (Nat × α) := (List.range items.length).zip items

/-- Range-check the plain naturals into the exchange types. Anything that does not fit a stored
word has no lowering. -/
def LoweringState.toArtifact (state : LoweringState) (sourceId : BridgeId)
    (observations : List Observation) : Option RawArtifact := do
  let rows ← (indexed state.rows).mapM fun (index, tag, lhs, rhs) => do
    pure (⟨← BridgeId.fromIndex? index, tag, ← Word32.decode? lhs, ← Word32.decode? rhs⟩ : Row)
  let extra ← state.extra.mapM Word32.decode?
  let blocks ← (indexed state.blocks).mapM fun (index, start, owner) => do
    pure (⟨← BridgeId.fromIndex? index, ← Word32.decode? start, ← Word32.decode? 3,
      ⟨none⟩, ⟨some (← BridgeId.fromIndex? (owner - 1))⟩, ⟨3, by decide⟩⟩ : Block)
  let functions ← (indexed state.functionNames).mapM fun (index, name) => do
    pure (⟨← BridgeId.fromIndex? index, name, ← Word32.decode? 0, ← Word32.decode? 0,
      ← Word32.decode? index, ← Word32.decode? 1, ← BridgeId.fromIndex? index,
      ← Word32.decode? (letSlot + 1)⟩ : FunctionRow)
  pure ⟨sourceId, rows, extra, state.ops, blocks, functions, observations⟩

/-- The span word of each binary row, in table order. -/
def binarySpans (artifact : RawArtifact) : List Nat :=
  artifact.rows.filterMap fun row =>
    match row.tag with
    | .exprBinary => (artifact.extra[row.lhs.val + 3]?).map (·.val)
    | _ => none

/-- A source function agrees with the artifact when the function of that name has the same
outcome under the IR meaning as under the source meaning. -/
def sourceAgrees (artifact : RawArtifact) (function : SourceFunction) : Bool :=
  artifact.functions.any fun row =>
    row.name == function.name && function.outcome?.isSome &&
      functionOutcome? artifact row == function.outcome?

/-- Lowering the parsed functions reproduces the artifact exactly, taking the span words from
the artifact. -/
def lowersTo (artifact : RawArtifact) (functions : List SourceFunction) : Bool :=
  match lowerFunctions (binarySpans artifact) functions with
  | some state => state.toArtifact artifact.sourceId artifact.observations == some artifact
  | none => false

/-- Translation validation of a decoded pair: the source text parses in the admitted
productions, lowering the parsed functions reproduces the raw-IR document's rows, pools, blocks
and function table exactly (the span words are taken from the document), the document names
this source, each function means the same under the source and the IR, the artifact
validates, and each observation is the outcome of its function. -/
def certifyCodes (sourceId : BridgeId) (textCodes : List Nat) (raw : RawArtifact) :
    Option (List (String × FunctionOutcome)) := do
  let functions ← parseSourceCodes textCodes
  if decide (raw.sourceId = sourceId) && lowersTo raw functions && validateArtifact raw &&
      functions.all (sourceAgrees raw) then
    namedResults raw
  else
    none

def certifyDecoded (source : SourceDocument) (raw : RawArtifact) :
    Option (List (String × FunctionOutcome)) :=
  certifyCodes source.id (source.text.toList.map Char.toNat) raw

/-- The same validation from the exchanged bytes of both documents. -/
def certifyFromSource (sourceText rawText : ExchangeCodes) :
    Option (List (String × FunctionOutcome)) := do
  let source ← decodeSourceDocument sourceText
  let raw ← decodeRawDocument rawText
  certifyDecoded source raw

theorem certify_from_source_of_decodes {sourceText rawText : ExchangeCodes}
    {source : SourceDocument} {raw : RawArtifact}
    (sourceDecoded : decodeSourceDocument sourceText = some source)
    (rawDecoded : decodeRawDocument rawText = some raw) :
    certifyFromSource sourceText rawText = certifyDecoded source raw := by
  unfold certifyFromSource
  rw [sourceDecoded, rawDecoded]
  rfl

theorem certify_decoded_of_checks {source : SourceDocument} {raw : RawArtifact}
    {functions : List SourceFunction} (same : raw.sourceId = source.id)
    (parsed : parseSource source.text = some functions) (lowers : lowersTo raw functions = true)
    (valid : validateArtifact raw = true) (agrees : functions.all (sourceAgrees raw) = true) :
    certifyDecoded source raw = namedResults raw := by
  unfold certifyDecoded certifyCodes
  have parsedCodes : parseSourceCodes (source.text.toList.map Char.toNat) = some functions :=
    parsed
  simp only [parsedCodes, Option.bind_some, Option.bind_eq_bind]
  simp [same, lowers, valid, agrees]

theorem certify_decoded_sound {source : SourceDocument} {raw : RawArtifact}
    {results : List (String × FunctionOutcome)}
    (certified : certifyDecoded source raw = some results) :
    ∃ functions, raw.sourceId = source.id ∧ parseSource source.text = some functions ∧
      lowersTo raw functions = true ∧ functions.all (sourceAgrees raw) = true ∧
      validateArtifact raw = true ∧ namedResults raw = some results := by
  unfold certifyDecoded certifyCodes at certified
  cases parsed : parseSourceCodes (source.text.toList.map Char.toNat) with
  | none => simp [parsed] at certified
  | some functions =>
  simp only [parsed, Option.bind_some, Option.bind_eq_bind] at certified
  split at certified
  · rename_i checks
    simp only [Bool.and_eq_true, decide_eq_true_eq] at checks
    exact ⟨functions, checks.1.1.1, parsed, checks.1.1.2, checks.2, checks.1.2, certified⟩
  · cases certified

theorem certify_from_source_sound {sourceText rawText : ExchangeCodes}
    {results : List (String × FunctionOutcome)}
    (certified : certifyFromSource sourceText rawText = some results) :
    ∃ source raw, decodeSourceDocument sourceText = some source ∧
      decodeRawDocument rawText = some raw ∧ certifyDecoded source raw = some results := by
  unfold certifyFromSource at certified
  cases sourceDecoded : decodeSourceDocument sourceText with
  | none => simp [sourceDecoded] at certified
  | some source =>
  cases rawDecoded : decodeRawDocument rawText with
  | none => simp [sourceDecoded, rawDecoded] at certified
  | some raw =>
  simp only [sourceDecoded, rawDecoded, Option.bind_some, Option.bind_eq_bind] at certified
  exact ⟨source, raw, rfl, rfl, certified⟩

theorem a_function_outside_the_lowered_shape_is_refused :
    (lowerFunctions [2] [⟨"f", [], .atom (.int 1)⟩]).isNone = true ∧
      (lowerFunctions [2] [⟨"f", [("a", 1), ("b", 2)], .binary .add (.var "a") (.var "b")⟩]).isNone =
        true ∧
      (lowerFunctions [2] [⟨"f", [("a", 1)], .binary .add (.var "a") (.var "b")⟩]).isNone = true ∧
      (lowerFunctions [2] [⟨"f", [("a", 9223372036854775808)], .binary .add (.var "a") (.int 1)⟩]).isNone =
        true ∧
      (lowerFunctions [] [⟨"f", [("a", 1)], .binary .add (.var "a") (.int 1)⟩]).isNone = true := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem a_lowered_function_has_the_observed_rows :
    (lowerFunctions [2] [⟨"f", [("a", 1)], .binary .add (.var "a") (.int 5)⟩]).map
        (fun state => (state.rows, state.extra, state.ops)) =
      some ([(.intInt, 0, 2), (.stmtLetInt, 2, 2), (.exprParam, 4, 1), (.exprInt, 5, 2),
          (.exprBinary, 7, 4), (.stmtReturn, 11, 1)],
        [1, 0, 1, 0, 1, 5, 0, 0, 2, 3, 2, 4, 2, 1, 5], [.add]) := by
  decide +kernel

end Xsh.Conformance.Vertical
