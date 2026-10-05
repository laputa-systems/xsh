var seen: Set[Str] = set.from(["a"])
seen = set.add(seen, "b") # error: check.removed-set-function
seen = set.remove(seen, "a") # error: check.removed-set-function
let none = set.empty() # error: check.local-inference
print ${seen.len()} ${none.len()}
