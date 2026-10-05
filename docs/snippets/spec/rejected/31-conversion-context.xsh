# begin example
pure width(field: Str) -> Int {
  let columns = field as Int  # error: check.try-context
  columns * 2
}

pure width_or(field: Str, fallback: Int) -> Int {
  field.parse_int() ?? fallback
}
# end example

print f"{width("4")} {width_or("x", 80)}"
