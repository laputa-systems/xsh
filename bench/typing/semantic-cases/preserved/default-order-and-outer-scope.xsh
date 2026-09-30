let seed = 6
proc mark(label: Str, value: Int) [io] -> Int { print $label; value }
proc combine(left: Int = mark("left", seed), right: Int = mark("right", 2)) [io] -> Int { left + right }
print ${combine()}
print ${combine(right: mark("supplied", 9))}
pure outer(seed: Int = seed) -> Int { seed }
print ${outer()}
