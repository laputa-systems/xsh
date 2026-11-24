pure clean(value: Str) -> Str { value.trim() }
let total = range(200000) |> map { |i| clean(" x ").byte_len() + i } |> reduce(0) { |total, value| total + value }
print $total
