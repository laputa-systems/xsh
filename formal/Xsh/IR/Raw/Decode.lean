import Xsh.IR.Raw.Tables

namespace Xsh.IR.Raw

open Xsh.Foundation

namespace Decode

/-- Read both parallel row arrays with checked `Array.get?` operations. -/
def readRow? (tables : Tables) (index : DirectIndex) : Option RawRow := do
  let tag ← tables.tags[index.value]?
  let data ← tables.data[index.value]?
  pure ⟨tag, data⟩

/-- Read `count` words from an array without partial indexing. -/
def readWords? (words : Array Word32) (start : Nat) : Nat → Option (List Word32)
  | 0 => some []
  | remaining + 1 => do
      let word ← words[start]?
      let rest ← readWords? words (start + 1) remaining
      pure (word :: rest)

/-- Resolve the row's payload range before reading each auxiliary word safely. -/
def readPayload? (tables : Tables) (range : RawRange) : Option (List Word32) := do
  let (start, finish) ← range.resolve? tables.extra.size
  readWords? tables.extra start (finish - start)

end Decode

end Xsh.IR.Raw
