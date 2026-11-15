import Xsh.Core.CleanupOrder
open Xsh.Core.CleanupOrder

def bit (b : Bool) : String := if b then "1" else "0"
def inputs : List Action :=
  [.always, .onError, .managedRelease] |>.flatMap fun kind =>
    [.ok, .fail false, .fail true] |>.map fun outcome => ⟨kind, outcome⟩
def sequences : Nat → List (List Action)
  | 0 => [[]]
  | n + 1 => (sequences n).flatMap fun xs => inputs.map fun x => xs ++ [x]
def encode (xs : List Nat) : String := if xs.isEmpty then "-" else String.intercalate "," (xs.map toString)
def entry (a : Action) : String :=
  (match a.kind with | .always => "A" | .onError => "E" | .managedRelease => "M") ++
  (match a.outcome with | .ok => "O" | .fail false => "F" | .fail true => "X")
def row (xs : List Action) (flag : Bool) : String :=
  let input := if xs.isEmpty then "-" else String.intercalate "," (xs.map entry)
  let (trace, last) := unwind xs flag
  s!"S {input} {bit flag} {encode trace} {bit last}\n"
def main : IO Unit := do
  let out ← IO.getStdout
  out.putStr "cleanup-order-v1\n"
  for n in [0:5] do
    for xs in sequences n do
      for flag in [false, true] do out.putStr (row xs flag)
