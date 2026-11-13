import Xsh.Core.MembershipDecision

open Xsh.Core Xsh.Values

abbrev T := ValueType Unit
abbrev V := Value Unit Unit Unit

def world : MembershipWorld Unit Unit Unit where
  entryFields := fun _ => []
  stream := fun _ _ => False
  callable := fun _ _ => False
  scheme := fun _ _ => False
  generic := fun _ _ => False

def contracts : ContractDecisions world where
  stream := fun _ _ => isFalse id
  callable := fun _ _ => isFalse id
  scheme := fun _ _ => isFalse id
  generic := fun _ _ => isFalse id

def types : List (String × T) := [
  ("any", .primitive .any), ("null", .primitive .null), ("bool", .primitive .boolean),
  ("int", .primitive .integer), ("uint", .primitive .unsignedInteger),
  ("float", .primitive .float), ("str", .primitive .string), ("bytes", .primitive .bytes),
  ("duration", .primitive .duration), ("digest", .primitive .digest),
  ("regex", .primitive .regex), ("path", .primitive .path), ("unit", .primitive .unit),
  ("list-int", .list (.primitive .integer)), ("nonempty-int", .nonEmpty (.primitive .integer)),
  ("map-str-int", .map (.primitive .string) (.primitive .integer)),
  ("map-int-int", .map (.primitive .integer) (.primitive .integer)),
  ("set-int", .set (.primitive .integer)), ("optional-int", .optional (.primitive .integer)),
  ("record-x-int", .record (.rowField "x" (.primitive .integer) .rowEmpty))]

def values : List (String × V) := [
  ("null", .scalar .null), ("bool", .scalar (.boolean true)),
  ("negative", .scalar (.integer intNegativeOne)), ("zero", .scalar (.integer intZero)),
  ("float", .scalar (.float ⟨0, by decide⟩)), ("str", .scalar (.string "a")),
  ("bytes", .scalar (.bytes [1])), ("duration", .scalar (.duration durationOne)),
  ("digest", .scalar (.digest ⟨"sha256", [1]⟩)), ("regex", .scalar (.regex "a")),
  ("path", .scalar (.path ⟨[97], by decide⟩)), ("unit", .scalar .unit), ("list-empty", .list []),
  ("list-int", .list [.scalar (.integer intOne)]),
  ("list-wrong", .list [.scalar (.string "a")]),
  ("map-empty", .map []), ("map-str-int", .map [(.string "a", .scalar (.integer intOne))]),
  ("map-int-int", .map [(.integer intOne, .scalar (.integer intOne))]),
  ("map-str-wrong", .map [(.string "a", .scalar (.string "a"))]),
  ("set-empty", .set []), ("set-int", .set [.integer intOne]),
  ("set-str", .set [.string "a"]), ("record-empty", .record []),
  ("record-x", .record [("x", .scalar (.integer intOne))]),
  ("record-wrong", .record [("x", .scalar (.string "a"))]),
  ("record-extra", .record [("x", .scalar (.integer intOne)), ("y", .scalar (.integer intOne))])]

def main : IO Unit := do
  let out ← IO.getStdout
  out.putStrLn "membership-v1"
  for (name, type) in types do
    for (valueName, candidate) in values do
      out.putStrLn s!"{name} {valueName} {if Membership.check world contracts type candidate then "1" else "0"}"
