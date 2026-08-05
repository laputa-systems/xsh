let results = [1, 2, 3]
  |> par-map { |value|
    value * 2
  }
  |> collect()
let first = results[0]
print $first
