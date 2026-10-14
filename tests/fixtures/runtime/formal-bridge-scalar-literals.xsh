pure signed_min(max: Int) -> Int {
  return -max - 1
}

pure signed_max() -> Int {
  return 9223372036854775807
}

pure duration_max() -> Duration {
  return 18446744073709551615ms
}

pure positive_zero() -> Float {
  return 0.0
}

pure negative_zero() -> Float {
  return -0.0
}

pure path_echo(input: Path) -> Path {
  return input
}
