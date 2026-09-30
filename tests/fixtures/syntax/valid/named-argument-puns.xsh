# Unicode before the spans: café 🐚
pure combine(first: Int, second: Int, third: Int = 30) -> Int {
  first + second + third
}

let first = 10
let second = 20
let third = 40
let inline = combine(first:, second:, third:)
let mixed = combine(1, second:, third: 50)
let multiline = combine(
  first:,
  second:,
  third:,
)
let commented = combine(
  first:, # Keep this comment on the punned name.
  second:,
  third:,
)
print $inline $mixed $multiline $commented
