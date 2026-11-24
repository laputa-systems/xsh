let values = range(2000) |> par-map(jobs: 4) { |i| i % 100 + 1 } |> collect()
print (values |> sum())
