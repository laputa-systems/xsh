import Xsh.Core.Arith
import Millipede

open Millipede

namespace MillipedeArith

structure Answer where
  tag : UInt64
  value : UInt64
  deriving Data

mp def magnitude (x : UInt64) : UInt64 :=
  if x < 9223372036854775808 then x else 0 - x

mp def checkedAdd (a b : UInt64) : Answer :=
  let x := a + b
  if (a < 9223372036854775808 && b < 9223372036854775808 && x ≥ 9223372036854775808) ||
     (a ≥ 9223372036854775808 && b ≥ 9223372036854775808 && x < 9223372036854775808) then
    { tag := 1, value := 0 }
  else { tag := 0, value := x }

mp def checkedSub (a b : UInt64) : Answer :=
  let x := a - b
  if (a < 9223372036854775808 && b ≥ 9223372036854775808 && x ≥ 9223372036854775808) ||
     (a ≥ 9223372036854775808 && b < 9223372036854775808 && x < 9223372036854775808) then
    { tag := 1, value := 0 }
  else { tag := 0, value := x }

mp def checkedMul (a b : UInt64) : Answer :=
  let x := magnitude a
  let y := magnitude b
  let negative := (a < 9223372036854775808) != (b < 9223372036854775808)
  let limit := if negative then 9223372036854775808 else 9223372036854775807
  if y != 0 && x > limit / y then { tag := 1, value := 0 }
  else
    let product := x * y
    { tag := 0, value := if negative then 0 - product else product }

mp def checkedDiv (a b : UInt64) : Answer :=
  if b == 0 then { tag := 2, value := 0 }
  else if a == 9223372036854775808 && b == 18446744073709551615 then { tag := 1, value := 0 }
  else
    let q := magnitude a / magnitude b
    let negative := (a < 9223372036854775808) != (b < 9223372036854775808)
    { tag := 0, value := if negative then 0 - q else q }

mp def checkedRem (a b : UInt64) : Answer :=
  if b == 0 then { tag := 2, value := 0 }
  else if a == 9223372036854775808 && b == 18446744073709551615 then { tag := 1, value := 0 }
  else
    let r := magnitude a % magnitude b
    { tag := 0, value := if a ≥ 9223372036854775808 then 0 - r else r }

mp def checkedOp (op a b : UInt64) : Answer :=
  if op == 0 then checkedAdd a b
  else if op == 1 then checkedSub a b
  else if op == 2 then checkedMul a b
  else if op == 3 then checkedDiv a b
  else checkedRem a b

mp extern "read" readByte (fd : UInt64) (buf : OutByte) : UInt64
mp extern "write" writeBytes (fd : UInt64) (buf : &[U8]) : UInt64
mp extern noreturn "exit" exitProcess (code : UInt64) : UInt64

structure Root where
  phase : UInt64
  pos : UInt64
  op : UInt64
  a : UInt64
  b : UInt64
  value : UInt64
  out : Arr UInt8 1
  deriving Data

mp def init : Root :=
  { phase := 0, pos := 0, op := 0, a := 0, b := 0, value := 0,
    out := Vector.replicate 1 0 }

mp def step (r : &mut Root) : Unit := do
  if r.phase == 0 then
    let c := readByte 0
    if c < 256 then
      if r.pos == 0 then
        r.op := c
        r.a := 0
        r.b := 0
        r.pos := 1
      else if r.pos <= 8 then
        r.a := r.a ||| (c <<< ((r.pos - 1) * 8))
        r.pos := r.pos + 1
      else if r.pos <= 16 then
        let b := r.b ||| (c <<< ((r.pos - 9) * 8))
        if r.pos == 16 then
          let result := checkedOp r.op r.a b
          r.out[0] := result.tag.toUInt8
          r.value := result.value
          r.phase := 1
          r.pos := 0
        else
          r.b := b
          r.pos := r.pos + 1
      else
        let _ := exitProcess 1
        ()
    else
      let _ := exitProcess (if r.pos == 0 then 0 else 1)
      ()
  else
    let n := writeBytes 1 (&r.out[0:1])
    if n == 1 then
      if r.pos == 8 then
        r.phase := 0
        r.pos := 0
      else
        r.out[0] := r.value.toUInt8
        r.value := r.value >>> 8
        r.pos := r.pos + 1
    else if n != 0 then
      let _ := exitProcess 1
      ()

mp program program (root : Root) init init step step

#mp_report program
#mp_compile program ".lake/millipede-arith"

end MillipedeArith
