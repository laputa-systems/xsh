import Xsh.Conformance.Connected.ResourceFacts

namespace Xsh.Conformance.Connected
open Xsh.Conformance.Transport

def scopedTransferFactsCodes : List Nat :=
  [123, 34, 115, 99, 104, 101, 109, 97, 34, 58, 34, 120, 115, 104, 45, 99, 111, 110, 110, 101, 99, 116, 101, 100, 45, 114, 101, 115, 111, 117, 114, 99, 101, 45, 116, 114, 97, 110, 115, 102, 101, 114, 115, 45, 118, 49, 34, 44, 34, 102, 97, 99, 116, 115, 34, 58, 91, 123, 34, 114, 111, 108, 101, 34, 58, 34, 98, 108, 111, 99, 107, 34, 44, 34, 115, 105, 116, 101, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 51, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 52, 50, 34, 125, 44, 34, 116, 114, 97, 110, 115, 102, 101, 114, 115, 34, 58, 34, 116, 114, 117, 101, 34, 44, 34, 99, 104, 101, 99, 107, 101, 100, 79, 118, 101, 114, 114, 105, 100, 101, 34, 58, 91, 93, 125, 44, 123, 34, 114, 111, 108, 101, 34, 58, 34, 98, 108, 111, 99, 107, 34, 44, 34, 115, 105, 116, 101, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 51, 55, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 51, 34, 125, 44, 34, 116, 114, 97, 110, 115, 102, 101, 114, 115, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 99, 104, 101, 99, 107, 101, 100, 79, 118, 101, 114, 114, 105, 100, 101, 34, 58, 91, 93, 125, 44, 123, 34, 114, 111, 108, 101, 34, 58, 34, 97, 115, 115, 105, 103, 110, 109, 101, 110, 116, 34, 44, 34, 115, 105, 116, 101, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 51, 57, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 102, 34, 125, 44, 34, 116, 114, 97, 110, 115, 102, 101, 114, 115, 34, 58, 34, 116, 114, 117, 101, 34, 44, 34, 99, 104, 101, 99, 107, 101, 100, 79, 118, 101, 114, 114, 105, 100, 101, 34, 58, 91, 93, 125, 93, 125]

def scopedTransferFactsWire : WireValue := .object [
  ("schema", .string "xsh-connected-resource-transfers-v1"),
  ("facts", .array [
    .object [("role", .string "block"), ("site", .object [("source", .string "00000000"),
      ("start", .string "00000013"), ("length", .string "00000042")]),
      ("transfers", .string "true"), ("checkedOverride", .array [])],
    .object [("role", .string "block"), ("site", .object [("source", .string "00000000"),
      ("start", .string "00000037"), ("length", .string "00000013")]),
      ("transfers", .string "false"), ("checkedOverride", .array [])],
    .object [("role", .string "assignment"), ("site", .object [("source", .string "00000000"),
      ("start", .string "00000039"), ("length", .string "0000000f")]),
      ("transfers", .string "true"), ("checkedOverride", .array [])]])]

set_option maxRecDepth 100000 in
theorem scoped_transfer_facts_parse_to_wire :
    parseCodes scopedTransferFactsCodes = some scopedTransferFactsWire := by rfl

def scopedTransferFacts : List TransferFact :=
  [⟨.blockValue, ⟨0, 19, 66⟩, true, none⟩,
   ⟨.blockValue, ⟨0, 55, 19⟩, false, none⟩,
   ⟨.assignment, ⟨0, 57, 15⟩, true, none⟩]

set_option maxRecDepth 100000 in
theorem scoped_transfer_facts_decode :
    decodeTransferFacts scopedTransferFactsWire = some scopedTransferFacts := by decide

end Xsh.Conformance.Connected
