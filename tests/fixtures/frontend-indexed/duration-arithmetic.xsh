pure intervals(left: Duration, right: Duration) -> Int {
  let count = left / right
  count
}

pure pause(base: Duration, count: Int) -> Duration { base * count + 1ms }
pure underflow() -> Duration { 0ms - 1ms }
pure overflow() -> Duration { 18446744073709551615ms + 1ms }
pure count_overflow() -> Int { 18446744073709551615ms / 1ms }
