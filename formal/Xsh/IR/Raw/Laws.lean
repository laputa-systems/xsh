import Xsh.IR.Raw.Decode

namespace Xsh.IR.Raw

open Xsh.Foundation

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

/-- A successful sequential read emits one word for every requested position. -/
theorem readWords_success_length (words : Array Word32) (start count : Nat)
    (decoded : List Word32)
    (success : Decode.readWords? words start count = some decoded) :
    decoded.length = count := by
  induction count generalizing start decoded with
  | zero =>
      simp [Decode.readWords?] at success
      subst decoded
      rfl
  | succ count ih =>
      cases first : words[start]? with
      | none =>
          simp [Decode.readWords?, first] at success
      | some word =>
          cases restRead : Decode.readWords? words (start + 1) count with
          | none =>
              simp [Decode.readWords?, first, restRead] at success
          | some rest =>
              have decodedEq : word :: rest = decoded := by
                simpa [Decode.readWords?, first, restRead] using success
              have restLength : rest.length = count :=
                ih (start := start + 1) (decoded := rest) restRead
              rw [← decodedEq]
              simp [restLength]

/-- A successful payload read has its declared length and stays in the extra table. -/
theorem readPayload_success_length_and_bounds (tables : Tables)
    (range : RawRange) (decoded : List Word32)
    (success : Decode.readPayload? tables range = some decoded) :
    decoded.length = range.length ∧
      range.startValue + decoded.length ≤ tables.extra.size := by
  unfold Decode.readPayload? at success
  cases resolved : range.resolve? tables.extra.size with
  | none =>
      simp [resolved] at success
  | some bounds =>
      rcases bounds with ⟨start, finish⟩
      have wordsRead : Decode.readWords? tables.extra start (finish - start) =
          some decoded := by
        simpa [resolved] using success
      have readLength :=
        readWords_success_length tables.extra start (finish - start) decoded wordsRead
      obtain ⟨startEq, finishEq, finishBound⟩ :=
        range.resolve_sound tables.extra.size start finish resolved
      constructor
      · rw [readLength, finishEq, startEq]
        omega
      · rw [readLength, finishEq]
        omega

def outOfBoundsPayloadRange : RawRange :=
  ⟨DirectIndex.decode ⟨2, by decide⟩, ⟨1, by decide⟩⟩

def payloadWitnessTables : Tables :=
  ⟨#[], #[], #[⟨0, by decide⟩, ⟨0, by decide⟩]⟩

/-- This concrete range witness extends past the payload table and is rejected. -/
theorem out_of_bounds_payload_read_is_rejected :
    Decode.readPayload? payloadWitnessTables outOfBoundsPayloadRange = none := by
  simp [Decode.readPayload?, payloadWitnessTables, outOfBoundsPayloadRange,
    RawRange.resolve?, RawRange.startValue, RawRange.length,
    DirectIndex.decode, DirectIndex.value]

end Xsh.IR.Raw
