import Xsh.Core.DeadlinePolicy

open Xsh.Core.DeadlinePolicy

abbrev Scope := Nat × Bool

def scopeText (scope : Scope) : String :=
  s!"{scope.1}:{if scope.2 then "1" else "0"}"

def row (now : Nat) (scopes : List Scope) : String :=
  let deadlines : List (Deadline Nat Unit) := (scopes.zipIdx).map fun (scope, index) =>
    ⟨index % 2, scope.1, (), scope.2⟩
  let input := if scopes.isEmpty then "-" else String.intercalate "," (scopes.map scopeText)
  let selected := match choose now deadlines with
    | none => "-"
    | some index => toString index
  s!"{now} {input} {selected}\n"

def main : IO Unit := do
  let out ← IO.getStdout
  out.putStr "deadline-policy-v1\n"
  let scopes : List Scope := (List.range 4).flatMap fun tick => [(tick, false), (tick, true)]
  for now in [0:5] do
    let mut chunk := row now []
    for a in scopes do
      chunk := chunk ++ row now [a]
      for b in scopes do
        chunk := chunk ++ row now [a, b]
        for c in scopes do
          chunk := chunk ++ row now [a, b, c]
    out.putStr chunk
