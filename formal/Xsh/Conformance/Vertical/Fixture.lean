import Xsh.Conformance.Vertical.Artifact

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

def bridgeId! (index : Nat) : BridgeId :=
  match BridgeId.fromRaw? (Fin.ofNat 4294967296 index) with
  | some id => id
  | none => ⟨⟨1, by decide⟩, by decide, by decide⟩

def fixtureSourceTextCodes : List Nat :=
  [112, 117, 114, 101, 32, 100, 105, 118, 105, 100, 101, 100, 40, 41, 32, 45, 62, 32, 73, 110,
   116, 32, 123, 10, 32, 32, 108, 101, 116, 32, 122, 101, 114, 111, 32, 61, 32, 48, 10, 32,
   32, 114, 101, 116, 117, 114, 110, 32, 49, 32, 47, 32, 122, 101, 114, 111, 10, 125, 10, 10,
   112, 117, 114, 101, 32, 111, 118, 101, 114, 102, 108, 111, 119, 101, 100, 40, 41, 32, 45, 62,
   32, 73, 110, 116, 32, 123, 10, 32, 32, 108, 101, 116, 32, 116, 111, 112, 32, 61, 32, 57,
   50, 50, 51, 51, 55, 50, 48, 51, 54, 56, 53, 52, 55, 55, 53, 56, 48, 55, 10, 32,
   32, 114, 101, 116, 117, 114, 110, 32, 116, 111, 112, 32, 43, 32, 49, 10, 125, 10]

def fixtureSourceDocument : SourceDocument :=
  ⟨bridgeId! 1, String.ofList (fixtureSourceTextCodes.map Char.ofNat)⟩

def fixtureArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .intInt, 0, 2⟩, ⟨bridgeId! 2, .stmtLetInt, 2, 2⟩, ⟨bridgeId! 3, .exprInt, 4, 2⟩,
    ⟨bridgeId! 4, .exprParam, 6, 1⟩, ⟨bridgeId! 5, .exprBinary, 7, 4⟩,
    ⟨bridgeId! 6, .stmtReturn, 11, 1⟩, ⟨bridgeId! 7, .intInt, 15, 2⟩,
    ⟨bridgeId! 8, .stmtLetInt, 17, 2⟩, ⟨bridgeId! 9, .exprParam, 19, 1⟩,
    ⟨bridgeId! 10, .exprInt, 20, 2⟩, ⟨bridgeId! 11, .exprBinary, 22, 4⟩,
    ⟨bridgeId! 12, .stmtReturn, 26, 1⟩]
  extra := [0, 0, 1, 0, 1, 0, 1, 0, 2, 3, 2, 4, 2, 1, 5, 4294967295, 2147483647, 1, 6, 1, 1, 0, 1, 8, 9, 3,
    10, 2, 7, 11]
  ops := [.div, .add]
  blocks := [⟨bridgeId! 1, 12, 3, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 27, 3, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "divided", 0, 0, 0, 1, bridgeId! 1, 2⟩,
    ⟨bridgeId! 2, "overflowed", 0, 0, 1, 1, bridgeId! 2, 2⟩]
  observations := [⟨bridgeId! 1, [], .failure .divisionByZero⟩, ⟨bridgeId! 2, [], .failure .integerOverflow⟩]

def callsSourceTextCodes : List Nat :=
  [112, 117, 114, 101, 32, 101, 120, 99, 101, 115, 115, 40, 118, 97, 108, 117, 101, 58, 32, 73,
   110, 116, 44, 32, 108, 105, 109, 105, 116, 58, 32, 73, 110, 116, 41, 32, 45, 62, 32, 73,
   110, 116, 32, 123, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 105, 102, 32, 118, 97, 108,
   117, 101, 32, 62, 32, 108, 105, 109, 105, 116, 32, 123, 32, 118, 97, 108, 117, 101, 32, 45,
   32, 108, 105, 109, 105, 116, 32, 125, 32, 101, 108, 115, 101, 32, 123, 32, 48, 32, 125, 10,
   125, 10, 10, 112, 117, 114, 101, 32, 115, 117, 114, 112, 108, 117, 115, 40, 41, 32, 45, 62,
   32, 73, 110, 116, 32, 123, 10, 32, 32, 108, 101, 116, 32, 120, 32, 61, 32, 49, 50, 10,
   32, 32, 114, 101, 116, 117, 114, 110, 32, 101, 120, 99, 101, 115, 115, 40, 120, 44, 32, 57,
   41, 10, 125, 10]

def callsSourceDocument : SourceDocument :=
  ⟨bridgeId! 1, String.ofList (callsSourceTextCodes.map Char.ofNat)⟩

def callsArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .exprParam, 0, 1⟩, ⟨bridgeId! 2, .exprParam, 1, 1⟩,
    ⟨bridgeId! 3, .exprBinary, 2, 4⟩, ⟨bridgeId! 4, .exprParam, 6, 1⟩,
    ⟨bridgeId! 5, .exprParam, 7, 1⟩, ⟨bridgeId! 6, .exprBinary, 8, 4⟩,
    ⟨bridgeId! 7, .stmtValue, 12, 1⟩, ⟨bridgeId! 8, .exprValueBlock, 15, 2⟩,
    ⟨bridgeId! 9, .exprInt, 20, 2⟩, ⟨bridgeId! 10, .stmtValue, 22, 1⟩,
    ⟨bridgeId! 11, .exprValueBlock, 25, 2⟩, ⟨bridgeId! 12, .exprIf, 27, 3⟩,
    ⟨bridgeId! 13, .stmtReturn, 30, 1⟩, ⟨bridgeId! 14, .intInt, 33, 2⟩,
    ⟨bridgeId! 15, .stmtLetInt, 35, 2⟩, ⟨bridgeId! 16, .exprParam, 37, 1⟩,
    ⟨bridgeId! 17, .exprInt, 38, 2⟩, ⟨bridgeId! 18, .exprDirectPureCall, 45, 3⟩,
    ⟨bridgeId! 19, .stmtReturn, 48, 1⟩]
  extra := [0, 1, 0, 0, 1, 2, 0, 1, 1, 3, 4, 3, 5, 1, 6, 1, 4, 1, 2, 7, 0, 0, 8, 1, 9, 3, 5, 2, 10, 6, 11,
    1, 12, 12, 0, 1, 13, 1, 9, 0, 2, 0, 15, 0, 16, 1, 5, 7, 17, 2, 14, 18]
  ops := [.gt, .sub]
  blocks := [⟨bridgeId! 1, 13, 2, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 1⟩,
    ⟨bridgeId! 2, 17, 3, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 0⟩,
    ⟨bridgeId! 3, 23, 2, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 1⟩,
    ⟨bridgeId! 4, 31, 2, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 5, 40, 5, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 0⟩,
    ⟨bridgeId! 6, 49, 3, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "excess", 0, 2, 0, 1, bridgeId! 4, 3⟩,
    ⟨bridgeId! 2, "surplus", 2, 0, 1, 1, bridgeId! 6, 2⟩]
  observations := [⟨bridgeId! 1, [12, 9], .value 3⟩, ⟨bridgeId! 1, [3, 9], .value 0⟩, ⟨bridgeId! 2, [], .value 3⟩]

def assignmentsSourceTextCodes : List Nat :=
  [112, 117, 114, 101, 32, 115, 99, 97, 108, 101, 100, 40, 41, 32, 45, 62, 32, 73, 110, 116, 32,
   123, 10, 32, 32, 118, 97, 114, 32, 116, 111, 116, 97, 108, 32, 61, 32, 53, 10, 32, 32, 116, 111,
   116, 97, 108, 32, 61, 32, 116, 111, 116, 97, 108, 32, 42, 32, 54, 10, 32, 32, 114, 101, 116,
   117, 114, 110, 32, 116, 111, 116, 97, 108, 32, 43, 32, 49, 10, 125, 10, 10, 112, 117, 114, 101,
   32, 119, 114, 97, 112, 112, 101, 100, 40, 41, 32, 45, 62, 32, 73, 110, 116, 32, 123, 10, 32, 32,
   118, 97, 114, 32, 116, 111, 112, 32, 61, 32, 57, 50, 50, 51, 51, 55, 50, 48, 51, 54, 56, 53, 52,
   55, 55, 53, 56, 48, 55, 10, 32, 32, 116, 111, 112, 32, 61, 32, 116, 111, 112, 32, 43, 32, 49,
   10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 116, 111, 112, 10, 125, 10]

def assignmentsSourceDocument : SourceDocument :=
  ⟨bridgeId! 1, String.ofList (assignmentsSourceTextCodes.map Char.ofNat)⟩

def assignmentsArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .intInt, 0, 2⟩,
    ⟨bridgeId! 2, .stmtLetInt, 2, 2⟩,
    ⟨bridgeId! 3, .intSlot, 4, 1⟩,
    ⟨bridgeId! 4, .intInt, 5, 2⟩,
    ⟨bridgeId! 5, .intBinary, 7, 3⟩,
    ⟨bridgeId! 6, .stmtAssignInt, 10, 4⟩,
    ⟨bridgeId! 7, .exprParam, 14, 1⟩,
    ⟨bridgeId! 8, .exprInt, 15, 2⟩,
    ⟨bridgeId! 9, .exprBinary, 17, 4⟩,
    ⟨bridgeId! 10, .stmtReturn, 21, 1⟩,
    ⟨bridgeId! 11, .intInt, 26, 2⟩,
    ⟨bridgeId! 12, .stmtLetInt, 28, 2⟩,
    ⟨bridgeId! 13, .intSlot, 30, 1⟩,
    ⟨bridgeId! 14, .intInt, 31, 2⟩,
    ⟨bridgeId! 15, .intBinary, 33, 3⟩,
    ⟨bridgeId! 16, .stmtAssignInt, 36, 4⟩,
    ⟨bridgeId! 17, .exprParam, 40, 1⟩,
    ⟨bridgeId! 18, .stmtReturn, 41, 1⟩]
  extra := [5, 0, 1, 0, 1, 6, 0, 0, 2, 3, 1, 0, 4, 2, 1, 1, 0, 1, 6, 7, 3, 8, 3, 1, 5, 9, 4294967295,
    2147483647, 1, 10, 1, 1, 0, 1, 12, 13, 1, 0, 14, 4, 1, 16, 3, 11, 15, 17]
  ops := [.mul, .add]
  blocks := [⟨bridgeId! 1, 22, 4, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 42, 4, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "scaled", 0, 0, 0, 1, bridgeId! 1, 2⟩,
    ⟨bridgeId! 2, "wrapped", 0, 0, 1, 1, bridgeId! 2, 2⟩]
  observations := [⟨bridgeId! 1, [], .value 31⟩, ⟨bridgeId! 2, [], .failure .integerOverflow⟩]

def booleanSourceTextCodes : List Nat :=
  [112, 117, 114, 101, 32, 101, 120, 99, 101, 101, 100, 115, 40, 118, 97, 108, 117, 101, 58, 32,
   73, 110, 116, 44, 32, 108, 105, 109, 105, 116, 58, 32, 73, 110, 116, 41, 32, 45, 62, 32,
   66, 111, 111, 108, 32, 123, 10, 32, 32, 114, 101, 116, 117, 114, 110, 32, 118, 97, 108, 117,
   101, 32, 62, 32, 108, 105, 109, 105, 116, 10, 125, 10, 10, 112, 117, 114, 101, 32, 98, 101,
   108, 111, 119, 40, 118, 97, 108, 117, 101, 58, 32, 73, 110, 116, 44, 32, 108, 105, 109, 105,
   116, 58, 32, 73, 110, 116, 41, 32, 45, 62, 32, 66, 111, 111, 108, 32, 123, 10, 32, 32,
   114, 101, 116, 117, 114, 110, 32, 118, 97, 108, 117, 101, 32, 60, 32, 108, 105, 109, 105, 116,
   10, 125, 10]

def booleanSourceDocument : SourceDocument :=
  ⟨bridgeId! 1, String.ofList (booleanSourceTextCodes.map Char.ofNat)⟩

def booleanArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .exprParam, 0, 1⟩, ⟨bridgeId! 2, .exprParam, 1, 1⟩,
    ⟨bridgeId! 3, .exprBinary, 2, 4⟩, ⟨bridgeId! 4, .stmtReturn, 6, 1⟩,
    ⟨bridgeId! 5, .exprParam, 9, 1⟩, ⟨bridgeId! 6, .exprParam, 10, 1⟩,
    ⟨bridgeId! 7, .exprBinary, 11, 4⟩, ⟨bridgeId! 8, .stmtReturn, 15, 1⟩]
  extra := [0, 1, 0, 0, 1, 2, 2, 1, 3, 0, 1, 1, 4, 5, 3, 6, 1, 7]
  ops := [.gt, .lt]
  blocks := [⟨bridgeId! 1, 7, 2, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 16, 2, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "exceeds", 0, 2, 0, 1, bridgeId! 1, 3⟩,
    ⟨bridgeId! 2, "below", 2, 2, 1, 1, bridgeId! 2, 3⟩]
  observations := [⟨bridgeId! 1, [12, 9], .value 1⟩, ⟨bridgeId! 1, [3, 9], .value 0⟩,
    ⟨bridgeId! 2, [3, 9], .value 1⟩, ⟨bridgeId! 2, [12, 9], .value 0⟩]

def resultSourceDocument : SourceDocument :=
  ⟨bridgeId! 1,
    "pure keeps(value: Int) -> Result[Int, Int] {\n  return Ok(value)\n}\n\npure stops(value: Int) -> Result[Int, Int] {\n  return Err(value)\n}\n\npure adds(value: Int) -> Result[Int, Int] {\n  return Ok(value + 1)\n}\n"⟩

def resultArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .exprParam, 0, 1⟩, ⟨bridgeId! 2, .exprOk, 1, 1⟩,
    ⟨bridgeId! 3, .stmtReturn, 2, 1⟩, ⟨bridgeId! 4, .exprParam, 5, 1⟩,
    ⟨bridgeId! 5, .exprErr, 6, 2⟩, ⟨bridgeId! 6, .stmtReturn, 8, 1⟩,
    ⟨bridgeId! 7, .exprParam, 11, 1⟩, ⟨bridgeId! 8, .exprInt, 12, 2⟩,
    ⟨bridgeId! 9, .exprBinary, 14, 4⟩, ⟨bridgeId! 10, .exprOk, 18, 1⟩,
    ⟨bridgeId! 11, .stmtReturn, 19, 1⟩]
  extra := [0, 0, 1, 1, 2, 0, 3, 0, 4, 1, 5, 0, 1, 0, 0, 6, 7, 2, 8, 9, 1, 10]
  ops := [.add]
  blocks := [⟨bridgeId! 1, 3, 2, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 9, 2, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩,
    ⟨bridgeId! 3, 20, 2, ⟨none⟩, ⟨some (bridgeId! 3)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "keeps", 0, 1, 0, 1, bridgeId! 1, 2⟩,
    ⟨bridgeId! 2, "stops", 1, 1, 1, 1, bridgeId! 2, 2⟩,
    ⟨bridgeId! 3, "adds", 2, 1, 2, 1, bridgeId! 3, 2⟩]
  observations := [⟨bridgeId! 1, [7], .ok 7⟩, ⟨bridgeId! 2, [5], .err 5⟩,
    ⟨bridgeId! 3, [4], .ok 5⟩, ⟨bridgeId! 3, [18446744073709551615], .ok 0⟩]

def propagationSourceDocument : SourceDocument :=
  ⟨bridgeId! 1,
    "pure keeps(value: Int) -> Result[Int, Int] {\n  return Ok(value + 1)\n}\n\npure stops(value: Int) -> Result[Int, Int] {\n  return Err(value)\n}\n\npure passes(value: Int) -> Result[Int, Int] {\n  let kept = keeps(value)?\n  return Ok(kept)\n}\n\npure blocks(value: Int) -> Result[Int, Int] {\n  let kept = stops(value)?\n  return Ok(kept)\n}\n"⟩

def propagationArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .exprParam, 0, 1⟩, ⟨bridgeId! 2, .exprInt, 1, 2⟩,
    ⟨bridgeId! 3, .exprBinary, 3, 4⟩, ⟨bridgeId! 4, .exprOk, 7, 1⟩,
    ⟨bridgeId! 5, .stmtReturn, 8, 1⟩, ⟨bridgeId! 6, .exprParam, 11, 1⟩,
    ⟨bridgeId! 7, .exprErr, 12, 2⟩, ⟨bridgeId! 8, .stmtReturn, 14, 1⟩,
    ⟨bridgeId! 9, .exprParam, 17, 1⟩, ⟨bridgeId! 10, .exprDirectPureCall, 21, 3⟩,
    ⟨bridgeId! 11, .exprTry, 24, 2⟩, ⟨bridgeId! 12, .stmtLet, 26, 3⟩,
    ⟨bridgeId! 13, .exprParam, 29, 1⟩, ⟨bridgeId! 14, .exprOk, 30, 1⟩,
    ⟨bridgeId! 15, .stmtReturn, 31, 1⟩, ⟨bridgeId! 16, .exprParam, 35, 1⟩,
    ⟨bridgeId! 17, .exprDirectPureCall, 39, 3⟩, ⟨bridgeId! 18, .exprTry, 42, 2⟩,
    ⟨bridgeId! 19, .stmtLet, 44, 3⟩, ⟨bridgeId! 20, .exprParam, 47, 1⟩,
    ⟨bridgeId! 21, .exprOk, 48, 1⟩, ⟨bridgeId! 22, .stmtReturn, 49, 1⟩]
  extra := [0, 1, 0, 0, 0, 1, 2, 2, 3, 1, 4, 0, 5, 0, 6, 1, 7, 0, 1, 0, 8, 1, 3, 3, 9, 4,
    2, 10, 0, 2, 12, 13, 2, 11, 14, 0, 1, 0, 15, 2, 5, 5, 16, 6, 2, 17, 0, 2, 19, 20, 2, 18,
    21]
  ops := [.add]
  blocks := [⟨bridgeId! 1, 9, 2, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 15, 2, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩,
    ⟨bridgeId! 3, 18, 3, ⟨none⟩, ⟨some (bridgeId! 3)⟩, 0⟩,
    ⟨bridgeId! 4, 32, 3, ⟨none⟩, ⟨some (bridgeId! 3)⟩, 3⟩,
    ⟨bridgeId! 5, 36, 3, ⟨none⟩, ⟨some (bridgeId! 4)⟩, 0⟩,
    ⟨bridgeId! 6, 50, 3, ⟨none⟩, ⟨some (bridgeId! 4)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "keeps", 0, 1, 0, 1, bridgeId! 1, 2⟩,
    ⟨bridgeId! 2, "stops", 1, 1, 1, 1, bridgeId! 2, 2⟩,
    ⟨bridgeId! 3, "passes", 2, 1, 2, 1, bridgeId! 4, 3⟩,
    ⟨bridgeId! 4, "blocks", 3, 1, 3, 1, bridgeId! 6, 3⟩]
  observations := [⟨bridgeId! 1, [7], .ok 8⟩, ⟨bridgeId! 2, [5], .err 5⟩,
    ⟨bridgeId! 3, [7], .ok 8⟩, ⟨bridgeId! 4, [5], .err 5⟩,
    ⟨bridgeId! 3, [18446744073709551613], .ok 18446744073709551614⟩]

def letCallSourceDocument : SourceDocument :=
  ⟨bridgeId! 1,
    "pure bumps(value: Int) -> Int {\n  let step = 2\n  return value + step\n}\n\npure runs(value: Int) -> Int {\n  return bumps(value)\n}\n"⟩

def letCallArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .intInt, 0, 2⟩, ⟨bridgeId! 2, .stmtLetInt, 2, 2⟩,
    ⟨bridgeId! 3, .exprParam, 4, 1⟩, ⟨bridgeId! 4, .exprParam, 5, 1⟩,
    ⟨bridgeId! 5, .exprBinary, 6, 4⟩, ⟨bridgeId! 6, .stmtReturn, 10, 1⟩,
    ⟨bridgeId! 7, .exprParam, 14, 1⟩, ⟨bridgeId! 8, .exprDirectPureCall, 18, 3⟩,
    ⟨bridgeId! 9, .stmtReturn, 21, 1⟩]
  extra := [2, 0, 2, 0, 0, 2, 0, 2, 3, 2, 4, 2, 1, 5, 0, 1, 0, 6, 1, 2, 3, 7, 1, 8]
  ops := [.add]
  blocks := [⟨bridgeId! 1, 11, 3, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 15, 3, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 0⟩,
    ⟨bridgeId! 3, 22, 2, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "bumps", 0, 1, 0, 1, bridgeId! 1, 3⟩,
    ⟨bridgeId! 2, "runs", 1, 1, 1, 1, bridgeId! 3, 2⟩]
  observations := [⟨bridgeId! 1, [⟨5, by decide⟩], .value ⟨7, by decide⟩⟩,
    ⟨bridgeId! 2, [⟨5, by decide⟩], .value ⟨7, by decide⟩⟩,
    ⟨bridgeId! 2, [⟨18446744073709551607, by decide⟩],
      .value ⟨18446744073709551609, by decide⟩⟩]

def computedLetSourceDocument : SourceDocument :=
  ⟨bridgeId! 1,
    "pure scales(value: Int) -> Int {\n  let step = value + 4\n  return value + step\n}\n\npure runs(value: Int) -> Int {\n  return scales(value)\n}\n"⟩

def computedLetArtifact : RawArtifact where
  sourceId := bridgeId! 1
  rows := [⟨bridgeId! 1, .intSlot, 0, 1⟩, ⟨bridgeId! 2, .intInt, 1, 2⟩,
    ⟨bridgeId! 3, .intBinary, 3, 3⟩, ⟨bridgeId! 4, .stmtLetInt, 6, 2⟩,
    ⟨bridgeId! 5, .exprParam, 8, 1⟩, ⟨bridgeId! 6, .exprParam, 9, 1⟩,
    ⟨bridgeId! 7, .exprBinary, 10, 4⟩, ⟨bridgeId! 8, .stmtReturn, 14, 1⟩,
    ⟨bridgeId! 9, .exprParam, 18, 1⟩, ⟨bridgeId! 10, .exprDirectPureCall, 22, 3⟩,
    ⟨bridgeId! 11, .stmtReturn, 25, 1⟩]
  extra := [0, 4, 0, 0, 0, 1, 2, 2, 0, 2, 0, 4, 5, 2, 6, 2, 3, 7, 0, 1, 0, 8, 1, 2, 3, 9, 1, 10]
  ops := [.add]
  blocks := [⟨bridgeId! 1, 15, 3, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 19, 3, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 0⟩,
    ⟨bridgeId! 3, 26, 2, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "scales", 0, 1, 0, 1, bridgeId! 1, 3⟩,
    ⟨bridgeId! 2, "runs", 1, 1, 1, 1, bridgeId! 3, 2⟩]
  observations := [⟨bridgeId! 1, [⟨5, by decide⟩], .value ⟨14, by decide⟩⟩,
    ⟨bridgeId! 2, [⟨5, by decide⟩], .value ⟨14, by decide⟩⟩,
    ⟨bridgeId! 2, [⟨18446744073709551607, by decide⟩],
      .value ⟨18446744073709551602, by decide⟩⟩]

end Xsh.Conformance.Vertical
