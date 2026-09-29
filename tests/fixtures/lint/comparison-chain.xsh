pure bounded(value: Int) -> Bool {
  let lower = 0
  let middle = value + 1
  let upper = 10
  return lower <= middle and middle < upper # inclusive lower bound
}

pure triple(value: Int) -> Bool {
  let middle = value + 1
  let upper = 10
  return 0 <= middle and middle < upper and upper <= 20
}

pure unicode(value: Int) -> Bool {
  let unused_unicode = "λ"
  let middle = value + unused_unicode.count_chars()
  return 0 < middle and middle <= 10
}
