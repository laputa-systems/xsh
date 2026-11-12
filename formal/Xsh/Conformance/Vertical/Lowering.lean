import Xsh.Conformance.Vertical.Frontend
import Xsh.Conformance.Vertical.Meaning

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport
open Xsh.Values
open Xsh.Semantics.BaseMachine

def maxIntLiteral : Nat := 9223372036854775807

def letSlot : Nat := 1

def SourceExpression.isAssigned : SourceExpression → Bool
  | .assigned _ _ _ => true
  | _ => false

/-- The slots a computed-let sequence binds: the parameter is slot 0 and the bindings take the
slots after the exporter's reserved one, in source order. Every binding name is distinct from the
parameter and from the other bindings. -/
def computedLetsScope? (parameter : String) :
    Nat → List (String × SourceValue) → Option (List (String × Nat))
  | _, [] => some []
  | slot, (name, _) :: rest => do
      if name == parameter || (rest.map (·.1)).contains name then none else pure ()
      let others ← computedLetsScope? parameter (slot + 1) rest
      some ((name, slot) :: others)

/-- The slot each name of a function reads: the parameters are the first slots in order, the one
literal let takes the slot after the unused first, and a computed let takes the same slot beside
its parameter. A function with one parameter and one let uses the parameter slot, the reserved
slot after it, and then the let, which is where the exporter places the binding of such a
function. A sequence of computed lets continues at the following slots. A propagation function has
one parameter and its target at the slot after the reserved one. -/
def SourceFunction.scope? (function : SourceFunction) : Option (List (String × Nat)) :=
  match function.parameters, function.lets, function.propagation with
  | [parameter], [], some propagation =>
      if parameter == propagation.target then none
      else some [(parameter, 0), (propagation.target, 2)]
  | [], [(name, _)], none => some [(name, letSlot)]
  | [parameter], [(name, _)], none =>
      if function.result.isAssigned then none
      else if parameter == name then none
      else some [(parameter, 0), (name, 2)]
  | [parameter], [], none =>
      match function.result with
      | .computedLet name _ _ =>
          if parameter == name then none
          else some [(parameter, 0), (name, 2)]
      | .computedLets bindings _ => do
          let bindings ← computedLetsScope? parameter 2 bindings
          some ((parameter, 0) :: bindings)
      | _ => some [(parameter, 0)]
  | _ :: _, [], none =>
      if function.parameters.Nodup then some (function.parameters.zip (List.range function.parameters.length))
      else none
  | _, _, _ => none

def SourceAtom.expression? (scope : List (String × Nat)) : SourceAtom → Option BaseExpr
  | .int value => if value ≤ maxIntLiteral then some (.integerLiteral value) else none
  | .var name => do
      let slot ← scope.lookup name
      let word ← Word32.decode? slot
      some (.identifier (NameId.mk word))

def SourceValue.expression? (scope : List (String × Nat)) : SourceValue → Option BaseExpr
  | .atom single => single.expression? scope
  | .binary op left right => do
      if op.isComparison then none else pure ()
      some (op.expression (← left.expression? scope) (← right.expression? scope))

/-- The base expression a source result denotes: a name resolves to its parameter or its let, a
call carries the body of the earlier function it names, a conditional is a comparison with
its two values, a comparison is its two atoms under the operation, and an assigned form is the
value it returns. `callees` holds every earlier function with its callee form, including its admitted bindings
and assignment. -/
def SourceExpression.expression? (callees : List (String × Option Callee))
    (scope : List (String × Nat)) : SourceExpression → Option BaseExpr
  | .value single => single.expression? scope
  | .call callee arguments => do
      let found ← (callees.lookup callee).bind id
      if arguments.length == found.parameters then pure () else none
      let values ← arguments.mapM (·.expression? scope)
      some (found.apply values)
  | .conditional op left right thenValue elseValue => do
      if op.isComparison then pure () else none
      some (.conditional (op.expression (← left.expression? scope)
          (← right.expression? scope))
        (← thenValue.expression? scope) (← elseValue.expression? scope))
  | .assigned _ _ result => result.expression? scope
  | .comparison op left right => do
      if op.isComparison then pure () else none
      some (op.expression (← left.expression? scope) (← right.expression? scope))
  | .constructed isOk payload => do
      some (constructorExpression isOk (← payload.expression? scope))
  | .computedLet _ _ result => result.expression? scope
  | .computedLets _ result => result.expression? scope

/-- The assignment a function's returned form carries, as the base machine would run it: it must
assign the function's one let, and its value is read in the function's scope. -/
def SourceFunction.assignment? (function : SourceFunction) (scope : List (String × Nat)) :
    Option (Option Assignment) :=
  match function.lets, function.result with
  | [(name, _)], .assigned target value _ => do
      if target == name then pure () else none
      some (some (⟨⟨letSlot, by decide⟩, ← value.expression? scope⟩ : Assignment))
  | _, .assigned _ _ _ => none
  | _, _ => some none

/-- The propagation a source function denotes: the callee is an earlier function with a callee
form, the argument expressions are read in the function's scope, and the target slot is the one
the scope gives it. The call is the applied callee, so a computed binding of the callee is one of
its arguments. -/
def propagationProgram? (callees : List (String × Option Callee))
    (scope : List (String × Nat)) (propagation : SourcePropagation) : Option Propagation := do
  let found ← (callees.lookup propagation.callee).bind id
  if propagation.arguments.length == found.parameters then pure () else none
  let slot ← scope.lookup propagation.target
  let word ← Word32.decode? slot
  let arguments ← propagation.arguments.mapM (·.expression? scope)
  some ⟨word, found.applyParameters, found.body, found.applyArguments arguments⟩

/-- A function the lowerer records as directly callable: it has no propagation binding. An
admitted assignment evaluates its updated binding as a call argument. -/
def SourceFunction.callable (function : SourceFunction) : Bool :=
  function.propagation.isNone

/-- The bindings a computed-let sequence declares, in source order: each value reads the
parameter and the bindings before it, and each binding takes the next slot from the start the
exporter reserves. -/
def computedBindingsFrom? (parameter : String) (earlier : List (String × Nat)) :
    Nat → List (String × SourceValue) → Option (List LetBinding)
  | _, [] => some []
  | slot, (name, value) :: rest => do
      let word ← Word32.decode? slot
      let bound ← value.expression? ((parameter, 0) :: earlier)
      let others ← computedBindingsFrom? parameter (earlier ++ [(name, slot)]) (slot + 1) rest
      some (⟨word, bound⟩ :: others)

/-- The bindings a computed-let result declares, evaluated in the parameters' scope. Any other
result declares none. -/
def SourceFunction.computedBindings? (function : SourceFunction)
    (scope : List (String × Nat)) : Option (List LetBinding) :=
  match function.result with
  | .computedLet name value _ => do
      let slot ← scope.lookup name
      let word ← Word32.decode? slot
      let valueScope := scope.filter fun entry => entry.1 != name
      let bound ← value.expression? valueScope
      some [(⟨word, bound⟩ : LetBinding)]
  | .computedLets [] _ => none
  | .computedLets bindings _ =>
      match function.parameters with
      | [parameter] => computedBindingsFrom? parameter [] 2 bindings
      | _ => none
  | _ => some []

/-- The base-machine program a source function denotes, straight from the parsed tree. A computed
let binds its value expression, read in the parameters' scope, before the returned expression that
reads it. -/
def SourceFunction.program? (callees : List (String × Option Callee)) (function : SourceFunction) :
    Option FunctionProgram := do
  let scope ← function.scope?
  let bindings ← match function.propagation with
    | some _ => some []
    | none =>
      match function.lets with
      | [] => function.computedBindings? scope
      | [(name, value)] => do
          let bound ← validateInt? value
          let slot ← scope.lookup name
          let word ← Word32.decode? slot
          some [(⟨word, .integerLiteral bound.val⟩ : LetBinding)]
      | _ => none
  let assignment ← function.assignment? scope
  let propagation ← match function.propagation with
    | none => some none
    | some found => (propagationProgram? callees scope found).map some
  let expression ← function.result.expression? callees scope
  some ⟨function.parameters.length, bindings, assignment, propagation, expression⟩

def sourceStep (state : List (String × Option Callee) × List (Option FunctionProgram))
    (function : SourceFunction) : List (String × Option Callee) × List (Option FunctionProgram) :=
  let program := function.program? state.1
  (state.1 ++ [(function.name, program.bind (·.callee?))], state.2 ++ [program])

/-- The programs of the source functions in order; a call names an earlier function. -/
def sourcePrograms (functions : List SourceFunction) : List (Option FunctionProgram) :=
  (functions.foldl sourceStep ([], [])).2

structure LoweringState where
  rows : List Row := []
  extra : List Word32 := []
  ops : List FragmentOp := []
  blocks : List Block := []
  functions : List FunctionRow := []
  callable : List Bool := []
  spans : List Word32 := []

abbrev Lower := StateT LoweringState Option

def Lower.fail : Lower α := fun _ => none

def Lower.require (holds : Bool) : Lower Unit := if holds then pure () else Lower.fail

def Lower.lift (value : Option α) : Lower α :=
  match value with
  | some found => pure found
  | none => Lower.fail

def Lower.word (value : Nat) : Lower Word32 := Lower.lift (Word32.decode? value)

/-- Append one instruction row and its payload words; the result is the row's index. -/
def Lower.pushRow (tag : FragmentTag) (payload : List Word32) : Lower Word32 := fun state => do
  let index ← Word32.decode? state.rows.length
  let id ← BridgeId.fromIndex? state.rows.length
  let start ← Word32.decode? state.extra.length
  let length ← Word32.decode? payload.length
  some (index, { state with rows := state.rows ++ [⟨id, tag, start, length⟩],
                            extra := state.extra ++ payload })

/-- Append one block over the given words, owned by a function. -/
def Lower.pushBlock (owner : BridgeId) (flags : WireByte) (words : List Word32) :
    Lower BridgeId := fun state => do
  let id ← BridgeId.fromIndex? state.blocks.length
  let start ← Word32.decode? state.extra.length
  let length ← Word32.decode? words.length
  some (id, { state with
    blocks := state.blocks ++ [⟨id, start, length, ⟨none⟩, ⟨some owner⟩, flags⟩],
    extra := state.extra ++ words })

def Lower.takeSpan : Lower Word32 := fun state =>
  match state.spans with
  | [] => none
  | span :: rest => some (span, { state with spans := rest })

def Lower.operationIndex (op : FragmentOp) : Lower Word32 := fun state =>
  match state.ops.findIdx? (· == op) with
  | some index => (Word32.decode? index).map fun word => (word, state)
  | none => (Word32.decode? state.ops.length).map fun word =>
      (word, { state with ops := state.ops ++ [op] })

def Lower.literalWords (value : Nat) : Lower (List Word32) := do
  Lower.require (decide (value ≤ maxIntLiteral))
  pure [← Lower.word (value % 4294967296), ← Lower.word (value / 4294967296)]

def Lower.atom (scope : List (String × Nat)) : SourceAtom → Lower Word32
  | .int value => do Lower.pushRow .exprInt (← Lower.literalWords value)
  | .var name => do
      let slot ← Lower.lift (scope.lookup name)
      Lower.pushRow .exprParam [← Lower.word slot]

def Lower.comparison (scope : List (String × Nat)) (op : FragmentOp) (left right : SourceAtom) :
    Lower Word32 := do
  let leftRow ← Lower.atom scope left
  let rightRow ← Lower.atom scope right
  let operation ← Lower.operationIndex op
  let span ← Lower.takeSpan
  Lower.pushRow .exprBinary [operation, leftRow, rightRow, span]

def Lower.value (scope : List (String × Nat)) : SourceValue → Lower Word32
  | .atom single => Lower.atom scope single
  | .binary op left right => do
      Lower.require (!op.isComparison)
      Lower.comparison scope op left right

/-- A branch value in its value block: the value, its `StmtValue`, the block holding that
statement, and the block expression. -/
def Lower.valueBlock (owner : BridgeId) (scope : List (String × Nat)) (value : SourceValue) :
    Lower Word32 := do
  let valueRow ← Lower.value scope value
  let statement ← Lower.pushRow .stmtValue [valueRow]
  let block ← Lower.pushBlock owner 1 [1, statement]
  let span ← Lower.takeSpan
  Lower.pushRow .exprValueBlock [block.raw, span]

/-- An atom of the typed integer family that assignments use. -/
def Lower.intAtom (scope : List (String × Nat)) : SourceAtom → Lower Word32
  | .int value => do Lower.pushRow .intInt (← Lower.literalWords value)
  | .var name => do
      let slot ← Lower.lift (scope.lookup name)
      Lower.pushRow .intSlot [← Lower.word slot]

def Lower.intValue (scope : List (String × Nat)) : SourceValue → Lower Word32
  | .atom single => Lower.intAtom scope single
  | .binary op left right => do
      Lower.require (!op.isComparison)
      let leftRow ← Lower.intAtom scope left
      let rightRow ← Lower.intAtom scope right
      let operation ← Lower.operationIndex op
      Lower.pushRow .intBinary [operation, leftRow, rightRow]

/-- The assignment statement of a function's var: its value in the integer family, then the
statement with the slot, the plain-assignment word and the span. -/
def Lower.assignment (scope : List (String × Nat)) (target : String) (value : SourceValue) :
    Lower Word32 := do
  let slot ← Lower.lift (scope.lookup target)
  let valueRow ← Lower.intValue scope value
  let span ← Lower.takeSpan
  Lower.pushRow .stmtAssignInt [← Lower.word slot, 0, valueRow, span]

/-- The assignment statement a function's returned form carries, if it carries one: it must
assign the function's one var. -/
def Lower.assignments (name : String) (scope : List (String × Nat)) :
    SourceExpression → Lower (List Word32)
  | .assigned target value _ => do
      Lower.require (target == name)
      pure [← Lower.assignment scope target value]
  | _ => pure []

/-- The call row of an earlier callable function: the argument rows, their table, the span and
the direct call. -/
def Lower.pureCall (owner : BridgeId) (scope : List (String × Nat)) (callee : String)
    (arguments : List SourceAtom) : Lower Word32 := do
  let callees := (← get).functions
  let index ← Lower.lift (callees.findIdx? (·.name == callee))
  let found ← Lower.lift callees[index]?
  Lower.require ((← get).callable[index]? == some true)
  Lower.require (arguments.length == found.paramsLen.val)
  let rows ← arguments.mapM (Lower.atom scope)
  let count ← Lower.word arguments.length
  let table ← Lower.pushBlock owner 0 (count :: rows.flatMap fun row => [0, row])
  let span ← Lower.takeSpan
  Lower.pushRow .exprDirectPureCall [← Lower.word (index + 1), table.raw, span]

def Lower.expression (owner : BridgeId) (scope : List (String × Nat)) :
    SourceExpression → Lower Word32
  | .value value => Lower.value scope value
  | .call callee arguments => Lower.pureCall owner scope callee arguments
  | .conditional op left right thenValue elseValue => do
      Lower.require op.isComparison
      let condition ← Lower.comparison scope op left right
      let thenRow ← Lower.valueBlock owner scope thenValue
      let table ← Lower.pushBlock owner 0 [1, condition, thenRow]
      let elseRow ← Lower.valueBlock owner scope elseValue
      let span ← Lower.takeSpan
      Lower.pushRow .exprIf [table.raw, elseRow, span]
  | .assigned _ _ result => Lower.value scope result
  | .comparison op left right => do
      Lower.require op.isComparison
      Lower.comparison scope op left right
  | .constructed isOk value => do
      let valueRow ← Lower.value scope value
      let row ← Lower.pushRow (if isOk then .exprOk else .exprErr)
        (if isOk then [valueRow] else [valueRow, 0])
      pure row
  | .computedLet _ _ result => Lower.value scope result
  | .computedLets _ result => Lower.value scope result

/-- The rows after a function's let statements: the rows of its returned expression, the
return, its body block, and its function row. `extraSlots` counts bindings emitted as statements
that are not in the function's literal lets. -/
def Lower.functionTail (function : SourceFunction) (scope : List (String × Nat))
    (owner : BridgeId) (functionsBefore parametersStart extraSlots : Nat)
    (statements : List Word32) :
    Lower Unit := do
  let result ← Lower.expression owner scope function.result
  let returned ← Lower.pushRow .stmtReturn [result]
  let count ← Lower.word (statements.length + 1)
  let body ← Lower.pushBlock owner 3 (count :: (statements ++ [returned]))
  let parameters ← Lower.word function.parameters.length
  let row : FunctionRow := ⟨owner, function.name, ← Lower.word parametersStart, parameters,
    ← Lower.word functionsBefore, 1, body,
    ← Lower.word (function.parameters.length + function.lets.length +
      (if function.propagation.isSome then 1 else 0) + extraSlots + 1)⟩
  modify fun state =>
    { state with functions := state.functions ++ [row],
                 callable := state.callable ++ [function.callable] }

/-- The computed-let statements of a sequence, in source order: each value's integer-family rows
and its `StmtLetInt` binding at the slot the exporter reserves, so a later value reads an earlier
binding at the earlier binding's slot. -/
def Lower.computedBindings (parameter : String) (earlier : List (String × Nat)) :
    Nat → List (String × SourceValue) → Lower (List Word32)
  | _, [] => pure []
  | slot, (name, value) :: rest => do
      let valueRow ← Lower.intValue ((parameter, 0) :: earlier) value
      let slotWord ← Lower.word slot
      let binding ← Lower.pushRow .stmtLetInt [slotWord, valueRow]
      let others ← Lower.computedBindings parameter (earlier ++ [(name, slot)]) (slot + 1) rest
      pure (binding :: others)

/-- The computed-let statements a function emits before its returned expression: the integer value
rows and the `StmtLetInt` binding at the slot the exporter uses. A function whose result is not a
computed let has none. -/
def Lower.computedLetStatements (function : SourceFunction) (scope : List (String × Nat)) :
    Lower (List Word32) :=
  match function.result with
  | .computedLet name value _ =>
      if function.lets.isEmpty then
        match function.parameters with
        | [_] => do
            let valueRow ← Lower.intValue (scope.filter fun entry => entry.1 != name) value
            let binding ← Lower.pushRow .stmtLetInt [← Lower.word 2, valueRow]
            pure [binding]
        | _ => Lower.fail
      else Lower.fail
  | .computedLets [] _ => Lower.fail
  | .computedLets bindings _ =>
      if function.lets.isEmpty then
        match function.parameters with
        | [parameter] => Lower.computedBindings parameter [] 2 bindings
        | _ => Lower.fail
      else Lower.fail
  | _ => pure []

/-- A propagation binding: the callee call row, the `?` row over it, and the `StmtLet` row that
binds the payload to the target slot. -/
def Lower.propagation (owner : BridgeId) (scope : List (String × Nat))
    (propagation : SourcePropagation) : Lower Word32 := do
  let call ← Lower.expression owner scope (.call propagation.callee propagation.arguments)
  let span ← Lower.takeSpan
  let tried ← Lower.pushRow .exprTry [call, span]
  let slot ← Lower.lift (scope.lookup propagation.target)
  Lower.pushRow .stmtLet [← Lower.word slot, tried, 0]

/-- Lower one function in the observed row order: its integer let, the rows of its returned
expression, the return, and then its body block. A propagation function lowers its binding before
the return, and a computed let lowers its integer value rows before the binding. -/
def Lower.function (function : SourceFunction) : Lower Unit := do
  let scope ← Lower.lift function.scope?
  let before ← get
  let owner ← Lower.lift (BridgeId.fromIndex? before.functions.length)
  let parametersStart := (before.functions.map (·.paramsLen.val)).sum
  match function.propagation with
  | some propagation => do
      Lower.require (function.lets.isEmpty && !function.result.isAssigned)
      let statement ← Lower.propagation owner scope propagation
      Lower.functionTail function scope owner before.functions.length parametersStart 0 [statement]
  | none =>
    match function.lets with
    | [] => do
        let computed ← Lower.computedLetStatements function scope
        Lower.require (!function.result.isAssigned)
        Lower.functionTail function scope owner before.functions.length parametersStart
          computed.length computed
    | [(name, value)] => do
        let computed ← Lower.computedLetStatements function scope
        Lower.require computed.isEmpty
        match function.parameters with
        | [] =>
            let literal ← Lower.pushRow .intInt (← Lower.literalWords value)
            let binding ← Lower.pushRow .stmtLetInt [← Lower.word letSlot, literal]
            let assigned ← Lower.assignments name scope function.result
            Lower.functionTail function scope owner before.functions.length parametersStart 0
              (binding :: assigned)
        | [_] =>
            let literal ← Lower.pushRow .intInt (← Lower.literalWords value)
            let binding ← Lower.pushRow .stmtLetInt [← Lower.word 2, literal]
            let assigned ← Lower.assignments name scope function.result
            Lower.functionTail function scope owner before.functions.length parametersStart 0
              (binding :: assigned)
        | _ => Lower.fail
    | _ => Lower.fail

def lowerFunctions (spans : List Word32) (functions : List SourceFunction) :
    Option LoweringState := do
  check (decide (functions.map (·.name)).Nodup)
  let ((), state) ← (functions.forM Lower.function).run { spans }
  check state.spans.isEmpty
  pure state

/-- The exchanged artifact a lowering state holds. -/
def LoweringState.toArtifact (state : LoweringState) (sourceId : BridgeId)
    (observations : List Observation) : RawArtifact :=
  ⟨sourceId, state.rows, state.extra, state.ops, state.blocks, state.functions, observations⟩

/-- The span word of each row that carries one, in table order. -/
def spanWords (artifact : RawArtifact) : List Word32 :=
  artifact.rows.filterMap fun row =>
    let word (offset : Nat) : Option Word32 := artifact.extra[row.lhs.val + offset]?
    match row.tag with
    | .exprBinary => word 3
    | .exprValueBlock => word 1
    | .exprIf => word 2
    | .exprDirectPureCall => word 2
    | .exprTry => word 1
    | .stmtAssignInt => word 3
    | .intInt | .stmtLetInt | .exprInt | .exprParam | .stmtReturn | .stmtValue | .intSlot
    | .intBinary | .exprOk | .exprErr | .stmtLet => none

/-- A source function agrees with the artifact on the given arguments when it and the function
of that name have the same defined outcome there, under the source tree and under the IR. -/
def agreesOn (artifact : RawArtifact) (name : String) (program : Option FunctionProgram)
    (arguments : List Int) : Bool :=
  match artifact.functions.find? (·.name == name) with
  | some row =>
      (program.bind (·.outcome? arguments)).isSome &&
        program.bind (·.outcome? arguments) == functionOutcome? artifact row arguments
  | none => false

/-- Every function without parameters means the same under the source and the IR, and every
recorded call means the same on its recorded arguments. -/
def sourceAgrees (artifact : RawArtifact) (functions : List SourceFunction) : Bool :=
  let programs := functions.zip (sourcePrograms functions)
  programs.all (fun (function, program) =>
    !function.parameters.isEmpty || agreesOn artifact function.name program []) &&
  artifact.observations.all fun observation =>
    match artifact.functions.find? (·.id == observation.function) with
    | some row =>
        programs.any fun (function, program) =>
          function.name == row.name &&
            agreesOn artifact row.name program observation.integerArguments
    | none => false

/-- Lowering the parsed functions reproduces the artifact exactly, taking the span words from
the artifact. -/
def lowersTo (artifact : RawArtifact) (functions : List SourceFunction) : Bool :=
  match lowerFunctions (spanWords artifact) functions with
  | some state => state.toArtifact artifact.sourceId artifact.observations == artifact
  | none => false

/-- Certification of a decoded pair: the source text parses in the admitted productions,
lowering the parsed functions reproduces the raw-IR document's rows, pools, blocks and function
table exactly (the span words are taken from the document), the document names this source, the
artifact validates, and each observation is the outcome of its call. That the functions mean the
same under the source and the IR is not checked here: `lowering_means_the_source` proves it for
every pair the lowerer reproduces. -/
def certifyCodes (sourceId : BridgeId) (textCodes : List Nat) (raw : RawArtifact) :
    Option (List ObservedResult) := do
  let functions ← parseSourceCodes textCodes
  if decide (raw.sourceId = sourceId) && lowersTo raw functions && validateArtifact raw then
    namedResults raw
  else
    none

def certifyDecoded (source : SourceDocument) (raw : RawArtifact) :
    Option (List ObservedResult) :=
  certifyCodes source.id (source.text.toList.map Char.toNat) raw

/-- The same validation from the exchanged bytes of both documents. -/
def certifyFromSource (sourceText rawText : ExchangeCodes) :
    Option (List ObservedResult) := do
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
    (valid : validateArtifact raw = true) :
    certifyDecoded source raw = namedResults raw := by
  unfold certifyDecoded certifyCodes
  have parsedCodes : parseSourceCodes (source.text.toList.map Char.toNat) = some functions :=
    parsed
  simp only [parsedCodes, Option.bind_some, Option.bind_eq_bind]
  simp [same, lowers, valid]

theorem certify_decoded_sound {source : SourceDocument} {raw : RawArtifact}
    {results : List ObservedResult}
    (certified : certifyDecoded source raw = some results) :
    ∃ functions, raw.sourceId = source.id ∧ parseSource source.text = some functions ∧
      lowersTo raw functions = true ∧ validateArtifact raw = true ∧
      namedResults raw = some results := by
  unfold certifyDecoded certifyCodes at certified
  cases parsed : parseSourceCodes (source.text.toList.map Char.toNat) with
  | none => simp [parsed] at certified
  | some functions =>
  simp only [parsed, Option.bind_some, Option.bind_eq_bind] at certified
  split at certified
  · rename_i checks
    simp only [Bool.and_eq_true, decide_eq_true_eq] at checks
    exact ⟨functions, checks.1.1, parsed, checks.1.2, checks.2, certified⟩
  · cases certified

theorem certify_from_source_sound {sourceText rawText : ExchangeCodes}
    {results : List ObservedResult}
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

end Xsh.Conformance.Vertical
