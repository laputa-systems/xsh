import Xsh.Core.EffectPredicates

open Xsh.Core Xsh.Syntax

abbrev Contract := SymbolicContract 2

instance : Inhabited Contract := ⟨.unknown⟩

def effects : List Effect := [.fs, .net, .process, .env, .time, .error, .io]

def effectSet (bits : Nat) : EffectSet :=
  EffectSet.ofList ((effects.zipIdx).filterMap fun (effect, index) =>
    if bits / (2 ^ index) % 2 == 1 then some effect else none)

def bits (set : EffectSet) : Nat :=
  (effects.zipIdx).foldl (fun result (effect, index) =>
    if set.contains effect then result + 2 ^ index else result) 0

def contracts : List Contract :=
  let p : Contract := .known (.parameter 0)
  let q : Contract := .known (.parameter 1)
  [.unknown, .finite (effectSet 0), .finite (effectSet 1), .finite (effectSet 32),
   .finite (effectSet 64), p, q, p.captureError, p.union q,
   p.union (.finite (effectSet 32)), p.masked (effectSet 1)]

def predicates : List (EffectPredicate 2) :=
  let c := contracts.toArray
  [ .equal c[0]! c[1]!, .equal c[5]! c[5]!, .included c[5]! c[4]!,
    .excluded c[5]! (effectSet 32), .covered c[5]! c[1]!,
    .covered c[5]! c[2]!, .covered c[5]! c[3]!, .covered c[5]! c[4]!,
    .covered c[7]! c[1]!, .covered c[7]! c[2]!, .covered c[7]! c[4]!,
    .covered c[6]! c[3]!, .covered c[5]! c[5]!, .covered c[0]! c[3]!,
    .covered c[9]! c[3]!, .covered c[10]! c[2]! ]

def encode (contract : Contract) : String :=
  match contract with
  | .unknown => "u"
  | .known expression =>
    s!"{bits expression.direct}:{bits (expression.coefficients.get 0)}:{bits (expression.coefficients.get 1)}"

def main : IO Unit := do
  let out ← IO.getStdout
  out.putStrLn "effect-algebra-v1"
  for l in [0:128] do
    let left := effectSet l
    let mut chunk := ""
    for r in [0:128] do
      let right := effectSet r
      chunk := chunk ++ s!"set {l} {r} {bits (left.union right)} {if left.included right then 1 else 0} {if left.included right.permissionClosure then 1 else 0}\n"
    out.putStr chunk
  for (left, l) in contracts.zipIdx do
    for (right, r) in contracts.zipIdx do
      out.putStrLn s!"contract {l} {r} {encode (left.union right)}"
  for (left, l) in predicates.zipIdx do
    for (right, r) in predicates.zipIdx do
      out.putStrLn s!"implies {l} {r} {if left.implies right then 1 else 0}"
