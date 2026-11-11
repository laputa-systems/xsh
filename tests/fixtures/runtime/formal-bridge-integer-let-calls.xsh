pure bumps(value: Int) -> Int {
  let step = 2
  return value + step
}

pure runs(value: Int) -> Int {
  return bumps(value)
}
