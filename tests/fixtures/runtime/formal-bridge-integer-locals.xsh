pure shifted() -> Int {
  let x = 2
  return x + 3
}

pure scaled() -> Int {
  let y = 7
  return y * 6
}

pure divided() -> Int {
  let zero = 0
  return 1 / zero
}

pure overflowed() -> Int {
  let top = 9223372036854775807
  return top + 1
}
