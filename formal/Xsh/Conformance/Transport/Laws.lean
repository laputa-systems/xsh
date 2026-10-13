import Xsh.Conformance.Transport.Codecs

namespace Xsh.Conformance.Transport

theorem bridge_id_from_raw_round_trip (id : BridgeId) :
    BridgeId.fromRaw? id.raw = some id := by
  simp [BridgeId.fromRaw?, id.nonzero, id.notNone]

theorem bridge_id_wire_round_trip (id : BridgeId) :
    BridgeId.fromHex? (BridgeId.toHex id) = some id := by
  change (decodeHex32? (encodeHex32 id.raw)).bind BridgeId.fromRaw? = some id
  rw [decode_encode_hex32]
  exact bridge_id_from_raw_round_trip id

theorem optional_bridge_id_from_raw_round_trip (id : OptionalBridgeId) :
    OptionalBridgeId.fromRaw? id.raw = some id := by
  cases id with
  | mk value =>
      cases value with
      | none => simp [OptionalBridgeId.fromRaw?, OptionalBridgeId.raw] <;> decide
      | some present =>
          simp [OptionalBridgeId.fromRaw?, OptionalBridgeId.raw,
            BridgeId.fromRaw?, present.nonzero, present.notNone]

theorem optional_bridge_id_wire_round_trip (id : OptionalBridgeId) :
    OptionalBridgeId.fromHex? (OptionalBridgeId.toHex id) = some id := by
  change (decodeHex32? (encodeHex32 id.raw)).bind OptionalBridgeId.fromRaw? = some id
  rw [decode_encode_hex32]
  exact optional_bridge_id_from_raw_round_trip id

theorem optional_bridge_id_raw_round_trip (raw : Xsh.Foundation.Word32)
    (nonzero : raw.val ≠ 0) :
    (OptionalBridgeId.fromRaw? raw).map OptionalBridgeId.raw = some raw := by
  by_cases notNone : raw.val ≠ 4294967295
  · simp [OptionalBridgeId.fromRaw?, BridgeId.fromRaw?, nonzero, notNone,
      OptionalBridgeId.raw]
  · have isNone : raw.val = 4294967295 := by omega
    have rawEq : raw = ⟨4294967295, by decide⟩ := Fin.ext isNone
    subst raw
    simp [OptionalBridgeId.fromRaw?, OptionalBridgeId.raw]

/-- Open boundary: every accepted u32 text re-encodes byte-for-byte. The proved
encoder round trip covers strings produced by `encodeHex32`; this stronger
accepted-input uniqueness statement remains unproved. -/
def acceptedHex32Reencodes : Prop :=
  ∀ (text : String) (value : Xsh.Foundation.Word32),
    decodeHex32? text = some value → encodeHex32 value = text

/-- Open boundary: every accepted u64 text re-encodes byte-for-byte. -/
def acceptedHex64Reencodes : Prop :=
  ∀ (text : String) (value : Word64),
    decodeHex64? text = some value → encodeHex64 value = text

/-- Open boundary: every accepted byte-list text re-encodes byte-for-byte. -/
def acceptedHexBytesReencodes : Prop :=
  ∀ (text : String) (value : List WireByte),
    decodeHexBytes? text = some value → encodeHexBytes value = text

end Xsh.Conformance.Transport
