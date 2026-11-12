pure scales(value: Int) -> Int {
  let step = value + 4
  return value + step
}

pure runs(value: Int) -> Int {
  return scales(value)
}
