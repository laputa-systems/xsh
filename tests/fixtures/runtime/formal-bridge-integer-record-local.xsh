pure spans(value: Int) -> Int {
  let box = {low: value - 3, high: 60 / value, wide: value + 9223372036854775800}
  return box.high - box.low
}
