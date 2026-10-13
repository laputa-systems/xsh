import Xsh.Foundation

namespace Xsh.Conformance.Transport

abbrev Word64 := Fin 18446744073709551616

abbrev WireByte := Fin 256

private def hexDigit (digit : Nat) : Char :=
  Char.ofNat (if digit < 10 then 48 + digit else 87 + digit)

private def hexDigit? (character : Char) : Option (Fin 16) :=
  let code := character.toNat
  if decimal : 48 ≤ code ∧ code ≤ 57 then
    some ⟨code - 48, by omega⟩
  else if lowercase : 97 ≤ code ∧ code ≤ 102 then
    some ⟨code - 87, by omega⟩
  else
    none

private def fixedHexCharacters (width value : Nat) : List Char :=
  (List.range width).reverse.map fun position =>
    hexDigit (value / 16 ^ position % 16)

private def decodeHexAcc (accumulator : Nat) : List Char → Option Nat
  | [] => some accumulator
  | character :: rest => do
      let digit ← hexDigit? character
      decodeHexAcc (accumulator * 16 + digit.val) rest

/-- The private bridge writes every stored u32 word and table ID as eight
lowercase hexadecimal digits, preserving leading zeroes. -/
def encodeHex32 (value : Xsh.Foundation.Word32) : String :=
  String.ofList (fixedHexCharacters 8 value.val)

/-- Decode exactly eight lowercase hexadecimal digits without truncation. -/
def decodeHex32? (text : String) : Option Xsh.Foundation.Word32 := do
  if text.toList.length != 8 then none else pure ()
  let value ← decodeHexAcc 0 text.toList
  Xsh.Foundation.Word32.decode? value

/-- The private bridge writes every u64 bit pattern as sixteen lowercase
hexadecimal digits. -/
def encodeHex64 (value : Word64) : String :=
  String.ofList (fixedHexCharacters 16 value.val)

/-- Decode exactly sixteen lowercase hexadecimal digits to a u64 bit pattern. -/
def decodeHex64? (text : String) : Option Word64 := do
  if text.toList.length != 16 then none else pure ()
  let value ← decodeHexAcc 0 text.toList
  if fits : value < 18446744073709551616 then
    some ⟨value, fits⟩
  else
    none

private def encodeByte (value : WireByte) : List Char :=
  [hexDigit (value.val / 16), hexDigit (value.val % 16)]

private def encodeByteList : List WireByte → List Char
  | [] => []
  | byte :: rest => encodeByte byte ++ encodeByteList rest

/-- Native path bytes and byte payloads use an even number of lowercase hex
digits, with each byte emitted high nibble first. -/
def encodeHexBytes (bytes : List WireByte) : String :=
  String.ofList (encodeByteList bytes)

private def decodeByteList : List Char → Option (List WireByte)
  | [] => some []
  | high :: low :: rest => do
      let highDigit ← hexDigit? high
      let lowDigit ← hexDigit? low
      let tail ← decodeByteList rest
      some (⟨highDigit.val * 16 + lowDigit.val, by
        have highBound := highDigit.isLt
        have lowBound := lowDigit.isLt
        omega⟩ :: tail)
  | [_] => none

/-- Decode an even-length lowercase hexadecimal byte string. -/
def decodeHexBytes? (text : String) : Option (List WireByte) :=
  decodeByteList text.toList

/-- The bridge reserves raw zero and `IR_NONE` and numbers present table
entries from one. -/
structure BridgeId where
  raw : Xsh.Foundation.Word32
  nonzero : raw.val ≠ 0
  notNone : raw.val ≠ 4294967295
  deriving DecidableEq

def BridgeId.fromIndex? (index : Nat) : Option BridgeId :=
  if fits : index + 1 < 4294967295 then
    have nonzero : index + 1 ≠ 0 := by omega
    have notNone : index + 1 ≠ 4294967295 := by omega
    some ⟨⟨index + 1, by omega⟩, nonzero, notNone⟩
  else
    none

def BridgeId.fromRaw? (raw : Xsh.Foundation.Word32) : Option BridgeId :=
  if valid : raw.val ≠ 0 ∧ raw.val ≠ 4294967295 then
    some ⟨raw, valid.1, valid.2⟩
  else
    none

def BridgeId.toHex (id : BridgeId) : String := encodeHex32 id.raw

def BridgeId.fromHex? (text : String) : Option BridgeId :=
  (decodeHex32? text).bind BridgeId.fromRaw?

structure OptionalBridgeId where
  value : Option BridgeId
  deriving DecidableEq

def OptionalBridgeId.fromRaw? (raw : Xsh.Foundation.Word32) : Option OptionalBridgeId :=
  if raw.val = 4294967295 then
    some ⟨none⟩
  else
    (BridgeId.fromRaw? raw).map fun id => ⟨some id⟩

def OptionalBridgeId.raw (id : OptionalBridgeId) : Xsh.Foundation.Word32 :=
  match id.value with
  | none => ⟨4294967295, by omega⟩
  | some present => present.raw

def OptionalBridgeId.toHex (id : OptionalBridgeId) : String := encodeHex32 id.raw

def OptionalBridgeId.fromHex? (text : String) : Option OptionalBridgeId :=
  (decodeHex32? text).bind OptionalBridgeId.fromRaw?

theorem decode_hex32_zero : decodeHex32? "00000000" = some ⟨0, by decide⟩ := by decide

theorem decode_hex32_max :
    decodeHex32? "ffffffff" = some ⟨4294967295, by decide⟩ := by decide

theorem encode_hex32_keeps_leading_zeroes :
    encodeHex32 ⟨1, by decide⟩ = "00000001" := by decide

theorem u32_wire_round_trip_example :
    decodeHex32? (encodeHex32 ⟨4294967295, by decide⟩) =
      some ⟨4294967295, by decide⟩ := by decide

theorem encode_hex32_max_is_canonical :
    encodeHex32 ⟨4294967295, by decide⟩ = "ffffffff" := by decide

theorem decode_hex32_rejects_wrong_width : decodeHex32? "0000000" = none := by decide

theorem decode_hex32_rejects_uppercase : decodeHex32? "0000000A" = none := by decide

theorem decode_hex32_rejects_invalid_digit : decodeHex32? "0000000g" = none := by decide

theorem decode_hex64_zero : decodeHex64? "0000000000000000" = some ⟨0, by decide⟩ := by decide

theorem decode_hex64_max :
    decodeHex64? "ffffffffffffffff" = some ⟨18446744073709551615, by decide⟩ := by decide

theorem encode_hex64_keeps_leading_zeroes :
    encodeHex64 ⟨1, by decide⟩ = "0000000000000001" := by decide

theorem u64_wire_round_trip_example :
    decodeHex64? (encodeHex64 ⟨18446744073709551615, by decide⟩) =
      some ⟨18446744073709551615, by decide⟩ := by decide

theorem encode_hex64_max_is_canonical :
    encodeHex64 ⟨18446744073709551615, by decide⟩ = "ffffffffffffffff" := by decide

theorem decode_hex64_rejects_short_input : decodeHex64? "fffffffffffffff" = none := by decide

theorem decode_hex64_rejects_uppercase :
    decodeHex64? "000000000000000A" = none := by decide

def byteZero : WireByte := ⟨0, by decide⟩
def byteFifteen : WireByte := ⟨15, by decide⟩
def byteSixteen : WireByte := ⟨16, by decide⟩
def byteMax : WireByte := ⟨255, by decide⟩

theorem bytes_round_trip_example :
    decodeHexBytes? (encodeHexBytes [byteZero, byteFifteen, byteSixteen, byteMax]) =
      some [byteZero, byteFifteen, byteSixteen, byteMax] := by decide

theorem bytes_encoding_is_lowercase_and_ordered :
    encodeHexBytes [byteZero, byteFifteen, byteSixteen, byteMax] = "000f10ff" := by decide

theorem decode_bytes_rejects_odd_width : decodeHexBytes? "0" = none := by decide

theorem decode_bytes_rejects_uppercase : decodeHexBytes? "0A" = none := by decide

theorem decode_bytes_accepts_empty : decodeHexBytes? "" = some [] := by decide

theorem bridge_id_is_one_based :
    (BridgeId.fromIndex? 0).map BridgeId.raw = some ⟨1, by decide⟩ := by decide

theorem bridge_id_uses_last_nonreserved_value :
    (BridgeId.fromIndex? 4294967293).map BridgeId.raw = some ⟨4294967294, by decide⟩ := by decide

theorem bridge_id_rejects_raw_zero :
    BridgeId.fromRaw? ⟨0, by decide⟩ = none := by decide

theorem bridge_id_rejects_ir_none :
    BridgeId.fromRaw? ⟨4294967295, by decide⟩ = none := by decide

theorem bridge_id_wire_round_trip_example :
    BridgeId.fromHex? (BridgeId.toHex ⟨⟨1, by decide⟩, by decide, by decide⟩) =
      some ⟨⟨1, by decide⟩, by decide, by decide⟩ := by decide

theorem bridge_id_wire_encoding_is_one_based :
    BridgeId.toHex ⟨⟨1, by decide⟩, by decide, by decide⟩ = "00000001" := by decide

theorem bridge_id_wire_rejects_zero : BridgeId.fromHex? "00000000" = none := by decide

theorem bridge_id_wire_rejects_ir_none :
    BridgeId.fromHex? "ffffffff" = none := by decide

theorem optional_bridge_id_preserves_ir_none :
    (OptionalBridgeId.fromRaw? ⟨4294967295, by decide⟩).map OptionalBridgeId.raw =
      some ⟨4294967295, by decide⟩ := by decide

theorem optional_bridge_id_wire_round_trip_example :
    (OptionalBridgeId.fromHex? "ffffffff").map OptionalBridgeId.toHex =
      some "ffffffff" := by decide

theorem bridge_id_rejects_reserved_max_index : BridgeId.fromIndex? 4294967294 = none := by decide

end Xsh.Conformance.Transport
