import Xsh.Conformance.Connected.ScopedParse
import Xsh.Conformance.Connected.ScopedDocumentProof

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def scopedVariable : Variable := ⟨0, .value⟩
def scopedBodySite : SourceSite := ⟨0, 19, 66⟩
def scopedValueRead : ReadBinding := ⟨"value", ⟨0, 12, 5⟩, none, false, false⟩
def scopedLocalRead : ReadBinding := ⟨"current", ⟨0, 21, 20⟩, none, false, false⟩
def scopedTarget : FunctionTarget := ⟨0, "scoped", none, scopedBodySite⟩
def scopedStore : StoreOperation := ⟨⟨0, 67, 5⟩, .variable scopedVariable, .variable scopedVariable⟩
def scopedBodyDocument : BodyDocument := ⟨scopedBodySite, .pure, [0], [("value", .variable scopedVariable)],
  .variable scopedVariable, false, .unknown, false, [],
  [(0, ⟨0, 35, 5⟩, .variable scopedVariable), (1, ⟨0, 67, 5⟩, .variable scopedVariable),
   (2, ⟨0, 55, 19⟩, .primitive .unit)],
  [(3, ⟨0, 76, 7⟩, .variable scopedVariable)], [scopedValueRead, scopedLocalRead],
  [(0, scopedValueRead), (1, scopedValueRead)], [(3, scopedLocalRead)], [(1, 0)]⟩
def scopedDocument : ScopedFactsDocument := ⟨[scopedTarget], [⟨scopedBodyDocument, [.store scopedStore]⟩],
  [⟨5, ⟨0, 93, 9⟩, .call, scopedTarget, [(0, .primitive .integer)], [.store]⟩]⟩
def scopedScopes : List (SourceSite × List Nat) := [(scopedBodySite, [0])]

end Xsh.Conformance.Connected
