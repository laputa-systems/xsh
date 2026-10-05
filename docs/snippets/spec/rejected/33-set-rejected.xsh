let ratios = {1.5, 2.5} # error: check.set-element-type
let nested: Set[List[Str]] = set.empty() # error: check.set-element-type
let rows = [[1], [2]].to_set() # error: check.set-element-type
let words = {"a", "b"}
let mixed = {"a", 1} # error: check.type-mismatch
let ready = true | false # error: check.set-operator
let joined = words | ["c"] # error: check.set-operator
let ints = {1, 2}
let numbers = words & ints # error: check.type-mismatch
let less = words - "a" # error: check.type-mismatch
let none: Set[Str] = {} # error: check.type-mismatch
let one: Set[Str] = {"a"} # error: check.type-mismatch
let first = words[0] # error: check.index-type
print ${ratios.len()} ${nested.len()} ${rows.len()} ${mixed.len()} $ready
print ${joined.len()} ${numbers.len()} ${less.len()} ${none.len()} ${one.len()} $first
