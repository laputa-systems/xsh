var total = 0
for _ in range(5) {
  let values = range(100) |> par-map(jobs: 4) { |i| i + 1 } |> collect()
  total += values |> sum()
}
print $total
