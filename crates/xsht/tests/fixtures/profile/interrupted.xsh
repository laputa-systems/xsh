proc broken() [error] -> Int {
  let _ = [1, 2] |> par-map(jobs: 2) { |i| 1 / (i - i) } |> collect()
  0
}
print (broken())
