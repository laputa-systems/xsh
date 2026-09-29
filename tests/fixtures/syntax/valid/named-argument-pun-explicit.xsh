# Unicode before the edit: café 🐚
let value = 10
let other = 20

pure accept(value: Int) -> Int {
  value
}
let fixed = accept(value: value)
let grouped = accept(value: (value))
let distinct = accept(value: other)
let selected = accept(value: {value}.value)
let commented = accept(value: # Preserve this comment.
  value)
print $fixed $grouped $distinct $selected $commented
