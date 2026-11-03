import Xsh.Conformance.Vertical.Artifact

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

def bridgeId! (index : Nat) : BridgeId :=
  match BridgeId.fromRaw? (Fin.ofNat 4294967296 index) with
  | some id => id
  | none => ⟨⟨1, by decide⟩, by decide, by decide⟩

def fixtureSourceTextCodes : List Nat :=
  [112, 117, 114, 101, 32, 115, 104, 105, 102, 116, 101, 100, 40, 41, 32, 45, 62, 32, 73, 110,
   116, 32, 123, 10, 32, 32, 108, 101, 116, 32, 120, 32, 61, 32, 50, 10, 32, 32, 114, 101,
   116, 117, 114, 110, 32, 120, 32, 43, 32, 51, 10, 125, 10, 10, 112, 117, 114, 101, 32, 115,
   99, 97, 108, 101, 100, 40, 41, 32, 45, 62, 32, 73, 110, 116, 32, 123, 10, 32, 32, 108,
   101, 116, 32, 121, 32, 61, 32, 55, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 121, 32,
   42, 32, 54, 10, 125, 10, 10, 112, 117, 114, 101, 32, 100, 105, 118, 105, 100, 101, 100, 40,
   41, 32, 45, 62, 32, 73, 110, 116, 32, 123, 10, 32, 32, 108, 101, 116, 32, 122, 101, 114,
   111, 32, 61, 32, 48, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 49, 32, 47, 32, 122,
   101, 114, 111, 10, 125, 10, 10, 112, 117, 114, 101, 32, 111, 118, 101, 114, 102, 108, 111, 119,
   101, 100, 40, 41, 32, 45, 62, 32, 73, 110, 116, 32, 123, 10, 32, 32, 108, 101, 116, 32,
   116, 111, 112, 32, 61, 32, 57, 50, 50, 51, 51, 55, 50, 48, 51, 54, 56, 53, 52, 55,
   55, 53, 56, 48, 55, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 116, 111, 112, 32, 43,
   32, 49, 10, 125, 10, 10, 112, 117, 114, 101, 32, 101, 120, 99, 101, 115, 115, 40, 118, 97,
   108, 117, 101, 58, 32, 73, 110, 116, 44, 32, 108, 105, 109, 105, 116, 58, 32, 73, 110, 116,
   41, 32, 45, 62, 32, 73, 110, 116, 32, 123, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32,
   105, 102, 32, 118, 97, 108, 117, 101, 32, 62, 32, 108, 105, 109, 105, 116, 32, 123, 32, 118,
   97, 108, 117, 101, 32, 45, 32, 108, 105, 109, 105, 116, 32, 125, 32, 101, 108, 115, 101, 32,
   123, 32, 48, 32, 125, 10, 125, 10, 10, 112, 117, 114, 101, 32, 115, 117, 114, 112, 108, 117,
   115, 40, 41, 32, 45, 62, 32, 73, 110, 116, 32, 123, 10, 32, 32, 108, 101, 116, 32, 120,
   32, 61, 32, 49, 50, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 101, 120, 99, 101, 115,
   115, 40, 120, 44, 32, 57, 41, 10, 125, 10]

def fixtureSourceDocument : SourceDocument :=
  ⟨bridgeId! 1, String.ofList (fixtureSourceTextCodes.map Char.ofNat)⟩

def fixtureArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .intInt, 0, 2⟩,
    ⟨bridgeId! 2, .stmtLetInt, 2, 2⟩,
    ⟨bridgeId! 3, .exprParam, 4, 1⟩,
    ⟨bridgeId! 4, .exprInt, 5, 2⟩,
    ⟨bridgeId! 5, .exprBinary, 7, 4⟩,
    ⟨bridgeId! 6, .stmtReturn, 11, 1⟩,
    ⟨bridgeId! 7, .intInt, 15, 2⟩,
    ⟨bridgeId! 8, .stmtLetInt, 17, 2⟩,
    ⟨bridgeId! 9, .exprParam, 19, 1⟩,
    ⟨bridgeId! 10, .exprInt, 20, 2⟩,
    ⟨bridgeId! 11, .exprBinary, 22, 4⟩,
    ⟨bridgeId! 12, .stmtReturn, 26, 1⟩,
    ⟨bridgeId! 13, .intInt, 30, 2⟩,
    ⟨bridgeId! 14, .stmtLetInt, 32, 2⟩,
    ⟨bridgeId! 15, .exprInt, 34, 2⟩,
    ⟨bridgeId! 16, .exprParam, 36, 1⟩,
    ⟨bridgeId! 17, .exprBinary, 37, 4⟩,
    ⟨bridgeId! 18, .stmtReturn, 41, 1⟩,
    ⟨bridgeId! 19, .intInt, 45, 2⟩,
    ⟨bridgeId! 20, .stmtLetInt, 47, 2⟩,
    ⟨bridgeId! 21, .exprParam, 49, 1⟩,
    ⟨bridgeId! 22, .exprInt, 50, 2⟩,
    ⟨bridgeId! 23, .exprBinary, 52, 4⟩,
    ⟨bridgeId! 24, .stmtReturn, 56, 1⟩,
    ⟨bridgeId! 25, .exprParam, 60, 1⟩,
    ⟨bridgeId! 26, .exprParam, 61, 1⟩,
    ⟨bridgeId! 27, .exprBinary, 62, 4⟩,
    ⟨bridgeId! 28, .exprParam, 66, 1⟩,
    ⟨bridgeId! 29, .exprParam, 67, 1⟩,
    ⟨bridgeId! 30, .exprBinary, 68, 4⟩,
    ⟨bridgeId! 31, .stmtValue, 72, 1⟩,
    ⟨bridgeId! 32, .exprValueBlock, 75, 2⟩,
    ⟨bridgeId! 33, .exprInt, 80, 2⟩,
    ⟨bridgeId! 34, .stmtValue, 82, 1⟩,
    ⟨bridgeId! 35, .exprValueBlock, 85, 2⟩,
    ⟨bridgeId! 36, .exprIf, 87, 3⟩,
    ⟨bridgeId! 37, .stmtReturn, 90, 1⟩,
    ⟨bridgeId! 38, .intInt, 93, 2⟩,
    ⟨bridgeId! 39, .stmtLetInt, 95, 2⟩,
    ⟨bridgeId! 40, .exprParam, 97, 1⟩,
    ⟨bridgeId! 41, .exprInt, 98, 2⟩,
    ⟨bridgeId! 42, .exprDirectPureCall, 105, 3⟩,
    ⟨bridgeId! 43, .stmtReturn, 108, 1⟩]
  extra := [2, 0, 1, 0, 1, 3, 0, 0, 2, 3, 2, 4, 2, 1, 5, 7, 0, 1, 6, 1, 6, 0, 1, 8, 9, 3, 10, 2, 7,
    11, 0, 0, 1, 12, 1, 0, 1, 2, 14, 15, 4, 16, 2, 13, 17, 4294967295, 2147483647, 1, 18,
    1, 1, 0, 0, 20, 21, 5, 22, 2, 19, 23, 0, 1, 3, 24, 25, 6, 0, 1, 4, 27, 28, 7, 29, 1,
    30, 5, 8, 1, 26, 31, 0, 0, 32, 1, 33, 7, 9, 6, 34, 10, 35, 1, 36, 12, 0, 1, 37, 1, 9,
    0, 2, 0, 39, 0, 40, 5, 9, 11, 41, 2, 38, 42]
  ops := [.add, .mul, .div, .gt, .sub]
  blocks := [⟨bridgeId! 1, 12, 3, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 27, 3, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩,
    ⟨bridgeId! 3, 42, 3, ⟨none⟩, ⟨some (bridgeId! 3)⟩, 3⟩,
    ⟨bridgeId! 4, 57, 3, ⟨none⟩, ⟨some (bridgeId! 4)⟩, 3⟩,
    ⟨bridgeId! 5, 73, 2, ⟨none⟩, ⟨some (bridgeId! 5)⟩, 1⟩,
    ⟨bridgeId! 6, 77, 3, ⟨none⟩, ⟨some (bridgeId! 5)⟩, 0⟩,
    ⟨bridgeId! 7, 83, 2, ⟨none⟩, ⟨some (bridgeId! 5)⟩, 1⟩,
    ⟨bridgeId! 8, 91, 2, ⟨none⟩, ⟨some (bridgeId! 5)⟩, 3⟩,
    ⟨bridgeId! 9, 100, 5, ⟨none⟩, ⟨some (bridgeId! 6)⟩, 0⟩,
    ⟨bridgeId! 10, 109, 3, ⟨none⟩, ⟨some (bridgeId! 6)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "shifted", 0, 0, 0, 1, bridgeId! 1, 2⟩,
    ⟨bridgeId! 2, "scaled", 0, 0, 1, 1, bridgeId! 2, 2⟩,
    ⟨bridgeId! 3, "divided", 0, 0, 2, 1, bridgeId! 3, 2⟩,
    ⟨bridgeId! 4, "overflowed", 0, 0, 3, 1, bridgeId! 4, 2⟩,
    ⟨bridgeId! 5, "excess", 0, 2, 4, 1, bridgeId! 8, 3⟩,
    ⟨bridgeId! 6, "surplus", 2, 0, 5, 1, bridgeId! 10, 2⟩]
  observations := [⟨bridgeId! 1, [], .value 5⟩,
    ⟨bridgeId! 2, [], .value 42⟩,
    ⟨bridgeId! 3, [], .failure .divisionByZero⟩,
    ⟨bridgeId! 4, [], .failure .integerOverflow⟩,
    ⟨bridgeId! 5, [12, 9], .value 3⟩,
    ⟨bridgeId! 5, [3, 9], .value 0⟩,
    ⟨bridgeId! 6, [], .value 3⟩]

end Xsh.Conformance.Vertical
