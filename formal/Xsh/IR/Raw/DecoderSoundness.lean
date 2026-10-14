import Xsh.IR.Raw.Laws

namespace Xsh.IR.Raw

open Xsh.Foundation

namespace DecoderSoundness

/- This finite model is independent of the Rust decoder and makes no runtime correspondence claim. -/

/-- The finite decoder schema used here has four raw tag ordinals. -/
inductive Tag where
  | immediate
  | reference
  | singleton
  | words
  deriving DecidableEq, Repr

/-- Unknown ordinals remain malformed instead of being mapped to a default tag. -/
def decodeTag? (word : Word32) : Option Tag :=
  match word.val with
  | 0 => some .immediate
  | 1 => some .reference
  | 2 => some .singleton
  | 3 => some .words
  | _ => none

inductive Row where
  | immediate (value : Word32)
  | reference (index : DirectIndex)
  | singleton (value : Word32)
  | words (values : List Word32)
  deriving DecidableEq, Repr

private def zeroWord : Word32 := ⟨0, by decide⟩
private def oneWord : Word32 := ⟨1, by decide⟩
private def immediateTag : Word32 := ⟨0, by decide⟩
private def referenceTag : Word32 := ⟨1, by decide⟩
private def singletonTag : Word32 := ⟨2, by decide⟩
private def wordsTag : Word32 := ⟨3, by decide⟩

/-- Decode only four rows: immediate values, bounded references, one payload word, or a word range. -/
def decodeRowData? (tables : Tables) (referenceCount : Nat) (row : RawRow) : Option Row := do
  match decodeTag? row.tag with
  | none => none
  | some .immediate =>
      if row.data.rhs.val = 0 then some (.immediate row.data.lhs) else none
  | some .reference =>
      if row.data.rhs.val = 0 ∧ row.data.lhs.val < referenceCount then
        some (.reference (DirectIndex.decode row.data.lhs))
      else
        none
  | some .singleton =>
      if row.data.rhs.val = 1 then
        let payload ← Decode.readPayload? tables row.data.range
        match payload with
        | [value] => some (.singleton value)
        | _ => none
      else
        none
  | some .words =>
      let payload ← Decode.readPayload? tables row.data.range
      some (.words payload)

/-- Reject unaligned parallel tables before a row is interpreted. -/
def decodeRow? (tables : Tables) (index : DirectIndex) (referenceCount : Nat) : Option Row := do
  if tables.tags.size = tables.data.size then
    let row ← Decode.readRow? tables index
    decodeRowData? tables referenceCount row
  else
    none

/-- The raw fields and auxiliary range have the schema required by the decoded row. -/
def RespectsSchema (tables : Tables) (referenceCount : Nat) (raw : RawRow) (decoded : Row) : Prop :=
  match decoded with
  | .immediate value =>
      raw.tag = immediateTag ∧ raw.data.lhs = value ∧ raw.data.rhs = zeroWord
  | .reference index =>
      raw.tag = referenceTag ∧ raw.data.lhs = index.word ∧
        raw.data.rhs = zeroWord ∧ index.value < referenceCount
  | .singleton value =>
      raw.tag = singletonTag ∧ raw.data.rhs = oneWord ∧
        Decode.readPayload? tables raw.data.range = some [value]
  | .words values =>
      raw.tag = wordsTag ∧ Decode.readPayload? tables raw.data.range = some values

/-- Successful rows in this finite schema preserve the tag and payload conditions they decode. -/
theorem decode_success_respects_tag_payload_schema (tables : Tables) (index : DirectIndex)
    (referenceCount : Nat) (decoded : Row)
    (success : decodeRow? tables index referenceCount = some decoded) :
    tables.aligned ∧ ∃ raw, Decode.readRow? tables index = some raw ∧
      RespectsSchema tables referenceCount raw decoded := by
  by_cases aligned : tables.tags.size = tables.data.size
  · have dataSuccess : (Decode.readRow? tables index).bind
        (decodeRowData? tables referenceCount) = some decoded := by
      simpa [decodeRow?, aligned] using success
    cases rowRead : Decode.readRow? tables index with
    | none => simp [rowRead] at dataSuccess
    | some raw =>
        have rawSuccess : decodeRowData? tables referenceCount raw = some decoded := by
          simpa [rowRead] using dataSuccess
        refine ⟨by simpa [Tables.aligned] using aligned, raw, rfl, ?_⟩
        unfold RespectsSchema
        unfold decodeRowData? at rawSuccess
        cases tagValue : raw.tag.val with
        | zero =>
            simp [decodeTag?, tagValue] at rawSuccess
            rcases rawSuccess with ⟨rhsZero, decodedEq⟩
            subst decoded
            have tagEq : raw.tag = immediateTag := by
              apply Fin.ext
              exact tagValue
            have rhsEq : raw.data.rhs = zeroWord := by
              simpa [zeroWord] using rhsZero
            exact ⟨tagEq, rfl, rhsEq⟩
        | succ ordinal =>
            cases ordinal with
            | zero =>
                simp [decodeTag?, tagValue] at rawSuccess
                rcases rawSuccess with ⟨guards, decodedEq⟩
                rcases guards with ⟨rhsZero, inBounds⟩
                subst decoded
                have tagEq : raw.tag = referenceTag := by
                  apply Fin.ext
                  exact tagValue
                have rhsEq : raw.data.rhs = zeroWord := by
                  simpa [zeroWord] using rhsZero
                exact ⟨tagEq, rfl, rhsEq, by
                  simpa [DirectIndex.value, DirectIndex.decode] using inBounds⟩
            | succ ordinal =>
                cases ordinal with
                | zero =>
                    simp [decodeTag?, tagValue] at rawSuccess
                    by_cases rhsOne : raw.data.rhs.val = 1
                    · cases payloadRead : Decode.readPayload? tables raw.data.range with
                      | none => simp [rhsOne, payloadRead] at rawSuccess
                      | some payload =>
                          cases payload with
                          | nil => simp [rhsOne, payloadRead] at rawSuccess
                          | cons value rest =>
                              cases rest with
                              | nil =>
                                  simp [rhsOne, payloadRead] at rawSuccess
                                  have decodedEq : Row.singleton value = decoded := by
                                    simpa [rhsOne, payloadRead] using rawSuccess
                                  subst decoded
                                  have tagEq : raw.tag = singletonTag := by
                                    apply Fin.ext
                                    simpa [singletonTag] using tagValue
                                  have rhsEq : raw.data.rhs = oneWord := by
                                    apply Fin.ext
                                    exact rhsOne
                                  exact ⟨tagEq, rhsEq, rfl⟩
                              | cons next tail => simp [rhsOne, payloadRead] at rawSuccess
                    · simp [rhsOne] at rawSuccess
                | succ ordinal =>
                    cases ordinal with
                    | zero =>
                        simp [decodeTag?, tagValue] at rawSuccess
                        cases payloadRead : Decode.readPayload? tables raw.data.range with
                        | none => simp [payloadRead] at rawSuccess
                        | some payload =>
                            simp [payloadRead] at rawSuccess
                            have decodedEq : Row.words payload = decoded := by
                              simpa [payloadRead] using rawSuccess
                            subst decoded
                            have tagEq : raw.tag = wordsTag := by
                              apply Fin.ext
                              exact tagValue
                            exact ⟨tagEq, rfl⟩
                    | succ larger => simp [decodeTag?, tagValue] at rawSuccess
  · simp [decodeRow?, aligned] at success

/-- A successful words row uses the exact payload range it declares, within the auxiliary table. -/
theorem successful_words_row_payload_has_length_and_bounds (tables : Tables)
    (index : DirectIndex) (referenceCount : Nat) (payload : List Word32)
    (success : decodeRow? tables index referenceCount = some (.words payload)) :
    ∃ raw, Decode.readRow? tables index = some raw ∧
      payload.length = raw.data.range.length ∧
        raw.data.range.startValue + payload.length ≤ tables.extra.size := by
  obtain ⟨_, raw, rawRead, schema⟩ :=
    decode_success_respects_tag_payload_schema tables index referenceCount (.words payload) success
  rcases schema with ⟨_, payloadRead⟩
  have bounds := readPayload_success_length_and_bounds tables raw.data.range payload payloadRead
  refine ⟨raw, rawRead, ?_, ?_⟩
  · simpa [RawData.range, RawRange.length] using bounds.1
  · simpa [RawData.range, RawRange.startValue, RawRange.length, DirectIndex.value,
      DirectIndex.decode] using bounds.2

/-- Any mismatch between the parallel row-table lengths makes decoding fail. -/
theorem unaligned_tables_are_rejected (tables : Tables) (index : DirectIndex)
    (referenceCount : Nat) (misaligned : tables.tags.size ≠ tables.data.size) :
    decodeRow? tables index referenceCount = none := by
  simp [decodeRow?, misaligned]

private def rowIndexOutOfBoundsTables : Tables :=
  ⟨#[immediateTag], #[⟨zeroWord, zeroWord⟩], #[]⟩
private def outOfBoundsStartTables : Tables :=
  ⟨#[singletonTag], #[⟨⟨2, by decide⟩, oneWord⟩], #[zeroWord, zeroWord]⟩
private def overrunTables : Tables :=
  ⟨#[wordsTag], #[⟨oneWord, ⟨2, by decide⟩⟩], #[zeroWord, zeroWord]⟩
private def unknownTagTables : Tables :=
  ⟨#[⟨4, by decide⟩], #[⟨zeroWord, zeroWord⟩], #[]⟩
private def singletonLengthMismatchTables : Tables :=
  ⟨#[singletonTag], #[⟨zeroWord, zeroWord⟩], #[zeroWord]⟩
private def immediatePayloadMismatchTables : Tables :=
  ⟨#[immediateTag], #[⟨oneWord, oneWord⟩], #[]⟩
private def invalidReferenceTables : Tables :=
  ⟨#[referenceTag], #[⟨oneWord, zeroWord⟩], #[]⟩
private def referenceMetadataMismatchTables : Tables :=
  ⟨#[referenceTag], #[⟨oneWord, oneWord⟩], #[]⟩
private def validSingletonTables : Tables :=
  ⟨#[singletonTag], #[⟨zeroWord, oneWord⟩], #[oneWord]⟩

theorem aligned_row_index_and_reference_metadata_errors_are_rejected :
    decodeRow? rowIndexOutOfBoundsTables (DirectIndex.decode oneWord) 0 = none ∧
      decodeRow? referenceMetadataMismatchTables DirectIndex.zero 2 = none := by
  decide

theorem out_of_range_reference_is_rejected :
    decodeRow? invalidReferenceTables DirectIndex.zero 0 = none := by
  decide

theorem out_of_bounds_payload_start_is_rejected :
    decodeRow? outOfBoundsStartTables DirectIndex.zero 0 = none := by
  decide

theorem payload_range_past_table_end_is_rejected :
    decodeRow? overrunTables DirectIndex.zero 0 = none := by
  decide

theorem unknown_tag_is_rejected :
    decodeRow? unknownTagTables DirectIndex.zero 0 = none := by
  decide

theorem singleton_tag_with_wrong_payload_length_is_rejected :
    decodeRow? singletonLengthMismatchTables DirectIndex.zero 0 = none := by
  decide

theorem immediate_tag_with_nonzero_payload_length_is_rejected :
    decodeRow? immediatePayloadMismatchTables DirectIndex.zero 0 = none := by
  decide

theorem valid_singleton_payload_is_accepted :
    decodeRow? validSingletonTables DirectIndex.zero 0 = some (.singleton oneWord) := by
  decide

/-- This local mutation drops the immediate row's required zero payload length. -/
private def mutatedDecodeRow? (tables : Tables) (index : DirectIndex)
    (referenceCount : Nat) : Option Row := do
  if tables.tags.size = tables.data.size then
    let raw ← Decode.readRow? tables index
    if raw.tag = immediateTag then
      some (.immediate raw.data.lhs)
    else
      decodeRowData? tables referenceCount raw
  else
    none

theorem immediate_payload_guard_mutation_is_detected :
    decodeRow? immediatePayloadMismatchTables DirectIndex.zero 0 = none ∧
      mutatedDecodeRow? immediatePayloadMismatchTables DirectIndex.zero 0 =
        some (.immediate oneWord) := by
  decide

end DecoderSoundness

end Xsh.IR.Raw
