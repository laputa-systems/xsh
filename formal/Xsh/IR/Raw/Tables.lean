import Xsh.IR.Raw.Ranges

namespace Xsh.IR.Raw

open Xsh.Foundation

/-- The two direct words stored beside each raw instruction tag. -/
structure RawData where
  lhs : Word32
  rhs : Word32
  deriving DecidableEq, Repr

namespace RawData

def range (data : RawData) : RawRange :=
  ⟨DirectIndex.decode data.lhs, data.rhs⟩

end RawData

/-- Tags, row data, and auxiliary words stay separate, including malformed tables. -/
structure Tables where
  tags : Array Word32
  data : Array RawData
  extra : Array Word32
  deriving Repr

namespace Tables

def aligned (tables : Tables) : Prop := tables.tags.size = tables.data.size

end Tables

/-- A row keeps the raw tag word so unknown ordinals remain representable. -/
structure RawRow where
  tag : Word32
  data : RawData
  deriving DecidableEq, Repr

end Xsh.IR.Raw
