pure scales(value: Int) -> Int {
  let a = value + 4
  let b = a * 2
  return b - value
}

pure runs(value: Int) -> Int {
  return scales(value)
}
