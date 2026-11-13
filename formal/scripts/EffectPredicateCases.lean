import Xsh.Core.EffectPredicates

open Xsh.Core Xsh.Syntax

abbrev Contract := SymbolicContract 17

def effects : List Effect := [.fs, .net, .process, .env, .time, .error, .io]

def effectSet (bits : Nat) : EffectSet :=
  EffectSet.ofList ((effects.zipIdx).filterMap fun (effect, index) =>
    if bits / (2 ^ index) % 2 == 1 then some effect else none)

def bits (set : EffectSet) : Nat :=
  (effects.zipIdx).foldl (fun result (effect, index) =>
    if set.contains effect then result + 2 ^ index else result) 0

def encode (contract : Contract) : String :=
  match contract with
  | .unknown => "u"
  | .known expression =>
    let coefficients := (List.finRange 17).filterMap fun p =>
      let mask := bits (expression.coefficients.get p)
      if mask == 0 then none else some s!"{p.val}:{mask}"
    s!"{bits expression.direct};{String.intercalate "," coefficients}"

def result (decision : Option Bool) : String :=
  match decision with
  | none => "?"
  | some true => "t"
  | some false => "f"

def row (op : String) (left right : Contract) (mask : Nat := 0) : String :=
  let decision := match op with
    | "eq" => (EffectPredicate.equal left right).satisfaction
    | "in" => (EffectPredicate.included left right).satisfaction
    | "cov" => (EffectPredicate.covered left right).satisfaction
    | _ => (EffectPredicate.excluded left (effectSet mask)).satisfaction
  let rhs := if op == "ex" then toString mask else encode right
  s!"{op} {encode left} {rhs} {result decision}\n"

def paramContract (id : Fin 17) (mask : Nat) : Contract :=
  (SymbolicContract.known (EffectExpression.parameter id)).masked (effectSet mask)

def symbolic : List Contract :=
  let p := paramContract 0 127
  let q := paramContract 1 127
  let many := (List.finRange 17).foldl (fun acc id => acc.union (paramContract id (2 ^ (id.val % 7)))) (SymbolicContract.finite (effectSet 0))
  [ .unknown, SymbolicContract.finite (effectSet 0), SymbolicContract.finite (effectSet 1),
    SymbolicContract.finite (effectSet 64), p, q, paramContract 0 1, paramContract 0 64,
    p.captureError, p.union q, many, many.captureError,
    p.union (SymbolicContract.finite (effectSet 1)),
    (paramContract 0 1).union (SymbolicContract.finite (effectSet 1)),
    (.unknown : Contract).union p, (paramContract 16 32).captureError ]

def main : IO Unit := do
  let out ← IO.getStdout
  out.putStr "effect-predicate-v1 fs net process env time error io scope=17\n"
  for l in [0:128] do
    let left : Contract := SymbolicContract.finite (effectSet l)
    let mut chunk := ""
    for r in [0:128] do
      let right : Contract := SymbolicContract.finite (effectSet r)
      for op in ["eq", "in", "cov", "ex"] do
        chunk := chunk ++ row op left right r
    out.putStr chunk
  for l in [0:128] do
    let left := paramContract 0 l
    let mut chunk := ""
    for r in [0:128] do
      let right := paramContract 0 r
      for op in ["eq", "in", "cov", "ex"] do
        chunk := chunk ++ row op left right r
    out.putStr chunk
  for left in symbolic do
    let mut chunk := ""
    for right in symbolic do
      for op in ["eq", "in", "cov"] do
        chunk := chunk ++ row op left right
    for mask in [0:128] do
      chunk := chunk ++ row "ex" left .unknown mask
    out.putStr chunk
