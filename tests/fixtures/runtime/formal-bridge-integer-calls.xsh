pure excess(value: Int, limit: Int) -> Int {
  return if value > limit { value - limit } else { 0 }
}

pure surplus() -> Int {
  let x = 12
  return excess(x, 9)
}
