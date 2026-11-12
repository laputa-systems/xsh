import Xsh.Conformance.Vertical.FactsCertificate
import Xsh.Conformance.Vertical.LoweringSound
import Xsh.Conformance.Vertical.SourceAdmissionSound

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

namespace FactsIR

mutual
inductive Rows (F : LoweringState) (scope : Scope) (slots : List (String × Nat)) :
    List NodeFact → Word32 → Prop where
  | literal {value : Nat} {row low high : Word32} :
      F.RowAt row.val .exprInt [low, high] →
      low.val = value % 4294967296 → high.val = value / 4294967296 →
      Rows F scope slots [.literal value] row
  | read {name : String} {resolution : Resolution} {slot : Nat} {word row : Word32} :
      scope.lookup name = some resolution → slots.lookup name = some slot → word.val = slot →
      F.RowAt row.val .exprParam [word] → Rows F scope slots [.read resolution] row
  | operation {op : FragmentOp} {result : FactType} {left right : List NodeFact}
      {a b row operation span : Word32} :
      Rows F scope slots left a → Rows F scope slots right b →
      F.RowAt row.val .exprBinary [operation, a, b, span] →
      F.ops[operation.val]? = some op →
      Rows F scope slots (left ++ right ++ [.operate op .int result]) row
  | call {arguments : List (List NodeFact)} {rows : List Word32}
      {index count : Nat} {row function table span : Word32} {countWord : Word32} :
      ArgumentsRows F scope slots arguments rows →
      function.val = index + 1 → countWord.val = count → rows.length = count →
      F.BlockAt table (countWord :: rows.flatMap fun row => [0, row]) →
      F.RowAt row.val .exprDirectPureCall [function, table, span] →
      Rows F scope slots (arguments.flatten ++ [.call index count]) row
  | conditional {condition thenFacts elseFacts : List NodeFact}
      {conditionRow thenRow elseRow row table span : Word32} :
      Rows F scope slots condition conditionRow →
      ValueBlockRows F scope slots thenFacts thenRow →
      ValueBlockRows F scope slots elseFacts elseRow →
      F.BlockAt table [1, conditionRow, thenRow] →
      F.RowAt row.val .exprIf [table, elseRow, span] →
      Rows F scope slots (condition ++ thenFacts ++ elseFacts ++ [.branch]) row
inductive ValueBlockRows (F : LoweringState) (scope : Scope) (slots : List (String × Nat)) :
    List NodeFact → Word32 → Prop where
  | valueBlock {nodes : List NodeFact} {valueRow statement block row span : Word32} :
      Rows F scope slots nodes valueRow →
      F.RowAt statement.val .stmtValue [valueRow] → F.BlockAt block [1, statement] →
      F.RowAt row.val .exprValueBlock [block, span] →
      ValueBlockRows F scope slots nodes row
inductive ArgumentsRows (F : LoweringState) (scope : Scope) (slots : List (String × Nat)) :
    List (List NodeFact) → List Word32 → Prop where
  | nil : ArgumentsRows F scope slots [] []
  | cons {nodes : List NodeFact} {rest : List (List NodeFact)} {row : Word32} {rows : List Word32} :
      Rows F scope slots nodes row → ArgumentsRows F scope slots rest rows →
      ArgumentsRows F scope slots (nodes :: rest) (row :: rows)
end


theorem atom_rows {scope : Scope} {slots : List (String × Nat)} {atom : SourceAtom}
    {nodes : List NodeFact} {s s' : LoweringState} {row : Word32}
    (facts : AtomHolds scope atom nodes) (run : Lower.atom slots atom s = some (row, s')) :
    ∀ F, StateLE s' F → Rows F scope slots nodes row := by
  cases facts with
  | literal valid =>
    simp only [Lower.atom, run_bind] at run
    obtain ⟨words, s1, literal, push⟩ := run
    obtain ⟨_, rfl, low, high, rfl, lowEq, highEq⟩ := literal_words_run literal
    intro F le
    exact .literal ((push_row_at push rfl).mono le) lowEq highEq
  | @read name resolution looked =>
    simp only [Lower.atom, run_bind, run_lift, Lower.word] at run
    obtain ⟨slot, _, ⟨found, rfl⟩, w, _, ⟨decoded, rfl⟩, push⟩ := run
    intro F le
    exact .read looked found (decode_val decoded) ((push_row_at push rfl).mono le)

theorem comparison_rows {scope : Scope} {slots : List (String × Nat)}
    {op : FragmentOp} {left right : SourceAtom} {ln rn : List NodeFact}
    {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s)
    (lf : AtomHolds scope left ln) (rf : AtomHolds scope right rn)
    (run : Lower.comparison slots op left right s = some (row, s')) (result : FactType) :
    ∀ F, StateLE s' F → Rows F scope slots (ln ++ rn ++ [.operate op .int result]) row := by
  simp only [Lower.comparison, run_bind] at run
  obtain ⟨a, s1, ha, b, s2, hb, operation, s3, hop, span, s4, hspan, push⟩ := run
  obtain ⟨_, _, al⟩ := atom_lowers (cs := []) wf ha
  obtain ⟨_, _, bl⟩ := atom_lowers (cs := []) al.wf hb
  obtain ⟨opLe, pooled, _, _, _, _, _⟩ := operation_index_run hop
  obtain ⟨rest, _, rfl⟩ := take_span_run hspan
  have spanLe : StateLE s3 { s3 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
  intro F le
  have afterOp := spanLe.trans ((push_row_le push).trans le)
  have afterRight := opLe.trans afterOp
  exact .operation (atom_rows lf ha F (bl.le.trans afterRight))
    (atom_rows rf hb F afterRight) ((push_row_at push rfl).mono le)
    (op_mono afterOp pooled)

theorem value_rows {scope : Scope} {slots : List (String × Nat)} {value : SourceValue}
    {nodes : List NodeFact} {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s)
    (facts : ValueHolds scope value nodes) (run : Lower.value slots value s = some (row, s')) :
    ∀ F, StateLE s' F → Rows F scope slots nodes row := by
  cases facts with
  | atom holds => exact atom_rows holds run
  | arithmetic plain left right =>
    simp only [Lower.value, run_bind, run_require] at run
    obtain ⟨_, _, ⟨_, rfl⟩, run⟩ := run
    exact comparison_rows wf left right run .int

theorem value_block_rows {scope : Scope} {slots : List (String × Nat)} {value : SourceValue}
    {nodes : List NodeFact} {s s' : LoweringState} {row : Word32} {owner : BridgeId}
    (wf : BlocksIndexed s) (facts : ValueHolds scope value nodes)
    (run : Lower.valueBlock owner slots value s = some (row, s')) :
    ∀ F, StateLE s' F → ValueBlockRows F scope slots nodes row := by
  simp only [Lower.valueBlock, run_bind] at run
  obtain ⟨valueRow, s1, hv, statement, s2, hs, block, s3, hb, span, s4, hspan, push⟩ := run
  obtain ⟨_, _, vl⟩ := value_lowers (cs := []) wf hv
  obtain ⟨rest, _, rfl⟩ := take_span_run hspan
  have wf2 : BlocksIndexed s2 := BlocksIndexed.congr (push_row_blocks hs).1 vl.wf
  have blockLe := (push_block_le hb).1
  have spanLe : StateLE s3 { s3 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
  intro F le
  have afterBlock := spanLe.trans ((push_row_le push).trans le)
  have afterStatement := blockLe.trans afterBlock
  exact .valueBlock (value_rows wf facts hv F ((push_row_le hs).trans afterStatement))
    ((push_row_at hs rfl).mono afterStatement) ((push_block_at wf2 hb).mono afterBlock)
    ((push_row_at push rfl).mono le)

theorem arguments_rows {scope : Scope} {slots : List (String × Nat)} :
    ∀ {atoms : List SourceAtom} {nodes : List NodeFact} {s s' : LoweringState}
      {rows : List Word32}, BlocksIndexed s → AtomsHold scope atoms nodes →
      (atoms.mapM (Lower.atom slots)) s = some (rows, s') →
      ∃ parts, parts.flatten = nodes ∧ ∀ F, StateLE s' F → ArgumentsRows F scope slots parts rows := by
  intro atoms
  induction atoms with
  | nil =>
    intro nodes s s' rows wf facts run
    cases facts
    simp only [List.mapM_nil, run_pure] at run
    obtain ⟨rfl, rfl⟩ := run
    exact ⟨[], rfl, fun _ _ => .nil⟩
  | cons atom rest ih =>
    intro nodes s s' rows wf facts run
    cases facts with
    | @cons _ _ firstNodes _ first others =>
      simp only [List.mapM_cons, run_bind, run_pure] at run
      obtain ⟨row, s1, ha, tail, _, hr, rfl, rfl⟩ := run
      obtain ⟨_, _, al⟩ := atom_lowers (cs := []) wf ha
      obtain ⟨_, _, le, _, _, _, _, _, _⟩ := atoms_lower (cs := []) al.wf hr
      obtain ⟨parts, joined, tailRows⟩ := ih al.wf others hr
      refine ⟨firstNodes :: parts, by simp [joined], ?_⟩
      intro F leF
      exact .cons (atom_rows first ha F (le.trans leF)) (tailRows F leF)

theorem find_index_of_names (name : String) :
    ∀ (functions : List FunctionRow) (callees : Callees),
      functions.map (·.name) = callees.map (·.1) →
      functions.findIdx? (·.name == name) = callees.findIdx? (fun entry => entry.1 == name) := by
  intro functions
  induction functions with
  | nil => intro callees names; cases callees <;> simp_all
  | cons head rest ih =>
    intro callees names
    cases callees with
    | nil => simp at names
    | cons entry others =>
      simp only [List.map_cons, List.cons.injEq] at names
      simp only [List.findIdx?_cons, names.1, ih others names.2]

theorem expression_rows {callees : Callees} {scope : Scope} {slots : List (String × Nat)}
    {expression : SourceExpression} {nodes : List NodeFact} {s s' : LoweringState}
    {row : Word32} {owner : BridgeId} (wf : BlocksIndexed s)
    (names : s.functions.map (·.name) = callees.map (·.1))
    (facts : ExpressionHolds callees scope expression nodes)
    (run : Lower.expression owner slots expression s = some (row, s')) :
    ∀ F, StateLE s' F → Rows F scope slots nodes row := by
  cases facts with
  | value holds => exact value_rows wf holds run
  | @call callee arguments index arity argumentNodes selected arityFound count argumentFacts =>
    simp only [Lower.expression, Lower.pureCall, run_bind, run_get, run_lift, run_require,
      Lower.word] at run
    obtain ⟨_, _, ⟨rfl, rfl⟩, index', _, ⟨foundIndex, rfl⟩, found, _, ⟨foundRow, rfl⟩,
      _, _, ⟨rfl, rfl⟩, _, _, ⟨_, rfl⟩, _, _, ⟨_, rfl⟩, rows, s7, hrows,
      countWord, _, ⟨hcount, rfl⟩, table, s9, htable, span, s10, hspan,
      function, _, ⟨hfunction, rfl⟩, push⟩ := run
    have sameIndex : index' = index := by
      rw [find_index_of_names callee _ _ names, selected] at foundIndex
      exact Option.some.inj foundIndex.symm
    subst sameIndex
    obtain ⟨_, _, argsLe, argsWf, _, rowsLength, _, _, _⟩ := atoms_lower (cs := []) wf hrows
    obtain ⟨parts, joined, argumentsAt⟩ := arguments_rows wf argumentFacts hrows
    obtain ⟨rest, _, rfl⟩ := take_span_run hspan
    have spanLe : StateLE s9 { s9 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
    intro F le
    have afterBlock := spanLe.trans ((push_row_le push).trans le)
    have afterArgs := (push_block_le htable).1.trans afterBlock
    rw [← joined]
    exact .call (argumentsAt F afterArgs) (decode_val hfunction) (decode_val hcount)
      rowsLength ((push_block_at argsWf htable).mono afterBlock)
      ((push_row_at push rfl).mono le)
  | conditional compared left right thenFacts elseFacts =>
    simp only [Lower.expression, run_bind, run_require] at run
    obtain ⟨_, _, ⟨_, rfl⟩, condition, s2, hcondition, thenRow, s3, hthen,
      table, s4, htable, elseRow, s5, helse, span, s6, hspan, push⟩ := run
    obtain ⟨_, _, _, _, cl⟩ := comparison_lowers (cs := []) wf hcondition
    obtain ⟨_, _, tl⟩ := value_block_lowers (cs := []) cl.wf hthen
    have wf4 := push_block_indexed tl.wf htable
    obtain ⟨_, _, el⟩ := value_block_lowers (cs := []) wf4 helse
    obtain ⟨rest, _, rfl⟩ := take_span_run hspan
    have spanLe : StateLE s5 { s5 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
    intro F le
    have afterElse := spanLe.trans ((push_row_le push).trans le)
    have afterTable := el.le.trans afterElse
    have afterThen := (push_block_le htable).1.trans afterTable
    have afterCondition := tl.le.trans afterThen
    apply Rows.conditional
      (comparison_rows wf left right hcondition .bool F afterCondition)
      (value_block_rows cl.wf thenFacts hthen F afterThen)
      (value_block_rows wf4 elseFacts helse F afterElse)
      ((push_block_at tl.wf htable).mono afterTable)
      ((push_row_at push rfl).mono le)


inductive LocalRows (F : LoweringState) (scope : Scope) (slots : List (String × Nat)) :
    List (String × Nat) → List Word32 → Prop where
  | nil : LocalRows F scope slots [] []
  | literal {name : String} {value slot : Nat} {word low high literal statement : Word32} :
      scope.lookup name = some (.letBinding 0) → slots.lookup name = some slot → word.val = slot →
      F.RowAt literal.val .intInt [low, high] → low.val = value % 4294967296 →
      high.val = value / 4294967296 → F.RowAt statement.val .stmtLetInt [word, literal] →
      LocalRows F scope slots [(name, value)] [statement]

def FunctionRows (F : LoweringState) (function : SourceFunction) (facts : FunctionFacts)
    (row : FunctionRow) : Prop :=
  row.name = facts.name ∧ row.paramsLen.val = facts.parameterTypes.length ∧
    facts.name = function.name ∧ facts.parameterTypes = function.parameters.map (fun _ => .int) ∧
    facts.localTypes = function.lets.map (fun _ => .int) ∧ facts.resultType = .int ∧
    facts.effects = [] ∧ ∃ scope slots operand returned count statements,
      function.scope = some scope ∧ function.scope? = some slots ∧
      LocalRows F scope slots function.lets statements ∧
      Rows F scope slots facts.nodes operand ∧
      F.RowAt returned.val .stmtReturn [operand] ∧
      count.val = statements.length + 1 ∧
      F.BlockAt row.body.raw (count :: (statements ++ [returned]))

theorem expression_plain {callees : Callees} {scope : Scope} {expression : SourceExpression}
    {nodes : List NodeFact} (facts : ExpressionHolds callees scope expression nodes) :
    expression.isAssigned = false ∧
      (∀ name value result, expression ≠ .computedLet name value result) ∧
      (∀ bindings result, expression ≠ .computedLets bindings result) := by
  cases facts <;> exact ⟨rfl, by intros; simp, by intros; simp⟩

theorem tail_rows {src : List (String × Option Callee)} {callees : Callees}
    {f : SourceFunction} {scope : Scope} {slots : List (String × Nat)} {nodes : List NodeFact}
    {owner : BridgeId} {s s' : LoweringState} {functionsBefore parametersStart extraSlots : Nat}
    {statements : List Word32} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (names : s.functions.map (·.name) = callees.map (·.1))
    (facts : ExpressionHolds callees scope f.result nodes)
    (run : Lower.functionTail f slots owner functionsBefore parametersStart extraSlots statements s =
      some ((), s')) :
    ∃ row, s'.functions = s.functions ++ [row] ∧ row.name = f.name ∧
      row.paramsLen.val = f.parameters.length ∧ StateLE s s' ∧ ∀ F, StateLE s' F →
        ∃ operand returned count, Rows F scope slots nodes operand ∧
          F.RowAt returned.val .stmtReturn [operand] ∧ count.val = statements.length + 1 ∧
          F.BlockAt row.body.raw (count :: (statements ++ [returned])) := by
  simp only [Lower.functionTail, run_bind, run_modify, Lower.word, run_lift] at run
  obtain ⟨result, s1, hexpr, returned, s2, hret, count, s3, ⟨hcount, rfl⟩, body, s4, hbody,
    parameters, s5, ⟨hparams, rfl⟩, pStart, s6, ⟨hpstart, rfl⟩, fBefore, s7, ⟨hfb, rfl⟩,
    slotCount, s8, ⟨hslots, rfl⟩, rfl⟩ := run
  obtain ⟨e, hsrc, lw⟩ := expression_lowers wf aligned hexpr
  have retFrame := push_row_frame hret
  have wf2 := BlocksIndexed.congr (push_row_blocks hret).1 lw.wf
  have bodyFrame := push_block_frame hbody
  have bodyLe := (push_block_le hbody).1
  refine ⟨⟨owner, f.name, pStart, parameters, fBefore, 1, body, slotCount⟩, ?_, rfl,
    decode_val hparams, lw.le.trans ((push_row_le hret).trans
      (bodyLe.trans (StateLE.of_eq rfl rfl rfl rfl))), ?_⟩
  · show _ ++ _ = _
    rw [bodyFrame.1, retFrame.1, lw.functions.1]
  · intro F le
    have toF : StateLE s8 F := StateLE.of_update le
    exact ⟨result, returned, count,
      expression_rows wf names facts hexpr F ((push_row_le hret).trans (bodyLe.trans toF)),
      (push_row_at hret rfl).mono (bodyLe.trans toF), decode_val hcount,
      (push_block_at wf2 hbody).mono toF⟩


theorem scope_eq {function : SourceFunction} {scope : Scope}
    (found : function.scope = some scope) :
    scope = (function.parameters.zipIdx.map fun (name, index) => (name, .parameter index)) ++
      (function.lets.zipIdx.map fun ((name, _), index) => (name, .letBinding index)) := by
  unfold SourceFunction.scope at found
  dsimp only at found
  split at found
  · split at found
    · exact (Option.some.inj found).symm
    · simp at found
  · simp at found

theorem finish_rows {src : List (String × Option Callee)} {callees : Callees}
    {f : SourceFunction} {scope : Scope} {slots : List (String × Nat)} {nodes : List NodeFact}
    {owner : BridgeId} {s s' : LoweringState} {functionsBefore parametersStart extraSlots : Nat}
    {statements : List Word32} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (names : s.functions.map (·.name) = callees.map (·.1))
    (facts : ExpressionHolds callees scope f.result nodes)
    (sourceScope : f.scope = some scope) (irScope : f.scope? = some slots)
    (locals : ∀ F, StateLE s F → LocalRows F scope slots f.lets statements)
    (run : Lower.functionTail f slots owner functionsBefore parametersStart extraSlots statements s =
      some ((), s')) :
    ∃ row, s'.functions = s.functions ++ [row] ∧ ∀ F, StateLE s' F →
      FunctionRows F f ⟨f.name, f.parameters.map (fun _ => .int), f.lets.map (fun _ => .int),
        .int, [], nodes⟩ row := by
  obtain ⟨row, appended, name, arity, le, body⟩ := tail_rows wf aligned names facts run
  refine ⟨row, appended, ?_⟩
  intro F leF
  obtain ⟨operand, returned, count, nodesAt, returnAt, counted, bodyAt⟩ := body F leF
  exact ⟨name, by simpa using arity, rfl, rfl, rfl, rfl, rfl,
    scope, slots, operand, returned, count, statements, sourceScope, irScope,
    locals F (le.trans leF), nodesAt, returnAt, counted, bodyAt⟩


theorem single_let_rows {src : List (String × Option Callee)} {callees : Callees}
    {f : SourceFunction} {scope : Scope} {slots : List (String × Nat)} {nodes : List NodeFact}
    {owner : BridgeId} {s s' : LoweringState} {functionsBefore parametersStart slot : Nat}
    {name : String} {value : Nat} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (names : s.functions.map (·.name) = callees.map (·.1))
    (facts : ExpressionHolds callees scope f.result nodes)
    (sourceScope : f.scope = some scope) (irScope : f.scope? = some slots)
    (oneLet : f.lets = [(name, value)])
    (sourceSlot : scope.lookup name = some (.letBinding 0))
    (targetSlot : slots.lookup name = some slot)
    (run : (do
      let literal ← Lower.pushRow .intInt (← Lower.literalWords value)
      let statement ← Lower.pushRow .stmtLetInt [← Lower.word slot, literal]
      Lower.functionTail f slots owner functionsBefore parametersStart 0 [statement]) s =
      some ((), s')) :
    ∃ row, s'.functions = s.functions ++ [row] ∧ ∀ F, StateLE s' F →
      FunctionRows F f ⟨f.name, f.parameters.map (fun _ => .int), f.lets.map (fun _ => .int),
        .int, [], nodes⟩ row := by
  simp only [run_bind, Lower.word, run_lift] at run
  obtain ⟨words, _, hlit, literal, _, hpush1, word, _, ⟨hword, rfl⟩, statement, s2,
    hpush2, tail⟩ := run
  obtain ⟨_, rfl, low, high, rfl, lowEq, highEq⟩ := literal_words_run hlit
  have frame1 := push_row_frame hpush1
  have frame2 := push_row_frame hpush2
  have wf2 := BlocksIndexed.congr (push_row_blocks hpush2).1
    (BlocksIndexed.congr (push_row_blocks hpush1).1 wf)
  have aligned2 := Aligned.congr (by rw [frame2.1, frame1.1])
    (by rw [frame2.2, frame1.2]) aligned
  have names2 : s2.functions.map (·.name) = callees.map (·.1) := by
    rw [frame2.1, frame1.1]; exact names
  obtain ⟨row, appended, matched⟩ := finish_rows wf2 aligned2 names2 facts sourceScope irScope
    (statements := [statement]) (fun F le => by
      rw [oneLet]
      exact .literal sourceSlot targetSlot (decode_val hword)
        ((push_row_at hpush1 rfl).mono ((push_row_le hpush2).trans le)) lowEq highEq
        ((push_row_at hpush2 rfl).mono le)) tail
  exact ⟨row, by rw [appended, frame2.1, frame1.1], matched⟩

theorem function_rows {src : List (String × Option Callee)} {callees : Callees}
    {f : SourceFunction} {facts : FunctionFacts} {s s' : LoweringState}
    (wf : BlocksIndexed s) (aligned : Aligned s src)
    (names : s.functions.map (·.name) = callees.map (·.1))
    (holds : FunctionHolds callees f facts) (run : Lower.function f s = some ((), s')) :
    ∃ row, s'.functions = s.functions ++ [row] ∧ ∀ F, StateLE s' F → FunctionRows F f facts row := by
  obtain ⟨absent, scope, nodes, sourceScope, expressionFacts, rfl⟩ := holds
  obtain ⟨plain, _, _⟩ := expression_plain expressionFacts
  have computedNone : ∀ slots, Lower.computedLetStatements f slots = pure [] := by
    intro slots
    unfold Lower.computedLetStatements
    generalize eq : f.result = expression at expressionFacts ⊢
    cases expressionFacts <;> rfl
  cases letsEq : f.lets with
  | nil =>
    simp only [Lower.function, absent, letsEq, computedNone, run_bind, run_lift, run_get,
      run_pure, run_require] at run
    obtain ⟨slots, _, ⟨irScope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨_, rfl⟩, computed, _,
      ⟨rfl, rfl⟩, _, _, ⟨_, rfl⟩, tail⟩ := run
    simpa only [letsEq] using finish_rows wf aligned names expressionFacts sourceScope irScope
      (fun _ _ => by rw [letsEq]; exact .nil) tail
  | cons binding more =>
    obtain ⟨bindingName, value⟩ := binding
    cases more with
    | nil =>
      have assignmentsNone : ∀ slots, Lower.assignments bindingName slots f.result = pure [] := by
        intro slots
        exact assignments_plain plain
      cases parametersEq : f.parameters with
      | nil =>
        simp only [Lower.function, absent, letsEq, parametersEq, computedNone, run_bind,
          run_lift, run_get, run_pure, run_require, assignmentsNone] at run
        obtain ⟨slots, _, ⟨irScope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨_, rfl⟩, computed, _,
          ⟨rfl, rfl⟩, _, _, ⟨_, rfl⟩, words, _, hlit, literal, _, hpush1, word, _, hword,
          statement, _, hpush2, assigned, _, ⟨rfl, rfl⟩, tail⟩ := run
        have scopeEq := scope_eq sourceScope
        simp only [parametersEq, letsEq, List.zipIdx_cons, List.zipIdx_nil, List.map_cons,
          List.map_nil, List.nil_append] at scopeEq
        have slotsEq : slots = [(bindingName, letSlot)] := by
          simpa [SourceFunction.scope?, parametersEq, letsEq, absent] using irScope.symm
        have completed := single_let_rows (slot := letSlot) wf aligned names expressionFacts
          sourceScope irScope letsEq (by rw [scopeEq]; simp) (by rw [slotsEq]; simp)
          (by
            simp only [run_bind]
            exact ⟨words, _, hlit, literal, _, hpush1, word, _, hword, statement, _, hpush2, tail⟩)
        simpa only [parametersEq, letsEq] using completed
      | cons parameter rest =>
        cases rest with
        | nil =>
          simp only [Lower.function, absent, letsEq, parametersEq, computedNone, run_bind,
            run_lift, run_get, run_pure, run_require, assignmentsNone] at run
          obtain ⟨slots, _, ⟨irScope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨_, rfl⟩, computed, _,
            ⟨rfl, rfl⟩, _, _, ⟨_, rfl⟩, words, _, hlit, literal, _, hpush1, word, _, hword,
            statement, _, hpush2, assigned, _, ⟨rfl, rfl⟩, tail⟩ := run
          have parts : f.result.isAssigned = false ∧ ¬parameter = bindingName ∧
              slots = [(parameter, 0), (bindingName, 2)] := by
            simpa [SourceFunction.scope?, parametersEq, letsEq, absent] using irScope.symm
          have scopeEq := scope_eq sourceScope
          simp only [parametersEq, letsEq, List.zipIdx_cons, List.zipIdx_nil, List.map_cons,
            List.map_nil, List.cons_append, List.nil_append] at scopeEq
          have reverse : (bindingName == parameter) = false := by
            apply beq_eq_false_iff_ne.mpr
            exact Ne.symm parts.2.1
          have completed := single_let_rows (slot := 2) wf aligned names expressionFacts
            sourceScope irScope letsEq (by rw [scopeEq]; simp [List.lookup, reverse])
            (by rw [parts.2.2]; simp [List.lookup, reverse]) (by
              simp only [run_bind]
              exact ⟨words, _, hlit, literal, _, hpush1, word, _, hword, statement, _, hpush2, tail⟩)
          simpa only [parametersEq, letsEq] using completed
        | cons next rest =>
          simp [Lower.function, SourceFunction.scope?, absent, letsEq, parametersEq,
            run_bind, Lower.fail] at run
    | cons next rest =>
      simp [Lower.function, SourceFunction.scope?, absent, letsEq, run_bind, Lower.fail] at run



theorem atom_has_no_call {scope : Scope} {atom : SourceAtom} {nodes : List NodeFact}
    (facts : AtomHolds scope atom nodes) (index count : Nat) : .call index count ∉ nodes := by
  cases facts <;> simp

theorem atoms_have_no_call {scope : Scope} {atoms : List SourceAtom} {nodes : List NodeFact}
    (facts : AtomsHold scope atoms nodes) (index count : Nat) : .call index count ∉ nodes := by
  induction facts with
  | nil => simp
  | cons first _ ih => simp [atom_has_no_call first index count, ih]

theorem value_has_no_call {scope : Scope} {value : SourceValue} {nodes : List NodeFact}
    (facts : ValueHolds scope value nodes) (index count : Nat) : .call index count ∉ nodes := by
  cases facts with
  | atom first => exact atom_has_no_call first index count
  | arithmetic _ left right =>
    simp [atom_has_no_call left index count, atom_has_no_call right index count]

theorem expression_call_entry {callees : Callees} {scope : Scope}
    {expression : SourceExpression} {nodes : List NodeFact} {index count : Nat}
    (facts : ExpressionHolds callees scope expression nodes) (member : .call index count ∈ nodes) :
    (callees[index]?).map (·.2) = some count := by
  cases facts with
  | value valueFacts => exact False.elim (value_has_no_call valueFacts index count member)
  | call selected arity counted arguments =>
    simp only [List.mem_append, List.mem_singleton] at member
    rcases member with bad | same
    · exact False.elim (atoms_have_no_call arguments index count bad)
    · cases same
      simpa only [counted] using arity
  | conditional _ left right thenFacts elseFacts =>
    simp [atom_has_no_call left index count, atom_has_no_call right index count,
      value_has_no_call thenFacts index count, value_has_no_call elseFacts index count] at member

def CallsResolve (earlier : List FunctionRow) (facts : FunctionFacts) : Prop :=
  ∀ index count, .call index count ∈ facts.nodes →
    ∃ row, earlier[index]? = some row ∧ row.paramsLen.val = count

theorem calls_resolve {callees : Callees} {earlier : List FunctionRow} {function : SourceFunction}
    {facts : FunctionFacts} (metadata : earlier.map (fun row => (row.name, row.paramsLen.val)) = callees)
    (holds : FunctionHolds callees function facts) : CallsResolve earlier facts := by
  obtain ⟨_, scope, nodes, _, expr, rfl⟩ := holds
  intro index count member
  have entry := expression_call_entry expr member
  rw [← metadata, List.getElem?_map] at entry
  cases found : earlier[index]? with
  | none => simp [found] at entry
  | some row =>
    simp only [found, Option.map_some, Option.some.injEq] at entry
    exact ⟨row, rfl, entry⟩

inductive FunctionsRows (F : LoweringState) :
    List FunctionRow → List SourceFunction → List FunctionFacts → List FunctionRow → Prop where
  | nil {earlier : List FunctionRow} : FunctionsRows F earlier [] [] []
  | cons {earlier : List FunctionRow} {function : SourceFunction} {functions : List SourceFunction} {facts : FunctionFacts}
      {others : List FunctionFacts} {row : FunctionRow} {rows : List FunctionRow} :
      CallsResolve earlier facts → FunctionRows F function facts row →
      FunctionsRows F (earlier ++ [row]) functions others rows →
      FunctionsRows F earlier (function :: functions) (facts :: others) (row :: rows)

theorem aligned_after {s s' : LoweringState} {src : List (String × Option Callee)}
    {f : SourceFunction} {program : FunctionProgram} {row : FunctionRow}
    (aligned : Aligned s src) (appended : s'.functions = s.functions ++ [row])
    (flags : s'.callable = s.callable ++ [f.callable])
    (callable : program.callee?.isSome = f.callable) (parameters : program.parameters = f.parameters.length)
    (name : row.name = f.name) (width : row.paramsLen.val = f.parameters.length) :
    Aligned s' (src ++ [(f.name, program.callee?)]) := by
  obtain ⟨names, oldFlags, widths⟩ := aligned
  have sameLength : s.functions.length = src.length := by
    simpa using congrArg List.length names
  refine ⟨?_, ?_, ?_⟩
  · rw [appended]; simp [names, name]
  · rw [flags]; simp [oldFlags, callable]
  · intro index found callee located calleeAt
    rw [appended] at located
    by_cases old : index < s.functions.length
    · rw [List.getElem?_append_left old] at located
      rw [List.getElem?_append_left (by omega)] at calleeAt
      exact widths index found callee located calleeAt
    · have atEnd : index = s.functions.length := by
        have := (List.getElem?_eq_some_iff.mp located).1
        simp at this
        omega
      subst atEnd
      rw [List.getElem?_append_right (by omega)] at located calleeAt
      simp only [Nat.sub_self, List.getElem?_cons_zero, Option.some.injEq] at located
      simp only [sameLength, Nat.sub_self, List.getElem?_cons_zero, Option.bind_some] at calleeAt
      subst located
      rw [width, callee_parameters calleeAt, parameters]

theorem all_rows :
    ∀ {functions : List SourceFunction} {expected : List FunctionFacts} {callees : Callees}
      {src : List (String × Option Callee)} {s s' : LoweringState},
      BlocksIndexed s → Aligned s src → s.functions.map (fun row => (row.name, row.paramsLen.val)) = callees →
      ProgramInOrder callees functions expected →
      (functions.forM Lower.function) s = some ((), s') →
      ∃ rows, s'.functions = s.functions ++ rows ∧ StateLE s s' ∧
        ∀ F, StateLE s' F → FunctionsRows F s.functions functions expected rows := by
  intro functions expected callees src s s' wf aligned names facts
  induction facts generalizing src s with
  | nil =>
    intro run
    unfold List.forM at run
    simp only [run_pure] at run
    obtain ⟨_, rfl⟩ := run
    exact ⟨[], by simp, StateLE.refl _, fun _ _ => .nil⟩
  | @cons callees function rest fact others first remaining ih =>
    intro run
    unfold List.forM at run
    simp only [run_bind] at run
    obtain ⟨_, s1, hf, hr⟩ := run
    obtain ⟨program, row, _, callable, parameters, appended, flags, le1, wf1, name, width, _⟩ :=
      function_lowers wf aligned hf
    have justNames : s.functions.map (·.name) = callees.map (·.1) := by
      simpa only [List.map_map, Function.comp_def] using congrArg (List.map (·.1)) names
    obtain ⟨row', appended', matched⟩ := function_rows wf aligned justNames first hf
    have rowEq : row' = row := by
      have := List.append_cancel_left (appended'.symm.trans appended)
      simpa using this
    subst rowEq
    have aligned1 := aligned_after aligned appended flags callable parameters name width
    have names1 : s1.functions.map (fun row => (row.name, row.paramsLen.val)) =
        calleesAfter callees function := by
      rw [appended]
      simp [calleesAfter, names, name, width]
    obtain ⟨rows, allAppended, le2, restMatched⟩ := ih wf1 aligned1 names1 hr
    refine ⟨row' :: rows, by rw [allAppended, appended]; simp, le1.trans le2, ?_⟩
    intro F leF
    apply FunctionsRows.cons (calls_resolve names first) (matched F (le2.trans leF))
    rw [← appended]
    exact restMatched F leF

def Consistent (artifact : RawArtifact) (functions : List SourceFunction)
    (candidate : List FunctionFacts) : Prop :=
  ∃ state expected, state.toArtifact artifact.sourceId artifact.observations = artifact ∧
    (∀ facts, facts ∈ candidate ↔ facts ∈ expected) ∧ candidate.length = expected.length ∧
    FunctionsRows state [] functions expected artifact.functions

/-- Accepted checker facts determine the actual function, binding, expression, operation and
call rows of every artifact the Lean lowerer reproduces. The source functions and span words
are arbitrary; this theorem makes no claim that Rust lowers other inputs the same way. -/
theorem accepted_facts_match_ir {functions : List SourceFunction} {candidate : List FunctionFacts}
    {artifact : RawArtifact} (checked : validateFacts functions candidate = true)
    (lowered : lowersTo artifact functions = true) :
    Consistent artifact functions candidate ∧ functionPrograms artifact = sourcePrograms functions := by
  obtain ⟨expected, derivation, _, same, length, _⟩ := validate_facts_iff.mp checked
  unfold lowersTo at lowered
  cases ran : lowerFunctions (spanWords artifact) functions with
  | none => simp [ran] at lowered
  | some state =>
    simp only [ran, beq_iff_eq] at lowered
    obtain ⟨rows, appended, _, matched⟩ := all_rows
      (src := []) (s := { spans := spanWords artifact })
      (fun index block located => by simp at located)
      ⟨rfl, rfl, fun index row callee located => by simp at located⟩ rfl derivation
      (lower_functions_run ran)
    have sameFunctions : state.functions = artifact.functions := by
      exact congrArg RawArtifact.functions lowered
    have rowEq : rows = artifact.functions := by simpa using appended.symm.trans sameFunctions
    refine ⟨⟨state, expected, lowered, same, length, ?_⟩, ?_⟩
    · rw [← rowEq]; exact matched state (StateLE.refl _)
    · rw [← lowered]
      exact lowering_means_the_source ran _ _


theorem admitted_facts_match_ir {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    {candidate : List FunctionFacts} {artifact : RawArtifact}
    (admitted : SourceAdmission.check bytes moduleInputs functions scopes = true)
    (checked : validateFacts functions candidate = true)
    (lowered : lowersTo artifact functions = true) :
    moduleInputs = [] ∧ SourceAdmission.AdmittedSource (bytes.map UInt8.toNat) functions ∧
      SourceAdmission.ScopesProjection functions scopes ∧ Consistent artifact functions candidate ∧
      functionPrograms artifact = sourcePrograms functions := by
  obtain ⟨noModules, _, grammar, projection⟩ := SourceAdmission.check_sound admitted
  obtain ⟨rows, meaning⟩ := accepted_facts_match_ir checked lowered
  exact ⟨noModules, grammar, projection, rows, meaning⟩

theorem certified_facts_match_certified_ir {sourceCodes factsCodes irCodes : ExchangeCodes}
    {results : List ObservedResult} (checked : certifiesFacts sourceCodes factsCodes = true)
    (certified : certifyFromSource sourceCodes irCodes = some results) :
    ∃ source facts raw functions,
      decodeSourceDocument sourceCodes = some source ∧
      decodeFactsDocument factsCodes = some facts ∧ decodeRawDocument irCodes = some raw ∧
      facts.sourceId = source.id ∧ raw.sourceId = source.id ∧
      parseSource source.text = some functions ∧ Consistent raw functions facts.functions ∧
      functionPrograms raw = sourcePrograms functions ∧ validateArtifact raw = true ∧
      namedResults raw = some results := by
  obtain ⟨source, facts, functions, sourceAt, factsAt, factsId, parsed, holds⟩ :=
    certifies_facts_sound checked
  obtain ⟨otherSource, raw, otherSourceAt, rawAt, certified⟩ := certify_from_source_sound certified
  have sameSource : otherSource = source := Option.some.inj (otherSourceAt.symm.trans sourceAt)
  subst otherSource
  obtain ⟨otherFunctions, rawId, otherParsed, lowers, valid, named⟩ := certify_decoded_sound certified
  have sameFunctions : otherFunctions = functions := Option.some.inj (otherParsed.symm.trans parsed)
  subst otherFunctions
  obtain ⟨consistent, meaning⟩ := accepted_facts_match_ir (validate_facts_iff.mpr holds) lowers
  exact ⟨source, facts, raw, functions, sourceAt, factsAt, rawAt, factsId, rawId, parsed,
    consistent, meaning, valid, named⟩

theorem checked_facts_exclude_unsupported_propagation {functions : List SourceFunction}
    {candidate : List FunctionFacts} (checked : validateFacts functions candidate = true) :
    ∀ function ∈ functions, function.propagation = none := by
  obtain ⟨expected, ordered, _⟩ := validate_facts_iff.mp checked
  have noPropagation : ∀ {callees : Callees} {functions : List SourceFunction}
      {facts : List FunctionFacts}, ProgramInOrder callees functions facts →
      ∀ function ∈ functions, function.propagation = none := by
    intro callees functions facts ordered
    induction ordered with
    | nil => simp
    | cons first _ ih =>
      intro function member
      rcases List.mem_cons.mp member with rfl | member
      · exact first.1
      · exact ih function member
  exact noPropagation ordered

theorem the_emitted_calls_facts_match_the_actual_ir_rows :
    Consistent callsArtifact callsFunctions callsEmitted ∧
      functionPrograms callsArtifact = sourcePrograms callsFunctions := by
  exact accepted_facts_match_ir
    (validate_facts_iff.mpr the_emitted_facts_are_a_valid_derivation.2)
    the_calls_functions_lower_to_the_calls_artifact

theorem reordered_emitted_facts_match_the_same_ir_rows :
    Consistent callsArtifact callsFunctions callsEmitted.reverse := by
  exact (accepted_facts_match_ir the_emitted_facts_are_accepted_in_any_order
    the_calls_functions_lower_to_the_calls_artifact).1

def identityFunction : SourceFunction :=
  ⟨"identity", ["value"], [], none, .value (.atom (.var "value"))⟩

def identityFacts : FunctionFacts :=
  ⟨"identity", [.int], [], .int, [], [.read (.parameter 0)]⟩

def unsupportedPropagation : SourceFunction :=
  { identityFunction with propagation := some ⟨"bound", "missing", [.var "value"]⟩ }

theorem a_no_propagation_derivation_is_accepted :
    validateFacts [identityFunction] [identityFacts] = true := by
  decide +kernel

theorem an_unsupported_propagation_cannot_reuse_the_same_facts :
    validateFacts [unsupportedPropagation] [identityFacts] = false ∧
      ¬ ProgramHolds [unsupportedPropagation] [identityFacts] := by
  constructor
  · decide +kernel
  · intro accepted
    have absent := checked_facts_exclude_unsupported_propagation
      (validate_facts_iff.mpr accepted) unsupportedPropagation (by simp)
    cases absent

theorem forged_facts_cannot_be_paired_with_the_unchanged_ir :
    (validateFacts callsFunctions
      (withFact callsEmitted 1 fun facts =>
        { facts with nodes := [.read (.parameter 0), .literal 9, .call 0 2] }) &&
      lowersTo callsArtifact callsFunctions) = false ∧
    (validateFacts callsFunctions
      (withFact callsEmitted 0 fun facts =>
        { facts with nodes := facts.nodes.set 5 (.operate .add .int .int) }) &&
      lowersTo callsArtifact callsFunctions) = false ∧
    (validateFacts callsFunctions
      (withFact callsEmitted 1 fun facts =>
        { facts with nodes := [.read (.letBinding 0), .literal 9, .call 1 2] }) &&
      lowersTo callsArtifact callsFunctions) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

theorem unchanged_facts_cannot_be_paired_with_mismatched_ir :
    (validateFacts callsFunctions callsEmitted &&
      lowersTo (callsArtifact.swapExtraWords 42 44) callsFunctions) = false ∧
    (validateFacts callsFunctions callsEmitted &&
      lowersTo (callsArtifact.setOp 1 .add) callsFunctions) = false ∧
    (validateFacts callsFunctions callsEmitted &&
      lowersTo (callsArtifact.setPayloadWord 17 0 ⟨2, by decide⟩) callsFunctions) = false ∧
    (validateFacts callsFunctions callsEmitted &&
      lowersTo (callsArtifact.setPayloadWord 15 0 ⟨0, by decide⟩) callsFunctions) = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end FactsIR

end Xsh.Conformance.Vertical
