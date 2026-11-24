let captured = [i for i in range(10000)]
let values = range(100) |> par-map(jobs: 4) { |i| i + captured[0] } |> collect()
print (values |> sum())
