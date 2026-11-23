import Xsh.Core.Arith

open Xsh.Core.Arith

def word (i : Int) : UInt64 := ⟨BitVec.ofInt 64 i⟩
def signed (u : UInt64) : Int := u.toBitVec.toInt

def bytes (n : UInt64) : ByteArray := Id.run do
  let mut out := ByteArray.empty
  for i in [0:8] do
    out := out.push (n >>> (UInt64.ofNat (i * 8))).toUInt8
  return out

def cases : List (Int × Int) :=
  let min : Int := -(2 ^ 63)
  let max : Int := 2 ^ 63 - 1
  let fixed := [(0, 0), (1, 1), (min, -1), (min, 0), (max, 1),
    (min, 1), (max, -1), (min, 2), (max, 2), (-7, 3), (7, -3),
    (-7, -3), (min, max), (max, min), (-1, min), (0, max)]
  let (_, random) := (List.range 128).foldl (fun (s, acc) _ =>
    let a := s * 6364136223846793005 + 1442695040888963407
    let b := a * 6364136223846793005 + 1442695040888963407
    (b, (signed a, signed b) :: acc)) ((1 : UInt64), [])
  fixed ++ random

def operations : List (UInt8 × IntOp) :=
  [(0, .add), (1, .sub), (2, .mul), (3, .div), (4, .rem)]

def main : IO Unit := do
  let mut requests := ByteArray.empty
  let mut expected := ByteArray.empty
  for (a, b) in cases do
    for (code, op) in operations do
      requests := requests.push code |>.append (bytes (word a)) |>.append (bytes (word b))
      match checked op a b with
      | .ok result => expected := expected.push 0 |>.append (bytes (word result))
      | .error .overflow => expected := expected.push 1 |>.append (bytes 0)
      | .error .divisionByZero => expected := expected.push 2 |>.append (bytes 0)
  IO.FS.writeBinFile ".lake/requests.bin" requests
  IO.FS.writeBinFile ".lake/expected.bin" expected
  IO.println s!"{cases.length * operations.length} vectors generated from Xsh.Core.Arith.checked"
