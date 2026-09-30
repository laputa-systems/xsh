##! A shared named-argument helper.
pure accept(value: Int) -> Int {
  value
}

## Pass the lexical value through the helper.
export pure relay(value: Int) -> Int {
  accept(value: value)
}
