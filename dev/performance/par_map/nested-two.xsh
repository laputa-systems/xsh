let values = range(2) |> par-map(jobs: 2) { |outer|
  let inner = range(2) |> par-map(jobs: 2) { |i| outer * 2 + i } |> collect()
  inner |> sum()
} |> collect()
print (values |> sum())
