pure scaled() -> Int {
  var total = 5
  total = total * 6
  return total + 1
}

pure runs(value: Int) -> Int {
  return scaled()
}

pure ignored() -> Int {
  var zero = 0
  zero = 1 / zero
  return 7
}

pure fails(value: Int) -> Int {
  return ignored()
}
