const override_name: Str? = null
let label = override_name?.trim() ?? "default"
const samples: List[Int]? = [1, 2, 3]
let first = samples?[0] ?? 0
let remaining = samples?[1..] ?? []
const text: Str? = "42"
let number = (text?.parse_int() ?? Ok(0))?
print $label $first $number
print remaining.len()
