let seed = 6
pure fallback(value = null) { value ?? seed }
pure count(...values) { values.len() }
pure chosen(value = []) { value }
let empty: List[Int] = chosen()
print ${fallback()} ${fallback(9)} ${count(1, 2)} ${empty.len()}
