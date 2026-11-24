let total = range(200000) |> map { |i| " x ".trim().byte_len() + i } |> reduce(0) { |total, value| total + value }
print $total
