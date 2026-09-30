let filtered = [1, 2] |> where { false } |> collect()
let values = [1, 2] |> map { |value| Ok(value) } |> collect()
print ${filtered.len()} ${values[0]?} ${values[1]?}
