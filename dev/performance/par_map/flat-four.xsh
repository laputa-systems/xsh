let values = range(4) |> par-map(jobs: 4) { |i| i } |> collect()
print (values |> sum())
