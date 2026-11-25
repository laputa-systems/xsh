var total = 0
for _ in range(3) {
  let first = range(20) |> par-map(jobs: 2) { |i| i + 1 } |> collect()
  let second = first |> par-map(jobs: 2) { |i| i + 1 } |> collect()
  total += second |> sum()
}
print $total
