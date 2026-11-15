import Xsh.Core.CleanupPolicy
open Xsh.Core.CleanupPolicy

def bit (b : Bool) : String := if b then "1" else "0"
def inputs : List (Bool × Bool) := [(false, false), (false, true), (true, false), (true, true)]
def sequences : Nat → List (List (Bool × Bool))
  | 0 => [[]]
  | n + 1 => (sequences n).flatMap fun xs => inputs.map fun x => xs ++ [x]
def encode (xs : List Nat) : String := if xs.isEmpty then "-" else String.intercalate "," (xs.map toString)
def run (xs : List (Bool × Bool)) : Option Error × List Nat := Id.run do
  let mut current := none
  let mut reports := []
  for (flags, i) in xs.zipIdx do
    let e : Error := ⟨i, flags.1⟩
    if failureIsRecorded flags.2 flags.1 then
      let (next, emitted) := record current e
      current := next
      if let some secondary := emitted then reports := reports ++ [secondary.id]
    else
      reports := reports ++ [i]
  return (current, reports)
def row (xs : List (Bool × Bool)) : String :=
  let input := if xs.isEmpty then "-" else String.intercalate "," (xs.map fun (f, m) => bit f ++ bit m)
  let (last, reports) := run xs
  s!"S {input} {last.elim "-" (fun e => toString e.id)} {encode reports}\n"
def main : IO Unit := do
  let out ← IO.getStdout
  out.putStr "cleanup-policy-v1\n"
  for (a, b) in inputs do
    let d := match disposition a b with | .raise => "R" | .reportSecondary => "P" | .surface => "S"
    out.putStr s!"D {bit a} {bit b} {d}\n"
  for (managed, forced) in inputs do
    out.putStr s!"F {bit managed} {bit forced} {bit (failureIsRecorded managed forced)}\n"
  for n in [0:5] do
    for xs in sequences n do out.putStr (row xs)
