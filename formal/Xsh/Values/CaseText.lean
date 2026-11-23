import Xsh.Values.CaseTables

namespace Xsh.Values.CaseText

open Xsh.Values.CaseTables

def inRanges (ranges : List (Nat × Nat)) (code : Nat) : Bool := ranges.any fun (first, last) => first ≤ code ∧ code ≤ last

/-- A code point's mapping through runs and multi-character exceptions,
itself when neither holds it. -/
def mapCode (runs : List (Nat × Nat × Nat × Int)) (multi : List (Nat × List Nat)) (code : Nat) : List Nat :=
  match multi.lookup code with
  | some out => out
  | none =>
    match runs.find? fun (first, last, stride, _) => first ≤ code ∧ code ≤ last ∧ (code - first) % stride = 0 with
    | some (_, _, _, offset) => [((code : Int) + offset).toNat]
    | none => [code]

/-- Whether the first character after the case-ignorable ones is cased. -/
def ignorableThenCased : List Char → Bool
  | [] => false
  | char :: rest => if inRanges ignorableRanges char.toNat then ignorableThenCased rest else inRanges casedRanges char.toNat

/-- `str::to_lowercase`: each character lowercased, and `Σ` as `ς` at the end
of a word, a cased character before it and none after it, case-ignorable
characters skipped, and as `σ` otherwise. -/
def lowerFrom : List Char → List Char → List Char
  | _, [] => []
  | before, char :: rest =>
    (if char = 'Σ' then [if ignorableThenCased before ∧ !ignorableThenCased rest then 'ς' else 'σ']
     else (mapCode lowerRuns lowerMulti char.toNat).map Char.ofNat) ++ lowerFrom (char :: before) rest

def lower (text : List Char) : List Char := lowerFrom [] text

/-- `str::to_uppercase`: each character uppercased. -/
def upper (text : List Char) : List Char := text.flatMap fun char => (mapCode upperRuns upperMulti char.toNat).map Char.ofNat

end Xsh.Values.CaseText
