import Xsh.Conformance.Vertical.Artifact

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

def bridgeId! (index : Nat) : BridgeId :=
  match BridgeId.fromRaw? (Fin.ofNat 4294967296 index) with
  | some id => id
  | none => ⟨⟨1, by decide⟩, by decide, by decide⟩

def fixtureSourceDocument : SourceDocument :=
  ⟨bridgeId! 1, "pure shifted() -> Int {\n  let x = 2\n  return x + 3\n}\n\npure scaled() -> Int {\n  let y = 7\n  return y * 6\n}\n"⟩

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
    ⟨bridgeId! 12, .stmtReturn, 26, 1⟩]
  extra := [2, 0, 1, 0, 1, 3, 0, 0, 2, 3, 2, 4, 2, 1, 5, 7, 0, 1, 6, 1, 6, 0, 1, 8, 9, 3, 10, 2, 7, 11]
  ops := [.add, .mul]
  blocks := [⟨bridgeId! 1, 12, 3, ⟨none⟩, ⟨some (bridgeId! 1)⟩, 3⟩,
    ⟨bridgeId! 2, 27, 3, ⟨none⟩, ⟨some (bridgeId! 2)⟩, 3⟩]
  functions := [⟨bridgeId! 1, "shifted", 0, 0, 0, 1, bridgeId! 1, 2⟩,
    ⟨bridgeId! 2, "scaled", 0, 0, 1, 1, bridgeId! 2, 2⟩]
  observations := [⟨bridgeId! 1, 5⟩,
    ⟨bridgeId! 2, 42⟩]

end Xsh.Conformance.Vertical
