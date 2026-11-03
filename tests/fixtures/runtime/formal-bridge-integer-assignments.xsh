pure scaled() -> Int {
  var total = 5
  total = total * 6
  return total + 1
}

pure wrapped() -> Int {
  var top = 9223372036854775807
  top = top + 1
  return top
}
