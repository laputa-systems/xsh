pure increasing() -> Bool {
  return 1 < 2 <= 3
}

pure skipped() -> Bool {
  return 3 < 2 < 1 / 0
}

pure failed_last() -> Bool {
  return 1 < 2 < 1
}
