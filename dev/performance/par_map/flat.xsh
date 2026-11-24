let values = range(16) |> par-map(jobs: 16) { |i| i } |> collect()
print (values |> sum())
