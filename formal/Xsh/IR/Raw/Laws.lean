import Xsh.IR.Raw.Decode

namespace Xsh.IR.Raw

theorem pool_id_decode_encode {kind : PoolKind} (id : PoolId kind) :
    PoolId.decode? kind (PoolId.encode id) = some id :=
  PoolId.decode_encode id

theorem range_resolution_sound (range : RawRange) (total start finish : Nat)
    (resolved : range.resolve? total = some (start, finish)) :
    start = range.startValue ∧ finish = range.startValue + range.length ∧ finish ≤ total :=
  range.resolve_sound total start finish resolved

theorem range_resolution_complete (range : RawRange) (total : Nat)
    (within : range.startValue + range.length ≤ total) :
    range.resolve? total = some (range.startValue, range.startValue + range.length) :=
  range.resolve_complete total within

theorem range_resolution_bounds (range : RawRange) (total start finish : Nat)
    (resolved : range.resolve? total = some (start, finish)) :
    start ≤ finish ∧ finish ≤ total :=
  range.resolve_bounds total start finish resolved

theorem decoded_row_preserves_raw_fields (tables : Tables) (index : DirectIndex)
    (row : RawRow) (decoded : Decode.readRow? tables index = some row) :
    tables.tags[index.value]? = some row.tag ∧
      tables.data[index.value]? = some row.data := by
  cases row with
  | mk tag data =>
      cases tagRead : tables.tags[index.value]? with
      | none => simp [Decode.readRow?, tagRead] at decoded
      | some readTag =>
          cases dataRead : tables.data[index.value]? with
          | none => simp [Decode.readRow?, tagRead, dataRead] at decoded
          | some readData =>
              have fields : readTag = tag ∧ readData = data := by
                simpa [Decode.readRow?, tagRead, dataRead] using decoded
              rcases fields with ⟨rfl, rfl⟩
              simp

end Xsh.IR.Raw
