import Xsh.Core.Arith

open Xsh.Core.Arith

def opName : IntOp → String
  | .add => "add" | .sub => "sub" | .mul => "mul" | .div => "div" | .rem => "rem"

def outcome : Except IntError Int → String
  | .ok value => toString value
  | .error .divisionByZero => "division-by-zero"
  | .error .overflow => "overflow"

def main : IO Unit := do
  let out ← IO.getStdout
  out.putStr "int-arith-v1\n"
  let values : List Int := [minInt, minInt + 1, -7, -2, -1, 0, 1, 2, 3, 7, maxInt - 1, maxInt]
  for op in [IntOp.add, .sub, .mul, .div, .rem] do
    let mut chunk := ""
    for a in values do
      for b in values do
        chunk := chunk ++ s!"{opName op} {a} {b} {outcome (checked op a b)}\n"
    out.putStr chunk
