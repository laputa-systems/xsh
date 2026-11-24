let values = range(4) |> par-map(jobs: 4) { |outer|
  let inner = range(4) |> par-map(jobs: 4) { |i| outer * 4 + i } |> collect()
  inner |> sum()
} |> collect()
print (values |> sum())
