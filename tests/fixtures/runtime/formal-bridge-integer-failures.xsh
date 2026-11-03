pure divided() -> Int {
  let zero = 0
  return 1 / zero
}

pure overflowed() -> Int {
  let top = 9223372036854775807
  return top + 1
}
