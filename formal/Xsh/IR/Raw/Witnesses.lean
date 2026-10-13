import Xsh.IR.Raw.Laws

namespace Xsh.IR.Raw

open Xsh.Foundation

def zeroWord : Word32 := ⟨0, by decide⟩
def sampleWord (value : Nat) (fits : value < wordModulus) : Word32 := ⟨value, fits⟩

def firstStringId : StringId :=
  ⟨sampleWord 1 (by decide), by decide, by decide⟩

theorem pool_id_one_is_accepted :
    PoolId.decode? .strings (PoolId.encode firstStringId) = some firstStringId :=
  PoolId.decode_encode firstStringId

theorem pool_id_zero_is_rejected :
    PoolId.decode? .strings zeroWord = none := by
  simpa [zeroWord] using PoolId.decode_rejects_zero .strings

theorem pool_id_ir_none_is_rejected :
    PoolId.decode? .strings IR_NONE = none := by
  exact PoolId.decode_rejects_ir_none .strings

def emptyTables : Tables := ⟨#[], #[], #[]⟩

theorem direct_zero_in_empty_table_is_out_of_bounds :
    Decode.readRow? emptyTables DirectIndex.zero = none := by
  decide

theorem optional_none_uses_ir_none : OptionalIndex.encode OptionalIndex.absent = IR_NONE :=
  OptionalIndex.encode_none

def presentOptionalZero : OptionalIndex :=
  ⟨some ⟨zeroWord, by decide⟩⟩

theorem optional_zero_is_distinct_from_none :
    OptionalIndex.decode zeroWord = presentOptionalZero ∧
      OptionalIndex.encode presentOptionalZero = zeroWord := by
  constructor
  · rfl
  · rfl

def endEmptyRange : RawRange := ⟨DirectIndex.zero, zeroWord⟩

theorem empty_range_at_end_is_accepted :
    endEmptyRange.resolve? 0 = some (0, 0) := by
  simp [endEmptyRange, RawRange.resolve?, RawRange.startValue,
    RawRange.length, DirectIndex.zero, DirectIndex.value, zeroWord]

def outOfBoundsRange : RawRange :=
  ⟨DirectIndex.decode (sampleWord 2 (by decide)), sampleWord 1 (by decide)⟩

theorem range_past_table_end_is_rejected :
    outOfBoundsRange.resolve? 2 = none := by
  simp [outOfBoundsRange, RawRange.resolve?, RawRange.startValue,
    RawRange.length, DirectIndex.decode, DirectIndex.value, sampleWord]

def mismatchedTables : Tables :=
  ⟨#[sampleWord 7 (by decide)], #[], #[]⟩

theorem tag_data_length_mismatch_is_preserved :
    mismatchedTables.tags.size = 1 ∧ mismatchedTables.data.size = 0 ∧
      ¬ mismatchedTables.aligned := by
  simp [mismatchedTables, Tables.aligned]

theorem tag_data_mismatch_row_read_fails :
    Decode.readRow? mismatchedTables DirectIndex.zero = none := by
  simp [Decode.readRow?, mismatchedTables, DirectIndex.zero, DirectIndex.value]

def unknownTag : Word32 := sampleWord 429 (by decide)
def unknownTagTables : Tables :=
  ⟨#[unknownTag], #[⟨zeroWord, zeroWord⟩], #[]⟩

theorem unknown_raw_tag_is_preserved :
    Decode.readRow? unknownTagTables DirectIndex.zero =
      some ⟨unknownTag, ⟨zeroWord, zeroWord⟩⟩ := by
  simp [Decode.readRow?, unknownTagTables, DirectIndex.zero,
    DirectIndex.value, unknownTag, zeroWord, sampleWord]

end Xsh.IR.Raw
