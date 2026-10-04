proc unused() {
  for _ in range("three") {} # error: check.type-mismatch
  for _ in range(end: 3) {} # error: check.named-arg
}
