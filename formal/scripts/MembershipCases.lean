import Xsh.Core.MembershipDecision

open Xsh.Core Xsh.Values

abbrev T := ValueType String
abbrev V := Value String Unit Unit

def world : MembershipWorld String Unit Unit where
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
  ("record-x-int", .record (.rowField "x" (.primitive .integer) .rowEmpty)),
  ("union-int-str", .union (.primitive .integer) (.primitive .string) .membersEmpty),
  ("result-int-str", .result (.primitive .integer) (.primitive .string)),
  ("list-optional-int", .list (.optional (.primitive .integer))),
  ("map-str-record", .map (.primitive .string) (.record (.rowField "x" (.primitive .integer) .rowEmpty))),
  ("union-record-bool", .union (.record (.rowField "x" (.primitive .integer) .rowEmpty)) (.primitive .boolean) .membersEmpty),
  ("erased-record", .primitive .erasedRecord), ("tag-a", .tagged "A"), ("tag-b", .tagged "B"),
  ("error", .primitive .error), ("process-error", .primitive .processError),
  ("family-a", .errorFamily "FamilyA"), ("variant-a", .errorVariant "FamilyA" "Bad"),
  ("facet-io", .errorFacet "io")]

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
  ("record-extra", .record [("x", .scalar (.integer intOne)), ("y", .scalar (.integer intOne))]),
  ("ok-int", .ok (.scalar (.integer intOne))), ("ok-str", .ok (.scalar (.string "a"))),
  ("err-str", .err (.scalar (.string "a"))), ("err-int", .err (.scalar (.integer intOne))),
  ("list-optional", .list [.scalar .null, .scalar (.integer intOne)]),
  ("list-optional-wrong", .list [.scalar .null, .scalar (.string "a")]),
  ("map-record", .map [(.string "a", .record [("x", .scalar (.integer intOne))])]),
  ("map-record-wrong", .map [(.string "a", .record [("x", .scalar (.string "a"))])]),
  ("tag-a-empty", .tagged "A" "One" []),
  ("tag-a-fields", .tagged "A" "Two" [.scalar (.integer intOne)]),
  ("tag-b-empty", .tagged "B" "One" []),
  ("error-a", .error ⟨"FamilyA", "Bad", "FamilyA.Bad", "bad", [], ["io"], none, [], none, none⟩),
  ("error-a-other", .error ⟨"FamilyA", "Other", "FamilyA.Other", "other", [], [], none, [], none, none⟩),
  ("error-b", .error ⟨"FamilyB", "Bad", "FamilyB.Bad", "bad", [], [], none, [], none, none⟩),
  ("run-error", .processError ⟨"failed", "failed", none, none, [], none⟩)]

def main : IO Unit := do
  let out ← IO.getStdout
  out.putStrLn "membership-v1"
  for (name, type) in types do
    for (valueName, candidate) in values do
      out.putStrLn s!"{name} {valueName} {if Membership.check world contracts type candidate then "1" else "0"}"
