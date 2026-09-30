pure forwarding_total(first: Int, second: Int) -> Int {
  first + second
}

pure forwarding_options() -> Pair {
  Pair(first: 2, second: 3)
}

type Pair = {first: Int, second: Int}
let options = {first: 2, second: 3}
print ${forwarding_total(first: options.first, second: options.second)}
print ${forwarding_total(
  first: options.first,
  # Retain this forwarding comment.
  second: options.second,
)}
var changing = {first: 2, second: 3}
print ${forwarding_total(first: changing.first, second: changing.second)}
let extra = {first: 2, second: 3, other: 4}
print ${forwarding_total(first: extra.first, second: extra.second)}
print ${forwarding_total(first: forwarding_options().first, second: forwarding_options().second)}
