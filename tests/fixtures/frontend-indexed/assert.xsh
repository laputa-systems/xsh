pure passes() -> Int {
  assert 2 == 2, f"${1 / 0}"
  7
}

pure fails() -> Int {
  assert 1 == 2, "comparison context"
  7
}

pure chain_fails() -> Int {
  assert 3 < 2 < (1 / 0), "chain context"
  7
}

pure short_circuit_fails() -> Int {
  assert 1 == 2 and (1 / 0 == 0), "logical context"
  7
}
