import Xsh.Conformance.Vertical.Reading

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport
open Xsh.IR.Raw
open Xsh.Semantics.BaseMachine
open Xsh.Values

theorem run_bind {α β : Type} {m : Lower α} {f : α → Lower β} {s : LoweringState} {b : β}
    {s'' : LoweringState} :
    (m >>= f) s = some (b, s'') ↔ ∃ a s', m s = some (a, s') ∧ f a s' = some (b, s'') := by
  show (StateT.bind m f s) = _ ↔ _
  simp only [StateT.bind]
  cases h : m s with
  | none => simp [Option.bind]
  | some p =>
    obtain ⟨a, s'⟩ := p
    constructor
    · intro hf; exact ⟨a, s', rfl, hf⟩
    · rintro ⟨a1, s1, hm, hf⟩
      simp only [Option.some.injEq, Prod.mk.injEq] at hm
      obtain ⟨rfl, rfl⟩ := hm
      exact hf

theorem run_pure {α : Type} {a : α} {s : LoweringState} {b : α} {s' : LoweringState} :
    (pure a : Lower α) s = some (b, s') ↔ b = a ∧ s' = s := by
  show (some (a, s) = some (b, s')) ↔ _
  simp [eq_comm]

theorem run_get {s : LoweringState} {b : LoweringState} {s' : LoweringState} :
    (get : Lower LoweringState) s = some (b, s') ↔ b = s ∧ s' = s := by
  show (some (s, s) = some (b, s')) ↔ _
  simp [eq_comm]

theorem run_modify {f : LoweringState → LoweringState} {s : LoweringState} {b : PUnit} {s' : LoweringState} :
    (modify f : Lower PUnit) s = some (b, s') ↔ s' = f s := by
  show (some (PUnit.unit, f s) = some (b, s')) ↔ _
  simp [eq_comm]

theorem run_lift {α : Type} {v : Option α} {s : LoweringState} {b : α} {s' : LoweringState} :
    (Lower.lift v) s = some (b, s') ↔ v = some b ∧ s' = s := by
  cases v <;> simp [Lower.lift, Lower.fail, pure, StateT.pure, eq_comm]

theorem run_require {holds : Bool} {s s' : LoweringState} {b : Unit} :
    (Lower.require holds) s = some (b, s') ↔ holds = true ∧ s' = s := by
  cases holds <;> simp [Lower.require, Lower.fail, pure, StateT.pure, eq_comm]

theorem decode_val {n : Nat} {w : Word32} (h : Word32.decode? n = some w) : w.val = n := by
  unfold Word32.decode? at h
  split at h
  · simp only [Option.some.injEq] at h; subst h; rfl
  · simp at h

theorem from_index_raw {n : Nat} {id : BridgeId} (h : BridgeId.fromIndex? n = some id) :
    id.raw.val = n + 1 := by
  unfold BridgeId.fromIndex? at h
  split at h <;> simp_all
  subst h; rfl

theorem push_row_run {tag : FragmentTag} {payload : List Word32} {s s' : LoweringState}
    {r : Word32} (h : Lower.pushRow tag payload s = some (r, s')) :
    ∃ row : Row, s' = { s with rows := s.rows ++ [row], extra := s.extra ++ payload } ∧
      r.val = s.rows.length ∧ row.tag = tag ∧ row.lhs.val = s.extra.length ∧
      row.rhs.val = payload.length := by
  unfold Lower.pushRow at h
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.some.injEq, Prod.mk.injEq] at h
  obtain ⟨index, hindex, id, hid, start, hstart, length, hlength, rfl, rfl⟩ := h
  exact ⟨⟨id, tag, start, length⟩, rfl, decode_val hindex, rfl, decode_val hstart,
    decode_val hlength⟩

theorem push_block_run {owner : BridgeId} {flags : WireByte} {words : List Word32}
    {s s' : LoweringState} {id : BridgeId} (h : Lower.pushBlock owner flags words s = some (id, s')) :
    ∃ block : Block, s' = { s with blocks := s.blocks ++ [block], extra := s.extra ++ words } ∧
      block.owner = ⟨some owner⟩ ∧ block.flags = flags ∧
      block.id = id ∧ id.raw.val = s.blocks.length + 1 ∧ block.start.val = s.extra.length ∧
      block.len.val = words.length := by
  unfold Lower.pushBlock at h
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.some.injEq, Prod.mk.injEq] at h
  obtain ⟨newId, hid, start, hstart, length, hlength, rfl, rfl⟩ := h
  exact ⟨⟨newId, start, length, ⟨none⟩, ⟨some owner⟩, flags⟩, rfl, rfl, rfl, rfl,
    from_index_raw hid, decode_val hstart, decode_val hlength⟩

theorem take_span_run {s s' : LoweringState} {span : Word32}
    (h : Lower.takeSpan s = some (span, s')) :
    ∃ rest, s.spans = span :: rest ∧ s' = { s with spans := rest } := by
  unfold Lower.takeSpan at h
  cases hs : s.spans with
  | nil => simp [hs] at h
  | cons first rest =>
    simp only [hs, Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl⟩ := h
    exact ⟨rest, rfl, rfl⟩

theorem operation_index_run {op : FragmentOp} {s s' : LoweringState} {w : Word32}
    (h : Lower.operationIndex op s = some (w, s')) :
    StateLE s s' ∧ s'.ops[w.val]? = some op ∧ s'.rows = s.rows ∧ s'.extra = s.extra ∧
      s'.blocks = s.blocks ∧ s'.functions = s.functions ∧ s'.callable = s.callable := by
  unfold Lower.operationIndex at h
  cases hf : s.ops.findIdx? (· == op) with
  | some index =>
    simp only [hf] at h
    cases hw : Word32.decode? index with
    | none => simp [hw] at h
    | some word =>
      simp only [hw, Option.map_some, Option.some.injEq, Prod.mk.injEq] at h
      obtain ⟨rfl, rfl⟩ := h
      have value := decode_val hw
      refine ⟨StateLE.refl _, ?_, rfl, rfl, rfl, rfl, rfl⟩
      obtain ⟨inBounds, matches_, _⟩ := List.findIdx?_eq_some_iff_getElem.mp hf
      rw [value, List.getElem?_eq_getElem inBounds]
      simpa using matches_
  | none =>
    simp only [hf] at h
    cases hw : Word32.decode? s.ops.length with
    | none => simp [hw] at h
    | some word =>
      simp only [hw, Option.map_some, Option.some.injEq, Prod.mk.injEq] at h
      obtain ⟨rfl, rfl⟩ := h
      have value := decode_val hw
      refine ⟨⟨List.prefix_refl _, List.prefix_refl _, List.prefix_append _ _,
        List.prefix_refl _⟩, ?_, rfl, rfl, rfl, rfl, rfl⟩
      rw [value]
      simp

def BlocksIndexed (s : LoweringState) : Prop :=
  ∀ i block, s.blocks[i]? = some block → block.id.raw.val = i + 1

theorem state_le_rows_length {s F : LoweringState} (le : StateLE s F) :
    s.rows.length ≤ F.rows.length := le.rows.length_le


theorem push_row_at {tag : FragmentTag} {payload : List Word32} {s s' : LoweringState}
    {r : Word32} (h : Lower.pushRow tag payload s = some (r, s'))
    (width : payload.length = tag.payloadWords) : s'.RowAt r.val tag payload := by
  obtain ⟨row, rfl, index, tagEq, start, length⟩ := push_row_run h
  refine ⟨row, ?_, tagEq, ?_, ?_, width⟩
  · simp [index]
  · simp [start, length]
  · simp only [slice, start, length]
    simp

theorem push_row_le {tag : FragmentTag} {payload : List Word32} {s s' : LoweringState}
    {r : Word32} (h : Lower.pushRow tag payload s = some (r, s')) : StateLE s s' := by
  obtain ⟨row, rfl, _⟩ := push_row_run h
  exact ⟨List.prefix_append _ _, List.prefix_append _ _, List.prefix_refl _, List.prefix_refl _⟩

theorem push_row_blocks {tag : FragmentTag} {payload : List Word32} {s s' : LoweringState}
    {r : Word32} (h : Lower.pushRow tag payload s = some (r, s')) :
    s'.blocks = s.blocks ∧ s'.ops = s.ops ∧ s'.rows.length = s.rows.length + 1 := by
  obtain ⟨row, rfl, _⟩ := push_row_run h
  simp

theorem push_block_le {owner : BridgeId} {flags : WireByte} {words : List Word32}
    {s s' : LoweringState} {id : BridgeId} (h : Lower.pushBlock owner flags words s = some (id, s')) :
    StateLE s s' ∧ s'.rows = s.rows := by
  obtain ⟨block, rfl, _⟩ := push_block_run h
  exact ⟨⟨List.prefix_refl _, List.prefix_append _ _, List.prefix_refl _,
    List.prefix_append _ _⟩, rfl⟩

theorem push_block_indexed {owner : BridgeId} {flags : WireByte} {words : List Word32}
    {s s' : LoweringState} {id : BridgeId} (wf : BlocksIndexed s)
    (h : Lower.pushBlock owner flags words s = some (id, s')) : BlocksIndexed s' := by
  obtain ⟨block, rfl, _, _, hid, hraw, _⟩ := push_block_run h
  intro i found located
  simp only [List.getElem?_append] at located
  split at located
  · exact wf i found located
  · rename_i beyond
    simp only [Nat.not_lt] at beyond
    cases gap : i - s.blocks.length with
    | zero =>
      rw [gap] at located
      simp only [List.getElem?_cons_zero, Option.some.injEq] at located
      subst located
      rw [hid, hraw]
      omega
    | succ k =>
      rw [gap] at located
      simp at located


theorem push_block_at {owner : BridgeId} {flags : WireByte} {words : List Word32}
    {s s' : LoweringState} {id : BridgeId} (wf : BlocksIndexed s)
    (h : Lower.pushBlock owner flags words s = some (id, s')) : s'.BlockAt id.raw words := by
  obtain ⟨block, rfl, _, _, hid, hraw, start, length⟩ := push_block_run h
  refine ⟨block, ?_, ?_, ?_⟩
  · have earlier : s.blocks.find? (fun b => b.id.raw == id.raw) = none := by
      rw [List.find?_eq_none]
      intro candidate member
      obtain ⟨index, inBounds, rfl⟩ := List.getElem_of_mem member
      have indexed := wf index _ (List.getElem?_eq_getElem inBounds)
      have bound : index < s.blocks.length := inBounds
      simp only [beq_iff_eq, Bool.not_eq_true]
      intro same
      have := congrArg Fin.val same
      omega
    simp [List.find?_append, earlier, hid]
  · simp [start, length]
  · simp only [slice, start, length]
    simp

theorem push_block_blocks_ops {owner : BridgeId} {flags : WireByte} {words : List Word32}
    {s s' : LoweringState} {id : BridgeId} (h : Lower.pushBlock owner flags words s = some (id, s')) :
    s'.ops = s.ops ∧ s'.rows = s.rows := by
  obtain ⟨block, rfl, _⟩ := push_block_run h
  simp


theorem literal_words_run {value : Nat} {s s' : LoweringState} {words : List Word32}
    (h : Lower.literalWords value s = some (words, s')) :
    value ≤ maxIntLiteral ∧ s' = s ∧ ∃ low high : Word32, words = [low, high] ∧
      low.val = value % 4294967296 ∧ high.val = value / 4294967296 := by
  unfold Lower.literalWords at h
  simp only [run_bind, run_pure, run_require, run_lift, Lower.word] at h
  obtain ⟨_, _, ⟨ok, rfl⟩, low, _, ⟨lowDecoded, rfl⟩, high, _, ⟨highDecoded, rfl⟩, rfl, rfl⟩ := h
  simp only [decide_eq_true_eq] at ok
  exact ⟨ok, rfl, low, high, rfl, decode_val lowDecoded, decode_val highDecoded⟩

theorem signed_of_bits_small {n : Nat} (small : n ≤ maxIntLiteral) : signedOfBits n = (n : Int) := by
  unfold signedOfBits
  unfold maxIntLiteral at small
  rw [if_pos (by omega)]
  rfl

theorem signed_of_words_literal {value : Nat} {low high : Word32} (small : value ≤ maxIntLiteral)
    (lowEq : low.val = value % 4294967296) (highEq : high.val = value / 4294967296) :
    signedOfWords low high = (value : Int) := by
  unfold signedOfWords
  rw [lowEq, highEq]
  have : value % 4294967296 + value / 4294967296 * 4294967296 = value := by omega
  rw [this]
  exact signed_of_bits_small small

/-- A lowering step that returns an expression row: the state only grew, blocks stay
numbered, the row precedes the next row, and the row reads as the expression in every
extension. -/
def Lowers (cs : List (Option Callee)) (s s' : LoweringState) (row : Word32) (e : BaseExpr) :
    Prop :=
  StateLE s s' ∧ BlocksIndexed s' ∧ row.val < s'.rows.length ∧
    s'.functions = s.functions ∧ s'.callable = s.callable ∧
    ∀ F : LoweringState, StateLE s' F → Reads F cs row e

theorem lowers_row {cs : List (Option Callee)} {s0 s1 s' : LoweringState} {tag : FragmentTag}
    {payload : List Word32} {row : Word32} {e : BaseExpr} (le : StateLE s0 s1)
    (wf : BlocksIndexed s1) (same : s1.functions = s0.functions ∧ s1.callable = s0.callable)
    (push : Lower.pushRow tag payload s1 = some (row, s'))
    (reads : ∀ F : LoweringState, StateLE s' F → Reads F cs row e) : Lowers cs s0 s' row e := by
  obtain ⟨blocks, _, length⟩ := push_row_blocks push
  obtain ⟨pushed, hs', index, _⟩ := push_row_run push
  refine ⟨le.trans (push_row_le push), ?_, by omega, ?_, ?_, reads⟩
  · intro i block located
    rw [blocks] at located
    exact wf i block located
  · rw [hs']; exact same.1
  · rw [hs']; exact same.2

theorem StateLE.of_eq {s s' : LoweringState} (rows : s'.rows = s.rows) (extra : s'.extra = s.extra)
    (ops : s'.ops = s.ops) (blocks : s'.blocks = s.blocks) : StateLE s s' := by
  exact ⟨by rw [rows]; exact List.prefix_refl _, by rw [extra]; exact List.prefix_refl _,
    by rw [ops]; exact List.prefix_refl _, by rw [blocks]; exact List.prefix_refl _⟩

theorem StateLE.of_update {s F : LoweringState} {fs : List FunctionRow} {cs : List Bool}
    (le : StateLE { s with functions := fs, callable := cs } F) : StateLE s F :=
  ⟨le.rows, le.extra, le.ops, le.blocks⟩

theorem BlocksIndexed.congr {s s' : LoweringState} (blocks : s'.blocks = s.blocks)
    (wf : BlocksIndexed s) : BlocksIndexed s' := by
  intro i block located
  rw [blocks] at located
  exact wf i block located

theorem Lowers.le {cs : List (Option Callee)} {s s' : LoweringState} {row : Word32} {e : BaseExpr}
    (l : Lowers cs s s' row e) : StateLE s s' := l.1

theorem Lowers.wf {cs : List (Option Callee)} {s s' : LoweringState} {row : Word32} {e : BaseExpr}
    (l : Lowers cs s s' row e) : BlocksIndexed s' := l.2.1

theorem Lowers.before {cs : List (Option Callee)} {s s' : LoweringState} {row : Word32}
    {e : BaseExpr} (l : Lowers cs s s' row e) : row.val < s'.rows.length := l.2.2.1

theorem Lowers.functions {cs : List (Option Callee)} {s s' : LoweringState} {row : Word32}
    {e : BaseExpr} (l : Lowers cs s s' row e) :
    s'.functions = s.functions ∧ s'.callable = s.callable := ⟨l.2.2.2.1, l.2.2.2.2.1⟩

theorem Lowers.reads {cs : List (Option Callee)} {s s' F : LoweringState} {row : Word32}
    {e : BaseExpr} (l : Lowers cs s s' row e) (le : StateLE s' F) : Reads F cs row e :=
  l.2.2.2.2.2 F le

theorem atom_lowers {cs : List (Option Callee)} {scope : List (String × Nat)} {atom : SourceAtom}
    {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s)
    (h : Lower.atom scope atom s = some (row, s')) :
    ∃ e, atom.expression? scope = some e ∧ Lowers cs s s' row e := by
  cases atom with
  | int value =>
    simp only [Lower.atom, run_bind, run_pure, run_require, run_lift, Lower.word] at h
    obtain ⟨words, s1, literal, push⟩ := h
    obtain ⟨ok, rfl, low, high, rfl, lowEq, highEq⟩ := literal_words_run literal
    refine ⟨.integerLiteral (value : Int), by simp [SourceAtom.expression?, ok], ?_⟩
    refine lowers_row (StateLE.refl _) wf ⟨rfl, rfl⟩ push ?_
    intro F le
    have at_ := (push_row_at push rfl).mono le
    have := reads_int (callees := cs) at_
    rwa [signed_of_words_literal ok lowEq highEq] at this
  | var name =>
    simp only [Lower.atom, run_bind, run_pure, run_require, run_lift, Lower.word] at h
    obtain ⟨slot, s1, ⟨found, rfl⟩, w, s2, ⟨decoded, rfl⟩, push⟩ := h
    refine ⟨.identifier (NameId.mk w), by simp [SourceAtom.expression?, found, decoded], ?_⟩
    refine lowers_row (StateLE.refl _) wf ⟨rfl, rfl⟩ push ?_
    intro F le
    exact reads_param (callees := cs) ((push_row_at push rfl).mono le)

theorem comparison_lowers {cs : List (Option Callee)} {scope : List (String × Nat)}
    {op : FragmentOp} {left right : SourceAtom} {s s' : LoweringState} {row : Word32}
    (wf : BlocksIndexed s) (h : Lower.comparison scope op left right s = some (row, s')) :
    ∃ first second, left.expression? scope = some first ∧ right.expression? scope = some second ∧
      Lowers cs s s' row (op.expression first second) := by
  simp only [Lower.comparison, run_bind] at h
  obtain ⟨leftRow, s1, hl, rightRow, s2, hr, operation, s3, hop, span, s4, hspan, push⟩ := h
  obtain ⟨first, hfirst, ll⟩ := atom_lowers (cs := cs) wf hl
  obtain ⟨second, hsecond, rl⟩ := atom_lowers (cs := cs) ll.wf hr
  obtain ⟨opLe, pooled, opRows, opExtra, opBlocks, opFunctions, opCallable⟩ :=
    operation_index_run hop
  obtain ⟨rest, spans, rfl⟩ := take_span_run hspan
  have spanLe : StateLE s3 { s3 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
  have le4 : StateLE s2 { s3 with spans := rest } := opLe.trans spanLe
  have wf4 : BlocksIndexed { s3 with spans := rest } :=
    BlocksIndexed.congr (s := s2) (by simpa using opBlocks) rl.wf
  have same : ({ s3 with spans := rest } : LoweringState).functions = s.functions ∧
      ({ s3 with spans := rest } : LoweringState).callable = s.callable := by
    obtain ⟨a, b⟩ := ll.functions
    obtain ⟨c, d⟩ := rl.functions
    exact ⟨by simp [opFunctions, c, a], by simp [opCallable, d, b]⟩
  refine ⟨first, second, hfirst, hsecond, ?_⟩
  refine lowers_row (ll.le.trans (rl.le.trans le4)) wf4 same push ?_
  intro F le
  have at_ := (push_row_at push rfl).mono le
  obtain ⟨pushed, hs', index, _⟩ := push_row_run push
  have leftBefore : leftRow.val < row.val := by
    have a := ll.before
    have b := state_le_rows_length (rl.le.trans le4)
    simp only [index]; simp at b ⊢; omega
  have rightBefore : rightRow.val < row.val := by
    have a := rl.before
    have b := state_le_rows_length le4
    simp only [index]; simp at b ⊢; omega
  have pushLe := push_row_le push
  have pooledF : F.ops[operation.val]? = some op :=
    op_mono (spanLe.trans (pushLe.trans le)) pooled
  exact reads_binary at_ leftBefore rightBefore pooledF
    (ll.reads ((rl.le.trans le4).trans (pushLe.trans le)))
    (rl.reads (le4.trans (pushLe.trans le)))

theorem value_lowers {cs : List (Option Callee)} {scope : List (String × Nat)}
    {value : SourceValue} {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s)
    (h : Lower.value scope value s = some (row, s')) :
    ∃ e, value.expression? scope = some e ∧ Lowers cs s s' row e := by
  cases value with
  | atom single =>
    simp only [Lower.value] at h
    obtain ⟨e, found, lowers⟩ := atom_lowers wf h
    exact ⟨e, by simpa [SourceValue.expression?] using found, lowers⟩
  | binary op left right =>
    simp only [Lower.value, run_bind, run_require] at h
    obtain ⟨_, s1, ⟨notComparison, rfl⟩, h⟩ := h
    obtain ⟨first, second, hfirst, hsecond, lowers⟩ := comparison_lowers wf h
    have plain : op.isComparison = false := by simpa using notComparison
    exact ⟨op.expression first second, by simp [SourceValue.expression?, plain, hfirst, hsecond],
      lowers⟩

theorem value_block_lowers {cs : List (Option Callee)} {owner : BridgeId}
    {scope : List (String × Nat)} {value : SourceValue} {s s' : LoweringState} {row : Word32}
    (wf : BlocksIndexed s) (h : Lower.valueBlock owner scope value s = some (row, s')) :
    ∃ e, value.expression? scope = some e ∧ Lowers cs s s' row e := by
  simp only [Lower.valueBlock, run_bind] at h
  obtain ⟨valueRow, s1, hv, statement, s2, hs, block, s3, hb, span, s4, hspan, push⟩ := h
  obtain ⟨e, found, vl⟩ := value_lowers (cs := cs) wf hv
  obtain ⟨rest, spans, rfl⟩ := take_span_run hspan
  obtain ⟨statementRow, statementState, statementIndex, _⟩ := push_row_run hs
  have statementBlocks := push_row_blocks hs
  have wf2 : BlocksIndexed s2 := BlocksIndexed.congr statementBlocks.1 vl.wf
  have wf3 : BlocksIndexed s3 := push_block_indexed wf2 hb
  have blockLe := (push_block_le hb).1
  have spanLe : StateLE s3 { s3 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
  have wf4 : BlocksIndexed { s3 with spans := rest } := BlocksIndexed.congr (by simp) wf3
  have le4 : StateLE s1 { s3 with spans := rest } :=
    (push_row_le hs).trans (blockLe.trans spanLe)
  have same : ({ s3 with spans := rest } : LoweringState).functions = s.functions ∧
      ({ s3 with spans := rest } : LoweringState).callable = s.callable := by
    obtain ⟨a, b⟩ := vl.functions
    obtain ⟨_, hblock, _⟩ := push_block_run hb
    refine ⟨?_, ?_⟩
    · simp [hblock, statementState, a]
    · simp [hblock, statementState, b]
  refine ⟨e, found, ?_⟩
  refine lowers_row (vl.le.trans le4) wf4 same push ?_
  intro F le
  have at_ := (push_row_at push rfl).mono le
  have blockAt := (push_block_at wf2 hb).mono (spanLe.trans ((push_row_le push).trans le))
  have statementAt :=
    (push_row_at hs rfl).mono (blockLe.trans (spanLe.trans ((push_row_le push).trans le)))
  obtain ⟨_, _, rowIndex, _⟩ := push_row_run push
  have blockRows := (push_block_blocks_ops hb).2
  have valueBefore : valueRow.val < statement.val := by
    have := vl.before
    omega
  have statementBefore : statement.val < row.val := by
    simp only [rowIndex, blockRows, statementState]
    simp [statementIndex]
  exact reads_value_block (single := rfl) at_ blockAt statementBefore statementAt valueBefore
    (vl.reads ((push_row_le hs).trans (blockLe.trans (spanLe.trans
      ((push_row_le push).trans le)))))


/-- A lowering step that returns an integer-family row: the same bookkeeping as `Lowers`, with the
row reading as the expression through `intExpression?`. -/
def IntLowers (cs : List (Option Callee)) (s s' : LoweringState) (row : Word32) (e : BaseExpr) :
    Prop :=
  StateLE s s' ∧ BlocksIndexed s' ∧ row.val < s'.rows.length ∧
    s'.functions = s.functions ∧ s'.callable = s.callable ∧
    ∀ F : LoweringState, StateLE s' F → IntReads F cs row e

theorem int_lowers_row {cs : List (Option Callee)} {s0 s1 s' : LoweringState} {tag : FragmentTag}
    {payload : List Word32} {row : Word32} {e : BaseExpr} (le : StateLE s0 s1)
    (wf : BlocksIndexed s1) (same : s1.functions = s0.functions ∧ s1.callable = s0.callable)
    (push : Lower.pushRow tag payload s1 = some (row, s'))
    (reads : ∀ F : LoweringState, StateLE s' F → IntReads F cs row e) :
    IntLowers cs s0 s' row e := by
  obtain ⟨blocks, _, length⟩ := push_row_blocks push
  obtain ⟨pushed, hs', index, _⟩ := push_row_run push
  refine ⟨le.trans (push_row_le push), ?_, by omega, ?_, ?_, reads⟩
  · intro i block located
    rw [blocks] at located
    exact wf i block located
  · rw [hs']; exact same.1
  · rw [hs']; exact same.2

theorem IntLowers.le {cs : List (Option Callee)} {s s' : LoweringState} {row : Word32}
    {e : BaseExpr} (l : IntLowers cs s s' row e) : StateLE s s' := l.1

theorem IntLowers.wf {cs : List (Option Callee)} {s s' : LoweringState} {row : Word32}
    {e : BaseExpr} (l : IntLowers cs s s' row e) : BlocksIndexed s' := l.2.1

theorem IntLowers.before {cs : List (Option Callee)} {s s' : LoweringState} {row : Word32}
    {e : BaseExpr} (l : IntLowers cs s s' row e) : row.val < s'.rows.length := l.2.2.1

theorem IntLowers.functions {cs : List (Option Callee)} {s s' : LoweringState} {row : Word32}
    {e : BaseExpr} (l : IntLowers cs s s' row e) :
    s'.functions = s.functions ∧ s'.callable = s.callable := ⟨l.2.2.2.1, l.2.2.2.2.1⟩

theorem IntLowers.reads {cs : List (Option Callee)} {s s' F : LoweringState} {row : Word32}
    {e : BaseExpr} (l : IntLowers cs s s' row e) (le : StateLE s' F) : IntReads F cs row e :=
  l.2.2.2.2.2 F le

theorem int_atom_lowers {cs : List (Option Callee)} {scope : List (String × Nat)}
    {atom : SourceAtom} {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s)
    (h : Lower.intAtom scope atom s = some (row, s')) :
    ∃ e, atom.expression? scope = some e ∧ IntLowers cs s s' row e := by
  cases atom with
  | int value =>
    simp only [Lower.intAtom, run_bind, run_pure, run_require, run_lift, Lower.word] at h
    obtain ⟨words, s1, literal, push⟩ := h
    obtain ⟨ok, rfl, low, high, rfl, lowEq, highEq⟩ := literal_words_run literal
    refine ⟨.integerLiteral (value : Int), by simp [SourceAtom.expression?, ok], ?_⟩
    refine int_lowers_row (StateLE.refl _) wf ⟨rfl, rfl⟩ push ?_
    intro F le
    have at_ := (push_row_at push rfl).mono le
    have := int_reads_int (callees := cs) at_
    rwa [signed_of_words_literal ok lowEq highEq] at this
  | var name =>
    simp only [Lower.intAtom, run_bind, run_pure, run_require, run_lift, Lower.word] at h
    obtain ⟨slot, s1, ⟨found, rfl⟩, w, s2, ⟨decoded, rfl⟩, push⟩ := h
    refine ⟨.identifier (NameId.mk w), by simp [SourceAtom.expression?, found, decoded], ?_⟩
    refine int_lowers_row (StateLE.refl _) wf ⟨rfl, rfl⟩ push ?_
    intro F le
    exact int_reads_slot (callees := cs) ((push_row_at push rfl).mono le)

theorem int_value_lowers {cs : List (Option Callee)} {scope : List (String × Nat)}
    {value : SourceValue} {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s)
    (h : Lower.intValue scope value s = some (row, s')) :
    ∃ e, value.expression? scope = some e ∧ IntLowers cs s s' row e := by
  cases value with
  | atom single =>
    simp only [Lower.intValue] at h
    obtain ⟨e, found, lowers⟩ := int_atom_lowers wf h
    exact ⟨e, by simpa [SourceValue.expression?] using found, lowers⟩
  | binary op left right =>
    simp only [Lower.intValue, run_bind, run_require] at h
    obtain ⟨_, s0, ⟨notComparison, h0⟩, leftRow, s1, hl, rightRow, s2, hr, operation, s3, hop,
      push⟩ := h
    have h0' : s = s0 := h0.symm
    subst h0'
    have plain : op.isComparison = false := by simpa using notComparison
    obtain ⟨first, hfirst, ll⟩ := int_atom_lowers (cs := cs) wf hl
    obtain ⟨second, hsecond, rl⟩ := int_atom_lowers (cs := cs) ll.wf hr
    obtain ⟨opLe, pooled, opRows, opExtra, opBlocks, opFunctions, opCallable⟩ :=
      operation_index_run hop
    have wf3 : BlocksIndexed s3 := BlocksIndexed.congr (s := s2) (by simpa using opBlocks) rl.wf
    have same : s3.functions = s.functions ∧ s3.callable = s.callable := by
      obtain ⟨a, b⟩ := ll.functions
      obtain ⟨c, d⟩ := rl.functions
      exact ⟨by simp [opFunctions, c, a], by simp [opCallable, d, b]⟩
    refine ⟨op.expression first second,
      by simp [SourceValue.expression?, plain, hfirst, hsecond], ?_⟩
    refine int_lowers_row (ll.le.trans (rl.le.trans opLe)) wf3 same push ?_
    intro F le
    have at_ := (push_row_at push rfl).mono le
    obtain ⟨pushed, hs', index, _⟩ := push_row_run push
    have pushLe := push_row_le push
    have leftBefore : leftRow.val < row.val := by
      have a := ll.before
      have b := state_le_rows_length (rl.le.trans opLe)
      simp only [index]; omega
    have rightBefore : rightRow.val < row.val := by
      have a := rl.before
      have b := state_le_rows_length opLe
      simp only [index]; omega
    have pooledF : F.ops[operation.val]? = some op := op_mono (pushLe.trans le) pooled
    exact int_reads_binary at_ leftBefore rightBefore pooledF plain
      (ll.reads (rl.le.trans (opLe.trans (pushLe.trans le))))
      (rl.reads (opLe.trans (pushLe.trans le)))

theorem push_row_frame {tag : FragmentTag} {payload : List Word32} {s s' : LoweringState}
    {r : Word32} (h : Lower.pushRow tag payload s = some (r, s')) :
    s'.functions = s.functions ∧ s'.callable = s.callable := by
  obtain ⟨row, rfl, _⟩ := push_row_run h
  exact ⟨rfl, rfl⟩

theorem push_block_frame {owner : BridgeId} {flags : WireByte} {words : List Word32}
    {s s' : LoweringState} {id : BridgeId} (h : Lower.pushBlock owner flags words s = some (id, s')) :
    s'.functions = s.functions ∧ s'.callable = s.callable := by
  obtain ⟨block, rfl, _⟩ := push_block_run h
  exact ⟨rfl, rfl⟩

theorem atoms_lower {cs : List (Option Callee)} {scope : List (String × Nat)} :
    ∀ {atoms : List SourceAtom} {s s' : LoweringState} {rows : List Word32},
      BlocksIndexed s → (atoms.mapM (Lower.atom scope)) s = some (rows, s') →
      ∃ exprs, atoms.mapM (·.expression? scope) = some exprs ∧ StateLE s s' ∧
        BlocksIndexed s' ∧ (∀ row ∈ rows, row.val < s'.rows.length) ∧
        rows.length = atoms.length ∧ s'.functions = s.functions ∧ s'.callable = s.callable ∧
        ∀ F : LoweringState, StateLE s' F → ReadsAll F cs rows exprs := by
  intro atoms
  induction atoms with
  | nil =>
    intro s s' rows wf h
    simp only [List.mapM_nil, run_pure] at h
    obtain ⟨rfl, rfl⟩ := h
    exact ⟨[], rfl, StateLE.refl _, wf, by simp, rfl, rfl, rfl, fun _ _ => ReadsAll.nil⟩
  | cons atom rest ih =>
    intro s s' rows wf h
    simp only [List.mapM_cons, run_bind, run_pure] at h
    obtain ⟨row, s1, hatom, others, s2, hrest, rfl, rfl⟩ := h
    obtain ⟨e, found, lowers⟩ := atom_lowers (cs := cs) wf hatom
    obtain ⟨es, foundRest, le, wf2, before, length, fa, fb, reads⟩ := ih lowers.wf hrest
    refine ⟨e :: es, by simp [found, foundRest], lowers.le.trans le, wf2, ?_, by simp [length],
      by rw [fa]; exact lowers.functions.1, by rw [fb]; exact lowers.functions.2, ?_⟩
    · intro r member
      simp only [List.mem_cons] at member
      rcases member with rfl | member
      · have := lowers.before
        have := state_le_rows_length le
        omega
      · exact before r member
    · intro F leF
      exact ReadsAll.cons (lowers.reads (le.trans leF)) (reads F leF)

/-- The lowered functions and the source callees describe the same earlier functions: the same
names in the same order, a callee form exactly for the functions without a let, and the
parameter count the function row records. -/
def Aligned (s : LoweringState) (src : List (String × Option Callee)) : Prop :=
  s.functions.map (·.name) = src.map (·.1) ∧ s.callable = src.map (fun entry => entry.2.isSome) ∧
    ∀ (index : Nat) (row : FunctionRow) (callee : Callee), s.functions[index]? = some row →
      (src[index]?).bind (·.2) = some callee → row.paramsLen.val = callee.parameters

theorem find_idx_lookup (callee : String) :
    ∀ (functions : List FunctionRow) (src : List (String × Option Callee)) (index : Nat),
      functions.map (·.name) = src.map (·.1) →
      functions.findIdx? (·.name == callee) = some index →
      ∃ entry, src[index]? = some entry ∧ src.lookup callee = some entry.2 := by
  intro functions
  induction functions with
  | nil => intro src index _ found; simp at found
  | cons row rest ih =>
    intro src index names found
    cases src with
    | nil => simp at names
    | cons entry others =>
      simp only [List.map_cons, List.cons.injEq] at names
      obtain ⟨sameName, restNames⟩ := names
      by_cases hit : (row.name == callee) = true
      · have hitEntry : (callee == entry.1) = true := by
          simp only [beq_iff_eq] at hit ⊢
          rw [← sameName]; exact hit.symm
        simp [List.findIdx?_cons, hit] at found
        subst found
        exact ⟨entry, rfl, by simp [List.lookup, hitEntry]⟩
      · have missEntry : (callee == entry.1) = false := by
          simp only [Bool.not_eq_true, beq_eq_false_iff_ne, ne_eq] at hit ⊢
          rw [← sameName]; exact fun h => hit h.symm
        simp [List.findIdx?_cons, hit] at found
        obtain ⟨previous, restFound, rfl⟩ := found
        obtain ⟨target, located, looked⟩ := ih others previous restNames restFound
        exact ⟨target, by simpa using located, by simp [List.lookup, missEntry, looked]⟩

theorem call_lowers {src : List (String × Option Callee)} {owner : BridgeId}
    {scope : List (String × Nat)} {callee : String} {arguments : List SourceAtom}
    {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (h : Lower.expression owner scope (.call callee arguments) s = some (row, s')) :
    ∃ e, (SourceExpression.call callee arguments).expression? src scope = some e ∧
      Lowers (src.map (·.2)) s s' row e := by
  simp only [Lower.expression, Lower.pureCall, run_bind, run_get, run_lift, run_require,
    run_pure, Lower.word] at h
  obtain ⟨_, _, ⟨rfl, rfl⟩, index, _, ⟨hfind, rfl⟩, found, _, ⟨hfound, rfl⟩, _, _, ⟨rfl, rfl⟩,
    _, _, ⟨hcallable, rfl⟩, _, _, ⟨hlen, rfl⟩, rows, s7, hrows, count, _, ⟨hcount, rfl⟩, table, s9,
    htable, span, s10, hspan, w, _, ⟨hw, rfl⟩, push⟩ := h
  simp only [beq_iff_eq] at hcallable hlen
  obtain ⟨entry, located, looked⟩ := find_idx_lookup callee _ src index aligned.1 hfind
  have flag : (src.map fun entry => entry.2.isSome)[index]? = some true := by
    rw [← aligned.2.1]; exact hcallable
  simp only [List.getElem?_map, located, Option.map_some, Option.some.injEq] at flag
  obtain ⟨callee0, hcallee0⟩ := Option.isSome_iff_exists.mp flag
  have width : found.paramsLen.val = callee0.parameters :=
    aligned.2.2 index found callee0 hfound (by simp [located, hcallee0])
  obtain ⟨exprs, foundArguments, le1, wf1, before, rowsLength, argFa, argFb, readsArguments⟩ :=
    atoms_lower (cs := src.map (·.2)) wf hrows
  have countValue := decode_val hcount
  have wordValue := decode_val hw
  obtain ⟨rest, spans, rfl⟩ := take_span_run hspan
  obtain ⟨blockLe, blockRows⟩ := push_block_le htable
  have spanLe : StateLE s9 { s9 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
  have wf2 : BlocksIndexed s9 := push_block_indexed wf1 htable
  have wf3 : BlocksIndexed { s9 with spans := rest } := BlocksIndexed.congr rfl wf2
  have blockFrame := push_block_frame htable
  refine ⟨.call (parameterNames callee0.parameters) callee0.body exprs, ?_, ?_⟩
  · have lookedUp : (src.lookup callee).bind id = some callee0 := by
      rw [looked, hcallee0]; rfl
    have countFits : arguments.length = callee0.parameters := by
      rw [← width]; exact hlen
    simp [SourceExpression.expression?, lookedUp, countFits, foundArguments]
  · refine lowers_row (le1.trans (blockLe.trans spanLe)) wf3
      ⟨by simp [blockFrame.1, argFa], by simp [blockFrame.2, argFb]⟩ push ?_
    intro F le
    have at_ := (push_row_at push rfl).mono le
    obtain ⟨_, _, rowIndex, _⟩ := push_row_run push
    have tableAt := (push_block_at wf1 htable).mono (spanLe.trans ((push_row_le push).trans le))
    have fitsArguments : rows.length = callee0.parameters := by
      rw [rowsLength, ← width]; exact hlen
    exact reads_call at_ (by omega) (by simp [wordValue, located, hcallee0]) tableAt
      (by rw [countValue, ← width]; exact hlen) fitsArguments
      (fun argument member => by
        have := before argument member
        simp only [rowIndex, blockRows]
        simpa using this)
      (readsArguments F ((blockLe.trans spanLe).trans ((push_row_le push).trans le)))


theorem conditional_lowers {src : List (String × Option Callee)} {cs : List (Option Callee)}
    {owner : BridgeId} {scope : List (String × Nat)} {op : FragmentOp}
    {left right : SourceAtom} {thenValue elseValue : SourceValue} {s s' : LoweringState}
    {row : Word32} (wf : BlocksIndexed s)
    (h : Lower.expression owner scope (.conditional op left right thenValue elseValue) s =
      some (row, s')) :
    ∃ e, (SourceExpression.conditional op left right thenValue elseValue).expression?
        src scope = some e ∧ Lowers cs s s' row e := by
  simp only [Lower.expression, run_bind, run_require] at h
  obtain ⟨_, _, ⟨isComparison, rfl⟩, condition, s2, hcond, thenRow, s3, hthen, table, s4, htable,
    elseRow, s5, helse, span, s6, hspan, push⟩ := h
  obtain ⟨first, second, hfirst, hsecond, cl⟩ := comparison_lowers (cs := cs) wf hcond
  obtain ⟨thenE, foundThen, tl⟩ := value_block_lowers (cs := cs) cl.wf hthen
  have wf4 := push_block_indexed tl.wf htable
  obtain ⟨blockLe, blockRows⟩ := push_block_le htable
  obtain ⟨elseE, foundElse, el⟩ := value_block_lowers (cs := cs) wf4 helse
  obtain ⟨rest, spans, rfl⟩ := take_span_run hspan
  have spanLe : StateLE s5 { s5 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
  have wf6 : BlocksIndexed { s5 with spans := rest } := BlocksIndexed.congr rfl el.wf
  have tableFrame := push_block_frame htable
  have comparisonHolds : op.isComparison = true := by simpa using isComparison
  refine ⟨.conditional (op.expression first second) thenE elseE, ?_, ?_⟩
  · simp [SourceExpression.expression?, comparisonHolds, hfirst, hsecond, foundThen, foundElse]
  · refine lowers_row (cl.le.trans (tl.le.trans (blockLe.trans (el.le.trans spanLe)))) wf6
      ⟨by simp [el.functions.1, tableFrame.1, tl.functions.1, cl.functions.1],
       by simp [el.functions.2, tableFrame.2, tl.functions.2, cl.functions.2]⟩ push ?_
    intro F le
    have at_ := (push_row_at push rfl).mono le
    obtain ⟨_, _, rowIndex, _⟩ := push_row_run push
    have tableAt := (push_block_at tl.wf htable).mono
      (el.le.trans (spanLe.trans ((push_row_le push).trans le)))
    have toFinal : StateLE s5 F := spanLe.trans ((push_row_le push).trans le)
    have conditionBefore : condition.val < row.val := by
      have a := cl.before
      have b := state_le_rows_length (tl.le.trans (blockLe.trans el.le))
      simp only [rowIndex]; simp at b ⊢; omega
    have thenBefore : thenRow.val < row.val := by
      have a := tl.before
      have b := state_le_rows_length (blockLe.trans el.le)
      simp only [rowIndex]; simp at b ⊢; omega
    have elseBefore : elseRow.val < row.val := by
      have a := el.before
      simp only [rowIndex]; simpa using a
    exact reads_if (count := 1) at_ tableAt rfl conditionBefore thenBefore elseBefore
      (cl.reads ((tl.le.trans (blockLe.trans el.le)).trans toFinal))
      (tl.reads ((blockLe.trans el.le).trans toFinal)) (el.reads toFinal)

theorem constructed_lowers {cs : List (Option Callee)} {src : List (String × Option Callee)}
    {owner : BridgeId}
    {scope : List (String × Nat)} {isOk : Bool} {payload : SourceValue}
    {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s)
    (h : Lower.expression owner scope (.constructed isOk payload) s = some (row, s')) :
    ∃ e, (SourceExpression.constructed isOk payload).expression? src scope = some e ∧
      Lowers cs s s' row e := by
  simp only [Lower.expression, run_bind] at h
  obtain ⟨valueRow, s1, hv, push⟩ := h
  obtain ⟨e, found, vl⟩ := value_lowers (cs := cs) wf hv
  refine ⟨constructorExpression isOk e, by simp [SourceExpression.expression?, found], ?_⟩
  have valueBefore : valueRow.val < row.val := by
    obtain ⟨_, _, rowIndex, _⟩ := push_row_run push
    have := vl.before
    simp only [rowIndex]; simp; omega
  cases isOk
  · simp only [Bool.false_eq_true, if_false] at push
    exact lowers_row vl.le vl.wf vl.functions push fun F le =>
      reads_err ((push_row_at push rfl).mono le) valueBefore rfl
        (vl.reads ((push_row_le push).trans le))
  · simp only [if_true] at push
    exact lowers_row vl.le vl.wf vl.functions push fun F le =>
      reads_ok ((push_row_at push rfl).mono le) valueBefore
        (vl.reads ((push_row_le push).trans le))

theorem expression_lowers {src : List (String × Option Callee)} {owner : BridgeId}
    {scope : List (String × Nat)} {expression : SourceExpression} {s s' : LoweringState}
    {row : Word32} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (h : Lower.expression owner scope expression s = some (row, s')) :
    ∃ e, expression.expression? src scope = some e ∧ Lowers (src.map (·.2)) s s' row e := by
  cases expression with
  | value single =>
    simp only [Lower.expression] at h
    obtain ⟨e, found, lowers⟩ := value_lowers wf h
    exact ⟨e, by simpa [SourceExpression.expression?] using found, lowers⟩
  | call callee arguments => exact call_lowers wf aligned h
  | conditional op left right thenValue elseValue => exact conditional_lowers wf h
  | assigned target assignedValue result =>
    simp only [Lower.expression] at h
    obtain ⟨e, found, lowers⟩ := value_lowers wf h
    exact ⟨e, by simpa [SourceExpression.expression?] using found, lowers⟩
  | comparison op left right =>
    simp only [Lower.expression, run_bind, run_require] at h
    obtain ⟨_, s1, ⟨isComparison, rfl⟩, h⟩ := h
    obtain ⟨first, second, hfirst, hsecond, lowers⟩ := comparison_lowers wf h
    have holds : op.isComparison = true := by simpa using isComparison
    exact ⟨op.expression first second,
      by simp [SourceExpression.expression?, holds, hfirst, hsecond], lowers⟩
  | constructed isOk value => exact constructed_lowers wf h


theorem bridge_id_beq (a b : BridgeId) : (a == b) = (a.raw == b.raw) := by
  by_cases same : a = b
  · subst same; simp
  · have apart : a.raw ≠ b.raw := fun r => by
      apply same
      cases a; cases b; simp_all
    rw [beq_eq_false_iff_ne.mpr same, beq_eq_false_iff_ne.mpr apart]

def LoweringState.BlockOwned (s : LoweringState) (id : BridgeId) (owner : BridgeId)
    (words : List Word32) : Prop :=
  ∃ block, s.blocks.find? (fun block => block.id == id) = some block ∧
    block.owner = ⟨some owner⟩ ∧ block.start.val + block.len.val ≤ s.extra.length ∧
    slice s.extra block.start.val block.len.val = words

theorem LoweringState.BlockOwned.mono {s F : LoweringState} (le : StateLE s F) {id owner : BridgeId}
    {words : List Word32} (at_ : s.BlockOwned id owner words) : F.BlockOwned id owner words := by
  obtain ⟨block, found, ownerIs, within, same⟩ := at_
  obtain ⟨blocksMore, blocksEq⟩ := le.blocks
  obtain ⟨extraMore, extraEq⟩ := le.extra
  refine ⟨block, ?_, ownerIs, ?_, ?_⟩
  · rw [← blocksEq, List.find?_append, found]; rfl
  · rw [← extraEq]; simp; omega
  · rw [← extraEq, slice_append_left _ _ within]; exact same

theorem push_block_owned {owner : BridgeId} {flags : WireByte} {words : List Word32}
    {s s' : LoweringState} {id : BridgeId} (wf : BlocksIndexed s)
    (h : Lower.pushBlock owner flags words s = some (id, s')) : s'.BlockOwned id owner words := by
  obtain ⟨block, rfl, ownerIs, _, hid, hraw, start, length⟩ := push_block_run h
  refine ⟨block, ?_, ownerIs, ?_, ?_⟩
  · have earlier : s.blocks.find? (fun b => b.id == id) = none := by
      rw [List.find?_eq_none]
      intro candidate member
      obtain ⟨index, inBounds, rfl⟩ := List.getElem_of_mem member
      have indexed := wf index _ (List.getElem?_eq_getElem inBounds)
      have bound : index < s.blocks.length := inBounds
      simp only [bridge_id_beq, beq_iff_eq, Bool.not_eq_true]
      intro same
      have := congrArg Fin.val same
      omega
    simp [List.find?_append, earlier, hid]
  · simp [start, length]
  · simp only [slice, start, length]
    simp

theorem return_expression_of {F : LoweringState} {cs : List (Option Callee)}
    {returned operand : Word32} {e : BaseExpr}
    (at_ : F.RowAt returned.val .stmtReturn [operand]) (before : operand.val < returned.val)
    (rowBound : returned.val < F.rows.length) (reads : Reads F cs operand e) :
    returnExpression? (F.context cs) returned = some e := by
  have fuel : operand.val < (F.context cs).tables.tags.size + 1 := by
    simp [LoweringState.tables]; omega
  have read := reads (F.tables.tags.size + 1) (by simpa using fuel)
  simp [returnExpression?, readInstruction_of_rowAt at_, before, read]

theorem body_return {F : LoweringState} {cs : List (Option Callee)} {returned operand : Word32}
    {e : BaseExpr} (at_ : F.RowAt returned.val .stmtReturn [operand])
    (before : operand.val < returned.val) (rowBound : returned.val < F.rows.length)
    (reads : Reads F cs operand e) :
    body? (F.context cs) [returned] = some ([], none, none, e) := by
  have returnRead := return_expression_of at_ before rowBound reads
  simp [body?, bodyStep, readInstruction_of_rowAt at_, returnRead]

theorem body_assign_return {F : LoweringState} {cs : List (Option Callee)}
    {assign returned slot kind valueRow span : Word32} {valueExpression e : BaseExpr}
    (assignAt : F.RowAt assign.val .stmtAssignInt [slot, kind, valueRow, span])
    (plain : kind.val = 0) (valueBefore : valueRow.val < assign.val)
    (rowBound : assign.val < F.rows.length) (valueReads : IntReads F cs valueRow valueExpression)
    (returnRead : returnExpression? (F.context cs) returned = some e) :
    body? (F.context cs) [assign, returned] = some ([], some ⟨slot, valueExpression⟩, none, e) := by
  have fuel : valueRow.val < (F.context cs).tables.tags.size + 1 := by
    simp [LoweringState.tables]; omega
  have read := valueReads (F.tables.tags.size + 1) (by simpa using fuel)
  have kindZero : kind = 0 := Fin.ext plain
  simp [body?, bodyStep, readInstruction_of_rowAt assignAt, read, returnRead, kindZero,
    valueBefore]

theorem propagation_of {F : LoweringState} {cs : List (Option Callee)}
    {row slot valueRow annotation callRow trySpan : Word32}
    {parameters : List Xsh.Syntax.RawName} {body : BaseExpr} {args : List BaseExpr}
    (bindingAt : F.RowAt row.val .stmtLet [slot, valueRow, annotation])
    (annotationZero : annotation.val = 0) (valueBefore : valueRow.val < row.val)
    (tryAt : F.RowAt valueRow.val .exprTry [callRow, trySpan])
    (callBefore : callRow.val < valueRow.val)
    (readsCall : Reads F cs callRow (.call parameters body args)) :
    propagationOf? (F.context cs) row = some ⟨slot, parameters, body, args⟩ := by
  unfold propagationOf?
  have rowBound : row.val < F.rows.length := by
    obtain ⟨found, located, _, _, _, _⟩ := bindingAt
    exact (List.getElem?_eq_some_iff.mp located).1
  have callRead := readsCall (F.tables.tags.size + 1) (by
    simp [LoweringState.tables]; omega)
  have annotationIs : annotation = 0 := Fin.ext annotationZero
  simp [readInstruction_of_rowAt bindingAt, readInstruction_of_rowAt tryAt, annotationIs,
    valueBefore, callBefore, callRead]

theorem body_propagation_return {F : LoweringState} {cs : List (Option Callee)}
    {binding returned slot valueRow annotation : Word32} {propagation : Propagation}
    {e : BaseExpr}
    (bindingAt : F.RowAt binding.val .stmtLet [slot, valueRow, annotation])
    (found : propagationOf? (F.context cs) binding = some propagation)
    (returnRead : returnExpression? (F.context cs) returned = some e) :
    body? (F.context cs) [binding, returned] = some ([], none, some propagation, e) := by
  simp [body?, bodyStep, readInstruction_of_rowAt bindingAt, found, returnRead]

theorem propagation_lowers {src : List (String × Option Callee)} {owner : BridgeId}
    {scope : List (String × Nat)} {propagation : SourcePropagation}
    {s s' : LoweringState} {row : Word32} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (h : Lower.propagation owner scope propagation s = some (row, s')) :
    ∃ slot word callee args tryRow,
      scope.lookup propagation.target = some slot ∧ Word32.decode? slot = some word ∧
      (src.lookup propagation.callee).bind id = some callee ∧
      propagation.arguments.length = callee.parameters ∧
      propagation.arguments.mapM (·.expression? scope) = some args ∧
      StateLE s s' ∧ BlocksIndexed s' ∧ s'.functions = s.functions ∧ s'.callable = s.callable ∧
      row.val < s'.rows.length ∧
      ∀ (F : LoweringState), StateLE s' F →
        F.RowAt row.val .stmtLet [word, tryRow, 0] ∧
        propagationOf? (F.context (src.map (·.2))) row =
          some ⟨word, parameterNames callee.parameters, callee.body, args⟩ := by
  simp only [Lower.propagation, run_bind, run_lift, Lower.word] at h
  obtain ⟨callRow, s1, hcall, trySpan, s2, htrySpan, tried, s3, htry, slot, s4, ⟨hslot, rfl⟩,
    w, s5, ⟨hw, rfl⟩, push⟩ := h
  obtain ⟨e, hsrc, callLowers⟩ := call_lowers wf aligned hcall
  unfold SourceExpression.expression? at hsrc
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at hsrc
  obtain ⟨callee, hcallee, hsrc⟩ := hsrc
  have lookedUp : (src.lookup propagation.callee).bind id = some callee := by
    obtain ⟨entry, hlookup, hid⟩ := hcallee
    rw [hlookup]
    exact hid
  by_cases countFits : (propagation.arguments.length == callee.parameters) = true
  · rw [if_pos countFits] at hsrc
    simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at hsrc
    obtain ⟨args, foundArguments, he⟩ := hsrc
    have he' : e = .call (parameterNames callee.parameters) callee.body args := by
      simpa using he.symm
    subst he'
    have countFitsEq : propagation.arguments.length = callee.parameters := by
      simpa [beq_iff_eq] using countFits
    obtain ⟨rest, spans, rfl⟩ := take_span_run htrySpan
    have spanLe : StateLE s1 { s1 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
    have wf2 : BlocksIndexed { s1 with spans := rest } := BlocksIndexed.congr rfl callLowers.wf
    obtain ⟨_, _, tryIndex, _⟩ := push_row_run htry
    have tryLe := push_row_le htry
    have tryFrame := push_row_frame htry
    have wf3 := BlocksIndexed.congr (push_row_blocks htry).1 wf2
    obtain ⟨_, _, rowIndex, _⟩ := push_row_run push
    have pushLe := push_row_le push
    have pushFrame := push_row_frame push
    have wf4 : BlocksIndexed s' := BlocksIndexed.congr (push_row_blocks push).1 wf3
    have callBefore : callRow.val < tried.val := by
      have := callLowers.before
      simp only [tryIndex]
      simp at this ⊢
      omega
    have tryBefore : tried.val < row.val := by
      have step := (push_row_blocks htry).2.2
      simp only [rowIndex]
      omega
    refine ⟨slot, w, callee, args, tried, hslot, hw, lookedUp, countFitsEq, foundArguments, ?_, ?_, ?_,
      ?_, ?_, ?_⟩
    · exact callLowers.le.trans (spanLe.trans (tryLe.trans pushLe))
    · exact wf4
    · rw [pushFrame.1, tryFrame.1, callLowers.functions.1]
    · rw [pushFrame.2, tryFrame.2, callLowers.functions.2]
    · have length := (push_row_blocks push).2.2
      simp only [rowIndex] at length ⊢
      omega
    · intro F le
      have toTry := tryLe.trans (pushLe.trans le)
      refine ⟨(push_row_at push rfl).mono le, ?_⟩
      exact propagation_of ((push_row_at push rfl).mono le) rfl tryBefore
        ((push_row_at htry rfl).mono (pushLe.trans le)) callBefore
        (callLowers.reads (spanLe.trans (tryLe.trans (pushLe.trans le))))
  · rw [if_neg countFits] at hsrc
    simp at hsrc

theorem fresh_bool (bindings : List LetBinding) (slot : Word32)
    (fresh : ∀ binding ∈ bindings, binding.slot ≠ slot) :
    (!bindings.any fun binding => binding.slot.val == slot.val) = true := by
  induction bindings with
  | nil => rfl
  | cons head rest ih =>
    have headFresh : head.slot.val ≠ slot.val := fun h => fresh head (by simp) (Fin.ext h)
    have restFresh := ih (fun b m => fresh b (by simp [m]))
    simp only [List.any_cons, Bool.not_or]
    rw [Bool.and_eq_true]
    refine ⟨?_, restFresh⟩
    simp [headFresh]

theorem body_let_cons {ctx : MeaningContext} {statement slot valueRow : Word32}
    {next : Word32} {more : List Word32} {value : IntValue} {bindings : List LetBinding}
    {assignment : Option Assignment} {propagation : Option Propagation} {result : BaseExpr}
    (statementAt : readInstruction? ctx.tables statement = some (.stmtLetInt, [slot, valueRow]))
    (valueBefore : valueRow.val < statement.val)
    (literal : literalValue? ctx valueRow = some value)
    (rest : body? ctx (next :: more) = some (bindings, assignment, propagation, result))
    (fresh : ∀ binding ∈ bindings, binding.slot ≠ slot) :
    body? ctx (statement :: next :: more) =
      some (⟨slot, value⟩ :: bindings, assignment, propagation, result) := by
  have freshBool := fresh_bool bindings slot fresh
  have beforeChecked : check (decide (valueRow.val < statement.val)) = some () := by
    unfold check; exact if_pos (decide_eq_true valueBefore)
  have freshChecked : check (!bindings.any fun binding => binding.slot.val == slot.val) = some () := by
    unfold check; exact if_pos freshBool
  rw [body?.eq_2]
  simp only [bodyStep, statementAt, beforeChecked, literal, rest, freshChecked,
    Option.bind_eq_bind, Option.bind_some]

theorem literal_value_of {ctx : MeaningContext} {row low high : Word32} {value : IntValue}
    (read : readInstruction? ctx.tables row = some (.intInt, [low, high]))
    (valid : validateInt? (signedOfWords low high) = some value) :
    literalValue? ctx row = some value := by
  unfold literalValue?
  simp only [read, Option.bind]
  exact valid

theorem body_let_return {F : LoweringState} {cs : List (Option Callee)}
    {binding literal returned slot low high operand : Word32} {value : IntValue} {e : BaseExpr}
    (bindingAt : F.RowAt binding.val .stmtLetInt [slot, literal])
    (literalBefore : literal.val < binding.val) (literalAt : F.RowAt literal.val .intInt [low, high])
    (valid : validateInt? (signedOfWords low high) = some value)
    (returnAt : F.RowAt returned.val .stmtReturn [operand]) (before : operand.val < returned.val)
    (rowBound : returned.val < F.rows.length) (reads : Reads F cs operand e) :
    body? (F.context cs) [binding, returned] = some ([⟨slot, value⟩], none, none, e) :=
  body_let_cons (readInstruction_of_rowAt bindingAt) literalBefore
    (literal_value_of (readInstruction_of_rowAt literalAt) valid)
    (body_return returnAt before rowBound reads) (fun _ member => by cases member)

theorem body_let_assign_return {F : LoweringState} {cs : List (Option Callee)}
    {binding literal assign returned slot low high assignSlot kind valueRow span operand : Word32}
    {value : IntValue} {valueExpression e : BaseExpr}
    (bindingAt : F.RowAt binding.val .stmtLetInt [slot, literal])
    (literalBefore : literal.val < binding.val) (literalAt : F.RowAt literal.val .intInt [low, high])
    (valid : validateInt? (signedOfWords low high) = some value)
    (assignAt : F.RowAt assign.val .stmtAssignInt [assignSlot, kind, valueRow, span])
    (plain : kind.val = 0) (valueBefore : valueRow.val < assign.val)
    (assignBound : assign.val < F.rows.length) (valueReads : IntReads F cs valueRow valueExpression)
    (returnAt : F.RowAt returned.val .stmtReturn [operand]) (before : operand.val < returned.val)
    (rowBound : returned.val < F.rows.length) (reads : Reads F cs operand e) :
    body? (F.context cs) [binding, assign, returned] =
      some ([⟨slot, value⟩], some ⟨assignSlot, valueExpression⟩, none, e) :=
  body_let_cons (readInstruction_of_rowAt bindingAt) literalBefore
    (literal_value_of (readInstruction_of_rowAt literalAt) valid)
    (body_assign_return assignAt plain valueBefore assignBound valueReads
      (return_expression_of returnAt before rowBound reads))
    (fun _ member => by cases member)


theorem statement_rows_of {artifact : RawArtifact} {function : FunctionRow} {block : Block}
    {count : Word32} {statements : List Word32}
    (found : artifact.blocks.find? (fun block => block.id == function.body) = some block)
    (owned : block.owner = ⟨some function.id⟩)
    (payload : Decode.readPayload? artifact.tables ⟨DirectIndex.decode block.start, block.len⟩ =
      some (count :: statements)) (counted : count.val = statements.length) :
    statementRows? artifact artifact.tables function = some statements := by
  unfold statementRows?
  simp only [found, owned, payload, counted, Option.bind_eq_bind, Option.bind_some, check_bind]
  simp

theorem function_program_of {artifact : RawArtifact} {callees : List (Option Callee)}
    {function : FunctionRow} {statements : List Word32} {bindings : List LetBinding}
    {assignment : Option Assignment} {propagation : Option Propagation} {e : BaseExpr}
    (rows : statementRows? artifact artifact.tables function = some statements)
    (body : body? (artifact.context callees) statements =
      some (bindings, assignment, propagation, e)) :
    functionProgram? artifact callees function =
      some ⟨function.paramsLen.val, bindings, assignment, propagation, e⟩ := by
  unfold functionProgram?
  simp only [rows, body, Option.bind_eq_bind, Option.bind_some]
  rfl


theorem function_tail_lowers {src : List (String × Option Callee)} {f : SourceFunction}
    {scope : List (String × Nat)} {owner : BridgeId} {s s' : LoweringState}
    {functionsBefore parametersStart : Nat} {statements : List Word32}
    {bindings : List LetBinding} {assignment : Option Assignment}
    {propagation : Option Propagation} (wf : BlocksIndexed s)
    (aligned : Aligned s src)
    (statementsRead : ∀ (F : LoweringState), StateLE s F →
      ∀ (returned operand : Word32) (e : BaseExpr), F.RowAt returned.val .stmtReturn [operand] →
        operand.val < returned.val → returned.val < F.rows.length →
          Reads F (src.map (·.2)) operand e →
          body? (F.context (src.map (·.2))) (statements ++ [returned]) =
            some (bindings, assignment, propagation, e))
    (h : Lower.functionTail f scope owner functionsBefore parametersStart statements s =
      some ((), s')) :
    ∃ e row, f.result.expression? src scope = some e ∧
      s'.functions = s.functions ++ [row] ∧
      s'.callable = s.callable ++ [f.callable] ∧
      StateLE s s' ∧ BlocksIndexed s' ∧ row.name = f.name ∧
      row.paramsLen.val = f.parameters.length ∧
      ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId) (observations : List Observation),
        functionProgram? (F.toArtifact sourceId observations) (src.map (·.2)) row =
          some ⟨f.parameters.length, bindings, assignment, propagation, e⟩ := by
  simp only [Lower.functionTail, run_bind, run_modify, run_pure, Lower.word, run_lift] at h
  obtain ⟨result, s1, hexpr, returned, s2, hret, count, s3, ⟨hcount, rfl⟩, body, s4, hbody,
    parameters, s5, ⟨hparams, rfl⟩, pStart, s6, ⟨hpstart, rfl⟩, fBefore, s7, ⟨hfb, rfl⟩, slots,
    s8, ⟨hslots, rfl⟩, rfl⟩ := h
  obtain ⟨e, hsrc, lw⟩ := expression_lowers wf aligned hexpr
  have retAt := push_row_at hret rfl
  obtain ⟨_, _, retIndex, _⟩ := push_row_run hret
  have retFrame := push_row_frame hret
  have retBlocks := push_row_blocks hret
  have wf2 := BlocksIndexed.congr retBlocks.1 lw.wf
  have bodyOwned := push_block_owned wf2 hbody
  have bodyFrame := push_block_frame hbody
  have bodyLe := (push_block_le hbody).1
  have retLe := push_row_le hret
  have wf4 := push_block_indexed wf2 hbody
  have chainLe := lw.le.trans (retLe.trans bodyLe)
  have paramsValue := decode_val hparams
  refine ⟨e, ⟨owner, f.name, pStart, parameters, fBefore, 1, body, slots⟩, hsrc, ?_, ?_, chainLe.trans (StateLE.of_eq rfl rfl rfl rfl),
    BlocksIndexed.congr rfl wf4, rfl, paramsValue, ?_⟩
  · show _ ++ _ = _
    rw [bodyFrame.1, retFrame.1, lw.functions.1]
  · show _ ++ _ = _
    rw [bodyFrame.2, retFrame.2, lw.functions.2]
  · intro F le sourceId observations
    have toF : StateLE s8 F := StateLE.of_update le
    obtain ⟨block, found, ownerIs, within, same⟩ := LoweringState.BlockOwned.mono toF bodyOwned
    have atReturn := LoweringState.RowAt.mono (bodyLe.trans toF) retAt
    have boundReturn : returned.val < F.rows.length := by
      obtain ⟨row, located, _⟩ := atReturn
      exact (List.getElem?_eq_some_iff.mp located).1
    have resultBefore : result.val < returned.val := by
      have := lw.before
      omega
    have rowsOk : statementRows? (F.toArtifact sourceId observations)
        (F.toArtifact sourceId observations).tables
        ⟨owner, f.name, pStart, parameters, fBefore, 1, body, slots⟩ =
        some (statements ++ [returned]) := by
      refine statement_rows_of (count := count) found ownerIs ?_ ?_
      · have payload := readPayload_eq F.tables block.start block.len
          (by simpa [LoweringState.tables] using within)
        simp only [toArtifact_tables]
        rw [payload]
        simp only [LoweringState.tables] at same ⊢
        simp [same]
      · simp [decode_val hcount]
    have bodyRead := statementsRead F (chainLe.trans toF) returned result e atReturn
      resultBefore boundReturn (lw.reads (retLe.trans (bodyLe.trans toF)))
    have program := function_program_of rowsOk bodyRead
    dsimp only at program
    rw [paramsValue] at program
    exact program


theorem Aligned.congr {s s' : LoweringState} {src : List (String × Option Callee)}
    (functions : s'.functions = s.functions) (callable : s'.callable = s.callable)
    (aligned : Aligned s src) : Aligned s' src := by
  obtain ⟨names, flags, widths⟩ := aligned
  refine ⟨by rw [functions]; exact names, by rw [callable]; exact flags, ?_⟩
  intro index row callee located
  rw [functions] at located
  exact widths index row callee located

theorem validate_int_of_literal {value : Nat} (small : value ≤ maxIntLiteral) :
    ∃ bound, validateInt? (value : Int) = some bound := by
  unfold validateInt?
  have inRange : signed64Min ≤ (value : Int) ∧ (value : Int) < signed64Limit := by
    unfold maxIntLiteral at small
    unfold signed64Min signed64Limit
    omega
  exact ⟨_, dif_pos inRange⟩

theorem assignments_plain {name : String} {scope : List (String × Nat)}
    {result : SourceExpression} (plain : result.isAssigned = false) :
    Lower.assignments name scope result = pure [] := by
  cases result <;> simp_all [Lower.assignments, SourceExpression.isAssigned]

theorem assignment_plain {f : SourceFunction} {scope : List (String × Nat)}
    (plain : f.result.isAssigned = false) : f.assignment? scope = some none := by
  obtain ⟨name, parameters, lets, propagation, result⟩ := f
  cases result <;> simp_all [SourceFunction.assignment?, SourceExpression.isAssigned]

theorem assignment_lowers {scope : List (String × Nat)}
    {target : String} {value : SourceValue} {s s' : LoweringState} {row : Word32}
    (wf : BlocksIndexed s) (h : Lower.assignment scope target value s = some (row, s')) :
    ∃ slot w e valueRow span, scope.lookup target = some slot ∧ Word32.decode? slot = some w ∧
      value.expression? scope = some e ∧ StateLE s s' ∧ BlocksIndexed s' ∧
      s'.functions = s.functions ∧ s'.callable = s.callable ∧ row.val < s'.rows.length ∧
      valueRow.val < row.val ∧
      ∀ (cs : List (Option Callee)) (F : LoweringState), StateLE s' F →
        F.RowAt row.val .stmtAssignInt [w, 0, valueRow, span] ∧ IntReads F cs valueRow e := by
  simp only [Lower.assignment, run_bind, run_lift, Lower.word] at h
  obtain ⟨slot, s1, ⟨found, rfl⟩, valueRow, s2, hv, span, s3, hspan, w, s4, ⟨hw, rfl⟩, push⟩ := h
  obtain ⟨e, hsrc, vl⟩ := int_value_lowers (cs := []) wf hv
  obtain ⟨rest, spans, rfl⟩ := take_span_run hspan
  have spanLe : StateLE s2 { s2 with spans := rest } := StateLE.of_eq rfl rfl rfl rfl
  have wf3 : BlocksIndexed { s2 with spans := rest } := BlocksIndexed.congr (s := s2) rfl vl.wf
  obtain ⟨pushed, hs', index, _⟩ := push_row_run push
  have pushLe := push_row_le push
  have blocks := push_row_blocks push
  have frame := push_row_frame push
  refine ⟨slot, w, e, valueRow, span, found, hw, hsrc, vl.le.trans (spanLe.trans pushLe), ?_, ?_,
    ?_, ?_, ?_, ?_⟩
  · intro i block located
    rw [blocks.1] at located
    exact wf3 i block located
  · rw [frame.1]; exact vl.functions.1
  · rw [frame.2]; exact vl.functions.2
  · have len := blocks.2.2
    have := vl.before
    simp only [index] at *; simp at *; omega
  · have := vl.before
    simp only [index]; simp; omega
  · intro cs F le
    obtain ⟨e', hsrc', vl'⟩ := int_value_lowers (cs := cs) wf hv
    have same : e' = e := by
      rw [hsrc] at hsrc'
      exact (Option.some.inj hsrc').symm
    subst same
    exact ⟨(push_row_at push rfl).mono le, vl'.reads (spanLe.trans (pushLe.trans le))⟩

theorem function_lowers_plain {src : List (String × Option Callee)} {f : SourceFunction}
    {s s' : LoweringState} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (propagationNone : f.propagation = none)
    (plain : f.result.isAssigned = false) (h : Lower.function f s = some ((), s')) :
    ∃ program row, f.program? src = some program ∧ program.callee?.isSome = (f.callable) ∧
      program.parameters = f.parameters.length ∧ s'.functions = s.functions ++ [row] ∧
      s'.callable = s.callable ++ [f.callable] ∧
      StateLE s s' ∧ BlocksIndexed s' ∧
      row.name = f.name ∧ row.paramsLen.val = f.parameters.length ∧
      ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId) (observations : List Observation),
        functionProgram? (F.toArtifact sourceId observations) (src.map (·.2)) row = some program := by
  have plainResult := plain
  obtain ⟨name, parameters, lets, propagation, result⟩ := f
  dsimp only at plainResult
  subst propagationNone
  cases lets with
  | nil =>
    simp only [Lower.function, run_bind, run_lift, run_get, run_pure, run_require] at h
    obtain ⟨scope, _, ⟨hscope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨hown, rfl⟩, _, _, ⟨_, rfl⟩,
      tail⟩ := h
    obtain ⟨e, row, hsrc, fa, fb, le, wf', hname, hparams, reads⟩ :=
      function_tail_lowers (statements := []) (bindings := []) (assignment := none)
        (propagation := none) wf aligned
        (fun F _ returned operand e' at_ before bound rd => by
          simpa using body_return at_ before bound rd) tail
    refine ⟨⟨parameters.length, [], none, none, e⟩, row, ?_,
      by simp [FunctionProgram.callee?, SourceFunction.callable, plainResult], rfl, fa,
      fb, le, wf', hname, hparams, reads⟩
    have assignmentNone :
        (⟨name, parameters, [], none, result⟩ : SourceFunction).assignment? scope = some none := by
      cases result <;> simp_all [SourceFunction.assignment?, SourceExpression.isAssigned]
    simp [SourceFunction.program?, hscope, hsrc, assignmentNone]
  | cons binding more =>
    obtain ⟨bindingName, value⟩ := binding
    cases more with
    | nil =>
      cases parameters with
      | nil =>
        simp only [Lower.function, run_bind, run_lift, run_get, run_pure, Lower.word, run_require,
          assignments_plain plainResult] at h
        obtain ⟨scope, _, ⟨hscope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨hown, rfl⟩, words, _, hlit,
          literalRow, _, hpush1, slotWord, _, ⟨hslot, rfl⟩, bindingRow, _, hpush2, assigned, _,
          ⟨hassigned, rfl⟩, tail⟩ := h
        subst hassigned
        obtain ⟨ok, rfl, low, high, rfl, lowEq, highEq⟩ := literal_words_run hlit
        obtain ⟨bound, hbound⟩ := validate_int_of_literal ok
        have slotEq : slotWord = ⟨letSlot, by decide⟩ := Fin.ext (decode_val hslot)
        subst slotEq
        have frame1 := push_row_frame hpush1
        have frame2 := push_row_frame hpush2
        have blocks1 := (push_row_blocks hpush1).1
        have blocks2 := (push_row_blocks hpush2).1
        have wf2 := BlocksIndexed.congr blocks2 (BlocksIndexed.congr blocks1 wf)
        have alignedLet : Aligned _ src :=
          Aligned.congr (by rw [frame2.1, frame1.1]) (by rw [frame2.2, frame1.2]) aligned
        obtain ⟨_, _, literalIndex, _⟩ := push_row_run hpush1
        obtain ⟨_, _, bindingIndex, _⟩ := push_row_run hpush2
        have rowsGrow := (push_row_blocks hpush1).2.2
        obtain ⟨e, row, hsrc, fa, fb, le, wf', hname, hparams, reads⟩ :=
          function_tail_lowers (statements := [bindingRow])
            (bindings := [⟨⟨letSlot, by decide⟩, bound⟩]) (assignment := none)
            (propagation := none) wf2 alignedLet
            (fun F leF returned operand e' at_ before bound' rd => by
              have literalAt := (push_row_at hpush1 rfl).mono
                ((push_row_le hpush2).trans leF)
              have bindingAt := (push_row_at hpush2 rfl).mono leF
              have literalBefore : literalRow.val < bindingRow.val := by
                rw [bindingIndex]; omega
              have valid : validateInt? (signedOfWords low high) = some bound := by
                rw [signed_of_words_literal ok lowEq highEq]; exact hbound
              simpa using body_let_return bindingAt literalBefore literalAt valid at_ before bound' rd)
            tail
        refine ⟨⟨0, [⟨⟨letSlot, by decide⟩, bound⟩], none, none, e⟩, row, ?_,
          by simp [FunctionProgram.callee?, SourceFunction.callable, plainResult], rfl, ?_, ?_, ?_, wf', hname, hparams, reads⟩
        · have assignmentNone :
              (⟨name, [], [(bindingName, value)], none, result⟩ : SourceFunction).assignment?
                scope = some none := by
            cases result <;> simp_all [SourceFunction.assignment?, SourceExpression.isAssigned]
          have scopeEq : scope = [(bindingName, letSlot)] := by
            simpa [SourceFunction.scope?] using hscope.symm
          subst scopeEq
          simp [SourceFunction.program?, hscope, hbound, hsrc, assignmentNone, List.lookup, hslot]
        · rw [fa, frame2.1, frame1.1]
        · rw [fb, frame2.2, frame1.2]
        · exact ((push_row_le hpush1).trans (push_row_le hpush2)).trans le
      | cons p ps =>
        cases ps with
        | nil =>
          simp only [Lower.function, run_bind, run_lift, run_get, run_pure, Lower.word, run_require,
            assignments_plain plainResult] at h
          obtain ⟨scope, _, ⟨hscope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨hown, rfl⟩, words, _, hlit,
            literalRow, _, hpush1, slotWord, _, ⟨hslot, rfl⟩, bindingRow, _, hpush2, assigned, _,
            ⟨hassigned, rfl⟩, tail⟩ := h
          subst hassigned
          obtain ⟨ok, rfl, low, high, rfl, lowEq, highEq⟩ := literal_words_run hlit
          obtain ⟨bound, hbound⟩ := validate_int_of_literal ok
          have slotEq : slotWord = ⟨2, by decide⟩ := Fin.ext (decode_val hslot)
          subst slotEq
          have frame1 := push_row_frame hpush1
          have frame2 := push_row_frame hpush2
          have blocks1 := (push_row_blocks hpush1).1
          have blocks2 := (push_row_blocks hpush2).1
          have wf2 := BlocksIndexed.congr blocks2 (BlocksIndexed.congr blocks1 wf)
          have alignedLet : Aligned _ src :=
            Aligned.congr (by rw [frame2.1, frame1.1]) (by rw [frame2.2, frame1.2]) aligned
          obtain ⟨_, _, literalIndex, _⟩ := push_row_run hpush1
          obtain ⟨_, _, bindingIndex, _⟩ := push_row_run hpush2
          have rowsGrow := (push_row_blocks hpush1).2.2
          obtain ⟨e, row, hsrc, fa, fb, le, wf', hname, hparams, reads⟩ :=
            function_tail_lowers (statements := [bindingRow])
              (bindings := [⟨⟨2, by decide⟩, bound⟩]) (assignment := none)
              (propagation := none) wf2 alignedLet
              (fun F leF returned operand e' at_ before bound' rd => by
                have literalAt := (push_row_at hpush1 rfl).mono
                  ((push_row_le hpush2).trans leF)
                have bindingAt := (push_row_at hpush2 rfl).mono leF
                have literalBefore : literalRow.val < bindingRow.val := by
                  rw [bindingIndex]; omega
                have valid : validateInt? (signedOfWords low high) = some bound := by
                  rw [signed_of_words_literal ok lowEq highEq]; exact hbound
                simpa using body_let_return bindingAt literalBefore literalAt valid at_ before bound' rd)
              tail
          refine ⟨⟨1, [⟨⟨2, by decide⟩, bound⟩], none, none, e⟩, row, ?_,
            by simp [FunctionProgram.callee?, SourceFunction.callable, plainResult], rfl, ?_, ?_, ?_, wf', hname, hparams, reads⟩
          · have assignmentNone :
                (⟨name, [p], [(bindingName, value)], none, result⟩ : SourceFunction).assignment?
                  scope = some none := by
              cases result <;> simp_all [SourceFunction.assignment?, SourceExpression.isAssigned]
            have notSame : (p == bindingName) = false := by
              cases same : (p == bindingName) with
              | false => rfl
              | true => simp [SourceFunction.scope?, SourceExpression.isAssigned, plainResult,
                  same] at hscope
            have reverse : (bindingName == p) = false := by
              simp only [Bool.not_eq_true, beq_eq_false_iff_ne, ne_eq] at notSame ⊢
              exact fun h => notSame h.symm
            have scopeEq : scope = [(p, 0), (bindingName, 2)] := by
              have parts : result.isAssigned = false ∧ ¬p = bindingName ∧
                  scope = [(p, 0), (bindingName, 2)] := by
                simpa [SourceFunction.scope?] using hscope.symm
              exact parts.2.2
            subst scopeEq
            simp [SourceFunction.program?, hscope, hbound, hsrc, assignmentNone, List.lookup,
              reverse, hslot]
          · rw [fa, frame2.1, frame1.1]
          · rw [fb, frame2.2, frame1.2]
          · exact ((push_row_le hpush1).trans (push_row_le hpush2)).trans le
        | cons q qs =>
          simp [Lower.function, SourceFunction.scope?, run_bind, Lower.fail] at h
    | cons _ _ =>
      simp only [Lower.function, run_bind, run_lift, run_get] at h
      obtain ⟨scope, _, ⟨hscope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨hown, rfl⟩, tail⟩ := h
      exact absurd tail (by simp [Lower.fail])


theorem function_lowers_assigned {src : List (String × Option Callee)} {f : SourceFunction}
    {s s' : LoweringState} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (propagationNone : f.propagation = none)
    (assigned : f.result.isAssigned = true) (h : Lower.function f s = some ((), s')) :
    ∃ program row, f.program? src = some program ∧ program.callee?.isSome = (f.callable) ∧
      program.parameters = f.parameters.length ∧ s'.functions = s.functions ++ [row] ∧
      s'.callable = s.callable ++ [f.callable] ∧
      StateLE s s' ∧ BlocksIndexed s' ∧
      row.name = f.name ∧ row.paramsLen.val = f.parameters.length ∧
      ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId) (observations : List Observation),
        functionProgram? (F.toArtifact sourceId observations) (src.map (·.2)) row = some program := by
  obtain ⟨name, parameters, lets, propagation, result⟩ := f
  subst propagationNone
  cases result with
  | value _ => simp [SourceExpression.isAssigned] at assigned
  | call _ _ => simp [SourceExpression.isAssigned] at assigned
  | conditional _ _ _ _ _ => simp [SourceExpression.isAssigned] at assigned
  | comparison _ _ _ => simp [SourceExpression.isAssigned] at assigned
  | constructed _ _ => simp [SourceExpression.isAssigned] at assigned
  | assigned target assignedValue returned =>
  cases lets with
  | nil =>
    simp only [Lower.function, run_bind, run_lift, run_get, run_require] at h
    obtain ⟨scope, _, ⟨hscope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨hown, rfl⟩, _, _, ⟨notAssigned, _⟩,
      _⟩ := h
    simp [SourceExpression.isAssigned] at notAssigned
  | cons binding more =>
    obtain ⟨bindingName, value⟩ := binding
    cases more with
    | cons _ _ =>
      simp only [Lower.function, run_bind, run_lift, run_get] at h
      obtain ⟨scope, _, ⟨hscope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨hown, rfl⟩, tail⟩ := h
      exact absurd tail (by simp [Lower.fail])
    | nil =>
      cases parameters with
      | nil =>
        simp only [Lower.function, Lower.assignments, run_bind, run_lift, run_get, run_pure,
          Lower.word, run_require] at h
        obtain ⟨scope, _, ⟨hscope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨hown, rfl⟩, words, _, hlit,
          literalRow, _, hpush1, slotWord, _, ⟨hslot, rfl⟩, bindingRow, _, hpush2, assignedList, _,
          ⟨_, _, ⟨targetEq, rfl⟩, assignRow, sC, hassign, hlist, hsC⟩, tail⟩ := h
        subst hlist
        rw [hsC] at tail
        obtain ⟨ok, rfl, low, high, rfl, lowEq, highEq⟩ := literal_words_run hlit
        have targetIs : target = bindingName := by simpa using targetEq
        subst targetIs
        have scopeEq : scope = [(target, letSlot)] := by
          simpa [SourceFunction.scope?] using hscope.symm
        subst scopeEq
        obtain ⟨bound, hbound⟩ := validate_int_of_literal ok
        have slotEq : slotWord = ⟨letSlot, by decide⟩ := Fin.ext (decode_val hslot)
        subst slotEq
        have frame1 := push_row_frame hpush1
        have frame2 := push_row_frame hpush2
        have blocks1 := (push_row_blocks hpush1).1
        have blocks2 := (push_row_blocks hpush2).1
        have wf2 := BlocksIndexed.congr blocks2 (BlocksIndexed.congr blocks1 wf)
        obtain ⟨_, _, literalIndex, _⟩ := push_row_run hpush1
        obtain ⟨_, _, bindingIndex, _⟩ := push_row_run hpush2
        have rowsGrow := (push_row_blocks hpush1).2.2
        obtain ⟨slot, w, ae, valueRow, span, found, decoded, hsrcAssign, assignLe, assignWf, assignFa,
          assignFb, assignBound, valueBefore, assignReads⟩ :=
          assignment_lowers wf2 hassign
        have slotLet : slot = letSlot := (by simpa using found : letSlot = slot).symm
        subst slotLet
        have wIs : w = ⟨letSlot, by decide⟩ := Fin.ext (decode_val decoded)
        subst wIs
        have alignedAssign : Aligned sC src :=
          Aligned.congr (by rw [assignFa, frame2.1, frame1.1]) (by rw [assignFb, frame2.2, frame1.2])
            aligned
        obtain ⟨e, row, hsrc, fa, fb, le, wf', hname, hparams, reads⟩ :=
          function_tail_lowers (statements := [bindingRow, assignRow])
            (bindings := [⟨⟨letSlot, by decide⟩, bound⟩])
            (assignment := some ⟨⟨letSlot, by decide⟩, ae⟩) (propagation := none)
            assignWf alignedAssign
            (fun F leF returned operand e' at_ before bound' rd => by
              have toBinding := assignLe.trans leF
              have literalAt := (push_row_at hpush1 rfl).mono ((push_row_le hpush2).trans toBinding)
              have bindingAt := (push_row_at hpush2 rfl).mono toBinding
              have literalBefore : literalRow.val < bindingRow.val := by
                rw [bindingIndex]; omega
              have valid : validateInt? (signedOfWords low high) = some bound := by
                rw [signed_of_words_literal ok lowEq highEq]; exact hbound
              obtain ⟨assignAt, valueReads⟩ := assignReads (src.map (·.2)) F leF
              have assignBoundF : assignRow.val < F.rows.length := by
                obtain ⟨row', located, _⟩ := assignAt
                exact (List.getElem?_eq_some_iff.mp located).1
              simpa using body_let_assign_return bindingAt literalBefore literalAt valid assignAt rfl
                valueBefore assignBoundF valueReads at_ before bound' rd)
            tail
        refine ⟨⟨0, [⟨⟨letSlot, by decide⟩, bound⟩], some ⟨⟨letSlot, by decide⟩, ae⟩, none, e⟩,
          row, ?_, by simp [FunctionProgram.callee?, SourceFunction.callable, assigned], rfl, ?_, ?_, ?_, wf', hname, hparams, reads⟩
        · simp [SourceFunction.program?, SourceFunction.assignment?, hscope, hbound, hsrcAssign, hsrc,
            hslot]
        · rw [fa, assignFa, frame2.1, frame1.1]
        · rw [fb, assignFb, frame2.2, frame1.2]
        · exact ((push_row_le hpush1).trans ((push_row_le hpush2).trans assignLe)).trans le
      | cons p ps =>
        cases ps with
        | nil =>
          simp [Lower.function, SourceFunction.scope?, SourceExpression.isAssigned, assigned,
            run_bind, Lower.lift, Lower.fail] at h
        | cons q qs =>
          simp [Lower.function, SourceFunction.scope?, run_bind, Lower.fail] at h

theorem function_lowers_propagated {src : List (String × Option Callee)} {f : SourceFunction}
    {s s' : LoweringState} {propagation : SourcePropagation}
    (wf : BlocksIndexed s) (aligned : Aligned s src)
    (propagated : f.propagation = some propagation)
    (h : Lower.function f s = some ((), s')) :
    ∃ program row, f.program? src = some program ∧
      program.callee?.isSome = (f.callable) ∧
      program.parameters = f.parameters.length ∧ s'.functions = s.functions ++ [row] ∧
      s'.callable = s.callable ++ [f.callable] ∧
      StateLE s s' ∧ BlocksIndexed s' ∧
      row.name = f.name ∧ row.paramsLen.val = f.parameters.length ∧
      ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId) (observations : List Observation),
        functionProgram? (F.toArtifact sourceId observations) (src.map (·.2)) row = some program := by
  obtain ⟨name, parameters, lets, propagationOption, result⟩ := f
  dsimp only at propagated
  rw [propagated] at h ⊢
  simp only [Lower.function, run_bind, run_lift, run_get, run_pure, run_require] at h
  obtain ⟨scope, _, ⟨hscope, rfl⟩, _, _, ⟨rfl, rfl⟩, owner, _, ⟨hown, rfl⟩, _, _,
    ⟨condition, rfl⟩, statement, s2, hprop, tail⟩ := h
  have conditionParts : lets.isEmpty = true ∧ result.isAssigned = false := by
    rw [Bool.and_eq_true] at condition
    exact ⟨condition.1, by simpa using condition.2⟩
  have letsEmpty : lets = [] := List.isEmpty_iff.mp conditionParts.1
  subst letsEmpty
  obtain ⟨slot, word, callee, args, tryRow, hslot, hword, lookedUp, countFits, foundArguments,
    propLe, propWf, propFa, propFb, propBefore, propReads⟩ :=
    propagation_lowers wf aligned hprop
  have alignedProp : Aligned s2 src := Aligned.congr propFa propFb aligned
  obtain ⟨e, row, hsrc, fa, fb, le, wf', hname, hparams, reads⟩ :=
    function_tail_lowers (statements := [statement]) (bindings := []) (assignment := none)
      (propagation := some ⟨word, parameterNames callee.parameters, callee.body, args⟩)
      propWf alignedProp
      (fun F leF returned operand e' at_ before bound rd => by
        obtain ⟨bindingAt, propRead⟩ := propReads F leF
        exact body_propagation_return bindingAt propRead
          (return_expression_of at_ before bound rd))
      tail
  have assignmentNone :
      (⟨name, parameters, [], some propagation, result⟩ : SourceFunction).assignment?
        scope = some none :=
    assignment_plain (f := ⟨name, parameters, [], some propagation, result⟩) conditionParts.2
  have propagationRead : propagationProgram? src scope propagation =
      some ⟨word, parameterNames callee.parameters, callee.body, args⟩ := by
    simp [propagationProgram?, lookedUp, countFits, hslot, hword, foundArguments, beq_iff_eq]
  have programProof : SourceFunction.program?
      src ⟨name, parameters, [], some propagation, result⟩ =
      some ⟨parameters.length, [], none,
        some ⟨word, parameterNames callee.parameters, callee.body, args⟩, e⟩ := by
    simp [SourceFunction.program?, hscope, propagationRead, assignmentNone, hsrc]
  refine ⟨⟨parameters.length, [], none,
      some ⟨word, parameterNames callee.parameters, callee.body, args⟩, e⟩, row, programProof,
    ?_, rfl, ?_, ?_, propLe.trans le, wf', hname, hparams, reads⟩
  · simp [FunctionProgram.callee?, SourceFunction.callable, propagated]
  · rw [fa, propFa]
  · rw [fb, propFb]

theorem function_lowers {src : List (String × Option Callee)} {f : SourceFunction}
    {s s' : LoweringState} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (h : Lower.function f s = some ((), s')) :
    ∃ program row, f.program? src = some program ∧
      program.callee?.isSome = (f.callable) ∧
      program.parameters = f.parameters.length ∧ s'.functions = s.functions ++ [row] ∧
      s'.callable = s.callable ++ [f.callable] ∧
      StateLE s s' ∧ BlocksIndexed s' ∧
      row.name = f.name ∧ row.paramsLen.val = f.parameters.length ∧
      ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId) (observations : List Observation),
        functionProgram? (F.toArtifact sourceId observations) (src.map (·.2)) row = some program := by
  cases propagationCase : f.propagation with
  | some propagation =>
      simpa [Option.isNone, propagationCase] using
        function_lowers_propagated wf aligned propagationCase h
  | none =>
      cases hassigned : f.result.isAssigned with
      | false =>
          simpa [Option.isNone, propagationCase] using
            function_lowers_plain wf aligned propagationCase hassigned h
      | true =>
          simpa [Option.isNone, propagationCase] using
            function_lowers_assigned wf aligned propagationCase hassigned h


theorem callee_parameters {program : FunctionProgram} {callee : Callee}
    (found : program.callee? = some callee) : callee.parameters = program.parameters := by
  unfold FunctionProgram.callee? at found
  split at found
  · simp only [Option.some.injEq] at found
    subst found; rfl
  · simp at found

theorem lower_all :
    ∀ (functions : List SourceFunction) {s s' : LoweringState}
      {src : List (String × Option Callee)} {programs : List (Option FunctionProgram)},
      BlocksIndexed s → Aligned s src →
      src.map (·.2) = programs.map (fun program => program.bind (·.callee?)) →
      (functions.forM Lower.function) s = some ((), s') →
      ∃ rows, s'.functions = s.functions ++ rows ∧ StateLE s s' ∧ BlocksIndexed s' ∧
        ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId)
          (observations : List Observation),
          rows.foldl (programStep (F.toArtifact sourceId observations)) programs =
            (functions.foldl sourceStep (src, programs)).2 := by
  intro functions
  induction functions with
  | nil =>
    intro s s' src programs wf aligned link h
    unfold List.forM at h
    simp only [run_pure] at h
    obtain ⟨_, rfl⟩ := h
    exact ⟨[], by simp, StateLE.refl _, wf, fun _ _ _ _ => rfl⟩
  | cons f rest ih =>
    intro s s' src programs wf aligned link h
    unfold List.forM at h
    simp only [run_bind] at h
    obtain ⟨_, s1, hf, hrest⟩ := h
    obtain ⟨program, row, hprogram, hbindings, hparameters, fa, fb, le1, wf1, hname, hwidth,
      reads⟩ := function_lowers wf aligned hf
    have aligned1 : Aligned s1 (src ++ [(f.name, program.callee?)]) := by
      obtain ⟨names, flags, widths⟩ := aligned
      have sameLength : s.functions.length = src.length := by
        simpa using congrArg List.length names
      refine ⟨?_, ?_, ?_⟩
      · rw [fa]; simp [names, hname]
      · rw [fb]; simp [flags, hbindings]
      · intro index found callee located callee_at
        rw [fa] at located
        by_cases old : index < s.functions.length
        · rw [List.getElem?_append_left old] at located
          rw [List.getElem?_append_left (by omega)] at callee_at
          exact widths index found callee located callee_at
        · have atEnd : index = s.functions.length := by
            have := (List.getElem?_eq_some_iff.mp located).1
            simp at this
            omega
          subst atEnd
          rw [List.getElem?_append_right (by omega)] at located callee_at
          simp only [Nat.sub_self, List.getElem?_cons_zero, Option.some.injEq] at located
          simp only [sameLength, Nat.sub_self, List.getElem?_cons_zero, Option.bind_some] at callee_at
          subst located
          rw [hwidth, callee_parameters callee_at, hparameters]
    have link1 : (src ++ [(f.name, program.callee?)]).map (·.2) =
        (programs ++ [some program]).map (fun program => program.bind (·.callee?)) := by
      simp [link]
    obtain ⟨rows, ra, le2, wf2, folded⟩ := ih wf1 aligned1 link1 hrest
    refine ⟨row :: rows, by rw [ra, fa]; simp, le1.trans le2, wf2, ?_⟩
    intro F leF sourceId observations
    have step := reads F (le2.trans leF) sourceId observations
    have e1 : programStep (F.toArtifact sourceId observations) programs row =
        programs ++ [some program] := by
      unfold programStep
      rw [← link, step]
    have e2 : sourceStep (src, programs) f =
        (src ++ [(f.name, program.callee?)], programs ++ [some program]) := by
      unfold sourceStep
      simp only [hprogram]
      rfl
    rw [List.foldl_cons, List.foldl_cons, e1, e2]
    exact folded F leF sourceId observations


theorem lower_functions_run {spans : List Word32} {functions : List SourceFunction}
    {state : LoweringState} (h : lowerFunctions spans functions = some state) :
    (functions.forM Lower.function) { spans } = some ((), state) := by
  unfold lowerFunctions at h
  simp only [Option.bind_eq_bind, check_bind] at h
  split at h
  · generalize hrun : StateT.run (forM functions Lower.function) { spans := spans } = result at h
    cases result with
    | none => simp at h
    | some pair =>
      obtain ⟨u, ran⟩ := pair
      change (if ran.spans.isEmpty = true then some ran else none) = some state at h
      split at h
      · simp only [Option.some.injEq] at h
        subst h
        cases u
        exact hrun
      · simp at h
  · simp at h


/-- Lowering is correct for the admitted shape: whenever the Lean lowerer succeeds on parsed
source functions, the base-machine program the raw-IR rows it emits denote, function by function,
is the program the source tree denotes. The statement is universal over the parsed functions and
the span words; it concerns the Lean lowerer, not the Rust compiler. -/
theorem lowering_means_the_source {spans : List Word32} {functions : List SourceFunction}
    {state : LoweringState} (lowered : lowerFunctions spans functions = some state)
    (sourceId : BridgeId) (observations : List Observation) :
    functionPrograms (state.toArtifact sourceId observations) = sourcePrograms functions := by
  have run := lower_functions_run lowered
  obtain ⟨rows, rowsEq, _, _, folded⟩ := lower_all functions (s := { spans := spans })
    (src := []) (programs := []) (fun index block located => by simp at located)
    ⟨rfl, rfl, fun index row callee located => by simp at located⟩ rfl run
  have := folded state (StateLE.refl _) sourceId observations
  unfold functionPrograms sourcePrograms
  show state.functions.foldl (programStep (state.toArtifact sourceId observations)) [] = _
  rw [rowsEq]
  simpa using this

theorem lowers_to_means_the_source {artifact : RawArtifact} {functions : List SourceFunction}
    (lowers : lowersTo artifact functions = true) :
    functionPrograms artifact = sourcePrograms functions := by
  unfold lowersTo at lowers
  cases lowered : lowerFunctions (spanWords artifact) functions with
  | none => simp [lowered] at lowers
  | some state =>
    simp only [lowered, beq_iff_eq] at lowers
    rw [← lowers]
    exact lowering_means_the_source lowered _ _


/-- A certified pair means the same under its source tree and its IR: the source text parses to
functions, the artifact is the lowerer's output on them, and their base-machine programs are the
programs of the artifact's functions, function by function. -/
theorem certified_pair_means_the_same {source : SourceDocument} {raw : RawArtifact}
    {results : List ObservedResult} (certified : certifyDecoded source raw = some results) :
    ∃ functions, parseSource source.text = some functions ∧
      functionPrograms raw = sourcePrograms functions ∧ validateArtifact raw = true ∧
      namedResults raw = some results := by
  obtain ⟨functions, _, parsed, lowers, valid, named⟩ := certify_decoded_sound certified
  exact ⟨functions, parsed, lowers_to_means_the_source lowers, valid, named⟩

end Xsh.Conformance.Vertical
