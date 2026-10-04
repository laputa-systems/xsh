let counted = {
  [item]: [2, 1] |> sort |> count
  for item in [3]
}
let collected = {
  [item]: [2, 1] |> sort |> collect()
  for item in [3]
}
let counts = [
  [2, 1] |> sort |> count()
  for item in [3]
]
let collections = [
  [2, 1] |> sort |> collect
  for item in [3]
]
assert counted.get(3)? == 2
assert collected.get(3)? == [1, 2]
assert counts == [2]
assert collections == [[1, 2]]
print "comprehension pipelines preserved"
