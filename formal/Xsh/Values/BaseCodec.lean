namespace Xsh.Values.BaseCodec

/-- The RFC 4648 alphabets `Str.base64_decode()`, `Str.base32_decode()`,
`Bytes.base64()` and `Bytes.base32()` use, most significant bits first. -/
inductive Alphabet where
  | base64 | base32
  deriving DecidableEq, Repr

/-- Bits per symbol. -/
def Alphabet.bits : Alphabet → Nat
  | .base64 => 6
  | .base32 => 5

/-- Symbols per padded block. -/
def Alphabet.block : Alphabet → Nat
  | .base64 => 4
  | .base32 => 8

def Alphabet.name : Alphabet → String
  | .base64 => "base64"
  | .base32 => "base32"

/-- A symbol's value, or `none` for any other byte, padding included. -/
def Alphabet.value : Alphabet → UInt8 → Option Nat
  | .base64, byte =>
    if 65 ≤ byte ∧ byte ≤ 90 then some (byte.toNat - 65)
    else if 97 ≤ byte ∧ byte ≤ 122 then some (byte.toNat - 71)
    else if 48 ≤ byte ∧ byte ≤ 57 then some (byte.toNat + 4)
    else if byte = 43 then some 62
    else if byte = 47 then some 63
    else none
  | .base32, byte =>
    if 65 ≤ byte ∧ byte ≤ 90 then some (byte.toNat - 65)
    else if 50 ≤ byte ∧ byte ≤ 55 then some (byte.toNat - 24)
    else none

/-- The byte that spells a symbol value. -/
def Alphabet.symbol : Alphabet → Nat → UInt8
  | .base64, value =>
    if value < 26 then (value + 65).toUInt8
    else if value < 52 then (value + 71).toUInt8
    else if value < 62 then (value - 4).toUInt8
    else if value = 62 then 43
    else 47
  | .base32, value => if value < 26 then (value + 65).toUInt8 else (value + 24).toUInt8

/-- How a decode fails, as the `data-encoding` crate the binary uses
reports it. -/
inductive Failure where
  | length | symbol | trailing | padding
  deriving DecidableEq, Repr

def Failure.text : Failure → String
  | .length => "invalid length"
  | .symbol => "invalid symbol"
  | .trailing => "non-zero trailing bits"
  | .padding => "invalid padding length"

/-- The symbol values of `input`, or the index of the first byte that is not
a symbol. -/
def symbolValues (alphabet : Alphabet) : Nat → List UInt8 → Except Nat (List Nat)
  | _, [] => .ok []
  | index, byte :: rest =>
    match alphabet.value byte with
    | none => .error index
    | some value => (symbolValues alphabet (index + 1) rest).map (value :: ·)

/-- The leading whole bytes of the symbols' bits, most significant first. -/
def bitsToBytes (bits : Nat) (values : List Nat) : List UInt8 :=
  let total := values.foldl (fun accumulated value => accumulated * 2 ^ bits + value) 0
  let count := bits * values.length / 8
  let whole := total / 2 ^ (bits * values.length - 8 * count)
  (List.range count).map fun index => (whole / 2 ^ (8 * (count - 1 - index)) % 256).toUInt8

/-- Unpadded symbols decoded: the first non-symbol is a `symbol` failure, and
non-zero bits past the last whole byte a `trailing` failure at the last
symbol. -/
def decodeSymbols (alphabet : Alphabet) (input : List UInt8) : Except (Failure × Nat) (List UInt8) :=
  match symbolValues alphabet 0 input with
  | .error index => .error (.symbol, index)
  | .ok values =>
    let trail := alphabet.bits * input.length % 8
    if trail ≠ 0 ∧ (values.getLast?.getD 0) % 2 ^ trail ≠ 0 then .error (.trailing, input.length - 1)
    else .ok (bitsToBytes alphabet.bits values)

/-- Padded input decoded block by block from `offset`: the symbols before the
first non-symbol decode as a run, and the block holding it must end in
padding that leaves at least one symbol and fewer than one symbol's worth of
spare bits, its symbols then decoding alone. -/
def decodePadded (alphabet : Alphabet) : Nat → Nat → List UInt8 → Except (Failure × Nat) (List UInt8)
  | 0, _, _ => .ok []
  | _ + 1, _, [] => .ok []
  | fuel + 1, offset, rest =>
    match decodeSymbols alphabet rest with
    | .ok decoded => .ok decoded
    | .error (_, index) =>
      let read := index / alphabet.block * alphabet.block
      let block := (rest.drop read).take alphabet.block
      let padding := (block.reverse.takeWhile (· = 61)).length
      let kept := alphabet.block - padding
      if 0 < kept ∧ alphabet.bits * kept % 8 < alphabet.bits then
        match decodeSymbols alphabet (block.take kept) with
        | .error (failure, position) => .error (failure, offset + read + position)
        | .ok piece =>
          (decodePadded alphabet fuel (offset + read + alphabet.block) (rest.drop (read + alphabet.block))).map
            fun after => bitsToBytes alphabet.bits
              (match symbolValues alphabet 0 (rest.take read) with
                | .ok values => values
                | .error _ => []) ++ piece ++ after
      else .error (.padding, offset + read + kept)

/-- Decode `data` as the binary does once whitespace is removed: input with a
`=` is padded and must be whole blocks, and input without one must not end
in a symbol that holds no whole byte; a failure at the largest valid length
otherwise. -/
def decode (alphabet : Alphabet) (data : List UInt8) : Except (Failure × Nat) (List UInt8) :=
  if data.contains 61 then
    if data.length % alphabet.block = 0 then decodePadded alphabet (data.length + 1) 0 data
    else .error (.length, data.length - data.length % alphabet.block)
  else
    let valid := data.length - alphabet.bits * data.length % 8 / alphabet.bits
    if valid = data.length then decodeSymbols alphabet data else .error (.length, valid)

/-- The bytes' bits in symbols, the last filled with zero bits, padded with
`=` to whole blocks. -/
def encode (alphabet : Alphabet) (bytes : List UInt8) : List UInt8 :=
  let total := bytes.foldl (fun accumulated byte => accumulated * 256 + byte.toNat) 0
  let count := (8 * bytes.length + alphabet.bits - 1) / alphabet.bits
  let filled := total * 2 ^ (alphabet.bits * count - 8 * bytes.length)
  let symbols := (List.range count).map fun index =>
    alphabet.symbol (filled / 2 ^ (alphabet.bits * (count - 1 - index)) % 2 ^ alphabet.bits)
  let blocks := (count + alphabet.block - 1) / alphabet.block
  symbols ++ List.replicate (blocks * alphabet.block - count) 61

end Xsh.Values.BaseCodec
