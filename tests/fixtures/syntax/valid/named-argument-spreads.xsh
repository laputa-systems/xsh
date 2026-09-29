pure spread_total(first: Int, second: Int) -> Int {
  first + second
}

let options = {first: 2, second: 3}
let first = 4
print ${spread_total(...options)}
print ${spread_total(first:, ...{second: 5})}
print ${spread_total(
  # Keep the spread entry comment.
  ...options,
)}
