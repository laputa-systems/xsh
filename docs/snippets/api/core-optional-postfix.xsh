let override_name: Str? = null
let label = override_name?.trim() ?? "default"
let samples: List[Int]? = [1, 2, 3]
let first = samples?[0] ?? 0
let remaining = samples?[1..] ?? []
let text: Str? = "42"
let number = (text?.parse_int() ?? Ok(0))?
print $label $first $number
print remaining.len()
