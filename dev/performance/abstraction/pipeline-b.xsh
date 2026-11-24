let total = range(200000) |> map { |i| i * 2 + 1 } |> where { |value| value % 3 != 0 } |> reduce(0) { |total, value| total + value }
print $total
