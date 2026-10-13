import Xsh.Foundation

namespace Xsh.IR.Raw

open Xsh.Foundation

def wordModulus : Nat := 4294967296

/-- `u32::MAX` is reserved for absent optional indexes and unfilled slots. -/
def IR_NONE : Word32 := ⟨4294967295, by decide⟩

inductive PoolKind where
  | blocks
  | functions
  | strings
  | bytes
  | locations
  | types
  | signatures
  | shapes
  deriving DecidableEq, Repr

/-- Pool identities are one-based and remain distinct across pool kinds. -/
structure PoolId (kind : PoolKind) where
  word : Word32
  positive : 0 < word.val
  belowNone : word.val < IR_NONE.val
  deriving DecidableEq

namespace PoolId

def encode {kind : PoolKind} (id : PoolId kind) : Word32 := id.word

def decode? (kind : PoolKind) (word : Word32) : Option (PoolId kind) :=
  if positive : 0 < word.val then
    if belowNone : word.val < IR_NONE.val then
      some ⟨word, positive, belowNone⟩
    else
      none
  else
    none

theorem decode_encode {kind : PoolKind} (id : PoolId kind) :
    decode? kind (encode id) = some id := by
  simp [decode?, encode, id.positive, id.belowNone]

theorem decode_rejects_zero (kind : PoolKind) :
    decode? kind ⟨0, by decide⟩ = none := by
  simp [decode?]

theorem decode_rejects_ir_none (kind : PoolKind) :
    decode? kind IR_NONE = none := by
  simp [decode?, IR_NONE]

end PoolId

abbrev BlockId := PoolId .blocks
abbrev FunctionId := PoolId .functions
abbrev StringId := PoolId .strings
abbrev BytesId := PoolId .bytes
abbrev LocationId := PoolId .locations
abbrev TypeId := PoolId .types
abbrev SignatureId := PoolId .signatures
abbrev ShapeId := PoolId .shapes

/-- Direct table indexes are zero-based and are not pool identities. -/
structure DirectIndex where
  word : Word32
  deriving DecidableEq, Repr

namespace DirectIndex

def decode (word : Word32) : DirectIndex := ⟨word⟩

def value (index : DirectIndex) : Nat := index.word.val

def zero : DirectIndex := ⟨⟨0, by decide⟩⟩

theorem decode_zero_is_zero : decode ⟨0, by decide⟩ = zero := rfl

end DirectIndex

/-- Optional direct indexes use `IR_NONE` as the absent representation. -/
structure PresentOptionalIndex where
  word : Word32
  belowNone : word.val < IR_NONE.val
  deriving DecidableEq, Repr

structure OptionalIndex where
  value : Option PresentOptionalIndex
  deriving DecidableEq, Repr

namespace OptionalIndex

def encode (index : OptionalIndex) : Word32 :=
  match index.value with
  | none => IR_NONE
  | some present => present.word

def decode (word : Word32) : OptionalIndex :=
  if isNone : word.val = IR_NONE.val then
    ⟨none⟩
  else
    ⟨some ⟨word, by
      have notNone : word.val ≠ IR_NONE.val := isNone
      have noneValue : IR_NONE.val = 4294967295 := rfl
      rw [noneValue] at notNone ⊢
      have atMostMax : word.val ≤ 4294967295 :=
        Nat.le_of_lt_succ word.isLt
      omega⟩⟩

def absent : OptionalIndex := ⟨Option.none⟩

theorem encode_none : encode absent = IR_NONE := rfl

theorem decode_encode (index : OptionalIndex) : decode (encode index) = index := by
  cases index with
  | mk value =>
      cases value with
      | none => simp [decode, encode, IR_NONE]
      | some present =>
          have notNone : present.word.val ≠ IR_NONE.val := Nat.ne_of_lt present.belowNone
          simp [decode, encode, notNone]

end OptionalIndex

end Xsh.IR.Raw
