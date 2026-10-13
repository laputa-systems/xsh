namespace Xsh.Foundation

/-- A stored 32-bit word has exactly the values representable by a `u32`. -/
abbrev Word32 := Fin 4294967296

namespace Word32

/-- Decode a natural number only when it fits; values are never truncated. -/
def decode? (value : Nat) : Option Word32 :=
  if h : value < 4294967296 then some ⟨value, h⟩ else none

/-- Values wider than a stored word fail decoding instead of wrapping. -/
theorem decode_rejects_out_of_range {value : Nat}
    (h : 4294967296 ≤ value) : decode? value = none := by
  simp [decode?, Nat.not_lt_of_ge h]

/-- Every valid word survives conversion to and from its natural representation. -/
theorem decode_value (word : Word32) : decode? word.val = some word := by
  simp [decode?, word.isLt]

end Word32

/-- A source binder identity cannot be confused with a name-table identity. -/
structure BinderId where
  word : Word32
  deriving DecidableEq

namespace BinderId

def decode? (value : Nat) : Option BinderId :=
  (Word32.decode? value).map BinderId.mk

def value (id : BinderId) : Nat := id.word.val

theorem decode_value (id : BinderId) : decode? id.value = some id := by
  simp [decode?, value, Word32.decode_value]

end BinderId

/-- A name-table identity is distinct from a source binder identity. -/
structure NameId where
  word : Word32
  deriving DecidableEq

namespace NameId

def decode? (value : Nat) : Option NameId :=
  (Word32.decode? value).map NameId.mk

def value (id : NameId) : Nat := id.word.val

theorem decode_value (id : NameId) : decode? id.value = some id := by
  simp [decode?, value, Word32.decode_value]

end NameId

end Xsh.Foundation
