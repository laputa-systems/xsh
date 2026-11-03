import Xsh.Conformance.Vertical.Lowering

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport
open Xsh.IR.Raw
open Xsh.Semantics.BaseMachine

/-- A lowering state extends another when every table only grew by appending. -/
structure StateLE (s F : LoweringState) : Prop where
  rows : s.rows <+: F.rows
  extra : s.extra <+: F.extra
  ops : s.ops <+: F.ops
  blocks : s.blocks <+: F.blocks

theorem StateLE.refl (s : LoweringState) : StateLE s s :=
  ⟨List.prefix_refl _, List.prefix_refl _, List.prefix_refl _, List.prefix_refl _⟩

theorem StateLE.trans {a b c : LoweringState} (ab : StateLE a b) (bc : StateLE b c) :
    StateLE a c :=
  ⟨ab.rows.trans bc.rows, ab.extra.trans bc.extra, ab.ops.trans bc.ops,
    ab.blocks.trans bc.blocks⟩

def LoweringState.tables (s : LoweringState) : Tables :=
  ⟨(s.rows.map fun row => row.tag.word).toArray,
    (s.rows.map fun row => (⟨row.lhs, row.rhs⟩ : RawData)).toArray, s.extra.toArray⟩

def LoweringState.context (s : LoweringState) (callees : List (Option Callee)) :
    MeaningContext :=
  ⟨s.tables, s.ops, s.blocks, callees⟩

@[simp] theorem context_tables (s : LoweringState) (callees : List (Option Callee)) :
    (s.context callees).tables = s.tables := rfl

@[simp] theorem context_ops (s : LoweringState) (callees : List (Option Callee)) :
    (s.context callees).ops = s.ops := rfl

@[simp] theorem context_blocks (s : LoweringState) (callees : List (Option Callee)) :
    (s.context callees).blocks = s.blocks := rfl

@[simp] theorem context_callees (s : LoweringState) (callees : List (Option Callee)) :
    (s.context callees).callees = callees := rfl

theorem toArtifact_tables (s : LoweringState) (sourceId : BridgeId)
    (observations : List Observation) : (s.toArtifact sourceId observations).tables = s.tables :=
  rfl

theorem toArtifact_context (s : LoweringState) (sourceId : BridgeId)
    (observations : List Observation) (callees : List (Option Callee)) :
    (s.toArtifact sourceId observations).context callees = s.context callees :=
  rfl

def slice (words : List Word32) (start length : Nat) : List Word32 :=
  (words.drop start).take length

theorem slice_append_left (words more : List Word32) {start length : Nat}
    (within : start + length ≤ words.length) :
    slice (words ++ more) start length = slice words start length := by
  unfold slice
  rw [List.drop_append_of_le_length (by omega)]
  rw [List.take_append_of_le_length (by simp; omega)]

theorem readWords_eq (words : Array Word32) :
    ∀ (count start : Nat), start + count ≤ words.size →
      Decode.readWords? words start count = some (slice words.toList start count) := by
  intro count
  induction count with
  | zero => intro start _; simp [Decode.readWords?, slice]
  | succ count ih =>
    intro start within
    have inBounds : start < words.size := by omega
    have restWithin : start + 1 + count ≤ words.size := by omega
    have startBound : start < words.toList.length := by simpa using inBounds
    have head : words[start]? = some (words.toList[start]) := by
      rw [Array.getElem?_eq_getElem inBounds]; simp
    simp only [Decode.readWords?, head, ih (start + 1) restWithin, Option.bind_some, pure]
    unfold slice
    rw [List.drop_eq_getElem_cons startBound]
    simp [List.drop_drop, Nat.add_comm]

theorem readPayload_eq (tables : Tables) (start : Word32) (length : Word32)
    (within : start.val + length.val ≤ tables.extra.size) :
    Decode.readPayload? tables ⟨DirectIndex.decode start, length⟩ =
      some (slice tables.extra.toList start.val length.val) := by
  unfold Decode.readPayload?
  have resolved : (RawRange.mk (DirectIndex.decode start) length).resolve? tables.extra.size =
      some (start.val, start.val + length.val) :=
    RawRange.resolve_complete _ _ within
  rw [resolved]
  simp only [Option.bind_some]
  have := readWords_eq tables.extra length.val start.val within
  simpa using this

def LoweringState.RowAt (s : LoweringState) (index : Nat) (tag : FragmentTag)
    (words : List Word32) : Prop :=
  ∃ row, s.rows[index]? = some row ∧ row.tag = tag ∧
    row.lhs.val + row.rhs.val ≤ s.extra.length ∧
    slice s.extra row.lhs.val row.rhs.val = words ∧ words.length = tag.payloadWords

theorem readInstruction_of_rowAt {s : LoweringState} {index : Word32} {tag : FragmentTag}
    {words : List Word32} (at_ : s.RowAt index.val tag words) :
    readInstruction? s.tables index = some (tag, words) := by
  obtain ⟨row, found, rfl, within, same, width⟩ := at_
  unfold readInstruction?
  have readRow : Decode.readRow? s.tables (DirectIndex.decode index) =
      some ⟨row.tag.word, ⟨row.lhs, row.rhs⟩⟩ := by
    simp [Decode.readRow?, LoweringState.tables, DirectIndex.decode, DirectIndex.value, found]
  rw [readRow]
  simp only [Option.bind_eq_bind, Option.bind, fragment_tag_words_decode_to_their_tags]
  have payload := readPayload_eq s.tables row.lhs row.rhs (by simpa [LoweringState.tables] using within)
  have payloadRange : (RawData.mk row.lhs row.rhs).range = ⟨DirectIndex.decode row.lhs, row.rhs⟩ := rfl
  simp only [payloadRange, payload]
  simp [LoweringState.tables, same, width]

theorem LoweringState.RowAt.mono {s F : LoweringState} (le : StateLE s F) {index : Nat}
    {tag : FragmentTag} {words : List Word32} (at_ : s.RowAt index tag words) :
    F.RowAt index tag words := by
  obtain ⟨row, found, tagEq, within, same, width⟩ := at_
  obtain ⟨rowsMore, rowsEq⟩ := le.rows
  obtain ⟨extraMore, extraEq⟩ := le.extra
  have inBounds : index < s.rows.length := (List.getElem?_eq_some_iff.mp found).1
  refine ⟨row, ?_, tagEq, ?_, ?_, width⟩
  · rw [← rowsEq, List.getElem?_append_left inBounds]; exact found
  · rw [← extraEq]; simp; omega
  · rw [← extraEq, slice_append_left _ _ within]; exact same

def LoweringState.BlockAt (s : LoweringState) (id : Word32) (words : List Word32) : Prop :=
  ∃ block, s.blocks.find? (fun block => block.id.raw == id) = some block ∧
    block.start.val + block.len.val ≤ s.extra.length ∧
    slice s.extra block.start.val block.len.val = words

theorem blockWords_of_blockAt {s : LoweringState} {id : Word32} {words : List Word32}
    (callees : List (Option Callee)) (at_ : s.BlockAt id words) :
    blockWords? (s.context callees) id = some words := by
  obtain ⟨block, found, within, same⟩ := at_
  unfold blockWords?
  have := readPayload_eq s.tables block.start block.len (by simpa [LoweringState.tables] using within)
  simp only [context_blocks, context_tables, found, Option.bind_some]
  simp only [LoweringState.tables] at this ⊢
  simp [this, same]

theorem LoweringState.BlockAt.mono {s F : LoweringState} (le : StateLE s F) {id : Word32}
    {words : List Word32} (at_ : s.BlockAt id words) : F.BlockAt id words := by
  obtain ⟨block, found, within, same⟩ := at_
  obtain ⟨blocksMore, blocksEq⟩ := le.blocks
  obtain ⟨extraMore, extraEq⟩ := le.extra
  refine ⟨block, ?_, ?_, ?_⟩
  · rw [← blocksEq, List.find?_append, found]; rfl
  · rw [← extraEq]; simp; omega
  · rw [← extraEq, slice_append_left _ _ within]; exact same

theorem op_mono {s F : LoweringState} (le : StateLE s F) {index : Nat} {op : FragmentOp}
    (found : s.ops[index]? = some op) : F.ops[index]? = some op := by
  obtain ⟨more, eq⟩ := le.ops
  have inBounds : index < s.ops.length := (List.getElem?_eq_some_iff.mp found).1
  rw [← eq, List.getElem?_append_left inBounds]; exact found

/-- A row denotes an expression in a context: at every fuel past its index, reading it gives
that expression. -/
def Reads (F : LoweringState) (callees : List (Option Callee)) (row : Word32) (e : BaseExpr) :
    Prop :=
  ∀ fuel, row.val < fuel → expression? (F.context callees) fuel row = some e

inductive ReadsAll (F : LoweringState) (callees : List (Option Callee)) :
    List Word32 → List BaseExpr → Prop where
  | nil : ReadsAll F callees [] []
  | cons {row : Word32} {e : BaseExpr} {rows : List Word32} {exprs : List BaseExpr} :
      Reads F callees row e → ReadsAll F callees rows exprs →
        ReadsAll F callees (row :: rows) (e :: exprs)

theorem reads_int {F : LoweringState} {callees : List (Option Callee)} {row low high : Word32}
    (at_ : F.RowAt row.val .exprInt [low, high]) :
    Reads F callees row (.integerLiteral (signedOfWords low high)) := by
  intro fuel above
  cases fuel with
  | zero => omega
  | succ fuel =>
    simp [expression?, readInstruction_of_rowAt at_]

theorem reads_param {F : LoweringState} {callees : List (Option Callee)} {row slot : Word32}
    (at_ : F.RowAt row.val .exprParam [slot]) :
    Reads F callees row (.identifier (NameId.mk slot)) := by
  intro fuel above
  cases fuel with
  | zero => omega
  | succ fuel =>
    simp [expression?, readInstruction_of_rowAt at_]

theorem reads_binary {F : LoweringState} {callees : List (Option Callee)}
    {row op left right span : Word32} {operation : FragmentOp} {first second : BaseExpr}
    (at_ : F.RowAt row.val .exprBinary [op, left, right, span])
    (leftBefore : left.val < row.val) (rightBefore : right.val < row.val)
    (pooled : F.ops[op.val]? = some operation)
    (readsLeft : Reads F callees left first) (readsRight : Reads F callees right second) :
    Reads F callees row (operation.expression first second) := by
  intro fuel above
  cases fuel with
  | zero => omega
  | succ fuel =>
    have leftReads := readsLeft fuel (by omega)
    have rightReads := readsRight fuel (by omega)
    simp [expression?, readInstruction_of_rowAt at_, leftBefore,
      rightBefore, pooled, leftReads, rightReads]

theorem reads_value_block {F : LoweringState} {callees : List (Option Callee)}
    {row block span count statement value : Word32} {e : BaseExpr}
    (at_ : F.RowAt row.val .exprValueBlock [block, span])
    (blockAt : F.BlockAt block [count, statement]) (single : count.val = 1)
    (statementBefore : statement.val < row.val)
    (statementAt : F.RowAt statement.val .stmtValue [value])
    (valueBefore : value.val < statement.val) (readsValue : Reads F callees value e) :
    Reads F callees row e := by
  intro fuel above
  cases fuel with
  | zero => omega
  | succ fuel =>
    have valueReads := readsValue fuel (by omega)
    simp [expression?, readInstruction_of_rowAt at_,
      blockWords_of_blockAt callees blockAt, single, statementBefore,
      readInstruction_of_rowAt statementAt, valueBefore, valueReads]

theorem reads_if {F : LoweringState} {callees : List (Option Callee)}
    {row table elseRow span count condition thenRow : Word32} {c t e : BaseExpr}
    (at_ : F.RowAt row.val .exprIf [table, elseRow, span])
    (tableAt : F.BlockAt table [count, condition, thenRow]) (single : count.val = 1)
    (conditionBefore : condition.val < row.val) (thenBefore : thenRow.val < row.val)
    (elseBefore : elseRow.val < row.val) (readsCondition : Reads F callees condition c)
    (readsThen : Reads F callees thenRow t) (readsElse : Reads F callees elseRow e) :
    Reads F callees row (.conditional c t e) := by
  intro fuel above
  cases fuel with
  | zero => omega
  | succ fuel =>
    have conditionReads := readsCondition fuel (by omega)
    have thenReads := readsThen fuel (by omega)
    have elseReads := readsElse fuel (by omega)
    simp [expression?, readInstruction_of_rowAt at_,
      blockWords_of_blockAt callees tableAt, single, conditionBefore, thenBefore, elseBefore,
      conditionReads, thenReads, elseReads]

theorem argument_rows_of_flat (user : Word32) :
    ∀ rows : List Word32, (∀ row ∈ rows, row.val < user.val) →
      argumentRows? user (rows.flatMap fun row => [0, row]) = some rows := by
  intro rows
  induction rows with
  | nil => intro _; rfl
  | cons row rest ih =>
    intro bound
    have rowBound := bound row (by simp)
    have restBound : ∀ x ∈ rest, x.val < user.val := fun x member => bound x (by simp [member])
    simp [argumentRows?, rowBound, ih restBound]

theorem map_m_reads {F : LoweringState} {callees : List (Option Callee)} (fuel : Nat)
    (rows : List Word32) (exprs : List BaseExpr)
    (all : ReadsAll F callees rows exprs)
    (fuelBound : ∀ row ∈ rows, row.val < fuel) :
    rows.mapM (expression? (F.context callees) fuel) = some exprs := by
  induction all with
  | nil => rfl
  | cons head rest ih =>
    rename_i row e rows' exprs'
    have rowBound := fuelBound row (by simp)
    have restBound : ∀ x ∈ rows', x.val < fuel := fun x member => fuelBound x (by simp [member])
    simp [List.mapM_cons, head _ rowBound, ih restBound]

theorem reads_call {F : LoweringState} {callees : List (Option Callee)}
    {row function arguments span count : Word32} {callee : Callee} {rows : List Word32}
    {exprs : List BaseExpr}
    (at_ : F.RowAt row.val .exprDirectPureCall [function, arguments, span])
    (named : function.val ≠ 0)
    (found : callees[function.val - 1]? = some (some callee))
    (tableAt : F.BlockAt arguments (count :: rows.flatMap fun argument => [0, argument]))
    (countMatches : count.val = callee.parameters) (fits : rows.length = callee.parameters)
    (before : ∀ argument ∈ rows, argument.val < row.val)
    (readsArguments : ReadsAll F callees rows exprs) :
    Reads F callees row (.call (parameterNames callee.parameters) callee.body exprs) := by
  intro fuel above
  cases fuel with
  | zero => omega
  | succ fuel =>
    have mapped := map_m_reads fuel rows exprs readsArguments
      (fun argument member => by have := before argument member; omega)
    have named' : function ≠ 0 := fun h => named (by simp [h])
    simp [expression?, readInstruction_of_rowAt at_, named', found,
      blockWords_of_blockAt callees tableAt, countMatches, argument_rows_of_flat row rows before,
      fits, mapped]

end Xsh.Conformance.Vertical
